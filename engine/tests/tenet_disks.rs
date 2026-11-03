// SPDX-License-Identifier: AGPL-3.0-or-later
use wfsim_engine::data::weapons;
use wfsim_engine::model::{CoBehavior, MeleeEventKind, StackPolicy, WeaponBase};

#[test]
fn agendus_replaces_only_the_first_heavy_with_its_disk() {
    let source = weapons::spec("tenet_agendus_heavy").unwrap();
    let script = &source.attack.combo_script;
    assert_eq!(script.len(), 2);
    assert!(matches!(script[0].event_timing.unwrap().kind, MeleeEventKind::Ranged));
    assert_eq!(script[0].profile.as_deref(), Some("disk"));
    assert!(matches!(script[1].event_timing.unwrap().kind, MeleeEventKind::Sweep));
    assert_eq!(script[1].profile, None);
    assert_eq!(script[1].multiplier, 5.0);
    assert_eq!(script.iter().filter(|h| h.ends_input()).count(), 2);
    let disk = &source.attack.profiles["disk"];
    assert_eq!(disk.damage["electricity"], 2880.0);
    assert_eq!(disk.projectile_speed_mps, Some(80.0));
    assert_eq!(disk.projectile_width_m, 2.7);
    assert_eq!(disk.falloff.as_ref().unwrap().reduction, 0.95);
}

#[test]
fn grigori_emits_one_disk_from_the_second_heavy_marker() {
    let source = weapons::spec("tenet_grigori_heavy").unwrap();
    let script = &source.attack.combo_script;
    assert_eq!(script.len(), 3);
    assert_eq!(script[0].profile, None);
    assert_eq!(script[1].profile, None);
    assert_eq!(script[2].profile.as_deref(), Some("disk"));
    assert_eq!(script[0].event_timing.unwrap().input, 0);
    assert_eq!(script[1].event_timing.unwrap().input, 1);
    assert_eq!(script[2].event_timing.unwrap().input, 1);
    assert_eq!(script.iter().filter(|h| h.ends_input()).count(), 2);
    let package: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/melee/package-2026-10-08.json"
    )).unwrap();
    let action = &package["attacks"]["/Lotus/Weapons/Tenno/Melee/Attacks/ScytheMelee30ChargeB"];
    let marker = action["events"].as_array().unwrap().iter()
        .find(|e| e["event"] == "GunbladeGunshot").unwrap();
    let release = script[1].hit_delay_seconds.unwrap() + script[2].hit_delay_seconds.unwrap();
    assert!((release - marker["t"].as_f64().unwrap() * action["duration"].as_f64().unwrap()).abs() < 1e-10);
    let disk = &source.attack.profiles["disk"];
    assert_eq!(disk.damage.values().sum::<f64>(), 4080.0);
    assert_eq!(disk.projectile_speed_mps, Some(1.0));
    assert!(disk.infinite_body_punch_through);
}

#[test]
fn tenet_disks_keep_independent_co_and_ignore_melee_damage_mods() {
    for (id, fraction) in [("tenet_agendus_heavy", 0.25), ("tenet_grigori_heavy", 1.0)] {
        let base = WeaponBase::from_data(id, false, &[]);
        let plain = wfsim_engine::build::loadout::resolve(&base, &[], StackPolicy::Emergent);
        let mods = wfsim_engine::data::mods::pool_for_weapon(id);
        let pressure = mods.iter().find(|m| m.id == "pressure_point").unwrap();
        let modded = wfsim_engine::build::loadout::resolve(&base, &[pressure], StackPolicy::Emergent);
        assert!(modded.modified_base > plain.modified_base);
        let disk = &plain.melee_profiles["disk"];
        assert_eq!(disk.modified_base, modded.melee_profiles["disk"].modified_base);
        assert_eq!(disk.co_behavior, CoBehavior::Independent);
        assert_eq!(disk.co_base_fraction(), fraction);
    }
}
