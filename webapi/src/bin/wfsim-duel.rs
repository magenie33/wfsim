// SPDX-License-Identifier: AGPL-3.0-or-later
//! `wfsim-duel` — the two search strategies on ONE request, head to head.
//!
//! `wfsim-truth` grades a strategy against an exhausted scope, which caps the
//! scope at what can be exhausted. This answers the other question — which
//! strategy wins on a scope too big for that, the whole pool included — by
//! running the production pipeline once per strategy and replaying both
//! winners through the simulator on ONE seed, so the gap is a paired
//! difference with its own standard error.
//!
//! Usage:
//!   wfsim-duel pool=all|id,… [weapon=verglas_prime] [fixed=id,…] [min=1] [size=8]
//!              [enemy=thrax_centurion] [level=9999] [steel_path=0|1] [duration=60]
//!              [runs=100] [finalists=10] [max_evals=N] [arcanes=id,…] [evo1=id,…] …
//!              [starts=…] [swap_width=N] [replay_runs=400] [threads=N]
//!
//! `max_evals` is the search budget BOTH strategies get; 0 lets each run to
//! its own end (the sampler's end is the whole space).

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
        get(k, "").split(',').filter(|s| !s.is_empty()).map(str::to_string).collect()
    };

    let weapon = get("weapon", "verglas_prime");
    // `pool=all` is every card the weapon can equip outside the exilus slot —
    // the scope the sampler cannot finish and the descent was built for.
    let pooled: Vec<String> = if get("pool", "") == "all" {
        wfsim_engine::data::mods::pool_for_weapon(&weapon)
            .into_iter()
            .filter(|m| !m.exilus)
            .map(|m| m.id.to_string())
            .collect()
    } else {
        ids("pool")
    };
    let mut mods = serde_json::Map::new();
    for id in ids("fixed") {
        mods.insert(id, Value::String("fixed".into()));
    }
    for id in &pooled {
        mods.entry(id.clone()).or_insert(Value::String("search".into()));
    }
    let arcanes: serde_json::Map<String, Value> =
        ids("arcanes").into_iter().map(|id| (id, Value::String("search".into()))).collect();
    let evolutions: serde_json::Map<String, Value> = (1..=5)
        .filter_map(|t| {
            let v = ids(&format!("evo{t}"));
            (!v.is_empty()).then(|| (t.to_string(), json!(v)))
        })
        .collect();
    let base = json!({
        "weapon": weapon,
        "mods": Value::Object(mods),
        "arcanes": Value::Object(arcanes),
        "evolutions": Value::Object(evolutions),
        "build_size": num("size", 8),
        "build_min": num("min", 1),
        "enemy": get("enemy", "thrax_centurion"),
        "level": num("level", 9999),
        "steel_path": get("steel_path", "1") != "0",
        "duration": get("duration", "60.0").parse::<f64>().unwrap_or(60.0),
        "runs": num("runs", 100),
        "finalists": num("finalists", 10),
        "threads": num("threads", 0),
        "max_evals": num("max_evals", 0),
        "swap_width": num("swap_width", 1),
        "starts": get("starts", "")
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.split('+').collect::<Vec<_>>())
            .collect::<Vec<_>>(),
    });
    println!("[scope] {} cards searched on {}", pooled.len(), base["weapon"]);

    let mut winners: Vec<(&str, Value)> = Vec::new();
    for strategy in ["sample", "descent"] {
        let mut req = base.clone();
        req["strategy"] = json!(strategy);
        let plan = match wfsim_webapi::parse_optimize(&req) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{}", e["error"].as_str().unwrap_or("bad request"));
                std::process::exit(1);
            }
        };
        let state = wfsim_optimizer::FunnelState::default();
        let t0 = std::time::Instant::now();
        let out = wfsim_webapi::run_optimize(plan, &state, |_, _| {}, None);
        if out.get("ok").and_then(Value::as_bool) != Some(true) {
            eprintln!("{strategy}: {}", out["error"].as_str().unwrap_or("failed"));
            std::process::exit(1);
        }
        let top = &out["results"][0];
        println!(
            "[{strategy:<7}] {:.1?} | {} subsets searched, exhaustive {} | {} sims | winner {:.4} ±{:.4} | {} | {}",
            t0.elapsed(),
            out["searched"],
            out["exhaustive"],
            state.sims_done.load(std::sync::atomic::Ordering::Relaxed),
            top["kill_progress"].as_f64().unwrap_or(0.0),
            top["kill_progress_se"].as_f64().unwrap_or(0.0),
            top["arcane"],
            top["mods"],
        );
        winners.push((strategy, top["replay"].clone()));
    }

    // THE REPLAY, paired: both winners on one seed, compared run by run.
    let replay_runs = num("replay_runs", 400);
    let series: Vec<Vec<f64>> = winners
        .iter()
        .map(|(_, r)| {
            let mut r = r.clone();
            r["runs"] = json!(replay_runs);
            r["seed"] = json!(7);
            r["run_series"] = json!(true);
            let out = wfsim_webapi::simulate_json(&r);
            out["score_runs"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_f64).collect())
                .unwrap_or_default()
        })
        .collect();
    let n = series[0].len().min(series[1].len());
    if n < 2 {
        eprintln!("the replay returned no per-run series");
        std::process::exit(1);
    }
    let d: Vec<f64> = (0..n).map(|i| series[1][i] - series[0][i]).collect();
    let mean = d.iter().sum::<f64>() / n as f64;
    let var = d.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1) as f64;
    let se = (var / n as f64).sqrt();
    let m0 = series[0].iter().sum::<f64>() / n as f64;
    println!(
        "[replay] {n} paired runs: sampler {m0:.4}, descent - sampler = {mean:+.4} ± {se:.4} ({:+.2}%, {:.1}σ)",
        if m0 > 0.0 { mean / m0 * 100.0 } else { 0.0 },
        if se > 0.0 { mean / se } else { 0.0 },
    );
}
