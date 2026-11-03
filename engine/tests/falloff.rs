// SPDX-License-Identifier: AGPL-3.0-or-later
use wfsim_engine::model::{Falloff, FalloffPoint, FalloffSpec};

fn spec() -> FalloffSpec {
    FalloffSpec {
        start_m: 11.0,
        end_m: 17.0,
        reduction: 69.0 / 70.0,
        points: vec![
            FalloffPoint { distance_m: 11.0, damage_multiplier: 0.5 },
            FalloffPoint { distance_m: 15.0, damage_multiplier: 0.5 },
        ],
    }
}

#[test]
fn falloff_points_preserve_steps_plateaus_and_final_slopes() {
    let curve = Falloff::from_spec(&spec(), 1.0);
    for (distance, want) in [(0.0, 1.0), (11.0, 1.0), (11.01, 0.5), (15.0, 0.5),
        (16.0, (0.5 + 1.0 / 70.0) / 2.0), (17.0, 1.0 / 70.0), (18.0, 1.0 / 70.0)] {
        assert!((curve.factor(distance) - want).abs() < 1e-12, "{distance}");
    }
}

#[test]
fn projectile_speed_scales_every_falloff_point() {
    let plain = Falloff::from_spec(&spec(), 1.0);
    let faster = Falloff::from_spec(&spec(), 2.0);
    for distance in [0.0, 11.0, 11.01, 15.0, 16.0, 17.0, 20.0] {
        assert_eq!(plain.factor(distance), faster.factor(distance * 2.0));
    }
}

#[test]
fn ordinary_falloff_keeps_its_linear_rule() {
    let source = FalloffSpec { start_m: 10.0, end_m: 20.0, reduction: 0.8, points: Vec::new() };
    let falloff = Falloff::from_spec(&source, 1.0);
    assert_eq!(falloff.factor(9.0), 1.0);
    assert!((falloff.factor(15.0) - 0.6).abs() < 1e-12);
    assert!((falloff.factor(20.0) - 0.2).abs() < 1e-12);
}

#[test]
#[should_panic(expected = "falloff points must descend")]
fn falloff_rejects_points_outside_the_declared_window() {
    let mut source = spec();
    source.points[0].distance_m = 10.0;
    Falloff::from_spec(&source, 1.0);
}
