// SPDX-License-Identifier: AGPL-3.0-or-later
use wfsim_engine::data::weapons;
use wfsim_engine::model::{BlastKind, StackPolicy, WeaponBase};

#[test]
fn wolf_sledge_separates_automatic_collision_and_manual_heavy_explosions() {
    let source = weapons::all().iter().find(|w| w.id == "wolf_sledge_throw").unwrap();
    assert_eq!(source.class, "hammer");
    assert!(!source.mod_pools.iter().any(|p| p == "glaive"));
    assert!(!source.attack.spends_combo, "collision hits earn combo");
    let recall = source.attack.radial.as_ref().unwrap();
    assert_eq!(recall.blast_kind, BlastKind::InFlight);
    assert_eq!(recall.damage["blast"], 1554.0);
    assert_eq!(recall.status_chance, Some(0.5));
    assert_eq!(recall.falloff_reduction, Some(0.0));
    let collision = source.attack.thrown_bounce.as_ref().unwrap();
    assert_eq!(collision.explosion_chance, 1.0);
    assert_eq!(collision.explosion.damage["blast"], 777.0);
    assert_eq!(collision.explosion.status_chance, Some(0.5));
    assert_eq!(collision.explosion.falloff_reduction, Some(0.4));
    let base = WeaponBase::from_data("wolf_sledge_throw", false, &[]);
    let panel = wfsim_engine::build::loadout::resolve(&base, &[], StackPolicy::Emergent);
    assert_eq!(panel.thrown_bounce.unwrap().explosion_chance, 1.0);
    let ordinary = WeaponBase::from_data("glaive_throw", false, &[]);
    let panel = wfsim_engine::build::loadout::resolve(&ordinary, &[], StackPolicy::Emergent);
    assert_eq!(panel.thrown_bounce.unwrap().explosion_chance, 0.0);
}

#[test]
fn an_innate_collision_explosion_needs_no_throw_mod_or_heavy_press() {
    use wfsim_engine::data::apl::{Action, Apl, Rule, When};
    use wfsim_engine::data::arcanes::ArcaneFx;
    use wfsim_engine::fight::{run_once, FightParams};
    use wfsim_engine::rules::rng::Rng;
    let base = WeaponBase::from_data("wolf_sledge_throw", false, &[]);
    let panel = wfsim_engine::build::loadout::resolve(&base, &[], StackPolicy::Emergent);
    let arena = wfsim_engine::arena::Arena::training(1.0);
    let mut params = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
    params.base_crit_chance = 0.0;
    params.status_chance = 0.0;
    params.thrown_bounce.as_mut().unwrap().explosion.crit_chance = 0.0;
    params.thrown_bounce.as_mut().unwrap().explosion.status_chance = 0.0;
    params.radial.as_mut().unwrap().crit_chance = 0.0;
    params.radial.as_mut().unwrap().status_chance = 0.0;
    let collision = run_once(&params, &mut Rng::new(7));
    assert!(collision.sources.direct > 0.0);
    assert!(collision.sources.radial > 0.0);
    let mut no_collision = params.clone();
    no_collision.thrown_bounce.as_mut().unwrap().explosion_chance = 0.0;
    let quiet = run_once(&no_collision, &mut Rng::new(7));
    assert_eq!(quiet.sources.radial, 0.0);
    params.apl_inserted = Apl(vec![Rule { action: Action::Detonate, when: When::Always }]);
    let heavy = run_once(&params, &mut Rng::new(7));
    assert!(heavy.sources.radial > collision.sources.radial);
}
