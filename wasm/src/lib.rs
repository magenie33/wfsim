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
/// frontend knows it ("/api/meta", "/api/panel", "/api/simulate",
/// "/api/opt-buffs"), `body` the request JSON ("" or "{}" for /api/meta).
/// Returns the response JSON as a string.
#[wasm_bindgen]
pub fn api(endpoint: &str, body: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let out = match endpoint {
        "/api/meta" => wfsim_webapi::meta_json(),
        "/api/i18n" => wfsim_webapi::i18n_json(),
        "/api/panel" => wfsim_webapi::panel_json(&v),
        "/api/simulate" => wfsim_webapi::simulate_json(&v),
        "/api/opt-buffs" => wfsim_webapi::opt_buffs_json(&v),
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
        "notes": notes,
    });
    if let Some((cands, jobs)) = counts {
        out["candidates"] = serde_json::json!(cands);
        out["jobs"] = serde_json::json!(jobs);
    }
    out.to_string()
}

/// Run an optimize request to completion (blocking — call inside a Worker).
/// `on_progress` receives a status-JSON string after enumeration and after
/// every funnel round; the returned string is the final result JSON.
#[wasm_bindgen]
pub fn optimize(body: &str, on_progress: &js_sys::Function) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let plan = match wfsim_webapi::parse_optimize(&v) {
        Ok(p) => p,
        Err(e) => return e.to_string(),
    };
    let state = FunnelState::default();
    let t0 = js_sys::Date::now();
    let post = |payload: String| {
        let _ = on_progress.call1(&JsValue::NULL, &JsValue::from_str(&payload));
    };
    let counts = std::cell::Cell::new(None);
    let result = wfsim_webapi::run_optimize(
        plan,
        &state,
        |cands, jobs| {
            counts.set(Some((cands, jobs)));
            post(status_json(&state, "running", Some((cands, jobs)), (js_sys::Date::now() - t0) / 1000.0));
        },
        Some(&|| {
            post(status_json(&state, "running", counts.get(), (js_sys::Date::now() - t0) / 1000.0));
        }),
    );
    result.to_string()
}
