#!/usr/bin/env python3
"""Is the fleet being handed work?

A STARVED FLEET LOOKS LIKE A BUSY ONE. Devices stay online and ask, the page
counts them as computing, and every speed it shows is a day's mean, so hours
of nobody handed anything barely move it. The daemon reads the fleet each
minute into `demand.json`'s `now` and says there whether it is starved
(worker/contribution.js `starvedOf`, docs/BOARD.md §"Demand"); this fails on
that verdict, and on a picture old enough that the daemon has stopped.

READ-ONLY AND CREDENTIAL-FREE: it fetches the object the page fetches.

    python scripts/check_dispatch.py              # exits non-zero when starved or stale
"""
import argparse
import json
import sys
import time
import urllib.request
from datetime import datetime

DEMAND = "https://cdn.wfsim.app/demand.json"

# The daemon writes the picture every minute and the edge keeps it 30 s; a
# quarter hour is a daemon that stopped, not a slow write.
MAX_MINUTES = 15.0

TIMEOUT = 60

# Cloudflare refuses the default `Python-urllib/3.x` agent with a 403.
UA = "wfsim-dispatch-check/1 (+https://wfsim.app)"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--demand", default=DEMAND, help="the picture's url")
    ap.add_argument("--max-minutes", type=float, default=MAX_MINUTES)
    args = ap.parse_args()
    try:
        req = urllib.request.Request(args.demand, headers={"User-Agent": UA, "Cache-Control": "no-cache"})
        with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
            d = json.loads(r.read())
    except Exception as e:  # noqa: BLE001 — any failure to read is the page's failure too
        print(f"FAIL  {args.demand} unreadable: {e}")
        return 1
    try:
        at = datetime.fromisoformat(d["at"].replace("Z", "+00:00")).timestamp()
    except (KeyError, ValueError, AttributeError):
        print(f"FAIL  {args.demand} dates no picture")
        return 1
    minutes = (time.time() - at) / 60
    if minutes > args.max_minutes:
        print(f"FAIL  the picture is {minutes:.0f} min old (limit {args.max_minutes:g}): the daemon stopped")
        return 1
    now = d.get("now")
    if not now:
        print(f"FAIL  {args.demand} carries no reading of the fleet")
        return 1
    said = (f"{now['asking']} devices asked in {now['window_seconds'] // 60} min, "
            f"{now['answered']} answers, {now['held']} rows held")
    if now.get("starved"):
        print(f"FAIL  starved: {said}, while rows wait on the server")
        return 1
    print(f"ok    {said}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
