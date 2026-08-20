# -*- coding: utf-8 -*-
"""Fill data/assets.yaml for every roster entry that has no picture yet.

A base entry resolves through its `internal_name`; a FORM points at its
weapon's picture, which is the roster's own rule (an Incarnon form shows its
base weapon's art, not the adapter icon).
"""
import glob
import io
import json
import re

import yaml

ASSETS = 'data/assets.yaml'
assets = io.open(ASSETS, encoding='utf-8').read()
have = dict(re.findall(r'^  ([a-z0-9_]+): (\S+)$', assets, re.M))

base, alts = {}, {}
for f in glob.glob('data/weapons/*/*.yaml'):
    d = yaml.safe_load(io.open(f, encoding='utf-8'))
    if d['id'] in have:
        continue
    if d.get('inherits'):
        alts[d['id']] = d['inherits']
    elif d.get('internal_name'):
        base[d['id']] = d['internal_name']

idx = {}
for f in glob.glob('vendor/warframe-items/data/json/*.json'):
    try:
        arr = json.load(io.open(f, encoding='utf-8'))
    except Exception:
        continue
    if isinstance(arr, list):
        for it in arr:
            if isinstance(it, dict) and it.get('uniqueName'):
                idx.setdefault(it['uniqueName'], it)

rows = {k: (idx.get(v) or {}).get('imageName') for k, v in base.items()}
missing = [k for k, v in rows.items() if not v]
for a, parent in alts.items():
    rows[a] = rows.get(parent) or have.get(parent)
    if not rows[a]:
        missing.append(a)
if missing:
    print('NO IMAGE:', sorted(set(missing)))
    raise SystemExit(1)

lines = assets.split('\n')
start = next(i for i, l in enumerate(lines) if l.startswith('weapons:')) + 1
end = next((i for i in range(start, len(lines)) if re.match(r'^[a-z_]+:', lines[i])), len(lines))


def ents():
    out = []
    for i in range(start, end):
        m = re.match(r'^  ([a-z0-9_]+): ', lines[i])
        if m:
            out.append((i, m.group(1)))
    return out


added = 0
for k in sorted(rows, reverse=True):
    e = ents()
    if k in {kk for _, kk in e}:
        continue
    at = next((i for i, kk in e if kk > k), e[-1][0] + 1)
    lines.insert(at, '  %s: %s' % (k, rows[k]))
    end += 1
    added += 1

io.open(ASSETS, 'w', encoding='utf-8', newline='').write('\n'.join(lines))
print('assets added', added)
