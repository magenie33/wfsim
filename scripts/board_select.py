#!/usr/bin/env python3
"""WHICH PUBLISHED ROWS CARRY A THING, and what rescoring them would cost.

WHAT RETIRES A FACT IS A PERSON DELETING ITS ROW (docs/BOARD.md), so repairing a
mechanic on purpose means finding the rows it reached. No hash answers that: a
hash says whether an input MOVED, which is a question about files, where this is
a question about BUILDS — and finding them by hand across 22,977 is not a thing
anyone does twice.

It prints the DELETE, so the two halves compose:

    python scripts/board_select.py --element heat --delete
    # …run against the database, then press the board's button

AND IT PRICES THE ANSWER FIRST. Every row records what it cost to measure, so
the summary says how many rows, how many groups and how many CPU minutes the
rescore is. A change that reaches eight thousand rows is a different decision
from one that reaches forty, and the difference belongs on screen rather than in
the bill afterwards.

BATCH THE FIXES, THEN RESCORE ONCE. Ten corrections landing separately are ten
rescores of overlapping rows; landing them together is one. That is the whole
economy of doing this by hand, and it is why this prints SQL rather than running
anything.

    --weapon/--mod/--arcane/--evolution   glob, repeatable, any-of within a flag
    --element <name>                      any entity whose data grants it
    --mode <id> --riven --plain --board <id>
    --delete                              print the DELETE that retires them
    --rows                                print one row per line instead
"""
from __future__ import annotations

import argparse
import fnmatch
import json
import re
import sys
from collections import defaultdict
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
# THE SLOT'S NAME, and `builds::RIVEN_SLOT` is where it is decided. A row
# carries it in `mods` like any other card, which is what makes "with a riven"
# a membership test rather than a shape to parse.
RIVEN_SLOT = "riven"


def load(path: Path) -> dict:
    return yaml.safe_load(path.read_text(encoding="utf-8")) or {}


def entities_granting(element: str) -> dict[str, set[str]]:
    """Every mod, arcane, evolution and weapon whose DATA names an element.

    READ OFF THE FILES RATHER THAN A LIST, because a list is a thing to forget:
    a card added tomorrow is found by the same walk that found the rest. The
    match is `element: <name>` — how a bonus names what it grants — or a damage
    key `<name>:`, which is how a weapon or a form carries an innate one.
    """
    want = element.lower()
    hits: dict[str, set[str]] = defaultdict(set)
    for family in ("mods", "arcanes", "evolutions", "weapons"):
        for f in sorted((ROOT / "data" / family).rglob("*.yaml")):
            text = f.read_text(encoding="utf-8")
            # Comments cite sources and name elements they do not grant.
            body = "\n".join(l for l in text.split("\n") if not l.lstrip().startswith("#"))
            # TWO SPELLINGS, because the data has two. A bonus NAMES what it
            # grants (`element: heat`); a weapon or a form carries an innate one
            # as a damage key, and that key can sit in a flow mapping
            # (`damage: { heat: 33 }`) as readily as on its own line.
            #
            # THE LEFT BOUNDARY IS WHAT KEEPS IT HONEST: without it `heat:`
            # answers for `overheat:` and the query hands back rows the mechanic
            # cannot reach — 57 weapons where 12 carry it.
            if not re.search(
                rf"(element:\s*{re.escape(want)}\b|(?<![A-Za-z0-9_]){re.escape(want)}\s*:)",
                body,
            ):
                continue
            spec = load(f)
            if isinstance(spec, dict) and spec.get("id"):
                hits[family].add(str(spec["id"]))
    return hits


def row_names(row: dict) -> dict[str, set[str]]:
    return {
        "weapons": {row.get("weapon", "")},
        "mods": set(row.get("mods") or []) | ({row["exilus"]} if row.get("exilus") else set()),
        "arcanes": set(row.get("arcanes") or []),
        "evolutions": set(row.get("evolutions") or []),
    }


def globbed(names: set[str], patterns: list[str]) -> bool:
    return any(fnmatch.fnmatch(n, p) for n in names for p in patterns)


def identity(row: dict) -> str:
    """The GROUP a published row belongs to: `(weapon, mode)`.

    A weapon with n modes is n independent rankings (docs/BOARD.md), and the
    group is the coarsest thing a published row names about itself — it carries
    no build id, because the id is derived from the build and a stored copy of a
    derived fact is the one that goes stale.
    """
    return f"{row.get('weapon', '')}#{row.get('mode') or 'base'}"


def main() -> int:
    ap = argparse.ArgumentParser(add_help=True, description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--weapon", action="append", default=[])
    ap.add_argument("--mod", action="append", default=[])
    ap.add_argument("--arcane", action="append", default=[])
    ap.add_argument("--evolution", action="append", default=[])
    ap.add_argument("--element")
    ap.add_argument("--mode")
    ap.add_argument("--board", action="append", default=[])
    ap.add_argument("--riven", action="store_true")
    ap.add_argument("--plain", action="store_true")
    ap.add_argument("--delete", action="store_true")
    ap.add_argument("--rows", action="store_true")
    a = ap.parse_args()

    grants: dict[str, set[str]] = {}
    if a.element:
        grants = entities_granting(a.element)
        total = sum(len(v) for v in grants.values())
        print(f"{a.element}: granted by {total} entities "
              + ", ".join(f"{len(v)} {k}" for k, v in sorted(grants.items()) if v),
              file=sys.stderr)
        if not total:
            print(f"nothing in data/ grants '{a.element}' — check the name", file=sys.stderr)
            return 2

    # THE PUBLISHED BOARD IS ONE FILE PER WEAPON, and the file's NAME is the
    # weapon: a row states the build and nothing that is derivable from where it
    # sits. So the weapon is put back on the row here, once, and everything
    # below asks a whole row.
    matched: list[tuple[str, dict]] = []
    seen = 0
    for f in sorted((ROOT / "site" / "board").glob("*.json")):
        if f.stem in ("index", "meta"):
            continue
        for row in json.loads(f.read_text(encoding="utf-8")):
            row = dict(row, weapon=f.stem)
            bid = row.get("benchmark") or ""
            if a.board and bid not in a.board:
                continue
            seen += 1
            names = row_names(row)
            has_riven = RIVEN_SLOT in names["mods"] or bool(row.get("riven"))
            if a.riven and not has_riven:
                continue
            if a.plain and has_riven:
                continue
            if a.mode and (row.get("mode") or "base") != a.mode:
                continue
            for flag, family in ((a.weapon, "weapons"), (a.mod, "mods"),
                                 (a.arcane, "arcanes"), (a.evolution, "evolutions")):
                if flag and not globbed(names[family], flag):
                    break
            else:
                if grants and not any(names[k] & v for k, v in grants.items()):
                    continue
                matched.append((bid, row))

    groups = sorted({identity(r) for _, r in matched})
    print(f"{len(matched)} of {seen} published rows, {len(groups)} group(s)",
          file=sys.stderr)

    if a.rows:
        for bid, r in matched:
            print(f"{bid} {identity(r)} {r.get('score')}")
    elif a.delete:
        # ONE STATEMENT PER (ruler, weapon, mode), WHICH IS THE GROUP.
        #
        # IT RETIRES THE WHOLE GROUP AND NOT ONLY THE ROWS THAT MATCHED, because
        # a published row names no build id — it states the build, and the id is
        # derived from that by the engine. Over-deleting costs TIME: the rows
        # that did not need it are measured again and come back the same. The
        # other way round would leave a repaired mechanic sitting under a stale
        # number nobody can argue the board out of.
        wanted: set[tuple[str, str, str]] = set()
        for bid, r in matched:
            wanted.add((bid, r.get("weapon", ""), r.get("mode") or "base"))
        where = " OR ".join(
            f"(s.ruler = '{ruler}' AND s.mode = '{mode}' AND b.weapon = '{weapon}')"
            for ruler, weapon, mode in sorted(wanted))
        # WHAT IT COSTS, FROM THE ONE PLACE THAT KNOWS. Every fact records what
        # it took to measure; the published file does not carry that, so the
        # price is a SELECT over the same rows the DELETE names. Run it first —
        # a change reaching eight thousand rows is a different decision from one
        # reaching forty, and that belongs on screen rather than in the bill.
        print("SELECT count(*) AS rows, sum(s.cost_seconds) / 60 AS cpu_minutes")
        print("FROM scores s JOIN builds b ON b.id = s.identity")
        print(f"WHERE {where};")
        print()
        print("DELETE FROM scores WHERE rowid IN (")
        print("  SELECT s.rowid FROM scores s JOIN builds b ON b.id = s.identity")
        print(f"  WHERE {where});")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
