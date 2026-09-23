use super::*;
use crate::rules::damage::DamageVector;

/// SECONDARY FORTIFIER MULTIPLIES A TICK, ONCE, WHILE THE OVERGUARD HOLDS.
///
/// Three claims in one fixture, because they only mean anything together:
/// the tick is multiplied at all, it is multiplied ONCE (a faction bonus
/// would be squared here and this is not one), and it stops the moment the
/// pool it is about is gone.
fn bleeder(og: f64, mult: f64) -> FightParams {
    let mut p = FightParams {
        damage: DamageVector::new().with(DamageType::Slash, 100.0),
        dot_modified_base: Some(100.0),
        status_chance: 1.0,
        base_status_chance: 1.0,
        fire_rate: 1.0,
        magazine_size: 1e9,
        duration_seconds: 12.0,
        base_crit_chance: 0.0,
        unmodded_crit_chance: 0.0,
        body_parts: super::mono_body(1.0),
        ..no_status()
    };
    p.status_chance = 1.0;
    p.base_status_chance = 1.0;
    p.arcane.overguard_multiplier = mult;
    p.target.base_overguard = og;
    p.target.base_health = 1e15;
    p
}

#[test]
fn the_arcane_multiplies_a_status_tick_exactly_once() {
    let dot = |og: f64, mult: f64| {
        let mut rng = crate::rules::rng::Rng::new(4);
        run_once(&bleeder(og, mult), &mut rng).tally.dot()
    };
    // A pool deep enough that it survives the run, so every tick lands on
    // Overguard and the ratio is the multiplier itself.
    let plain = dot(1e15, 1.0);
    assert!(plain > 0.0);
    let buffed = dot(1e15, 8.0);
    let r = buffed / plain;
    assert!((r - 8.0).abs() < 1e-6, "once, not squared: x{r:.4} (64 would be twice)");

    // NO OVERGUARD, NO BONUS — "lost entirely after depleting the Overguard
    // from an enemy". Same build, same seed, a target that never had one.
    assert!((dot(0.0, 8.0) - dot(0.0, 1.0)).abs() < 1e-6);
}
