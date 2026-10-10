#!/usr/bin/env python3
"""Generate `data/market.yaml` — what warframe.market calls the things we model.

warframe.market is the only source for its OWN identifiers, so this is the one
place the site is authoritative: it is not a source for a game fact and nothing
here feeds a number. The file it writes carries slugs and nothing else.

EVERY JOIN IS BY ID, NEVER BY NAME. Their `gameRef` is DE's uniqueName, which
is our `internal_name`, and their riven attribute `gameRef` is our stat `tag`.
A name join would silently pair "Blaze" the mod with "Blaze" the arcane.

The riven attribute join has a SECOND key: DE's two name fragments, which both
sides carry. It is not redundancy — WM merges each gun stat with its melee twin
into one attribute (`WeaponMeleeFactionDamageCorpus` has no `gameRef` of its
own), so four of our tags resolve only through the fragments. Where both keys
answer they must agree, or this refuses to write.

An entry WM does not list is not tradeable there, and its absence is the
answer — that is why nothing here is hand-filled.

    python scripts/gen_market.py           # report
    python scripts/gen_market.py --write   # write data/market.yaml
"""

import json
import re
import sys
import urllib.request
from bot_auth import bot_headers
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "data" / "market.yaml"
API = "https://api.warframe.market/v2"


def fetch(path):
    url = f"{API}{path}"
    with urllib.request.urlopen(urllib.request.Request(url, headers=bot_headers(url)), timeout=60) as r:
        return json.load(r)["data"]


def internal_names(*patterns):
    """`id → internal_name` for every data file under `patterns` that has one."""
    out = {}
    for f in sorted(f for pat in patterns for f in ROOT.glob(pat)):
        text = f.read_text(encoding="utf-8")
        m = re.search(r"^internal_name:\s*(\S+)", text, re.M)
        if m:
            out[f.stem] = m.group(1)
    return out


def riven_stats():
    """`id → {(tag, prefix, suffix)}` for every riven stat, across the classes.

    A stat id is shared across the classes and the TAG behind it is not: melee
    carries `WeaponMeleeFactionDamageCorpus` where a gun carries
    `WeaponFactionDamageCorpus`. So every variant is collected and the rule is
    applied after resolution, where it belongs — the page has one id to send,
    so the variants must land on one slug.
    """
    out = {}
    for f in sorted((ROOT / "data" / "rivens").glob("*.yaml")):
        doc = yaml.safe_load(f.read_text(encoding="utf-8"))
        for s in (doc or {}).get("stats", []):
            if all(k in s for k in ("id", "tag", "prefix", "suffix")):
                out.setdefault(s["id"], set()).add((s["tag"], str(s["prefix"]), str(s["suffix"])))
    return out


def riven_families(weapon_refs, errors):
    """`riven family → auction slug`, one row per FAMILY rather than per weapon.

    A riven belongs to a family, not to an entry in it: one card fits Braton,
    Braton Prime and MK1-Braton alike, and warframe.market lists the auctions
    under the family — `braton`, whose `gameRef` is the base rifle's. Joining
    entry-to-entry would lose every variant in the game.

    THE FAMILY IS THE KEY BECAUSE THE FAMILY IS THE FACT. Which entry belongs
    to which family — and that a form takes the family of the weapon it
    transforms from — is OUR structure, and `engine::data::market` applies it at
    lookup. Spelling it out per entry here would put our structure in a
    generated file, where a new form silently gets no link until someone
    remembers to re-run this.

    The family's slug is whichever member they DO list. WM may list a
    variant as its own riven weapon (Dex Nikana) where the wiki files it under
    the family; the wiki wins, so the slug is the member NAMED as the family,
    and a family with no such member among several slugs is refused.
    """
    out = {}
    for f in sorted(ROOT.glob("data/weapons/**/*.yaml")):
        text = f.read_text(encoding="utf-8")
        ref = re.search(r"^internal_name:\s*(\S+)", text, re.M)
        # The value STOPS AT THE COMMENT. Most of these lines carry one citing
        # the wiki module, and swallowing it gives the weapon a family of its
        # own spelled "Boar # module `Family` - shared with the Boar", which
        # groups with nothing and loses every variant in silence.
        fam = re.search(r"^riven_family:\s*([^#\n]+?)\s*(?:#.*)?$", text, re.M)
        if not ref or ref.group(1) not in weapon_refs:
            continue
        # A weapon declaring no family is its own — the entry means "no
        # variants", not "no riven".
        name = re.search(r"^name:\s*([^#\n]+?)\s*(?:#.*)?$", text, re.M)
        out.setdefault(fam.group(1) if fam else f.stem, {})[weapon_refs[ref.group(1)]] = name and name.group(1)
    picked = {}
    for fam, members in sorted(out.items()):
        if len(members) == 1:
            picked[fam] = next(iter(members))
            continue
        named = [slug for slug, n in members.items() if n == fam]
        if len(named) == 1:
            picked[fam] = named[0]
        else:
            errors.append(f"riven_families: {fam} resolves to several slugs: {sorted(members)}")
    return picked


def resolve(ours, theirs, label, errors):
    """Join `id → key` against `key → slug`.

    TWO ENTRIES MAY LAND ON ONE SLUG, and that is not a collision to refuse: a
    Catchmoon is ONE chamber part, filed here as a primary entry and a
    secondary one because the slot decides the mod pool. They share a
    uniqueName, so by construction they are the same thing to buy. The join is
    by id, so anything sharing a slug shares DE's own key for it.
    """
    return {ident: theirs[key] for ident, key in sorted(ours.items()) if key in theirs}


def main():
    errors = []

    items = fetch("/items")
    # ONE ITEM PER uniqueName, and a SET WINS. A Prime sells as a set, and
    # `braton_prime_set` is the row carrying the weapon's own gameRef — its
    # parts carry theirs. Tags do not narrow this beyond that: filtering to
    # `weapon` lost the Prisma Burst Laser, which they tag `sentinel`.
    by_ref = {}
    for it in items:
        ref = it.get("gameRef")
        if not ref:
            continue
        if ref in by_ref and "set" not in it.get("tags", []):
            continue
        by_ref[ref] = it["slug"]
    mods = resolve(internal_names("data/mods/**/*.yaml"), by_ref, "mods", errors)
    weapons = resolve(internal_names("data/weapons/**/*.yaml"), by_ref, "weapons", errors)
    arcanes = resolve(internal_names("data/arcanes/**/*.yaml"), by_ref, "arcanes", errors)

    # AN ADVERSARY WEAPON IS AUCTIONED, NOT SOLD, because the valence bonus it
    # came out of its Lich with is part of what is being traded — the same
    # reason a riven is auctioned. Kuva and Tenet are two auction types, so
    # they are two tables: the table a weapon is in IS the `type=` its link
    # needs.
    lich = resolve(internal_names("data/weapons/**/*.yaml"),
                   {w["gameRef"]: w["slug"] for w in fetch("/lich/weapons") if w.get("gameRef")},
                   "lich_weapons", errors)
    sister = resolve(internal_names("data/weapons/**/*.yaml"),
                     {w["gameRef"]: w["slug"] for w in fetch("/sister/weapons") if w.get("gameRef")},
                     "sister_weapons", errors)

    weapon_refs = {w["gameRef"]: w["slug"] for w in fetch("/riven/weapons") if w.get("gameRef")}
    families = riven_families(weapon_refs, errors)

    attrs = fetch("/riven/attributes")
    by_tag = {a["gameRef"]: a["slug"] for a in attrs if a.get("gameRef")}
    # A FRAGMENT PAIR WM GIVES TO TWO ATTRIBUTES IS NO KEY: it fills the
    # splicer's stats with placeholders ("Laci Nus" on five melee ones), and
    # keeping the last of them pairs a stat with a stranger's slug.
    pairs = {}
    for a in attrs:
        if a.get("prefix"):
            pairs.setdefault((a["prefix"].lower(), a["suffix"].lower()), set()).add(a["slug"])
    by_fragments = {k: next(iter(v)) for k, v in pairs.items() if len(v) == 1}
    ours = riven_stats()
    stats = {}
    for ident, variants in sorted(ours.items()):
        slugs = set()
        for tag, prefix, suffix in sorted(variants):
            tagged = by_tag.get(tag)
            named = by_fragments.get((prefix.lower(), suffix.lower()))
            if tagged and named and tagged != named:
                errors.append(f"riven_stats: {ident} is '{tagged}' by tag and '{named}' by name fragments")
            if tagged or named:
                slugs.add(tagged or named)
            else:
                errors.append(f"riven_stats: {ident} ({tag}) resolves by neither key")
        if len(slugs) > 1:
            errors.append(f"riven_stats: {ident} resolves to several slugs: {sorted(slugs)}")
        if slugs:
            stats[ident] = sorted(slugs)[0]

    w_total = len(internal_names("data/weapons/**/*.yaml"))
    print(f"mods           {len(mods):4} / {len(internal_names('data/mods/**/*.yaml'))}")
    print(f"weapons        {len(weapons):4} / {w_total}")
    print(f"arcanes        {len(arcanes):4} / {len(internal_names('data/arcanes/**/*.yaml'))}")
    print(f"lich_weapons   {len(lich):4}")
    print(f"sister_weapons {len(sister):4}")
    print(f"riven_families {len(families):4}")
    print(f"riven_stats    {len(stats):4} / {len(ours)}")
    for e in errors:
        print(f"  ERROR {e}")
    if errors:
        return 1

    # EVERY STAT OR NOTHING: a partly-filled table would hand the auction page
    # a filter missing one of the roll's stats, which is a DIFFERENT search
    # returning rivens the visitor is not holding.
    if len(stats) != len(ours):
        print("  ERROR riven_stats is incomplete")
        return 1

    def table(name, rows):
        return f"{name}:\n" + "".join(f"  {k}: {v}\n" for k, v in rows.items())

    text = (
        "# GENERATED by scripts/gen_market.py. Do not hand-edit: re-run the script.\n"
        "#\n"
        "# What warframe.market calls things we model, so a card can link to what it\n"
        "# sells for. Slugs only — no price travels with them, because a price needs a\n"
        "# refresh rule and a staleness rule and this file would then be a second\n"
        "# source for a number the site already owns.\n"
        "#\n"
        "# AN ABSENT ENTRY IS NOT LISTED THERE, and that is the whole rule for whether\n"
        "# a link is offered — there is no `tradeable` field anywhere in this repo. The\n"
        "# Amalgam and Umbral cards are absent, so are the 250 weapons that only ever\n"
        "# come off a blueprint, and so are the kitgun chambers, whose yaml carries no\n"
        "# `internal_name` to join on.\n"
        "#\n"
        "# SOLD OR AUCTIONED, and the table says which. `mods`, `weapons` and `arcanes`\n"
        "# are item pages. `lich_weapons` and `sister_weapons` are auctions, because the\n"
        "# valence an adversary weapon came out of its Lich with is part of what is\n"
        "# being traded — and the table a weapon is in is the `type=` its link needs.\n"
        "#\n"
        "# `riven_families` is keyed by our `riven_family` (a weapon with none is its\n"
        "# own family), because that is what a riven fits: Braton Prime and its Incarnon\n"
        "# form both reach `braton` through it. The engine walks a weapon to its family.\n"
        "#\n"
        "# `riven_stats` keys are the stat ids in data/rivens/; the slugs are what\n"
        "# `positive_stats` / `negative_stats` take on an auction search.\n"
        "\n"
        + table("mods", mods) + "\n"
        + table("weapons", weapons) + "\n"
        + table("arcanes", arcanes) + "\n"
        + table("lich_weapons", lich) + "\n"
        + table("sister_weapons", sister) + "\n"
        + table("riven_families", families) + "\n"
        + table("riven_stats", stats)
    )
    if "--write" in sys.argv:
        OUT.write_text(text, encoding="utf-8", newline="\n")
        print(f"wrote {OUT}")
    else:
        print("(--write to commit it)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
