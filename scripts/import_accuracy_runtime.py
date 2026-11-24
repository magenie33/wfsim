#!/usr/bin/env python3
"""Project positioned firing profiles from pinned client accuracy and weapon records."""
import argparse
import hashlib
import json
from pathlib import Path

from import_primary_calibration import FIXTURES, ROOT, ROSTER, PINS, blocks, inner, pair, roles, source

AIM = "20261109T023827Z-beam-bounce-aim"
AIM_PIN = "7cd3c449c71f1ff509047651eab60b32d1c77a6e0d4e6d6322381c08c98a4f4b"
SAMPLER = "20261109T025857Z-spread-sampling"
SAMPLER_PIN = "e5351a0d64a5b31f5c3b3267184b30bbbe0d47e6ce63b78fd3fb388cb28ab70e"
CODA = "20261111T080715Z-coda-runtime-queue-followup"
CODA_PIN = "1713c05a89450c02e906d56203267e1349d5b0d3960046b9706dd7a97a798931"
OUTPUT = ROOT / "data/geometry/accuracy-runtime.yaml"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exchange", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    batches = args.exchange / "batches"
    for folder, expected in [(AIM, AIM_PIN), (ROSTER, PINS[ROSTER]), (SAMPLER, SAMPLER_PIN), (CODA, CODA_PIN)]:
        assert hashlib.sha256((batches / folder / "manifest.json").read_bytes()).hexdigest() == expected, folder

    payload, _ = source(batches / AIM, "beam-bounce-aim-evidence.zip", [
        "aim-property-scopes.json", "native/accuracy-constructor--00.asm.txt",
    ])
    # WeaponAccuracyInfo constructs a zeroed linear shooting channel; its
    # recovery speed and delay at +0x60/+0x64 both start at float32 one.
    ctor = payload["native/accuracy-constructor--00.asm.txt"]
    assert all(token in ctor for token in (
        b"xor      eax, eax", b"[rbx + 0x30], al", b"[rbx + 0x34], rax",
        b"[rbx + 0x60], 0x3f800000", b"[rbx + 0x64], 0x3f800000",
    ))
    scopes = json.loads(payload["aim-property-scopes.json"])
    profiles = {(row["path"], row["member_path"]): row["members"] for row in scopes}
    payload, _ = source(batches / ROSTER, "weapon-roster-dossiers.zip", ["shared/objects.jsonl"])
    objects = {row["path"]: row
               for line in payload["shared/objects.jsonl"].splitlines() if (row := json.loads(line))}
    payload, _ = source(batches / SAMPLER, "spread-sampling-evidence.zip", [
        "native/projectile-sampler--01.asm.txt",
    ])
    assert b"[r15 + 0x8ac]" in payload["native/projectile-sampler--01.asm.txt"]
    payload, _ = source(batches / CODA, "client-evidence.zip", ["emission-preparation.json"])
    coda_preparation = json.loads(payload["emission-preparation.json"])
    assert coda_preparation["failed"] == 0 and coda_preparation["passed"] > 0

    attacks = {}
    used_profiles = {}
    for family in ("primary", "secondary"):
        path = FIXTURES / f"{family}-calibration-2026-10-28.json"
        for row in json.loads(path.read_bytes())["forms"]:
            weapon = row["id"]
            type_ = row.get("role_types", {}).get("fire:Type", "")
            info = row.get("accuracy", {})
            if "behavior_index" not in row or row["owner"] not in objects:
                continue
            # The form and behavior index were joined in the primary/secondary intake.
            raw = list(blocks(objects[row["owner"]]["composed_fields"]["Behaviors"].partition("=")[2]))
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
                if mode in info:
                    assert range_ == info[mode]["range"], (weapon, position)
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

            if not sampled and (len(bounds) != 2 or not any(any(bounds[mode]) for mode in bounds)):
                continue
            if fan or charge:
                if len(bounds) != 2 or "Projectile" not in type_:
                    continue
                if charge:
                    lower = actual.get("MinChargeSpreadAtten")
                    upper = actual.get("MaxChargeSpreadAtten")
                    if lower is None or upper is None or row["form"] not in ("base", "charged"):
                        continue
                    # These separate catalog forms schedule released and full-charge inputs.
                    if row["form"] == "base" and not weapon.endswith("_uncharged"):
                        continue
                    attack["charge_attenuation"] = float(upper if row["form"] == "charged" else lower)
                if fan:
                    assert pair(actual["chargeRotation"]) == [0, 0], weapon
                    start = float(actual["StartAngle"])
                    assert start != 0, weapon
                    attack["fan"] = {"start_deg": start, "end_deg": float(actual["EndAngle"]),
                                     "vertical": actual["FanSpreadVertical"] == "1"}
            if not sampled:
                attack["resting"] = {
                    "min_deg": bounds["aimed"][0], "max_deg": bounds["aimed"][1],
                    "sampling": {
                        "horizontal_ratio": float(actual.get("SpreadRatioHorizontal", "0.5")),
                        "uniform": actual.get("spreadUniform") == "1",
                        "hip_min_deg": bounds["hip"][0], "hip_max_deg": bounds["hip"][1],
                    },
                }
            if attack and actual.get("SpreadInverse") == "1":
                attack["inverse"] = True
            if weapon == "coda_bubonico":
                assert actual.get("SpreadInverse") == "1"
                assert actual.get("SpreadExtraProjectiles") == "1"
                assert actual.get("spreadUniform") == "0"
                assert actual.get("MultishotAmmoSpread") == "1"
                assert not fan
                attack["extra_floor"] = float(actual["MultishotMinSpread"])
            if attack:
                attacks[weapon] = attack

    # Keep the large table readable without depending on a YAML formatter version.
    referenced = {a[mode] for a in attacks.values() for mode in ("aimed", "hip") if mode in a}
    runtime_profiles = {name: used_profiles[name] for name in referenced}
    result = json.dumps({"profiles": runtime_profiles, "attacks": attacks}, sort_keys=True, indent=2) + "\n"
    if args.check:
        assert OUTPUT.read_text() == result, OUTPUT
    else:
        OUTPUT.write_text(result)
    print(f"{len(attacks)} firing profiles joined across {len(runtime_profiles)} client accuracy properties")


if __name__ == "__main__":
    main()
