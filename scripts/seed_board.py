#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""A build for every weapon nobody has submitted one for.

   python scripts/seed_board.py --check http://127.0.0.1:8799     # propose
   python scripts/seed_board.py --check … --submit https://wfsim.app

WHAT IT IS FOR. Half the roster has no row, and an empty board is a page with
no answer on it — which is the one thing a weapon page exists to have. A seed
is an ORDINARY SUBMISSION: same door, same canonicalisation, same scorer, and
anybody can beat it. It gets no mark and no privilege, because a board that
treated its own builds differently would stop being one board.

WHERE THE BUILD COMES FROM. The winners already on the board, by MOD POOL: the
cards that win on a pool are near-universal inside it — `galvanized_hell` is on
95% of shotgun winners, `primed_dual_rounds` on every Arch-gun one — so the
pool's own frequency order is the best answer available without a search.

IT IS NOT A SEARCH AND DOES NOT CLAIM TO BE. What varies between two weapons of
one pool is the last few slots, which is exactly the element and crit/status
question a real optimizer answers; this fills the slots nobody disagrees about
and leaves the rest to whoever runs one. The board's own wording is already
honest about what a row is: the best build SUBMITTED, not the best there is.

THE ONLY TRIGGER IS AN EMPTY BOARD. A weapon with one row is a weapon the
ordinary machinery owns, and seeding it again would put this script in
competition with real builds. New weapons reach it by having no rows yet.
"""
import argparse
import collections
import glob
import io
import json
import os
import re
import sys
import urllib.error
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MAIN_SLOTS = 8


def yfield(text, name):
    m = re.search(rf"^{name}:\s*(.+)$", text, re.M)
    return m.group(1).strip().strip("\"'") if m else ""


def roster():
    """weapon id -> what a candidate has to respect."""
    out = {}
    for p in glob.glob(os.path.join(ROOT, "data", "weapons", "*", "*.yaml")):
        s = io.open(p, encoding="utf-8", errors="replace").read()
        wid = yfield(s, "id")
        if not wid:
            continue
        pools = re.search(r"^mod_pools:\s*\[([^\]]*)\]", s, re.M)
        out[wid] = {
            "id": wid,
            "slot": os.path.basename(os.path.dirname(p)),
            "pools": [x.strip() for x in pools.group(1).split(",")] if pools else [],
            "exalted": yfield(s, "exalted") == "true",
        }
    return out


def mod_pool():
    """mod id -> the pool directory it lives in, and who may not take it."""
    out = {}
    for p in glob.glob(os.path.join(ROOT, "data", "mods", "*", "*.yaml")):
        s = io.open(p, encoding="utf-8", errors="replace").read()
        mid = yfield(s, "id")
        if not mid:
            continue
        ex = re.search(r"^excludes_weapon:\s*\[([^\]]*)\]", s, re.M)
        out[mid] = {
            "pool": os.path.basename(os.path.dirname(p)),
            "excludes": [x.strip() for x in ex.group(1).split(",")] if ex else [],
            "stance": "stance:" in s,
        }
    return out


def winners_by_pool(weapons):
    """THE POOL'S OWN ORDER: every card on a riven-free winner, by how often.

    Counted per WEAPON and not per row — a weapon with three thousand rows
    would otherwise decide the pool on its own.
    """
    freq = collections.defaultdict(collections.Counter)
    arcanes = collections.defaultdict(collections.Counter)
    for p in sorted(glob.glob(os.path.join(ROOT, "site", "board", "*.json"))):
        if p.endswith("index.json"):
            continue
        wid = os.path.basename(p)[:-5]
        spec = weapons.get(wid)
        if not spec:
            continue
        best = None
        for r in json.load(io.open(p, encoding="utf-8")):
            if "riven" in (r.get("mods") or ()):
                continue
            if best is None or r["score"] > best["score"]:
                best = r
        if not best:
            continue
        key = tuple(spec["pools"])
        for m in best.get("mods") or ():
            freq[key][m] += 1
        for a in best.get("arcanes") or ():
            arcanes[key][a] += 1
    return freq, arcanes


def candidate(spec, freq, arcfreq):
    """The pool's consensus, cut to what THIS weapon can hold.

    THE ENGINE SAYS WHAT IS LEGAL. `/api/meta` carries each weapon's own mod
    ids, arcane ids, seat count and evolution tiers, so nothing here re-derives
    a pool rule — the frequency order only decides which of the legal cards to
    reach for first.
    """
    key = tuple(spec["pools"])
    legal = set(spec.get("mods") or ())
    order = [m for m, _ in freq.get(key, collections.Counter()).most_common()]
    picked = [m for m in order if m in legal][:MAIN_SLOTS]
    # A POOL WITH NO DONOR STILL GETS A BUILD. Six of the roster's pools have
    # never had a winner — a sentinel weapon's among them — so the order falls
    # back to what wins ANYWHERE, and then to the weapon's own list, which is
    # a worse answer than a pool-mate's and a much better one than no row.
    if len(picked) < MAIN_SLOTS:
        every = collections.Counter()
        for c in freq.values():
            every.update(c)
        for m in [m for m, _ in every.most_common()] + sorted(legal):
            if m in legal and m not in picked:
                picked.append(m)
            if len(picked) == MAIN_SLOTS:
                break

    # EVERY SEAT, because a ruler that wants them full wants them full. The
    # pool's favourite that this weapon can seat, then anything it can, so a
    # second seat is filled rather than left to refuse the build.
    seats = int(spec.get("arcane_slots") or 0)
    pool_arcs = [a for a, _ in arcfreq.get(key, collections.Counter()).most_common()]
    ok_arcs = [a for a in pool_arcs if a in set(spec.get("arcanes") or ())]
    rest = [a for a in (spec.get("arcanes") or ()) if a not in ok_arcs]
    arc = (ok_arcs + rest)[:seats]

    # EVERY TIER, one option each, in tier order. Which option is a real
    # question and this answers it with the first — an Incarnon weapon with no
    # evolutions is the BASE FORM, a different gun, so an unfilled ladder is
    # not a weaker build but the wrong one.
    evos = []
    for tier in spec.get("evolutions") or ():
        opts = [o for o in (tier.get("options") or ()) if not o.get("broken")]
        if opts:
            evos.append(opts[0]["id"])

    # AN ADVERSARY WEAPON NAMES ITS PROGENITOR ELEMENT or it is not a build.
    # The first the weapon allows; the roll is scored at its ceiling anyway.
    val = ""
    v = spec.get("valence")
    if isinstance(v, dict) and v.get("elements"):
        val = v["elements"][0]
    return picked, arc, evos, val


def get(base, path, timeout=30):
    """`/api/meta` IS A GET and everything else is a POST — the native server
    matches on the exact (method, path), so the wrong verb is a 404."""
    req = urllib.request.Request(base.rstrip("/") + path,
                                 headers={"User-Agent": "wfsim-seed"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read().decode("utf-8"))


def post(base, path, payload, timeout=30):
    req = urllib.request.Request(
        base.rstrip("/") + path,
        data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json", "User-Agent": "wfsim-seed"},
    )
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read().decode("utf-8"))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", required=True, help="a wfsim server that answers /api/board/check")
    ap.add_argument("--submit", help="where to send what the check accepted")
    ap.add_argument("--benchmark", default="standard_single_target")
    ap.add_argument("--limit", type=int, default=0)
    a = ap.parse_args()

    weapons = roster()
    # THE MODES A WEAPON DECLARES, from the engine rather than from the yaml:
    # a mode is how a weapon is FIRED and the engine derives the list. One
    # submission per mode, because a row is per (build, ruler, mode) and a
    # weapon seeded only in its base form leaves its cycle empty.
    try:
        meta = get(a.check, "/api/meta")
        for w in meta.get("weapons") or ():
            if w.get("id") in weapons:
                weapons[w["id"]].update({
                    k: w.get(k) for k in
                    ("modes", "mods", "arcanes", "arcane_slots", "evolutions", "valence")
                })
    except urllib.error.URLError as e:
        print(f"meta unreachable ({e})")
        return 1
    freq, arcanes = winners_by_pool(weapons)

    empty = []
    for p in sorted(glob.glob(os.path.join(ROOT, "site", "board", "*.json"))):
        if p.endswith("index.json"):
            continue
        wid = os.path.basename(p)[:-5]
        if json.load(io.open(p, encoding="utf-8")):
            continue
        spec = weapons.get(wid)
        # AN EXALTED WEAPON IS NOT SEEDED: its row is its Warframe's too, and
        # a frame nobody chose is not a build anybody would want offered.
        if spec and not spec["exalted"]:
            empty.append(spec)
    if a.limit:
        empty = empty[: a.limit]
    print(f"{len(empty)} weapon(s) with no row")

    ok, refused, sent = 0, collections.Counter(), 0
    for spec in empty:
        picked, arc, evos, val = candidate(spec, freq, arcanes)
        body = {
            "benchmark": a.benchmark,
            "weapon": spec["id"],
            "mods": picked,
            "evolutions": evos,
            "arcanes": arc,
            "valence": val,
        }
        # THE DOOR NAMES WHAT IS WRONG, so a refusal that names a card is a
        # candidate to fix rather than one to give up on: a kitgun's seat is
        # narrower than the arcane list its weapon entry carries, and the door
        # is the only thing that knows. Bounded, so an unfixable reason still
        # ends the attempt instead of looping.
        v, barred = {}, set()
        for _ in range(10):
            try:
                v = post(a.check, "/api/board/check", body)
            except urllib.error.URLError as e:
                print(f"  {spec['id']}: check unreachable ({e})")
                v = {}
                break
            if v.get("accepted"):
                break
            named = re.match(r"^(\S+) is not an arcane", v.get("reason", ""))
            if not named or named.group(1) not in body["arcanes"]:
                break
            barred.add(named.group(1))
            keep = [x for x in body["arcanes"] if x not in barred]
            spare = [x for x in (spec.get("arcanes") or ())
                     if x not in barred and x not in keep]
            body["arcanes"] = (keep + spare)[: int(spec.get("arcane_slots") or 0)]
        if not v.get("accepted"):
            refused[v.get("reason", "?")[:60]] += 1
            if "0 mods" in v.get("reason", ""):
                print(f"  no cards for {spec['id']} ({len(spec.get('mods') or ())} legal)")
            continue
        ok += 1
        if a.submit:
            for m in spec.get("modes") or ["base"]:
                try:
                    post(a.submit, "/api/board/submit", {**body, "mode": m})
                    sent += 1
                except urllib.error.URLError as e:
                    print(f"  {spec['id']} {m}: submit failed ({e})")
    print(f"accepted {ok} of {len(empty)}" + (f", submitted {sent}" if a.submit else ""))
    for why, n in refused.most_common(8):
        print(f"  {n:4}  {why}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
