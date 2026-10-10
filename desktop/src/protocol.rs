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

/// HOW MANY BOARD FILES ARE FETCHED AT ONCE. A file is a few KB and costs a
/// round trip — ~1.25 s each from Shanghai to `wfsim.app` — so six hundred one
/// at a time is a quarter of an hour; eight lanes land the whole board in
/// about a minute.
const BOARD_LANES: usize = 8;

/// Where the site lives. The client is served from `wfsim.localhost`, so the
/// page's own same-origin `/api/…` cannot reach it without the proxy below.
pub const SITE_ORIGIN: &str = "https://wfsim.app";

/// WHAT IS FORWARDED: the site's whole api, and nothing outside it. Every
/// engine call is answered by the wasm inside the page and never reaches the
/// protocol, so what arrives here is the SITE — the board, the account, cloud
/// sync, the extension script — and leaving any of it out is a feature the
/// browser has and the client silently does not. The target is fixed, so a
/// link cannot address anything but `wfsim.app`.
const PROXIED: &str = "/api/";

/// …AND THE PAGES THE SITE'S WORKER FILLS (`worker/ext_documents.js`, read at
/// build time). The privacy, terms and refunds pages carry sections the
/// worker writes in; the copy in `site/` has empty slots where those are, so
/// served from disk the client would show terms that are not the site's.
const SITE_PAGES: &str = env!("WFSIM_SITE_PAGES");

pub fn is_proxied(req: &Request<Vec<u8>>) -> bool {
    let path = req.uri().path();
    path.starts_with(PROXIED) || SITE_PAGES.split(',').any(|p| path.trim_end_matches('/') == p)
}

/// The headers worth forwarding. NOT `Origin`: the page's is
/// `wfsim.localhost`, which the site's `sameSite` refuses, and a request this
/// shell sends on the page's behalf is the site's own client.
const FORWARDED: [&str; 4] = ["content-type", "accept", "accept-language", "user-agent"];

/// A redirect is never followed here: the WEBVIEW follows it, so a sign-in
/// that leaves for a provider leaves in the window and its cookies stay ours.
fn agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .redirects(0)
            .timeout(std::time::Duration::from_secs(30))
            .build()
    })
}

/// A redirect back to the site is a redirect back INTO the app.
pub fn local_target(location: &str) -> String {
    match location.strip_prefix(SITE_ORIGIN) {
        Some(rest) if rest.is_empty() || rest.starts_with('/') || rest.starts_with('?') => {
            if rest.starts_with('/') { rest.to_string() } else { format!("/{rest}") }
        }
        _ => location.to_string(),
    }
}

fn failed(status: StatusCode, error: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(serde_json::json!({ "ok": false, "error": error }).to_string().into_bytes())
        .expect("proxy error response")
}

/// Forward one request to the site and return what it said.
///
/// THE SITE IS A SERVICE, NOT A CALCULATION — the one thing the engine in the
/// page cannot answer, and `app.js` fetches it same-origin deliberately (a
/// second DNS name is a second thing that can be blocked). Without this the SPA
/// fallback answers with `index.html` and **HTTP 200**, which `res.ok` reads
/// as success — and `/api/account` answered that way is a page that says
/// accounts are not available here.
///
/// A FAILURE HERE MUST LOOK LIKE A FAILURE. Anything that goes wrong answers
/// 502 with a JSON body, because the page's only test is `res.ok`.
pub fn proxy(req: &Request<Vec<u8>>, jar: &crate::session::Jar) -> Response<Vec<u8>> {
    let path = req.uri().path_and_query().map_or(req.uri().path(), |p| p.as_str());
    let url = format!("{SITE_ORIGIN}{path}");
    let method = req.method().as_str().to_ascii_uppercase();
    let mut call = agent().request(&method, &url);
    for name in FORWARDED {
        if let Some(v) = req.headers().get(name).and_then(|v| v.to_str().ok()) {
            call = call.set(name, v);
        }
    }
    if let Some(c) = jar.header() {
        call = call.set("Cookie", &c);
    }

    let result = if method == "GET" || method == "HEAD" {
        call.call()
    } else {
        call.send_bytes(req.body())
    };

    // ureq treats a 4xx/5xx as an error while carrying the response; the page
    // wants the server's own verdict, not this proxy's opinion of it.
    let resp = match result {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(e) => return failed(StatusCode::BAD_GATEWAY, &e.to_string()),
    };
    jar.take(resp.all("set-cookie"));

    let status = resp.status();
    if (300..400).contains(&status) {
        let to = local_target(resp.header("location").unwrap_or("/"));
        // A NAVIGATION IS REDIRECTED BY A PAGE, not by a 302: a redirect
        // answered through a custom protocol is not one every webview
        // follows, and a sign-in that stops on a blank page is a sign-in that
        // failed with nothing to say so.
        let navigating = req
            .headers()
            .get("accept")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|a| a.contains("text/html"));
        if navigating {
            let to = serde_json::Value::from(to).to_string();
            return Response::builder()
                .header("Content-Type", "text/html; charset=utf-8")
                .header("Cache-Control", "no-store")
                .body(format!("<!doctype html><meta charset=utf-8><script>location.replace({to})</script>").into_bytes())
                .expect("redirect page");
        }
        return Response::builder()
            .status(StatusCode::from_u16(status).unwrap_or(StatusCode::FOUND))
            .header("Location", to)
            .body(Vec::new())
            .expect("redirect response");
    }

    let ctype = resp.content_type().to_string();
    let mut body = Vec::new();
    let mut reader = resp.into_reader();
    if std::io::Read::take(&mut reader, 16 * 1024 * 1024)
        .read_to_end(&mut body)
        .is_err()
    {
        return failed(StatusCode::BAD_GATEWAY, "truncated response");
    }

    Response::builder()
        .status(StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_GATEWAY))
        .header("Content-Type", ctype)
        .header("Cache-Control", "no-store")
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
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let meta = fetch(&format!("{SITE_ORIGIN}/board.meta.json"))?;
    let (files, want) = manifest_of(&meta).ok_or("board.meta.json names no manifest")?;

    let dir = live.join("board");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let due: Vec<(&String, &String)> = files
        .iter()
        .filter(|(name, sha)| {
            let have = std::fs::read(dir.join(format!("{name}.json")))
                .ok()
                .map(|b| format!("{:x}", Sha256::digest(&b)));
            have.as_deref() != Some(sha.as_str())
        })
        .collect();
    // ONE FILE FAILING DOES NOT STOP THE REST. Every file that lands is kept,
    // and the stamp waits for the next pass to fetch what did not.
    let (next, fetched, torn) = (AtomicUsize::new(0), AtomicUsize::new(0), AtomicBool::new(false));
    let failed = std::sync::Mutex::new(Vec::<String>::new());
    std::thread::scope(|s| {
        for _ in 0..BOARD_LANES.min(due.len()) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some((name, sha)) = due.get(i) else { break };
                let landed = fetch(&format!("{SITE_ORIGIN}/board/{name}.json")).and_then(|body| {
                    // A PUBLISH BETWEEN THE STAMP AND THIS FILE IS A TORN READ,
                    // not a corrupt one: the next pass fetches it against the
                    // newer stamp.
                    if format!("{:x}", Sha256::digest(&body)) != **sha {
                        torn.store(true, Ordering::Relaxed);
                        return Ok(());
                    }
                    write_atomic(&dir.join(format!("{name}.json")), &body)?;
                    fetched.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                });
                if let Err(e) = landed {
                    failed.lock().expect("failure list").push(e);
                }
            });
        }
    });
    if torn.load(Ordering::Relaxed) {
        return Ok(None);
    }
    let failed = failed.into_inner().expect("failure list");
    if let Some(first) = failed.first() {
        return Err(format!("{} of {} board files did not land, first {first}", failed.len(), due.len()));
    }
    let fetched = fetched.into_inner();
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

    // A MISSING FILE IS A 404, never the SPA: `index.html` served under a
    // script's name is executed as one, and the page's boot reports a syntax
    // error as "WFSim could not start". The SPA answers only for a ROUTE.
    if body.is_none() && is_file_path(&rel) {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-Type", "text/plain; charset=utf-8")
            .header("Cache-Control", "no-store")
            .body(format!("{rel}: not in this copy of the app").into_bytes())
            .expect("missing file");
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

/// Whether a path names a FILE rather than a route: its last segment carries
/// a type this protocol serves. Routes are wiki names, which carry none.
fn is_file_path(rel: &str) -> bool {
    let last = rel.rsplit('/').next().unwrap_or("");
    last.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty() && ext != "html" && (mime_of(last) != "application/octet-stream" || ext == "onnx" || ext == "txt")
    })
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

    /// THE SITE SENDING A READER HOME SENDS THEM HOME IN THE APP, and nowhere
    /// else is touched: a provider's sign-in page is still the provider's.
    #[test]
    fn a_redirect_to_the_site_lands_in_the_app() {
        assert_eq!(local_target("https://wfsim.app/account?auth=signed_in"), "/account?auth=signed_in");
        assert_eq!(local_target("https://wfsim.app"), "/");
        assert_eq!(local_target("https://wfsim.app?auth=linked"), "/?auth=linked");
        assert_eq!(local_target("/login?auth_error=state"), "/login?auth_error=state");
        for away in ["https://discord.com/oauth2/authorize?x=1", "https://wfsim.app.example.com/"] {
            assert_eq!(local_target(away), away);
        }
    }

    /// A FILE THE APP DOES NOT HAVE IS NOT A ROUTE, and a route is not a file.
    #[test]
    fn only_a_route_falls_back_to_the_app() {
        for file in ["lib/ua-parser-1.0.41.min.js", "asset/app.0123.js", "pkg/x_bg.wasm", "ocr/a.onnx",
                     "ocr/ppocr-keys-v1.txt", "img/w/braton.webp", "style.css"] {
            assert!(is_file_path(file), "{file} is a file");
        }
        for route in ["weapons/Kuva_Nukor", "download", "", "weapons/Braton", "account/billing", "index.html"] {
            assert!(!is_file_path(route), "{route} is a route");
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
