#!/usr/bin/env python3
"""Localized names from WFCD warframe-items (DE's official client strings).

The dual-verification setup (data/README.md "i18n"): every zh display name
should be witnessed by BOTH sources —
  1. WFCD warframe-items i18n.json (datamined DE client strings) — this
     script automates that arm, joining by internal_name == uniqueName;
  2. the community wiki's 对照 table
     (https://warframe.huijiwiki.com/wiki/Project:中英名称对照) — human
     cross-check, tracked in CONTRIBUTING/PR review.

A locale is a DIRECTORY (`data/i18n/<locale>/`) whose files are merged:
`names.yaml` and `ui.yaml` are hand-written, `descriptions.yaml` is written
by this script and never by hand.

Usage:
  python scripts/wfcd_i18n.py check [--i18n PATH] [--locale zh]
      Diff data/i18n/<locale>/names.yaml against WFCD: report entries that
      disagree, and ids WFCD could name that we haven't filled.
  python scripts/wfcd_i18n.py fill --section mods --section arcanes
      [--i18n PATH] [--locale zh]
      Rewrite the given empty-or-existing sections of names.yaml from
      WFCD names (sorted by id). Existing sections are REPLACED — run
      check first, and let the engine's i18n integrity tests gate the
      result.
  python scripts/wfcd_i18n.py descriptions [--i18n PATH] [--locale zh]
      Rewrite data/i18n/<locale>/descriptions.yaml from DE's per-rank
      localized card text (`levelStats`) for every mod and arcane.

      This is DE's OWN sentence, not a translation we assemble: a card is
      not a bag of terms, and "+30% Fire Rate (x2 for Bows)" is
      "+30% 射速（弓类武器效果加倍）" in their client. Phrase substitution
      gets the terms and leaves the idiom in English.

Without --i18n the file is downloaded (~52 MB) to a temp cache.
"""

import argparse
import json
import re
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
I18N_URL = "https://raw.githubusercontent.com/WFCD/warframe-items/master/data/json/i18n.json"
SECTIONS = {
    "weapons": "data/weapons/**/*.yaml",
    "mods": "data/mods/**/*.yaml",
    "arcanes": "data/arcanes/**/*.yaml",
}


def load_wfcd(path: str | None) -> dict:
    if path:
        return json.load(open(path, encoding="utf-8"))
    vendored = ROOT / "vendor" / "warframe-items" / "data" / "json" / "i18n.json"
    if vendored.exists():
        return json.load(open(vendored, encoding="utf-8"))
    cache = Path(__file__).parent / ".cache" / "wfcd_i18n.json"
    if not cache.exists():
        cache.parent.mkdir(exist_ok=True)
        print(f"downloading {I18N_URL} ... (tip: scripts/vendor.py clones the whole dataset)", file=sys.stderr)
        urllib.request.urlretrieve(I18N_URL, cache)
    return json.load(open(cache, encoding="utf-8"))


def our_ids(pattern: str) -> dict[str, str]:
    """id -> internal_name for every yaml matching the pattern."""
    out = {}
    for f in ROOT.glob(pattern):
        idv = iname = None
        for line in open(f, encoding="utf-8"):
            if m := re.match(r"^id: (\S+)", line):
                idv = m.group(1)
            if m := re.match(r"^internal_name: (\S+)", line):
                iname = m.group(1)
        if idv and iname:
            out[idv] = iname
    return out


def wfcd_names(wfcd: dict, locale: str, section: str) -> tuple[dict, list]:
    hits, missing = {}, []
    for idv, iname in sorted(our_ids(SECTIONS[section]).items()):
        name = wfcd.get(iname, {}).get(locale, {}).get("name")
        if name:
            hits[idv] = name
        else:
            missing.append(idv)
    return hits, missing


# DE's client markup, present in BOTH the English and the localized strings:
# damage-type colour spans (`<DT_POISON_COLOR>`) and a `<LOWER_IS_BETTER>`
# flag. Stripped — the cards are plain text today. (If the UI ever colours
# damage types, this is the hook to keep instead of dropping.)
MARKUP = re.compile(r"<[^>]+>")


def clean_line(s: str) -> str:
    # DE writes line breaks as a literal backslash-n inside the string.
    return MARKUP.sub("", s.replace("\\n", "\n")).strip()


def card_text(stats: list) -> str:
    """One card, from DE's stat list for one rank.

    The list is NOT one line per entry. When a rank UNLOCKS extra bonuses,
    DE's first entry is the whole card — unlock lines included — and the
    remaining entries repeat those same lines on their own. Joining verbatim
    printed them twice on the top-rank card of Primary/Secondary Deadhead,
    Primary/Secondary Dexterity, Primary/Secondary Merciless, and on EVERY
    rank of Secondary Fortifier. A card never states the same line twice, so
    the first occurrence wins.
    """
    seen, out = set(), []
    for s in stats:
        for line in clean_line(s).split("\n"):
            line = line.strip()
            if line and line not in seen:
                seen.add(line)
                out.append(line)
    return "\n".join(out)


def _already_said(prefix: str, rank: str) -> bool:
    """True if the rank line already carries what `prefix` says.

    DE writes an augment's whole sentence in BOTH fields, `|val|` standing in
    for the rank's number, so prepending would print it twice. Compared with
    digits, the placeholder and whitespace removed, which is what makes those
    two spellings of one sentence compare equal.
    """
    norm = lambda s: re.sub(r"[\s\d.%|]|val", "", s)
    return norm(prefix) and norm(prefix) in norm(rank)


def wfcd_descriptions(wfcd: dict, locale: str, section: str) -> tuple[dict, list]:
    """id -> one card text per rank (rank 0 first), from DE's own card text.

    TWO fields, and a card is both of them. `levelStats` carries the per-rank
    numbers; `description` carries the RULE the card opens with — "仅适用于半
    自动扳机。射速无法修改。", "当瞄准时，", "击中后：". Reading only the
    first dropped that opening line from 35 mods and arcanes, so the Cannonades
    printed their damage and punch through and said nothing about the trigger
    they need or the fire rate they lock (found 2026-08-03).

    Which field a rule lands in is DE's choice and not predictable: Primary
    Acuity's "多重射击无法变动。" is inside `levelStats`, the Cannonades' is in
    `description`. So both are read and joined, and neither is assumed.
    """
    hits, missing = {}, []
    for idv, iname in sorted(our_ids(SECTIONS[section]).items()):
        loc = wfcd.get(iname, {}).get(locale, {})
        ranks = loc.get("levelStats")
        if not ranks:
            missing.append(idv)
            continue
        prefix = card_text(loc.get("description") or [])
        cards = []
        for r in ranks:
            body = card_text(r.get("stats", []))
            cards.append(f"{prefix}\n{body}" if prefix and not _already_said(prefix, body) else body)
        hits[idv] = cards
    return hits, missing


def write_descriptions(path: Path, locale: str, tables: dict[str, dict]) -> None:
    out = [
        f"# {locale} — mod and arcane card text, in DE's OWN words.",
        "#",
        "# GENERATED by scripts/wfcd_i18n.py descriptions — DO NOT HAND-EDIT.",
        "# Source: WFCD warframe-items i18n.json, joined on",
        "# internal_name == uniqueName, one entry per rank (rank 0 first) with",
        "# DE's client markup (<DT_*_COLOR>, <LOWER_IS_BETTER>) stripped.",
        "#",
        "# TWO fields per card: `description` is the RULE it opens with (\"仅适用",
        "# 于半自动扳机。射速无法修改。\"), `levelStats` the rank's numbers. Which",
        "# one a rule lands in is DE's choice, not a pattern — so both are read.",
        "#",
        "# Whole sentences, already carrying that rank's numbers. This is what",
        "# supersedes phrase substitution for everything DE wrote: no table of",
        '# term replacements gets from "(x2 for Bows)" to "（弓类武器效果加倍）".',
        "",
    ]
    for table, rows in tables.items():
        out.append(f"{table}:")
        for idv, ranks in rows.items():
            out.append(f"  {idv}:")
            out += [f"    - {json.dumps(r, ensure_ascii=False)}" for r in ranks]
        out.append("")
    path.write_text("\n".join(out), encoding="utf-8", newline="\n")


def overlay_section(text: str, section: str) -> dict:
    m = re.search(rf"^{section}:(.*?)(?=^\S|\Z)", text, re.M | re.S)
    if not m:
        return {}
    out = {}
    for line in m.group(1).splitlines():
        if mm := re.match(r"^\s+(\S+): (.+?)(?:\s+#.*)?$", line):
            out[mm.group(1)] = mm.group(2).strip()
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["check", "fill", "descriptions"])
    ap.add_argument("--i18n", help="local path to warframe-items i18n.json")
    ap.add_argument("--locale", default="zh")
    ap.add_argument("--section", action="append", choices=list(SECTIONS),
                    help="(fill) sections to rewrite")
    args = ap.parse_args()

    wfcd = load_wfcd(args.i18n)
    locale_dir = ROOT / "data" / "i18n" / args.locale

    if args.mode == "descriptions":
        tables, report = {}, []
        for section, table in [("mods", "mod_descriptions"), ("arcanes", "arcane_descriptions")]:
            hits, missing = wfcd_descriptions(wfcd, args.locale, section)
            tables[table] = hits
            report.append(f"{table}: {len(hits)} entries, "
                          f"{sum(len(v) for v in hits.values())} ranks"
                          + (f" — NO localized text for {missing}" if missing else ""))
        write_descriptions(locale_dir / "descriptions.yaml", args.locale, tables)
        print("\n".join(report))
        print(f"wrote {locale_dir / 'descriptions.yaml'}")
        return 0

    # `check` / `fill` operate on the hand-written NAMES file.
    overlay_path = locale_dir / "names.yaml"
    text = overlay_path.read_text(encoding="utf-8")

    if args.mode == "check":
        bad = 0
        for section in SECTIONS:
            ours = overlay_section(text, section)
            theirs, missing = wfcd_names(wfcd, args.locale, section)
            for idv, name in sorted(ours.items()):
                if idv in theirs and theirs[idv] != name:
                    print(f"MISMATCH {section}.{idv}: overlay='{name}' wfcd='{theirs[idv]}'")
                    bad += 1
            unfilled = sorted(set(theirs) - set(ours))
            if unfilled:
                print(f"unfilled {section}: {len(unfilled)} ids WFCD could name: {unfilled[:8]}{' ...' if len(unfilled) > 8 else ''}")
            if missing:
                print(f"no wfcd name for {section}: {missing}")
        print("check done" + (f" — {bad} mismatches" if bad else " — no mismatches"))
        return 1 if bad else 0

    for section in args.section or []:
        theirs, missing = wfcd_names(wfcd, args.locale, section)
        body = f"{section}:\n" + "".join(f"  {i}: {n}\n" for i, n in theirs.items())
        pat = re.compile(rf"^{section}:( \{{\}})?\n?(?:^[ \t]+.*\n?|^\n)*", re.M)
        if pat.search(text):
            text = pat.sub(body + "\n", text, count=1)
        else:
            text += "\n" + body
        print(f"filled {section}: {len(theirs)} names" + (f" (no wfcd name: {missing})" if missing else ""))
    overlay_path.write_text(text, encoding="utf-8", newline="")
    return 0


if __name__ == "__main__":
    sys.exit(main())
