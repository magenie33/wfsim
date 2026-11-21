# SPDX-License-Identifier: AGPL-3.0-or-later
"""Import the pinned Demolisher Devourer source-pose and body-query controls."""
import argparse
from hashlib import sha256
import io
import json
from pathlib import Path, PurePosixPath
import zipfile

from import_native_queries import pinned_member

BATCH = "20261121T015856Z-demolisher-devourer-source-geometry"
MANIFEST = "a878aa22020e8dc04ad650a2543767372562d08c570abd23fd3759bc1755848d"


def affine(columns):
    return [columns[column * 4 + row] for row in range(3) for column in range(4)]


def point(matrix):
    return [matrix[i] for i in (3, 7, 11)]


def shape(tokens):
    if tokens["type"] == 2:
        return {"type": "sphere", "radius": tokens["radius"]}
    assert tokens["type"] == 1
    return {"type": "capsule", "radius": tokens["radius"], "height": tokens["capsule_height"]}


def read_fixture(root):
    manifest_bytes = (root / "manifest.json").read_bytes()
    assert sha256(manifest_bytes).hexdigest() == MANIFEST
    ready = json.loads((root / "READY.json").read_bytes())
    assert ready["batch_id"] == root.name == BATCH and ready["manifest_sha256"] == MANIFEST
    manifest = json.loads(manifest_bytes)
    name = "evidence.zip"
    raw = (root / name).read_bytes()
    metadata = next(item for item in manifest["files"] if item["path"] == name)
    assert len(raw) == metadata["bytes"] and sha256(raw).hexdigest() == metadata["sha256"]
    members = {item["path"]: item for item in manifest["archive_members"][name]}
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        model, source, controls = (pinned_member(archive, members[key]) for key in
                                   ("model.json", "source-metadata.json", "fixture-controls.json"))
    enemy = source["enemy"]
    assert enemy["path"] == "/Lotus/Types/Enemies/Grineer/Disruption/Avatars/DemoDevourerAvatar"
    assert enemy["mesh"] == model["mesh"]
    bones = {bone["name"]: bone["index"] for bone in model["model"]["bones"]}
    assert len(bones) == 72
    native_rows = model["native_source_default_pose"]["native_rows"]
    assert len(native_rows) == len(bones)
    world = controls["fixtures"]
    local = next(fixture for fixture in world if fixture["supplied_scale"] == 1.0)
    assert affine(local["world_column_major"]) == [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0]
    proxies = []
    mine_slots = []
    heads = []
    for index, (record, declaration) in enumerate(zip(local["records"], enemy["declared_hit_proxies"], strict=True)):
        assert index == record["index"] == declaration["index"]
        assert record["bone"] == declaration["attachment_fields"]["BoneName"]
        if "/GhoulMine" in declaration["proxy_path"]:
            mine_slots.append(index)
        if declaration["proxy_fields"].get("Part") == "HEAD":
            heads.append(index)
        proxies.append({
            "cache_index": index, "bone_token": bones[record["bone"]] + 1, "part": record["part"],
            "world": record["world_rows"], "shape": shape(record["shape"]), "ray_enabled": record["enabled"],
        })
    assert len(proxies) == 15 and heads == [0, 11] and mine_slots == [12, 13, 14]
    physics = source["objects"][enemy["physics_behavior"]]["declared_fields"]
    key = str(PurePosixPath(enemy["physics_behavior"]).parent / physics["ControllerAttributes"])
    attributes = source["objects"][key]["declared_fields"]
    controller = {key: float(attributes[token]) for key, token in
                  (("height", "Height"), ("radius", "CapsuleRadius"), ("skin", "SkinWidth"))}
    by_scale = {fixture["supplied_scale"]: i for i, fixture in enumerate(world)}
    cases = []
    for case in controls["cases"]:
        value = case["native"]
        result = None
        if value["hit"]:
            slot = value["proxy_index"]
            assert proxies[slot]["bone_token"] == value["bone_token"]
            result = {
                "proxy": slot, "part": proxies[slot]["part"], "point": value["point"],
                "tied_slots": value["portable_nearest_within_tolerance"],
            }
        cases.append({
            "name": case["case"], "fixture": by_scale[case["supplied_scale"]],
            "origin": case["origin"], "direction": case["direction"], "maximum": case["distance_limit"],
            "disabled": case["disabled_fixture_slots"], "expected": result,
        })
    assert len(cases) == 62 and sum(case["expected"] is not None for case in cases) == 58
    anchors = {
        "head_bone": point(native_rows[bones[enemy["declared_head_bone"]]]),
        "hip_bone": point(native_rows[bones[enemy["raycast_proxy"]["BoneName"]]]),
    }
    return {
        "source": {"batch_id": BATCH, "manifest_sha256": MANIFEST, "avatar": enemy["path"], "mesh": enemy["mesh"]},
        "bone_count": len(bones), "controller": controller, "anchors": anchors,
        "head_slots": heads, "mine_slots": mine_slots, "proxies": proxies,
        "fixtures": [{"scale": row["supplied_scale"], "world": affine(row["world_column_major"])}
                     for row in world],
        "cases": cases,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--batches", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    fixture = read_fixture(args.batches / BATCH)
    output = json.dumps(fixture, separators=(",", ":"), allow_nan=False) + "\n"
    if args.check:
        assert args.out.read_text() == output
    else:
        args.out.write_text(output)


if __name__ == "__main__":
    main()
