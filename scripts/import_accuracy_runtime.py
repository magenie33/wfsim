#!/usr/bin/env python3
"""Project positioned firing profiles from pinned client accuracy and weapon records."""
import argparse
import hashlib
import json
import re
import struct
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "tests/fixtures/client"
OUTPUT = ROOT / "data/geometry/accuracy-runtime.yaml"

ROSTER = "20261109T053558Z-weapon-roster-dossiers"
ROSTER_PIN = "7b830c6730e20621029d67d9d26e9aa8a216c998c32de73e2e22fe187c979d91"
OFFLINE = "20261106T150853Z-client-metadata-offline"
OFFLINE_PIN = "dbeb51209c791af1692f91f087826805b8f99198ee0958ec4606170ab2badea2"
AIM = "20261109T023827Z-beam-bounce-aim"
AIM_PIN = "7cd3c449c71f1ff509047651eab60b32d1c77a6e0d4e6d6322381c08c98a4f4b"
SAMPLER = "20261109T025857Z-spread-sampling"
SAMPLER_PIN = "e5351a0d64a5b31f5c3b3267184b30bbbe0d47e6ce63b78fd3fb388cb28ab70e"
CODA = "20261111T080715Z-coda-runtime-queue-followup"
CODA_PIN = "1713c05a89450c02e906d56203267e1349d5b0d3960046b9706dd7a97a798931"
ACC_SPREAD = "20261128T044717Z-negative-accuracy-beam-forking-and-weapon-classes"
ACC_SPREAD_PIN = "63cc60005b87b3c54e00ef3243fad470a0f10ba7f3ca7899082f3bee53a935df"


def fields(text):
    lines, out, i = text.splitlines(), {}, 0
    while i < len(lines):
        s = lines[i].strip()
        i += 1
        m = re.match(r"([\w:]+)=(.*)", s)
        if not m:
            continue
        depth = s.count("{") - s.count("}")
        while depth > 0:
            line = lines[i]
            i += 1
            s += "\n" + line.strip()
            depth += line.count("{") - line.count("}")
        out[m[1]] = s.partition("=")[2]
    return out


def blocks(s):
    depth = 0
    for i, c in enumerate(s):
        if c == "{":
            depth += 1
            if depth == 2:
                start = i + 1
        elif c == "}":
            if depth == 2:
                yield s[start:i]
            depth -= 1


def canonical(token):
    raw = struct.pack("<f", float(token))
    value = struct.unpack("<f", raw)[0]
    for digits in range(1, 10):
        text = format(value, f".{digits}g")
        if struct.pack("<f", float(text)) == raw:
            return float(text)
    raise ValueError(token)


def inner(raw):
    return fields(raw.strip()[1:-1])


def pair(raw):
    return [canonical(v) for v in raw.strip("{}").split(",")]


def roles(raw):
    parsed = fields(raw)
    return (
        {
            kind: inner(v)
            for kind in ("fire", "impact", "state")
            for k, v in parsed.items()
            if k.startswith(kind + ":") and not k.endswith(":Type")
        },
        {k: v for k, v in parsed.items() if k.endswith(":Type")},
    )


def parse_rings(raw):
    if not raw:
        return []
    out = []
    for blk in blocks(raw):
        parsed = fields(blk)
        if "min" in parsed and "max" in parsed:
            out.append([int(parsed["min"]), int(parsed["max"])])
    return out


def catalog_forms():
    entries = {}
    for path in sorted((ROOT / "data/weapons").rglob("*.yaml")):
        text = path.read_text()
        m_id = re.search(r"(?m)^id:\s*(\S+)", text)
        if not m_id:
            continue
        wid = m_id.group(1)
        m_form = re.search(r"(?m)^form:\s*(\S+)", text)
        m_inh = re.search(r"(?m)^inherits:\s*(\S+)", text)
        entries[wid] = {
            "id": wid,
            "form": m_form.group(1) if m_form else "base",
            "group": m_inh.group(1) if m_inh else wid,
        }
    by_group_form = {}
    for wid, meta in entries.items():
        by_group_form.setdefault((meta["group"], meta["form"]), []).append(wid)
    return entries, by_group_form


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exchange", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    batches = args.exchange / "batches"
    coda_pin = "1713c05a89450c02e906d56203267e1349d5b0d3960046b9706dd7a97a798931"
    for folder, expected in [
        (AIM, AIM_PIN),
        (ROSTER, ROSTER_PIN),
        (OFFLINE, OFFLINE_PIN),
        (SAMPLER, SAMPLER_PIN),
        (CODA, coda_pin),
        (ACC_SPREAD, ACC_SPREAD_PIN),
    ]:
        actual_sha = hashlib.sha256((batches / folder / "manifest.json").read_bytes()).hexdigest()
        assert actual_sha == expected, (folder, actual_sha, expected)

    with zipfile.ZipFile(batches / AIM / "beam-bounce-aim-evidence.zip") as zf:
        ctor = zf.read("native/accuracy-constructor--00.asm.txt")
        scopes = json.loads(zf.read("aim-property-scopes.json"))
    assert all(token in ctor for token in (
        b"xor      eax, eax", b"[rbx + 0x30], al", b"[rbx + 0x34], rax",
        b"[rbx + 0x60], 0x3f800000", b"[rbx + 0x64], 0x3f800000",
    ))
    profiles = {(row["path"], row["member_path"]): row["members"] for row in scopes}

    with zipfile.ZipFile(batches / ROSTER / "weapon-roster-dossiers.zip") as zf:
        objects = {
            row["path"]: row
            for line in zf.read("shared/objects.jsonl").splitlines()
            if line and (row := json.loads(line))
        }
    with zipfile.ZipFile(batches / OFFLINE / "data.zip") as zf:
        extra = [
            json.loads(line)
            for line in zf.read("packages_owntext.jsonl").splitlines()
            if line
        ]
    for obj in extra:
        if obj["path"] not in objects:
            objects[obj["path"]] = {
                "path": obj["path"],
                "parent": obj.get("parent"),
                "composed_fields": obj.get("fields", {}),
                "ancestry_leaf_first": [obj["path"]],
            }
    for obj in extra:
        target = objects[obj["path"]]
        cur = target.get("parent")
        seen = set(target["ancestry_leaf_first"])
        while cur and cur in objects and cur not in seen:
            target["ancestry_leaf_first"].append(cur)
            seen.add(cur)
            cur = objects[cur].get("parent")

    with zipfile.ZipFile(batches / SAMPLER / "spread-sampling-evidence.zip") as zf:
        assert b"[r15 + 0x8ac]" in zf.read("native/projectile-sampler--01.asm.txt")
    with zipfile.ZipFile(batches / CODA / "client-evidence.zip") as zf:
        coda_preparation = json.loads(zf.read("emission-preparation.json"))
        assert coda_preparation["failed"] == 0 and coda_preparation["passed"] > 0
    with zipfile.ZipFile(batches / ACC_SPREAD / "evidence.zip") as zf:
        beam_modes = json.loads(zf.read("beam-spread-forking-roster.json"))["modes"]
        firearm_forms = json.loads(zf.read("firearm-multishot-cone-roster.json"))["forms"]

    entries, by_group_form = catalog_forms()
    attacks = {}
    used_profiles = {}

    def record_timing(attack, actual, info=None):
        bounds = {}
        for mode, key in (("aimed", "AIMED_ACCURACY"), ("hip", "HIP_FIRE_ACCURACY")):
            position = actual.get(key)
            if position not in objects:
                continue
            spread = {}
            shooting = {}
            for ancestor in reversed(objects[position]["ancestry_leaf_first"]):
                prop = objects[ancestor]["composed_fields"].get("Spread")
                if prop:
                    layer = inner(prop.partition("=")[2])
                    spread.update(layer)
                    if "SHOOTING" in layer:
                        shooting.update(inner(layer["SHOOTING"]))
            if not spread:
                spread = profiles.get((position, ".Spread"), {})
            range_ = pair(shooting.get("range", "{0,0}"))
            bounds[mode] = range_
            if info and mode in info:
                assert range_ == info[mode]["range"], position
            rate = float(shooting.get("rate", "0"))
            spread_type = shooting.get("type", "ST_LINEAR")
            if range_[0] != range_[1] and rate and spread_type in {"ST_EXPONENTIAL", "ST_LINEAR"}:
                used_profiles[position] = {
                    "rate": rate,
                    "recovery_rate": float(spread.get("recoveryRate", "1")),
                    "recovery_delay": float(spread.get("recoveryDelay", "1")),
                    "exponential": spread_type == "ST_EXPONENTIAL",
                }
                attack[mode] = position
        return bounds

    for family in ("primary", "secondary"):
        path = FIXTURES / f"{family}-calibration-2026-10-28.json"
        for row in json.loads(path.read_bytes())["forms"]:
            weapon = row["id"]
            type_ = row.get("role_types", {}).get("fire:Type", "")
            info = row.get("accuracy", {})
            if "behavior_index" not in row or row["owner"] not in objects:
                continue
            beh_field = objects[row["owner"]]["composed_fields"].get("Behaviors", "")
            if not beh_field:
                for anc in objects[row["owner"]]["ancestry_leaf_first"]:
                    if "Behaviors" in objects[anc]["composed_fields"]:
                        beh_field = objects[anc]["composed_fields"]["Behaviors"]
                        break
            raw = list(blocks(beh_field.partition("=")[2]))
            if row["behavior_index"] >= len(raw):
                continue
            typed, types = roles(raw[row["behavior_index"]])
            assert types.get("fire:Type") == type_, weapon
            actual = typed["fire"]
            if "Beam" in type_ and (actual.get("UseBeamSpread") == "1" or actual.get("UseFanSpread") == "1"):
                continue
            if "Projectile" not in type_ and "Trace" not in type_ and "Beam" not in type_:
                continue
            fan = actual.get("UseFanSpread") == "1"
            charge = actual.get("AttenuateSpreadForCharge") == "1"
            primary = row["applied"].get("attack.spread", {})
            sampled = "sampling" in primary

            attack = {}
            bounds = record_timing(attack, actual, info)

            if not sampled and len(bounds) != 2:
                continue
            if fan or charge:
                if len(bounds) != 2 or "Projectile" not in type_:
                    continue
                if charge:
                    lower = actual.get("MinChargeSpreadAtten")
                    upper = actual.get("MaxChargeSpreadAtten")
                    if lower is None or upper is None or row["form"] not in ("base", "charged"):
                        continue
                    if row["form"] == "base" and not weapon.endswith("_uncharged"):
                        continue
                    attack["charge_attenuation"] = float(upper if row["form"] == "charged" else lower)
                if fan:
                    assert pair(actual["chargeRotation"]) == [0, 0], weapon
                    start = float(actual["StartAngle"])
                    end = float(actual["EndAngle"])
                    assert start != end, weapon
                    attack["fan"] = {
                        "start_deg": start,
                        "end_deg": end,
                        "vertical": actual["FanSpreadVertical"] == "1",
                    }
            if not sampled and weapon != "miter":
                attack["resting"] = {
                    "min_deg": bounds["aimed"][0],
                    "max_deg": bounds["aimed"][1],
                    "sampling": {
                        "horizontal_ratio": float(actual.get("SpreadRatioHorizontal", "0.5")),
                        "uniform": actual.get("spreadUniform") == "1",
                        "hip_min_deg": bounds["hip"][0],
                        "hip_max_deg": bounds["hip"][1],
                    },
                }
            if attack and actual.get("SpreadInverse") == "1":
                attack["inverse"] = True
            if attack:
                attacks[weapon] = attack

    # Join ACC-SPREAD-01 non-beam firearm flags across all 524 forms (including archgun, kitgun, sentinel).
    for row in firearm_forms:
        candidates = by_group_form.get((row["weapon_id"], row["form_id"]), [])
        if not candidates and row["weapon_id"] in entries:
            candidates = [row["weapon_id"]]
        if not candidates:
            continue
        fl = row["flags"]
        owner = row["native_owner"]
        beh_blocks = []
        if owner in objects:
            beh_field = ""
            for anc in objects[owner]["ancestry_leaf_first"]:
                if "Behaviors" in objects[anc]["composed_fields"]:
                    beh_field = objects[anc]["composed_fields"]["Behaviors"]
                    break
            beh_blocks = [roles(b)[0].get("fire", {}) for b in blocks(beh_field.partition("=")[2])]
        matched_fire = None
        for fb in beh_blocks:
            if (fb.get("HIP_FIRE_ACCURACY") == row["HIP_FIRE_ACCURACY"]
                    and fb.get("AIMED_ACCURACY") == row["AIMED_ACCURACY"]):
                matched_fire = fb
                break
        if matched_fire is None and beh_blocks:
            matched_fire = beh_blocks[0]

        for wid in candidates:
            attack = dict(attacks.get(wid, {}))
            if matched_fire and wid not in attacks:
                record_timing(attack, matched_fire)
                if matched_fire.get("SpreadInverse") == "1":
                    attack["inverse"] = True
            if fl["MultishotAmmoSpread_0x7d0"]:
                attack["extra_floor"] = float(fl["MultishotMinSpread_0x7d4"])
            if fl["MultishotUseConeAngle_0x2fd"]:
                attack["multishot_cone"] = {
                    "cone_deg": float(fl["MultishotConeAngle_0x300"]),
                    "offset_deg": float(fl["MultishotConeAngleOffset_0x304"]),
                }
            if fl["IgnoreSpreadOnFirstFireIteration_0xa0c"]:
                attack["first_pellet_pinpoint"] = True
            if fl["UseFanSpread_0x8a0"] and "fan" not in attack and matched_fire:
                start = float(matched_fire.get("StartAngle", "0"))
                end = float(matched_fire.get("EndAngle", "0"))
                if start != end:
                    attack["fan"] = {
                        "start_deg": start,
                        "end_deg": end,
                        "vertical": matched_fire.get("FanSpreadVertical") == "1",
                    }
            if fl["SpreadExtraProjectiles_0x9e8"] and not fl["UseFanSpread_0x8a0"]:
                rings = parse_rings(matched_fire.get("ProjectileRings", "")) if matched_fire else []
                assert len(rings) <= 4, (wid, rings)
                rot_offset = float(matched_fire.get("SpreadRotationOffset", "0")) if matched_fire else 0.0
                attack["extra_spread"] = {
                    "uniform": bool(fl["spreadUniform_0x1a8"]),
                    "extra_deg": float(fl["ExtraSpread_0xa00"]),
                    "multiplier": float(fl["SpreadMultiplier_0xa04"]),
                    "rotation_offset_deg": rot_offset,
                    "rings": rings,
                }
            if attack:
                attacks[wid] = attack

    # Join ACC-SPREAD-01 continuous beam spread sub-regimes (Regimes B, C, D).
    for mode in beam_modes:
        wid = mode["id"]
        if wid not in entries:
            continue
        fl = mode["spread_and_beam_flags"]
        bg = mode["beam_geometry_and_pt"]
        attack = dict(attacks.get(wid, {}))
        if fl["mUseBeamSpread_0x1100"]:
            children = mode["visual_beam_slots"]["BeamChildren"]
            assert len(children) == 4, wid
            attack["beam_mode"] = {
                "kind": "child_quat",
                "horizontal_ratio": float(fl["mSpreadRatioHorizontal_0x1b4"]),
                "quats": [c["LocalRotation"] for c in children],
            }
        elif fl["mUseFanSpread_0x1101"]:
            aim_total = float(mode["accuracy_profiles"]["AIMED_ACCURACY"]["total_min_deg"])
            attack["beam_mode"] = {
                "kind": "fan",
                "base_deg": aim_total,
                "base_rays": int(round(float(bg["fireIterations"]))),
                "atten_max": float(fl["mAttenuationAtMaxCharge_0x1118"]),
                "expansion_time": float(fl["mExpansionTime_0x10f4"]),
                "reverse": bool(fl["mReverseSpread_0x110c"]),
                "force_loop": bool(fl["mForceFireLoop_0x1115"]),
                "inner_ring": bool(fl["mUseInnerRing_0x1120"]),
                "horizontal_ratio": float(fl["mSpreadRatioHorizontal_0x1b4"]),
            }
        elif fl["mUseSmoothedAccuracy_0x110d"] and float(fl["mSmoothTime_0x1128"]) > 0.0:
            attack["beam_mode"] = {
                "kind": "smoothed",
                "smooth_time": float(fl["mSmoothTime_0x1128"]),
            }
        if attack:
            attacks[wid] = attack

    profile_names = sorted({a[mode] for a in attacks.values() for mode in ("aimed", "hip") if mode in a})
    profile_index = {name: idx for idx, name in enumerate(profile_names)}
    for attack in attacks.values():
        for mode in ("aimed", "hip"):
            if mode in attack:
                attack[mode] = profile_index[attack[mode]]
    runtime_profiles = [used_profiles[name] for name in profile_names]
    result = json.dumps({"profiles": runtime_profiles, "attacks": attacks}, sort_keys=True, separators=(",", ":")) + "\n"
    if args.check:
        assert OUTPUT.read_text() == result, OUTPUT
    else:
        OUTPUT.write_text(result)
    print(f"{len(attacks)} firing profiles joined across {len(runtime_profiles)} client accuracy properties")


if __name__ == "__main__":
    main()
