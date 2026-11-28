# SPDX-License-Identifier: AGPL-3.0-or-later
"""Project pinned 12-target, Warframe hand-bone, and 19-carrier 3D profiles into a compact fixture."""

import argparse
from hashlib import file_digest, sha256
import json
from pathlib import Path
from zipfile import ZipFile

from import_native_attachments import rows
from import_native_queries import pinned_member

WFGEO_BATCH = "20261127T112655Z-warframe-and-target-3d-profiles-closure"
WFGEO_PIN = "5397288382409952fb5697fd142b8fc21c11288357ab619d1664f13199d60209"
W3D_BATCH = "20261127T114535Z-w3d-emit-and-routes-full-roster-closure"
W3D_PIN = "20d0b27e4daa225da30e89570740958ebed29c958a23f3d7806c6915cb46967a"
IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0]


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


def unscale_rows(matrix_rows, scale):
    return [v / scale for v in matrix_rows]


def shape_primitive(shape):
    kind = shape["type"]
    if kind == 2:
        return {"type": "sphere", "radius": shape["radius"]}
    if kind == 1:
        return {"type": "capsule", "radius": shape["radius"], "height": shape["capsule_height"]}
    assert kind == 0
    return {"type": "box", "half_extents": [d * 0.5 for d in shape["dimensions"]]}


def build_enemies(e3d):
    targets = e3d["targets"]
    specs = (
        ("crewman", 0, 1, 1.0),
        ("legacyte", 0, 1, 1.5),
        ("malice", 0, 1, 1.1),
        ("techrot_babau", 2, 0, 1.0),
    )
    out = {}
    for target_id, head_proxy, body_proxy, declared_scale in specs:
        target = targets[target_id]
        scale = target["effective_scale_f32"]
        assert abs(scale - declared_scale) < 1e-6
        install = target["controller_and_contact"]["unscaled_1_0_install"]
        proxies = []
        for p in target["ordered_direct_hit_proxies"]:
            sem = p["native_semantics"]
            part = 0 if sem["treat_headshot_as_normal_hit"] else sem["resolved_part_id"]
            proxies.append({
                "cache_index": p["slot"],
                "part": part,
                "bone_token": p["slot"] + 1,
                "world": unscale_rows(p["world_rows_at_declared_scale"], scale),
                "shape": shape_primitive(p["shape"]),
                "ray_enabled": bool(sem["raycast_enabled"]),
            })
        whole = target["whole_body_raycast_proxy"]
        radial = None
        if whole and whole.get("proxy_path"):
            radial = {
                "local": unscale_rows(whole["world_rows_at_declared_scale"], scale),
                "shape": shape_primitive(whole["shape"]),
            }
        anchors = target["anchors_at_declared_scale_origin"]
        if target_id == "malice":
            candidate = [proxies[head_proxy]["world"][i] for i in (3, 7, 11)]
        else:
            candidate = [v / scale for v in anchors["homing_candidate_aim_point"]["point"]]
        steering = [v / scale for v in anchors["steering_navigation_point"]["point"]]
        model = {
            "world": IDENTITY,
            "proxies": proxies,
            "controller": {
                "height": install["source_total_height"],
                "radius": install["source_radius"],
                "skin": install["source_skin"],
                "scale": declared_scale,
            },
            "candidate_anchor": candidate,
            "steering_center": steering,
        }
        if radial is not None:
            model["radial_proxy"] = radial
        out[target_id] = {
            "head_proxy": head_proxy,
            "body_proxy": body_proxy,
            "model": model,
        }
    return out


def build_warframes(wfgeo):
    profiles = {p["profile_id"]: p for p in wfgeo["profiles"]}
    by_id = {e["frame_id"]: e for e in wfgeo["entries"] if e.get("form") == "default_playable_suit"}
    chroma = profiles[by_id["chroma_prime"]["rig_profile"]]["raw_values"]["reference_bones_fk"]["GAME_L1_WEAPON1"]["native_world_rows"]
    narin = profiles[by_id["narin"]["rig_profile"]]["raw_values"]["reference_bones_fk"]["GAME_R1_WEAPON1"]["native_world_rows"]
    return {
        "chroma_prime": {"17": chroma},
        "narin": {"48": narin},
    }


def build_carriers(w3d):
    modes = w3d["suite_1_carriers_and_archgun_mounts"]["carrier_modes"]
    assert len(modes) == 19
    out = {}
    for entry in modes:
        host = entry["hosts"][0]
        kind = host["carrier_kind"]
        scale = host["carrier_scale"]
        muzzle_rows = unscale_rows(rows(host["native_world_column_major"]), scale)
        if kind == "sentinel":
            ox, oy, oz = entry["sentinel_owner_local_offset"]
            owner_offset = [ox, 1.5 + oy, oz]
            controller = {"height": 1.25, "radius": 0.25, "skin": 0.02, "scale": scale}
            eye_y = 0.5
        else:
            assert kind == "moa"
            owner_offset = [0.0, 0.0, 0.0]
            controller = {"height": 1.3, "radius": 0.25, "skin": 0.02, "scale": scale}
            eye_y = 1.3
        out[entry["weapon_id"]] = {
            "kind": kind,
            "owner_offset": owner_offset,
            "controller": controller,
            "muzzle_local": muzzle_rows,
            "eye_y": eye_y,
        }
    return out


def build(exchange):
    batches = exchange / "batches"
    wf_zip, wf_members = pinned_archive(batches, WFGEO_BATCH, WFGEO_PIN, "evidence.zip")
    with ZipFile(wf_zip) as archive:
        wfgeo = pinned_member(archive, wf_members["wfgeo-roster-profiles.json"])
        e3d = pinned_member(archive, wf_members["e3d-all12-target-controls.json"])
    w3d_zip, w3d_members = pinned_archive(batches, W3D_BATCH, W3D_PIN, "evidence.zip")
    with ZipFile(w3d_zip) as archive:
        w3d = pinned_member(archive, w3d_members["w3d-focused-route-controls.json"])
    return {
        "enemies": build_enemies(e3d),
        "warframes": build_warframes(wfgeo),
        "carriers": build_carriers(w3d),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exchange", type=Path, required=True)
    parser.add_argument(
        "--out",
        type=Path,
        default=Path(__file__).resolve().parents[1]
        / "engine/src/rules/geometry/fixtures/native-roster-closure.json",
    )
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    output = json.dumps(build(args.exchange), separators=(",", ":"), allow_nan=False) + "\n"
    if args.check:
        assert args.out.read_text() == output
    else:
        args.out.write_text(output)


if __name__ == "__main__":
    main()
