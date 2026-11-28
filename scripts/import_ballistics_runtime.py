#!/usr/bin/env python3
"""Project client projectile and hitscan ballistics profiles from PROJ-HITSCAN-01."""
import argparse
import hashlib
import json
import re
import struct
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "data/geometry/ballistics-runtime.yaml"

PROJ_HITSCAN = "20261128T092539Z-unified-projectile-hitscan-and-melee-ranged-calibration"
PROJ_HITSCAN_PIN = "32e8d38cfa4879e7c58fa537b8856159895a1b1b3857f2343fbce29e8219fed7"


def canonical(token):
    raw = struct.pack("<f", float(token))
    value = struct.unpack("<f", raw)[0]
    for digits in range(1, 10):
        text = format(value, f".{digits}g")
        if struct.pack("<f", float(text)) == raw:
            return float(text)
    raise ValueError(token)


def catalog_forms():
    entries = {}
    for path in sorted((ROOT / "data/weapons").rglob("*.yaml")):
        text = path.read_text()
        m_id = re.search(r"(?m)^id:\s*(\S+)", text)
        if not m_id:
            continue
        wid = m_id.group(1)
        m_form = re.search(r"(?m)^form:\s*(\S+)", text)
        m_inh = re.search(r"(?m)^inherits:\s*(\S+)", text)
        m_int = re.search(r"(?m)^internal_name:\s*(\S+)", text)
        entries[wid] = {
            "id": wid,
            "form": m_form.group(1) if m_form else "base",
            "group": m_inh.group(1) if m_inh else wid,
            "internal_name": m_int.group(1) if m_int else None,
        }
    for wid, meta in entries.items():
        if meta["internal_name"] is None and meta["group"] in entries:
            meta["internal_name"] = entries[meta["group"]]["internal_name"]
    by_group_form = {}
    by_internal = {}
    for wid, meta in entries.items():
        by_group_form.setdefault((meta["group"], meta["form"]), []).append(wid)
        if meta["internal_name"]:
            by_internal.setdefault(meta["internal_name"], []).append(wid)
    return entries, by_group_form, by_internal


def extract_wave_resize(wp):
    if not wp:
        return None
    resizes = [r for r in (wp.get("ProjectileResizes") or []) if float(r.get("Time") or 0.0) > 0.0]
    if not resizes:
        return None
    target = resizes[-1]
    init_r = float(wp.get("InitialRadius") or 0.0)
    init_w = float(wp.get("InitialWidth") or 0.0)
    fin_r = float(target.get("ResizeRadius") or init_r)
    fin_w = float(target.get("ResizeWidth") or init_w)
    dur = float(target.get("Time") or 0.0)
    if dur <= 0.0 or (fin_r == init_r and fin_w == init_w):
        return None
    return {
        "initial_radius_m": canonical(init_r),
        "final_radius_m": canonical(fin_r),
        "resize_duration_seconds": canonical(dur),
        "initial_height_m": canonical(init_w),
        "final_height_m": canonical(fin_w),
    }


def profile_from_t2_proj(proj, fire_charge):
    fp = proj.get("flight_physics") or {}
    pbf = proj.get("penetration_bounce_and_fuse") or {}
    wp = fp.get("WaveProjectile")
    srgb = pbf.get("SmartRicochetAndGlaiveBounce") or {}
    out = {}

    if fp.get("MaxLife") is not None and float(fp["MaxLife"]) > 0.0:
        out["max_life_seconds"] = canonical(fp["MaxLife"])
    if fp.get("Speed_effective") is not None and float(fp["Speed_effective"]) > 0.0:
        out["initial_speed_mps"] = canonical(fp["Speed_effective"])
    if fp.get("MaxSpeed_effective") is not None and float(fp["MaxSpeed_effective"]) > 0.0:
        out["max_speed_mps"] = canonical(fp["MaxSpeed_effective"])
    if fp.get("MinSpeed_declared") is not None and float(fp["MinSpeed_declared"]) > 0.0:
        out["min_speed_mps"] = canonical(fp["MinSpeed_declared"])
    if fp.get("KinematicAccel") is not None and float(fp["KinematicAccel"]) != 0.0:
        out["accel_mps2"] = canonical(fp["KinematicAccel"])
    if fp.get("GravityEnabled"):
        gf = fp.get("GravityFactor_effective")
        if gf is not None and float(gf) != 0.0:
            out["gravity_multiplier"] = canonical(gf)
        gd = fp.get("GravityDelay")
        if gd is not None and float(gd) > 0.0:
            out["gravity_delay_seconds"] = canonical(gd)

    if pbf.get("ArmingDistance") is not None and float(pbf["ArmingDistance"]) > 0.0:
        out["arming_distance_m"] = canonical(pbf["ArmingDistance"])

    if (
        pbf.get("ExplodeOnLifeExpired")
        and not pbf.get("ExplodeOnImpact_effective")
        and not (fire_charge or {}).get("DetonateManually")
    ):
        out["explode_on_impact"] = False
        out["explode_on_expiry"] = True

    free_pierce = (
        proj.get("native_class") == "/Lotus/Types/Weapon/WaveProjectile"
        and wp is not None
        and not wp.get("AllowNormalContactBehavior", True)
    ) or (pbf.get("StopOnAvatars_effective") is False)
    if free_pierce:
        out["free_avatar_pierce"] = True

    raw_mr = int(srgb.get("MaxReflections") or 0)
    raw_mb = int(pbf.get("MaxBounces_effective") or 0)
    if 0 < raw_mr < 0x80000000:
        max_bounces = raw_mr
    elif 2 <= raw_mb < 0x80000000 and float(pbf.get("Toughness_effective") or 0.0) > 0.0:
        max_bounces = raw_mb - 1
    else:
        max_bounces = 0
    if max_bounces > 0:
        out["can_bounce"] = True
        out["max_bounces"] = max_bounces
        rn = pbf.get("KinematicRestitution_effective")
        rt = pbf.get("KinematicRestitutionParallel_effective")
        if rn is not None and float(rn) != 0.0:
            out["restitution_normal"] = canonical(rn)
        if rt is not None and float(rt) != 0.0:
            out["restitution_tangent"] = canonical(rt)

    wr = extract_wave_resize(wp)
    if wr is not None:
        out["wave_resize"] = wr

    if srgb.get("BounceTowardSeekTarget") and srgb.get("SeekDistance"):
        cone = srgb.get("BounceSeekCone")
        out["bounce_seek"] = {
            "cone_deg": canonical(180.0 if cone is None else cone),
            "range_m": canonical(srgb["SeekDistance"]),
        }
    return out


def profile_from_t3_parsed(parsed, ancestry):
    out = {}
    if parsed.get("MaxLife") is not None and float(parsed["MaxLife"]) > 0.0:
        out["max_life_seconds"] = canonical(parsed["MaxLife"])
    spd = parsed.get("KinematicInitialSpeed", parsed.get("Speed"))
    if spd is not None and float(spd) > 0.0:
        out["initial_speed_mps"] = canonical(spd)
    mspd = parsed.get("KinematicMaxSpeed", spd)
    if mspd is not None and float(mspd) > 0.0:
        out["max_speed_mps"] = canonical(mspd)
    if parsed.get("MinSpeed") is not None and float(parsed["MinSpeed"]) > 0.0:
        out["min_speed_mps"] = canonical(parsed["MinSpeed"])
    if parsed.get("KinematicAccel") is not None and float(parsed["KinematicAccel"]) != 0.0:
        out["accel_mps2"] = canonical(parsed["KinematicAccel"])
    if parsed.get("KinematicGravity"):
        gf = parsed.get("KinematicGravityMultiplier", 1.0)
        if float(gf) != 0.0:
            out["gravity_multiplier"] = canonical(gf)
        gd = parsed.get("GravityDelay")
        if gd is not None and float(gd) > 0.0:
            out["gravity_delay_seconds"] = canonical(gd)

    is_wave = "/Lotus/Types/Weapon/WaveProjectile" in ancestry
    is_glaive = "/Lotus/Types/Weapon/Glaive" in ancestry
    stop_on_avatars = parsed.get("StopOnAvatars")
    if stop_on_avatars == 0 or stop_on_avatars is False or (stop_on_avatars is None and (is_wave or is_glaive)):
        out["free_avatar_pierce"] = True

    raw_mr = int(parsed.get("MaxReflections") or 0)
    raw_mb = int(parsed.get("MaxBounces") or 0)
    if is_glaive and 0 < raw_mr < 0x80000000:
        max_bounces = raw_mr
    elif 2 <= raw_mb < 0x80000000:
        max_bounces = raw_mb - 1
    else:
        max_bounces = 0
    if max_bounces > 0:
        out["can_bounce"] = True
        out["max_bounces"] = max_bounces
        out["restitution_normal"] = canonical(parsed.get("KinematicRestitution", 0.25))
        out["restitution_tangent"] = canonical(parsed.get("KinematicRestitutionParallel", 0.8))

    resizes = parsed.get("ProjectileResizes")
    if resizes:
        wr = extract_wave_resize({
            "InitialRadius": parsed.get("InitialRadius", 0.0),
            "InitialWidth": parsed.get("InitialWidth", 0.0),
            "ProjectileResizes": resizes,
        })
        if wr is not None:
            out["wave_resize"] = wr

    if is_glaive and parsed.get("SeekDistance"):
        out["bounce_seek"] = {
            "cone_deg": canonical(parsed.get("BounceSeekCone", 180.0)),
            "range_m": canonical(parsed["SeekDistance"]),
        }
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exchange", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    batch_dir = args.exchange / "batches" / PROJ_HITSCAN
    actual_sha = hashlib.sha256((batch_dir / "manifest.json").read_bytes()).hexdigest()
    assert actual_sha == PROJ_HITSCAN_PIN, (actual_sha, PROJ_HITSCAN_PIN)

    with zipfile.ZipFile(batch_dir / "evidence.zip") as zf:
        t1 = json.loads(zf.read("track1_hitscan/hitscan-full-calibration-roster.json"))
        t2 = json.loads(zf.read("track2_gun_projectiles/gun-projectile-full-calibration-roster.json"))
        t3 = json.loads(zf.read("track3_melee_ranged/melee-ranged-full-calibration-roster.json"))

    entries, by_group_form, by_internal = catalog_forms()
    raw_attacks = {}

    # Track 1: InfinitePunctureDepth hitscan forms (Zenith semi-auto disc alt-fire).
    for row in t1["firearm_w3d_hitscan_forms"]:
        cal = row.get("calibration") or {}
        tgp = cal.get("trace_geometry_and_penetration") or {}
        if tgp.get("InfinitePunctureDepth") == 1:
            candidates = by_group_form.get((row["weapon_id"], row["form_id"]), [])
            if not candidates and row["weapon_id"] in entries:
                candidates = [row["weapon_id"]]
            for wid in candidates:
                raw_attacks[wid] = {"infinite_puncture_depth": True}

    # Track 2: All firearm/archgun/sentinel projectile forms.
    proj_catalog = t2["projectile_catalog"]
    for form in t2["forms"]:
        candidates = by_group_form.get((form["weapon_id"], form["form_id"]), [])
        if not candidates and form["weapon_id"] in entries:
            candidates = [form["weapon_id"]]
        if not candidates:
            continue
        proj_path = form.get("w3d_projectile_path")
        if not proj_path:
            dp = form.get("direct_projectile_paths") or []
            proj_path = dp[0] if dp else None
        if not proj_path or proj_path not in proj_catalog:
            continue
        prof = profile_from_t2_proj(proj_catalog[proj_path], form.get("fire_and_charge"))
        if not prof:
            continue
        for wid in candidates:
            merged = dict(raw_attacks.get(wid, {}))
            merged.update(prof)
            raw_attacks[wid] = merged

    # Track 3: Thrown melee & Glaive uncharged/charged projectiles.
    for row in t3["glaives_calibration_table"]:
        wids = by_internal.get(row["weapon_path"], [])
        if not wids:
            continue
        uncharged = row.get("uncharged_projectile")
        charged = row.get("charged_turbo_projectile") or uncharged
        for wid in wids:
            form_kind = entries[wid]["form"]
            if form_kind in ("throw", "base") and wid.endswith("_throw") and not wid.endswith("_charged_throw"):
                src = uncharged
            elif form_kind == "charged_throw" or wid.endswith("_charged_throw"):
                src = charged
            elif wid in ("deconstructor", "deconstructor_prime"):
                src = uncharged
            else:
                continue
            if not src or not src.get("parsed_fields"):
                continue
            prof = profile_from_t3_parsed(src["parsed_fields"], src.get("ancestry_leaf_first") or [])
            if prof:
                raw_attacks[wid] = prof

    # Track 3: Ranged heavy/combo melee projectiles (Gunblades, Warfans, Heavy Scythes, Archon/Tenet melees).
    for mw in t3["melee_weapons"]:
        if not mw.get("in_w3d_forms_baseline"):
            continue
        projs = mw.get("resolved_projectiles_and_triggers") or []
        if not projs:
            continue
        wids = by_internal.get(mw["path"], [])
        if not wids:
            continue
        primary_proj = None
        for p in projs:
            anc = p.get("ancestry_leaf_first") or []
            if "/EE/Types/Engine/Projectile" in anc:
                primary_proj = p
                break
        if primary_proj is None:
            continue
        prof = profile_from_t3_parsed(
            primary_proj.get("parsed_fields") or {},
            primary_proj.get("ancestry_leaf_first") or [],
        )
        # Preserve calibrated arena range on Stropha/Warfans where YAML range_m is explicitly tested;
        # keep wave_resize, gravity, initial/max speed, and free_avatar_pierce.
        prof.pop("max_life_seconds", None)
        if not prof:
            continue
        for wid in wids:
            if entries[wid]["form"] == "heavy" or wid.endswith("_heavy"):
                raw_attacks[wid] = prof

    for prof in raw_attacks.values():
        if prof.get("max_speed_mps") == prof.get("initial_speed_mps"):
            prof.pop("max_speed_mps", None)
        if prof.get("can_bounce"):
            if prof.get("restitution_normal") == canonical(0.25):
                prof.pop("restitution_normal", None)
            if prof.get("restitution_tangent") == canonical(0.8):
                prof.pop("restitution_tangent", None)

    canonical_json = {
        wid: json.dumps(prof, sort_keys=True, separators=(",", ":"))
        for wid, prof in sorted(raw_attacks.items())
    }
    unique_keys = sorted(set(canonical_json.values()))
    key_to_idx = {k: idx for idx, k in enumerate(unique_keys)}
    profiles = [json.loads(k) for k in unique_keys]
    attacks = {wid: key_to_idx[k] for wid, k in canonical_json.items()}

    result = json.dumps({"attacks": attacks, "profiles": profiles}, sort_keys=True, separators=(",", ":")) + "\n"
    if args.check:
        assert OUTPUT.read_text() == result, OUTPUT
    else:
        OUTPUT.write_text(result)
    print(f"{len(attacks)} ballistics attacks bound across {len(profiles)} unique client profiles")


if __name__ == "__main__":
    main()
