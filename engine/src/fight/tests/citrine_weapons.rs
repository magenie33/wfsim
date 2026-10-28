use super::*;

fn panel(id: &str, mods: &[&str]) -> crate::build::loadout::ResolvedPanel {
    let base = crate::model::WeaponBase::from_data(id, false, &[]);
    let pool = crate::data::mods::pool_for_weapon(id);
    let refs: Vec<_> = mods.iter().map(|id| pool.iter().find(|m| m.id == *id).expect("mod")).collect();
    crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent)
}

#[test]
fn steflos_prime_keeps_the_family_projectile_and_its_own_falloff() {
    let base = panel("steflos", &[]);
    let prime = panel("steflos_prime", &[]);
    assert_eq!(prime.modified_base, 480.0);
    assert_eq!(prime.crit_chance, 0.18);
    assert_eq!(prime.status_chance, 0.28);
    assert_eq!(prime.projectile_width_m, 2.0);
    assert!(prime.infinite_body_punch_through);
    assert_eq!(prime.co_behavior, base.co_behavior);
    let p = FightParams::from_panel(&prime, &crate::arena::Arena::training(1.0), &ArcaneFx::none());
    assert_eq!(p.falloff.unwrap().end_m, 24.0);
    assert_eq!(p.falloff.unwrap().keep, 0.375);
    let s = crate::data::weapons::spec("steflos_prime").unwrap();
    assert!(s.unmodeled.iter().any(|line| line.contains("does not stop")));
}

#[test]
fn corufell_heavy_uses_projectile_stats_and_windup_mods() {
    for (id, damage) in [("corufell_heavy", 600.0), ("corufell_prime_heavy", 650.0)] {
        let p = panel(id, &[]);
        assert_eq!(p.modified_base, damage);
        assert_eq!(p.status_chance, 0.14);
        assert!(p.spends_combo && p.infinite_body_punch_through);
        assert!(p.follow_through.is_none());
        assert_eq!(p.co_behavior, crate::model::CoBehavior::Independent);
        let kb = panel(id, &["killing_blow"]);
        assert!(kb.combo_script[0].windup_seconds < p.combo_script[0].windup_seconds);
        assert!(kb.heavy_attack_damage > p.heavy_attack_damage);
    }
    let normal = panel("corufell_heavy", &[]);
    let prime = panel("corufell_prime_heavy", &[]);
    assert!((normal.combo_script[0].windup_seconds / prime.combo_script[0].windup_seconds - 1.5).abs() < 1e-12);
    for id in ["corufell", "corufell_prime"] {
        assert!(crate::data::weapons::play_modes(id).iter().any(|m| m.id == "heavy"));
    }
}

#[test]
fn corufell_energy_crosses_bodies_beyond_melee_reach_without_follow_through() {
    let mut arena = crate::arena::Arena::training(1.0);
    arena.target_at = crate::rules::space::Vec2::new(0.0, 5.0);
    arena.others = (6..=8).map(|y| crate::formation::FoeSpec {
        id: String::new(), params: Foe::training_dummy(), body_parts: BodyPart::humanoid(),
        at: crate::rules::space::Vec2::new(0.0, f64::from(y)),
    }).collect();
    let mut p = FightParams::from_panel(&panel("corufell_prime_heavy", &[]), &arena, &ArcaneFx::none());
    p.base_crit_chance = 0.0;
    p.status_chance = 0.0;
    let all = run_once(&p, &mut Rng::new(3));
    p.others.clear();
    let one = run_once(&p, &mut Rng::new(3));
    assert!(one.sources.direct > 0.0, "the projectile reaches beyond the 2.6 m scythe");
    assert_eq!(all.taken.touched(), 4);
    assert!((all.sources.direct / one.sources.direct - 4.0).abs() < 1e-10);
}
