// SPDX-License-Identifier: AGPL-3.0-or-later
//! Time frozen API calls and retain their complete responses for exact comparison.
//! `simulation_cost calls.json output.json [repeats]` reads `{id, path, body}` rows.

use std::time::Instant;

use serde_json::{json, Value};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(args.len() >= 2, "calls.json output.json [repeats]");
    let calls: Vec<Value> = serde_json::from_slice(&std::fs::read(&args[0]).unwrap()).unwrap();
    let repeats: usize = args.get(2).map_or(3, |s| s.parse().expect("repeats"));
    let mut rows = Vec::new();
    for call in calls {
        let path = call["path"].as_str().expect("endpoint path");
        let body = call["body"].to_string();
        let result = wfsim_webapi::route_text(path, &body);
        let parsed: Value = serde_json::from_str(&result).expect("JSON response");
        assert_eq!(parsed["ok"], true, "{}: {result}", call["id"]);
        let mut milliseconds = Vec::new();
        for _ in 0..repeats.max(1) {
            let start = Instant::now();
            let next = wfsim_webapi::route_text(path, &body);
            milliseconds.push(start.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(next, result, "{} changed between repeats", call["id"]);
        }
        eprintln!("{}: {milliseconds:?} ms", call["id"]);
        rows.push(json!({ "id": call["id"], "milliseconds": milliseconds, "result": parsed }));
    }
    std::fs::write(&args[1], serde_json::to_vec(&rows).unwrap()).unwrap();
}
