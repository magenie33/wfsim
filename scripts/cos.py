"""Tencent COS: request signing and object upload, in about forty lines.

Shared by `cos_probe.py` (reachability and throughput) and `release_desktop.py`
(the update channel's publish step). Hand-written against COS's own spec rather
than pulled from an SDK, because the release job should not need a dependency
tree to push a few files, and because a signature is the one part of this
pipeline worth being able to read end to end.

Credentials come from `private/cos.json`, which is gitignored:

    { "secret_id": "...", "secret_key": "...",
      "bucket": "wfsim-1388973035", "region": "ap-shanghai" }
"""
import hashlib
import hmac
import json
import pathlib
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
CREDS = ROOT / "private" / "cos.json"


def creds() -> dict:
    if not CREDS.exists():
        sys.exit(
            f"missing {CREDS}\n\nCreate it with:\n"
            '  { "secret_id": "...", "secret_key": "...",\n'
            '    "bucket": "wfsim-1388973035", "region": "ap-shanghai" }'
        )
    return json.loads(CREDS.read_text(encoding="utf-8"))


def host(c: dict) -> str:
    return f"{c['bucket']}.cos.{c['region']}.myqcloud.com"


def sign(method: str, path: str, secret_id: str, secret_key: str, expire: int = 900) -> str:
    """COS `q-sign-algorithm=sha1`. Empty header and param lists — nothing else
    is signed, so the request must not depend on a signed header."""
    now = int(time.time())
    key_time = f"{now - 60};{now + expire}"
    sign_key = hmac.new(secret_key.encode(), key_time.encode(), hashlib.sha1).hexdigest()
    http_string = f"{method.lower()}\n{path}\n\n\n"
    to_sign = "sha1\n" + key_time + "\n" + hashlib.sha1(http_string.encode()).hexdigest() + "\n"
    signature = hmac.new(sign_key.encode(), to_sign.encode(), hashlib.sha1).hexdigest()
    return (
        f"q-sign-algorithm=sha1&q-ak={secret_id}&q-sign-time={key_time}&q-key-time={key_time}"
        f"&q-header-list=&q-url-param-list=&q-signature={signature}"
    )


# The types the update channel actually publishes. `application/wasm` matters:
# a wasm module served as anything else is refused by instantiateStreaming, and
# the client would silently fall back to buffering 5.4 MB twice.
MIME = {
    ".html": "text/html; charset=utf-8",
    ".js": "text/javascript; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".json": "application/json; charset=utf-8",
    ".wasm": "application/wasm",
    ".svg": "image/svg+xml",
    ".png": "image/png",
    ".jpg": "image/jpeg",
    ".jpeg": "image/jpeg",
    ".webp": "image/webp",
    ".zip": "application/zip",
    ".sig": "text/plain; charset=utf-8",
}


def put(c: dict, key: str, body: bytes, content_type: str | None = None) -> int:
    """Upload one object. `key` has no leading slash."""
    url = f"https://{host(c)}/{key}"
    req = urllib.request.Request(url, data=body, method="PUT")
    req.add_header("Authorization", sign("put", "/" + key, c["secret_id"], c["secret_key"]))
    ext = "." + key.rsplit(".", 1)[-1] if "." in key else ""
    req.add_header("Content-Type", content_type or MIME.get(ext, "application/octet-stream"))
    try:
        with urllib.request.urlopen(req, timeout=300) as r:
            return r.status
    except urllib.error.HTTPError as e:
        raise SystemExit(
            f"upload failed for {key}: HTTP {e.code}\n{e.read().decode('utf-8', 'replace')[:600]}"
        ) from e


def head(c: dict, key: str) -> int | None:
    """Object size if it exists, else None."""
    url = f"https://{host(c)}/{key}"
    req = urllib.request.Request(url, method="HEAD")
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return int(r.headers.get("Content-Length", 0))
    except urllib.error.HTTPError:
        return None
    except OSError:
        return None


def list_keys(c: dict, prefix: str = "") -> set[str]:
    """Every object key under `prefix`, in as few requests as COS allows.

    ONE LIST BEATS N HEADS, and by a lot: asking whether each of 764 blobs
    already exists took 4m35s of round trips, where listing them all takes two
    requests and about a second. It is the same question — the release step
    only needs to know which content the bucket already holds — asked in the
    shape the API is good at.
    """
    import xml.etree.ElementTree as ET

    keys: set[str] = set()
    marker = ""
    while True:
        q = f"?prefix={urllib.parse.quote(prefix)}&max-keys=1000"
        if marker:
            q += f"&marker={urllib.parse.quote(marker)}"
        req = urllib.request.Request(f"https://{host(c)}/{q}", method="GET")
        req.add_header("Authorization", sign("get", "/", c["secret_id"], c["secret_key"]))
        try:
            with urllib.request.urlopen(req, timeout=120) as r:
                body = r.read()
        except urllib.error.HTTPError as e:
            raise SystemExit(
                f"list failed: HTTP {e.code}\n{e.read().decode('utf-8', 'replace')[:600]}"
            ) from e
        root = ET.fromstring(body)
        # COS returns the S3-style listing without a namespace.
        page = [k.text for k in root.iter("Key") if k.text]
        keys.update(page)
        truncated = (root.findtext("IsTruncated") or "false").lower() == "true"
        if not truncated or not page:
            return keys
        marker = root.findtext("NextMarker") or page[-1]
