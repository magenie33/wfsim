use super::*;

/// One impact hit of `amount` a second for ten seconds, crit and status off.
fn tiny(amount: f64, foe: Foe) -> FightParams {
    FightParams {
        damage: DamageVector::new().with(DamageType::Impact, amount),
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        foe,
        ..no_status()
    }
}

/// THE 1-DAMAGE FLOOR HOLDS WITHOUT ARMOUR. A 0.3 hit on an unarmoured body
/// deals 1 — the floor is the owner's reading, extended past wiki `Armor`'s
/// (docs/MEASUREMENTS.md queue), and this is what it states.
#[test]
fn a_hit_below_one_deals_one_on_an_unarmoured_body() {
    let s = monte_carlo(&tiny(0.3, frail_target(TargetMode::InfiniteHealth, 0.0, 0.0)), 3, 7);
    assert!((s.mean_effective_damage - s.mean_shots).abs() < 1e-9,
        "{} damage over {} shots", s.mean_effective_damage, s.mean_shots);
    // …and a hit already above it is untouched.
    let big = monte_carlo(&tiny(40.0, frail_target(TargetMode::InfiniteHealth, 0.0, 0.0)), 3, 7);
    assert!((big.mean_effective_damage - 40.0 * big.mean_shots).abs() < 1e-6);
}

/// …ON OVERGUARD TOO, which is the first pool a Thrax Centurion shows.
#[test]
fn a_hit_below_one_deals_one_to_overguard() {
    let s = monte_carlo(&tiny(0.3, frail_target(TargetMode::InfiniteHealth, 0.0, 1000.0)), 3, 7);
    assert!((s.mean_effective_damage - s.mean_shots).abs() < 1e-9,
        "{} damage over {} shots", s.mean_effective_damage, s.mean_shots);
}

/// NEGATIVE DAMAGE HEALS NOTHING and deals the floor. A Damage total below
/// −100% (a riven malus) drives every hit below zero; routed as it stood it
/// ADDED to the Overguard it was taken from.
#[test]
fn a_hit_below_zero_deals_one_and_never_heals() {
    // The vector a −145% Damage total leaves behind: `resolve` scales the base
    // by `1 + bonus`, which is negative.
    let s = monte_carlo(&tiny(-18.0, frail_target(TargetMode::InfiniteHealth, 0.0, 1000.0)), 3, 7);
    assert!((s.mean_effective_damage - s.mean_shots).abs() < 1e-9,
        "{} damage over {} shots", s.mean_effective_damage, s.mean_shots);
}

/// AN IMMUNITY STAYS ONE. A column reading 0 lets nothing through, and the
/// floor lifts what landed — it does not create a hit.
#[test]
fn the_floor_never_breaks_an_immunity() {
    let mut foe = frail_target(TargetMode::InfiniteHealth, 0.0, 0.0);
    foe.type_mods.faction = crate::data::factions::Column::from_multipliers(&[(DamageType::Impact, 0.0)]);
    let s = monte_carlo(&tiny(40.0, foe), 3, 7);
    assert_eq!(s.mean_effective_damage, 0.0);
}
