#!/usr/bin/env python3
"""Bounded, read-only health evidence for the database and its derived outputs."""
import argparse
import datetime
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
import urllib.request
import uuid

SQL = """
BEGIN READ ONLY;
SET LOCAL statement_timeout='4s';
SET LOCAL lock_timeout='1s';
SET LOCAL temp_file_limit='16MB';
SET LOCAL application_name='wfsim-health';
SELECT json_build_object(
 'snapshot',(SELECT row_to_json(s) FROM (SELECT daemon_at,daemon_commit,engine,at,
   open,computing,queue_rows,settle_queue_rows FROM factory_snapshot WHERE id=1)s),
 'inventory',(SELECT json_build_object('at',body::jsonb->>'at','started_at',body::jsonb->>'started_at',
   'expires_at',body::jsonb->>'expires_at','generation',body::jsonb->>'generation',
   'epoch',body::jsonb->>'epoch','engine',body::jsonb->>'engine') FROM factory_findings WHERE kind='task_inventory'),
 'generation',inventory_status(),
 'dirty',(SELECT json_build_object('estimated',n_live_tup,'dead',n_dead_tup,'last_analyze',last_autoanalyze)
   FROM pg_stat_user_tables WHERE relname='inventory_dirty'),
 'sessions',(SELECT coalesce(json_agg(s),'[]'::json) FROM (SELECT pid,left(application_name,60) application,
   state,wait_event_type,wait_event,query_id::text,
   round(extract(epoch FROM clock_timestamp()-xact_start)) transaction_seconds,
   round(extract(epoch FROM clock_timestamp()-state_change)) idle_seconds,
   pg_blocking_pids(pid) blockers FROM pg_stat_activity WHERE datname=current_database()
   AND pid<>pg_backend_pid() AND (wait_event_type='Lock' OR (state LIKE 'idle in transaction%'
     AND xact_start<clock_timestamp()-interval '1 minute'))
   ORDER BY xact_start NULLS LAST LIMIT 32)s),
 'dirty_locks',(SELECT coalesce(json_agg(l),'[]'::json) FROM (SELECT pid,mode,granted
   FROM pg_locks WHERE relation='inventory_dirty'::regclass ORDER BY granted,pid LIMIT 32)l));
COMMIT;
"""


def instant(now):
    return datetime.datetime.fromtimestamp(now, datetime.timezone.utc).isoformat()


def age(value, now):
    try:
        return max(0, now - datetime.datetime.fromisoformat(value.replace('Z', '+00:00')).timestamp())
    except (TypeError, ValueError, AttributeError):
        return None


def database():
    result = subprocess.run(['psql', '-X', '-q', '-At', '-v', 'ON_ERROR_STOP=1', '-d', 'wfsim'],
                            input=SQL, capture_output=True, text=True, timeout=15)
    if result.returncode:
        # SQL text, row contents and connection strings never enter diagnostic output.
        code = 'lock_timeout' if 'lock timeout' in result.stderr else 'statement_timeout' if 'statement timeout' in result.stderr else 'query_failed'
        raise RuntimeError(code)
    return json.loads(result.stdout)


def public_board():
    request = urllib.request.Request('https://cdn.wfsim.app/board.meta.json', headers={'User-Agent': 'WFSim-health/1'})
    with urllib.request.urlopen(request, timeout=8) as response:
        raw = response.read(1024 * 1024 + 1)
        if len(raw) > 1024 * 1024:
            raise RuntimeError('response_too_large')
        body = json.loads(raw)
    values = [int(row.get('scored_at_epoch_seconds') or 0) for row in body['boards'].values()]
    return {'newest_epoch': max(values, default=0), 'boards': len(values)}


def services():
    result = subprocess.run(['systemctl', 'show', 'wfsim-factoryd.service', 'wfsim-task-archive.service',
                             'wfsim-task-archive.timer', '-p', 'Id', '-p', 'ActiveState', '-p', 'SubState',
                             '-p', 'ExecMainStatus', '-p', 'ExecMainExitTimestamp', '-p', 'NRestarts'],
                            capture_output=True, text=True, timeout=5, check=True)
    rows = [dict(line.split('=', 1) for line in block.splitlines() if '=' in line)
            for block in result.stdout.strip().split('\n\n')]
    return {row['Id']: row for row in rows if 'Id' in row}


def resources():
    result = {'disk_free_bytes': shutil.disk_usage('/var/lib/postgresql').free}
    for line in Path('/proc/meminfo').read_text().splitlines():
        key, value = line.split(':', 1)
        if key in ('MemAvailable', 'MemTotal', 'SwapFree', 'SwapTotal'):
            result[key] = int(value.split()[0]) * 1024
    for key in ('memory', 'io'):
        path = Path('/proc/pressure') / key
        if path.exists():
            result[key + '_pressure'] = path.read_text().strip()[:400]
    return result


def collect():
    result = {}
    for name, probe in [('database', database), ('board', public_board), ('services', services), ('resources', resources)]:
        start = time.monotonic()
        try:
            result[name] = {'ok': True, 'value': probe()}
        except Exception as error:
            code = str(error) if type(error) is RuntimeError and re.fullmatch('[a-z_]{1,40}', str(error)) else type(error).__name__
            result[name] = {'ok': False, 'error': code}
        result[name]['seconds'] = round(time.monotonic() - start, 3)
    return result


def assess(probes, previous=None, now=None):
    now = time.time() if now is None else now
    previous = previous or {}
    alerts = ['probe_failed:' + name for name in ('database', 'board', 'services', 'resources')
              if not probes.get(name, {}).get('ok')]
    db = probes.get('database', {}).get('value') or {}
    snapshot = db.get('snapshot') or {}
    heartbeat = age(snapshot.get('daemon_at'), now)
    if probes.get('database', {}).get('ok'):
        if heartbeat is None or heartbeat > 300:
            alerts.append('daemon_stale')
        inventory = db.get('inventory') or {}
        captured = age(inventory.get('started_at'), now)
        if captured is None or captured > 10800:
            alerts.append('inventory_stale')
        if inventory and inventory.get('engine') != snapshot.get('engine'):
            alerts.append('inventory_engine_mismatch')
        if any(row.get('blockers') for row in db.get('sessions', [])):
            alerts.append('database_lock_wait')
        if any((row.get('idle_seconds') or 0) > 300 and row.get('state', '').startswith('idle in transaction')
               for row in db.get('sessions', [])):
            alerts.append('idle_transaction')
    board = probes.get('board', {}).get('value') or {}
    newest = board.get('newest_epoch') or 0
    if probes.get('board', {}).get('ok') and (not newest or now - newest > 21600):
        alerts.append('board_stale')
    since = previous.get('settlement_backlog_since') if snapshot.get('settle_queue_rows', 0) else None
    if snapshot.get('settle_queue_rows', 0):
        since = since or instant(now)
        if (age(since, now) or 0) > 600:
            alerts.append('settlement_backlog')
    units = probes.get('services', {}).get('value') or {}
    if probes.get('services', {}).get('ok'):
        for name in ('wfsim-factoryd.service', 'wfsim-task-archive.timer'):
            if units.get(name, {}).get('ActiveState') != 'active':
                alerts.append('service_inactive:' + name)
        archive = units.get('wfsim-task-archive.service', {})
        if not archive or archive.get('ExecMainStatus') != '0':
            alerts.append('archive_failed')
    memory = probes.get('resources', {}).get('value') or {}
    if memory.get('disk_free_bytes', 8 * 1024**3) < 8 * 1024**3:
        alerts.append('disk_reserve_low')
    prior = previous.get('alerts') or []
    incident = previous.get('incident_id') if prior else None
    return {'v': 1, 'at': instant(now), 'event': 'incident' if alerts and not prior else 'recovered' if prior and not alerts else 'sample',
            'incident_id': (incident or str(uuid.uuid4())) if alerts else previous.get('incident_id') if prior else None,
            'status': 'degraded' if alerts else 'healthy', 'alerts': sorted(alerts),
            'settlement_backlog_since': since, 'probes': probes}


def persist(directory, report, limit=2 * 1024 * 1024):
    directory.mkdir(parents=True, exist_ok=True)
    data = json.dumps(report, separators=(',', ':')) + '\n'
    target = directory / 'status.json'
    partial = directory / 'status.partial'
    with partial.open('w') as output:
        output.write(data); output.flush(); os.fsync(output.fileno())
    os.replace(partial, target)
    history = directory / 'events.ndjson'
    if history.exists() and history.stat().st_size + len(data.encode()) > limit:
        older = directory / 'events.2.ndjson'
        older.unlink(missing_ok=True)
        prior = directory / 'events.1.ndjson'
        if prior.exists():
            os.replace(prior, older)
        os.replace(history, prior)
    with history.open('a') as output:
        output.write(data); output.flush(); os.fsync(output.fileno())


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--state-dir', type=Path, default=Path('/var/lib/wfsim-health'))
    parser.add_argument('--once', action='store_true', help='print without writing state')
    args = parser.parse_args()
    try:
        previous = json.loads((args.state_dir / 'status.json').read_text())
    except (OSError, ValueError):
        previous = {}
    report = assess(collect(), previous)
    source = Path(__file__).resolve().parent.parent / 'COMMIT'
    report['source'] = source.read_text().strip() if source.exists() else 'working-tree'
    if not args.once:
        persist(args.state_dir, report)
    print(json.dumps(report), flush=True)
    return 0 if report['status'] == 'healthy' else 1


if __name__ == '__main__':
    raise SystemExit(main())
