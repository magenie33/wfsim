//! Embed the `data/` tree at compile time (docs/WASM.md phase 1).
//!
//! Scans `../data` for `*.yaml` and generates `$OUT_DIR/embedded_data.rs`
//! with one `include_str!` entry per file, sorted by relative path with
//! forward slashes. `crate::data` includes the table; every loader reads
//! from it, so native and wasm builds carry the identical data set and no
//! binary depends on the current working directory.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|x| x == "yaml") {
            out.push(path);
        }
    }
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // No canonicalize: on Windows it yields `\\?\`-prefixed paths, which
    // `include_str!` rejects. The `engine/../data` form works everywhere.
    let data_root = manifest.join("../data");
    println!("cargo:rerun-if-changed={}", data_root.display());

    let mut files = Vec::new();
    collect(&data_root, &mut files);

    // (relative path with forward slashes, absolute path) — sorted by the
    // relative path so lookup order is deterministic across platforms.
    let mut entries: Vec<(String, String)> = files
        .iter()
        .map(|abs| {
            let rel = abs
                .strip_prefix(&data_root)
                .expect("under data/")
                .to_string_lossy()
                .replace('\\', "/");
            (rel, abs.to_string_lossy().replace('\\', "/"))
        })
        .collect();
    entries.sort();

    let mut src = String::from(
        "/// Every `data/**.yaml`, as (path relative to `data/`, contents), sorted by path.\n\
         pub static FILES: &[(&str, &str)] = &[\n",
    );
    for (rel, abs) in &entries {
        writeln!(src, "    ({rel:?}, include_str!({abs:?})),").unwrap();
    }
    src.push_str("];\n");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("embedded_data.rs");
    fs::write(&out, src).unwrap_or_else(|e| panic!("write {}: {e}", out.display()));
}
