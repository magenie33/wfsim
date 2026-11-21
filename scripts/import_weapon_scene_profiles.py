# SPDX-License-Identifier: AGPL-3.0-or-later
"""Extract standing single-attachment weapon profiles from the pinned source roster."""
import argparse
from collections import defaultdict
from hashlib import file_digest, sha256
import json
import math
from pathlib import Path, PurePosixPath
import re
import struct
from zipfile import ZipFile

from import_client_cadence import blocks, fields
from import_native_queries import pinned_member

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "data/geometry/weapon-attachments.yaml"
BATCH = "20261121T025655Z-all-weapon-3d-source-map"
PIN = "24cc77326dccbb35a335892534f53981d6a80b3beeefb1aead7d43578a5ceb7a"
METADATA = "20261106T150853Z-client-metadata-offline"
METADATA_PIN = "dbeb51209c791af1692f91f087826805b8f99198ee0958ec4606170ab2badea2"


def pinned_archive(batches, batch, pin, archive):
    root = batches / batch
    raw = (root / "manifest.json").read_bytes()
    ready = json.loads((root / "READY.json").read_bytes())
    assert ready["batch_id"] == batch and ready["manifest_sha256"] == sha256(raw).hexdigest() == pin
    manifest = json.loads(raw)
    assert manifest["batch_id"] == batch
    entry = next(record for record in manifest["files"] if record["path"] == archive)
    filename = root / archive
    with filename.open("rb") as stream:
        digest = file_digest(stream, "sha256").hexdigest()
    assert filename.stat().st_size == entry["bytes"] and digest == entry["sha256"]
    return filename, {row["path"]: row for row in manifest["archive_members"][archive]}


def shape_data(batches):
    filename, members = pinned_archive(batches, BATCH, PIN, "evidence.zip")
    names = ("map/weapon-forms.json", "map/client-source-records.json", "models/models.json")
    with ZipFile(filename) as archive:
        return [pinned_member(archive, members[name]) for name in names]


def source_mount_layers(batches, paths):
    filename, members = pinned_archive(batches, METADATA, METADATA_PIN, "data.zip")
    member = members["packages_owntext.jsonl"]
    layers = {}
    digest = sha256()
    size = 0
    with ZipFile(filename) as archive, archive.open(member["path"]) as stream:
        for line in stream:
            digest.update(line)
            size += len(line)
            row = json.loads(line)
            if row["path"] in paths:
                layers[row["path"]] = fields(row["ownText"]).get("THIRD_PERSON_ATTACHMENT")
    assert size == member["bytes"] and digest.hexdigest() == member["sha256"]
    return layers


def vector(text, n):
    values = [float(v) for v in text.strip("{}").split(",")]
    assert len(values) == n and all(math.isfinite(v) for v in values)
    return values


def affine(position, quaternion, scale=1.):
    norm = math.hypot(*quaternion)
    assert norm > 0 and scale > 0
    x, y, z, w = (v/norm for v in quaternion)
    return [scale*(1-2*y*y-2*z*z), scale*(2*x*y-2*z*w), scale*(2*x*z+2*y*w), position[0],
            scale*(2*x*y+2*z*w), scale*(1-2*x*x-2*z*z), scale*(2*y*z-2*x*w), position[1],
            scale*(2*x*z-2*y*w), scale*(2*y*z+2*x*w), scale*(1-2*x*x-2*y*y), position[2]]


def compose(a, b):
    return [sum(a[i*4+k]*b[k*4+j] for k in range(3))+(a[i*4+3] if j == 3 else 0)
            for i in range(3) for j in range(4)]


def offset(record):
    return affine(vector(record.get("LocalPosition", "{0,0,0}"), 3),
                  vector(record.get("LocalRotation", "{0,0,0,1}"), 4),
                  float(record.get("Scale", "1")))


def source_bone(model, wanted):
    bones = model["bones"]
    indices = {bone["name"]: i for i,bone in enumerate(bones)}
    current = indices[wanted]
    full = None
    while True:
        bone = bones[current]
        raw = bytes.fromhex(bone["default_pose_hex"])
        assert bone["index"] == current and len(raw) == 40
        flags = struct.unpack_from("<H", raw, 38)[0]
        if flags not in (0, 0x4000):
            raise ValueError("unbound pose tag")
        position = struct.unpack_from("<3f", raw, 16) if flags else (0., 0., 0.)
        matrix = affine(position, struct.unpack_from("<4f", raw))
        full = matrix if full is None else compose(matrix, full)
        parent = bone["parent_u16"]
        if parent == current:
            break
        assert parent < current
        current = parent
    native = model.get("representative_original_fk")
    if native:
        expected = native["native_rows"][indices[wanted]]
        assert max(abs(a-b) for a,b in zip(expected, full, strict=True)) < 0.00002
        return expected
    return full


def attach_slots(ancestry, layers, candidates, records, bone_numbers, transformed):
    hands = {}
    supported = {"MAIN_HAND", "OFF_HAND", "EXTRA1"} if transformed else {"MAIN_HAND", "OFF_HAND"}
    for owner in reversed(ancestry):
        declaration = layers.get(owner)
        if declaration is None:
            continue
        entries = fields(declaration.strip()[1:-1])
        if set(entries) - supported:
            return
        for hand, raw in entries.items():
            slot = fields(raw.strip()[1:-1])
            token = slot.get("Type", '""')
            if token != '""' and not token.startswith("/"):
                slot["Type"] = str(PurePosixPath(owner).parent / token)
            hands.setdefault(hand, {}).update(slot)
    slots = {hand: row for hand, row in hands.items() if row.get("Type", '""') != '""'}
    if sum(hand != "EXTRA1" for hand in slots) != 1:
        return
    for slot in slots.values():
        if slot.get("BoneName") not in bone_numbers or slot.get("RelativeToBounds", "0") != "0":
            return
        if slot["Type"] not in records or slot["Type"] not in candidates:
            return
    return slots


def weapon_rig(path, slot, linked, mounts, models, bone_numbers):
    declaration = linked[path]["fields"].get("FiringPositions")
    model_paths = mounts.get(path, {}).get("model_paths", [])
    firing_positions = []
    used_bones = []
    bone_ids = {}
    if declaration:
        for row in blocks(declaration["source_text"]):
            entry = fields(row)
            if entry.get("UseMultipleFirePositions") != "0" or "FirePosition" not in entry:
                return
            point = fields(entry["FirePosition"].strip()[1:-1])
            if point.get("RelativeToBounds", "0") != "0":
                return
            bone_name = point.get("BoneName", '""')
            bone_id = None
            if bone_name != '""':
                if len(model_paths) != 1:
                    return
                model = models[model_paths[0]]
                try:
                    rows = source_bone(model, bone_name)
                except (KeyError, ValueError):
                    return
                if bone_name not in bone_ids:
                    bone_ids[bone_name] = len(used_bones)
                    used_bones.append(rows)
                bone_id = bone_ids[bone_name]
            firing_positions.append([{"bone": bone_id, "local": offset(point)}])
        if len(firing_positions) != 1:
            return
    return {
        "mount": {"bone": bone_numbers[slot["BoneName"]], "local": offset(slot)},
        "bones": used_bones,
        "profiles": firing_positions,
    }


def select_groups(form_rows, source, models, layers):
    player = (ROOT / "data/geometry/excalibur-standing.yaml").read_text()
    bones = {name:i for i,name in enumerate(re.findall(r"\] # (\w+)\s*$", player, re.M))}
    assert bones["GAME_R1_WEAPON1"] == 48 and bones["GAME_L1_WEAPON1"] == 17
    gun_groups = defaultdict(list)
    for row in form_rows:
        if row["lane"] == "gun":
            gun_groups[row["group_id"]].append(row)
    assert len(gun_groups) == 395
    source_weapons = source["weapon_paths"]
    links = source["linked_paths"]
    mapped = {}
    for id, forms in sorted(gun_groups.items()):
        paths = {row["native_path"] for row in forms}
        if len(paths) != 1:
            continue
        entry = source_weapons.get(next(iter(paths)), {})
        data = entry.get("fields", {})
        if set(data).intersection(("IncarnonAttachments", "HeavyWeaponAttachments",
                                   "UsesExtraHandAttachments", "PrimaryAttachmentOverride", "AlwaysUseAttachmentChildBone",
                                   "AuxiliaryAttachmentFiringIndex")):
            continue
        style = data.get("AttachmentFireStyle", {}).get("source_text")
        if style not in (None, "LEFT_HAND_ONLY") or ("AuxiliaryAttachments" in data and style is None):
            continue
        if "THIRD_PERSON_ATTACHMENT" not in data:
            continue
        involved = set.intersection(*(set(row["named_hand_or_heavy_mount_candidates"]) for row in forms))
        declaration = data["THIRD_PERSON_ATTACHMENT"]
        assert layers[declaration["declared_by"]] == declaration["source_text"]
        transformed = {row["form_or_action_id"] for row in forms} == {"base", "incarnon"}
        slots = attach_slots(entry["ancestry_leaf_first"], layers, involved, links, bones, transformed)
        if not slots:
            continue
        extra = slots.pop("EXTRA1", None)
        slot = next(iter(slots.values()))
        if (style == "LEFT_HAND_ONLY") != slot["BoneName"].startswith("GAME_L1_"):
            continue
        attach = weapon_rig(slot["Type"], slot, links, models["mount_source"], models["models"], bones)
        if attach is None:
            continue
        if extra is not None:
            other = weapon_rig(extra["Type"], extra, links, models["mount_source"], models["models"], bones)
            if not attach["profiles"] or other != attach:
                continue
        fire = {
            "attachment": 0, "mode": 0,
            "refresh_per_iteration": False, "advance_after_round": False,
        }
        mapped[id] = {"attachments": [attach], "round_order": [fire]}
    assert "nataruk" in mapped or "nataruk_perfect" in mapped
    return mapped


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--batches", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    form_rows, source, models = shape_data(args.batches)
    paths = {path for row in source["weapon_paths"].values() for path in row["ancestry_leaf_first"]}
    groups = select_groups(form_rows, source, models, source_mount_layers(args.batches, paths))
    value = json.dumps(groups, separators=(",", ":"), allow_nan=False) + "\n"
    if args.check:
        assert OUTPUT.read_text() == value
    else:
        OUTPUT.write_text(value)
    print(f"{len(groups)} calculated attachment mounts, "
          f"{sum(bool(v['attachments'][0]['profiles']) for v in groups.values())} declared fire-position lists")


if __name__ == "__main__":
    main()
