// SPDX-License-Identifier: AGPL-3.0-or-later
//! wfsim-web: a tiny, dependency-light web UI for the engine.
//!
//! A std-only HTTP server (no web framework) that serves a static frontend
//! and routes the JSON endpoints to `wfsim-webapi` — the transport-free API
//! layer shared with the wasm build (docs/WASM.md phase 2). This file keeps
//! only: sockets, routing, static assets, and the background-job registry
//! wrapping `run_optimize` (jobs + status/cancel endpoints).
//!
//! The compute is the SAME engine the CLI and optimizer use — this is just a
//! different front door. Static assets are embedded via `include_str!`, so
//! the binary is self-contained (no cwd assumptions).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{json, Value};
use wfsim_optimizer::FunnelState;
use wfsim_webapi::{err_json, funnel_status_json, parse_optimize, run_optimize};

// ---- Embedded static assets (self-contained binary) --------------------

const INDEX_HTML: &str = include_str!("static/index.html");
const APP_JS: &str = include_str!("static/app.js");
const STYLE_CSS: &str = include_str!("static/style.css");
const LOGO_SVG: &str = include_str!("static/logo.svg");

/// NONA'S MODULES, served at `/nona/<path>`. Every file under `static/nona/` is
/// listed here and in no other way; `check_nona_boundary` fails on one missing.
const NONA_FILES: &[(&str, &str)] = &[
    ("index.js", include_str!("static/nona/index.js")),
    ("core/budget.js", include_str!("static/nona/core/budget.js")),
    ("core/calc.js", include_str!("static/nona/core/calc.js")),
    ("core/measure.js", include_str!("static/nona/core/measure.js")),
    ("core/memory.js", include_str!("static/nona/core/memory.js")),
    ("core/prompt.js", include_str!("static/nona/core/prompt.js")),
    ("core/record.js", include_str!("static/nona/core/record.js")),
    ("core/size.js", include_str!("static/nona/core/size.js")),
    ("core/skills.js", include_str!("static/nona/core/skills.js")),
    ("core/summary.js", include_str!("static/nona/core/summary.js")),
    ("core/tools.js", include_str!("static/nona/core/tools.js")),
    ("core/view.js", include_str!("static/nona/core/view.js")),
    ("core/protocols/anthropic.js", include_str!("static/nona/core/protocols/anthropic.js")),
    ("core/protocols/openai.js", include_str!("static/nona/core/protocols/openai.js")),
    ("core/protocols/sse.js", include_str!("static/nona/core/protocols/sse.js")),
    ("runtime/agent.js", include_str!("static/nona/runtime/agent.js")),
    ("runtime/policy.js", include_str!("static/nona/runtime/policy.js")),
    ("runtime/store.js", include_str!("static/nona/runtime/store.js")),
    ("runtime/transport.js", include_str!("static/nona/runtime/transport.js")),
    ("ui/cards.js", include_str!("static/nona/ui/cards.js")),
    ("ui/chips.js", include_str!("static/nona/ui/chips.js")),
    ("ui/kit.js", include_str!("static/nona/ui/kit.js")),
    ("ui/panel.js", include_str!("static/nona/ui/panel.js")),
    ("ui/settings.js", include_str!("static/nona/ui/settings.js")),
];

// ---- main / server loop ------------------------------------------------

fn main() {
    let port: u16 = std::env::var("WFSIM_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8787);
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
        eprintln!("wfsim-web: cannot bind {addr}: {e}");
        std::process::exit(1);
    });
    println!("wfsim-web listening on http://{addr}  (Ctrl-C to stop)");

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                std::thread::spawn(move || {
                    if let Err(e) = handle(s) {
                        eprintln!("connection error: {e}");
                    }
                });
            }
            Err(e) => eprintln!("accept error: {e}"),
        }
    }
}

// ---- minimal HTTP ------------------------------------------------------

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn read_request(stream: &TcpStream) -> std::io::Result<Option<Request>> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(None); // connection closed
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break; // end of headers
        }
        if let Some(v) = line
            .to_ascii_lowercase()
            .strip_prefix("content-length:")
            .map(|s| s.trim().to_string())
        {
            content_length = v.parse().unwrap_or(0);
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    Ok(Some(Request { method, path, body }))
}

fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn respond_json(stream: &mut TcpStream, value: &Value) -> std::io::Result<()> {
    respond(
        stream,
        "200 OK",
        "application/json; charset=utf-8",
        value.to_string().as_bytes(),
    )
}

/// Serve a static asset (icon/image) with a long cache lifetime.
fn respond_asset(stream: &mut TcpStream, content_type: &str, body: &[u8]) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: public, max-age=604800\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// 302 redirect (used as the /img CDN fallback when the local cache misses).
fn respond_redirect(stream: &mut TcpStream, location: &str) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(header.as_bytes())?;
    stream.flush()
}

// Polarity/damage icons are vendored (tiny, stable) so they load instantly —
// no more slow wiki `Special:FilePath` 302 redirects.
const POL_MADURAI: &[u8] = include_bytes!("static/pol/Madurai_Pol.svg");
const POL_NARAMON: &[u8] = include_bytes!("static/pol/Naramon_Pol.svg");
const POL_VAZARIN: &[u8] = include_bytes!("static/pol/Vazarin_Pol.svg");
const POL_UMBRA: &[u8] = include_bytes!("static/pol/Umbra_Pol.svg");
const POL_ZENURIK: &[u8] = include_bytes!("static/pol/Zenurik_Pol.svg");
const POL_UNAIRU: &[u8] = include_bytes!("static/pol/Unairu_Pol.svg");
const POL_PENJAGA: &[u8] = include_bytes!("static/pol/Penjaga_Pol.svg");
const POL_ANY: &[u8] = include_bytes!("static/pol/Any_Pol.png");

/// Serve a vendored polarity icon by filename.
fn pol_icon(file: &str) -> Option<(&'static [u8], &'static str)> {
    Some(match file {
        "Madurai_Pol.svg" => (POL_MADURAI, "image/svg+xml"),
        "Naramon_Pol.svg" => (POL_NARAMON, "image/svg+xml"),
        "Vazarin_Pol.svg" => (POL_VAZARIN, "image/svg+xml"),
        "Zenurik_Pol.svg" => (POL_ZENURIK, "image/svg+xml"),
        "Unairu_Pol.svg" => (POL_UNAIRU, "image/svg+xml"),
        "Penjaga_Pol.svg" => (POL_PENJAGA, "image/svg+xml"),
        "Umbra_Pol.svg" => (POL_UMBRA, "image/svg+xml"),
        "Any_Pol.png" => (POL_ANY, "image/png"),
        _ => return None,
    })
}

/// THE PUBLISHED BOARD, READ OFF DISK — `site/board*`, written by the scoring
/// bot and by `build_site_app.py`, never by this server.
///
/// SERVING THEM IS WHAT MAKES THE DEV SERVER THE SAME PAGE. Without these
/// paths a weapon page says nobody has submitted a build, the finder has
/// nothing to find and a board row cannot be opened — and the worse half is
/// silent: a board-dependent browser check pointed here PASSES, on a page
/// where the feature it checks was never drawn.
///
/// Read per request rather than embedded, because the bot rewrites these files
/// between builds and a stale copy compiled into the binary would be a second
/// source for rows that already have one.
fn board_response(stream: &mut TcpStream, path: &str) -> std::io::Result<()> {
    let rel = path.trim_start_matches('/');
    // One directory deep, `.json` only, and no segment that could climb: this
    // reads from the working tree, so the name decides which file is opened.
    let safe = rel.len() < 128
        && rel.ends_with(".json")
        && !rel.contains("..")
        && rel.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'));
    // A repo whose `site/` has never been generated is the ordinary case for a
    // fresh clone, and an empty board is a state the page already handles.
    let miss = |s: &mut TcpStream| respond(s, "404 Not Found", "text/plain; charset=utf-8", b"not found");
    if !safe {
        return miss(stream);
    }
    let file = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../site")).join(rel);
    match std::fs::read(&file) {
        Ok(bytes) => respond_asset(stream, "application/json; charset=utf-8", &bytes),
        Err(_) => miss(stream),
    }
}

/// Weapon/mod/arcane art: served from a local on-disk cache (web/cache/img/,
/// gitignored, pre-warmed by scripts/fetch_images.py) so it loads locally and
/// works offline. On a cache miss, 302-redirect to the WFCD CDN — so it always
/// works, and DE art never has to be committed to the repo. `name` is a bare
/// filename (traversal-guarded).
fn img_response(stream: &mut TcpStream, name: &str) -> std::io::Result<()> {
    // Parentheses admit the wiki's evolution-icon names ("…(xWhite).png").
    let safe = !name.is_empty()
        && name.len() < 128
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '(' | ')'));
    // `IMG()` asks for `<cached name>.webp` — the form `ship_art` derives for
    // the static deployment. THE CACHE HOLDS WHAT WAS DOWNLOADED, so strip the
    // suffix to get the cached name back exactly, and let the header carry the
    // format: a browser reads Content-Type, not the extension, so the original
    // bytes render under the derived name and dev needs no derived files.
    let name = name.strip_suffix(".webp").unwrap_or(name);
    if safe {
        let path = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/cache/img"))
            .join(name);
        if let Ok(bytes) = std::fs::read(&path) {
            let ct = if name.ends_with(".png") { "image/png" } else { "image/jpeg" };
            return respond_asset(stream, ct, &bytes);
        }
    }
    // Cache miss: wiki-hosted art (evolution icons) redirects to the wiki
    // file path; everything else to the WFCD CDN.
    if name.contains('(') {
        return respond_redirect(
            stream,
            &format!("https://wiki.warframe.com/w/Special:FilePath/{name}"),
        );
    }
    respond_redirect(stream, &format!("https://cdn.warframestat.us/img/{name}"))
}

/// THE METHOD A PURE ENDPOINT ANSWERS, which `api()` in app.js matches: these
/// two are GET and every other one is a POST. `wfsim_webapi::route` owns the
/// paths; the method is this transport's, and a wrong one falls through.
const GET_ENDPOINTS: &[&str] = &["/api/meta", "/api/i18n"];

fn endpoint_method(path: &str) -> &'static str {
    if GET_ENDPOINTS.contains(&path) {
        "GET"
    } else {
        "POST"
    }
}

fn handle(mut stream: TcpStream) -> std::io::Result<()> {
    let Some(req) = read_request(&stream)? else {
        return Ok(());
    };
    // Strip any query string.
    let path = req.path.split('?').next().unwrap_or("/");
    let body = serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
    if req.method == endpoint_method(path) {
        if let Some(out) = wfsim_webapi::route(path, &body) {
            return respond_json(&mut stream, &out);
        }
    }

    match (req.method.as_str(), path) {
        ("GET", "/") => respond(&mut stream, "200 OK", "text/html; charset=utf-8", INDEX_HTML.as_bytes()),
        ("GET", "/app.js") => respond(
            &mut stream,
            "200 OK",
            "text/javascript; charset=utf-8",
            APP_JS.as_bytes(),
        ),
        ("GET", "/style.css") => respond(&mut stream, "200 OK", "text/css; charset=utf-8", STYLE_CSS.as_bytes()),
        ("GET", "/logo.svg") => respond(&mut stream, "200 OK", "image/svg+xml", LOGO_SVG.as_bytes()),
        ("POST", "/api/optimize") => respond_json(&mut stream, &optimize_start(&body)),
        ("POST", "/api/optimize/status") => respond_json(&mut stream, &optimize_status(&body)),
        ("POST", "/api/optimize/cancel") => respond_json(&mut stream, &optimize_cancel(&body)),
        ("GET", p) if p.starts_with("/nona/") => match NONA_FILES.iter().find(|(f, _)| *f == &p[6..]) {
            Some((_, body)) => respond(&mut stream, "200 OK", "text/javascript; charset=utf-8", body.as_bytes()),
            None => respond(&mut stream, "404 Not Found", "text/plain; charset=utf-8", b"not found"),
        },
        ("GET", p) if p.starts_with("/pol/") => match pol_icon(&p[5..]) {
            Some((bytes, ct)) => respond_asset(&mut stream, ct, bytes),
            None => respond(&mut stream, "404 Not Found", "text/plain; charset=utf-8", b"not found"),
        },
        ("GET", p) if p.starts_with("/img/") => img_response(&mut stream, &p[5..]),
        ("GET", p) if p == "/board.json" || p == "/board.meta.json" || p.starts_with("/board/") => {
            board_response(&mut stream, p)
        }
        // SPA fallback, DERIVED RATHER THAN LISTED. Every page path is a
        // client-side route, so serve the shell and let app.js's router
        // resolve it — which is what the static deployment does
        // (not_found_handling = single-page-application). Naming the routes
        // here instead makes each new shell page 404 until someone adds it,
        // on the server that is the only thing anyone develops against.
        //
        // A DOT MEANS A FILE. An asset this server does not have must still
        // 404, or a mistyped script tag arrives as HTML and fails somewhere
        // else entirely; a page path has no extension.
        ("GET", p) if !p.rsplit('/').next().unwrap_or("").contains('.') => {
            respond(&mut stream, "200 OK", "text/html; charset=utf-8", INDEX_HTML.as_bytes())
        }
        _ => respond(&mut stream, "404 Not Found", "text/plain; charset=utf-8", b"not found"),
    }
}

// ---- /api/optimize job registry ----------------------------------------
//
// The search runs as a BACKGROUND JOB: POST /api/optimize validates the scope
// synchronously (bad input still fails fast — `parse_optimize`) and returns a
// `job_id`; a worker thread runs `run_optimize`, publishing live progress
// through the optimizer lib's `FunnelState`; the frontend polls POST
// /api/optimize/status and can POST /api/optimize/cancel. One job runs at a
// time — a single run already saturates every core via `evaluate_batch`, so a
// second concurrent run would only slow both down.

struct OptJob {
    id: u64,
    started: std::time::Instant,
    state: Arc<FunnelState>,
    /// "enumerating" → "running" → "done" | "cancelled" | "error".
    phase: Mutex<&'static str>,
    /// (candidates, jobs) — known once enumeration finishes.
    counts: Mutex<Option<(usize, usize)>>,
    /// The finished payload (exactly the old synchronous endpoint's JSON).
    /// A cancelled job still carries the last COMPLETED round's top-10 when
    /// at least one funnel round finished before the cancel.
    result: Mutex<Option<Value>>,
}

impl OptJob {
    fn active(&self) -> bool {
        matches!(*self.phase.lock().unwrap(), "enumerating" | "running")
    }
}

fn opt_jobs() -> &'static Mutex<Vec<Arc<OptJob>>> {
    static J: OnceLock<Mutex<Vec<Arc<OptJob>>>> = OnceLock::new();
    J.get_or_init(|| Mutex::new(Vec::new()))
}

/// Find a job by `id`, defaulting to the most recent one (lets the frontend
/// reattach after a page reload without persisting the id).
fn opt_job(v: &Value) -> Option<Arc<OptJob>> {
    let jobs = opt_jobs().lock().unwrap();
    match v.get("id").and_then(|x| x.as_u64()) {
        Some(id) => jobs.iter().find(|j| j.id == id).cloned(),
        None => jobs.last().cloned(),
    }
}

fn optimize_status(v: &Value) -> Value {
    let Some(j) = opt_job(v) else {
        return err_json("no such optimize job");
    };
    let phase = *j.phase.lock().unwrap();
    let counts = *j.counts.lock().unwrap();
    let mut out = funnel_status_json(&j.state, phase, counts, j.started.elapsed().as_secs_f64());
    out["job_id"] = json!(j.id);
    if let Some(r) = j.result.lock().unwrap().clone() {
        out["result"] = r;
    }
    out
}

fn optimize_cancel(v: &Value) -> Value {
    let Some(j) = opt_job(v) else {
        return err_json("no such optimize job");
    };
    // The flag is checked between funnel jobs AND inside enumeration (a
    // huge scope must stay cancellable); the worker flips the phase.
    j.state.cancel.store(true, Ordering::Relaxed);
    json!({ "ok": true, "job_id": j.id })
}

fn optimize_start(v: &Value) -> Value {
    {
        let jobs = opt_jobs().lock().unwrap();
        if let Some(j) = jobs.iter().find(|j| j.active()) {
            return json!({
                "ok": false,
                "error": "an optimization is already running — cancel it or wait",
                "job_id": j.id,
            });
        }
    }
    let plan = match parse_optimize(v) {
        Ok(p) => p,
        Err(e) => return e,
    };

    // ---- register the job and hand the heavy work to a worker thread ----
    static NEXT_JOB_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed);
    let job = Arc::new(OptJob {
        id,
        started: std::time::Instant::now(),
        state: Arc::new(FunnelState::default()),
        phase: Mutex::new("enumerating"),
        counts: Mutex::new(None),
        result: Mutex::new(None),
    });
    {
        let mut jobs = opt_jobs().lock().unwrap();
        jobs.push(job.clone());
        // Prune old finished jobs; the running one is never removed.
        while jobs.len() > 6 {
            match jobs.iter().position(|j| !j.active()) {
                Some(pos) => drop(jobs.remove(pos)),
                None => break,
            }
        }
    }

    let worker = job.clone();
    std::thread::spawn(move || {
        // The enumeration/producer runs on THIS thread — it must yield to
        // interactive work just like the evaluation workers do.
        wfsim_optimizer::deprioritize_current_thread();
        let result = run_optimize(
            plan,
            &worker.state,
            |cands, jobs| {
                *worker.counts.lock().unwrap() = Some((cands, jobs));
                *worker.phase.lock().unwrap() = "running";
            },
            None, // native: the status endpoint polls FunnelState instead
        );
        let ok = result.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
        let cancelled = result.get("cancelled").and_then(|x| x.as_bool()).unwrap_or(false);
        *worker.result.lock().unwrap() = Some(result);
        *worker.phase.lock().unwrap() = if !ok {
            "error"
        } else if cancelled {
            "cancelled"
        } else {
            "done"
        };
    });

    json!({ "ok": true, "job_id": id })
}
