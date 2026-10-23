//! LAETUM UNDER PISTOL ACUITY against the owner's readings (MEASUREMENTS
//! M123): rank 10 is +349.8% Weak Point Damage, added to a 3x head at 1.5x,
//! so the head reads 8.247x where the card's 350% would make it 8.25x.
use super::*;
use crate::record::{Kind, Origin};

/// The Laetum in `form`, with `evos` chosen and `mods` from its own pool.
fn laetum(form: &str, evos: &[&str], mods: &[&str]) -> crate::build::loadout::ResolvedPanel {
    let base = crate::model::WeaponBase::from_data(form, false, evos);
    let pool = crate::data::mods::pool_for_weapon("laetum");
    let cards: Vec<&crate::model::ModDef> = mods
        .iter()
        .map(|id| pool.iter().find(|m| m.id == *id).unwrap_or_else(|| panic!("{id} on the Laetum")))
        .collect();
    crate::build::loadout::resolve_for(&base, &cards, crate::model::StackPolicy::Emergent, crate::data::tenno::default_tenno())
}

/// The number the game draws: halves round up.
fn shown(x: f64) -> f64 {
    (x + 1e-6).round()
}

/// A Deimos Runner's head: 3x, a weak point, crit-eligible. Its faction
/// takes Impact, Slash and Radiation as written, so the training dummy's
/// unmodified health stands in for the body.
fn runner_head() -> Vec<BodyPart> {
    vec![BodyPart {
        name: "head".into(),
        aim_weight: 1.0,
        multiplier: 3.0,
        is_head: true,
        is_weak_point: true,
        crit_bonus: true,
    }]
}

/// Every number the head popped over a fight, as `(crit tier, radial,
/// number)`: one pellet's damage types summed, as the game draws them.
fn pops(panel: &crate::build::loadout::ResolvedPanel) -> Vec<(u32, bool, f64)> {
    let mut p = FightParams::from_panel(panel, &crate::arena::Arena::training(30.0), &ArcaneFx::none());
    p.foe.base_health = 1e15;
    p.status_chance = 0.0;
    p.base_status_chance = 0.0;
    p.body_parts = runner_head();
    let rec = record(&p, 3, 0.0, f64::INFINITY, 10_000, 0);
    let mut out: Vec<(f64, Option<u32>, u32, bool, f64)> = Vec::new();
    for e in rec.events() {
        if let Kind::Damage(d) = &e.kind {
            if d.origin != Origin::Own || d.pellet.is_none() {
                continue;
            }
            match out.last_mut() {
                Some((t, pl, tier, radial, sum))
                    if (*t - e.t).abs() < 1e-9 && *pl == d.pellet && *tier == d.crit_tier && *radial == d.radial =>
                {
                    *sum += d.raw
                }
                _ => out.push((e.t, d.pellet, d.crit_tier, d.radial, d.raw)),
            }
        }
    }
    out.into_iter().map(|(_, _, tier, radial, x)| (tier, radial, shown(x))).collect()
}

/// The distinct numbers of one kind the head popped.
fn read(pops: &[(u32, bool, f64)], tier: u32, radial: bool) -> Vec<f64> {
    let mut v: Vec<f64> = pops.iter().filter(|p| p.0 == tier && p.1 == radial).map(|p| p.2).collect();
    v.sort_by(f64::total_cmp);
    v.dedup();
    v
}

#[test]
fn m123_pistol_acuity_is_three_point_four_nine_eight() {
    let panel = laetum("laetum", &[], &["pistol_acuity"]);
    assert!((panel.weakpoint_damage - 3.498).abs() < 1e-9, "{}", panel.weakpoint_damage);
    assert!((panel.weakpoint_crit_chance_relative - 3.498).abs() < 1e-9);
}

/// BASE FORM, Acuity alone: a yellow head reads `160 x 8.247 x 4.407326 =
/// 5815.6`, shown 5816 (the card's 350% would read 5818).
#[test]
fn m123_base_form_yellow_head_is_5816() {
    let got = pops(&laetum("laetum", &[], &["pistol_acuity"]));
    assert_eq!(read(&got, 1, false), [5816.0]);
}

/// INCARNON FORM, Acuity alone: the direct hit's yellow reads `100 x 8.247 x
/// 4.407326 = 3634.7`, shown 3635; the explosion beside it reads its own 300
/// white, Acuity and the head left out.
#[test]
fn m123_incarnon_direct_hit_is_3635_and_the_explosion_300() {
    let got = pops(&laetum("laetum_incarnon", &[], &["pistol_acuity"]));
    assert_eq!(read(&got, 1, false), [3635.0]);
    assert_eq!(read(&got, 0, true), [300.0]);
}

/// HORNET STRIKE (+220%, 512 a hit) with Acuity: the yellow head reads
/// `512 x 8.247 x 4.407326 = 18609.8`, shown 18610 (350% would read 18616).
#[test]
fn m123_hornet_strike_yellow_head_is_18610() {
    let got = pops(&laetum("laetum", &[], &["hornet_strike", "pistol_acuity"]));
    assert_eq!(read(&got, 1, false), [18610.0]);
}

/// DEVOURING ATTRITION'S x21 ON A WHITE HEAD: `512 x 3 x 21 = 32256` without
/// Acuity — the control, exact — and `512 x 8.247 x 21 = 88671.7`, shown
/// 88672, with it (350% would read 88704). Elemental Excess's -10% crit
/// chance is what lets a head under Acuity read white.
#[test]
fn m123_devouring_white_heads_are_32256_and_88672() {
    let evos = ["laetum_devouring_attrition", "laetum_elemental_excess"];
    let bare = pops(&laetum("laetum", &evos, &["hornet_strike"]));
    assert!(read(&bare, 0, false).contains(&32256.0), "{:?}", read(&bare, 0, false));
    let acuity = pops(&laetum("laetum", &evos, &["hornet_strike", "pistol_acuity"]));
    assert!(read(&acuity, 0, false).contains(&88672.0), "{:?}", read(&acuity, 0, false));
}
