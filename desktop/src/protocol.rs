//! Serving `current/` to the webview.
//!
//! A custom scheme rather than a localhost server, and the difference is a
//! firewall prompt on every install: a server listens on a port, this does
//! not. Measured through it, the 5.43 MB wasm module instantiates in 205 ms
//! (against 2.11 MB/s off the CDN, and eight worker lanes each asking for the
//! same file) — the entire reason the desktop build is worth shipping.
use std::path::Path;

use tauri::http::{Request, Response, StatusCode};

/// `application/wasm` is not cosmetic: `WebAssembly.instantiateStreaming`
/// refuses anything else, and wasm-bindgen then falls back to buffering the
/// whole module — working, but paying for 5.43 MB twice.
fn mime_of(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "wasm" => "application/wasm",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

pub fn serve(root: &Path, req: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let raw = req.uri().path().trim_start_matches('/');
    let rel = percent_decode(if raw.is_empty() { "index.html" } else { raw });

    // THE SELFTEST REPORTS THROUGH THE PROTOCOL, not through Tauri's IPC.
    // A check that depends on IPC cannot tell "the page never ran" from "the
    // page ran and could not talk back", and those need different fixes.
    if let Some(rest) = rel.strip_prefix("__selftest__/") {
        crate::selftest_line(&percent_decode(rest));
        return Response::builder()
            .header("Content-Type", "text/plain")
            .body(b"ok".to_vec())
            .expect("selftest ack");
    }

    let body = crate::layout::safe_join(root, &rel).and_then(|p| std::fs::read(&p).ok());
    if std::env::var("WFSIM_TRACE").is_ok() {
        println!("[serve] {rel} -> {}", body.as_ref().map_or("MISS".into(), |b| format!("{} bytes", b.len())));
    }

    let (bytes, mime) = match body {
        Some(b) => (b, mime_of(&rel)),
        // SPA fallback — `/weapons/<Wiki_Name>` is a client-side route, so
        // anything that is not a real file is the shell, which mirrors the
        // CDN's `not_found_handling: single-page-application`.
        None => match std::fs::read(root.join("index.html")) {
            Ok(b) => (b, "text/html; charset=utf-8"),
            Err(e) => {
                return Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(format!("{e}").into_bytes())
                    .expect("error response")
            }
        },
    };

    Response::builder()
        .header("Content-Type", mime)
        // THE FILENAMES ARE FIXED (`app.js`, `wfsim_wasm_bg.wasm`), so after an
        // update swaps the directory the webview would happily serve its own
        // cached copy of the old one. Caching a local file buys nothing here
        // anyway — this is a disk read, not a network round trip.
        .header("Cache-Control", "no-store")
        .body(bytes)
        .expect("response")
}

/// Paths reach us encoded (`/weapons/Kuva_Nukor`, and any weapon whose wiki
/// name carries a space or an apostrophe).
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
