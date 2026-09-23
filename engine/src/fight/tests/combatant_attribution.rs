//! EVERY NUMBER NAMES WHO DEALT IT, and the names add up to the total.
//!
//! `body` answers who took a damage instance and `Combatant` answers who dealt
//! it. The second half is new, so what has to hold is the same thing the first
//! half holds: the per-attacker figures are the run's damage sorted, not a
//! second account of it, and nothing reaches a total without going through
//! `ledger::settle` and naming an attacker on the way.
use super::*;

/// A fight of the ordinary shape, run once.
fn one_fight() -> RunResult {
    let base = crate::model::WeaponBase::from_data("cernos_prime", false, &[]);
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let arena = crate::arena::Arena::training(10.0);
    let p = FightParams::from_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none());
    run_once(&p, &mut Rng::new(0x5EED))
}

/// THE SUM OVER ATTACKERS IS THE RUN'S DAMAGE. Not approximately: both sides
/// are the same additions through the same door, so any drift at all means a
/// damage site reached a total without naming who dealt it — which is the one
/// thing `ledger` exists to make impossible.
#[test]
fn every_instance_is_credited_to_an_attacker() {
    let r = one_fight();
    let dealt: f64 = r.dealt.by_combatant().0.iter().sum();
    assert!(dealt > 0.0, "the fight dealt nothing at all");
    assert!(
        (dealt - r.meter.effective()).abs() < 1e-6,
        "attributed {dealt} against a meter of {}",
        r.meter.effective()
    );
}

/// …AND THE SAME SUM AS THE BODIES. The two are one fight cut two ways — who
/// dealt it and who took it — so they answer with one number or one of them is
/// counting something the other is not.
#[test]
fn who_dealt_it_and_who_took_it_come_to_the_same_total() {
    let r = one_fight();
    let dealt: f64 = r.dealt.by_combatant().0.iter().sum();
    let taken: f64 = r.spread.by_body().0.iter().sum();
    assert!(
        (dealt - taken).abs() < 1e-6,
        "dealt {dealt} but taken {taken}"
    );
}

/// WITH ONE ATTACKER ON THE FIELD, ALL OF IT IS THE WIELDER'S — and every
/// other seat is empty rather than absent. A roster that shrank to what was
/// used would make "nobody else fired" and "nobody else exists" the same
/// reading, which is exactly the distinction a squad needs.
#[test]
fn a_solo_fight_credits_the_wielder_and_nobody_else() {
    let r = one_fight();
    let by = r.dealt.by_combatant().0;
    assert!(by[Combatant::WIELDER.0] > 0.0, "the wielder dealt nothing");
    assert!(
        by.iter().skip(1).all(|d| *d == 0.0),
        "a seat nobody is in was credited: {by:?}"
    );
}
