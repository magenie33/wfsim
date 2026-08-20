# -*- coding: utf-8 -*-
"""The Castanas family takes NO Condition Overload — a 0% row on every attack."""
import io

WHY = {
    'castanas': [
        '# Condition Overload: THE CATALOG NAMES THIS ATTACK, and its answer is that',
        '# it does not take the term at all. Verbatim (docs/CATALOGS.md §1):',
        '#',
        '#   Castanas | Normal Attack | AoE | 160 | 0 | 0% | N/A',
        '#',
        '#   Notes cell: "Does not apply"',
        '#',
        '# `inert` is that row. This entry read Adding at 100% until 2026-08-20,',
        '# which gave the whole 160 Electricity a Condition Overload term the game',
        '# does not — and on a mine whose damage IS the blast, that is the whole',
        '# weapon. The Sonicor and the Stug carry the same kind of row.',
    ],
    'sancti_castanas': [
        '# Condition Overload: THE CATALOG NAMES BOTH OF THIS WEAPON\'S ATTACKS, and',
        '# gives both the same answer. Verbatim (docs/CATALOGS.md §1):',
        '#',
        '#   Sancti Castanas | Mid-Flight Detonation | AoE | 300 | 0 | 0% | N/A',
        '#   Sancti Castanas | Embedded Detonation   | AoE | 300 | 0 | 0% | N/A',
        '#',
        '#   Notes cell on both: "Does not apply"',
        '#',
        '# `inert` is that pair. The entry carries the EMBEDDED detonation as its',
        '# own damage (see the admission below), and the catalog says neither',
        '# detonation takes the term — so the class is the same whichever one is',
        '# modelled. It read Adding at 100% until 2026-08-20.',
    ],
}

for wid, block in WHY.items():
    p = 'data/weapons/secondary/%s.yaml' % wid
    s = io.open(p, encoding='utf-8').read()
    lines = s.split('\n')
    i = next(i for i, l in enumerate(lines) if l.startswith('co_behavior:'))
    j = i
    while j > 0 and lines[j - 1].startswith('#'):
        j -= 1
    end = i + 1
    if end < len(lines) and lines[end].startswith('co_base_fraction:'):
        end += 1
    lines[j:end] = block + ['co_behavior: inert']
    io.open(p, 'w', encoding='utf-8', newline='').write('\n'.join(lines))
    print(wid, '-> inert')

# and the Kulstar's bomblet row is an AoE part this tool should not try to place
p = 'scripts/audit_condition_overload.py'
s = io.open(p, encoding='utf-8').read()
old = "'turret bullets', 'disk bonk', 'appendages', 'spore')"
new = "'turret bullets', 'disk bonk', 'appendages', 'spore', 'cluster bombs')"
assert old in s
io.open(p, 'w', encoding='utf-8', newline='').write(s.replace(old, new, 1))
print('AOE_WORDS += cluster bombs')
