# SPDX-License-Identifier: AGPL-3.0-or-later
"""THE ORDER A BATCH IS ASKED IN — the most popular weapon first.

    python3 scripts/popular_order.py <popularity.json> <facts.ndjson> < rows.ndjson > rows.ndjson
    python3 scripts/popular_order.py <popularity.json> <facts.ndjson> --split DIR < rows.ndjson

Weapons in the order `/api/popularity` counts them, inside one by the row's
last score (the builds its board shows first), then by the row itself, so two
runs over the same input agree. Each row names its weapon's place as `order`,
which the worker turns into its deadline (`rowAsk` in worker/board_tasks.js):
the batch's day plus the producer's deadline, a second a place. With `--split`,
one file a weapon in that order, `DIR/<place>-<weapon>.ndjson`, for one batch a
weapon (docs/BOARD.md §"Compute orders"). An unreadable popularity file orders
by weapon.
"""
import json
import os
import sys


def main(popularity_path, facts_path, split_dir=None):
    try:
        with open(popularity_path, encoding="utf-8") as f:
            popular = json.load(f).get("scores") or {}
    except (OSError, ValueError, AttributeError):
        popular = {}
    score = {}
    try:
        with open(facts_path, encoding="utf-8") as f:
            for line in f:
                if line.strip():
                    v = json.loads(line)
                    score[(v["identity"], v["ruler"], v.get("mode") or "base")] = float(v.get("score") or 0)
    except OSError:
        pass
    rows = [json.loads(line) for line in sys.stdin if line.strip()]
    rows.sort(key=lambda r: (-popular.get(r.get("weapon") or "", 0), r.get("weapon") or "",
                             -score.get((r["build_id"], r["ruler"], r["mode"]), -1.0),
                             r["ruler"], r["build_id"], r["mode"]))
    place, last, out = -1, None, None
    for r in rows:
        weapon = r.get("weapon") or ""
        if weapon != last:
            place, last = place + 1, weapon
            if split_dir is not None:
                if out:
                    out.close()
                out = open(os.path.join(split_dir, f"{place:04d}-{weapon}.ndjson"), "w", encoding="utf-8")
        r["order"] = place
        line = json.dumps(r, separators=(",", ":")) + "\n"
        (out or sys.stdout).write(line)
    if out:
        out.close()


if __name__ == "__main__":
    args = sys.argv[1:]
    split = None
    if "--split" in args:
        i = args.index("--split")
        split = args[i + 1]
        del args[i:i + 2]
    main(args[0], args[1], split)
