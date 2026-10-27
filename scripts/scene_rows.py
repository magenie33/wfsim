# SPDX-License-Identifier: AGPL-3.0-or-later
"""Missing rows admitted at a pace, one weapon before the next, popular first.

    scene_rows.py <facts> <popularity> <missing> <scene_limit> [total_limit]

The total cap includes base rulers and cells: a library backfill can owe a
whole board without adding a scene. Rows outside the cap remain missing for
the next pass. Inside a weapon, base scores order the rows; ties use the row
itself. Without a total cap, base rows are unlimited. An unreadable popularity
file orders by weapon. See docs/BOARD.md §"A ruler's ladder".
"""
import heapq
import itertools
import json
import sys
import subprocess
import tempfile
from pathlib import Path


def self_test():
    with tempfile.TemporaryDirectory() as d:
        root = Path(d)
        facts, popular, missing = (root / name for name in ("facts", "popular", "missing"))
        facts.write_text("")
        popular.write_text(json.dumps({"scores": {"popular": 10, "other": 1}}))
        rows = [{"build_id": str(i), "ruler": ruler, "mode": "base", "weapon": weapon}
                for i, (weapon, ruler) in enumerate([
                    ("other", "single"), ("popular", "single"),
                    ("popular", "single__crowd"), ("other", "single__crowd")])]
        def paced(records, scene_limit=10):
            missing.write_text("".join(json.dumps(r) + "\n" for r in records))
            out = subprocess.check_output([sys.executable, __file__, str(facts), str(popular), str(missing), str(scene_limit), "2"], text=True)
            return [json.loads(line) for line in out.splitlines()]
        first = paced(rows)
        assert len(first) == 2, f"the total cap emitted {len(first)} rows"
        assert all(r["weapon"] == "popular" for r in first), first
        ids = {r["build_id"] for r in first}
        second = paced([r for r in rows if r["build_id"] not in ids])
        assert len(second) == 2 and ids.isdisjoint(r["build_id"] for r in second)
        assert ids | {r["build_id"] for r in second} == {r["build_id"] for r in rows}
        assert all("__" not in r["ruler"] for r in paced(rows, 0))
    print("scene pacing: the combined cap preserves every row and popularity order")


def main(facts_path, popularity_path, rows_path, n, total=None):
    if n < 0 or (total is not None and total < 0):
        raise ValueError("row limits must be nonnegative")
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
    def entries(cells):
        with open(rows_path, encoding="utf-8") as f:
            for line in f:
                if not line.strip():
                    continue
                r = json.loads(line)
                if ("__" in r["ruler"]) != cells:
                    continue
                row = {"build_id": r["build_id"], "ruler": r["ruler"], "mode": r["mode"], "weapon": r.get("weapon") or ""}
                score = best.get((r["build_id"], r["ruler"].split("__")[0], r["mode"]))
                key = (-popular.get(row["weapon"], 0), row["weapon"], score is None, -(score or 0),
                       row["ruler"], row["build_id"], row["mode"])
                yield key, row

    key = lambda item: item[0]
    cells = heapq.nsmallest(n if total is None else min(n, total), entries(True), key=key)
    if total is None:
        kept = itertools.chain(entries(False), cells)
    else:
        base = heapq.nsmallest(total, entries(False), key=key)
        kept = heapq.nsmallest(total, itertools.chain(base, cells), key=key)
    for _, row in kept:
        print(json.dumps(row, separators=(",", ":")))


if __name__ == "__main__":
    if sys.argv[1] == "--self-test":
        self_test()
    else:
        main(sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]),
             int(sys.argv[5]) if len(sys.argv) > 5 else None)
