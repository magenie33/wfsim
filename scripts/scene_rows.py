# SPDX-License-Identifier: AGPL-3.0-or-later
"""A SCENE'S FIRST PASS, ASKED AT A PACE — the base rulers' rows all, a cell's
rows at most N a run, the most popular weapons' best builds first.

    python3 scripts/scene_rows.py <facts.ndjson> <popularity.json> <missing.ndjson> <n> > out.ndjson

A new rung owes every build a row at once (docs/BOARD.md §"A ruler's ladder"),
and asking a whole board in one run floods the factory's open questions; what
is left is asked the next run. A cell's rows go in ROUNDS of `ROUND` builds a
weapon, its best on the base ruler first, so a weapon's board row lands early;
inside a round the weapon `/api/popularity` counts more goes first. A cell's id
is its base's plus `__<rung>`; ties by the row itself, so two runs over the same
input ask the same rows. An unreadable popularity file orders by score alone.
"""
import json
import sys

ROUND = 10


def main(facts_path, popularity_path, rows_path, n):
    best = {}
    with open(facts_path, encoding="utf-8") as f:
        for line in f:
            if line.strip():
                v = json.loads(line)
                if "__" not in v["ruler"]:
                    best[(v["identity"], v["ruler"], v.get("mode") or "base")] = float(v.get("score") or 0)
    try:
        with open(popularity_path, encoding="utf-8") as f:
            popular = json.load(f).get("scores") or {}
    except (OSError, ValueError, AttributeError):
        popular = {}
    cells = []
    with open(rows_path, encoding="utf-8") as f:
        for line in f:
            if not line.strip():
                continue
            r = json.loads(line)
            row = {"build_id": r["build_id"], "ruler": r["ruler"], "mode": r["mode"]}
            if "__" not in r["ruler"]:
                print(json.dumps(row, separators=(",", ":")))
                continue
            score = best.get((r["build_id"], r["ruler"].split("__")[0], r["mode"]))
            cells.append((r.get("weapon") or "", score, row))
    # A BUILD'S PLACE AMONG ITS WEAPON'S on this cell and mode, by base score.
    rank, seen = {}, {}
    for weapon, score, row in sorted(cells, key=lambda c: (c[0], c[2]["ruler"], c[2]["mode"], c[1] is None, -(c[1] or 0), c[2]["build_id"])):
        group = (weapon, row["ruler"], row["mode"])
        rank[(row["build_id"], row["ruler"], row["mode"])] = seen.get(group, 0)
        seen[group] = seen.get(group, 0) + 1
    cells.sort(key=lambda c: (rank[(c[2]["build_id"], c[2]["ruler"], c[2]["mode"])] // ROUND,
                              -popular.get(c[0], 0), c[1] is None, -(c[1] or 0),
                              c[2]["ruler"], c[2]["build_id"], c[2]["mode"]))
    for _, _, row in cells[:n]:
        print(json.dumps(row, separators=(",", ":")))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]))
