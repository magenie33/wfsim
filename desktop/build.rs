//! Packs the client's slice of `site/` into the binary, with a manifest.
//!
//! WHAT GOES IN, AND WHAT DOES NOT. `site/` is 67 MB, and 38 MB of it exists
//! for CRAWLERS: `og/` is link-preview cards for chat apps, `weapons/` is one
//! prerendered HTML shell per weapon so a search engine sees text. A desktop
//! app has no crawler and no link preview, so both are dropped — the payload is
//! the 29 MB that a reader actually uses, and `img/` is most of it. The art
//! ships rather than streams because "installed means offline" is the whole
//! point of the client; a page that fetches its own pictures over the network
//! would put the CDN back on the critical path this app exists to leave.
//!
//! THE MANIFEST IS BUILT HERE, not written by hand, and it is the same shape
//! the update server serves. That is what lets the first launch and every
//! later update run the identical comparison: a released `current/` is
//! indistinguishable from an updated one.
use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};

/// WHAT STAYS OUT, read from the one file that declares it — `desktop/payload.lst`,
/// which `scripts/payload_manifest.py` reads too. Embedded rather than opened at
/// build time so a missing list is a compile error and not an empty payload.
const PAYLOAD_LIST: &str = include_str!("payload.lst");

/// Whether a path under `site/` (slash-separated) is left out — the list's
/// lines, and the generation rule the list's header states.
fn excluded(rel: &str, current: &dyn Fn(&str) -> Option<Vec<String>>) -> bool {
    for line in PAYLOAD_LIST.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let hit = if let Some(any) = line.strip_prefix('*') {
            rel.ends_with(any)
        } else if line.ends_with('/') {
            rel.starts_with(line)
        } else {
            rel == line
        };
        if hit {
            return true;
        }
    }
    // Each directory on the way down that names a generation admits only it.
    let segs: Vec<&str> = rel.split('/').collect();
    (0..segs.len()).any(|i| {
        current(&segs[..i].join("/"))
            .is_some_and(|names| segs[i] != "generation.json" && !names.iter().any(|n| n == segs[i]))
    })
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    // Sorted so the payload is byte-identical across builds of the same tree —
    // a reproducible archive makes "did anything change" answerable by hash.
    entries.sort_by_key(std::fs::DirEntry::path);
    for e in entries {
        let p = e.path();
        if p.is_dir() { walk(&p, out) } else { out.push(p) }
    }
}

fn main() {
    tauri_build::build();

    let site = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("site");
    // THE MODULE IS NAMED BY ITS DIGEST (`ship_wasm_pkg`), so the check is that
    // `pkg/` holds one — a fixed name would pass on a stale build and fail on
    // every fresh one.
    let has_wasm = std::fs::read_dir(site.join("pkg")).is_ok_and(|d| {
        d.filter_map(Result::ok)
            .any(|e| e.path().extension().is_some_and(|x| x == "wasm"))
    });
    if !has_wasm {
        panic!("site/ is not built — run `python scripts/build_site_app.py` first");
    }
    println!("cargo:rerun-if-changed=../site");

    let generation = |dir: &str| -> Option<Vec<String>> {
        let body = std::fs::read(site.join(dir).join("generation.json")).ok()?;
        let v: serde_json::Value = serde_json::from_slice(&body).ok()?;
        Some(v["names"].as_array()?.iter().filter_map(|n| n.as_str().map(str::to_string)).collect())
    };
    let mut all = Vec::new();
    walk(&site, &mut all);
    let files: Vec<PathBuf> = all
        .into_iter()
        .filter(|p| {
            let rel = p.strip_prefix(&site).unwrap().to_string_lossy().replace(char::from(92), "/");
            !excluded(&rel, &generation)
        })
        .collect();

    let mut index = Vec::new();
    let mut blob = Vec::new();
    for path in &files {
        let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let rel = path.strip_prefix(&site).unwrap().to_string_lossy().replace(char::from(92), "/");
        index.push(serde_json::json!({
            "p": rel,
            "n": bytes.len(),
            "h": format!("{:x}", Sha256::digest(&bytes)),
        }));
        blob.extend_from_slice(&bytes);
    }

    let version = std::process::Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "nogit".into());

    // WHICH SHELL THIS IS — the one identifier no update can move, because
    // content replaces itself and the binary does not. It carries both halves
    // for two readers: the DATE is what a person compares at a glance ("mine
    // is from August"), the COMMIT is what a bug report needs. The payload's
    // `version` below is a different fact — the release this binary unpacks —
    // and after the first update it is no longer even this one.
    let conf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    println!("cargo:rerun-if-changed={}", conf.display());
    let date = std::fs::read_to_string(&conf)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|c| c["version"].as_str().map(str::to_string))
        .unwrap_or_else(|| "undated".into());
    println!("cargo:rustc-env=WFSIM_SHELL_BUILD={date} {version}");

    // THE PAGES THE SITE'S WORKER FILLS, read from the worker's own list: the
    // shell asks the site for them (`protocol::SITE_PAGES`), and a copy of the
    // list kept here would be a second answer to "which pages are the site's".
    let docs_js = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../worker/ext_documents.js");
    println!("cargo:rerun-if-changed={}", docs_js.display());
    let docs = std::fs::read_to_string(&docs_js).unwrap_or_else(|e| panic!("{}: {e}", docs_js.display()));
    let set = docs
        .split_once("EXT_DOCUMENTS = new Set([")
        .and_then(|(_, rest)| rest.split_once("])"))
        .map(|(list, _)| list)
        .unwrap_or_else(|| panic!("{}: no `EXT_DOCUMENTS = new Set([…])`", docs_js.display()));
    let pages: Vec<&str> = set.split(',').map(|s| s.trim().trim_matches('"')).filter(|s| s.starts_with('/')).collect();
    assert!(!pages.is_empty(), "{}: EXT_DOCUMENTS names no page", docs_js.display());
    println!("cargo:rustc-env=WFSIM_SITE_PAGES={}", pages.join(","));

    let manifest = serde_json::json!({ "version": version, "files": index });
    let head = serde_json::to_vec(&manifest).unwrap();

    // Format: u32 LE manifest length, the manifest JSON, then every file's
    // bytes back to back in manifest order. No compression — the NSIS
    // installer LZMAs the whole binary anyway, so compressing here would pay
    // twice and buy nothing.
    let mut out = Vec::with_capacity(4 + head.len() + blob.len());
    out.extend_from_slice(&(head.len() as u32).to_le_bytes());
    out.extend_from_slice(&head);
    out.extend_from_slice(&blob);

    let dest = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("payload.bin");
    std::fs::write(&dest, &out).unwrap();

    // THE FILE LIST IS DECLARED ONCE — here — and the release job reads it back
    // out rather than keeping a second copy of the exclusions. Two
    // lists that must agree are two lists that will not: a file added to one
    // and not the other is a client that either downloads something it cannot
    // use or is missing something it needs, and neither fails loudly.
    let shared = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&shared).ok();
    std::fs::write(shared.join("payload-manifest.json"), &head).ok();

    println!("cargo:warning=payload: {} files, {:.1} MB", files.len(), out.len() as f64 / 1e6);
}
