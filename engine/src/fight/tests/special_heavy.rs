//! Ranged heavy inputs keep their own profile, counters and landing identity.
use super::*;

fn fixture(id: &str, duration: f64) -> FightParams {
    let mut base = crate::model::WeaponBase::from_data(id, true, &[]);
    base.base_crit_chance = 0.0;
    base.base_status_chance = 0.0;
    if let Some(h) = base.melee_heavy.as_mut() {
        h.base_crit_chance = 0.0;
        h.base_status_chance = 0.0;
    }
    let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    FightParams::from_panel(&panel, &crate::arena::Arena::training(duration), &ArcaneFx::none())
}

#[test]
fn sarpa_reads_and_spends_combo_once_for_the_whole_burst() {
    let plain = fixture("sarpa_heavy", 2.4);
    let mut charged = plain.clone();
    charged.initial_combo = 100.0;
    let one = run_once(&plain, &mut Rng::new(7));
    let six = run_once(&charged, &mut Rng::new(7));
    assert_eq!(one.shots, 5);
    assert_eq!(six.shots, 5);
    assert!((six.sources.direct / one.sources.direct - 6.0).abs() < 1e-10);
}

#[test]
fn tennokai_uses_the_heavy_projectile_profile_and_keeps_it_during_flight() {
    let mut p = fixture("corufell", 8.0);
    assert!(p.melee_heavy.is_some());
    p.tennokai.enabled = true;
    p.tennokai.every_n_hits = 1;
    p.tennokai.window_seconds = 10.0;
    let h = p.melee_heavy.as_mut().unwrap();
    h.tennokai = p.tennokai;
    h.damage = DamageVector::new().with(DamageType::Heat, 10_000.0);
    h.dot_modified_base = Some(10_000.0);
    h.projectile_speed_mps = Some(0.5);
    let actual = run_once(&p, &mut Rng::new(7));
    let mut no_profile = p.clone();
    no_profile.melee_heavy = None;
    let wrong = run_once(&no_profile, &mut Rng::new(7));
    assert!(actual.sources.direct > wrong.sources.direct * 2.0,
        "ranged profile {} vs scalar swing {}", actual.sources.direct, wrong.sources.direct);
}

#[test]
fn every_special_heavy_fires_after_charge_and_can_reach_beyond_melee() {
    for id in ["redeemer_heavy", "redeemer_prime_heavy", "sarpa_heavy", "stropha_heavy",
        "vastilok_heavy", "syam_heavy", "quassus_heavy", "quassus_prime_heavy", "arum_spinosa_heavy",
        "edun_heavy", "argo_and_vel_heavy", "corufell_heavy", "corufell_prime_heavy"] {
        let mut p = fixture(id, 5.0);
        p.target_at = crate::rules::space::Vec2::new(0.0, 6.0);
        assert!(p.follow_through.is_none(), "{id}");
        assert!(run_once(&p, &mut Rng::new(7)).sources.direct > 0.0, "{id}");
        p.duration_seconds = 0.1;
        assert_eq!(run_once(&p, &mut Rng::new(7)).sources.direct, 0.0, "{id}: cannot fire before charging");
    }
}
