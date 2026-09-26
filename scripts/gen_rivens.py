#!/usr/bin/env python3
"""Generate `data/rivens/<class>.yaml` from DE's Public Export (`scripts/de_export.py`).

A Riven's stat pool is DE's own data: `upgradeEntries` on the riven mod item
carries, for every stat it can roll, the internal tag, the BASE value, the two
name fragments, and the display template. That is the authoritative source and
nothing here is scraped or typed by hand.

The one thing the export does not say is which of OUR effect kinds a tag maps
to, so `KIND` below is that table, kept in this file where it can be read
beside the tags it maps.

    python scripts/gen_rivens.py           # report
    python scripts/gen_rivens.py --write   # write data/rivens/
"""

import io
import json
import re
import sys
from pathlib import Path

import de_export

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "data" / "rivens"

# our mod class -> the riven item that carries its stat pool
POOLS = {
    "rifle": "/Lotus/Upgrades/Mods/Randomized/LotusRifleRandomModRare",
    "pistol": "/Lotus/Upgrades/Mods/Randomized/LotusPistolRandomModRare",
    # Arch-Guns roll their own riven, on the ground and in Archwing alike --
    # one item, because the deployment is a scenario and not a second weapon.
    # Its pool is 22 stats, two short of the rifle's: no Projectile Speed and
    # no Damage to Infested.
    #
    # The wiki agrees on the first ("-" in the Archgun column of the Projectile
    # Speed row) and disagrees on the second, printing x0.45 in the Infested
    # row for every class. DE is right and the wiki's row is a fill: checked
    # 2026-08-02 against the LIVE PublicExport
    # (content.warframe.com/PublicExport/Manifest/ExportUpgrades_en.json),
    # which is 22 entries with Corpus and Grineer and no Infested -- byte-equal
    # to the vendored snapshot this script reads. The classes are internally
    # consistent about it too: melee/zaw carry their own
    # WeaponMeleeFactionDamage* trio, every other gun class carries all three
    # WeaponFactionDamage* tags, and the archgun alone carries two.
    "archgun": "/Lotus/Upgrades/Mods/Randomized/LotusArchgunRandomModRare",
    # Shotguns roll their own riven — the pool is NOT the rifle's. Its own
    # stats are what make it a different item: Multishot is worth far more on
    # a weapon whose damage is already per-pellet, and the class carries a
    # Punch Through entry the rifle pool words differently.
    "shotgun": "/Lotus/Upgrades/Mods/Randomized/LotusShotgunRandomModRare",
    # Melee rolls a pool that shares only twelve stats with a gun's: no
    # Multishot, no Magazine, no Reload, no Ammo, no Zoom, no Recoil, no
    # Punch Through, no Projectile Speed -- and eleven of its own, from Combo
    # Duration to Heavy Attack Efficiency.
    #
    # ZAW IS NOT HERE and is not an oversight: DE's Zaw riven
    # (LotusModularMeleeRandomModRare) carries the same 24 entries with the
    # same tags, bases and name fragments, so it would be this file twice. It
    # earns its own line the day a zaw enters the roster and `mod_pools` names
    # a `zaw` class for `class_for_weapon` to find.
    "melee": "/Lotus/Upgrades/Mods/Randomized/PlayerMeleeWeaponRandomModRare",
}

# DE's tag -> our effect kind (engine/src/data/mods/parse.rs `effect`). `None` = a
# stat the engine does not model; it still loads and still counts as a rolled
# stat, it simply contributes nothing, exactly like an unmodeled mod.
KIND = {
    "WeaponDamageAmountMod": ("base_damage_bonus", None),
    "WeaponFireIterationsMod": ("multishot_bonus", None),
    "WeaponCritChanceMod": ("crit_chance_bonus", None),
    "WeaponCritDamageMod": ("crit_damage_bonus", None),
    "WeaponStunChanceMod": ("status_chance_bonus", None),
    "WeaponProcTimeMod": ("status_duration_bonus", None),
    "WeaponFireRateMod": ("fire_rate_bonus", None),
    "WeaponReloadSpeedMod": ("reload_speed_bonus", None),
    "WeaponClipMaxMod": ("magazine_capacity_bonus", None),
    "WeaponFireDamageMod": ("elemental_damage_bonus", "heat"),
    "WeaponFreezeDamageMod": ("elemental_damage_bonus", "cold"),
    "WeaponElectricityDamageMod": ("elemental_damage_bonus", "electricity"),
    "WeaponToxinDamageMod": ("elemental_damage_bonus", "toxin"),
    "WeaponImpactDamageMod": ("physical_damage_bonus", "impact"),
    "WeaponArmorPiercingDamageMod": ("physical_damage_bonus", "puncture"),
    "WeaponSlashDamageMod": ("physical_damage_bonus", "slash"),
    "WeaponFactionDamageCorpus": ("faction_damage_bonus", "corpus"),
    "WeaponFactionDamageGrineer": ("faction_damage_bonus", "grineer"),
    "WeaponFactionDamageInfested": ("faction_damage_bonus", "infested"),
    "WeaponPunctureDepthMod": ("punch_through_bonus", None),
    "WeaponAmmoMaxMod": ("ammo_max_bonus", None),
    "WeaponRecoilReductionMod": ("recoil_reduction", None),
    "WeaponProjectileSpeedMod": ("projectile_speed_bonus", None),
    "WeaponZoomFovMod": ("zoom_bonus", None),
    # MELEE'S OWN. `WeaponMeleeDamageMod` is Pressure Point's tag and the same
    # base-damage bucket; the faction trio is melee-specific because DE gave
    # melee its own three tags for the stat every other class spells
    # `WeaponFactionDamage*`.
    "WeaponMeleeDamageMod": ("base_damage_bonus", None),
    "WeaponMeleeFactionDamageCorpus": ("faction_damage_bonus", "corpus"),
    "WeaponMeleeFactionDamageGrineer": ("faction_damage_bonus", "grineer"),
    "WeaponMeleeFactionDamageInfested": ("faction_damage_bonus", "infested"),
    "ComboDurationMod": ("melee_combo_duration_bonus", None),
    "SlideAttackCritChanceMod": ("crit_chance_on_slide", None),
    "WeaponMeleeRangeIncMod": ("melee_range_bonus_m", None),
    "WeaponMeleeComboEfficiencyMod": ("heavy_attack_efficiency", None),
    "WeaponMeleeComboInitialBonusMod": ("initial_combo", None),
    "WeaponMeleeComboBonusOnHitMod": ("combo_count_chance", None),
    # The malus pole of the combo axis, a different mechanic (data/rivens/melee.yaml).
    "WeaponMeleeComboPointsOnHitMod": ("combo_gain_chance", None),
    # A FINISHER IS AN ANIMATION THIS ARENA HAS NO CONCEPT OF -- the same
    # answer Finishing Touch's card already gets, and the riven rolls it
    # anyway, so it occupies a slot and names the card and pays nothing.
    "WeaponMeleeFinisherDamageMod": (None, None),
    # SPLICED, below: each lands in the bucket its MOD already lands in.
    "WeaponWeakpointDamage": ("weakpoint_damage_bonus", None),
    "WeaponWeakpointCriticalChance": ("weakpoint_crit_chance_bonus", None),
    "WeaponStatusDamage": ("status_damage_bonus", None),
    "WeaponAmmoEfficiency": ("ammo_efficiency_bonus", None),
    "WeaponGasDamageMod": ("combined_element_bonus", "gas"),
    "WeaponCorrosiveDamageMod": ("combined_element_bonus", "corrosive"),
    "WeaponViralDamageMod": ("combined_element_bonus", "viral"),
    "WeaponRadiationDamageMod": ("combined_element_bonus", "radiation"),
    "WeaponBlastDamageMod": ("combined_element_bonus", "blast"),
    "WeaponMagneticDamageMod": ("combined_element_bonus", "magnetic"),
    "WeaponFactionDamageOrokin": ("faction_damage_bonus", "orokin"),
    "WeaponFactionDamageTechrot": ("faction_damage_bonus", "techrot"),
    "WeaponFactionDamageScaldra": ("faction_damage_bonus", "scaldra"),
    "WeaponMeleeHeavyAttackDamageMod": ("heavy_attack_damage_bonus", None),
    "WeaponMeleeHeavyAttackChargeMod": ("heavy_windup_speed_bonus", None),
    "WeaponMeleeSlamDamageMod": ("slam_damage_bonus", None),
    # Nothing in this arena is ever holstered or parries.
    "WeaponMagazineReloadHolstered": (None, None),
    "WeaponMeleeParryAngleMod": (None, None),
}

# SPLICED STATS: made by a Riven Splicer out of two rolled ones, never rolled.
# The export ships them in `upgradeEntries` with nothing to tell them apart;
# the wiki's Riven_Mods §"Spliced Values" is what names these eighteen. Each
# is bonus-only, and a card carries at most one (`RivenSpec::illegal`).
SPLICED = {
    "WeaponWeakpointDamage",
    "WeaponWeakpointCriticalChance",
    "WeaponStatusDamage",
    "WeaponAmmoEfficiency",
    "WeaponMagazineReloadHolstered",
    "WeaponGasDamageMod",
    "WeaponCorrosiveDamageMod",
    "WeaponViralDamageMod",
    "WeaponRadiationDamageMod",
    "WeaponBlastDamageMod",
    "WeaponMagneticDamageMod",
    "WeaponFactionDamageOrokin",
    "WeaponFactionDamageTechrot",
    "WeaponFactionDamageScaldra",
    "WeaponMeleeHeavyAttackDamageMod",
    "WeaponMeleeHeavyAttackChargeMod",
    "WeaponMeleeParryAngleMod",
    "WeaponMeleeSlamDamageMod",
}

# A TAG IS NOT ALWAYS ONE EFFECT. `WeaponCritChanceMod` is the tag a rifle
# riven carries and the tag a melee riven carries, and only the melee card
# reads "(x2 for Heavy Attacks)" -- which is a different bucket in the engine
# (True Steel's, `crit_chance_bonus_heavy_doubled`). Read before KIND.
CLASS_KIND = {
    ("melee", "WeaponCritChanceMod"): ("crit_chance_bonus_heavy_doubled", None),
}

# wiki Riven_Mods: these roll as a BONUS ONLY and never appear as the malus.
NEVER_MALUS = {
    "WeaponFireDamageMod",
    "WeaponFreezeDamageMod",
    "WeaponElectricityDamageMod",
    "WeaponToxinDamageMod",
    "WeaponPunctureDepthMod",
    # ...and its malus is a SEPARATE ENTRY, below. The wiki prints the pair as
    # one row -- "Additional Combo Count Chance (bonus) / Chance to Gain Combo
    # Count (malus)" -- and DE ships two tags with two bases for it.
    "WeaponMeleeComboBonusOnHitMod",
}

# ...AND THE ONE THAT ROLLS AS A MALUS ONLY, the other half of that pair.
#
# Its base is NEGATIVE (-0.01165) because it already IS the malus, which is
# what makes it different from Weapon Recoil: recoil's negative base is the
# BONUS ("-90% Weapon Recoil" is the good one) and flips positive in the malus
# slot, while this one stays negative there. `engine::build::rivens` carries that
# rule; this set is what tells it which stats it applies to.
NEVER_BONUS = {
    "WeaponMeleeComboPointsOnHitMod",
}

# A STAT THE GAME ROLLS THAT THE EXPORT DOES NOT CARRY YET. The melee riven
# rolls Status Damage in game and its `upgradeEntries` has no row for it, so
# the gun pools' row stands in: same tag, same name fragments, base 0.01, which
# is 90% at rank 8 and disposition 1.0. Used only while the export lacks the
# tag — once DE ships the row, the export's own wins and this line is dead.
EXPORT_LAGS = {
    "melee": [
        {
            "tag": "WeaponStatusDamage",
            "prefixTag": "plaga",
            "suffixTag": "mna",
            "upgradeValues": [{"value": 0.0099999998, "locTag": "|val|% Status Damage"}],
        },
    ],
}


def hole(text):
    """DE's template with ONE spelling of the hole the value goes in.

    Every entry writes `|val|` except Critical Chance for Slide Attack, which
    writes `|STAT1|`. `RivenStat::print` fills one hole; two spellings would
    make that line the only one in any pool to print its template raw."""
    return re.sub(r"<[^>]*>", "", text).replace("|STAT1|", "|val|")


# The one display text that slugs badly: "Magazine Reloaded/s when Holstered".
# The wiki's own name for the stat is the id.
SLUG = {"WeaponMagazineReloadHolstered": "reload_while_holstered"}


def slug(tag, text):
    """A stable English id, from the display text rather than DE's tag."""
    if tag in SLUG:
        return SLUG[tag]
    # The UNIT sits between the hole and the name -- `%` on most, `s` on Combo
    # Duration -- and is not part of what the stat is called.
    t = hole(text).replace("|val|", "").lstrip(" %s").strip()
    t = re.sub(r"\(.*?\)", "", t).strip()
    t = re.sub(r"[^A-Za-z ]", "", t).strip().lower().replace(" ", "_")
    return t or tag.lower()


def main():
    write = "--write" in sys.argv
    by_unique = de_export.items("en")
    unknown = []

    for cls, unique in POOLS.items():
        item = by_unique.get(unique)
        if not item:
            print(f"! no export entry for {cls}: {unique}")
            continue
        rows = []
        entries = list(item["upgradeEntries"])
        shipped = {e["tag"] for e in entries}
        for e in EXPORT_LAGS.get(cls, []):
            if e["tag"] in shipped:
                print(f"! {cls}: the export now carries {e['tag']} -- delete it from EXPORT_LAGS")
            else:
                entries.append(e)
        for order, e in enumerate(entries):
            tag = e["tag"]
            val = e["upgradeValues"][0]
            # `reverseValueSymbol` prints the number with its sign flipped:
            # Ammo Efficiency's base is -0.001 and its card reads "+9%". The
            # stored base is what the card means, so the flip happens here.
            base = -val["value"] if val.get("reverseValueSymbol") and tag in SPLICED else val["value"]
            kind, arg = CLASS_KIND.get((cls, tag)) or KIND.get(tag, (None, None))
            if kind is None:
                unknown.append((cls, tag))
            rows.append(
                {
                    "id": slug(tag, val["locTag"]),
                    "tag": tag,
                    # DE's own position in `upgradeEntries`. Rows are written
                    # sorted by id for reading; this keeps the export's order,
                    # which is the only non-arbitrary way to break a tie
                    # between two stats that share a base value.
                    "order": order,
                    "base": base,
                    "prefix": e.get("prefixTag", ""),
                    "suffix": e.get("suffixTag", ""),
                    "text": hole(val["locTag"]).strip(),
                    "kind": kind or "unmodelled",
                    "arg": arg,
                    "malus": tag not in NEVER_MALUS and tag not in SPLICED,
                    "spliced": tag in SPLICED,
                    "bonus": tag not in NEVER_BONUS,
                }
            )
        rows.sort(key=lambda r: r["id"])
        print(f"{cls}: {len(rows)} stats from {unique}")
        if write:
            OUT.mkdir(parents=True, exist_ok=True)
            # The header's worked example is THIS pool's own rank-8 numbers,
            # not a class's quoted from elsewhere: the bases differ per class
            # (rifle Damage 165%, archgun 99.9%), so a fixed line would be
            # wrong in every file but one.
            shown = {r["id"]: f"{r['base'] * 90 * 100:.4g}%" for r in rows}
            example = ", ".join(
                f"{n} {shown[i]}"
                for i, n in [("damage", "Damage"), ("melee_damage", "Melee Damage"),
                             ("critical_chance", "Crit Chance"),
                             ("critical_damage", "Crit Damage"), ("multishot", "Multishot")]
                if i in shown
            )
            out = [
                f"# GENERATED by scripts/gen_rivens.py from DE's Public Export",
                f"# ({unique}). Do not hand-edit: re-run the script.",
                "#",
                "# `base` is DE's own per-stat number. The value a riven SHOWS is",
                "#   base x 10 x (rank + 1) x disposition x config multiplier x roll",
                "# where roll is 0.9-1.1. At rank 8 that is base x 90, which lands on",
                f"# this class's canonical values exactly: {example}",
                "# at disposition 1.0.",
                "#",
                # FIVE FILES SAY THESE, so they are ids rather than five copies
                # (AGENTS.md, `data/notes.yaml`).
                "# see notes: malus_false_wiki_listed",
                "#",
                "# see notes: bonus_false_malus_only",
                "#",
                "# see notes: order_de_s_own",
                "#",
                "# see notes: spliced_riven_stat",
                f"class: {cls}",
                "stats:",
            ]
            for r in rows:
                out.append(f"  - id: {r['id']}")
                out.append(f"    tag: {r['tag']}")
                out.append(f"    order: {r['order']}")
                out.append(f"    base: {r['base']}")
                # QUOTED, because the malus-only stat has NEITHER -- it never
                # contributes a name fragment, and a bare `prefix:` is null.
                out.append(f"    prefix: \"{r['prefix']}\"")
                out.append(f"    suffix: \"{r['suffix']}\"")
                out.append(f"    text: \"{r['text']}\"")
                out.append(f"    kind: {r['kind']}")
                if r["arg"]:
                    out.append(f"    arg: {r['arg']}")
                if not r["malus"]:
                    out.append("    malus: false")
                if not r["bonus"]:
                    out.append("    bonus: false")
                if r["spliced"]:
                    out.append("    spliced: true")
            io.open(OUT / f"{cls}.yaml", "w", encoding="utf-8", newline="\n").write(
                "\n".join(out) + "\n"
            )
    for cls, tag in unknown:
        print(f"  ! {cls}: no effect kind for {tag} -> unmodelled")
    if write:
        print(f"wrote {OUT.relative_to(ROOT)}")
    else:
        print("(re-run with --write)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
