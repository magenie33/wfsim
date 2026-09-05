//! THE BOARD — `boards/*.yaml` at the repository root, one file per benchmark.
//!
//! OUTSIDE `data/` DELIBERATELY. Everything under `data/` is compiled into the
//! binary, and a board is generated output rather than game data: embedding it
//! put every row of every board into the wasm each visitor downloads, to serve
//! three integers. Those integers are `data/board_state.yaml`; the rows are
//! read from DISK, by the scorer that writes them and by the site build.
//!
//! What a board holds is BUILDS, never scores anyone reported: a score here was
//! produced by running this engine over that build under that benchmark's own
//! pinned seed, so anyone with the repo reproduces any row exactly. That is why
//! the board lives in the repo rather than in a database — reproducible stops
//! being a claim and becomes a property — and why a change to the ENGINE or to
//! `data/` re-scores every row rather than migrating it. Nobody is asked to
//! resubmit, because nobody ever submitted a number.
//!
//! THEY ARE READ-ONLY PRESETS RATHER THAN A PAGE OF THEIR OWN: a board entry's
//! OUTPUT is a build, and the builder is what consumes one.

use std::sync::OnceLock;

use serde::Deserialize;

impl BoardEntry {
    /// The row's parts as the engine's own shape, or `None` where it names none.
    pub fn assembly(&self) -> Option<crate::kitguns_data::Assembly> {
        if self.grip.is_empty() && self.loader.is_empty() {
            return None;
        }
        // The chamber's WEAPON id, which is what `Assembly` holds — not the
        // per-slot record id in `spec.kitgun`; the slot follows from the grip.
        let chamber = crate::weapons_data::spec(&self.weapon)
            .and_then(|s| s.kitgun.clone())
            .and_then(|r| crate::kitguns_data::default_assembly(&r))
            .map(|d| d.chamber)
            .unwrap_or_default();
        Some(crate::kitguns_data::Assembly {
            chamber,
            grip: self.grip.clone(),
            loader: self.loader.clone(),
        })
    }
}

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
    /// IS THIS ROW PUBLISHED, or is it the record of one the floor held back?
    ///
    /// The file carries BOTH. A row below the floor is not shown — `board.json`
    /// is written from the listed rows alone — but it is kept here because the
    /// file is also the next run's REUSE CACHE, and a scored row missing from
    /// it is one that gets re-simulated from scratch every run for ever, to be
    /// discarded again. It is a record too: without it, "scored and not listed"
    /// and "lost" read the same from the submitter's side.
    ///
    /// DEFAULT TRUE, so every row written before this existed is what it was.
    #[serde(default = "listed")]
    pub listed: bool,
    /// WAS THIS SCREENED RATHER THAN MEASURED?
    ///
    /// A PROBE IS NOT A MEASUREMENT. It is the same fight at a tenth of the
    /// ruler's runs, run to decide whether the full one is worth paying for —
    /// so a row carrying this is recorded and never published, and never reused
    /// as a score. What it says is "this build reads a quarter of its group's
    /// leader, so it was not measured this run", which is a different claim
    /// from a number the board stands behind.
    #[serde(default)]
    pub probe: bool,
    /// SECONDS THIS ROW TOOK TO SIMULATE, as the run that measured it found.
    ///
    /// NOT PART OF THE ANSWER — it is scheduling data, and the score does not
    /// depend on it. It rides the board because the board is what survives
    /// between runs, and the next one packs its shards by measured work rather
    /// than by row count: the makespan is set by the slowest shard, and a
    /// modulo cannot see which rows are the monsters. Zero = never measured.
    #[serde(default)]
    pub cost: f64,
    /// The benchmark's metric, as this engine computed it. Deterministic —
    /// re-running the same build under the same benchmark reproduces it to the
    /// last digit, in the browser as well as natively.
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
    /// first one landed.
    /// THE EXILUS SLOT'S MOD, when the row wears one. Its own field for the
    /// reason `builds::ValidBuild::exilus` is: an exilus-eligible mod is legal
    /// in a MAIN slot, so `mods` alone cannot say which entry came out of it.
    #[serde(default)]
    pub exilus: String,
    /// THE PARTS, on a modular weapon's row. Empty on every other, which is all
    /// but the Kitguns.
    ///
    /// They are the BUILD — a grip sets damage, fire rate and the charge — so a
    /// row that did not carry them could not be reproduced and two assemblies
    /// of one chamber would be one row. Flat, matching the record the worker
    /// stores: an object would be a third shape for one fact.
    #[serde(default)]
    pub grip: String,
    #[serde(default)]
    pub loader: String,
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
/// its whole riven block.
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
    /// HOW MANY BUILDS THE RUN THAT WROTE THIS BOARD READ. The library reports
    /// its own size at `/api/board/pending`; the difference is what has arrived
    /// since, which is the one thing a static file cannot say about itself.
    #[serde(default)]
    pub submissions: usize,
    #[serde(default)]
    pub entries: Vec<BoardEntry>,
}

/// A score as it is PUBLISHED: at least four significant figures and at least
/// four decimal places.
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
fn listed() -> bool {
    true
}

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

/// WHAT THE RUNTIME KNOWS ABOUT A BOARD, which is everything except the rows.
///
/// THE ROWS ARE NOT EMBEDDED, and that is the whole reason this type exists.
/// `data/` is compiled into the binary, so a board archive living there put
/// every row of every board into the wasm every visitor downloads — measured at
/// 1.47 MB of a 6.36 MB module, to serve one integer per benchmark. The archive
/// is `boards/*.yaml` at the repository root, read from DISK by the scorer that
/// writes it; this file is the handful of scalars the page actually asks for.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BoardState {
    /// HOW MANY BUILDS THE RUN THAT WROTE THIS BOARD READ. The library reports
    /// its own size at `/api/board/pending`; the difference is what has arrived
    /// since, which is the one thing a static file cannot say about itself.
    pub submissions: usize,
    /// Rows published — what `board.json` carries and the page can rank.
    #[serde(default)]
    pub listed: usize,
    /// …AND ROWS SCORED AND HELD BACK by the floor. Reported beside `listed`
    /// rather than folded into it: the two answer different questions, and a
    /// board that says only how many it shows cannot say how much it looked at.
    #[serde(default)]
    pub held: usize,
    /// WHEN THE RUN THAT WROTE THIS BOARD FINISHED, in seconds since the epoch.
    ///
    /// The counts above say how far behind the board is in BUILDS; this is the
    /// one thing neither they nor the fingerprints can say — a fingerprint
    /// answers "did an input move", never "when was this measured", and a
    /// reader looking at a number wants to know how old it is. ZERO means a
    /// board written before this field existed, which the page reads as unknown
    /// rather than as 1970.
    #[serde(default)]
    pub scored_at_epoch_seconds: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct BoardStates {
    #[serde(default)]
    boards: std::collections::BTreeMap<String, BoardState>,
}

/// One benchmark's board state, by id.
pub fn of(benchmark: &str) -> Option<&'static BoardState> {
    static S: OnceLock<std::collections::BTreeMap<String, BoardState>> = OnceLock::new();
    S.get_or_init(|| {
        // NOT UNDER `benchmarks/`. That directory's own loader parses every
        // yaml at its top level as a RULER definition, so a state file there is
        // a benchmark with no `id`.
        crate::data::files_under("board_state")
            .next()
            .map(|(p, text)| {
                serde_norway::from_str::<BoardStates>(text)
                    .unwrap_or_else(|e| panic!("{p}: {e}"))
                    .boards
            })
            .unwrap_or_default()
    })
    .get(benchmark)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE ARCHIVE, READ FROM DISK. It is `boards/*.yaml` at the repository
    /// root and deliberately outside `data/`, so it is not embedded and there
    /// is nothing for a test to reach through the binary — which is the point:
    /// the rows are a CI artifact, not something the browser carries.
    fn archive() -> Vec<Board> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../boards");
        let mut out: Vec<Board> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
            .map(|p| {
                let text = std::fs::read_to_string(&p).expect("read board");
                parse(&text).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
            })
            .collect();
        out.sort_by(|a, b| a.benchmark.cmp(&b.benchmark));
        assert!(!out.is_empty(), "no board archive under boards/");
        out
    }

    /// EVERY ROW IS A BUILD SOMEONE COULD EQUIP. A board is public and copyable,
    /// so a row that cannot be built is worse than a missing row — it is an
    /// instruction that fails in the arsenal. `builds::validate` is the same
    /// check a submission will face, run here against what is already published.
    #[test]
    fn every_published_row_is_a_legal_build() {
        for b in archive() {
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
                    e.assembly().as_ref(),
                )
                    .unwrap_or_else(|err| panic!("{} row on {}: {err}", e.weapon, b.benchmark));
                // `validate` already refused anything over capacity, and the
                // capacity is the weapon's own — so the assertion is that it
                // fits, not that it fits some number written here.
                //
                // An empty build is a LEGAL build — and not a publishable one.
                // This asserted `drain > 0`, then nothing, and now the rule is
                // in `validate_for_board`: the board takes complete builds only. Ranking a bad build last only works when there
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
        for b in archive() {
            for w in ["torid", "boar_prime", "laetum"] {
                // THE LISTED ROWS ONLY. The archive also carries what the floor
                // held back, and those are appended after the ranking rather
                // than woven into it — `board.json`, which is what the page
                // ranks, is written from the listed set alone.
                let rows: Vec<&BoardEntry> = b
                    .entries
                    .iter()
                    .filter(|e| e.listed && e.weapon == w)
                    .collect();
                for pair in rows.windows(2) {
                    assert!(pair[0].score >= pair[1].score, "{w} out of order");
                }
            }
        }
    }
}
