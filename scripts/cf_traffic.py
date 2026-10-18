"""What the edge saw: Cloudflare's own counts for wfsim.app, read back per UTC day.

    python scripts/cf_traffic.py              # the last 7 days
    python scripts/cf_traffic.py --days 3

THE EDGE IS FOR RUNNING THE SITE, THE LOG IS FOR WHO USES IT (docs/ANALYTICS.md
§"The edge"). This prints requests, cache, status classes, countries, each
worker's requests and exceptions, and every cron run; it writes nothing, and no
number here is ever fed into a count. With `PG_URL` set it adds the log's
visitors per day beside the edge's unique IPs, to be compared, never summed.

It reads with wrangler's login (`npx wrangler login`), refreshed by `wrangler
whoami` on every run. One GraphQL query a run: the account's analytics budget
is shared, and a refused query says how long to wait.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.request
from collections import Counter, defaultdict
from datetime import datetime, timedelta, timezone
from pathlib import Path

ZONE = "wfsim.app"
API = "https://api.cloudflare.com/client/v4"
# Wrangler's login file, by platform; the first that exists is read.
WRANGLER_CONFIG = [
    Path(os.environ.get("APPDATA", "")) / "xdg.config/.wrangler/config/default.toml",
    Path.home() / ".config/.wrangler/config/default.toml",
    Path.home() / "Library/Preferences/.wrangler/config/default.toml",
    Path.home() / ".wrangler/config/default.toml",
]


def token():
    subprocess.run("npx wrangler whoami", shell=True, capture_output=True)
    for p in WRANGLER_CONFIG:
        if p.is_file():
            m = re.search(r'^oauth_token = "([^"]+)"', p.read_text(encoding="utf-8"), re.M)
            if m:
                return m.group(1)
    sys.exit("cf_traffic.py: no wrangler login found — run `npx wrangler login`")


def call(tok, url, body=None):
    req = urllib.request.Request(url, data=body and json.dumps(body).encode(),
                                 headers={"authorization": f"Bearer {tok}", "content-type": "application/json"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.load(r)


def query(zone, account, start, end):
    return f"""{{ viewer {{
  zones(filter: {{zoneTag: "{zone}"}}) {{
    httpRequests1dGroups(limit: 100, filter: {{date_geq: "{start}", date_lt: "{end}"}}, orderBy: [date_ASC]) {{
      dimensions {{ date }}
      uniq {{ uniques }}
      sum {{ requests cachedRequests bytes
             responseStatusMap {{ edgeResponseStatus requests }}
             countryMap {{ clientCountryName requests }} }} }} }}
  accounts(filter: {{accountTag: "{account}"}}) {{
    workersInvocationsAdaptive(limit: 10000, filter: {{date_geq: "{start}", date_lt: "{end}"}}) {{
      dimensions {{ date scriptName status }}
      sum {{ requests errors }} }}
    workersInvocationsScheduled(limit: 10000, filter: {{datetime_geq: "{start}T00:00:00Z", datetime_lt: "{end}T00:00:00Z"}}) {{
      datetime scriptName cron status }} }} }} }}"""


def visitors(start):
    """The log's visitors per day, by the same rule as usage.py."""
    if not os.environ.get("PG_URL"):
        return None
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from usage import Log
    seen = defaultdict(set)
    for r in Log(start).rows():
        seen[str(r["day"])[:10]].add(r["cid"])
    return {d: len(s) for d, s in seen.items()}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--days", type=int, default=7)
    ap.add_argument("--countries", type=int, default=5)
    args = ap.parse_args()
    today = datetime.now(timezone.utc).date()
    start, end = (today - timedelta(days=args.days - 1)).isoformat(), (today + timedelta(days=1)).isoformat()

    tok = token()
    zones = call(tok, f"{API}/zones?name={ZONE}")["result"]
    if not zones:
        sys.exit(f"cf_traffic.py: this login cannot see {ZONE}")
    zone, account = zones[0]["id"], zones[0]["account"]["id"]
    j = call(tok, f"{API}/graphql", {"query": query(zone, account, start, end)})
    if j.get("errors"):
        sys.exit("cf_traffic.py: " + "; ".join(e["message"] for e in j["errors"]))
    edge = j["data"]["viewer"]["zones"][0]["httpRequests1dGroups"]
    acct = j["data"]["viewer"]["accounts"][0]
    logged = visitors(start)

    print(f"{ZONE}, UTC days {start} .. {today} (today is partial)\n")
    print(f"{'day':10} {'requests':>10} {'cached':>7} {'unique IPs':>10} {'4xx':>8} {'5xx':>7}"
          + (f" {'log visitors':>12}" if logged is not None else "") + "  top countries")
    for g in edge:
        s, day = g["sum"], g["dimensions"]["date"]
        status = Counter()
        for x in s["responseStatusMap"]:
            status[x["edgeResponseStatus"] // 100] += x["requests"]
        top = sorted(s["countryMap"], key=lambda x: -x["requests"])[:args.countries]
        print(f"{day:10} {s['requests']:>10} {100 * s['cachedRequests'] / max(1, s['requests']):6.1f}%"
              f" {g['uniq']['uniques']:>10} {status[4]:>8} {status[5]:>7}"
              + (f" {logged.get(day, 0):>12}" if logged is not None else "") + "  "
              + " ".join(f"{x['clientCountryName']} {x['requests']}" for x in top))

    print("\nworkers: requests (exceptions) per day")
    per = defaultdict(lambda: defaultdict(lambda: [0, 0]))
    for r in acct["workersInvocationsAdaptive"]:
        d = r["dimensions"]
        cell = per[d["scriptName"]][d["date"]]
        cell[0] += r["sum"]["requests"]
        cell[1] += r["sum"]["requests"] if d["status"] == "scriptThrewException" else 0
    days = [g["dimensions"]["date"] for g in edge] or sorted({d for s in per.values() for d in s})
    print(f"{'worker':16}" + "".join(f"{d[5:]:>20}" for d in days))
    for name in sorted(per, key=lambda n: -sum(c[0] for c in per[n].values())):
        print(f"{name:16}" + "".join(f"{f'{per[name][d][0]} ({per[name][d][1]})':>20}" for d in days))

    # CLOUDFLARE'S CRON DATASET LAGS, by days at times: a day missing past its
    # last run is the dataset's, not a cron that never fired.
    scheduled = acct["workersInvocationsScheduled"]
    last = max((r["datetime"] for r in scheduled), default="none")
    print(f"\ncron runs: status counts per day (the dataset's last run: {last})")
    runs = Counter((r["scriptName"], r["cron"], r["datetime"][:10], r["status"]) for r in scheduled)
    for (name, cron, day, status), n in sorted(runs.items()):
        print(f"  {day} {name:12} {cron:14} {status:22} {n}")


if __name__ == "__main__":
    main()
