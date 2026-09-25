#!/usr/bin/env python3
"""Localized names and card text from DE's Public Export (`scripts/de_export.py`).

The dual-verification setup (data/README.md "i18n"): every zh display name
should be witnessed by BOTH sources —
  1. DE's Public Export in that language (`Export*_zh.json`) — this script
     automates that arm, joining by internal_name == uniqueName;
  2. the community wiki's 对照 table
     (https://warframe.huijiwiki.com/wiki/Project:中英名称对照) — human
     cross-check, tracked in CONTRIBUTING/PR review.

A locale is a DIRECTORY (`data/i18n/<locale>/`) whose files are merged:
`names.yaml` and `ui.yaml` are hand-written, `descriptions.yaml` is written
by this script and never by hand.

Usage:
  python scripts/de_i18n.py check [--locale zh]
      Two questions, and the second is the one that matters more.
      (1) Does what we have DISAGREE with DE's export? A non-base FORM is expected
          to — DE names the weapon, ours names the form — and is reported as
          a form, not as a mismatch to re-approve every run.
      (2) COVERAGE: what can the UI name that has NO chinese name at all,
          across EVERY family — including enemies and Incarnon evolutions,
          which the export cannot supply and which the old check was therefore
          blind to. It is where a gap is most likely and was least visible.
  python scripts/de_i18n.py fill --section mods --section arcanes [--locale zh]
      ADD the ids the export can name that have no line yet. Existing lines are
      never touched — not the names, not the comments explaining them — so
      a deliberate divergence survives (`cernos_prime_uncharged` is
      西诺斯 Prime (速射) here and plain 西诺斯 Prime in DE's export).
      Disagreements are `check`'s business, and a human's.
  python scripts/de_i18n.py descriptions [--locale zh]
      Rewrite data/i18n/<locale>/descriptions.yaml from DE's per-rank
      localized card text (`levelStats`) for every mod and arcane.

      This is DE's OWN sentence, not a translation we assemble: a card is
      not a bag of terms, and "+30% Fire Rate (x2 for Bows)" is
      "+30% 射速（弓类武器效果加倍）" in their client. Phrase substitution
      gets the terms and leaves the idiom in English.

The export is read from vendor/public-export/ — `python scripts/de_export.py fetch`.
"""

import argparse
import json
import re
import sys
from pathlib import Path

import de_export

ROOT = Path(__file__).resolve().parent.parent
SECTIONS = {
    "weapons": "data/weapons/**/*.yaml",
    "mods": "data/mods/**/*.yaml",
    "arcanes": "data/arcanes/**/*.yaml",
    "auras": "data/auras/*.yaml",
    "warframe_mods": "data/warframe_mods/*.yaml",
    "warframe_arcanes": "data/warframe_arcanes/*.yaml",
    "artifact_mods": "data/artifact_mods/*.yaml",
}


def load_export(locale: str) -> dict:
    """uniqueName -> {locale: entry}: DE's export in that language, in the shape
    every reader below takes."""
    return {u: {locale: e} for u, e in de_export.items(locale).items()}


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


def export_names(export: dict, locale: str, section: str) -> tuple[dict, list]:
    hits, missing = {}, []
    for idv, iname in sorted(our_ids(SECTIONS[section]).items()):
        name = export.get(iname, {}).get(locale, {}).get("name")
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
    # DE writes a line break as CRLF, and in older strings as a literal "\n".
    return MARKUP.sub("", s.replace("\\n", "\n").replace("\r\n", "\n")).strip()


def card_text(stats: list) -> str:
    """One card, from DE's stat list for one rank.

    The list is NOT one line per entry. When a rank UNLOCKS extra bonuses,
    DE's first entry is the whole card — unlock lines included — and the
    remaining entries repeat those same lines on their own. Joining verbatim
    printed them twice on the top-rank card of Primary/Secondary Deadhead,
    Primary/Secondary Dexterity, Primary/Secondary Merciless, and on EVERY
    rank of Secondary Fortifier. So a line a LATER entry repeats is dropped.
    Within one entry every line is kept: Galvanized Scope's own card says
    "On Weak Point Hit:" twice, and dropping the second leaves its stacks
    with no trigger at all.
    """
    seen, out = set(), []
    for s in stats:
        lines = [x.strip() for x in clean_line(s).split("\n") if x.strip()]
        out += [x for x in lines if x not in seen]
        seen.update(lines)
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


# DE'S OWN TYPOS, corrected after the join: (text, replacement, which occurrence).
# Galvanized Scope heads its KILL stacks "On Weak Point Hit" in every language;
# its pistol twin Galvanized Crosshairs, same mechanic, reads "Kill", and so
# does the wiki ("Headshot kills will grant"). The twin's own words replace it.
ERRATA = {
    ("galvanized_scope", "zh"): ("命中弱点时：", "弱点击杀时：", 2),
}


def _errata(idv: str, locale: str, text: str) -> str:
    fix = ERRATA.get((idv, locale))
    if not fix:
        return text
    old, new, nth = fix
    at = -1
    for _ in range(nth):
        at = text.find(old, at + 1)
        if at < 0:
            return text
    return text[:at] + new + text[at + len(old):]


def export_descriptions(export: dict, locale: str, section: str) -> tuple[dict, list]:
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
        loc = export.get(iname, {}).get(locale, {})
        ranks = loc.get("levelStats")
        if not ranks:
            missing.append(idv)
            continue
        prefix = card_text(loc.get("description") or [])
        cards = []
        for r in ranks:
            body = card_text(r.get("stats", []))
            card = f"{prefix}\n{body}" if prefix and not _already_said(prefix, body) else body
            cards.append(_errata(idv, locale, card))
        hits[idv] = cards
    return hits, missing


def write_descriptions(path: Path, locale: str, tables: dict[str, dict]) -> None:
    out = [
        f"# {locale} — mod and arcane card text, in DE's OWN words.",
        "#",
        "# GENERATED by scripts/de_i18n.py descriptions — DO NOT HAND-EDIT.",
        "# Source: DE's Public Export in this language, joined on",
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


# EVERY family the UI can show a Chinese name for, and where a name comes from.
#
# `SECTIONS` is only the export-joinable part of this: enemies carry no
# `internal_name` (DE's export has no entity to join them to) and Incarnon
# evolutions are not items at all, so neither can ever be filled from the
# export. They were therefore invisible to `check`, which only ever asked
# "what could the export name that we haven't filled" — a question that cannot
# report a gap in the two families where a gap is most likely.
#
# That is not hypothetical: five Boar Prime evolution names were simply absent,
# nothing said so, and they got TRANSLATED from the English instead — four of
# the five wrong (docs/DATA_SOURCES.md). A name that cannot be read
# must be left empty and asked for; being told it is empty is the first half.
#
#   family -> (ids glob, overlay file, table, the export can name it)
FAMILIES = {
    "weapons": ("data/weapons/**/*.yaml", "names.yaml", "weapons", True),
    "mods": ("data/mods/**/*.yaml", "names.yaml", "mods", True),
    "arcanes": ("data/arcanes/**/*.yaml", "names.yaml", "arcanes", True),
    "enemies": ("data/enemies/**/*.yaml", "names.yaml", "enemies", False),
    "evolutions": ("data/evolutions/**/*.yaml", "evolutions.yaml", "evolutions", False),
}
HAND_SOURCE = (
    "hand-transcribe from the CN wiki's API — never translate "
    "(docs/DATA_SOURCES.md §The CN wiki is reachable through its API)"
)


def weapon_forms() -> set:
    """Ids that are a non-base FORM of a transform group.

    DE's export names the WEAPON, so every form of it comes back with the base
    weapon's name — `boar_prime_incarnon` is 野猪 Prime, not 野猪 Prime (灵化
    形态). Ours says which form it is, because the UI shows the two side by
    side and one name for both is not a name. That difference is the RULE for
    a form, not an exception to be listed, so `check` reports it as a form
    rather than as a mismatch a human has to re-approve every run.
    """
    out = set()
    for f in ROOT.glob("data/weapons/**/*.yaml"):
        idv = group = None
        for line in open(f, encoding="utf-8"):
            if m := re.match(r"^id: (\S+)", line):
                idv = m.group(1)
            if m := re.match(r"^transform_group: (\S+)", line):
                group = m.group(1)
        if idv and group and idv != group:
            out.add(idv)
    return out


def data_ids(pattern: str) -> set:
    """Every `id:` under a glob, whether or not it has an internal_name."""
    out = set()
    for f in ROOT.glob(pattern):
        for line in open(f, encoding="utf-8"):
            if m := re.match(r"^id: (\S+)", line):
                out.add(m.group(1))
                break
    return out


def coverage(locale_dir: Path) -> int:
    """Report every id the UI can show that has no localized name."""
    gaps = 0
    for family, (glob, fname, table, from_export) in FAMILIES.items():
        path = locale_dir / fname
        named = overlay_section(path.read_text(encoding="utf-8"), table) if path.exists() else {}
        missing = sorted(data_ids(glob) - set(named))
        if not missing:
            print(f"  {family}: {len(named)} named, complete")
            continue
        gaps += len(missing)
        how = f"run `fill --section {family}`" if from_export else HAND_SOURCE
        print(f"  {family}: {len(missing)} UNNAMED — {how}")
        for idv in missing[:12]:
            print(f"      {idv}")
        if len(missing) > 12:
            print(f"      … and {len(missing) - 12} more")
    return gaps


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("mode", choices=["check", "fill", "descriptions"])
    ap.add_argument("--locale", default="zh")
    ap.add_argument("--section", action="append", choices=list(SECTIONS),
                    help="(fill) sections to rewrite")
    args = ap.parse_args()

    export = load_export(args.locale)
    locale_dir = ROOT / "data" / "i18n" / args.locale

    if args.mode == "descriptions":
        tables, report = {}, []
        for section, table in [("mods", "mod_descriptions"), ("arcanes", "arcane_descriptions")]:
            hits, missing = export_descriptions(export, args.locale, section)
            tables[table] = hits
            report.append(f"{table}: {len(hits)} entries, "
                          f"{sum(len(v) for v in hits.values())} ranks"
                          + (f" — NO localized text for {missing}" if missing else ""))
        write_descriptions(locale_dir / "descriptions.yaml", args.locale, tables)
        print("\n".join(report))
        print(f"wrote {locale_dir / 'descriptions.yaml'}")
        # THE WARFRAME BUILDER'S, in a file of its own: same source, same join.
        wf_tables = {}
        for section, table in [("warframe_mods", "warframe_mod_descriptions"),
                               ("warframe_arcanes", "warframe_arcane_descriptions")]:
            hits, missing = export_descriptions(export, args.locale, section)
            # A LADDER OF ANOTHER LENGTH IS LEFT OUT, never trimmed: the wiki's
            # `max_rank` is the one we state, and a card of DE's with a different
            # count would show one rank's numbers on another. It falls back to
            # English, and the report names it.
            ranks = {i: int(re.search(r"^max_rank: (\d+)", (ROOT / p).read_text(encoding="utf-8"), re.M).group(1))
                     for p in [str(x.relative_to(ROOT)) for x in ROOT.glob(SECTIONS[section])]
                     for i in [Path(p).stem]}
            off = sorted(i for i, r in hits.items() if len(r) != ranks.get(i, -1) + 1)
            wf_tables[table] = {i: r for i, r in hits.items() if i not in off}
            print(f"{table}: {len(wf_tables[table])} entries"
                  + (f" — NO localized text for {missing}" if missing else "")
                  + (f" — rank count differs from ours, left out: {off}" if off else ""))
        write_descriptions(locale_dir / "warframe_descriptions.yaml", args.locale, wf_tables)
        # An ability's text sits on its FRAME's entry, keyed by the ability's own
        # uniqueName, one string rather than a rank ladder.
        # The Helminth's own abilities are a category of their own, keyed the same.
        by_unique = {}
        for entry in export.values():
            e = (entry or {}).get(args.locale, {})
            for a in (e.get("abilities") or []) + ([e] if "abilityUniqueName" in e else []):
                if a.get("abilityUniqueName") and a.get("description"):
                    by_unique.setdefault(a["abilityUniqueName"], clean_line(a["description"]))
        abilities = {i: by_unique[u] for i, u in sorted(our_ids("data/warframe_abilities/*.yaml").items())
                     if u in by_unique}
        with open(locale_dir / "warframe_descriptions.yaml", "a", encoding="utf-8", newline="\n") as fh:
            fh.write("warframe_ability_descriptions:\n")
            fh.writelines(f"  {i}: {json.dumps(t, ensure_ascii=False)}\n" for i, t in abilities.items())
        print(f"warframe_ability_descriptions: {len(abilities)} entries")
        return 0

    # `check` / `fill` operate on the hand-written NAMES file.
    overlay_path = locale_dir / "names.yaml"
    text = overlay_path.read_text(encoding="utf-8")

    if args.mode == "check":
        bad = 0
        forms = weapon_forms()
        for section in SECTIONS:
            ours = overlay_section(text, section)
            theirs, missing = export_names(export, args.locale, section)
            for idv, name in sorted(ours.items()):
                if idv not in theirs or theirs[idv] == name:
                    continue
                if idv in forms:
                    print(f"form     {section}.{idv}: '{name}' (DE names the weapon: '{theirs[idv]}')")
                    continue
                print(f"MISMATCH {section}.{idv}: overlay='{name}' export='{theirs[idv]}'")
                bad += 1
            unfilled = sorted(set(theirs) - set(ours))
            if unfilled:
                print(f"unfilled {section}: {len(unfilled)} ids the export could name: {unfilled[:8]}{' ...' if len(unfilled) > 8 else ''}")
            if missing:
                print(f"no export name for {section}: {missing}")
        print("\ncoverage — every id the UI can name, in every family:")
        gaps = coverage(locale_dir)
        print("\ncheck done" + (f" — {bad} mismatches" if bad else " — no mismatches")
              + (f", {gaps} unnamed" if gaps else ", nothing unnamed"))
        return 1 if bad else 0

    # FILL IS ADDITIVE. Rewriting the whole section from the export
    # destroyed two things a generated list cannot carry: the COMMENTS (where
    # the Acolytes' names came from, "the tapped form, same weapon") and the
    # DELIBERATE DIVERGENCES they explain — `cernos_prime_uncharged` is
    # 西诺斯 Prime (速射) here and plain 西诺斯 Prime in DE's export, because a
    # FORM has no name of its own and ours says which form it is.
    #
    # So an existing line is never touched. `check` is where a disagreement
    # with the export gets reported and a human decides; `fill` only ever adds ids
    # that have no line at all.
    for section in args.section or []:
        theirs, missing = export_names(export, args.locale, section)
        pat = re.compile(rf"^{section}:( \{{\}})?\n?((?:^[ \t]+.*\n?|^\n)*)", re.M)
        m = pat.search(text)
        kept = overlay_section(text, section) if m else {}
        fresh = {i: n for i, n in theirs.items() if i not in kept}
        if m:
            body = m.group(2).rstrip("\n")
            lines = ([body] if body else []) + [f"  {i}: {n}" for i, n in sorted(fresh.items())]
            text = text[: m.start()] + f"{section}:\n" + "\n".join(lines) + "\n\n" + text[m.end():]
        else:
            text += f"\n{section}:\n" + "".join(f"  {i}: {n}\n" for i, n in sorted(fresh.items()))
        differs = sorted(i for i, n in kept.items() if i in theirs and theirs[i] != n)
        print(f"filled {section}: +{len(fresh)} new, {len(kept)} kept"
              + (f" ({len(differs)} of them differ from the export: {differs[:4]} — see `check`)" if differs else "")
              + (f" (no export name: {missing})" if missing else ""))
    overlay_path.write_text(text, encoding="utf-8", newline="")
    return 0


if __name__ == "__main__":
    sys.exit(main())
