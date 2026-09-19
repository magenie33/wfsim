// SPDX-License-Identifier: AGPL-3.0-or-later
//! The facts: one measurement per row, appended as it is taken and read back
//! per ruler or across every live ruler.

use serde_json::Value;
use wfsim_engine::board::benchmarks::family;

/// A SCORE IS READ FROM ITS TEXT, NEVER THROUGH THE JSON NUMBER PARSER.
///
/// `serde_json`'s parser is not correctly rounding: it reads
/// `1.1070976928071055` back as `1.1070976928071057`, one ULP away, and the
/// same for roughly one value in ten. Every score crosses from the database to
/// the publish process through it, so the board published a number the engine
/// never produced — and a reader reproducing the row from the repo, as the
/// board invites them to, got the engine's answer and not the board's.
///
/// Rust's own `str::parse::<f64>` IS correctly rounding, and every writer in
/// the chain emits the shortest text that round-trips (Ryu here, and
/// `JSON.stringify` at the database's edge). So the literal is exact and only
/// the reader was lossy. `RawValue` is what hands the literal over unparsed;
/// the quotes are trimmed because the same field arrives as a json number out
/// of the database and as a string from anything that batches it.
pub(crate) fn exact_score(line: &str) -> Option<f64> {
    #[derive(serde::Deserialize)]
    struct Scored<'a> {
        #[serde(borrow)]
        score: &'a serde_json::value::RawValue,
    }
    serde_json::from_str::<Scored<'_>>(line)
        .ok()?
        .score
        .get()
        .trim_matches('"')
        .parse()
        .ok()
}

/// THE BUILD A ROW KEY NAMES, which is the key without its mode. The accounting
/// asks whether a BUILD reached a row, and a run that defers work answers with
/// identities rather than keys.
pub(crate) fn identity_of(key: &str) -> String {
    key.rsplit_once('#').map_or_else(|| key.to_string(), |(i, _)| i.to_string())
}

/// ONE MEASUREMENT, as the database holds it — and there is exactly ONE per
/// row, the last one taken.
///
/// A FACT IS NOT WRONG FOR BEING OLD. Neither the clock nor the build that
/// wrote it says anything about whether the number is right, so neither is in
/// the key and neither decides anything here. The one question that can be
/// asked of a stored score is whether its INPUTS still hold, which is
/// `data_fp` — carried ON the fact rather than in the key, so a row that has
/// been measured under three generations of data is one row and not three.
#[derive(Clone)]
pub(crate) struct Fact {
    pub(crate) score: f64,
    pub(crate) cost_seconds: f64,
    /// The riven corner the search settled on, when there is one. It travels
    /// WITH the score because it was found by the same fight: reusing one
    /// without the other would publish a number for a riven nobody can build.
    /// BOTH ENDS OF THE FIGHT. `cost_seconds` is not derivable from them: a row
    /// the clock stopped and resumed spans a wall clock longer than the runs it
    /// contains, and what the packing needs is the runs.
    pub(crate) started_at: String,
    pub(crate) finished_at: String,
}

/// WHAT THE DATABASE HOLDS FOR ONE RULER: the last measurement of each row.
///
/// ONE INDEX, because there is one fact. It was two — by `(row, what it read)`
/// and by the row alone — and every reader had to be told which of the two it
/// was: a pass that can REFIGHT must not reuse a number whose data moved, a
/// pass that only ASSEMBLES must publish it anyway or the row leaves the board.
/// That is not two facts, it is one fact and two questions, and the fact
/// carries what both of them need.
pub(crate) type Facts = std::collections::HashMap<String, Fact>;

/// A WALL CLOCK AS THE DATABASE WANTS IT: seconds, UTC, no fraction. The
/// column is TEXT and the only thing that ever compares two of them is "which
/// is newer", which this ordering answers lexically.
pub(crate) fn stamp(at: &std::time::SystemTime) -> String {
    let secs = at
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // A HAND-ROLLED CIVIL DATE, because the alternative is a dependency for one
    // line of output nothing parses back. Days since the epoch to y/m/d by the
    // proleptic Gregorian rule.
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = 400 * era + yoe + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// A FACT, WRITTEN THE MOMENT IT IS COMPUTED, to a file that is only ever
/// APPENDED to and flushed per row.
///
/// `(build, ruler, mode, what it read, what measured it) -> score` is true for
/// ever once computed, so it is written down when it is computed rather than
/// when a batch ends. What ships it to the database is
/// `scripts/ship_facts.sh`, running beside the scorer, so a shard killed at
/// nine tenths keeps nine tenths, rather than nothing at all.
///
/// `measured_by` is handed in rather than derived: the scorer cannot see which
/// commit built it, and a hash it invented would be a second answer to a
/// question the workflow already knows.
pub(crate) struct FactLog {
    pub(crate) out: Option<std::io::BufWriter<std::fs::File>>,
    pub(crate) measured_by: String,
}

impl FactLog {
    pub(crate) fn open(path: Option<String>, measured_by: Option<String>) -> Self {
        let out = path.and_then(|p| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&p)
                .map_err(|e| eprintln!("facts: cannot append to {p}: {e}"))
                .ok()
                .map(std::io::BufWriter::new)
        });
        Self { out, measured_by: measured_by.unwrap_or_default() }
    }

    /// THE KEY IS `identity#mode` and the row needs them apart, because a mode
    /// is an independent ranking: a melee carries seven and collapsing them
    /// would keep whichever was written last.
    /// THE CLOCKS ARE THE SCORER'S, and they are on the `Fact` rather than taken
    /// here: the shipper runs beside this and may send a row minutes later, so a
    /// timestamp invented downstream would be the shipping time under the
    /// fight's name.
    pub(crate) fn write(&mut self, ruler: &str, metric: &str, key: &str, f: &Fact) {
        use std::io::Write;
        let Some(out) = self.out.as_mut() else { return };
        let (identity, mode) = key.rsplit_once('#').unwrap_or((key, ""));
        let line = serde_json::json!({
            "identity": identity,
            "ruler": ruler,
            // WHAT THE SCORE IS IN, written down beside it because nothing else
            // can say so later. The ruler's file answers what it ranks by NOW;
            // a row deliberately outlives the file it was measured under — that
            // is what `data_fp` in the key is for — and reading an old one back
            // without this means checking out the commit that produced it.
            //
            // IT DECIDES NOTHING, the same terms as `measured_by`: a ruler that
            // changes its core changes its file, which moves `data_fp`, which
            // is already the whole of invalidation. Branching on this too would
            // be a second answer to a question that has one.
            "metric": metric,
            "mode": mode,
            "measured_by": self.measured_by,
            "score": f.score,
            "cost_seconds": f.cost_seconds,
            "started_at": f.started_at,
            "finished_at": f.finished_at,
        });
        // FLUSHED PER ROW. A buffer that is written at the end is the batch
        // this exists to stop being.
        if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
            eprintln!("facts: the log stopped accepting rows");
            self.out = None;
        }
    }
}

/// THE SAME FACTS, READ BACK OUT OF THE DATABASE, as `scripts/fetch_facts.sh`
/// wrote them: one json object a line, one row per (build, ruler, mode).
///
/// A ROW FROM ANOTHER RULER IS NOT MERGED. The key is `identity#mode` and
/// carries no benchmark, so two boards scoring one build produce the SAME key
/// with different numbers — a file holding both would publish whichever landed
/// last under a ruler that never measured it.
///
/// THE SCORER NEVER SPEAKS TO THE DATABASE. It reads a file, and the network
/// lives in a script a stub `curl` can drive.
pub(crate) fn load_facts(spec: Option<String>, bench_id: &str) -> Facts {
    let mut facts = Facts::default();
    let Some(path) = spec else { return facts };
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("facts: cannot read {path}");
        return facts;
    };
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if v.get("ruler").and_then(Value::as_str) != Some(bench_id) {
            continue;
        }
        let (Some(id), Some(score)) =
            (v.get("identity").and_then(Value::as_str), exact_score(line))
        else {
            continue;
        };
        let mode = v.get("mode").and_then(Value::as_str).unwrap_or("");
        let key = if mode.is_empty() { id.to_string() } else { format!("{id}#{mode}") };
        let fact = Fact {
            score,
            cost_seconds: v.get("cost_seconds").and_then(Value::as_f64).unwrap_or_default(),
            // THE ROLLS TRAVEL AS TEXT, because the column is one and a riven
            // corner is a list. A row without one is a plain row, not a broken
            // one.
            started_at: v
                .get("started_at")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            finished_at: v
                .get("finished_at")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        };
        // THE NEWEST WINS THE ROW, decided by the clock rather than by the
        // order the file happens to be in. The database holds one row per key,
        // so this only ever arbitrates between what the run FETCHED and what
        // its own shards have measured since — and the newer of those is the
        // one the database is about to hold.
        let newer = facts
            .get(&key)
            .is_none_or(|held: &Fact| held.finished_at <= fact.finished_at);
        if newer {
            facts.insert(key, fact);
        }
    }
    eprintln!("facts: {} rows read for {bench_id}", facts.len());
    facts
}

/// ONE FACT, UNDER WHICHEVER RULER MEASURED IT — the one reader here that is
/// NOT narrowed to `bench_id`.
pub(crate) struct CrossFact {
    pub(crate) identity: String,
    pub(crate) ruler: String,
    pub(crate) mode: String,
    pub(crate) score: f64,
}

/// EVERY LIVE RULER'S FACTS, READ WHOLE, and only the ENTRY LINE asks for them.
///
/// The line is a question about a BUILD — is it any good ANYWHERE — and this
/// tool runs one ruler at a time, so a gate that read only its own ruler would
/// park a build for being bad at the one fight it happened to be asked about.
/// That is the property that lets the fan-out find a build good where its
/// submitter never tried it, and it is worth one extra pass over a file the run
/// has already been handed.
///
/// A RETIRED RULER IS LEFT OUT: a share of a leader nobody publishes decides
/// nothing, and a build whose only facts are its own is owed a first fight.
pub(crate) fn load_cross_facts(spec: Option<String>) -> Vec<CrossFact> {
    let Some(path) = spec else { return Vec::new() };
    let live: std::collections::BTreeSet<String> = wfsim_engine::board::benchmarks::all()
        .iter()
        .map(|b| family(&b.id).to_string())
        .collect();
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("entry line: cannot read {path}");
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        let (Some(id), Some(ruler), Some(score)) = (
            v.get("identity").and_then(Value::as_str),
            v.get("ruler").and_then(Value::as_str),
            exact_score(line),
        ) else {
            continue;
        };
        if !live.contains(family(ruler)) {
            continue;
        }
        out.push(CrossFact {
            identity: id.to_string(),
            ruler: family(ruler).to_string(),
            mode: v.get("mode").and_then(Value::as_str).unwrap_or("base").to_string(),
            score,
        });
    }
    eprintln!("entry line: {} fact(s) read across every live ruler", out.len());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A FACT CARRIES ITS MODE IN A COLUMN OF ITS OWN.
    ///
    /// The key is `identity#mode` and a mode is an INDEPENDENT RANKING — a
    /// melee carries seven — so a fact table that kept them joined, or split on
    /// the FIRST `#`, would file seven measurements under one row and keep
    /// whichever was written last.
    /// …AND THE DATABASE AGREES, which is the half this file cannot assume.
    ///
    /// `load_facts` holds one fact per row because the table does. Put anything
    /// back in that key — `data_fp` was there — and the table holds several
    /// while every reader here can return only one, so which of them a run sees
    /// is decided by a `SELECT` nobody wrote down. Nothing fails; the board
    /// publishes whichever the paging happened to reach.
    ///
    /// READ FROM THE SCHEMA ITSELF, so the two cannot drift.
    #[test]
    fn the_scores_table_holds_one_fact_per_row() {
        let schema = include_str!("../../../worker/schema.sql");
        let key = schema
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with("PRIMARY KEY (identity, ruler, mode"))
            .expect("the scores table names its key");
        assert_eq!(
            key, "PRIMARY KEY (identity, ruler, mode)",
            "a row must key on what makes it a different QUESTION and nothing else — \
             anything more and one row holds several facts"
        );
    }

    /// A ROW HAS ONE FACT: THE LAST MEASUREMENT OF IT.
    ///
    /// Two lines for one row is the same row measured twice, and the newer one
    /// is the fact — decided by the CLOCK, never by the order the file happens
    /// to be in. What each read travels ON the fact, so the one row answers
    /// both questions asked of it: may this be reused (its `data_fp` still
    /// holds), and what does the assembly publish while nothing current exists.
    #[test]
    fn a_row_measured_twice_keeps_the_newer_and_what_it_read() {
        let dir = std::env::temp_dir().join("wfsim-facts-two");
        std::fs::create_dir_all(&dir).expect("tmp");
        let path = dir.join("facts.ndjson");
        let row = |fp: &str, score: f64, finished: &str| {
            format!(
                concat!(
                    r#"{{"identity":"braton|m","ruler":"single_target","mode":"base","#,
                    r#""data_fp":"{}","score":{},"cost_seconds":{},"finished_at":"{}"}}"#
                ),
                fp, score, score, finished
            )
        };
        std::fs::write(
            &path,
            // THE OLDER ROW LAST IN THE FILE, so an implementation that took the
            // last line would get the wrong carry.
            format!("{}
{}
", row("new", 9.0, "2026-02-02"), row("old", 4.0, "2026-01-01")),
        )
        .expect("write");

        let facts = load_facts(Some(path.to_string_lossy().into_owned()), "single_target");
        assert_eq!(facts.len(), 1, "one row, one fact");
        // THE NEWER ONE, though the file ends with the older.
        assert_eq!(facts["braton|m#base"].score, 9.0);
        // …AND ANOTHER RULER'S ROWS ARE NOT HERE AT ALL.
        let other = load_facts(Some(path.to_string_lossy().into_owned()), "group_clear");
        assert!(other.is_empty());
        let _ = std::fs::remove_file(&path);
    }

    /// THE CLAIM IS THAT THE READER IS EXACT, and the first half of the test is
    /// what makes the second half mean anything: this value is one serde's
    /// number parser actually moves, so a reader that agreed with it would be
    /// publishing a score the engine never produced.
    #[test]
    fn a_score_is_read_from_its_text_and_not_through_the_number_parser() {
        let want = 1.1070976928071055_f64;
        let line = format!(r#"{{"identity":"k","ruler":"r","score":{want}}}"#);
        let through_serde = serde_json::from_str::<Value>(&line)
            .unwrap()
            .get("score")
            .and_then(Value::as_f64)
            .unwrap();
        assert_ne!(
            through_serde.to_bits(),
            want.to_bits(),
            "serde now reads this value exactly — the test has stopped biting"
        );
        assert_eq!(exact_score(&line).unwrap().to_bits(), want.to_bits());
        // …AND AS A STRING, which is how anything that batches the line writes
        // it. The same literal, the same f64.
        let quoted = format!(r#"{{"identity":"k","ruler":"r","score":"{want}"}}"#);
        assert_eq!(exact_score(&quoted).unwrap().to_bits(), want.to_bits());
    }

    #[test]
    fn a_fact_carries_its_mode_apart_from_the_build() {
        let dir = std::env::temp_dir().join(format!("wfsim-facts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("facts.ndjson");
        let _ = std::fs::remove_file(&path);
        let mut log = super::FactLog::open(
            Some(path.to_string_lossy().into_owned()),
            Some("abc1234".into()),
        );
        let at = || Fact {
            score: 12.5,
            cost_seconds: 3.0,
            started_at: "T0".into(),
            finished_at: "T1".into(),
        };
        log.write("group_clear", "kpm", "orthos_prime|mods#heavy_slam", &at());
        log.write(
            "group_clear",
            "kpm",
            "no_mode_here",
            &Fact { score: 1.0, ..at() },
        );
        drop(log);

        let text = std::fs::read_to_string(&path).unwrap();
        let rows: Vec<serde_json::Value> =
            text.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        let _ = std::fs::remove_file(&path);
        assert_eq!(rows.len(), 2, "one line per fact: {text}");
        assert_eq!(rows[0]["identity"], "orthos_prime|mods");
        assert_eq!(rows[0]["mode"], "heavy_slam");
        assert_eq!(rows[0]["measured_by"], "abc1234");
        assert_eq!(rows[0]["ruler"], "group_clear");
        // THE UNITS TRAVEL WITH THE NUMBER. A row outlives the ruler file it
        // was measured under, and nothing else can say what it is in.
        assert_eq!(rows[0]["metric"], "kpm");
        // A KEY WITH NO MODE KEEPS THE WHOLE OF ITSELF as the identity, rather
        // than losing its last segment to an empty mode.
        assert_eq!(rows[1]["identity"], "no_mode_here");
        assert_eq!(rows[1]["mode"], "");
    }

    /// AND WITHOUT A PATH IT WRITES NOTHING AND SAYS NOTHING. The flag is
    /// optional while the store is migrating, and a scorer that failed without
    /// it would take the whole pipeline down for a file nothing reads yet.
    #[test]
    fn a_fact_log_with_nowhere_to_write_is_a_working_state() {
        let mut log = super::FactLog::open(None, None);
        log.write(
            "single_target",
            "kpm",
            "k#base",
            &Fact {
                score: 1.0,
                cost_seconds: 1.0,
                    started_at: "T0".into(),
                finished_at: "T1".into(),
            },
        );
    }
}
