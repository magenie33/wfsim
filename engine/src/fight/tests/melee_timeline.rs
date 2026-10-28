//! The Gemini Cross timeline, checked against independent package evidence.
use super::*;
use serde_norway::Value;

fn package() -> Value {
    serde_norway::from_str(include_str!("../../../../tests/fixtures/melee/package-2026-10-08.json")).unwrap()
}

fn fixture(form: &str, extra: &[&str]) -> FightParams {
    let weapon = if form == "neutral" { "praedos".to_string() } else { format!("praedos_{form}") };
    let mut base = crate::model::WeaponBase::from_data(&weapon, true, &[]);
    base.base_fire_rate = 1.0;
    base.base_crit_chance = 0.0;
    base.base_status_chance = 0.0;
    let pool = crate::data::mods::pool_for_weapon(&weapon);
    let ids = std::iter::once("gemini_cross").chain(extra.iter().copied());
    let mods = ids.map(|id| pool.iter().find(|m| m.id == id).unwrap()).collect::<Vec<_>>();
    let panel = crate::build::loadout::resolve(&base, &mods, crate::model::StackPolicy::Emergent);
    let arena = crate::arena::Arena::training(12.0);
    FightParams::for_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none(), || panel.clone())
}

fn own_contacts(p: &FightParams) -> Vec<f64> {
    let rec = record(p, 7, 0.0, f64::INFINITY, 30_000, 0);
    rec.events().iter().filter_map(|e| match &e.kind {
        crate::record::Kind::Damage(d) if d.origin == crate::record::Origin::Own => Some(e.t),
        _ => None,
    }).collect()
}

fn near(got: &[f64], want: &[f64]) {
    assert!(got.len() >= want.len(), "contacts {got:?}; expected {want:?}");
    for (a, b) in got.iter().zip(want) {
        assert!((a - b).abs() < 1e-8, "contacts {got:?}; expected {want:?}");
    }
}

#[test]
fn all_six_gemini_cross_modes_follow_the_package_events_and_recovery() {
    let data = package();
    let tree = &data["trees"]["/Lotus/Weapons/Tenno/Melee/MeleeTrees/TonfaCmbOneMeleeTree"];
    for (form, context) in [("neutral", "CC_GROUND"), ("forward", "CC_GROUND_BRANCH_A"),
        ("block", "CC_GROUND_BRANCH_C"), ("block_forward", "CC_GROUND_BRANCH_B"),
        ("heavy", "CC_GROUND_HEAVY"), ("slide", "CC_SLIDING")] {
        let mut expected = Vec::new();
        let mut start = 0.0;
        for path in tree["contexts"][context].as_sequence().unwrap() {
            let a = &data["attacks"][path.as_str().unwrap()];
            let duration = a["duration"].as_f64().unwrap();
            if form == "heavy" { start += 0.69999999; }
            let mut events = a["hits"].as_sequence().unwrap().iter()
                .map(|e| start + duration * e["t"].as_f64().unwrap()).collect::<Vec<_>>();
            events.extend(a["events"].as_sequence().unwrap().iter()
                .filter(|e| e["event"].as_str() == Some("MeleeSlam"))
                .map(|e| start + duration * e["t"].as_f64().unwrap()));
            events.sort_by(f64::total_cmp);
            expected.extend(events);
            start += duration;
        }
        let cycle = start;
        let mut repeated = expected.clone();
        repeated.extend(expected.iter().map(|t| t + cycle));
        let p = fixture(form, &[]);
        near(&own_contacts(&p), &repeated);
        let faster = fixture(form, &["fury"]);
        if form != "heavy" {
            near(&own_contacts(&faster), &repeated.iter().map(|t| t / 1.3).collect::<Vec<_>>());
        }
        assert_eq!(p.combo_script.iter().filter(|h| h.ends_input()).count(), tree["contexts"][context].as_sequence().unwrap().len());
    }
}

#[test]
fn bare_ground_inputs_use_the_native_tree_and_its_default_aliases() {
    let raw = package();
    for weapon in ["praedos", "magistar"] {
        let native: Value = serde_norway::from_str(crate::data::file(&format!("weapons/melee/{weapon}.yaml")).unwrap()).unwrap();
        let tree = raw["weapons"][native["internal_name"].as_str().unwrap()]["tree"].as_str().unwrap();
        let contexts = &raw["trees"][tree]["contexts"];
        for (form, requested) in [("neutral", "CC_GROUND"), ("forward", "CC_GROUND_BRANCH_A"),
            ("block", "CC_GROUND_BRANCH_C"), ("block_forward", "CC_GROUND_BRANCH_B")] {
            let context = if !contexts[requested].is_null() { requested }
                else if !contexts["CC_GROUND"].is_null() { "CC_GROUND" }
                else { "CC_GROUND_BRANCH_A" };
            let id = if form == "neutral" { weapon.to_string() } else { format!("{weapon}_{form}") };
            let mut base = crate::model::WeaponBase::from_data(&id, true, &[]);
            base.base_fire_rate = 1.0;
            base.base_crit_chance = 0.0;
            base.base_status_chance = 0.0;
            let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
            let arena = crate::arena::Arena::training(12.0);
            let p = FightParams::for_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none(), || panel.clone());
            let mut start = 0.0;
            let mut expected = Vec::new();
            for path in contexts[context].as_sequence().unwrap() {
                let a = &raw["attacks"][path.as_str().unwrap()];
                let duration = a["duration"].as_f64().unwrap();
                let mut events = a["hits"].as_sequence().unwrap().iter()
                    .map(|e| start + duration * e["t"].as_f64().unwrap()).collect::<Vec<_>>();
                events.extend(a["events"].as_sequence().unwrap().iter()
                    .filter(|e| matches!(e["event"].as_str(), Some("MeleeSlam" | "AxeSlam")))
                    .map(|e| start + duration * e["t"].as_f64().unwrap()));
                events.sort_by(f64::total_cmp);
                expected.extend(events);
                start += duration;
            }
            let mut repeated = expected.clone();
            repeated.extend(expected.iter().map(|t| t + start));
            near(&own_contacts(&p), &repeated);
            assert!(crate::data::weapons::combo_error(&id, panel.form, None).is_none());
        }
    }
}

#[test]
fn source_index_overrides_stay_with_their_hand_when_events_are_sorted() {
    let p = fixture("neutral", &[]);
    let events = p.combo_script.iter().filter(|h| h.event_timing.unwrap().input == 1).collect::<Vec<_>>();
    assert_eq!(events.iter().map(|h| h.multiplier).collect::<Vec<_>>(), [1.0, 0.5, 1.0, 1.0, 0.5]);
    assert_eq!(events[2].forced_procs, ["slash"]);
    assert!(events[4].forced_procs.is_empty());
    let p = fixture("block_forward", &[]);
    assert_eq!(p.combo_script.iter().find(|h| h.event_timing.unwrap().input == 2).unwrap().slash_multiplier, 1.25);
}

#[test]
fn tennokai_counts_whole_inputs_and_plays_each_heavy_event() {
    for (form, before, four_inputs, first_light, first_hits) in [
        ("neutral", 16, 0.933333 + 0.833333 + 0.733333 + 0.566667, 0.933333 * 0.574597, 2),
        ("block_forward", 7, 2.2, 1.166667 * 0.483794, 1),
    ] {
        let p = fixture(form, &["disciplines_merit"]);
        let heavy = four_inputs + 0.666667 * 0.191532;
        let next_light = four_inputs + 0.666667 + first_light;
        let mut want = vec![heavy, heavy];
        want.extend(std::iter::repeat_n(next_light, first_hits));
        near(&own_contacts(&p)[before..], &want);
        assert_eq!(p.tennokai.windup_seconds, 0.0);
        let faster_charge = fixture(form, &["disciplines_merit", "killing_blow"]);
        near(&own_contacts(&faster_charge)[before..], &want);
    }
}

#[test]
fn one_input_can_contain_two_actions_with_independent_charge_and_motion() {
    let source = "inputs:\n  - actions:\n      - source: /first\n        animation: /first_anim\n        duration_seconds: 1.0\n        charge: {animation: /charge, duration_seconds: 0.2}\n        events: [{at_seconds: 0.4, kind: sweep, multiplier: 1, combo_points: 1}]\n      - source: /second\n        animation: /second_anim\n        duration_seconds: 2.0\n        charge: {animation: /charge, duration_seconds: 0.5}\n        events: [{at_seconds: 0.6, kind: sweep, multiplier: 1, combo_points: 1}]\n";
    let timeline: crate::model::MeleeTimeline = serde_norway::from_str(source).unwrap();
    let mut p = fixture("neutral", &[]);
    p.combo_script = timeline.compile();
    near(&own_contacts(&p), &[0.6, 2.3, 4.3, 6.0]);
    assert_eq!(p.combo_script.iter().filter(|h| h.ends_input()).count(), 1);
    let mut invalid: crate::model::MeleeTimeline = serde_norway::from_str(source).unwrap();
    invalid.inputs[0].actions[0].events[0].at_seconds = 1.1;
    assert!(std::panic::catch_unwind(|| invalid.compile()).is_err(), "an event outside its animation must fail");
    let mut idle: crate::model::MeleeTimeline = serde_norway::from_str(source).unwrap();
    idle.inputs[0].actions[1].events.clear();
    p.combo_script = idle.compile();
    near(&own_contacts(&p), &[0.6, 4.3, 8.0]);
}

#[test]
fn ordinary_heavy_charge_and_motion_take_separate_speed_buckets() {
    let charge = 0.69999999;
    let strike = 0.666667 * 0.191532;
    near(&own_contacts(&fixture("heavy", &[])), &[charge + strike, charge + strike]);
    near(&own_contacts(&fixture("heavy", &["fury"])), &[charge + strike / 1.3, charge + strike / 1.3]);
    near(&own_contacts(&fixture("heavy", &["killing_blow"])), &[charge / 1.6 + strike, charge / 1.6 + strike]);
}

#[test]
fn a_timed_heavy_waits_for_the_combo_rung_between_actual_strikes() {
    let mut p = fixture("heavy", &["corrupt_charge"]);
    p.fire_rate = 2.0;
    for h in &mut p.combo_script { h.windup_seconds = 0.0; }
    let first = 0.666667 * 0.191532 / 2.0;
    near(&own_contacts(&p), &[first, first, first + 0.5, first + 0.5, first + 1.0, first + 1.0]);
}
