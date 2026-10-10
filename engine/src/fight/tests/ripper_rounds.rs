//! RIPPER ROUNDS — *"On Kill: +3 Punch Through for 7s"* (Dual Toxocyst
//! Incarnon Genesis): a kill opens a 7 s window, the shot reads its metres
//! live, and a ricochet carries none of them (MECHANICS §Ricochet).
//!
//! BEHAVIOURAL, NOT GOLDEN: no in-game reading of the perk exists yet, so these
//! pin the shape the card states and invent no number.
use super::*;

const RIPPER: &str = "dual_toxocyst_ripper_rounds";

fn panel(form: &str, evos: &[&str]) -> crate::build::loadout::ResolvedPanel {
    let base = crate::model::WeaponBase::from_data(form, true, evos);
    crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent)
}

/// The card's one buff, as the panel carries it into the fight.
fn ripper_buff(form: &str) -> crate::model::StackingBuff {
    *panel(form, &[RIPPER])
        .stacking_buffs
        .iter()
        .find(|b| b.grant == crate::model::BuffGrant::PunchThrough)
        .expect("Ripper Rounds grants punch through")
}

/// `form` on the dummy at contact, with `others` standing at `at`. A body that
/// dies to one shot and stands straight back up when `frail`, so every shot is
/// a kill; the never-dying dummy otherwise.
fn dt_among(form: &str, evos: &[&str], at: &[crate::rules::space::Vec2], frail: bool) -> FightParams {
    let foe = || if frail { frail_target(TargetMode::InstantRespawn, 0.0, 0.0) } else { Foe::training_dummy() };
    let mut p = FightParams::from_panel(
        &panel(form, evos),
        &crate::arena::Arena::training(10.0),
        &crate::data::arcanes::ArcaneFx::none(),
    );
    p.foe = foe();
    p.body_parts = mono_body(1.0);
    p.others = at
        .iter()
        .map(|&at| crate::formation::FoeSpec {
            id: String::new(),
            params: foe(),
            body_parts: mono_body(1.0),
            at,
        })
        .collect();
    p
}

/// A COLUMN BEHIND THE TARGET at the multi-target ruler's 3 m spacing, five
/// deep — the ruler's own clip.
fn column() -> Vec<crate::rules::space::Vec2> {
    let t = crate::arena::Arena::training(1.0).target_at;
    (1..=5).map(|i| crate::rules::space::Vec2::new(t.x, t.y + 3.0 * i as f64)).collect()
}

/// THE CARD LOADS AS WHAT IT SAYS, on both forms — evolutions are the
/// group's: a kill-triggered window of one stack, 3 m, 7 s, and nothing on
/// the static metres, which a window that has not opened must not touch.
#[test]
fn ripper_rounds_is_a_seven_second_kill_window_worth_three_metres() {
    for form in ["dual_toxocyst", "dual_toxocyst_incarnon"] {
        let b = ripper_buff(form);
        assert_eq!(b.id, "on_kill_punch_through", "{form}");
        assert_eq!(b.trigger, crate::model::BuffTrigger::Kill, "{form}");
        assert_eq!((b.per_stack, b.max_stacks, b.duration), (3.0, 1, 7.0), "{form}");
        assert_eq!(panel(form, &[RIPPER]).punch_through_m, 0.0, "{form}");
        let def = crate::data::evolutions::get(RIPPER).expect("loaded");
        assert!(def.unmodeled_effects().is_empty(), "{:?}", def.unmodeled_effects());
    }
}

/// THE WINDOW OPENS ON A KILL AND CLOSES 7 S LATER. A round a second against
/// a body that takes ten of them to die: each kill opens the window, it shuts
/// seven seconds on with no kill to refresh it, and the next kill opens it
/// again.
#[test]
fn the_window_opens_on_a_kill_and_lasts_seven_seconds() {
    let mut foe = frail_target(TargetMode::InstantRespawn, 0.0, 0.0);
    foe.base_health = 950.0;
    let p = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 100.0),
        magazine_size: 1000.0,
        fire_rate: 1.0,
        stacking_buffs: vec![ripper_buff("dual_toxocyst")],
        duration_seconds: 40.0,
        body_parts: mono_body(1.0),
        foe,
        ..flat_base()
    };
    let trace = replay(&p, Rng::new(3).state(), 800);
    let i = trace.buffs_of(0).iter().position(|x| x.id == "on_kill_punch_through").expect("on the roster");
    let up: Vec<(f64, u16)> = trace.frames.iter().map(|f| (f.t, f.stacks_of(0)[i])).collect();
    assert!(up.iter().all(|&(_, s)| s <= 1), "one stack: {up:?}");
    // EVERY EDGE, as (when, now up?) — the window's whole history.
    let edges: Vec<(f64, u16)> = up.windows(2).filter(|w| w[0].1 != w[1].1).map(|w| w[1]).collect();
    let first_kill = trace.frames.iter().find(|f| f.kills > 0).expect("the fixture kills").t;
    assert!(edges.len() >= 3, "open, shut, open again: {edges:?}");
    assert_eq!(edges[0].1, 1, "{edges:?}");
    assert!(edges[0].0 >= first_kill && edges[0].0 - first_kill <= 1.0 + 1e-9,
        "it opens at the kill, by the next shot at the latest: {edges:?} after a kill at {first_kill}");
    let frame = p.duration_seconds / 800.0;
    assert!(
        (edges[1].0 - edges[0].0 - 7.0).abs() <= 2.0 * frame,
        "{:.2} s up against the card's 7: {edges:?}",
        edges[1].0 - edges[0].0
    );
    assert_eq!(edges[2].1, 1, "the next kill opens it again: {edges:?}");
}

/// …AND WHILE IT IS UP THE ROUND REACHES THE COLUMN. The Dual Toxocyst has no
/// punch through of its own, so without the card the bodies behind take
/// nothing; with it they take the shot once something has died, and against
/// bodies that never die the window never opens and the card is worth nothing.
#[test]
fn the_window_reaches_the_bodies_behind_the_target() {
    let behind = |evos: &[&str], frail: bool| -> Vec<f64> {
        let s = monte_carlo(&dt_among("dual_toxocyst", evos, &column(), frail), 4, 0x41);
        s.mean_damage_by_body.0[1..=5].to_vec()
    };
    assert!(behind(&[], true).iter().all(|&d| d == 0.0), "no punch through, no column");
    let with = behind(&[RIPPER], true);
    assert_eq!(with.len(), 5);
    assert!(with.iter().all(|&d| d > 0.0), "3 m reaches all five behind: {with:?}");
    assert!(behind(&[RIPPER], false).iter().all(|&d| d == 0.0), "no kill, no window");
}

/// …AND THE NUMBERS OF A FIGHT THE WINDOW NEVER OPENS IN ARE THE NUMBERS
/// WITHOUT THE CARD, to the bit: the live read adds nothing to a line it
/// does not move.
#[test]
fn a_window_that_never_opens_changes_nothing() {
    let run = |evos: &[&str]| monte_carlo(&dt_among("dual_toxocyst_incarnon", evos, &column(), false), 3, 7);
    let (bare, card) = (run(&[]), run(&[RIPPER]));
    assert_eq!(bare.mean_damage.to_bits(), card.mean_damage.to_bits());
    assert_eq!(bare.mean_damage_by_body.0, card.mean_damage_by_body.0);
}

/// A RICOCHET DOES NOT PUNCH THROUGH. Nobody stands on the aim line; a body
/// 1 m to the side is where the bounce goes, and a second one 1 m beyond it,
/// on the bounce's own line. With the window held up the whole fight the
/// first takes the ricochets and the second nothing. The control: with the
/// ricochet taken away, a body BEHIND THE TARGET is still reached, so the
/// window is up and its 3 m are real.
#[test]
fn a_ricochet_does_not_inherit_the_window() {
    let t = crate::arena::Arena::training(1.0).target_at;
    let v = crate::rules::space::Vec2::new;
    let held = |at: &[crate::rules::space::Vec2], ricochet: bool| {
        let mut p = dt_among("dual_toxocyst_incarnon", &[RIPPER], at, false);
        assert!(p.ricochet.is_some(), "the Incarnon form ricochets");
        if !ricochet {
            p.ricochet = None;
        }
        let mut cfg = BuffConfig::new();
        cfg.insert("on_kill_punch_through".to_string(), (1, true));
        p.apply_buff_config(&cfg);
        monte_carlo(&p, 4, 0x52).mean_damage_by_body.0[..3].to_vec()
    };
    let side = held(&[v(t.x + 1.0, t.y), v(t.x + 2.0, t.y)], true);
    assert!(side[1] > 0.0, "the bounce lands on the nearest body: {side:?}");
    assert_eq!(side[2], 0.0, "and goes no further: {side:?}");
    let line = held(&[v(t.x, t.y + 1.0)], false);
    assert!(line[1] > 0.0, "the aimed round does punch through: {line:?}");
}
