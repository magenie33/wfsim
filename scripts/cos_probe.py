#!/usr/bin/env python3
"""COS reachability + throughput probe, and the prototype of the CI upload.

Answers the two questions the desktop client's update channel rests on:

  1. can the CI subaccount actually WRITE to the bucket — the console's ACL
     grant is a claim, and a PUT is the only proof;
  2. how fast is the bucket compared with wfsim.app, from where the reader
     actually sits. That decides which one is the PRIMARY update source, and
     it is a MEASUREMENT rather than an assumption about Cloudflare in China.

Credentials come from `private/cos.json` (gitignored) so a key never reaches
a shell history, the repo, or a chat transcript:

    { "secret_id": "...", "secret_key": "...",
      "bucket": "wfsim-1388973035", "region": "ap-shanghai" }

The signature is hand-written against COS's own spec (q-sign-algorithm=sha1)
rather than pulled from an SDK — it is thirty lines, and the upload step in CI
should not need a dependency to push six files.
"""
import hashlib
import hmac
import json
import pathlib
import ssl
import sys
import time
import urllib.parse
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
CREDS = ROOT / "private" / "cos.json"
# The real payload, because a probe on a synthetic file measures the wrong
# thing: this is the exact object the updater will pull on every engine change.
PAYLOAD = ROOT / "site" / "pkg" / "wfsim_wasm_bg.wasm"
KEY = "probe/wfsim_wasm_bg.wasm"


def sign(method: str, path: str, sid: str, skey: str, expire: int = 600) -> str:
    """COS request signature. Empty header/param lists — nothing else is signed."""
    now = int(time.time())
    key_time = f"{now - 60};{now + expire}"
    sign_key = hmac.new(skey.encode(), key_time.encode(), hashlib.sha1).hexdigest()
    http_string = f"{method.lower()}\n{path}\n\n\n"
    to_sign = "sha1\n" + key_time + "\n" + hashlib.sha1(http_string.encode()).hexdigest() + "\n"
    signature = hmac.new(sign_key.encode(), to_sign.encode(), hashlib.sha1).hexdigest()
    return (
        f"q-sign-algorithm=sha1&q-ak={sid}&q-sign-time={key_time}&q-key-time={key_time}"
        f"&q-header-list=&q-url-param-list=&q-signature={signature}"
    )


# A DEFAULT PYTHON UA IS REFUSED BY CLOUDFLARE, and a 403 reads exactly like
# "China cannot reach wfsim.app" while meaning "this client was not a browser".
# The probe exists to tell those two apart, so it asks as a browser would.
UA = ("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
      "(KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36")


def timed_get(url: str, label: str) -> None:
    """Download and report throughput. A failure is a RESULT here, not a crash —
    'wfsim.app is unreachable from here' is exactly what this probe is for."""
    ctx = ssl.create_default_context()
    t0 = time.time()
    try:
        req = urllib.request.Request(url, headers={"User-Agent": UA})
        with urllib.request.urlopen(req, timeout=120, context=ctx) as r:
            n = 0
            while chunk := r.read(1 << 16):
                n += len(chunk)
        dt = time.time() - t0
        print(f"  {label:22} {n / 1e6:7.2f} MB in {dt:6.2f}s  =  {n / 1e6 / dt:6.2f} MB/s")
    except Exception as e:  # noqa: BLE001 — any failure is the answer
        print(f"  {label:22} FAILED after {time.time() - t0:.1f}s: {type(e).__name__}: {e}")


def main() -> None:
    if not CREDS.exists():
        sys.exit(f"missing {CREDS}\n\nCreate it with:\n"
                 '  { "secret_id": "...", "secret_key": "...",\n'
                 '    "bucket": "wfsim-1388973035", "region": "ap-shanghai" }')
    c = json.loads(CREDS.read_text(encoding="utf-8"))
    host = f"{c['bucket']}.cos.{c['region']}.myqcloud.com"
    url = f"https://{host}/{KEY}"
    body = PAYLOAD.read_bytes()
    print(f"payload: {PAYLOAD.name}  {len(body) / 1e6:.2f} MB")
    print(f"bucket:  {host}\n")

    print("1. UPLOAD (proves the subaccount can write)")
    req = urllib.request.Request(url, data=body, method="PUT")
    req.add_header("Authorization", sign("put", "/" + KEY, c["secret_id"], c["secret_key"]))
    req.add_header("Content-Type", "application/wasm")
    t0 = time.time()
    try:
        with urllib.request.urlopen(req, timeout=300) as r:
            dt = time.time() - t0
            print(f"  HTTP {r.status} in {dt:.2f}s  =  {len(body) / 1e6 / dt:.2f} MB/s up\n")
    except urllib.error.HTTPError as e:
        sys.exit(f"  UPLOAD FAILED HTTP {e.code}\n{e.read().decode('utf-8', 'replace')[:800]}")

    print("2. DOWNLOAD (public read, no credentials — what the updater does)")
    timed_get(url, "COS ap-shanghai")
    timed_get("https://wfsim.app/pkg/wfsim_wasm_bg.wasm", "wfsim.app (CF)")
    print("\n3. wfsim.app api reachability (the board submit path)")
    timed_get("https://wfsim.app/api/board/pending", "wfsim.app /api")


if __name__ == "__main__":
    main()
