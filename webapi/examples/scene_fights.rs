// SPDX-License-Identifier: AGPL-3.0-or-later
//! Actual combat under explicit player/enemy pose, reticle and flight inputs.
use serde_json::{json, Value};
#[path = "../../tests/fixtures/geometry/scene.rs"]
mod fixture;
#[path = "../../tests/fixtures/geometry/equipped.rs"]
mod equipped;

fn call(path: &str, request: &Value) -> Value {
    let result: Value =
        serde_json::from_str(&wfsim_webapi::route_text(path, &request.to_string())).unwrap();
    assert_eq!(result["ok"], true, "{path}: {result}");
    result
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("scene_fights output.json [--equipped]");
    let equipped = match args.next().as_deref() {
        Some("--equipped") => true,
        None => false,
        _ => panic!("scene_fights output.json [--equipped]"),
    };
    let mut results = Vec::new();
    for aligned in [false, true].into_iter().filter(|&aligned| !equipped || !aligned) {
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
                        let muzzle = prepared.camera().along(flight.direction, 3.);
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
