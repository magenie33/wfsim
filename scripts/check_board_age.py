#!/usr/bin/env python3
"""Is the live board still being ranked?

A STALLED BOARD LOOKS LIKE A QUIET ONE. The page shows no board-wide age, and
a row's own time says when that build was measured, so nothing a reader sees
tells a dead pipeline from a day nobody submitted. The ranker stamps every
ruler on each publish and on the hourly read of the store, however little
moved (docs/BOARD.md §"The live board"), so the newest
`scored_at_epoch_seconds` in the public stamp is the last time the pipeline
reached readers end to end: the box, the loop, the push and the CDN.

READ-ONLY AND CREDENTIAL-FREE: it fetches the stamp the page fetches. An
unreachable stamp fails too, since readers cannot reach it either.

    python scripts/check_board_age.py                 # exits non-zero when stale
    python scripts/check_board_age.py --max-hours 6
"""
import argparse
import json
import sys
import time
import urllib.request

STAMP = "https://cdn.wfsim.app/board.meta.json"

# The read of the store takes most of an hour and the next starts an hour after
# it ends, so a healthy board is stamped at least every two hours. Four is two
# reads missed.
MAX_HOURS = 4.0

TIMEOUT = 60

# Cloudflare refuses the default `Python-urllib/3.x` agent with a 403.
UA = "wfsim-board-age-check/1 (+https://wfsim.app)"


def newest(stamp: dict) -> int:
    """The newest stamp over every ruler; zero when none says.

    THE NEWEST, NOT THE OLDEST: a ruler the ranker no longer runs keeps its
    last row in the state, and would hold the oldest at its last stamp.
    """
    boards = stamp.get("boards") or {}
    return max((int(b.get("scored_at_epoch_seconds") or 0) for b in boards.values()), default=0)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--stamp", default=STAMP, help="the stamp's url")
    ap.add_argument("--max-hours", type=float, default=MAX_HOURS)
    args = ap.parse_args()
    try:
        req = urllib.request.Request(args.stamp, headers={"User-Agent": UA, "Cache-Control": "no-cache"})
        with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
            stamp = json.loads(r.read())
    except Exception as e:  # noqa: BLE001 — any failure to read is a reader's failure too
        print(f"FAIL  {args.stamp} unreadable: {e}")
        return 1
    at = newest(stamp)
    if not at:
        print(f"FAIL  {args.stamp} dates no board")
        return 1
    hours = (time.time() - at) / 3600
    when = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(at))
    if hours > args.max_hours:
        print(f"FAIL  last stamped {when}, {hours:.1f} h ago (limit {args.max_hours:g} h)")
        return 1
    print(f"ok    last stamped {when}, {hours:.1f} h ago (limit {args.max_hours:g} h)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
