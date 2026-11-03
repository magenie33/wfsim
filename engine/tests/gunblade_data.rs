// SPDX-License-Identifier: AGPL-3.0-or-later
use wfsim_engine::data::weapons;
use wfsim_engine::model::{FormKind, MeleeEventKind};

#[test]
fn gunblade_heavies_use_ranged_profiles_and_package_firing_markers() {
    let package: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/melee/package-2026-10-08.json"
    )).unwrap();
    let source: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/melee/gunblade-heavy-sources.json"
    )).unwrap();
    for row in source["weapons"].as_array().unwrap() {
        let id = row["weapon"].as_str().unwrap();
        let heavy_id = format!("{id}_heavy");
        let heavy = weapons::all().iter().find(|w| w.id == heavy_id).unwrap();
        assert_eq!(heavy.form_kind(), FormKind::Heavy, "{id}");
        assert_eq!(heavy.group(), id);
        let attack = &heavy.attack;
        assert!(attack.spends_combo, "{id}");
        assert!(attack.follow_through.is_none(), "{id} shoots instead of sweeping");
        assert_eq!(attack.range_m.as_ref().unwrap().metres(), if id == "stropha" { 17.0 } else { 300.0 });
        assert_eq!(attack.multishot, row["attack"]["multishot"].as_f64().unwrap());
        assert_eq!(attack.status_chance, row["attack"]["status_chance"].as_f64().unwrap());
        assert_eq!(attack.crit_chance, row["attack"]["crit_chance"].as_f64().unwrap());
        assert_eq!(attack.crit_multiplier, row["attack"]["crit_multiplier"].as_f64().unwrap());
        for (name, amount) in row["attack"]["damage"].as_object().unwrap() {
            assert_eq!(attack.damage[name], amount.as_f64().unwrap(), "{id} {name}");
        }
        let script = &attack.combo_script;
        let shots_per_input = if id == "sarpa" { 5 } else { 1 };
        assert_eq!(script.len(), 2 * shots_per_input);
        assert_eq!(script.iter().filter(|h| h.ends_input()).count(), 2);
        for (input, action) in row["package_actions"].as_array().unwrap().iter().enumerate() {
            let raw = &package["attacks"][action.as_str().unwrap()];
            let firing = raw["events"].as_array().unwrap().iter()
                .find(|e| e["event"] == "GunbladeGunshot").unwrap();
            let first = &script[input * shots_per_input];
            let want = raw["duration"].as_f64().unwrap() * firing["t"].as_f64().unwrap();
            assert!((first.hit_delay_seconds.unwrap() - want).abs() < 1e-10, "{id}");
            for (n, shot) in script[input * shots_per_input..(input + 1) * shots_per_input].iter().enumerate() {
                assert!(matches!(shot.event_timing.unwrap().kind, MeleeEventKind::Ranged));
                assert_eq!(shot.multiplier, raw["atten"].as_f64().unwrap(), "{id}");
                assert_eq!(shot.combo_points, 0.0);
                assert_eq!(shot.impact_multiplier, 1.25);
                assert_eq!(shot.puncture_multiplier, 1.25);
                assert_eq!(shot.slash_multiplier, 1.25);
                if n > 0 {
                    assert!((shot.hit_delay_seconds.unwrap() - 0.05).abs() < 1e-10);
                }
            }
        }
    }
}

#[test]
fn stropha_heavy_keeps_its_projectile_rules() {
    let heavy = weapons::all().iter().find(|w| w.id == "stropha_heavy").unwrap();
    assert_eq!(heavy.headshot_multiplier, Some(1.0));
    assert!(heavy.attack.infinite_body_punch_through);
    assert_eq!(heavy.attack.projectile_speed_mps, Some(120.0));
    assert_eq!(heavy.attack.multishot, 1.0);
}
