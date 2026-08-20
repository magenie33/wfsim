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

    python scripts/audit_weapon_stats.py            # the whole roster
    python scripts/audit_weapon_stats.py kohm lex   # named entries only

Needs `private/scripts/wiki_weapons.py` (the wiki module reader) and therefore
does not run in CI — it is a bench tool, catalogued in docs/DATA_SOURCES.md.
"""
import glob
import io
import os
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
MOD = {}
for module in ('primary', 'secondary'):
    table = W.load(module)
    MOD.update(table.get('Weapons', table))

# (our field, the module's) — weapon level, then attack level.
WEAPON_FIELDS = [('mastery_rank', 'Mastery'), ('disposition', 'Disposition'),
                 ('magazine', 'Magazine'), ('reload_seconds', 'Reload'),
                 ('ammo_max', 'AmmoMax'), ('ammo_pickup', 'AmmoPickup'),
                 ('accuracy', 'Accuracy')]
ATTACK_FIELDS = [('crit_chance', 'CritChance'), ('crit_multiplier', 'CritMultiplier'),
                 ('status_chance', 'StatusChance'), ('fire_rate', 'FireRate'),
                 ('multishot', 'Multishot'), ('punch_through_m', 'PunchThrough'),
                 ('projectile_speed_mps', 'ShotSpeed'), ('ammo_cost', 'AmmoCost')]
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
    # THE MAGAZINE IN SHOTS. 32 rounds at 4 a shot is 8 shots, and the entry
    # counts shots — the reload lands in the same place either way.
    ('ballistica_prime', 'magazine'): 'expressed in SHOTS: 32 rounds / 4 a shot',
    ('ballistica_prime', 'attack.ammo_cost'): 'expressed in SHOTS: 32 rounds / 4 a shot',
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


def attack_score(ours, theirs):
    """How many attack fields agree — the tie-break when damage is shared."""
    n = 0
    for k, m in ATTACK_FIELDS:
        if k in ours and theirs.get(m) is not None and near(ours[k], theirs[m]):
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
            row = MOD.get(parent['name'])
            weapon_level = False
        else:
            row = MOD.get(d['name'])
            weapon_level = True
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
        for ours, theirs in ATTACK_FIELDS:
            if ours not in atk or best.get(theirs) is None:
                continue
            if not near(atk[ours], best[theirs]) and (wid, 'attack.' + ours) not in EXPECTED:
                findings.append('%s.attack.%s: yaml %s vs module %s (attack %r)'
                                % (wid, ours, atk[ours], best[theirs], best.get('AttackName')))

    print('%d entries re-read against the module, %d finding(s)' % (checked, len(findings)))
    for f in findings:
        print('  ', f)
    return 1 if any('vs module' in f or 'no module attack' in f for f in findings) else 0


if __name__ == '__main__':
    raise SystemExit(main(set(sys.argv[1:])))
