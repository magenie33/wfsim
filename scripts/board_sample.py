#!/usr/bin/env python3
"""ONE PUBLISHED ROW PER WEAPON, as submissions the scorer can be fed.

WHAT IT IS FOR. A code change declares every stored score stale, because the
engine fingerprint is a hash of `engine`, `webapi` and `cli` and a score is a
pure function of the code that produced it. Most changes cannot MOVE a number —
a comment, a test, a validation rule, a field only the page reads — and paying
a full rescore for them is what stopped the board updating: 130 hours of CPU and
four of wall clock, against a schedule that fires every twenty minutes.

So the workflow does not guess. It re-scores THIS sample under the new code and
compares each row with what the board already says (`wfsim-board --verify`).
Identical throughout and the code is score-equivalent: the published numbers are
what this code computes, so the run reuses them. One difference and it falls
back to the full rescore it would have done anyway.

STRATIFIED BY WEAPON, and that is the whole design. A change that moves a number
moves it for some weapon, so a sample holding every weapon catches any change
that is not confined to a subset of ONE weapon's builds — and a uniform sample
of a 7,000-row board would miss a melee-only mechanic outright. The weekly full
rescore is the backstop for what remains.

THE WEAPON'S BEST ROW, not a random one. It is the row every other row in the
group is measured against (the floor is half the leader), so it is both the row
most worth being right about and the one the probe screen cannot turn away.

NO RIVEN ROWS. A riven row's rolls are SEARCHED rather than stored, so scoring
one again pays for the corner search and compares a number that was an argmax
— expensive, and a weaker statement than the plain rows give for free.

    python scripts/board_sample.py boards/single_target.yaml > sample.json
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import yaml

# EVERY AXIS OF THE BUILD, and dropping one is not a smaller sample — it is a
# DIFFERENT build, which the door then refuses. `valence` was the one that
# proved it: without it every Coda and Kuva weapon came back "has no Valence
# element, and every copy of it comes out of a Lich with one", so the whole
# adversary roster verified nothing at all.
#
# A submission never carries a SCORE — that is the point of the board — so the
# only field deliberately dropped is the one being re-derived. `fp`, `cost` and
# `listed` are the scorer's own bookkeeping and are not the build either.
SUBMISSION_FIELDS = ("weapon", "mode", "mods", "arcanes", "evolutions", "exilus",
                     "valence", "grip", "loader", "arcane_rank")


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: board_sample.py <board.yaml>", file=sys.stderr)
        return 2
    path = Path(sys.argv[1])
    board = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
    bench = board.get("benchmark") or path.stem
    best: dict[str, dict] = {}
    for e in board.get("entries") or []:
        if e.get("riven"):
            continue
        w = e.get("weapon")
        if w and (w not in best or (e.get("score") or 0) > (best[w].get("score") or 0)):
            best[w] = e
    subs = []
    for w in sorted(best):
        s = {k: v for k, v in best[w].items() if k in SUBMISSION_FIELDS}
        s["benchmark"] = bench
        subs.append(s)
    json.dump(subs, sys.stdout)
    print(f"{path.name}: {len(subs)} weapons sampled", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
