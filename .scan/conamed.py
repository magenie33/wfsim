# -*- coding: utf-8 -*-
"""Add the thirty-five corrected entries to the CO anomaly roll call."""
import glob
import io

import yaml

WANT = ['acceltra', 'aeolak', 'aeolak_alt', 'alternox', 'alternox_prime', 'basmu', 'battacor',
        'buzlok', 'buzlok_beacon', 'cernos', 'cinta', 'cinta_charged', 'daikyu_prime',
        'drakgoon', 'evensong', 'exergis', 'fulmin_semi', 'harpak_harpoon', 'javlok', 'lanka',
        'mutalist_cernos', 'mutalist_cernos_uncharged', 'nataruk_perfect', 'paracyst_harpoon',
        'quellor_alt', 'rakta_cernos', 'stahlta', 'stahlta_charged', 'steflos', 'tenet_envoy',
        'trumna_grenade', 'epitaph', 'seer', 'laser_rifle', 'prime_laser_rifle']

specs = {}
for f in sorted(glob.glob('data/weapons/*/*.yaml')):
    d = yaml.safe_load(io.open(f, encoding='utf-8'))
    specs[d['id']] = d

lines = []
for wid in sorted(WANT):
    d = specs[wid]
    lines.append('            ("%s", "%s", %.4g),'
                 % (wid, d['co_behavior'], float(d.get('co_base_fraction', 1.0))))

NOTE = [
    '',
    '            // THE 2026-08-20 SWEEP, and the reason it found so many at once:',
    '            // every one of these was filed ORDINARY because the check for a row',
    "            // had been run against docs/CATALOGS.md — our own transcription,",
    '            // which by construction only ever carried the rows the roster already',
    '            // had. Reading the WIKI PAGE instead turned up thirty-five entries the',
    '            // Attack Catalog names and the roster contradicted, a third of them',
    '            // weapons that had been here for months (the Lanka at 38%, both Laser',
    '            // Rifles, the Cernos family at 50%).',
    '            //',
    '            // A "Multiplying" row is `independent`; the relative column is',
    '            // `co_base_fraction`, and 100% leaves the field off.',
]

p = 'engine/src/weapons_data.rs'
s = io.open(p, encoding='utf-8').read()
i = s.index('const NAMED: &[(&str, &str, f64)] = &[')
j = s.index('\n        ];', i)
s = s[:j] + '\n' + '\n'.join(NOTE + lines).rstrip('\n') + s[j:]
io.open(p, 'w', encoding='utf-8', newline='').write(s)
print('added %d rows to NAMED' % len(lines))
