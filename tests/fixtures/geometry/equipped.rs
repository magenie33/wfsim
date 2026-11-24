// SPDX-License-Identifier: AGPL-3.0-or-later
//! Source-default weapon bones mounted on supplied Excalibur hands for controls.
use serde_json::{json, Value};

pub fn equip(scene: &mut Value, weapon: &str) {
    let source: Value = serde_json::from_str(include_str!(
        "../../../engine/src/rules/geometry/fixtures/native-attachments.json"
    ))
    .unwrap();
    let control: &[(&str, usize)] = match weapon {
        "laetum" => &[("ZarimanHeavyPistolAttachment", 48), ("VoidPistolAttachment", 48)],
        "akarius_prime" => &[
            ("PrimeAkariusAttachmentRight", 48),
            ("PrimeAkariusAttachmentLeft", 17),
        ],
        _ => panic!("no weapon pose fixture for {weapon}"),
    };
    let mut attachments = Vec::new();
    let mut round_order = Vec::new();
    for (attachment, (id, bone)) in control.iter().enumerate() {
        let pose = source["attachments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == *id)
            .unwrap();
        attachments.push(json!({
            "mount":{"bone":bone,"local":[1,0,0,0,0,1,0,0,0,0,1,0]},
            "bones":pose["bones"], "profiles":pose["profiles"]
        }));
        round_order.push(json!({
            "attachment":attachment,"mode":0,
            "refresh_per_iteration":false,"advance_after_round":true
        }));
    }
    let mut fire = json!({"attachments":attachments,"round_order":round_order});
    if weapon == "laetum" {
        fire["round_order"] = json!([round_order[0]]);
        fire["round_order_by_form"] = json!({"incarnon":[round_order[1]]});
    }
    scene["player"]["equipped_fire"] = fire;
}
