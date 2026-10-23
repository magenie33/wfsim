//! A BATTERY HELD ONE SHOT SHORT, against one fired dry (M121).
use super::*;

/// The Haalvu fired in one of its modes for two minutes, against a body that
/// never dies.
fn haalvu(mode: &str) -> FightParams {
    let modes = crate::data::weapons::play_modes("haalvu");
    let m = modes.iter().find(|m| m.id == mode).expect("a mode of the haalvu");
    let base = crate::model::WeaponBase::from_data(m.weapon_id, false, &[]);
    let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(10.0), &crate::data::arcanes::ArcaneFx::none());
    p.apl_inserted = m.rules();
    p.duration_seconds = 120.0;
    p.foe = frail_target(TargetMode::InfiniteHealth, 0.0, 0.0);
    p
}

/// **EACH FORM DECLARES ITS NEVER-EMPTIED MODE**, and the mode is one line on
/// top of the form's own list.
#[test]
fn the_haalvu_declares_a_never_emptied_mode_on_each_form() {
    let ids: Vec<&str> = crate::data::weapons::play_modes("haalvu").iter().map(|m| m.id).collect();
    assert_eq!(ids, ["base", "base_recharge", "alternate", "alternate_recharge"]);
    assert_eq!(haalvu("base_recharge").apl().to_simc(), "recharge,if=battery.next_shot_empties\nreload,if=!can_fire\nshoot");
}

/// **DRY IS THE 2 S ANIMATION; ONE SHOT SHORT IS THE 1 S REFILL** (M121). Fired
/// dry, every reload costs 2 s. Held short, it never reloads: 149 rounds, then
/// the 0.5 s delay and 149/300 s back to full from the last shot — 12.38 s a
/// cycle against 13.54 s, 8.6% more shots.
#[test]
fn a_haalvu_held_short_refills_in_a_second_and_never_reloads() {
    let dry = monte_carlo(&haalvu("base"), 2, 7);
    let short = monte_carlo(&haalvu("base_recharge"), 2, 7);
    assert!(dry.mean_reloads >= 8.0, "fired dry it reloads: {}", dry.mean_reloads);
    assert!((dry.mean_downtime_seconds / dry.mean_reloads - 2.0).abs() < 1e-6,
        "each reload is the 2 s animation: {} over {}", dry.mean_downtime_seconds, dry.mean_reloads);
    assert_eq!(short.mean_reloads, 0.0, "held short it never empties");
    let gain = short.mean_shots / dry.mean_shots;
    assert!((1.07..=1.10).contains(&gain), "{} against {} shots", short.mean_shots, dry.mean_shots);
}

/// **A SHOT THAT COSTS NOTHING NEVER HOLDS**, and neither does an empty battery
/// or a magazine that is not one.
#[test]
fn only_a_battery_shot_that_empties_it_holds() {
    let p = haalvu("base_recharge");
    let holds = |loaded, cost| crate::fight::state::battery_would_empty(&p, loaded, cost);
    assert!(holds(1.0, 1.0) && holds(1.5, 1.5) && holds(0.5, 1.0));
    assert!(!holds(2.0, 1.0), "two left is one to fire");
    assert!(!holds(1.0, 0.0), "infinite ammo");
    assert!(!holds(0.0, 1.0), "already empty");
    let magazine = FightParams { battery: None, ..p.clone() };
    assert!(!crate::fight::state::battery_would_empty(&magazine, 1.0, 1.0));
}

/// **A DECLARED MODE ASKS ONLY WHAT ITS FORM CAN ANSWER**: a battery condition
/// on a form with a magazine never holds, and the mode would be the base mode
/// under another name.
#[test]
fn a_battery_rule_is_declared_only_on_a_battery() {
    use crate::data::apl::When;
    let wrong: Vec<String> = crate::data::weapons::all().iter()
        .flat_map(|w| w.modes.iter().map(move |m| (w, m)))
        .filter(|(w, m)| w.battery.is_none() && m.rules.0.iter().any(|r| r.when == When::BatteryWouldEmpty))
        .map(|(w, m)| format!("{}: {}", w.id, m.id))
        .collect();
    assert!(wrong.is_empty(), "{wrong:?}");
}
