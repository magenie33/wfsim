//! wfsim-wasm: the browser build (docs/WASM.md phases 2–3).
//!
//! Exposes the `wfsim-webapi` endpoint functions to JavaScript. The intended
//! host is a Web Worker owned by the frontend's `api()` transport shim: the
//! quick endpoints go through [`api`]; the long-running optimizer goes
//! through [`optimize`], which posts per-round progress via a JS callback
//! (shaped like the native /api/optimize/status payload, so the progress UI
//! renders it unchanged) and returns the final result JSON. Cancellation of
//! a busy single-threaded worker is impossible from the outside — the page
//! terminates the Worker instead (all state lives inside it), which is the
//! clean v1 cancel.

use std::sync::atomic::Ordering;

use wasm_bindgen::prelude::*;
use wfsim_optimizer::FunnelState;

/// Dispatch a quick endpoint call: `endpoint` is the API path as the
/// frontend knows it ("/api/meta", "/api/panel", "/api/simulate", "/api/targets",
/// "/api/opt-buffs"), `body` the request JSON ("" or "{}" for /api/meta).
/// Returns the response JSON as a string.
#[wasm_bindgen]
pub fn api(endpoint: &str, body: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let out = match endpoint {
        "/api/meta" => wfsim_webapi::meta_json(),
        "/api/i18n" => wfsim_webapi::i18n_json(),
        "/api/panel" => wfsim_webapi::panel_json(&v),
        "/api/pairings" => wfsim_webapi::pairings_json(&v),
        "/api/simulate" => wfsim_webapi::simulate_json(&v),
        "/api/opt-buffs" => wfsim_webapi::opt_buffs_json(&v),
        "/api/riven" => wfsim_webapi::riven_json(&v),
        "/api/targets" => wfsim_webapi::targets_json(&v),
        other => wfsim_webapi::err_json(format!("unknown endpoint: {other}")),
    };
    out.to_string()
}

/// Snapshot `FunnelState` into the native /api/optimize/status shape (minus
/// `job_id`/`result` — the worker protocol owns those).
fn status_json(state: &FunnelState, phase: &str, counts: Option<(usize, usize)>, elapsed_s: f64) -> String {
    let notes: Vec<serde_json::Value> = state
        .notes
        .lock()
        .unwrap()
        .iter()
        .map(|n| {
            serde_json::json!({
                "round": n.round, "jobs": n.jobs, "runs": n.runs,
                "by_kills": n.by_kills, "kept": n.kept, "best": n.best, "ms": n.ms,
            })
        })
        .collect();
    let mut out = serde_json::json!({
        "ok": true,
        "phase": phase,
        "elapsed_s": elapsed_s,
        "round": state.round.load(Ordering::Relaxed),
        "rounds": state.rounds.load(Ordering::Relaxed),
        "round_jobs": state.round_jobs.load(Ordering::Relaxed),
        "round_runs": state.round_runs.load(Ordering::Relaxed),
        "sims_done": state.sims_done.load(Ordering::Relaxed),
        "sims_planned": state.sims_planned.load(Ordering::Relaxed),
        "enumerated": state.enumerated.load(Ordering::Relaxed),
        "rewalking": state.rewalking.load(Ordering::Relaxed),
        "notes": notes,
    });
    if let Some((cands, jobs)) = counts {
        out["candidates"] = serde_json::json!(cands);
        out["jobs"] = serde_json::json!(jobs);
    }
    out.to_string()
}

/// Run an optimize request to completion (blocking — call inside a Worker).
/// `on_progress` receives a status-JSON string continuously: a throttled
/// heartbeat DURING enumeration/screening/rounds (the optimizer lib's tick
/// hook — a busy single-threaded worker cannot be polled, and before the
/// heartbeat a big scope looked dead), plus one message after enumeration
/// and after every funnel round. The returned string is the final result
/// JSON.
///
/// `on_board` receives a RESULT-shaped best-so-far during the search. Cancel
/// in the browser is a worker kill, so a leaderboard that has not already left
/// the worker dies with it (user 2026-07-30: 20 minutes, cancelled, nothing).
#[wasm_bindgen]
pub fn optimize(
    body: &str,
    on_progress: &js_sys::Function,
    on_checkpoint: &js_sys::Function,
    on_board: &js_sys::Function,
) -> String {
    optimize_inner(body, "", on_progress, on_checkpoint, on_board)
}

/// Resume a run from a checkpoint written by a previous session — a page
/// reload kills the worker (and a SharedWorker too; measured 2026-07-30), so
/// this is what stops a reload costing the whole search.
#[wasm_bindgen]
pub fn optimize_resume(
    body: &str,
    checkpoint: &str,
    on_progress: &js_sys::Function,
    on_checkpoint: &js_sys::Function,
    on_board: &js_sys::Function,
) -> String {
    optimize_inner(body, checkpoint, on_progress, on_checkpoint, on_board)
}

fn optimize_inner(
    body: &str,
    checkpoint: &str,
    on_progress: &js_sys::Function,
    on_checkpoint: &js_sys::Function,
    on_board: &js_sys::Function,
) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let plan = match wfsim_webapi::parse_optimize(&v) {
        Ok(p) => p,
        Err(e) => return e.to_string(),
    };
    let state = std::rc::Rc::new(FunnelState::default());
    let t0 = js_sys::Date::now();
    let post = |payload: String| {
        let _ = on_progress.call1(&JsValue::NULL, &JsValue::from_str(&payload));
    };
    let counts = std::rc::Rc::new(std::cell::Cell::new(None));
    {
        // The ~4 Hz heartbeat out of the busy worker.
        let state = state.clone();
        let counts = counts.clone();
        let f = on_progress.clone();
        let last = std::cell::Cell::new(0.0f64);
        // The budget clock.
        let budget_t0 = std::cell::Cell::new(t0);
        wfsim_optimizer::set_tick_hook(Some(Box::new(move || {
            let now = js_sys::Date::now();
            if now - last.get() < 250.0 {
                return;
            }
            last.set(now);
            if state.rewalking.load(std::sync::atomic::Ordering::Relaxed) {
                budget_t0.set(now);
            }
            // THE SEARCH BUDGET. A browser tab cannot sit through the whole
            // space (the full pool is ~10⁹ subsets and this build simulates one
            // engagement per candidate, single-threaded), so the search is
            // given a clock and answers with the best it found. NOT `cancel`:
            // that would return an empty result — this asks for a best-so-far,
            // and the result reports the COVERAGE it reached.
            //
            // Five minutes, not the 20 s it was (user, 2026-08-03: accuracy
            // over convenience — a search the visitor asked for, with a
            // progress bar and a Cancel button in front of it, may take real
            // time). What the number cannot fix is WHICH builds a cut leaves
            // behind; that was the depth-first walk's doing and is the search's
            // to fix (see optimizer/src/space.rs).
            const ENUM_BUDGET_MS: f64 = 300_000.0;
            // The EXPLORE share ends first. The search samples the space until
            // then and climbs from what it found afterwards; a host whose
            // budget is a clock cannot express a FRACTION of it as an
            // evaluation count, so it says so here instead. The 0.3 is
            // measured — see `SearchConfig::default`.
            const EXPLORE_MS: f64 = ENUM_BUDGET_MS * 0.3;
            if counts.get().is_none() {
                let spent = now - budget_t0.get();
                if spent > EXPLORE_MS {
                    state
                        .stop_explore
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
                if spent > ENUM_BUDGET_MS {
                    state
                        .stop_enumeration
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            let phase = if counts.get().is_some() { "running" } else { "searching" };
            let payload = status_json(&state, phase, counts.get(), (now - t0) / 1000.0);
            let _ = f.call1(&JsValue::NULL, &JsValue::from_str(&payload));
        })));
    }
    // A checkpoint carries IDENTITIES only — (ordered pool indices, evo-set,
    // exilus, arcane) — so it stays small enough for localStorage and cannot
    // drift from what the enumerator would rebuild.
    let resume: Option<wfsim_webapi::ResumeFrom> = (!checkpoint.is_empty())
        .then(|| serde_json::from_str::<serde_json::Value>(checkpoint).ok())
        .flatten()
        .and_then(|cp| {
            // Only one kind survives: a completed funnel ROUND, as identities
            // the enumerator rebuilds. The mid-search kind is gone — see
            // `webapi::run_optimize_resumable`.
            let alive = cp.get("alive")?.as_array()?.iter().filter_map(|e| {
                let a = e.as_array()?;
                let ordered = a.first()?.as_array()?
                    .iter().filter_map(|x| x.as_u64().map(|n| n as usize)).collect();
                Some((ordered, a.get(1)?.as_u64()? as u32, a.get(2)?.as_u64()? as u32,
                      a.get(3)?.as_u64()? as usize))
            }).collect::<Vec<_>>();
            (!alive.is_empty()).then_some(wfsim_webapi::ResumeFrom::Round {
                round: cp.get("round").and_then(|r| r.as_u64()).unwrap_or(0) as usize,
                jobs_at_start: cp.get("jobs_at_start").and_then(|r| r.as_u64()).unwrap_or(0) as usize,
                alive,
            })
        });
    let result = wfsim_webapi::run_optimize_resumable(
        plan,
        &state,
        |cands, jobs| {
            counts.set(Some((cands, jobs)));
            post(status_json(&state, "running", Some((cands, jobs)), (js_sys::Date::now() - t0) / 1000.0));
        },
        Some(&|| {
            post(status_json(&state, "running", counts.get(), (js_sys::Date::now() - t0) / 1000.0));
        }),
        resume,
        Some(
            &|round: usize,
              jobs_at_start: usize,
              ids: &[(Vec<usize>, u32, u32, usize)],
              board: &serde_json::Value| {
                let list: Vec<serde_json::Value> = ids
                    .iter()
                    .map(|(o, v, x, a)| serde_json::json!([o, v, x, a]))
                    .collect();
                // The round's leaderboard rides with its resume point: one
                // message answers both "continue from where?" and "what had
                // it found?".
                let payload = serde_json::json!({
                    "kind": "round",
                    "round": round, "jobs_at_start": jobs_at_start, "alive": list,
                    "board": board,
                })
                .to_string();
                let _ = on_checkpoint.call1(&JsValue::NULL, &JsValue::from_str(&payload));
            },
        ),
        Some(&|board: &serde_json::Value| {
            let _ = on_board.call1(&JsValue::NULL, &JsValue::from_str(&board.to_string()));
        }),
    );
    wfsim_optimizer::set_tick_hook(None);
    result.to_string()
}
