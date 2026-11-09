#!/usr/bin/env python3
"""Install the read-only health timer with a versioned source and rollback."""
import io
import subprocess
import tarfile
from ship_factoryd import ROOT, ssh, target

FILES = ['scripts/pg_health.py', 'deploy/wfsim-health.service', 'deploy/wfsim-health.timer']


def main():
    if subprocess.check_output(['git', 'status', '--porcelain', '--', *FILES], cwd=ROOT).strip():
        raise RuntimeError('Health deployment requires committed sources')
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode='w:gz') as archive:
        for name in FILES:
            archive.add(ROOT / name, arcname=name)
        body = (commit + '\n').encode()
        entry = tarfile.TarInfo('COMMIT'); entry.size = len(body)
        archive.addfile(entry, io.BytesIO(body))
    host = target()
    ssh(host, 'install -d -m 755 /opt/wfsim-health.new && tar xzf - -C /opt/wfsim-health.new && '
        'chown -R root:root /opt/wfsim-health.new && chmod -R go-w /opt/wfsim-health.new && '
        'python3 -m py_compile /opt/wfsim-health.new/scripts/pg_health.py && '
        'systemd-analyze verify /opt/wfsim-health.new/deploy/wfsim-health.service '
        '/opt/wfsim-health.new/deploy/wfsim-health.timer', stdin=output.getvalue())
    ssh(host, """set -eu
systemctl stop wfsim-health.timer wfsim-health.service 2>/dev/null || true
if [ -d /opt/wfsim-health ]; then mv /opt/wfsim-health /opt/wfsim-health.previous-$(date +%s); fi
mv /opt/wfsim-health.new /opt/wfsim-health
install -m 644 /opt/wfsim-health/deploy/wfsim-health.service /etc/systemd/system/
install -m 644 /opt/wfsim-health/deploy/wfsim-health.timer /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now wfsim-health.timer
systemctl start wfsim-health.service
test -s /var/lib/wfsim-health/status.json
systemctl is-active wfsim-health.timer
""")
    print('Health timer installed; inspect status.json alerts and freshness separately from the unit state.')


if __name__ == '__main__':
    main()
