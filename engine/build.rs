//! Embed the `data/` tree at compile time (docs/WASM.md phase 1).
//!
//! Scans `../data` for `*.yaml` and generates `$OUT_DIR/embedded_data.rs`
//! with one `include_str!` entry per file, sorted by relative path with
//! forward slashes. `crate::data` includes the table; every loader reads
//! from it, so native and wasm builds carry the identical data set and no
//! binary depends on the current working directory.
//!
//! WHAT IS EMBEDDED IS THE DATA, NOT THE PROSE (2026-08-21). `data/` is
//! **43% comments and blank lines** — 1.55 MB of 3.57 MB, and 55% of
//! `weapons/`, 67% of `evolutions/`, 75% of `debuffs/` — because this repo
//! makes every value cite its source, which is a rule about the REPO and not
//! about the artefact players download. So each file is stripped of its
//! full-line comments on the way into `$OUT_DIR` and the stripped copy is what
//! `include_str!` takes. AGENTS.md's own rule is what makes this safe: *"YAML
//! fields are consumed data; narrative/prose belongs in comments"*, so nothing
//! reads a comment.
//!
//! MEASURED, on the wire and not on disk, which is the only figure that is
//! about a reader: the wasm goes 6.74 MB -> 4.33 MB raw, and 1,192 KB -> 927 KB
//! under the same brotli the CDN serves — **-22%** of what every visitor
//! downloads. Essentially all of it is this: `wasm-opt -Oz`, run for the first
//! time in the same change, is worth -0.3% there, because it shrinks CODE and
//! 59% of this binary is data.
//!
//! A LINE IS DROPPED OR KEPT BYTE FOR BYTE. Several loaders read the embedded
//! text as LINES rather than through serde — `l.starts_with("internal_name:")`
//! in `mods_data`, `strip_prefix("set:")` in `mod_sets_data` — so re-emitting
//! the yaml through a parser, which is the obvious way to drop comments, would
//! silently rewrite the quoting those readers stand on. Removing whole lines
//! cannot. Inline comments are left alone for the same reason: `mod_sets_data`
//! strips its own with `split('#')`, so they are already somebody's input.
//!
//! THE BLOCK-SCALAR ARM IS DEFENSIVE, AND SAYS SO. A `#` line inside a `|`/`>`
//! block is CONTENT, so the scanner tracks blocks and passes them through — but
//! measured against today's `data/`, removing that arm changes the meaning of
//! ZERO of 1,607 files, because none of the 110 live blocks contains such a
//! line. It is here for the block somebody writes next year, and claiming the
//! data needs it today would be claiming a test that does not test anything.
//!
//! THE BUILD PROVES IT RATHER THAN A TEST. Every file is parsed before and
//! after and the two `Value`s must be equal, so a stripper that changed a
//! meaning cannot compile — there is no window in which the artefact is wrong
//! and the suite is green. Verified to bite: also dropping `name:` lines fails
//! the build naming `data/abilities/eclipse.yaml`.
//!
//! It is strict in a second way that turned out to matter: `serde_norway`
//! refuses a DUPLICATE KEY, and the first run of this build found fourteen in
//! `data/i18n/zh/ui.yaml` — four of them a Chinese translation that had never
//! reached a screen, which no i18n check could see because a key translated
//! twice still counts as translated.

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
