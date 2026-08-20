# -*- coding: utf-8 -*-
"""Apply the Condition Overload catalog rows the roster was contradicting."""
import glob
import io
import os
import re
import textwrap

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
    notes = re.sub(r'\{\{[A-Za-z]+\|([^}|]+)(\|[^}]*)?\}\}', r'\1',
                   cells[7].strip() if len(cells) > 7 else '')
    notes = re.sub(r'\[\[([^\]|]+)(\|[^\]]*)?\]\]', r'\1', notes).replace("'''", '').strip()
    rows.append(dict(weapon=name, attack=cells[1], shot=cells[2], unmodded=cells[3],
                     bonus=cells[4], rel=cells[5], kind=cells[6].split('\n')[0].strip(),
                     notes=notes))

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

FORMS = {
    'normal attack': ('base',), 'main-fire': ('base',), 'primary fire': ('base',),
    'semi-mode': ('semi_auto', 'alt_fire'),
    'alt-fire': ('alt_fire', 'charged'),
    'charged attack': ('charged',), 'charged shot': ('charged',),
    'uncharged attack': ('base',), 'uncharged shot': ('base',),
    'incarnon mode': ('incarnon',), 'incarnon form': ('incarnon',),
    'burst shot': ('alt_fire',), 'throw': ('alt_fire',),
    # THE SECOND PASS, 2026-08-20: the catalog names an attack the way that
    # weapon's page does, so the vocabulary is per weapon and a short list
    # misses whole rows silently.
    'projectile impact': ('base',), 'direct hit': ('base',),
    'uncharged direct hit': ('base',), 'primary-fire': ('base',),
    'slug impact': ('base',), 'blob impact': ('base',), 'slug': ('base',),
    'arrow direct hit': ('base',), 'rocket impact': ('base',),
    'partial reload impact': ('base',), 'reload from empty impact': ('base',),
    'lock-on mode': ('alt_fire',), 'burst mode': ('alt_fire',),
    'alt fire impact': ('alt_fire', 'charged'),
    'perfect shot': ('charged',),
}


def pct(s):
    m = re.match(r'([\d.]+)\s*%', s.replace(',', ''))
    return float(m.group(1)) / 100.0 if m else None


fixed = 0
for r in rows:
    ids = fam.get(r['weapon'])
    if not ids:
        continue
    key = re.sub(r'\s*\(.*\)$', '', r['attack'].lower().strip())
    forms = FORMS.get(key)
    if forms is None:
        continue
    kind = r['kind'].lower()
    want = 'independent' if kind.startswith('multipl') else \
        ('additive_with_base_damage' if kind.startswith('add') else None)
    if want is None:
        continue
    frac = pct(r['rel'])
    for wid in sorted(ids):
        d = specs[wid]
        if d.get('form') not in forms:
            continue
        have_k = d.get('co_behavior')
        have_f = float(d.get('co_base_fraction', 1.0))
        if have_k == want and (not frac or abs(have_f - frac) <= 5e-3):
            continue
        s = io.open(files[wid], encoding='utf-8').read()
        verbatim = ('%s | %s | %s | %s | %s | %s | %s'
                    % (r['weapon'], r['attack'], r['shot'], r['unmodded'],
                       r['bonus'], r['rel'], r['kind']))
        block = ['# Condition Overload: THE CATALOG NAMES THIS ATTACK. Verbatim',
                 '# (docs/CATALOGS.md §1):',
                 '#']
        block += ['#   ' + w for w in textwrap.wrap(verbatim, 70, break_long_words=False)]
        if r['notes']:
            block += ['#',
                      '# Notes cell:']
            block += ['#   ' + w for w in textwrap.wrap(r['notes'], 70, break_long_words=False)]
        block += ['#',
                  '# THIS ENTRY READ %s AT %g%% UNTIL 2026-08-20, which was the'
                  % ('ADDING' if have_k == 'additive_with_base_damage' else 'MULTIPLYING',
                     have_f * 100),
                  '# ordinary class assumed because nobody had opened the catalog for this',
                  '# weapon — the check was run against our own transcription of it, which',
                  '# only ever carried the rows the roster already had.']
        block.append('co_behavior: %s' % want)
        if frac and abs(frac - 1.0) > 1e-9:
            block.append('co_base_fraction: %g' % frac)

        # replace the whole existing co_behavior comment + line
        lines = s.split('\n')
        i = next(i for i, l in enumerate(lines) if l.startswith('co_behavior:'))
        j = i
        while j > 0 and (lines[j - 1].startswith('#') or lines[j - 1].strip() == ''):
            if lines[j - 1].strip() == '':
                break
            j -= 1
        end = i + 1
        if end < len(lines) and lines[end].startswith('co_base_fraction:'):
            end += 1
        lines[j:end] = block
        io.open(files[wid], 'w', encoding='utf-8', newline='').write('\n'.join(lines))
        fixed += 1
        print('%-26s %-26s -> %-26s %s' % (wid, have_k, want, frac or ''))

print('%d entries corrected' % fixed)
