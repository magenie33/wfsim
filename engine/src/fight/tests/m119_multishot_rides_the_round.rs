//! A SHOT AGAINST A BODY IT KILLS (MEASUREMENTS M119): multishot rides the
//! weapon's own rounds, and a body the shot killed takes nothing more of it.
use super::*;

/// A BURST IS ONE ROUND AS FAR AS A KILL GOES: a Burston Prime's three rounds
/// on a body that dies to anything kill it once, not three times.
#[test]
fn m119_a_burst_kills_one_body_however_many_rounds_it_spends() {
    let base = crate::model::WeaponBase::from_data("burston_prime", false, &[]);
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(10.0), &ArcaneFx::none());
    p.foe = frail_target(TargetMode::InstantRespawn, 0.0, 0.0);
    let rec = record(&p, 1, 0.0, f64::INFINITY, 100_000, 0);
    let rounds = rec.events().iter().filter(|e| matches!(e.kind, crate::record::Kind::Strike { .. })).count();
    let kills = rec.events().iter().filter(|e| matches!(&e.kind, crate::record::Kind::Damage(d) if d.killed)).count();
    assert!(rounds >= 9, "the fight fired: {rounds} rounds");
    assert_eq!(kills, rounds.div_ceil(3), "{rounds} rounds, {kills} kills");
}

/// …AND A SHOTGUN'S PELLETS ARE ITS ROUNDS: on one body that dies to anything,
/// the first pellet kills it and the rest of the shot finds nothing.
#[test]
fn m119_a_shot_kills_one_body_once() {
    let base = crate::model::WeaponBase::from_data("hek", false, &[]);
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(10.0), &ArcaneFx::none());
    p.foe = frail_target(TargetMode::InstantRespawn, 0.0, 0.0);
    let rec = record(&p, 1, 0.0, f64::INFINITY, 100_000, 0);
    let shots = rec.events().iter().filter(|e| matches!(e.kind, crate::record::Kind::Strike { .. })).count();
    let kills = rec.events().iter().filter(|e| matches!(&e.kind, crate::record::Kind::Damage(d) if d.killed)).count();
    assert!(shots >= 3, "the fight fired: {shots} shots");
    assert_eq!(kills, shots, "{shots} shots of seven pellets, {kills} kills");
}
