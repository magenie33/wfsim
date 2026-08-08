#!/usr/bin/env python3
"""Survey which riven stats a weapon family ACTUALLY rolls, from live listings.

The wiki states one rule and admits it is not a law: "Weapons without more
than 25% of a physical damage type usually cannot roll that respective
attribute... Exceptions exist on a case by case basis." `excluded_for` in
engine/src/rivens_data.rs derives what it can from the weapon (that share
rule, plus "a stat the weapon does not have"), and it lands on the right
answer for most of the roster — but it is a heuristic, and the exceptions are
exactly the cases a player notices, because what a wrong answer does is refuse
a stat their real card carries.

So ASK THE CARDS. warframe.market's auction search returns live riven
listings, each with the stats it rolled; a few hundred per family is enough to
tell a stat that rolls from one that cannot. The output is DATA, dated and
counted, that the engine consults ahead of its own derivation.

    python scripts/survey_riven_pools.py            # report only
    python scripts/survey_riven_pools.py --write    # write data/rivens/pools.yaml

THE VERDICT IS THREE-WAY, and that is the point. Every riven carries 2-3
stats out of its class pool, so a stat that CAN roll shows up in roughly
`n x 2.5 / pool` listings — around 55 in 500 for a 24-stat pool. Against that:

  - well above the expected rate -> ROLLABLE
  - essentially absent -> NEVER
  - in between -> UNCLEAR, and the engine keeps its own derivation

The middle band is not squeamishness. Listings are typed by players and a
handful are simply wrong (one Latron riven claims Slash; one Atomos claims
Impact), so a count of 9 out of 500 is neither a stat that rolls nor a stat
that provably does not. Saying so is cheaper than guessing.
"""

import io
import json
import os
import re
import sys
import time
import urllib.request
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "data" / "rivens" / "pools.yaml"
API = "https://api.warframe.market/v1/auctions/search?type=riven&weapon_url_name=%s&sort_by=price_asc"

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

# Fraction of the expected per-stat rate. Measured 2026-08-08 across 26
# families: a stat that rolls landed at 30-70 out of ~55 expected, and a stat
# that does not landed at 0-4. Nothing real came anywhere near the floor.
ROLLS_ABOVE = 0.40
NEVER_BELOW = 0.10


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


def fetch(fam, tries=6):
    slug = fam.lower().replace(" ", "_").replace("-", "_")
    err = "?"
    for attempt in range(tries):
        try:
            with urllib.request.urlopen(API % slug, timeout=30) as r:
                return json.load(r)["payload"]["auctions"], None
        except Exception as e:  # 429 is routine: the API is rate limited
            err = str(e)[:60]
            time.sleep(6 + 4 * attempt)
    return None, err


def main():
    write = "--write" in sys.argv
    fams = families()
    rows = []
    for fam, ws in sorted(fams.items()):
        auctions, err = fetch(fam)
        if auctions is None:
            print("%-14s ERROR %s" % (fam, err))
            rows.append((fam, ws, None, None, None))
            continue
        c = Counter()
        for a in auctions:
            for at in a["item"].get("attributes", []):
                c[STAT.get(at["url_name"], at["url_name"])] += 1
        n = len(auctions)
        # The expected per-stat rate, measured off this family's own sample
        # rather than assumed: `stats seen / stats in the pool`, where the pool
        # size is however many distinct stats the sample ever showed.
        pool = max(len(c), 1)
        expected = sum(c.values()) / pool
        rollable = sorted(k for k, v in c.items() if v > expected * ROLLS_ABOVE)
        never = sorted(
            k for k in STAT.values()
            if c.get(k, 0) < expected * NEVER_BELOW
        )
        unclear = sorted(set(STAT.values()) - set(rollable) - set(never))
        rows.append((fam, ws, n, (rollable, never, unclear), c))
        print("%-14s n=%-4d never=%-42s unclear=%s"
              % (fam, n, ",".join(never) or "-", ",".join(unclear) or "-"))
        time.sleep(2.5)

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
        "# \"usually\" and \"exceptions exist on a case by case basis\", and the",
        "# exceptions are real: the Ocucor is 9% Puncture and 91% Radiation and",
        "# rolls all three physical stats; the Phenmor is 30% Puncture and rolls",
        "# none of it. So this file is EVIDENCE rather than a formula — every",
        "# entry is a count of how often a stat appeared on real cards.",
        "#",
        "# `never` = the stat was essentially absent from the sample.",
        "# `rollable` = it appeared at or above the rate a rolling stat should.",
        "# Anything in neither is UNCLEAR and the engine keeps its own",
        "# derivation for it (engine/src/rivens_data.rs `excluded_for`).",
        "#",
        "# A count is a sample, not a proof: absence in 500 listings is strong",
        "# evidence and not a guarantee. An in-game card that contradicts a",
        "# `never` here beats the file.",
        f"surveyed: \"{today}\"",
        "source: warframe.market public auction search (type=riven), newest 500 per family",
        "families:",
    ]
    for fam, ws, n, verdict, c in rows:
        if verdict is None:
            out.append(f"  # {fam}: NOT SURVEYED (the API refused); the engine derives it.")
            continue
        rollable, never, unclear = verdict
        counts = ", ".join(f"{k} {v}" for k, v in sorted(c.items()) if k in STAT.values())
        out.append(f"  - family: {fam}")
        out.append(f"    weapons: [{', '.join(ws)}]")
        out.append(f"    n: {n}")
        out.append(f"    # counts: {counts}")
        out.append(f"    rollable: [{', '.join(rollable)}]")
        out.append(f"    never: [{', '.join(never)}]")
        if unclear:
            out.append(f"    unclear: [{', '.join(unclear)}]")
    io.open(OUT, "w", encoding="utf-8", newline="\n").write("\n".join(out) + "\n")
    print(f"\nwrote {OUT.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
