// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json::Value;
use wfsim_engine::data::weapons::{self, WeaponSpec};

fn weapon(id: &str) -> &'static WeaponSpec {
    weapons::all().iter().find(|w| w.id == id).unwrap()
}

fn sources() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/melee/special-heavy-wiki.json")).unwrap()
}

fn total(v: &Value) -> f64 {
    v.as_object().unwrap().values().map(|n| n.as_f64().unwrap()).sum()
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-8, "{actual} != {expected}");
}

#[test]
fn warfan_volley_counts_damage_and_status_match_each_source_attack() {
    let source = sources();
    for (id, name) in [("quassus_heavy", "Quassus"), ("quassus_prime_heavy", "Quassus Prime"), ("arum_spinosa_heavy", "Arum Spinosa")] {
        let w = weapon(id);
        let base: f64 = w.attack.damage.values().sum();
        assert_eq!(w.attack.combo_script.len(), 2, "{id}");
        for (i, hit) in w.attack.combo_script.iter().enumerate() {
            let attack = &source["weapons"][name]["Attacks"][i + 1];
            close(f64::from(hit.hits), attack["Multishot"].as_f64().unwrap());
            close(base * hit.multiplier, total(&attack["Damage"]));
            close(w.attack.status_chance * hit.status_multiplier, attack["StatusChance"].as_f64().unwrap());
            close(w.attack.crit_chance, attack["CritChance"].as_f64().unwrap());
            close(w.attack.crit_multiplier, attack["CritMultiplier"].as_f64().unwrap());
            assert_eq!(hit.combo_points, 0.0);
            assert_eq!(hit.forced_procs.iter().any(|p| p == "slash"), name != "Arum Spinosa");
        }
        assert!(w.attack.follow_through.is_none(), "projectiles do not inherit melee follow through");
    }
}

#[test]
fn syam_wave_damage_and_release_clocks_are_independent() {
    let source = sources();
    let w = weapon("syam_heavy");
    let script = &w.attack.combo_script;
    let base: f64 = w.attack.damage.values().sum();
    assert_eq!(script.len(), 2);
    for (i, h) in script.iter().enumerate() {
        close(base * h.multiplier, total(&source["weapons"]["Syam"]["Attacks"][i + 1]["Damage"]));
    }
    assert_eq!(script[0].forced_procs, ["impact"]);
    assert!(script[1].forced_procs.is_empty());
    close(script[0].hit_delay_seconds.unwrap(), 0.833333 * 0.12);
    close(script[1].hit_delay_seconds.unwrap(), 0.12);
    assert!(w.attack.infinite_body_punch_through);
}

#[test]
fn tak_and_lug_keeps_four_hits_and_only_three_forced_blasts() {
    let source = sources();
    let w = weapon("tak_and_lug_heavy");
    let base: f64 = w.attack.damage.values().sum();
    let script = &w.attack.combo_script;
    assert_eq!(script.len(), 4);
    close(script.iter().map(|h| h.multiplier * base).sum(), source["weapons"]["Tak & Lug"]["HeavyAttack"].as_f64().unwrap());
    assert!(script[0].forced_procs.is_empty());
    assert!(script[1..].iter().all(|h| h.forced_procs == ["blast"]));
    close(script[0].hit_delay_seconds.unwrap(), 1.1 * 0.15000001);
}

#[test]
fn ranged_heavies_keep_projectile_stats_separate_from_the_melee_weapon() {
    let source = sources();
    let edun = weapon("edun_heavy");
    close(edun.attack.damage.values().sum(), total(&source["weapons"]["Edun"]["Attacks"][1]["Damage"]));
    close(edun.attack.crit_chance, 0.3);
    close(edun.co_base_fraction.unwrap(), 0.25);
    let argo = weapon("argo_and_vel_heavy");
    let hit = &argo.attack.combo_script[0];
    close(argo.attack.damage.values().sum::<f64>() * hit.multiplier, 600.0);
    close(argo.attack.radial.as_ref().unwrap().damage.values().sum::<f64>() * hit.multiplier, 300.0);
}
