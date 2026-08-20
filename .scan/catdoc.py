# -*- coding: utf-8 -*-
"""Regenerate docs/CATALOGS.md §2's "Rows the ROSTER carries" from the data."""
import glob
import io
import re

import yaml

rows = []
for f in sorted(glob.glob('data/weapons/*/*.yaml')):
    d = yaml.safe_load(io.open(f, encoding='utf-8'))
    c = (d.get('attack') or {}).get('compression')
    if not c:
        continue
    a = d['attack']
    rad = (a.get('radial') or {}).get('radius_m')
    if rad is None:
        rad = (a.get('lingering') or {}).get('radius_m')
    if rad is None:
        rad = (a.get('beam') or {}).get('damage_radius_m')
    eff = c['effectiveness']
    calc = c.get('radius_calculation', 'snapshot')
    bonus = '—' if eff == 0.0 else '+%g%%' % (float(rad or 0) * 0.8 * 100)
    if c.get('reads_radius_m') is not None:
        bonus = '+%g%%' % (c['reads_radius_m'] * 0.8 * 100)
    rows.append((d['id'], '%g%%' % (eff * 100), '%g m' % float(rad or 0), bonus,
                 c['stacking'].capitalize() if eff else "Doesn't Work", calc))

body = ['| our entry | eff | base radius | max bonus | stacking | radius calc |',
        '| --- | --- | --- | --- | --- | --- |']
for r in sorted(rows):
    body.append('| `%s` | %s | %s | %s | %s | %s |' % r)

p = 'docs/CATALOGS.md'
s = io.open(p, encoding='utf-8').read()
start = s.index('### Rows the ROSTER carries')
end = s.index('\n### ', start + 10)
head = ('### Rows the ROSTER carries\n\n'
        'Only ours, so this stays diffable. The full table lives on the wiki, and\n'
        '`scripts/audit_weapon_stats.py` is not what checks it —\n'
        '`the_roster_reproduces_primary_compressions_published_column` is, by\n'
        "re-deriving the wiki's own Max Damage Bonus column from each entry's radius.\n\n"
        'RE-READ 2026-08-20, when the roster finished its primary/secondary intake.\n'
        'The published table named **fifty-nine** more of our attacks than we carried,\n'
        'and an attack with no `compression:` pays the arcane NOTHING — so every one\n'
        'of them was silently worth zero to a build holding Primary Compression. Half\n'
        'the additions are a tested **0%%** ("Archguns cannot equip", the beam\n'
        'exclusion), which is a ROW and not an omission: saying so is the difference\n'
        'between "checked" and "nobody looked".\n\n'
        '%d rows.\n\n' % len(rows))
s = s[:start] + head + '\n'.join(body) + '\n' + s[end:]
io.open(p, 'w', encoding='utf-8', newline='').write(s)
print('wrote %d rows into docs/CATALOGS.md' % len(rows))
