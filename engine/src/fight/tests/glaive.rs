//! A GLAIVE'S THROW — docs/MELEE.md §10.
use super::*;
use crate::data::apl::{Action, Apl, Rule, When};
use crate::rules::space::Vec2;

/// One throw form, played with or without `detonate` on its list, `gap_m` from
/// the target and with `others` standing where they are given.
fn throw_among(form: &str, mods: &[&str], detonate: bool, gap_m: f64, others: &[Vec2]) -> Summary {
    let base = crate::model::WeaponBase::from_data(form, true, &[]);
    let pool = crate::data::mods::pool_for_weapon(form);
    let refs: Vec<&crate::model::ModDef> = mods
        .iter()
        .map(|id| pool.iter().find(|m| m.id == *id).unwrap_or_else(|| panic!("{id} not in the pool")))
        .collect();
    let panel = crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let mut arena = crate::arena::Arena::training(60.0);
    arena.target_at = Vec2::new(0.0, crate::rules::space::CONTACT_RANGE_M + gap_m);
    if detonate {
        arena.apl = Apl(vec![Rule { action: Action::Detonate, when: When::Always }]);
    }
    let mut p = FightParams::for_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none(), || panel.clone());
    p.others = others
        .iter()
        .map(|&at| crate::formation::FoeSpec {
            id: String::new(),
            params: p.foe.clone(),
            body_parts: p.body_parts.clone(),
            at,
        })
        .collect();
    monte_carlo(&p, 20, 7)
}

fn throw(form: &str, mods: &[&str], detonate: bool, gap_m: f64) -> Summary {
    throw_among(form, mods, detonate, gap_m, &[])
}

/// **A THROW NOBODY DETONATES EXPLODES NOT AT ALL**, and `detonate` is the
/// only difference between the two: the explosion is several times the throw.
#[test]
fn the_explosion_goes_off_only_when_the_list_detonates() {
    let left = throw("glaive_prime_charged_throw", &[], false, 0.0);
    let set_off = throw("glaive_prime_charged_throw", &[], true, 0.0);
    // 360 thrown against 360 + 1184 Blast — less than its face, since the
    // throw's forced Slash bleeds in both.
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

/// **A GLAIVE IN THE AIR HOLDS THE NEXT THROW**: one detonated for its flight
/// out to the target, one left to return for its flight out to where it turns
/// and back. Glaive Prime's quick throw: 0.8 s a throw, 40 m/s, 30 m of range.
#[test]
fn the_glaive_in_the_air_holds_the_next_throw() {
    let at_hand = throw("glaive_prime_throw", &[], true, 0.0).mean_shots;
    // Detonated 20 m out: 0.5 s more.
    let far_set_off = throw("glaive_prime_throw", &[], true, 20.0).mean_shots;
    assert!((at_hand / far_set_off - 1.3 / 0.8).abs() < 0.1, "out only: {at_hand} / {far_set_off}");
    // ITS PUNCH THROUGH CARRIES IT THROUGH THE TARGET, and with no wall here it
    // flies its whole 30 m before it turns: 1.5 s out and back.
    let passed = throw("glaive_prime_throw", &[], false, 0.0).mean_shots;
    assert!((at_hand / passed - 2.3 / 0.8).abs() < 0.15, "its range and back: {at_hand} / {passed}");
    // …AND ONE THAT CANNOT PASS turns at the target: 20 m out and back, 1 s.
    let bounced = throw("glaive_prime_throw", &["volatile_rebound"], false, 20.0).mean_shots;
    assert!((at_hand / bounced - 1.8 / 0.8).abs() < 0.1, "out to the body and back: {at_hand} / {bounced}");
}

/// **WHIRLWIND SPEEDS THE FLIGHT AND STRETCHES THE RANGE TOGETHER** — *"As a
/// direct result, this mod also increases throwing distance"* (wiki). So a
/// glaive that flies its whole range comes back no sooner, and one detonated
/// at a distant target does.
#[test]
fn whirlwind_is_worth_time_only_where_the_glaive_turns_short() {
    let passed = throw("glaive_prime_throw", &[], false, 0.0).mean_shots;
    let passed_ww = throw("glaive_prime_throw", &["whirlwind"], false, 0.0).mean_shots;
    assert!((passed_ww / passed - 1.0).abs() < 0.03, "its range grew with its speed: {passed} / {passed_ww}");
    let far = throw("glaive_prime_throw", &[], true, 20.0).mean_shots;
    let far_ww = throw("glaive_prime_throw", &["whirlwind"], true, 20.0).mean_shots;
    // 0.5 s of flight becomes 0.5 / 2.8.
    assert!((far_ww / far - 1.3 / (0.8 + 0.5 / 2.8)).abs() < 0.1, "a faster flight out: {far} / {far_ww}");
}

/// **POWER THROW PAYS THE THROW AND NOT ITS EXPLOSION**: *"On Consecutive
/// throw (Max stacks 3): +100% Throw Damage"*, and a loop buffers every throw,
/// so all but the first carry up to +300% — sixteen throws a minute here, so
/// `(0 + 1 + 2 + 3 x 13) / 16 + 1` = 3.6x on the hits.
#[test]
fn power_throw_pays_the_thrown_glaive_alone() {
    let hits = |s: &Summary| s.mean_damage - s.mean_dot_damage;
    let left = throw("glaive_prime_charged_throw", &[], false, 0.0);
    let left_pt = throw("glaive_prime_charged_throw", &["power_throw"], false, 0.0);
    assert_eq!(left.mean_shots, 16.0);
    assert!((hits(&left_pt) / hits(&left) - 3.625).abs() < 0.1, "three stacks on the throw: {}", hits(&left_pt) / hits(&left));
    let plain = left.damage_per_shot;
    let pt = left_pt.damage_per_shot;
    let boom = throw("glaive_prime_charged_throw", &[], true, 0.0).damage_per_shot - plain;
    let boom_pt = throw("glaive_prime_charged_throw", &["power_throw"], true, 0.0).damage_per_shot - pt;
    assert!((boom_pt / boom - 1.0).abs() < 0.15, "Power Throw reached the explosion: {}", boom_pt / boom);
}

/// **A VOLATILE CARD TAKES THE PUNCH THROUGH AND EXPLODES ON THE BOUNCE**:
/// the glaive turns off the body it was thrown into, and that turn explodes.
#[test]
fn a_volatile_glaive_explodes_off_the_body_it_cannot_pass() {
    // Left to return, so the only explosion is the bounce's: 296 Blast beside
    // the throw's 180, on the hits (the bleeds the throw forces are in both),
    // and less than that face since Blast and Slash meet the target differently.
    let hits = |s: &Summary| (s.mean_damage - s.mean_dot_damage) / s.mean_shots;
    let plain = throw("glaive_prime_throw", &[], false, 0.0);
    let volatile = throw("glaive_prime_throw", &["volatile_rebound"], false, 0.0);
    let ratio = hits(&volatile) / hits(&plain);
    assert!(ratio > 1.5, "the bounce explosion went off: {ratio}");
}

/// **A BOUNCE GOES WHERE REFLECTION TAKES IT, AND THE COUNT IS A CEILING** —
/// three bounces into a ring of bodies reach bodies the explosion at the target
/// does not, and Quick Return's none reach no one.
#[test]
fn rebound_bounces_into_the_crowd_and_quick_return_turns_home() {
    let t = Vec2::new(0.0, crate::rules::space::CONTACT_RANGE_M + 8.0);
    // A RING 7 m OUT, past the 4.8 m explosion (7.8 m with Volatile Quick
    // Return's +3), so only a bounce reaches it.
    let ring: Vec<Vec2> = (0..36)
        .map(|i| {
            let a = f64::from(i) * std::f64::consts::TAU / 36.0;
            Vec2::new(t.x + 8.5 * a.cos(), t.y + 8.5 * a.sin())
        })
        .collect();
    let others = |s: &Summary| s.mean_damage_by_body.0[1..].iter().sum::<f64>();
    let rebound = throw_among("glaive_prime_charged_throw", &["volatile_rebound", "rebound"], false, 8.0, &ring);
    let quick = throw_among("glaive_prime_charged_throw", &["volatile_quick_return"], false, 8.0, &ring);
    assert!(others(&rebound) > 0.0, "the bounces found the ring");
    assert_eq!(others(&quick), 0.0, "no bounce after the first");
}

/// **A DETONATABLE THROW IS ITS OWN MODE**, the throw's form with one more line
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

