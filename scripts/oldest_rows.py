# SPDX-License-Identifier: AGPL-3.0-or-later
"""THE OLDEST OWED ROWS FIRST, at most N — the refresh's hourly asks.

    python3 scripts/oldest_rows.py <facts.ndjson> <rows.ndjson> <n> > out.ndjson

`rows` are queue lines ({build_id, ruler, mode}); each is ordered by when its
fact in `facts` finished, a row with none first, and ties by the row itself so
two runs over the same input ask the same rows.
"""
import json
import sys


def main(facts_path, rows_path, n):
    finished = {}
    with open(facts_path, encoding="utf-8") as f:
        for line in f:
            if line.strip():
                v = json.loads(line)
                finished[(v["identity"], v["ruler"], v.get("mode") or "base")] = v.get("finished_at") or ""
    rows = []
    with open(rows_path, encoding="utf-8") as f:
        for line in f:
            if line.strip():
                r = json.loads(line)
                rows.append((finished.get((r["build_id"], r["ruler"], r["mode"]), ""), r["build_id"], r["ruler"], r["mode"]))
    rows.sort()
    for _, build_id, ruler, mode in rows[:n]:
        print(json.dumps({"build_id": build_id, "ruler": ruler, "mode": mode}, separators=(",", ":")))


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], int(sys.argv[3]))
