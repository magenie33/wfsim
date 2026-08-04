//! THE SCORER — turns submitted builds into a board.
//!
//! Reads a JSON array of submissions on stdin, writes the board YAML on
//! stdout. That shape is deliberate: fetching from KV and committing the
//! result are the workflow's job (`.github/workflows/board.yml`), and neither
//! needs the engine. What needs the engine is the only thing here — running
//! each build under the benchmark and reading the number off.
//!
//! WHY A SUBMISSION CARRIES NO SCORE, restated because this binary is where it
//! becomes true: nobody's number is trusted because nobody's number is asked
//! for. A row's score is produced HERE, by this engine, under the benchmark's
//! own pinned seed — so anyone with the repo can reproduce any row exactly, in
//! a browser as well as natively (measured 2026-08-04: wasm and native agree to
//! the last digit). A forged submission cannot forge a rank; the worst it can
//! do is submit a build that scores badly.
//!
//! It also means an engine change re-scores everything instead of migrating
//! anything: the builds are still builds. Nobody is ever asked to resubmit.
//!
//! TWO OUTPUTS, because the board has two readers with different needs. The
//! YAML on stdout is the CANONICAL record, committed and diffable. The JSON is
//! what the PAGE fetches at runtime — and it exists because a board that
//! changes hourly must not require rebuilding a 2.5 MB wasm to reach anyone.
//! Compiling it in made every board update a full site rebuild: install
//! wasm-bindgen, fetch 300 images, recompile — to change a few numbers.
//!
//!   cat submissions.json | wfsim-board single_target_v1 site/board.json > board.yaml

use std::io::Read;

use serde_json::{json, Value};

/// One scored row, before it is trimmed to the top N.
struct Row {
    weapon: String,
    score: f64,
    mods: Vec<String>,
    evolutions: Vec<String>,
    arcanes: Vec<String>,
}

/// How many rows a weapon keeps. Ten so a board can hold real ALTERNATIVES —
/// a build without the arcane you lack, one that costs fewer Forma — rather
/// than ten spellings of one answer.
const KEEP: usize = 10;

fn main() {
    let bench_id = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: wfsim-board <benchmark-id>  (submissions as JSON on stdin)");
        std::process::exit(2);
    });
    let bench = wfsim_engine::benchmarks_data::get(&bench_id).unwrap_or_else(|| {
        eprintln!("unknown benchmark: {bench_id}");
        std::process::exit(2);
    });

    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw).expect("stdin");
    let subs: Vec<Value> = serde_json::from_str(&raw).unwrap_or_default();

    // The benchmark's scenario, as the wire shape `simulate_json` parses. It is
    // the SAME map the app sends, which is what stops the board and the page
    // from measuring two different fights.
    let scenario: Value = serde_json::to_value(&bench.scenario).expect("scenario");

    let mut rows: Vec<Row> = Vec::new();
    let (mut seen, mut refused) = (0usize, 0usize);
    for s in &subs {
        if s.get("benchmark").and_then(Value::as_str) != Some(bench_id.as_str()) {
            continue;
        }
        seen += 1;
        let weapon = s.get("weapon").and_then(Value::as_str).unwrap_or("").to_string();
        let get = |k: &str| -> Vec<String> {
            s.get(k)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
                .unwrap_or_default()
        };
        let (mods, evos, arcs) = (get("mods"), get("evolutions"), get("arcanes"));

        // THE SAME CHECK A BOARD ROW FACES ANYWHERE. A submission arrives over
        // a network with no UI on the path, so "could a player equip this" is
        // asked here rather than assumed — and it NORMALISES first, so what
        // gets scored and what gets published are the same object.
        let Ok(v) = wfsim_engine::builds::validate(&weapon, &mods, &evos, &arcs) else {
            refused += 1;
            continue;
        };

        let mut req = scenario.clone();
        if let Some(o) = req.as_object_mut() {
            o.insert("weapon".into(), json!(v.weapon));
            o.insert("mods".into(), json!(v.mods));
            o.insert("evolutions".into(), json!(v.evolutions));
            o.insert("arcane".into(), json!(v.arcanes));
        }
        let out = wfsim_engine_webapi_simulate(&req);
        let ok = out.get("ok").and_then(Value::as_bool).unwrap_or(false);
        let score = out.get("score").and_then(Value::as_f64).unwrap_or(0.0);
        if !ok || score <= 0.0 {
            refused += 1;
            continue;
        }
        rows.push(Row {
            weapon: v.weapon,
            score,
            mods: v.mods,
            evolutions: v.evolutions,
            arcanes: v.arcanes,
        });
    }

    // Best first, then the top KEEP per weapon. Ties keep the FEWER-Forma build
    // — same fight, cheaper to own.
    rows.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut per: std::collections::BTreeMap<String, usize> = Default::default();
    let mut kept = Vec::new();
    for r in rows {
        let n = per.entry(r.weapon.clone()).or_insert(0);
        if *n < KEEP {
            *n += 1;
            kept.push(r);
        }
    }
    kept.sort_by(|a, b| {
        a.weapon
            .cmp(&b.weapon)
            .then(b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal))
    });

    eprintln!("{seen} submissions, {refused} refused, {} rows", kept.len());

    // The runtime copy, keyed by weapon because that is how the page asks.
    if let Some(path) = std::env::args().nth(2) {
        let mut by_weapon: std::collections::BTreeMap<&str, Vec<Value>> = Default::default();
        for r in &kept {
            by_weapon.entry(&r.weapon).or_default().push(json!({
                "benchmark": bench_id,
                "source": "submissions",
                "score": r.score,
                "mods": r.mods,
                "evolutions": r.evolutions,
                "arcanes": r.arcanes,
            }));
        }
        std::fs::write(&path, serde_json::to_string(&by_weapon).expect("json"))
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        eprintln!("wrote {path}");
    }
    println!("# THE OFFICIAL BOARD for `{bench_id}` — GENERATED by `wfsim-board`.");
    println!("#");
    println!("# Rows are BUILDS players submitted; every score was computed here, by");
    println!("# this engine, under that benchmark's own pinned seed. Nobody submits a");
    println!("# number, so any row can be reproduced exactly by anyone with the repo.");
    println!("#");
    println!("# Regenerated whole on every run — never edited by hand, and never");
    println!("# merged: an engine or data change re-scores everything rather than");
    println!("# migrating anything, because the builds are still builds.");
    println!("benchmark: {bench_id}");
    println!("source: submissions");
    println!("entries:");
    for r in kept {
        println!("  - weapon: {}", r.weapon);
        println!("    score: {:.6}", r.score);
        println!("    mods: [{}]", r.mods.join(", "));
        if !r.evolutions.is_empty() {
            println!("    evolutions: [{}]", r.evolutions.join(", "));
        }
        if r.arcanes.iter().any(|a| a != "none") {
            println!("    arcanes: [{}]", r.arcanes.join(", "));
        }
    }
}

/// `webapi::simulate_json` under a name that says the crate boundary is
/// deliberate: the scorer runs the SAME entry point the web api runs, so the
/// board cannot drift from what the page computes.
fn wfsim_engine_webapi_simulate(v: &Value) -> Value {
    wfsim_webapi::simulate_json(v)
}
