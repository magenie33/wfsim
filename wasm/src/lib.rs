//! wfsim-wasm: the browser build (docs/WASM.md phase 2).
//!
//! Exposes the `wfsim-webapi` endpoint functions to JavaScript. The intended
//! host is a Web Worker owned by the frontend's `api()` transport shim: the
//! quick endpoints go through [`api`]; the long-running optimizer goes
//! through [`optimize`], which reports per-round progress via a JS callback
//! and returns the final result JSON. Cancellation of a busy single-threaded
//! worker is impossible from the outside — the page terminates the Worker
//! instead (all state lives inside it), which is the clean v1 cancel.

use wasm_bindgen::prelude::*;

/// Dispatch a quick endpoint call: `endpoint` is the API path as the
/// frontend knows it ("/api/meta", "/api/panel", "/api/simulate",
/// "/api/opt-buffs"), `body` the request JSON ("" or "{}" for /api/meta).
/// Returns the response JSON as a string.
#[wasm_bindgen]
pub fn api(endpoint: &str, body: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let out = match endpoint {
        "/api/meta" => wfsim_webapi::meta_json(),
        "/api/panel" => wfsim_webapi::panel_json(&v),
        "/api/simulate" => wfsim_webapi::simulate_json(&v),
        "/api/opt-buffs" => wfsim_webapi::opt_buffs_json(&v),
        other => wfsim_webapi::err_json(format!("unknown endpoint: {other}")),
    };
    out.to_string()
}

/// Run an optimize request to completion (blocking — call inside a Worker).
/// `on_progress` receives a JSON string after enumeration and after every
/// funnel round, shaped like the native /api/optimize/status payload so the
/// frontend's progress UI renders it unchanged.
#[wasm_bindgen]
pub fn optimize(body: &str, on_progress: &js_sys::Function) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let plan = match wfsim_webapi::parse_optimize(&v) {
        Ok(p) => p,
        Err(e) => return e.to_string(),
    };
    let state = wfsim_optimizer::FunnelState::default();
    let post = |payload: String| {
        let _ = on_progress.call1(&JsValue::NULL, &JsValue::from_str(&payload));
    };
    let result = wfsim_webapi::run_optimize(plan, &state, |cands, jobs| {
        post(
            serde_json::json!({
                "ok": true, "phase": "running", "candidates": cands, "jobs": jobs,
            })
            .to_string(),
        );
    });
    result.to_string()
}
