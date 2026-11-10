#!/usr/bin/env python3
"""Bind the Coda Bubonico scope to the closed matching-client dossier."""
import argparse
import json
from pathlib import Path

from import_client_cadence import source, blocks
from import_primary_calibration import roles, ROSTER, PINS

ROOT = Path(__file__).resolve().parents[1]
OWNER = "/Lotus/Weapons/Infested/InfestedLich/LongGuns/CodaBubonico/CodaBubonicoCannon"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exchange", required=True, type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    names = ["shared/objects.jsonl", "weapons/coda_bubonico/dossier.json"]
    payload, binding = source(args.exchange / "batches" / ROSTER, "weapon-roster-dossiers.zip", names)
    assert binding["manifest_sha256"] == PINS[ROSTER]
    dossier = json.loads(payload[names[1]])
    assert dossier["identity"]["native_paths"] == [OWNER]
    owner = next(row for line in payload[names[0]].splitlines()
                 if (row := json.loads(line))["path"] == OWNER)
    typed, _ = roles(list(blocks(owner["composed_fields"]["Behaviors"].partition("=")[2]))[0])
    state, fire = typed["state"], typed["fire"]
    fixture = {
        "source": {**binding, "owner": OWNER},
        "primary_state": {k: state[k] for k in [
            "ContinuousFireRateAtten", "ContinuousMaxShots", "SpindownTime",
            "InitialFireIterations", "MaximumFireIterations", "FireIterationMode"]},
        "primary_fire": {k: fire[k] for k in [
            "SpreadInverse", "HIP_FIRE_ACCURACY", "AIMED_ACCURACY",
            "SpreadExtraProjectiles", "ExtraSpread", "SpreadMultiplier", "MultiShotProjectileType"]},
        "unverified_consumers": [
            "ContinuousMaxShots counter advancement and reset",
            "Per-form spread update, smoothing and emission scheduling",
            "Additional-multishot projectile spread and native seed correlations",
            "Projectile gravity and collision consumer",
        ],
    }
    path = ROOT / "tests/fixtures/client/coda-bubonico-scope-2026-10-28.json"
    text = json.dumps(fixture, indent=2) + "\n"
    if args.check:
        assert path.read_text() == text, f"{path}: differs from verified source"
    else:
        path.write_text(text)
    print(json.dumps({"owner": OWNER, "check": args.check,
                      "fixed_pellets": state["InitialFireIterations"] == state["MaximumFireIterations"],
                      "unverified_consumers": len(fixture["unverified_consumers"])}))


if __name__ == "__main__":
    main()
