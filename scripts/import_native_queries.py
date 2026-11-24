# SPDX-License-Identifier: AGPL-3.0-or-later
"""Project pinned client replay fields into the geometry tests."""
import argparse
from hashlib import sha256
import io
import json
import math
from pathlib import Path
import sys
import zipfile


PINS = {
    "eye": ("20261116T094904Z-shooter-3d-origins", "c938a7e8ef08d58fba86bd3c1c8fea18062015000bee7cc8fed9c6e521b0dc13"),
    "selection": ("20261118T145138Z-native-homing-ownership", "b8c0a59ae87801ce7028c428fa751e0a8453ae6c9b2f68ce7f2c2b4caed7d860"),
    "motion": ("20261119T043220Z-glaive-motion-queries", "f4489f4039a59a68d66395b08cbf050ae715f07e4c1d44fac77fce736da8405e"),
    "flight": ("20261119T055143Z-native-3d-answers-hardware", "f625babe3d4da2364e4d6ba195b83036d5bcb050a892ed8bf8c7446d96dcf2bd"),
}


def pinned_member(archive, record):
    data = archive.read(record["path"])
    assert len(data) == record["bytes"]
    assert sha256(data).hexdigest() == record["sha256"], record["path"]
    return json.loads(data)


def fixtures_for(batches, name):
    batch, sha = PINS[name]
    root = batches / batch
    manifest = (root / "manifest.json").read_bytes()
    assert sha256(manifest).hexdigest() == sha
    assert json.loads((root / "READY.json").read_bytes())["manifest_sha256"] == sha
    manifest = json.loads(manifest)
    filename = "client-evidence.zip"
    metadata = next(item for item in manifest["files"] if item["path"] == filename)
    raw = (root / filename).read_bytes()
    assert len(raw) == metadata["bytes"] and sha256(raw).hexdigest() == metadata["sha256"]
    records = {item["path"]: item for item in manifest["archive_members"][filename]}
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        if name == "flight":
            return [pinned_member(archive, records[path]) for path in sorted(records) if path.startswith("flight-") and path.endswith(".json")]
        path = "eye/index.json" if name == "eye" else "index.json"
        return pinned_member(archive, records[path])


def query_fields(q, fields):
    return {key: q[key] for key in fields}


def closer_candidate_control(exe, ownership_scripts):
    sys.path.insert(0, str(ownership_scripts))
    import audit_native_homing_ownership as ownership

    ownership.native.initialize(exe)
    actors = [{"root": (0.7, 0., 4.92)}, {"root": (0., 0., 4.99)}]
    assert math.dist((0., 0., 0.), actors[0]["root"]) < actors[1]["root"][2]
    scene = ownership.Scene(actors, [0, 1])
    selected = scene.selected_index(ownership.native.seek())
    assert selected == 1
    return {
        "name": "nearer-euclidean-distance-loses-to-better-cone-score",
        "actors": actors, "options": {}, "candidates": scene.list_calls,
        "queries": scene.query_calls, "selected": selected,
        "steering_point": ownership.native.get(ownership.native.obj + 0x1740, "<4f"),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("batches", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--exe", type=Path, required=True)
    parser.add_argument("--ownership-scripts", type=Path, required=True)
    args = parser.parse_args()
    eye = [c["native"] for c in fixtures_for(args.batches, "eye")["cases"]]
    selection = fixtures_for(args.batches, "selection")["selection"]
    selection.append(closer_candidate_control(args.exe, args.ownership_scripts))
    motion = []
    for case in fixtures_for(args.batches, "motion")["cases"]:
        motion.append({
            "input": case["input"],
            "queries": [query_fields(q, ("start", "end", "supplied_records", "backend_success")) for q in case["queries"]],
            "impact": None if case["first_impact_dispatch"] is None else case["first_impact_dispatch"]["point"],
        })
    flight = []
    for case in fixtures_for(args.batches, "flight"):
        flight.append({
            **query_fields(case, ("enemy", "supplied_dt", "supplied_locked", "first_impact", "actors")),
            "steps": [{
                **query_fields(step, ("step", "motion_displacement")),
                "start": step["before"]["retained1750"],
                "orientation": step["before"]["world_quaternion"],
                "queries": [{**query_fields(q, ("end", "chosen_actor")),
                             "proxy_index": q["body_query"].get("proxy_index"),
                             "nearest_proxy_set": q["body_query"].get("portable_nearest_within_tolerance")}
                            for q in step["queries"]],
            } for step in case["steps"]],
        })
    output = dict(source_pins=PINS, exe_sha256=sha256(args.exe.read_bytes()).hexdigest(),
                  eye=eye, selection=selection, motion=motion, flight=flight)
    args.output.write_text(json.dumps(output, separators=(",", ":"), allow_nan=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
