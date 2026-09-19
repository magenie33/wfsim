//! STORMBURST: a buff whose condition is on the TARGET, not on the shot.
//!
//! It was inert until the StackingBuff refactor, and the reason is worth
//! keeping: the static path (`AssumedMaxMultishot`) carries a total and a cap
//! and NO trigger, so declaring it there would have granted +1.2 multishot to
//! a build with no Electricity in it at all. A LIVE buff is bumped inside the
//! fight, where the target's debuffs are in hand — so the condition it could
//! not state statically is simply readable.
//!
//! ON THIS WEAPON IT ONLY WORKS AS A CYCLE, which the sim found rather than
//! anyone predicting it. The Incarnon form's base damage is Heat, so a
//! Convulsion's Electricity COMBINES with it: that form deals Radiation 339 and
//! no Electricity whatever. The BASE form has no innate element, so its
//! Electricity stays raw — it is the half of the cycle that puts the status on
//! the target, and the Incarnon half then finds it there. Fire the Incarnon
//! form alone and this perk can never trigger off its own procs.
use super::*;

/// Extra pellets beyond one per shot — what a flat multishot bonus looks
/// like. Run as the CYCLE, because that is the only way this weapon ever
/// has Electricity on the target (see the note above).
fn extra_pellets(mods: &[&str]) -> f64 {
    let evos = ["furis_evo1_incarnon_form", "furis_stormburst", "furis_extended_volley"];
    let inc = crate::model::WeaponBase::from_data("furis_incarnon", true, &evos);
    let base = crate::model::WeaponBase::from_data("furis", true, &evos);
    let pool = crate::data::mods::pool_for_weapon("furis_incarnon");
    let picked: Vec<&crate::model::ModDef> = mods
        .iter()
        .map(|id| pool.iter().find(|m| m.id == *id).unwrap_or_else(|| panic!("{id}")))
        .collect();
    let pol = crate::model::StackPolicy::Emergent;
    let pi = crate::build::loadout::resolve(&inc, &picked, pol);
    let pb = crate::build::loadout::resolve(&base, &picked, pol);
    let params = FightParams::incarnon_cycle_from_panels(
        &pi,
        &pb,
        false,
        LockMode::Initial(0),
        &crate::arena::Arena::training(60.0),
        &ArcaneFx::none(),
    );
    let s = monte_carlo(&params, 6, 777);
    s.mean_pellets - s.mean_shots
}

/// With no Electricity anywhere the perk grants nothing: one pellet a shot.
/// This is the assertion that fails if it is ever modelled the static way.
#[test]
fn it_grants_nothing_without_electricity() {
    let extra = extra_pellets(&[]);
    assert!(extra.abs() < 1e-9, "extra pellets with no Electricity: {extra}");
}

/// ...and pays out once the target is shocked.
#[test]
fn it_pays_out_once_the_target_is_shocked() {
    let extra = extra_pellets(&["primed_convulsion"]);
    assert!(extra > 0.0, "expected extra pellets once Electricity lands, got {extra}");
}
