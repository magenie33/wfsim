// SPDX-License-Identifier: AGPL-3.0-or-later
//! Compare scene policies with all actor roots projected by the engine itself.
use serde_json::{json, Value};
use wfsim_engine::{rules::geometry::Vec3, scene::Scene};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(
        args.len(),
        2,
        "scene_controls explicit-calls.json paired-calls.json"
    );
    let calls: Vec<Value> = serde_json::from_slice(&std::fs::read(&args[0]).unwrap()).unwrap();
    let mut paired = Vec::new();
    for call in calls {
        let raw = &call["body"];
        let count = 1 + raw["formation"].as_array().map_or(0, Vec::len);
        let scene = Scene::new(
            serde_json::from_value(raw["scene_3d"].clone()).unwrap(),
            count,
        )
        .unwrap();
        let mut control = call.clone();
        let body = control["body"].as_object_mut().unwrap();
        body.remove("scene_3d");
        let planar = |v: Vec3| json!([v.0[0], v.0[2]]);
        body.insert(
            "player_at".into(),
            planar(scene.input().player.world.point(Vec3([0.; 3]))),
        );
        body.insert("target_at".into(), planar(scene.root(0)));
        if let Some(formation) = body.get_mut("formation").and_then(Value::as_array_mut) {
            for (i, enemy) in formation.iter_mut().enumerate() {
                enemy["at"] = planar(scene.root(i + 1));
            }
            assert_eq!(formation.len(), count - 1);
        }
        let id = call["id"].as_str().unwrap().trim_end_matches("-explicit");
        control["id"] = json!(format!("{id}-planar-control"));
        paired.push(call);
        paired.push(control);
    }
    std::fs::write(&args[1], serde_json::to_vec(&paired).unwrap()).unwrap();
}
