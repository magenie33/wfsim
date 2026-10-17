// SPDX-License-Identifier: AGPL-3.0-or-later
//! `/api/catalogue`: the cards of every category the builder does not hold yet,
//! by slot id. Nothing in it is simulated (`wfsim_engine::data::catalogue`).

use serde_json::{json, Value};

pub fn catalogue_json() -> Value {
    let by_slot: serde_json::Map<String, Value> = wfsim_engine::data::catalogue::all()
        .iter()
        .map(|c| (c.slot.clone(), json!({ "kind": c.kind, "cards": c.cards })))
        .collect();
    Value::Object(by_slot)
}
