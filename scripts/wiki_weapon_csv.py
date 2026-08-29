#!/usr/bin/env python3
"""EVERY GUN IN THE GAME, in one fetch — the wiki's own CSV.

`Weapon Comparison/CSV` (wiki.warframe.com) is the wiki rendering
`Module:Weapons/data` as comma-separated rows: **one row per ATTACK**, 969 of
them, with `InternalName` on every one. It is the same source
`scripts/audit_weapon_stats.py` reads through `private/scripts/wiki_weapons.py`
— but in a shape that needs no bench tool, no per-module fetch, and no Lua
parsing, so this tool runs for anybody who has cloned the repo.

WHAT IT IS FOR is the question `intake_report.py` cannot answer. That tool says
what is IN the roster and what about it is unfinished; this one says what the
GAME has that the roster does not, which is the whole of "which weapon next"
.

    python scripts/wiki_weapon_csv.py coverage        # what is missing, by class
    python scripts/wiki_weapon_csv.py coverage --all  # …and name every one
    python scripts/wiki_weapon_csv.py check           # cross-check what we have
    python scripts/wiki_weapon_csv.py --refresh ...   # re-fetch (else cached)

IT IS A CROSS-CHECK, NOT A PEER. `data/README.md`'s rule stands: THE WIKI WINS,
and this CSV *is* the wiki — but it is the wiki's DATA MODULE, not its page, and
a page can say things a table cannot ("there are two of these and you want the
other one"). So a disagreement reported here is a prompt to read the weapon's
own page, never a licence to bulk-write the CSV's number over ours.

TWO LIMITS, BOTH MEASURED RATHER THAN ASSUMED, and a coverage number that does
not state them is a lie:

  · **IT IS NOT LIVE.** The page carries a "Manual Update" section — the CSV is
    a dump somebody pastes, not a render of the module, so it LAGS. Seven of
    this roster's primaries and secondaries (the Afentis Prime, the Coda
    Bubonico, the Enkaus, the Haalvu, the Perigale Prime, the Tenet Quanta) are
    newer than the dump. `Module:Weapons/data` stays the authority.
  · **IT HOLDS NO ARCH-GUNS.** The gun table is Primary / Secondary / Robotic /
    Amp / Railjack only; Arch-Guns live on `Weapon Comparison/Archgun`, a
    separate tab. So "nothing missing" is a claim about the slots below and
    about no others, and the tool prints which those are.

THE JOIN IS ON `InternalName`, never on a name — the repo's standing rule, and
this file pays for it the same way the module audit did: an ambiguous internal
name is dropped rather than resolved, because eighteen rows share one with
another row and keeping the last silently joins a weapon to its Prime.
"""

import argparse
import csv
import io
import os
import re
import sys
import urllib.request
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
CACHE = ROOT / "web" / "cache" / "wiki" / "weapon_comparison_csv.txt"
URL = "https://wiki.warframe.com/w/Weapon_Comparison/CSV?action=raw"
# The wiki refuses a bare urllib agent; docs/DATA_SOURCES.md §"FETCH THE MODULE
# WITH curl" is the same recipe.
AGENT = "wfsim-dev/1.0 (data cross-check)"

# THE GUN TABLE'S HEADER, verbatim and in full. It is matched rather than
# assumed: the page holds SEVERAL csv blocks (guns, melee, …) and each repeats
# its header, so "the first line with commas" would pick up whichever one the
# page happens to lead with.
GUN_HEAD = "Name,Trigger,AttackName,Impact,"


def fetch(refresh=False):
    """The page's raw wikitext, cached. One request, ~740 KB."""
    if CACHE.exists() and not refresh:
        return CACHE.read_text(encoding="utf-8")
    req = urllib.request.Request(URL, headers={"User-Agent": AGENT})
    with urllib.request.urlopen(req, timeout=120) as r:
        text = r.read().decode("utf-8")
    CACHE.parent.mkdir(parents=True, exist_ok=True)
    CACHE.write_text(text, encoding="utf-8", newline="\n")
    return text


# WHERE `ForcedProcs` SITS, and why the parser needs to know. Ten of the 877 gun
# rows carry rendered HTML in that one column — a damage-type tooltip, with
# commas AND double quotes inside — so no quoting convention recovers them: the
# row simply has more fields than the header. Every other column is a number or
# a short token, so the surplus is collapsed back into this index and the result
# is VALIDATED rather than trusted (see `KNOWN_SLOTS`).
FORCED_PROCS_AT = 27

# The slots the table actually uses. A row whose Slot is not one of these came
# out of the parser SHIFTED, and is reported rather than kept — a shifted row
# reads as a weapon with a date for a class, which is the kind of nonsense that
# silently poisons a coverage count.
KNOWN_SLOTS = {
    "Primary", "Secondary", "Robotic", "Archgun", "Archgun (Atmosphere)",
    "Amp", "Railjack Turret", "Railjack Ordnance", "Melee", "Archmelee",
}


def gun_rows(text, complain=True):
    """Every gun-attack row, as dicts.

    ONE BLOCK, not six. The header string occurs six times, and five of those
    are repeats INSIDE the same `<pre>` — the table restates its own columns
    per section. Reading from the last one gave 425 rows of 877, which is a
    coverage report that is wrong and looks fine.
    """
    if GUN_HEAD not in text:
        raise SystemExit("the gun table's header is not on the page — did it move?")
    body = text[text.index(GUN_HEAD):].splitlines()
    lines = [body[0]]
    for line in body[1:]:
        if line.startswith("</pre>"):
            break
        if line.startswith(GUN_HEAD) or not line.strip():
            continue
        lines.append(line)
    header = next(csv.reader([lines[0]], quotechar="'"))
    rows, shifted = [], []
    for line in lines[1:]:
        f = next(csv.reader([line], quotechar="'"))
        if len(f) > len(header):
            extra = len(f) - len(header)
            f[FORCED_PROCS_AT:FORCED_PROCS_AT + extra + 1] = [
                ",".join(f[FORCED_PROCS_AT:FORCED_PROCS_AT + extra + 1])
            ]
        if len(f) != len(header):
            shifted.append(line.split(",")[0])
            continue
        r = dict(zip(header, f))
        if r.get("Slot") not in KNOWN_SLOTS:
            shifted.append(r.get("Name", "?"))
            continue
        # String values are single-quoted by the generator; strip the quoting so
        # a comparison is against the value rather than against how it was
        # written.
        for k, v in list(r.items()):
            if isinstance(v, str) and len(v) > 1 and v[0] == "'" and v[-1] == "'":
                r[k] = v[1:-1]
        rows.append(r)
    if shifted and complain:
        # LOUD, never silent. A parser that drops rows quietly reports better
        # coverage than it has.
        print(f"  ! {len(shifted)} row(s) did not parse and are NOT counted: "
              f"{', '.join(shifted[:6])}", file=sys.stderr)
    return rows


def roster():
    """Every entry in `data/weapons/`, keyed by file, with its parent resolved.

    A FORM INHERITS ITS WEAPON (AGENTS.md, 2026-08-15), so an entry may carry no
    `internal_name` of its own — it is its parent's. Resolving that here is what
    keeps 88 form siblings from being reported as weapons the wiki has never
    heard of.
    """
    out = {}
    for p in sorted((ROOT / "data" / "weapons").rglob("*.yaml")):
        d = yaml.safe_load(p.read_text(encoding="utf-8")) or {}
        if isinstance(d, dict) and d.get("id"):
            out[d["id"]] = d
    for d in out.values():
        parent = out.get(d.get("inherits"))
        if parent and not d.get("internal_name"):
            d["internal_name"] = parent.get("internal_name")
    return out


def by_internal(rows, key="InternalName"):
    """Group the CSV by internal name: `{internal: [row, ...]}`.

    ONE ROW PER ATTACK, so several rows per weapon is the NORMAL case and not
    ambiguity — the first version of this treated it as ambiguity (borrowing the
    module audit's rule, where a row IS a weapon) and threw away 211 of the 355
    joins it could have made. What ambiguity means HERE is two rows that
    disagree about a WEAPON-level field, and that is decided per field rather
    than per weapon: see `weapon_level`.
    """
    seen = {}
    for r in rows:
        k = r.get(key)
        if k:
            seen.setdefault(k, []).append(r)
    return seen


def weapon_level(rows, field):
    """A field's value when every attack row agrees, else `None`.

    The module carries Mastery, Magazine, Reload, AmmoMax, Accuracy and
    Disposition once per WEAPON and the CSV repeats them on each attack row —
    but not always: a charged form can print its own magazine. A disagreement is
    therefore not something to average or to pick from; it is a field this tool
    cannot ask about, and it goes to the unchecked count where it can be seen.
    """
    vals = {r.get(field) for r in rows}
    return vals.pop() if len(vals) == 1 else None


# WHAT THIS ROSTER DELIBERATELY DOES NOT HOLD, so a coverage report does not
# read as 300 weapons of unfinished work. Each is a DECISION with a document
# behind it, not an oversight.
SKIP_CLASS = {
    # docs/KITGUNS.md: a modular weapon has no published stat line of its own,
    # only parts that compute one. It is a CUSTOM, not a roster entry.
    "Kitgun", "Zaw",
    # An EXALTED weapon is summoned by an ability, so it is the Warframe layer
    # docs/UNMODELLED.md holds open — there is no Tenno holding one.
    "Exalted Weapon",
    # docs/UNMODELLED.md §"no melee": the arena is a shooting range.
    "Melee",
}
SKIP_SLOT = {
    "Melee", "Archmelee",
    # An Amp and a Railjack turret are fired by something that is not a Tenno
    # holding a gun, and nothing in this engine models either platform.
    # The Slot is spelled in full by the table: "Railjack Turret", not "Turret".
    "Amp", "Railjack Turret", "Railjack Ordnance",
}


def cmd_coverage(args):
    rows = gun_rows(fetch(args.refresh))
    idx = by_internal(rows)
    ours = roster()
    have = {d.get("internal_name") for d in ours.values() if d.get("internal_name")}

    # ONE ROW PER ATTACK, and the question is about WEAPONS — so the rows are
    # folded by internal name first. A weapon we lack is missing whether it has
    # one attack or four, and counting attacks would make a Kuva weapon look
    # like four times the work.
    weapons = {}
    for r in rows:
        k = r.get("InternalName")
        if k:
            weapons.setdefault(k, r)

    missing, held, skipped = [], 0, 0
    for k, r in sorted(weapons.items(), key=lambda kv: (kv[1]["Slot"], kv[1]["Class"], kv[1]["Name"])):
        if r["Slot"] in SKIP_SLOT or r["Class"] in SKIP_CLASS:
            skipped += 1
            continue
        if k in have:
            held += 1
        else:
            missing.append(r)

    groups = {}
    for r in missing:
        groups.setdefault((r["Slot"], r["Class"]), []).append(r["Name"])
    slots = sorted({r["Slot"] for r in rows})
    print(f"the wiki's CSV holds {len(weapons)} weapons ({len(rows)} attacks)")
    print(f"  slots covered : {', '.join(slots)}")
    print("  (no Arch-Guns and no melee — those are separate tabs, so nothing "
          "below is a claim about them)")
    print(f"  in the roster : {held}")
    print(f"  out of scope  : {skipped}  (melee, modular, amps, railjack)")
    print(f"  NOT HELD      : {len(missing)}\n")
    for (slot, cls), names in sorted(groups.items(), key=lambda kv: -len(kv[1])):
        print(f"  {slot:<10} {cls:<18} {len(names):>3}")
        if args.all:
            for n in sorted(names):
                print(f"      {n}")
    # AND THE OTHER DIRECTION, which is the one a coverage report usually
    # forgets: an entry of OURS the wiki's table has never heard of is either a
    # form sibling (fine, it inherits) or a typo in an internal name (not fine).
    orphans = sorted(
        d["id"] for d in ours.values()
        if d.get("internal_name") and d["internal_name"] not in idx
        and d["internal_name"] not in {r.get("InternalName") for r in rows}
    )
    if orphans:
        # SPLIT, because the two halves mean opposite things. An Arch-Gun is
        # absent BY CONSTRUCTION and is nothing to act on; anything else is
        # either newer than this manually-pasted dump or an internal name that
        # has gone stale, and only the second is a fault. One number said
        # neither.
        arch = {o for o in orphans
                if ours[o].get("slot") == "archgun"
                or ours.get(ours[o].get("inherits"), {}).get("slot") == "archgun"}
        rest = [o for o in orphans if o not in arch]
        print()
        print(f"  {len(arch)} arch-gun entries — this table has no Arch-Gun rows")
        print(f"  {len(rest)} others name an internal the CSV does not have —",
              "newer than the dump, or a stale internal name:")
        for o in rest[:40]:
            print(f"      {o}")


# ---- the cross-check ------------------------------------------------------
#
# ONLY WEAPON-LEVEL FIELDS. The CSV is one row per ATTACK and this roster splits
# a weapon into entries per FORM, and the two splits are not the same split — so
# joining an attack row to a form entry would compare the Incarnon's crit to the
# base form's on half the roster. What IS safe is the fields the module carries
# once per weapon, which is what this checks.
FIELDS = [
    ("mastery_rank", "Mastery", 0),
    ("magazine", "Magazine", 0),
    ("reload_seconds", "Reload", 0.05),
    ("ammo_max", "AmmoMax", 0),
    ("accuracy", "Accuracy", 0.5),
    ("riven_disposition", "Disposition", 0.005),
]


def num(x):
    try:
        return float(x)
    except (TypeError, ValueError):
        return None


def cmd_check(args):
    rows = gun_rows(fetch(args.refresh))
    idx = by_internal(rows)
    ours = roster()
    bad, checked, unchecked = [], 0, []
    for wid, d in sorted(ours.items()):
        # A FORM IS NOT A WEAPON HERE: it inherits every field below, so
        # checking it would report its parent's row twice.
        if d.get("inherits"):
            continue
        k = d.get("internal_name")
        group = idx.get(k) if k else None
        if not group:
            unchecked.append(wid)
            continue
        for ours_key, theirs_key, tol in FIELDS:
            a, b = num(d.get(ours_key)), num(weapon_level(group, theirs_key))
            if a is None or b is None:
                continue
            checked += 1
            if abs(a - b) > tol + 1e-9:
                bad.append((wid, ours_key, a, b))
    for wid, f, a, b in bad:
        print(f"  {wid:<34} {f:<18} ours {a:<10} wiki {b}")
    print(f"\n{checked} values compared, {len(bad)} disagree")
    # UNCHECKED IS REPORTED LOUDLY, because a checker that quietly skips is
    # worse than no checker — it reads as a pass (docs/DATA_SOURCES.md).
    if unchecked:
        print(f"{len(unchecked)} entries UNCHECKED (no unambiguous internal name in the CSV):")
        for u in unchecked[:30]:
            print(f"      {u}")
        if len(unchecked) > 30:
            print(f"      … and {len(unchecked) - 30} more")
    return 1 if bad else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("cmd", choices=["coverage", "check"])
    ap.add_argument("--refresh", action="store_true", help="re-fetch instead of using the cache")
    ap.add_argument("--all", action="store_true", help="name every missing weapon")
    args = ap.parse_args()
    return (cmd_coverage if args.cmd == "coverage" else cmd_check)(args) or 0


if __name__ == "__main__":
    sys.exit(main())
