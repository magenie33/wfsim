use super::*;
use crate::model::{ModEffect, StackPolicy, TargetPreference as P, WeaponBase};
use crate::rules::space::Vec2;

fn panel(weapon: &str, ids: &[&str]) -> crate::build::loadout::ResolvedPanel {
    let base = WeaponBase::from_data(weapon, false, &[]);
    let mut pool = crate::data::mods::pool_for_weapon(weapon);
    crate::data::mods::with_ranks(&mut pool, ids.iter().copied());
    let cards: Vec<_> = ids.iter().map(|id| pool.iter().find(|m| m.id == *id).unwrap()).collect();
    crate::build::loadout::resolve(&base, &cards, StackPolicy::Emergent)
}

fn params(weapon: &str, ids: &[&str], positions: &[(f64, f64)]) -> FightParams {
    let mut panel = panel(weapon, ids);
    panel.crit_chance = 0.0;
    panel.status_chance = 0.0;
    let mut arena = crate::arena::Arena::training(0.01);
    arena.target_at = Vec2::new(0.0, 10.0);
    arena.body_parts = vec![BodyPart { name: "torso".into(), aim_weight: 1.0,
        multiplier: 1.0, is_head: false, is_weak_point: false, crit_bonus: false }];
    arena.others = positions.iter().enumerate().map(|(i, &(x, y))| crate::formation::FoeSpec {
        id: format!("body{i}"), params: Foe::training_dummy(), body_parts: arena.body_parts.clone(),
        at: Vec2::new(x, y),
    }).collect();
    FightParams::from_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none())
}

#[test]
fn posture_rank_values_match_the_native_callback_float32_arguments() {
    let source: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/client/postures-2026-10-28.json")).unwrap();
    assert_eq!(source["cards"].as_array().unwrap().len(), 6);
    for row in source["scalar_cases"].as_array().unwrap() {
        let card = source["cards"].as_array().unwrap().iter().find(|c| c["path"] == row["path"]).unwrap();
        let name = card["id"].as_str().unwrap();
        if name == "elusive_posture" { continue }
        let rank = row["rank"].as_u64().unwrap() as u32;
        let id = crate::data::mods::ranked_id(name, rank, 3);
        let pool = crate::data::mods::pool_for_weapon("adarza_claws");
        let m = if rank == 3 { pool.iter().find(|m| m.id == name).unwrap().clone() }
            else { crate::data::mods::at_rank(&id).unwrap() };
        let actual = m.effects.iter().find_map(|e| match e {
            ModEffect::OverguardDamage(v) | ModEffect::StatusDuration(v) | ModEffect::CombinedElement(_, v) => Some(*v),
            _ => None,
        }).unwrap();
        assert_eq!(actual, row["native_scalar_argument_float32"].as_f64().unwrap(), "{id}");
    }
}

#[test]
fn persistent_adds_impact_from_the_whole_base_even_without_innate_impact() {
    let mut base = WeaponBase::from_data("adarza_claws", false, &[]);
    base.base_vector = DamageVector::new().with(DamageType::Slash, 100.0);
    let pool = crate::data::mods::pool_for_weapon("adarza_claws");
    let card = pool.iter().find(|m| m.id == "persistent_posture").unwrap();
    let p = crate::build::loadout::resolve(&base, &[card], StackPolicy::Emergent);
    assert!((p.modified_base - 100.0).abs() < 1e-9);
    assert!((p.damage.total() - 140.00000059604645).abs() < 1e-7, "{:?}", p.damage);
}

#[test]
fn protector_forces_its_callback_rank_count_on_every_swipe_recipient() {
    for rank in 0..=3 {
        let id = crate::data::mods::ranked_id("protector_posture", rank, 3);
        let p = params("adarza_claws", &[&id, "swipe"], &[(0.5, 10.0), (1.0, 10.0)]);
        let r = run_once(&p, &mut Rng::new(9));
        assert_eq!(r.shots, 1);
        assert_eq!(r.procs, (rank + 1) * 3, "{id}");
        assert!(r.taken.by_body().0.iter().take(3).all(|v| *v > 0.0));
    }
}

#[test]
fn elusive_suppresses_ordinary_attacks_for_both_claw_families() {
    for weapon in ["adarza_claws", "chesa_claws"] {
        let mut p = params(weapon, &["elusive_posture"], &[]);
        p.duration_seconds = 5.0;
        let r = run_once(&p, &mut Rng::new(4));
        assert_eq!(r.shots, 0, "{weapon}");
        assert_eq!(r.total_damage(), 0.0);
    }
}

#[test]
fn assassin_selects_the_eximus_and_uses_its_own_overguard() {
    let mut p = params("adarza_claws", &["assassin_posture"], &[(5.0, 10.0)]);
    p.others[0].params.eximus = true;
    p.others[0].params.can_be_eximus = true;
    p.others[0].params.base_overguard = 1e12;
    let boosted = run_once(&p, &mut Rng::new(4));
    let mut unboosted = p.clone();
    unboosted.overguard_damage_bonus = 0.0;
    let bare = run_once(&unboosted, &mut Rng::new(4));
    assert_eq!(boosted.taken.by_body().0[0], 0.0);
    assert!((boosted.effective_damage() / bare.effective_damage() - 4.0).abs() < 1e-9,
        "boosted {} bare {}", boosted.effective_damage(), bare.effective_damage());
    p.others[0].params.eximus = false;
    p.others[0].params.base_overguard = 0.0;
    unboosted = p.clone();
    unboosted.overguard_damage_bonus = 0.0;
    assert_eq!(run_once(&p, &mut Rng::new(4)).effective_damage(), run_once(&unboosted, &mut Rng::new(4)).effective_damage());
}

#[test]
fn protector_prefers_a_body_within_its_owner_range() {
    let p = params("chesa_claws", &["protector_posture"], &[(0.0, 30.0)]);
    let mut p = p;
    p.target_at = Vec2::new(0.0, 40.0);
    p.others[0].at = Vec2::new(0.0, 5.0);
    let r = run_once(&p, &mut Rng::new(4));
    assert_eq!(r.taken.by_body().0[0], 0.0);
    assert!(r.taken.by_body().0[1] > 0.0);
}

#[test]
fn frenzied_changes_targets_while_persistent_holds_a_living_individual() {
    for (card, changes) in [("frenzied_posture", true), ("persistent_posture", false)] {
        let mut p = params("chesa_claws", &[card], &[(1.0, 10.0)]);
        p.duration_seconds = 2.1;
        let r = run_once(&p, &mut Rng::new(7));
        assert!(r.shots >= 2);
        assert_eq!(r.taken.by_body().0[1] > 0.0, changes, "{card}");
    }
    assert_eq!(panel("adarza_claws", &["frenzied_posture"]).target_preference, Some(P::Different));
    assert_eq!(panel("adarza_claws", &["frenzied_posture"]).status_duration_multiplier, 1.800000011920929);
}

#[test]
fn swipe_centres_on_the_posture_selected_body_and_respects_its_cap() {
    let mut p = params("adarza_claws", &["assassin_posture", "swipe@0"], &[(10.0, 10.0), (10.5, 10.0), (11.0, 10.0)]);
    p.others[0].params.eximus = true;
    p.others[0].params.can_be_eximus = true;
    let r = run_once(&p, &mut Rng::new(4));
    let hits = r.taken.by_body().0;
    assert_eq!(hits[0], 0.0);
    assert!(hits[1] > 0.0 && hits[2] > 0.0);
    assert_eq!(hits[3], 0.0);
    let record = crate::fight::replay::record(&p, 4, 0.0, 0.01, 100, 0);
    for (subject, origin) in [(1, crate::record::Origin::Own), (2, crate::record::Origin::MeleeRadius)] {
        assert!(record.events().iter().any(|e| e.subject == Some(subject)
            && matches!(&e.kind, crate::record::Kind::Damage(d) if d.origin == origin)));
    }
}

#[test]
fn balanced_keeps_lone_target_damage_and_statuses_unchanged() {
    let mut bare = params("adarza_claws", &["venom_teeth"], &[]);
    bare.duration_seconds = 5.0;
    bare.status_chance = 1.0;
    let mut balanced = bare.clone();
    balanced.target_preference = Some(P::Free);
    let plain = run_once(&bare, &mut Rng::new(17));
    let selected = run_once(&balanced, &mut Rng::new(17));
    assert_eq!(selected.shots, plain.shots);
    assert_eq!(selected.procs, plain.procs);
    assert!((selected.effective_damage() - plain.effective_damage()).abs() < 1e-8,
        "selected {} plain {}", selected.effective_damage(), plain.effective_damage());
}

#[test]
fn elusive_does_not_disable_another_combatant() {
    let mut passive = params("adarza_claws", &["elusive_posture"], &[]);
    passive.duration_seconds = 5.0;
    let mut active = params("chesa_claws", &["balanced_posture"], &[]);
    active.duration_seconds = 5.0;
    let solo = run_once(&active, &mut Rng::new(8));
    let pair = run_once(&passive.and_also(active), &mut Rng::new(8));
    assert_eq!(pair.dealt.by_combatant().0[0], 0.0);
    assert_eq!(pair.dealt.by_combatant().0[1], solo.effective_damage());
}
