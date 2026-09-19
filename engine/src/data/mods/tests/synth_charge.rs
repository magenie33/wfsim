/// SYNTH CHARGE: the LAST ROUND, its own multiplier, and three ways off.
///
/// It shipped as a plain `base_damage_bonus` — +200% on EVERY shot, in the
/// bucket Hornet Strike is in — which is wrong twice and wrong upward both
/// times.
#[test]
fn synth_charge_is_the_last_round_only_and_only_where_it_can_be() {
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::{ModEffect, StackPolicy};
    let pool = crate::data::mods::class_pool("pistol");
    let sc = pool.iter().find(|m| m.id == "synth_charge").expect("synth charge");
    assert!(
        sc.effects.iter().any(|e| matches!(e, ModEffect::LastRoundDamage(v) if (v - 2.0).abs() < 1e-9)),
        "the card is +200% at max rank, on the final shot: {:?}",
        sc.effects
    );

    // THE MAGAZINE GATE reads the BASE magazine, so it is an equip rule.
    // The Bronco (2) and the Angstrum (1) are turned away; the Lex sits
    // exactly on 6 and keeps it.
    let has = |w: &str| crate::data::mods::pool_for_weapon(w).iter().any(|m| m.id == "synth_charge");
    for w in ["lex", "vasto", "vasto_prime", "lato", "laetum"] {
        assert!(has(w), "{w}: base magazine is 6 or more");
    }
    for w in ["bronco", "bronco_prime", "angstrum", "prisma_angstrum"] {
        assert!(!has(w), "{w}: base magazine is under 6");
    }

    // …AND WHERE IT IS EQUIPPABLE IT IS STILL WORTH NOTHING on a continuous
    // weapon or an Incarnon form, both the mod's own words. The Kuva Nukor
    // has a 77-round magazine and is a beam: it may hold the mod and gets
    // nothing from it.
    let val = |w: &str| {
        let base = WeaponBase::from_data(w, true, &[]);
        resolve(&base, &[sc], StackPolicy::Emergent).last_round_damage
    };
    assert!((val("lex") - 2.0).abs() < 1e-9, "an ordinary pistol pays it");
    assert!(has("kuva_nukor"), "77 rounds, so it EQUIPS");
    assert_eq!(val("kuva_nukor"), 0.0, "…and a continuous weapon gets nothing");
    assert_eq!(val("lex_incarnon"), 0.0, "…and neither does an Incarnon fire mode");
}
