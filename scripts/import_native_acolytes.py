# SPDX-License-Identifier: AGPL-3.0-or-later
"""Project the retained Acolyte poses and source controller into 3D scene inputs."""

import argparse
from hashlib import sha256
import json
from pathlib import Path
from zipfile import ZipFile

from import_native_queries import pinned_member

REQUEST = "20261120T173808Z-wfsim-3d-roster-migration"
REQUEST_PIN = "1216005068033ea4564770022a8118ca061d422e2e5cc957e90e014c8eee0ea8"
RIG_BATCH = "20261120T092528Z-warframe-geometry-roster-source-controls"
RIG_PIN = "21a8eb6266ed0275e60f2e95478bcdc582e81a4314dd90c1b04e106ee57affb8"
METADATA = "20261106T150853Z-client-metadata-offline"
METADATA_PIN = "dbeb51209c791af1692f91f087826805b8f99198ee0958ec4606170ab2badea2"
CONTROLLER = "/Lotus/Types/Enemies/Stalker/StalkerWalkAttribs"
CONTROLLER_PARENT = "/EE/Types/Physics/WalkingAttribs"
PARTS = ("TORSO", "HEAD", "ARM_LEFT", "ARM_RIGHT", "LEG_LEFT", "LEG_RIGHT")
IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0]


def pinned_manifest(root, pin):
    raw = (root / "manifest.json").read_bytes()
    assert sha256(raw).hexdigest() == pin
    assert json.loads((root / "READY.json").read_bytes()) == {
        "batch_id": root.name, "manifest_sha256": pin,
    }
    return json.loads(raw)


def record_named(records, name):
    return next(r for r in records if r["path"] == name)


def checked_file(root, manifest, name):
    file = root / name
    record = record_named(manifest["files"], name)
    with file.open("rb") as stream:
        digest = sha256()
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    assert file.stat().st_size == record["bytes"]
    assert digest.hexdigest() == record["sha256"]
    return file


def point(transform):
    return [transform[i] for i in (3, 7, 11)]


def controller(ex):
    root = ex / "batches" / METADATA
    manifest = pinned_manifest(root, METADATA_PIN)
    name = "packages_owntext.jsonl"
    record = record_named(manifest["archive_members"]["data.zip"], name)
    objects = {}
    with ZipFile(checked_file(root, manifest, "data.zip")) as arc, arc.open(name) as src:
        digest = sha256()
        for line in src:
            digest.update(line)
            path = line.split(b'"', 4)[3].decode()
            if path in (CONTROLLER_PARENT, CONTROLLER):
                objects[path] = json.loads(line)
        assert digest.hexdigest() == record["sha256"]
    assert objects[CONTROLLER]["parent"] == CONTROLLER_PARENT
    parent, child = (objects[p]["ownText"].splitlines() for p in (CONTROLLER_PARENT, CONTROLLER))
    fields = dict(line.split("=", 1) for line in parent + child if "=" in line)
    return {k: float(fields[v]) for k, v in
            (("height", "Height"), ("radius", "CapsuleRadius"), ("skin", "SkinWidth"))}


def read_profiles(ex):
    root = ex / "requests" / REQUEST
    manifest = pinned_manifest(root, REQUEST_PIN)
    source = json.loads(checked_file(root, manifest, "additional-target-geometry.json").read_bytes())
    rig_root = ex / "batches" / RIG_BATCH
    rig_manifest = pinned_manifest(rig_root, RIG_PIN)
    arc_name = "client-evidence.zip"
    rig_member = record_named(rig_manifest["archive_members"][arc_name], "rigs/models.json")
    assert source["rig_batch"] == RIG_BATCH and source["rig_member"] == rig_member
    with ZipFile(checked_file(rig_root, rig_manifest, arc_name)) as archive:
        rigs = pinned_member(archive, rig_member)
    cct = controller(ex)
    profiles = {}
    for target in source["targets"]:
        if "native_source_default_proxy_shapes" not in target:
            continue
        assert target["physics_behavior"] == "/Lotus/Types/Enemies/Stalker/StalkerWalk"
        bones = {b["name"]: b["index"] for b in rigs[target["mesh"]]["bones"]}
        rows = target["native_source_default_pose"]["native_rows"]
        records = target["native_source_default_proxy_shapes"]["native_capsule_records"]
        declarations = target["declared_hit_proxies"]
        assert len(records) == len(declarations) == 10
        proxies = []
        for index, (made, declared) in enumerate(zip(records, declarations)):
            bone = declared["attachment_fields"]["BoneName"]
            tags = declared["proxy_fields"]
            assert index == made["source_proxy_index"] == declared["index"]
            assert tags["Type"] == "CAPSULE"
            proxies.append({
                "cache_index": index, "part": PARTS.index(tags.get("Part", "TORSO")),
                "bone_token": bones[bone] + 1, "world": made["native_world_rows"],
                "shape": {"type": "capsule", "radius": made["radius"], "height": 2 * made["half_height"]},
                "ray_enabled": True,
            })
        assert len(proxies) == 10 and [p["part"] for p in proxies[:2]] == [1, 0]
        profiles[target["enemy_id"]] = {
            "head_proxy": 0, "body_proxy": 1,
            "model": {
                "world": IDENTITY, "proxies": proxies,
                "controller": dict(cct, scale=float(target["mesh_scale_declaration"])),
                "candidate_anchor": point(proxies[0]["world"]),
                "steering_center": point(rows[bones["GAME_C1_HIP1"]]),
            },
        }
    assert set(profiles) == {"angst", "mania", "misery", "torment", "violence"}
    return profiles


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exchange", type=Path, required=True)
    parser.add_argument("--out", type=Path, default=Path(__file__).resolve().parents[1]
                        / "engine/src/rules/geometry/fixtures/native-acolytes.json")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    output = json.dumps(read_profiles(args.exchange), separators=(",", ":"), allow_nan=False) + "\n"
    if args.check:
        assert args.out.read_text() == output
    else:
        args.out.write_text(output)


if __name__ == "__main__":
    main()
