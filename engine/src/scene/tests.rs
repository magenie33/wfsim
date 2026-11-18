// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
#[path = "../../../tests/fixtures/geometry/scene.rs"]
mod fixture;

fn scene(enemy: usize, rows: usize, contact: bool) -> Scene {
    Scene::new(
        serde_json::from_value(fixture::scene(enemy, rows, contact, false, false)).unwrap(),
        rows * rows,
    )
    .unwrap()
}

#[test]
fn player_and_enemy_models_define_contact_and_mounts() {
    assert_eq!(
        fixture::formation(&fixture::scene(0, 5, true, false, false))
            .as_array()
            .unwrap()
            .len(),
        24
    );
    let s = scene(0, 5, true);
    for (index,expected) in [(0,3),(1,1)] {
        let supplied=scene(index,1,false);
        assert_eq!(supplied.input.enemies[0].proxies.iter().filter(|p|matches!(p.shape,crate::rules::geometry::Shape::Sphere{..})).count(),expected);
    }
    assert!((s.root(0).0[2] - 0.87).abs() < 1e-6);
    let h = scene(1, 5, true);
    assert!((h.root(0).0[2] - 0.7825).abs() < 1e-6);
    assert_eq!(s.muzzle, Vec3([0., 1.5, 0.3]));
    let mut input = s.input.clone();
    input.player.bones[48].0[7] += 0.6;
    let taller = Scene::new(input, 25).unwrap();
    assert!((taller.muzzle.0[1] - s.muzzle.0[1] - 0.6).abs() < 1e-12);
    let mut input = s.input.clone();
    input.player.controller.height = 0.1;
    input.player.controller.radius = 0.01;
    assert!(Scene::new(input, 25)
        .unwrap_err()
        .contains("inner segments"));
}

#[test]
fn perfect_head_intent_does_not_force_a_head_contact() {
    let s = scene(1, 1, false);
    let path = s.trace(true, 0., 0., Some(100.), None, 0., false, 0., 100., &[true]);
    assert_eq!(path.impacts.len(), 1);
    assert_ne!(
        path.impacts[0].part, 1,
        "reticle direction must not teleport the muzzle onto its ray"
    );
    let mut input = s.input.clone();
    // Put the supplied muzzle on the camera ray: an actual head contact.
    let direction = path.direction;
    let muzzle = s.camera.along(direction, 3.);
    input.player.bones[48].0[3] = muzzle.0[0];
    input.player.bones[48].0[7] = muzzle.0[1];
    input.player.bones[48].0[11] = muzzle.0[2];
    let aligned = Scene::new(input, 1).unwrap();
    let path = aligned.trace(true, 0., 0., Some(100.), None, 0., false, 0., 100., &[true]);
    assert_eq!(path.impacts[0].part, 1);
}

#[test]
fn each_round_recomputes_the_25_body_path_and_material_budget() {
    let s = scene(1, 5, false);
    let line = s.trace(
        false,
        0.,
        0.,
        Some(100.),
        None,
        100.,
        false,
        0.,
        100.,
        &[true; 25],
    );
    assert!(line.impacts.len() >= 3, "{line:?}");
    let wide = s.trace(
        false,
        12.,
        0.,
        Some(100.),
        None,
        100.,
        false,
        0.,
        100.,
        &[true; 25],
    );
    assert_ne!(
        line.impacts.iter().map(|h| h.body).collect::<Vec<_>>(),
        wide.impacts.iter().map(|h| h.body).collect::<Vec<_>>()
    );
    let stopped = s.trace(
        false,
        0.,
        0.,
        Some(100.),
        None,
        0.,
        false,
        0.,
        100.,
        &[true; 25],
    );
    assert_eq!(stopped.impacts.len(), 1);
    assert!(stopped.impacts[0].material > 0.1);
    assert_eq!(stopped.impacts[0].remaining, 0.);
    assert!(line.impacts.windows(2).all(|w| w[0].seconds < w[1].seconds));
}

#[test]
fn unsupported_transforms_and_missing_models_are_errors() {
    let s = scene(1, 1, false);
    let mut input = s.input.clone();
    input.player.world.0[0] = 2.;
    assert!(Scene::new(input, 1).is_err());
    assert!(Scene::new(s.input, 25).is_err());
}
