"""HK activation preserves the bot and rolls back only configuration it owns."""
import json
import http.server
import io
import os
from pathlib import Path
import subprocess
import tempfile
import tarfile
import threading
from types import SimpleNamespace
from unittest.mock import patch

import website_config
import website_hk
import ship_website


def case(mode):
    with tempfile.TemporaryDirectory() as directory:
        prefix = Path(directory)
        nginx = prefix / 'etc/nginx'
        site = nginx / 'sites-enabled/wfsim-relay'
        site.parent.mkdir(parents=True)
        site.write_bytes(b'existing relay')
        secret = nginx / 'wfsim-relay-secret.conf'
        secret.write_text('proxy_set_header X-WFSim-Relay "' + 'a' * 64 + '";\n')
        secret.chmod(0o600)
        stage = prefix / 'release/nginx'
        stage.mkdir(parents=True)
        for name, body in website_config.files().items():
            (stage / name).write_text(body)
        calls, failed_once, reloaded = [], False, False

        def run(*args):
            nonlocal failed_once, reloaded
            calls.append(args)
            if args[:2] == ('systemctl', 'show'):
                pid = '2' if mode == 'bot-drift' and reloaded else '1'
                return 'MainPID=' + pid + '\nActiveState=active'
            if args == ('nginx', '-t') and mode == 'syntax' and not failed_once:
                failed_once = True
                raise subprocess.CalledProcessError(1, args)
            if args == ('systemctl', 'reload', 'nginx.service'):
                reloaded = True
            return ''

        with patch.object(website_hk, 'run', run):
            configuration = website_hk.Configuration(stage, prefix)
            try:
                configuration.apply()
            except (ValueError, subprocess.CalledProcessError):
                assert mode in ('syntax', 'bot-drift')
                try:
                    configuration.rollback()
                except ValueError:
                    assert mode == 'bot-drift'
            else:
                assert mode not in ('syntax', 'bot-drift')
                assert site.read_text() == website_config.files()['wfsim-hk-website.conf']
                backend = nginx / 'wfsim-backend.conf'
                assert backend.stat().st_mode & 0o777 == 0o600
                assert 'X-WFSim-Website-Host cn.wfsim.app;' in backend.read_text()
                assert 'proxy_set_header Origin' not in backend.read_text()
                if mode == 'health':
                    configuration.rollback()
                elif mode == 'foreign':
                    site.write_text('independent change')
                    try:
                        configuration.rollback()
                    except ValueError:
                        pass
                    else:
                        raise AssertionError('rollback overwrote a foreign configuration')
                    assert site.read_text() == 'independent change'
            if mode in ('health', 'syntax', 'bot-drift'):
                assert site.read_bytes() == b'existing relay'
                assert not (nginx / 'wfsim-backend.conf').exists()
                assert not (nginx / 'wfsim-website-locations.conf').exists()
            assert secret.read_text() == 'proxy_set_header X-WFSim-Relay "' + 'a' * 64 + '";\n'
            assert not any('restart' in call or 'stop' in call for call in calls)


def check():
    for mode in ('success', 'health', 'syntax', 'foreign', 'bot-drift'):
        case(mode)
    with patch.dict(os.environ, {'WEBSITE_TARGET': '', 'WEBSITE_MIRROR_TARGET': '{"host":"root@hk.example"}'}):
        assert ship_website.targets()[0]['hostname'] == 'cn.wfsim.app'
    with patch.dict(os.environ, {'WEBSITE_TARGET': '{"host":"root@same.example"}',
                                'WEBSITE_MIRROR_TARGET': '{"host":"root@same.example"}'}):
        try:
            ship_website.targets()
        except ValueError:
            pass
        else:
            raise AssertionError('two entrances accepted the same server')
    config = website_config.files()
    assert 'board\\.meta\\.json' in config['wfsim-website-locations.conf']
    assert 'Cookie ""' in config['wfsim-website-public.conf']
    assert 'Host cdn.wfsim.app;' in config['wfsim-website-public.conf']
    activation()
    courier()
    print('HK configuration: existing relay and bot preserved; syntax/health failures restore files; foreign drift refuses rollback')


def activation():
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        base, release = root / 'host', root / 'release'
        old = base / 'releases/old/site'
        old.mkdir(parents=True)
        (base / 'current').symlink_to(old)
        (release / 'site').mkdir(parents=True)
        (release / 'nginx').mkdir()
        expected = {'release': 'same', 'commit': 'new', 'engine': 'engine'}
        (release / 'site/release.json').write_text(json.dumps(expected))
        (release / 'hk.py').write_text('from pathlib import Path\nclass Configuration:\n'
            ' def __init__(self,stage):self.stage=Path(stage)\n'
            ' def apply(self):(self.stage/"applied").touch()\n'
            ' def check_bot(self):pass\n'
            ' def rollback(self):(self.stage/"rolled-back").touch()\n')
        served, pending, failure, observed = [expected], [0], [None], []

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                status = 421 if self.headers.get('Host') != 'cn.wfsim.app' else 200
                if pending[0] or served[0] is None:
                    pending[0] = max(0, pending[0] - 1)
                    status = 404
                if failure[0] is not None:
                    status = failure[0]
                observed.append(status)
                self.send_response(status)
                self.end_headers()
                self.wfile.write(json.dumps(served[0]).encode())

            def log_message(self, *_):
                pass

        server = http.server.HTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            args = ['python3', '-c', ship_website.REMOTE_ACTIVATE, str(release), str(base),
                    'http://127.0.0.1:' + str(server.server_address[1]), 'cn.wfsim.app']
            env = {**os.environ, 'http_proxy': 'http://127.0.0.1:1', 'HTTP_PROXY': 'http://127.0.0.1:1',
                   'no_proxy': '', 'NO_PROXY': ''}
            assert subprocess.run(args, capture_output=True, env=env).returncode == 0
            assert (base / 'current').resolve() == (release / 'site').resolve()
            assert (release / 'nginx/applied').exists()
            (base / 'current').unlink()
            (base / 'current').symlink_to(old)
            pending[0] = 2
            start = len(observed)
            delayed = subprocess.run(args, capture_output=True)
            assert delayed.returncode == 0, 'old nginx workers still answer 404 after reload: ' + delayed.stderr.decode()
            assert observed[start:] == [404, 404, 200, 200]
            assert (base / 'current').resolve() == (release / 'site').resolve()
            (base / 'current').unlink()
            (base / 'current').symlink_to(old)
            served[0] = {**expected, 'commit': 'old'}
            assert subprocess.run(args, capture_output=True).returncode != 0
            assert (base / 'current').resolve() == old.resolve()
            assert (release / 'nginx/rolled-back').exists()
            failure[0] = 503
            start = len(observed)
            assert subprocess.run(args, capture_output=True).returncode != 0
            assert observed[start:] == [503]
            assert (base / 'current').resolve() == old.resolve()
            failure[0] = None
            served[0] = None
            assert subprocess.run(args, capture_output=True).returncode != 0
            assert (base / 'current').resolve() == old.resolve()
        finally:
            server.shutdown()
            server.server_close()


def courier():
    deleted = []

    def put(_creds, _key, body, _type):
        with tarfile.open(fileobj=io.BytesIO(body), mode='r:gz') as archive:
            names = archive.getnames()
            assert 'hk.py' in names and 'site/release.json' in names
            manifest = json.load(archive.extractfile('manifest.json'))
            assert len(manifest['support']) == 5
        raise OSError('upload reply lost')

    cos = SimpleNamespace(creds=lambda: {}, delete=lambda *_: deleted.append(True),
                          put=put)
    with tempfile.TemporaryDirectory() as directory:
        site = Path(directory)
        (site / 'release.json').write_text('{"release":"012345abcdef"}')
        with patch.dict(os.environ, {'BOT_VIA_COS': '1'}), patch.dict('sys.modules', {'cos': cos}), \
                patch.object(ship_website, 'backend_matches'), patch.object(ship_website, 'connection', return_value=[]):
            try:
                ship_website.deploy(site, {'hostname': 'cn.wfsim.app'})
            except OSError:
                pass
            else:
                raise AssertionError('the failed upload was ignored')
        assert deleted == [True], 'a possibly uploaded courier object must still be removed'


if __name__ == '__main__':
    check()
