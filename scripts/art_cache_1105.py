import concurrent.futures
import hashlib
import json
from pathlib import Path
import re
import subprocess

root = Path(__file__).resolve().parent.parent
cache = root / 'web/cache/img'
rows = json.loads((root / 'scripts/art-cache-1105.json').read_text())
def fetch(row):
    name, url, sha = (row[k] for k in ('name','url','sha256'))
    assert re.fullmatch(r'[A-Za-z0-9_.-]+\.(jpg|png)',name)
    assert url.startswith('https://content.warframe.com/')
    path = cache / name
    if not path.exists() or hashlib.sha256(path.read_bytes()).hexdigest() != sha:
        subprocess.run(['curl','-fsSL','--retry','2','--max-time','30','-A','wfsim/0.1 (https://github.com/magenie33/wfsim)',url,'-o',str(path)],check=True,capture_output=True)
    assert hashlib.sha256(path.read_bytes()).hexdigest() == sha, name
    return name
with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
    for name in pool.map(fetch, rows):
        print('Verified '+name)
