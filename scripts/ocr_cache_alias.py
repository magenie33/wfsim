import hashlib
import json
from pathlib import Path

root = Path(__file__).resolve().parent.parent
pins = json.loads((root / 'web/ocr/pins.json').read_text())['files']
pin = next(p for p in pins if p['name'].startswith('ort-wasm-') and p['name'].endswith('.js'))
alias = root / 'web/cache/ocr' / pin['name']
original = alias.with_suffix('.mjs')
data = original.read_bytes()
assert hashlib.sha256(data).hexdigest() == pin['sha256']
if alias.exists():
    assert alias.read_bytes() == data
else:
    alias.write_bytes(data)
print(json.dumps({'alias': alias.name, 'sha256': pin['sha256'], 'old_runtime_preserved': True}))
