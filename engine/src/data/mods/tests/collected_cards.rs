use super::*;
use crate::model::IndirectStat;

fn card(id: &str) -> ModDef {
    classes().into_iter().flat_map(class_pool).find(|m| m.id == id).unwrap_or_else(|| panic!("{id}"))
}

#[test]
fn partially_modelled_cards_name_their_unsupported_clauses() {
    assert_eq!(unmodeled_clauses("covert_lethality"), ["finisher damage"]);
    assert_eq!(unmodeled_clauses("amalgam_furax_body_count").len(), 2);
    assert!(unmodeled_clauses("amalgam_ripkas_true_steel")
        .iter().any(|s| s.contains("shotgun")));
    assert_eq!(unmodeled_clauses("sentient_incision").len(), 2);
    assert!(!lower_ranks_unmodelled("no_return"));
    assert!(lower_ranks_unmodelled("blade_of_truth"));
}

#[test]
fn collected_weapon_cards_apply_their_supported_clauses() {
    for id in ["arrow_mutation", "sniper_ammo_mutation", "primed_sniper_ammo_mutation",
        "covert_lethality", "amalgam_furax_body_count", "amalgam_ripkas_true_steel",
        "amanata_pressure", "avenging_truth", "blade_of_truth", "bright_purity",
        "justice_blades", "gleaming_blight", "toxic_blight", "entropy_flight",
        "entropy_detonation", "no_return", "tainted_clip", "depleted_reload",
        "target_acquired", "boreals_contempt", "niras_contempt", "aero_agility"] {
        assert!(!card(id).effects.is_empty(), "{id}: known clauses must reach the model");
    }
    for id in ["covert_lethality", "amalgam_furax_body_count", "amalgam_ripkas_true_steel",
        "amanata_pressure", "avenging_truth", "entropy_detonation"] {
        assert!(card(id).unmodeled, "{id}: its unsupported clause is still disclosed");
    }
}

#[test]
fn collected_card_package_values_keep_their_actual_precision_and_bucket() {
    assert!(card("no_return").effects.iter().any(|e| matches!(e,
        ModEffect::Physical(DamageType::Puncture, x) if crate::rules::upgrades::near_f32(*x, 0.90000006))));
    assert!(card("amalgam_ripkas_true_steel").effects.iter().any(|e| matches!(e,
        ModEffect::CritChanceHeavyDoubled(x) if crate::rules::upgrades::near_f32(*x, 1.8696))));
    assert!(card("boreals_contempt").effects.iter().any(|e| matches!(e,
        ModEffect::StatusDamage(x) if crate::rules::upgrades::near_f32(*x, 0.6))));
    assert!(card("niras_contempt").effects.iter().any(|e| matches!(e,
        ModEffect::StatusChance(x) if crate::rules::upgrades::near_f32(*x, 0.6))));
    for (id, max) in [("arrow_mutation", 0.504), ("sniper_ammo_mutation", 0.504),
        ("primed_sniper_ammo_mutation", 0.924)] {
        assert!(card(id).effects.iter().any(|e| matches!(e,
            ModEffect::Indirect(IndirectStat::AmmoConversion, x) if crate::rules::upgrades::near_f32(*x, max))), "{id}");
        assert!(at_rank(&format!("{id}@0")).unwrap().effects.iter().any(|e| matches!(e,
            ModEffect::Indirect(IndirectStat::AmmoConversion, x) if crate::rules::upgrades::near_f32(*x, 0.084))), "{id}");
    }
}

#[test]
fn syndicate_augment_stats_and_the_shared_radial_are_both_kept() {
    for (id, syndicate) in [("blade_of_truth", "truth"), ("bright_purity", "purity"),
        ("justice_blades", "justice"), ("gleaming_blight", "blight"),
        ("toxic_blight", "blight"), ("entropy_flight", "entropy"),
        ("avenging_truth", "truth"), ("entropy_detonation", "entropy")] {
        assert!(card(id).effects.iter().any(|e| matches!(e,
            ModEffect::SyndicateRadial { syndicate: s, amount } if *s == syndicate && (*amount - 1.0).abs() < 1e-12)), "{id}");
        assert!(at_rank(&format!("{id}@0")).is_none(), "{id}: lower-rank affinity thresholds are not simulated");
    }
    assert!(card("covert_lethality").effects.iter().any(|e| matches!(e, ModEffect::InitialCombo(x) if *x == 16.0)));
    assert!(card("amanata_pressure").effects.iter().any(|e| matches!(e, ModEffect::BaseDamage(x) if *x == 3.3)));
    assert_eq!(card("amanata_pressure").family, card("pressure_point").family);
}

#[test]
fn ordinary_new_card_ranks_are_available_and_signed_penalties_survive() {
    for id in ["no_return", "tainted_clip", "depleted_reload", "target_acquired",
        "boreals_contempt", "niras_contempt", "aero_agility", "covert_lethality",
        "amalgam_furax_body_count", "amalgam_ripkas_true_steel", "amanata_pressure"] {
        assert!(at_rank(&format!("{id}@0")).is_some(), "{id}: its linear rank ladder is supported");
    }
    assert!(card("tainted_clip").effects.iter().any(|e| matches!(e,
        ModEffect::ReloadSpeed(x) if crate::rules::upgrades::near_f32(*x, -(0.3)))));
    assert!(card("depleted_reload").effects.iter().any(|e| matches!(e,
        ModEffect::MagazineCapacity(x) if crate::rules::upgrades::near_f32(*x, -(0.6)))));
}

#[test]
fn aero_agility_speeds_reload_only_during_aim_glide() {
    use crate::build::loadout::resolve_for;
    use crate::model::{StackPolicy, WeaponBase};

    let base = WeaponBase::from_data("vectis", false, &[]);
    let aero = card("aero_agility");
    let neutral = crate::data::tenno::default_tenno();
    let bare = resolve_for(&base, &[], StackPolicy::Emergent, neutral).reload_seconds;
    assert!(bare > 0.0);
    for (airborne, sliding, aim_gliding, expected) in [
        (false, false, false, bare),
        (true, false, false, bare),
        (false, true, false, bare),
        (false, false, true, bare / 2.0),
    ] {
        let mut tenno = neutral.clone();
        tenno.state.airborne = airborne;
        tenno.state.sliding = sliding;
        tenno.state.aim_gliding = aim_gliding;
        let panel = resolve_for(&base, &[&aero], StackPolicy::Emergent, &tenno);
        assert!((panel.reload_seconds - expected).abs() < 1e-12,
            "airborne={airborne}, sliding={sliding}, aim_gliding={aim_gliding}: {} vs {expected}", panel.reload_seconds);
    }
}
