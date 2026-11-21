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


def replay(base, weapon, selection, mounted):
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
    args = parser.parse_args()
    meta = call(args.url, "/api/meta")
    configs = meta["scene_3d_source_default"]
    mounted = set(configs["weapons"]) - set(configs["reference_weapons"])
    advertised = [(weapon, {"mode": mode}) for weapon in meta["weapons"] for mode in weapon["modes"]]
    transformed = [(weapon, {"form": form["id"]}) for weapon in meta["weapons"]
                   for form in weapon["forms"] if form["gauge_switched"]]
    with ThreadPoolExecutor(max_workers=5) as jobs:
        rows = list(jobs.map(lambda wm: replay(args.url, *wm, mounted), advertised))
        fixed_rows = list(jobs.map(lambda wm: replay(args.url, *wm, mounted), transformed))
    for row in rows + fixed_rows:
        if row["status"] == "traced":
            assert (row["emitter"] == "attachment") == row["mapped_attachment"]
    assert len({(row["weapon"], row["mode"]) for row in rows}) == len(advertised)
    assert len({(row["weapon"], row["form"]) for row in fixed_rows}) == len(transformed)
    summary = {
        "catalog_weapons": len(meta["weapons"]),
        "advertised_modes": len(advertised),
        "by_type": {kind: dict(Counter(r["status"] for r in rows if r["weapon_type"] == kind))
                    for kind in sorted({r["weapon_type"] for r in rows})},
        "traced_weapon_count": len({r["weapon"] for r in rows if r["status"] == "traced"}),
        "traced_attachment_modes": sum(r.get("emitter") == "attachment" and r["status"] == "traced" for r in rows),
        "traced_reference_modes": sum(r.get("emitter") == "player_mount" and r["status"] == "traced" for r in rows),
        "catalog_attachment_count": len(mounted),
        "refused_reasons": dict(Counter(r["error"] for r in rows if r["status"] == "refused")),
        "fixed_transformed_forms": dict(Counter(r["status"] for r in fixed_rows)),
    }
    report = {"summary": summary, "modes": rows, "fixed_transformed": fixed_rows}
    args.out.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
