// SPDX-License-Identifier: AGPL-3.0-or-later
//! Join `static/app/*.js` into the one `app.js` the page loads.
//!
//! The parts are fragments of ONE classic script, concatenated in filename
//! order with nothing between them — the filename prefix is the only statement
//! of order, and `scripts/build_site_app.py` and `scripts/app_source.mjs` join
//! them the same way. Generates `$OUT_DIR/app_js.rs` holding `APP_JS`.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // No canonicalize: on Windows it yields `\\?\`-prefixed paths, which
    // `include_str!` rejects.
    let dir = manifest.join("src/static/app");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut parts: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "js"))
        .collect();
    parts.sort();
    assert!(!parts.is_empty(), "{} holds no parts", dir.display());

    let mut src = String::from(
        "/// `app.js`: every `static/app/*.js`, joined in filename order.\n\
         pub const APP_JS: &str = concat!(\n",
    );
    for p in &parts {
        println!("cargo:rerun-if-changed={}", p.display());
        let abs = p.to_string_lossy().replace('\\', "/");
        writeln!(src, "    include_str!({abs:?}),").unwrap();
    }
    src.push_str(");\n");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("app_js.rs");
    fs::write(&out, src).unwrap_or_else(|e| panic!("write {}: {e}", out.display()));
}
