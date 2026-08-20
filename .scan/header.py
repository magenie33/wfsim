# -*- coding: utf-8 -*-
"""Correct the header claim on the entries where WFCD does NOT agree.

Every weapon yaml opens with "cross-checked against WFCD warframe-items — 0
disagreements". For twenty-three entries that sentence is false, and a header
that claims a check it fails is worse than no header.
"""
import glob
import io

WHY = {
    'angstrum': "WFCD's magazine is in SHOTS (3 rounds = 1 charged shot)",
    'prisma_angstrum': "WFCD's magazine is in SHOTS (3 rounds = 1 charged shot)",
    'fulmin': "WFCD's magazine is in SHOTS (60 rounds = 6 at 10 apiece)",
    'fulmin_prime': "WFCD's magazine is in SHOTS (80 rounds = 8 at 10 apiece)",
    'panthera': "WFCD's magazine is in SHOTS (60 rounds = 30 at 2 apiece)",
    'panthera_prime': "WFCD's magazine is in SHOTS (80 rounds = 40 at 2 apiece)",
    'staticor': "WFCD's magazine is in SHOTS (48 rounds = 12 at 4 apiece)",
    'twin_grakatas': "WFCD's magazine is in SHOTS (120 rounds = 60 at 2 apiece)",
    'basmu': 'WFCD carries the PARTIAL reload, this is the wiki\'s full one',
    'shedu': 'WFCD carries the PARTIAL reload, this is the wiki\'s full one',
    'nataruk': "WFCD's reload is the nock, not the wiki's figure",
    'flux_rifle': 'WFCD gives a different RELOAD; the wiki wins',
    'efv_8_mars': 'WFCD gives a different RELOAD; the wiki wins',
    'riot_848': 'WFCD gives a different RELOAD; the wiki wins',
    'tenet_detron': 'WFCD gives a different RELOAD; the wiki wins',
    'grimoire': "a Tome does not reload; WFCD's 0.01 s is a floor",
    'coda_bassocyst': 'WFCD holds an older DISPOSITION',
    'coda_bubonico': 'WFCD holds an older DISPOSITION',
    'tenet_diplos': 'WFCD holds an older DISPOSITION',
    'tenet_plinx': 'WFCD holds an older DISPOSITION',
    'thornbak': 'WFCD holds an older DISPOSITION',
    'vinquibus': 'WFCD holds an older DISPOSITION',
    'furis': 'WFCD holds an older MASTERY RANK',
}

OLD = '# against WFCD warframe-items — 0 disagreements).'
done = set()
for f in sorted(glob.glob('data/weapons/*/*.yaml')):
    s = io.open(f, encoding='utf-8').read()
    wid = s.split('\n', 1)[0].replace('id: ', '').strip()
    if wid not in WHY or OLD not in s:
        continue
    new = ('# against WFCD warframe-items, which DISAGREES on one field:\n'
           '# %s. The wiki wins (data/README.md), and\n'
           '# `scripts/audit_weapon_stats.py` carries the divergence with its reason\n'
           '# so it cannot be mistaken for a transcription error.' % WHY[wid])
    io.open(f, 'w', encoding='utf-8', newline='').write(s.replace(OLD, new, 1))
    done.add(wid)
    print('corrected', f)

missing = set(WHY) - done
if missing:
    print('NOT FOUND:', sorted(missing))
