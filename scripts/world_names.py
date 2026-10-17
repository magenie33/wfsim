#!/usr/bin/env python3
"""The names the Utility pages read, in English and Chinese: `worker/world_names.json`.

DE's world state names a fissure by ids — `SolNode717`, `MT_SURVIVAL`,
`VoidT6` — and the arbitration schedule names a node; this table turns each
into the words the game shows, and gives each node its planet, mission type
and faction. Every
string is DE's own, transcribed from its localization dictionaries through
`browse.wf/warframe-public-export-plus` (docs/DATA_SOURCES.md), joined by the
dictionary key and never by name.

Usage:
  python scripts/world_names.py      fetch into vendor/export-plus/, write the table

Run it again when DE adds nodes: a node the table lacks shows its id.
"""

import json
import sys
import urllib.request
from collections import Counter
from pathlib import Path

from bot_auth import bot_headers

ROOT = Path(__file__).resolve().parent.parent
CACHE = ROOT / "vendor" / "export-plus"
OUT = ROOT / "worker" / "world_names.json"
SOURCE = "https://browse.wf/warframe-public-export-plus/{file}"
FILES = ("ExportRegions.json", "dict.en.json", "dict.zh.json")
UA = {"User-Agent": "wfsim-data/1.0"}

# THE FISSURE TIERS, in the order the world state numbers them: `VoidT1` is
# Lith and each step is the next relic era, Requiem at 5 and Omnia at 6 (the
# wiki's Void Fissure page lists the tiers in this order).
TIERS = {
    "VoidT1": "/Lotus/Language/Relics/Era_LITH",
    "VoidT2": "/Lotus/Language/Relics/Era_MESO",
    "VoidT3": "/Lotus/Language/Relics/Era_NEO",
    "VoidT4": "/Lotus/Language/Relics/Era_AXI",
    "VoidT5": "/Lotus/Language/Relics/Era_REQUIEM",
    "VoidT6": "/Lotus/Language/Relics/Era_OMNI",
}

# THE FACTIONS a node states (`ExportRegions` `faction`), each by the key the
# game's own faction labels use. A faction missing here shows its id.
FACTIONS = {
    "FC_GRINEER": "/Lotus/Language/Game/Faction_GrineerUC",
    "FC_CORPUS": "/Lotus/Language/Game/Faction_CorpusUC",
    "FC_INFESTATION": "/Lotus/Language/Game/Faction_InfestationUC",
    "FC_OROKIN": "/Lotus/Language/Game/Faction_OrokinUC",
    "FC_SENTIENT": "/Lotus/Language/Game/Faction_SentientUC",
    "FC_MITW": "/Lotus/Language/Game/Faction_MITW",
    "FC_SCALDRA": "/Lotus/Language/1999/Faction_Scaldra",
    "FC_TECHROT": "/Lotus/Language/1999/Faction_Techrot",
}


def fetch() -> dict:
    CACHE.mkdir(parents=True, exist_ok=True)
    got = {}
    for name in FILES:
        url = SOURCE.format(file=name)
        with urllib.request.urlopen(urllib.request.Request(url, headers=bot_headers(url, UA)), timeout=120) as r:
            body = r.read()
        (CACHE / name).write_bytes(body)
        got[name] = json.loads(body)
    return got


def display(s: str) -> str:
    """DE's mission names are ALL CAPS in English ("MOBILE DEFENSE"); the page
    shows them as words. A name DE writes in mixed case is kept as written."""
    return s.title() if s.isupper() else s


def main() -> None:
    got = fetch()
    regions, en, zh = got["ExportRegions.json"], got["dict.en.json"], got["dict.zh.json"]

    def both(key: str, words=lambda s: s) -> dict | None:
        if key not in en:
            return None
        # A KEY CHINESE DOES NOT HAVE IS LEFT OUT, never filled from English:
        # the page then shows the English, which is what the game would.
        return {"en": words(en[key]), **({"zh": zh[key]} if zh.get(key) else {})}

    # A NODE'S PLANET is its `systemName` key, never `systemIndex`: a Proxima
    # shares its planet's index, which would make Earth Proxima Earth.
    nodes, by_type, planets = {}, {}, {}
    for node, r in sorted(regions.items()):
        name = both(r.get("name", ""))
        if not name:
            continue
        system = both(r.get("systemName", ""))
        if system:
            planets[r["systemName"]] = (r.get("systemIndex", 99), system)
        nodes[node] = {"name": name, "system": system, "planet": r["systemName"] if system else None,
                       "mission": both(r.get("missionName", ""), display),
                       "type": r.get("missionType"), "faction": r.get("faction")}
        if r.get("missionType") and r.get("missionName"):
            by_type.setdefault(r["missionType"], Counter())[r["missionName"]] += 1
    # A FISSURE STATES ITS MISSION TYPE, which may not be its node's own, so the
    # type is named by the name DE gives most nodes of that type.
    missions = {t: both(c.most_common(1)[0][0], display) for t, c in sorted(by_type.items())}
    tiers = {t: both(k) for t, k in TIERS.items()}
    factions = {f: both(k, display) for f, k in FACTIONS.items()}
    missing = [t for t, v in {**tiers, **factions}.items() if not v]
    if missing:
        sys.exit(f"dictionary lost the name of {missing}")
    # `planets` in the star chart's order, a Proxima beside its planet.
    planets = {k: v for k, (_, v) in sorted(planets.items(), key=lambda kv: (kv[1][0], kv[0]))}
    OUT.write_text(json.dumps({"tiers": tiers, "missions": missions, "factions": factions,
                               "planets": planets, "nodes": nodes},
                              ensure_ascii=False, separators=(",", ":")) + "\n",
                   encoding="utf-8", newline="\n")
    print(f"{OUT.relative_to(ROOT)}: {len(nodes)} nodes, {len(planets)} planets, {len(missions)} mission types, {len(factions)} factions")


if __name__ == "__main__":
    main()
