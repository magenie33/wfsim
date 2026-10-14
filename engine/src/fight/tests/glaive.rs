//! A GLAIVE'S THROW — docs/MELEE.md §10.
use super::*;
use crate::data::apl::{Action, Apl, Rule, When};

/// One throw form, played with or without `detonate` on its list, `gap_m` from
/// the target.
fn throw(form: &str, mods: &[&str], detonate: bool, gap_m: f64) -> Summary {
    let base = crate::model::WeaponBase::from_data(form, true, &[]);
    let pool = crate::data::mods::pool_for_weapon(form);
    let refs: Vec<&crate::model::ModDef> = mods
        .iter()
        .map(|id| pool.iter().find(|m| m.id == *id).unwrap_or_else(|| panic!("{id} not in the pool")))
        .collect();
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let mut arena = crate::arena::Arena::training(60.0);
    arena.target_at = crate::rules::space::Vec2::new(0.0, crate::rules::space::CONTACT_RANGE_M + gap_m);
    if detonate {
        arena.apl = Apl(vec![Rule { action: Action::Detonate, when: When::Always }]);
    }
    let p = FightParams::for_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none(), || panel.clone());
    monte_carlo(&p, 20, 7)
}

/// **A THROW NOBODY DETONATES EXPLODES NOT AT ALL**, and `detonate` is the
/// only difference between the two: the explosion is several times the throw.
#[test]
fn the_explosion_goes_off_only_when_the_list_detonates() {
    let left = throw("glaive_prime_charged_throw", &[], false, 0.0);
    let set_off = throw("glaive_prime_charged_throw", &[], true, 0.0);
    // 360 thrown against 360 + 1184 Blast: at contact the two loops take the
    // same time, so the explosion is all of the difference — less than its
    // face, since the throw's forced Slash bleeds in both.
    assert_eq!(left.mean_shots, set_off.mean_shots);
    let ratio = set_off.damage_per_shot / left.damage_per_shot;
    assert!(ratio > 1.8, "detonated / left {ratio}");
}

/// **THE DETONATION TAKES THE COUNTER AND THE THROW DOES NOT** — *"Only
/// explosive damage will use the Combo Multiplier"* (wiki, Melee). Corrupt
/// Charge's +30 initial combo is a 2x floor, worth nothing to a throw left to
/// come back and close to double to one that is set off.
#[test]
fn initial_combo_pays_the_detonation_and_not_the_throw() {
    let plain = throw("glaive_prime_charged_throw", &[], false, 0.0).damage_per_shot;
    let plain_floor = throw("glaive_prime_charged_throw", &["corrupt_charge"], false, 0.0).damage_per_shot;
    assert!((plain_floor / plain - 1.0).abs() < 0.02, "a throw left to return reads the counter: {}", plain_floor / plain);
    let boom = throw("glaive_prime_charged_throw", &[], true, 0.0).damage_per_shot;
    let boom_floor = throw("glaive_prime_charged_throw", &["corrupt_charge"], true, 0.0).damage_per_shot;
    assert!(boom_floor / boom > 1.5, "the detonation ignores the counter: {}", boom_floor / boom);
}

/// **KILLING BLOW REACHES THE EXPLOSION AND NOT THE THROW** — *"increase the
/// damage of their mid-air explosion, but will not affect the damage of the
/// thrown weapon itself"* (wiki). Its damage half, that is: its wind-up half
/// shortens the charge, so the comparison is per THROW, never per second.
#[test]
fn killing_blow_pays_the_explosion_alone() {
    // WHAT THE EXPLOSION ADDS to a throw, with and without the card: the
    // throw's own share (and the bleeds it forces, which a faster loop leaves
    // ticking into more throws) cancels out of the difference.
    let plain = throw("glaive_prime_charged_throw", &[], false, 0.0).damage_per_shot;
    let kb = throw("glaive_prime_charged_throw", &["killing_blow"], false, 0.0).damage_per_shot;
    assert!((kb / plain - 1.0).abs() < 0.06, "Killing Blow reached the throw: {}", kb / plain);
    let boom = throw("glaive_prime_charged_throw", &[], true, 0.0).damage_per_shot - plain;
    let boom_kb = throw("glaive_prime_charged_throw", &["killing_blow"], true, 0.0).damage_per_shot - kb;
    assert!(boom_kb / boom > 1.25, "Killing Blow missed the explosion: {}", boom_kb / boom);
}

/// **A GLAIVE LEFT TO COME BACK HOLDS THE NEXT THROW** for its flight out and
/// back, and one that is detonated for its flight out alone.
#[test]
fn the_glaive_in_the_air_holds_the_next_throw() {
    let near = throw("glaive_prime_throw", &[], false, 0.0).mean_shots;
    let far = throw("glaive_prime_throw", &[], false, 20.0).mean_shots;
    let far_set_off = throw("glaive_prime_throw", &[], true, 20.0).mean_shots;
    // 0.8 s a throw at contact; 20 m at 40 m/s is 0.5 s each way.
    assert!((near / far - 1.8 / 0.8).abs() < 0.1, "out and back: {near} / {far}");
    assert!((near / far_set_off - 1.3 / 0.8).abs() < 0.1, "out only: {near} / {far_set_off}");
}

/// **A DETONATED THROW IS ITS OWN MODE**, the throw's form with one more line
/// on the list — and a quick throw that is not a strike has no form at all.
#[test]
fn a_detonatable_throw_is_played_both_ways() {
    let ids = |w: &str| {
        crate::data::weapons::play_modes(w).iter().map(|m| (m.id, m.detonates)).collect::<Vec<_>>()
    };
    let gp = ids("glaive_prime");
    for want in [("throw", false), ("throw_detonate", true), ("charged_throw", false), ("charged_throw_detonate", true)] {
        assert!(gp.contains(&want), "glaive_prime: {want:?} in {gp:?}");
    }
    let orvius = ids("orvius");
    assert!(!orvius.iter().any(|(id, _)| *id == "throw"), "orvius: {orvius:?}");
    let mode = crate::data::weapons::play_modes("glaive_prime").into_iter().find(|m| m.id == "throw_detonate").unwrap();
    assert_eq!(mode.rules().to_simc(), "detonate");
}

