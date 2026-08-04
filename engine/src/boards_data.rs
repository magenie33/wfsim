//! THE BOARD — `data/benchmarks/boards/*.yaml`, one file per benchmark.
//!
//! What a board holds is BUILDS, never scores that anyone reported. A score
//! here was produced by running this engine over that build under that
//! benchmark, with the benchmark's own pinned seed, so anyone with the repo can
//! reproduce any row exactly. That is the whole reason the board lives in the
//! repo rather than in a database: reproducible stops being a claim and becomes
//! a property.
//!
//! It also means a change to the ENGINE or to `data/` invalidates every row at
//! once, and re-scoring is the answer rather than migration — the builds are
//! still builds. Nobody is asked to resubmit, because nobody ever submitted a
//! number.
//!
//! # Why these are read-only presets and not a page of their own
//!
//! A board entry's OUTPUT is a build, and the builder is what consumes a build
//! — the same relationship the riven editor has with mods. So it earns a place
//! in the build bar rather than a tab (user, 2026-08-04: "不需要多的 tab"), as
//! a chip you can select and copy but not edit, exactly like the official
//! scenario in the scenario bar.

use std::sync::OnceLock;

use serde::Deserialize;

/// One row: a build, and what it measured.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BoardEntry {
    pub weapon: String,
    /// The benchmark's metric, as this engine computed it. Deterministic —
    /// re-running the same build under the same benchmark reproduces it to the
    /// last digit, in the browser as well as natively (measured 2026-08-04:
    /// wasm and native both give 0.9647804061510868 for the same payload).
    pub score: f64,
    // NO `forma`/`drain` FIELD. Both are DERIVED from the build by
    // `builds::validate`, and a file that also stated them would be a second
    // copy of a fact — the one that goes stale when the planner improves or a
    // mod's drain is corrected. They are computed where they are shown.
    #[serde(default)]
    pub mods: Vec<String>,
    #[serde(default)]
    pub evolutions: Vec<String>,
    #[serde(default)]
    pub arcanes: Vec<String>,
}

/// One benchmark's board.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Board {
    /// The benchmark these rows were measured under. A row is only meaningful
    /// against one ruler, so the board carries the id rather than each row.
    pub benchmark: String,
    /// How the rows got here. `seed` = produced by the maintainer to fill an
    /// empty board; `submissions` = scored from what players sent. Stated
    /// because a board nobody has contributed to yet should not look like one
    /// that has.
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub entries: Vec<BoardEntry>,
}

/// Every board, parsed once.
pub fn all() -> &'static [Board] {
    static B: OnceLock<Vec<Board>> = OnceLock::new();
    B.get_or_init(|| {
        crate::data::files_under("benchmarks/boards/")
            .filter(|(p, _)| p.ends_with(".yaml"))
            .map(|(p, text)| {
                serde_norway::from_str::<Board>(text).unwrap_or_else(|e| panic!("{p}: {e}"))
            })
            .collect()
    })
}

/// This weapon's rows on this benchmark's board, best first.
pub fn for_weapon(benchmark: &str, weapon: &str) -> Vec<&'static BoardEntry> {
    let mut rows: Vec<&BoardEntry> = all()
        .iter()
        .filter(|b| b.benchmark == benchmark)
        .flat_map(|b| b.entries.iter())
        .filter(|e| e.weapon == weapon)
        .collect();
    rows.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EVERY ROW IS A BUILD SOMEONE COULD EQUIP. A board is public and copyable,
    /// so a row that cannot be built is worse than a missing row — it is an
    /// instruction that fails in the arsenal. `builds::validate` is the same
    /// check a submission will face, run here against what is already published.
    #[test]
    fn every_published_row_is_a_legal_build() {
        for b in all() {
            assert!(
                crate::benchmarks_data::get(&b.benchmark).is_some(),
                "board names benchmark {}, which does not exist",
                b.benchmark
            );
            for e in &b.entries {
                let v = crate::builds::validate(&e.weapon, &e.mods, &e.evolutions, &e.arcanes)
                    .unwrap_or_else(|err| panic!("{} row on {}: {err}", e.weapon, b.benchmark));
                assert!(v.drain <= crate::builds::CAPACITY);
                assert!(e.score > 0.0, "{} scored nothing", e.weapon);
            }
        }
    }

    /// Rows come back best-first, because "the top 10" is the only order a
    /// board has.
    #[test]
    fn a_weapons_rows_are_ranked() {
        for b in all() {
            for w in ["torid", "boar_prime", "laetum"] {
                let rows = for_weapon(&b.benchmark, w);
                for pair in rows.windows(2) {
                    assert!(pair[0].score >= pair[1].score, "{w} out of order");
                }
            }
        }
    }
}
