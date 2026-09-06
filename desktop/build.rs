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

/// WHAT GOES IN, read from the one file that declares it — `desktop/payload.lst`,
/// which `scripts/payload_manifest.py` reads too. Embedded rather than opened at
/// build time so a missing list is a compile error and not an empty payload.
const PAYLOAD_LIST: &str = include_str!("payload.lst");

/// The list, split into single files and whole trees. A trailing `/` is a tree.
fn declared() -> (Vec<String>, Vec<String>) {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for line in PAYLOAD_LIST.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match line.strip_suffix('/') {
            Some(d) => dirs.push(d.to_string()),
            None => files.push(line.to_string()),
        }
    }
    (files, dirs)
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

    let (root_files, root_dirs) = declared();
    let mut files: Vec<PathBuf> = root_files.iter().map(|f| site.join(f)).collect();
    for d in &root_dirs {
        walk(&site.join(d), &mut files);
    }

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
    // out rather than keeping a second copy of `ROOT_FILES`/`ROOT_DIRS`. Two
    // lists that must agree are two lists that will not: a file added to one
    // and not the other is a client that either downloads something it cannot
    // use or is missing something it needs, and neither fails loudly.
    let shared = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target");
    std::fs::create_dir_all(&shared).ok();
    std::fs::write(shared.join("payload-manifest.json"), &head).ok();

    println!("cargo:warning=payload: {} files, {:.1} MB", files.len(), out.len() as f64 / 1e6);
}
