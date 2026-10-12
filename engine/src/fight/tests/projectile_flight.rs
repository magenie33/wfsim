//! A SHOT THAT FLIES ARRIVES `gap / speed` AFTER THE PULL (docs/MECHANICS.md
//! §"Projectile flight") — its damage, the buffs its hit triggers and the kill
//! it makes are all stamped at the landing, and the trigger does not wait for
//! it. A shot that does not fly lands inside its own pull, as it always did.
use super::*;
use crate::record::{Kind, Origin};
use crate::rules::space::{Vec2, CONTACT_RANGE_M};

/// `weapon` with `mods`, against a dummy standing `gap_m` from the muzzle.
fn fight(weapon: &str, mods: &[&str], gap_m: f64) -> FightParams {
    let base = crate::model::WeaponBase::from_data(weapon, true, &[]);
    let pool = crate::data::mods::pool_for_weapon(weapon);
    let refs: Vec<_> = mods
        .iter()
        .map(|id| pool.iter().find(|m| m.id == *id).expect("the mod fits"))
        .collect();
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let mut arena = crate::arena::Arena::training(3.0);
    arena.target_at = Vec2::new(0.0, CONTACT_RANGE_M + gap_m);
    let mut p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
    // NOTHING MISSES: a cone at range would drop pellets and with them the
    // one-hit-per-pull pairing these tests read.
    p.spread = None;
    p
}

/// The pulls, and the aimed body's direct hits, in time order.
fn timeline(p: &FightParams) -> (Vec<f64>, Vec<f64>) {
    let rec = record(p, 1, 0.0, p.duration_seconds, 100_000, 0);
    let (mut pulls, mut hits) = (Vec::new(), Vec::new());
    for e in rec.events() {
        match &e.kind {
            Kind::Strike { .. } => pulls.push(e.t),
            Kind::Damage(d) if d.origin == Origin::Own && !d.radial && e.subject == Some(0) => {
                hits.push(e.t)
            }
            _ => {}
        }
    }
    (pulls, hits)
}

fn assert_lands_after(p: &FightParams, flight: f64) {
    let (pulls, hits) = timeline(p);
    let arriving = pulls.iter().filter(|&&t| t + flight < p.duration_seconds).count();
    assert_eq!(hits.len(), arriving, "one hit per pull that arrives inside the fight");
    for (pull, hit) in pulls.iter().zip(&hits) {
        assert!((hit - pull - flight).abs() < 1e-9, "pulled {pull}, landed {hit}, flight {flight}");
    }
}

/// THE BOLTOR at 17 m: 85 m/s is 0.2 s in the air, and the pulls go on at the
/// weapon's own cadence meanwhile.
#[test]
fn a_projectile_lands_the_gap_over_its_speed_after_the_pull() {
    let p = fight("boltor", &[], 17.0);
    assert_eq!(p.projectile_speed_mps, Some(85.0));
    assert_lands_after(&p, 0.2);
    let (pulls, _) = timeline(&p);
    assert!((pulls[1] - pulls[0] - 1.0 / p.fire_rate).abs() < 1e-9, "the trigger does not wait");
}

/// …AND A PROJECTILE SPEED BONUS SHORTENS THE FLIGHT: Terminal Velocity's
/// +60% flies the same 17 m at 136 m/s.
#[test]
fn a_projectile_speed_bonus_pays_through_the_flight() {
    let p = fight("boltor", &["terminal_velocity"], 17.0);
    assert!((p.projectile_speed_mps.unwrap() - 85.0 * 1.6).abs() < 1e-9);
    assert_lands_after(&p, 17.0 / (85.0 * 1.6));
}

/// A HIT-SCAN WEAPON lands inside its own pull at any range, and a projectile
/// at contact has nothing to fly.
#[test]
fn hitscan_and_point_blank_land_at_the_pull() {
    let hitscan = fight("braton_prime", &[], 17.0);
    assert_eq!(hitscan.projectile_speed_mps, None);
    assert_lands_after(&hitscan, 0.0);
    assert_lands_after(&fight("boltor", &[], 0.0), 0.0);
}

/// A HEADSHOT-TRIGGERED BUFF OPENS AT THE LANDING. A +100% fire-rate window on
/// a weak-point hit: at contact the second pull is already quick; at 42.5 m
/// (0.5 s of flight) every pull before the first round arrives keeps the base
/// cadence, and the first one after it takes the buff.
#[test]
fn a_headshot_buff_opens_when_the_round_arrives() {
    let with_window = |gap_m: f64| {
        let mut p = fight("boltor", &[], gap_m);
        p.body_parts = all_head();
        p.stacking_buffs = vec![crate::model::StackingBuff {
            id: "on_headshot_fire_rate",
            trigger: crate::model::BuffTrigger::Headshot,
            grant: crate::model::BuffGrant::FireRate,
            chance: 1.0,
            decay: crate::model::BuffDecay::LoseOneAndReset,
            per_stack: p.fire_rate,
            max_stacks: 1,
            duration: 100.0,
            initial_stacks: 0,
            stacks_per_trigger: 1,
            per_shell: false,
            cleared_by: crate::model::ClearedBy::Nothing,
            card_opens_full: false,
        }];
        p
    };
    let p = with_window(0.0);
    let (slow, quick) = (1.0 / p.fire_rate, 0.5 / p.fire_rate);
    let (pulls, _) = timeline(&p);
    assert!((pulls[1] - pulls[0] - quick).abs() < 1e-9, "at contact: {pulls:?}");

    let (pulls, hits) = timeline(&with_window(42.5));
    assert!((hits[0] - 0.5).abs() < 1e-9, "the first round lands at 0.5 s: {hits:?}");
    for w in pulls.windows(2).take_while(|w| w[0] < hits[0]) {
        assert!((w[1] - w[0] - slow).abs() < 1e-9, "no window before the landing: {pulls:?}");
    }
    let first_after = pulls.iter().position(|&t| t >= hits[0]).expect("a pull after it");
    let gap = pulls[first_after + 1] - pulls[first_after];
    assert!((gap - quick).abs() < 1e-9, "the window is open from the landing on: {pulls:?}");
}

/// THE KILL TIME IS THE LANDING TIME: a one-hit target at 17 m dies 0.2 s
/// after the first pull, not at it.
#[test]
fn a_kill_is_made_when_the_round_arrives() {
    let mut p = fight("boltor", &[], 17.0);
    p.foe.mode = TargetMode::InstantRespawn;
    let at = run_once(&p, &mut Rng::new(1)).first_kill_at.expect("a kill");
    assert!((at - 0.2).abs() < 1e-9, "killed at {at}");
}
