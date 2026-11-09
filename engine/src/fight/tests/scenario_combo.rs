use super::*;

fn configured(form: &str, points: u32, held: bool) -> FightParams {
    let base = crate::model::WeaponBase::from_data(form, false, &[]);
    let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let mut params = FightParams::from_panel(&panel, &crate::arena::Arena::training(10.0), &ArcaneFx::none());
    params.apply_buff_config(&BuffConfig::from([("melee_combo".into(), (points, held))]));
    params
}

fn counts(params: &FightParams) -> Vec<u16> {
    let rep = replay(params, 7, 100);
    let at = rep.buffs[0].iter().position(|b| b.id == "melee_combo").expect("the counter is recorded");
    rep.frames.iter().map(|f| f.stacks_of(0)[at]).collect()
}

#[test]
fn scenario_combo_opens_full_then_heavy_attacks_spend_it() {
    let cold = configured("nikana_prime_heavy", 0, false);
    let opening = configured("nikana_prime_heavy", 220, false);
    let maintained = configured("nikana_prime_heavy", 220, true);
    let points = counts(&opening);
    assert_eq!(points[0], 220);
    assert!(points.iter().skip(1).any(|&n| n == 0), "a heavy spends the opening points: {points:?}");
    assert!(counts(&maintained).iter().all(|&n| n >= 220));
    assert_eq!(configured("nikana_prime_heavy", 1000, true).melee_combo_initial, MELEE_COMBO_MAX_POINTS);
    let damage = |p: &FightParams| monte_carlo(p, 4, 7).mean_damage;
    assert!(damage(&cold) < damage(&opening));
    assert!(damage(&opening) < damage(&maintained));
}

#[test]
fn scenario_combo_expires_but_light_hits_can_build_above_the_maintained_points() {
    let mut no_gain = configured("nikana_prime", 100, false);
    no_gain.combo_duration_seconds = 0.1;
    no_gain.combo_gain_chance = -1.0;
    let points = counts(&no_gain);
    assert_eq!(points[0], 100);
    assert!(points.iter().skip(2).all(|&n| n == 0));
    no_gain.melee_combo_held = true;
    assert!(counts(&no_gain).iter().all(|&n| n >= 100));
    assert!(counts(&configured("nikana_prime", 100, true)).iter().any(|&n| n > 100));
}

#[test]
fn scenario_combo_is_separate_from_the_builds_refilling_floor_and_efficiency() {
    let mut opening = configured("nikana_prime_heavy", 100, false);
    opening.initial_combo = 30.0;
    let points = counts(&opening);
    assert_eq!(points[0], 100);
    assert!(points.iter().rev().take(50).all(|&n| n < 100));
    assert!(points.iter().skip(1).any(|&n| n == 30), "the build's floor still refills: {points:?}");
    opening.initial_combo = 0.0;
    opening.heavy_attack_efficiency = 0.9;
    assert!(counts(&opening).contains(&90), "efficiency retains 90% of the first heavy's points");
}

#[test]
fn scenario_combo_does_not_grant_a_counter_to_a_gun() {
    let cold = configured("braton", 0, false);
    let configured = configured("braton", 220, true);
    assert_eq!(configured.melee_combo_initial, 0);
    assert!(!configured.buff_roster().iter().any(|b| b.id == "melee_combo"));
    assert_eq!(monte_carlo(&cold, 4, 7).mean_damage, monte_carlo(&configured, 4, 7).mean_damage);
}

#[test]
fn scenario_combo_remains_configurable_without_a_stance_script() {
    let base = crate::model::WeaponBase::from_data("garuda_prime_talons", false, &[]);
    let mut panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    panel.combo_script.clear();
    let mut params = FightParams::from_panel(&panel, &crate::arena::Arena::training(10.0), &ArcaneFx::none());
    params.apply_buff_config(&BuffConfig::from([("melee_combo".into(), (100, false))]));
    assert!(params.combo_script.is_empty());
    assert_eq!(params.melee_combo_initial, 100);
    assert!(params.buff_roster().iter().any(|b| b.id == "melee_combo"));
}
