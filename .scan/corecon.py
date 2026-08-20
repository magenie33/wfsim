# -*- coding: utf-8 -*-
"""Reconcile every Condition Overload catalog row with the entry it names.

`co_behavior: independent` IS the catalog's "Multiplying"; `additive_with_base_
damage` is "Adding". `co_base_fraction` is the "CO Damage Bonus Relative To Base
Damage" column, and 100% means the field is left off.
"""
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
    cells = [c.strip() for c in chunk.strip().lstrip('|').strip().split('||')]
    if len(cells) < 7:
        continue
    name = re.sub(r'\{\{(?:Weapon|Resource)\|([^}|]+)(\|[^}]*)?\}\}', r'\1',
                  cells[0].lstrip('|').strip())
    name = re.sub(r'\[\[([^\]|]+)(\|[^\]]*)?\]\]', r'\1', name).replace("'''", '').strip()
    rows.append((name, cells[1], cells[5], cells[6].split('\n')[0].strip(),
                 cells[7].strip() if len(cells) > 7 else ''))

specs, files = {}, {}
for f in sorted(glob.glob('data/weapons/*/*.yaml')):
    d = yaml.safe_load(io.open(f, encoding='utf-8'))
    specs[d['id']] = d
    files[d['id']] = f.replace('\\', '/')


def display(d):
    return (d.get('name') or specs.get(d.get('inherits'), {}).get('name', '')).split(' (')[0]


fam = {}
for wid, d in specs.items():
    fam.setdefault(display(d), []).append(wid)

# the catalog's attack name -> the FORM it belongs to
FORMS = {
    'normal attack': ('base',), 'main-fire': ('base',), 'primary fire': ('base',),
    'semi-mode': ('semi_auto', 'alt_fire'),
    'alt-fire': ('alt_fire', 'charged'),
    'charged attack': ('charged',), 'charged shot': ('charged',),
    'uncharged attack': ('base',), 'uncharged shot': ('base',),
    'incarnon mode': ('incarnon',), 'incarnon form': ('incarnon',),
    'burst shot': ('alt_fire',), 'throw': ('alt_fire',),
}


def pct(s):
    m = re.match(r'([\d.]+)\s*%', s.replace(',', ''))
    return float(m.group(1)) / 100.0 if m else None


print('%-20s %-28s %-8s %-12s | %s' % ('weapon', 'attack', 'rel', 'type', 'our entry'))
diffs = []
for name, attack, rel, kind, notes in rows:
    ids = fam.get(name)
    if not ids:
        continue
    key = attack.lower().strip()
    key = re.sub(r'\s*\(.*\)$', '', key)
    forms = None
    for k, v in FORMS.items():
        if key == k:
            forms = v
            break
    if forms is None:
        continue                      # a radial / cloud row — a different field
    want_kind = 'independent' if kind.lower().startswith('multipl') else \
        ('additive_with_base_damage' if kind.lower().startswith('add') else None)
    if want_kind is None:
        continue                      # "N/A" — a 0% row, which is its own thing
    want_frac = pct(rel)
    for wid in sorted(ids):
        d = specs[wid]
        if d.get('form') not in forms:
            continue
        have_kind = d.get('co_behavior')
        have_frac = float(d.get('co_base_fraction', 1.0))
        bad = (have_kind != want_kind) or (want_frac and abs(have_frac - want_frac) > 5e-3)
        if bad:
            diffs.append((wid, have_kind, have_frac, want_kind, want_frac, name, attack, notes))

print('\n%d DISAGREEMENTS' % len(diffs))
for wid, hk, hf, wk, wf, name, attack, notes in diffs:
    print('   %-26s ours %-26s %-5s   catalog %-26s %-6s  (%s | %s)'
          % (wid, hk, hf, wk, wf, name, attack))
