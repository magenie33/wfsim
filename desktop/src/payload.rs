//! The copy of the app that ships INSIDE the binary, and the manifest for it.
//!
//! This is only ever the STARTING point. It is unpacked once, on first launch,
//! into a writable directory the updater then owns — packaged bytes are
//! read-only, and an app that can only run what it was compiled with is an app
//! that has to be reinstalled to be fixed. Everything after the first launch
//! reads that directory, never this.
use std::path::Path;

use sha2::Digest;

use serde::{Deserialize, Serialize};

/// Built by `build.rs`: u32 LE manifest length, manifest JSON, then every
/// file's bytes back to back in manifest order.
#[cfg(not(target_os = "macos"))]
const PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.bin"));

/// ON A MAC THE PAYLOAD IS A FILE IN THE BUNDLE, not a static: the binary is
/// universal, and a static would ship the payload once per slice. A DEBUG binary
/// run from cargo reads the copy its own build wrote; a release one never does,
/// so a bundle missing its payload fails its selftest instead of passing on a
/// file only the build machine has.
#[cfg(target_os = "macos")]
fn file() -> std::path::PathBuf {
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.join("Resources").join("payload.bin")));
    match bundled {
        Some(p) if p.is_file() || !cfg!(debug_assertions) => p,
        _ => concat!(env!("OUT_DIR"), "/payload.bin").into(),
    }
}

/// The manifest's own bytes, read without the files behind them.
#[cfg(not(target_os = "macos"))]
fn head() -> Vec<u8> {
    let len = u32::from_le_bytes(PAYLOAD[..4].try_into().expect("payload header")) as usize;
    PAYLOAD[4..4 + len].to_vec()
}
#[cfg(target_os = "macos")]
fn head() -> Vec<u8> {
    use std::io::Read;
    let mut f = std::fs::File::open(file()).expect("payload file");
    let mut len = [0u8; 4];
    f.read_exact(&mut len).expect("payload header");
    let mut head = vec![0u8; u32::from_le_bytes(len) as usize];
    f.read_exact(&mut head).expect("payload manifest");
    head
}

#[cfg(not(target_os = "macos"))]
fn whole() -> std::borrow::Cow<'static, [u8]> {
    PAYLOAD.into()
}
#[cfg(target_os = "macos")]
fn whole() -> std::borrow::Cow<'static, [u8]> {
    std::fs::read(file()).expect("payload file").into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// Forward-slashed path relative to the app root, e.g. `pkg/wfsim_wasm.js`.
    pub p: String,
    pub n: usize,
    /// Lowercase hex SHA-256. The updater compares these and nothing else.
    pub h: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// The commit `site/` was generated from. NOT a version number — this
    /// project releases too often for one to mean anything, and a commit is a
    /// fact rather than a decision (see docs/DESKTOP.md).
    pub version: String,
    pub files: Vec<Entry>,
    /// Where updates come from. Absent in the embedded copy — the first launch
    /// has no opinion about servers; the shipped default fills it in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
}

pub fn manifest() -> Manifest {
    serde_json::from_slice(&head()).expect("payload manifest")
}

/// WHICH EXECUTABLE'S CONTENT THIS IS: a digest of the embedded manifest's own
/// bytes. Not the commit — two builds of one commit from different trees carry
/// different files, and this has to tell them apart.
pub fn digest() -> String {
    format!("{:x}", sha2::Sha256::digest(head()))
}

/// Write the embedded app into `dir`, creating it. Any existing content is
/// left alone unless a file of the same name is in the payload.
pub fn unpack_to(dir: &Path) -> std::io::Result<Manifest> {
    let payload = whole();
    let len = u32::from_le_bytes(payload[..4].try_into().expect("payload header")) as usize;
    let manifest: Manifest = serde_json::from_slice(&payload[4..4 + len]).expect("payload manifest");
    let blob = &payload[4 + len..];
    let mut at = 0usize;
    for e in &manifest.files {
        let dest = dir.join(&e.p);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, &blob[at..at + e.n])?;
        at += e.n;
    }
    Ok(manifest)
}
