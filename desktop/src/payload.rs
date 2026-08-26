//! The copy of the app that ships INSIDE the binary, and the manifest for it.
//!
//! This is only ever the STARTING point. It is unpacked once, on first launch,
//! into a writable directory the updater then owns — packaged bytes are
//! read-only, and an app that can only run what it was compiled with is an app
//! that has to be reinstalled to be fixed. Everything after the first launch
//! reads that directory, never this.
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Built by `build.rs`: u32 LE manifest length, manifest JSON, then every
/// file's bytes back to back in manifest order.
const PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.bin"));

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

fn split() -> (Manifest, &'static [u8]) {
    let len = u32::from_le_bytes(PAYLOAD[..4].try_into().expect("payload header")) as usize;
    let manifest = serde_json::from_slice(&PAYLOAD[4..4 + len]).expect("payload manifest");
    (manifest, &PAYLOAD[4 + len..])
}

pub fn manifest() -> Manifest {
    split().0
}

/// Write the embedded app into `dir`, creating it. Any existing content is
/// left alone unless a file of the same name is in the payload.
pub fn unpack_to(dir: &Path) -> std::io::Result<Manifest> {
    let (manifest, blob) = split();
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
