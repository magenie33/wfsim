# SPDX-License-Identifier: AGPL-3.0-or-later
"""Extract standing weapon profiles from the pinned source roster."""
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
from import_native_attachments import rows
from import_native_queries import pinned_member

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "data/geometry/weapon-attachments.yaml"
BATCH = "20261121T025655Z-all-weapon-3d-source-map"
PIN = "24cc77326dccbb35a335892534f53981d6a80b3beeefb1aead7d43578a5ceb7a"
METADATA = "20261106T150853Z-client-metadata-offline"
METADATA_PIN = "dbeb51209c791af1692f91f087826805b8f99198ee0958ec4606170ab2badea2"
CONTROLS = "20261121T133426Z-firearm-3d-fidelity-source-controls"
CONTROLS_PIN = "1603969884404ad0b512fc05d3b326ed4107583f7a62b2fe478b69ba6b9ac28e"


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


def read_controls(batches):
    filename, members = pinned_archive(batches, CONTROLS, CONTROLS_PIN, "evidence.zip")
    with ZipFile(filename) as archive:
        cases = pinned_member(archive, members["emission/attachment-cases.json"])
    return {(case["attachment_path"], case["supplied_mode"]): case for case in cases}


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


def attach_slots(ancestry, layers, candidates, records, bone_numbers, transformed, alternating):
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
    if sum(hand != "EXTRA1" for hand in slots) != (2 if alternating else 1):
        return
    for slot in slots.values():
        if slot.get("BoneName") not in bone_numbers or slot.get("RelativeToBounds", "0") != "0":
            return
        if slot["Type"] not in records or slot["Type"] not in candidates:
            return
    return slots


def single_iteration(behaviors, forms):
    for form in forms:
        index = form.get("dispatch_behavior_index")
        if not isinstance(index, int) or not 0 <= index < len(behaviors):
            return False
        emit = behaviors[index]["selected_geometry_and_emit_fields"]
        counts = [value for key, value in emit.items() if key.endswith(".fireIterations")]
        if counts != ["1"]:
            return False
    return True


def weapon_rig(path, slot, linked, mounts, models, bone_numbers, controls):
    declaration = linked[path]["fields"].get("FiringPositions")
    model_paths = mounts.get(path, {}).get("model_paths", [])
    firing_positions = []
    used_bones = []
    bone_ids = {}
    if declaration:
        for row in blocks(declaration["source_text"]):
            entry = fields(row)
            match entry.get("UseMultipleFirePositions"):
                case "0" if "FirePosition" in entry:
                    points = [entry["FirePosition"].strip()[1:-1]]
                case "1" if "CyclingFirePositions" in entry:
                    points = list(blocks(entry["CyclingFirePositions"]))
                case _:
                    return
            if not points:
                return
            positions = []
            for point in map(fields, points):
                if point.get("RelativeToBounds", "0") != "0":
                    return
                bone_name = point.get("BoneName", '""')
                bone_id = None
                if bone_name != '""':
                    if len(model_paths) != 1:
                        return
                    if bone_name not in bone_ids:
                        try:
                            matrix = source_bone(models[model_paths[0]], bone_name)
                        except (KeyError, ValueError):
                            return
                        bone_ids[bone_name] = len(used_bones)
                        used_bones.append(matrix)
                    bone_id = bone_ids[bone_name]
                positions.append({"bone": bone_id, "local": offset(point)})
            firing_positions.append(positions)
        if not firing_positions:
            return
        if len(firing_positions) > 1 or len(firing_positions[0]) > 1:
            for mode, positions in enumerate(firing_positions):
                case = controls.get((path, mode))
                if not case or not agrees_with_native(case, declaration, positions, used_bones):
                    return
    return {
        "mount": {"bone": bone_numbers[slot["BoneName"]], "local": offset(slot)},
        "bones": used_bones,
        "profiles": firing_positions,
    }


def agrees_with_native(case, declaration, positions, bones):
    if case["firing_positions_source"] != declaration:
        return False
    mounted = rows(case["mount_world_column_major"])
    reached = set()
    for step in case["ordered_reads"]:
        read = step["read"]
        cursor = read["cursor"]
        index = cursor % len(positions)
        if read["selected_profile"] != case["supplied_mode"] or read["selected_candidate"] != index:
            return False
        reached.add(index)
        pos = positions[index]
        bone = mounted if pos["bone"] is None else compose(mounted, bones[pos["bone"]])
        actual = compose(bone, pos["local"])
        expected = rows(read["native_world_column_major"])
        advance = step["advance"]
        next_cursor = (cursor + 1) % len(positions) if len(positions) > 1 else cursor
        if advance["before"] != cursor or advance["after"] != next_cursor:
            return False
        if max(abs(a-b) for a,b in zip(actual, expected, strict=True)) > 0.00002:
            return False
    return len(reached) == len(positions)


def select_groups(form_rows, source, models, layers, controls):
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
        if set(data).intersection(("HeavyWeaponAttachments", "UsesExtraHandAttachments",
                                   "PrimaryAttachmentOverride", "AlwaysUseAttachmentChildBone",
                                   "AuxiliaryAttachmentFiringIndex")):
            continue
        style = data.get("AttachmentFireStyle", {}).get("source_text")
        if style not in (None, "LEFT_HAND_ONLY", "ALTERNATE_HANDS") or ("AuxiliaryAttachments" in data and style is None):
            continue
        if "THIRD_PERSON_ATTACHMENT" not in data:
            continue
        involved = set.intersection(*(set(row["named_hand_or_heavy_mount_candidates"]) for row in forms))
        declaration = data["THIRD_PERSON_ATTACHMENT"]
        assert layers[declaration["declared_by"]] == declaration["source_text"]
        transformed = {row["form_or_action_id"] for row in forms} == {"base", "incarnon"}
        incarnon_slots = data.get("IncarnonAttachments")
        explicit_transform = incarnon_slots and fields(incarnon_slots["source_text"].strip()[1:-1]) == {"EXTRA1": "1"}
        if incarnon_slots and not (transformed and explicit_transform):
            continue
        alternating = style == "ALTERNATE_HANDS"
        if alternating and (transformed or not single_iteration(entry["behavior_records"], forms)):
            continue
        slots = attach_slots(entry["ancestry_leaf_first"], layers, involved, links, bones, transformed, alternating)
        if not slots:
            continue
        if alternating:
            hands = [slots["MAIN_HAND"], slots["OFF_HAND"]]
            if not hands[0]["BoneName"].startswith("GAME_R1_") or not hands[1]["BoneName"].startswith("GAME_L1_"):
                continue
            rigs = [weapon_rig(hand["Type"], hand, links, models["mount_source"], models["models"], bones, controls)
                    for hand in hands]
            if any(rig is None or len(rig["profiles"]) > 1
                   or any(len(p) > 1 for p in rig["profiles"]) for rig in rigs):
                continue
            mapped[id] = {
                "attachments": rigs,
                "round_order": [firing_slot(index) for index in range(2)],
            }
            continue
        extra = slots.pop("EXTRA1", None)
        slot = next(iter(slots.values()))
        if (style == "LEFT_HAND_ONLY") != slot["BoneName"].startswith("GAME_L1_"):
            continue
        attach = weapon_rig(slot["Type"], slot, links, models["mount_source"], models["models"], bones, controls)
        if attach is None:
            continue
        rigs = [attach]
        if extra is not None:
            other = weapon_rig(extra["Type"], extra, links, models["mount_source"], models["models"], bones, controls)
            if other is None or (not explicit_transform and (not attach["profiles"] or other != attach)):
                continue
            if explicit_transform:
                rigs.append(other)
        elif explicit_transform:
            continue
        orders = {}
        for form in forms:
            kind = form["form_or_action_id"]
            index = int(kind == "incarnon" and len(rigs) > 1)
            profiles = rigs[index]["profiles"]
            mode = form["dispatch_behavior_index"]
            if len(profiles) > 1 and mode is None:
                break
            mode = mode if mode is not None and mode < len(profiles) else 0
            cycling = bool(profiles and len(profiles[mode]) > 1)
            orders[kind] = [firing_slot(index, mode, cycling)]
        else:
            base = orders.get("base", next(iter(orders.values())))
            mapped[id] = {"attachments": rigs, "round_order": base}
            overrides = {kind: order for kind, order in orders.items() if order != base}
            if overrides:
                mapped[id]["round_order_by_form"] = overrides
    assert "nataruk" in mapped or "nataruk_perfect" in mapped
    return mapped


def firing_slot(index, mode=0, cycling=False):
    return {
        "attachment": index, "mode": mode,
        "refresh_per_iteration": cycling, "advance_after_round": cycling,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--batches", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    form_rows, source, models = shape_data(args.batches)
    paths = {path for row in source["weapon_paths"].values() for path in row["ancestry_leaf_first"]}
    groups = select_groups(form_rows, source, models, source_mount_layers(args.batches, paths), read_controls(args.batches))
    value = json.dumps(groups, separators=(",", ":"), allow_nan=False) + "\n"
    if args.check:
        assert OUTPUT.read_text() == value
    else:
        OUTPUT.write_text(value)
    print(f"{len(groups)} calculated attachment mounts, "
          f"{sum(bool(a['profiles']) for v in groups.values() for a in v['attachments'])} declared fire-position lists")


if __name__ == "__main__":
    main()
