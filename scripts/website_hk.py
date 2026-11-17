"""Install the HK website configuration without changing the bot or its credentials."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


def run(*args):
    return subprocess.run(args, capture_output=True, text=True, timeout=20, check=True).stdout.strip()


def replace(path, body, mode):
    with tempfile.NamedTemporaryFile(dir=path.parent, prefix='.wfsim-', delete=False) as stream:
        temporary = Path(stream.name)
        stream.write(body)
        stream.flush()
        os.fsync(stream.fileno())
    try:
        temporary.chmod(mode)
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def backend(token):
    return ('proxy_pass https://wfsim_edge;\n'
            'proxy_http_version 1.1;\n'
            'proxy_set_header Connection "";\n'
            'proxy_set_header Host wfsim.mgnmz1999.workers.dev;\n'
            'proxy_set_header X-WFSim-Website ' + token + ';\n'
            'proxy_set_header X-WFSim-Website-Host cn.wfsim.app;\n'
            'proxy_set_header X-WFSim-Website-IP $remote_addr;\n'
            'proxy_set_header X-WFSim-Relay "";\n'
            'proxy_set_header X-WFSim-Client-IP "";\n'
            'proxy_ssl_server_name on;\n'
            'proxy_ssl_name wfsim.mgnmz1999.workers.dev;\n'
            'proxy_ssl_verify on;\n'
            'proxy_ssl_trusted_certificate /etc/ssl/certs/ca-certificates.crt;\n'
            'proxy_hide_header Alt-Svc;\n'
            'proxy_hide_header NEL;\n'
            'proxy_hide_header Report-To;\n'
            'proxy_connect_timeout 10s;\n'
            'proxy_read_timeout 60s;\n').encode()


class Configuration:
    def __init__(self, stage, prefix=Path('/')):
        self.prefix = prefix
        self.nginx = prefix / 'etc/nginx'
        self.site = self.nginx / 'sites-enabled/wfsim-relay'
        if not self.site.exists():
            raise ValueError('the existing HK relay site is missing')
        self.site = self.site.resolve()
        if not self.site.is_relative_to(self.nginx.resolve()):
            raise ValueError('the existing relay site points outside nginx')
        token = re.fullmatch(r'\s*proxy_set_header\s+X-WFSim-Relay\s+("?)([A-Za-z0-9_-]{32,128})\1;\s*',
                             (self.nginx / 'wfsim-relay-secret.conf').read_text())
        if not token:
            raise ValueError('the existing relay credential include has an unexpected format')
        self.wanted = {self.nginx / name: (stage / name).read_bytes() for name in (
            'wfsim-website-maps.conf', 'wfsim-website-locations.conf', 'wfsim-website-public.conf')}
        self.wanted[self.site] = (stage / 'wfsim-hk-website.conf').read_bytes()
        self.wanted[self.nginx / 'wfsim-backend.conf'] = backend(token[2])
        self.old = {}
        for path in self.wanted:
            if path.is_symlink():
                raise ValueError('a managed nginx include is a symlink')
            self.old[path] = (path.read_bytes(), path.stat().st_mode & 0o777) if path.exists() else None
        self.changed = []
        self.bot = run('systemctl', 'show', 'wfsim-bot.service', '--property=MainPID,ActiveState', '--no-pager')
        if 'ActiveState=active' not in self.bot or 'MainPID=0\n' in self.bot + '\n':
            raise ValueError('the bot must be active before website configuration changes')
        saved = Path(tempfile.mkdtemp(prefix='.nginx-before-', dir=stage.parent))
        receipt = []
        for index, (path, old) in enumerate(self.old.items()):
            name = str(index)
            if old is not None:
                replace(saved / name, old[0], 0o600)
            receipt.append({'path': str(path), 'saved': name if old else None,
                            'old_mode': old[1] if old else None,
                            'new_sha256': hashlib.sha256(self.wanted[path]).hexdigest()})
        replace(saved / 'receipt.json', json.dumps(receipt).encode(), 0o600)

    def check_bot(self):
        if run('systemctl', 'show', 'wfsim-bot.service', '--property=MainPID,ActiveState', '--no-pager') != self.bot:
            raise ValueError('the bot changed during website activation')

    def apply(self):
        self.check_bot()
        for path, body in self.wanted.items():
            mode = 0o600 if path.name == 'wfsim-backend.conf' else 0o644
            old = self.old[path]
            if old == (body, mode):
                continue
            replace(path, body, mode)
            self.changed.append(path)
        if self.changed:
            run('nginx', '-t')
            run('systemctl', 'reload', 'nginx.service')
        self.check_bot()

    def rollback(self):
        for path in self.changed:
            if not path.is_file() or hashlib.sha256(path.read_bytes()).digest() != hashlib.sha256(self.wanted[path]).digest():
                raise ValueError('nginx changed independently; automatic rollback refused')
        for path in reversed(self.changed):
            old = self.old[path]
            if old is None:
                path.unlink()
            else:
                replace(path, *old)
        if self.changed:
            run('nginx', '-t')
            run('systemctl', 'reload', 'nginx.service')
        self.check_bot()
