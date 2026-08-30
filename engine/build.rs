//! Embed the `data/` tree at compile time (docs/WASM.md phase 1).
//!
//! Scans `../data` for `*.yaml` and generates `$OUT_DIR/embedded_data.rs` with
//! one `include_str!` entry per file, so native and wasm builds carry the
//! identical data set and no binary depends on the working directory.
//!
//! WHAT IS EMBEDDED IS THE DATA, NOT THE PROSE. `data/` is **43% comments and
//! blank lines**, because this repo makes every value cite its source — a rule
//! about the REPO rather than about the artefact players download. Each file is
//! stripped of its full-line comments on the way into `$OUT_DIR`, which
//! AGENTS.md's own rule makes safe: *"YAML fields are consumed data;
//! narrative/prose belongs in comments"*. MEASURED on the wire at **-22%** of
//! what a visitor downloads, against `wasm-opt -Oz`'s -0.3%.
//!
//! A LINE IS DROPPED OR KEPT BYTE FOR BYTE, because several loaders read the
//! embedded text as LINES rather than through serde, so re-emitting the yaml
//! through a parser would rewrite the quoting they stand on. Inline comments
//! are left alone for the same reason, and THE BLOCK-SCALAR ARM IS DEFENSIVE:
//! a `#` inside a `|`/`>` block is CONTENT, though removing that arm changes
//! the meaning of ZERO of today's 1,607 files.
//!
//! THE BUILD PROVES IT RATHER THAN A TEST: every file is parsed before and
//! after and the two `Value`s must be equal, so a stripper that changed a
//! meaning cannot compile. `serde_norway` also refuses a DUPLICATE KEY.

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

/// Does this line open a block scalar — `key: |`, `- >-`, `key: |2+`?
///
/// DEFENSIVE: no block in `data/` currently contains a line that would be
/// mistaken for a comment (see the module header), so this arm protects a file
/// written later rather than one that exists.
///
/// The indicator is the last token on the line: `|` or `>`, then an optional
/// indentation digit, then an optional chomping `-`/`+`. Anything after it
/// would be a syntax error in YAML, so matching the tail is exact rather than
/// approximate.
fn opens_block_scalar(line: &str) -> bool {
    let t = line.trim_end();
    let t = t.strip_suffix(['-', '+']).unwrap_or(t);
    let t = t.trim_end_matches(|c: char| c.is_ascii_digit());
    t.ends_with('|') || t.ends_with('>')
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Drop full-line comments and blank lines; every other line survives byte for
/// byte. Lines inside a block scalar are content and are never touched.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    // The indentation of the line that OPENED the block we are inside, if any.
    // A block's content is whatever is indented further than its opener.
    let mut block: Option<usize> = None;
    for line in text.lines() {
        if let Some(open_at) = block {
            // A blank line inside a block is content under `|+`, and harmless
            // under the others — so it is kept rather than guessed about.
            if line.trim().is_empty() || indent_of(line) > open_at {
                out.push_str(line);
                out.push('\n');
                continue;
            }
            block = None;
        }
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if opens_block_scalar(line) {
            block = Some(indent_of(line));
        }
        out.push_str(line);
        out.push('\n');
    }
    out
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

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let lean_root = out_dir.join("data");

    let mut src = String::from(
        "/// Every `data/**.yaml`, as (path relative to `data/`, contents), sorted by path.\n\
         ///\n\
         /// The contents are the source file with its full-line COMMENTS removed\n\
         /// (see `build.rs`): 43% of `data/` is prose citing sources, which is a\n\
         /// rule about the repo and not about what a browser downloads. Every\n\
         /// line that survives is byte for byte the source's, and the build\n\
         /// proves each file still parses to the same value.\n\
         pub static FILES: &[(&str, &str)] = &[\n",
    );
    for (rel, abs) in &entries {
        let raw = fs::read_to_string(abs).unwrap_or_else(|e| panic!("read {abs}: {e}"));
        let lean = strip_comments(&raw);

        // THE PROOF, at build time. A file whose meaning moved cannot compile.
        let before: serde_norway::Value = serde_norway::from_str(&raw)
            .unwrap_or_else(|e| panic!("data/{rel} does not parse: {e}"));
        let after: serde_norway::Value = serde_norway::from_str(&lean)
            .unwrap_or_else(|e| panic!("data/{rel} stopped parsing once stripped: {e}"));
        assert!(
            before == after,
            "data/{rel}: stripping comments changed what it MEANS — the scanner \
             took a line it should have kept, or kept one it should have taken"
        );

        let dest = lean_root.join(rel);
        fs::create_dir_all(dest.parent().expect("has a parent"))
            .unwrap_or_else(|e| panic!("mkdir for {rel}: {e}"));
        fs::write(&dest, &lean).unwrap_or_else(|e| panic!("write {}: {e}", dest.display()));

        let dest = dest.to_string_lossy().replace('\\', "/");
        writeln!(src, "    ({rel:?}, include_str!({dest:?})),").unwrap();
    }
    src.push_str("];\n");

    let out = out_dir.join("embedded_data.rs");
    fs::write(&out, src).unwrap_or_else(|e| panic!("write {}: {e}", out.display()));
}
