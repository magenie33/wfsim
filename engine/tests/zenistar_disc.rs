// SPDX-License-Identifier: AGPL-3.0-or-later
use wfsim_engine::{arena::Arena, build::loadout, data::{arcanes::ArcaneFx, mods}, fight::{FightParams, record}, model::{StackPolicy, WeaponBase}, record::{Kind, Origin}};

fn panel(ids: &[&str]) -> loadout::ResolvedPanel {
    let pool = mods::pool_for_weapon("zenistar_heavy");
    let refs = ids.iter().map(|id| pool.iter().find(|m| m.id == *id).unwrap()).collect::<Vec<_>>();
    loadout::resolve(&WeaponBase::from_data("zenistar_heavy", true, &[]), &refs, StackPolicy::Emergent)
}

fn events(ids: &[&str], duration: f64) -> (Vec<f64>, Vec<(f64, f64)>) {
    let mut p = FightParams::from_panel(&panel(ids), &Arena::training(duration), &ArcaneFx::none());
    let disc = p.melee_profiles.get_mut("disc").unwrap();
    let field = disc.lingering.as_mut().unwrap();
    field.status_chance = 0.0;
    field.crit_chance = 0.0;
    field.base_crit_chance = 0.0;
    let (mut bursts, mut pulses) = (Vec::new(), Vec::new());
    for e in record(&p, 5, 0.0, duration, 100_000, 0).events() {
        if let Kind::Damage(d) = &e.kind {
            if e.subject != Some(0) { continue; }
            if d.origin == Origin::Own && d.radial { bursts.push(e.t); }
            if d.origin == Origin::Field { pulses.push((e.t, d.effective)); }
        }
    }
    (bursts, pulses)
}

#[test]
fn the_disc_and_aura_ignore_heavy_damage_and_co_but_keep_melee_mods() {
    let plain = panel(&[]);
    let blow = panel(&["killing_blow"]);
    let pressure = panel(&["pressure_point"]);
    let disc = &plain.melee_profiles["disc"];
    assert!(!disc.heavy_damage_mods_apply);
    assert!(!disc.combo_damage_applies);
    assert_eq!(disc.co_behavior, wfsim_engine::model::CoBehavior::Inert);
    assert_eq!(disc.modified_base, 75.0);
    assert_eq!(disc.radial.unwrap().modified_base, 350.0);
    assert_eq!(disc.lingering.unwrap().modified_base, 50.0);
    assert_eq!(disc.lingering.unwrap().modified_base, blow.melee_profiles["disc"].lingering.unwrap().modified_base);
    assert!(pressure.melee_profiles["disc"].lingering.unwrap().modified_base > 50.0);
    let light = loadout::resolve(&WeaponBase::from_data("zenistar", true, &[]), &[], StackPolicy::Emergent);
    assert!(light.melee_heavy.is_none());
    assert!(light.heavy.is_none());
}

#[test]
fn a_deployment_waits_for_expiry_and_combo_extends_lifetime_without_scaling_pulses() {
    let (bursts, pulses) = events(&[], 25.0);
    assert!(bursts.len() >= 2, "{bursts:?}");
    assert!(bursts.windows(2).all(|w| w[1] - w[0] >= 10.0));
    let (charged, charged_pulses) = events(&["corrupt_charge"], 45.0);
    assert!(charged.len() >= 2, "{charged:?}");
    assert!(charged.windows(2).all(|w| w[1] - w[0] >= 20.0), "{charged:?}");
    assert_eq!(pulses[0].1, charged_pulses[0].1);
    assert!(charged_pulses.iter().any(|p| p.0 > charged[0] + 10.0 && p.0 < charged[0] + 20.0));
}

#[test]
fn attack_speed_accelerates_pulses_and_reach_grows_the_aura() {
    let (_, plain) = events(&[], 3.0);
    let (_, faster) = events(&["fury"], 3.0);
    assert!((plain[1].0 - plain[0].0 - 0.8).abs() < 1e-8);
    assert!((faster[1].0 - faster[0].0 - 0.8 / 1.3).abs() < 1e-8);
    let ranged = panel(&["primed_reach"]);
    let plain = panel(&[]);
    let added_range = ranged.range_m - plain.range_m;
    assert!((ranged.melee_profiles["disc"].lingering.unwrap().radius_m - 4.0 - added_range).abs() < 1e-8);
    assert_eq!(ranged.melee_profiles["disc"].range_m, 9.0);
}

#[test]
fn aura_pulses_settle_before_another_combatants_later_attacks() {
    let arena = Arena::training(4.0);
    let mut p = FightParams::from_panel(&panel(&[]), &arena, &ArcaneFx::none());
    p.status_chance = 0.0;
    let disc = p.melee_profiles.get_mut("disc").unwrap();
    disc.status_chance = 0.0;
    disc.radial.as_mut().unwrap().status_chance = 0.0;
    disc.lingering.as_mut().unwrap().status_chance = 0.0;
    let other = loadout::resolve(&WeaponBase::from_data("braton", true, &[]), &[], StackPolicy::Emergent);
    let mut other = FightParams::from_panel(&other, &arena, &ArcaneFx::none());
    other.status_chance = 0.0;
    let p = p.and_also(other);
    let rec = record(&p, 5, 0.0, 4.0, 100_000, 0);
    let times = rec.events().iter().filter_map(|e| matches!(e.kind, Kind::Damage(_)).then_some(e.t)).collect::<Vec<_>>();
    assert!(times.windows(2).all(|w| w[1] + 1e-9 >= w[0]), "{times:?}");
    assert!(rec.events().iter().any(|e| matches!(&e.kind, Kind::Damage(d) if d.origin == Origin::Field)));
}
