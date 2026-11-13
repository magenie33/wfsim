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

fn critical_punch(crit_damage: f64, crit_chance: f64, continuous: bool) -> FightParams {
    let mut p = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 32.0),
        base_crit_chance: crit_chance,
        crit_multiplier: crit_damage,
        unmodded_crit_damage: crit_damage,
        multishot: 1.0,
        continuous,
        punch_through_m: 1.0,
        duration_seconds: 0.01,
        target_at: crate::rules::space::Vec2::new(0.0, 0.5),
        ..flat_base()
    };
    p.others = vec![crate::formation::FoeSpec {
        id: "behind".into(),
        params: p.foe.clone(),
        body_parts: p.body_parts.clone(),
        at: crate::rules::space::Vec2::new(0.0, 3.5),
    }];
    p
}

/// A negative crit factor cannot become a positive damage amplifier behind it.
#[test]
fn m119_punch_through_never_amplifies_a_negative_crit_factor() {
    for continuous in [false, true] {
        let p = critical_punch(0.1, 2.0, continuous);
        let r = run_once(&p, &mut Rng::new(0x119));
        assert!(r.punched_contacts > 0, "the round reaches the second body");
        assert!(r.total_damage() <= 64.0, "continuous={continuous}: {}", r.total_damage());
    }
}

/// A zero-damage aimed crit must leave the next body's independent roll intact.
#[test]
fn m119_punch_through_can_roll_noncritical_after_a_zero_crit_factor() {
    for continuous in [false, true] {
        let p = critical_punch(0.0, 0.5, continuous);
        let mut independent = 0;
        for seed in 0..64 {
            let rec = record(&p, seed, 0.0, f64::INFINITY, 100, 0);
            let aimed_zero = !rec.events().iter().any(|e| matches!(&e.kind,
                crate::record::Kind::Damage(d) if e.subject == Some(0) && d.raw > 1.0));
            let punched_noncrit = rec.events().iter().any(|e| matches!(&e.kind,
                crate::record::Kind::Damage(d) if e.subject == Some(1) && d.crit_tier == 0 && d.raw > 1.0));
            independent += usize::from(aimed_zero && punched_noncrit);
        }
        assert!(independent > 0, "continuous={continuous}: an aimed zero swallowed every later noncrit");
    }
}
