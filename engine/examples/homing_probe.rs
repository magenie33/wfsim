// SPDX-License-Identifier: AGPL-3.0-or-later
//! Query supplied posed bodies along recorded native empty-world motion segments.
use serde_json::{json, Value};
use wfsim_engine::rules::geometry::*;

fn vector(v: &Value) -> Vec3 {
    Vec3(std::array::from_fn(|i| v[i].as_f64().unwrap()))
}

fn main() {
    let destination = std::env::args().nth(1).expect("homing_probe output.json");
    let bodies: Value =
        serde_json::from_str(include_str!("../src/rules/geometry/fixtures/native.json")).unwrap();
    let movement: Value = serde_json::from_str(include_str!(
        "../src/rules/geometry/fixtures/native-motion.json"
    ))
    .unwrap();
    let mut rows = Vec::new();
    for case in movement["motion_cases"].as_array().unwrap() {
        let distance = case["center_separation"].as_f64().unwrap();
        for enemy in bodies["enemies"].as_array().unwrap() {
            let world = Affine([-1., 0., 0., 0., 0., 1., 0., 0., 0., 0., -1., distance]);
            let proxies: Vec<_> = enemy["proxies"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    let radius = v["shape"]["radius"].as_f64().unwrap();
                    Proxy {
                        cache_index: v["cache_index"].as_u64().unwrap() as usize,
                        bone_token: v["bone_token"].as_u64().unwrap_or(u64::from(u32::MAX)) as u32,
                        part: v["part"].as_u64().unwrap() as u8,
                        shape: if v["shape"]["type"] == 1 {
                            Shape::Capsule {
                                radius,
                                height: v["shape"]["capsule_height"].as_f64().unwrap(),
                            }
                        } else {
                            Shape::Sphere { radius }
                        },
                        world: world.compose(Affine(std::array::from_fn(|i| {
                            v["source_default_rows"][i].as_f64().unwrap()
                        }))),
                        ray_enabled: true,
                    }
                })
                .collect();
            let mut contact = None;
            for (i, step) in case["trace"].as_array().unwrap().iter().enumerate() {
                let origin = vector(&step["old"]);
                let delta = vector(&step["new"]).minus(origin);
                let length = delta.length();
                let Some(direction) = delta.normalized() else {
                    continue;
                };
                if let Refinement::Contact(hit) = refine(
                    1,
                    &proxies,
                    Ray {
                        origin,
                        direction,
                        maximum: length,
                        previous_best: f64::INFINITY,
                        excluded_parts: 0,
                    },
                    Gate::Active,
                ) {
                    contact = Some(
                        json!({"point":hit.point.0,"part":hit.part,"cache_index":hit.cache_index,
                        "segment":i,"segment_fraction":hit.distance/length,
                        "diagnostic_time_seconds":(i as f64+hit.distance/length)/case["hz"].as_f64().unwrap()}),
                    );
                    break;
                }
            }
            rows.push(json!({"enemy":enemy["path"],"distance":distance,"hz":case["hz"],"target_mode":case["target_mode"],
                "supplied_world_scale":1,"supplied_yaw_deg":180,"contact":contact}));
        }
    }
    let out = json!({"scope":"Recorded native motion queried with analytic segment rays against supplied source-default pose; not actual native projectile sweep or gameplay impact",
        "source_batch":movement["batch"],"source_member_sha256":movement["source_member_sha256"],
        "live_reticle_muzzle_spread_bound":false,"full_3d_fight_validated":false,"rows":rows});
    std::fs::write(destination, serde_json::to_vec_pretty(&out).unwrap()).unwrap();
}
