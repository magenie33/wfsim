#!/usr/bin/env python3
"""Cross-check data/mods/pistol/*.yaml against the AUTHORITATIVE wiki module.

The Warframe wiki's `Module:Mods/data` (Weird Gloop MediaWiki) is the ground
truth — a single canonical entry per mod, unlike warframestat which carries
stale duplicates. It also exposes `Conclave` (PvP flag) and `IsExilus`, which
warframestat lacks. This script pulls it via the MediaWiki API and reports every
mechanical mismatch (drain / polarity / rarity / max_rank) plus PvP/exilus
flags, so the auto-imported pool can be corrected against the source.

Requires a DESCRIPTIVE User-Agent (Weird Gloop policy) — hardcoded below.

Usage:
  python scripts/verify_pistol_mods.py                 # fetch + verify
  python scripts/verify_pistol_mods.py --wiki mods.lua # reuse a saved module
"""
import argparse
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import wiki_mods  # noqa: E402  (shared fetch + parse)

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PISTOL_DIR = os.path.join(ROOT, "data", "mods", "pistol")


def read_yaml(path: str) -> dict:
    out = {}
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            m = re.match(r"(\w+):\s*(.+?)\s*(?:#.*)?$", line)
            if m and m.group(1) in ("name", "polarity", "rarity", "base_drain", "max_rank", "exilus", "mod_type"):
                v = m.group(2).strip().strip('"')
                out[m.group(1)] = v
    return out


def reconcile_file(path: str, w: dict) -> list:
    """Rewrite the MECHANICAL field lines of one yaml from the wiki entry
    (drain/max_rank/polarity/rarity/internal_name/exilus), preserving effects
    and comments. Returns a list of change strings. Wiki is authoritative."""
    with open(path, encoding="utf-8") as fh:
        lines = fh.readlines()
    pol = (w.get("Polarity") or "").lower()
    rar = (w.get("Rarity") or "").lower()
    drain = w.get("BaseDrain")
    maxr = w.get("MaxRank")
    iname = w.get("InternalName")
    is_ex = bool(w.get("IsExilus"))
    changes = []
    out, seen = [], set()
    max_rank_idx = None
    for line in lines:
        m = re.match(r"(\s*)(\w+):(.*)$", line)
        if not m:
            out.append(line)
            continue
        indent, key, _rest = m.group(1), m.group(2), m.group(3)
        seen.add(key)
        if key == "polarity":
            if _rest.strip() != pol:
                changes.append(f"polarity {_rest.strip()} -> {pol}")
            out.append(f"{indent}polarity: {pol}\n")
        elif key == "rarity":
            if _rest.strip() != rar:
                changes.append(f"rarity {_rest.strip()} -> {rar}")
            out.append(f"{indent}rarity: {rar}\n")
        elif key == "base_drain":
            cur = _rest.split("#")[0].strip()
            if cur != str(drain):
                changes.append(f"base_drain {cur} -> {drain}")
            out.append(f"{indent}base_drain: {drain}                 # max drain {drain + maxr} at rank {maxr}\n")
        elif key == "max_rank":
            cur = _rest.split("#")[0].strip()
            if cur != str(maxr):
                changes.append(f"max_rank {cur} -> {maxr}")
            out.append(f"{indent}max_rank: {maxr}\n")
            max_rank_idx = len(out)
        elif key == "internal_name":
            cur = _rest.strip()
            if cur != (iname or "null"):
                changes.append(f"internal_name {cur} -> {iname}")
            out.append(f"{indent}internal_name: {iname}\n")
        elif key == "exilus":
            # Keep/normalize when the wiki says exilus; drop otherwise.
            if is_ex:
                out.append(f"{indent}exilus: true\n")
            else:
                changes.append("removed exilus (wiki IsExilus false)")
        else:
            out.append(line)
    # Ensure internal_name exists (insert after max_rank) when the file lacked it.
    if "internal_name" not in seen and iname and max_rank_idx is not None:
        out.insert(max_rank_idx, f"internal_name: {iname}\n")
        changes.append(f"added internal_name {iname}")
        max_rank_idx = None  # index shifted; exilus insert recomputed below
    # Ensure exilus: true exists when the wiki says so.
    if is_ex and "exilus" not in seen:
        idx = next((i for i, l in enumerate(out) if l.startswith("max_rank:")), None)
        if idx is not None:
            out.insert(idx + 1, "exilus: true\n")
            changes.append("added exilus: true (wiki IsExilus)")
    if changes:
        with open(path, "w", encoding="utf-8", newline="\n") as fh:
            fh.writelines(out)
    return changes


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--wiki", help="path to a saved Module:Mods/data dump")
    ap.add_argument("--fix", action="store_true", help="rewrite mechanical fields from the wiki")
    args = ap.parse_args()

    wiki = wiki_mods.load(args.wiki)
    print(f"parsed {len(wiki)} mods from the wiki module\n")

    files = sorted(f for f in os.listdir(PISTOL_DIR) if f.endswith(".yaml"))
    checked = matched = 0
    issues = []
    for fn in files:
        y = read_yaml(os.path.join(PISTOL_DIR, fn))
        name = y.get("name")
        if not name:
            continue
        checked += 1
        w = wiki.get(name)
        if not w:
            issues.append(f"{fn}: NOT FOUND in wiki module (name '{name}')")
            continue
        matched += 1
        if args.fix:
            ch = reconcile_file(os.path.join(PISTOL_DIR, fn), w)
            if ch:
                print(f"FIXED {fn}: " + "; ".join(ch))
            continue
        probs = []
        # NOTE: `Conclave = true` is NOT a PvP-exclusive signal — many mods
        # marked Conclave are fully PvE-usable (historical artifact, per user
        # 2026-07-26). So it is intentionally NOT treated as an error here.
        if (y.get("polarity") or "").lower() != (w.get("Polarity") or "").lower():
            probs.append(f"polarity {y.get('polarity')} != wiki {w.get('Polarity')}")
        if (y.get("rarity") or "").lower() != (w.get("Rarity") or "").lower():
            probs.append(f"rarity {y.get('rarity')} != wiki {w.get('Rarity')}")
        if str(y.get("base_drain")) != str(w.get("BaseDrain")):
            probs.append(f"base_drain {y.get('base_drain')} != wiki {w.get('BaseDrain')}")
        if str(y.get("max_rank")) != str(w.get("MaxRank")):
            probs.append(f"max_rank {y.get('max_rank')} != wiki {w.get('MaxRank')}")
        our_ex = (y.get("exilus") == "true")
        wiki_ex = bool(w.get("IsExilus"))
        if our_ex != wiki_ex:
            probs.append(f"exilus {our_ex} != wiki IsExilus {wiki_ex}")
        if probs:
            issues.append(f"{fn} ({name}):\n    - " + "\n    - ".join(probs))

    print(f"checked {checked} yaml files, matched {matched} to the wiki\n")
    if issues:
        print(f"=== {len(issues)} mods with discrepancies ===")
        for it in issues:
            print(it)
    else:
        print("ALL CLEAN — every mod matches the wiki module.")


if __name__ == "__main__":
    main()
