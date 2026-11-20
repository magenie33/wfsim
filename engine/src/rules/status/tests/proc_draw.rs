// SPDX-License-Identifier: AGPL-3.0-or-later

use super::super::ordinary;
use super::*;
use crate::rules::{damage::DamageType, rng::Rng};

#[test]
fn grouped_choices_and_rng_state_match_the_one_hit_sampler() {
    let mut dense = DamageVector::new();
    for (i, kind) in DamageType::ALL.into_iter().enumerate() {
        dense.set(kind, 3.0_f64.powi(i as i32 - 9));
    }
    let exceptional = DamageVector::new()
        .with(DamageType::Impact, -1.0)
        .with(DamageType::Slash, -0.0)
        .with(DamageType::Heat, f64::NAN)
        .with(DamageType::Cold, f64::from(f32::MAX))
        .with(DamageType::Toxin, f64::from(f32::MAX))
        .with(DamageType::Void, f64::NAN)
        .with(DamageType::Tau, f64::INFINITY);
    let all = SceneProcHandlers::all();
    let none = SceneProcHandlers {
        ordinary_mask: 0,
        void: false,
        tau: false,
    };
    let mask_sequence = [
        all,
        all,
        SceneProcHandlers {
            ordinary_mask: all.ordinary_mask | (1 << 25),
            ..all
        },
        none,
        SceneProcHandlers { tau: true, ..none },
        SceneProcHandlers { void: true, ..none },
        SceneProcHandlers {
            ordinary_mask: 1 << 4,
            ..none
        },
        all,
    ];
    for vector in [dense, exceptional, DamageVector::new()] {
        let mut group = SceneProcDraw::new(&vector);
        let mut rng = Rng::new(7);
        let mut reference_rng = rng;
        for i in 0..4_096 {
            let handlers = mask_sequence[i % mask_sequence.len()];
            let chances = [f32::NAN, -1.0, -0.0, 0.01, 0.5, 1.0, 1.4, 4.25];
            let chance = chances[(i / mask_sequence.len()) % chances.len()];
            let mut expected = Vec::new();
            let mut actual = Vec::new();
            ordinary::sample(
                chance,
                0,
                || scene_weights(&vector, handlers),
                || reference_rng.unit(),
                |choice| expected.push(choice),
            );
            group.sample(
                chance,
                handlers,
                || rng.unit(),
                |choice| actual.push(choice),
            );
            assert_eq!(actual, expected, "iteration {i}");
            assert_eq!(rng.state(), reference_rng.state(), "iteration {i}");
        }
    }
}

#[test]
fn grouped_hit_procs_keep_forced_order_and_per_hit_saturation() {
    let vector = DamageVector::new()
        .with(DamageType::Heat, 1.0)
        .with(DamageType::Void, 1.0)
        .with(DamageType::Tau, 1.0);
    let all = SceneProcHandlers::all();
    let heat = SceneProcHandlers {
        void: false,
        tau: false,
        ..all
    };
    let void = SceneProcHandlers {
        ordinary_mask: 0,
        void: true,
        tau: false,
    };
    let tau = SceneProcHandlers {
        ordinary_mask: 0,
        void: false,
        tau: true,
    };
    let mut forced = vec![DamageType::True, DamageType::Gas];
    forced.extend(std::iter::repeat_n(DamageType::Heat, 254));
    forced.extend(std::iter::repeat_n(DamageType::Void, 255));
    forced.extend(std::iter::repeat_n(DamageType::Tau, 256));
    let mut group = SceneProcDraw::new(&vector);
    let mut rng = Rng::new(23);
    let mut reference_rng = rng;

    for (handlers, extra) in [
        (heat, Some(DamageType::Heat)),
        (heat, Some(DamageType::Heat)),
        (void, None),
        (tau, None),
    ] {
        let actual = group.procs_for_hit(&forced, 2.0, handlers, &mut rng);
        let expected =
            super::super::procs_for_hit(&forced, 2.0, &vector, handlers, &mut reference_rng);
        assert_eq!(actual, expected);
        assert_eq!(&actual[..forced.len()], &forced);
        assert_eq!(&actual[forced.len()..], extra.as_slice());
        assert_eq!(rng.state(), reference_rng.state());
    }
}

#[test]
fn chance_failure_leaves_the_cdf_lazy_and_boundaries_stay_inclusive() {
    let vector = DamageVector::new()
        .with(DamageType::Impact, 1.0)
        .with(DamageType::Heat, 1.0);
    let handlers = SceneProcHandlers::all();
    let mut group = SceneProcDraw::new(&vector);
    group.sample(0.0, handlers, || panic!("no roll"), |_| panic!("no choice"));
    group.sample(0.25, handlers, || 0.5, |_| panic!("failed chance"));
    assert!(group.key.is_none());

    let mut draws = [0.5, 0.5, 0.0, f32::from_bits(0.5_f32.to_bits() + 1)].into_iter();
    let mut choices = Vec::new();
    group.sample(
        0.5,
        handlers,
        || draws.next().unwrap(),
        |choice| choices.push(choice),
    );
    assert_eq!(choices, [Some(0)]);
    let key = group.key;
    group.sample(
        0.25,
        SceneProcHandlers {
            void: false,
            ..handlers
        },
        || 0.5,
        |_| panic!("failed chance"),
    );
    assert_eq!(group.key, key);
    group.sample(
        1.0,
        handlers,
        || draws.next().unwrap(),
        |choice| choices.push(choice),
    );
    assert_eq!(choices, [Some(0), Some(3)]);
    assert_eq!(draws.next(), None);
}

#[test]
fn overflowed_total_preserves_multiply_and_first_infinite_prefix() {
    let vector = DamageVector::new()
        .with(DamageType::Impact, f64::from(f32::MAX))
        .with(DamageType::Heat, f64::from(f32::MAX))
        .with(DamageType::Tau, f64::INFINITY);
    let mut group = SceneProcDraw::new(&vector);
    let mut draws = [0.0, 0.0, 0.0, 0.5].into_iter();
    let mut choices = Vec::new();
    group.sample(
        2.0,
        SceneProcHandlers::all(),
        || draws.next().unwrap(),
        |choice| choices.push(choice),
    );
    assert_eq!(choices, [None, Some(3)]);
    assert_eq!(draws.next(), None);
}
