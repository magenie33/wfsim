// SPDX-License-Identifier: AGPL-3.0-or-later
//! A RULER'S WORDS: the one place in `board` that reads prose
//! (engine/src/prose.rs).
use super::benchmarks::Benchmark;

/// The first `{axis}` a cut ruler's words still say, which no axis it spans
/// filled.
pub(super) fn unfilled(c: &Benchmark) -> Option<&str> {
    std::iter::once(c.name.as_str())
        .chain(c.rules.iter().map(|t| t.shown()))
        .chain([c.standard.shown()])
        .find_map(hole)
}

/// The first `{word}` hole left in `text`, if any.
fn hole(text: &str) -> Option<&str> {
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let word = after.split('}').next().unwrap_or("");
        if after.contains('}') && !word.is_empty() && word.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            return Some(word);
        }
        rest = after;
    }
    None
}
