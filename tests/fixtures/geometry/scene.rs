// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit scene inputs shared by geometry tests and the combat experiment.
use serde_json::{json, Value};

fn matrix(scale: f64, x: f64, y: f64, z: f64, yaw: f64) -> Value {
    let (s, c) = yaw.to_radians().sin_cos();
    json!([
        scale * c,
        0.,
        scale * s,
        x,
        0.,
        scale,
        0.,
        y,
        -scale * s,
        0.,
        scale * c,
        z
    ])
}

pub fn full_body_capsule(local_scale: f64) -> Value {
    // AHOM-RUNTIME-01 GrineerFullBody on the hip in both pinned source poses.
    json!({
        "local": matrix(local_scale, 0., 1.1208325624465942, -0.011569245718419552, 0.),
        "shape": {"type":"capsule", "radius":f64::from(0.6_f32), "height":1.}
    })
}

pub fn scene(enemy: usize, rows: usize, contact: bool, left: bool, source_pose: bool) -> Value {
    let native: Value = serde_json::from_str(include_str!(
        "../../../engine/src/rules/geometry/fixtures/native.json"
    ))
    .unwrap();
    let player: Value = serde_json::from_str(include_str!(
        "../../../engine/src/rules/geometry/fixtures/player.json"
    ))
    .unwrap();
    let scale = if enemy == 0 { 1.6 } else { 1.25 };
    let source = &native["enemies"][enemy]["proxies"];
    let proxies:Vec<_>=source.as_array().unwrap().iter().map(|p|json!({
        "cache_index":p["cache_index"],"bone_token":p["bone_token"],"part":p["part"],
        "world":p["source_default_rows"],"ray_enabled":true,
        "shape":if p["shape"]["type"]==2{json!({"type":"sphere","radius":p["shape"]["radius"]})}
        else{json!({"type":"capsule","radius":p["shape"]["radius"],"height":p["shape"]["capsule_height"]})}
    })).collect();
    let anchor = source[0]["source_default_rows"].as_array().unwrap();
    let head = json!([anchor[3], anchor[7], anchor[11]]);
    let mut bones = player["bones"].clone();
    let head_bone = player["bones"][35].as_array().unwrap();
    let camera = matrix(
        1.,
        -3. * head_bone[8].as_f64().unwrap(),
        -3. * head_bone[9].as_f64().unwrap(),
        -3. * head_bone[10].as_f64().unwrap(),
        0.,
    );
    let hand = if left { 17 } else { 48 };
    // Supplied alternate pose uses the native empty-world trace's muzzle.
    // No claim that these are the equipped Akarius/Laetum attachment writers.
    if !source_pose {
        bones[hand] = matrix(1., 0., 1.5, 0.3, 0.);
    }
    let mut enemies = Vec::new();
    for row in 0..rows {
        for col in 0..rows {
            let x = (col as f64 - (rows / 2) as f64) * 3.;
            enemies.push(json!({"world":matrix(scale,x,0.,5.+row as f64*3.,180.),
            "controller":{"height":1.3,"radius":0.25,"skin":0.02,"scale":scale},
            "proxies":proxies,"radial_proxy":full_body_capsule(1.),
            "candidate_anchor":head,"steering_center":[0.,1.,0.]}));
        }
    }
    // The intended head is the front row's middle enemy, index zero.
    let first = enemies.remove(rows / 2);
    enemies.insert(0, first);
    json!({"player":{"world":matrix(1.,0.,0.,0.,0.),
        "controller":{"height":1.5,"radius":0.44999998807907104,"skin":0.019999999552965164,"scale":1.},
        "bones":bones,"muzzle":{"bone":hand,"local":matrix(1.,0.,0.,0.,0.)}},
        "enemies":enemies,"camera":{"bone":35,"local":camera},
        "reference_point":[0.,1.5,0.],"head_proxy":0,"body_proxy":1,
        "frame_seconds":1./60.,"maximum_flight_seconds":2.,"preselected_target":null,
        "placement":if contact{json!({"kind":"contact"})}else{json!({"kind":"center_distance","distance":5.})}})
}

pub fn formation(scene: &Value) -> Value {
    json!(scene["enemies"]
        .as_array()
        .unwrap()
        .iter()
        .skip(1)
        .enumerate()
        .map(|(i, e)| json!({
            "id":format!("e{}",i+2),"at":[e["world"][3],e["world"][11]]
        }))
        .collect::<Vec<_>>())
}
