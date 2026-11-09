#!/usr/bin/env python3
"""Archive settled task detail in private R2 before bounded PostgreSQL pruning."""
import argparse
import datetime
import fcntl
import gzip
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import time
import uuid

from pg_backup import Rclone, put_verified

ROOT = Path(__file__).resolve().parent.parent
ARCHIVES = 'r2:wfsim-pg-backup/task-details-v1'
MAX_BYTES = 32 * 1024 * 1024
CAPTURE_TASKS = 500
MAX_CAPTURE_TASKS = 5000
PRUNE_TASKS = 100


class ArchiveStore(Rclone):
    def read(self, key):
        return subprocess.check_output(['rclone', 'cat', key, '--s3-no-check-bucket'], timeout=60)


def read_archive(store, key):
    if not re.fullmatch(re.escape(ARCHIVES) + r'/\d{8}/[a-f0-9]{32}/manifest[.]json', key):
        raise ValueError('Only private task archive manifests can be read')
    manifest = json.loads(store.read(key))
    payload = manifest['payload']
    if payload['object'] != key.removesuffix('manifest.json') + 'tasks.json.gz':
        raise ValueError('Task archive payload must share its manifest prefix')
    packed = store.read(payload['object'])
    if (len(packed), hashlib.sha256(packed).hexdigest()) != (payload['bytes'], payload['sha256']):
        raise RuntimeError('Task archive payload verification failed')
    with gzip.GzipFile(fileobj=io.BytesIO(packed)) as stream:
        data = stream.read(MAX_BYTES + 1)
    if len(data) > MAX_BYTES or (len(data), hashlib.sha256(data).hexdigest()) != (
            manifest['uncompressed_bytes'], manifest['uncompressed_sha256']):
        raise RuntimeError('Task archive expanded content verification failed')
    rows = json.loads(data)
    if rows['before'] != manifest['before'] or [r['question']['id'] for r in rows['tasks']] != manifest['questions']:
        raise RuntimeError('Task archive manifest/content mismatch')
    return rows


def keep_days():
    contract = (ROOT / 'worker/task_contract.js').read_text()
    return int(re.search(r'export const TASK_DETAIL_KEEP_DAYS = (\d+);', contract)[1])


def schema_sql():
    source = (ROOT / 'worker/pg/sync.sql').read_text()
    names = ['factory_past_safe', 'factory_forget_past', 'factory_task_detail',
             'factory_archive_candidates', 'factory_forget_archived']
    functions = [re.search(r'CREATE OR REPLACE FUNCTION ' + name + r'\([\s\S]*?\$\$;', source)[0] for name in names]
    return "SET ROLE wfsim;\nSET lock_timeout='1s';\n" + '\n'.join(functions) + '''
REVOKE ALL ON FUNCTION factory_task_detail(text),factory_archive_candidates(text,bigint),
  factory_forget_archived(text,jsonb,text,text) FROM PUBLIC;
'''


def literal(value):
    return "'" + str(value).replace("'", "''") + "'"


class Postgres:
    def query(self, sql, read=False):
        mode = 'ISOLATION LEVEL REPEATABLE READ READ ONLY' if read else ''
        body = f"BEGIN {mode};\nSET LOCAL statement_timeout='8s';\n"
        body += "SET LOCAL lock_timeout='1s';\nSET LOCAL temp_file_limit='32MB';\n"
        body += "SET LOCAL standard_conforming_strings=on;\nSET LOCAL ROLE wfsim;\n"
        body += "SET LOCAL plan_cache_mode='force_custom_plan';\n"
        body += sql + ';\nCOMMIT;\n'
        result = subprocess.run(['sudo', '-u', 'postgres', 'psql', '-X', '-q', '-At',
            '-v', 'ON_ERROR_STOP=1', '-d', 'wfsim'], input=body, text=True,
            capture_output=True, timeout=20, check=True)
        return json.loads(result.stdout)

    def candidates(self, before, size):
        return self.query(f'SELECT factory_archive_candidates({literal(before)},{size})', read=True)

    def prune(self, before, rows, receipt):
        payload = literal(json.dumps(rows, separators=(',', ':')))
        return self.query(f'SELECT factory_forget_archived({literal(before)},{payload}::jsonb,'
                          f'{literal(receipt["object"])},{literal(receipt["sha256"])})')


def archive_once(store, database, before, size=CAPTURE_TASKS, now=None):
    size = min(max(1, size), MAX_CAPTURE_TASKS)
    started = time.monotonic()
    try:
        rows = database.candidates(before, size)
    except subprocess.CalledProcessError as error:
        if size > PRUNE_TASKS and 'statement timeout' in (error.stderr or ''):
            return archive_once(store, database, before, max(PRUNE_TASKS, size // 2), now)
        raise
    captured_at = time.monotonic()
    if not rows:
        return {'captured': 0, 'deleted': 0}
    data = json.dumps({'v': 1, 'before': before, 'tasks': rows}, separators=(',', ':'), ensure_ascii=False).encode()
    if len(data) > MAX_BYTES:
        if size > 1:
            return archive_once(store, database, before, max(1, size // 2), now)
        raise RuntimeError('Single task exceeds archive memory budget; detail retained')
    now = now or datetime.datetime.now(datetime.timezone.utc)
    prefix = ARCHIVES + '/' + now.strftime('%Y%m%d') + '/' + uuid.uuid4().hex
    packed = gzip.compress(data, compresslevel=6, mtime=0)
    payload = put_verified(store, prefix + '/tasks.json.gz', packed)
    manifest = {'v': 1, 'captured_at': now.isoformat(), 'before': before, 'payload': payload,
        'uncompressed_bytes': len(data), 'uncompressed_sha256': hashlib.sha256(data).hexdigest(),
        'questions': [r['question']['id'] for r in rows],
        'meaning': 'Captured task detail. Pruning rechecks current rows; capture does not assert deletion.'}
    receipt = put_verified(store, prefix + '/manifest.json', (json.dumps(manifest, indent=2) + '\n').encode())
    verified_at = time.monotonic()
    deleted = []
    for offset in range(0, len(rows), PRUNE_TASKS):
        deleted.extend(database.prune(before, rows[offset:offset + PRUNE_TASKS], receipt))
    return {'captured': len(rows), 'deleted': len(deleted), 'manifest': receipt,
            'capture_limit': size, 'prune_statements': (len(rows) + PRUNE_TASKS - 1) // PRUNE_TASKS,
            'seconds': {'capture': round(captured_at - started, 3),
                        'publish': round(verified_at - captured_at, 3),
                        'prune': round(time.monotonic() - verified_at, 3)},
            'compressed_bytes': len(packed), 'uncompressed_bytes': len(data)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--once', action='store_true', help='one bounded archive batch')
    parser.add_argument('--verify', metavar='MANIFEST', help='read back and verify an archived task batch')
    args = parser.parse_args()
    capture_tasks = int(os.environ.get('WFSIM_TASK_ARCHIVE_BATCH', str(CAPTURE_TASKS)))
    if not 1 <= capture_tasks <= MAX_CAPTURE_TASKS:
        parser.error(f'WFSIM_TASK_ARCHIVE_BATCH must be between 1 and {MAX_CAPTURE_TASKS}')
    os.umask(0o077)
    if args.verify:
        rows = read_archive(ArchiveStore(), args.verify)
        print(json.dumps({'verified': True, 'before': rows['before'], 'questions': len(rows['tasks'])}))
        return
    with open('/run/lock/wfsim-task-archive.lock', 'w') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return
        database, store = Postgres(), ArchiveStore()
        before = database.query("SELECT to_jsonb(to_char(clock_timestamp() AT TIME ZONE 'UTC' - "
            f"interval '{keep_days()} days','YYYY-MM-DD\"T\"HH24:MI:SS.MS\"Z\"'))", read=True)
        started = time.monotonic()
        captured = deleted = 0
        for _ in range(50):
            result = archive_once(store, database, before, size=capture_tasks)
            capture_tasks = min(capture_tasks, result.get('capture_limit', capture_tasks))
            captured += result['captured']
            deleted += result['deleted']
            print(json.dumps({'stage': 'task_archive', **result}), flush=True)
            state = Path('/var/lib/wfsim-task-archive')
            state.mkdir(mode=0o700, parents=True, exist_ok=True)
            record = {'at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                      'keep_days': keep_days(), 'before': before, 'captured': captured, 'deleted': deleted, 'last': result}
            temporary = state / 'latest.json.part'
            temporary.write_text(json.dumps(record) + '\n')
            temporary.replace(state / 'latest.json')
            if args.once or not result['deleted'] or time.monotonic() - started >= 30:
                break


if __name__ == '__main__':
    main()
