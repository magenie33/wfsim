//! Serving `current/` to the webview.
//!
//! A custom scheme rather than a localhost server, and the difference is a
//! firewall prompt on every install: a server listens on a port, this does
//! not. Measured through it, the 5.43 MB wasm module instantiates in 205 ms
//! (against 2.11 MB/s off the CDN, and eight worker lanes each asking for the
//! same file) — the entire reason the desktop build is worth shipping.
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};
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

/// THE PATHS SERVED FROM THIS SHELL'S OWN CACHE, and never from `current/`.
///
/// The board is 4.8 MB across 387 files and is rescored once an hour; a release
/// is code and moves on a code change. So it does not travel in one — it is
/// fetched, kept beside the release, and survives an update instead of being
/// replaced by it. docs/DISTRIBUTION.md §The data plane.
///
/// A PREFIX AND NOT A LIST, because the board is a directory now. Matched
/// strictly: one segment, a `.json`, and nothing that could climb out of `live/`
/// — the path comes off a URL and `safe_join` is not reached on this branch.
pub fn live_path(rel: &str) -> Option<&str> {
    if rel == "board.meta.json" {
        return Some(rel);
    }
    let stem = rel.strip_prefix("board/")?.strip_suffix(".json")?;
    let ok = !stem.is_empty()
        && stem.len() <= 64
        && stem.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_');
    ok.then_some(rel)
}

/// ONE PUBLISHED FILE'S DIGEST, by stem, as `scripts/board_meta.py` wrote it.
type Manifest = std::collections::BTreeMap<String, String>;

/// THE BOARD'S IDENTITY, computed the way the stamp computes it: a line per file
/// over the sorted manifest. Recomputed here rather than trusted, so the value
/// written beside the payload cannot disagree with the payload.
fn digest_of(files: &Manifest) -> String {
    let mut h = Sha256::new();
    for (name, sha) in files {
        h.update(format!("{name} {sha}\n"));
    }
    format!("{:x}", h.finalize())
}

fn manifest_of(meta: &[u8]) -> Option<(Manifest, String)> {
    let v = serde_json::from_slice::<serde_json::Value>(meta).ok()?;
    let want = v.get("digest")?.as_str()?.to_owned();
    let files = v
        .get("files")?
        .as_object()?
        .iter()
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_owned())))
        .collect::<Manifest>();
    // THE STAMP MUST NAME THE MANIFEST IT CARRIES. Anything else is a stamp
    // written by something this shell does not understand, and a board is only
    // as trustworthy as the one value that says what it is.
    (!files.is_empty() && digest_of(&files) == want).then_some((files, want))
}

/// Fetch the files of the board that are not the ones being served.
///
/// THE STAMP IS ASKED FIRST, and that is the whole economy of it: a few hundred
/// bytes decide what is worth fetching, and most of the time most clients learn
/// they already have all of it. A rescore that moved twenty weapons then costs
/// twenty small files rather than the board.
///
/// NOTHING IS WRITTEN THAT DOES NOT HASH TO WHAT THE STAMP PROMISED. A board is
/// public data and needs no signature — anyone can recompute it — but a
/// truncated download is not a smaller board, it is a broken one.
///
/// THE STAMP LANDS LAST, and only if every file it named landed. A run that dies
/// partway leaves a shell holding some files from before and some from after,
/// with a stamp still naming the board it had — so the next check fetches the
/// rest. Writing the stamp first would make that gap invisible for ever.
pub fn refresh_board(live: &Path) -> Result<Option<String>, String> {
    let meta = fetch(&format!("{BOARD_ORIGIN}/board.meta.json"))?;
    let (files, want) = manifest_of(&meta).ok_or("board.meta.json names no manifest")?;

    let dir = live.join("board");
    let mut fetched = 0usize;
    for (name, sha) in &files {
        let at = dir.join(format!("{name}.json"));
        let have = std::fs::read(&at).ok().map(|b| format!("{:x}", Sha256::digest(&b)));
        if have.as_deref() == Some(sha.as_str()) {
            continue;
        }
        let body = fetch(&format!("{BOARD_ORIGIN}/board/{name}.json"))?;
        let got = format!("{:x}", Sha256::digest(&body));
        // A PUBLISH BETWEEN THE STAMP AND THIS FILE IS A TORN READ, not a
        // corrupt one, and it is the ordinary case here: the loop runs for as
        // long as the files take. So it is not an error — the stamp is asked
        // again on the next pass and this file is fetched against the newer one.
        if got != *sha {
            return Ok(None);
        }
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        write_atomic(&at, &body)?;
        fetched += 1;
    }
    // …AND THE ONES THE BOARD NO LONGER HAS. A weapon dropped from the roster
    // leaves a file this shell would go on serving rows from.
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Some(stem) = name.strip_suffix(".json") {
                if !files.contains_key(stem) {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
    }
    let stamped = std::fs::read(live.join("board.meta.json"))
        .ok()
        .and_then(|b| manifest_of(&b).map(|(_, d)| d));
    if fetched == 0 && stamped.as_deref() == Some(want.as_str()) {
        return Ok(None);
    }
    write_atomic(&live.join("board.meta.json"), &meta)?;
    Ok(Some(want))
}

fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(60))
        .call()
        .map_err(|e| format!("{url}: {e}"))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .take(32 * 1024 * 1024)
        .read_to_end(&mut buf)
        .map_err(|e| format!("{url}: {e}"))?;
    Ok(buf)
}

/// Written beside the target and renamed over it, so a reader never sees half
/// a board: the page fetches these while this is running.
fn write_atomic(path: &Path, body: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, body).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn serve(root: &Path, live: &Path, req: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let raw = req.uri().path().trim_start_matches('/');
    let rel = percent_decode(if raw.is_empty() { "index.html" } else { raw });

    // LIVE DATA, AND A MISS IS A 404 — never the SPA fallback. `index.html`
    // answered with a 200 is what `res.ok` reads as success, and the page would
    // then parse markup as a board. An honest miss is what sends the page to
    // the site instead (`fetchJson`), which is how a client that has not
    // fetched one yet still has a board to show.
    if let Some(rel) = live_path(&rel) {
        return match std::fs::read(live.join(rel)) {
            Ok(b) => Response::builder()
                .header("Content-Type", "application/json; charset=utf-8")
                .header("Cache-Control", "no-store")
                .body(b)
                .expect("live response"),
            Err(_) => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .header("Content-Type", "application/json; charset=utf-8")
                .body(br#"{"error":"not fetched yet"}"#.to_vec())
                .expect("live miss"),
        };
    }

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
        // `index.html` AND `app.js` KEEP THEIR NAMES across an update, so the
        // webview would happily serve its own cached copy of the old one after
        // the directory is swapped. Caching a local file buys nothing here
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

#[cfg(test)]
mod tests {
    use super::*;

    /// THE PATH COMES OFF A URL AND IS JOINED TO A DIRECTORY, so what this
    /// accepts is the whole of the guard: `safe_join` is not reached on the live
    /// branch. A weapon id is a lowercase slug off the roster and nothing else
    /// is a board file.
    #[test]
    fn only_a_weapon_file_is_served_from_the_live_directory() {
        for ok in ["board.meta.json", "board/braton_prime.json", "board/index.json"] {
            assert_eq!(live_path(ok), Some(ok), "{ok} should be live");
        }
        for no in [
            "board/../../secrets.json",
            "board/..%2F..%2Fsecrets.json",
            "board/sub/dir.json",
            "board/Braton_Prime.json",
            "board/.json",
            "board/braton prime.json",
            "board/braton_prime.txt",
            "board.json",
            "index.html",
            "",
        ] {
            assert_eq!(live_path(no), None, "{no} must not be served from live/");
        }
    }

    /// A STAMP THAT DOES NOT DESCRIBE ITS OWN MANIFEST IS NOT A STAMP. The
    /// digest is the one value saying which board this is, and a client that
    /// took the file list without checking it would fetch whatever a truncated
    /// or edited stamp happened to name.
    #[test]
    fn a_stamp_is_refused_unless_it_matches_the_manifest_it_carries() {
        let files: Manifest =
            [("index".to_string(), "aa".to_string()), ("braton".to_string(), "bb".to_string())]
                .into_iter()
                .collect();
        let good = format!(
            r#"{{"digest":"{}","files":{{"index":"aa","braton":"bb"}}}}"#,
            digest_of(&files)
        );
        let (got, want) = manifest_of(good.as_bytes()).expect("a matching stamp is taken");
        assert_eq!(got, files);
        assert_eq!(want, digest_of(&files));

        // …AND THE ORDER IS THE SORTED ONE, so two clients reading the same
        // board cannot compute two digests for it.
        let reversed: Manifest =
            [("braton".to_string(), "bb".to_string()), ("index".to_string(), "aa".to_string())]
                .into_iter()
                .collect();
        assert_eq!(digest_of(&reversed), digest_of(&files));

        for bad in [
            r#"{"digest":"0000","files":{"index":"aa","braton":"bb"}}"#,
            r#"{"files":{"index":"aa"}}"#,
            r#"{"digest":"0000","files":{}}"#,
            "not json",
        ] {
            assert!(manifest_of(bad.as_bytes()).is_none(), "{bad} must be refused");
        }
    }

    /// ONE FILE MOVING IS ONE DIGEST MOVING, which is what makes a refresh cost
    /// the weapons that changed rather than the board.
    #[test]
    fn a_weapon_moving_moves_the_board_digest_and_only_its_own() {
        let mut files: Manifest =
            [("a".to_string(), "1".to_string()), ("b".to_string(), "2".to_string())]
                .into_iter()
                .collect();
        let before = digest_of(&files);
        files.insert("b".to_string(), "3".to_string());
        assert_ne!(digest_of(&files), before);
        assert_eq!(files["a"], "1");
    }
}
