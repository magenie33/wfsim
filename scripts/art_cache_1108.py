"""Transfer reviewed art by its exact URL and SHA without fetching the export index."""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import re
import subprocess
from fetch_images import is_image

ROOT = Path(__file__).resolve().parent.parent
CACHE = ROOT / 'web/cache/img'
rows = json.loads((ROOT / 'scripts/art-cache-1108.json').read_text())


def fetch(row):
    name, url, sha = (row[key] for key in ('name', 'url', 'sha256'))
    assert re.fullmatch(r'[A-Za-z0-9_.-]+\.(jpg|png)', name)
    assert url.startswith(('https://content.warframe.com/', 'https://browse.wf/Lotus/Interface/'))
    assert re.fullmatch(r'[a-f0-9]{64}', sha)
    path = CACHE / name
    if not path.exists() or hashlib.sha256(path.read_bytes()).hexdigest() != sha:
        subprocess.run(['curl', '-fsSL', '--retry', '2', '--max-time', '30',
            '-A', 'wfsim/0.1 (https://github.com/magenie33/wfsim)', url, '-o', str(path)],
            check=True, capture_output=True)
    assert hashlib.sha256(path.read_bytes()).hexdigest() == sha, name
    assert is_image(str(path)), name
    return name


with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
    for name in pool.map(fetch, rows):
        print('Verified ' + name)
