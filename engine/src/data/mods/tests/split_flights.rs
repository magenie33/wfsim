/// SPLIT FLIGHTS: a MOD that grants a live stacking buff, which is a route
/// the engine had no door for until this card.
///
/// Every stacking buff before it came from an evolution or an arcane, so a
/// mod that stacked on a trigger had to invent a bespoke `ModEffect`
/// (`OnKillMultishot`, `OnHeadshotKillCritChance`, `ConditionOverload` —
/// three variants for one idea). This one is a trigger already in
/// `BuffTrigger` feeding a grant already in `BuffGrant`, so it carries the
/// whole spec and `resolve` hands it to the panel beside the weapon's own.
#[test]
fn split_flights_reaches_the_panel_as_a_live_stacking_buff() {
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::{BuffDecay, BuffGrant, BuffTrigger};
    use crate::model::StackPolicy;
    let pool = crate::data::mods::class_pool("bow");
    let sf = pool.iter().find(|m| m.id == "split_flights").expect("split flights");

    let base = WeaponBase::from_data("paris_prime", true, &[]);
    let panel = resolve(&base, &[sf], StackPolicy::Emergent);
    let b = panel
        .stacking_buffs
        .iter()
        .find(|b| b.id == "split_flights")
        .expect("the mod's buff reaches the panel");
    assert_eq!(b.trigger, BuffTrigger::Hit, "every landing pellet earns one");
    // The PERCENTAGE bracket, which is where a multishot MOD's bonus goes —
    // not `Multishot` (a flat add) and not `BaseMultishot` (added before
    // mods). Split Chamber's +90% is in the same one, which is also why the
    // two share a family and cannot be equipped together.
    assert_eq!(b.grant, BuffGrant::Multishot);
    assert!((b.per_stack - 1.0).abs() < 1e-9, "+100% a stack at rank 5");
    assert_eq!(b.max_stacks, 4);
    assert!((b.duration - 2.0).abs() < 1e-9);
    assert_eq!(
        b.decay,
        BuffDecay::AllAtOnce,
        "\"Stacks expire all at once after 2 seconds without a hit\""
    );

    // THE PENALTY IS DEGREES OF CONE, outside the accuracy bucket — "Added
    // spread is not affected by bonuses that increase accuracy". The Paris
    // Prime's aimed deviation is 0, so the whole of what this mod costs
    // shows up as widening from nothing rather than as a divided cone.
    let bare = resolve(&base, &[], StackPolicy::Emergent);
    let (b0, s0) = (bare.spread.expect("a bow has a cone"), panel.spread.unwrap());
    assert!(
        (s0.min_deg - b0.min_deg - 7.2).abs() < 1e-6,
        "+1.8 degrees a stack at four stacks: {} -> {}",
        b0.min_deg, s0.min_deg
    );
    assert!((s0.max_deg - b0.max_deg - 7.2).abs() < 1e-6);
}

/// …AND ONLY A BOW CAN HOLD IT. The `bow` pool existed as a NAME on nine
/// weapons and as no directory at all, so this is the first assertion in
/// the app that the tag reaches anything.
#[test]
fn the_bow_pool_reaches_bows_and_only_bows() {
    let has = |w: &str| {
        crate::data::mods::pool_for_weapon(w).iter().any(|m| m.id == "split_flights")
    };
    for w in ["paris", "paris_prime", "mk1_paris", "dread", "cernos_prime"] {
        assert!(has(w), "{w} is a bow");
    }
    for w in ["braton_prime", "vectis_prime", "boar_prime", "lex"] {
        assert!(!has(w), "{w} is not a bow");
    }
}
