"""Every WEAPON-EXCLUSIVE gun mod our roster can equip, from WFCD's export.

A mod that only fits one weapon is invisible to every other check we have: the
mod pools are built from what `data/mods/` contains, so a mod nobody has
transcribed is a mod the builder cannot offer and nothing notices. The Dread's
Unseen Dread and the Latron's Double Tap sat missing that way until someone
looked at a wiki page.

THE JOIN IS `compatName` x our weapon names, minus what we already carry by
`internal_name` — never by display name, which WFCD duplicates (AGENTS.md).
`compatName` names the BASE of a family and every variant inherits it, which is
why the Latron's augment lists for the Wraith and the Prime too.

Writes `data/surveys/weapon_exclusive_mods.yaml`, which a test reads and nothing else
does. The survey is the FACT (what exists in game); the test is what stops the
gap growing quietly.

    python scripts/survey_weapon_mods.py
"""
import glob
import io
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXPORT = os.path.join(ROOT, 'vendor/warframe-items/data/json/Mods.json')
OUT = os.path.join(ROOT, 'data/surveys/weapon_exclusive_mods.yaml')
GUN_TYPES = {'Primary Mod', 'Secondary Mod', 'Shotgun Mod'}

# MODS THIS ROSTER DELIBERATELY DOES NOT CARRY, and why.
#
# The join is `compatName` against our weapon names, which answers "does a mod
# for this weapon exist in the export" — and that is not the same question as
# "may a player equip it in the mission we simulate". Two reasons it is not,
# and both make a row here rather than a gap:
#
#   PvP — a Conclave-exclusive mod is a separate balance pass and cannot be
#   equipped in PvE at all. `engine::mods_data`'s
#   `only_pve_legal_conclave_mods_are_in_the_pools` already refuses to let one
#   into a pool; without this table the survey would keep pointing at six mods
#   that test forbids. The evidence is each page's own opening sentence, which
#   says "is Conclave-exclusive" beside a {{PvPItem}} banner — the strongest
#   form of the tag the wiki's mod tables carry. Double Tap is the contrast
#   that proves it is read and not assumed: same `/PvPMods/` path, and its page
#   says "is a PvE and Conclave … mod", so it IS carried.
#
#   UNRELEASED — a mod DE built and never shipped is in the export beside the
#   real ones, because WFCD reads the game's files.
#
# Keyed by `uniqueName`, never by display name (AGENTS.md).
EXCLUDED = {
    '/Lotus/Upgrades/Mods/PvPMods/Pistol/DespairEnergyDrainAoE':
        'PvP: "Draining Gloom is Conclave-exclusive Despair mod".',
    '/Lotus/Upgrades/Mods/PvPMods/Rifle/FasterRoFonKillGorgonMod':
        'PvP: "Gorgon Frenzy is Conclave-exclusive Gorgon mod".',
    '/Lotus/Upgrades/Mods/PvPMods/Rifle/MoreAccuracyOnHitGrinlokMod':
        'PvP: "Grinloked is Conclave-exclusive Grinlok mod".',
    '/Lotus/Upgrades/Mods/PvPMods/Rifle/SybarisIncreaseRoFonHit':
        'PvP: "Sudden Justice is Conclave-exclusive Sybaris mod".',
    '/Lotus/Upgrades/Mods/PvPMods/Rifle/ExplodingMiterBlades':
        'PvP: "Thundermiter is Conclave-exclusive Miter mod".',
    '/Lotus/Upgrades/Mods/PvPMods/Rifle/MoreDamageonMultiHitRifleMod':
        'PvP: "Triple Tap is Conclave-exclusive Burston mod".',
    '/Lotus/Upgrades/Mods/Syndicate/BallisticaMod':
        'unreleased: "Soaring Truth" has no wiki page, no row in '
        'Template:AugmentedMods (which lists every released augment), and is '
        'not among warframe.market\'s 3,837 tradeable items — while its three '
        'sibling syndicate augments are in all three. Checked 2026-08-13.',
}


def carried():
    """internal_name -> the file that carries it."""
    out = {}
    for p in glob.glob(os.path.join(ROOT, 'data/mods/**/*.yaml'), recursive=True):
        t = io.open(p, encoding='utf-8').read()
        m = re.search(r'^internal_name:\s*(\S+)', t, re.M)
        if m:
            out[m.group(1).strip()] = os.path.relpath(p, ROOT).replace(os.sep, '/')
    return out


def weapon_names():
    out = set()
    for p in glob.glob(os.path.join(ROOT, 'data/weapons/**/*.yaml'), recursive=True):
        t = io.open(p, encoding='utf-8').read()
        m = re.search(r'^name:\s*(.+)$', t, re.M)
        if m:
            out.add(m.group(1).strip().strip('"'))
    return out


def fits(compat, names):
    """Does `compatName` name a weapon we carry, or the base of one?"""
    if not compat:
        return []
    return sorted(n for n in names
                  if n == compat or n.startswith(compat + ' ') or n.endswith(' ' + compat))


def main():
    if not os.path.exists(EXPORT):
        sys.exit('%s missing — run scripts/vendor.py first' % EXPORT)
    mods = json.load(io.open(EXPORT, encoding='utf-8'))
    have, names = carried(), weapon_names()

    rows = []
    for m in mods:
        if m.get('type') not in GUN_TYPES:
            continue
        who = fits(m.get('compatName'), names)
        if not who:
            continue
        uniq = m.get('uniqueName', '')
        rows.append({
            'name': m['name'],
            'compat': m['compatName'],
            'kind': m['type'],
            'internal_name': uniq,
            'carried': have.get(uniq),
            'excluded': EXCLUDED.get(uniq),
            'weapons': who,
        })
    rows.sort(key=lambda r: r['name'])

    miss = [r for r in rows if not r['carried'] and not r['excluded']]
    excl = [r for r in rows if r['excluded']]
    lines = [
        '# EVERY WEAPON-EXCLUSIVE GUN MOD OUR ROSTER CAN EQUIP.',
        '#',
        '# GENERATED by scripts/survey_weapon_mods.py from WFCD\'s export — read by',
        '# a TEST and by nothing else. Do not hand-edit; re-run the script.',
        '#',
        '# A mod that fits one weapon is invisible to every other check: the pools are',
        '# built from what data/mods/ holds, so one nobody transcribed is one the',
        '# builder cannot offer and nothing notices.',
        '#',
        '# `carried:` is the file that holds it; `~` is a gap; `excluded` is a mod we',
        '# refuse on purpose, and the reason rides beside it. The export answers "does',
        '# a mod for this weapon exist", which is not "may a player equip it here".',
        '#',
        '# %d of %d carried, %d excluded on purpose, %d still to transcribe.'
        % (len(rows) - len(miss) - len(excl), len(rows), len(excl), len(miss)),
        '#',
        '# THE ROSTER THIS WAS JOINED AGAINST, because a survey nobody re-runs',
        '# reports the question it was asked LAST time. This file sat at 20 rows',
        '# and "0 still to transcribe" while the real answer grew to 197 and 103,',
        '# and the ratchet reading it passed the whole way — a weapon added after',
        '# the last run can only ever be absent from a generated file, which is',
        '# the lesson docs/CATALOGS.md already records in another domain.',
        '#',
        '# The test compares this to the live roster, so ADDING A WEAPON makes it',
        '# stale and RED. Only this script can clear it, and only on a machine',
        '# with `vendor/` — which is why the guard is a comparison here rather',
        '# than a regeneration in CI, where the export does not exist.',
        'roster: %d' % len(names),
        'mods:',
    ]
    for r in rows:
        lines.append('  - name: %s' % r['name'])
        lines.append('    compat: %s' % r['compat'])
        lines.append('    kind: %s' % r['kind'])
        lines.append('    internal_name: %s' % r['internal_name'])
        if r['excluded']:
            lines.append('    carried: excluded')
            lines.append('    reason: %s' % json.dumps(r['excluded']))
        else:
            lines.append('    carried: %s' % (r['carried'] or '~'))
    io.open(OUT, 'w', encoding='utf-8', newline='\n').write('\n'.join(lines) + '\n')
    print('%s: %d rows, %d carried, %d excluded, %d missing' % (
        os.path.relpath(OUT, ROOT), len(rows),
        len(rows) - len(miss) - len(excl), len(excl), len(miss)))
    for r in excl:
        print('   EXCLUDED %-24s %s' % (r['name'], r['excluded'][:60]))
    for r in miss:
        print('   MISSING  %-24s (%s) -> %s' % (r['name'], r['kind'], ', '.join(r['weapons'][:3])))


if __name__ == '__main__':
    main()
