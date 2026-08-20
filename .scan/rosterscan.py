# -*- coding: utf-8 -*-
"""Structural sweep over data/weapons — things no existing guard asks."""
import glob
import io
import os
import re
from collections import Counter, defaultdict

import yaml

specs, paths = {}, {}
for f in sorted(glob.glob('data/weapons/*/*.yaml')):
    d = yaml.safe_load(io.open(f, encoding='utf-8'))
    if d['id'] in specs:
        print('DUPLICATE ID: %s in %s and %s' % (d['id'], paths[d['id']], f))
    specs[d['id']] = d
    paths[d['id']] = f.replace('\\', '/')

print('%d entries' % len(specs))
bad = []

# 1. the file name is the id
for wid, p in paths.items():
    if os.path.basename(p)[:-5] != wid:
        bad.append('file/id mismatch: %s carries id %s' % (p, wid))

# 2. `inherits` points at an entry, and never at another form
for wid, d in specs.items():
    par = d.get('inherits')
    if par is None:
        continue
    if par not in specs:
        bad.append('%s inherits %r, which is not an entry' % (wid, par))
    elif specs[par].get('inherits'):
        bad.append('%s inherits %s, which itself inherits — no chains' % (wid, par))

# 3. every transform group has exactly one default form, and its members agree
groups = defaultdict(list)
for wid, d in specs.items():
    g = d.get('transform_group')
    if g:
        groups[g].append(wid)
for g, members in sorted(groups.items()):
    defaults = [m for m in members if specs[m].get('default_form')]
    if len(defaults) != 1:
        bad.append('group %s has %d default forms: %s' % (g, len(defaults), defaults))
    # A FORM MAY QUALIFY THE NAME: "Dread (Incarnon Form)" is one weapon's two
    # forms, and the parenthesis is ours. The BARE name must still be one.
    names = {specs[m]['name'].split(' (')[0] for m in members}
    if len(names) != 1:
        bad.append('group %s spans %d display names: %s' % (g, len(names), sorted(names)))
    slots = {specs[m].get('slot') or specs[specs[m]['inherits']]['slot'] for m in members}
    if len(slots) != 1:
        bad.append('group %s spans slots %s' % (g, slots))
    forms = Counter(specs[m]['form'] for m in members)
    dupes = [f for f, n in forms.items() if n > 1]
    if dupes:
        bad.append('group %s registers %s twice' % (g, dupes))
    if g not in specs:
        bad.append('group %s is not the id of any entry' % g)

# 4. an entry with no group is alone: nothing may inherit it and it inherits nothing
for wid, d in specs.items():
    if d.get('transform_group') or d.get('inherits'):
        continue
    kids = [k for k, v in specs.items() if v.get('inherits') == wid]
    if kids:
        bad.append('%s has no transform_group but %s inherit it' % (wid, kids))

# 5. every default form is a WEAPON row: it must carry the metadata a page needs
NEEDED = ('slot', 'class', 'mastery_rank', 'name')
for wid, d in specs.items():
    if not d.get('default_form'):
        continue
    merged = dict(specs.get(d.get('inherits'), {}))
    merged.update({k: v for k, v in d.items() if v is not None})
    for k in NEEDED:
        if k not in merged:
            bad.append('%s is a default form and has no %s (even after inheriting)' % (wid, k))

# 6. assets: every entry has a picture, and the file is on disk
assets = yaml.safe_load(io.open('data/assets.yaml', encoding='utf-8'))['weapons']
for wid in specs:
    # A FORM WITHOUT A ROW SHOWS ITS WEAPON'S PICTURE, which is the roster's
    # own rule — so the row that must exist is the GROUP's.
    img = assets.get(wid) or assets.get(specs[wid].get('transform_group') or '')         or assets.get(specs[wid].get('inherits') or '')
    if not img:
        bad.append('%s has no picture, and neither has its weapon' % wid)
        continue
    src = img.split(':', 1)[-1]
    if not os.path.exists(os.path.join('web/cache/img', src)):
        bad.append('%s: %s is not in web/cache/img' % (wid, src))

# 7. zh: every entry's WEAPON is named
zh = yaml.safe_load(io.open('data/i18n/zh/names.yaml', encoding='utf-8')).get('weapons', {})
for wid, d in specs.items():
    if d.get('inherits'):
        continue
    if wid not in zh:
        bad.append('%s has no Chinese name' % wid)

# 8. the source url matches the display name
for wid, d in specs.items():
    url = (d.get('source') or {}).get('url', '')
    if not url:
        bad.append('%s has no source url' % wid)
        continue
    parent = specs.get(d.get('inherits'), d)
    # An INCARNON form's page is the GENESIS adapter's, not the weapon's.
    want = parent['name'].split(' (')[0].replace(' ', '_')
    ok = url.endswith('/' + want) or url.endswith('/' + want + '_Incarnon_Genesis')
    if not ok:
        bad.append('%s: source %s does not match name %r' % (wid, url, parent['name']))

# 9. a damage vector is never empty and never negative
for wid, d in specs.items():
    dm = (d.get('attack') or {}).get('damage') or {}
    if not dm:
        bad.append('%s has no damage' % wid)
    for k, v in dm.items():
        if v is None or float(v) < 0:
            bad.append('%s: damage %s = %s' % (wid, k, v))
    rad = (d.get('attack') or {}).get('radial') or {}
    if rad and not rad.get('damage'):
        bad.append('%s has a radial with no damage' % wid)

# 10. every `reason:` names a template that exists, with the holes it needs
reasons = yaml.safe_load(io.open('data/unmodelled/reasons.yaml', encoding='utf-8'))['reasons']
for wid, d in specs.items():
    for u in d.get('unmodeled') or []:
        if not isinstance(u, dict):
            continue
        rid = u.get('reason')
        if rid not in reasons:
            bad.append('%s: unknown reason %r' % (wid, rid))
            continue
        holes = set(re.findall(r'\{(\w+)\}', reasons[rid]['text']))
        given = set(u) - {'reason'}
        if holes != given:
            bad.append('%s: reason %s wants %s, given %s' % (wid, rid, sorted(holes), sorted(given)))

print('%d structural finding(s)' % len(bad))
for b in bad:
    print('  ', b)
