#!/usr/bin/env python3
"""Write the Warframe builder's frames and ability cards from the wiki's modules.

    python scripts/wiki_warframes.py            # report what would be written
    python scripts/wiki_warframes.py --write    # write the missing files
    python scripts/wiki_warframes.py --refresh  # fetch the modules again

Every frame in `data/frames.yaml` gets `data/warframes/<id>.yaml` and each of
its four abilities a card in `data/warframe_abilities/`, from
`Module:Warframes/data` and `Module:Ability/data`. A file that already exists
is never touched: a hand-written one carries rules this script cannot know.

The roster row reaches the wiki through DE's export: its name is the export's
own, which names a `uniqueName`, joined to the module's `InternalName`.

What this writes is the frame as the builder SEATS it — stats, polarities,
passive text and ability cards without numbers. The passive is admitted as
unmodelled (`passive_unmodelled:`), and a card with no `stats:` says so on the
panel; see docs/WARFRAMES.md §Adding a frame.
"""

import argparse
import json
import re
import sys
from pathlib import Path

import de_export
import wiki_lua

ROOT = Path(__file__).resolve().parent.parent
CACHE = ROOT / "vendor" / "wiki"
FRAMES = ROOT / "data" / "warframes"
ABILITIES = ROOT / "data" / "warframe_abilities"
NAMES = ROOT / "data" / "i18n" / "zh" / "names.yaml"

# Not builder frames: a Necramech seats another mod set, the Helminth is the
# infusion pool, and Sirius & Orion is two bodies sharing one build.
SKIP = {"bonewidow", "voidrig", "helminth", "orion_sirius", "sirius_orion"}
POLARITIES = {"madurai", "naramon", "vazarin", "zenurik", "unairu", "penjaga", "umbra", "omni"}
# THE PASSIVE MOVES A NUMBER A WEAPON'S FIGHT READS — damage, crit, status,
# fire rate, punch through, or the frame's armor or health a weapon perk reads —
# so the weapon page says it is missing. Every other passive is the frame's own
# (movement, pickups, survival) and only the builder admits it.
WEAPON_PASSIVES = {
    "ash", "banshee", "cyte_09", "dante", "excalibur", "frost", "garuda", "grendel",
    "hydroid", "jade", "koumei", "kullervo", "mesa", "nyx", "qorvex", "saryn",
    "styanax", "trinity", "uriel", "vauban", "volt", "voruna", "yareli", "zephyr",
}
MARKUP = re.compile(r"<[^>]+>")


def slug(name: str) -> str:
    return re.sub(r"[^a-z0-9]+", "_", name.lower().replace("'", "")).strip("_")


def q(s: str) -> str:
    return json.dumps(s, ensure_ascii=False)


def roster():
    text = (ROOT / "data" / "frames.yaml").read_text(encoding="utf-8")
    return re.findall(r"^  - id: (\S+)\n    name: (.+)$", text, re.M)


def existing_cards():
    """internal_name -> id of every ability card already filed."""
    out = {}
    for p in ABILITIES.glob("*.yaml"):
        t = p.read_text(encoding="utf-8")
        i = re.search(r"^id: (\S+)", t, re.M).group(1)
        n = re.search(r"^internal_name: (\S+)", t, re.M)
        if n:
            out[n.group(1)] = i
    return out


def augment_ids():
    """name -> id of every Warframe mod that augments an ability."""
    out = {}
    for p in (ROOT / "data" / "warframe_mods").glob("*.yaml"):
        t = p.read_text(encoding="utf-8")
        if re.search(r"^augments: ", t, re.M):
            out[re.search(r"^name: \"?(.+?)\"?$", t, re.M).group(1)] = re.search(r"^id: (\S+)", t, re.M).group(1)
    return out


def base_id(fid: str) -> str:
    return fid.removesuffix("_prime").removesuffix("_umbra")


def rank30(frame: dict, stat: str, bonus: int) -> int:
    """Module:Warframes/infobox: `HealthRank30 or Health+100`, the same for shields."""
    v = frame.get(stat + "Rank30")
    return v if v is not None else frame[stat] + bonus


def pol(name, where: str) -> str:
    # W`Polarity`: the {{Pol|Aura}} icon is "Any - O (Any polarity except Umbra)",
    # and Dante, Jade, Protea and Protea Prime have "an innate {{Pol|Aura}}
    # Universal aura polarity" — Omni Forma's polarity, not a colour of its own.
    if isinstance(name, list):
        if "Aura" not in name:
            raise SystemExit(f"{where}: two polarities {name} for one slot")
        name = "Aura"
    p = "omni" if name == "Aura" else name.lower()
    if p not in POLARITIES:
        raise SystemExit(f"{where}: unknown polarity {name}")
    return p


def frame_yaml(fid: str, f: dict, abilities: list[str]) -> str:
    lines = [
        f"# {f['Name'].upper()}, at rank 30, from Module:Warframes/data.",
        "# see notes: generated_frame",
        f"id: {fid}",
        f"name: {f['Name']}",
        f"internal_name: {f['InternalName']}",
        "",
        f"health: {rank30(f, 'Health', 100)}",
        f"shield: {rank30(f, 'Shield', 100)}",
        "",
        f"polarities: [{', '.join(pol(p, fid) for p in f.get('Polarities') or [])}]",
    ]
    if f.get("AuraPolarity"):
        lines.append(f"aura_polarity: {pol(f['AuraPolarity'], fid)}")
    if f.get("ExilusPolarity"):
        lines.append(f"exilus_polarity: {pol(f['ExilusPolarity'], fid)}")
    lines += [
        "",
        f"passive: {q(MARKUP.sub('', f.get('Passive') or '').strip())}",
        f"passive_unmodelled: {'weapon' if base_id(fid) in WEAPON_PASSIVES else 'frame'}",
        f"abilities: [{', '.join(abilities)}]",
        "",
        "source:",
        f"  url: https://wiki.warframe.com/w/{f['Link'].replace(' ', '_')}",
        "",
    ]
    return "\n".join(lines)


def card_yaml(aid: str, a: dict, frame_id: str, augments: list[str]) -> str:
    lines = [
        "# see notes: generated_ability",
        f"id: {aid}",
        f"name: {q(a['Name'])}",
        f"frame: {frame_id}",
    ]
    if a.get("Key"):
        lines.append(f"slot: {a['Key']}")
    lines.append(f"energy_cost: {a['Cost']}")
    if a.get("CostType"):
        lines.append(f"cost_type: {q(a['CostType'])}")
    if a.get("Subsumable"):
        lines.append("subsumable: true")
    if augments:
        lines.append(f"augments: [{', '.join(augments)}]")
    # A COST OF 0 IS THE MODULE SAYING THE PRICE IS NOT A CAST: a drain, a cost
    # per mark, a meter. Printed as 0 it would read as free.
    if a["Cost"] == 0 and not a.get("CostType"):
        lines.append('unmodelled: ["Its cost is not paid per cast and is not entered; see its page."]')
    lines += [
        f"internal_name: {a['InternalName']}",
        f"icon: {a['Icon']}",
        f"description: {q(MARKUP.sub('', a.get('Description') or '').strip())}",
        "",
        "source:",
        f"  url: https://wiki.warframe.com/w/{a.get('Link', a['Name']).replace(' ', '_')}",
        "",
    ]
    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true")
    ap.add_argument("--refresh", action="store_true")
    args = ap.parse_args()

    frames = wiki_lua.parse(wiki_lua.fetch("Module:Warframes/data", CACHE, args.refresh))["Warframes"]
    by_internal = {f["InternalName"]: f for f in frames.values()}
    abil = wiki_lua.parse_local(wiki_lua.fetch("Module:Ability/data", CACHE, args.refresh), "AbilityData")["Ability"]
    export = de_export.items("en")
    by_export_name = {e.get("name"): u for u, e in export.items() if u.startswith("/Lotus/Powersuits/")}
    zh = {}
    for entry in json.loads((ROOT / "vendor" / "public-export" / "zh" / "ExportWarframes_zh.json").read_text(encoding="utf-8"))["ExportWarframes"]:
        for a in entry.get("abilities") or []:
            zh.setdefault(a["abilityUniqueName"], a["abilityName"])
    cards = existing_cards()
    augments = augment_ids()

    new_frames, new_cards, zh_names, problems = {}, {}, {}, []
    for fid, name in roster():
        if fid in SKIP or fid == "prototype":
            continue
        unique = by_export_name.get(name)
        f = by_internal.get(unique)
        if not f:
            problems.append(f"{fid}: no wiki entry for export {unique or name}")
            continue
        base = frames.get(f["Name"].removesuffix(" Prime")) or f
        ids = []
        for slot, an in enumerate(f["Abilities"], 1):
            cand = [a for a in abil.values() if a["Name"] == an and a.get("Powersuit") in (f["Name"], base["Name"])]
            cand = cand or [a for a in abil.values() if a["Name"] == an]
            if len(cand) != 1:
                problems.append(f"{fid}: ability {an} matches {len(cand)} entries")
                break
            a = cand[0]
            aid = cards.get(a["InternalName"]) or new_cards.get(a["InternalName"], (slug(a["Name"]),))[0]
            if a["InternalName"] not in cards and a["InternalName"] not in new_cards:
                if (ABILITIES / f"{aid}.yaml").exists():
                    problems.append(f"{fid}: card id {aid} is taken by another ability")
                    break
                owner = slug(base["Name"]) if base is not f else fid
                augs = [augments[n] for n in a.get("Augments") or [] if n in augments]
                new_cards[a["InternalName"]] = (aid, card_yaml(aid, a, owner, augs))
                if a["InternalName"] in zh:
                    zh_names[aid] = zh[a["InternalName"]]
            ids.append(aid)
        else:
            if not (FRAMES / f"{fid}.yaml").exists():
                new_frames[fid] = frame_yaml(fid, f, ids)

    print(f"frames to write: {len(new_frames)}; ability cards to write: {len(new_cards)}; zh names: {len(zh_names)}")
    for p in problems:
        print("PROBLEM", p)
    if not args.write:
        return 1 if problems else 0
    for fid, text in new_frames.items():
        (FRAMES / f"{fid}.yaml").write_text(text, encoding="utf-8", newline="\n")
    for aid, text in new_cards.values():
        (ABILITIES / f"{aid}.yaml").write_text(text, encoding="utf-8", newline="\n")
    # ADDITIVE, like `de_i18n.py fill`: a line already there is never rewritten.
    text = NAMES.read_text(encoding="utf-8")
    m = re.search(r"^warframe_abilities:\n((?:^[ \t]+.*\n?)*)", text, re.M)
    have = set(re.findall(r"^  (\S+):", m.group(1), re.M))
    added = "".join(f"  {i}: {q(n)}\n" for i, n in sorted(zh_names.items()) if i not in have)
    text = text[: m.end(1)] + added + text[m.end(1):]
    NAMES.write_text(text, encoding="utf-8", newline="\n")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
