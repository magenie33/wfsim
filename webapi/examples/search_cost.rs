// SPDX-License-Identifier: AGPL-3.0-or-later
//! Measure a frozen optimize request, including the fleet's leader overhead.
//! `search_cost meta output.json` writes this engine's request metadata.
//! `search_cost request.json output.json [parts]` uses a serial fleet when parts > 0.

use std::time::Instant;

use serde_json::{json, Value};
use wfsim_optimizer::FunnelState;

fn call(request: &Value) -> (Value, f64, f64) {
    let start = Instant::now();
    let plan = wfsim_webapi::parse_optimize(request).expect("valid frozen request");
    let parse_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    let result = wfsim_webapi::run_optimize(plan, &FunnelState::default(), |_, _| {}, None);
    assert_eq!(result["ok"], true, "{result}");
    (result, parse_ms, start.elapsed().as_secs_f64() * 1000.0)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(args.len() >= 2, "request.json output.json [parts], or meta output.json");
    if args[0] == "meta" {
        std::fs::write(&args[1], serde_json::to_vec(&wfsim_webapi::meta_json()).unwrap()).unwrap();
        return;
    }
    let mut request: Value = serde_json::from_slice(&std::fs::read(&args[0]).unwrap()).unwrap();
    request["threads"] = json!(1);
    let parts: usize = args.get(2).map_or(0, |s| s.parse().expect("parts"));
    let start = Instant::now();
    let mut timings = Vec::new();
    let mut fights = 0u64;
    let mut work = 0u64;
    let mut scores: Vec<Value> = Vec::new();
    let result = if parts == 0 {
        let (result, parse_ms, run_ms) = call(&request);
        timings.push(json!({ "role": "native", "parse_ms": parse_ms, "run_ms": run_ms }));
        result
    } else {
        let mut step = 0;
        loop {
            request["quick_fleet"] = json!({ "lead": true, "fresh": step == 0, "scores": scores });
            let (out, parse_ms, run_ms) = call(&request);
            let pending = out.get("pending").and_then(Value::as_array);
            timings.push(json!({ "role": "leader", "step": step, "parse_ms": parse_ms,
                "run_ms": run_ms, "pending": pending.map_or(0, Vec::len) }));
            let Some(pending) = pending else { break out };
            step += 1;
            assert!(step < 10000, "the fleet must settle");
            scores = Vec::new();
            for batch in pending.chunks(pending.len().div_ceil(parts)) {
                request["quick_fleet"] = json!({ "score": batch });
                let (out, parse_ms, run_ms) = call(&request);
                fights += out["fights"].as_u64().unwrap_or(0);
                work += out["work"].as_u64().unwrap_or(0);
                timings.push(json!({ "role": "scorer", "step": step, "parse_ms": parse_ms,
                    "run_ms": run_ms, "builds": batch.len(), "fights": out["fights"] }));
                scores.extend(out["scores"].as_array().unwrap().iter().cloned());
            }
            if step % 20 == 0 {
                eprintln!("{step} batches, {fights} scored fights, {:.1}s", start.elapsed().as_secs_f64());
            }
        }
    };
    work += result["final_work"].as_u64().unwrap_or(0);
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let report = json!({ "parts": parts, "elapsed_ms": elapsed_ms,
        "scored_fights": (parts > 0).then_some(fights),
        "fleet_work": (parts > 0).then_some(work), "timings": timings, "result": result });
    std::fs::write(&args[1], serde_json::to_vec(&report).unwrap()).unwrap();
    println!("{elapsed_ms:.1} ms; {fights} fleet-scored fights; output {}", args[1]);
}
