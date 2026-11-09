use super::*;
use crate::model::{Spread, SpreadSampling};

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../../tests/fixtures/client/primary-five-2026-10-28.json")).unwrap()
}

#[test]
fn selected_primary_forms_match_their_actual_damage_component_and_nominal_rate() {
    let f = fixture();
    assert_eq!(f["forms"].as_array().unwrap().len(), 13);
    for row in f["forms"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let w = spec(id).unwrap();
        let p = base_panel(id, false);
        let expected = &row["expected"];
        assert!((w.attack.fire_rate - expected["fire_rate"].as_f64().unwrap()).abs() < 1e-10, "{id}: RPM");
        for (actual, key) in [(p.base_crit_chance, "crit_chance"),
            (p.base_crit_damage, "crit_multiplier"), (p.base_status_chance, "status_chance"),
            (p.base_multishot, "multishot")] {
            assert!((actual - expected[key].as_f64().unwrap()).abs() < 1e-7, "{id}: {key}");
        }
        // Serialized fractions retain their rounding; final composition has no native arithmetic claim.
        for (kind, value) in expected["damage"].as_object().unwrap() {
            let actual = w.attack.damage.get(kind).copied().unwrap_or(0.0);
            assert!((actual - value.as_f64().unwrap()).abs() < 3e-5, "{id}: {kind}");
        }
        if let Some(t) = row.get("transition") {
            let g = w.gauge_form.as_ref().unwrap();
            assert_eq!(g.transmute_in_seconds, t["entry_seconds"].as_f64().unwrap(), "{id}: enter");
            assert_eq!(g.transmute_out_seconds, t["exit_seconds"].as_f64().unwrap(), "{id}: exit");
            assert!((p.base_reload - g.transmute_in_seconds - g.transmute_out_seconds).abs() < 1e-10, "{id}: locked gauge downtime");
        }
        if let Some(s) = row.get("sampling") {
            let raw = w.attack.spread.unwrap().sampling.unwrap();
            let panel = p.spread.unwrap().sampling.unwrap();
            assert_eq!(raw.horizontal_ratio, s["horizontal_ratio"].as_f64().unwrap());
            assert_eq!(panel.uniform, s["uniform"].as_bool().unwrap());
        }
        if let Some(r) = row.get("beam_ramp") {
            let beam = p.beam.unwrap();
            let ramp = beam.ramp.unwrap();
            assert_eq!(ramp.rise_seconds, r["rise_seconds"].as_f64().unwrap());
            assert_eq!(ramp.cooldown_delay_seconds, r["cooldown_delay_seconds"].as_f64().unwrap());
            assert_eq!(ramp.cooldown_divisor, r["cooldown_divisor"].as_f64().unwrap());
            let expected = &row["beam_expected"];
            assert_eq!(beam.range_m, expected["range_m"].as_f64().unwrap());
            assert_eq!(beam.damage_radius_m, expected["damage_radius_m"].as_f64().unwrap());
            assert_eq!(beam.chain_hops as u64, expected["chain_hops"].as_u64().unwrap());
            assert_eq!(beam.chain_range_m, expected["chain_range_m"].as_f64().unwrap());
            assert_eq!(beam.chain_damage_per_hop, expected["chain_damage_per_hop"].as_f64().unwrap());
            assert_eq!(beam.chain_compounds, expected["chain_compounds"].as_bool().unwrap());
            assert!(row["beam_expected"]["per_target"].as_bool().unwrap());
            assert!(w.unmodeled.iter().any(|line| line.contains("per-target ramp")));
        }
        assert!(w.unmodeled.iter().any(|line| line.contains("spread growth")), "{id}: remaining live envelope");
    }
}

#[test]
fn primary_sampler_geometry_reproduces_bounded_native_directions() {
    let f = fixture();
    let mut checked = 0;
    for row in f["sampler_vectors"].as_array().unwrap() {
        let spread = row["incoming_spread"].as_f64().unwrap();
        let s = Spread { min_deg: spread, max_deg: spread, sampling: Some(SpreadSampling {
            horizontal_ratio: row["horizontal_scale"].as_f64().unwrap(),
            uniform: row["spread_uniform"].as_bool().unwrap(),
            hip_min_deg: None, hip_max_deg: None,
        }) };
        let samples = &row["samples"];
        let (dev, phase) = s.sample(row["sampled_spread"].as_f64().unwrap() / spread,
            samples["azimuth_radians"].as_f64().unwrap() / std::f64::consts::TAU,
            samples["radial_input"].as_f64().unwrap(),
            row["index"].as_u64().unwrap() as u32, row["count"].as_u64().unwrap() as u32);
        let direction = row["direction"].as_array().unwrap();
        let (x, y, z) = (direction[0].as_f64().unwrap(), direction[1].as_f64().unwrap(), direction[2].as_f64().unwrap());
        let expected = x.hypot(y).atan2(z).to_degrees();
        assert!((dev - expected).abs() < 5e-6, "{row}: {dev} != {expected}");
        let slope = dev.to_radians().tan();
        let phi = phase * std::f64::consts::TAU;
        assert!((slope * phi.cos() - x / z).abs() < 5e-7, "{row}: horizontal");
        assert!((slope * phi.sin() - y / z).abs() < 5e-7, "{row}: vertical");
        // Native Cartesian directions independently pin the arena projection.
        for turn in [0.0_f64, 0.7, 2.1] {
            let (fy, fx) = turn.sin_cos();
            let muzzle = crate::rules::space::Vec2::new(0.3, -0.2);
            let aim = crate::rules::space::Vec2::new(muzzle.x + fx, muzzle.y + fy);
            let norm = x.hypot(y).hypot(z);
            let ray = [(fx * z - fy * x) / norm, (fy * z + fx * x) / norm, y / norm];
            for (forward, sideways) in [(20.0, 0.0), (17.0, 4.0), (10.0, -8.0), (-5.0, 1.0)] {
                let body = crate::rules::space::Vec2::new(muzzle.x + fx * forward - fy * sideways,
                    muzzle.y + fy * forward + fx * sideways);
                let t = ((body.x - muzzle.x) * ray[0] + (body.y - muzzle.y) * ray[1]).max(0.0);
                let expected = [muzzle.x + t * ray[0], muzzle.y + t * ray[1], (t * ray[2]).abs()];
                let projected = crate::rules::space::detonation_of_miss(muzzle, aim, body, dev, phase);
                assert!((projected.at.x - expected[0]).abs() < 5e-5, "{row}: floor x");
                assert!((projected.at.y - expected[1]).abs() < 5e-5, "{row}: floor y");
                assert!((projected.height_m - expected[2]).abs() < 5e-5, "{row}: height");
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 126);
}

#[test]
fn native_primary_accuracy_uses_endpoint_reductions_and_the_actual_aim_gate() {
    use crate::model::{StackPolicy, WeaponBase};
    let f = fixture();
    assert_eq!(f["accuracy_upgrades"].as_array().unwrap().len(), 3);
    for row in f["accuracy_upgrades"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let weapon = id.strip_suffix("_practiced_grip").unwrap_or("felarx");
        let bare = WeaponBase::from_data(weapon, true, &[]);
        let evolved = WeaponBase::from_data(weapon, true, &[id]);
        let native = row["value"].as_f64().unwrap();
        for aiming in [false, true] {
            let mut tenno = crate::data::tenno::default_tenno().clone();
            tenno.state.aiming = aiming;
            let plain = crate::build::loadout::resolve_for(&bare, &[], StackPolicy::Emergent, &tenno).spread.unwrap();
            let changed = crate::build::loadout::resolve_for(&evolved, &[], StackPolicy::Emergent, &tenno).spread.unwrap();
            let factor = if !row["aiming"].as_bool().unwrap() || aiming { 1.0 + native } else { 1.0 };
            assert!((changed.min_deg - plain.min_deg * factor).abs() < 1e-10, "{id}: aiming {aiming}");
            if !aiming {
                let hip = bare.spread.unwrap().sampling.unwrap().hip_min_deg.unwrap();
                assert_eq!(plain.min_deg, hip);
            }
        }
    }
}
