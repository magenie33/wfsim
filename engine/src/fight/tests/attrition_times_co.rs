use super::*;

/// DEVASTATING ATTRITION AND GUN CO MULTIPLY, they do not share a bracket.
///
/// MEASURED IN GAME measured, which is what makes this a
/// fact rather than a reading. It also agrees with both sources: the
/// perk's own wiki note says "multiplicative to base damage bonuses such
/// as Hornet Strike", and the weapon sits on the CO catalog's
/// Multiplying row — so neither term is in the base-damage bucket and they
/// have nothing to share.
///
/// The measurement is what this test is for. "They are separate factors in
/// the product" is a claim about the code; a ratio of ratios is a claim
/// about the number, and only the second one can be wrong quietly.
///
/// The test is a ratio of ratios: whatever the perk is worth alone, and
/// whatever CO is worth alone, having both must be worth their product. If
/// either ever joined the other's bucket the product would collapse toward
/// the larger of the two.
#[test]
fn devastating_attrition_multiplies_with_gun_condition_overload() {
    let base = crate::model::WeaponBase::from_data("felarx", true, &[]);
    let panel = crate::loadout::resolve(&base, &[], crate::model::StackPolicy::AssumedMax);
    assert_eq!(
        panel.co_behavior,
        crate::model::CoBehavior::Independent,
        "the Felarx is on the catalog's Multiplying row, both modes"
    );
    let arena = crate::arena::Arena::training(30.0);

    // Four fights that differ ONLY in the two terms under test, on one
    // seed, so the crit rolls and the shot timing are identical.
    let run = |attrition: bool, co: bool| {
        let mut p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
        // Chance 1.0, not the perk's own 0.5: a coin flip inside the
        // measurement would need thousands of runs to say anything, and
        // the question is about the BRACKET, not about the odds.
        p.noncrit_bonus = attrition.then_some((1.0, 20.0));
        p.co_per_type = if co { 0.8 } else { 0.0 };
        let mut rng = crate::rng::Rng::new(11);
        run_once(&p, &mut rng).total_damage()
    };
    let plain = run(false, false);
    let with_attr = run(true, false);
    let with_co = run(false, true);
    let with_both = run(true, true);
    assert!(plain > 0.0 && with_attr > plain && with_co > plain);

    let r_attr = with_attr / plain;
    let r_co = with_co / plain;
    let r_both = with_both / plain;
    assert!(
        (r_both - r_attr * r_co).abs() < r_both * 0.02,
        "multiplicative: attrition x{r_attr:.2}, CO x{r_co:.2}, both x{r_both:.2}              (their product is {:.2})",
        r_attr * r_co
    );
    // …and NOT additive, which is the reading this rules out. With both
    // worth several times the base, the two answers are far apart.
    let additive = 1.0 + (r_attr - 1.0) + (r_co - 1.0);
    assert!(
        (r_both - additive).abs() > r_both * 0.2,
        "and nowhere near sharing a bracket: {r_both:.2} vs {additive:.2}"
    );
}
