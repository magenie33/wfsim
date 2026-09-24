//! HOW AN INCARNON GAUGE FILLS, on the real weapons rather than on a fixture.
//!
//! `charge_on` is weapon data and `dummy` already tells the two rules apart —
//! but the only test of it built its own `FightParams` by hand, so it proved
//! the shot loop honours the flag and never asked whether any weapon in the
//! roster carries the right one. The difference is not a matter of speed: at a
//! 0% headshot rate a weakpoint-charged weapon never transforms at all, which
//! is the largest thing an Incarnon weapon can do decided by one field.
use super::*;
use crate::fight::{monte_carlo, FightParams};

/// Transformations in a fixed engagement at a given headshot rate.
fn transforms(weapon: &str, evo: &str, headshot_pct: f64) -> u32 {
    let base = WeaponBase::from_data(weapon, true, &[evo]);
    let form = crate::data::weapons::spec(weapon)
        .and_then(|s| s.transforms_to.clone())
        .expect("a cycling weapon names its second form");
    let inc = WeaponBase::from_data(&form, true, &[evo]);
    let policy = crate::model::StackPolicy::Emergent;
    let p0 = crate::build::loadout::resolve(&base, &[], policy);
    let p1 = crate::build::loadout::resolve(&inc, &[], policy);
    // The head takes every shot or none of them, which is what makes this
    // a test of the CHARGE RULE rather than of the aim model.
    let parts = vec![
        crate::target::BodyPart {
            name: "head".into(), aim_weight: headshot_pct / 100.0,
            multiplier: 3.0, is_head: true,
 is_weak_point: true, crit_bonus: true,
        },
        crate::target::BodyPart {
            name: "body".into(), aim_weight: 1.0 - headshot_pct / 100.0,
            multiplier: 1.0, is_head: false,
 is_weak_point: false, crit_bonus: false,
        },
    ];
    let arena = crate::arena::Arena { body_parts: parts, ..crate::arena::Arena::training(120.0) };
    // Frenzy off and earned-from-zero: neither weapon here has the passive,
    // and pinning it keeps this a test of the GAUGE.
    let params = FightParams::incarnon_cycle_from_panels(
        &p1, &p0, false, crate::fight::LockMode::Initial(0), &arena,
        &crate::data::arcanes::ArcaneFx::none(),
    );
    monte_carlo(&params, 5, 3).mean_transforms.round() as u32
}

#[test]
fn a_direct_hit_gauge_does_not_need_headshots() {
    // The Torid: "Angstrum Incarnon Genesis and Torid Incarnon Genesis are
    // instead charged through direct hits" (wiki Incarnon).
    let none = transforms("torid", "torid_evo1_incarnon_form", 0.0);
    let all = transforms("torid", "torid_evo1_incarnon_form", 100.0);
    assert!(none > 0, "the Torid must transform with no headshots at all, got {none}");
    assert!(all > 0, "and still does when every shot is a headshot, got {all}");
}

#[test]
fn a_weakpoint_gauge_does() {
    // The control, and the reason the Torid's rule needs its own field: on
    // a Zariman-rule weapon a body-only engagement never transforms.
    let none = transforms("burston_prime", "burston_prime_evo1_incarnon_form", 0.0);
    let all = transforms("burston_prime", "burston_prime_evo1_incarnon_form", 100.0);
    assert_eq!(none, 0, "no headshots, no Incarnon form");
    assert!(all > 0, "and with headshots it gets there, got {all}");
}
