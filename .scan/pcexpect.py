# -*- coding: utf-8 -*-
"""Emit the Rust `table` rows for the compression cross-check, and flag any
entry whose own radius disagrees with the table's Base Radius column."""
import glob
import io

import yaml

# id -> the table's published (base radius m, max damage bonus as a fraction)
PUB = {
    'acceltra': (4.0, 3.20), 'acceltra_prime': (5.0, 4.00),
    'aeolak_alt': (7.0, 5.60),
    'afentis': (3.0, 2.40), 'afentis_prime': (5.5, 4.40),
    'alternox_alt': (6.0, 4.80), 'alternox_prime_alt': (6.0, 4.80),
    'ambassador_charged': (6.0, 4.80),
    'proboscis_cernos': (7.0, 5.60),
    'evensong': (4.0, 3.20),
    'lenz': (7.2, 5.75), 'prisma_lenz': (7.2, 5.75),
    'kuva_bramma': (8.3, 6.64),
    'astilla': (2.4, 1.92), 'astilla_prime': (2.4, 1.92),
    'basmu': (1.7, 1.36),
    'battacor_charged': (3.4, 2.08),
    'corinth_airburst': (9.4, 7.52), 'corinth_prime_airburst': (9.8, 7.84),
    'cedo_alt': (6.0, 4.80), 'cedo_prime_alt': (6.0, 4.80),
    'kuva_chakkhurr': (2.9, 2.32),
    'javlok': (2.4, 1.92), 'javlok_throw': (6.0, 4.80),
    'ogris': (7.1, 5.68), 'kuva_ogris': (7.9, 6.32),
    'opticor': (6.0, 4.80), 'opticor_quick': (6.0, 4.80),
    'opticor_vandal': (4.6, 3.68), 'opticor_vandal_quick': (4.6, 3.68),
    'panthera_prime': (1.6, 1.28),
    'penta': (4.0, 3.20), 'secura_penta': (4.0, 3.20), 'carmine_penta': (4.0, 3.20),
    'mutalist_quanta_orb': (4.4, 3.52),
    'simulor': (5.0, 4.00), 'synoid_simulor': (5.0, 4.00),
    'sporothrix': (1.7, 1.36), 'coda_sporothrix': (2.0, 1.60),
    'tonkor': (7.0, 5.60), 'kuva_tonkor': (7.0, 5.60),
    'trumna': (1.6, 1.28), 'trumna_prime': (1.6, 1.28),
    'zarr': (4.9, 3.92), 'kuva_zarr': (7.0, 5.60),
    'zhuge_prime': (2.6, 2.08),
    # "Doesn't Work" rows pay nothing.
    'mutalist_cernos': (0.0, 0.0), 'enkaus_alt': (0.0, 0.0), 'stahlta_charged': (0.0, 0.0),
    'ignis': (0.0, 0.0), 'ignis_wraith': (0.0, 0.0), 'komorex': (0.0, 0.0),
    'glaxion_vandal': (0.0, 0.0), 'vadarya_prime': (0.0, 0.0), 'arbucep': (0.0, 0.0),
    'kuva_ayanga': (0.0, 0.0), 'cortege': (0.0, 0.0), 'cortege_alt': (0.0, 0.0),
    'grattler': (0.0, 0.0), 'kuva_grattler': (0.0, 0.0),
    'morgha': (0.0, 0.0), 'morgha_alt': (0.0, 0.0),
    'larkspur_charged': (0.0, 0.0), 'larkspur_prime_charged': (0.0, 0.0),
}

specs = {}
for f in sorted(glob.glob('data/weapons/*/*.yaml')):
    d = yaml.safe_load(io.open(f, encoding='utf-8'))
    specs[d['id']] = d


def our_radius(d):
    a = d.get('attack') or {}
    for k in ('radial',):
        if a.get(k):
            return a[k].get('radius_m')
    if a.get('lingering'):
        return a['lingering'].get('radius_m')
    b = a.get('beam') or {}
    if b.get('damage_radius_m'):
        return b['damage_radius_m']
    return None


print('rows whose OWN radius disagrees with the table:')
bad = 0
for wid, (pub_r, pub_b) in sorted(PUB.items()):
    d = specs.get(wid)
    if d is None:
        print('   %-26s NOT AN ENTRY' % wid)
        continue
    r = our_radius(d)
    if pub_r and (r is None or abs(float(r) - pub_r) > 1e-6):
        print('   %-26s ours %-8s table %s' % (wid, r, pub_r))
        bad += 1
print('   (%d)' % bad)

print('\n// paste into the table')
for wid, (pub_r, pub_b) in sorted(PUB.items()):
    d = specs.get(wid)
    r = our_radius(d) if d else None
    calc = ((d.get('attack') or {}).get('compression') or {}).get('radius_calculation') if d else ''
    if calc == 'doesnt_work':
        print('            ("%s", 0.0),' % wid)
        continue
    exact = round(float(r) * 0.8, 6) if r else 0.0
    note = '' if abs(exact - pub_b) < 5e-3 else '   // table prints +%g%%' % (pub_b * 100)
    print('            ("%s", %.2f),%s' % (wid, exact, note))
