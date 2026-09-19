//! The Burston Incarnon's radial — the roster's only entry in the CO catalog
//! whose row constrains an EVOLUTION, so it is pinned here rather than left to
//! `data::evolutions::apply`'s comment. The row:
//!
//!   Burston/Burston Prime | Incarnon Form Radial Attack | AoE |
//!     Attack Damage 55 | CO Damage Bonus at +100% 13 | 24% | Adding
//!     "Radial hit only receives CO bonus on target directly hit by bullet.
//!      AoE does not scale off multishot."
use super::*;

/// THE BRATON'S RADIAL CO BASE, the same shape the Burston's row has and
/// the same arithmetic: 70 + 4 = 74, and 70/74 is the 95% the catalog's
/// third column prints. The explosion TAKES the tier-2 evolution's flat
/// damage and does not take it into the base its CO term multiplies.
///
/// Both tier-2 options land it, at their own values — the row's note reads
/// "Listed values for Braton Prime with inactive Daring Reverie", i.e. that
/// perk's unconditional +4 is in the 74 and its conditional half is not.
/// A BURST TRIGGER DECLARES ITS BURST. Without the block the sim reads
/// `fire_rate` as rounds per second when it counts PULLS, and the weapon is
/// understated by its whole burst count — the Sybaris by 2x, the Sicarus by
/// 3x, the Vasto's Incarnon form by 6x.
///
/// It also decides a TRIGGER: `BuffTrigger::FullBurst` asks "every count-th
/// round", and with no block the count is 1, so every round completes a
/// burst and Reaver's Rapture stacks at the wrong rate.
///
/// Twelve entries shipped without one until 2026-08-12, which is why this
/// is a test rather than a habit.
#[test]
fn every_burst_weapon_declares_how_many_rounds_a_pull_fires() {
    let missing: Vec<&str> = all()
        .iter()
        .filter(|s| s.attack.trigger == "burst" && s.attack.burst.is_none())
        .map(|s| s.id.as_str())
        .collect();
    assert!(
        missing.is_empty(),
        "burst trigger with no `burst:` block — `fire_rate` counts PULLS, so these are              understated by their burst count: {missing:?}"
    );
    // …and a block never claims a burst of one, which would be a weapon
    // that is not a burst weapon wearing the trigger.
    for s in all() {
        if let Some(b) = s.attack.burst {
            assert!(b.count >= 2, "{}: a burst of {} is not a burst", s.id, b.count);
            // A ZERO DELAY IS A REAL VALUE, and the Morgha is why this is
            // `>= 0.0` instead of `> 0.0`: its page gives Burst Count 2 and
            // Burst Delay 0.0 s, which means both rounds leave together.
            // It is still a BURST and not multishot — a burst of two spends
            // two rounds, and on an Arch-Gun's finite ground reserve that
            // is the difference between 160 shots and 320.
            assert!(b.delay_seconds >= 0.0, "{}: a negative burst delay", s.id);
        }
    }
}

/// THE AKARIUS PAIR: two damage instances a rocket, two rockets a pull, and
/// the CO row that names one of them.
///
/// The burst is the half a reader gets wrong: the listed fire rate counts
/// PULLS, so reading 3.667 as rounds per second halves the weapon. The
/// module carries `BurstCount = 2` and the wiki's Notes name it in words —
/// "the guaranteed 2-round burst".
#[test]
fn the_akarius_fires_two_rockets_a_pull_and_each_one_explodes() {
    for (id, blast, radius, cc) in [
        ("akarius", 419.0, 7.2, 0.06),
        ("akarius_prime", 509.0, 7.8, 0.18),
    ] {
        let b = WeaponBase::from_data(id, true, &[]);
        assert_eq!(b.base_vector.total(), 68.0, "{id}: Rocket Impact is 68 Impact");
        let burst = b.burst.expect("{id} declares its burst");
        assert_eq!(burst.count, 2, "{id}: two rockets a pull");

        let r = b.radial.as_ref().unwrap_or_else(|| panic!("{id} declares Rocket Detonation"));
        assert!((r.base_vector.total() - blast).abs() < 1e-9, "{id} blast");
        assert!((r.radius_m - radius).abs() < 1e-9, "{id} radius");
        assert!((r.base_crit_chance - cc).abs() < 1e-9, "{id}: the explosion crits like the impact");
        // "Rocket Detonation | 0% | Does not apply" on the Prime's row, and
        // no row at all on the base — ordinary either way for an AoE.
        assert!(!r.takes_condition_overload, "{id}: the explosion takes no CO");
    }

    // The CO row names the PRIME's Rocket Impact and nothing else.
    assert_eq!(
        spec("akarius_prime").unwrap().co_behavior.as_deref(),
        Some("independent")
    );
    // The base has no row. Written out rather than left blank, so the
    // assertion is on the VALUE — absence and the ordinary value are the
    // same statement and a file may make either.
    assert_eq!(
        spec("akarius").unwrap().co_behavior.as_deref().unwrap_or("additive_with_base_damage"),
        "additive_with_base_damage",
        "no row, so ordinary"
    );
}

#[test]
fn the_bratons_radial_co_base_is_the_catalogs_ninety_five_percent() {
    let b = WeaponBase::from_data("braton_prime_incarnon", true, &["braton_prime_daring_reverie"]);
    let r = b.radial.as_ref().expect("the radial survives an evolution");
    assert!((r.base_vector.total() - 74.0).abs() < 1e-9, "{}", r.base_vector.total());
    assert!(
        (r.co_base_fraction() - 70.0 / 74.0).abs() < 1e-9,
        "the explosion's CO base stays 70/74 = {:.1}%, got {}",
        70.0 / 74.0 * 100.0,
        r.co_base_fraction()
    );
}

/// THE ZYLOK'S ROW MIXES ITS TWO VARIANTS, so its printed 90% is the one
/// figure in the catalog this engine does not reproduce — and should not.
///
/// The row reads `776 || 700 || 90%` with the note "Listed Values for Zylok
/// Prime". 700 IS the Prime's radial; the +76 that makes 776 is the base
/// Zylok's Precision's Payoff, which the evolution table prints per variant
/// as X = 76 (Zylok) and X = 30 (Zylok Prime). 700/776 is therefore one
/// weapon's explosion under the other's perk.
///
/// Each variant is self-consistent here, and the per-variant evolution
/// table is the more specific source.
#[test]
fn the_zyloks_two_variants_each_carry_their_own_radial_co_base() {
    let prime = WeaponBase::from_data("zylok_prime_incarnon", true, &["zylok_prime_precisions_payoff"]);
    let pr = prime.radial.as_ref().expect("radial");
    assert!((pr.base_vector.total() - 730.0).abs() < 1e-9, "700 + 30");
    assert!((pr.co_base_fraction() - 700.0 / 730.0).abs() < 1e-9);

    let plain = WeaponBase::from_data("zylok_incarnon", true, &["zylok_precisions_payoff"]);
    let cr = plain.radial.as_ref().expect("radial");
    assert!((cr.base_vector.total() - 676.0).abs() < 1e-9, "600 + 76");
    assert!((cr.co_base_fraction() - 600.0 / 676.0).abs() < 1e-9);
}

#[test]
fn the_incarnon_form_is_two_damage_instances() {
    let b = WeaponBase::from_data("burston_prime_incarnon", true, &[]);
    assert_eq!(b.base_vector.total(), 13.0, "direct hit is 13 Heat");
    let r = b.radial.as_ref().expect("the Incarnon form declares a radial");
    assert_eq!(r.base_vector.total(), 13.0, "the explosion is 13 Heat too");
    assert_eq!(r.radius_m, 2.0);
    assert!(r.takes_condition_overload, "the catalog row grants it");
    assert!(!r.takes_multishot, "\"AoE does not scale off multishot\"");
}

/// 55 = 13 + 42, and 13/55 = the 24% the catalog's third column prints:
/// the explosion TAKES the tier-2 evolution's flat damage but does not
/// take it into the base its CO term multiplies. Both tier-2 options give
/// the same +42, so both must land the same radial.
#[test]
fn a_flat_damage_evolution_raises_the_explosion_but_not_its_co_base() {
    for evo in ["burston_prime_forceful_finality", "burston_prime_fortress_salvo"] {
        let b = WeaponBase::from_data("burston_prime_incarnon", true, &[evo]);
        let r = b.radial.as_ref().expect("the radial survives an evolution");
        assert!(
            (r.base_vector.total() - 55.0).abs() < 1e-9,
            "{evo}: radial should evolve to 55, got {}",
            r.base_vector.total()
        );
        assert!(
            (r.co_base_fraction() - 13.0 / 55.0).abs() < 1e-9,
            "{evo}: the explosion's CO base stays 13/55, got {}",
            r.co_base_fraction()
        );
        // …AND SO DOES THE DIRECT HIT — MEASURED,
        // which reversed this assertion. It read `co_base_fraction == 1.0`
        // on the reasoning that the catalog's row names the RADIAL and the
        // direct hit is therefore not discrepant. It is: at one and two
        // Galvanized Aptitude stacks against one status type the game gives
        // 181 and 196 where the full-base reading gives 231 and 261, and
        // both solve to 13/55. The exclusion belongs to the PERK, so it
        // reaches wherever the +42 landed. MEASUREMENTS M48.
        assert!((b.base_vector.total() - 55.0).abs() < 1e-9);
        assert!(
            (b.co_base_fraction() - 13.0 / 55.0).abs() < 1e-9,
            "{evo}: the direct hit's CO base is 13/55 too, got {}",
            b.co_base_fraction()
        );
    }
}

/// Both instances land on the SAME enemy, and multishot moves only one of
/// them: `takes_multishot: false` means the explosion fires once per pull
/// while the direct hit fires once per pellet.
#[test]
fn multishot_multiplies_the_direct_hit_and_not_the_explosion() {
    use crate::fight::{monte_carlo, FightParams};
    let b = WeaponBase::from_data("burston_prime_incarnon", true, &[]);
    let body = || {
        vec![crate::target::BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            crit_bonus: false,
        }]
    };
    let pool = crate::data::mods::pool_for_weapon("burston_prime_incarnon");
    let sim = |mods: &[&crate::model::ModDef]| {
        let p = crate::build::loadout::resolve(&b, mods, crate::model::StackPolicy::AssumedMax);
        let params = FightParams::from_panel(
            &p,
            &crate::arena::Arena {
                body_parts: body(),
                ..crate::arena::Arena::training(30.0)
            },
            &crate::data::arcanes::ArcaneFx::none(),
        );
        monte_carlo(&params, 30, 11).source_damage
    };
    let bare = sim(&[]);
    assert!(bare.direct > 0.0 && bare.radial > 0.0, "both instances land: {bare:?}");

    let split = pool.iter().find(|m| m.id == "split_chamber").expect("split_chamber");
    let multishot = sim(&[split]);
    assert!(
        multishot.direct > bare.direct * 1.5,
        "+90% multishot must grow the direct hit: {} -> {}",
        bare.direct,
        multishot.direct
    );
    assert!(
        (multishot.radial / bare.radial - 1.0).abs() < 0.1,
        "the explosion must not follow it: {} -> {}",
        bare.radial,
        multishot.radial
    );
}
