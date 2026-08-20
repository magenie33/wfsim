# -*- coding: utf-8 -*-
"""Extend the compression cross-check table and its roll calls."""
import io

rows = [l.rstrip('\n') for l in io.open('.scan/rows.txt', encoding='utf-8')
        if l.strip().startswith('("')]
rows = [r for r in rows if '"glaxion_vandal"' not in r]

NOTE = [
    '',
    '            // THE 2026-08-20 SWEEP. The published table named FIFTY-NINE more',
    '            // roster attacks than the roster had transcribed — most of them from',
    "            // this month's intake, and a dozen that had been here far longer. An",
    '            // attack with no `compression:` pays the arcane NOTHING (',
    '            // `loadout::resolve` reads `Some(c)` or nothing at all), so every one',
    '            // of them was silently worth zero to a build carrying it.',
    '            //',
    '            // Each figure below is OUR radius x 0.8, which is what the arcane',
    "            // takes. Where that disagrees with the table's own Max Damage Bonus",
    '            // column the line says so, and there are exactly three:',
    '            //',
    '            //   lenz / prisma_lenz — 7.2 m x 0.8 is 5.76 and the table rounds its',
    '            //     own arithmetic to +575%.',
    '            //   secura_penta — the table gives the three Pentas ONE row at 4.0 m,',
    "            //     and this weapon's own module row is 6.0 m. The weapon wins.",
    "            //   battacor_charged — the table's radius column says 3.4 m and its",
    '            //     bonus column says +208%, which is 2.6 m. The table disagrees',
    '            //     with ITSELF there; ours follows its radius column.',
]

p = 'engine/src/weapons_data.rs'
s = io.open(p, encoding='utf-8').read()

anchor = '            ("glaxion_vandal", 0.0),\n        ];'
assert anchor in s
new = '            ("glaxion_vandal", 0.0),\n' + '\n'.join(NOTE + rows) + '\n        ];'
s = s.replace(anchor, new, 1)

old_adds = '        assert!(rows >= 20, "only {rows} rows transcribed");'
assert old_adds in s
s = s.replace(old_adds, '        assert!(rows >= 80, "only {rows} rows transcribed");', 1)

io.open(p, 'w', encoding='utf-8', newline='').write(s)
print('ok, %d rows added' % len(rows))
