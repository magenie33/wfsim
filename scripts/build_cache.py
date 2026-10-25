#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""THE BUILD'S CACHE (`web/cache/`), kept in the bucket for a runner to build from.

    python scripts/build_cache.py push   # this tree's cache -> the bucket
    python scripts/build_cache.py pull   # the bucket's cache -> web/cache/
    python scripts/build_cache.py digest # the digest the bucket holds, for a runner's cache key

`web/cache/` is gitignored — the art, pinned libraries and OCR models the site
build derives `site/` from — so a runner building the site (`.github/workflows/
ship.yml`) has none of it. One archive, `build-cache/web-cache.tar.gz`, beside
`build-cache/latest.json` naming its digest: a runner keys its own cache on that
digest and asks the bucket only when it changes. PUSH AFTER FETCHING NEW ART
(`fetch_images.py`): the build fails on an image it does not hold, and says so.
"""
import hashlib
import io
import json
import pathlib
import sys
import tarfile
import urllib.request

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import cos  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parent.parent
CACHE = ROOT / "web" / "cache"
ARCHIVE = "build-cache/web-cache.tar.gz"
LATEST = "build-cache/latest.json"
PUBLIC = "https://wfsim-1388973035.cos.ap-shanghai.myqcloud.com"


def pack() -> bytes:
    """The cache as one archive, its members in path order with no times, so
    the same files make the same bytes and an unchanged cache is not re-sent."""
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz", compresslevel=6) as tar:
        for p in sorted(x for x in CACHE.rglob("*") if x.is_file()):
            info = tarfile.TarInfo(p.relative_to(CACHE).as_posix())
            data = p.read_bytes()
            info.size, info.mtime, info.mode = len(data), 0, 0o644
            tar.addfile(info, io.BytesIO(data))
    raw = buf.getvalue()
    # gzip writes a time into its header; zeroed, as tar's are.
    return raw[:4] + b"\0\0\0\0" + raw[8:]


def push() -> None:
    c = cos.creds()
    body = pack()
    digest = hashlib.sha256(body).hexdigest()
    have = cos.get(c, LATEST)
    if have and json.loads(have).get("sha256") == digest:
        print(f"build cache: the bucket already holds {digest[:12]}")
        return
    cos.put(c, ARCHIVE, body, "application/gzip")
    cos.put(c, LATEST, json.dumps({"sha256": digest, "bytes": len(body)}).encode(), "application/json")
    print(f"build cache: pushed {digest[:12]}, {len(body) / 1e6:.1f} MB")


def digest() -> str:
    have = cos.get(cos.creds(), LATEST)
    return json.loads(have)["sha256"] if have else ""


def public_get(path: str) -> bytes:
    """The build inputs are public, so PR builds need no bucket credentials."""
    with urllib.request.urlopen(f"{PUBLIC}/{path}", timeout=120) as response:
        return response.read()


def pull(public: bool = False) -> None:
    c = None if public else cos.creds()
    want = json.loads(public_get(LATEST))["sha256"] if public else digest()
    if not want:
        sys.exit("build cache: the bucket holds none — run `python scripts/build_cache.py push` where web/cache/ is")
    body = public_get(ARCHIVE) if public else cos.get(c, ARCHIVE)
    if body is None or hashlib.sha256(body).hexdigest() != want:
        sys.exit("build cache: the archive is not the one latest.json names — push again")
    CACHE.mkdir(parents=True, exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(body), mode="r:gz") as tar:
        tar.extractall(CACHE, filter="data")
    print(f"build cache: pulled {want[:12]} into {CACHE.relative_to(ROOT)}")


if __name__ == "__main__":
    what = sys.argv[1] if len(sys.argv) > 1 else ""
    if what == "push":
        push()
    elif what == "pull":
        pull()
    elif what == "digest":
        print(digest())
    elif what == "public-digest":
        print(json.loads(public_get(LATEST))["sha256"])
    elif what == "public-pull":
        pull(public=True)
    else:
        sys.exit(__doc__)
