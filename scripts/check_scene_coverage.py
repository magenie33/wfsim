# SPDX-License-Identifier: AGPL-3.0-or-later
"""Replay every advertised mode through the standing 3D request."""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
from urllib.request import Request, urlopen


def call(base, path, body=None):
    data = None if body is None else json.dumps(body).encode()
    req = Request(base + path, data=data, headers={"Content-Type": "application/json"})
    with urlopen(req, timeout=60) as reply:
        return json.load(reply)


def replay(base, weapon, selection, mounted, carrier_ids, carrier):
    request = {
        "weapon": weapon["id"], **selection, "enemy": "corrupted_heavy_gunner", "level": 225,
        "eximus": False, "headshot_pct": 100, "aiming": True, "duration": 1, "runs": 1,
        "infinite_ammo": True, "arcane": ["none"], "mods": [],
        "target_at": [0, 5], "scene_3d": {"preset": "source_default", "placement": {"kind": "supplied"}},
    }
    audit = {
        "weapon": weapon["id"], "weapon_type": weapon["weapon_type"], **selection,
        "mapped_attachment": weapon["id"] in mounted,
    }
    if carrier is not None and weapon["id"] in carrier_ids:
        request["scene_3d"]["carrier"] = carrier
        audit["supplied_carrier"] = True
        audit["mapped_attachment"] = carrier["player"].get("equipped_fire") is not None
    replay = call(base, "/api/log", request)
    if not replay["ok"]:
        return dict(audit, status="refused", error=replay["error"])
    if not replay["scene_3d"]["flights"]:
        replay = call(base, "/api/log", dict(request, duration=8))
    assert replay["ok"] and "scene_3d" in replay
    flights = replay["scene_3d"]["flights"]
    return dict(audit, status="traced" if flights else "admitted_no_emission",
                emitter=replay["scene_3d"]["emitter"], projectiles=len(flights),
                contacts=sum(len(f["flight"]["impacts"]) for f in flights))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--carrier", type=Path, help="Supplied carrier pose for companion firearm controls")
    args = parser.parse_args()
    meta = call(args.url, "/api/meta")
    configs = meta["scene_3d_source_default"]
    mounted = set(configs["weapons"]) - set(configs["reference_weapons"])
    carrier_ids = set(configs.get("carrier_weapons", []))
    carrier = json.loads(args.carrier.read_text()) if args.carrier else None
    advertised = [(weapon, {"mode": mode}) for weapon in meta["weapons"] for mode in weapon["modes"]]
    transformed = [(weapon, {"form": form["id"]}) for weapon in meta["weapons"]
                   for form in weapon["forms"] if form["gauge_switched"]]
    with ThreadPoolExecutor(max_workers=5) as jobs:
        rows = list(jobs.map(lambda wm: replay(args.url, *wm, mounted, carrier_ids, carrier), advertised))
        fixed_rows = list(jobs.map(lambda wm: replay(args.url, *wm, mounted, carrier_ids, carrier), transformed))
    for row in rows + fixed_rows:
        if row["status"] == "traced":
            assert (row["emitter"] == "attachment") == row["mapped_attachment"]
    assert len({(row["weapon"], row["mode"]) for row in rows}) == len(advertised)
    assert len({(row["weapon"], row["form"]) for row in fixed_rows}) == len(transformed)
    standing_traces = [r for r in rows if r["status"] == "traced" and not r.get("supplied_carrier")]
    standing_emitters = Counter(r["emitter"] for r in standing_traces)
    summary = {
        "catalog_weapons": len(meta["weapons"]),
        "advertised_modes": len(advertised),
        "by_type": {kind: dict(Counter(r["status"] for r in rows if r["weapon_type"] == kind))
                    for kind in sorted({r["weapon_type"] for r in rows})},
        "traced_weapon_count": len({r["weapon"] for r in rows if r["status"] == "traced"}),
        "traced_attachment_modes": standing_emitters["attachment"],
        "traced_reference_modes": standing_emitters["player_mount"],
        "catalog_attachment_count": len(mounted),
        "supplied_carrier_modes": sum(r.get("supplied_carrier", False) and r["status"] == "traced" for r in rows),
        "refused_reasons": dict(Counter(r["error"] for r in rows if r["status"] == "refused")),
        "fixed_transformed_forms": dict(Counter(r["status"] for r in fixed_rows)),
    }
    report = {"summary": summary, "modes": rows, "fixed_transformed": fixed_rows}
    args.out.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
