use super::*;
use crate::rules::damage::DamageType;

fn card(id: &str) -> ModDef {
    crate::data::mods::pool_for_build("coda_motovore", &[])
        .into_iter().find(|m| m.id == id).unwrap_or_else(|| panic!("missing {id}"))
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-4, "{actual} != {expected}");
}

/// M131 supplies a status panel and an amplified mod-card coefficient, not a
/// measured hit. Other source-only cases below remain separate regressions.
#[test]
fn golden_m131_motovore_panel_readings() {
    let measured: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"), "/../tests/golden/M131-coda-motovore.json"
    ))).unwrap();
    let base = WeaponBase::from_data(measured["weapon"].as_str().unwrap(), true, &[]);
    let prowess = card(measured["status"]["mods"][0].as_str().unwrap());
    let panel = resolve(&base, &[&prowess], StackPolicy::Emergent);
    close(panel.status_chance, measured["status"]["reported_status_chance"].as_f64().unwrap());
    let physical = card(measured["physical_mod"]["id"].as_str().unwrap());
    let panel = resolve(&base, &[&physical], StackPolicy::Emergent);
    let coefficient = panel.damage.get(DamageType::Impact) / base.base_vector.get(DamageType::Impact) - 1.0;
    close(coefficient, measured["physical_mod"]["reported_card_bonus"].as_f64().unwrap());
}

/// Wiki Coda_Motovore, Characteristics and Notes; source regressions, not
/// in-game golden readings. The 30% base remains separate from the wheel.
#[test]
fn motovore_wheel_uses_the_same_stat_buckets_as_equipped_mods() {
    let base = WeaponBase::from_data("coda_motovore", true, &[]);
    close(base.base_status_chance, 0.3);
    for policy in [StackPolicy::BaseOnly, StackPolicy::AssumedMax, StackPolicy::Emergent] {
        let bare = resolve(&base, &[], policy);
        close(bare.status_chance, 0.6);
        close(bare.fire_rate, 1.0);
        close(bare.range_m, 2.5);
        let impact = resolve(&base, &[&card("primed_heavy_trauma"), &card("fury")], policy);
        close(impact.fire_rate, 1.7);
        close(impact.status_chance, 0.3);
        let puncture = resolve(&base, &[&card("auger_strike"), &card("primed_reach")], policy);
        close(puncture.range_m, 6.9997);
        close(puncture.status_chance, 0.3);
        let slash = resolve(&base, &[&card("buzz_kill"), &card("melee_prowess")], policy);
        close(slash.status_chance, 0.87);
        close(slash.damage.get(DamageType::Slash), 683.47);
    }
}

#[test]
fn motovore_progenitor_impact_is_base_damage_and_does_not_select_the_wheel() {
    let mut base = WeaponBase::from_data("coda_motovore", true, &[]);
    crate::data::weapons::apply_valence(&mut base, "coda_motovore", "impact", 0.6);
    let bare = resolve(&base, &[], StackPolicy::Emergent);
    close(bare.modified_base, 400.0);
    close(bare.damage.get(DamageType::Impact), 233.325);
    close(bare.status_chance, 0.6);
    close(bare.fire_rate, 1.0);
    let modded = resolve(&base, &[&card("primed_heavy_trauma"), &card("fever_strike")], StackPolicy::Emergent);
    close(modded.modified_base, 400.0);
    close(modded.damage.get(DamageType::Impact), 2543.2425);
    close(modded.damage.get(DamageType::Toxin), 360.0);
    close(modded.fire_rate, 1.4);
}

#[test]
fn motovore_physical_riven_lines_share_mod_potency_and_cannot_make_negative_damage() {
    let base = WeaponBase::from_data("coda_motovore", true, &[]);
    let mut riven = card("fury");
    riven.id = "physical_riven";
    riven.effects = vec![ModEffect::Physical(DamageType::Impact, -0.982), ModEffect::Physical(DamageType::Puncture, 0.3)];
    let panel = resolve(&base, &[&riven], StackPolicy::Emergent);
    close(panel.damage.get(DamageType::Impact), 0.0);
    close(panel.damage.get(DamageType::Puncture), 233.31);
    close(panel.range_m, 4.0);
    close(panel.status_chance, 0.3);
    let ordinary = WeaponBase::from_data("magistar", true, &[]);
    let panel = resolve(&ordinary, &[&card("primed_heavy_trauma")], StackPolicy::Emergent);
    close(panel.damage.get(DamageType::Impact), 445.2);
    close(panel.fire_rate, ordinary.base_fire_rate);
}

#[test]
fn motovore_all_modes_keep_valence_and_the_weapon_physical_passive() {
    for form in crate::data::weapons::forms_of("coda_motovore") {
        let id = form.weapon_id;
        assert!(crate::data::weapons::valence_of(id).is_some(), "{id}");
        let mut base = WeaponBase::from_data(id, true, &[]);
        assert!(base.physical_mods.is_some(), "{id}");
        let direct = base.base_vector.total();
        let radial = base.radial.as_ref().map(|r| r.base_vector.total());
        let slam = base.slam.as_ref().map(|r| r.base_vector.total());
        crate::data::weapons::apply_valence(&mut base, id, "heat", 0.6);
        close(base.base_vector.total(), direct * 1.6);
        if let Some(amount) = radial {
            close(base.radial.as_ref().unwrap().base_vector.total(), amount * 1.6);
        }
        if let Some(amount) = slam {
            close(base.slam.as_ref().unwrap().base_vector.total(), amount * 1.6);
        }
        let panel = resolve(&base, &[&card("auger_strike")], StackPolicy::Emergent);
        close(panel.range_m, 4.0);
        if let Some(radial) = panel.radial {
            close(radial.radius_m, 15.0);
            close(radial.modified_base, 1200.0);
        }
    }
}
