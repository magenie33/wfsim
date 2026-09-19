//! A CYCLE RESOLVES ONE BUFF TWICE, once per form, and the two answers differ.
//!
//! `BuffGrant::FireRate` carries an ABSOLUTE rate that `resolve` derives from
//! that form's own base — the Furis Incarnon's 12 ticks/s against the base
//! form's 10 — so the same perk is worth a different number in each half of the
//! engagement. The stacks are shared (one buff, one count, one fight); only the
//! conversion is per form.
//!
//! This is the bug the stacking-buff refactor introduced and the baseline
//! caught: reading the outer params instead of the ACTIVE form handed the base
//! form the Incarnon form's rate, which showed up as more shots per engagement
//! and moved nothing else. A diff against a hand-captured baseline will not
//! exist next time, so it is asserted here.
#[test]
fn a_fire_rate_buff_converts_against_each_forms_own_base() {
    let evos = [
        "furis_evo1_incarnon_form",
        "furis_haven_foray",
        "furis_extended_volley",
        "furis_headcracker",
    ];
    let inc = crate::model::WeaponBase::from_data("furis_incarnon", true, &evos);
    let base = crate::model::WeaponBase::from_data("furis", true, &evos);
    let pol = crate::model::StackPolicy::Emergent;
    let pi = crate::loadout::resolve(&inc, &[], pol);
    let pb = crate::loadout::resolve(&base, &[], pol);

    let rate_of = |p: &crate::loadout::ResolvedPanel| {
        p.stacking_buffs
            .iter()
            .find(|b| b.grant == crate::model::BuffGrant::FireRate)
            .map(|b| b.per_stack)
            .expect("Headcracker resolves a fire-rate buff on both forms")
    };
    // +5% of each form's own base: 12 x 0.05 = 0.6, and 10 x 0.05 = 0.5.
    assert!((rate_of(&pi) - 0.6).abs() < 1e-9, "incarnon {}", rate_of(&pi));
    assert!((rate_of(&pb) - 0.5).abs() < 1e-9, "base {}", rate_of(&pb));
    assert!(
        rate_of(&pi) > rate_of(&pb),
        "the faster form must be worth more per stack, or the sim is reading one form's \
         rate while firing the other"
    );
}
