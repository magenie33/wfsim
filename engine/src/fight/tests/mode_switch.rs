//! A LIST THAT SWITCHES MODES INSIDE THE FIGHT (`Action::SwitchMode`).
use super::*;
use crate::data::apl::{Action, Apl, Rule, When};

/// One Haalvu form's params, crit and status off so a round deals its base.
fn form(id: &str) -> (crate::build::loadout::ResolvedPanel, FightParams) {
    let base = crate::model::WeaponBase::from_data(id, false, &[]);
    let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(60.0), &crate::data::arcanes::ArcaneFx::none());
    p.crit_multiplier = 1.0;
    p.base_crit_chance = 0.0;
    p.status_chance = 0.0;
    p.base_status_chance = 0.0;
    p.body_parts = mono_body(1.0);
    p.foe = frail_target(TargetMode::InfiniteHealth, 0.0, 0.0);
    (panel, p)
}

/// The Haalvu's base mode holding its burst, both on `rules`, the switch
/// `seconds` long before reload speed `bonus`.
fn switching(rules: Vec<Rule>, seconds: f64, bonus: f64) -> FightParams {
    let (_, mut base) = form("haalvu");
    let (_, mut alt) = form("haalvu_alt");
    base.apl_inserted = Apl(rules.clone());
    alt.apl_inserted = Apl(rules);
    base.with_toggle(alt, seconds, bonus)
}

fn to(form: &str, when: When) -> Rule {
    Rule { action: Action::SwitchMode { form: form.into() }, when }
}

/// **A SWITCH COSTS ITS SECONDS ON THE RELOAD CLOCK AND THE OTHER MODE FIRES
/// FROM THEN ON**: every round after it is the burst's 34, and reload speed
/// shortens it — +100% halves the 3 s.
#[test]
fn a_switch_pays_its_seconds_and_the_other_mode_fires() {
    let once = |bonus| monte_carlo(&switching(vec![to("alt_fire", When::Always)], 3.0, bonus), 1, 7);
    let (plain, quick) = (once(0.0), once(1.0));
    assert!((plain.mean_damage / plain.mean_pellets - 34.0).abs() < 1e-9, "the burst's round: {}", plain.mean_damage / plain.mean_pellets);
    assert!(plain.mean_downtime_seconds >= 3.0, "{}", plain.mean_downtime_seconds);
    assert!((plain.mean_downtime_seconds - quick.mean_downtime_seconds - 1.5).abs() < 0.2,
        "+100% reload speed takes 1.5 s off: {} against {}", plain.mean_downtime_seconds, quick.mean_downtime_seconds);
}

/// **A FIGHT HOLDING ONE MODE CANNOT SWITCH**: the line is passed over, and the
/// base mode's 33 is all it fires.
#[test]
fn a_switch_to_a_mode_the_fight_does_not_hold_is_passed_over() {
    let (_, mut p) = form("haalvu");
    p.apl_inserted = Apl(vec![to("alt_fire", When::Always)]);
    let s = monte_carlo(&p, 1, 7);
    assert!((s.mean_damage / s.mean_pellets - 33.0).abs() < 1e-9);
}

/// **BOTH WAYS**: switching instead of emptying the battery, each mode on its
/// own line, the fight fires both and never reloads — the two rounds' worths
/// between them, and the battery refilling through every switch.
#[test]
fn a_list_can_switch_back_and_forth() {
    let p = switching(vec![to("alt_fire", When::BatteryWouldEmpty), to("base", When::BatteryWouldEmpty)], 3.0, 0.0);
    let s = monte_carlo(&p, 1, 7);
    let per = s.mean_damage / s.mean_pellets;
    assert!(per > 33.0 + 1e-6 && per < 34.0 - 1e-6, "both modes fired: {per}");
    assert_eq!(s.mean_reloads, 0.0);
}
