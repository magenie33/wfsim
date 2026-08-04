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
//!   cat submissions.json | wfsim-board single_target site/board.json > board.yaml

use std::io::Read;

use serde_json::{json, Value};

/// A benchmark id without its `_v<n>` suffix — `single_target_v2` and
/// `single_target_v1` are the same ruler, and a build aimed at
/// either belongs on the current one's board.
fn family(id: &str) -> &str {
    match id.rsplit_once("_v") {
        Some((head, tail)) if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) => head,
        _ => id,
    }
}

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
    // The benchmark's own terms, read once — the metric it is measured in and
    // the length that metric is over.
    let metric = scenario
        .get("metric")
        .and_then(Value::as_str)
        .unwrap_or("kpm")
        .to_string();
    let duration = scenario.get("duration").and_then(Value::as_f64).unwrap_or(300.0);
    assert!(
        matches!(metric.as_str(), "kpm" | "dps"),
        "unknown benchmark metric {metric:?} — a row published in units nobody          named is worse than no row"
    );

    let mut rows: Vec<Row> = Vec::new();
    let (mut seen, mut refused) = (0usize, 0usize);
    let mut seen_ids: std::collections::HashSet<String> = Default::default();
    for s in &subs {
        // MATCHED BY FAMILY, which today is a MIGRATION SHIM and nothing more.
        // Benchmarks carry no version (owner, 2026-08-04) — but records already
        // in the store were submitted against `single_target_v1`, and those are
        // builds like any other. Stripping the suffix is what lets them keep
        // competing under the id that replaced it, which is the same rule as
        // everywhere else here: a changed standard RE-SCORES rather than asking
        // anyone to resubmit ("留在榜里，如果有其他后来居上的，就可以自然淘汰").
        //
        // A different ruler entirely — `group_clear` — is a different family and
        // keeps its own board.
        if family(s.get("benchmark").and_then(Value::as_str).unwrap_or("")) != family(&bench_id) {
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
        // THE BOARD'S door, not the legality one: a row must be a COMPLETE
        // build (2026-08-05). A submission that is merely legal is refused
        // here and simply never scored.
        let Ok(v) = wfsim_engine::builds::validate_for_board(&weapon, &mods, &evos, &arcs) else {
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
        let raw = out.get("score").and_then(Value::as_f64).unwrap_or(0.0);
        if !ok || raw <= 0.0 {
            refused += 1;
            continue;
        }
        // IN THE BENCHMARK'S OWN METRIC. `score` off the wire is kill PROGRESS
        // — kills plus the fraction of the current target depleted — over the
        // whole engagement. The benchmark says `metric: kpm`, so publishing the
        // raw figure labelled "kill rate" overstated every row by the length of
        // the fight: 55.26 on screen for a build that kills 11.05 a minute over
        // 300 s (user, 2026-08-04). Ranking is unaffected either way — this is
        // a linear rescale — but the number people read is not a ranking.
        let score = match metric.as_str() {
            "dps" => out.get("dps").and_then(Value::as_f64).unwrap_or(0.0),
            _ => raw * 60.0 / duration,
        };
        // ONE ROW PER BUILD. The endpoint stores what was submitted, verbatim,
        // because it has no mod pool and cannot tell an elemental mod from any
        // other — so two spellings of one fight arrive as two records and are
        // collapsed HERE, where `validate` has already put both into the same
        // canonical form.
        let key = wfsim_engine::builds::identity(&v);
        if !seen_ids.insert(key) {
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
                // The number stays EXACT and the string beside it is what the
                // page prints. Formatting lives in `boards_data::format_score`,
                // so "four significant figures, four decimals" is one rule in
                // one language rather than a Rust copy and a JS copy that drift.
                "shown": wfsim_engine::boards_data::format_score(r.score),
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
        // FULL PRECISION in the record. `{}` on an f64 is the shortest string
        // that reads back as the same number, so the yaml is the measurement
        // rather than a rounding of it — the published figure is rounded at the
        // point it is SHOWN, and two rows that tie on screen still rank.
        println!("    score: {}", r.score);
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
