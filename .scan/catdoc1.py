# -*- coding: utf-8 -*-
"""Record the 2026-08-20 CO sweep in docs/CATALOGS.md §1."""
import glob
import io

import yaml

specs = {}
for f in sorted(glob.glob('data/weapons/*/*.yaml')):
    d = yaml.safe_load(io.open(f, encoding='utf-8'))
    specs[d['id']] = d

SWEPT = ['acceltra', 'aegrit', 'aeolak', 'aeolak_alt', 'alternox', 'alternox_prime', 'basmu',
         'battacor', 'buzlok', 'buzlok_beacon', 'catabolyst', 'cernos', 'cinta', 'cinta_charged',
         'cyanex', 'cyanex_burst', 'daikyu_prime', 'drakgoon', 'epitaph', 'epitaph_uncharged',
         'evensong', 'exergis', 'fulmin_semi', 'harpak_harpoon', 'javlok', 'lanka', 'laser_rifle',
         'mutalist_cernos', 'mutalist_cernos_uncharged', 'nataruk_perfect', 'paracyst_harpoon',
         'prime_laser_rifle', 'quellor_alt', 'rakta_cernos', 'seer', 'sepulcrum',
         'sepulcrum_lockon', 'sonicor', 'stahlta', 'stahlta_charged', 'steflos', 'tenet_diplos_lock_on',
         'tenet_envoy', 'trumna_grenade']

rows = []
for wid in sorted(SWEPT):
    d = specs.get(wid)
    if d is None:
        raise SystemExit('not an entry: ' + wid)
    rows.append('| `%s` | %s | %s |' % (
        wid, d['co_behavior'],
        ('%g%%' % (float(d.get('co_base_fraction', 1.0)) * 100))))

SECTION = '''
### THE 2026-08-20 SWEEP — forty-four entries the catalog named and the roster contradicted

**A method error, and it is the useful part of this entry.** Every weapon yaml
written this month opened with *"NO row in the wiki's CO catalog (re-read
2026-08-20)"*. That check was run against **THIS FILE** — our own transcription,
which by construction carries only "rows the roster already has". Asking it
whether a NEW weapon has a row can only ever answer no. The check has to read
the WIKI PAGE, and when it finally did it found forty-four disagreements.

**Not all of them were new.** The Lanka has read Adding at 100% since it was
written and its row says 38%; both Laser Rifles, the whole Cernos family and the
Catabolyst are the same story. Condition Overload is on most builds, so each of
these was a wrong damage number rather than a wrong comment.

**And it took TWO passes, for a reason worth writing down.** The first
reconciliation matched a row to a form through a short list of attack NAMES —
"Normal Attack", "Alt-fire", "Charged Attack". The catalog names an attack the
way that WEAPON's page does, so "Projectile Impact", "Direct Hit", "Lock-On
Mode", "Slug Impact", "Burst Mode" and "Reload From Empty Impact" matched
nothing and were skipped in silence. A narrow vocabulary does not fail, it
under-reports.

| our entry | behaviour | co_base_fraction |
| --- | --- | --- |
@@ROWS@@

**Seven AoE PARTS** were reading `takes_condition_overload: false` where the
catalog gives them their own row — not a fraction being off, the WHOLE term
missing from an explosion that is most of the weapon: the Ambassador's radial
(75%), both Ferroxes (350% / 333%), both Opticors (250% / 200%), the Trumna's
main fire (164%), and the Mutalist Cernos's charged cloud at **4100%**, which is
the most extreme relative column in the catalog. The per-part fraction is still
not expressible — `co_base_fraction` is one number per ENTRY — and each says so
on its card, which is the call the Pox has carried since its own 250% row.

'''.replace('@@ROWS@@', chr(10).join(rows))

p = 'docs/CATALOGS.md'
s = io.open(p, encoding='utf-8').read()
anchor = '### "CO-bonus does not use base damage increase Evolution" — eleven rows'
assert anchor in s
io.open(p, 'w', encoding='utf-8', newline='').write(s.replace(anchor, SECTION + anchor, 1))
print('added %d rows to docs/CATALOGS.md §1' % len(rows))
