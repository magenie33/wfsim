// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{budget::Budget, engine::Engine, state::Result};
use serde_json::{json, Value};
use std::sync::{atomic::Ordering, mpsc::Sender, Arc};

#[derive(Clone, Debug)]
pub enum Progress {
    Row { done: usize, total: usize },
    Search { step: usize, builds: usize },
}

fn ask(
    engine: &Engine,
    budget: &Budget,
    path: &str,
    body: &Value,
    spent: &mut f64,
) -> Result<Value> {
    let (out, ms) = budget.call(|| engine.call(path, body))?;
    *spent += ms;
    Ok(out)
}

pub fn row(
    engine: &Arc<Engine>,
    budget: &Arc<Budget>,
    task: &Value,
    tx: &Sender<Progress>,
) -> Result<Value> {
    let mut spent = 0.0;
    let order = if task["fight"].is_object() { json!({"ok":true,"request":task["fight"]}) } else { ask(
        engine,
        budget,
        "/api/board/order",
        &json!({"record": task["record"], "ruler":task["ruler"], "mode":task["mode"]}),
        &mut spent,
    )? };
    let request = &order["request"];
    let total = request["runs"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= 1_000_000)
        .ok_or("invalid board run count")? as usize;
    let mut acc = Value::Null;
    let mut at = 0;
    let mut per_run: f64 = 250.0;
    while at < total {
        if budget.cancelled.load(Ordering::Relaxed) {
            return Err("computation cancelled".into());
        }
        let lanes = budget.lanes().min(total - at);
        let size = (250.0 / per_run).round().clamp(1.0, 1000.0) as usize;
        let calls = lanes.min((total - at).div_ceil(size));
        let mut results = Vec::new();
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..calls)
                .map(|lane| {
                    let from = at + lane * size;
                    let count = size.min(total - from);
                    scope.spawn(move || {
                        budget
                            .call(|| {
                                engine.call(
                                    "/api/board/runs",
                                    &json!({"request":request,"from":from,"count":count}),
                                )
                            })
                            .map(|(r, ms)| (r, ms, count))
                    })
                })
                .collect();
            for h in handles {
                results.push(
                    h.join()
                        .map_err(|_| "engine lane panicked")
                        .and_then(|r| r.map_err(|_| "engine run failed")),
                );
            }
        });
        let mut pieces = Vec::new();
        for out in results {
            let (r, ms, count) = out?;
            let shards = r["shards"]
                .as_array()
                .filter(|x| x.len() == count)
                .ok_or("incomplete board run result")?;
            pieces.extend(shards.iter().cloned());
            spent += ms;
            per_run = per_run * 0.7 + ms / count as f64 * 0.3;
        }
        let count = pieces.len();
        let fold = ask(
            engine,
            budget,
            "/api/board/fold",
            &json!({"request":request,"acc":acc,"pieces":pieces}),
            &mut spent,
        )?;
        acc = fold["acc"].clone();
        at += count;
        let _ = tx.send(Progress::Row { done: at, total });
    }
    let mut score = ask(
        engine,
        budget,
        "/api/board/score",
        &json!({"ruler":task["ruler"],"request":request,"acc":acc}),
        &mut spent,
    )?;
    score["question"] = task["question"].clone();
    score["compute_ms"] = json!(spent.round() as u64);
    score["lanes"] = json!(budget.lanes());
    Ok(score)
}

pub fn search(
    engine: &Arc<Engine>,
    budget: &Arc<Budget>,
    work: &Value,
    tx: &Sender<Progress>,
) -> Result<Value> {
    let mut spent = 0.0;
    let mut scores = Vec::new();
    let mut fights = 0u64;
    let mut builds = 0u64;
    let mut measured_work = 0u64;
    let mut step = 0;
    loop {
        let mut body = work["request"].clone();
        body["quick_fleet"] = json!({"lead":true,"fresh":step==0,"scores":scores});
        let r = ask(engine, budget, "/api/compute/optimize", &body, &mut spent)?;
        let Some(pending) = r["pending"].as_array() else {
            let best = r["results"]
                .as_array()
                .and_then(|xs| {
                    xs.iter()
                        .find(|x| x["mods"].as_array().is_some_and(|m| !m.is_empty()))
                })
                .ok_or("search found no complete build")?;
            return Ok(
                json!({"build":payload(best, &work["context"]),"score":best["kill_progress"].as_f64().unwrap_or(0.0),
                "work":measured_work+r["final_work"].as_u64().unwrap_or(0),"compute_ms":spent.round() as u64,
                "counts":{"builds":builds,"fights":fights,"steps":step},"lanes":budget.lanes()}),
            );
        };
        if pending.is_empty() {
            return Err("search returned an empty pending round".into());
        }
        let lanes = budget.lanes().min(pending.len());
        let size = pending.len().div_ceil(lanes * 2).max(1);
        let slices: Vec<_> = pending.chunks(size).collect();
        scores.clear();
        for batch in slices.chunks(lanes) {
            let mut outs = Vec::new();
            std::thread::scope(|scope| {
                let handles: Vec<_> = batch
                    .iter()
                    .map(|slice| {
                        let mut request = work["request"].clone();
                        request["quick_fleet"] = json!({"score":slice});
                        scope.spawn(move || {
                            budget.call(|| engine.call("/api/compute/optimize", &request))
                        })
                    })
                    .collect();
                for h in handles {
                    outs.push(
                        h.join()
                            .map_err(|_| "search lane panicked")
                            .and_then(|r| r.map_err(|_| "search scoring failed")),
                    );
                }
            });
            for out in outs {
                let (o, ms) = out?;
                scores.extend(
                    o["scores"]
                        .as_array()
                        .ok_or("missing search scores")?
                        .iter()
                        .cloned(),
                );
                spent += ms;
                fights += o["fights"].as_u64().unwrap_or(0);
                measured_work += o["work"].as_u64().unwrap_or(0);
            }
            let _ = tx.send(Progress::Search {
                step,
                builds: scores.len(),
            });
        }
        builds += scores.len() as u64;
        step += 1;
    }
}

pub fn payload(best: &Value, context: &Value) -> Value {
    let exilus = best["exilus"]
        .as_str()
        .filter(|s| !s.is_empty() && *s != "none");
    let mods = best["mods"].as_array().cloned().unwrap_or_default();
    let riven = mods
        .iter()
        .filter_map(Value::as_str)
        .find(|s| s.starts_with("riven:"))
        .unwrap_or("-");
    let mut mods: Vec<Value> = mods
        .iter()
        .filter(|v| v.as_str() != exilus || exilus.is_none())
        .map(|v| {
            if v.as_str().is_some_and(|s| s.starts_with("riven:")) {
                json!("riven")
            } else {
                v.clone()
            }
        })
        .collect();
    if let Some(stance) = context["stance"].as_str().filter(|s| !s.is_empty()) {
        if !mods.contains(&json!(stance)) {
            mods.push(json!(stance));
        }
    }
    let arcs = if let Some(xs) = best["arcane"].as_array() {
        xs.clone()
    } else if best["arcane"].is_null() {
        Vec::new()
    } else {
        vec![best["arcane"].clone()]
    };
    let parts = best
        .get("assembly")
        .filter(|v| !v.is_null())
        .or_else(|| best["replay"].get("assembly").filter(|v| !v.is_null()))
        .unwrap_or(context);
    let mut out = json!({"weapon":context["weapon"],"mode":best.get("mode").filter(|v|v.as_str().is_some_and(|s|!s.is_empty())).unwrap_or(&context["mode"]),
        "mods":mods,"evolutions":best["evolutions"].as_array().cloned().unwrap_or_default(),
        "arcanes":arcs.into_iter().filter(|v|v.as_str().is_some_and(|s|!s.is_empty()&&s!="none")).collect::<Vec<_>>(),
        "riven_pos":[],"riven_neg":""});
    if let Some(shape) = context["rivens"].get(riven).and_then(Value::as_object) {
        for (k, v) in shape {
            out[k] = v.clone();
        }
    }
    for key in ["benchmark", "wielder"] {
        if let Some(v) = context
            .get(key)
            .filter(|v| !v.is_null() && **v != json!(""))
        {
            out[key] = v.clone();
        }
    }
    let valence = best
        .get("valence")
        .filter(|v| !v.is_null() && **v != json!(""))
        .or_else(|| context.get("valence"));
    if let Some(v) = valence {
        out["valence"] = v.clone();
    }
    if let Some(v) = exilus {
        out["exilus"] = json!(v);
    }
    for key in ["grip", "loader", "link"] {
        if let Some(v) = parts.get(key) {
            out[key] = v.clone();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn results_match_the_shared_board_wire_adapter() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../../tests/client_payloads.json")).unwrap();
        for case in cases {
            assert_eq!(payload(&case["result"], &case["context"]), case["expected"]);
        }
    }
}
