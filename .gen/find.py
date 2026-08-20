import glob
import io
import json
import os

hits = set()
for f in glob.glob('vendor/warframe-items/data/json/*.json'):
    try:
        arr = json.load(io.open(f, encoding='utf-8'))
    except Exception:
        continue
    if not isinstance(arr, list):
        continue
    for it in arr:
        if isinstance(it, dict) and it.get('name') == 'Enkaus':
            hits.add((os.path.basename(f), it.get('uniqueName'), it.get('imageName')))
for h in sorted(hits):
    print(h)
