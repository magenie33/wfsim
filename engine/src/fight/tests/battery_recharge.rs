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
    assert_eq!(haalvu("base_recharge").apl().to_simc(), "wait_for_full,if=battery.next_shot_empties\nwait_for_full,if=battery.needs_refill\nshoot");
}

/// Parsed empty recharge is 2 s delay + 150/300 s refill; a partial refill
/// waits 0.5 s. M121 retains the original observed convention separately.
#[test]
fn a_haalvu_held_short_refills_in_a_second_and_never_reloads() {
    let dry = monte_carlo(&haalvu("base"), 2, 7);
    let short = monte_carlo(&haalvu("base_recharge"), 2, 7);
    assert_eq!(dry.mean_reloads, 0.0, "passive recovery never reloads");
    assert!(dry.mean_downtime_seconds > 15.0, "empty recovery still takes time");
    assert_eq!(short.mean_reloads, 0.0, "held short it never empties");
    let gain = short.mean_shots / dry.mean_shots;
    assert!(gain > 1.07, "{} against {} shots", short.mean_shots, dry.mean_shots);
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

fn battery_form(id: &str) -> FightParams {
    let base = crate::model::WeaponBase::from_data(id, false, &[]);
    let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(30.0), &crate::data::arcanes::ArcaneFx::none());
    p.duration_seconds = 30.0;
    p.foe = frail_target(TargetMode::InfiniteHealth, 0.0, 0.0);
    p.infinite_reserve = false;
    p
}

#[test]
fn coda_bubonico_waits_from_the_last_shot_without_reloading() {
    for (id, rounds) in [("coda_bubonico", 36), ("coda_bubonico_burst", 12)] {
        let p = battery_form(id);
        assert!(p.apl().to_simc().contains("wait_for_full,if=battery.needs_refill"));
        let rec = record(&p, 7, 0.0, 30.0, 20_000, 0);
        let shots: Vec<_> = rec.events().iter().filter_map(|e| {
            matches!(e.kind, crate::record::Kind::Strike { .. }).then_some(e.t)
        }).collect();
        assert!(shots.len() > rounds, "{id}: the battery refills with no reserve");
        assert!((shots[rounds] - shots[rounds - 1] - 4.1).abs() < 1e-9,
            "{id}: parsed empty delay plus refill, counted from the last shot: {shots:?}");
        assert!(!rec.events().iter().any(|e| matches!(e.kind,
            crate::record::Kind::ReloadStart { .. } | crate::record::Kind::ReloadEnd)));
        let plain = run_once(&p, &mut Rng::new(7));
        assert_eq!(plain.reloads, 0);
        let mut buffed = p.clone();
        buffed.base_damage_on_reload = Some(crate::model::TimedBuff { value: 10.0, duration: 100.0, initial_active: false });
        buffed.fire_rate_on_reload = Some(crate::model::TimedBuff { value: 10.0, duration: 100.0, initial_active: false });
        let buffed = run_once(&buffed, &mut Rng::new(7));
        assert_eq!(plain.shots, buffed.shots, "{id}: recovery cannot grant on-reload fire rate");
        assert_eq!(plain.tally.raw(), buffed.tally.raw(), "{id}: recovery cannot grant on-reload damage");
    }
}

#[test]
fn battery_recovery_is_credited_once_and_can_be_interrupted() {
    let p = battery_form("coda_bubonico");
    let b = p.battery.unwrap();
    let (_, mut me) = open(&p, Seat::WIELDER, &mut Rng::new(7), &crate::record::Record::off(), &None);
    me.ammo.loaded = 9.0;
    me.ammo.recover_battery(b, 0.0, 2.0);
    assert_eq!(me.ammo.loaded, 13.5);
    assert_eq!(me.ammo.battery_full_at(b, 2.0), 4.5);
    me.ammo.recover_battery(b, 0.0, 2.0);
    assert_eq!(me.ammo.loaded, 13.5, "a second APL scan grants no extra recovery");
    me.ammo.loaded -= 1.0;
    me.ammo.recover_battery(b, 2.0, 3.0);
    assert_eq!(me.ammo.loaded, 12.5, "firing before full restarts the partial delay");
    me.ammo.recover_battery(b, 2.0, 4.0);
    assert_eq!(me.ammo.loaded, 17.0);
}

#[test]
fn an_empty_animation_remains_a_lock_after_the_battery_recovers() {
    let p = battery_form("coda_bubonico");
    let b = crate::model::Battery {
        regen_per_second: 1000.0, empty_animation_seconds: Some(2.0), ..p.battery.unwrap()
    };
    let (_, mut me) = open(&p, Seat::WIELDER, &mut Rng::new(7), &crate::record::Record::off(), &None);
    me.ammo.loaded = 0.0;
    me.ammo.recover_battery(b, 0.0, 1.0);
    assert_eq!(me.ammo.loaded, 36.0);
    assert_eq!(me.ammo.battery_full_at(b, 1.0), 2.0);
    assert!(me.ammo.battery_needs_refill(1.0, 1.0));
    assert!(!me.ammo.battery_needs_refill(1.0, 2.0));
}
