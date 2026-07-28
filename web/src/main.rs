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
use wfsim_webapi::{
    err_json, meta_json, opt_buffs_json, panel_json, parse_optimize, run_optimize, simulate_json,
};

// ---- Embedded static assets (self-contained binary) --------------------

const INDEX_HTML: &str = include_str!("static/index.html");
const APP_JS: &str = include_str!("static/app.js");
const STYLE_CSS: &str = include_str!("static/style.css");

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
const POL_ANY: &[u8] = include_bytes!("static/pol/Any_Pol.png");

/// Serve a vendored polarity icon by filename.
fn pol_icon(file: &str) -> Option<(&'static [u8], &'static str)> {
    Some(match file {
        "Madurai_Pol.svg" => (POL_MADURAI, "image/svg+xml"),
        "Naramon_Pol.svg" => (POL_NARAMON, "image/svg+xml"),
        "Vazarin_Pol.svg" => (POL_VAZARIN, "image/svg+xml"),
        "Umbra_Pol.svg" => (POL_UMBRA, "image/svg+xml"),
        "Any_Pol.png" => (POL_ANY, "image/png"),
        _ => return None,
    })
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

fn handle(mut stream: TcpStream) -> std::io::Result<()> {
    let Some(req) = read_request(&stream)? else {
        return Ok(());
    };
    // Strip any query string.
    let path = req.path.split('?').next().unwrap_or("/");

    match (req.method.as_str(), path) {
        ("GET", "/") => respond(&mut stream, "200 OK", "text/html; charset=utf-8", INDEX_HTML.as_bytes()),
        ("GET", "/app.js") => respond(
            &mut stream,
            "200 OK",
            "text/javascript; charset=utf-8",
            APP_JS.as_bytes(),
        ),
        ("GET", "/style.css") => respond(&mut stream, "200 OK", "text/css; charset=utf-8", STYLE_CSS.as_bytes()),
        ("GET", "/api/meta") => respond_json(&mut stream, &meta_json()),
        ("POST", "/api/simulate") => {
            let value = serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
            respond_json(&mut stream, &simulate_json(&value))
        }
        ("POST", "/api/optimize") => {
            let value = serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
            respond_json(&mut stream, &optimize_start(&value))
        }
        ("POST", "/api/optimize/status") => {
            let value = serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
            respond_json(&mut stream, &optimize_status(&value))
        }
        ("POST", "/api/optimize/cancel") => {
            let value = serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
            respond_json(&mut stream, &optimize_cancel(&value))
        }
        ("POST", "/api/opt-buffs") => {
            let value = serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
            respond_json(&mut stream, &opt_buffs_json(&value))
        }
        ("POST", "/api/panel") => {
            let value = serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
            respond_json(&mut stream, &panel_json(&value))
        }
        ("GET", p) if p.starts_with("/pol/") => match pol_icon(&p[5..]) {
            Some((bytes, ct)) => respond_asset(&mut stream, ct, bytes),
            None => respond(&mut stream, "404 Not Found", "text/plain; charset=utf-8", b"not found"),
        },
        ("GET", p) if p.starts_with("/img/") => img_response(&mut stream, &p[5..]),
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
    let st = &j.state;
    let notes: Vec<Value> = st
        .notes
        .lock()
        .unwrap()
        .iter()
        .map(|n| {
            json!({
                "round": n.round, "jobs": n.jobs, "runs": n.runs,
                "by_kills": n.by_kills, "kept": n.kept, "best": n.best, "ms": n.ms,
            })
        })
        .collect();
    let mut out = json!({
        "ok": true,
        "job_id": j.id,
        "phase": *j.phase.lock().unwrap(),
        "elapsed_s": j.started.elapsed().as_secs_f64(),
        "round": st.round.load(Ordering::Relaxed),
        "rounds": st.rounds.load(Ordering::Relaxed),
        "round_jobs": st.round_jobs.load(Ordering::Relaxed),
        "round_runs": st.round_runs.load(Ordering::Relaxed),
        "sims_done": st.sims_done.load(Ordering::Relaxed),
        "sims_planned": st.sims_planned.load(Ordering::Relaxed),
        "notes": notes,
    });
    if let Some((cands, jobs)) = *j.counts.lock().unwrap() {
        out["candidates"] = json!(cands);
        out["jobs"] = json!(jobs);
    }
    if let Some(r) = j.result.lock().unwrap().clone() {
        out["result"] = r;
    }
    out
}

fn optimize_cancel(v: &Value) -> Value {
    let Some(j) = opt_job(v) else {
        return err_json("no such optimize job");
    };
    // The flag is checked between jobs (not during enumeration, which is
    // bounded and quick relative to the funnel); the worker flips the phase.
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
        let result = run_optimize(plan, &worker.state, |cands, jobs| {
            *worker.counts.lock().unwrap() = Some((cands, jobs));
            *worker.phase.lock().unwrap() = "running";
        });
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
