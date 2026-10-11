// SPDX-License-Identifier: AGPL-3.0-or-later
//! `wfsim-gas <rows.json> [share] [repeats] [check]` — board rows fought
//! natively through the page's own doors (`webapi::board_rows`): one JSON line
//! a row with its gas, work, score and best time over the first `share` of its
//! runs. With `check`, also whether five other cuts of those runs — one run a
//! shard, seven, asked last first, two halves carried, asked again — gave the
//! same bits. The native side of `scripts/check_gas.mjs` and the work fit.
use serde_json::{json, Value};
use wfsim_webapi::board_rows::{board_fold_json, board_order_json, board_runs_json, board_score_json};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let Some(file) = a.get(1) else {
        eprintln!("usage: wfsim-gas <rows.json> [share] [repeats] [check]");
        std::process::exit(2);
    };
    let rows: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(file).expect("rows file")).expect("rows json");
    let share: f64 = a.get(2).map_or(1.0, |s| s.parse().expect("share"));
    let repeats: u32 = a.get(3).map_or(1, |s| s.parse().expect("repeats"));
    let check = a.get(4).is_some_and(|s| s == "check");
    for row in rows {
        let order = board_order_json(&row);
        let Some(req) = order.get("request").cloned() else {
            println!("{}", json!({ "error": order }));
            continue;
        };
        let runs = req["runs"].as_u64().unwrap_or(0) as u32;
        let n = ((f64::from(runs) * share).ceil() as u32).clamp(1, runs.max(1));
        let ruler = row["ruler"].clone();
        let score = |acc: &Value| board_score_json(&json!({ "ruler": ruler, "request": req, "acc": acc }));
        let fold = |from: u32, count: u32, acc: &Value| board_fold_json(&json!({ "request": req, "from": from, "count": count, "acc": acc }))["acc"].clone();
        let merged = |pieces: Vec<(u32, Value)>| {
            let mut pieces = pieces;
            pieces.sort_by_key(|(k, _)| *k);
            let pieces: Vec<Value> = pieces.into_iter().flat_map(|(_, s)| s.as_array().cloned().unwrap_or_default()).collect();
            board_fold_json(&json!({ "request": req, "acc": Value::Null, "pieces": pieces }))["acc"].clone()
        };
        let shards = |order: &[u32], piece: usize| -> Vec<(u32, Value)> {
            order.chunks(piece).map(|c| (c[0], board_runs_json(&json!({ "request": req, "from": c[0], "count": c.len() }))["shards"].clone())).collect()
        };
        let bits = |s: &Value| format!("{} {} {} {}", s["score"], s["metric"], s["work"], s["counts"]["gas"]);
        let (mut best, mut acc) = (f64::INFINITY, Value::Null);
        for _ in 0..repeats.max(1) {
            let t = std::time::Instant::now();
            acc = fold(0, n, &Value::Null);
            best = best.min(t.elapsed().as_secs_f64());
        }
        let s = score(&acc);
        let mut same = Value::Null;
        if check {
            let forward: Vec<u32> = (0..n).collect();
            let backward: Vec<u32> = (0..n).rev().collect();
            let half = fold(0, n / 2, &Value::Null);
            let ways = [
                score(&fold(0, n, &Value::Null)),
                score(&merged(shards(&forward, 1))),
                score(&merged(shards(&forward, 7))),
                score(&merged(shards(&backward, 1))),
                score(&fold(n / 2, n - n / 2, &half)),
            ];
            same = json!(ways.iter().all(|w| bits(w) == bits(&s)));
        }
        let key = format!("{}|{}|{}", row["record"]["weapon"].as_str().unwrap_or(""), row["ruler"].as_str().unwrap_or(""), row["mode"].as_str().unwrap_or(""));
        println!(
            "{}",
            json!({ "key": key, "runs": n, "seconds": best, "gas": s["counts"]["gas"], "work": s["work"], "score": s["score"], "metric": s["metric"], "same": same })
        );
    }
}
