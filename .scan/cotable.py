# -*- coding: utf-8 -*-
"""Every row of the wiki's Condition Overload Attack Catalog, against the roster."""
import glob
import io
import os
import re

import yaml

t = io.open(os.path.expanduser('~/sc/co.txt'), encoding='utf-8').read()
cat = t[t.index('==Attack Catalog=='):t.index('==Patch History==')]

rows = []
for chunk in cat.split('|-'):
    if '{{Weapon|' not in chunk and '{{Resource|' not in chunk:
        continue
    line = chunk.strip().lstrip('|').strip()
    cells = [c.strip() for c in line.split('||')]
    if len(cells) < 7:
        continue
    name = cells[0].lstrip('|').strip()
    name = re.sub(r'\{\{(?:Weapon|Resource)\|([^}|]+)(\|[^}]*)?\}\}', r'\1', name)
    name = re.sub(r'\[\[([^\]|]+)(\|[^\]]*)?\]\]', r'\1', name).replace("'''", '').strip()
    rows.append({
        'weapon': name,
        'attack': cells[1],
        'shot': cells[2],
        'unmodded': cells[3],
        'bonus': cells[4],
        'relative': cells[5],
        'type': cells[6].split('\n')[0].strip(),
        'notes': cells[7].strip() if len(cells) > 7 else '',
    })
print('%d catalog rows' % len(rows))

specs = {}
for f in sorted(glob.glob('data/weapons/*/*.yaml')):
    d = yaml.safe_load(io.open(f, encoding='utf-8'))
    specs[d['id']] = d


def display(d):
    return (d.get('name') or specs.get(d.get('inherits'), {}).get('name', '')).split(' (')[0]


by_name = {}
for wid, d in specs.items():
    by_name.setdefault(display(d), []).append(wid)

print('\nrows whose weapon is in the roster:')
hit = 0
for r in rows:
    ids = by_name.get(r['weapon'])
    if not ids:
        continue
    hit += 1
    ours = []
    for wid in sorted(ids):
        d = specs[wid]
        ours.append('%s=%s/%s' % (wid, d.get('co_behavior'), d.get('co_base_fraction', 1.0)))
    print('%-22s | %-26s | %-8s | %-14s | %s'
          % (r['weapon'][:22], r['attack'][:26], r['relative'][:8], r['type'][:14],
             ' '.join(ours)[:80]))
print('%d of %d rows name a roster weapon' % (hit, len(rows)))
