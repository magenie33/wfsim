use super::*;
use crate::rules::damage::DamageVector;

/// A DAMAGING STATUS LANDS ON A FULL OVERGUARD BAR, and its ticks come off
/// the Overguard rather than waiting for it.
///
/// Owner-confirmed in game. Overguard blocks CROWD CONTROL,
/// not damage — and the difference decides how a DoT weapon is scored
/// against every Eximus in the roster, because Overguard carries no armor:
/// while it is up, a tick lands unmitigated on a unit
/// whose health would keep 10% of it.
///
/// It is also the mechanism behind M39 — Secondary Fortifier coming out
/// NEGATIVE at low level, because breaking the pool sooner throws that
/// window away — so if this ever silently flips, that result flips with it
/// and nothing else would say why.
#[test]
fn a_dot_ticks_into_a_full_overguard_bar() {
    let mut p = FightParams {
        damage: DamageVector::new().with(DamageType::Slash, 100.0),
        dot_modified_base: Some(100.0),
        fire_rate: 1.0,
        magazine_size: 1e9,
        duration_seconds: 10.0,
        base_crit_chance: 0.0,
        unmodded_crit_chance: 0.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    p.status_chance = 1.0;
    p.base_status_chance = 1.0;
    // A pool deep enough to survive the run, and ARMOR under it — so a tick
    // that waited for the Overguard would be worth a tenth of one that did
    // not, and the two readings could never be confused.
    p.target.base_overguard = 1e15;
    p.target.base_armor = 2700.0;
    p.target.base_health = 1e15;
    let r = run_once(&p, &mut crate::rules::rng::Rng::new(7));
    assert!(r.procs > 0, "the status has to land at all");
    assert!(r.meter.dot() > 0.0, "and its ticks have to do damage");
    // UNMITIGATED: Overguard has no armor, so the tick keeps its full
    // value. At 2700 armor a tick that had landed on health would keep 10%.
    let ticks = r.meter.dot() / 35.0; // Slash is 35% of ModifiedBase (100)
    assert!(ticks > 5.0, "ticks came out mitigated: {:.1}", ticks);
}
