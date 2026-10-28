//! Package animation events, with independently paced charge and motion.
use super::*;

fn fixture(weapon: &str, mods: &[&str], gap: f64) -> FightParams {
    let mut base = crate::model::WeaponBase::from_data(weapon, true, &[]);
    base.base_fire_rate = 1.0;
    base.base_crit_chance = 0.0;
    let pool = crate::data::mods::pool_for_weapon(weapon);
    let mut refs = mods.iter().map(|id| pool.iter().find(|m| m.id == *id).unwrap()).collect::<Vec<_>>();
    if weapon == "magistar" { refs.push(pool.iter().find(|m| m.id == "crushing_ruin").unwrap()); }
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let mut arena = crate::arena::Arena::training(8.0);
    arena.target_at = crate::rules::space::Vec2::new(0.0, crate::rules::space::CONTACT_RANGE_M + gap);
    FightParams::for_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none(), || panel.clone())
}

fn contacts(p: &FightParams) -> Vec<f64> {
    let rec = record(p, 7, 0.0, f64::INFINITY, 20_000, 0);
    let mut times = rec.events().iter().filter_map(|e| match &e.kind {
        crate::record::Kind::Damage(d) if d.origin == crate::record::Origin::Own => Some(e.t),
        _ => None,
    }).collect::<Vec<_>>();
    times.sort_by(f64::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    times
}

fn close(got: &[f64], want: &[f64]) {
    assert!(got.len() >= want.len(), "contacts: {got:?}");
    for (a, b) in got.iter().zip(want) {
        assert!((a - b).abs() < 2e-6, "contacts: {got:?}; expected {want:?}");
    }
}

#[test]
fn magistar_lands_each_sweep_and_slam_at_its_animation_event() {
    let p = fixture("magistar", &[], 0.0);
    let want = [0.585, 1.136972, 1.740721, 2.252384, 2.372181];
    close(&contacts(&p), &want);
    assert_eq!(p.combo_script.iter().filter(|h| h.ends_input()).count(), 3);
    let faster = fixture("magistar", &["fury"], 0.0);
    close(&contacts(&faster), &want.map(|t| t / 1.3));
    let windup = fixture("magistar", &["killing_blow"], 0.0);
    close(&contacts(&windup), &want);
    // The legacy timing is a negative control: it lands at input time.
    let mut untimed = p.clone();
    for h in &mut untimed.combo_script { h.hit_delay_seconds = None; }
    assert_eq!(contacts(&untimed)[0], 0.0);
}

#[test]
fn a_glaive_plays_its_opener_once_then_alternates_consecutive_throws() {
    let mut p = fixture("glaive_charged_throw", &[], 0.0);
    p.apl_inserted = crate::data::apl::Apl(vec![crate::data::apl::Rule {
        action: crate::data::apl::Action::Recall,
        when: crate::data::apl::When::Always,
    }]);
    close(&contacts(&p), &[1.566667, 3.159035, 4.865304, 6.559035]);
    assert_eq!(p.combo_script[2].next_swing_index, Some(1));
    let cycle = crate::model::ComboHit::repeating(&p.combo_script);
    assert_eq!(cycle.len(), 2, "the mode summary excludes the opener");
}

#[test]
fn a_glaive_times_charge_motion_and_projectile_flight_separately() {
    let plain = fixture("glaive_charged_throw", &[], 7.0);
    close(&contacts(&plain), &[1.2 + 0.366667 + 7.0 / 35.0]);
    let faster = fixture("glaive_charged_throw", &["fury"], 7.0);
    close(&contacts(&faster), &[1.2 + 0.366667 / 1.3 + 7.0 / 35.0]);
    let windup = fixture("glaive_charged_throw", &["killing_blow"], 7.0);
    close(&contacts(&windup), &[1.2 / 1.6 + 0.366667 + 7.0 / 35.0]);
    let flight = fixture("glaive_charged_throw", &["whirlwind"], 7.0);
    close(&contacts(&flight), &[1.2 + 0.366667 + 7.0 / (35.0 * 2.8)]);
}
