use super::*;
use crate::model::{StackPolicy, WeaponBase};
use crate::rules::space::Vec2;

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../tests/fixtures/client/swipe-2026-10-28.json")).unwrap()
}

fn params(mods: &[&str], neighbours: &[(f64, f64)]) -> FightParams {
    let base = WeaponBase::from_data("adarza_claws", false, &[]);
    let mut pool = crate::data::mods::pool_for_weapon("adarza_claws");
    crate::data::mods::with_ranks(&mut pool, mods.iter().copied());
    let cards: Vec<_> = mods.iter().map(|id| pool.iter().find(|m| m.id == *id).unwrap()).collect();
    let mut panel = crate::build::loadout::resolve(&base, &cards, StackPolicy::Emergent);
    panel.crit_chance = 0.0;
    if let Some(inputs) = panel.critical_inputs.as_mut() { inputs.set_supplied_chance(panel.crit_chance, panel.base_crit_chance); }
    panel.status_chance = 0.0;
    let mut arena = crate::arena::Arena::training(0.01);
    arena.target_at = Vec2::new(0.0, 10.0);
    arena.body_parts = vec![BodyPart { name: "torso".into(), aim_weight: 1.0,
        multiplier: 1.0, is_head: false, is_weak_point: false, crit_bonus: false }];
    arena.others = neighbours.iter().enumerate().map(|(i, &(x, y))| crate::formation::FoeSpec {
        id: format!("near{i}"), params: Foe::training_dummy(), body_parts: arena.body_parts.clone(),
        at: Vec2::new(x, y + 10.0),
    }).collect();
    FightParams::from_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none())
}

#[test]
fn swipe_source_ranks_and_weapon_ancestry_bind_the_pool() {
    let f = fixture();
    let weapons = f["weapons"].as_array().unwrap();
    assert_eq!(weapons.len(), 8);
    for w in crate::data::weapons::all().iter().filter(|w| w.class == "beast_claws") {
        let eligible = weapons.iter().find(|r| r["weapon"] == w.id);
        let pool = crate::data::mods::pool_for_weapon(&w.id);
        assert_eq!(pool.iter().any(|m| m.id == "swipe"), eligible.is_some(), "{}", w.id);
        let Some(raw) = eligible else { continue };
        let yaml: serde_norway::Value = serde_norway::from_str(crate::data::file(
            &format!("weapons/beast_claws/{}.yaml", w.id)).unwrap()).unwrap();
        assert_eq!(yaml["internal_name"].as_str(), raw["path"].as_str());
        assert!(raw["parent_chain"].as_array().unwrap().contains(&f["card"]["item_compatibility"]));
        assert_eq!(raw["max_hit_avatars_per_attack"], "1");
        assert_eq!(raw["attack_data"]["radius"], "1");
        for rank in 0..=3 {
            let id = crate::data::mods::ranked_id("swipe", rank, 3);
            let card = if rank == 3 { pool.iter().find(|m| m.id == "swipe").unwrap().clone() }
                else { crate::data::mods::at_rank(&id).unwrap() };
            let base = WeaponBase::from_data(&w.id, false, &[]);
            let panel = crate::build::loadout::resolve(&base, &[&card], StackPolicy::Emergent);
            let area = panel.melee_radius.unwrap();
            assert_eq!(area.max_targets, rank as usize + 2);
            assert_eq!(area.radius_m, 1.0 + (rank + 1) as f64 * 0.5);
        }
    }
    assert_eq!(crate::data::companions::companion_mod("swipe").unwrap().compat, "kavat_claws");
    assert!(crate::data::companions::seatable_by("smeeta_kavat").all(|m| m.id != "swipe"));
}

#[test]
fn swipe_hits_each_neighbour_once_up_to_its_rank_limit() {
    let crowd = [(0.5, 0.0), (0.0, 0.5), (-0.5, 0.0), (0.0, -0.5), (0.6, 0.0), (0.7, 0.0)];
    for (mods, count) in [(vec![], 1), (vec!["swipe@0"], 2), (vec!["swipe@1"], 3),
                          (vec!["swipe@2"], 4), (vec!["swipe"], 5)] {
        let p = params(&mods, &crowd);
        let r = run_once(&p, &mut Rng::new(4));
        let hit: Vec<_> = r.taken.by_body().0.into_iter().filter(|d| *d > 0.0).collect();
        assert_eq!(r.shots, 1);
        assert_eq!(hit.len(), count, "{mods:?}");
        for amount in &hit { assert!((amount - hit[0]).abs() < 1e-9, "{hit:?}"); }
    }
}

#[test]
fn swipe_range_uses_intersections_and_nearest_bodies_at_the_aimed_contact() {
    let crowd = [(5.0, 0.0), (3.251, 0.0), (3.25, 0.0), (2.0, 0.0), (0.5, 0.0)];
    let p = params(&["swipe"], &crowd);
    assert_eq!(p.melee_radius_struck(0.0), vec![0, 5, 4, 3]);
    let r = run_once(&p, &mut Rng::new(4));
    let hit = r.taken.by_body().0;
    assert_eq!(hit[1], 0.0);
    assert_eq!(hit[2], 0.0);
    assert!(hit[3] > 0.0 && hit[4] > 0.0 && hit[5] > 0.0);
    let low = run_once(&params(&["swipe@0"], &[(1.751, 0.0)]), &mut Rng::new(4));
    assert_eq!(low.taken.by_body().0[1], 0.0);
}

#[test]
fn swipe_does_not_inflate_a_lone_target_and_records_other_contacts() {
    let alone = run_once(&params(&[], &[]), &mut Rng::new(4));
    let swipe = run_once(&params(&["swipe"], &[]), &mut Rng::new(4));
    assert_eq!(alone.taken.by_body().0, swipe.taken.by_body().0);
    let p = params(&["swipe"], &[(0.5, 0.0)]);
    let record = crate::fight::replay::record(&p, 4, 0.0, 0.01, 100, 0);
    assert!(record.events().iter().any(|e| e.subject == Some(1)
        && matches!(&e.kind, crate::record::Kind::Damage(d) if d.origin == crate::record::Origin::MeleeRadius)));
}
