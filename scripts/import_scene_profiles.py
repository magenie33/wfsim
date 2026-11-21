# SPDX-License-Identifier: AGPL-3.0-or-later
"""Project retained 3D source controls into the standing, open-arena profiles."""
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "engine/src/rules/geometry/fixtures"
OUTPUT = ROOT / "data/geometry/source-default.yaml"
IDENTITY = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0]


def read(name):
    return json.loads((FIXTURES / name).read_bytes())


def center(rows):
    return [rows[i] for i in (3, 7, 11)]


def posed_proxy(p):
    shape = p["shape"]
    if shape["type"] == 2:
        primitive = {"type": "sphere", "radius": shape["radius"]}
    else:
        assert shape["type"] == 1
        primitive = {"type": "capsule", "radius": shape["radius"], "height": shape["capsule_height"]}
    return {"cache_index": p["cache_index"], "part": p["part"], "bone_token": p["bone_token"],
            "world": p["source_default_rows"], "shape": primitive, "ray_enabled": True}


def enemies():
    native = read("native.json")["enemies"]
    radial = {
        "local": [1, 0, 0, 0, 0, 1, 0, 1.1208325624465942, 0, 0, 1, -0.011569245718419552],
        "shape": {"type": "capsule", "radius": 0.6000000238418579, "height": 1},
    }
    result = {}
    for i, (id, path, scale) in enumerate((
        ("thrax_centurion", "/Lotus/Types/Enemies/Duviri/Avatars/DuviriMeleeAvatar", 1.6),
        ("corrupted_heavy_gunner", "/Lotus/Types/Enemies/Orokin/OrokinHeavyFemaleAvatar", 1.25),
    )):
        assert native[i]["path"] == path
        proxies = [posed_proxy(p) for p in native[i]["proxies"]]
        result[id] = {
            "head_proxy": 0, "body_proxy": 1,
            "model": {
                "world": IDENTITY, "proxies": proxies,
                "controller": {"height": 1.3, "radius": 0.25, "skin": 0.02, "scale": scale},
                "radial_proxy": radial,
                "candidate_anchor": center(proxies[0]["world"]), "steering_center": [0, 1, 0],
            },
        }
    demo = read("native-demolisher.json")
    assert demo["head_slots"] == [0, 11]
    for p in demo["proxies"]:
        if p["cache_index"] in demo["mine_slots"]:
            p["ray_enabled"] = False
    result["demolisher_devourer"] = {
        "head_proxy": 11, "body_proxy": 1,
        "model": {
            "world": IDENTITY, "proxies": demo["proxies"],
            "controller": dict(demo["controller"], scale=demo["fixtures"][1]["scale"]),
            "candidate_anchor": demo["anchors"]["head_bone"],
            "steering_center": demo["anchors"]["hip_bone"],
        },
    }
    return result


def weapons():
    source = {row["id"]: row for row in read("native-attachments.json")["attachments"]}
    result = {}
    for id, pair in {
        "akarius": [("SapientPistolAttachment", 48), ("SapientPistolLeftAttachment", 17)],
        "akarius_prime": [("PrimeAkariusAttachmentRight", 48), ("PrimeAkariusAttachmentLeft", 17)],
        "laetum": [("ZarimanHeavyPistolAttachment", 48), ("VoidPistolAttachment", 48)],
    }.items():
        attachments = [{"mount": {"bone": bone, "local": IDENTITY},
                        "bones": source[resource]["bones"], "profiles": source[resource]["profiles"]}
                       for resource, bone in pair]
        round_order = [{"attachment": i, "mode": 0, "refresh_per_iteration": False,
                        "advance_after_round": True} for i in range(len(attachments))]
        layout = {"attachments": attachments, "round_order": round_order}
        if id == "laetum":
            layout["round_order"] = round_order[:1]
            layout["round_order_by_form"] = {"incarnon": round_order[1:]}
        result[id] = layout
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    data = json.dumps({"enemies": enemies(), "weapons": weapons()}, separators=(",", ":"), allow_nan=False) + "\n"
    if args.check:
        assert OUTPUT.read_text() == data
    else:
        OUTPUT.write_text(data)


if __name__ == "__main__":
    main()
