from pathlib import Path
import hashlib
import io
import json
import shutil
import sys
import tarfile

root = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(root / 'scripts'))
import build_cache

manifest = json.loads((root / '.release-cache/manifest.json').read_text())
if sys.argv[1] == 'prepare':
    # The complete cache is retained; only the explicitly pinned images change.
    for image in manifest['images']:
        name = image['file']
        assert Path(name).name == name
        source = root / '.release-cache/img' / name
        body = source.read_bytes()
        assert len(body) == image['bytes']
        assert hashlib.sha256(body).hexdigest() == image['sha256']
        shutil.copyfile(source, root / 'web/cache/img' / name)
    print(f"merged {len(manifest['images'])} verified images into complete cache")
else:
    latest = json.loads(build_cache.public_get(build_cache.LATEST))
    body = build_cache.public_get(build_cache.ARCHIVE)
    assert hashlib.sha256(body).hexdigest() == latest['sha256']
    with tarfile.open(fileobj=io.BytesIO(body), mode='r:gz') as archive:
        names = set(archive.getnames())
        assert all(any(n.startswith(d + '/') for n in names) for d in ['img', 'lib', 'ocr', 'wiki', 'cnwiki'])
        for image in manifest['images']:
            raw = archive.extractfile('img/' + image['file']).read()
            assert hashlib.sha256(raw).hexdigest() == image['sha256'], image['file']
    receipt = {**latest, 'images_verified': len(manifest['images']), 'files': len(names)}
    (root / 'cache-receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))
