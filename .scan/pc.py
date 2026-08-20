# -*- coding: utf-8 -*-
"""Which AoE entries the published Primary Compression table names."""
import glob
import io
import os
import re

import yaml

src = os.path.expanduser('~/sc/pc.txt')
txt = io.open(src, encoding='utf-8').read()

names = set()
for line in txt.split('\n'):
    if not line.startswith('|') or line.startswith('|-') or line.startswith('|}'):
        continue
    cells = [c.strip() for c in re.split(r'\|\||\|', line) if c.strip()]
    if not cells:
        continue
    n = cells[0]
    n = re.sub(r'\{\{Weapon\|([^}|]+)(\|[^}]*)?\}\}', r'\1', n)
    n = re.sub(r'\{\{[^}]*\|([^}|]+)\}\}', r'\1', n)
    n = re.sub(r'\[\[([^\]|]+)(\|[^\]]*)?\]\]', r'\1', n)
    n = n.replace("'''", '').replace('{{', '').replace('}}', '').strip()
    if n and len(n) < 60 and not n.startswith('!'):
        names.add(n)
print(len(names), 'names in the table')

rows = {}
for f in sorted(glob.glob('data/weapons/*/*.yaml')):
    d = yaml.safe_load(io.open(f, encoding='utf-8'))
    a = d.get('attack') or {}
    aoe = a.get('radial') or a.get('lingering') or (a.get('beam') or {}).get('damage_radius_m')
    if not aoe:
        continue
    rows[d['id']] = (d.get('name') or '', 'compression' in a)

named = []
for wid, (nm, has) in sorted(rows.items()):
    base = nm.split(' (')[0]
    hit = sorted(n for n in names if n == base or n.startswith(base + ' '))
    if hit and not has:
        named.append((wid, base, hit[:3]))
print('AoE entries the TABLE names and our yaml does NOT declare:', len(named))
for w, b, h in named:
    print('   %-26s %-22s %s' % (w, b, h))

unnamed = [w for w, (nm, has) in sorted(rows.items())
           if not has and not any(n == nm.split(' (')[0] or n.startswith(nm.split(' (')[0] + ' ')
                                  for n in names)]
print('\nAoE entries the table does NOT name (absence means ORDINARY):', len(unnamed))
