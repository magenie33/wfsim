#!/usr/bin/env python3
"""WHICH PUBLISHED ROWS CARRY A THING, and what rescoring them would cost.

THE BOARD DOES NOT DECIDE THIS ANY MORE, and that is the point. A fingerprint
answers "did an input move", which is a question about FILES; a person fixing a
mechanic asks "which rows can this reach", which is a question about BUILDS, and
no hash answers it. Nothing automatic re-scores more than a bounded slice
(docs/BOARD.md), so repairing a mechanic on purpose means naming the rows —
and naming them by hand across 22,977 is not a thing anyone does twice.

It prints what `--rescore` takes, so the two halves compose:

    python scripts/board_select.py --element heat --selectors
    # …paste into Actions -> board -> Run workflow -> weapon

AND IT PRICES THE ANSWER BEFORE ANYONE PRESSES ANYTHING. Every row records what
it cost to measure, so the summary says how many rows, how many groups and how
many CPU minutes the rescore is. A change that reaches eight thousand rows is a
different decision from one that reaches forty, and the difference should be on
screen before the button, not in the bill afterwards.

BATCH THE FIXES, THEN RESCORE ONCE. Ten corrections landing separately are ten
rescores of overlapping rows; landing them together is one. That is the whole
economy of doing this by hand, and it is why this prints a selector rather than
starting anything.

    --weapon/--mod/--arcane/--evolution   glob, repeatable, any-of within a flag
    --element <name>                      any entity whose data grants it
    --mode <id> --riven --plain --board <id>
    --selectors                           print what `--rescore` accepts
    --rows                                print one key per line instead
"""
from __future__ import annotations

import argparse
import fnmatch
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
    """The row's key, rebuilt the way `builds::identity` writes it.

    IT IS NOT REBUILT, IT IS READ: the board stores `fp` and the axes, and the
    scorer's own key is what `--rescore` matches, so this prints the coarsest
    selector that covers a row rather than a key it might spell differently.
    A group is `(weapon, mode)` and that is what the coarse form names.
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
    ap.add_argument("--selectors", action="store_true")
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

    matched: list[tuple[str, dict]] = []
    seen = 0
    for f in sorted((ROOT / "boards").glob("*.yaml")):
        board = load(f)
        bid = board.get("benchmark") or f.stem
        if a.board and bid not in a.board:
            continue
        for row in board.get("entries") or []:
            if row.get("probe"):
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

    cost = sum(float(r.get("cost") or 0) for _, r in matched)
    groups = sorted({identity(r) for _, r in matched})
    listed = sum(1 for _, r in matched if r.get("listed", True))
    print(f"{len(matched)} of {seen} rows, {listed} of them published, "
          f"{len(groups)} group(s), {cost / 60:.0f} CPU minutes to rescore",
          file=sys.stderr)

    if a.rows:
        for bid, r in matched:
            print(f"{bid} {identity(r)} {r.get('score')}")
    elif a.selectors:
        # THE COARSEST FORM THAT COVERS THEM. A group whose rows all matched is
        # named once; anything else would hand the operator a wall of keys to
        # paste and no way to read what they add up to.
        by_group: dict[str, int] = defaultdict(int)
        for _, r in matched:
            by_group[identity(r)] += 1
        print(";".join(sorted(by_group)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
