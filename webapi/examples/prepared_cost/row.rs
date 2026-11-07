// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json::{json, Value};
use wfsim_engine::fight::Shard;
use wfsim_webapi::PreparedSimulation;

fn piece(prepared: &PreparedSimulation, acc: &mut Shard, from: u32, count: u32) {
    for index in from..from + count {
        let value =
            serde_json::to_value(prepared.shard(index, 1, &mut |_, _| {}).unwrap()).unwrap();
        let shard: Shard = serde_json::from_value(value.clone()).unwrap();
        acc.merge(&shard);
    }
}

pub fn row(request: &Value, prepared: &PreparedSimulation, width: u32, retain: bool) -> Value {
    let mut acc = Shard::default();
    for from in (0..40).step_by(width as usize) {
        let count = width.min(40 - from);
        if retain {
            piece(prepared, &mut acc, from, count);
        } else {
            let current = PreparedSimulation::new(request).unwrap();
            piece(&current, &mut acc, from, count);
        }
        // Both paths validate the accumulator at the same batch boundaries.
        acc = serde_json::from_value(serde_json::to_value(&acc).unwrap()).unwrap();
    }
    json!({"ok": true, "acc": acc})
}
