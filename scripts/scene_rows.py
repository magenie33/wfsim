# SPDX-License-Identifier: AGPL-3.0-or-later
"""A SCENE'S FIRST PASS, ASKED AT A PACE — the base rulers' rows all, a cell's
rows at most N a run, ONE WEAPON AT A TIME, the most popular first.

    python3 scripts/scene_rows.py <facts.ndjson> <popularity.json> <missing.ndjson> <n> > out.ndjson

A new rung owes every build a row at once (docs/BOARD.md §"A ruler's ladder"),
and asking a whole board in one run floods the factory's open questions; what
is left is asked the next run. A WEAPON IS FINISHED BEFORE THE NEXT STARTS,
every build under every cell: the build that leads a scene need not be the one
leading the base, so a weapon half measured ranks the wrong build. Weapons go
in the order `/api/popularity` counts them, inside one by base score. A cell's
id is its base's plus `__<rung>`; ties by the row itself, so two runs over the
same input ask the same rows. An unreadable popularity file orders by weapon.
"""
import json
import sys


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
            row = {"build_id": r["build_id"], "ruler": r["ruler"], "mode": r["mode"], "weapon": r.get("weapon") or ""}
            if "__" not in r["ruler"]:
                print(json.dumps(row, separators=(",", ":")))
                continue
            score = best.get((r["build_id"], r["ruler"].split("__")[0], r["mode"]))
            cells.append((r.get("weapon") or "", score, row))
    cells.sort(key=lambda c: (-popular.get(c[0], 0), c[0], c[1] is None, -(c[1] or 0),
                              c[2]["ruler"], c[2]["build_id"], c[2]["mode"]))
    for _, _, row in cells[:n]:
        print(json.dumps(row, separators=(",", ":")))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]))
