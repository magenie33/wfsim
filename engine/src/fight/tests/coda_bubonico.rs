use super::*;
use crate::record::{Kind, Origin};

fn weapon(id: &str, seconds: f64) -> FightParams {
    let base = crate::model::WeaponBase::from_data(id, false, &[]);
    let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let mut p = FightParams::from_panel(
        &panel,
        &crate::arena::Arena::training(seconds),
        &ArcaneFx::none(),
    );
    p.foe.base_health = 1e15;
    p.body_parts = mono_body(1.0);
    p
}

fn traced(p: &FightParams) -> crate::record::Record {
    record(p, 7, 0.0, p.duration_seconds, 50_000, 0)
}

#[test]
fn coda_bubonico_uses_native_burst_spacing_and_refire_waits() {
    let p = weapon("coda_bubonico_burst", 2.0);
    let rec = traced(&p);
    let times: Vec<_> = rec
        .events()
        .iter()
        .filter_map(|e| matches!(e.kind, Kind::Strike { .. }).then_some(e.t))
        .collect();
    assert!(times.len() >= 9);
    for pull in 0..3 {
        let start = pull as f64 * (0.23 * 2.0 + 1.0 / 7.0);
        for round in 0..3 {
            assert!(
                (times[3 * pull + round] - start - round as f64 * 0.23).abs() < 1e-6,
                "native three-round timing, pull {pull}, round {round}: {times:?}"
            );
        }
    }
}

#[test]
fn a_fixed_initial_and_maximum_pellet_count_has_no_spool_admission() {
    let f: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/client/coda-bubonico-scope-2026-10-28.json"
    ))
    .unwrap();
    let state = &f["primary_state"];
    assert_eq!(state["InitialFireIterations"], "7");
    assert_eq!(state["MaximumFireIterations"], "7");
    let p = weapon("coda_bubonico", 2.0);
    let rec = traced(&p);
    let pellets: Vec<_> = rec
        .events()
        .iter()
        .filter_map(|e| match e.kind {
            Kind::Strike { pellets, .. } => Some(pellets),
            _ => None,
        })
        .collect();
    assert!(!pellets.is_empty());
    assert!(pellets.iter().all(|n| *n == 7));
    let spec = crate::data::weapons::spec("coda_bubonico").unwrap();
    assert!(!spec
        .unmodeled_parts
        .iter()
        .any(|p| p.reason.as_deref() == Some("client_spool_pellets")));
}

#[test]
fn coda_radial_adds_duration_before_modifiers_on_every_body() {
    for id in ["bubonico_burst", "coda_bubonico_burst"] {
        for multiplier in [1.0, 2.0] {
            for (kind, name, span) in [
                ("viral", "virus", 15.0),
                ("heat", "ignite", 15.0),
                ("cold", "freeze", 15.0),
                ("blast", "blast", 10.5),
            ] {
                let mut p = weapon(id, 2.0);
                p.status_duration_multiplier = multiplier;
                p.status_chance = 0.0;
                p.forced_procs = vec![DamageType::Impact];
                let radial = p.radial.as_mut().unwrap();
                assert_eq!(
                    radial.status_duration_extra_seconds, 9.0,
                    "{id}: parsed ProcExtraTime"
                );
                radial.status_chance = 0.0;
                radial.forced_procs = crate::rules::damage::ForcedProcs::from_names([kind]);
                let mut foe = Foe::training_dummy();
                foe.base_health = 1e15;
                p.others = vec![crate::formation::FoeSpec {
                    id: "nearby".into(),
                    params: foe,
                    body_parts: mono_body(1.0),
                    at: crate::rules::space::Vec2::new(p.target_at.x, p.target_at.y + 1.0),
                }];
                let rec = traced(&p);
                let virus = DEBUFF_ROSTER
                    .iter()
                    .position(|(id, _)| *id == name)
                    .unwrap();
                let impact = DEBUFF_ROSTER
                    .iter()
                    .position(|(name, _)| *name == "stagger")
                    .unwrap();
                for subject in [0, 1] {
                    let expiry = rec
                        .events()
                        .iter()
                        .filter_map(|e| match &e.kind {
                            Kind::Damage(d)
                                if e.subject == Some(subject) && d.debuffs[virus].0 > 0 =>
                            {
                                Some(d.debuffs[virus].1)
                            }
                            _ => None,
                        })
                        .fold(f64::INFINITY, f64::min);
                    assert!(
                        (expiry - span * multiplier).abs() < 1e-6,
                        "{id}/{kind}: body {subject}, {span} * {multiplier} = {expiry}"
                    );
                }
                let impact_expiry = rec
                    .events()
                    .iter()
                    .filter_map(|e| match &e.kind {
                        Kind::Damage(d) if e.subject == Some(0) && d.debuffs[impact].0 > 0 => {
                            Some(d.debuffs[impact].1)
                        }
                        _ => None,
                    })
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    (impact_expiry - 6.0 * multiplier).abs() < 1e-6,
                    "the direct packet has no extra duration: {impact_expiry}"
                );
            }
        }
    }
}

#[test]
fn coda_radial_extra_duration_extends_each_dot_without_increasing_tick_damage() {
    for ty in [
        DamageType::Slash,
        DamageType::Toxin,
        DamageType::Electricity,
        DamageType::Gas,
    ] {
        for (multiplier, ticks) in [(1.0, 15), (2.0, 30)] {
            let mut p = weapon("coda_bubonico_burst", 35.0);
            p.status_duration_multiplier = multiplier;
            p.battery = None;
            p.magazine_size = 3.0;
            p.reserve_ammo = 0.0;
            p.infinite_reserve = false;
            p.status_chance = 0.0;
            p.base_crit_chance = 0.0;
            p.crit_multiplier = 1.0;
            let radial = p.radial.as_mut().unwrap();
            radial.status_chance = 0.0;
            radial.forced_procs = crate::rules::damage::ForcedProcs::from_names([ty.name()]);
            radial.crit_chance = 0.0;
            radial.crit_damage = 1.0;
            let rec = traced(&p);
            let dots: Vec<_> = rec
                .events()
                .iter()
                .filter_map(|e| match &e.kind {
                    Kind::Damage(d) if d.origin == Origin::Status && d.dtype == ty => {
                        Some((e.t, d.raw))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(dots.len(), ticks, "{ty:?} at {multiplier}: {dots:?}");
            assert!(
                dots.iter()
                    .all(|(_, damage)| (damage - dots[0].1).abs() < 1e-6),
                "duration changes tick count, not payload"
            );
        }
    }
}
