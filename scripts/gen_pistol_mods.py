#!/usr/bin/env python3
"""Generate data/mods/pistol/*.yaml SKELETONS from the WIKI mod module.

SOURCE OF TRUTH = the Warframe wiki's `Module:Mods/data` (see scripts/wiki_mods.py),
NOT warframestat — warframestat carries stale duplicates (wrong drain / rank /
internal_name), so the wiki is authoritative for every mechanical field (user
directive 2026-07-26). Filters to `Type == "Pistol"` (the wiki's own weapon-type
tag, which correctly excludes shotgun/rifle mods warframestat mis-tags).

Per mod: mechanical fields come straight from the wiki; EFFECTS are parsed from
the max-rank `Description` into the unified schema (kinds the engine models get a
real kind; the rest are `kind: unmodeled` with the raw text, so nothing is lost —
the final 2D replica implements them all). Existing curated files (matched BY
NAME) are never overwritten. Run scripts/verify_pistol_mods.py --fix to reconcile
EXISTING files' mechanical fields to the wiki.

Usage:
  python scripts/gen_pistol_mods.py                 # DRY RUN — report only
  python scripts/gen_pistol_mods.py --write         # write new skeletons
  python scripts/gen_pistol_mods.py --cache mods.lua # reuse a saved module
"""
import argparse
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import wiki_mods  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PISTOL_DIR = os.path.join(ROOT, "data", "mods", "pistol")

# Loader-accepted vocab (engine::mods_data) — anything outside is flagged, not
# emitted, so a bad value can never panic the build.
POLARITIES = {"madurai", "naramon", "vazarin", "zenurik", "unairu", "penjaga", "umbra"}
RARITIES = {"common", "uncommon", "rare", "legendary"}

# PvP-ONLY exclusions. The wiki has NO clean flag: `Conclave=true` and a
# `/PvPMods/` InternalName appear on BOTH genuinely-PvP mods (Blind Shot) and
# PvE-usable ones (Eject Magazine, Air Recon) — a historical artifact (user
# 2026-07-26). So the PvP-only set is MAINTAINED BY HAND here; everything else
# Type=Pistol (and non-Flawed) is treated as PvE. Flawed variants are excluded
# programmatically via the wiki `IsFlawed` flag.
PVP_ONLY = {
    "blind shot", "calculated victory", "full capacity", "heavy warhead",
    "hydraulic barrel", "impaler munitions", "loose magazine", "meteor munitions",
    "night stalker", "no return", "razor munitions", "recuperate",
    "secondary wind", "spry sights", "strafing slide", "tainted clip", "air recon",
}

# Mods the wiki still lists but that no longer exist in the live game (removed /
# legacy) — never import (user 2026-07-26).
REMOVED_FROM_GAME = {"cannonade"}

# ---- Description -> unified structured effects ------------------------------
ELEMENTS = {
    "heat": "heat", "cold": "cold", "electricity": "electricity", "toxin": "toxin",
    "blast": "blast", "radiation": "radiation", "gas": "gas", "magnetic": "magnetic",
    "viral": "viral", "corrosive": "corrosive",
    "impact": "impact", "puncture": "puncture", "slash": "slash",
}
FACTIONS = ["grineer", "corpus", "infested", "corrupted", "orokin", "murmur"]

# (matcher substring, kind, is_percent) — all these kinds are engine-modeled.
DESCRIPTORS = [
    ("critical chance", "crit_chance_bonus", True),
    ("critical damage", "crit_damage_bonus", True),
    ("status chance", "status_chance_bonus", True),
    ("status duration", "status_duration_bonus", True),
    ("multishot", "multishot_bonus", True),
    ("fire rate", "fire_rate_bonus", True),
    ("reload speed", "reload_speed_bonus", True),
    ("magazine capacity", "magazine_capacity_bonus", True),
    ("ammo maximum", "ammo_max_bonus", True),
    ("punch through", "punch_through_bonus", False),  # meters
    ("recoil", "recoil_reduction", True),
    ("zoom", "zoom_bonus", True),
    ("accuracy", "accuracy_bonus", True),
    ("projectile", "projectile_speed_bonus", True),
    ("holster", "holstered_reload", True),
]


def parse_stat(text: str):
    """One '+50% Multishot' style line -> effect dict, or a raw record."""
    text = re.sub(r"<[^>]*>", "", text).strip()  # strip DT_*_COLOR markup
    low = text.lower()
    mult = re.search(r"x\s*(\d+(?:\.\d+)?)", low)  # faction banes: "x1.3 Damage"
    if mult:
        num = float(mult.group(1))
        val = num - 1.0
    else:
        m = re.search(r"([+\-]?\d+(?:\.\d+)?)", text)
        num = float(m.group(1)) if m else None
        val = (num / 100.0) if num is not None else None
    for fac in FACTIONS:
        if fac in low and "damage" in low:
            return {"kind": "faction_damage_bonus", "faction": fac, "max": val}
    for word, el in ELEMENTS.items():
        if word in low:
            return {"kind": "elemental_damage_bonus", "element": el, "max": val}
    for sub, kind, pct in DESCRIPTORS:
        if sub in low:
            return {"kind": kind, "max": (val if pct else num)}
    if "damage" in low and val is not None:
        return {"kind": "base_damage_bonus", "max": val}
    # Unrecognized: keep the wiki text in a `desc` FIELD (not a comment) so
    # nothing is lost and the data stays clean.
    return {"kind": "unmodeled", "max": val, "desc": text}


def parse_effects(desc: str) -> list:
    """Structured effects from the wiki max-rank Description (one per line).

    The Lua module stores multi-effect descriptions with LITERAL escape
    sequences (`"+165% Damage\\r\\n-55% Accuracy"`), not real newlines — split on
    both so each stat becomes its own effect."""
    if not desc:
        return []
    lines = [ln.strip() for ln in re.split(r"\\r\\n|\\n|\r?\n", desc) if ln.strip()]
    return [parse_stat(ln) for ln in lines]


def yaml_effect(eff) -> list:
    fields = [f"kind: {eff['kind']}"]
    if eff.get("element"):
        fields.append(f"element: {eff['element']}")
    if eff.get("faction"):
        fields.append(f"faction: {eff['faction']}")
    if eff.get("max") is not None:
        fields.append(f"rankMax: {round(eff['max'], 4)}")
    if eff.get("desc"):
        fields.append(f'desc: "{eff["desc"]}"')
    lines = [f"  - {fields[0]}"]
    for fpart in fields[1:]:
        lines.append(f"    {fpart}")
    return lines


def existing_names() -> set:
    names = set()
    for fn in os.listdir(PISTOL_DIR):
        if not fn.endswith(".yaml"):
            continue
        with open(os.path.join(PISTOL_DIR, fn), encoding="utf-8") as fh:
            for line in fh:
                m = re.match(r"\s*name:\s*(.+?)\s*$", line)
                if m:
                    names.add(m.group(1).strip().strip('"').lower())
                    break
    return names


def skeleton(name: str, w: dict) -> str:
    pol = (w.get("Polarity") or "").lower()
    rar = (w.get("Rarity") or "").lower()
    base = int(w.get("BaseDrain") or 0)
    maxr = int(w.get("MaxRank") or 0)
    iname = w.get("InternalName") or "null"
    exilus = bool(w.get("IsExilus"))
    wiki = f"https://wiki.warframe.com/w/{name.replace(' ', '_')}"

    lines = [
        f"id: {wiki_mods.slug(name)}",
        f"name: {name}",
        f"rarity: {rar}",
        f"polarity: {pol}",
        f"base_drain: {base}                 # max drain {base + maxr} at rank {maxr}",
        f"max_rank: {maxr}",
    ]
    if exilus:
        lines.append("exilus: true")
    lines.append(f"internal_name: {iname}")
    lines.append("")
    effects = parse_effects(w.get("Description"))
    if effects:
        lines.append("effects:")
        for eff in effects:
            lines += yaml_effect(eff)
    else:
        lines.append("effects: []")
    lines += ["", "source:", f"  url: {wiki}", ""]
    return "\n".join(lines)


def is_auto_generated(path: str) -> bool:
    """True if this file was produced by an earlier gen run (safe to overwrite).

    Detects the historical dev note; hand-authored mods (with modeled
    stacking_buff / condition_overload effects and deliberate corrections) never
    carry it, so `--force` never clobbers them."""
    if not os.path.exists(path):
        return False
    with open(path, encoding="utf-8") as fh:
        return "gen_pistol_mods.py" in fh.read()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true", help="write skeletons (default: dry run)")
    ap.add_argument("--force", action="store_true",
                    help="also regenerate EXISTING auto-generated files (never hand-authored ones)")
    ap.add_argument("--cache", help="path to a saved Module:Mods/data dump")
    ap.add_argument("--out", default=PISTOL_DIR, help="output dir")
    args = ap.parse_args()

    mods = wiki_mods.load(args.cache)
    # Type=Pistol, excluding Flawed starter variants (wiki IsFlawed) and the
    # hand-maintained PvP-only set (no clean wiki flag exists).
    pistol = {
        n: w for n, w in mods.items()
        if (w.get("Type") or "") == "Pistol"
        and not w.get("IsFlawed")
        and n.lower() not in PVP_ONLY
    }

    have = existing_names()
    # Existing pool files whose wiki Type is NOT Pistol → wrong-type, prune.
    wrong_type = [n for n, w in mods.items()
                  if n.lower() in have and (w.get("Type") or "") != "Pistol"]
    # Candidates: brand-new mods, plus (with --force) existing AUTO-generated
    # files to refresh from the wiki. Hand-authored files are never touched.
    writing = {}
    regen = 0
    for n, w in pistol.items():
        if (w.get("Polarity") or "").lower() not in POLARITIES \
           or (w.get("Rarity") or "").lower() not in RARITIES:
            continue
        path = os.path.join(args.out, wiki_mods.slug(n) + ".yaml")
        if n.lower() not in have:
            writing[n] = (w, path, "new")
        elif args.force and is_auto_generated(path):
            writing[n] = (w, path, "regen")
            regen += 1

    print(f"=== wiki Module:Mods/data (Type == Pistol) ===")
    print(f"total pistol mods in wiki : {len(pistol)}")
    print(f"NEW skeletons             : {sum(1 for v in writing.values() if v[2] == 'new')}")
    print(f"regenerate (auto, --force): {regen}")
    if wrong_type:
        print(f"\n!! {len(wrong_type)} EXISTING pool files are NOT Type=Pistol in the wiki "
              f"(prune candidates): {sorted(wrong_type)}")

    print(f"\n{'writing' if args.write else 'would write'} {len(writing)} files:")
    for n in sorted(writing):
        w, path, why = writing[n]
        rel = os.path.relpath(path, ROOT) if path.startswith(ROOT) else path
        print(f"   {'WRITE' if args.write else 'dry '} [{why}] {rel}")
        if args.write:
            with open(path, "w", encoding="utf-8", newline="\n") as fh:
                fh.write(skeleton(n, w))
    if not args.write:
        print("\n(dry run — pass --write to apply)")


if __name__ == "__main__":
    main()
