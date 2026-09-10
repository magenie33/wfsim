#!/usr/bin/env python3
"""THE THANKS LIST, PUBLISHED AS NAMES AND NOTHING ELSE.

    python scripts/publish_thanks.py            # read D1, write site/thanks.json
    python scripts/publish_thanks.py --self-test

The ledger (`ledger/schema.sql`) is entered by hand and lives in D1; this is the
only thing that ever comes out of it, and what comes out is a list of names in
an order. NO AMOUNT LEAVES THIS SCRIPT — not a total, not a band, not a figure
per person. The order is derived from money and the money stays here.

THE FILE IS COMMITTED, like `site/board/`. Nothing at the edge reads the ledger,
so publishing is this script plus a commit, and the site has no way to ask.
"""

import argparse
import datetime as dt
import json
import math
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "site" / "thanks.json"

# THE ORDER: `total_cny * (1 + days_since_first) ** BETA`, largest first.
#
# TWO THINGS MATTER AND NEITHER MAY DROWN THE OTHER, so they are combined the
# way two goods are: a weighted product, which in log space is a weighted sum.
# Money carries exponent 1 and is therefore the unit; seniority carries BETA.
#
# BETA IS ONE SENTENCE, and this is it: TEN TIMES THE SENIORITY IS WORTH TWICE
# THE MONEY. That fixes it at log10(2), which is also why the multiplier table
# is round — a day is x1, ten days x2, a hundred x4, a thousand x8. Any other
# rule for reconciling "early" with "large" is an exchange rate somebody
# invented; this one is written down where it can be disagreed with.
SENIORITY_PER_DOUBLING = 10.0
BETA = math.log(2) / math.log(SENIORITY_PER_DOUBLING)

# THE LEDGER'S CLOCK IS BEIJING, and `paid_at` carries the offset to prove it
# (`ledger/schema.sql`). The day a score is computed for has to be read on the
# same clock, or a publish run near midnight UTC dates itself yesterday and
# every seniority is a day short.
BEIJING = dt.timezone(dt.timedelta(hours=8))

# One row per payment, joined to what it needs and nothing more. `net_amount` is
# what LANDED and `cny_per_unit` is today's rate: the books are in CNY and the
# ledger stores no historical rate, so a total is what the money is worth now.
QUERY = """
SELECT d.paid_at, d.channel, d.donor, d.donor_id, p.display_name,
       d.net_amount * r.cny_per_unit AS net_cny
  FROM donations d
  JOIN rates r ON r.currency = d.currency
  LEFT JOIN donors p ON p.id = d.donor_id
"""


def d1_rows(database: str) -> list[dict]:
    """Ask D1 for the ledger. Wrangler is the owner's own credential."""
    out = subprocess.run(
        ["npx", "wrangler", "d1", "execute", database, "--remote", "--json",
         "--command", " ".join(QUERY.split())],
        capture_output=True,
        # WITHOUT AN ENCODING, `text=True` DECODES AS THE CONSOLE'S CODEPAGE and
        # a wrangler error comes back as mojibake or as nothing at all.
        text=True, encoding="utf-8", errors="replace", shell=(sys.platform == "win32"),
    )
    if out.returncode != 0:
        sys.exit(f"wrangler refused the read:\n{out.stderr.strip() or out.stdout.strip()}")
    # Wrangler prints the json as a list of statement results.
    payload = json.loads(out.stdout[out.stdout.index("["):])
    return payload[0]["results"]


def entities(rows: list[dict], as_of: dt.date) -> list[dict]:
    """Fold payments into the people who made them, and score each.

    AN UNATTRIBUTED ACCOUNT IS ITS OWN ENTITY, so the list is complete before
    the linking is: a payment nobody has said belongs to a person still names
    somebody, and dropping it would thank fewer people than paid.
    """
    by_key: dict[str, dict] = {}
    for r in rows:
        key = f"#{r['donor_id']}" if r.get("donor_id") is not None \
            else f"{r['channel']}:{r['donor']}"
        day = dt.date.fromisoformat(r["paid_at"][:10])
        e = by_key.setdefault(key, {"name": None, "handle": r["donor"], "cny": 0.0, "first": day})
        e["cny"] += float(r["net_cny"])
        e["first"] = min(e["first"], day)
        # The person's chosen name wins wherever it is set; an account with none
        # is thanked under the handle that paid.
        if r.get("display_name"):
            e["name"] = r["display_name"]

    out = []
    for e in by_key.values():
        # A REFUND IS A NEGATIVE ROW, so a person can net to nothing — and a
        # negative raised to a fractional power is not a number. Nobody who has
        # given nothing on balance is on a list of people who gave.
        if e["cny"] <= 0:
            continue
        days = (as_of - e["first"]).days
        # `1 + days`, NEVER `days`: somebody who gave today is 0 days old and
        # `0 ** BETA` is 0, which drops them off the list on the one day they
        # would most notice.
        out.append({
            "name": e["name"] or e["handle"],
            "since": e["first"].strftime("%Y-%m"),
            "score": e["cny"] * (1 + max(days, 0)) ** BETA,
        })
    # Score decides; the name breaks a tie so two publishes of one ledger agree.
    out.sort(key=lambda e: (-e["score"], e["name"]))
    return out


def publish(rows: list[dict], as_of: dt.date) -> dict:
    ranked = entities(rows, as_of)
    return {
        "as_of": as_of.isoformat(),
        # NAME AND MONTH, and the score is dropped on the way out. It ordered
        # the list and it is a statement about somebody's money; the page needs
        # the order, not the number that produced it.
        "supporters": [{"name": e["name"], "since": e["since"]} for e in ranked],
    }


def self_test() -> int:
    ok = bad = 0

    def say(good, what, detail=""):
        nonlocal ok, bad
        ok, bad = (ok + 1, bad) if good else (ok, bad + 1)
        print(f"  {'ok  ' if good else 'FAIL'}  {what}{'' if good or not detail else f'   {detail}'}")

    today = dt.date(2026, 9, 10)

    def row(donor, cny, day, donor_id=None, name=None, channel="kofi"):
        return {"paid_at": f"{day}T12:00:00+08:00", "channel": channel, "donor": donor,
                "donor_id": donor_id, "display_name": name, "net_cny": cny}

    names = lambda rows: [e["name"] for e in entities(rows, today)]

    say(abs(BETA - 0.30103) < 1e-5, "ten times the seniority is twice the money", f"beta={BETA}")
    mult = lambda days: (1 + days) ** BETA
    say(all(abs(mult(d) - m) < 0.01 for d, m in [(0, 1), (9, 2), (99, 4), (999, 8)]),
        "...so the multiplier table is round: 1 day x1, 10 x2, 100 x4, 1000 x8",
        f"{[round(mult(d), 3) for d in (0, 9, 99, 999)]}")

    # EQUAL MONEY: THE EARLIER PERSON WINS, and that is the property that makes
    # this list safe to publish — nobody drops down the page by doing nothing.
    say(names([row("early", 100, "2025-09-10"), row("late", 100, "2026-09-01")])
        == ["early", "late"], "equal money, the earlier supporter is ahead")

    # …AND MONEY IS THE STRONGER OF THE TWO, which is the whole point of the
    # exponents. Ten times the seniority is only twice the money, so 5x the
    # money at three days beats a year of it.
    say(names([row("old", 200, "2025-09-10"), row("big", 1000, "2026-09-07")])
        == ["big", "old"], "...but five times the money beats a year of seniority")

    say(names([row("today", 5, "2026-09-10")]) == ["today"],
        "somebody who gave today is on the list (0 ** beta would drop them)")

    # TWO ACCOUNTS, ONE PERSON: one entry, one total, one seniority — the
    # earliest of the two, because that is when this person first chipped in.
    merged = entities([row("bili", 50, "2025-09-10", donor_id=1, name="Lucas", channel="bilibili"),
                       row("kofi", 50, "2026-09-01", donor_id=1, name="Lucas")], today)
    say(len(merged) == 1 and merged[0]["since"] == "2025-09",
        "two accounts of one donor are one entry, dated from the first",
        json.dumps(merged, default=str))

    say(names([row("refunded", 30, "2026-01-01"), row("refunded", -30, "2026-01-02")]) == [],
        "a payment that was refunded in full thanks nobody")

    # THE UNATTRIBUTED ARE STILL THANKED. A batch entered before anybody said
    # who it was is a real supporter with a real name on their account.
    say(names([row("nobody-linked-me", 40, "2026-01-01")]) == ["nobody-linked-me"],
        "an account with no donor_id is thanked under its own handle")

    # AND NO AMOUNT LEAVES. The published shape is asserted, not described:
    # every key of every entry, so a field added later has to be decided on.
    pub = publish([row("a", 10, "2026-01-01"), row("b", 20, "2026-02-01")], today)
    say(all(set(e) == {"name", "since"} for e in pub["supporters"]),
        "the published entry is a name and a month, and carries no figure",
        json.dumps(pub))

    print(f"\n{ok} ok, {bad} failed")
    return 0 if bad == 0 else 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--database", default="wfsim", help="the D1 database to read")
    ap.add_argument("--from-json", type=pathlib.Path,
                    help="read the rows from a file instead of D1")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()

    rows = json.loads(args.from_json.read_text("utf-8")) if args.from_json \
        else d1_rows(args.database)
    doc = publish(rows, dt.datetime.now(BEIJING).date())
    OUT.write_text(json.dumps(doc, ensure_ascii=False, indent=1) + "\n",
                   encoding="utf-8", newline="\n")
    print(f"thanks: {len(doc['supporters'])} supporter(s) -> {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
