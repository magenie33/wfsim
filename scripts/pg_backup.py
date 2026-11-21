#!/usr/bin/env python3
"""Bounded-memory PostgreSQL dumps, verified in private R2 before commit."""
import datetime
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import uuid

ROOT = 'r2:wfsim-pg-backup'
DAILY = ROOT + '/daily-v2'
CHUNK_BYTES = 64 * 1024 * 1024
DATABASES = ('globals', 'wfsim_accounts', 'wfsim')


def instant():
    return datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='milliseconds').replace('+00:00', 'Z')


class Rclone:
    def put(self, key, data):
        subprocess.run(['rclone', 'rcat', key, '--size', str(len(data)),
            '--s3-no-check-bucket', '--s3-no-head', '--s3-upload-cutoff', '16M',
            '--s3-chunk-size', '16M', '--s3-upload-concurrency', '2'], input=data, check=True)

    def digest(self, key):
        process = subprocess.Popen(['rclone', 'cat', key, '--s3-no-check-bucket'], stdout=subprocess.PIPE)
        digest = hashlib.sha256()
        size = 0
        for data in iter(lambda: process.stdout.read(4 * 1024 * 1024), b''):
            size += len(data)
            digest.update(data)
        if process.wait():
            raise RuntimeError('R2 readback failed')
        return size, digest.hexdigest()


def put_verified(store, key, data):
    digest = hashlib.sha256(data).hexdigest()
    store.put(key, data)
    if store.digest(key) != (len(data), digest):
        raise RuntimeError('R2 byte/hash verification failed')
    return {'object': key, 'bytes': len(data), 'sha256': digest}


def dump_command(database):
    if database == 'globals':
        return ['sudo', '-u', 'postgres', 'pg_dumpall', '--globals-only']
    return ['nice', '-n', '10', 'sudo', '-u', 'postgres', 'pg_dump', '-Fc',
            '--compress=zstd:3', '--lock-wait-timeout=10s', '-d', database]


def dump_one(store, prefix, database, chunk_bytes=CHUNK_BYTES):
    started = instant()
    process = subprocess.Popen(dump_command(database), stdout=subprocess.PIPE)
    parts = []
    whole = hashlib.sha256()
    size = 0
    try:
        for data in iter(lambda: process.stdout.read(chunk_bytes), b''):
            key = prefix + '/' + database + '/part-' + str(len(parts)).zfill(6)
            parts.append(put_verified(store, key, data))
            whole.update(data)
            size += len(data)
            print(json.dumps({'stage': 'part_verified', 'database': database,
                              'parts': len(parts), 'bytes': size}), flush=True)
        if process.wait():
            raise RuntimeError('pg_dump failed for ' + database)
        if not parts:
            raise RuntimeError('Empty PostgreSQL dump')
    except BaseException:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        raise
    finally:
        process.stdout.close()
    return {'database': database, 'format': 'sql' if database == 'globals' else 'postgres-custom',
            'started_at': started, 'completed_at': instant(), 'parts': parts,
            'bytes': size, 'sha256': whole.hexdigest()}


def backup(store, prefix, databases=DATABASES, chunk_bytes=CHUNK_BYTES):
    if not prefix.startswith(ROOT + '/') or not re.fullmatch(r'[A-Za-z0-9_/-]+', prefix[len(ROOT) + 1:]) or any(part in ('', '.', '..') for part in prefix[len(ROOT) + 1:].split('/')):
        raise ValueError('Backup must use a private bucket child prefix')
    started = instant()
    entries = [dump_one(store, prefix, database, chunk_bytes) for database in databases]
    manifest = {'v': 2, 'started_at': started, 'completed_at': instant(),
        'database_snapshots': 'independent pg_dump snapshots', 'databases': entries,
        'verification': {'full_r2_readback_sha256': True, 'isolated_restore': False},
        'restore': 'Concatenate each database parts in order, checking part and whole digests. Restore globals first, then custom archives using pg_restore --exit-on-error.'}
    data = (json.dumps(manifest, indent=2) + '\n').encode()
    receipt = put_verified(store, prefix + '/manifest.json', data)
    print(json.dumps({'stage': 'committed', **receipt}), flush=True)
    return receipt


def retention_plan(paths, published, now):
    runs = {}
    for path in paths:
        name, separator, leaf = path.partition('/')
        if not separator or not re.fullmatch(r'[0-9]{8}T[0-9]{6}Z-[0-9a-f]{8}', name):
            continue
        runs[name] = leaf == 'manifest.json' or runs.get(name, False)
    if not runs.get(published):
        raise RuntimeError('New backup manifest missing from R2 inventory')

    daily_cutoff = now.date() - datetime.timedelta(days=6)
    expiry = now - datetime.timedelta(days=30)
    incomplete_cutoff = now - datetime.timedelta(days=1)
    kept_days = set()
    kept_weeks = set()
    retire = []
    for name, committed in sorted(runs.items(), reverse=True):
        # An older backup process must leave any newer generation alone.
        if name > published:
            continue
        try:
            created = datetime.datetime.strptime(name[:16], '%Y%m%dT%H%M%SZ').replace(tzinfo=datetime.timezone.utc)
        except ValueError:
            continue
        if not committed and created > incomplete_cutoff:
            continue
        day = created.date()
        week = day.isocalendar()[:2]
        if committed and (name == published or (day >= daily_cutoff and day not in kept_days)):
            kept_days.add(day)
            continue
        if committed and expiry <= created and day < daily_cutoff and week not in kept_weeks and len(kept_weeks) < 3:
            kept_weeks.add(week)
            continue
        retire.append((name, committed))
    return retire


def prune(published, now):
    common = ['--s3-no-check-bucket']
    result = subprocess.run(['rclone', 'lsjson', '--recursive', '--files-only', '--no-modtime',
                             '--no-mimetype', DAILY, *common], capture_output=True, text=True, check=True)
    paths = (row['Path'] for row in json.loads(result.stdout))
    for name, committed in retention_plan(paths, published, now):
        prefix = DAILY + '/' + name
        if committed:
            # A partial deletion must never leave an advertised restore point.
            subprocess.run(['rclone', 'deletefile', prefix + '/manifest.json', *common], check=True)
        subprocess.run(['rclone', 'purge', prefix, *common], check=True)
    subprocess.run(['rclone', 'delete', ROOT + '/daily/', '--min-age', '30d', *common], check=True)


def main():
    os.umask(0o077)
    with open('/run/lock/wfsim-pg-backup.lock', 'w') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            print('A database backup is already running', flush=True)
            return
        now = datetime.datetime.now(datetime.timezone.utc)
        name = now.strftime('%Y%m%dT%H%M%SZ') + '-' + uuid.uuid4().hex[:8]
        receipt = backup(Rclone(), DAILY + '/' + name)
        state = Path('/var/lib/wfsim-pg-backup')
        state.mkdir(mode=0o700, parents=True, exist_ok=True)
        temp = state / 'latest.json.part'
        with temp.open('w') as handle:
            json.dump({'completed_at': instant(), **receipt}, handle)
            handle.flush()
            os.fsync(handle.fileno())
        temp.replace(state / 'latest.json')
        prune(name, datetime.datetime.now(datetime.timezone.utc))


if __name__ == '__main__':
    main()
