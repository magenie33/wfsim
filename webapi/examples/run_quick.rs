// Local probe: run one frozen optimize request and print its rows.
use serde_json::{json, Value};
use std::sync::atomic::Ordering::Relaxed;
use wfsim_optimizer::FunnelState;
use wfsim_webapi::{parse_optimize, run_optimize};

fn main() {
    let path = std::env::args().nth(1).expect("request file");
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut req = v.get("request").cloned().unwrap_or(v);
    for kv in std::env::args().skip(2) {
        let (k, x) = kv.split_once('=').unwrap();
        req[k] = serde_json::from_str(x).unwrap_or(Value::String(x.into()));
    }
    let per_min = req["duration"].as_f64().unwrap_or(300.0) / 60.0;
    let state = FunnelState::default();
    let t = std::time::Instant::now();
    let out = run_optimize(parse_optimize(&req).unwrap(), &state, |_, _| {}, None);
    let secs = t.elapsed().as_secs_f64();
    let rows: Vec<Value> = out["results"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            json!({
                "kpm": r["kill_progress"].as_f64().unwrap_or(f64::NAN) / per_min,
                "arcane": r["arcane"], "evolutions": r["evolutions"], "mods": r["mods"],
                "exilus": r["exilus"], "mode": r["mode"], "valence": r["valence_element"], "from": r["from_starts"],
            })
        })
        .collect();
    println!(
        "{}",
        json!({ "secs": secs, "sims": state.sims_done.load(Relaxed), "searched": out["searched"], "fights": out["probe_fights"],
                "final_work": out["final_work"], "rows": rows, "error": out.get("error") })
    );
}
