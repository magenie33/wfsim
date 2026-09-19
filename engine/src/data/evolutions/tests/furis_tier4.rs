//! The two Furis tier-4 perks, both added 2026-08-06, and both from the RAW
//! wikitext rather than the rendered page — which is the point of the pair.
//! Reading the effect column alone gave Headcracker no 50% roll and Prelude of
//! Might no "Base", and each omission makes the perk stronger than the game's.
use super::*;
use crate::build::loadout::resolve;
use crate::model::WeaponBase;
use crate::model::StackPolicy;

fn cd_with(mods: &[&str], evos: &[&str]) -> f64 {
    let owned: Vec<String> = evos.iter().map(|s| (*s).to_string()).collect();
    let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
    let base = WeaponBase::from_data("furis_incarnon", true, &refs);
    let pool = crate::data::mods::pool_for_weapon("furis_incarnon");
    let picked: Vec<&crate::model::ModDef> = mods
        .iter()
        .map(|id| pool.iter().find(|m| m.id == *id).unwrap_or_else(|| panic!("{id}")))
        .collect();
    resolve(&base, &picked, StackPolicy::AssumedMax).crit_damage
}

/// "Increase BASE Critical Damage Multiplier by +3x" — so crit-damage mods
/// multiply the raised base. Added AFTER the mods instead, a Primed Target
/// Cracker build reads 10.14x where the game gives 13.44x, and the two only
/// diverge once a crit-damage mod is on — which is why the word "Base",
/// present in the wikitext and absent from the summary, decides it.
#[test]
fn prelude_of_might_raises_the_base_multiplier_not_the_final_one() {
    let evo = ["furis_evo1_incarnon_form", "furis_prelude_of_might"];
    let bare = ["furis_evo1_incarnon_form"];
    // 3.4 base, +3 = 6.4 with no crit-damage mod either way.
    assert!((cd_with(&[], &evo) - 6.4).abs() < 1e-9, "{}", cd_with(&[], &evo));
    // With +110%: (3.4 + 3.0) x 2.1 = 13.44, NOT 3.4 x 2.1 + 3.0 = 10.14.
    let modded = cd_with(&["primed_target_cracker"], &evo);
    assert!((modded - 13.44).abs() < 1e-6, "expected 13.44x, got {modded}");
    assert!((cd_with(&["primed_target_cracker"], &bare) - 7.14).abs() < 1e-6);
}

/// ...and it is CONDITIONAL: the perk pays only while the build's own crit
/// chance stays under 40%, so taking it means not building crit chance.
#[test]
fn prelude_of_might_switches_off_above_the_threshold() {
    let evo = ["furis_evo1_incarnon_form", "furis_prelude_of_might"];
    // The form's own 26% is under the line; Primed Pistol Gambit clears it.
    assert!((cd_with(&[], &evo) - 6.4).abs() < 1e-9);
    let over = cd_with(&["primed_pistol_gambit"], &evo);
    assert!((over - 3.4).abs() < 1e-9, "over 40% crit it must pay nothing, got {over}");
}

/// Headcracker is a LIVE buff, so it is asserted on the loaded spec rather
/// than on a panel: the 50% roll is the half that a summary drops.
#[test]
fn headcracker_carries_its_fifty_percent_roll() {
    let e = get("furis_headcracker").expect("furis_headcracker");
    let hit = e.effects.iter().find_map(|x| match x {
        EvoEffect::StackingFireRateOnHeadshot { per_stack, max_stacks, duration, chance, .. } => {
            Some((*per_stack, *max_stacks, *duration, *chance))
        }
        _ => None,
    });
    assert_eq!(
        hit,
        Some((0.05, 10, 2.0, 0.50)),
        "raw wikitext: +5% for 2s, x10, \"This effect has a 50% chance of activating\""
    );
}
