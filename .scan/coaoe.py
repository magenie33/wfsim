# -*- coding: utf-8 -*-
"""Set `takes_condition_overload` where the catalog says the AoE part takes it.

The per-part FRACTION is not expressible — `co_base_fraction` is one number per
ENTRY — so each of these carries an admission naming the number it cannot hold,
which is the call the Pox has carried since its own 250% row.
"""
import io

# entry -> (which part, the verbatim row, the relative column, over/under)
FIX = {
    'ambassador_charged': (
        'radial',
        'Ambassador | Alt-fire Hitscan Radial Attack | Hitscan | 400 | 300 | 75% | Adding',
        0.75),
    'mutalist_cernos': (
        'lingering',
        'Mutalist Cernos | Charged AoE Toxin Cloud | AoE | 5 | 205 | 4100% | Adding',
        41.0),
    'ferrox': (
        'radial',
        'Ferrox | Hitscan AoE Direct | Hitscan | 200 | 700 | 350% | Adding',
        3.5),
    'tenet_ferrox': (
        'radial',
        'Tenet Ferrox | Hitscan AoE Direct | Hitscan | 240 | 800 | 333% | Adding',
        3.33),
    'opticor': (
        'radial',
        'Opticor | Charged Hitscan Direct Hit Radial | Hitscan | 400 | 1000 | 250% | Adding',
        2.5),
    'opticor_vandal': (
        'radial',
        'Opticor Vandal | Charged Hitscan Radial Attack | Hitscan | 300 | 600 | 200% | Adding',
        2.0),
    'trumna': (
        'radial',
        'Trumna | Main-fire Hitscan Radial Attack | Hitscan | 55 | 90 | 164% | Adding',
        1.64),
}

import glob  # noqa: E402

import yaml  # noqa: E402

files = {}
for f in sorted(glob.glob('data/weapons/*/*.yaml')):
    d = yaml.safe_load(io.open(f, encoding='utf-8'))
    files[d['id']] = f.replace('\\', '/')

for wid, (part, row, rel) in FIX.items():
    p = files[wid]
    s = io.open(p, encoding='utf-8').read()
    assert '\n  %s:' % part in s, (wid, part)
    assert 'takes_condition_overload' not in s, wid
    # the block's last field line is where the flag goes: put it right after the
    # part's `damage:` line, which every part has.
    idx = s.index('\n  %s:' % part)
    dmg = s.index('\n    damage:', idx)
    end = s.index('\n', dmg + 1)
    note = (
        '\n    # THE CATALOG SAYS THIS AoE PART TAKES CONDITION OVERLOAD, which almost\n'
        '    # no area part does. Verbatim (docs/CATALOGS.md §1):\n'
        '    #\n'
        '    #   %s\n'
        '    #\n'
        '    # The RELATIVE column is %g%% and there is no per-part CO fraction —\n'
        '    # `co_base_fraction` is one number per ENTRY and this entry\'s direct hit\n'
        '    # has its own — so this part takes the term at 100%% of its own base and\n'
        '    # the difference is admitted below. It read FALSE until 2026-08-20, which\n'
        '    # was the whole term missing rather than the fraction being off.\n'
        '    takes_condition_overload: true' % (row, rel * 100))
    s = s[:end] + note + s[end:]

    which = 'understated' if rel > 1.0 else 'overstated'
    line = ('  - "the CATALOG gives this weapon\'s %s its own Condition Overload row at %g%% of '
            'its base (\'%s\'), and there is no per-part CO fraction here — `co_base_fraction` is '
            'one number per ENTRY. So the part takes the term at 100%% of its own base and a '
            'status-stacking build is %s on it"'
            % (part, rel * 100, row, which))
    if '\nunmodeled:\n' in s:
        s = s.replace('\nunmodeled:\n', '\nunmodeled:\n' + line + '\n', 1)
    else:
        s = s.replace('\nsource:\n', '\nunmodeled:\n' + line + '\n\nsource:\n', 1)
    io.open(p, 'w', encoding='utf-8', newline='').write(s)
    print('%-22s %s takes CO (row says %g%%)' % (wid, part, rel * 100))
