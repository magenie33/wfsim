"""Every CLASS-TAGGED gun mod our roster's pools can hold, from WFCD's export.

The sibling of `survey_weapon_mods.py`, and the half it never covered. That one
joins `compatName` against WEAPON NAMES, which finds the mod written for one
gun; this one joins it against the POOL TAGS — Rifle, Bow, Sniper, Shotgun,
Pistol, Assault Rifle, PRIMARY, Archgun — which is where the other 500 live.

Nothing looked at those, and the cost was invisible in exactly the way a
missing pool is: nine bows have declared `mod_pools: [primary, rifle, bow]`
since the roster began and `data/mods/bow/` did not exist, so the pool resolved
to an empty list and Split Flights, the only multishot mod a bow can hold, was
not offered. Fifteen snipers drew `[primary, rifle]` and no `sniper` at all, so
both Chambers were unreachable (owner, 2026-08-18).

THE POOL SET COMES FROM THE ROSTER, not from a list here: every distinct tag in
every weapon's `mod_pools:` must be a value of `POOL_TAG` below, and the script
FAILS if one is not. That is the check that would have caught `bow` and
`sniper` on the day they were first written down — a pool a weapon claims and
no export tag maps to is a pool that can only ever be empty.

Writes `data/surveys/pool_mods.yaml`, which a test reads and nothing else does.
The survey is the FACT (what exists in game); the test is the ratchet that
stops the gap growing quietly.

    python scripts/survey_pool_mods.py
"""
import glob
import io
import json
import os
import re

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
EXPORT = os.path.join(ROOT, 'vendor/warframe-items/data/json/Mods.json')
OUT = os.path.join(ROOT, 'data/surveys/pool_mods.yaml')

# DE's (type, compatName) pair -> OUR pool directory.
#
# The pair and not the tag alone: "Shotgun" appears under both `Primary Mod`
# and `Shotgun Mod`, and the game means the same pool by both.
#
# The `(No Aoe)` tags are the same pool with an equip rule on top — the rule is
# the MOD's business (`requires_weapon`/`excludes_weapon` on its own card), not
# a reason to file it somewhere the weapon cannot see it.
POOL_TAG = {
    ('Primary Mod', 'PRIMARY'): 'primary',
    ('Primary Mod', 'Rifle'): 'rifle',
    ('Primary Mod', 'Rifle (No Aoe)'): 'rifle',
    ('Primary Mod', 'Assault Rifle'): 'assault_rifle',
    ('Primary Mod', 'Bow'): 'bow',
    ('Primary Mod', 'Sniper'): 'sniper',
    ('Primary Mod', 'Shotgun'): 'shotgun',
    ('Shotgun Mod', 'Shotgun'): 'shotgun',
    ('Secondary Mod', 'Pistol'): 'pistol',
    ('Secondary Mod', 'Pistol (No Aoe)'): 'pistol',
    ('Arch-Gun Mod', 'Archgun'): 'archgun',
}

# WHAT THE EXPORT HOLDS THAT NO PLAYER CAN EQUIP HERE, by RULE rather than by
# name — a per-mod table would be 150 rows of the same three sentences.
#
# Each rule is a function of the `uniqueName`, and each carries the evidence in
# its own text. `survey_weapon_mods.py` keeps a per-`uniqueName` table instead,
# and that is right for it: its rows are one-offs with individual arguments,
# where these are whole families of the game's own bookkeeping.
def rule_out(uniq, name, carried_names):
    # A RIVEN PLACEHOLDER. `/Randomized/Raw*RandomMod` is the unrolled riven
    # itself, which this app models as an EDITOR producing a mod (AGENTS.md),
    # never as a pool entry.
    if '/Randomized/' in uniq:
        return 'riven placeholder: the unrolled riven, which is the riven editor\'s output rather than a pool entry'
    # A FLAWED MOD. DE ships lower-tier copies of the commons under
    # /Beginner/ and /Intermediate/ — same display name, shorter rank ladder,
    # strictly worse than the one already carried. Keyed on the NAME being one
    # we already hold, so a genuinely new mod under those paths is NOT ruled
    # out (Rifle Ammo Mutation and Sniper Ammo Mutation both live there).
    if ('/Beginner/' in uniq or '/Intermediate/' in uniq) and name in carried_names:
        return 'flawed variant: a shorter rank ladder on a mod already carried, strictly worse at every rank'
    # An /Expert/ entry sharing a carried mod's display name is DE's own
    # duplicate bookkeeping, not the Primed version — the Primed ones carry
    # "Primed" in the name and their own `uniqueName` (PrimedWeaponFactionDamageCorpus).
    if '/Expert/' in uniq and name in carried_names:
        return 'export duplicate: an /Expert/ entry under a name already carried, where the real Primed mod has its own uniqueName'
    # CONCLAVE. The path is an ORIGIN and not a restriction — Update 17.9 made
    # a set of these PvE-legal, and thirteen of them are in our pools — so a
    # PvP-path mod we do NOT carry is one the wiki's mod tables tag "Exclusive
    # to PvP". `engine::mods_data::only_pve_legal_conclave_mods_are_in_the_pools`
    # is the other half of the same rule, pinning the survivors by name.
    if '/PvPMods/' in uniq:
        return 'Conclave: a PvP-path mod the wiki mod tables leave tagged "Exclusive to PvP" (the PvE-legal ones are carried and pinned by name in the engine)'
    return None


def carried():
    """Every mod we hold, by `internal_name` and by display name."""
    by_uniq, names = {}, set()
    for p in sorted(glob.glob(os.path.join(ROOT, 'data/mods/*/*.yaml'))):
        text = io.open(p, encoding='utf-8').read()
        u = re.search(r'^internal_name:\s*(\S+)', text, re.M)
        n = re.search(r'^name:\s*(.+)$', text, re.M)
        rel = os.path.relpath(p, ROOT).replace('\\', '/')
        if u:
            by_uniq[u.group(1)] = rel
        if n:
            names.add(n.group(1).strip())
    return by_uniq, names


def roster_pools():
    """Every pool tag any weapon in `data/weapons/` claims."""
    out = set()
    for p in glob.glob(os.path.join(ROOT, 'data/weapons/*/*.yaml')):
        text = io.open(p, encoding='utf-8').read()
        m = re.search(r'^mod_pools:\s*\[([^\]]*)\]', text, re.M)
        if m:
            out.update(t.strip() for t in m.group(1).split(',') if t.strip())
    return out


def main():
    mods = json.load(io.open(EXPORT, encoding='utf-8'))
    have, have_names = carried()

    # THE POOL A WEAPON CLAIMS MUST BE REACHABLE. A tag with no export mapping
    # can only ever resolve to an empty directory, which is what `bow` and
    # `sniper` silently were.
    claimed = roster_pools()
    unmapped = sorted(claimed - set(POOL_TAG.values()))
    if unmapped:
        raise SystemExit(
            'weapons claim mod pools no export tag maps to: %s\n'
            '  add the (type, compatName) pair to POOL_TAG, or the pool can only be empty'
            % ', '.join(unmapped))

    rows = []
    for m in mods:
        pool = POOL_TAG.get((m.get('type'), m.get('compatName')))
        if pool is None or pool not in claimed:
            continue
        uniq = m.get('uniqueName', '')
        rows.append({
            'name': m['name'],
            'pool': pool,
            'internal_name': uniq,
            'carried': have.get(uniq),
            'excluded': None if uniq in have else rule_out(uniq, m['name'], have_names),
        })
    rows.sort(key=lambda r: (r['pool'], r['name'], r['internal_name']))

    miss = [r for r in rows if not r['carried'] and not r['excluded']]
    excl = [r for r in rows if r['excluded']]
    # Per-pool counts, because "48 missing" says nothing and "the bow pool is
    # empty" says everything.
    pools = sorted(claimed)
    tally = {
        p: (
            sum(1 for r in rows if r['pool'] == p),
            sum(1 for r in rows if r['pool'] == p and r['carried']),
            sum(1 for r in rows if r['pool'] == p and not r['carried'] and not r['excluded']),
        )
        for p in pools
    }

    lines = [
        '# EVERY CLASS-TAGGED GUN MOD OUR ROSTER\'S POOLS CAN HOLD.',
        '#',
        '# GENERATED by scripts/survey_pool_mods.py from WFCD\'s export — read by',
        '# a TEST and by nothing else. Do not hand-edit; re-run the script.',
        '#',
        '# The sibling of weapon_exclusive_mods.yaml: that one joins compatName against',
        '# WEAPON NAMES, this one against the POOL TAGS the roster claims. A pool a',
        '# weapon declares and no directory holds resolves to nothing, which is how',
        '# `bow` and `sniper` sat empty from the day the roster was written.',
        '#',
        '# `carried:` is the file that holds it; `~` is a gap; `excluded` is a mod the',
        '# app refuses by RULE (flawed variants, riven placeholders, Conclave-only),',
        '# and the rule rides beside it.',
        '#',
        '# %d of %d carried, %d excluded by rule, %d still to transcribe.'
        % (len(rows) - len(miss) - len(excl), len(rows), len(excl), len(miss)),
        '#',
        '# Per pool (carried / in export, gap):',
    ]
    for p in pools:
        total, held, gap = tally[p]
        lines.append('#   %-14s %3d / %3d, gap %d' % (p, held, total, gap))
    lines.append('mods:')
    for r in rows:
        lines.append('  - name: %s' % r['name'])
        lines.append('    pool: %s' % r['pool'])
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
    for p in pools:
        total, held, gap = tally[p]
        print('   %-14s %3d / %3d carried, gap %d' % (p, held, total, gap))
    for r in miss:
        print('   MISSING  %-14s %-28s %s' % (r['pool'], r['name'], r['internal_name']))


if __name__ == '__main__':
    main()
