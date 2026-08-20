"""Re-read every weapon entry back against the wiki's own weapon module.

THE ONLY CHECK THAT COMPARES OUR NUMBERS TO THE SOURCE. Every guard in the repo
checks a SHAPE — that a burst declares its count, that a falloff spans, that a
range page was opened — and a field transcribed into the wrong slot satisfies
all of them. This one asks the other question: does the number in the yaml equal
the number on the wiki?

HOW AN ENTRY IS MATCHED TO ITS ATTACK. A weapon's module row holds several
attacks and an entry carries ONE of them, without recording which. So the match
is by VALUE: the candidate attacks are those whose damage vector is the entry's,
and among them the one that also agrees on crit, status, fire rate and multishot
wins. That matters — the Kohm's Single Pellet and Fully Spooled rows carry the
SAME damage and differ in everything else, and an entry deliberately carries the
spooled one.

An entry with no matching attack at all is the finding this exists for.

THE SECOND PASS IS WFCD, which is what every weapon yaml's header claims
("cross-checked against WFCD warframe-items"). It is the CROSS-CHECK and not a
peer: where the two sources disagree, THE WIKI WINS (data/README.md), so a
disagreement here is recorded rather than fixed — but it has to be recorded,
because a header that claims a check nobody ran is worse than no header.

WFCD carries three quantities under names that look like ours and are not:
`magazineSize` is in SHOTS where ours is in ROUNDS (the Panthera spends 2 a
shot, so 60 rounds read as 30), `reloadTime` is sometimes the PARTIAL reload,
and `omegaAttenuation`/`masteryReq` are an older snapshot than the wiki's.
Every one of those is in `EXPECTED_WFCD` with its reason.

    python scripts/audit_weapon_stats.py            # the whole roster
    python scripts/audit_weapon_stats.py kohm lex   # named entries only

Needs `private/scripts/wiki_weapons.py` (the wiki module reader) and therefore
does not run in CI — it is a bench tool, catalogued in docs/DATA_SOURCES.md.
"""
import glob
import io
import json
import os
import re
import sys

import yaml

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, 'private', 'scripts'))
try:
    import wiki_weapons as W
except ImportError:
    raise SystemExit('private/scripts/wiki_weapons.py is missing — this is a bench tool')

# PRIMARY AND SECONDARY ONLY, and the reason is the whole point of this tool.
#
# An ARCH-GUN's page carries TWO stat columns — Archwing and Atmosphere — and
# the roster ships the ATMOSPHERE one, because the arena is on the ground. The
# module's row is the ARCHWING column, so comparing an Arch-Gun entry to it
# reports a disagreement on almost every field and every one of them is us being
# right. That is precisely the trap `data/README.md` names ("an export cannot say
# 'there are two of these and you want the other one'"), and a checker that
# cannot tell the columns apart must not pretend to check them. Companion weapons
# are unverified for the same reason and are left out until somebody reads their
# module's shape.
# COMPANION WEAPONS ARE IN as of 2026-08-21. The header used to say they were
# "unverified for the same reason and left out until somebody reads their
# module's shape" — so somebody read it: `Module:Weapons/data/companion` carries
# ONE stat column (Accuracy, AmmoMax, Magazine, Reload, Disposition, Mastery and
# one Attacks list), which is the primary/secondary shape and not the Arch-Gun's
# two. There is nothing here for a checker to pick the wrong column of, which is
# the whole of the exclusion above.
MOD = {}
for module in ('primary', 'secondary', 'companion'):
    table = W.load(module)
    MOD.update(table.get('Weapons', table))

# A CASE DIFFERENCE IS NOT A DIFFERENT WEAPON, and one silently excluded a whole
# family: this roster writes "MK1-Braton" and the module writes "Mk1-Braton", so
# every MK1 entry was reported UNCHECKED and nobody read the word (2026-08-21).
# It hid a real error — the MK1-Kunai's Incarnon multishot.
MOD_CI = {k.casefold(): k for k in MOD}


def module_row(name):
    """The module's row for a display name, tolerating case."""
    if name in MOD:
        return MOD[name]
    key = MOD_CI.get(name.casefold())
    return MOD[key] if key else None

# (our field, the module's) — weapon level, then attack level.
WEAPON_FIELDS = [('mastery_rank', 'Mastery'), ('disposition', 'Disposition'),
                 ('magazine', 'Magazine'), ('reload_seconds', 'Reload'),
                 ('ammo_max', 'AmmoMax'), ('ammo_pickup', 'AmmoPickup'),
                 ('accuracy', 'Accuracy')]
ATTACK_FIELDS = [('crit_chance', 'CritChance'), ('crit_multiplier', 'CritMultiplier'),
                 ('status_chance', 'StatusChance'), ('fire_rate', 'FireRate'),
                 ('multishot', 'Multishot'), ('punch_through_m', 'PunchThrough'),
                 ('projectile_speed_mps', 'ShotSpeed'), ('ammo_cost', 'AmmoCost'),
                 # THE CONE, which decides how much of a shot lands and which
                 # nothing else here checks.
                 ('spread.min_deg', 'MinSpread'), ('spread.max_deg', 'MaxSpread'),
                 ('charge_seconds', 'ChargeTime'),
                 # DAMAGE FALLOFF, which nothing checked and which the arena
                 # started reading the day it gained a distance. The module
                 # nests it exactly as the yaml does.
                 ('falloff.start_m', 'Falloff.StartRange'),
                 ('falloff.end_m', 'Falloff.EndRange'),
                 ('falloff.reduction', 'Falloff.Reduction')]
# A transcription rounds where the module carries more places than a card does
# (0.101429 -> 0.1014); anything past a tenth of a per cent is a disagreement.
TOL = 2e-3

# WHERE THE YAML IS RIGHT AND THE MODULE IS NOT WHAT WE MEAN. Each is a
# DECISION, written down with its reason, so a new disagreement stands out
# instead of being lost among the known ones. A refusal is not a shortcut.
EXPECTED = {
    # INFINITE punch through, which the module writes as 0 (the Arca Plasmor
    # family, the Phantasmas, the Felarx) or as a SURFACE figure (the Lanka's
    # "Innate Infinite Body Punch Through; fully charged shots have innate 5
    # meter punch through for surfaces"). 999 is how this roster spells it.
    ('arca_plasmor', 'attack.punch_through_m'): 'infinite body punch through',
    ('tenet_arca_plasmor', 'attack.punch_through_m'): 'infinite body punch through',
    ('coda_bassocyst', 'attack.punch_through_m'): 'infinite body punch through',
    ('felarx', 'attack.punch_through_m'): 'infinite body punch through',
    ('phantasma', 'attack.punch_through_m'): 'infinite body punch through',
    ('phantasma_prime', 'attack.punch_through_m'): 'infinite body punch through',
    ('lanka', 'attack.punch_through_m'): 'infinite BODY punch through; the 5 m is surfaces',
    ('lanka_uncharged', 'attack.punch_through_m'): 'infinite BODY punch through',
    # …and the two the punch-through page's own EXCEPTION LIST names as
    # "Lex (Incarnon Form)". The module carries 1.4, which is the SURFACE
    # figure; the prose is what says the body case is unlimited, and an export
    # column cannot.
    ('lex_incarnon', 'attack.punch_through_m'): 'infinite BODY punch through; the 1.4 is surfaces',
    ('lex_prime_incarnon', 'attack.punch_through_m'): 'infinite BODY punch through; the 1.4 is surfaces',
    # THE MAGAZINE IN SHOTS. 32 rounds at 4 a shot is 8 shots, and the entry
    # counts shots — the reload lands in the same place either way.
    ('ballistica_prime', 'magazine'): 'expressed in SHOTS: 32 rounds / 4 a shot',
    ('ballistica_prime', 'attack.ammo_cost'): 'expressed in SHOTS: 32 rounds / 4 a shot',
    # HALF AN AMMO A SHOT, which is the entry's own decision and is about the
    # MAGAZINE rather than the reserve: the Verglas Prime's 80-round magazine at
    # half a round a shot is what halves how often it reloads, and that is DPS.
    ('verglas_prime', 'attack.ammo_cost'): 'half a round a shot; see the entry',
    # THE DAMAGE TYPE ROTATION, AVERAGED. Both Deconstructors throw 130 of ONE
    # type at a time, cycling Impact then Puncture then Slash, and the entry
    # carries an equal three-way split — so no single module attack matches its
    # vector by construction. The cost is stated on the entry's own `unmodeled:`
    # line: total damage and status mix come out exact, ARMOUR does not.
    ('deconstructor', 'damage'): 'the three-attack type rotation, averaged',
    ('deconstructor_prime', 'damage'): 'the three-attack type rotation, averaged',
    # A TOME HAS NO MAGAZINE and the sim cannot fire a zero — see the entry.
    ('grimoire', 'magazine'): 'a Tome has none; 1 so the sim can fire it',
    # THE EMBEDDED DETONATION is the one modelled (the mine sticks), and it
    # shares its damage with the mid-flight one, which is what the matcher finds.
    ('sancti_castanas', 'attack.status_chance'): 'the EMBEDDED detonation, not the mid-flight',
    # The mag burst shares its damage with the primary fire, so the matcher
    # picks the wrong row; 2.0 is the Burst Shot's own rate.
    ('tenet_detron_mag_burst', 'attack.fire_rate'): 'the Burst Shot row, not Normal Attack',
    # THE MAGAZINE IS THE MULTIPLIER: this alt-fire's damage is its 100 Impact
    # times the magazine it eats, so the entry carries 1000 and no module row
    # says so.
    ('tenet_plinx_charged', 'damage'): '100 Impact x the unmodded magazine of 10',
}


def near(a, b):
    return abs(float(a) - float(b)) <= TOL * max(1.0, abs(float(b)))


# HOW A SHOT ARRIVES, which is behaviour rather than a number: a projectile has
# travel time and an arc, a hit-scan does not, and modelling one as the other is
# a real fault that no numeric field can show. Ours -> the module's spellings.
#
# `AoE` and `DoT` are SKIPPED rather than compared. Those are the module's
# sub-attacks — an explosion, a cloud — which this roster carries as a `radial:`
# or `lingering:` block on the attack that delivers them, so the entry's own
# `shot_type` is the delivery's and comparing it to `AoE` would report every
# grenade launcher. An UNKNOWN value is reported, never skipped: a vocabulary
# that quietly drops what it does not recognise under-reports, which is exactly
# how the first Condition Overload pass missed nine rows.
SHOT_TYPES = {
    'hit_scan': {'Hit-Scan', 'Hitscan'},
    'projectile': {'Projectile'},
    # The one `beam` entry: a continuous beam arrives instantly, and the module
    # has no separate spelling for it.
    'beam': {'Hit-Scan', 'Hitscan'},
}
SHOT_TYPES_SKIP = {'AoE', 'DoT'}


def dig(d, key):
    """`spread.min_deg` reads through the nested block; a plain key is itself."""
    cur = d
    for part in key.split('.'):
        if not isinstance(cur, dict) or part not in cur:
            return None
        cur = cur[part]
    return cur


def attack_score(ours, theirs):
    """How many attack fields agree — the tie-break when damage is shared."""
    n = 0
    for k, m in ATTACK_FIELDS:
        v = dig(ours, k)
        if v is not None and theirs.get(m) is not None and near(v, theirs[m]):
            n += 1
    return n


def main(only):
    specs = {}
    for f in sorted(glob.glob(os.path.join(ROOT, 'data/weapons/*/*.yaml'))):
        d = yaml.safe_load(io.open(f, encoding='utf-8'))
        specs[d['id']] = d

    checked, findings = 0, []
    for wid, d in sorted(specs.items()):
        if only and wid not in only:
            continue
        if d.get('inherits'):
            # A form states only what differs; its weapon carries the metadata,
            # and its ATTACK is checked on its own below through the parent's row.
            parent = specs.get(d['inherits'])
            if parent is None:
                findings.append('%s: inherits %r, which is not an entry' % (wid, d['inherits']))
                continue
            row = module_row(parent['name'])
            weapon_level = False
        else:
            row = module_row(d['name'])
            weapon_level = True
        if row is None:
            # A FORM THAT DID NOT DECLARE `inherits`. 88 roster entries are form
            # siblings and most say so; the ones written before that rule stand
            # alone with a name like "Torid (Incarnon Form)", which is not a
            # module key. The module carries the form as an ATTACK on the parent
            # ("Incarnon Form" in Torid's Attacks list), so stripping the
            # parenthetical finds the right row — and only the ATTACK fields may
            # be compared, because magazine, reload and disposition belong to
            # the weapon and the form only restates what differs.
            #
            # This rescued 59 of the 106 names that were reported UNCHECKED on
            # 2026-08-21, which is every Incarnon form in the roster. The 47 that
            # remain are companion and Arch-Gun weapons, which this tool excludes
            # on purpose (see the header).
            bare = re.sub(r'\s*\([^)]*\)$', '', d['name'])
            if bare != d['name'] and module_row(bare) is not None:
                row, weapon_level = module_row(bare), False
        if row is None:
            # A display name that is not a module key is a WIKI PAGE name we
            # chose (the module calls the Vinquibus "Vinquibus (Primary)"), not
            # a fault — but it is worth listing so nobody assumes it was checked.
            findings.append('%s: no module row for %r (not checked)' % (wid, d['name']))
            continue
        checked += 1

        if weapon_level:
            for ours, theirs in WEAPON_FIELDS:
                if ours not in d or row.get(theirs) is None:
                    continue
                if not near(d[ours], row[theirs]) and (wid, ours) not in EXPECTED:
                    findings.append('%s.%s: yaml %s vs module %s'
                                    % (wid, ours, d[ours], row[theirs]))

        atk = d.get('attack') or {}
        dm = {k.lower(): float(v) for k, v in (atk.get('damage') or {}).items()}
        cands = [a for a in row['Attacks']
                 if {k.lower(): float(v) for k, v in a['Damage'].items()} == dm]
        if not cands:
            if (wid, 'damage') not in EXPECTED:
                findings.append('%s: no module attack carries its damage %s' % (wid, dm))
            continue
        best = max(cands, key=lambda a: attack_score(atk, a))
        st, mst = atk.get('shot_type'), best.get('ShotType')
        if st is not None and mst is not None and mst not in SHOT_TYPES_SKIP:
            want = SHOT_TYPES.get(st)
            if want is None:
                findings.append('%s.attack.shot_type: %r is not in SHOT_TYPES' % (wid, st))
            elif mst not in want and (wid, 'attack.shot_type') not in EXPECTED:
                findings.append('%s.attack.shot_type: yaml %r vs module %r (attack %r)'
                                % (wid, st, mst, best.get('AttackName')))
        for ours, theirs in ATTACK_FIELDS:
            v = dig(atk, ours)
            # THE MODULE SIDE NESTS TOO — `Falloff.StartRange` — so the same
            # walker reads both. A flat key is a path of one.
            t = dig(best, theirs)
            if v is None or t is None:
                continue
            if not near(v, t) and (wid, 'attack.' + ours) not in EXPECTED:
                findings.append('%s.attack.%s: yaml %s vs module %s (attack %r)'
                                % (wid, ours, v, t, best.get('AttackName')))

    print('%d entries re-read against the module, %d finding(s)' % (checked, len(findings)))
    for f in findings:
        print('  ', f)
    return 1 if any('vs module' in f or 'no module attack' in f for f in findings) else 0


# THE SECOND SOURCE, and where it is allowed to differ. Each entry names the
# QUANTITY that differs, not just the weapon — a divergence with no reason is
# indistinguishable from a transcription error.
EXPECTED_WFCD = {
    # `magazineSize` IS IN SHOTS. Ours is in ROUNDS, which is the wiki module's
    # own quantity; divide by the attack's `ammo_cost` and the two agree.
    ('angstrum', 'magazine'): '3 rounds = 1 charged shot',
    ('prisma_angstrum', 'magazine'): '3 rounds = 1 charged shot',
    ('fulmin', 'magazine'): '60 rounds = 6 semi-auto shots at 10 apiece',
    ('fulmin_prime', 'magazine'): '80 rounds = 8 semi-auto shots at 10 apiece',
    ('panthera', 'magazine'): '60 rounds = 30 shots at 2 apiece',
    ('panthera_prime', 'magazine'): '80 rounds = 40 shots at 2 apiece',
    ('staticor', 'magazine'): '48 rounds = 12 charged throws at 4 apiece',
    ('twin_grakatas', 'magazine'): '120 rounds = 60 shots at 2 apiece',
    # `reloadTime` IS SOMETIMES THE PARTIAL ONE. The Basmu's page says it
    # outright — "a 2 second reload animation from empty… if there are still
    # rounds left, there is a delay of 0.x" — and this sim always empties the
    # magazine, so the FULL reload is the one that applies.
    ('basmu', 'reload_seconds'): "WFCD holds the PARTIAL reload; ours is the wiki's full one",
    ('shedu', 'reload_seconds'): "WFCD holds the PARTIAL reload; ours is the wiki's full one",
    ('nataruk', 'reload_seconds'): "WFCD holds the nock, not the wiki's reload",
    ('flux_rifle', 'reload_seconds'): 'the two sources disagree; the wiki wins',
    ('efv_8_mars', 'reload_seconds'): 'the two sources disagree; the wiki wins',
    ('riot_848', 'reload_seconds'): 'the two sources disagree; the wiki wins',
    ('tenet_detron', 'reload_seconds'): 'the two sources disagree; the wiki wins',
    ('grimoire', 'reload_seconds'): "a Tome does not reload; WFCD's 0.01 is a floor",
    # AN OLDER SNAPSHOT. Riven disposition moves with DE's balance passes and
    # mastery ranks are re-set; the wiki module is the current one.
    ('coda_bassocyst', 'disposition'): 'WFCD is an older snapshot',
    ('coda_bubonico', 'disposition'): 'WFCD is an older snapshot',
    ('tenet_diplos', 'disposition'): 'WFCD is an older snapshot',
    ('tenet_plinx', 'disposition'): 'WFCD is an older snapshot',
    ('thornbak', 'disposition'): 'WFCD is an older snapshot',
    ('vinquibus', 'disposition'): 'WFCD is an older snapshot',
    ('furis', 'mastery_rank'): 'WFCD is an older snapshot',
}

WFCD_FIELDS = [('mastery_rank', 'masteryReq'), ('disposition', 'omegaAttenuation'),
               ('magazine', 'magazineSize'), ('reload_seconds', 'reloadTime')]


def wfcd_index():
    """Every export item by `uniqueName` — the ONLY join (never the name)."""
    out = {}
    for f in glob.glob(os.path.join(ROOT, 'vendor/warframe-items/data/json/*.json')):
        try:
            arr = json.load(io.open(f, encoding='utf-8'))
        except Exception:
            continue
        if not isinstance(arr, list):
            continue
        for it in arr:
            if isinstance(it, dict) and it.get('uniqueName'):
                out.setdefault(it['uniqueName'], it)
    return out


def wfcd_pass(only):
    idx = wfcd_index()
    if not idx:
        print('(vendor/warframe-items is not present — the WFCD pass is skipped)')
        return 0
    checked, findings = 0, []
    for f in sorted(glob.glob(os.path.join(ROOT, 'data/weapons/*/*.yaml'))):
        d = yaml.safe_load(io.open(f, encoding='utf-8'))
        if only and d['id'] not in only:
            continue
        # ARCH-GUNS and COMPANION weapons carry the ARCHWING column in WFCD,
        # and the roster ships the ATMOSPHERE one — the same reason the module
        # pass skips them.
        path = f.replace('\\', '/')
        if d.get('inherits') or '/archgun/' in path or '/sentinel/' in path:
            continue
        it = idx.get(d.get('internal_name'))
        if it is None:
            continue
        checked += 1
        for ours, theirs in WFCD_FIELDS:
            if ours not in d or it.get(theirs) is None:
                continue
            if not near(d[ours], it[theirs]) and (d['id'], ours) not in EXPECTED_WFCD:
                findings.append('%s.%s: yaml %s vs WFCD %s' % (d['id'], ours, d[ours], it[theirs]))
    print('%d entries joined to WFCD, %d unexplained disagreement(s)'
          % (checked, len(findings)))
    for f in findings:
        print('  ', f)
    return 1 if findings else 0


if __name__ == '__main__':
    rc = main(set(sys.argv[1:]))
    rc |= wfcd_pass(set(sys.argv[1:]))
    raise SystemExit(rc)
