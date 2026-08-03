//! `wfsim-truth` — grade the optimizer's search against ground truth.
//!
//! Takes the SAME request the app sends to `/api/optimize`, exhausts the
//! scope, evaluates every job flat, and reports where the production search
//! landed in that reference ranking. See docs/OPTIMIZER.md §Accuracy.
//!
//! Usage:
//!   wfsim-truth pool=serration,split_chamber,… [weapon=verglas_prime]
//!               [fixed=id,…] [min=1] [size=8] [enemy=thrax_centurion] [level=9999]
//!               [steel_path=0|1] [duration=300] [runs=100] [truth_runs=200]
//!               [finalists=10] [max_jobs=200000] [threads=N]
//!
//! `runs` is the search's own final-round precision (the scenario's);
//! `truth_runs` is the reference's, and it should be several times larger —
//! the reference has to be able to separate builds the search cannot.

use serde_json::{json, Value};

fn main() {
    let mut a: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for arg in std::env::args().skip(1) {
        match arg.split_once('=') {
            Some((k, v)) => {
                a.insert(k.to_string(), v.to_string());
            }
            None => {
                eprintln!("expected key=value, got {arg}");
                std::process::exit(2);
            }
        }
    }
    let get = |k: &str, d: &str| a.get(k).cloned().unwrap_or_else(|| d.to_string());
    let num = |k: &str, d: u64| get(k, &d.to_string()).parse::<u64>().unwrap_or(d);
    let ids = |k: &str| -> Vec<String> {
        get(k, "")
            .split(',')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    };

    let pooled = ids("pool");
    let fixed = ids("fixed");
    if pooled.is_empty() && fixed.is_empty() {
        eprintln!("nothing to search: give pool=<ids> (and/or fixed=<ids>)");
        std::process::exit(2);
    }
    let mut mods = serde_json::Map::new();
    for id in &fixed {
        mods.insert(id.clone(), Value::String("fixed".into()));
    }
    for id in &pooled {
        mods.entry(id.clone()).or_insert(Value::String("search".into()));
    }
    let req = json!({
        "weapon": get("weapon", "verglas_prime"),
        "mods": Value::Object(mods),
        "build_size": num("size", 8),
        "build_min": num("min", 1),
        "enemy": get("enemy", "thrax_centurion"),
        "level": num("level", 9999),
        "steel_path": get("steel_path", "1") != "0",
        "duration": get("duration", "300.0").parse::<f64>().unwrap_or(300.0),
        "runs": num("runs", 100),
        "finalists": num("finalists", 10),
        "threads": num("threads", 0),
    });

    let truth_runs = num("truth_runs", 200) as u32;
    let max_jobs = num("max_jobs", 200_000) as usize;
    let t0 = std::time::Instant::now();
    let out = wfsim_webapi::grade_optimize(&req, truth_runs, max_jobs);
    if out.get("ok").and_then(|x| x.as_bool()) != Some(true) {
        eprintln!("{}", out.get("error").and_then(|e| e.as_str()).unwrap_or("failed"));
        std::process::exit(1);
    }

    let (scope, refr, search) = (&out["scope"], &out["reference"], &out["search"]);
    println!(
        "[scope] {} builds x arcanes = {} jobs, exhaustive",
        scope["builds"], scope["jobs"]
    );
    println!(
        "[reference] {} runs each = {} sims | answer set {} builds | settled across seeds: {} | top-{} overlap {:.2}",
        refr["runs"],
        refr["sims"],
        refr["answer_set"],
        refr["settled"],
        refr["top"].as_array().map(|a| a.len()).unwrap_or(0),
        refr["cross_seed_overlap"].as_f64().unwrap_or(0.0),
    );
    if refr["settled"].as_bool() != Some(true) {
        println!("    !! the two reference seeds disagree on the best build — raise truth_runs;");
        println!("       every verdict below is measured against a ranking that is still noise.");
    }
    println!(
        "[search] rank {} | regret {:.3}% | within noise: {} | top-{} recall {:.0}% | {} sims ({:.1}% of the reference)",
        search["rank"],
        search["regret"].as_f64().unwrap_or(0.0) * 100.0,
        search["within_noise"],
        search["top"].as_array().map(|a| a.len()).unwrap_or(0),
        search["recall"].as_f64().unwrap_or(0.0) * 100.0,
        search["sims"],
        search["sims"].as_f64().unwrap_or(0.0) / refr["sims"].as_f64().unwrap_or(1.0) * 100.0,
    );
    let show = |tag: &str, rows: &Value| {
        println!("\n=== {tag} ===");
        for (i, r) in rows.as_array().map(|a| &a[..]).unwrap_or(&[]).iter().enumerate() {
            let v: Vec<&str> = r["vector"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
            let m: Vec<&str> = r["mods"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
            println!(
                "#{:<2} {:.4} ±{:.4} | {:<28} | {}",
                i + 1,
                r["mean"].as_f64().unwrap_or(0.0),
                r["se"].as_f64().unwrap_or(0.0),
                v.join(" / "),
                m.join(", ")
            );
        }
    };
    show("REFERENCE (flat, every job)", &refr["top"]);
    show("SEARCH (the production funnel)", &search["top"]);
    println!("\n[total] {:.1?}", t0.elapsed());
}
