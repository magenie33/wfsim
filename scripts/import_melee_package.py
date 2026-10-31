#!/usr/bin/env python3
"""Import shared melee actions; --apply writes, --check verifies generated data.

uv run --with PyYAML python scripts/import_melee_package.py --apply
The catalog owns its manual geometry masks. Package fields come only from the
pinned JSON; source omissions are reported rather than filled with fake hits.
"""

import argparse
from collections import Counter, defaultdict
import copy
import json
from pathlib import Path
import re

import yaml

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "tests/fixtures/melee/package-2026-10-08.json"
CATALOG = ROOT / "data/melee/actions.yaml"
REPORT = ROOT / "docs/melee/package-coverage.json"
CONTEXTS = {"neutral": "CC_GROUND", "forward": "CC_GROUND_BRANCH_A",
            "block_forward": "CC_GROUND_BRANCH_B", "block": "CC_GROUND_BRANCH_C",
            "heavy": "CC_GROUND_HEAVY", "slide": "CC_SLIDING"}
PROCS = {"PT_KNOCKBACK": "impact", "PT_BLEEDING": "slash", "PT_KNOCKED_DOWN": "knockdown",
         "PT_LIFT_HIT": "lifted", "PT_ELECTROCUTION": "electricity", "PT_FRAILTY": "puncture",
         "PT_INFECTED": "viral", "PT_POISONED": "toxin", "PT_CHILLED": "cold"}
PHYSICAL = {"DT_IMPACT": "impact_multiplier", "DT_PUNCTURE": "puncture_multiplier", "DT_SLASH": "slash_multiplier"}
ELEMENTAL = {"DT_ELECTRICITY": "electricity", "DT_POISON": "toxin"}
BAD_DAMAGE_VECTOR = "/Lotus/Weapons/Tenno/Melee/Attacks/DualSwordBranchAB"
GLAIVE_THROW = ["/Lotus/Weapons/Tenno/Melee/Attacks/" + x for x in
               ("GlaiveDSCharge", "GlaiveDSConsecutiveCharge", "GlaiveDSConsecutiveChargeB")]


def read(path):
    return yaml.safe_load(path.read_text())


def dump(value):
    return yaml.safe_dump(value, sort_keys=False, width=160)


def hit_properties(attack, index):
    overrides = attack["swings"]
    return overrides[index] if index < len(overrides) and overrides[index] is not None else attack


def action(path, raw, omissions):
    a = raw["attacks"][path]
    if not a["duration"] or not a["anim"]:
        raise ValueError("missing animation duration")
    if not a["hits"]:
        if path in GLAIVE_THROW:
            release = next(e for e in a["events"] if e["event"] == "ThrowGlaive")
            return {"source": path, "animation": a["anim"], "duration_seconds": a["duration"],
                    "charge": {"weapon_throw": True},
                    "events": [{"at_seconds": round(a["duration"] * release["t"], 12), "kind": "throw",
                                "multiplier": 1.0, "combo_points": 1}]}, [None]
        raise ValueError("missing sweep events or projectile attack data")
    events = []
    for index, hit in enumerate(a["hits"]):
        if not hit["event"].startswith(("StartMeleeSweep", "StartMeleeThrust")):
            raise ValueError("unknown attack event: " + hit["event"])
        p = hit_properties(a, index)
        e = {"at_seconds": round(a["duration"] * hit["t"], 12), "kind": "sweep",
             "hand": hit["param"], "multiplier": p["atten"], "combo_points": p["comboHits"]}
        forced = [PROCS[x] for x in p["extraProcs"] if x in PROCS]
        if forced:
            e["forced_procs"] = forced
        unknown = [x for x in p["extraProcs"] if x not in PROCS]
        if unknown:
            omissions.append({"action": path, "hit": index, "unmodelled_control": unknown})
        damage = p["damagePct"]
        if path == BAD_DAMAGE_VECTOR and index == 2:
            omissions.append({"action": path, "hit": index, "invalid_damage_vector": damage})
            damage = {}
        for damage_type, value in damage.items():
            if not 0 <= value <= 100:
                raise ValueError("invalid damage override")
            if damage_type in PHYSICAL:
                e[PHYSICAL[damage_type]] = value
            elif damage_type in ELEMENTAL:
                e.setdefault("damage_type_multipliers", {})[ELEMENTAL[damage_type]] = value
            else:
                raise ValueError("unsupported damage override: " + damage_type)
        if p["procAtten"] != 1.0:
            e["status_multiplier"] = p["procAtten"]
        events.append((e, index))
    # Pair properties before sorting hand-interleaved source events.
    events.sort(key=lambda item: item[0]["at_seconds"])
    result = {"source": path, "animation": a["anim"], "duration_seconds": a["duration"]}
    if a["extraAction"] == "MELEE_CHARGE":
        result["charge"] = {"animation": a["extraAnim"], "weapon_heavy": True}
        for e, _ in events:
            e["combo_points"] = 0
    elif a["extraAction"] is not None:
        raise ValueError("unsupported extra action: " + a["extraAction"])
    # A combo slam uses the weapon's own declared slam profile. These are
    # dedicated attack events, unlike cosmetic whooshes and ground effects.
    for e in a["events"]:
        if e["event"] in ("MeleeSlam", "AxeSlam"):
            events.append(({"at_seconds": round(a["duration"] * e["t"], 12), "kind": "slam",
                            "multiplier": 1.0, "combo_points": 1}, None))
    events.sort(key=lambda item: item[0]["at_seconds"])
    result["events"] = [e for e, _ in events]
    return result, [index for _, index in events]


def expanded_inputs(rows):
    result, pending = [], []
    for h in rows:
        if h.get("slam_multiplier") is None:
            pending.extend([h] * h.get("hits", 1))
        if h.get("input_end", h.get("delay_seconds", 0) > 0):
            result.append(pending)
            pending = []
    return result


def comparable(rows):
    return [{k: v for k, v in h.items() if k not in ("delay_seconds", "windup_seconds")} for h in rows]


def damage_vector(amount, current, types=None):
    names = {"DT_FIRE": "heat", "DT_FREEZE": "cold", "DT_POISON": "toxin", "DT_EXPLOSION": "blast"}
    if types:
        scale = 1.0 if sum(types.values()) > 1.001 else amount
        vector = {names.get(k, k.removeprefix("DT_").lower()): v * scale for k, v in types.items()}
    else:
        total = sum(current.values())
        assert total > 0
        vector = {k: amount * v / total for k, v in current.items()}
    if set(vector) == set(current) and all(abs(v - current[k]) < 0.001 for k, v in vector.items()):
        return current
    return {k: round(v, 6) for k, v in vector.items()}


def replace_mapping(text, key, value, indent=0):
    # Preserve unrelated numbers, sourcing comments and measured rules.
    pattern = rf"(?m)^{' ' * indent}{key}:.*\n(?:^(?:{' ' * (indent + 1)}[^\n]*|[ \t]*|[ \t]*#.*)\n)*"
    replacement = "".join(" " * indent + line + "\n" for line in dump({key: value}).splitlines())
    changed, n = re.subn(pattern, lambda _: replacement, text, count=1)
    if n != 1:
        raise ValueError("cannot replace " + key)
    return changed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    raw = json.loads(SOURCE.read_text())
    omissions, invalid = [], {}
    actions, orders = {}, {}
    for path in sorted(raw["attacks"]):
        try:
            actions[path], orders[path] = action(path, raw, omissions)
        except ValueError as e:
            invalid[path] = str(e)
    trees = {}
    for tree, t in raw["trees"].items():
        usable = {c: paths for c, paths in t["contexts"].items()
                  if c in CONTEXTS.values() and paths and all(p in actions for p in paths)}
        if usable:
            trees[tree] = usable
    mods = [(p, read(p)) for p in sorted((ROOT / "data/mods").glob("**/*.yaml"))]
    mods = [(p, m) for p, m in mods if m.get("combos")]
    geometry = read(CATALOG).get("geometry", {}) if CATALOG.exists() else {}
    observations = defaultdict(list)
    writes = {}
    migrated, retained, aliases = [], [], []
    for path, m in mods:
        tree = m["internal_name"]
        previous = m["combos"]
        updated = copy.deepcopy(previous)
        for form, rows in previous.items():
            context = CONTEXTS.get(form)
            if context in trees.get(tree, {}):
                updated[form] = {"tree": tree, "context": context}
                migrated.append([m["id"], form])
                if isinstance(rows, list):
                    inputs = expanded_inputs(rows)
                    sequence = trees[tree][context]
                    if len(inputs) == len(sequence):
                        for source, old_hits in zip(sequence, inputs):
                            count = len(raw["attacks"][source]["hits"])
                            if count == len(old_hits):
                                flags = [False if i is None else old_hits[i].get("all_around", False) for i in orders[source]]
                                observations[source].append(flags)
                continue
            # The wiki transcription states fallback inputs explicitly.
            if isinstance(rows, dict) and "reuse" in rows:
                aliases.append([m["id"], form, rows["reuse"]])
                continue
            candidates = [f for f, other in previous.items() if f != form and isinstance(rows, list)
                          and isinstance(other, list) and comparable(rows) == comparable(other)
                          and CONTEXTS.get(f) in trees.get(tree, {})]
            if candidates:
                updated[form] = {"reuse": candidates[0]}
                aliases.append([m["id"], form, candidates[0]])
            else:
                retained.append([m["id"], form, "package events incomplete"])
        for form, context in CONTEXTS.items():
            throwing = any(p in GLAIVE_THROW for p in trees.get(tree, {}).get(context, []))
            if form not in updated and context in trees.get(tree, {}) and not throwing:
                updated[form] = {"tree": tree, "context": context}
                migrated.append([m["id"], form])
        m["combos"] = updated
        header = "# see notes: melee_shared_package_actions\n# see notes: stance_grants_capacity\n"
        if any(isinstance(c, list) and c for c in updated.values()):
            header += "# see notes: combo_points_from_multiplier\n# see notes: stance_ranged_rows\n# see notes: stance_from_the_module\n"
        if m["id"] == "gemini_cross":
            header += "# see notes: gemini_cross_timeline\n# see notes: melee_package_timing\n"
        writes[path] = header + dump(m)
    for source, flags in observations.items():
        if source not in geometry:
            # A disagreement retains the arena's conservative forward arc.
            geometry[source] = [all(row[i] for row in flags) for i in range(len(flags[0]))]
    for source, a in actions.items():
        if "Slide" in source and source not in geometry:
            geometry[source] = [True] * len(a["events"])
    weapon_files = [(p, read(p)) for p in sorted((ROOT / "data/weapons").glob("**/*.yaml"))]
    weapons = {w["id"]: w for _, w in weapon_files if isinstance(w, dict) and "id" in w}
    weapon_modes, weapon_gaps, stance_only, bare_aliases = [], [], [], []
    for path, w in weapon_files:
        if not isinstance(w, dict) or "id" not in w:
            continue
        root = w
        seen = set()
        while root.get("inherits"):
            assert root["id"] not in seen, "weapon inheritance cycle"
            seen.add(root["id"])
            root = weapons[root["inherits"]]
        if root.get("slot") != "melee":
            continue
        if root.get("zaw"):
            hand = {"one_handed": "one", "two_handed": "two"}[root["zaw"]["handedness"]]
            source = dict(raw["weapons"]["/Lotus/Weapons/Ostron/Melee/LotusModularWeapon"])
            source.update(raw["zaws"][root["internal_name"]][hand])
            # Unassembled package scalars are placeholders; parts own the panel.
            source["damage"] = {"amount": 0}
            source["attackSpeed"] = 0
        else:
            source = raw["weapons"].get(root.get("internal_name"))
        if source is None:
            weapon_gaps.append([w["id"], "weapon absent from package export"])
            continue
        if root.get("internal_name") in raw.get("hybrids", {}):
            suffix = {"heavy_blade": "DarkSwordDaggerSingle", "dual_swords": "DarkSwordDaggerDuals"}[root["class"]]
            child = next(k for k in raw["hybrids"][root["internal_name"]] if k.rsplit("/", 1)[-1] == suffix)
            source = raw["weapons"][child]
        tree = source.get("stance") if root.get("weapon_type") == "exalted_melee" else source["tree"]
        tree = tree or source["tree"]
        form = w.get("form", "neutral")
        context = CONTEXTS.get(form)
        native = raw["trees"].get(tree, {}).get("contexts", {})
        if form in ("neutral", "forward", "block", "block_forward") and context not in native:
            # Bare block inputs use the default combo; movement uses its own
            # branch only when that branch exists. A default may be stored in A.
            fallback = "CC_GROUND" if "CC_GROUND" in native else "CC_GROUND_BRANCH_A"
            if fallback in trees.get(tree, {}):
                bare_aliases.append([w["id"], context, fallback])
                context = fallback
        if form == "charged_throw" and trees.get(tree, {}).get("CC_GROUND_HEAVY") == GLAIVE_THROW:
            context = "CC_GROUND_HEAVY"
        attack = copy.deepcopy(w.get("attack", {}))
        original = copy.deepcopy(attack)
        projectile_heavy = form == "heavy" and attack.get("shot_type") == "projectile"
        if not projectile_heavy and form in (*CONTEXTS, "heavy_slam") and root.get("weapon_type") == "melee" and not root.get("fixed_stance") and source["damage"]["amount"] > 0:
            base_attack = root["attack"]
            if attack.get("damage") == base_attack.get("damage"):
                attack["damage"] = damage_vector(source["damage"]["amount"], attack["damage"], source["damage"]["types"])
            for field, source_key in (("crit_chance", "crit"), ("crit_multiplier", "critMultiplier"), ("status_chance", "procChance")):
                if field in attack and attack[field] == base_attack.get(field) and source["damage"].get(source_key) is not None:
                    attack[field] = round(source["damage"][source_key], 6)
        if form in ("throw", "charged_throw") and source["glaive"]:
            projectile = source["glaive"]["turbo" if form == "charged_throw" else "projectile"]
            if projectile:
                attack["damage"] = damage_vector(projectile["amount"], attack["damage"], projectile["types"])
                for field, source_key in (("crit_chance", "crit"), ("crit_multiplier", "critMultiplier"),
                                          ("status_chance", "procChance"), ("projectile_speed_mps", "speed")):
                    attack[field] = round(projectile[source_key], 6)
                attack["forced_procs"] = [PROCS[p] for p in projectile["forcedProcs"] if p in PROCS and PROCS[p] not in ("lifted", "knockdown")]
                if isinstance(attack.get("combo_script"), list):
                    for h in attack["combo_script"]:
                        h.pop("forced_procs", None)
                recall = attack.get("radial")
                if recall:
                    recall["damage"] = damage_vector(projectile["explosion"] * projectile["heavyRecallAtten"], recall["damage"])
                    recall["forced_procs"] = [PROCS[p] for p in projectile["heavyRecallProcs"] if p in PROCS]
                    recall["radius_m"] = round(projectile["radius"], 6)
                    recall["crit_chance"] = attack["crit_chance"]
                    recall["crit_multiplier"] = attack["crit_multiplier"]
                    recall["status_chance"] = round(projectile["explosionProcChance"], 6)
                bounce = attack.get("thrown_bounce", {}).get("explosion")
                if bounce:
                    bounce["damage"] = damage_vector(projectile["explosion"], bounce["damage"])
                    bounce["radius_m"] = round(projectile["radius"], 6)
                    bounce["crit_chance"] = attack["crit_chance"]
                    bounce["crit_multiplier"] = attack["crit_multiplier"]
                    bounce["status_chance"] = round(projectile["explosionProcChance"], 6)
                    bounce["falloff_reduction"] = round(projectile["falloff"], 6)
        if root.get("class") == "glaive" and source["heavy"]["chargeDuration"] is not None:
            attack["throw_charge_seconds"] = source["heavy"]["chargeDuration"]
        if context in trees.get(tree, {}):
            attack["combo_script"] = {"tree": tree, "context": context}
            weapon_modes.append(w["id"])
        elif form in CONTEXTS:
            if form in ("neutral", "forward", "block", "block_forward") and context not in raw["trees"].get(tree, {}).get("contexts", {}):
                stance_only.append(w["id"])
            else:
                weapon_gaps.append([w["id"], "package context or attack events incomplete"])
            if form in ("neutral", "forward", "block", "block_forward") and not root.get("fixed_stance"):
                # An absent branch cannot inherit the parent's neutral input.
                attack["combo_script"] = []
        if not projectile_heavy and "fire_rate" in attack and form in (*CONTEXTS, "heavy_slam", "charged_throw") and source["attackSpeed"] > 0:
            attack["fire_rate"] = source["attackSpeed"]
        charge = source["heavy"]["chargeTime"]
        if not projectile_heavy and attack.get("heavy") and charge is not None:
            attack["heavy"]["windup_seconds"] = charge
            heavy_paths = trees.get(tree, {}).get("CC_GROUND_HEAVY", [])
            if heavy_paths and not any(p in GLAIVE_THROW for p in heavy_paths):
                attack["heavy"]["multiplier"] = sum(e["multiplier"] for e in actions[heavy_paths[0]]["events"] if e["kind"] == "sweep")
        if attack != original or isinstance(attack.get("combo_script"), dict):
            text = replace_mapping(path.read_text(), "attack", attack)
            if "# see notes: melee_shared_package_actions" not in text:
                text = "# see notes: melee_shared_package_actions\n" + text
            if isinstance(attack.get("combo_script"), list) and attack["combo_script"] and "M95" not in text:
                if "see notes: combo_points_from_multiplier" not in text:
                    text = "# see notes: combo_points_from_multiplier\n" + text
            if attack.get("spectral_dagger") and "see notes: okina_spectral_dagger" not in text:
                text = "# see notes: okina_spectral_dagger\n" + text
            if isinstance(attack.get("combo_script"), dict):
                old = w.get("unmodeled", [])
                new = [r for r in old if not (isinstance(r, str) and r.startswith("One attack input's animation length is unpublished"))
                       and not (isinstance(r, dict) and ((r.get("reason") in ("heavy_charge_share_assumed", "melee_heavy_total_disputed") and form == "heavy")
                                or (r.get("reason") == "glaive_throw_animation_assumed" and form == "charged_throw")
                                or (r.get("reason") == "melee_slide_unpublished" and form == "slide")))]
                if old != new:
                    text = replace_mapping(text, "unmodeled", new)
            writes[path] = text
        elif isinstance(attack.get("combo_script"), list) and attack["combo_script"] and "M95" not in path.read_text():
            text = path.read_text()
            if "see notes: combo_points_from_multiplier" not in text:
                writes[path] = "# see notes: combo_points_from_multiplier\n" + text
    loops = {t: {c: 1 for c, paths in ctx.items() if paths == GLAIVE_THROW} for t, ctx in trees.items()}
    referenced = {path for contexts in trees.values() for paths in contexts.values() for path in paths}
    catalog_actions = {path: a for path, a in actions.items() if path in referenced}
    catalog = {"actions": catalog_actions, "trees": trees, "geometry": geometry,
               "repeat_from_input": {t: ctx for t, ctx in loops.items() if ctx}}
    report = {"build": raw["buildLabel"], "source": str(SOURCE.relative_to(ROOT)),
              "actions": len(catalog_actions), "available_actions": len(actions), "trees": len(trees), "migrated_stance_modes": migrated,
              "aliases": aliases, "retained_stance_modes": retained,
              "unavailable_actions": invalid, "unmodelled_fields": omissions,
              "migrated_weapon_modes": weapon_modes, "retained_weapon_modes": weapon_gaps,
              "bare_inputs_requiring_a_stance": stance_only, "bare_input_aliases": bare_aliases}
    # JSON syntax is valid YAML and lets runtime decode without a YAML document.
    writes[CATALOG] = "# see notes: melee_shared_package_actions\n# see notes: melee_package_timing\n" + json.dumps(catalog, indent=2) + "\n"
    writes[REPORT] = json.dumps(report, indent=2) + "\n"
    changed = [p for p, text in writes.items() if not p.exists() or p.read_text() != text]
    differences = [str(p.relative_to(ROOT)) for p in changed]
    if args.check and differences:
        raise SystemExit("melee package differs: " + ", ".join(differences))
    if args.apply:
        for path in changed:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(writes[path])
    print(json.dumps({"actions": len(catalog_actions), "available_actions": len(actions), "trees": len(trees), "stance_modes": len(migrated),
                      "aliases": len(aliases), "retained": len(retained), "weapon_modes": len(weapon_modes),
                      "weapon_gaps": len(weapon_gaps), "stance_only_inputs": len(stance_only), "changed_files": len(differences),
                      "bare_aliases": len(bare_aliases),
                      "unavailable_reasons": dict(Counter(invalid.values()))}, indent=2))


if __name__ == "__main__":
    main()
