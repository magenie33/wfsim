"""How the community computes: the machines that ran board work, read back.

    python scripts/compute.py              # every day `verifier_hours` holds
    python scripts/compute.py --days 7

Reads the fact factory's tables, read-only, from the library's Postgres
through `scripts/pgq.mjs`: PG_URL (docs/BOARD.md §"Postgres").
docs/ANALYTICS.md §"Community compute".

TOTALS ONLY. A device is a random id its browser made; this script reads ids to
count them and prints none, and it never reads the accounts database.
"""

import argparse
import json
import os
import subprocess
import sys
from collections import defaultdict
from datetime import date, datetime, timedelta, timezone

# A UTC HOUR's offset for the hour-of-day table: the readers are mostly UTC+8.
SHOW_OFFSET = 8


def db(query):
    if not os.environ.get("PG_URL"):
        sys.exit('compute.py: PG_URL is not set — the library\'s Postgres, docs/BOARD.md §"Postgres"')
    here = os.path.dirname(os.path.abspath(__file__))
    r = subprocess.run(["node", os.path.join(here, "pgq.mjs")], input=json.dumps({"sql": query}),
                       capture_output=True, text=True, encoding="utf-8")
    try:
        return json.loads(r.stdout)["result"][0]["results"]
    except (ValueError, IndexError, KeyError):
        sys.exit(f"compute.py: postgres failed\n{r.stdout[-400:]}{r.stderr[-400:]}")


def quantile(xs, q):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(q * len(xs)))] if xs else 0


def big(n):
    for unit, k in (("T", 1e12), ("G", 1e9), ("M", 1e6), ("k", 1e3)):
        if n >= k:
            return f"{n / k:.1f}{unit}"
    return str(n)


def pct(a, b):
    return f"{100 * a / b:5.1f}%" if b else "    -"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--days", type=int, default=90)
    ap.add_argument("--before", type=date.fromisoformat, help="exclusive UTC end day; defaults to including today")
    ap.add_argument("--json", action="store_true", help="aggregate work receipt, no device ids or CPU-hour conversion")
    args = ap.parse_args()
    if args.days <= 0:
        ap.error("--days must be positive")
    until = args.before or datetime.now(timezone.utc).date()
    since = (until - timedelta(days=args.days)).isoformat()

    # ONE ROW PER (device, UTC hour) it sent anything in: `tasks` results sent,
    # `ms` their fights' time on its machine, `work` what facts credited it. A
    # device COMPUTED in an hour only if it sent tasks: work alone is credit for
    # results it sent earlier, and the totals carried in from before hours were kept.
    end = f" AND hour < '{args.before.isoformat()}'" if args.before else ""
    rows = db(f"SELECT verifier v, hour h, tasks t, ms, work w FROM verifier_hours WHERE hour >= '{since}'{end}")
    if args.json:
        daily = defaultdict(lambda: {"devices": set(), "tasks": 0, "credited_work": 0, "client_elapsed_ms": 0})
        for r in rows:
            d = daily[r["h"][:10]]
            if r["t"]:
                d["devices"].add(r["v"])
            d["tasks"] += r["t"]
            d["credited_work"] += r["w"]
            d["client_elapsed_ms"] += r["ms"]
        print(json.dumps({"since": since, "before": args.before.isoformat() if args.before else None,
            "unit": "billionths_of_credited_point", "devices": len({r["v"] for r in rows if r["t"]}),
            "tasks": sum(r["t"] for r in rows), "credited_work": sum(r["w"] for r in rows),
            "client_elapsed_ms": sum(r["ms"] for r in rows),
            "active_device_hours": sum(bool(r["t"]) for r in rows),
            "first_hour": min((r["h"] for r in rows), default=None), "last_hour": max((r["h"] for r in rows), default=None),
            "days": [{"day": day, **d, "devices": len(d["devices"])} for day, d in sorted(daily.items())]}, indent=2))
        return
    if not rows:
        print("no compute in the window")
        return
    first = {}
    by_day = defaultdict(lambda: defaultdict(lambda: [0, 0, 0, 0]))  # day -> device -> hours, tasks, ms, work
    by_hour = defaultdict(set)
    for r in rows:
        d = r["h"][:10]
        s = by_day[d][r["v"]]
        if r["t"]:
            first[r["v"]] = min(first.get(r["v"], r["h"]), r["h"])
            s[0] += 1
            by_hour[r["h"]].add(r["v"])
        s[1] += r["t"]
        s[2] += r["ms"]
        s[3] += r["w"]
    days = sorted(by_day)
    while days and not any(s[0] for s in by_day[days[0]].values()):
        days.pop(0)
    print(f"covered {days[0]} .. {days[-1]} UTC ({len(days)} days); the first and last are partial\n")

    print(f"{'day':10}  {'devices':>7}  {'new':>4}  {'tasks':>7}  {'elapsed h':>9}  {'work':>9}  "
          f"{'online h p50':>12}  {'p90':>4}  {'>=12h':>5}")
    for d in days:
        devs = {v: s for v, s in by_day[d].items() if s[0]}
        hours = [s[0] for s in devs.values()]
        new = sum(1 for v in devs if first.get(v, "")[:10] == d)
        print(f"{d:10}  {len(devs):7}  {new:4}  {sum(s[1] for s in devs.values()):7}  "
              f"{sum(s[2] for s in devs.values()) / 3.6e6:6.1f}  {big(sum(s[3] for s in by_day[d].values())):>9}  "
              f"{quantile(hours, .5):12}  {quantile(hours, .9):4}  {sum(h >= 12 for h in hours):5}")

    # COME BACK: of the devices first seen on a day, how many sent work on a later one.
    print(f"\n{'first day':10}  {'devices':>7}  {'+1 day':>7}  {'+2 days':>7}  {'+7 days':>7}")
    active = {d: {v for v, s in by_day[d].items() if s[0]} for d in days}
    for d in days:
        cohort = {v for v, h in first.items() if h[:10] == d}
        if not cohort:
            continue
        cols = []
        for k in (1, 2, 7):
            later = (date.fromisoformat(d) + timedelta(days=k)).isoformat()
            cols.append(pct(len(cohort & active[later]), len(cohort)) if later in active else "      -")
        print(f"{d:10}  {len(cohort):7}  " + "  ".join(f"{c:>7}" for c in cols))

    # HOW SPREAD THE WORK IS: the share the busiest devices did.
    total = defaultdict(int)
    for d in days:
        for v, s in by_day[d].items():
            total[v] += s[3]
    work = sorted(total.values(), reverse=True)
    whole = sum(work) or 1
    top = lambda f: sum(work[:max(1, round(len(work) * f))]) / whole
    print(f"\nwork over {len(work)} devices: the busiest 1% did {100 * top(.01):.0f}%, "
          f"10% did {100 * top(.1):.0f}%, half did {100 * top(.5):.0f}%")

    # WHEN THEY RUN: devices computing in each hour of the day, averaged over the
    # days that hour was kept on, shown in UTC+8.
    kept = sorted(h for h, vs in by_hour.items() if vs)
    per = defaultdict(list)
    d = date.fromisoformat(days[0])
    while d.isoformat() <= days[-1]:
        for h in range(24):
            key = f"{d.isoformat()}T{h:02}"
            if kept[0] <= key < kept[-1]:
                per[(h + SHOW_OFFSET) % 24].append(len(by_hour.get(key, ())))
        d += timedelta(days=1)
    mean = lambda h: f"{sum(per[h]) / len(per[h]):4.0f}" if per[h] else "   -"
    print(f"\ndevices computing by hour of the day (UTC+{SHOW_OFFSET}, mean over the days kept):")
    print("  " + "  ".join(f"{h:02}:{mean(h)}" for h in range(12)))
    print("  " + "  ".join(f"{h:02}:{mean(h)}" for h in range(12, 24)))

    # WHO SAID YES, by the day of the statement they agreed to last.
    agreed = db(f"SELECT substr(consent_at, 1, 10) d, COUNT(*) n FROM verifiers "
                f"WHERE consent_at >= '{since}' GROUP BY d ORDER BY d")
    print("\nconsents by day: " + ", ".join(f"{r['d']} {r['n']}" for r in agreed))


if __name__ == "__main__":
    main()
