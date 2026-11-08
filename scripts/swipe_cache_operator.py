#!/usr/bin/env python3
"""Operator-only publication of one pinned image; never merge this branch."""
from datetime import datetime, timezone
import hashlib
import io
import json
from pathlib import Path
import sys
import tarfile
import time
import urllib.request

import build_cache

NAME = 'img/CatbrowSwipePrecept.jpg'
URL = 'https://content.warframe.com/PublicExport/Lotus/Interface/Cards/Images/Kavat/CatbrowSwipePrecept.jpg!00_mno+nYS5a8UfjztBqhODRw'
HASH = '0129d16de064f46270a0f683f74621ca26ba6bd24a0fad5bd66ae7f608343bed'
SIZE = 20437
STATE = Path('target/swipe-cache-state.json')
RECEIPT = Path('target/swipe-cache-receipt.json')


def sha(body):
    return hashlib.sha256(body).hexdigest()


def files():
    return {p.relative_to(build_cache.CACHE).as_posix(): sha(p.read_bytes())
            for p in build_cache.CACHE.rglob('*') if p.is_file()}


def get(path):
    request = urllib.request.Request(path, headers={'User-Agent': 'wfsim-art-cache/1'})
    with urllib.request.urlopen(request, timeout=120) as response:
        return response.read()


def prepare():
    old = json.loads(build_cache.public_get(build_cache.LATEST))
    body = build_cache.public_get(build_cache.ARCHIVE)
    assert sha(body) == old['sha256']
    build_cache.CACHE.mkdir(parents=True, exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(body), mode='r:gz') as archive:
        archive.extractall(build_cache.CACHE, filter='data')
    before = files()
    image = get(URL)
    assert len(image) == SIZE and sha(image) == HASH and image.startswith(b'\xff\xd8\xff')
    assert NAME not in before or before[NAME] == HASH
    target = build_cache.CACHE / NAME
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(image)
    after = files()
    assert set(after) - set(before) <= {NAME}
    assert all(after[name] == digest for name, digest in before.items())
    STATE.parent.mkdir(parents=True, exist_ok=True)
    STATE.write_text(json.dumps({'before': old, 'existing_files': before, 'after_files': after}))
    print(json.dumps({'prepared': NAME, 'sha256': HASH, 'existing_files_preserved': len(before)}))


def publish():
    state = json.loads(STATE.read_text())
    credentials = build_cache.cos.creds()
    assert json.loads(build_cache.cos.get(credentials, build_cache.LATEST)) == state['before']
    assert files() == state['after_files']
    build_cache.push()
    suffix = '?verify=' + str(time.time_ns())
    published = json.loads(get(build_cache.PUBLIC + '/' + build_cache.LATEST + suffix))
    body = get(build_cache.PUBLIC + '/' + build_cache.ARCHIVE + suffix)
    assert len(body) == published['bytes'] and sha(body) == published['sha256']
    with tarfile.open(fileobj=io.BytesIO(body), mode='r:gz') as archive:
        actual = {m.name: sha(archive.extractfile(m).read()) for m in archive.getmembers() if m.isfile()}
    assert actual == state['after_files']
    receipt = {'verified_at_utc': datetime.now(timezone.utc).isoformat(),
        'before': state['before'], 'after': published,
        'existing_files_preserved': len(state['existing_files']),
        'added': {'path': NAME, 'url': URL, 'sha256': HASH, 'bytes': SIZE},
        'public_archive_readback_verified': True, 'product_source_changed': False,
        'site_built_locally': False}
    RECEIPT.write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))


if __name__ == '__main__':
    {'prepare': prepare, 'publish': publish}[sys.argv[1]]()
