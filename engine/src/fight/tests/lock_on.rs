//! THE HAALVU'S MINIATURE RIFLES, in outline (`model::LockOn`).
use super::*;

/// The burst at `gap_m` from one body, the aim on its head, crit and status
/// off and every part worth the same, so damage counts landed rounds.
fn burst_at(gap_m: f64, lock: bool) -> FightParams {
    let base = crate::model::WeaponBase::from_data("haalvu_alt", false, &[]);
    let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(60.0), &crate::data::arcanes::ArcaneFx::none());
    p.player_at = crate::rules::space::Vec2::ORIGIN;
    p.target_at = crate::rules::space::Vec2::new(0.0, gap_m + crate::rules::space::CONTACT_RANGE_M);
    p.body_parts = vec![
        BodyPart { name: "head".into(), aim_weight: 1.0, multiplier: 1.0, is_head: true, is_weak_point: true, crit_bonus: false },
        BodyPart { name: "body".into(), aim_weight: 0.0, multiplier: 1.0, is_head: false, is_weak_point: false, crit_bonus: false },
    ];
    p.crit_multiplier = 1.0;
    p.base_crit_chance = 0.0;
    p.status_chance = 0.0;
    p.base_status_chance = 0.0;
    p.foe = frail_target(TargetMode::InfiniteHealth, 0.0, 0.0);
    if !lock {
        p.lock_on = None;
    }
    p
}

/// **THE OUTLINE, BY THE NUMBERS**: nothing at contact, half the rounds and
/// every head lost at the edge against one body, a third fewer misses for each
/// further body, none at four, and nothing beyond the reach.
#[test]
fn a_lock_on_loses_rounds_and_heads_with_distance_and_fewer_in_a_crowd() {
    let l = crate::data::weapons::spec("haalvu_alt").and_then(|s| s.attack.lock_on).expect("the burst locks on");
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    let at = |gap, n| l.at(gap, n).unwrap();
    assert!(near(at(0.0, 1).0, 0.0) && near(at(0.0, 1).1, 1.0));
    assert!(near(at(10.0, 1).0, 0.5) && near(at(10.0, 1).1, 0.0));
    assert!(near(at(5.0, 1).0, 0.25) && near(at(5.0, 1).1, 0.5));
    assert!(near(at(10.0, 2).0, 0.5 * 2.0 / 3.0));
    assert!(near(at(10.0, 4).0, 0.0) && near(at(10.0, 6).0, 0.0));
    assert!(l.at(10.5, 1).is_none());
}

/// **THE BOARD IS UNTOUCHED**: at contact the lock-on draws nothing, so the
/// burst deals exactly what it dealt before it locked on — and at 10 m against
/// one body half its rounds land and none on the head.
#[test]
fn at_contact_a_lock_on_changes_nothing_and_at_its_edge_halves_the_burst() {
    let (with, without) = (monte_carlo(&burst_at(0.0, true), 3, 7), monte_carlo(&burst_at(0.0, false), 3, 7));
    assert_eq!(with.mean_damage, without.mean_damage);
    assert_eq!(with.mean_headshot_rate, without.mean_headshot_rate);
    assert!((with.mean_headshot_rate - 1.0).abs() < 1e-9, "contact on the head is every head");
    let edge = monte_carlo(&burst_at(10.0, true), 3, 7);
    let share = edge.mean_damage / with.mean_damage;
    assert!((0.46..=0.54).contains(&share), "half the rounds land: {share}");
    assert_eq!(edge.mean_headshot_rate, 0.0);
}
