// SPDX-License-Identifier: AGPL-3.0-or-later
//! Actual combat under explicit player/enemy pose, reticle and flight inputs.
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use wfsim_engine::rules::geometry::Vec3;
#[path = "../../tests/fixtures/geometry/equipped.rs"]
mod equipped;
#[path = "../../tests/fixtures/geometry/scene.rs"]
mod fixture;

fn call(path: &str, request: &Value) -> Value {
    let result: Value =
        serde_json::from_str(&wfsim_webapi::route_text(path, &request.to_string())).unwrap();
    assert_eq!(result["ok"], true, "{path}: {result}");
    result
}

fn standing_head_fights(out: &str) {
    let mut results = Vec::new();
    for (weapon, mods) in [
        ("akarius_prime", &[][..]),
        ("laetum", &[][..]),
        ("laetum", &["seeker"][..]),
    ] {
        let mut scene = fixture::scene(1, 5, false, false, true);
        equipped::equip(&mut scene, weapon);
        let equipment = scene["player"]["equipped_fire"].take();
        scene["player"] = json!({"equipped_fire":equipment});
        for key in ["camera", "reference_point"] {
            scene.as_object_mut().unwrap().remove(key);
        }
        scene["frame_seconds"] = json!(1. / 64.);
        let mut request = json!({
            "weapon":weapon, "mode":"base", "mods":mods, "enemy":"corrupted_heavy_gunner",
            "level":225, "steel_path":true, "headshot_pct":100, "duration":4,
            "runs":1, "infinite_ammo":true, "formation":fixture::formation(&scene),
            "scene_3d":scene, "limit":20000,
        });
        let mut shots = 0;
        let mut first_bodies = BTreeMap::new();
        let mut first_parts = BTreeMap::new();
        let mut follow_up_bodies = BTreeMap::new();
        let mut head_contacts = 0;
        let mut launch_directions = BTreeSet::new();
        let mut run_states = BTreeSet::new();
        let mut direct_counts = BTreeMap::new();
        let mut radial_counts = BTreeMap::new();
        let mut acquired = 0;
        let mut example = Value::Null;
        for seed in 24301..24349 {
            request["seed"] = json!(seed);
            let simulation = call("/api/simulate", &request);
            let run: [u32; 2] = serde_json::from_value(simulation["run"].clone()).unwrap();
            assert!(run_states.insert(run));
            request["run"] = json!(run);
            let record = call("/api/log", &request);
            assert_eq!(record["run"], request["run"]);
            let flights = record["scene_3d"]["flights"].as_array().unwrap();
            assert!(!flights.is_empty());
            for row in flights {
                let path = &row["flight"];
                let direct = path["impacts"].as_array().unwrap();
                let radial = path["radial_impacts"].as_array().map_or(0, Vec::len);
                let vector =
                    |field: &str| serde_json::from_value::<Vec3>(path[field].clone()).unwrap();
                let camera = vector("camera");
                let aimed = vector("aim_target").minus(camera).normalized().unwrap();
                let head_center = vector("head_anchor").minus(camera).normalized().unwrap();
                assert!((1. - aimed.dot(head_center)).abs() < 1e-12);
                launch_directions.insert(vector("direction").0.map(f64::to_bits));
                shots += 1;
                acquired += usize::from(path["steering_target"].is_number());
                *direct_counts.entry(direct.len()).or_insert(0) += 1;
                if let Some(first) = direct.first() {
                    *first_bodies
                        .entry(first["body"].as_u64().unwrap())
                        .or_insert(0) += 1;
                    *first_parts
                        .entry(first["part"].as_u64().unwrap())
                        .or_insert(0) += 1;
                }
                head_contacts += direct.iter().filter(|hit| hit["part"] == 1).count();
                for hit in direct.iter().skip(1) {
                    *follow_up_bodies
                        .entry(hit["body"].as_u64().unwrap())
                        .or_insert(0) += 1;
                }
                if path["blast_center"].is_array() {
                    *radial_counts.entry(radial).or_insert(0) += 1;
                }
            }
            if example.is_null() {
                example = json!({"request":request,"log":record});
            }
        }
        let summary = json!({
            "weapon":weapon, "mods":mods, "seeds":run_states.len(), "physical_flights":shots,
            "first_contact_actor_counts":first_bodies, "first_contact_part_counts":first_parts,
            "follow_up_actor_counts":follow_up_bodies, "head_contacts":head_contacts,
            "distinct_launch_directions":launch_directions.len(),
            "direct_contacts_per_flight":direct_counts, "acquired":acquired,
            "recipients_per_armed_blast":radial_counts,
        });
        eprintln!("{summary}");
        results.push(json!({"summary":summary,"example":example}));
    }
    std::fs::write(out, serde_json::to_vec(&results).unwrap()).unwrap();
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args
        .next()
        .expect("scene_fights output.json [--equipped|--standing-head]");
    let equipped = match args.next().as_deref() {
        Some("--standing-head") => return standing_head_fights(&out),
        Some("--equipped") => true,
        None => false,
        _ => panic!("scene_fights output.json [--equipped|--standing-head]"),
    };
    let mut results = Vec::new();
    for aligned in [false, true]
        .into_iter()
        .filter(|&aligned| !equipped || !aligned)
    {
        for enemy in 0..2 {
            for contact in [true, false] {
                for weapon in ["laetum", "akarius_prime"] {
                    let mut scene = fixture::scene(enemy, 5, contact, false, equipped);
                    if equipped {
                        equipped::equip(&mut scene, weapon);
                    }
                    if aligned {
                        let prepared = wfsim_engine::scene::Scene::new(
                            serde_json::from_value(scene.clone()).unwrap(),
                            25,
                        )
                        .unwrap();
                        let flight = prepared.trace(
                            true,
                            0.,
                            0.,
                            Some(100.),
                            None,
                            0.,
                            false,
                            false,
                            0.,
                            100.,
                            &[true; 25],
                            None,
                        );
                        let view = flight.aim_target.minus(flight.camera).normalized().unwrap();
                        let muzzle = prepared.camera().along(view, 3.);
                        for (j, i) in [3, 7, 11].iter().enumerate() {
                            scene["player"]["bones"][48][i] = json!(muzzle.0[j]);
                        }
                    }
                    let request = json!({"weapon":weapon,"mode":"base","mods":["hornet_strike","lethal_torrent","primed_pistol_gambit","primed_target_cracker"],
            "enemy":if enemy==0{"thrax_centurion"}else{"corrupted_heavy_gunner"},"level":225,"steel_path":true,
            "headshot_pct":100,"duration":10,"runs":20,"seed":24301,"infinite_ammo":true,
            "formation":fixture::formation(&scene),"scene_3d":scene});
                    let simulation = call("/api/simulate", &request);
                    let mut log_request = request.clone();
                    log_request["run"] = simulation["run"].clone();
                    log_request["limit"] = json!(20000);
                    let log = call("/api/log", &log_request);
                    let flights = log["scene_3d"]["flights"]
                        .as_array()
                        .expect("actual scene trace");
                    let direct: Vec<_> = log["events"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|e| {
                            e["kind"] == "damage" && e["part"].is_string() && e["radial"] != true
                        })
                        .collect();
                    let heads = direct.iter().filter(|e| e["head"] == true).count();
                    let summary = json!({"weapon":weapon,"enemy":request["enemy"],"supplied_muzzle_on_camera_ray":aligned,"placement":request["scene_3d"]["placement"],
            "score":simulation["score"],"dps":simulation["dps"],"flights":flights.len(),
            "acquired":flights.iter().filter(|f|f["flight"]["steering_target"].is_number()).count(),
            "direct_instances":direct.len(),"head_instances":heads});
                    eprintln!("{summary}");
                    results.push(json!({"summary":summary,"request":request,"simulation":simulation,"log":log}));
                    std::fs::write(&out, serde_json::to_vec(&results).unwrap()).unwrap();
                }
            }
        }
    }
}
