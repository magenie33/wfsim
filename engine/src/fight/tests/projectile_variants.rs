use super::*;
use crate::model::{StackPolicy, WeaponBase};
use crate::record::{Kind, Origin};
use crate::rules::damage::quantize_base_crit_damage;

#[test]
fn additional_projectiles_take_the_selected_resource_inputs() {
    let cases = [
        (
            "latron_prime_incarnon",
            "",
            [50.0, 140.0, 0.44, 3.4, 0.30],
            [50.0, 80.0, 0.32, 3.0, 0.24],
        ),
        (
            "grattler",
            "archwing",
            [225.0, 310.0, 0.25, 2.0, 0.25],
            [100.0, 205.0, 0.25, 2.0, 0.25],
        ),
        (
            "kuva_grattler",
            "archwing",
            [50.0, 155.0, 0.27, 2.1, 0.27],
            [100.0, 205.0, 0.25, 2.0, 0.25],
        ),
        (
            "kuva_grattler",
            "",
            [140.0, 235.0, 0.27, 2.0, 0.27],
            [140.0, 235.0, 0.25, 2.0, 0.25],
        ),
        (
            "tenet_detron",
            "",
            [26.0, 0.0, 0.18, 2.0, 0.10],
            [40.0, 0.0, 0.04, 1.5, 0.09],
        ),
    ];
    for (weapon, deployment, ordinary, additional) in cases {
        let mut base = WeaponBase::from_data(weapon, false, &[]);
        crate::data::weapons::apply_deployment(&mut base, weapon, deployment);
        let extra = base.additional_projectile.as_deref().unwrap();
        for (projectile, [direct, radial, cc, cd, sc]) in [(&base, ordinary), (extra, additional)] {
            assert_eq!(
                projectile.base_vector.total(),
                direct,
                "{weapon}/{deployment}"
            );
            assert_eq!(projectile.base_crit_chance, cc, "{weapon}/{deployment}");
            assert_eq!(projectile.base_crit_damage, cd, "{weapon}/{deployment}");
            assert_eq!(projectile.base_status_chance, sc, "{weapon}/{deployment}");
            if let Some(blast) = &projectile.radial {
                assert_eq!(blast.base_vector.total(), radial, "{weapon}/{deployment}");
                assert_eq!(blast.base_crit_chance, cc, "{weapon}/{deployment}");
                assert_eq!(blast.base_crit_damage, cd, "{weapon}/{deployment}");
                assert_eq!(blast.base_status_chance, sc, "{weapon}/{deployment}");
            } else {
                assert_eq!(radial, 0.0);
            }
        }
    }

    for unchanged in [
        "latron_incarnon",
        "latron_wraith_incarnon",
        "grattler",
        "tenet_detron_mag_burst",
    ] {
        assert!(
            WeaponBase::from_data(unchanged, false, &[])
                .additional_projectile
                .is_none(),
            "{unchanged}"
        );
    }
}

#[test]
fn latron_additional_explosion_receives_the_same_flat_evolution() {
    let id = "latron_prime_incarnon";
    let evos = ["latron_prime_riddled_target"];
    let base = WeaponBase::from_data(id, false, &evos);
    let panel = crate::build::loadout::resolve(&base, &[], StackPolicy::Emergent);
    let mut p = FightParams::from_panel(
        &panel,
        &crate::arena::Arena::training(0.1),
        &ArcaneFx::none(),
    );
    p.body_parts = mono_body(1.0);
    p.multishot = 2.0;
    p.target_at.y = 0.5;
    let rec = record(&p, 0x2301, 0.0, f64::INFINITY, 100_000, 0);
    let hits: Vec<_> = rec
        .events()
        .iter()
        .filter_map(|event| match &event.kind {
            Kind::Damage(row) => row
                .pellet
                .map(|pellet| (pellet, row.radial, row.base, row.crit_damage)),
            _ => None,
        })
        .collect();
    assert_eq!(hits.len(), 4, "{hits:?}");
    for ((pellet, radial, base, cd), (want_pellet, want_radial, want_base, want_cd)) in
        hits.into_iter().zip([
            (1, false, 56.0, 3.4),
            (1, true, 146.0, 3.4),
            (2, false, 56.0, 3.0),
            (2, true, 86.0, 3.0),
        ])
    {
        assert_eq!((pellet, radial), (want_pellet, want_radial));
        assert!((base - want_base).abs() < 1e-9, "{base} vs {want_base}");
        assert!(
            (cd - quantize_base_crit_damage(want_cd)).abs() < 1e-9,
            "{cd} vs {want_cd}"
        );
    }
}

#[test]
fn extra_detron_rounds_settle_after_all_ten_ordinary_rounds() {
    let id = "tenet_detron";
    let mut base = WeaponBase::from_data(id, false, &[]);
    crate::data::weapons::apply_valence(&mut base, id, "heat", 0.5);
    let panel = crate::build::loadout::resolve(&base, &[], StackPolicy::Emergent);
    let mut p = FightParams::from_panel(
        &panel,
        &crate::arena::Arena::training(0.1),
        &ArcaneFx::none(),
    );
    p.body_parts = mono_body(1.0);
    p.multishot = 20.0;
    p.target_at.y = 0.5;
    let rec = record(&p, 0x2302, 0.0, f64::INFINITY, 100_000, 0);
    let hits: Vec<_> = rec
        .events()
        .iter()
        .filter_map(|event| match &event.kind {
            Kind::Damage(row) => row
                .pellet
                .map(|pellet| (pellet, row.origin, row.base, row.crit_damage)),
            _ => None,
        })
        .collect();
    assert_eq!(hits.len(), 20, "{hits:?}");
    for (index, (pellet, origin, damage, cd)) in hits.into_iter().enumerate() {
        let (expected_origin, expected_base, expected_cd) = if index < 10 {
            (Origin::Own, 39.0, 2.0)
        } else {
            (Origin::Multishot, 60.0, 1.5)
        };
        assert_eq!(pellet, index as u32 + 1);
        assert_eq!(origin, expected_origin);
        assert!((damage - expected_base).abs() < 1e-9);
        assert_eq!(cd, quantize_base_crit_damage(expected_cd));
    }
}
