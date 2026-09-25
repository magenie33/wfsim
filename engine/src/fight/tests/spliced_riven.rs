use super::*;
use crate::build::loadout::resolve;
use crate::build::rivens::{RivenSpec, RolledStat, MAX_RANK};
use crate::model::{StackPolicy, WeaponBase};
use crate::rules::capacity::Polarity;

fn riven(ids: &[&str]) -> crate::model::ModDef {
    RivenSpec {
        class: "rifle".into(),
        bonuses: ids.iter().map(|id| RolledStat { id: (*id).into(), roll: 1.0 }).collect(),
        malus: None,
        rank: MAX_RANK,
        polarity: Polarity::Madurai,
    }
    .to_mod_def("riven:spliced", 1.0)
}

/// A SPLICED AMMO EFFICIENCY BUYS SHOTS, through the whole path a build takes:
/// the riven's card, the panel, `from_panel` adding it to the arcane's bucket,
/// and the magazine paying less per shot. One magazine and no reserve, so the
/// shot count is exactly what the magazine bought.
#[test]
fn a_spliced_ammo_efficiency_buys_shots_in_the_fight() {
    let base = WeaponBase::from_data("braton_prime", true, &[]);
    let with = riven(&["ammo_efficiency", "damage"]);
    let without = riven(&["zoom", "damage"]);
    let arena = crate::arena::Arena { body_parts: mono_body(1.0), ..crate::arena::Arena::training(1000.0) };
    let fight = |m: &crate::model::ModDef| {
        let panel = resolve(&base, &[m], StackPolicy::AssumedMax);
        let mut p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
        p.infinite_reserve = false;
        p.reserve_ammo = 0.0;
        p
    };
    let (a, b) = (fight(&without), fight(&with));
    // 0.001 x 90 x 0.99 (a plain two-stat card) at disposition 1.0.
    assert!((b.arcane.ammo_efficiency - 0.0891).abs() < 1e-9, "{}", b.arcane.ammo_efficiency);
    assert_eq!(a.arcane.ammo_efficiency, 0.0);
    let shots = |p: &FightParams| monte_carlo(p, 4, 3).mean_shots;
    let (plain, efficient) = (shots(&a), shots(&b));
    assert!((plain - a.magazine_size).abs() < 1e-9, "one magazine, {plain} shots");
    // Each shot costs 1 - 0.0891, so the magazine buys that much more.
    let want = (a.magazine_size / (1.0 - 0.0891)).ceil();
    assert!((efficient - want).abs() <= 1.0, "{efficient} shots, about {want} expected");
}

/// DAMAGE TO TECHROT LANDS ON A TECHROT UNIT and on nothing else — read off the
/// roster's own Techrot Babau, whose yaml says `faction: techrot`, so the name
/// is proven to reach the fight's faction rather than a hand-set one.
#[test]
fn a_spliced_techrot_bonus_lands_on_a_techrot_unit() {
    let base = WeaponBase::from_data("braton_prime", true, &[]);
    let panel = resolve(&base, &[&riven(&["damage_to_techrot", "damage"])], StackPolicy::AssumedMax);
    let babau = crate::data::enemies::all()
        .into_iter()
        .find(|e| e.id == "techrot_babau")
        .expect("techrot_babau")
        .target_params(1, false, false, crate::target::TargetMode::InstantRespawn)
        .expect("it builds a target");
    assert_eq!(babau.faction, crate::model::Faction::Techrot);
    let vs = |target| {
        let arena = crate::arena::Arena { target, body_parts: mono_body(1.0), ..crate::arena::Arena::training(10.0) };
        FightParams::from_panel(&panel, &arena, &ArcaneFx::none()).faction_multiplier
    };
    // 0.45 x 0.99 (a plain two-stat card) at disposition 1.0; DE stores the
    // base as 0.0049999999, hence the looser tolerance.
    let got = vs(babau);
    assert!((got - (1.0 + 0.45 * 0.99)).abs() < 1e-6, "{got}");
    assert_eq!(vs(Foe::training_dummy()), 1.0);
}
