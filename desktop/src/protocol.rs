//! Serving `current/` to the webview.
//!
//! A custom scheme rather than a localhost server, and the difference is a
//! firewall prompt on every install: a server listens on a port, this does
//! not. Measured through it, the 5.43 MB wasm module instantiates in 205 ms
//! (against 2.11 MB/s off the CDN, and eight worker lanes each asking for the
//! same file) — the entire reason the desktop build is worth shipping.
use std::io::Read;
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

/// Where the board lives. The client is served from `wfsim.localhost`, so the
/// page's own same-origin `/api/board/…` cannot reach it without this.
const BOARD_ORIGIN: &str = "https://wfsim.app";

/// The only paths forwarded to the network. NOT `/api/` — every other endpoint
/// is answered by the wasm engine inside the page, and forwarding a wider
/// prefix would turn this into an open proxy for whatever a link can address.
const PROXIED: &str = "/api/board/";

pub fn is_proxied(req: &Request<Vec<u8>>) -> bool {
    req.uri().path().starts_with(PROXIED)
}

/// Forward one board request and return what the server said.
///
/// THE BOARD IS A SERVICE, NOT A CALCULATION — the one thing the engine in the
/// page cannot answer, and `app.js` fetches it same-origin deliberately (a
/// second DNS name is a second thing that can be blocked). Inside this app
/// "same origin" is a custom protocol, so without this the SPA fallback answers
/// a submission with `index.html` and **HTTP 200**, which `res.ok` reads as
/// success: the page says 已发送 having sent nothing. That is the failure mode
/// `wrangler.jsonc` was already written about — "a 200 carrying the wrong
/// content type is the quietest possible failure" — reappearing one layer down.
///
/// A FAILURE HERE MUST LOOK LIKE A FAILURE. Anything that goes wrong answers
/// 502 with a JSON body, because the page's only test is `res.ok`.
pub fn proxy(req: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let url = format!("{BOARD_ORIGIN}{}", req.uri().path());
    let method = req.method().as_str().to_ascii_uppercase();
    let call = ureq::request(&method, &url)
        .timeout(std::time::Duration::from_secs(30))
        .set(
            "Content-Type",
            req.headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("application/json"),
        );

    let result = if method == "GET" {
        call.call()
    } else {
        call.send_bytes(req.body())
    };

    // ureq treats a 4xx/5xx as an error while carrying the response; the page
    // wants the server's own verdict, not this proxy's opinion of it.
    let resp = match result {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(e) => {
            return Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .header("Content-Type", "application/json")
                .body(format!("{{\"ok\":false,\"error\":\"{e}\"}}").into_bytes())
                .expect("proxy error response")
        }
    };

    let status = resp.status();
    let ctype = resp.content_type().to_string();
    let mut body = Vec::new();
    let mut reader = resp.into_reader();
    if std::io::Read::take(&mut reader, 4 * 1024 * 1024)
        .read_to_end(&mut body)
        .is_err()
    {
        return Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .header("Content-Type", "application/json")
            .body(br#"{"ok":false,"error":"truncated response"}"#.to_vec())
            .expect("proxy error response");
    }

    Response::builder()
        .status(StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY))
        .header("Content-Type", ctype)
        .body(body)
        .expect("proxy response")
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
