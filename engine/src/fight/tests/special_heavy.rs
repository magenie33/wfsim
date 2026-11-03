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

#[test]
fn a_projectile_spends_combo_at_release_before_its_delayed_hit() {
    let mut p = fixture("sarpa_heavy", 10.0);
    p.initial_combo = 100.0;
    p.projectile_speed_mps = Some(1.0);
    p.spread = None;
    p.body_parts = vec![BodyPart { name: "body".into(), multiplier: 1.0, aim_weight: 1.0, crit_bonus: false, is_head: false, is_weak_point: false }];
    p.target_at = crate::rules::space::Vec2::new(0.0, 5.0);
    for h in &mut p.combo_script { h.windup_seconds = 0.0; }
    let rec = record(&p, 7, 0.0, 10.0, 20_000, 0);
    let mut contacts: Vec<(f64, f64)> = Vec::new();
    for e in rec.events() {
        if let crate::record::Kind::Damage(d) = &e.kind {
            if d.origin == crate::record::Origin::Own && !d.radial {
                if let Some((_, amount)) = contacts.last_mut().filter(|(t, _)| (e.t - *t).abs() < 1e-9) {
                    *amount += d.effective;
                } else { contacts.push((e.t, d.effective)); }
            }
        }
    }
    let amounts = contacts.iter().map(|(_, damage)| *damage).collect::<Vec<_>>();
    assert!(amounts.len() > 5, "contacts: {amounts:?}");
    for value in &amounts[..5] { assert!((value - amounts[0]).abs() < 1e-8, "contacts: {contacts:?}"); }
    assert!(amounts[5] < amounts[0], "the second burst cannot reuse unspent initial combo: {amounts:?}");
}

#[test]
fn a_converted_heavy_uses_its_own_contact_reach() {
    let mut p = fixture("magistar", 10.0);
    let mut heavy = fixture("magistar_heavy", 10.0);
    heavy.range_m = 30.0;
    heavy.follow_through = Some(1.0);
    p.tennokai.enabled = true;
    p.tennokai.every_n_hits = 1;
    p.tennokai.window_seconds = 10.0;
    heavy.tennokai = p.tennokai;
    p.heavy_script = heavy.combo_script.clone();
    p.melee_heavy = Some(Box::new(heavy));
    p.others.push(crate::formation::FoeSpec {
        id: "far".into(), params: Foe::training_dummy(), body_parts: BodyPart::humanoid(),
        at: crate::rules::space::Vec2::new(0.0, 25.0),
    });
    p.melee_heavy.as_mut().unwrap().others = p.others.clone();
    let reached = run_once(&p, &mut Rng::new(7)).taken.touched();
    p.melee_heavy.as_mut().unwrap().range_m = p.range_m;
    let control = run_once(&p, &mut Rng::new(7)).taken.touched();
    assert_eq!(reached, 2);
    assert_eq!(control, 1);
}
