//! THE SCORER — turns submitted builds into a board.
//!
//! Reads the library as a JSON array on stdin, reads the open generation's
//! FACTS as a file, and writes one file per weapon plus a cross-weapon index.
//! Fetching either and committing the result are the workflow's job
//! (`.github/workflows/scores.yml`), and neither needs the engine. What needs
//! the engine is the only thing here — running each build under the benchmark
//! and reading the number off.
//!
//! WHY A SUBMISSION CARRIES NO SCORE: nobody's number is trusted because
//! nobody's number is asked for. A row's score is produced HERE under the
//! benchmark's own pinned seed, so anyone with the repo reproduces any row
//! exactly.
//!
//! IT NEVER SPEAKS TO THE DATABASE. Files in, files out; the network lives in
//! `scripts/ship_facts.sh` and `scripts/fetch_facts.sh`, which a stub `curl`
//! can drive — which is what makes every hop of the chain testable.
//!
//!   cat library.json | wfsim-board single_target site/board //!     --facts-in facts-known.ndjson --facts facts.ndjson

use std::io::Read;

use serde_json::value::RawValue;
use serde_json::{json, Value};

/// A benchmark id without its `_v<n>` suffix — `single_target_v2` and
/// `single_target_v1` are the same ruler, and a build aimed at
/// either belongs on the current one's board.
fn family(id: &str) -> &str {
    match id.rsplit_once("_v") {
        Some((head, tail)) if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) => head,
        _ => id,
    }
}

/// DOES A ROW ALREADY IN A WEAPON'S FILE SURVIVE THIS PASS?
///
/// A weapon's file is written WHOLE by a tool that measures one ruler, so every
/// other ruler's rows are read back and carried. Two of them are not:
///
///   * THIS RULER'S, which are replaced from the facts — by FAMILY, so a
///     `_vN` row is the same ruler at another version and goes with it;
///   * A RETIRED RULER'S. Nothing ever invokes this tool with an id the roster
///     no longer has, so such a row would be carried for ever, and
///     `every_published_row_is_a_legal_build` would fail on it with no pass
///     able to clear it. Retiring a ruler is deleting its file; this is what
///     makes that enough.
///
/// A row that does not parse is carried: this pass measured nothing about it,
/// and dropping what it cannot read would lose a row to a shape change.
fn carried(row_benchmark: Option<&str>, bench_id: &str, live: &std::collections::BTreeSet<&str>)
    -> bool
{
    let Some(b) = row_benchmark else { return true };
    family(b) != family(bench_id) && live.contains(family(b))
}

/// One scored row, before it is trimmed to the top N.
struct Row {
    weapon: String,
    /// THE BUILD THIS ROW IS, without the mode — `builds::build_id`, which is
    /// what the `builds` table is keyed by.
    ///
    /// Carried so the run can prove every validated build ended up somewhere:
    /// published, or deferred. It is NOT written to the yaml, which
    /// states the build itself and from which the id is recomputed — a stored
    /// copy of a derived fact is the one that goes stale.
    identity: String,
    /// HOW the weapon was played — `base`, `cycle`, `alternate`. Part of the
    /// entrant's identity, not of the fight: a Torid through its Incarnon
    /// cycle and a Torid that never transmutes are two things to hold, and a
    /// board that keeps one row for both can only ever show whichever the
    /// benchmark happened to pin.
    mode: String,
    score: f64,
    mods: Vec<String>,
    evolutions: Vec<String>,
    arcanes: Vec<String>,
    /// An ADVERSARY weapon's progenitor element. Empty on every other weapon.
    /// The BONUS is not a row field: the ruler scores every row at the roll's
    /// maximum, which is investment rather than a choice.
    valence: String,
    /// THE EXILUS SLOT'S MOD, empty on almost every row. Optional as of
    /// 2026-08-25 — see `benchmarks_data::BuildRequirement::allows_exilus`.
    exilus: String,
    /// THE PARTS, on a modular weapon's row; empty on every other. They are the
    /// BUILD — a grip sets damage, fire rate and the charge — so a row that
    /// dropped them would publish a number for a weapon nobody submitted, and
    /// two assemblies of one chamber would be one row.
    grip: String,
    loader: String,
    /// THE RIVEN THIS BUILD CARRIES, as a SHAPE — which stats and which is the
    /// malus, never a roll. A row states a shape and the shape is scored at its
    /// own ceiling (`rivens_data::perfect`), for the reason every row is scored
    /// at full Forma: what one copy landed on is luck, and the board does not
    /// rank luck.
    ///
    /// WHERE it sits is in `mods`, which carries `riven` at its own position —
    /// an elemental riven pairs with the build's other elementals, so position
    /// is part of the build.
    ///
    /// The ROLLS the scorer settled on go in `riven_rolls`, because opening
    /// this row has to be able to build that riven on the reader's machine.
    riven: Option<RowRiven>,
}

/// A row's riven: the SHAPE it states, and the ROLLS this engine found best for
/// it. The shape is what a player acts on; the rolls are what the number rests
/// on and what a reader needs to reproduce it.
#[derive(Debug, Clone, PartialEq)]
struct RowRiven {
    bonuses: Vec<String>,
    malus: Option<String>,
    /// One roll per stat, bonuses first then the malus — the corner
    /// `rivens_data::perfect` picked for THIS fight. Not part of the identity.
    rolls: Vec<f64>,
}

/// …AND THE PART OF ONE THAT IS READ BACK. Only the four fields the carry and
/// the index decide on: everything else travels as bytes, because a publish that
/// reparses a number it is not measuring publishes a number nobody computed.
#[derive(serde::Deserialize)]
struct Published<'a> {
    benchmark: &'a str,
    #[serde(default)]
    mode: Option<&'a str>,
    #[serde(borrow)]
    score: &'a RawValue,
    #[serde(default, borrow)]
    riven: Option<&'a RawValue>,
}

/// A row the page would not understand is a row this cannot speak for either, so
/// it is CARRIED and never indexed — dropping it would take it off the board.
fn published(raw: &RawValue) -> Option<Published<'_>> {
    serde_json::from_str(raw.get()).ok()
}

/// ONE ROW AS THE PAGE RECEIVES IT — `site/board/<weapon>.json`'s shape, in one
/// place.
///
/// Extracted so it can be TESTED, because the one thing that went wrong with it
/// is invisible from the outside: it was a hand-written list of nine keys and
/// `riven` was never one of them, so a riven row reached the yaml and never
/// reached the page. Three separate reports came out of that single missing key
/// — the board's "riven only" view listed nothing, the builder could not group
/// riven builds apart, and TAKING one left an empty mod slot.
///
/// BYTE FOR BYTE WITH `build_site_app.py`, which is what makes a local site
/// build a no-op against the scorer's own output. That is why the riven key is
/// OMITTED rather than written as `null` on a plain row: the Python side copies
/// the yaml entry, and a plain entry simply has no `riven` key.
fn page_row(bench_id: &str, r: &Row) -> Value {
    let mut row = json!({
        "benchmark": bench_id,
        "mode": r.mode,
        "source": "submissions",
        "score": r.score,
        // The number stays EXACT and the string beside it is what the page
        // prints. Formatting lives in `boards_data::format_score`, so "four
        // significant figures, four decimals" is one rule in one language
        // rather than a Rust copy and a JS copy that drift.
        "shown": wfsim_engine::boards_data::format_score(r.score),
        "mods": r.mods,
        "evolutions": r.evolutions,
        "arcanes": r.arcanes,
        "valence": r.valence,
    });
    if let Some(rv) = &r.riven {
        if let Some(o) = row.as_object_mut() {
            o.insert(
                "riven".into(),
                json!({ "bonuses": rv.bonuses, "malus": rv.malus, "rolls": rv.rolls }),
            );
        }
    }
    // THE EXILUS SLOT'S MOD, on the same terms as the riven above: OMITTED on a
    // row that wears none rather than written as an empty string, because the
    // Python side copies the yaml entry and a row without one simply has no key.
    if !r.exilus.is_empty() {
        if let Some(o) = row.as_object_mut() {
            o.insert("exilus".into(), json!(r.exilus));
        }
    }
    // …AND THE PARTS. The builder reads `row.grip` / `row.loader` to open a
    // board row as a build, so a row that omitted them opened as the chamber's
    // DEFAULT assembly — a different weapon from the one the number is for.
    if !r.grip.is_empty() {
        if let Some(o) = row.as_object_mut() {
            o.insert("grip".into(), json!(r.grip));
            o.insert("loader".into(), json!(r.loader));
        }
    }
    row
}

/// WHAT A ROW IS ASSUMED TO COST when nothing has measured it — a new build, or
/// a board written before costs were recorded.
///
/// THE MEDIAN ROW, counted on a published board: 3.6 s across 7,659 of them,
/// against 16.9 at the ninetieth percentile, 65.4 at the ninety-ninth and 281
/// for the worst — a spread of 79x, which is why the packing only has to keep
/// the monsters apart and a monster is measured the first time it runs.
///
/// IT IS THE MEDIAN AND NOT A ROUND NUMBER because it is charged to every row
/// nobody has scored, and a run takes ~450 of those: guessing one second
/// under-charges the whole backlog fourfold and hands one shard the tail.
const DEFAULT_ROW_SECONDS: f64 = 3.6;

/// THE FEW SCALARS THE RUNTIME ASKS OF A BOARD, and the only part of one that
/// is embedded. See its own header for why the rows are not.
/// THE CROSS-WEAPON INDEX, under the same directory as the weapon files and
/// therefore excluded from them by name. `index` is not a weapon id: ids are
/// `[a-z0-9_]` slugs off the roster and none of them is this.
const INDEX_STEM: &str = "index";

const BOARD_STATE: &str = "data/board_state.yaml";

/// WHAT THE LAST RUN LEFT, read the way the writer below spells it.
///
/// BY LINE, not by a yaml crate: the shape is this program's own output and
/// never anybody's input, so a parser dependency here would be one more thing
/// that can disagree with the writer. Read from DISK rather than from the
/// compiled-in copy, because publish rewrites the file after the binary was
/// built and the cursor has to be the one the last run actually stored.
#[derive(Default, Clone)]
struct BoardRow {
    submissions: usize,
    listed: usize,
    scored_at: usize,
}

fn stored_state() -> std::collections::BTreeMap<String, BoardRow> {
    let mut state: std::collections::BTreeMap<String, BoardRow> = Default::default();
    let text = std::fs::read_to_string(BOARD_STATE).unwrap_or_default();
    let mut cur = String::new();
    for line in text.lines() {
        let t = line.trim_end();
        if let Some(id) = t.strip_prefix("  ").and_then(|x| x.strip_suffix(':')) {
            if !id.starts_with(' ') {
                cur = id.to_string();
                state.entry(cur.clone()).or_default();
            }
        } else if let Some((k, v)) = t.trim().split_once(": ") {
            let n = v.trim().parse::<usize>().unwrap_or(0);
            if let Some(e) = state.get_mut(&cur) {
                match k.trim() {
                    "submissions" => e.submissions = n,
                    "listed" => e.listed = n,
                    "scored_at_epoch_seconds" => e.scored_at = n,
                    _ => {}
                }
            }
        }
    }
    state
}


const BOARD_STATE_HEADER: &str = "# WHAT THE RUNTIME KNOWS ABOUT EACH BOARD — GENERATED by `wfsim-board`.
#
# THE ROWS ARE NOT HERE and must never be. `data/` is compiled into the binary,
# so a board archive under it put every row of every board into the wasm that
# every visitor downloads. The rows live in `site/board/<weapon>.json`, which
# the page FETCHES, and durably in the `scores` table they were published from.
#
# `submissions` paired with the library's own size (`/api/board/pending`) is how
# a STATIC board says how far behind it is. `listed` is every row it scored,
# because every scored row is published — how deep to read is the reader's
# question and the page answers it (docs/BOARD.md).
";

/// WHICH SHARD PAYS FOR THIS ROW — the least loaded one — and the load is
/// charged to it.
///
/// LIST SCHEDULING, and it is the whole of the packing. Every shard walks the
/// same rows in the same order and keeps the same array, so they agree on the
/// answer without talking to each other: a decision, not a negotiation. That is
/// the property `row_idx % shards` had and the reason this could replace it in
/// place rather than needing a planning pass over the whole board first.
///
/// IT IS NOT OPTIMAL AND DOES NOT NEED TO BE. The makespan it produces is
/// within 2x of the best possible split; what it has to do is keep the few
/// monster rows apart, which a modulo leaves to luck — a board is walked weapon
/// by weapon, so expensive rows arrive in runs and a stride sharing a factor
/// with the shard count puts them all on one worker.
fn charge(load: &mut [f64], cost: f64) -> usize {
    let mine = load
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map_or(0, |(i, _)| i);
    load[mine] += cost;
    mine
}

/// A flag's value, `--name value` anywhere after the positionals.
fn flag(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|x| x == name)
        .and_then(|i| a.get(i + 1).cloned())
}

fn has_flag(name: &str) -> bool {
    std::env::args().any(|x| x == name)
}

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
fn exact_score(line: &str) -> Option<f64> {
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
fn identity_of(key: &str) -> String {
    key.rsplit_once('#').map_or_else(|| key.to_string(), |(i, _)| i.to_string())
}

/// THE CARD A BUILD NAMES — its own rolls, or the god roll if it names none.
///
/// ONE READER, so a row's card cannot depend on whether the row was fought or
/// reused. The fact carried a copy while a record could state only a shape;
/// nothing made the two agree, and two copies of one truth is a rule about
/// which wins waiting to be needed.
///
/// A RECORD NAMING NO ROLLS is what the library held before `wfsim-intake`
/// resolved them, and the god roll is what it would resolve to today.
fn card_of(
    v: &wfsim_engine::builds::ValidBuild,
    shape: &wfsim_engine::rivens_data::RivenShape,
) -> Vec<f64> {
    if !v.riven_rolls.is_empty() {
        return v.riven_rolls.clone();
    }
    let cls = wfsim_engine::rivens_data::class_for_weapon(&v.weapon).unwrap_or("");
    let g = wfsim_engine::rivens_data::god_roll(shape, cls);
    g.bonuses.iter().map(|b| b.roll).chain(g.malus.iter().map(|m| m.roll)).collect()
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
struct Fact {
    score: f64,
    cost_seconds: f64,
    /// The riven corner the search settled on, when there is one. It travels
    /// WITH the score because it was found by the same fight: reusing one
    /// without the other would publish a number for a riven nobody can build.
    /// BOTH ENDS OF THE FIGHT. `cost_seconds` is not derivable from them: a row
    /// the clock stopped and resumed spans a wall clock longer than the runs it
    /// contains, and what the packing needs is the runs.
    started_at: String,
    finished_at: String,
}

/// WHAT THE DATABASE HOLDS FOR ONE RULER: the last measurement of each row.
///
/// ONE INDEX, because there is one fact. It was two — by `(row, what it read)`
/// and by the row alone — and every reader had to be told which of the two it
/// was: a pass that can REFIGHT must not reuse a number whose data moved, a
/// pass that only ASSEMBLES must publish it anyway or the row leaves the board.
/// That is not two facts, it is one fact and two questions, and the fact
/// carries what both of them need.
type Facts = std::collections::HashMap<String, Fact>;


/// A WALL CLOCK AS THE DATABASE WANTS IT: seconds, UTC, no fraction. The
/// column is TEXT and the only thing that ever compares two of them is "which
/// is newer", which this ordering answers lexically.
fn stamp(at: &std::time::SystemTime) -> String {
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
struct FactLog {
    out: Option<std::io::BufWriter<std::fs::File>>,
    measured_by: String,
}

impl FactLog {
    fn open(path: Option<String>, measured_by: Option<String>) -> Self {
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
    fn write(&mut self, ruler: &str, metric: &str, key: &str, f: &Fact) {
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
fn load_facts(spec: Option<String>, bench_id: &str) -> Facts {
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

/// WHAT SOMEBODY ASKED THIS RULER TO MEASURE, in the order they will be done —
/// as `scripts/fetch_queue.sh` wrote it, one json object a line.
///
/// A RUN DOES NOT DECIDE WHAT TO COMPUTE, IT READS IT. The order is the
/// database's (a batch's `at`, then the key), taken once and never re-derived,
/// so every shard is handed the same list and they agree on it without talking.
///
/// AN ABSENT FILE IS NOT AN EMPTY QUEUE — it is "no queue was given", which is
/// what `--project` and a local run mean, and there the fact alone decides.
fn load_queue(spec: Option<String>, bench_id: &str) -> Option<Vec<(String, String)>> {
    let path = spec?;
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("queue: cannot read {path}");
        return Some(Vec::new());
    };
    let owed: Vec<(String, String)> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|v| v.get("ruler").and_then(Value::as_str) == Some(bench_id))
        .filter_map(|v| {
            Some((
                v.get("build_id").and_then(Value::as_str)?.to_string(),
                v.get("mode").and_then(Value::as_str).unwrap_or("base").to_string(),
            ))
        })
        .collect();
    eprintln!("queue: {} row(s) owed on {bench_id}", owed.len());
    Some(owed)
}




fn main() {
    let bench_id = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!(
            "usage: wfsim-board <benchmark-id> [site/board] [--shard i/n] \
                   [--facts-in <file>] [--facts <file>] [--measured-by <sha>] \
                   [--project]  (library on stdin)"
        );
        std::process::exit(2);
    });
    // WHICH SLICE OF THE SUBMISSIONS THIS PROCESS SIMULATES. By INDEX in the
    // stdin array rather than by any property of the row: every shard is handed
    // the same file, so the split is identical without the shards agreeing on
    // anything else. A build submitted twice can land in two shards and be
    // simulated twice — the merge dedups by identity, and paying for one extra
    // fight is cheaper than a coordination scheme that would not.
    let (shard, shards) = match flag("--shard") {
        Some(s) => {
            let (i, n) = s.split_once('/').unwrap_or(("0", "1"));
            (
                i.parse::<usize>().unwrap_or(0),
                n.parse::<usize>().unwrap_or(1).max(1),
            )
        }
        None => (0, 1),
    };
    // WHAT IS ALREADY MEASURED, AND NOTHING ELSE. A row with a fact for what it
    // reads is done; a row without one is work. That set difference is the whole
    // of what a run decides, and it replaces a prior board, a merged store and a
    // directory of this run's own artifacts — three sources that could
    // disagree, and a rule about which of them won.
    let facts = load_facts(flag("--facts-in"), &bench_id);
    let mut reused = 0usize;

    // ---- WHAT THIS RUN IS ALLOWED TO FIGHT ----------------------------
    //
    // HOW MANY OF THE QUEUE'S ROWS A RUN TAKES ON. Without it the backlog is
    // unbounded, so a run has to clear all of it before anything is published
    // — 4,570 rows and hours of it, during which the board shows the number it
    // showed yesterday. Bounded, each run publishes a board with more rows on
    // it than the last.
    let new_limit = flag("--new-limit").and_then(|s| s.parse::<usize>().ok());
    // WHAT IS OWED, AND WHAT THIS RUN TAKES OF IT — the front of the queue,
    // which is where the ORDER lives: a batch jumps the line by its own `at`,
    // and truncating here is what makes that ordering mean something when the
    // run cannot do all of it.
    let queued = load_queue(flag("--queue-in"), &bench_id);
    let owed: Option<std::collections::HashSet<(String, String)>> =
        queued.as_ref().map(|q| q.iter().cloned().collect());
    let taking: Option<std::collections::HashSet<(String, String)>> = queued.as_ref().map(|q| {
        q.iter().take(new_limit.unwrap_or(usize::MAX)).cloned().collect()
    });
    // …AND WHAT NOTHING HAS ASKED FOR YET, written out for the reconciliation.
    // The queue is written by hand — intake for an arrival, a person for a
    // rescore — and a hand-written list's one failure is a row nobody wrote,
    // which would never be computed and never be noticed. This names them; the
    // shipper puts them in a batch.
    // …AND THE WEAPONS A SWEEP NAMED, whose rows go in the same file whatever
    // they already carry.
    //
    // A SAFETY NET RATHER THAN A CORRECTION. Nothing here says a stored number
    // is wrong: it says nobody has looked at that weapon in a long time, and a
    // board is a claim about what this code computes TODAY. Whole weapons,
    // because a weapon is the publication unit — half a weapon re-measured is a
    // file ranking two generations against each other.
    let sweep: std::collections::BTreeSet<String> = flag("--queue-weapons")
        .and_then(|p| std::fs::read_to_string(&p).ok())
        .map(|t| t.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
        .unwrap_or_default();
    let mut missing_out = flag("--queue-missing").and_then(|p| {
        std::fs::File::create(&p)
            .map_err(|e| eprintln!("queue: cannot write {p}: {e}"))
            .ok()
            .map(std::io::BufWriter::new)
    });
    let mut missing = 0usize;
    // WHEN THE RUN STOPS TAKING ON WORK, in seconds of wall clock.
    //
    // A BUDGET PREDICTS AND A DEADLINE GUARANTEES, and a count can only
    // predict: rows differ 79x, so a limit of 150 is nine minutes or fifty
    // depending on which builds arrived. Both are here because the count is
    // what every shard can agree on without talking, and the clock is what
    // each one reads for itself.
    // ASSEMBLE, NEVER FIGHT. The publish pass computes almost nothing already;
    // this makes that a guarantee instead of an outcome, so a shard that failed
    // costs a row on this board rather than an hour on the merge.
    let project = has_flag("--project");
    let mut absent = 0usize;
    let deadline = flag("--deadline")
        .and_then(|s| s.parse::<u64>().ok())
        .map(std::time::Duration::from_secs);
    let mut paused = 0usize;
    // A row's banked progress, by key. Emptied as each row is taken up and
    // refilled only where the clock stopped one.
    let mut partials_out: std::collections::HashMap<String, Partial> = Default::default();
    let started = std::time::Instant::now();
    let dry = has_flag("--dry-run");
    let mut todo = 0usize;
    // …AND WHAT IT IS EXPECTED TO COST, which is the number the SPLIT is sized
    // from. A count cannot answer that: rows differ by 79x, so 3,000 of them is
    // nine minutes or fifty depending on which builds arrived. Each row is
    // charged what whoever last measured it paid, and a row nobody has measured
    // takes the median.
    let mut work_seconds = 0.0f64;
    let mut fresh_seen = 0usize;
    let mut fresh_left = 0usize;
    // WHOSE ROWS WERE DEFERRED, as identities. The accounting below asserts
    // that every validated build reached a row, and a bounded run makes that
    // false ON PURPOSE — a build the budget did not reach this time is queued,
    // not lost, which is a FOURTH outcome and has to be one the check knows.
    let mut deferred_ids: std::collections::BTreeSet<String> = Default::default();
    // THE ASSEMBLY TAKES ITS NUMBERS FROM THE FACTS AND NOWHERE ELSE.
    //
    // `--project` is the pass that PUBLISHES, and a publisher with more than
    // one source needs a rule for which one wins — which is where every defect
    // this pipeline has produced has lived. So under it there is one source: a
    // row with a fact is published from that fact, and a row without one is not
    // a row.
    //
    // Measured before this: the three highest rows of a Ballistica group were
    // 3992.29, 3934.90 and 3928.68, and not one of them had a fact — they were
    // the prior board's, published beside freshly measured ones half their
    // size.
    // WHERE EVERY SCORE THIS RUN MEASURES IS APPENDED, one line a row and
    // flushed per row, so a shard that dies has banked what it finished.
    let mut log = FactLog::open(flag("--facts"), flag("--measured-by"));

    // WHAT THIS RUN MEASURED, for the accounting line at the end.
    let mut computed: std::collections::HashMap<String, f64> = Default::default();

    let bench = wfsim_engine::benchmarks_data::get(&bench_id).unwrap_or_else(|| {
        eprintln!("unknown benchmark: {bench_id}");
        std::process::exit(2);
    });

    let mut raw = String::new();
    std::io::stdin().read_to_string(&mut raw).expect("stdin");
    let subs: Vec<Value> = serde_json::from_str(&raw).unwrap_or_default();

    // The benchmark's scenario, as the wire shape `simulate_json` parses. It is
    // the SAME map the app sends, which is what stops the board and the page
    // from measuring two different fights.
    let scenario: Value = serde_json::to_value(&bench.scenario).expect("scenario");
    // WHAT THIS RULER JUDGES BY, asked of the benchmark rather than resolved
    // here. A ruler names exactly one core and `benchmarks_data` is where that
    // rule is applied, so this cannot fall back on anything: a default reached
    // at the point of use publishes a whole ranking in the units of a question
    // nobody asked, and the number looks exactly like a right one.
    let metric = bench.metric();
    let duration = scenario
        .get("duration")
        .and_then(Value::as_f64)
        .unwrap_or(300.0);
    // THE ROW'S NUMBER IN THE RULER'S OWN UNITS, said once. `score` off the
    // wire is kill PROGRESS over the whole engagement — kills plus the fraction
    // of the current target depleted — so a `kpm` ruler turns it into a rate
    // and a `dps` one reads a different field entirely.
    let score_in = |out: &Value| -> f64 {
        metric.of(
            out.get(metric.field).and_then(Value::as_f64).unwrap_or(0.0),
            duration,
        )
    };

    let mut rows: Vec<Row> = Vec::new();
    let (mut seen, mut refused) = (0usize, 0usize);
    // …AND THE ROWS, counted separately from the submissions because one
    // submission is now one row per mode.
    // WHAT EACH SHARD IS CARRYING, in seconds of measured work — the input and
    // the output of `charge`, which decides whose row each one is.
    let mut load = vec![0.0f64; shards];
    let mut seen_ids: std::collections::HashSet<String> = Default::default();
    // EVERY BUILD THAT PASSED THE DOOR, by identity. The accounting below
    // partitions this set; anything left over is a build the library holds and
    // this board silently did not rank.
    let mut scored_ids: std::collections::BTreeSet<String> = Default::default();
    for s in subs {
        // EVERY SUBMISSION IS A CANDIDATE FOR EVERY RULER.
        //
        // A submission has never carried a score — it carries a BUILD, and the
        // number is produced here. So the ruler it happened to be measured
        // under was never a property of the record; it was a gate, and the gate
        // was expensive: of 914 distinct builds players have submitted, only 46
        // had ever been scored on more than one board. Ninety-five per cent of
        // everything anyone had contributed was being read once and then held
        // back from the two boards it could also have answered.
        //
        // A build that is not admissible here is refused below like any other
        // and its reason printed, which is what the ruler's own admission rule
        // is for. Nothing filters by benchmark any more: THE STORE IS A LIBRARY
        // OF BUILDS and each ruler crosses the whole of it.
        //
        // This is also what makes a NEW ruler cost no community effort: it is
        // scored from the library the day it lands, rather than waiting for
        // players to resubmit everything under it.
        seen += 1;
        let weapon = s
            .get("weapon")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let get = |k: &str| -> Vec<String> {
            s.get(k)
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default()
        };
        let (mods, evos, arcs) = (get("mods"), get("evolutions"), get("arcanes"));

        // THE SAME CHECK A BOARD ROW FACES ANYWHERE. A submission arrives over
        // a network with no UI on the path, so "could a player equip this" is
        // asked here rather than assumed — and it NORMALISES first, so what
        // gets scored and what gets published are the same object.
        // THE BOARD'S door, not the legality one: a row must be a COMPLETE
        // build. A submission that is merely legal is refused
        // here and simply never scored.
        // THE REASON IS PRINTED, not counted. "2 refused" is a number that
        // tells nobody anything — including me, on the day two complete-looking
        // Dual Toxocyst builds were turned away and the log said only that they
        // were. A board that refuses in silence cannot be debugged
        // by the person whose build it refused, either.
        // AN ADVERSARY WEAPON'S PROGENITOR ELEMENT is part of the submission,
        // like its mods and its evolutions — a different element is a different
        // build, not a weaker one. `builds::validate` refuses one the weapon
        // cannot have and refuses a MISSING one on a weapon that always has
        // one, so neither can arrive by omission — a legality rule rather than
        // a ruler's, since a build without an element is not a build a ruler
        // declines, it is not a build.
        let valence = s.get("valence").and_then(Value::as_str).unwrap_or("");
        // A RIVEN'S SHAPE, when the submission carries one. Two flat lists, the
        // way the endpoint stores them: the ROLLS are never submitted because
        // they are never ranked — `rivens_data::perfect` finds this shape's own
        // best corner for this fight, below.
        let shape = {
            let bonuses = get("riven_pos");
            let malus = s
                .get("riven_neg")
                .and_then(Value::as_str)
                .filter(|x| !x.is_empty());
            (!bonuses.is_empty()).then(|| wfsim_engine::rivens_data::RivenShape {
                bonuses: {
                    let mut b = bonuses;
                    b.sort();
                    b
                },
                malus: malus.map(String::from),
            })
        };
        // THE EXILUS SLOT'S MOD. Optional as of 2026-08-25 — see
        // `benchmarks_data::BuildRequirement::allows_exilus` — and its own
        // field on the wire because a flat `mods` list cannot say which entry
        // came out of the exilus slot.
        let exilus = s
            .get("exilus")
            .and_then(Value::as_str)
            .filter(|x| !x.is_empty());
        // THE PARTS, flat, exactly as the worker stores them and as the page's
        // own door reads them (`webapi::board_assembly_of`). The chamber is the
        // weapon's, never the record's.
        let asm = {
            let g = s.get("grip").and_then(Value::as_str).unwrap_or("");
            let l = s.get("loader").and_then(Value::as_str).unwrap_or("");
            (!(g.is_empty() && l.is_empty())).then(|| wfsim_engine::kitguns_data::Assembly {
                // The chamber's WEAPON id, which is what `Assembly` holds.
                chamber: wfsim_engine::weapons_data::spec(&weapon)
                    .and_then(|sp| sp.kitgun.clone())
                    .and_then(|r| wfsim_engine::kitguns_data::default_assembly(&r))
                    .map(|d| d.chamber)
                    .unwrap_or_default(),
                grip: g.to_string(),
                loader: l.to_string(),
            })
        };
        let v = match wfsim_engine::builds::validate_for_board_with(
            &bench_id,
            &weapon,
            &mods,
            &evos,
            &arcs,
            valence,
            shape.as_ref(),
            exilus,
            asm.as_ref(),
        ) {
            Ok(v) => v,
            Err(e) => {
                // THE BUILD, not just the weapon. "refused burston_prime:
                // needs 64 of 60" says a build was turned away and leaves
                // "which one, and was it really impossible?" unanswerable —
                // which is the question asked of this log the first time
                // somebody's submission went missing. The
                // whole row is what makes a refusal checkable by hand.
                eprintln!(
                    "refused {weapon}: {e}
  mode={} mods=[{}] evolutions=[{}] arcanes=[{}] valence={}",
                    s.get("mode").and_then(Value::as_str).unwrap_or("—"),
                    mods.join(", "),
                    evos.join(", "),
                    arcs.join(", "),
                    if valence.is_empty() { "—" } else { valence },
                );
                refused += 1;
                continue;
            }
        };

        // …AND THE NUMBERS ITS RIVEN ROLLED, if the record names them. They are
        // part of the fight, so they are part of the identity every key here is
        // taken from: two ends of one shape are two builds with two numbers.
        //
        // A RECORD THAT NAMES NONE STATES ONLY A SHAPE, which is what the
        // library held before `wfsim-intake` resolved them, and the branch in
        // the scoring loop below searches for the corner as it always did.
        let v = match s.get("riven_rolls").and_then(Value::as_array) {
            Some(rolls) => v.with_riven_rolls(
                rolls.iter().filter_map(Value::as_f64).collect::<Vec<_>>(),
            ),
            None => v,
        };

        // IT PASSED THE DOOR, so it owes a row somewhere. Recorded before the
        // modes are enumerated, because what has to be provable is that a
        // VALIDATED build was ranked — not that some particular mode of it was.
        scored_ids.insert(wfsim_engine::builds::build_id(&v));
        // EVERY MODE THIS WEAPON CAN BE PLAYED IN, and not the one the
        // submitter happened to try.
        //
        // THE MODE WAS NEVER A PROPERTY OF THE RECORD, for the same reason the
        // ruler was not: a submission carries a BUILD. Mods are equipped on the
        // WEAPON and a mode is how it is fired, so every mode of that weapon is
        // a fight this same build can answer — nothing about it can become
        // illegal by being played differently. Some of what it carries pays
        // nothing in some of them; that costs a low row, which the floor and
        // the per-mode dedup drop.
        //
        // A FORM'S UNLOCKING EVOLUTION IS IMPLIED, not required of the
        // submitter — `webapi`'s `form_unlock_evo` already decides that, and it
        // carries no stat: tier 1 of an Incarnon ladder is `fixed`, so the form
        // and the evolution are two controls for one fact.
        //
        // AN UNSUSTAINABLE MODE IS STILL REFUSED. "Always Incarnon" is not a
        // way to play for three hundred seconds, and a board may not rank a
        // fight nobody can hold — derived from the mode, so no benchmark has to
        // carry a list of what it will not take.
        let modes: Vec<wfsim_engine::weapons_data::WeaponPlayMode> =
            wfsim_engine::weapons_data::play_modes(&v.weapon)
                .into_iter()
                .filter(|m| m.sustainable)
                .collect();
        if modes.is_empty() {
            eprintln!("refused {weapon}: it has no mode that can be sustained for an engagement");
            refused += 1;
            continue;
        }
        for played in modes {
            // ONE BUILD, SCORED ONCE PER MODE. The clone is the row's own copy:
            // `Row` takes the vectors by value and there is a row per mode.
            let v = v.clone();
            let mut req = wfsim_webapi::simulate_request(&scenario, &v, played);
            // ONE ROW PER BUILD, and the identity is computed BEFORE the fight
            // because it decides whether there is one to run at all, rather than
            // being computed afterwards for dedup alone.
            //
            // The endpoint stores what was submitted, verbatim — it has no mod pool
            // and cannot tell an elemental mod from any other — so two spellings of
            // one fight arrive as two records and are collapsed HERE, where
            // `validate` has already put both into the same canonical form. The
            // MODE is part of that identity: one build played two ways is two
            // entrants, and collapsing them would keep whichever arrived first.
            let key = wfsim_engine::builds::board_key(&v, played.id);
            if !seen_ids.insert(key.clone()) {
                continue;
            }
            // A FACT IS REUSED BECAUSE IT EXISTS, and that is the whole of
            // the rule. Nothing here asks how old it is or which build wrote
            // it: age is not evidence, and a hash of the INPUTS was tried and
            // was a worse instrument than the one it replaced — it fired on
            // every edit to a file no entity owns, including files that cannot
            // move a number, and stayed silent on the one case that matters,
            // a code change that does.
            //
            // WHAT ASKS FOR A ROW AGAIN IS THE QUEUE. A stored number is not a
            // reason to skip a row somebody asked to have measured again, so a
            // row this run is TAKING is fought whatever it already carries —
            // and the old number stays published until the new one replaces it,
            // which is why asking costs the board nothing where deleting the
            // fact would have left a hole.
            let asked = (
                wfsim_engine::builds::build_id(&v),
                if played.id.is_empty() { "base".to_string() } else { played.id.to_string() },
            );
            let take = taking.as_ref().is_some_and(|t| t.contains(&asked));
            // …AND A ROW NOBODY ASKED FOR IS NOT WORK. Where there is no queue
            // at all — `--project`, a local run — the fact alone decides, which
            // is what this said before there was one.
            let current = if take { None } else { facts.get(&key) };
            if missing_out.is_some()
                && (sweep.contains(&v.weapon) || !facts.contains_key(&key))
                && !owed.as_ref().is_some_and(|o| o.contains(&asked))
            {
                use std::io::Write;
                let line = json!({
                    "build_id": asked.0, "ruler": bench_id, "mode": asked.1,
                });
                if let Some(out) = missing_out.as_mut() {
                    let _ = writeln!(out, "{line}");
                }
                missing += 1;
            }
            // THE SHARD IS A PROPERTY OF THE ROW, not of the submission it came
            // from: a melee weapon is seven rows off one record. `charge`
            // decides which, below, and every shard walks this same sequence
            // and skips only the SIMULATION — so they stay in step.
            //
            // THE CARD IS THE BUILD'S, whether this row is fought or reused.
            // It was on the FACT as well while a record could state only a
            // shape, and two copies of one truth is a rule about which wins
            // waiting to be needed: a reused row read the fact's, a fought one
            // read the build's, and nothing made them agree.
            let row_riven: Option<RowRiven> = v.riven.as_ref().map(|shape| RowRiven {
                bonuses: shape.bonuses.clone(),
                malus: shape.malus.clone(),
                rolls: card_of(&v, shape),
            });
            let score = match current {
                Some(f) => {
                    reused += 1;
                    f.score
                }
                None => {
                    // A ROW WITH NO FACT IS NOT ON THIS BOARD. `--project`
                    // ASSEMBLES and fights nothing: it groups what the facts
                    // hold, ranks it, applies the floor and writes the files.
                    // A row this generation has not measured lands on the next
                    // board — which is what makes the publish cost bounded by
                    // construction rather than by whichever shard fell over.
                    if project {
                        absent += 1;
                        deferred_ids.insert(identity_of(&key));
                        continue;
                    }
                    // A ROW THIS RUN IS NOT TAKING IS NOT ITS WORK. Under a
                    // queue that is the whole selection: what is owed and what
                    // this run took of it are decided before the walk, in the
                    // ORDER a person set, and a row outside that is left for a
                    // later run. The reconciliation is what guarantees it is
                    // owed at all, so nothing here can be forgotten.
                    if taking.is_some() && !take {
                        fresh_left += 1;
                        deferred_ids.insert(identity_of(&key));
                        continue;
                    }
                    // THE RUN HAS TAKEN ITS SHARE, and every row without a
                    // fact is a share: a generation is opened deliberately and
                    // starts empty, so "repair" and "never scored" are one set.
                    // That is why the bound is a count and a clock, and why
                    // convergence is over runs rather than inside one.
                    //
                    // COUNTED BEFORE THE SHARD FILTER, because every shard must
                    // reach the same verdict on the same row: a bound that
                    // stopped one and not another leaves them disagreeing about
                    // who owns the rows after it, and a row both believe is the
                    // other’s is a row nobody scores.
                    fresh_seen += 1;
                    if new_limit.is_some_and(|n| fresh_seen > n) {
                        fresh_left += 1;
                        deferred_ids.insert(identity_of(&key));
                        continue;
                    }
                    // WHOSE ROW IS THIS, charged to the least-loaded shard at
                    // the cost whoever last measured it paid. It survives a
                    // stale fact: the fight has to be redone, but how long it
                    // takes is a property of the build and the ruler, and those
                    // did not move. A row nobody has measured takes the neutral
                    // default, which degrades to round-robin and no worse.
                    //
                    // DECIDED INSIDE THE `None` ARM, because a row whose score
                    // is already known costs nothing to publish and must not be
                    // charged to anybody.
                    let cost = facts
                        .get(&key)
                        .map(|f| f.cost_seconds)
                        .filter(|c| *c > 0.0)
                        .unwrap_or(DEFAULT_ROW_SECONDS);
                    let mine = charge(&mut load, cost);
                    // Not this shard's slice: another one is simulating it right
                    // now, and publishing a row for it here would mean scoring it
                    // twice and ranking it once.
                    if shards > 1 && mine != shard {
                        continue;
                    }
                    // …AND THE CLOCK, WHICH ONLY THIS SHARD CAN READ. Spent
                    // differently in every shard, so it is asked AFTER the row
                    // has been dealt: a shard out of time drops rows of its OWN
                    // and leaves the deal itself untouched.
                    //
                    // IT BOUNDS REPAIRS TOO, which it could not while the
                    // assembly dropped a stale row: a repair not taken now
                    // keeps the number it has and is refought next run, and the
                    // board says how old it is. That is what replaced the
                    // slice, the cursor and the budget that steered them.
                    if deadline.is_some_and(|d| started.elapsed() > d) {
                        fresh_left += 1;
                        deferred_ids.insert(identity_of(&key));
                        continue;
                    }
                    if dry {
                        todo += 1;
                        work_seconds += cost;
                        continue;
                    }
                    // WHAT THIS ROW COST, when it cost enough to matter.
                    //
                    // The fan-out's efficiency is set by its SLOWEST shard, not by
                    // its total: measured on 2026-08-26 at 128 shards, 824
                    // shard-minutes of work finished in 35.5 because one shard took
                    // that alone — 6.4 minutes of mean work against a 35.5 minute
                    // makespan, **18% efficiency**. Raising the shard count barely
                    // touched it (32 -> 128 shards moved the worst shard only 52.9
                    // -> 35.5), which is the signature of a few very expensive ROWS
                    // rather than of a split that is too coarse.
                    //
                    // Balancing the deal needs to know what a row costs, and
                    // nothing here has ever measured that. This is the measurement,
                    // and it is a `eprintln` rather than a stored column on purpose:
                    // the question it answers — is the tail one row or twenty — is
                    // asked once, and a schema for it before that answer is known
                    // would be a guess wearing a table.
                    let began = std::time::Instant::now();
                    // THE WALL CLOCK TOO, because the fact records when the
                    // fight started and when it ended and `Instant` cannot say
                    // either out loud.
                    let began_at = stamp(&std::time::SystemTime::now());
                    // THE DEADLINE REACHES INSIDE THE ROW, which is what makes
                    // it a deadline. Checked before dealing, it only ever said
                    // when to stop TAKING rows — so one row set the makespan,
                    // and the board holds rows costing 95 minutes against a
                    // schedule that fires every 20. A row that runs out here is
                    // banked where it stopped and resumes on a later run.
                    //
                    // The SAME clock, not a second budget: a run is given a
                    // length once. `full` passes none and is unbounded, which is
                    // what it is for.
                    let row_deadline = deadline.map(|d| started + d);
                    // BANKED WITHIN THIS ROW AND NOWHERE ELSE. The clock can
                    // stop a fight between its runs and resume it a few lines
                    // down, which is what makes an arbitrarily slow row
                    // finishable; it does not survive the PROCESS, because a
                    // half-measured row is a fact under construction and the
                    // table holds facts. A row the clock stopped starts again.
                    let part = std::cell::RefCell::new(Partial::default());
                    // EVERY ROW IS MEASURED AT THE RULER'S OWN PRECISION. There
                    // is no screen: a list is published when every build in it
                    // has been measured, and a cheap probe deciding which ones
                    // to skip is a second kind of number on the same board.
                    //
                    // A RIVEN ROW IS SCORED AT ITS SHAPE'S CEILING, and finding
                    // that ceiling is a search: every corner of the roll band, at a
                    // CHEAP run count, then the winner measured properly at the
                    // ruler's own. Sixteen probes and one real measurement rather
                    // than sixteen real ones — the same "search cheaply, then
                    // measure the winner" the optimizer's `finalists x final_runs`
                    // is built on, and here it takes the cost of a riven row from
                    // 16x a plain one to about 2.6x.
                    //
                    // The corners are far apart, so picking between them does not
                    // need the precision the published number does.
                    if let Some(shape) = &v.riven {
                        let cls =
                            wfsim_engine::rivens_data::class_for_weapon(&v.weapon).unwrap_or("");
                        // THE BUILD NAMES ITS OWN CARD. `wfsim-intake`
                        // resolved the shape when the record entered the
                        // library — the god roll, unless a stat's sign had
                        // stopped saying which end was better — so the row
                        // measures the riven the record states and searches
                        // for nothing.
                        //
                        // A RECORD THAT NAMES NO ROLLS IS SCORED AT THE GOD
                        // ROLL, which is what it would have resolved to on
                        // every build the library holds.
                        let spec = shape.at(cls, &card_of(&v, shape));
                        if let Some(o) = req.as_object_mut() {
                            o.insert("rivens".into(), wfsim_webapi::riven_request(&spec));
                        }
                    }
                    // THE MEASUREMENT, IN AS MANY SITTINGS AS THE CLOCK ALLOWS.
                    // The ruler's run count is untouchable — it is the accuracy
                    // promise — so what bends is how many of those runs one
                    // board run pays for. `run_budgeted` merges the pieces into
                    // exactly what one call over the range produces.
                    let want = req.get("runs").and_then(Value::as_u64).unwrap_or(0) as u32;
                    let Some(out) = run_budgeted(
                        &mut part.borrow_mut(), "measure", &req, want, row_deadline,
                    ) else {
                        pause_row(&key, &part.borrow(), &mut partials_out, &mut deferred_ids);
                        paused += 1;
                        continue;
                    };
                    let ok = out.get("ok").and_then(Value::as_bool).unwrap_or(false);
                    let raw = out.get("score").and_then(Value::as_f64).unwrap_or(0.0);
                    if !ok || raw <= 0.0 {
                        eprintln!(
                            "refused {weapon}: did not simulate ({})",
                            out.get("error")
                                .and_then(Value::as_str)
                                .unwrap_or("scored zero")
                        );
                        refused += 1;
                        continue;
                    }
                    // IN THE RULER'S OWN METRIC — `score_in`, the same
                    // conversion the two probes above use. Publishing the raw
                    // figure under a `kpm` ruler labels a 180-second total as a
                    // per-minute rate: 55.26 on screen for a build that kills
                    // 11.05 a minute over 300 s. The RANKING survives either way,
                    // being a linear rescale; the number people read does not.
                    let s = score_in(&out);
                    computed.insert(key.clone(), s);
                    // …AND THE FACT IS DURABLE HERE, not when the run ends. A
                    // shard whose work becomes useful only once it FINISHES and
                    // then UPLOADS is a shard a service timeout can empty: one
                    // of 128 did exactly that, and cost a whole rescore — see
                    // docs/BOARD.md §"The pipeline, designed around one rule".
                    log.write(
                        &bench_id,
                        metric.id,
                        &key,
                        &Fact {
                            score: s,
                            cost_seconds: began.elapsed().as_secs_f64(),
                            started_at: began_at,
                            finished_at: stamp(&std::time::SystemTime::now()),
                        },
                    );
                    // THIRTY SECONDS is a row worth naming: the median row is under
                    // one, so this prints the tail and nothing else — a line per
                    // slow row rather than 2,474 lines nobody reads.
                    let took = began.elapsed().as_secs_f64();
                    if took >= 30.0 {
                        eprintln!(
                            "slow row: {:7.1}s  {}  key={key}  riven={}  evos={}  arcanes={}",
                            took,
                            v.weapon,
                            v.riven.is_some(),
                            v.evolutions.len(),
                            v.arcanes.len(),
                        );
                    }
                    s
                }
            };
            let exilus_for_row = v.exilus.clone().unwrap_or_default();
            rows.push(Row {
                identity: wfsim_engine::builds::build_id(&v),
                weapon: v.weapon,
                mode: played.id.to_string(),
                score,
                mods: v.mods,
                evolutions: v.evolutions,
                arcanes: v.arcanes,
                valence: v.valence,
                exilus: exilus_for_row,
                grip: v.assembly.as_ref().map(|a| a.grip.clone()).unwrap_or_default(),
                loader: v.assembly.as_ref().map(|a| a.loader.clone()).unwrap_or_default(),
                riven: row_riven,
            });
        }
    }

    // EVERY SCORED ROW IS PUBLISHED. A row is a FACT — a build measured under a
    // pinned seed — and holding one back made it indistinguishable from a build
    // that was lost, while the page had no way to ask for it. HOW DEEP TO READ
    // IS THE READER'S QUESTION, and it is theirs to answer: the page shows
    // builds within half their group's leader by default and will widen to all
    // of them (docs/BOARD.md).
    let mut kept = rows;
    kept.sort_by(|a, b| {
        a.weapon.cmp(&b.weapon).then(
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });

    // HOW MUCH OF THIS BOARD WAS KEPT rather than recomputed, said out loud. A
    // run that reuses everything and a run that scored everything look
    // identical from the outside, and the difference is an hour.
    eprintln!(
        "{seen} submissions, {refused} refused, {} rows ({reused} reused, {} scored here)",
        kept.len(),
        computed.len(),
    );
    // HOW MUCH BACKLOG IS LEFT, said out loud. A run that defers rows is not a
    // run that failed to score them: the next one takes the next share, and the
    // count falling run over run is what says the board is catching up.
    if absent > 0 {
        eprintln!("project: {absent} row(s) nobody has banked yet — they land on the next board");
    }
    // …AND HOW MANY RAN OUT OF CLOCK PARTWAY. Not the same as `fresh_left`,
    // which is a row never started: these carry banked progress and resume on
    // the next run, which is what makes an arbitrarily slow row finishable.
    if paused > 0 {
        eprintln!("paused: {paused} row(s) banked partway — they resume on the next run");
    }
    if fresh_left > 0 {
        let why = if deadline.is_some_and(|d| started.elapsed() > d) { "clock" } else { "count" };
        eprintln!(
            "new: {} of {fresh_seen} never-scored row(s) taken, {fresh_left} left for the next run ({why})",
            fresh_seen - fresh_left
        );
    }

    // EVERY STORED SUBMISSION IS ACCOUNTED FOR, and the run says so rather than
    // being trusted. Three outcomes and no fourth: refused at the door,
    // published, or deferred to the next run by a budget. A build that fell out
    // of all three
    // would be one the library holds and this board never looked at — the
    // failure mode that has to be impossible rather than unlikely, because from
    // the submitter's side it is indistinguishable from the other two.
    //
    // KEYED BY IDENTITY, not by submission: two players sending the same build
    // are ONE build, collapsed by `seen_ids`, and counting them as two would
    // make this fire on the healthy case.
    // A SHARD CANNOT ASK THIS. It skips every row that is not its slice, so its
    // own `kept` covers a fraction by construction — the question "did every
    // build get ranked" is only meaningful where every row was in scope, which
    // is the unsharded PUBLISH run.
    // ONE MACHINE-READABLE LINE, because the workflow reads it to decide
    // whether to fan out at all. It stands BEFORE the accounting
    // below, which asserts every validated build reached a row — true of a run
    // that scores and false by construction of one that only counts.

    // WHAT NOTHING HAS ASKED FOR YET, flushed before anything reads the file.
    // The reconciliation is the reason a hand-written queue cannot quietly lose
    // a row, so the count is said out loud on every run: a number that is not
    // zero after the first pass is a writer that is forgetting to enqueue.
    if let Some(out) = missing_out.as_mut() {
        use std::io::Write;
        let _ = out.flush();
        eprintln!("queue-missing: {missing} row(s) nothing has asked for on {bench_id}");
    }
    if dry {
        eprintln!(
            "dry-run: todo={todo} work={work_seconds:.0} reused={reused} seen={seen}"
        );
        return;
    }

    let listed: std::collections::BTreeSet<&str> =
        kept.iter().map(|r| r.identity.as_str()).collect();
    let unaccounted: Vec<&String> = scored_ids
        .iter()
        .filter(|id| {
            !listed.contains(id.as_str())
                // …AND NOT ONE THE BUDGET DEFERRED. A build queued for the next
                // run is the fourth outcome, and the only one that is a
                // statement about this run rather than about the build.
                && !deferred_ids.contains(id.as_str())
        })
        .collect();
    assert!(
        shards > 1 || unaccounted.is_empty(),
        "{} validated build(s) produced no row at all. The library holds them and this board never looked at them: {:?}",
        unaccounted.len(),
        &unaccounted[..unaccounted.len().min(5)],
    );
    eprintln!(
        "accounted: {} published, {refused} refused at the door",
        listed.len(),
    );


    // WHAT THE PAGE FETCHES, and the only thing published: one file per weapon,
    // plus an INDEX of each group's leader for the one view that ranks across
    // weapons.
    //
    // A WEAPON'S FILE IS THE SOURCE OF ITS OWN CARRY. The rows of every OTHER
    // ruler live in it and this pass measured none of them, so they are read
    // back and kept — and a weapon this generation cannot speak for yet keeps
    // this ruler's rows too, because a file written from an incomplete source is
    // a file missing whatever the source lacks.
    if let Some(dir) = std::env::args().nth(2).filter(|p| !p.starts_with("--")) {
        let dir = std::path::Path::new(&dir);
        if let Err(e) = std::fs::create_dir_all(dir) {
            panic!("{}: {e}", dir.display());
        }
        // A CARRIED ROW IS COPIED, NEVER REPARSED. `serde_json`'s number parser
        // is not correctly rounding (`exact_score`), so a row read into a
        // `Value` and written back comes out one ULP from what was published:
        // measured, 21 of 387 weapon files moved on a publish that measured
        // nothing. `RawValue` hands the bytes through untouched, which is also
        // the honest meaning of a carry.
        // EVERY RULER THE ROSTER STILL HAS, by FAMILY — a `_vN` row belongs to
        // the ruler it is a version of, so a live ruler's older version is not
        // an orphan.
        let live_rulers: std::collections::BTreeSet<&str> =
            wfsim_engine::benchmarks_data::all().iter().map(|b| family(&b.id)).collect();
        let mut by_weapon: std::collections::BTreeMap<String, Vec<Box<RawValue>>> =
            Default::default();
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let Some(weapon) = name.strip_suffix(".json") else { continue };
                if weapon == INDEX_STEM {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
                let Ok(rows) = serde_json::from_str::<Vec<Box<RawValue>>>(&text) else { continue };
                // THIS RULER'S ROWS ARE REPLACED FROM THE FACTS; every OTHER
                // ruler's are read back and kept, because a weapon's file is
                // written whole and this pass measured none of them.
                //
                // …EXCEPT A ROW WHOSE RULER NO LONGER EXISTS. This tool is
                // invoked one ruler at a time, so nothing ever visits a retired
                // one — its rows would be read back and written out for ever,
                // and `every_published_row_is_a_legal_build` would fail on
                // them with no pass able to clear it. Retiring a ruler is
                // deleting its file, and this is what makes that enough.
                let keep: Vec<Box<RawValue>> = rows
                    .into_iter()
                    .filter(|r| {
                        carried(published(r).map(|p| p.benchmark), &bench_id, &live_rulers)
                    })
                    .collect();
                by_weapon.insert(weapon.to_string(), keep);
            }
        }
        // …AND WHAT THE FACTS HOLD IS PUBLISHED, WHOLE AND UNCONDITIONALLY.
        //
        // A WEAPON IS NEVER HELD BACK FOR HAVING AN UNMEASURED ROW. It was, and
        // the reason was that asking for a row again meant DELETING its fact,
        // which left a hole a publish would have written out as rows
        // disappearing. Asking is a queue row now and `scores` only ever grows,
        // so there is no hole to protect against: a build with no fact was
        // never on this board, and one with an old fact keeps the number it
        // has until a new one replaces it.
        for r in &kept {
            let row = serde_json::value::to_raw_value(&page_row(&bench_id, r)).expect("json");
            by_weapon.entry(r.weapon.clone()).or_default().push(row);
        }
        for (weapon, rows) in &by_weapon {
            let f = dir.join(format!("{weapon}.json"));
            if let Err(e) = std::fs::write(&f, serde_json::to_string(rows).expect("json")) {
                eprintln!("{}: {e}", f.display());
            }
        }

        // THE CROSS-WEAPON VIEW IS AN INDEX, DERIVED FROM WHAT IS PUBLISHED.
        //
        // One row per GROUP — (weapon, ruler, mode, riven) — because that is
        // what the benchmark page ranks: each weapon's own leader, and the page
        // filters riven from plain client-side, so both leaders travel.
        //
        // IT CANNOT DISAGREE WITH A WEAPON PAGE, because it is computed FROM the
        // weapon files rather than written beside them. The whole board was
        // published as a second file for this one view: 4.7 MB of the same rows
        // a second time, in git, rewritten every run, to draw about a thousand
        // of them. The index is 509 KB, and 37 KB on the wire against 249.
        let mut index: std::collections::BTreeMap<String, Vec<&RawValue>> = Default::default();
        for (weapon, rows) in &by_weapon {
            let mut best: std::collections::BTreeMap<(&str, &str, bool), (f64, &RawValue)> =
                Default::default();
            for r in rows {
                let Some(p) = published(r) else { continue };
                let k = (p.benchmark, p.mode.unwrap_or("base"), p.riven.is_some());
                let score = p.score.get().trim_matches('"').parse().unwrap_or(f64::NEG_INFINITY);
                let e = best.entry(k).or_insert((f64::NEG_INFINITY, r));
                if score > e.0 {
                    *e = (score, r);
                }
            }
            if !best.is_empty() {
                index.insert(weapon.clone(), best.into_values().map(|(_, r)| r).collect());
            }
        }
        let f = dir.join(format!("{INDEX_STEM}.json"));
        std::fs::write(&f, serde_json::to_string(&index).expect("json"))
            .unwrap_or_else(|e| panic!("{}: {e}", f.display()));

        eprintln!(
            "wrote {} weapon file(s) and an index of {} group leader(s) under {}",
            by_weapon.len(),
            index.values().map(Vec::len).sum::<usize>(),
            dir.display(),
        );
    }

    // WHAT THE RUNTIME NEEDS, and only that.
    //
    // The rows are not embedded — `site/board/` is outside `data/` for exactly
    // that reason — so the page's few scalars per board come from a small
    // generated file that is. Merged rather than overwritten: this binary runs
    // once per benchmark.
    if std::path::Path::new(BOARD_STATE).exists() {
        let mut state = stored_state();
        // WHEN, AND NOT ONLY WHAT. The counts say how far behind the board is in
        // BUILDS; a reader looking at a number wants to know how old it is, and
        // that is the one thing neither the counts nor the fingerprints can
        // say — a fingerprint answers "did an input move", never "when was this
        // measured". Seconds rather than a date because the page does the
        // rendering and a board that should move in hours cannot say "today".
        //
        // A board written before this existed carries 0, which the page reads as
        // "unknown" rather than as 1970.
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as usize);
        state.insert(
            bench_id.clone(),
            BoardRow { submissions: seen, listed: kept.len(), scored_at: now },
        );
        let mut out = String::from(BOARD_STATE_HEADER);
        out.push_str("boards:
");
        for (id, r) in &state {
            out.push_str(&format!(
                "  {id}:
    submissions: {}
    listed: {}
    scored_at_epoch_seconds: {}
",
                r.submissions, r.listed, r.scored_at,
            ));
        }
        std::fs::write(BOARD_STATE, out).unwrap_or_else(|e| panic!("{BOARD_STATE}: {e}"));
    }
}


/// HOW MANY RUNS A PIECE OF A ROW IS, and it is ONE because nothing else
/// reproduces the number.
///
/// A single call folds the runs one at a time into one `Shard`, and float
/// addition is not associative — so a coarser piece regroups the sums and moves
/// the last bit of every quantity derived from one. MEASURED over a 200-run
/// crowd fight: pieces of 1 come out identical to a single call and pieces of
/// 2, 5, 10, 25, 50 and 100 all differ.
///
/// A ULP IS NOT BELOW ANYONE'S NOTICE HERE. `audit.yml` compares a republished
/// score to the stored one EXACTLY — "close enough would be a tolerance nobody
/// can defend" — so a row that was interrupted would report as moved for ever.
///
/// It costs 2.5% of a crowd fight against one call, which is what a bounded
/// makespan and a resumable row are worth.
const CHUNK_RUNS: u32 = 1;

/// `webapi::simulate_json` under a name that says the crate boundary is
/// deliberate: the scorer runs the SAME entry point the web api runs, so the
/// board cannot drift from what the page computes.
fn wfsim_engine_webapi_simulate(v: &Value) -> Value {
    wfsim_webapi::simulate_json(v)
}

/// WHAT A ROW HAS PAID FOR SO FAR, when it ran out of clock partway.
///
/// A row is not one measurement. A riven row is sixteen corner probes at 100
/// runs and one measurement at the ruler's 1000 — 2,600 runs, of which the
/// corners are 62% — so a budget that only bounded the last one would leave
/// most of the bill unbounded.
///
/// ONE CURSOR, BECAUSE ONE SUB-MEASUREMENT IS IN FLIGHT AT A TIME. The screen,
/// then each corner in turn, then the measurement: whatever the clock
/// interrupts is the only thing partway, and everything before it is a number.
#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
struct Partial {
    /// Sub-measurements this row has finished: `screen` for the plain-row
    /// probe, a riven corner under its own index.
    #[serde(default)]
    priced: std::collections::BTreeMap<String, f64>,
    /// The one that is partway: its label, how many runs are banked, and what
    /// they contributed.
    #[serde(default)]
    cursor: Option<(String, u32, Value)>,
}

/// Run `want` runs of `req`, resuming and pausing on the clock.
///
/// `None` means the budget ran out and `part` now carries the progress —
/// **including the runs this call paid for**, which is the whole point: a row
/// that cannot finish inside one run of the board still finishes, across as
/// many as it takes. Every run's dice are a pure function of `(seed, index)`,
/// so the pieces merge into exactly what one call over the range produces
/// (`wfsim_webapi::simulate_shard_json`, `dummy::tests::eight_shards_are_one_run`).
///
/// THE FIRST CHUNK IS ONE RUN, and the rest are sized from what it cost. A
/// fixed chunk is wrong in both directions here: the rows this exists for are
/// seconds a run, and the median row is milliseconds — one chunk of a thousand
/// would blow any budget, and a thousand chunks of one would pay to build a
/// 361-body arena a thousand times.
fn run_budgeted(
    part: &mut Partial,
    label: &str,
    req: &Value,
    want: u32,
    deadline: Option<std::time::Instant>,
) -> Option<Value> {
    let mut acc = wfsim_engine::dummy::Shard::default();
    let mut done = 0u32;
    if let Some((who, at, shard)) = part.cursor.take() {
        if who == label {
            if let Ok(s) = serde_json::from_value::<wfsim_engine::dummy::Shard>(shard.clone()) {
                acc = s;
                done = at;
            }
        } else {
            // Another sub-measurement's progress, which this row will come back
            // to. Putting it back is what keeps "one in flight" true.
            part.cursor = Some((who, at, shard));
        }
    }
    while done < want {
        let piece = wfsim_webapi::simulate_shard_json(req, done, CHUNK_RUNS, &mut |_, _| {});
        let Ok(s) = serde_json::from_value::<wfsim_engine::dummy::Shard>(piece) else {
            // A shard that will not parse is not a slow row, it is a broken
            // one; the caller's `ok` check answers it the way it always did.
            return Some(wfsim_engine_webapi_simulate(req));
        };
        acc.merge(&s);
        done += CHUNK_RUNS;
        if done < want && deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            let shard = serde_json::to_value(&acc).unwrap_or(Value::Null);
            part.cursor = Some((label.to_string(), done, shard));
            return None;
        }
    }
    let shard = serde_json::to_value(&acc).unwrap_or(Value::Null);
    Some(wfsim_webapi::simulate_merged_json(req, std::slice::from_ref(&shard)))
}

/// BANK WHERE THIS ROW STOPPED, and leave it for the next run.
///
/// A paused row is a FOURTH outcome beside listed, held and refused: the build
/// reached no row on this board and is not lost either. `deferred_ids` is what
/// tells the accounting below so, because a run that quietly dropped a build
/// would look exactly like one that ranked it.
fn pause_row(
    key: &str,
    part: &Partial,
    out: &mut std::collections::HashMap<String, Partial>,
    deferred: &mut std::collections::BTreeSet<String>,
) {
    out.insert(key.to_string(), part.clone());
    deferred.insert(identity_of(key));
}

#[cfg(test)]
mod tests {
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


    use super::*;

    /// A ROW PAID FOR IN SITTINGS IS THE ROW PAID FOR IN ONE.
    ///
    /// This is the whole warrant for `--row-budget`. The board's accuracy
    /// promise is the ruler's run count, so the ONLY thing a budget may bend is
    /// how many board runs those runs are spread over — and the moment a
    /// resumed row answered even one ULP away from an uninterrupted one, the
    /// budget would be buying speed with the number.
    ///
    /// BIT FOR BIT, not "close": every run's dice are a pure function of
    /// `(seed, index)`, so this is an identity and not a tolerance. A crowd,
    /// because the per-body means are part of what has to survive the merge and
    /// a single target cannot test them — the same reason
    /// `dummy::tests::eight_shards_are_one_run` uses one.
    ///
    /// THE PAUSES ARE FORCED, with a deadline already in the past: every call
    /// banks after its first chunk and hands back, so 40 runs are taken in
    /// pieces no larger than the probe. A budget that never expired would make
    /// this assert that one call equals one call.
    #[test]
    fn a_row_paid_for_in_sittings_is_the_row_paid_for_in_one() {
        let req = json!({
            "weapon": "torid",
            "mods": ["serration", "split_chamber"],
            "enemy": "corrupted_heavy_gunner",
            "level": 30,
            "steel_path": false,
            "duration": 12,
            "runs": 40,
            "seed": 12648430,
            "formation": [
                { "at": [0.7, 1.2] }, { "at": [1.4, 1.2] }, { "at": [2.1, 1.2] }
            ],
        });

        // THE UNTOUCHED PATH is the yardstick, not another budgeted call. The
        // archive holds numbers this produced, and `audit.yml` recomputes and
        // compares them EXACTLY — so what has to hold is that paying for a row
        // in pieces does not move it, and two budgeted calls agreeing with each
        // other would assert nothing at all.
        let one = wfsim_engine_webapi_simulate(&req);
        assert!(one.get("ok").and_then(Value::as_bool).unwrap_or(false), "{one}");

        let mut whole = Partial::default();
        let unbounded = run_budgeted(&mut whole, "measure", &req, 40, None)
            .expect("an unbounded run cannot pause");
        assert!(whole.cursor.is_none(), "a finished row carries no cursor");
        assert_eq!(
            serde_json::to_string(&unbounded).unwrap(),
            serde_json::to_string(&one).unwrap(),
            "chunking moved the number even without a pause",
        );

        let mut part = Partial::default();
        let mut sittings = 0;
        let resumed = loop {
            sittings += 1;
            assert!(sittings < 200, "40 runs took more than 200 sittings");
            // ALREADY EXPIRED, so each call takes exactly one chunk and banks.
            let past = std::time::Instant::now() - std::time::Duration::from_secs(1);
            if let Some(out) = run_budgeted(&mut part, "measure", &req, 40, Some(past)) {
                break out;
            }
            let (label, done, _) = part.cursor.as_ref().expect("a pause banks a cursor");
            assert_eq!(label, "measure");
            assert!(*done > 0 && *done < 40, "banked {done} of 40");
        };
        assert!(sittings > 3, "the deadline did not force pauses ({sittings} sittings)");
        assert_eq!(
            serde_json::to_string(&resumed).unwrap(),
            serde_json::to_string(&one).unwrap(),
            "a resumed row answered differently from an uninterrupted one",
        );
    }


    /// …AND A PAUSE IS NOT A CACHE ACROSS A DATA CHANGE.
    ///
    /// The cursor is keyed by the sub-measurement it belongs to, so progress
    /// banked under one label is never spent on another: a corner's runs
    /// resumed into the final measurement would publish a 100-run probe as the
    /// ruler's thousand.
    #[test]
    fn banked_progress_is_only_spent_on_what_it_was_taken_for() {
        let req = json!({
            "weapon": "torid",
            "mods": ["serration"],
            "enemy": "corrupted_heavy_gunner",
            "level": 30, "steel_path": false,
            "duration": 8, "runs": 12, "seed": 12648430,
        });
        let mut part = Partial::default();
        let past = std::time::Instant::now() - std::time::Duration::from_secs(1);
        assert!(run_budgeted(&mut part, "0", &req, 12, Some(past)).is_none());
        let banked = part.cursor.clone().expect("a cursor");
        assert_eq!(banked.0, "0");
        // A different label finds nothing to resume and starts at zero — and
        // the other one's progress is still there afterwards.
        assert!(run_budgeted(&mut part, "measure", &req, 12, Some(past)).is_none());
        let now = part.cursor.clone().expect("a cursor");
        assert_eq!(now.0, "measure", "the new label took over the cursor");
        assert!(now.1 > 0);
    }


    /// THE SCORER ASKS FOR A MODE, AND TWO MODES ARE TWO QUESTIONS. A weapon
    /// that can fill its gauge either way sent one request for both cycles, so
    /// the two modes scored to the last digit.
    ///
    /// DERIVED, NOT LISTED: every weapon, every pair of its modes, and two
    /// sharing a form must still differ.
    #[test]
    fn two_modes_sharing_one_form_are_two_requests() {
        let scenario = json!({ "enemy": "thrax_centurion", "level": 9999 });
        let build = |id: &str| wfsim_engine::builds::ValidBuild {
            weapon: id.to_string(),
            mods: vec![],
            evolutions: vec![],
            arcanes: vec![],
            valence: String::new(),
            exilus: None,
            riven: None,
            riven_rolls: Vec::new(),
            assembly: None,
            forma: 0,
            drain: 0,
        };
        let mut shared = 0usize;
        for w in wfsim_engine::weapons_data::roster() {
            let modes = wfsim_engine::weapons_data::play_modes(&w.id);
            let v = build(&w.id);
            for (i, a) in modes.iter().enumerate() {
                for b in modes.iter().skip(i + 1) {
                    let (ra, rb) = (
                        wfsim_webapi::simulate_request(&scenario, &v, *a),
                        wfsim_webapi::simulate_request(&scenario, &v, *b),
                    );
                    if a.form() == b.form() {
                        shared += 1;
                    }
                    assert_ne!(
                        ra, rb,
                        "{}: `{}` and `{}` ask the simulator the same question",
                        w.id, a.id, b.id
                    );
                }
            }
        }
        // …AND THE CASE EXISTS: "no pair collides" passes vacuously on a
        // roster where no two modes share a form.
        assert!(
            shared > 0,
            "no weapon has two modes sharing one form: the case is untested"
        );
    }

    /// **ONE SUBMISSION IS ONE ROW PER MODE**, so the keys those rows are
    /// deduped by must differ — otherwise `seen_ids` keeps the first and the
    /// fan-out silently scores nothing extra at all.
    ///
    /// THE FAILURE IS INVISIBLE FROM THE OUTPUT: a board with one row per build
    /// and a board with one row per build-and-mode look identical unless you
    /// know which weapon should have had four. So it is asserted on the KEY,
    /// which is the thing that would collapse them.
    ///
    /// DERIVED, NOT LISTED: every weapon in the roster, and the case has to
    /// exist — a roster where no weapon has two sustainable modes would pass
    /// this vacuously.
    #[test]
    fn one_build_is_a_distinct_row_in_every_mode_it_can_be_played() {
        let mut multi = 0usize;
        for w in wfsim_engine::weapons_data::roster() {
            let v = wfsim_engine::builds::ValidBuild {
                weapon: w.id.clone(),
                mods: vec![],
                evolutions: vec![],
                arcanes: vec![],
                valence: String::new(),
                exilus: None,
                riven: None,
            riven_rolls: Vec::new(),
                assembly: None,
                forma: 0,
                drain: 0,
            };
            let modes: Vec<_> = wfsim_engine::weapons_data::play_modes(&w.id)
                .into_iter()
                .filter(|m| m.sustainable)
                .collect();
            if modes.len() > 1 {
                multi += 1;
            }
            let mut keys = std::collections::HashSet::new();
            for m in &modes {
                assert!(
                    keys.insert(wfsim_engine::builds::board_key(&v, m.id)),
                    "{}: `{}` shares a board key with another of its modes, so the                      fan-out would publish one row for both",
                    w.id,
                    m.id
                );
            }
        }
        assert!(
            multi > 0,
            "no weapon has two sustainable modes: the fan-out is untested"
        );
    }

    /// …AND IT NAMES THE MODE RATHER THAN THE FORM. The assertion above is met
    /// by any two requests that differ; this says WHICH field carries it.
    #[test]
    fn the_request_names_the_mode_and_not_a_form() {
        let scenario = json!({
            "enemy": "thrax_centurion", "form": "stale",
            // A RULER'S OWN TERM, which the scorer carries rather than knows
            // about — how a benchmark declares a fight with no kills in it.
            "buff_triggers_off": ["headshot_kill"],
        });
        let v = wfsim_engine::builds::ValidBuild {
            weapon: "ballistica_prime".to_string(),
            mods: vec![],
            evolutions: vec![],
            arcanes: vec![],
            valence: String::new(),
            exilus: None,
            riven: None,
            riven_rolls: Vec::new(),
            assembly: None,
            forma: 0,
            drain: 0,
        };
        let modes = wfsim_engine::weapons_data::play_modes("ballistica_prime");
        let m = modes
            .iter()
            .find(|m| m.id == "alternate_cycle")
            .expect("alternate_cycle");
        let req = wfsim_webapi::simulate_request(&scenario, &v, *m);
        assert_eq!(
            req.get("mode").and_then(Value::as_str),
            Some("alternate_cycle")
        );
        assert_eq!(
            req.get("form"),
            None,
            "a stale `form` survived beside the mode"
        );
        assert_eq!(
            req["buff_triggers_off"],
            json!(["headshot_kill"]),
            "the ruler's own term was dropped"
        );
    }

    /// **THE SPLIT IS BY WORK, NOT BY COUNT**, and the case that says so is the
    /// one the board actually has: a few monster rows among many cheap ones.
    ///
    /// THE COST DISTRIBUTION IS SKEWED: the median row is under a second and
    /// the tail runs to minutes, so the makespan is decided by where the
    /// monsters land rather than by how many rows each worker holds.
    ///
    /// ASSERTED AGAINST ROUND-ROBIN rather than against a constant, because the
    /// claim is comparative: a modulo on the same input is the number to beat.
    #[test]
    fn the_shards_are_packed_by_cost_and_beat_round_robin() {
        const SHARDS: usize = 8;
        // Four monsters in a crowd of cheap rows, at a stride that SHARES A
        // FACTOR with the shard count — which is the case that bites and is not
        // exotic: a board is walked weapon by weapon, so expensive rows arrive
        // in runs rather than at random. A stride coprime to the count spreads
        // them by luck, and luck is what this replaces.
        let costs: Vec<f64> = (0..100)
            .map(|i| if i % 8 == 0 && i < 32 { 100.0 } else { 1.0 })
            .collect();

        let mut packed = vec![0.0f64; SHARDS];
        for &c in &costs {
            charge(&mut packed, c);
        }
        let mut robin = vec![0.0f64; SHARDS];
        for (i, &c) in costs.iter().enumerate() {
            robin[i % SHARDS] += c;
        }
        let worst = |v: &[f64]| v.iter().cloned().fold(0.0f64, f64::max);
        let total: f64 = costs.iter().sum();
        // ROUND-ROBIN PUT ALL FOUR ON ONE WORKER: every monster index is 0 mod
        // 8, so `i % SHARDS` is 0 for all of them.
        assert!(
            worst(&robin) > worst(&packed),
            "packing did not beat round-robin: {} vs {}",
            worst(&robin),
            worst(&packed)
        );
        // …AND IT IS NEAR THE FLOOR. No split can beat `total / shards`, and no
        // split can break a single row up, so the best possible makespan is the
        // larger of those two.
        let floor = (total / SHARDS as f64).max(100.0);
        assert!(
            worst(&packed) <= 1.05 * floor,
            "packed makespan {} against a floor of {floor}",
            worst(&packed)
        );
    }
}

#[cfg(test)]
mod page_row_tests {
    use super::*;

    /// **A RETIRED RULER'S ROWS LEAVE THE BOARD; A LIVE RULER'S ARE CARRIED.**
    ///
    /// The carry is what makes a weapon's file writable by a tool that measured
    /// one ruler, and it is also what kept a retired ruler alive: nothing
    /// invokes this tool with an id the roster no longer has, so its rows were
    /// read back and rewritten on every publish. That deadlocked a retirement —
    /// `every_published_row_is_a_legal_build` fails on such a row, and
    /// `publish.yml` will not run from a commit whose tests failed.
    #[test]
    fn a_row_under_a_ruler_the_roster_no_longer_has_is_not_carried() {
        let live: std::collections::BTreeSet<&str> = wfsim_engine::benchmarks_data::all()
            .iter()
            .map(|b| family(&b.id))
            .collect();
        assert!(live.contains("single_target"), "the roster has its primary ruler: {live:?}");

        // ANOTHER LIVE RULER IS CARRIED — the whole reason this pass reads the
        // file back rather than truncating it.
        for other in live.iter().filter(|b| **b != "single_target") {
            assert!(carried(Some(other), "single_target", &live), "{other} was dropped");
        }
        // THIS RULER'S ARE NOT: they are replaced from the facts.
        assert!(!carried(Some("single_target"), "single_target", &live));
        // …AND NEITHER IS ANOTHER VERSION OF IT, which is what `family` is for.
        assert!(!carried(Some("single_target_v2"), "single_target", &live));

        // A RETIRED RULER GOES, whichever ruler is being assembled.
        assert!(!live.contains("single_target_no_aim"), "it was retired");
        for driving in live.iter() {
            assert!(
                !carried(Some("single_target_no_aim"), driving, &live),
                "assembling {driving} carried a retired ruler's row"
            );
        }

        // A ROW THIS PASS CANNOT READ IS CARRIED. Dropping what it cannot parse
        // would lose rows to a shape change rather than to a retirement.
        assert!(carried(None, "single_target", &live));
    }

    /// WHAT IS PUBLISHED, READ FROM DISK. `site/board/` is outside `data/`, so
    /// it is not embedded and there is nothing to reach through the binary —
    /// which is the point: the rows are a CI artifact and not something the
    /// browser carries. The WEAPON is the file name, because that is what makes
    /// a weapon page one fetch.
    fn published() -> Vec<(String, Vec<Value>)> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../site/board");
        let mut out: Vec<(String, Vec<Value>)> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .filter(|p| p.file_stem().is_some_and(|x| x != INDEX_STEM))
            .map(|p| {
                let text = std::fs::read_to_string(&p).expect("read");
                let rows = serde_json::from_str::<Vec<Value>>(&text)
                    .unwrap_or_else(|e| panic!("{}: {e}", p.display()));
                (p.file_stem().unwrap().to_string_lossy().into_owned(), rows)
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        assert!(!out.is_empty(), "nothing published under site/board/");
        out
    }

    fn strings(r: &Value, k: &str) -> Vec<String> {
        r.get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
            .unwrap_or_default()
    }

    /// EVERY PUBLISHED ROW IS A BUILD SOMEONE COULD EQUIP. The board is public
    /// and copyable, so a row that cannot be built is worse than a missing row
    /// — it is an instruction that fails in the arsenal.
    /// `validate_for_board_with` is the same check a submission faces, run here
    /// against what is already published.
    #[test]
    fn every_published_row_is_a_legal_build() {
        let mut n = 0usize;
        for (weapon, rows) in published() {
            for r in &rows {
                let bench = r.get("benchmark").and_then(Value::as_str).unwrap_or("");
                assert!(
                    wfsim_engine::benchmarks_data::get(bench).is_some(),
                    "{weapon} names benchmark {bench:?}, which does not exist",
                );
                let riven = r.get("riven").map(|rv| wfsim_engine::rivens_data::RivenShape {
                    bonuses: strings(rv, "bonuses"),
                    malus: rv.get("malus").and_then(Value::as_str).map(str::to_owned),
                });
                let grip = r.get("grip").and_then(Value::as_str).unwrap_or("");
                let assembly = (!grip.is_empty()).then(|| {
                    let chamber = wfsim_engine::weapons_data::spec(&weapon)
                        .and_then(|s| s.kitgun.clone())
                        .and_then(|k| wfsim_engine::kitguns_data::default_assembly(&k))
                        .map(|d| d.chamber)
                        .unwrap_or_default();
                    wfsim_engine::kitguns_data::Assembly {
                        chamber,
                        grip: grip.to_string(),
                        loader: r
                            .get("loader")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    }
                });
                let v = wfsim_engine::builds::validate_for_board_with(
                    bench,
                    &weapon,
                    &strings(r, "mods"),
                    &strings(r, "evolutions"),
                    &strings(r, "arcanes"),
                    r.get("valence").and_then(Value::as_str).unwrap_or(""),
                    riven.as_ref(),
                    r.get("exilus").and_then(Value::as_str),
                    assembly.as_ref(),
                )
                .unwrap_or_else(|e| panic!("{weapon} row on {bench}: {e}"));
                // `validate` already refused anything over capacity, and the
                // capacity is the weapon's own — so the assertion is that the
                // row survived the door whole, not that it fits a number here.
                assert_eq!(v.mods.len(), strings(r, "mods").len(), "{weapon} lost a mod");
                let score = exact_score(&serde_json::to_string(r).expect("json"));
                assert!(score.is_some_and(|x| x > 0.0), "{weapon} scored nothing");
                n += 1;
            }
        }
        eprintln!("{n} published row(s) are legal builds");
    }

    /// BEST FIRST INSIDE A GROUP, because "the top 10" is the only order a board
    /// has — and a group is (benchmark, mode, riven-ness), which is what the
    /// page ranks independently.
    #[test]
    fn a_weapons_rows_are_ranked() {
        for (weapon, rows) in published() {
            let mut last: std::collections::HashMap<(String, String, bool), f64> =
                Default::default();
            for r in &rows {
                let k = (
                    r.get("benchmark").and_then(Value::as_str).unwrap_or("").to_string(),
                    r.get("mode").and_then(Value::as_str).unwrap_or("base").to_string(),
                    r.get("riven").is_some(),
                );
                let s = exact_score(&serde_json::to_string(r).expect("json")).unwrap_or(0.0);
                if let Some(prev) = last.insert(k.clone(), s) {
                    assert!(prev >= s, "{weapon} out of order in {k:?}: {prev} then {s}");
                }
            }
        }
    }

    fn row(riven: Option<RowRiven>) -> Row {
        Row {
            identity: "dual_toxocyst|cycle".into(),
            weapon: "dual_toxocyst".into(),
            mode: "cycle".into(),
            score: 139.28,
            mods: vec!["galvanized_diffusion".into()],
            evolutions: vec!["dual_toxocyst_evo1_incarnon_form".into()],
            arcanes: vec!["secondary_deadhead".into()],
            valence: String::new(),
            exilus: String::new(),
            grip: String::new(),
            loader: String::new(),
            riven,
        }
    }

    /// **THE RIVEN REACHES THE PAGE**, which is the whole of what went wrong.
    ///
    /// A row wearing one was written to the page's file without it for as
    /// long as the writer existed, and the board simply held none until
    /// 2026-08-24 — so nothing was ever visibly broken until the hour the first
    /// riven build landed, and then three unrelated-looking things were.
    #[test]
    fn a_riven_row_carries_its_riven_to_the_page() {
        let v = page_row(
            "single_target",
            &row(Some(RowRiven {
                bonuses: vec!["critical_chance".into(), "multishot".into()],
                malus: Some("zoom".into()),
                rolls: vec![1.1, 1.1, 0.9],
            })),
        );
        let rv = v.get("riven").expect("the riven reaches the page");
        assert_eq!(rv["bonuses"], json!(["critical_chance", "multishot"]));
        assert_eq!(rv["malus"], json!("zoom"));
        // THE ROLLS TOO: taking a board row has to give the reader that riven,
        // and a shape without its corner is a card they cannot build.
        assert_eq!(rv["rolls"], json!([1.1, 1.1, 0.9]));
    }

    /// …AND A PLAIN ROW OMITS THE KEY RATHER THAN WRITING `null`.
    ///
    /// Not a style choice: this file is compared BYTE FOR BYTE against
    /// `build_site_app.py`'s output, which is what makes a local site build a
    /// no-op against the scorer. The Python side copies the yaml entry and a
    /// plain entry has no `riven` key at all, so a `null` here would leave
    /// every local build dirty.
    #[test]
    fn a_plain_row_omits_the_key_entirely() {
        let v = page_row("single_target", &row(None));
        assert!(v.get("riven").is_none(), "{v}");
        // …and the fields a page reads by name are all still there, so the
        // extraction did not quietly drop one of the other nine.
        for k in [
            "benchmark",
            "mode",
            "source",
            "score",
            "shown",
            "mods",
            "evolutions",
            "arcanes",
            "valence",
        ] {
            assert!(v.get(k).is_some(), "{k} is missing");
        }
    }


}
