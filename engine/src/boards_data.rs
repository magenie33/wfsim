//! WHAT THE RUNTIME KNOWS ABOUT EACH BOARD, which is everything except the rows.
//!
//! THE ROWS ARE NOT EMBEDDED, and that is the whole reason this module is this
//! small. Everything under `data/` is compiled into the binary, and a board is
//! generated output rather than game data: embedding it put every row of every
//! board into the wasm each visitor downloads — measured at 1.47 MB of a
//! 6.36 MB module — to serve a handful of scalars. Those scalars are
//! `data/board_state.yaml`, and they are all of this.
//!
//! THE ROWS LIVE IN `site/board/<weapon>.json`, which the page FETCHES, and
//! durably in the `scores` table they were published from. They have ONE writer,
//! `wfsim-board --project`, and NOTHING READS THEM BACK THROUGH THE ENGINE: a
//! reader here would be a second declaration of the published shape, and two
//! declarations of one shape drift.
//!
//! What a board holds is BUILDS, never scores anyone reported: a score was
//! produced by running this engine over that build under that benchmark's own
//! pinned seed, so anyone with the repo reproduces any row exactly.

use std::sync::OnceLock;

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
pub fn format_score(v: f64) -> String {
    let mag = if v.is_normal() { v.abs().log10().floor() as i32 } else { 0 };
    // Four significant figures need `3 - mag` decimals; four decimals is the
    // floor, and 12 the stop so a denormal cannot ask for hundreds.
    let dp = (3 - mag).clamp(4, 12) as usize;
    format!("{v:.dp$}")
}

/// The handful of scalars the page asks about a board it does not hold.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct BoardState {
    /// HOW MANY BUILDS THE RUN THAT WROTE THIS BOARD READ. The library reports
    /// its own size at `/api/board/pending`; the difference is what has arrived
    /// since, which is the one thing a static file cannot say about itself.
    pub submissions: usize,
    /// Rows published — what `site/board/` carries and the page can rank.
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
}
