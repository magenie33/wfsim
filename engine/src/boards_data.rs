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
//! in the build bar rather than a tab (user, 2026-08-04), as a chip you
//! can select and copy but not edit, exactly like the official scenario
//! in the scenario bar.

use std::sync::OnceLock;

use serde::Deserialize;

/// One row: a build, and what it measured.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BoardEntry {
    pub weapon: String,
    /// HOW the weapon was played — `base`, `cycle`, `alternate`. The file has
    /// carried it since mode became part of a row's identity, and this struct
    /// did not, so serde dropped it: a Torid through its Incarnon cycle and a
    /// Torid that never transmutes read as the same row to anything parsing a
    /// board back. Empty = a row written before the dimension existed, which is
    /// `base` by the same fallback the scorer uses.
    #[serde(default)]
    pub mode: String,
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
    /// An ADVERSARY weapon's VALENCE ELEMENT (Kuva, Tenet, Coda). Part of the
    /// row because it is part of the build — a different progenitor element is
    /// a different weapon, not a weaker one — and because 25-60% of base damage
    /// is not a difference a row may leave unstated and still be reproducible.
    ///
    /// The PERCENTAGE is not here: the board scores every row at the roll's
    /// maximum, which every player can reach by Valence Fusion, so it is
    /// investment rather than a choice (the same rule that scores every row at
    /// full Forma). Empty on every weapon that has no valence.
    #[serde(default)]
    pub valence: String,
    /// THE RIVEN THIS ROW WEARS, as a SHAPE. Absent on a row without one.
    ///
    /// It went the way `mode` did, one comment up: the file has carried it
    /// since a riven build could reach the board and this struct did not, so
    /// serde dropped it — and `every_published_row_is_a_legal_build` then
    /// validated a build with a `riven` in its mods and no riven to put there,
    /// which passed for as long as the board held none and failed the hour the
    /// first one landed (2026-08-24).
    /// THE EXILUS SLOT'S MOD, when the row wears one. Its own field for the
    /// reason `builds::ValidBuild::exilus` is: an exilus-eligible mod is legal
    /// in a MAIN slot, so `mods` alone cannot say which entry came out of it.
    #[serde(default)]
    pub exilus: String,
    #[serde(default)]
    pub riven: Option<BoardRiven>,
    /// WHAT THIS ROW'S SCORE DEPENDS ON, as one hash — the ruler, the weapon
    /// and every form it fires, each mod, each arcane, each evolution, and
    /// everything no entity owns (`data_fingerprint`). The next run recomputes
    /// it and reuses the score only if it matches, so a mod correction rescores
    /// the rows carrying that mod and leaves the rest alone.
    ///
    /// Empty = written before per-row fingerprints existed, which reads as
    /// "rescore it", the same way an absent board fingerprint does.
    #[serde(default)]
    pub fp: String,
}

/// A published row's riven: the SHAPE, plus the ROLLS this engine settled on.
///
/// Not `rivens_data::RivenShape` directly, because the rolls have to survive a
/// parse. They travel because a score is not enough to publish a riven row —
/// the reader has to be able to BUILD that riven, and the corner the scorer
/// chose took a sixteen-way search to find. Before this they were written and
/// never read back, which nothing noticed because the board's coarse
/// fingerprint made almost every run a full rescore; the moment reuse became
/// the common case a reused riven row would have lost its rolls, and with them
/// its whole riven block (2026-08-25).
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct BoardRiven {
    pub bonuses: Vec<String>,
    #[serde(default)]
    pub malus: Option<String>,
    #[serde(default)]
    pub rolls: Vec<f64>,
}

impl BoardRiven {
    /// The shape alone — what validation and scoring ask for.
    pub fn shape(&self) -> crate::rivens_data::RivenShape {
        crate::rivens_data::RivenShape {
            bonuses: self.bonuses.clone(),
            malus: self.malus.clone(),
        }
    }
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
    /// THE ENGINE THAT SCORED THESE ROWS — a hash of everything a score depends
    /// on that is not the build: `engine/`, `webapi/`, `cli/` and `data/` minus
    /// the boards themselves.
    ///
    /// It is what lets the next run tell reuse from staleness EXACTLY. A score
    /// is a pure function of (build, the ruler's terms, this code and this
    /// The ENGINE CODE this board was scored by (`engine`, `webapi`, `cli`).
    /// The DATA half is per row — see `BoardEntry::fp` — because a data change
    /// moves the rows that read the file that changed and no others, while a
    /// change in `damage.rs` can move any row and no dependency set can say
    /// otherwise. Empty = scored before this was recorded, which reads as
    /// "rescore everything".
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub entries: Vec<BoardEntry>,
}

/// A score as it is PUBLISHED: at least four significant figures and at least
/// four decimal places (owner, 2026-08-04).
///
/// One rule for both, because a board figure is read two ways. `11.0522` is
/// the KPM case — four decimals already carry six significant figures, and the
/// fourth decimal is the digit that separates two builds a player is choosing
/// between. `0.0001234` is the other end: four decimals there would publish
/// `0.0001`, which is one significant figure and cannot rank anything.
///
/// It lives HERE and not in the client because the client does not do this
/// arithmetic — `wfsim-board` writes the formatted string beside the number and
/// the page prints it. The number stays exact in the record; only what is shown
/// is rounded, so two rows that tie on screen are still ordered underneath.
pub fn format_score(v: f64) -> String {
    let mag = if v.is_normal() { v.abs().log10().floor() as i32 } else { 0 };
    // Four significant figures need `3 - mag` decimals; four decimals is the
    // floor, and 12 the stop so a denormal cannot ask for hundreds.
    let dp = (3 - mag).clamp(4, 12) as usize;
    format!("{v:.dp$}")
}

/// Every board, parsed once.
/// Parse ONE board file. The scorer reads the board it is about to replace —
/// to tell reuse from staleness — and that is a path on disk rather than one of
/// the embedded set, so the parse lives here beside the type rather than as a
/// second copy of it in the binary.
pub fn parse(text: &str) -> Result<Board, String> {
    serde_norway::from_str::<Board>(text).map_err(|e| e.to_string())
}

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
                let v = crate::builds::validate_for_board_with(
                    &b.benchmark, &e.weapon, &e.mods, &e.evolutions, &e.arcanes, &e.valence,
                    e.riven.as_ref().map(BoardRiven::shape).as_ref(),
                    Some(e.exilus.as_str()).filter(|x| !x.is_empty()),
                )
                    .unwrap_or_else(|err| panic!("{} row on {}: {err}", e.weapon, b.benchmark));
                // `validate` already refused anything over capacity, and the
                // capacity is the weapon's own — so the assertion is that it
                // fits, not that it fits some number written here.
                //
                // An empty build is a LEGAL build — and not a publishable one.
                // This asserted `drain > 0`, then nothing, and now the rule is
                // in `validate_for_board`: the board takes complete builds only
                // (2026-08-05). Ranking a bad build last only works when there
                // is something else to rank it against. Nothing here decides
                // what is worth submitting.
                assert_eq!(v.mods.len(), e.mods.len(), "{} lost a mod", e.weapon);
                assert!(e.score > 0.0, "{} scored nothing", e.weapon);
            }
        }
    }

    /// AT LEAST FOUR SIGNIFICANT FIGURES AND AT LEAST FOUR DECIMALS — pinned,
    /// because it is what every published figure is read at.
    #[test]
    fn a_published_score_carries_four_of_each() {
        assert_eq!(format_score(11.052231199820268), "11.0522");
        assert_eq!(format_score(0.9647804061510868), "0.9648");
        assert_eq!(format_score(0.000123456), "0.0001235");
        assert_eq!(format_score(1234.56789), "1234.5679");
        assert_eq!(format_score(0.0), "0.0000");
        for v in [11.05, 0.964, 0.000123, 1234.5, 7.0] {
            let s = format_score(v);
            let dec = s.split_once('.').map(|(_, d)| d.len()).unwrap_or(0);
            assert!(dec >= 4, "{v} shown as {s}: fewer than four decimals");
            let sig = s.replace(['.', '-'], "").trim_start_matches('0').len();
            assert!(sig >= 4, "{v} shown as {s}: fewer than four significant figures");
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
