"""What readers did: the usage points `POST /api/e` writes, read back.

    python scripts/usage.py              # the last 14 complete UTC days
    python scripts/usage.py --days 28 --shell web --summary
    python scripts/usage.py --before 2026-11-06 --json

THE LOG IS THE ONLY SOURCE (`usage_points`, docs/ANALYTICS.md §"The log"):
every point, unsampled, read through `scripts/pgq.mjs` from whatever `PG_URL`
names — a role that owns the table, since no pipeline role is granted it.

VISITORS ARE COUNTED HERE, NOT IN SQL: every query returns (day, visitor, event)
rows and Python does the sets, so a number never depends on which aggregates
the SQL dialect has. The log keeps 90 days; a window past that is silently
shorter, so the script says what it actually covered.
"""

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from collections import Counter, defaultdict
from datetime import date, datetime, timedelta, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BOOT = "app.boot"
# THE RELEASE a deploy check writes its one point under, never a reader.
CHECK_RELEASE = "deploy-check"
# A RESULT, as opposed to a page opened: a build computed, a fight or a search finished.
RESULTS = ("builder.weapon", "builder.warframe", "builder.operator", "builder.riven",
           "simulator.run", "optimizer.run")
# A boot that is not a landing: the reader was already on the site.
INSIDE = ("from_site", "reload", "back_forward")
MARKETS = ("china", "overseas", "unknown")
SHELLS = ("web", "desktop")


def MARKET(country):
    if country == "CN":
        return "china"
    return "overseas" if re.fullmatch(r"[A-Z]{2}", country or "") and country not in ("XX", "ZZ") else "unknown"


def visitor_of(cid):
    """The visitor as stored: the first 128 bits of the SHA-256 of the browser's
    id (`usageVisitor`, worker/usage_log.js)."""
    return hashlib.sha256(cid.encode()).hexdigest()[:32]


class Log:
    """`usage_points` in the library's Postgres: every point, so `n` is a count."""

    def __init__(self, start, before=None, shell="all"):
        if not os.environ.get("PG_URL"):
            sys.exit('usage.py: PG_URL is not set — the library\'s Postgres, docs/BOARD.md §"Postgres"')
        self.start = start
        self.before = before or (datetime.now(timezone.utc).date() + timedelta(days=1)).isoformat()
        self.shell = shell

    def q(self, query, *params):
        r = subprocess.run(["node", str(ROOT / "scripts" / "pgq.mjs")],
                           input=json.dumps({"sql": query, "params": [self.start, CHECK_RELEASE, self.before, self.shell, *params]}),
                           capture_output=True, text=True, encoding="utf-8")
        try:
            return json.loads(r.stdout)["result"][0]["results"]
        except (ValueError, IndexError, KeyError):
            sys.exit(f"usage.py: postgres failed\n{r.stdout[-400:]}{r.stderr[-400:]}")

    WHERE = "FROM usage_points WHERE day >= ?1 AND release != ?2 AND day < ?3 AND (?4 = 'all' OR shell = ?4)"

    def rows(self):
        # Aggregate one calendar day at a time; one SELECT keeps the same snapshot.
        return self.q(f"""SELECT points.* FROM generate_series((?1::text)::date::timestamp,
                          (?3::text)::date::timestamp - INTERVAL '1 day', INTERVAL '1 day') AS days(d)
                        CROSS JOIN LATERAL (
                          SELECT day, visitor AS cid, event AS e, shell, country, COUNT(*) AS n,
                                 MIN(at) AS first_at, MAX(at) AS last_at {self.WHERE}
                          AND day = to_char(days.d, 'YYYY-MM-DD')
                          GROUP BY day, visitor, event, shell, country
                        ) AS points""")

    def boots(self):
        return self.q(f"""SELECT visitor AS cid, subject AS arrival, route, lang, shell, country,
                          MIN(n) AS ms, COUNT(*) AS n {self.WHERE} AND event = ?5
                          GROUP BY visitor, subject, route, lang, shell, country""", BOOT)

    def placed(self):
        return self.q(f"SELECT visitor AS cid, country {self.WHERE} GROUP BY visitor, country")

    def probes(self):
        return self.q(f"SELECT country, subject, n AS ms {self.WHERE} AND event = 'road.probe'")

    def subjects(self):
        return self.q(f"""SELECT event AS e, subject, COUNT(*) AS n {self.WHERE} AND event != ?5 AND subject != ''
                          GROUP BY event, subject ORDER BY n DESC LIMIT 5000""", BOOT)

    def held(self):
        return self.q(f"SELECT visitor AS cid, subject, MAX(n) AS n {self.WHERE} AND event = 'presets.saved' "
                      "GROUP BY visitor, subject")

    def visitor(self, cid):
        return self.q(f"""SELECT at AS timestamp, event AS e, subject, route, country, n {self.WHERE}
                          AND visitor = ?5 ORDER BY at LIMIT 5000""", visitor_of(cid))


def visitor_report(rows, start, before, today, shell_filter="all"):
    """Browser sets over calendar windows; no visitor identifiers in the report."""
    start, before = date.fromisoformat(start), date.fromisoformat(before)
    rows = [r for r in rows if shell_filter == "all" or r["shell"] == shell_filter]
    daily = defaultdict(lambda: defaultdict(lambda: defaultdict(set)))
    for r in rows:
        day = str(r["day"])[:10]
        for shell in ("all", r["shell"]):
            daily[day][shell]["visitors"].add(r["cid"])
            if r["e"] in RESULTS:
                daily[day][shell]["result"].add(r["cid"])

    def seen(first, end, shell, metric="visitors"):
        return set().union(*(daily[d.isoformat()][shell][metric]
                             for d in (first + timedelta(days=i) for i in range((end - first).days))))

    def counts(first, end, shell):
        visitors = seen(first, end, shell)
        return {"visitors": len(visitors), "result": len(seen(first, end, shell, "result")),
                "returning": len(visitors & seen(first - timedelta(days=7), first, shell))}

    shells = ("all", *SHELLS) if shell_filter == "all" else (shell_filter,)
    days = [{"day": d.isoformat(), "partial": d >= today,
             "shells": {s: counts(d, d + timedelta(days=1), s) for s in shells}}
            for d in (start + timedelta(days=i) for i in range((before - start).days))]
    current = [r for r in rows if start.isoformat() <= str(r["day"])[:10] < before.isoformat()]
    report = {"source": "usage_points", "unit": "browsers", "timezone": "UTC",
              "from": start.isoformat(), "before": before.isoformat(), "partial": before > today,
              "shell_filter": shell_filter,
              "observed_from": min((str(r["day"])[:10] for r in current), default=None),
              "observed_to": max((str(r["day"])[:10] for r in current), default=None),
              "latest_point": max((r.get("last_at", "") for r in current), default=None),
              "returning_lookback_days": 7,
              "history_observed_from": min((str(r["day"])[:10] for r in rows), default=None),
              "returning_history_complete": any(str(r["day"])[:10] <= (start - timedelta(days=7)).isoformat() for r in rows),
              "period": {s: counts(start, before, s) for s in shells}, "daily": days}
    if (before - start).days >= 14:
        a, b = before - timedelta(days=14), before - timedelta(days=7)
        report["weekly"] = {s: {"previous": len(seen(a, b, s)), "current": len(seen(b, before, s)),
                                 "returned": len(seen(a, b, s) & seen(b, before, s))} for s in shells}
    return report


def print_report(report):
    print(f"UTC {report['from']} .. {report['before']} exclusive; browsers, not people"
          + ("; today is partial" if report["partial"] else "; complete dates only"))
    print(f"observed {report['observed_from']} .. {report['observed_to']}; latest point {report['latest_point']}")
    print("returning: seen in the preceding 7 calendar days; absent points are not proof of zero traffic\n")
    if not report["returning_history_complete"]:
        print("the beginning of the returning lookback has no observations; early returning counts may be incomplete\n")
    print(f"{'day':10}  {'shell':7}  {'visitors':>8}  {'result':>7}  {'returning':>9}")
    for day in report["daily"]:
        for shell, v in day["shells"].items():
            if shell == "all":
                continue
            print(f"{day['day']:10}  {shell:7}  {v['visitors']:8}  {v['result']:7}  {v['returning']:9}"
                  + ("  partial" if day["partial"] else ""))
    print("\nwhole-period unique browsers (daily counts are not added):")
    for shell, v in report["period"].items():
        print(f"  {shell:7} visitors {v['visitors']}; result {v['result']}; returning {v['returning']}")
    for shell, v in report.get("weekly", {}).items():
        print(f"  {shell:7} weekly visitors {v['previous']} -> {v['current']}; returned {v['returned']}")


def pct(a, b):
    return f"{100 * a / b:5.1f}%" if b else "    -"


def quantile(xs, q):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(q * len(xs)))] if xs else None


def seconds(ms):
    return f"{ms / 1000:5.1f}s" if ms is not None else "    -"


def label(e):
    return e.split(".")[-1] if e.startswith("builder.") else e.split(".")[0]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--days", type=int, default=14, help="calendar days ending before --before")
    ap.add_argument("--before", type=date.fromisoformat, help="exclusive UTC end date; default today")
    ap.add_argument("--shell", choices=("all", *SHELLS), default="all")
    ap.add_argument("--json", action="store_true", help="aggregate visitor report only, without browser identifiers")
    ap.add_argument("--summary", action="store_true", help="visitor report only")
    ap.add_argument("--top", type=int, default=15)
    ap.add_argument("--visitor", help="one visitor's history, by the id /support shows")
    args = ap.parse_args()
    today = datetime.now(timezone.utc).date()
    before = args.before or today
    if args.days < 1 or before > today + timedelta(days=1):
        ap.error("--days must be positive; --before cannot be later than tomorrow UTC")
    start = (before - timedelta(days=args.days)).isoformat()
    src = Log(start, before.isoformat(), args.shell)

    if args.visitor:
        return visitor(src, args.visitor)
    history = Log((date.fromisoformat(start) - timedelta(days=7)).isoformat(), before.isoformat(), args.shell).rows()
    report = visitor_report(history, start, before.isoformat(), today, args.shell)
    if args.json:
        print(json.dumps(report, indent=2))
        return
    print_report(report)
    if args.summary:
        return
    rows = [r for r in history if r["day"][:10] >= start]
    if not rows:
        print(f"no usage points in the last {args.days} days")
        return

    by_event = defaultdict(set)
    by_day = defaultdict(lambda: defaultdict(set))
    for r in rows:
        by_event[r["e"]].add(r["cid"])
        by_day[r["day"][:10]][r["e"]].add(r["cid"])
    print("\nmodule visitors per UTC day, within the selected shells:")
    print(f"{'day':10}  " + "  ".join(f"{label(e):>9}" for e in RESULTS))
    for day, events in sorted(by_day.items()):
        print(f"{day:10}  " + "  ".join(f"{len(events.get(e, ())):9}" for e in RESULTS))
    # Mixed and unknown locations stay unknown; no point borrows another's country.
    boots = src.boots()
    locations = defaultdict(set)
    for r in rows:
        locations[r["cid"]].add(MARKET(r["country"]))
    market = {cid: next(iter(ms)) if len(ms) == 1 else "unknown" for cid, ms in locations.items()}
    boot_ms = defaultdict(list)
    for r in boots:
        if float(r["ms"]) > 0:
            boot_ms[market[r["cid"]]].append(float(r["ms"]))
    everyone = set().union(*by_event.values())
    got = set().union(*(by_event.get(e, set()) for e in RESULTS))
    failed = by_event.get("engine.fail", set())
    print(f"\n{'market':9}  {'visitors':>8}  {'activ.':>6}  {'boot p50':>8}  {'boot p90':>8}  {'engine fail':>11}  {'shares':>6}  {'opened':>6}")
    for m in MARKETS:
        who = {c for c in everyone if market.get(c) == m}
        n = lambda e: len(by_event.get(e, set()) & who)
        print(f"{m:9}  {len(who):8}  {pct(len(got & who), len(who)):>6}  {seconds(quantile(boot_ms[m], 0.5)):>8}  "
              f"{seconds(quantile(boot_ms[m], 0.9)):>8}  {pct(len(failed & who), len(who)):>11}  {n('share.create'):6}  {n('share.open'):6}")
    never = failed - by_event.get(BOOT, set())
    if never:
        print(f"  {len(never)} visitor(s) had the engine fail and never booted")

    # THE TWO ROADS, per market: each browser's daily fetch of one public file
    # directly and through the relay (docs/DISTRIBUTION.md §"The relay").
    probes = src.probes()
    if probes:
        print(f"\n{'road':16}  {'probes':>6}  {'failed':>6}  {'p50':>6}  {'p90':>6}")
        for m in MARKETS:
            for road in ("direct", "relay"):
                mine = [r for r in probes if MARKET(r["country"]) == m and r["subject"].startswith(road + "_")]
                ok = [float(r["ms"]) for r in mine if r["subject"].endswith("_ok")]
                print(f"{m + ' ' + road:16}  {len(mine):6}  {pct(len(mine) - len(ok), len(mine)):>6}  "
                      f"{seconds(quantile(ok, 0.5)):>6}  {seconds(quantile(ok, 0.9)):>6}")

    others = [e for e in sorted(by_event) if e != BOOT and e not in RESULTS]
    if others:
        print("\nvisitors per event: " + ", ".join(f"{e} {len(by_event[e])}" for e in others))

    per = defaultdict(list)
    for r in src.subjects():
        per[r["e"]].append((r["subject"], int(float(r["n"]))))
    for e in sorted(per):
        print(f"\n{e}: " + ", ".join(f"{s} {n}" for s, n in per[e][:args.top]))

    # HOW MUCH A BROWSER KEEPS: per pool, each browser's largest count in the
    # window, so a sync allowance is set against the spread rather than a guess.
    pools = defaultdict(list)
    for r in src.held():
        pools[r["subject"]].append(int(float(r["n"])))
    for pool in sorted(pools):
        xs = pools[pool]
        over = ", ".join(f"≥{t} {pct(sum(x >= t for x in xs), len(xs))}" for t in (25, 50, 100, 200))
        print(f"\nsaved {pool} per browser ({len(xs)} browsers): p50 {quantile(xs, .5)}  p75 {quantile(xs, .75)}  "
              f"p90 {quantile(xs, .9)}  p99 {quantile(xs, .99)}  max {max(xs)}  |  {over}")

    # THE MOST ACTIVE VISITORS. A visitor is the hash of a random per-browser id
    # and nothing more: this ranks browsers, and says who they are only if they
    # tell us (their id is on /support). Days seen first, then results produced.
    per = defaultdict(lambda: {"days": set(), "loads": 0, "results": 0, "sims": 0, "searches": 0, "shares": 0})
    for r in rows:
        v, n = per[r["cid"]], int(float(r["n"]))
        v["days"].add(r["day"][:10])
        if r["e"] == BOOT: v["loads"] += n
        if r["e"] in RESULTS: v["results"] += n
        if r["e"] == "simulator.run": v["sims"] += n
        if r["e"] == "optimizer.run": v["searches"] += n
        if r["e"] == "share.create": v["shares"] += n
    where = {r["cid"]: (r["lang"], r["country"], r["shell"]) for r in boots}
    ranked = sorted(per.items(), key=lambda kv: (-len(kv[1]["days"]), -kv[1]["results"], -kv[1]["loads"]))
    print(f"\nmost active visitors (by days seen, then results):")
    print(f"  {'visitor':10}  {'days':>4}  {'loads':>5}  {'results':>7}  {'sims':>4}  {'searches':>8}  {'shares':>6}  where")
    for cid, v in ranked[:args.top]:
        lang, country, shell = where.get(cid, ("?", "?", "?"))
        print(f"  {cid[:8]:10}  {len(v['days']):4}  {v['loads']:5}  {v['results']:7}  {v['sims']:4}  {v['searches']:8}  {v['shares']:6}  {country} {lang} {shell}")

    # HOW THEY ARRIVE: each boot's navigation, as visitors and as page loads.
    # A boot from inside the site, a reload or a back/forward is not a landing;
    # "?" is a boot from before arrivals were recorded.
    arrived, loads = defaultdict(set), Counter()
    for r in boots:
        arrived[r["arrival"] or "?"].add(r["cid"])
        loads[r["arrival"] or "?"] += int(float(r["n"]))
    print("\nhow they arrive (visitors / page loads): " + ", ".join(
        f"{k} {len(v)}/{loads[k]}" for k, v in sorted(arrived.items(), key=lambda kv: -len(kv[1]))[:args.top]))
    landings = [r for r in boots if r["arrival"] not in INSIDE]
    for dim in ("route", "lang", "shell", "country"):
        c = Counter()
        for value, cids in _group(landings, dim).items():
            c[value or "?"] = len(cids)
        print(f"\nvisitors by landing {dim}: " + ", ".join(f"{k} {v}" for k, v in c.most_common(args.top)))


def visitor(src, cid):
    """Every point one browser sent, oldest first — for a browser someone has
    named to us by the whole id /support shows them."""
    if len(cid) != 32 or not all(c in "0123456789abcdef" for c in cid):
        sys.exit("usage.py: a visitor id is the 32 hex characters /support shows")
    rows = src.visitor(cid)
    for r in rows:
        n = float(r["n"])
        print(f"{r['timestamp'][:16]}  {r['e']:17} {r['subject'] or '':24} {r['route']:10} {r['country']}"
              + (f"  n={n:g}" if n else ""))
    if not rows:
        print("no points from that visitor in this window")


def _group(rows, dim):
    out = defaultdict(set)
    for r in rows:
        out[r[dim]].add(r["cid"])
    return out


if __name__ == "__main__":
    main()
