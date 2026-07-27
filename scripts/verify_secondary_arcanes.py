#!/usr/bin/env python3
"""Cross-check data/arcanes/secondary/*.yaml against the WIKI arcane module.

SOURCE OF TRUTH = `Module:Arcane/data` (scripts/wiki_arcanes.py), same split as
the pistol-mod pipeline: the wiki is authoritative for mechanical fields
(Name / Rarity / MaxRank / InternalName / max-rank Description); warframestat
levelStats supplied the per-rank numbers when the files were authored.

Checks:
  1. COVERAGE — every wiki `Type == "Secondary"` arcane has a file, and every
     file matches a wiki Secondary arcane (catches removals/renames/new drops).
  2. MECHANICAL FIELDS — rarity / max_rank / internal_name equal the wiki's.
  3. DESCRIPTION — the file's X-templated text token-matches the wiki max-rank
     text (markup stripped): non-X tokens must be identical; each X token must
     correspond to a numeric wiki token (numbers substituted by X).

Usage:
  python scripts/verify_secondary_arcanes.py                 # report
  python scripts/verify_secondary_arcanes.py --cache arc.lua # reuse a dump
"""
import argparse
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import wiki_arcanes  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ARC_DIR = os.path.join(ROOT, "data", "arcanes", "secondary")


def read_fields(path: str) -> dict:
    """Top-level scalar fields of one arcane YAML (regex — no yaml dep)."""
    fields = {}
    for line in open(path, encoding="utf-8"):
        m = re.match(r'([a-z_]+):\s*(.+?)\s*$', line)
        if m and m.group(1) not in fields:
            fields[m.group(1)] = m.group(2).strip()
    return fields


def norm_wiki_desc(desc: str) -> str:
    """Wiki Lua description -> plain text (markup stripped, real newlines)."""
    desc = desc.replace("\\r\\n", "\n").replace("\\n", "\n")
    desc = re.sub(r"<[^>]*>", "", desc)
    return desc.strip()


def x_match(file_desc: str, wiki_desc: str) -> str | None:
    """None if the X-templated file text matches the wiki text, else why not."""
    ft = file_desc.split()
    wt = wiki_desc.split()
    if len(ft) != len(wt):
        return f"token count {len(ft)} vs wiki {len(wt)}"
    for a, b in zip(ft, wt):
        if a == b:
            continue
        if "X" in a and re.sub(r"\d+(?:\.\d+)?", "X", b) == a:
            continue  # rank-varying number correctly templated
        return f"token {a!r} vs wiki {b!r}"
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cache", help="path to a saved Module:Arcane/data dump")
    args = ap.parse_args()

    wiki = {n: w for n, w in wiki_arcanes.load(args.cache).items()
            if (w.get("Type") or "") == "Secondary"}

    files = {}
    for fn in sorted(os.listdir(ARC_DIR)):
        if fn.endswith(".yaml"):
            files[fn] = read_fields(os.path.join(ARC_DIR, fn))

    problems = []
    have_names = {f.get("name"): fn for fn, f in files.items()}
    for n in wiki:
        if n not in have_names:
            problems.append(f"MISSING file for wiki Secondary arcane: {n}")
    for name, fn in have_names.items():
        if name not in wiki:
            problems.append(f"{fn}: name {name!r} is not a wiki Secondary arcane")

    for fn, f in files.items():
        w = wiki.get(f.get("name"))
        if not w:
            continue
        if f.get("id") != wiki_arcanes.slug(w["Name"]):
            problems.append(f"{fn}: id {f.get('id')} != slug {wiki_arcanes.slug(w['Name'])}")
        if f.get("rarity") != (w.get("Rarity") or "").lower():
            problems.append(f"{fn}: rarity {f.get('rarity')} != wiki {w.get('Rarity')}")
        if f.get("max_rank") != str(w.get("MaxRank")):
            problems.append(f"{fn}: max_rank {f.get('max_rank')} != wiki {w.get('MaxRank')}")
        if f.get("internal_name") != w.get("InternalName"):
            problems.append(f"{fn}: internal_name != wiki {w.get('InternalName')}")
        fd = (f.get("description") or "").strip('"').replace("\\n", "\n")
        why = x_match(fd, norm_wiki_desc(w.get("Description") or ""))
        if why:
            problems.append(f"{fn}: description mismatch — {why}")

    print(f"wiki Secondary arcanes: {len(wiki)}; files: {len(files)}")
    if problems:
        print(f"\n!! {len(problems)} problems:")
        for p in problems:
            print("   - " + p)
        sys.exit(1)
    print("all files match the wiki (coverage, fields, X-templated description)")


if __name__ == "__main__":
    main()
