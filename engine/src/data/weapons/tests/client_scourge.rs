use super::*;

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../../tests/fixtures/client/scourge-2026-10-28.json")).unwrap()
}

#[test]
fn scourge_components_and_primary_economy_match_pinned_package_inputs() {
    let f = fixture();
    assert_eq!(f["forms"].as_array().unwrap().len(), 4);
    for row in f["forms"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let w = spec(id).unwrap();
        let p = base_panel(id, false);
        let e = &row["expected"];
        assert!((w.attack.fire_rate - e["fire_rate"].as_f64().unwrap()).abs() < 1e-12, "{id}: nominal RPM");
        assert_eq!(w.disposition.unwrap(), e["disposition"].as_f64().unwrap(), "{id}: package disposition");
        for (actual, key) in [(p.base_crit_chance, "crit_chance"),
            (p.base_crit_damage, "crit_multiplier"), (p.base_status_chance, "status_chance")] {
            assert!((actual - e[key].as_f64().unwrap()).abs() < 1e-7, "{id}: {key}");
        }
        for (kind, amount) in e["damage"].as_object().unwrap() {
            assert!((w.attack.damage[kind] - amount.as_f64().unwrap()).abs() < 3e-5, "{id}: direct {kind}");
        }
        let radial = w.attack.radial.as_ref().unwrap();
        for (kind, amount) in e["radial_damage"].as_object().unwrap() {
            assert!((radial.damage[kind] - amount.as_f64().unwrap()).abs() < 1e-7, "{id}: radial {kind}");
        }
        assert_eq!(radial.radius_m, e["radius_m"].as_f64().unwrap());
        assert_eq!(radial.falloff_reduction.unwrap(), e["falloff_reduction"].as_f64().unwrap());
        assert_eq!(radial.status_chance.unwrap(), e["radial_status_chance"].as_f64().unwrap());
        assert!(!radial.takes_condition_overload);
        if row["behavior_index"] == 0 {
            assert_eq!(w.magazine.unwrap(), e["magazine"].as_f64().unwrap());
            assert_eq!(w.ammo_max.unwrap(), e["ammo_max"].as_f64().unwrap());
            assert_eq!(w.reload_seconds.unwrap(), e["reload_seconds"].as_f64().unwrap());
            assert_eq!(w.attack.projectile_speed_mps.unwrap(), e["projectile_speed_mps"].as_f64().unwrap());
        }
    }
}

#[test]
fn scourge_throw_uses_native_aim_profiles_and_retains_the_measured_cycle() {
    for row in fixture()["forms"].as_array().unwrap().iter().filter(|r| r["behavior_index"] == 1) {
        let id = row["id"].as_str().unwrap();
        let w = spec(id).unwrap();
        let conventions = &row["retained_conventions"];
        assert_eq!(w.magazine.unwrap(), conventions["magazine"].as_f64().unwrap());
        assert_eq!(w.reload_seconds.unwrap(), conventions["reload_seconds"].as_f64().unwrap());
        assert_eq!(w.attack.attractor_seconds.unwrap(), conventions["attractor_seconds"].as_f64().unwrap());
        let bare = crate::model::WeaponBase::from_data(id, false, &[]);
        for aiming in [false, true] {
            let mut tenno = crate::data::tenno::default_tenno().clone();
            tenno.state.aiming = aiming;
            let p = crate::build::loadout::resolve_for(&bare, &[], crate::model::StackPolicy::Emergent, &tenno);
            let spread = p.spread.unwrap();
            let profile = &row["accuracy"][if aiming { "aimed" } else { "hip" }];
            assert_eq!(spread.min_deg, profile["min"].as_f64().unwrap(), "{id}: aim {aiming}");
            assert_eq!(spread.max_deg, profile["max"].as_f64().unwrap());
            assert_eq!(spread.sampling.unwrap().horizontal_ratio, row["sampling"]["horizontal_ratio"].as_f64().unwrap());
        }
        assert!(w.unmodeled.iter().any(|line| line.contains("projectile selection is unverified")));
        assert!(w.unmodeled.iter().any(|line| line.contains("spatial application")));
    }
    assert!(spec("scourge_prime").unwrap().unmodeled.iter().any(|line| line.contains("multiplicative claim")));
}
