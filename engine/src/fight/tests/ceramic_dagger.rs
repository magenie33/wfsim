use super::*;

fn panel(mods: &[&str], incarnon: bool) -> crate::build::loadout::ResolvedPanel {
    let evo = if incarnon { vec!["ceramic_dagger_evo1_incarnon_form"] } else { Vec::new() };
    let base = crate::model::WeaponBase::from_data("ceramic_dagger_heavy", false, &evo);
    let pool = crate::data::mods::pool_for_weapon("ceramic_dagger_heavy");
    let refs: Vec<_> = mods.iter().map(|id| pool.iter().find(|m| m.id == *id).unwrap()).collect();
    crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent)
}

#[test]
fn spectral_daggers_have_their_own_contact_and_explosion() {
    let p = panel(&[], true);
    let dagger = &p.melee_profiles["spectral_dagger"];
    assert_eq!(dagger.modified_base, 350.0);
    assert_eq!(dagger.projectile_speed_mps, Some(60.0));
    assert_eq!(dagger.co_behavior, crate::model::CoBehavior::Independent);
    let radial = dagger.radial.as_ref().unwrap();
    assert_eq!(radial.radius_m, 4.0);
    assert!(!radial.takes_condition_overload);
    let events: Vec<_> = p.combo_script.iter().filter(|h| h.profile.as_deref() == Some("spectral_dagger")).collect();
    assert_eq!(events.len(), 2);
    assert!(events.iter().all(|h| h.hits == 2 && h.incarnon_only && h.combo_points == 0.0));
}

#[test]
fn spectral_daggers_exclude_melee_damage_but_keep_elemental_mods() {
    let plain = panel(&[], false);
    let incarnon = panel(&[], true);
    let damage = panel(&["pressure_point", "killing_blow"], true);
    let element = panel(&["molten_impact"], true);
    assert!(damage.modified_base > incarnon.modified_base);
    for p in [&plain, &incarnon, &damage] {
        assert_eq!(p.melee_profiles["spectral_dagger"].modified_base, 350.0);
    }
    assert!(element.melee_profiles["spectral_dagger"].damage.total() > incarnon.melee_profiles["spectral_dagger"].damage.total());
}

#[test]
fn spectral_daggers_emit_only_while_incarnon_is_armed() {
    let armed = panel(&[], true);
    let unarmed = panel(&[], false);
    let mut arena = crate::arena::Arena::training(1.4);
    arena.target_at = crate::rules::space::Vec2::new(0.0, 5.0);
    let mut p = FightParams::for_panel(&armed, &arena, &ArcaneFx::none(), || unarmed.clone());
    let before = run_once(&p, &mut Rng::new(11));
    assert_eq!(before.sources.direct, 0.0);
    let mut config = BuffConfig::new();
    config.insert("melee_incarnon".to_string(), (1, true));
    p.apply_buff_config(&config);
    let after = run_once(&p, &mut Rng::new(11));
    assert!(after.sources.direct > 0.0, "the two projectiles must reach beyond the physical dagger");
    assert!(after.sources.radial > 0.0, "each spectral dagger also explodes");
}

#[test]
fn spectral_daggers_exclude_evolution_base_damage_but_keep_critical_chance() {
    let plain = panel(&[], true);
    for evo in ["ceramic_dagger_gun_and_blade", "ceramic_dagger_breachers_opportunity"] {
        let evos = ["ceramic_dagger_evo1_incarnon_form", evo, "ceramic_dagger_absolute_valor"];
        let base = crate::model::WeaponBase::from_data("ceramic_dagger_heavy", false, &evos);
        let evolved = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
        assert!(evolved.modified_base > plain.modified_base);
        assert_eq!(evolved.melee_profiles["spectral_dagger"].modified_base, 350.0);
        assert!(evolved.melee_profiles["spectral_dagger"].crit_chance > plain.melee_profiles["spectral_dagger"].crit_chance);
    }
}
