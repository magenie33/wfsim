#!/usr/bin/env python3
"""Generate `data/rivens/<class>.yaml` from the COMMITTED WFCD export.

A Riven's stat pool is DE's own data: `upgradeEntries` on the riven mod item
carries, for every stat it can roll, the internal tag, the BASE value, the two
name fragments, and the display template. That is the authoritative source and
it travels with the repo — nothing here is scraped or typed by hand.

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

ROOT = Path(__file__).resolve().parent.parent
EXPORT = ROOT / "vendor" / "warframe-items" / "data" / "json" / "All.json"
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
}

# DE's tag -> our effect kind (engine/src/mods_data.rs `effect`). `None` = a
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
}

# wiki Riven_Mods: these roll as a BONUS ONLY and never appear as the malus.
NEVER_MALUS = {
    "WeaponFireDamageMod",
    "WeaponFreezeDamageMod",
    "WeaponElectricityDamageMod",
    "WeaponToxinDamageMod",
    "WeaponPunctureDepthMod",
}


def slug(tag, text):
    """A stable English id, from the display text rather than DE's tag."""
    t = re.sub(r"<[^>]*>", "", text).replace("|val|", "").strip(" %")
    t = re.sub(r"\(.*?\)", "", t).strip()
    t = re.sub(r"[^A-Za-z ]", "", t).strip().lower().replace(" ", "_")
    return t or tag.lower()


def main():
    write = "--write" in sys.argv
    export = json.loads(EXPORT.read_text(encoding="utf-8"))
    by_unique = {it["uniqueName"]: it for it in export if it.get("uniqueName")}
    unknown = []

    for cls, unique in POOLS.items():
        item = by_unique.get(unique)
        if not item:
            print(f"! no export entry for {cls}: {unique}")
            continue
        rows = []
        for order, e in enumerate(item["upgradeEntries"]):
            tag = e["tag"]
            val = e["upgradeValues"][0]
            kind, arg = KIND.get(tag, (None, None))
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
                    "base": val["value"],
                    "prefix": e.get("prefixTag", ""),
                    "suffix": e.get("suffixTag", ""),
                    "text": re.sub(r"<[^>]*>", "", val["locTag"]).strip(),
                    "kind": kind or "unmodeled",
                    "arg": arg,
                    "malus": tag not in NEVER_MALUS,
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
                for i, n in [("damage", "Damage"), ("critical_chance", "Crit Chance"),
                             ("critical_damage", "Crit Damage"), ("multishot", "Multishot")]
                if i in shown
            )
            out = [
                f"# GENERATED by scripts/gen_rivens.py from the committed WFCD export",
                f"# ({unique}). Do not hand-edit: re-run the script.",
                "#",
                "# `base` is DE's own per-stat number. The value a riven SHOWS is",
                "#   base x 10 x (rank + 1) x disposition x config multiplier x roll",
                "# where roll is 0.9-1.1. At rank 8 that is base x 90, which lands on",
                f"# this class's canonical values exactly: {example}",
                "# at disposition 1.0.",
                "#",
                "# `malus: false` = wiki-listed bonus-only, never the negative stat.",
                "# `kind: unmodeled` = a real riven stat the engine does not model; it",
                "# still occupies a rolled slot and still shapes the name.",
                "#",
                "# `order` is DE's own index in upgradeEntries. Stats that share a base",
                "# are worth exactly the same at the same roll, so the name's magnitude",
                "# ordering ties between them. This is what breaks it.",
                f"class: {cls}",
                "stats:",
            ]
            for r in rows:
                out.append(f"  - id: {r['id']}")
                out.append(f"    tag: {r['tag']}")
                out.append(f"    order: {r['order']}")
                out.append(f"    base: {r['base']}")
                out.append(f"    prefix: {r['prefix']}")
                out.append(f"    suffix: {r['suffix']}")
                out.append(f"    text: \"{r['text']}\"")
                out.append(f"    kind: {r['kind']}")
                if r["arg"]:
                    out.append(f"    arg: {r['arg']}")
                if not r["malus"]:
                    out.append("    malus: false")
            io.open(OUT / f"{cls}.yaml", "w", encoding="utf-8", newline="\n").write(
                "\n".join(out) + "\n"
            )
    for cls, tag in unknown:
        print(f"  ! {cls}: no effect kind for {tag} -> unmodeled")
    if write:
        print(f"wrote {OUT.relative_to(ROOT)}")
    else:
        print("(re-run with --write)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
