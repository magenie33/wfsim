#!/usr/bin/env python3
"""Install the versioned seven-day task archive beside the database."""
import io
import subprocess
import tarfile

from pg_task_archive import schema_sql
from ship_factoryd import ROOT, ssh, target

FILES = ['scripts/ship_task_archive.py', 'scripts/ship_factoryd.py',
         'scripts/pg_task_archive.py', 'scripts/pg_backup.py', 'worker/task_contract.js',
         'worker/pg/sync.sql', 'deploy/wfsim-task-archive.service', 'deploy/wfsim-task-archive.timer']
REMOTE = '/opt/wfsim-task-archive'


def bundle():
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT).decode().strip()
    dirty = subprocess.check_output(['git', 'status', '--porcelain', '--', *FILES], cwd=ROOT).strip()
    if dirty:
        raise RuntimeError('Task archive deploy requires committed sources')
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode='w:gz') as archive:
        for filename in FILES:
            archive.add(ROOT / filename, arcname=filename)
        data = (commit + '\n').encode()
        info = tarfile.TarInfo('COMMIT')
        info.size = len(data)
        archive.addfile(info, io.BytesIO(data))
    return output.getvalue()


def main():
    host = target()
    artifact = bundle()
    ssh(host, 'command -v rclone >/dev/null && command -v python3 >/dev/null && '
        'sudo -u postgres psql -X -q -At -v ON_ERROR_STOP=1 -d wfsim -c "SELECT 1" && '
        'rclone lsf r2:wfsim-pg-backup --max-depth 1 --s3-no-check-bucket >/dev/null')
    ssh(host, f'mkdir -p {REMOTE}.new && tar xzf - -C {REMOTE}.new && '
        f'chown -R root:root {REMOTE}.new && chmod -R go-rwx {REMOTE}.new', stdin=artifact)
    ssh(host, 'sudo -u postgres psql -X -q -v ON_ERROR_STOP=1 -d wfsim',
        stdin=('BEGIN;\n' + schema_sql() + '\nCOMMIT;\n').encode())
    ssh(host, 'systemctl stop wfsim-task-archive.timer 2>/dev/null || true; '
        'systemctl stop wfsim-task-archive.service 2>/dev/null || true')
    ssh(host, f'if [ -d {REMOTE} ]; then mv {REMOTE} {REMOTE}.previous-$(date +%s); fi; '
        f'mv {REMOTE}.new {REMOTE} && '
        f'install -m 644 {REMOTE}/deploy/wfsim-task-archive.service /etc/systemd/system/ && '
        f'install -m 644 {REMOTE}/deploy/wfsim-task-archive.timer /etc/systemd/system/ && '
        'systemctl daemon-reload && systemd-analyze verify /etc/systemd/system/wfsim-task-archive.service '
        '/etc/systemd/system/wfsim-task-archive.timer && systemctl enable --now wfsim-task-archive.timer')
    print('Task archive timer enabled; inspect its first verified manifest and pruning result.')


if __name__ == '__main__':
    main()
