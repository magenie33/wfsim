# SPDX-License-Identifier: AGPL-3.0-or-later
"""Import the verified source attachment reads for the supplied 3D scene."""
import argparse
from hashlib import sha256
import io
import json
from pathlib import Path
import re
import zipfile

from import_native_queries import pinned_member

BATCH = "20261118T083810Z-native-3d-homing-replay-retention"
MANIFEST = "5f57e9c7c377f16b7571b2900a6f7ab8f9c76d9fa584e316b0c16fb7eb72aea3"


def rows(columns):
    return [columns[4 * column + row] for row in range(3) for column in range(4)]


def translation(position):
    x, y, z = (float(v) for v in position.split(","))
    return [1, 0, 0, x, 0, 1, 0, y, 0, 0, 1, z]


def read_fixture(root):
    raw = (root / "manifest.json").read_bytes()
    assert sha256(raw).hexdigest() == MANIFEST
    assert json.loads((root / "READY.json").read_bytes())["manifest_sha256"] == MANIFEST
    manifest = json.loads(raw)
    name = "client-evidence.zip"
    arc = (root / name).read_bytes()
    record = next(item for item in manifest["files"] if item["path"] == name)
    assert len(arc) == record["bytes"] and sha256(arc).hexdigest() == record["sha256"]
    members = {item["path"]: item for item in manifest["archive_members"][name]}
    with zipfile.ZipFile(io.BytesIO(arc)) as archive:
        def read(path):
            return pinned_member(archive, members[path])

        inputs = read("weapons/index.json")
        sources = read("weapons/source-objects.json")["objects"]
        replay = read("attachment/index.json")
        poses = {p["mesh"]: p["native_source_default_pose"]["native_rows"] for p in replay["poses"]}
        attachments = {}
        for model in inputs["models"]:
            path = model["path"]
            bones = read("weapons/" + model["model_file"])["bones"]
            pose_by_name = {bone["name"]: poses[path][bone["index"]] for bone in bones}
            for spec in model["attachments"]:
                path = spec["attachment_path"]
                declaration = sources[path]["declared_fields"].get("FiringPositions", "")
                translations = re.findall(r"LocalPosition=\{([^{}]+)\}", declaration)
                rotations = re.findall(r"LocalRotation=\{([^{}]+)\}", declaration)
                names = re.findall(r"BoneName=([^\r\n]+)", declaration)
                assert len(translations) == len(rotations) == len(names)
                assert all(q == "0,0,0,1" for q in rotations)
                assert [name for name in names if name != '\"\"'] == spec["declared_bones"]
                firing_bones = []
                positions = []
                for name, position in zip(names, translations):
                    bone = None
                    if name in pose_by_name:
                        bone = len(firing_bones)
                        firing_bones.append(pose_by_name[name])
                    positions.append({"bone": bone, "local": translation(position)})
                attachments[path] = {
                    "id": path.rsplit("/", 1)[1],
                    "bones": firing_bones,
                    "profiles": [positions] if positions else [],
                }
        cases = []
        for source in replay["cases"]:
            query = source["query"]
            cases.append({
                "attachment": attachments[source["attachment"]]["id"],
                "world": rows(source["supplied_attachment_world"]),
                "mode": query["mode"],
                "profile": query["selected_profile"],
                "cursor": query["cursor"],
                "next_cursor": source["advance"]["after"],
                "expected": rows(query["native_world_column_major"]),
            })
        assert len(cases) == 108
        return {
            "manifest_sha256": MANIFEST, "exe_sha256": replay["client_executable_sha256"],
            "attachments": list(attachments.values()), "cases": cases,
        }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--batches", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    fixture = read_fixture(args.batches / BATCH)
    args.out.write_text(json.dumps(fixture, indent=2) + "\n")


if __name__ == "__main__":
    main()
