#!/usr/bin/env python3
"""Survey which PHYSICAL riven stats each family rolls, from live listings.

No rule predicts a physical pool (docs/DATA_SOURCES.md §"Riven pools"), so the
evidence is the cards themselves. warframe.market's auction search filters over
EVERY listing when asked per stat, so each (family, stat, sign) is one query:

    python scripts/survey_riven_physical.py             # survey, resumable
    python scripts/survey_riven_physical.py --write     # also write data/rivens/physical.yaml
    python scripts/survey_riven_physical.py --only ocucor,boar

WHAT COUNTS AS EVIDENCE:
  - A POSITIVE physical stat is VERIFIED by the listing's name. DE builds the
    name from the positive stats' syllables, so a name that carries the stat's
    syllable is a card the game generated, not a seller's typo. It takes TWO,
    and a count above the noise floor: a card filed under the wrong weapon
    passes the name check (two Boltor listings carry Slash, against 0 of the
    maluses a rolling physical stat mostly appears as).
  - A NEGATIVE stat never reaches the name, and all three physical maluses share
    one base, so no single listing can verify one. Those are COUNTED against the
    family's own scale: the listings carrying Damage and Critical Chance, two
    stats every family rolls.

THE VERDICT IS THREE-WAY, and only two of them are written:
  rolls    a count at a rolling stat's rate, or two verified positives above
           the noise floor
  never    no verified positive, a count at the noise floor, and a market deep
           enough for absence to mean something
  (else)   nothing is written, and the stat stays UNCONFIRMED in the app

AN UNANSWERED QUERY IS `None`, NEVER 0 — a 0 reads exactly like "never rolls".
Answers are cached in vendor/riven-survey/ (gitignored), so an interrupted run
resumes where it stopped; `--fresh` asks again.
"""

import io
import json
import re
import sys
import time
import urllib.parse
import urllib.request
from itertools import permutations
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "data" / "rivens" / "physical.yaml"
CACHE = ROOT / "vendor" / "riven-survey" / "physical.json"
UA = {"User-Agent": "wfsim-data/1.0"}
WEAPONS_V2 = "https://api.warframe.market/v2/riven/weapons"
SEARCH = "https://api.warframe.market/v1/auctions/search?type=riven&weapon_url_name=%s"

PHYSICAL = {"impact": "impact_damage", "puncture": "puncture_damage", "slash": "slash_damage"}
# Two stats every riven family rolls, as the family's own scale.
REFERENCE = {"damage": "base_damage_/_melee_damage", "critical_chance": "critical_chance"}

# The market sustains about one request per six seconds; a burst earns 429s
# for minutes afterwards.
SLEEP = 6.0
# Fractions of the family's scale (the mean listings per reference stat).
ROLLS_ABOVE = 0.25
NEVER_AT_MOST = 0.03
# Below this scale an absence says nothing, and no `never` is written.
MIN_SCALE = 30
# Name-verified positives that prove a stat rolls; one can be a misfiled card.
VERIFIED_MIN = 2


def roster_families():
    """family -> (members, internal names), from every weapon file."""
    out = {}
    for p in sorted((ROOT / "data" / "weapons").rglob("*.yaml")):
        t = p.read_text(encoding="utf-8")
        fam = re.search(r"^riven_family: ([^#\n]+)", t, re.M)
        if not fam:
            continue
        name = re.search(r"^internal_name: ([^#\n]+)", t, re.M)
        e = out.setdefault(fam.group(1).strip().strip('"'), ([], set()))
        e[0].append(p.stem)
        if name:
            e[1].add(name.group(1).strip())
    return out


def syllables():
    """Our stat id -> (prefix, suffix), from every class pool."""
    out = {}
    for p in (ROOT / "data" / "rivens").glob("*.yaml"):
        cur = None
        for line in p.read_text(encoding="utf-8").splitlines():
            m = re.match(r"\s*- id: (\S+)", line)
            if m:
                cur = m.group(1)
                continue
            m = re.match(r"\s*(prefix|suffix): \"?([^\"\s#]+)", line)
            if m and cur:
                out.setdefault(cur, {})[m.group(1)] = m.group(2).lower()
    return {k: (v["prefix"], v["suffix"]) for k, v in out.items() if "prefix" in v and "suffix" in v}


MARKET_TO_OURS = {
    "impact_damage": "impact", "puncture_damage": "puncture", "slash_damage": "slash",
    "base_damage_/_melee_damage": "damage", "critical_chance": "critical_chance",
    "critical_damage": "critical_damage", "multishot": "multishot", "status_chance": "status_chance",
    "status_duration": "status_duration", "fire_rate_/_attack_speed": "fire_rate",
    "heat_damage": "heat", "cold_damage": "cold", "electric_damage": "electricity",
    "toxin_damage": "toxin", "magazine_capacity": "magazine_capacity", "reload_speed": "reload_speed",
    "ammo_maximum": "ammo_maximum", "punch_through": "punch_through", "projectile_speed": "projectile_speed",
    "recoil": "weapon_recoil", "zoom": "zoom", "damage_vs_corpus": "damage_to_corpus",
    "damage_vs_grineer": "damage_to_grineer", "damage_vs_infested": "damage_to_infested",
}


def name_carries(name, positives, stat, syl):
    """Does this listing's NAME account for `stat` among its positive stats?

    Two positives name as Prefix+suffix, three as Prefix-prefix+suffix, in an
    order set by the rolls, which a listing does not publish — so every order
    is tried, and the name must be fully accounted for, not merely contain the
    syllable.
    """
    ids = [MARKET_TO_OURS.get(p) for p in positives]
    if stat not in ids or any(i is None or i not in syl for i in ids):
        return False
    n = name.lower().replace(" ", "")
    for order in permutations(ids):
        pre = [syl[i][0] for i in order[:-1]]
        cand = "-".join(pre) + syl[order[-1]][1] if len(order) > 1 else syl[order[0]][0]
        if cand == n:
            return True
    return False


def get(url, tries=6):
    err = "?"
    for attempt in range(tries):
        try:
            with urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=30) as r:
                return json.load(r), None
        except Exception as e:  # 429 is routine; back off and ask again
            err = str(e)[:80]
            time.sleep(SLEEP * (2 + attempt))
    return None, err


def load_cache():
    try:
        return json.loads(CACHE.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def save_cache(c):
    CACHE.parent.mkdir(parents=True, exist_ok=True)
    tmp = CACHE.with_suffix(".tmp")
    tmp.write_text(json.dumps(c, indent=0, sort_keys=True), encoding="utf-8")
    tmp.replace(CACHE)


def query(cache, slug, attr, sign, fresh):
    """The listings for one (slug, attribute, sign): a list, or None if unanswered."""
    key = f"{slug}|{attr}|{sign}"
    if not fresh and cache.get(key) is not None:
        return cache[key]["auctions"], False
    url = SEARCH % slug + f"&{sign}_stats=" + urllib.parse.quote(attr, safe="")
    data, err = get(url)
    if data is None:
        print(f"    {key}: UNANSWERED ({err})")
        return None, True
    auctions = [
        {"name": a["item"].get("name", ""),
         "pos": [x["url_name"] for x in a["item"]["attributes"] if x["positive"]]}
        for a in data["payload"]["auctions"]
    ]
    cache[key] = {"asked": time.strftime("%Y-%m-%d"), "auctions": auctions}
    save_cache(cache)
    return auctions, True


def main():
    args = sys.argv[1:]
    write, fresh = "--write" in args, "--fresh" in args
    only = next((a.split("=", 1)[1] for a in args if a.startswith("--only=")), None)
    only = set(only.split(",")) if only else None

    fams = roster_families()
    market, err = get(WEAPONS_V2)
    if market is None:
        print(f"the weapon list did not answer: {err}")
        return 1
    by_ref = {w["gameRef"]: w["slug"] for w in market["data"]}
    syl = syllables()
    cache = load_cache()

    verdicts, unjoined, day = {}, [], time.strftime("%Y-%m-%d")
    for fam, (members, refs) in sorted(fams.items()):
        slugs = sorted({by_ref[r] for r in refs if r in by_ref})
        if not slugs:
            unjoined.append(fam)
            continue
        slug = slugs[0]
        if only and slug not in only:
            continue
        counts, missing = {}, False
        for stat, attr in list(PHYSICAL.items()) + list(REFERENCE.items()):
            for sign in ("positive", "negative"):
                got, asked = query(cache, slug, attr, sign, fresh)
                if asked:
                    time.sleep(SLEEP)
                if got is None:
                    missing = True
                counts[(stat, sign)] = got
        if missing:
            print(f"{fam:18} INCOMPLETE — re-run to resume")
            continue
        scale = sum(len(counts[(s, g)]) for s in REFERENCE for g in ("positive", "negative")) / len(REFERENCE)
        fv = {}
        for stat, attr in PHYSICAL.items():
            pos, neg = counts[(stat, "positive")], counts[(stat, "negative")]
            verified = sum(1 for a in pos if name_carries(a["name"], a["pos"], stat, syl))
            total = len(pos) + len(neg)
            note = (f"survey {day}: {len(pos)} positive ({verified} name-verified), "
                    f"{len(neg)} negative; family scale {scale:.0f}")
            noise = NEVER_AT_MOST * scale
            if total >= ROLLS_ABOVE * scale or (verified >= VERIFIED_MIN and total > noise):
                fv[stat] = ("rolls", note)
            elif scale >= MIN_SCALE and total <= noise:
                fv[stat] = ("never", note)
            else:
                fv[stat] = (None, note)
        verdicts[fam] = fv
        print(f"{fam:18} scale={scale:5.0f}  " + "  ".join(
            f"{s}:{(v or 'unclear')}" for s, (v, _) in fv.items()))

    if unjoined:
        print(f"\nno market entry joins by internal_name: {', '.join(unjoined)}")
    if not write:
        print("\n(re-run with --write to write data/rivens/physical.yaml)")
        return 0
    if only:
        print("\n--write needs the whole roster; drop --only")
        return 1

    hand = hand_written()
    out = [
        "# PHYSICAL STAT EVIDENCE PER RIVEN FAMILY — the only thing that refuses one.",
        "# Generated by scripts/survey_riven_physical.py; do not hand-edit. A player's",
        "# card goes in exceptions.yaml, and one (family, stat) may not appear in both.",
        "#",
        "# No rule predicts a physical pool (docs/DATA_SOURCES.md §\"Riven pools\"), so a",
        "# stat absent here for a family is UNCONFIRMED: offered, and marked.",
        "#",
        "#   rolls:  a count at a rolling stat's rate, or two name-verified positives above the noise",
        "#   never:  no verified listing and a count at the noise floor of a deep market",
        f"surveyed: \"{day}\"",
        "families:",
    ]
    for fam, fv in sorted(verdicts.items()):
        rows = {k: [(s, n) for s, (v, n) in fv.items() if v == k and (fam, s) not in hand]
                for k in ("rolls", "never")}
        if not rows["rolls"] and not rows["never"]:
            continue
        out.append(f"  - family: {fam}")
        for k in ("rolls", "never"):
            if rows[k]:
                out.append(f"    {k}:")
                for s, n in rows[k]:
                    out.append(f"      - stat: {s}")
                    out.append(f"        note: \"{n}\"")
    io.open(OUT, "w", encoding="utf-8", newline="\n").write("\n".join(out) + "\n")
    print(f"\nwrote {OUT.relative_to(ROOT)}")
    return 0


def hand_written():
    """(family, stat) pairs exceptions.yaml already answers — a card beats a count."""
    out, fam = set(), None
    for line in (ROOT / "data" / "rivens" / "exceptions.yaml").read_text(encoding="utf-8").splitlines():
        m = re.match(r"  - family: (.+)", line)
        if m:
            fam = m.group(1).strip()
            continue
        m = re.match(r"\s+- stat: (\S+)", line)
        if m and fam:
            out.add((fam, m.group(1)))
    return out


if __name__ == "__main__":
    raise SystemExit(main())
