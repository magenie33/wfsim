// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use serde_json::Value;
mod rotation;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../fixtures/native.json")).unwrap()
}

fn vector(value: &Value) -> Vec3 {
    Vec3(std::array::from_fn(|i| value[i].as_f64().unwrap()))
}

fn affine(value: &Value) -> Affine {
    Affine(std::array::from_fn(|i| value[i].as_f64().unwrap()))
}

fn shape(value: &Value) -> Shape {
    let radius = value["radius"].as_f64().unwrap();
    match value["type"].as_u64().unwrap() {
        1 => Shape::Capsule {
            radius,
            height: value["capsule_height"].as_f64().unwrap(),
        },
        2 => Shape::Sphere { radius },
        other => panic!("unbound shape {other}"),
    }
}

#[test]
fn the_618_native_ray_cases_agree_with_the_portable_query() {
    let input = fixture();
    let cases = input["ray_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 618);
    let mut contacts = 0;
    for (i, case) in cases.iter().enumerate() {
        let mut proxies = Vec::new();
        let gate = match case["gate"].as_str() {
            Some("inactive-avatar") => Gate::Inactive,
            Some("missing-animation-controller") => Gate::MissingAnimation,
            Some("empty-body-list") => Gate::EmptyBodyList,
            _ => Gate::Active,
        };
        if case["kind"] == "strict-ray-boundary" {
            proxies.push(Proxy {
                cache_index: 0,
                bone_token: 77,
                part: 1,
                shape: shape(&case["primitive"]),
                world: affine(&case["world_rows"]),
                ray_enabled: true,
            });
        } else if gate == Gate::Active {
            let fixture = &input["fixtures"][case["fixture_id"].as_u64().unwrap() as usize];
            let world = &fixture["world_column_major"];
            let world = Affine(std::array::from_fn(|j| {
                world[(j % 4) * 4 + j / 4].as_f64().unwrap()
            }));
            let enemy = input["enemies"]
                .as_array()
                .unwrap()
                .iter()
                .find(|e| e["path"] == case["enemy"])
                .unwrap();
            for source in enemy["proxies"].as_array().unwrap() {
                let index = source["cache_index"].as_u64().unwrap() as usize;
                proxies.push(Proxy {
                    cache_index: index,
                    bone_token: source["bone_token"].as_u64().unwrap_or(u64::from(u32::MAX)) as u32,
                    part: source["part"].as_u64().unwrap() as u8,
                    shape: shape(&source["shape"]),
                    world: world.compose(affine(&fixture["cache_local_rows"][index])),
                    ray_enabled: !case["disabled"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|v| v.as_u64() == Some(index as u64))),
                });
            }
        }
        let ray = Ray {
            origin: if gate == Gate::Active {
                Vec3(vector(&case["origin"]).0.map(|v| f64::from(v as f32)))
            } else {
                Vec3([0.; 3])
            },
            direction: if gate == Gate::Active {
                vector(&case["direction"])
            } else {
                Vec3([0., 0., 1.])
            },
            maximum: case["max_distance"].as_f64().unwrap_or(16.),
            previous_best: case["previous_best"].as_f64().unwrap_or(f64::INFINITY),
            excluded_parts: case["part_mask"].as_u64().unwrap_or(0) as u32,
        };
        let result = refine(0x1234, &proxies, ray, gate);
        if gate != Gate::Active {
            assert!(
                matches!(result, Refinement::Bypassed { accepted } if accepted == case["native_return"].as_bool().unwrap()),
                "case {i}"
            );
        } else if case["kind"] == "strict-ray-boundary" {
            assert_eq!(
                matches!(result, Refinement::Contact(_)),
                case["hit"].as_bool().unwrap(),
                "case {i}"
            );
        } else {
            let expected = &case["native"];
            assert_eq!(
                matches!(result, Refinement::Contact(_)),
                expected["hit"].as_bool().unwrap(),
                "case {i}: {result:?}"
            );
            if let Refinement::Contact(hit) = result {
                contacts += 1;
                assert_eq!(hit.owner, 0x1234);
                let indices = expected["portable_nearest_within_tolerance"]
                    .as_array()
                    .unwrap();
                assert!(
                    indices
                        .iter()
                        .any(|v| v.as_u64() == Some(hit.cache_index as u64)),
                    "case {i}: {hit:?}"
                );
                let difference = hit.point.minus(vector(&expected["point"]));
                assert!(
                    difference.0.iter().all(|v| v.abs() < 0.0003),
                    "case {i}: {hit:?}, {expected}"
                );
                if hit.cache_index == expected["proxy_index"].as_u64().unwrap() as usize {
                    assert_eq!(
                        u64::from(hit.bone_token),
                        expected["bone_token"].as_u64().unwrap(),
                        "case {i}"
                    );
                }
            }
        }
    }
    assert!(contacts > 500);
}

#[test]
fn the_121_native_penetration_fragments_match_float32_bits() {
    use penetration::*;
    let input = fixture();
    let cases = input["penetration_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 121);
    for (i, case) in cases.iter().enumerate() {
        let reference = case["candidate_reference_value"].as_u64().unwrap();
        let reverse: Vec<_> = case["reverse_hits"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(j, v)| ReverseHit {
                point: vector(v).0.map(|n| n as f32),
                reference: case["exit_reference_values"][j]
                    .as_u64()
                    .unwrap_or(reference),
            })
            .collect();
        let result = traverse(
            vector(&case["entry"]).0.map(|n| n as f32),
            reference,
            &reverse,
            Budget {
                maximum: case["budget"].as_f64().unwrap() as f32,
                accumulated: case["prior_accumulated"].as_f64().unwrap() as f32,
                infinite: case["infinite_flag"].as_bool().unwrap(),
            },
        );
        assert_eq!(
            result.matching_index.map(|n| n as u64),
            case["matching_reverse_index"].as_u64(),
            "case {i}"
        );
        assert_eq!(
            result.continues,
            case["native_continue"].as_bool().unwrap(),
            "case {i}"
        );
        assert_eq!(
            result.accumulated.to_bits(),
            (case["native_stored_accumulated"].as_f64().unwrap() as f32).to_bits(),
            "case {i}"
        );
    }
}

#[test]
fn an_invalid_transform_or_ray_has_no_contact() {
    let proxy = Proxy {
        cache_index: 5,
        bone_token: 91,
        part: 1,
        shape: Shape::Sphere { radius: 1. },
        world: Affine([0.; 12]),
        ray_enabled: true,
    };
    let ray = Ray {
        origin: Vec3([0., 0., 2.]),
        direction: Vec3([0., 0., -1.]),
        maximum: 10.,
        previous_best: 10.,
        excluded_parts: 0,
    };
    assert!(matches!(
        refine(9, &[proxy], ray, Gate::Active),
        Refinement::Miss
    ));
    assert!(Shape::Sphere { radius: 1. }
        .intersect(Vec3([0.; 3]), Vec3([0.; 3]))
        .is_none());
    assert!(Shape::Sphere { radius: 1. }
        .intersect(Vec3([f64::NAN, 0., 0.]), ray.direction)
        .is_none());
}

#[test]
fn guided_motion_matches_the_36_native_empty_world_sequences() {
    let data: Value = serde_json::from_str(include_str!("../fixtures/native-motion.json")).unwrap();
    let cases=data["motion_cases"].as_array().unwrap();assert_eq!(cases.len(),36);
    for (i,case) in cases.iter().enumerate() {
        let origin=vector(&case["muzzle"]);
        let direction=vector(&case["initial_aim_point"]).minus(origin).normalized().unwrap();
        let mut state=guided::Guided::new(origin.0.map(|v|v as f32),direction.0.map(|v|v as f32),40.);
        for (j,step) in case["trace"].as_array().unwrap().iter().enumerate() {
            let acquired=case["target_mode"]=="preselected"||(case["target_mode"]=="acquire-after-launch"&&state.flight>0.);
            let target=acquired.then(||vector(&case["steering_center"]).0.map(|v|v as f32));
            state.step(target,1./case["hz"].as_f64().unwrap() as f32,0.08,40.,80.);
            for k in 0..3 {
                let expected=step["new"][k].as_f64().unwrap();
                assert!((f64::from(state.position[k])-expected).abs()<0.0003,"case {i} step {j} axis{k}: {} != {expected}",state.position[k]);
            }
        }
    }
}
