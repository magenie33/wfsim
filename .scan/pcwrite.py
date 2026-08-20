# -*- coding: utf-8 -*-
"""Transcribe the published Primary Compression rows the roster was missing.

One row, one ENTRY — the form the row's attack column names. Where the table
gives a whole family one row (the Pentas, the Opticors) the comment says so.
"""
import glob
import io

import yaml

M = 'multiplies'
A = 'adds'
SNAP = 'snapshot'
DEAD = 'doesnt_work'

# id -> (effectiveness, stacking, radius_calculation, the row, verbatim)
ROWS = {
    # --- 100% / Multiplies / Snapshot, the ordinary row -------------------
    'acceltra': (1.00, M, SNAP, 'Acceltra (Acceltra Prime) | Primary Fire + AoE | 100% | Multiplies | Snapshot | 4.0 m | +320%'),
    'acceltra_prime': (1.00, M, SNAP, 'Acceltra (Acceltra Prime) | Primary Fire + AoE | 100% | Multiplies | Snapshot | 5.0 m | +400%'),
    'aeolak_alt': (1.00, M, SNAP, 'Aeolak | Alt-Fire + AoE | 100% | Multiplies | Snapshot | 7.0 m | +560%'),
    'afentis': (1.00, M, SNAP, 'Afentis (Afentis Prime) | Primary Fire + AoE | 100% | Multiplies | Snapshot | 3.0 m | +240%'),
    'afentis_prime': (1.00, M, SNAP, 'Afentis (Afentis Prime) | Primary Fire + AoE | 100% | Multiplies | Snapshot | 5.5 m | +440%'),
    'alternox_alt': (1.00, M, SNAP, 'Alternox (Alternox Prime) | Alt-Fire + AoE | 100% | Multiplies | Snapshot | 6.0 m | +480% | "Pulse radius is not reduced, damage bonus is granted based off alt-fire radius."'),
    'alternox_prime_alt': (1.00, M, SNAP, 'Alternox (Alternox Prime) | Alt-Fire + AoE | 100% | Multiplies | Snapshot | 6.0 m | +480%'),
    'proboscis_cernos': (1.00, M, SNAP, 'Proboscis Cernos | Charged Shot + AoE | 100% | Multiplies | Snapshot | 7.0 m | +560% | "Pull radius is not reduced. Damage bonus is not granted to tendril tick damage."'),
    'evensong': (1.00, M, SNAP, 'Evensong | Charged Shot + AoE | 100% | Multiplies | Snapshot | 4.0 m | +320%'),
    'lenz': (1.00, M, SNAP, 'Lenz (Prisma Lenz) | Charged Shot + AoE | 100% | Multiplies | Snapshot | 7.2 m | +575%'),
    'prisma_lenz': (1.00, M, SNAP, 'Lenz (Prisma Lenz) | Charged Shot + AoE | 100% | Multiplies | Snapshot | 7.2 m | +575%'),
    'kuva_bramma': (1.00, M, SNAP, 'Kuva Bramma | Charged Shot + AoE | 100% | Multiplies | Snapshot | 8.3 m | +664%'),
    'astilla': (1.00, M, SNAP, 'Astilla (Astilla Prime) | Primary Fire + AoE | 100% | Multiplies | Snapshot | 2.4 m | +192% | "Shotguns cannot equip mod"'),
    'astilla_prime': (1.00, M, SNAP, 'Astilla (Astilla Prime) | Primary Fire + AoE | 100% | Multiplies | Snapshot | 2.4 m | +192% | "Shotguns cannot equip mod"'),
    'basmu': (1.00, M, SNAP, 'Basmu | Primary Fire + AoE | 100% | Multiplies | Snapshot | 1.7 m | +136%'),
    'corinth_airburst': (1.00, M, SNAP, 'Corinth (Corinth Prime) | Alt-Fire + AoE | 100% | Multiplies | Snapshot'),
    'corinth_prime_airburst': (1.00, M, SNAP, 'Corinth (Corinth Prime) | Alt-Fire + AoE | 100% | Multiplies | Snapshot'),
    'cedo_alt': (1.00, M, SNAP, 'Cedo (Cedo Prime) | Alt-Fire + AoE | 100% | Multiplies | Snapshot'),
    'cedo_prime_alt': (1.00, M, SNAP, 'Cedo (Cedo Prime) | Alt-Fire + AoE | 100% | Multiplies | Snapshot'),
    'kuva_chakkhurr': (1.00, M, SNAP, 'Kuva Chakkhurr | Primary Fire + AoE | 100% | Multiplies | Snapshot'),
    'javlok': (1.00, M, SNAP, 'Javlok | Primary Fire + AoE | 100% | Multiplies | Snapshot'),
    'javlok_throw': (1.00, M, SNAP, 'Javlok | Throw + AoE | 100% | Multiplies | Snapshot'),
    'ogris': (1.00, M, SNAP, 'Ogris (Kuva Ogris) | Primary Fire + AoE | 100% | Multiplies | Snapshot'),
    'kuva_ogris': (1.00, M, SNAP, 'Ogris (Kuva Ogris) | Primary Fire + AoE | 100% | Multiplies | Snapshot'),
    'panthera_prime': (1.00, M, SNAP, 'Panthera (Panthera Prime) | Primary Fire + AoE | 100% | Multiplies | Snapshot'),
    'penta': (1.00, M, SNAP, 'Penta (Secura Penta, Carmine Penta) | Primary Fire + AoE | 100% | Multiplies | Snapshot'),
    'secura_penta': (1.00, M, SNAP, 'Penta (Secura Penta, Carmine Penta) | Primary Fire + AoE | 100% | Multiplies | Snapshot'),
    'carmine_penta': (1.00, M, SNAP, 'Penta (Secura Penta, Carmine Penta) | Primary Fire + AoE | 100% | Multiplies | Snapshot'),
    'mutalist_quanta_orb': (1.00, M, SNAP, 'Mutalist Quanta | Orb Explosion AoE | 100% | Multiplies | Snapshot'),
    'simulor': (1.00, M, SNAP, 'Simulor (Synoid Simulor) | Orb Explosion AoE | 100% | Multiplies | Snapshot'),
    'synoid_simulor': (1.00, M, SNAP, 'Simulor (Synoid Simulor) | Orb Explosion AoE | 100% | Multiplies | Snapshot'),
    'sporothrix': (1.00, M, SNAP, 'Sporothrix (Coda Sporothrix) | Primary Fire + AoE | 100% | Multiplies | Snapshot'),
    'coda_sporothrix': (1.00, M, SNAP, 'Sporothrix (Coda Sporothrix) | Primary Fire + AoE | 100% | Multiplies | Snapshot'),
    'tonkor': (1.00, M, SNAP, 'Tonkor (Kuva Tonkor) | Primary Fire + AoE | 100% | Multiplies | Snapshot'),
    'kuva_tonkor': (1.00, M, SNAP, 'Tonkor (Kuva Tonkor) | Primary Fire + AoE | 100% | Multiplies | Snapshot'),
    'zarr': (1.00, M, SNAP, 'Zarr (Kuva Zarr) | Cannon Mode + AoE | 100% | Multiplies | Snapshot | 4.9 m'),
    'kuva_zarr': (1.00, M, SNAP, 'Zarr (Kuva Zarr) | Cannon Mode + AoE | 100% | Multiplies | Snapshot'),
    'zhuge_prime': (1.00, M, SNAP, 'Zhuge Prime | Primary Fire + AoE | 100% | Multiplies | Snapshot | 2.6 m'),

    # --- 100% / ADDS, the minority bracket --------------------------------
    'ambassador_charged': (1.00, A, SNAP, 'Ambassador | Alt-Fire + AoE | 100% | Adds | Snapshot | 6.0 m | +480% | "Loses no AoE radius but gains the expected damage bonus."'),
    'opticor': (1.00, A, SNAP, 'Opticor (Opticor Vandal) | Primary Fire + AoE | 100% | Adds | Snapshot'),
    'opticor_quick': (1.00, A, SNAP, 'Opticor (Opticor Vandal) | Primary Fire + AoE | 100% | Adds | Snapshot'),
    'opticor_vandal': (1.00, A, SNAP, 'Opticor (Opticor Vandal) | Primary Fire + AoE | 100% | Adds | Snapshot'),
    'opticor_vandal_quick': (1.00, A, SNAP, 'Opticor (Opticor Vandal) | Primary Fire + AoE | 100% | Adds | Snapshot'),
    'trumna': (1.00, A, SNAP, 'Trumna (Trumna Prime) | Primary Fire + AoE | 100% | Adds | Snapshot'),
    'trumna_prime': (1.00, A, SNAP, 'Trumna (Trumna Prime) | Primary Fire + AoE | 100% | Adds | Snapshot'),
    'battacor_charged': (1.00, A, 'constant_check', 'Battacor | Alt-Fire + AoE | 100% | Adds | Constant Check'),

    # --- DOESN'T WORK, which is a row and not an omission -----------------
    'mutalist_cernos': (0.0, M, DEAD, "Mutalist Cernos | Toxin Cloud | 0% | Doesn't Work | N/A | 2.5 m"),
    'enkaus_alt': (0.0, M, DEAD, "Enkaus | Alt-Fire + AoE | 0% | Doesn't Work | N/A"),
    'stahlta_charged': (0.0, M, DEAD, "Stahlta | Alt-Fire + AoE | 0% | Doesn't Work | N/A"),
    'ignis': (0.0, M, DEAD, "Ignis (Ignis Wraith) | Primary Fire + AoE | 0% | Doesn't Work | N/A — and the arcane's own page says it: \"Does not work on Continuous Weapons or beam attacks with an AoE component. For example, Ignis\""),
    'ignis_wraith': (0.0, M, DEAD, "Ignis (Ignis Wraith) | Primary Fire + AoE | 0% | Doesn't Work | N/A"),
    'komorex': (0.0, M, DEAD, "Komorex | Primary Fire + AoE | 0% | Doesn't Work | N/A"),
    'glaxion_vandal': (0.0, M, DEAD, "Glaxion Vandal | Primary Fire + AoE | 0% | Doesn't Work | N/A"),
    'vadarya_prime': (0.0, M, DEAD, "Vadarya Prime | Lightning Strikes | 0% | Doesn't work | N/A"),
    'arbucep': (0.0, M, DEAD, "Arbucep | Primary Fire + AoE | 0% | Doesn't Work | N/A | \"Archguns cannot equip mod\""),
    'kuva_ayanga': (0.0, M, DEAD, "Kuva Ayanga | Primary Fire + AoE | 0% | Doesn't Work | N/A | \"Archguns cannot equip mod\""),
    'cortege': (0.0, M, DEAD, "Cortege | Primary Fire + AoE | 0% | Doesn't Work | N/A | \"Archguns cannot equip mod\""),
    'cortege_alt': (0.0, M, DEAD, "Cortege | Alt-Fire + AoE | 0% | Doesn't Work | N/A | \"Archguns cannot equip mod\"; \"Also does not work on napalm.\""),
    'grattler': (0.0, M, DEAD, "Grattler (Kuva Grattler) | Primary Fire + AoE | 0% | Doesn't Work | N/A | \"Archguns cannot equip mod\""),
    'kuva_grattler': (0.0, M, DEAD, "Grattler (Kuva Grattler) | Primary Fire + AoE | 0% | Doesn't Work | N/A | \"Archguns cannot equip mod\""),
    'morgha': (0.0, M, DEAD, "Morgha | Primary Fire + AoE | 0% | Doesn't Work | N/A | \"Archguns cannot equip mod\""),
    'morgha_alt': (0.0, M, DEAD, "Morgha | Alt-Fire + AoE | 0% | Doesn't Work | N/A | \"Archguns cannot equip mod\""),
    'larkspur_charged': (0.0, M, DEAD, "Larkspur (Atmosphere) | Charged Shot + AoE | 0% | Doesn't Work | N/A | \"Archguns cannot equip mod\" — the ARCHWING row is 'Untested', and this roster ships the ATMOSPHERE column"),
    'larkspur_prime_charged': (0.0, M, DEAD, "Larkspur Prime (Atmosphere) | Charged Shot + AoE | 0% | Doesn't Work | N/A | \"Archguns cannot equip mod\""),
}

FAMILY_NOTE = {
    'opticor_quick', 'opticor_vandal_quick',
}

specs, paths = {}, {}
for f in sorted(glob.glob('data/weapons/*/*.yaml')):
    d = yaml.safe_load(io.open(f, encoding='utf-8'))
    specs[d['id']] = d
    paths[d['id']] = f

written, skipped, missing = 0, [], []
for wid, (eff, stack, calc, row) in ROWS.items():
    if wid not in specs:
        missing.append(wid)
        continue
    d = specs[wid]
    if 'compression' in (d.get('attack') or {}):
        skipped.append(wid)
        continue
    s = io.open(paths[wid], encoding='utf-8').read()
    block = ['',
             '  # PRIMARY COMPRESSION — the published table names this attack. Verbatim',
             '  # (docs/CATALOGS.md §2):',
             '  #']
    for chunk in [row[i:i + 74] for i in range(0, len(row), 74)]:
        block.append('  #   %s' % chunk)
    if wid in FAMILY_NOTE:
        block += ['  #',
                  "  # THE TABLE GIVES THIS WEAPON ONE ROW for its explosion and this weapon",
                  '  # has two forms that carry the same one, so both take it.']
    if eff == 0.0:
        block += ['  #',
                  '  # ZERO IS A ROW, NOT AN OMISSION: the arcane does not apply to this',
                  '  # attack at all, and saying so is the difference between "checked" and',
                  '  # "nobody looked".']
    block += ['  compression:',
              '    effectiveness: %.2f' % eff,
              '    stacking: %s' % stack,
              '    radius_calculation: %s' % calc]
    # insert just before the damage line of the attack, which every entry has
    anchor = '\n  damage: {'
    assert anchor in s, wid
    s = s.replace(anchor, '\n'.join(block) + anchor, 1)
    io.open(paths[wid], 'w', encoding='utf-8', newline='').write(s)
    written += 1

print('wrote %d compression rows' % written)
if skipped:
    print('already declared (%d): %s' % (len(skipped), ', '.join(sorted(skipped))))
if missing:
    print('NOT IN THE ROSTER: %s' % ', '.join(sorted(missing)))
