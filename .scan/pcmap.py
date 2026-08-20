# -*- coding: utf-8 -*-
"""Map every published Primary Compression row to the roster entries it names."""
import glob
import io
import os
import re

import yaml

txt = io.open(os.path.expanduser('~/sc/pc.txt'), encoding='utf-8').read()
tbl = txt[txt.index('==Primary Compression Table=='):]

rows, cur = [], []
for line in tbl.split('\n'):
    st = line.strip()
    if st.startswith('|-'):
        if cur:
            rows.append(cur)
        cur = []
        continue
    if st.startswith('!') or st.startswith('{|') or st.startswith('|}'):
        continue
    if st.startswith('|'):
        cur.extend(p.strip() for p in re.split(r'\|\|', st[1:]))
if cur:
    rows.append(cur)


def clean(c):
    c = re.sub(r'\{\{Weapon\|([^}|]+)(\|[^}]*)?\}\}', r'\1', c)
    c = re.sub(r'\{\{Resource\|([^}|]+)(\|[^}]*)?\}\}', r'\1', c)
    c = re.sub(r'\{\{[A-Za-z ]+\|([^}|]+)(\|[^}]*)?\}\}', r'\1', c)
    c = re.sub(r'<ref[^>]*>.*?</ref>', '', c, flags=re.S)
    c = re.sub(r'<[^>]+>', '', c)
    c = re.sub(r'\[\[([^\]|]+)(\|[^\]]*)?\]\]', r'\1', c)
    c = re.sub(r'data-sort="[^"]*"', '', c)
    return c.replace("'''", '').strip()


def names_of(cell):
    """'Acceltra (Acceltra Prime)' and 'A/B (C) (D)' -> every weapon named."""
    out = []
    for part in cell.split('/'):
        part = part.strip()
        base = re.sub(r'\s*\([^)]*\)\s*$', '', part).strip()
        if base:
            out.append(base)
        for m in re.finditer(r'\(([^)]+)\)', part):
            inner = m.group(1).strip()
            if inner and 'Atmosphere' not in inner:
                out.append(inner)
    return [n for n in out if n and len(n) < 40]


specs = {}
for f in sorted(glob.glob('data/weapons/*/*.yaml')):
    d = yaml.safe_load(io.open(f, encoding='utf-8'))
    specs[d['id']] = d
by_name = {}
for wid, d in specs.items():
    nm = d.get('name') or specs.get(d.get('inherits'), {}).get('name', '')
    by_name.setdefault(nm.split(' (')[0], []).append(wid)

print('%-34s %-24s %-6s %-12s %-14s %s'
      % ('row weapon(s)', 'attack', 'eff', 'stacking', 'radius calc', 'our entries'))
for r in rows:
    c = [clean(x) for x in r]
    if len(c) < 5 or not c[0] or c[0].startswith('+') or c[0].isdigit():
        continue
    weapons, attack, eff, stack, calc = c[0], c[1], c[2], c[3], c[4]
    ours = []
    for n in names_of(weapons):
        ours += by_name.get(n, [])
    print('%-34s %-24s %-6s %-12s %-14s %s'
          % (weapons[:34], attack[:24], eff[:6], stack[:12], calc[:14], ','.join(sorted(set(ours)))))
