"""Read-only backend contracts must hold before advancing client pointers."""
import json
from pathlib import Path
import re
import time
import urllib.request

from ship_website import BACKEND


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError(f"readiness endpoint redirected ({code})")


def read_json(url):
    request = urllib.request.Request(url, headers={"user-agent": "wfsim-ship", "cache-control": "no-cache"})
    with urllib.request.build_opener(NoRedirect).open(request, timeout=10) as response:
        if response.status != 200:
            raise ValueError(f"readiness endpoint returned {response.status}")
        data = response.read(65537)
    if len(data) > 65536:
        raise ValueError("readiness endpoint exceeded 64 KiB")
    return data, json.loads(data)


def verify_backend(expected, fetch=read_json, backend=BACKEND):
    _, release = fetch(backend + "/release.json")
    for key in ("release", "engine", "wasm"):
        if not expected.get(key) or release.get(key) != expected[key]:
            raise ValueError(f"backend readiness: {key} differs from the prepared site")
    helper = (Path(__file__).resolve().parent.parent / "worker/client_release.js").read_text()
    canonical = re.search(r'const LATEST = "([^"]+)";', helper).group(1)
    reference, _ = fetch(canonical)
    actual, manifest = fetch(backend + "/api/cli/latest")
    if (type(manifest.get("v")) is not int or manifest["v"] != 1 or not isinstance(manifest.get("files"), dict)
            or actual != reference):
        raise ValueError("backend readiness: CLI manifest differs from the canonical public pointer")
    return {"release": release["release"], "engine": release["engine"], "cli_bytes": len(actual)}


def wait_backend(expected, timeout=60):
    deadline = time.monotonic() + timeout
    while True:
        try:
            result = verify_backend(expected)
            print("backend contracts ready: " + json.dumps(result), flush=True)
            return result
        except Exception:
            if time.monotonic() >= deadline:
                raise
            time.sleep(2)
