// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json::{json, Value};
use wfsim_optimizer::FunnelState;

use super::{LAST, MAX_KEY_BYTES};
use crate::optimize::{parse_optimize, parse_uncached, run_optimize, OptimizePlan};

fn request() -> Value {
    json!({
        "weapon": "braton_prime", "enemy": "thrax_centurion", "level": 100,
        "duration": 1, "runs": 3, "candidate_runs": 10, "finalists": 2, "threads": 1,
        "strategy": "quick", "search_strategy": "four_elements", "build_size": 3,
        "mods": {"serration":"search", "split_chamber":"search", "point_strike":"search",
            "thermite_rounds":"search", "rime_rounds":"search", "malignant_force":"search"},
        "starts": [{"slots": ["serration"], "fixed": [{"kind":"mods", "idx":0, "placement":"slot"}]}],
        "quick_fleet": {"lead":true, "fresh":true, "scores":[]}
    })
}

fn run(plan: OptimizePlan) -> Value {
    run_optimize(plan, &FunnelState::default(), |_, _| {}, None)
}

fn trace_output(mut value: Value) -> Value {
    // Independent invocations can encounter builds in a different order.
    if let Some(builds) = value["builds"].as_array_mut() {
        builds.sort_by_key(Value::to_string);
    }
    value
}

fn trace(mut req: Value, cached: bool) -> Vec<Value> {
    let mut trace = Vec::new();
    let parse = if cached { parse_optimize } else { parse_uncached };
    for step in 0..500 {
        let result = run(parse(&req).unwrap());
        assert_eq!(result["ok"], true, "{result}");
        trace.push(trace_output(result.clone()));
        let Some(pending) = result["pending"].as_array() else { return trace };
        let mut scores = Vec::new();
        for batch in pending.chunks(pending.len().div_ceil(3)) {
            req["quick_fleet"] = json!({"score": batch});
            let result = run(parse(&req).unwrap());
            assert_eq!(result["ok"], true, "{result}");
            scores.extend(result["scores"].as_array().unwrap().iter().cloned());
            trace.push(trace_output(result));
        }
        req["quick_fleet"] = json!({"lead":true, "fresh":false, "scores":scores});
        assert!(step < 499, "the search must settle");
    }
    unreachable!()
}

#[test]
fn prepared_fleet_matches_every_cold_batch_and_new_search() {
    let req = request();
    let cold = trace(req.clone(), false);
    assert!(cold.len() > 8, "cover more than one pending/scoring round");
    assert_eq!(trace(req.clone(), true), cold, "all outputs, ordering, shards, builds and work");
    assert_eq!(trace(req, true), cold, "a warm new search still performs the same work");
}

#[test]
fn every_request_field_participates_in_preparation_identity() {
    let mut req = request();
    parse_optimize(&req).unwrap();
    for (key, value) in [
        ("level", json!(101)), ("duration", json!(2)), ("runs", json!(4)),
        ("candidate_runs", json!(11)), ("headshot_pct", json!(25)), ("seed", json!(99)),
        ("starts", json!([{"slots":["rime_rounds"]}])),
        ("mods", json!({"serration":"search", "split_chamber":"search", "rime_rounds":"search"})),
        ("search_strategy", json!("invalid")), ("weapon", json!("unknown-weapon")),
        ("future_field", json!({"value": 1})),
    ] {
        let mut changed = req.clone();
        changed[key] = value;
        match (parse_optimize(&changed), parse_uncached(&changed)) {
            (Ok(actual), Ok(expected)) => {
                assert_eq!(actual.replay_base, expected.replay_base, "{key}");
                assert_eq!(run(actual), run(expected), "{key}");
            }
            (Err(actual), Err(expected)) => assert_eq!(actual, expected, "{key}"),
            _ => panic!("cached validation disagrees for {key}"),
        }
        parse_optimize(&req).unwrap();
    }
    req["quick_fleet"] = json!({"score": []});
    let before = parse_optimize(&req).unwrap();
    let after = parse_optimize(&req).unwrap();
    assert!(std::sync::Arc::ptr_eq(&before.forms, &after.forms), "traffic does not rebuild tables");
}

#[test]
fn cache_is_bounded_and_never_keeps_failed_or_large_requests() {
    let mut req = request();
    let first = parse_optimize(&req).unwrap();
    req["level"] = json!(500);
    parse_optimize(&req).unwrap();
    req["level"] = json!(100);
    let replaced = parse_optimize(&req).unwrap();
    assert!(!std::sync::Arc::ptr_eq(&first.forms, &replaced.forms));
    req["extra"] = json!("x".repeat(MAX_KEY_BYTES));
    let large = parse_optimize(&req).unwrap();
    assert!(LAST.with(|last| last.borrow().is_none()));
    assert_eq!(run(large), run(parse_uncached(&req).unwrap()));
    req["weapon"] = json!("unknown-weapon");
    assert!(parse_optimize(&req).is_err());
    assert!(LAST.with(|last| last.borrow().is_none()));
}

#[test]
fn prepared_plans_keep_independent_build_streams_after_cancel_and_concurrent_parse() {
    // Each stream preserves its own cursor order; parallel encounters can arrive
    // in a different order across invocations while retaining identical builds.
    let batch = |stream: &super::BuildStream| {
        let mut value = stream.batch(0);
        assert_eq!(stream.batch(0), value, "a cursor read must be stable");
        value["builds"].as_array_mut().unwrap().sort_by_key(Value::to_string);
        value
    };
    let ids = ["serration", "split_chamber", "point_strike", "vital_sense", "hellfire",
        "infected_clip", "stormbringer", "speed_trigger"];
    let req = json!({
        "weapon":"acceltra_prime", "enemy":"thrax_centurion", "level":100, "duration":1,
        "runs":2, "candidate_runs":10, "finalists":1, "threads":1, "strategy":"quick", "build_size":8,
        "mods":ids.iter().map(|id| (id.to_string(), json!("fixed"))).collect::<serde_json::Map<_, _>>(),
        "arcanes":{"primary_deadhead":"fixed"}, "modes":{"base":"fixed"},
        "starts":[{"slots":ids, "fixed":(0..8).map(|idx| json!({"kind":"mods", "idx":idx})).collect::<Vec<_>>()}],
        "quick_fleet":{}
    });
    let first = parse_optimize(&req).unwrap();
    let second = parse_optimize(&req).unwrap();
    let stream1 = first.build_stream();
    let stream2 = second.build_stream();
    let expected = run(first);
    assert!(stream1.has_pending());
    assert!(!stream2.has_pending(), "an unrun plan has no encountered builds");
    assert_eq!(run(second), expected);
    assert_eq!(batch(&stream1), batch(&stream2));
    assert!(stream1.batch(0)["builds"].as_array().unwrap().len() > 1);
    let cancelled = parse_optimize(&req).unwrap();
    let state = FunnelState::default();
    state.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    run_optimize(cancelled, &state, |_, _| {}, None);
    let restarted = parse_optimize(&req).unwrap();
    let stream3 = restarted.build_stream();
    assert_eq!(run(restarted), expected);
    assert_eq!(batch(&stream3), batch(&stream1));
    let worker = std::thread::spawn(move || {
        assert!(LAST.with(|last| last.borrow().is_none()));
        run(parse_optimize(&req).unwrap())
    });
    assert_eq!(worker.join().unwrap(), expected);
}
