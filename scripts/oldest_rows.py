# SPDX-License-Identifier: AGPL-3.0-or-later
"""THE OLDEST OWED ROWS FIRST, at most N — the refresh's hourly asks.

    python3 scripts/oldest_rows.py <facts.ndjson> <rows.ndjson> <n> [<room> <force_before>] > out.ndjson
    python3 scripts/oldest_rows.py --self-test

`rows` are queue lines ({build_id, ruler, mode}); each is ordered by when its
fact in `facts` finished, ties by the row itself so two runs over the same input
ask the same rows. A ROW WITH NO FACT IS NOT A REFRESH: it is the reconciliation's
to ask, at its own pace (`scene_rows.py`); counted here, every unmeasured row of a
new scene would be the oldest, and forced.

ROOM IS WHAT THE FACTORY HAS SPACE FOR, and a row whose fact finished before
`force_before` is asked whatever the room: at most N, at least those, and
otherwise no more than `room`. Spare volunteers refresh a row early; a busy
fleet lets it wait, never past the age `force_before` names.
"""
import io
import json
import os
import sys
import tempfile
from contextlib import redirect_stdout


def main(facts_path, rows_path, n, room=None, force_before=""):
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
                at = finished.get((r["build_id"], r["ruler"], r["mode"]))
                if at:
                    rows.append((at, r["build_id"], r["ruler"], r["mode"]))
    rows.sort()
    if room is not None:
        forced = sum(1 for finished, *_ in rows if finished < force_before)
        n = min(n, max(room, forced, 0))
    for _, build_id, ruler, mode in rows[:n]:
        print(json.dumps({"build_id": build_id, "ruler": ruler, "mode": mode}, separators=(",", ":")))


def self_test():
    facts = [("old", "2026-09-20"), ("mid", "2026-10-01"), ("new", "2026-10-05")]
    with tempfile.TemporaryDirectory() as d:
        fp, rp = os.path.join(d, "f"), os.path.join(d, "r")
        with open(fp, "w", encoding="utf-8") as f:
            for i, at in facts:
                f.write(json.dumps({"identity": i, "ruler": "r", "mode": "base", "finished_at": at}) + "\n")
        with open(rp, "w", encoding="utf-8") as f:
            for i in ["unmeasured", *(i for i, _ in reversed(facts))]:
                f.write(json.dumps({"build_id": i, "ruler": "r", "mode": "base"}) + "\n")

        def asked(*args):
            out = io.StringIO()
            with redirect_stdout(out):
                main(fp, rp, *args)
            return [json.loads(x)["build_id"] for x in out.getvalue().splitlines()]

        cases = [
            ("the oldest first, at most n", asked(2), ["old", "mid"]),
            ("no room asks only what is forced", asked(10, 0, "2026-09-30"), ["old"]),
            ("room asks the oldest up to it", asked(10, 2, "2026-09-30"), ["old", "mid"]),
            ("a room below zero is none", asked(10, -5, "2026-09-01"), []),
            ("what is forced is asked past the room", asked(10, 1, "2026-10-03"), ["old", "mid"]),
            ("...but never past n", asked(1, 0, "2026-10-09"), ["old"]),
            ("a row with no fact is never a refresh", asked(10, 0, "2026-12-01"), ["old", "mid", "new"]),
        ]
    for what, got, want in cases:
        print(f"  {'ok  ' if got == want else 'FAIL'}  {what}" + ("" if got == want else f"  got {got}, want {want}"))
    sys.exit(1 if any(got != want for _, got, want in cases) else 0)


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        self_test()
    elif len(sys.argv) == 6:
        main(sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4]), sys.argv[5])
    else:
        main(sys.argv[1], sys.argv[2], int(sys.argv[3]))
