/// THE CHAMBER FAMILY: two cards, one bracket, and no family tie.
///
/// Both are `Sniper`-tagged, which is a pool this roster had no directory
/// for at all until 2026-08-18 — fifteen snipers were drawing `[primary,
/// rifle]` and nothing else, so every sniper-only mod in the game was
/// invisible to the builder. `scripts/survey_pool_mods.py` is what stops
/// that happening again.
#[test]
fn the_chambers_sum_into_one_first_round_bracket_and_are_not_a_family() {
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::{ModEffect, StackPolicy};
    let pool = crate::data::mods::class_pool("sniper");
    let pick = |id: &str| {
        pool.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{id}")).clone()
    };
    let cc = pick("charged_chamber");
    let pc = pick("primed_chamber");
    assert!(
        cc.effects.iter().any(|e| matches!(e, ModEffect::FirstRoundDamage(v) if (v - 0.4).abs() < 1e-9)),
        "Charged Chamber is +40% at rank 3: {:?}", cc.effects
    );
    assert!(
        pc.effects.iter().any(|e| matches!(e, ModEffect::FirstRoundDamage(v) if (v - 1.0).abs() < 1e-9)),
        "Primed Chamber is +100% at rank 3: {:?}", pc.effects
    );
    // "Despite its name … it is not the 'Primed version' of Charged
    // Chamber, and thus can be equipped alongside it." A shared `family`
    // would have made the pair mutually exclusive, which is the one thing
    // the page goes out of its way to deny.
    assert_ne!(
        (cc.family, pc.family), (Some("chamber"), Some("chamber")),
        "the two chambers are not a mod family"
    );
    assert!(cc.family.is_none() && pc.family.is_none());

    // ONE BRACKET: "Stacks additively with … for up to 140% bonus damage."
    let base = WeaponBase::from_data("vectis_prime", true, &[]);
    let both = resolve(&base, &[&cc, &pc], StackPolicy::Emergent);
    assert!(
        (both.first_round_damage - 1.4).abs() < 1e-9,
        "140%, not 1.4x1.0: {}", both.first_round_damage
    );

    // …AND THE INCARNON FORM KEEPS IT, which is where this card parts
    // company with Synth Charge. "Fixed the Vectis Incarnon Form
    // benefitting from Primed Chamber on every shot" (ver 43.5) says the
    // form pays it once a magazine — a bug in HOW OFTEN, not an exemption.
    let inc = WeaponBase::from_data("vectis_prime_incarnon", true, &[]);
    assert!(
        (resolve(&inc, &[&pc], StackPolicy::Emergent).first_round_damage - 1.0).abs() < 1e-9,
        "an Incarnon fire mode still takes the first-round bonus"
    );
}

/// EVERY SNIPER SEES THE SNIPER POOL, and nothing else does.
#[test]
fn the_sniper_pool_reaches_snipers_and_only_snipers() {
    let has = |w: &str| {
        crate::data::mods::pool_for_weapon(w).iter().any(|m| m.id == "primed_chamber")
    };
    for w in ["vectis", "vectis_prime", "rubico_prime", "lanka", "vulkar", "komorex"] {
        assert!(has(w), "{w} is a sniper");
    }
    for w in ["braton_prime", "paris_prime", "boar_prime", "lex", "kuva_nukor"] {
        assert!(!has(w), "{w} is not a sniper");
    }
    // A FORM DECLARES NO POOL AT ALL — modding is the WEAPON's — so the
    // pool goes on the weapon and the Incarnon halves need nothing, which
    // is why this edit touched fifteen files and not thirty. Naming the
    // form resolves the weapon's pool (`weapon_of`), so the sniper mods
    // reach it exactly as they reach the rifle it is a form of.
    assert!(has("vectis_incarnon"), "a form is modded as its weapon");
}
