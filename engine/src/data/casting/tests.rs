use super::*;
use crate::data::apl::{Rule, When};

const SLING: f64 = TRANSFERENCE_SECONDS + CHAINED_SLING_SECONDS_UNMEASURED;
const SLING_TRIP: Action = Action::Operator { sling: true, ability: false };

fn rule(action: Action, when: When) -> Rule {
    Rule { action, when }
}
fn cast(id: &str) -> Action {
    Action::Cast { ability: id.into() }
}

/// A frame at `strength` and `duration`, Warcry picked, in `school`, fighting
/// with a `slot` weapon that `summoned_by` summons.
#[allow(clippy::too_many_arguments)]
fn frame(apl: Vec<Rule>, strength: f64, duration: f64, school: &str, summoned_by: Option<&str>, slot: &'static str, arcanes: CastArcanes, augments: &[&str]) -> Arc<FrameSpec> {
    let picks = [AbilityPick { id: "warcry", duration_seconds: Some(20.0), element: None }];
    let caster = Caster { strength, duration, augments, ..Default::default() };
    let assumed = abilities::resolve(&picks, &caster, "claws", slot);
    Arc::new(spec(&Apl(apl), &caster, &picks, &assumed, school, summoned_by, "claws", slot, &arcanes))
}

/// THE FRAME'S TURNS OVER `seconds`, one every `step` — the shot loop's
/// cadence — with `kills(t)` made by then. The windows and the busy time.
fn drive(spec: &Arc<FrameSpec>, seconds: f64, step: f64, kills: impl Fn(f64) -> u32) -> (Vec<ActiveAbility>, f64, FrameRuntime) {
    let live = Arc::new(Mutex::new(spec.opening()));
    let mut rt = FrameRuntime::new(spec.clone(), live.clone());
    let (mut t, mut busy) = (0.0, 0.0);
    while t < seconds {
        let after = rt.act_all(t, kills(t));
        busy += after - t;
        t = after + step;
    }
    let windows = live.lock().expect("one thread").clone();
    (windows, busy, rt)
}

fn casts(w: &[ActiveAbility]) -> Vec<&ActiveAbility> {
    w.iter().filter(|a| a.id == "warcry" && a.starts_at_seconds.is_finite()).collect()
}

fn fire_rate(a: &ActiveAbility) -> f64 {
    a.effects
        .iter()
        .find_map(|e| match e {
            abilities::AbilityEffect::FireRate(v) => Some(*v),
            _ => None,
        })
        .expect("warcry grants attack speed")
}

/// **A CAST LASTS THE CARD'S DURATION TIMES THE FRAME'S** — 20 s at 150% is
/// 30 s, whatever the pick typed — and IS RECAST ONLY ONCE IT IS DOWN.
#[test]
fn a_cast_lasts_the_builds_duration_and_is_recast_when_down() {
    let f = frame(vec![rule(cast("warcry"), When::Always)], 1.0, 1.5, "vazarin", None, "melee", CastArcanes::NONE, &[]);
    let (w, busy, _) = drive(&f, 100.0, 0.1, |_| 0);
    let c = casts(&w);
    assert!((c[0].ends_at_seconds - c[0].starts_at_seconds - 30.0).abs() < 1e-9, "{:?}", c[0]);
    for pair in c.windows(2) {
        assert!(pair[1].starts_at_seconds >= pair[0].ends_at_seconds - 1e-9, "not before it lapsed: {pair:?}");
        assert!(pair[1].starts_at_seconds - pair[0].ends_at_seconds < 0.2, "at the first turn after: {pair:?}");
    }
    assert!(busy > 0.0, "each cast rooted the frame");
}

/// **A MELEE KILL LENGTHENS THE WINDOW, AND THE RECAST WAITS FOR IT** — Eternal
/// War moves the lapse, so the next cast is later than the bare window, and
/// never past double the window.
#[test]
fn eternal_war_moves_the_recast() {
    let rules = || vec![rule(cast("warcry"), When::Always)];
    let bare = frame(rules(), 1.0, 1.0, "vazarin", None, "melee", CastArcanes::NONE, &[]);
    let grown = frame(rules(), 1.0, 1.0, "vazarin", None, "melee", CastArcanes::NONE, &["eternal_war"]);
    let one_a_second = |t: f64| t as u32;
    let (b, _, _) = drive(&bare, 60.0, 0.1, one_a_second);
    let (g, _, _) = drive(&grown, 60.0, 0.1, one_a_second);
    let (b, g) = (casts(&b), casts(&g));
    assert!(g[1].starts_at_seconds > b[1].starts_at_seconds + 5.0, "{} vs {}", g[1].starts_at_seconds, b[1].starts_at_seconds);
    assert!(g[0].ends_at_seconds <= g[0].starts_at_seconds + 40.0 + 1e-9, "capped at double: {:?}", g[0]);
    // …and a gun's kills are not melee kills.
    let gun = frame(rules(), 1.0, 1.0, "vazarin", None, "primary", CastArcanes::NONE, &["eternal_war"]);
    let (p, _, _) = drive(&gun, 60.0, 0.1, one_a_second);
    assert!((casts(&p)[1].starts_at_seconds - b[1].starts_at_seconds).abs() < 1e-9);
}

/// **A CAST SNAPSHOTS THE STRENGTH OF ITS INSTANT** (M105): cast inside Sling
/// Strength's window, Warcry keeps the +40% after the window lapses.
#[test]
fn a_cast_keeps_the_strength_it_was_cast_at() {
    let f = frame(
        vec![rule(SLING_TRIP, When::Once), rule(cast("warcry"), When::Always)],
        1.0, 1.0, "madurai", None, "melee", CastArcanes::NONE, &[],
    );
    let (w, _, _) = drive(&f, 25.0, 0.1, |_| 0);
    let first = casts(&w)[0];
    assert!((first.starts_at_seconds - SLING).abs() < 1e-9, "after the trip");
    assert!((first.snapshot_strength - 1.4).abs() < 1e-9);
    let plain = frame(vec![rule(cast("warcry"), When::Always)], 1.0, 1.0, "vazarin", None, "melee", CastArcanes::NONE, &[]);
    let (p, _, _) = drive(&plain, 5.0, 0.1, |_| 0);
    assert!((fire_rate(first) - fire_rate(casts(&p)[0]) * 1.4).abs() < 1e-9);
    assert!(first.live_at(SLING + 19.9), "the sling lapsed at {}, the snapshot did not", SLING + 20.0);
}

/// **AN EXALTED WEAPON IS THE STRENGTH ITS SUMMONING READ** — and a sling after
/// the summon earns the claws nothing; one the list never casts was out before
/// the fight, at the frame's own.
#[test]
fn the_summon_snapshots_the_strength_of_its_cast() {
    let at = |rules: Vec<Rule>| frame(rules, 2.0, 1.0, "madurai", Some("hysteria"), "melee", CastArcanes::NONE, &[]).summon();
    let (sling, hysteria) = (rule(SLING_TRIP, When::Once), rule(cast("hysteria"), When::Always));
    let before = at(vec![sling.clone(), hysteria.clone()]);
    assert_eq!(before.at_seconds, SLING);
    assert!((before.strength - 2.4).abs() < 1e-9);
    assert!((at(vec![hysteria, sling]).strength - 2.0).abs() < 1e-9);
    assert_eq!(at(vec![]), Summon { at_seconds: f64::NEG_INFINITY, strength: 2.0 });
}

/// **RECAST FOR STRENGTH, BUT ONLY WITH EVERY STACK IN** — Molt Augmented
/// climbing kill by kill does not recast Warcry at one stack; at the cap it
/// recasts once for the higher snapshot, and not again.
#[test]
fn strength_gain_waits_for_full_stacks() {
    let molt = CastArcanes { per_kill: Some((0.0024, 250, 0)), ..CastArcanes::NONE };
    let f = frame(vec![rule(cast("warcry"), When::StrengthGain)], 1.0, 10.0, "vazarin", None, "melee", molt, &[]);
    // 5 kills a second: full at 50 s, inside the first 200 s window.
    let (w, _, _) = drive(&f, 100.0, 0.1, |t| (t * 5.0) as u32);
    let c = casts(&w);
    assert_eq!(c.len(), 2, "one at the buzzer, one at the cap: {:?}", c.iter().map(|a| a.starts_at_seconds).collect::<Vec<_>>());
    assert!((c[1].starts_at_seconds - 50.0).abs() < 0.2);
    assert!((c[1].snapshot_strength - 1.6).abs() < 1e-9);
    assert!(c[0].ends_at_seconds <= c[1].starts_at_seconds + 1e-9, "the recast replaced the running one");
    // …and a frame with no stacking source never recasts early.
    let plain = frame(vec![rule(cast("warcry"), When::StrengthGain)], 1.0, 10.0, "vazarin", None, "melee", CastArcanes::NONE, &[]);
    let (p, _, _) = drive(&plain, 100.0, 0.1, |t| (t * 5.0) as u32);
    assert_eq!(casts(&p).len(), 1);
}

/// **MOLT AUGMENTED'S KILLS REACH THE NEXT SNAPSHOT** — the fight adds a stack
/// a kill on top of the ones it opened with.
#[test]
fn molt_augmented_kills_reach_the_next_cast() {
    let molt = CastArcanes { per_kill: Some((0.0024, 250, 50)), ..CastArcanes::NONE };
    let f = frame(vec![rule(cast("warcry"), When::Always)], 1.12, 1.0, "vazarin", None, "melee", molt, &[]);
    let (w, _, rt) = drive(&f, 25.0, 0.1, |t| if t >= 10.0 { 100 } else { 0 });
    let c = casts(&w);
    assert!((c[0].snapshot_strength - 1.12).abs() < 1e-9);
    assert!((c[1].snapshot_strength - 1.36).abs() < 1e-9, "{}", c[1].snapshot_strength);
    assert!((rt.strength_now(0.0, 1000) - 1.6).abs() < 1e-9, "capped at 250 stacks");
}

/// **MOLT VIGOR IS SPENT BY THE NEXT CAST, AND ONLY AN OPERATOR ABILITY ARMS
/// IT** — a sling alone does not (W`Molt_Vigor`).
#[test]
fn molt_vigor_is_armed_by_an_operator_ability_and_spent_once() {
    let vigor = CastArcanes { after_operator_ability: 0.45, ..CastArcanes::NONE };
    let at = |trip: Action| {
        frame(vec![rule(trip, When::Once), rule(cast("hysteria"), When::Always)], 1.0, 1.0, "madurai", Some("hysteria"), "melee", vigor.clone(), &[])
            .summon()
            .strength
    };
    assert!((at(Action::Operator { sling: false, ability: true }) - 1.45).abs() < 1e-9);
    assert!((at(Action::Operator { sling: true, ability: true }) - 1.85).abs() < 1e-9, "both, one trip");
    assert!((at(SLING_TRIP) - 1.4).abs() < 1e-9, "a sling arms nothing");
}

/// **POWER RAMP: EACH CAST ARMS THE NEXT, AND A REPEAT DROPS IT TO ZERO AND
/// ARMS NOTHING** (M105).
#[test]
fn power_ramp_stacks_across_casts_and_a_repeat_empties_it() {
    let ramp = CastArcanes { per_cast_stack: Some((0.09, 4)), ..CastArcanes::NONE };
    let at = |rules: Vec<Rule>| frame(rules, 1.0, 1.0, "vazarin", Some("hysteria"), "melee", ramp.clone(), &[]).summon().strength;
    let (wc, hy) = (rule(cast("warcry"), When::Once), rule(cast("hysteria"), When::Always));
    assert!((at(vec![wc.clone(), hy.clone()]) - 1.09).abs() < 1e-9);
    assert!((at(vec![wc.clone(), wc, hy]) - 1.0).abs() < 1e-9);
}

/// A TRIP THAT EARNS NOTHING STILL COSTS ITS TIME, once.
#[test]
fn a_trip_that_earns_nothing_costs_its_time_once() {
    let f = frame(vec![rule(SLING_TRIP, When::Always)], 1.0, 1.0, "vazarin", None, "melee", CastArcanes::NONE, &[]);
    let (_, busy, _) = drive(&f, 60.0, 0.1, |_| 0);
    assert!((busy - SLING).abs() < 1e-9);
}

/// AN ABILITY THE LIST NEVER NAMES IS ASSUMED UP, untouched, and costs nothing.
#[test]
fn an_ability_the_list_never_names_is_assumed_up() {
    let f = frame(vec![], 1.0, 1.0, "vazarin", None, "melee", CastArcanes::NONE, &[]);
    assert!(!f.needed());
    assert_eq!(f.opening(), f.assumed);
}

/// `remains<N` RE-DOES IT N SECONDS EARLY, so a sling's window never lapses —
/// and the overlap is a refresh, not a second +40%.
#[test]
fn a_lead_keeps_the_window_up_and_a_refresh_does_not_stack() {
    let f = frame(
        vec![rule(SLING_TRIP, When::BuffRemainsUnder { ability: "sling_strength".into(), seconds: SLING + 0.2 })],
        1.0, 1.0, "madurai", None, "melee", CastArcanes::NONE, &[],
    );
    let (_, _, rt) = drive(&f, 60.0, 0.1, |_| 0);
    for t in [SLING + 0.05, 25.0, 45.0, 59.0] {
        assert!((rt.strength_now(t, 0) - 1.4).abs() < 1e-9, "at {t}: {}", rt.strength_now(t, 0));
    }
}
