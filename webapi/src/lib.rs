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
mod buffs;
mod fight;
mod forma;
mod i18n;
mod kitgun;
mod log;
mod meta;
mod optimize;
mod panel;
mod registry;
mod request;
mod rivens;
mod shapley;
mod simulate;
mod tenno;
mod warframe;

pub use board::{board_check_json, build_keys_json, targets_json};
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
    ("/api/pairings", pairings_json),
    ("/api/simulate", simulate_json),
    ("/api/shapley", shapley_json),
    ("/api/log", log_json),
    ("/api/opt-buffs", opt_buffs_json),
    ("/api/riven", riven_json),
    ("/api/targets", targets_json),
    ("/api/board/check", board_check_json),
    ("/api/build/keys", build_keys_json),
    ("/api/forma/plan", forma_plan_json),
    ("/api/forma/optimize", forma_optimize_json),
    ("/api/warframe/catalog", |_| warframe_catalog_json()),
    ("/api/warframe/panel", warframe_panel_json),
    ("/api/operator/panel", operator_panel_json),
];

/// The response to a pure endpoint, or `None` for a path [`ROUTES`] does not
/// hold. The stateful ones — the optimize job, a simulate that reports
/// progress, a shard — stay with the transport that owns the state.
pub fn route(path: &str, body: &Value) -> Option<Value> {
    ROUTES.iter().find(|(p, _)| *p == path).map(|(_, f)| f(body))
}
