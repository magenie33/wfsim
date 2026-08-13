#!/usr/bin/env python3
"""Survey which riven stats a weapon family ACTUALLY rolls, from live listings.

The wiki states one rule and admits it is not a law: "Weapons without more
than 25% of a physical damage type usually cannot roll that respective
attribute... Exceptions exist on a case by case basis." `excluded_for` in
engine/src/rivens_data.rs derives what it can from the weapon, and it lands on
the right answer for most of the roster — but it is a heuristic, and the
exceptions are exactly the cases a player notices, because what a wrong answer
does is refuse a stat their real card carries.

So ASK THE CARDS. warframe.market's auction search returns live riven
listings, each with the stats it rolled.

    python scripts/survey_riven_pools.py            # report only
    python scripts/survey_riven_pools.py --write    # write data/rivens/pools.yaml

ONE QUERY PER STAT, NOT ONE PER FAMILY. The first version of this script
pulled a family's listings in a single call and counted the stats it saw, and
that answer was WRONG in a way worth writing down: the endpoint caps at 500
rows and orders them, so what came back was the 500 CHEAPEST listings of a
family that may have thousands, which is not a sample of anything. It reported
2 Boar cards with Projectile Speed. Asking the server directly — `Boar rivens
whose positive stat is projectile_speed`, then the negative — finds 31 and 50,
because the filter runs over every listing instead of over the cheap tail.
Same weapon, same day, 40x apart (owner, 2026-08-08).

THE VERDICT IS THREE-WAY. Counts are compared to the family's own MEDIAN stat
count rather than to a fixed number, because market depth varies twenty-fold
between a Boltor and a Bronco: most of a class's 24 stats do roll on any given
weapon, so the median IS roughly what a rolling stat looks like there.

  - well above the family's median rate -> ROLLABLE
  - essentially absent -> NEVER
  - in between -> UNCLEAR, and the engine keeps its own derivation

The middle band is not squeamishness. Listings are typed by players and some
are simply wrong, so a handful of cards is neither proof that a stat rolls nor
proof that it cannot. Saying so is cheaper than guessing.
"""

import io
import json
import os
import re
import sys
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "data" / "rivens" / "pools.yaml"
API = "https://api.warframe.market/v1/auctions/search?type=riven&weapon_url_name=%s"

# The market's attribute slug -> our stat id (data/rivens/<class>.yaml).
STAT = {
    "ammo_maximum": "ammo_maximum",
    "base_damage_/_melee_damage": "damage",
    "cold_damage": "cold",
    "critical_chance": "critical_chance",
    "critical_damage": "critical_damage",
    "damage_vs_corpus": "damage_to_corpus",
    "damage_vs_grineer": "damage_to_grineer",
    "damage_vs_infested": "damage_to_infested",
    "electric_damage": "electricity",
    "fire_rate_/_attack_speed": "fire_rate",
    "heat_damage": "heat",
    "impact_damage": "impact",
    "magazine_capacity": "magazine_capacity",
    "multishot": "multishot",
    "projectile_speed": "projectile_speed",
    "punch_through": "punch_through",
    "puncture_damage": "puncture",
    "recoil": "weapon_recoil",
    "reload_speed": "reload_speed",
    "slash_damage": "slash",
    "status_chance": "status_chance",
    "status_duration": "status_duration",
    "toxin_damage": "toxin",
    "zoom": "zoom",
}
# Five stats are bonus-only in game, so asking for them as a negative is a
# query that can only ever return nothing.
BONUS_ONLY = {"heat", "cold", "electricity", "toxin", "punch_through"}

# Fractions of the family's own MEDIAN stat count.
ROLLS_ABOVE = 0.25
NEVER_BELOW = 0.05
SLEEP = 1.6


def families():
    """Every riven family in the roster, and the weapons that carry it."""
    out = {}
    for slot in ("primary", "secondary"):
        d = ROOT / "data" / "weapons" / slot
        for p in sorted(d.glob("*.yaml")):
            t = p.read_text(encoding="utf-8")
            m = re.search(r"^riven_family: ([^#\n]+)", t, re.M)
            if m:
                out.setdefault(m.group(1).strip(), []).append(p.stem)
    return out


def count(slug, extra="", tries=6):
    """How many live listings match. `None` = the API never answered."""
    err = "?"
    for attempt in range(tries):
        try:
            with urllib.request.urlopen(API % slug + extra, timeout=30) as r:
                return len(json.load(r)["payload"]["auctions"]), None
        except Exception as e:  # 429 is routine: the API is rate limited
            err = str(e)[:60]
            time.sleep(6 + 4 * attempt)
    return None, err


def main():
    write = "--write" in sys.argv
    fams = families()
    rows = []
    for fam, ws in sorted(fams.items()):
        slug = fam.lower().replace(" ", "_").replace("-", "_")
        n, err = count(slug)
        if n is None:
            print("%-14s ERROR %s" % (fam, err))
            rows.append((fam, ws, None, None, None))
            continue
        time.sleep(SLEEP)
        c = {}
        for stat in sorted(set(STAT.values())):
            pos, err = count(slug, "&positive_stats=%s" % stat)
            time.sleep(SLEEP)
            neg = 0
            if pos is not None and stat not in BONUS_ONLY:
                neg, err2 = count(slug, "&negative_stats=%s" % stat)
                time.sleep(SLEEP)
                neg = neg or 0
            c[stat] = (pos or 0) + neg
        # The family's own scale. Most of a class's stats roll on any given
        # weapon, so the median stat count is what "this stat rolls" looks
        # like HERE — which is what makes a 309-listing family comparable to a
        # 5000-listing one.
        med = sorted(c.values())[len(c) // 2]
        rollable = sorted(k for k, v in c.items() if v > med * ROLLS_ABOVE)
        never = sorted(k for k, v in c.items() if v <= med * NEVER_BELOW)
        unclear = sorted(set(c) - set(rollable) - set(never))
        rows.append((fam, ws, n, (rollable, never, unclear), c))
        print("%-14s listings=%-5d median=%-4d never=%-38s unclear=%s"
              % (fam, n, med, ",".join(never) or "-", ",".join(unclear) or "-"))

    if not write:
        print("\n(re-run with --write)")
        return 0

    today = os.environ.get("SURVEY_DATE", time.strftime("%Y-%m-%d"))
    out = [
        "# SURVEYED FROM LIVE RIVEN LISTINGS by scripts/survey_riven_pools.py.",
        "# Do not hand-edit: re-run the script.",
        "#",
        "# What a weapon's riven CAN roll is not published anywhere and is not",
        "# reliably derivable — the wiki's 25%-of-a-physical-type rule says",
        "# \"usually\" and \"exceptions exist on a case by case basis\". So this",
        "# file is EVIDENCE rather than a formula: every number below is a count",
        "# of live listings carrying that stat, positive and negative together,",
        "# asked of the server one stat at a time.",
        "#",
        "# `never` = essentially absent. `rollable` = it appears at the rate a",
        "# rolling stat does on this family. Anything in neither is UNCLEAR and",
        "# the engine keeps its own derivation for it (engine/src/rivens_data.rs",
        "# `excluded_for`).",
        "#",
        "# A count is a sample, not a proof: absence is strong evidence and not a",
        "# guarantee. An in-game card that contradicts a `never` here beats the",
        "# file. NOTHING IN THE ENGINE READS THIS: it is checked against the",
        "# rules by `the_survey_still_agrees_with_the_rules`, and a disagreement",
        "# is promoted BY HAND into data/rivens/exceptions.yaml with its count.",
        f"surveyed: \"{today}\"",
        "source: warframe.market public auction search (type=riven), one filtered query per stat",
        "families:",
    ]
    for fam, ws, n, verdict, c in rows:
        if verdict is None:
            out.append(f"  # {fam}: NOT SURVEYED (the API refused); the engine derives it.")
            continue
        rollable, never, unclear = verdict
        counts = ", ".join(f"{k} {v}" for k, v in sorted(c.items()))
        out.append(f"  - family: {fam}")
        out.append(f"    weapons: [{', '.join(ws)}]")
        out.append(f"    n: {n}")
        out.append(f"    # listings carrying each stat: {counts}")
        out.append(f"    rollable: [{', '.join(rollable)}]")
        out.append(f"    never: [{', '.join(never)}]")
        if unclear:
            out.append(f"    unclear: [{', '.join(unclear)}]")
    io.open(OUT, "w", encoding="utf-8", newline="\n").write("\n".join(out) + "\n")
    print(f"\nwrote {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
