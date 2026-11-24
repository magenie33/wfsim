// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::rules::geometry::{eye::eye_aim, glaive};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../fixtures/native-queries.json")).unwrap()
}

fn f32s<const N: usize>(v: &Value) -> [f32; N] {
    std::array::from_fn(|i| v[i].as_f64().unwrap() as f32)
}

fn same<const N: usize>(actual: [f32; N], expected: [f32; N], label: impl std::fmt::Debug) {
    assert_eq!(
        actual.map(f32::to_bits),
        expected.map(f32::to_bits),
        "{label:?}"
    );
}

#[test]
fn native_eye_projection_keeps_fire_origin_and_aim_basis_separate() {
    let fixture = fixture();
    let cases = fixture["eye"].as_array().unwrap();
    assert_eq!(cases.len(), 76);
    for (i, case) in cases.iter().enumerate() {
        let camera: [f32; 16] = f32s(&case["camera_world_column_major"]);
        let eye = eye_aim(
            camera[12..15].try_into().unwrap(),
            camera[8..11].try_into().unwrap(),
            f32s(&case["target_input"]),
            f32s(&case["Avatar_anchor_4b0_input"]),
            [0., 0., 1.],
            [0., 1., 0., 0.],
        );
        same(eye.target, f32s(&case["native_final_aim_target"]), i);
        same(eye.basis, f32s(&case["native_final_basis_column_major"]), i);
    }
}

#[test]
fn glaive_descriptors_and_record_consumption_replay_original_calls() {
    let fixture = fixture();
    let cases = fixture["motion"].as_array().unwrap();
    assert_eq!(cases.len(), 68);
    for (i, case) in cases.iter().enumerate() {
        let input = &case["input"];
        let start = f32s(&input["start"]);
        let delta = f32s(&input["delta"]);
        let ends = glaive::query_ends(
            start,
            delta,
            f32s(&input["world_quaternion"]),
            input["radius"].as_f64().unwrap() as f32,
            input["continuation"].as_bool().unwrap(),
        )
        .collect::<Vec<_>>();
        let queries = case["queries"].as_array().unwrap();
        let mut result = None;
        let mut count = 0;
        for (ray, end) in ends.into_iter().enumerate() {
            let query = &queries[ray];
            count += 1;
            same(start, f32s(&query["start"]), (i, ray, "origin"));
            same(end, f32s(&query["end"]), (i, ray, "end"));
            result = glaive::impact_record(
                delta,
                query["backend_success"].as_bool().unwrap(),
                query["supplied_records"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|record| {
                        (
                            f32s::<3>(&record["point"]),
                            record["acceptance"].as_bool().unwrap_or(true),
                            f32s(&record["normal"]),
                        )
                    }),
            );
            if result.is_some() {
                break;
            }
        }
        assert_eq!(count, queries.len(), "{i}");
        assert_eq!(
            result,
            (!case["impact"].is_null()).then(|| f32s(&case["impact"])),
            "{i}"
        );
    }
}

#[test]
fn descriptors_follow_the_original_flight_up_to_first_impact() {
    let data = fixture();
    let cases = data["flight"].as_array().unwrap();
    assert_eq!(cases.len(), 38);
    let mut count = 0;
    for (i, case) in cases.iter().enumerate() {
        let actors = case["actors"].as_array().unwrap();
        assert!(matches!(actors.len(), 1 | 25));
        for step in case["steps"].as_array().unwrap() {
            let queries = step["queries"].as_array().unwrap();
            let ends = glaive::query_ends(
                f32s(&step["start"]),
                f32s(&step["motion_displacement"]),
                f32s(&step["orientation"]),
                0.02,
                false,
            );
            assert!(!queries.is_empty());
            for (q, (end, actual)) in ends.zip(queries).enumerate() {
                same(end, f32s(&actual["end"]), (i, &step["step"], q));
                count += 1;
            }
        }
    }
    assert_eq!(count, 3099);
}

#[test]
fn homing_uses_root_range_anchor_score_and_first_equal_candidate() {
    let data = fixture();
    let cases = data["selection"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    for (i, case) in cases.iter().enumerate() {
        let actors = case["actors"].as_array().unwrap();
        let order = case["candidates"][0]["order"].as_array().unwrap();
        let queries = case["queries"].as_array().unwrap();
        let mut observed = Vec::new();
        let winner = glaive::select_candidate(
            order.iter().filter_map(|index| {
                let index = index.as_u64().unwrap() as usize;
                let actor = &actors[index];
                if actor["health"] == 0
                    || (actor["faction"].is_number()
                        && actor["faction"] == case["options"]["owner_faction"])
                {
                    return None;
                }
                let root = f32s(&actor["root"]);
                let anchor = if case["options"]["native_anchor"] == true {
                    f32s(&queries[0]["to"])
                } else if actor["anchor"].is_null() {
                    root
                } else {
                    f32s(&actor["anchor"])
                };
                glaive::candidate_score([0.; 3], [0., 0., 1.], root, anchor, 30., 0.9848077)
                    .map(|score| (index, score))
            }),
            |index| {
                observed.push(index);
                let hit = actors[index]["visibility_hit"]
                    .as_u64()
                    .map(|id| id as usize);
                hit.is_none_or(|hit| hit == index)
            },
        );
        assert_eq!(
            winner,
            case["selected"].as_u64().map(|x| x as usize),
            "case {i}"
        );
        assert!(
            observed.iter().copied().eq(queries
                .iter()
                .map(|q| q["candidate"].as_u64().unwrap() as usize)),
            "visibility calls in case {i}: {observed:?}"
        );
    }
}
