// SPDX-License-Identifier: AGPL-3.0-or-later
// The weapon entry in `meta_json` is one `json!` literal deep enough to hit
// the macro's default expansion limit — reached when the evolution tile
// gained its "what this does not do yet" fields. A limit, not a smell:
// splitting the literal to please a counter would scatter one payload
// across helpers that exist for no other reason.
#![recursion_limit = "512"]
//! wfsim-webapi: the JSON API layer, independent of any transport.
//!
//! Every endpoint the frontend talks to lives here as a plain
//! `&Value -> Value` function, one module per endpoint: [`meta_json`],
//! [`panel_json`], [`simulate_json`], [`opt_buffs_json`], and the optimize
//! pair [`parse_optimize`] / [`run_optimize`]. The native HTTP server
//! (`wfsim-web`) and the wasm build (docs/WASM.md phase 2) both dispatch the
//! pure ones through [`route`]. The compute is the SAME engine the CLI and
//! optimizer use — this crate only shapes JSON.

mod board;
pub mod board_rows;
mod buffs;
mod candidates;
mod catalogue;
mod fight;
mod forma;
mod i18n;
mod kitgun;
mod log;
mod meta;
mod optimize;
mod panel;
mod qr;
mod registry;
mod request;
mod rivens;
mod shapley;
mod simulate;
mod tenno;
mod warframe;

pub use board::{board_check_json, build_keys_json, targets_json};
pub use candidates::candidates_json;
pub use fight::pairings_json;
pub use forma::{forma_optimize_json, forma_plan_json};
pub use i18n::i18n_json;
pub use log::log_json;
pub use meta::meta_json;
pub use optimize::{
    funnel_status_json, grade_optimize, opt_buffs_json, parse_optimize, run_optimize,
    run_optimize_resumable, BoardSink, CheckpointSink, JobIdentity, OptimizePlan, ResumeFrom,
};
pub use panel::panel_json;
pub use request::err_json;
pub use rivens::riven_json;
pub use shapley::{shapley_json, MAX_PARTICIPANTS as SHAPLEY_MAX_PARTICIPANTS};
pub use simulate::{
    riven_request, simulate_json, simulate_json_reporting, simulate_merged_json,
    simulate_request, simulate_shard_json, RIVEN_ITEM,
};
pub use catalogue::catalogue_json;
pub use warframe::{operator_panel_json, warframe_catalog_json, warframe_panel_json};

use serde_json::Value;

/// A pure endpoint: the request body in, the response out.
pub type Endpoint = fn(&Value) -> Value;

/// EVERY PURE ENDPOINT, by the path the frontend knows it by. The native server
/// and the wasm worker both dispatch through [`route`], so an endpoint added
/// here reaches both transports and one added anywhere else reaches one.
/// `/api/meta`, `/api/i18n` and `/api/warframe/catalog` ignore the body.
pub const ROUTES: &[(&str, Endpoint)] = &[
    ("/api/meta", |_| meta_json()),
    ("/api/i18n", |_| i18n_json()),
    ("/api/panel", panel_json),
    ("/api/candidates", candidates_json),
    ("/api/pairings", pairings_json),
    ("/api/simulate", simulate_json),
    ("/api/shapley", shapley_json),
    ("/api/log", log_json),
    ("/api/opt-buffs", opt_buffs_json),
    ("/api/riven", riven_json),
    ("/api/targets", targets_json),
    ("/api/board/check", board_check_json),
    ("/api/board/order", board_rows::board_order_json),
    ("/api/board/runs", board_rows::board_runs_json),
    ("/api/board/fold", board_rows::board_fold_json),
    ("/api/board/score", board_rows::board_score_json),
    ("/api/build/keys", build_keys_json),
    ("/api/forma/plan", forma_plan_json),
    ("/api/forma/optimize", forma_optimize_json),
    ("/api/warframe/catalog", |_| warframe_catalog_json()),
    ("/api/warframe/panel", warframe_panel_json),
    ("/api/operator/panel", operator_panel_json),
    ("/api/catalogue", |_| catalogue_json()),
    ("/api/qr", qr::qr_json),
];

/// The response to a pure endpoint, or `None` for a path [`ROUTES`] does not
/// hold. The stateful ones — the optimize job, a simulate that reports
/// progress, a shard — stay with the transport that owns the state.
pub fn route(path: &str, body: &Value) -> Option<Value> {
    ROUTES.iter().find(|(p, _)| *p == path).map(|(_, f)| f(body))
}

/// A CALL THAT SAYS IT IS ALIVE: `beat` runs from inside the engine's work, every
/// fight run and every data file read (`wfsim_engine::alive`), whatever the endpoint.
pub fn set_heartbeat(beat: Option<fn()>) {
    wfsim_engine::alive::set(beat);
}

/// INSTALL ONE DATA PACK, as the wasm module's `install_data` and the native
/// library's `wfsim_install_data` both do: `{"pack", "format", "files":
/// {path: text}}` (docs/CLIENTS.md §"The official data pack"). Returns "" or
/// what was wrong.
pub fn install_pack(text: &str) -> String {
    #[derive(serde::Deserialize)]
    struct Pack {
        pack: String,
        format: u32,
        files: std::collections::BTreeMap<String, String>,
    }
    match serde_json::from_str::<Pack>(text) {
        Ok(p) if p.format != wfsim_engine::data::PACK_FORMAT => {
            format!("pack {}: format {} is not the {} this engine reads", p.pack, p.format, wfsim_engine::data::PACK_FORMAT)
        }
        Ok(p) => wfsim_engine::data::install(&p.pack, p.files.into_iter().collect()).err().unwrap_or_default(),
        Err(e) => format!("not a data pack: {e}"),
    }
}

/// THE PACKS A CALL READS: `data` always, and the two only a catalog reads —
/// which a page taking those catalogs from the build's files never downloads
/// (docs/WASM.md §"Answers the build already knows").
fn packs_needed(endpoint: &str) -> &'static [&'static str] {
    match endpoint {
        "/api/meta" | "/api/warframe/catalog" => &["data", "site"],
        "/api/i18n" => &["text"],
        _ => &["data"],
    }
}

/// A CALL BEFORE ITS PACKS ARE IN is answered with what is missing, never with
/// a catalog that silently lacks it. `endpoint` "" is a computation.
pub fn packs_missing(endpoint: &str) -> Option<String> {
    let missing: Vec<&str> = wfsim_engine::data::missing_packs()
        .into_iter()
        .filter(|p| packs_needed(endpoint).contains(p))
        .collect();
    (!missing.is_empty()).then(|| err_json(format!("engine data not installed: {}", missing.join(", "))).to_string())
}

/// [`route`] as text in and text out: the door the wasm module's `api` and the
/// desktop's native library both are, so the two answer one request one way.
/// A body that is not JSON is `null`; a path [`ROUTES`] does not hold is an error.
pub fn route_text(path: &str, body: &str) -> String {
    if let Some(e) = packs_missing(path) {
        return e;
    }
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    route(path, &v).unwrap_or_else(|| err_json(format!("unknown endpoint: {path}"))).to_string()
}
