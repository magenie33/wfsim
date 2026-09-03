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


def plain_rows(board: dict) -> list[dict]:
    """Every published row a re-score can reproduce without searching.

    A PROBE ROW IS NOT A CLAIM. Its number is the same fight at a tenth of the
    ruler's runs, recorded to say the build was looked at and never published,
    so measuring it at full precision and calling the difference a disagreement
    would be an alarm the board never asked for.
    """
    return [e for e in (board.get("entries") or [])
            if not e.get("riven") and not e.get("probe")]


def as_submission(entry: dict, bench: str) -> dict:
    s = {k: v for k, v in entry.items() if k in SUBMISSION_FIELDS}
    s["benchmark"] = bench
    return s


def buckets_of(rows: list[dict], budget_s: float) -> list[list[dict]]:
    """The board cut into runs of roughly equal WORK, deterministically.

    BY COST AND NOT BY COUNT, because the rows differ by four orders of
    magnitude: `group_clear`'s median row is 20 s and its worst is 121 minutes,
    so equal counts would give one bucket a two-hour job and the next a
    two-minute one. A row costing more than the whole budget becomes a bucket of
    its own — the tail is audited rarely, which is the correct frequency for the
    rows that cost the most to check.

    ORDERED BY `fp`, which is a hash, so the walk interleaves cheap and
    expensive rows instead of following the board's weapon-by-weapon order —
    and it is stable, so the same board cuts the same way in every run and a
    bucket index means the same thing twice.
    """
    out: list[list[dict]] = []
    cur: list[dict] = []
    spent = 0.0
    for e in sorted(rows, key=lambda e: str(e.get("fp") or "")):
        cost = float(e.get("cost") or 0.0)
        if cur and spent + cost > budget_s:
            out.append(cur)
            cur, spent = [], 0.0
        cur.append(e)
        spent += cost
    if cur:
        out.append(cur)
    return out


def main() -> int:
    args = sys.argv[1:]
    if not args:
        print("usage: board_sample.py <board.yaml> [--budget <seconds> --bucket <k>]",
              file=sys.stderr)
        return 2
    path = Path(args[0])

    def flag(name: str) -> str | None:
        return args[args.index(name) + 1] if name in args[:-1] else None

    board = yaml.safe_load(path.read_text(encoding="utf-8")) or {}
    bench = board.get("benchmark") or path.stem

    # THE ROTATING SLICE, for the audit. The probe asks "did this code change
    # move a number", so it wants one row per weapon and the same rows every
    # time; the audit asks "does the board still say what this code computes",
    # which is a question about EVERY row and is answered a slice at a time.
    #
    # THE KNOB IS THE CROSSING, not a budget in seconds. Each board is cut into
    # the number of runs it should take to audit all of it, so every board
    # finishes its lap together and the expensive one is not still on its first
    # while the cheap ones have been re-read a dozen times. It also states the
    # number the audit is judged by — how long it takes to cross the store.
    crossing = flag("--crossing")
    if crossing is not None:
        rows = plain_rows(board)
        total = sum(float(e.get("cost") or 0.0) for e in rows)
        buckets = buckets_of(rows, total / max(int(crossing), 1))
        k = int(flag("--bucket") or 0) % max(len(buckets), 1)
        chosen = buckets[k] if buckets else []
        json.dump([as_submission(e, bench) for e in chosen], sys.stdout)
        spent = sum(float(e.get("cost") or 0.0) for e in chosen)
        print(f"{path.name}: bucket {k} of {len(buckets)}, {len(chosen)} rows, "
              f"{spent / 60:.1f} min of measured work", file=sys.stderr)
        return 0

    best: dict[str, dict] = {}
    for e in plain_rows(board):
        w = e.get("weapon")
        if w and (w not in best or (e.get("score") or 0) > (best[w].get("score") or 0)):
            best[w] = e
    subs = [as_submission(best[w], bench) for w in sorted(best)]
    json.dump(subs, sys.stdout)
    print(f"{path.name}: {len(subs)} weapons sampled", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
