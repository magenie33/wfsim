"""Exercise real HTTP refusals and release/manifest readiness without production."""
import json
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer
from unittest.mock import patch

from release_readiness import read_json, verify_backend

expected = {"release": "release", "engine": "engine", "wasm": "wasm"}
manifest = b'{"v":1,"files":{"linux-x64":{}}}'


def fetch(url):
    return ((json.dumps(expected).encode(), expected) if url.endswith("/release.json")
            else (manifest, json.loads(manifest)))


assert verify_backend(expected, fetch)["engine"] == "engine"
for key in expected:
    try:
        verify_backend({**expected, key: "stale"}, fetch)
    except ValueError:
        pass
    else:
        raise AssertionError(f"stale {key} accepted")
for body in [b'{"v":2,"files":{}}', b'{"v":1,"files":[]}', b'{"v":1,"files":{}}']:
    try:
        verify_backend(expected, lambda url: (body, json.loads(body)) if url.endswith("/api/cli/latest") else fetch(url))
    except ValueError:
        pass
    else:
        raise AssertionError("bad or different CLI manifest accepted")
try:
    invalid = b'{"v":true,"files":{}}'
    verify_backend(expected, lambda url: fetch(url) if url.endswith("/release.json") else (invalid, json.loads(invalid)))
except ValueError:
    pass
else:
    raise AssertionError("JSON true was accepted as manifest version 1")


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        self.send_response(302 if self.path == "/redirect" else 200)
        self.send_header("location", "/good")
        self.end_headers()
        self.wfile.write(b"x" * 65537 if self.path == "/oversized" else b'{"v":1}')


with patch("socket.getfqdn", return_value="localhost"):
    server = HTTPServer(("127.0.0.1", 0), Handler)
thread = threading.Thread(target=server.serve_forever, daemon=True)
thread.start()
try:
    origin = f"http://127.0.0.1:{server.server_port}"
    assert read_json(origin + "/good")[1] == {"v": 1}
    for path in ("/redirect", "/oversized"):
        try:
            read_json(origin + path)
        except ValueError:
            pass
        else:
            raise AssertionError(f"HTTP refusal bypassed: {path}")
finally:
    server.shutdown()
    server.server_close()
    thread.join()
print("Backend readiness PASS: exact identity and canonical manifest; stale identities, redirects and oversized responses refuse")
