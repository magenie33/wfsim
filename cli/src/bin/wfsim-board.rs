//! THE SCORER — turns submitted builds into a board.
//!
//! Reads a JSON array of submissions on stdin, writes the board YAML on
//! stdout. That shape is deliberate: fetching from KV and committing the
//! result are the workflow's job (`.github/workflows/board.yml`), and neither
//! needs the engine. What needs the engine is the only thing here — running
//! each build under the benchmark and reading the number off.
//!
//! WHY A SUBMISSION CARRIES NO SCORE: nobody's number is trusted because
//! nobody's number is asked for. A row's score is produced HERE under the
//! benchmark's own pinned seed, so anyone with the repo reproduces any row
//! exactly, and an engine change re-scores everything instead of migrating
//! anything — nobody is asked to resubmit.
//!
//! TWO OUTPUTS, because the board has two readers. The YAML on stdout is the
//! CANONICAL record, committed and diffable; the JSON is what the PAGE fetches
//! at runtime, because a board that changes hourly must not require rebuilding
//! the wasm to reach anyone.
//!
//!   cat submissions.json | wfsim-board single_target site/board.json > board.yaml

use std::io::Read;

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

/// One scored row, before it is trimmed to the top N.
struct Row {
    weapon: String,
    /// WAS THIS SCREENED RATHER THAN MEASURED? A probe score is recorded and
    /// never published, and never reused — see `PROBE_MARGIN`.
    probe: bool,
    /// SECONDS THIS ROW TOOK TO SIMULATE, or what the last run measured for it
    /// when this run reused the score. Written to the yaml and read back by the
    /// NEXT run to pack the shards — see `load`.
    cost_seconds: f64,
    /// THE BUILD THIS ROW IS, without the mode — `builds::identity`.
    ///
    /// Carried so the run can prove every validated build ended up somewhere:
    /// listed, or held below the floor. It is NOT written to the yaml, which
    /// states the build itself and from which the identity is recomputed — a
    /// stored copy of a derived fact is the one that goes stale.
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
    /// WHAT THIS ROW'S SCORE DEPENDS ON, as one hash — see
    /// `engine::data_fingerprint`. Written into the board so the NEXT run can
    /// ask, per row, whether anything it reads has moved.
    fp: String,
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

/// ONE ROW AS THE PAGE RECEIVES IT — `site/board.json`'s shape, in one place.
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

/// THE FLOOR: a row must score at least half its group's leader to be listed.
/// A COUNT bounds how LONG the list gets and says nothing about whether the
/// hundredth row is worth reading — the three groups that ever reached a cap of
/// 100 had a hundredth row at 18.6%, 25.9% and 25.4% of their leader.
///
/// WHAT IT REMOVES IS NOT THE CHEAP BUILD: the rows below the line carry 8 of 8
/// mods like the rows above and differ by taking the WORSE arcane, or by
/// spending slots on mods this fight cannot pay (docs/UNMODELLED.md).
///
/// IT IS MECHANICAL: the seed is pinned and a score reproduces to the last
/// digit, so 50.3% and 49.5% are two different NUMBERS rather than two
/// estimates of one, and an exact board has no tie band to grant.
///
/// FIFTY IS A CUT LINE rather than a measurement: the pooled distribution has
/// no knee to sit on (the largest gap below 90% is 1.2 points), so the data
/// cannot pick the number, only say it is not fragile — about 12 of 1274 rows
/// per point. It is very generous against F1's 107% rule, which is the intent.
///
/// THERE IS NO CEILING, so a group whose builds are close keeps all of them.
const FLOOR: f64 = 0.5;

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
const BOARD_STATE: &str = "data/board_state.yaml";

/// WHAT THE LAST RUN LEFT, read the way the writer below spells it.
///
/// BY LINE, not by a yaml crate: the shape is this program's own output and
/// never anybody's input, so a parser dependency here would be one more thing
/// that can disagree with the writer. Read from DISK rather than from the
/// compiled-in copy, because publish rewrites the file after the binary was
/// built and the cursor has to be the one the last run actually stored.
fn stored_state() -> std::collections::BTreeMap<String, [usize; 5]> {
    let mut state: std::collections::BTreeMap<String, [usize; 5]> = Default::default();
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
                    "submissions" => e[0] = n,
                    "listed" => e[1] = n,
                    "held" => e[2] = n,
                    "scored_at_epoch_seconds" => e[3] = n,
                    "refresh_cursor" => e[4] = n,
                    _ => {}
                }
            }
        }
    }
    state
}

/// HOW FAR UNDER THE CUT A PROBE HAS TO LAND before the full measurement is
/// skipped — a share of the floor, which is itself a share of the leader.
///
/// A PROBE IS A COARSER MEASUREMENT OF THE SAME THING, not a different one: 100
/// runs against the ruler's 1000, so its standard error is about three times
/// larger and it is unbiased. Half the floor is a 2x margin on top of that, so
/// a row is screened out only when it reads a QUARTER of its group's leader —
/// far outside anything three standard errors can explain, and exactly the
/// population the mode fan-out creates: a build measured in a mode it was never
/// tuned for is off by a multiple, not by a few per cent.
const PROBE_MARGIN: f64 = 0.5;


const BOARD_STATE_HEADER: &str = "# WHAT THE RUNTIME KNOWS ABOUT EACH BOARD — GENERATED by `wfsim-board`.
#
# THE ROWS ARE NOT HERE and must never be. `data/` is compiled into the binary,
# so a board archive under it put every row of every board into the wasm that
# every visitor downloads. The archive is `boards/*.yaml` at the repository
# root, read from DISK by the scorer that writes it and by the site build;
# nothing at runtime asks a board for a row, because the page fetches
# `board.json`.
#
# `submissions` paired with the library's own size (`/api/board/pending`) is how
# a STATIC board says how far behind it is. `listed` and `held` say how much of
# what it scored it is showing — a board that reports only what it shows cannot
# say how much it looked at.
";

/// Best first, then everything within `FLOOR` of each WEAPON AND MODE's own
/// leader. Returns the rows to publish and how many the floor took.
///
/// Ties keep the FEWER-Forma build — same fight, cheaper to own.
///
/// PER MODE, not per weapon: the two ways to play a Torid compete with each
/// other for nothing, and a shared reference would let the stronger mode decide
/// what the weaker one may show, which is the opposite of what the dimension is
/// for. Per BOARD too, since this binary runs once per ruler.
///
/// A group's leader is the FIRST row of it this loop meets, because the sort is
/// descending and global.
fn keep_above_floor(mut rows: Vec<Row>) -> (Vec<Row>, Vec<Row>) {
    rows.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut leader: std::collections::BTreeMap<(String, String, bool), f64> = Default::default();
    let mut kept = Vec::new();
    // THE ROWS THE FLOOR TOOK, RETURNED RATHER THAN COUNTED.
    //
    // They were dropped here, and dropping them cost twice. A build scored and
    // not listed is indistinguishable from one that was LOST, which is the
    // failure this repo has already paid for; and the published yaml is what
    // the next run REUSES, so a row missing from it has no cached score and is
    // re-simulated from scratch every hour, for ever, to be discarded again.
    // The fan-out multiplied that population. They go in the record now.
    let mut below = Vec::new();
    for r in rows {
        // …AND PER RIVEN-NESS, for the reason it is per mode: a riven build and
        // a plain one compete with each other for nothing, and a shared
        // reference would let whichever is stronger on this weapon decide what
        // the other may show. On most weapons that is the riven build, and the
        // plain ones — the builds most players can actually make — would be the
        // ones to disappear.
        //
        // THE RANKING IS STILL ONE LIST. Only the floor partitions: a riven
        // build does not always beat a plain one, so ranking them apart would
        // publish a comparison the fight does not make.
        let top = *leader
            .entry((r.weapon.clone(), r.mode.clone(), r.riven.is_some()))
            .or_insert(r.score);
        if r.score >= FLOOR * top {
            kept.push(r);
        } else {
            below.push(r);
        }
    }
    (kept, below)
}

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

/// SCORING IS THE WHOLE COST — 67 minutes of a 71-minute run at the rulers'
/// 1000 runs — and it is embarrassingly parallel: every
/// row is an independent fight. So the job splits N ways and the scores are
/// carried between processes as a plain map.
///
/// This is not a cache and must never become one. It is keyed by the row's
/// identity ALONE, with no engine version in it, because it only ever travels
/// between shards of ONE run — every shard built from one commit. Persisting it
/// across runs would publish yesterday's numbers under today's engine, which is
/// exactly the failure the board exists to prevent.
type ScoreMap = std::collections::HashMap<String, f64>;

/// AN f64 CROSSES BETWEEN PROCESSES AS TEXT, NOT AS A JSON NUMBER.
///
/// `serde_json`'s number parser is not correctly rounding: it reads
/// `1.1070976928071055` back as `1.1070976928071057`, one ULP away, and the
/// same for roughly one value in ten. Every score a shard computes crosses to
/// the publish process through that parser, so the board published a number the
/// engine never produced — and a reader reproducing the row from the repo, as
/// the board invites them to, got the engine's answer and not the board's.
///
/// Rust's own `str::parse::<f64>` IS correctly rounding, so the fix is to keep
/// the value out of the number path: `{f}` writes the shortest text that
/// round-trips, and `parse` reads exactly that back. The files these travel in
/// are written and read inside ONE run, so the encoding is nobody else's.
fn num_out(v: f64) -> String {
    format!("{v}")
}

/// The other half of [`num_out`], tolerating a plain number for a file a run
/// already in flight wrote — where the ULP is the old behaviour and not worse.
fn num_in(v: &Value) -> Option<f64> {
    match v.as_str() {
        Some(s) => s.parse().ok(),
        None => v.as_f64(),
    }
}

/// WHICH ROWS A `--rescore` NAMES, at whatever precision the operator has.
///
/// The backstop has to cover the extreme case, and the extreme case is ONE
/// build. A row key is `identity#mode` and an identity is `weapon|mods|…`, so a
/// selector is read as `<identity prefix>[#<mode>][:riven|:plain]` and the
/// prefix is matched at a COMPONENT BOUNDARY — `felarx` names every row of the
/// weapon and cannot half-match `felarx_prime`, while pasting a whole identity
/// names exactly one build.
///
///   `felarx`                       every mode, every build
///   `felarx#cycle`                 one mode
///   `felarx#cycle:plain`           one mode, the rows without a riven
///   `felarx|galvanized_hell,…`     one build, every mode
///   `felarx|galvanized_hell,…#base`  one row
///
/// SEPARATED BY `;` AND NOT `,`, because a mod list is commas and a selector
/// that ended at the first one could not name a build at all.
struct Selector {
    ident: String,
    mode: Option<String>,
    riven: Option<bool>,
}

impl Selector {
    fn parse(text: &str) -> Option<Selector> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        // `:riven` / `:plain` last, then `#mode`: an identity holds neither
        // character, so the split is unambiguous whatever the build is called.
        let (head, riven) = match text.rsplit_once(':') {
            Some((h, "riven")) => (h, Some(true)),
            Some((h, "plain")) => (h, Some(false)),
            _ => (text, None),
        };
        let (ident, mode) = match head.split_once('#') {
            Some((i, m)) => (i, Some(m.to_string())),
            None => (head, None),
        };
        Some(Selector { ident: ident.to_string(), mode, riven })
    }

    fn matches(&self, key: &str) -> bool {
        let (ident, mode) = key.rsplit_once('#').unwrap_or((key, "base"));
        if let Some(want) = &self.mode {
            if want != mode {
                return false;
            }
        }
        if let Some(want) = self.riven {
            // THE MODS COMPONENT HOLDS THE RIVEN SLOT BY NAME, so membership is
            // exact where a substring would match a card merely spelled like
            // one. `builds::RIVEN_SLOT` is that name and this is the only place
            // outside the engine that has to know it.
            let mods = ident.split('|').nth(1).unwrap_or("");
            let has = mods.split(',').any(|m| m == wfsim_engine::builds::RIVEN_SLOT);
            if has != want {
                return false;
            }
        }
        // AT A COMPONENT BOUNDARY. `felarx` may not name `felarx_prime`, and a
        // half-typed mod list may not name the build it is a prefix of.
        ident == self.ident
            || (ident.starts_with(&self.ident)
                && ident[self.ident.len()..].starts_with('|'))
    }
}

/// THE GROUP A ROW BELONGS TO — `(weapon, mode)`, read off the row key.
///
/// A weapon with n modes is n independent rankings (docs/BOARD.md), so a mode
/// is the unit a probe can clear and a rescore can skip. The key is
/// `identity#mode` and an identity opens with the weapon and a `|`, so the two
/// ends are the group and nothing has to carry it separately.
fn group_of(key: &str) -> String {
    let (identity, mode) = key.rsplit_once('#').unwrap_or((key, "base"));
    let weapon = identity.split('|').next().unwrap_or(identity);
    format!("{weapon}|{mode}")
}

fn load_scores(
    spec: Option<String>,
    bench_id: &str,
) -> (ScoreMap, ScoreMap, ScoreMap, std::collections::HashMap<String, String>) {
    let mut out = std::collections::HashMap::new();
    // …AND WHAT EACH ONE COST THE SHARD THAT PAID. Merged the same way and for
    // the same reason as the score: the publish process computes almost
    // nothing, so it has no figure of its own to write into the board.
    let mut cost = std::collections::HashMap::new();
    // …AND THE PROBE OF EVERY ROW A SHARD SCREENED, which is a third thing and
    // not a score. It is kept apart from `out` for the reason the board keeps
    // it apart: a probe is 100 runs against the ruler's 1000 and may never be
    // published or reused AS a measurement. What it can do is spare the publish
    // process from taking it again.
    let mut probes = std::collections::HashMap::new();
    // WHAT EACH SCORE READ, carried so the caller can refuse one whose data has
    // moved. A file from THIS run needs none — every shard is one binary over
    // one checkout — and a file from a durable store needs it for every row.
    let mut fps: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let Some(spec) = spec else { return (out, cost, probes, fps) };
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    let p = std::path::Path::new(&spec);
    if p.is_dir() {
        if let Ok(rd) = std::fs::read_dir(p) {
            files.extend(
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|f| f.extension().is_some_and(|e| e == "json")),
            );
        }
    } else {
        files.push(p.to_path_buf());
    }
    files.sort();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        let Ok(file) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        // A FILE SAYS WHICH BOARD IT IS, and another board's is refused rather
        // than merged. The key is `identity#mode` and carries no benchmark, so
        // two boards scoring the same build produce the SAME key with different
        // numbers — and the publish step is handed one directory holding every
        // benchmark's shards. Merging them silently published one ruler's score
        // under the other's name: the Torid's aimed 28.44 kpm sat at the top of
        // the NO-AIM board, where that build actually scores 0.5.
        if file.get("benchmark").and_then(Value::as_str) != Some(bench_id) {
            continue;
        }
        if let Some(fs) = file.get("fps").and_then(Value::as_object) {
            for (k, v) in fs {
                if let Some(t) = v.as_str() {
                    fps.insert(k.clone(), t.to_string());
                }
            }
        }
        if let Some(cs) = file.get("costs").and_then(Value::as_object) {
            for (k, v) in cs {
                if let Some(n) = num_in(v) {
                    cost.insert(k.clone(), n);
                }
            }
        }
        if let Some(ps) = file.get("probes").and_then(Value::as_object) {
            for (k, v) in ps {
                if let Some(n) = num_in(v) {
                    probes.insert(k.clone(), n);
                }
            }
        }
        let Some(scores) = file.get("scores").and_then(Value::as_object) else {
            continue;
        };
        for (k, v) in scores {
            if let Some(n) = num_in(v) {
                out.insert(k.clone(), n);
            }
        }
    }
    (out, cost, probes, fps)
}

/// The prior board's scores, keyed the way this run keys them — but only if it
/// was computed by the same engine.
///
/// The rows are re-validated on the way in rather than trusted: the identity a
/// score is filed under is `builds::identity`, which is a function of the
/// canonical build, so it has to be recomputed from the row rather than stored
/// beside it. That also means a row this engine would now REFUSE simply fails
/// to produce a key and is rescored (and then refused) rather than carried.
/// What a prior board still answers for.
#[derive(Default)]
struct Prior {
    scores: std::collections::HashMap<String, f64>,
    /// THE ROLLS TRAVEL WITH THE SCORE. They were written and never read back,
    /// which nothing noticed while the fingerprint was coarse enough to make
    /// almost every run a full rescore. The moment reuse became the common
    /// case, a reused riven row would have come back with no rolls — and a
    /// riven row with no rolls loses its whole riven block.
    rolls: std::collections::HashMap<String, Vec<f64>>,
    /// THE BEST LISTED SCORE IN EACH `(weapon, mode, riven)` GROUP, carried
    /// forward so this run can tell a hopeless row from a contender BEFORE it
    /// pays for one. The floor is relative to the leader and the leader is not
    /// known until the group is scored, so the last run's is the estimate — and
    /// it only has to be good enough to place a 4x margin.
    leaders: std::collections::HashMap<(String, String, bool), f64>,
    /// WHAT EACH ROW COST THE RUN THAT MEASURED IT, carried forward so the next
    /// one can pack its shards by work rather than by count. Read back even for
    /// a STALE row: the fight has to be redone, but how long it takes is a
    /// property of the build and the ruler, and those did not move.
    costs: std::collections::HashMap<String, f64>,
    /// EVERY ROW THE PRIOR BOARD HELD, whatever its score or staleness.
    ///
    /// It is what tells a REPAIR from a row nobody has ever scored, and the two
    /// are bounded separately: the repair slice is `--refresh`, and the backlog
    /// of never-scored builds is `--new-limit`. `costs` cannot answer it — a
    /// row written before costs were recorded carries none, and would read as
    /// never scored.
    present: std::collections::HashSet<String>,
    /// Rows whose OWN data moved. Printed rather than counted silently: it is
    /// the number that says how much a change actually cost.
    stale: usize,
    /// …AND WHICH ONES, with what each cost to measure.
    ///
    /// A stale row is not a WRONG row, it is one whose inputs moved and whose
    /// number no longer carries a promise. Dropping it made every such row a
    /// row this run had to fight, which is how one data correction became a
    /// full rescore. Kept, the run can fight a BOUNDED slice of them and leave
    /// the rest holding what they have until their turn comes.
    stale_keys: Vec<(String, f64)>,
}

/// THERE IS NO CODE FINGERPRINT, and there never honestly could be. A hash of
/// the source says the BYTES moved, which is a different question from whether
/// any NUMBER did — and most edits that move it cannot move one: a comment, a
/// test, a validation rule, a field only the page reads. Hashing more finely
/// would not fix it either, because the failure inverts: a data file a row
/// forgets to name costs one wasted rescore, where a code path a row forgets to
/// name silently reuses a score the change moved.
///
/// SO CODE IS MEASURED, NOT HASHED. The audit re-fights published rows and
/// compares exactly; what it finds moved is what gets recomputed. A stored
/// score is valid until something PROVES it moved, which is a stronger claim
/// than "valid until a hash moved", because a hash moving proves nothing.
///
/// What is left here is the DATA fingerprint, which is evidence: a row's data
/// dependencies are enumerable from the row, and forgetting one only costs time.
fn reuse_prior(path: &str, bench_id: &str, keep_stale: bool) -> Result<Prior, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let prior = wfsim_engine::boards_data::parse(&text).map_err(|e| format!("{path}: {e}"))?;
    // A PRIOR BOARD THAT CANNOT BE REUSED CAN STILL BE READ.
    //
    // Returning here turns the probe screen off on exactly the runs that need
    // it. The screen's threshold is the group's LEADER from the last board and
    // its packing input is each row's measured COST, both of which the walk
    // below reads before the per-row staleness check, deliberately, "because a
    // stale group still had a leader". Dropping them with the scores makes a
    // full rescore run blind: measured across the three boards, ZERO rows
    // screened out of 22,479 while 36% of the 132-hour bill went on rows
    // scoring under a quarter of their group's leader.
    //
    // A STALE LEADER IS A FINE THRESHOLD. It does not decide what is published;
    // it decides whether a row is worth 1000 runs rather than 100, under a 2x
    // margin on top of a floor that is itself half the leader.
    if family(&prior.benchmark) != family(bench_id) {
        return Err(format!("{path} is {}'s board", prior.benchmark));
    }
    let mut out = Prior::default();
    for e in &prior.entries {
        let Ok(v) = wfsim_engine::builds::validate_for_board_with(
            bench_id,
            &e.weapon,
            &e.mods,
            &e.evolutions,
            &e.arcanes,
            &e.valence,
            e.riven
                .as_ref()
                .map(wfsim_engine::boards_data::BoardRiven::shape)
                .as_ref(),
            Some(e.exilus.as_str()).filter(|x| !x.is_empty()),
            e.assembly().as_ref(),
        ) else {
            continue;
        };
        // THE ROW'S OWN DATA, recomputed from the row rather than trusted — the
        // same reason its identity is recomputed one line down. A row written
        // before per-row fingerprints existed carries an empty one and is
        // rescored, which is the safe direction and the only one available.
        let want = wfsim_engine::data_fingerprint::row_fingerprint(
            bench_id,
            &v.weapon,
            &v.mods,
            &v.arcanes,
            &v.evolutions,
            v.exilus.as_deref(),
            v.assembly.as_ref(),
        );
        // THE COST IS READ BEFORE THE STALENESS CHECK, and that is the point.
        // A stale row has to be fought again, so it is exactly the row whose
        // cost the packing needs — throwing the figure away with the score
        // would blind the split to the very work it is about to schedule.
        let key = wfsim_engine::builds::board_key(&v, &e.mode);
        out.present.insert(key.clone());
        if e.cost > 0.0 {
            out.costs.insert(key.clone(), e.cost);
        }
        // THE LEADER IS READ FROM LISTED ROWS ONLY, and before the staleness
        // check for the same reason the cost is: a stale group still had a
        // leader, and a stale row still needs something to be measured against.
        if e.listed && !e.probe {
            let g = (e.weapon.clone(), e.mode.clone(), e.riven.is_some());
            let top = out.leaders.entry(g).or_insert(e.score);
            if e.score > *top {
                *top = e.score;
            }
        }
        // A PROBE SCORE IS NOT A MEASUREMENT and may never be reused as one. Its
        // COST is still worth having — a screened row costs what it costs.
        if e.probe {
            continue;
        }
        // STALE MEANS ITS OWN DATA MOVED, and nothing else. What the row
        // reads is enumerable FROM the row, so this is evidence; a code
        // difference is not, and no longer marks anything.
        if e.fp != want {
            out.stale += 1;
            out.stale_keys.push((key.clone(), e.cost));
            // KEPT WHEN THE CALLER ASKS. `--refresh` fights a slice of
            // these and the rest keep the number they have; without it a row
            // whose data moved is dropped, which is what the manual full
            // rescore and the targeted one both want.
            if !keep_stale {
                continue;
            }
        }
        if let Some(rv) = &e.riven {
            if !rv.rolls.is_empty() {
                out.rolls.insert(key.clone(), rv.rolls.clone());
            }
        }
        out.scores.insert(key, e.score);
    }
    Ok(out)
}

/// THE RULER'S TERMS PLUS THE ENTRANT, as the request the simulator answers.
///
/// **IT NAMES THE MODE, NEVER THE FORM THE MODE RESOLVES TO.** `form()` maps
/// every cycle onto the one policy word `gauge_cycle`, which does not say in
/// which half the gauge is filled — so a weapon with two cycles sent one
/// request for both and `parse_fight` fell back to the arsenal's own form.
///
/// Extracted so the assertion can be made on the REQUEST: a decision taken
/// inline in a scoring loop is one no test can reach.
fn simulate_request(
    scenario: &Value,
    v: &wfsim_engine::builds::ValidBuild,
    played: wfsim_engine::weapons_data::WeaponPlayMode,
) -> Value {
    let mut req = scenario.clone();
    let Some(o) = req.as_object_mut() else {
        return req;
    };
    o.insert("weapon".into(), json!(v.weapon));
    // THE RIVEN'S SLOT IS SPELLED DIFFERENTLY ON THE WIRE. A record carries the
    // bare `riven` because the endpoint's ids are `[a-z0-9_]`; a simulate
    // request names the riven ITEM, which is `riven:<name>`. The translation is
    // one line and lives here so neither protocol has to bend for the other.
    o.insert(
        "mods".into(),
        json!(v
            .mods
            .iter()
            .map(|m| if m == wfsim_engine::builds::RIVEN_SLOT {
                RIVEN_ITEM.to_string()
            } else {
                m.clone()
            })
            .chain(v.exilus.iter().cloned())
            .collect::<Vec<_>>()),
    );
    o.insert("evolutions".into(), json!(v.evolutions));
    o.insert("arcane".into(), json!(v.arcanes));
    // THE VALENCE, at the ruler's own terms: the element the entrant named, and
    // the roll's MAXIMUM whatever they said it was. Every player can fuse to
    // 60%, so ranking a lower roll would be ranking how many duplicates someone
    // farmed — the same reason every row here is scored at full Forma.
    if !v.valence.is_empty() {
        o.insert("valence_element".into(), json!(v.valence));
        let max = wfsim_engine::weapons_data::valence_of(&v.weapon).map_or(0.0, |s| s.max);
        o.insert("valence_bonus".into(), json!(max));
    }
    // THE PARTS. Without them the fight is fought with the chamber's DEFAULT
    // assembly, so a submitted grip is stored, validated and then silently not
    // used — the number published would be for a weapon nobody built.
    if let Some(a) = &v.assembly {
        o.insert("assembly".into(), json!({ "grip": a.grip, "loader": a.loader }));
    }
    o.insert("mode".into(), json!(played.id));
    // ONE SPELLING OF ONE FACT. `form` is what a request carries when it names
    // no mode, and a ruler that carried both would be two answers to one
    // question with the loser silent.
    o.remove("form");
    req
}

fn main() {
    let bench_id = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!(
            "usage: wfsim-board <benchmark-id> [board.json] [--shard i/n] \
                   [--scores <file|dir>] [--emit-scores <file>]  (submissions on stdin)"
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
    // EVERY STORED SCORE PROVES ITSELF THE SAME WAY, whether it came from this
    // run's own shards or from a store that outlived the run that wrote it: the
    // row's data hash, recomputed in the loop below. A same-run file passes
    // trivially, so there is one rule rather than a flag saying which to apply.
    //
    // They wait in `store_scores` rather than joining `known`, which also holds
    // the PRIOR BOARD's rows — those were validated on their own way in and
    // carry no entry here, so a check applied to the merged map threw the whole
    // board away and made every row todo.
    let (store_scores, known_costs, known_probes, store_fps) =
        load_scores(flag("--scores"), &bench_id);
    let mut known: ScoreMap = Default::default();
    let mut reused = 0usize;
    let mut stale = 0usize;
    let mut prior_rolls: std::collections::HashMap<String, Vec<f64>> = Default::default();
    // WHAT THE LAST RUN MEASURED, per row — the input to the shard packing.
    let mut prior_costs: ScoreMap = Default::default();
    let mut prior_present: std::collections::HashSet<String> = Default::default();
    // …AND THE ROWS WHOSE OWN DATA MOVED, which `--refresh` walks a slice of.
    let mut prior_stale_keys: Vec<(String, f64)> = Vec::new();
    // …AND WHAT EACH GROUP'S BEST WAS, the threshold the probe screens against.
    let mut prior_leaders: std::collections::HashMap<(String, String, bool), f64> =
        Default::default();
    // ---- WHAT THIS RUN IS ALLOWED TO FIGHT ----------------------------
    //
    // A fingerprint says a stored score is UNVERIFIED under this generation,
    // never that it is wrong, so nothing here rescores a board. `--verify`
    // MEASURES which groups moved, `--dry-run` counts what is left so a run
    // with nothing to do costs one job, and `--refresh` is the bounded slice of
    // the unverified that this run repairs. The rates and the reasons are
    // `docs/BOARD.md` §"When the code moved".

    // HOW MANY NEVER-SCORED ROWS A RUN TAKES ON, and it is the other half of
    // `--refresh`. The slice bounds the REPAIR of rows the board already holds;
    // without this, the backlog of submitted builds that have no score yet is
    // unbounded, so a run has to clear all of it before `publish` assembles
    // anything — 4,570 rows and hours of it, during which the board shows the
    // number it showed yesterday. Bounded, each run publishes a board with more
    // rows on it than the last.
    let new_limit = flag("--new-limit").and_then(|s| s.parse::<usize>().ok());
    // WHEN THE RUN STOPS TAKING ON NEW WORK, in seconds of wall clock.
    //
    // A BUDGET PREDICTS AND A DEADLINE GUARANTEES, and only one of those can be
    // had here: `--refresh` is spent in seconds the last run MEASURED, so it is
    // self-correcting, while a never-scored row has no cost and `--new-limit`
    // can only count. Rows differ 79x, so a count of 150 is nine minutes or
    // fifty depending on which builds arrived.
    //
    // IT APPLIES TO NEW ROWS ALONE, and that asymmetry is the whole safety
    // argument: a new row not taken is a row that is not on the board yet,
    // which is where it already was, while a REPAIR not taken is a row that
    // would vanish from it. The predictable half is the one that must finish.
    let deadline = flag("--deadline")
        .and_then(|s| s.parse::<u64>().ok())
        .map(std::time::Duration::from_secs);
    let started = std::time::Instant::now();
    let refresh = flag("--refresh").and_then(|s| s.parse::<f64>().ok());
    // WHERE THE SLICE STARTS. The operator may pin it, but the default is the
    // cursor the last run stored: every shard of a run reads the same file, so
    // they agree on the slice without being told, and the next run starts past
    // what this one repaired instead of on top of it.
    let refresh_from = flag("--refresh-from")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| stored_state().get(&bench_id).map_or(0, |e| e[4]));
    let dry = has_flag("--dry-run");
    let mut todo = 0usize;
    let mut fresh_seen = 0usize;
    let mut fresh_left = 0usize;
    // WHOSE ROWS WERE DEFERRED, as identities. The accounting below asserts
    // that every validated build reached a row, and a bounded run makes that
    // false ON PURPOSE — a build the budget did not reach this time is queued,
    // not lost, which is a FOURTH outcome and has to be one the check knows.
    let mut deferred_ids: std::collections::BTreeSet<String> = Default::default();
    let verify = has_flag("--verify");
    // THE PROBE'S VERDICT, CARRIED IN. Set by the workflow only after a
    // `--verify` run over the sample came back identical throughout, which is
    // what makes reusing across a code change a MEASUREMENT rather than a hope.

    let mut verify_against: ScoreMap = Default::default();
    if let Some(path) = flag("--reuse") {
        match reuse_prior(&path, &bench_id, refresh.is_some()) {
            Ok(p) => {
                reused = if verify { 0 } else { p.scores.len() };
                stale = p.stale;
                prior_rolls = p.rolls;
                prior_costs = p.costs;
                prior_present = p.present;
                prior_stale_keys = p.stale_keys;
                prior_leaders = p.leaders;
                if verify {
                    // NOT into `known`: a verify run has to MEASURE the sample,
                    // and a seeded score is a row that is never fought.
                    verify_against = p.scores;
                } else {
                    // The shards' own scores win: they were computed by THIS run.
                    for (k, v) in p.scores {
                        known.entry(k).or_insert(v);
                    }
                }
            }
            Err(why) => eprintln!("full rescore: {why}"),
        }
    }
    let emit_to = flag("--emit-scores");
    // EVERYTHING THIS RUN KNOWS, not only what it computed. A shard banks its
    // own slice — re-emitting the store it read would make every delta a copy
    // of the whole — where the publish pass writes the MERGED set the next run
    // starts from, and a merged set missing what it reused is not merged.
    let emit_all = has_flag("--emit-all");
    // WHAT EACH ROW THIS RUN TOUCHED READS, filed beside the score it produced.
    // Written into the emitted file so the score survives the run — see the
    // emit block for why the engine hash alone is not enough.
    let mut row_fps: std::collections::HashMap<String, String> = Default::default();
    let mut computed: std::collections::HashMap<String, f64> = Default::default();
    // …AND WHAT EACH ONE COST, travelling with it — see the emit block.
    let mut costs: std::collections::HashMap<String, f64> = Default::default();
    // ROWS THE PROBE TURNED AWAY. Recorded so the archive can say "screened,
    // not measured" rather than saying nothing — which is the shape a build
    // that was never looked at would also have.
    let mut probed: Vec<Row> = Vec::new();
    // …AND THEIR PROBE NUMBERS, to travel to the publish process the way a
    // score does. The shards screen roughly two rows in five; without this the
    // publish process finds none of them in `--scores` and takes every one of
    // those probes AGAIN, alone, on the critical path — 2.5 hours measured on
    // the step that assembles the three boards, against about a minute.
    let mut probes: std::collections::HashMap<String, f64> = Default::default();
    // THE ROLLS TRAVEL BESIDE THE SCORE, keyed the same way and reused on the
    // same terms.
    //
    // They have to travel at all because a score is not enough to publish a
    // riven row: the reader has to be able to BUILD that riven, and the corner
    // this engine chose is not something a page can re-derive without paying
    // for the search again. And they reuse on the same terms as the score
    // because they were found by the same fingerprint — anything that could
    // move the rolls moves the score, since the rolls ARE the argmax of it.
    let mut rolls: std::collections::HashMap<String, Vec<f64>> =
        load_rolls(flag("--scores"), &bench_id);
    // …and the ones a prior board already found, on the same terms as its
    // scores. This run's shards win: they were computed by this engine.
    for (k, v) in prior_rolls {
        rolls.entry(k).or_insert(v);
    }


    // FORCING ONE WEAPON BACK THROUGH THE FIGHT, and it is the backstop rather
    // than a tool. Reuse is decided by fingerprints, and a fingerprint answers
    // "did an INPUT move" — so a correction the hashes cannot see, or a run the
    // pipeline lost, leaves a published number nobody can argue the board out
    // of. `--rescore <id[,id]>` drops what is stored for those weapons and
    // nothing else: every other row still reuses, so a wrong number costs
    // minutes to replace rather than the whole board.
    //
    // THE ROLLS GO WITH THE SCORES. A riven row's rolls are the argmax of the
    // score, so keeping them while dropping the score would re-measure the
    // corner a stale number chose.
    // THE SLICE THIS RUN REPAIRS. Everything stale keeps its number; these go
    // back through the fight. Budgeted by the cost the last run MEASURED, so a
    // slice is a wall-clock promise rather than a row count over a population
    // whose rows differ by four orders of magnitude.
    let mut refresh_next: Option<usize> = None;
    if let Some(budget) = refresh {
        let mut stale_keys = prior_stale_keys;
        stale_keys.sort_by(|a, b| a.0.cmp(&b.0));
        let n = stale_keys.len();
        let mut spent = 0.0;
        let mut took = 0usize;
        for i in 0..n {
            let (k, cost) = &stale_keys[(refresh_from + i) % n];
            if took > 0 && spent + cost > budget {
                break;
            }
            known.remove(k);
            rolls.remove(k);
            spent += cost;
            took += 1;
        }
        reused = reused.saturating_sub(took);
        // ADVANCED BY WHAT WAS TAKEN, which is the whole point of storing it.
        refresh_next = Some(if n > 0 { (refresh_from + took) % n } else { 0 });
        eprintln!(
            "refresh: {took} of {n} stale row(s) go back through the fight, \
             {:.1} min of measured work, from offset {}",
            spent / 60.0,
            if n > 0 { refresh_from % n } else { 0 }
        );
    }

    if let Some(list) = flag("--rescore") {
        let sels: Vec<Selector> = list.split(';').filter_map(Selector::parse).collect();
        let hit = |k: &String| sels.iter().any(|sel| sel.matches(k));
        let before = known.len();
        known.retain(|k, _| !hit(k));
        rolls.retain(|k, _| !hit(k));
        let dropped = before - known.len();
        reused = reused.saturating_sub(dropped);
        eprintln!("rescore: forced {dropped} stored row(s) back through the fight for {list}");
        if dropped == 0 {
            eprintln!("rescore: nothing stored matched {list} — check the selector");
        }
    }
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
    // The benchmark's own terms, read once — the metric it is measured in and
    // the length that metric is over.
    // WHAT THIS RULER JUDGES BY, resolved against the one table that declares
    // the metrics (`engine::metrics`). Not a match on "dps" with everything
    // else falling through to kills per minute: a ruler naming a metric this
    // build has never heard of would then publish a number in the units of a
    // different question.
    let metric = wfsim_engine::metrics::get(
        scenario
            .get("metric")
            .and_then(Value::as_str)
            .unwrap_or(wfsim_engine::metrics::DEFAULT),
    );
    let duration = scenario
        .get("duration")
        .and_then(Value::as_f64)
        .unwrap_or(300.0);
    let metric = metric.unwrap_or_else(|| {
        panic!(
            "unknown benchmark metric — a row published in units nobody named is              worse than no row; the metrics are in `engine::metrics::ALL`"
        )
    });
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

        // IT PASSED THE DOOR, so it owes a row somewhere. Recorded before the
        // modes are enumerated, because what has to be provable is that a
        // VALIDATED build was ranked — not that some particular mode of it was.
        scored_ids.insert(wfsim_engine::builds::identity(&v));
        // WHAT EVERY ROW OF THIS BUILD READS. Per BUILD and not per row: the
        // hash is taken from the canonical build and the ruler, and a mode is
        // how the same build is fired, so the seven rows of a melee share one.
        let build_fp = wfsim_engine::data_fingerprint::row_fingerprint(
            &bench_id,
            &v.weapon,
            &v.mods,
            &v.arcanes,
            &v.evolutions,
            v.exilus.as_deref(),
            v.assembly.as_ref(),
        );

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
            let mut req = simulate_request(&scenario, &v, played);
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
            row_fps.insert(key.clone(), build_fp.clone());
            // A STORED SCORE IS HELD TO ITS OWN HASH. The engine gate is the
            // coarse half and cannot see a data change — that is what
            // `row_fingerprint` is for — so an entry is admitted only where the
            // build still reads what it read, and refought otherwise.
            //
            // `or_insert`, because the prior board and this run's own shards
            // were validated on their own way in and must win.
            if let Some(&stored) = store_scores.get(&key) {
                if store_fps.get(&key).map(String::as_str) == Some(build_fp.as_str()) {
                    known.entry(key.clone()).or_insert(stored);
                }
            }
            // THE SHARD IS A PROPERTY OF THE ROW, not of the submission it came
            // from: a melee weapon is seven rows off one record. `charge`
            // decides which, below, and every shard walks this same sequence
            // and skips only the SIMULATION — so they stay in step.
            // THE ROLLS THIS ENGINE SETTLED ON, filled in by the search below. A row
            // that was REUSED keeps whatever the last run found, which is correct:
            // reuse only happens when the fingerprint says nothing that could move
            // the answer has changed.
            let mut row_riven: Option<RowRiven> = v.riven.as_ref().and_then(|shape| {
                rolls.get(&key).map(|r| RowRiven {
                    bonuses: shape.bonuses.clone(),
                    malus: shape.malus.clone(),
                    rolls: r.clone(),
                })
            });
            // WHAT THIS ROW COST THIS RUN, when this run is the one that paid.
            // `None` where the score came from a sibling shard or from the prior
            // board — those carry their own figure, resolved below.
            let mut measured: Option<f64> = None;
            let score = match known.get(&key) {
                // A SIBLING SHARD OF THIS RUN ALREADY PAID FOR IT. Not a cache: the
                // map only ever travels between processes built from one commit.
                Some(&s) => s,
                None => {
                    // WHOSE ROW IS THIS. Charged to the least-loaded shard at
                    // the cost the last run measured — an unmeasured row (a new
                    // build, or a board written before costs were recorded)
                    // takes the neutral default, which makes this degrade to
                    // round-robin rather than to anything worse.
                    //
                    // It is decided HERE, inside the `None` arm, because a row
                    // whose score is already known costs nothing to publish and
                    // must not be charged to anybody.
                    // NEVER SCORED, AND THE RUN HAS TAKEN ITS SHARE. Counted
                    // BEFORE the shard filter, because every shard must reach
                    // the same verdict on the same row: a bound that stopped
                    // one and not another leaves them disagreeing about who
                    // owns the rows after it, and a row both believe is the
                    // other's is a row nobody scores. Only where a prior board
                    // was read — with none every row is new.
                    let fresh = !prior_present.is_empty() && !prior_present.contains(&key);
                    if fresh {
                        fresh_seen += 1;
                        if new_limit.is_some_and(|n| fresh_seen > n) {
                            fresh_left += 1;
                            deferred_ids
                                .insert(key.rsplit_once('#').map_or(key.clone(), |(i, _)| i.to_string()));
                            continue;
                        }
                    }
                    let cost = prior_costs.get(&key).copied().unwrap_or(DEFAULT_ROW_SECONDS);
                    let mine = charge(&mut load, cost);
                    // Not this shard's slice: another one is simulating it right
                    // now, and publishing a row for it here would mean scoring it
                    // twice and ranking it once.
                    if shards > 1 && mine != shard {
                        continue;
                    }
                    // …AND THE CLOCK, WHICH ONLY THIS SHARD CAN READ. It is
                    // spent differently in every shard, so it is asked AFTER
                    // the row has been dealt: a shard out of time drops rows of
                    // its OWN and leaves the deal itself untouched.
                    if fresh && deadline.is_some_and(|d| started.elapsed() > d) {
                        fresh_left += 1;
                        deferred_ids
                            .insert(key.rsplit_once('#').map_or(key.clone(), |(i, _)| i.to_string()));
                        continue;
                    }
                    if dry {
                        todo += 1;
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
                    // SCREEN BEFORE MEASURING. A row that reads a quarter of
                    // its group's leader at a tenth of the runs is not going to
                    // be listed, and paying the ruler's full precision to find
                    // that out again every hour is what the mode fan-out made
                    // expensive: most of what it adds is a build in a mode it
                    // was never tuned for.
                    //
                    // A RIVEN ROW IS SCREENED TOO, and against ITS OWN group's
                    // leader: a riven group and a plain one are different
                    // populations of the same weapon and each has its own top.
                    //
                    // NOT HERE, THOUGH. Its riven is not chosen yet, so a probe
                    // at this point measures the build without the thing it is
                    // built around. The corner search below picks the riven AND
                    // prices it, so that is where its screen goes.
                    //
                    // …AND NEVER UNDER `--verify`, which exists to MEASURE the
                    // sample: a screened row drops out of `compared`, so the
                    // verdict is drawn from fewer rows than were sampled.
                    let cut = (!verify)
                        .then(|| {
                            prior_leaders.get(&(
                                v.weapon.clone(),
                                played.id.to_string(),
                                v.riven.is_some(),
                            ))
                        })
                        .flatten()
                        .map(|top| PROBE_MARGIN * FLOOR * top);
                    if let (Some(cut), None) = (cut, v.riven.as_ref()) {
                        // WHAT A SHARD ALREADY PROBED IS NOT PROBED AGAIN. The
                        // publish process walks every row and finds a screened
                        // one in neither `--scores` nor the prior board, so
                        // without this it retakes the probe — serially, for two
                        // rows in five, at the end of the run.
                        let (ok, s) = match known_probes.get(&key) {
                            Some(&s) => (true, s),
                            None => {
                                let mut probe = req.clone();
                                if let Some(o) = probe.as_object_mut() {
                                    o.insert("runs".into(), json!(PROBE_RUNS));
                                }
                                let out = wfsim_engine_webapi_simulate(&probe);
                                (out.get("ok").and_then(Value::as_bool).unwrap_or(false), score_in(&out))
                            }
                        };
                        if ok && s < cut {
                            probes.insert(key.clone(), s);
                            // WHAT IT COST THE RUN THAT ACTUALLY PAID. Reading
                            // the clock here would write ~0 for every probe this
                            // process carried rather than took, and that figure
                            // is the input to the NEXT run's shard packing — so
                            // a carried probe would teach the packing that two
                            // rows in five are free.
                            let took = known_probes
                                .get(&key)
                                .and(known_costs.get(&key).copied())
                                .unwrap_or_else(|| began.elapsed().as_secs_f64());
                            costs.insert(key.clone(), took);
                            // A ROW LIKE ANY OTHER, marked. It is not published
                            // and its number is not a measurement, but it is
                            // the record that this build WAS looked at — and a
                            // build that was never looked at has to be
                            // distinguishable from one that was and lost.
                            probed.push(Row {
                                probe: true,
                                cost_seconds: took,
                                identity: wfsim_engine::builds::identity(&v),
                                weapon: v.weapon.clone(),
                                mode: played.id.to_string(),
                                score: s,
                                mods: v.mods.clone(),
                                evolutions: v.evolutions.clone(),
                                arcanes: v.arcanes.clone(),
                                valence: v.valence.clone(),
                                exilus: v.exilus.clone().unwrap_or_default(),
                                grip: v.assembly.as_ref().map(|a| a.grip.clone()).unwrap_or_default(),
                                loader: v.assembly.as_ref().map(|a| a.loader.clone()).unwrap_or_default(),
                                riven: None,
                                fp: wfsim_engine::data_fingerprint::row_fingerprint(
                                    &bench_id, &v.weapon, &v.mods, &v.arcanes, &v.evolutions,
                                    v.exilus.as_deref(), v.assembly.as_ref(),
                                ),
                            });
                            continue;
                        }
                    }
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
                        // THE BEST CORNER'S OWN PROBE SCORE, kept rather than
                        // thrown away: it is the screen, and it is already paid
                        // for. IN THE BOARD'S OWN METRIC, so it can be compared
                        // with the cut without a second conversion — which for
                        // `kpm` is a positive linear rescale of what this
                        // returned before, so the corner it picks, and every
                        // number published, are unchanged.
                        let top_probe = std::cell::Cell::new(f64::NEG_INFINITY);
                        let best = wfsim_engine::rivens_data::perfect(shape, cls, |sp| {
                            let mut probe = req.clone();
                            if let Some(o) = probe.as_object_mut() {
                                o.insert("rivens".into(), riven_request(sp));
                                o.insert("runs".into(), json!(PROBE_RUNS));
                            }
                            let s = score_in(&wfsim_engine_webapi_simulate(&probe));
                            top_probe.set(top_probe.get().max(s));
                            s
                        });
                        // …AND THE SCREEN, ON A NUMBER THAT COST NOTHING EXTRA.
                        //
                        // A riven row is sixteen probes and one real measurement.
                        // If the BEST of the sixteen reads under a quarter of its
                        // group's leader, no corner of this shape can be listed
                        // and the measurement buys nothing — so the row costs 16
                        // probes rather than 16 + 10 probe-equivalents, which is
                        // 38% off every riven row that is not going to place —
                        // and riven rows are 58% of the group-clear bill.
                        //
                        // SOUNDER THAN THE PLAIN-ROW SCREEN, not weaker: what is
                        // judged is the very corner that would have been measured.
                        if let Some(cut) = cut {
                            if top_probe.get() < cut {
                                let took = began.elapsed().as_secs_f64();
                                costs.insert(key.clone(), took);
                                probed.push(Row {
                                    probe: true,
                                    cost_seconds: took,
                                    identity: wfsim_engine::builds::identity(&v),
                                    weapon: v.weapon.clone(),
                                    mode: played.id.to_string(),
                                    score: top_probe.get(),
                                    mods: v.mods.clone(),
                                    evolutions: v.evolutions.clone(),
                                    arcanes: v.arcanes.clone(),
                                    valence: v.valence.clone(),
                                    exilus: v.exilus.clone().unwrap_or_default(),
                                    grip: v.assembly.as_ref().map(|a| a.grip.clone()).unwrap_or_default(),
                                    loader: v.assembly.as_ref().map(|a| a.loader.clone()).unwrap_or_default(),
                                    // THE SHAPE, EVEN THOUGH THE ROW IS NOT
                                    // PUBLISHED. `mods` carries the riven SLOT,
                                    // so a row stating one and naming no riven
                                    // is refused when the next run reads it back
                                    // — and what that run wanted from it was its
                                    // COST, which is the input to the packing for
                                    // exactly the rows that are expensive.
                                    //
                                    // THE ROLLS ARE THE WINNING CORNER'S, which
                                    // is what was probed. They are never read as
                                    // a measurement: `reuse_prior` takes the cost
                                    // and then skips a probe row.
                                    riven: Some(RowRiven {
                                        bonuses: shape.bonuses.clone(),
                                        malus: shape.malus.clone(),
                                        rolls: best
                                            .bonuses
                                            .iter()
                                            .map(|b| b.roll)
                                            .chain(best.malus.iter().map(|m| m.roll))
                                            .collect(),
                                    }),
                                    fp: wfsim_engine::data_fingerprint::row_fingerprint(
                                        &bench_id, &v.weapon, &v.mods, &v.arcanes,
                                        &v.evolutions, v.exilus.as_deref(), v.assembly.as_ref(),
                                    ),
                                });
                                continue;
                            }
                        }
                        row_riven = Some(RowRiven {
                            bonuses: shape.bonuses.clone(),
                            malus: shape.malus.clone(),
                            rolls: best
                                .bonuses
                                .iter()
                                .map(|b| b.roll)
                                .chain(best.malus.iter().map(|m| m.roll))
                                .collect(),
                        });
                        if let Some(o) = req.as_object_mut() {
                            o.insert("rivens".into(), riven_request(&best));
                        }
                        if let Some(rv) = &row_riven {
                            rolls.insert(key.clone(), rv.rolls.clone());
                        }
                    }
                    let out = wfsim_engine_webapi_simulate(&req);
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
                    costs.insert(key.clone(), began.elapsed().as_secs_f64());
                    // THIRTY SECONDS is a row worth naming: the median row is under
                    // one, so this prints the tail and nothing else — a line per
                    // slow row rather than 2,474 lines nobody reads.
                    let took = began.elapsed().as_secs_f64();
                    measured = Some(took);
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
            // WHAT THIS ROW READS, hashed from the CANONICAL build rather than from
            // the submission — the same object `identity` is taken from, so the
            // next run recomputes the identical hash off its own stored row.
            let fp = wfsim_engine::data_fingerprint::row_fingerprint(
                &bench_id,
                &v.weapon,
                &v.mods,
                &v.arcanes,
                &v.evolutions,
                v.exilus.as_deref(), v.assembly.as_ref(),
            );
            let exilus_for_row = v.exilus.clone().unwrap_or_default();
            rows.push(Row {
                // MEASURED HERE, else what a sibling shard measured, else what
                // the last run did, else the default. The chain matters: the
                // publish process computes almost nothing, so without the
                // shards' own figures every cost would decay to the default
                // one run after it was measured.
                probe: false,
                cost_seconds: measured
                    .or_else(|| known_costs.get(&key).copied())
                    .or_else(|| prior_costs.get(&key).copied())
                    .unwrap_or(DEFAULT_ROW_SECONDS),
                identity: wfsim_engine::builds::identity(&v),
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
                fp,
            });
        }
    }

    let (mut kept, below) = keep_above_floor(rows);
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
    //
    // AND HOW MANY THE FLOOR TOOK. It is a number you can go and READ rather
    // than a count in a log nobody keeps: those rows are in the yaml, carrying
    // `listed: false`.
    eprintln!(
        "{seen} submissions, {refused} refused, {} rows ({reused} reused, {stale} rescored for a data change, {} scored here, {} below the floor, {} screened at {PROBE_RUNS} runs)",
        kept.len(),
        computed.len(),
        below.len(),
        probed.len(),
    );
    // HOW MUCH BACKLOG IS LEFT, said out loud. A run that defers rows is not a
    // run that failed to score them: the next one takes the next share, and the
    // count falling run over run is what says the board is catching up.
    if fresh_left > 0 {
        let why = if deadline.is_some_and(|d| started.elapsed() > d) { "clock" } else { "count" };
        eprintln!(
            "new: {} of {fresh_seen} never-scored row(s) taken, {fresh_left} left for the next run ({why})",
            fresh_seen - fresh_left
        );
    }

    // EVERY STORED SUBMISSION IS ACCOUNTED FOR, and the run says so rather than
    // being trusted. Four outcomes and no fifth: refused at the door, listed,
    // scored and held below the floor, or deferred to the next run by a budget. A build that fell out of all three
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
    // ONE MACHINE-READABLE LINE, the way `verify-result` is: the workflow reads
    // it to decide whether to fan out at all. It stands BEFORE the accounting
    // below, which asserts every validated build reached a row — true of a run
    // that scores and false by construction of one that only counts.
    if dry {
        eprintln!("dry-run: todo={todo} reused={reused} stale={stale} seen={seen}");
        return;
    }

    let listed: std::collections::BTreeSet<&str> =
        kept.iter().map(|r| r.identity.as_str()).collect();
    let held: std::collections::BTreeSet<&str> =
        below.iter().map(|r| r.identity.as_str()).collect();
    let screened: std::collections::BTreeSet<&str> =
        probed.iter().map(|r| r.identity.as_str()).collect();
    let unaccounted: Vec<&String> = scored_ids
        .iter()
        .filter(|id| {
            !listed.contains(id.as_str())
                && !held.contains(id.as_str())
                && !screened.contains(id.as_str())
                // …AND NOT ONE THE BUDGET DEFERRED. A build queued for the next
                // run is the fourth outcome, and the only one that is a
                // statement about this run rather than about the build.
                && !deferred_ids.contains(id.as_str())
        })
        .collect();
    assert!(
        shards > 1 || unaccounted.is_empty(),
        "{} validated build(s) produced no row at all — neither listed nor below the              floor. The library holds them and this board never looked at them: {:?}",
        unaccounted.len(),
        &unaccounted[..unaccounted.len().min(5)],
    );
    eprintln!(
        "accounted: {} listed, {} held below the floor, {} screened without measuring,          {refused} refused at the door",
        listed.len(),
        held.len(),
        screened.len(),
    );
    // HOW MANY GROUPS THE SCREEN COULD EVEN JUDGE. It needs a leader to measure
    // against, and when it has none it silently passes everything — which it
    // does whenever the prior board is unreadable. "0 screened" is
    // indistinguishable from
    // "nothing deserved screening", so the number of THRESHOLDS is printed
    // beside it and a run with none says so.
    if prior_leaders.is_empty() {
        eprintln!(
            "screen: NO group leaders — nothing can be screened this run, so every                  row pays the ruler's full precision"
        );
    } else {
        eprintln!(
            "screen: {} group leaders available",
            prior_leaders.len()
        );
    }

    // ---- THE VERDICT, and nothing is written on this path -------------
    //
    // EXACT EQUALITY, because a score is a pure function of (build, ruler,
    // code, data) and an f64 round-trips through the yaml: "close enough" would
    // be a tolerance nobody can defend, and a change that moves a rank by a
    // thousandth still moves the board.
    //
    // TOO FEW ROWS COMPARED IS NOT A PASS. A sample that the floor screened
    // away, or one whose rows are no longer on the board, proves nothing — and
    // a verification that cannot fail is worse than none, so it answers "moved"
    // and the run does what it would have done anyway.
    if verify {
        let mut compared = 0usize;
        let mut moved: Vec<(&String, f64, f64)> = Vec::new();
        for (k, &now) in &computed {
            if let Some(&was) = verify_against.get(k) {
                compared += 1;
                if now != was {
                    moved.push((k, was, now));
                }
            }
        }
        let floor = flag("--verify-min")
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(20);
        for (k, was, now) in moved.iter().take(5) {
            eprintln!("verify: {k} {was} -> {now}");
        }
        // HOW FAR IT MOVED, BESIDE THE FACT THAT IT DID. The test itself stays
        // EXACT — a score is a pure function and the carry between processes is
        // now lossless (`num_out`), so any difference at all is a difference —
        // and `worst` is the first thing a reader wants when one is reported:
        // a defect moves a number by orders of magnitude where a numerical
        // artefact moves it by a bit.
        let rel = |was: f64, now: f64| {
            let scale = was.abs().max(now.abs());
            if scale == 0.0 { 0.0 } else { (was - now).abs() / scale }
        };
        let worst = moved.iter().map(|&(_, w, n)| rel(w, n)).fold(0.0f64, f64::max);
        // ONE MACHINE-READABLE LINE, because the workflow shards this and has to
        // SUM the counts before it can apply a floor: a shard comparing three
        // rows proves nothing on its own and everything together.
        // `stale` RIDES ALONG because "nothing was compared" has two causes and
        // they are opposite findings: a sampler that drew an empty slice is
        // broken, while rows whose DATA moved have no number under this
        // generation yet and are not a claim anything can be held to.
        eprintln!(
            "verify-result: compared={compared} moved={} stale={stale} worst={worst:e}",
            moved.len()
        );
        // WHICH GROUPS CLEARED, and not merely how many rows did. A whole-board
        // verdict spends the entire board on one row that moved; a per-group one
        // spends the groups that moved and reuses the rest, which is the whole
        // saving. A group is cleared only when it was COMPARED and nothing in it
        // moved — an absent group is not a cleared one, so a sample that never
        // reached a group leaves it to be rescored.
        let bad: std::collections::BTreeSet<String> =
            moved.iter().map(|(k, _, _)| group_of(k)).collect();
        let mut seen_groups: std::collections::BTreeSet<String> = Default::default();
        for k in computed.keys() {
            if verify_against.contains_key(k) {
                seen_groups.insert(group_of(k));
            }
        }
        // BOTH SIDES, because a shard's view is a slice. Two shards can sample
        // one group, and a group clear in one and moved in the other is MOVED —
        // so the collector needs what moved as well as what cleared, or the
        // clear half of a split group would carry the whole of it.
        for g in seen_groups.difference(&bad) {
            println!("verified-group: {bench_id} {g}");
        }
        for g in &bad {
            println!("moved-group: {bench_id} {g}");
        }
        if compared < floor {
            eprintln!("verify: only {compared} rows compared (want {floor}) — inconclusive");
            std::process::exit(1);
        }
        eprintln!(
            "verify: {} of {compared} rows moved, {} of {} groups clear",
            moved.len(),
            seen_groups.len() - bad.len(),
            seen_groups.len()
        );
        std::process::exit(i32::from(!moved.is_empty()));
    }

    // The runtime copy, keyed by weapon because that is how the page asks.
    //
    // MERGED, NOT OVERWRITTEN. One file holds every benchmark's rows (each row
    // says which ruler it was measured under), and the scoring job runs this
    // binary ONCE PER BENCHMARK — so writing the whole file each time meant the
    // last benchmark in the loop erased the others. That was invisible while
    // there was one, and became wrong the hour a second one landed.
    //
    // This benchmark's own rows are dropped first, so a re-run replaces rather
    // than duplicates; every other benchmark's are carried through untouched,
    // which is what lets one benchmark be re-scored on its own.
    // WHAT THIS PROCESS PAID FOR, for its siblings and the merge. Written even
    // when empty: an absent file and an empty one say different things to the
    // step that collects them, and "this shard found nothing to do" is a real
    // answer.
    if let Some(path) = &emit_to {
        // THE FILE SAYS WHICH BOARD IT IS. The reader is handed a directory
        // holding every benchmark's shards, and the key inside carries no
        // benchmark — so without this the two boards' scores merge.
        // AS TEXT, NOT AS JSON NUMBERS — see `num_out`. The three maps below
        // are f64s crossing to another process, and the number path loses a ULP
        // on about one value in ten.
        let as_text = |m: &std::collections::HashMap<String, f64>| {
            m.iter().map(|(k, v)| (k.clone(), num_out(*v))).collect::<std::collections::HashMap<_, _>>()
        };
        // WHAT IS BEING WRITTEN, decided once so the maps below agree.
        // A SCORE WITHOUT ITS FINGERPRINT CANNOT BE READ BACK, so it is not
        // written: the reader admits an entry only where the row's hash still
        // matches, and `row_fps` is filled for every key this run walked. A
        // board row whose build is no longer submitted is not walked, and
        // emitting its number would put a byte in the store nothing can use.
        let emitted: std::collections::HashMap<String, f64> = if emit_all {
            let mut all = known.clone();
            all.extend(computed.iter().map(|(k, v)| (k.clone(), *v)));
            all.retain(|k, _| row_fps.contains_key(k));
            all
        } else {
            computed.clone()
        };
        let text = serde_json::to_string(&serde_json::json!({
            "benchmark": bench_id,
            // WHAT EACH ROW READ, which is the whole of what a stored score
            // has to prove. There is no code hash beside it and no need of one:
            // what a row reads is enumerable from the row, where what it
            // EXECUTES is not, so the second question is answered by the audit
            // measuring rather than by anything here declaring.
            // ONE PER SCORE AND NO MORE. `row_fps` holds every key the run
            // walked, most of which it neither computed nor is emitting.
            "fps": emitted
                .keys()
                .filter_map(|k| row_fps.get(k).map(|f| (k.clone(), f.clone())))
                .collect::<std::collections::HashMap<_, _>>(),
            "scores": as_text(&emitted),
            // WHAT EACH ONE COST, so the publish step can write it into the
            // board and the NEXT run can pack the shards with it. Without this
            // the figure survives exactly one run: publish measures almost
            // nothing, so it would write the default over every real number.
            // THE PRIOR BOARD'S FIGURES TOO, under `--emit-all`. They are what
            // packs the next run's shards, and a merged set that dropped them
            // would charge every row the default and deal the tail to one shard.
            "costs": as_text(&if emit_all {
                let mut all = prior_costs.clone();
                all.extend(known_costs.iter().map(|(k, v)| (k.clone(), *v)));
                all.extend(costs.iter().map(|(k, v)| (k.clone(), *v)));
                all
            } else {
                costs.clone()
            }),
            // …AND THE PROBE OF EVERY ROW THIS SHARD SCREENED, apart from the
            // scores because a probe is not one. The publish process walks
            // every row and would otherwise retake each of these alone.
            "probes": as_text(&probes),
            // Only the rows this shard actually searched; a plain build has no
            // entry, so an ordinary board's shard file is what it always was.
            "rolls": emitted
                .keys()
                .filter_map(|k| rolls.get(k).map(|r| (k.clone(), r.clone())))
                .collect::<std::collections::HashMap<_, _>>(),
        }))
        .expect("scores");
        std::fs::write(path, text).unwrap_or_else(|e| panic!("write {path}: {e}"));
        eprintln!(
            "shard {shard}/{shards}: scored {} rows -> {path}",
            computed.len()
        );
    }

    if let Some(path) = std::env::args().nth(2).filter(|p| !p.starts_with("--")) {
        let mut by_weapon: std::collections::BTreeMap<String, Vec<Value>> = Default::default();
        if let Ok(prior) = std::fs::read_to_string(&path) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&prior) {
                for (weapon, rows) in map {
                    let keep: Vec<Value> = rows
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter(|r| {
                                    let b =
                                        r.get("benchmark").and_then(Value::as_str).unwrap_or("");
                                    family(b) != family(&bench_id)
                                })
                                .cloned()
                                .collect()
                        })
                        .unwrap_or_default();
                    if !keep.is_empty() {
                        by_weapon.insert(weapon, keep);
                    }
                }
            }
        }
        for r in &kept {
            by_weapon
                .entry(r.weapon.clone())
                .or_default()
                .push(page_row(&bench_id, r));
        }
        std::fs::write(&path, serde_json::to_string(&by_weapon).expect("json"))
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        eprintln!("wrote {path}");
    }
    println!("# THE OFFICIAL BOARD for `{bench_id}` — GENERATED by `wfsim-board`.");
    println!("#");
    println!("# Rows are BUILDS players submitted; every score was computed here, by");
    println!("# this engine, under that benchmark's own pinned seed. Nobody submits a");
    println!("# number, so any row can be reproduced exactly by anyone with the repo.");
    println!("#");
    println!("# Regenerated whole on every run — never edited by hand, and never");
    println!("# merged: an engine or data change re-scores everything rather than");
    println!("# migrating anything, because the builds are still builds.");
    println!("benchmark: {bench_id}");
    println!("source: submissions");
    // HOW MANY BUILDS THIS RUN READ, so the page can say whether the board on
    // screen is current: the library reports its own size at
    // `/api/board/pending`, and the difference is what has arrived since. The
    // board is a static file and always will be — this is the one fact the file
    // cannot carry about itself.
    println!("submissions: {seen}");
    // THE FINGERPRINT THIS BOARD WAS SCORED UNDER, so the next run can tell
    // whether these numbers are still its own answer. Absent = "scored by an
    // engine that did not record one", which reads as a full rescore.
    // WHAT THE RUNTIME NEEDS, and only that.
    //
    // The rows are not embedded — `boards/` is outside `data/` for exactly that
    // reason — so the page's three scalars per board come from a small
    // generated file that is. Merged rather than overwritten, the same as
    // `board.json`: this binary runs once per benchmark.
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
        // THE CURSOR ONLY MOVES WHEN A SLICE WAS TAKEN. A run that took none —
        // a full rescore, a verify, a board with nothing stale — leaves it
        // where it was rather than resetting the rotation to the top.
        let cursor = refresh_next.unwrap_or_else(|| state.get(&bench_id).map_or(0, |e| e[4]));
        state.insert(
            bench_id.clone(),
            [seen, kept.len(), below.len() + probed.len(), now, cursor],
        );
        let mut out = String::from(BOARD_STATE_HEADER);
        out.push_str("boards:
");
        for (id, [subs, listed, held, at, cur]) in &state {
            out.push_str(&format!(
                "  {id}:
    submissions: {subs}
    listed: {listed}
    held: {held}
    scored_at_epoch_seconds: {at}
    refresh_cursor: {cur}
"
            ));
        }
        std::fs::write(BOARD_STATE, out).unwrap_or_else(|e| panic!("{BOARD_STATE}: {e}"));
    }
    println!("entries:");
    // KEPT FIRST, THEN THE ONES THE FLOOR HELD BACK. Two populations in one
    // file, told apart by `listed:`, because the file is BOTH the record and
    // the next run's reuse cache and the second population belongs in each: a
    // scored row absent from it is indistinguishable from a lost one, and it
    // has no cached score, so it is re-fought every run to be discarded again.
    //
    // CACHING A LOW ROW CANNOT FREEZE A WRONG NUMBER — reuse is gated on the
    // engine fingerprint and on the row's own `fp`. `site/board.json` is
    // written from `kept` alone, so the page is unchanged.
    // TWO POPULATIONS, WALKED SEPARATELY. Not one pass over a set of ids: the
    // same BUILD can be listed in one mode and held below the floor in another,
    // so the flag is a property of the ROW and only the loop it came from knows
    // it.
    for (group, listed_here) in [(&kept, true), (&below, false), (&probed, false)] {
        for r in group.iter() {
            println!("  - weapon: {}", r.weapon);
            println!("    mode: {}", r.mode);
            // WRITTEN ONLY WHERE IT IS FALSE, so a board of listed rows is
            // byte-for-byte what it was and the default carries the common case.
            if !listed_here {
                println!("    listed: false");
            }
            // A SCREENED ROW SAYS SO. Its score is a probe at a tenth of the
            // ruler's runs, so it is neither published nor reused — the reader
            // and `reuse_prior` both need that from the row itself.
            if r.probe {
                println!("    probe: true");
            }
            // FULL PRECISION in the record. `{}` on an f64 is the shortest string
            // that reads back as the same number, so the yaml is the measurement
            // rather than a rounding of it — the published figure is rounded at the
            // point it is SHOWN, and two rows that tie on screen still rank.
            println!("    score: {}", r.score);
            // WHAT THIS SCORE DEPENDS ON. The next run recomputes it from the row
            // and reuses the number only if it matches, which is what makes a mod
            // correction cost the rows carrying that mod instead of the board.
            if !r.fp.is_empty() {
                println!("    fp: {}", r.fp);
            }
            // WHAT IT COST, to the millisecond — enough to tell a monster from
            // the median and no more. It is scheduling data rather than part of
            // the answer, so it is written where it survives (the board is what
            // carries between runs) and rounded where it stops being useful.
            if r.cost_seconds > 0.0 {
                println!("    cost: {:.3}", r.cost_seconds);
            }
            if !r.exilus.is_empty() {
                println!("    exilus: {}", r.exilus);
            }
            // THE PARTS, on the same terms as the exilus above: omitted on a
            // row that takes none, so every existing row is byte for byte what
            // it was and the Python side has nothing new to copy on one.
            if !r.grip.is_empty() {
                println!("    grip: {}", r.grip);
            }
            if !r.loader.is_empty() {
                println!("    loader: {}", r.loader);
            }
            println!("    mods: [{}]", r.mods.join(", "));
            if !r.evolutions.is_empty() {
                println!("    evolutions: [{}]", r.evolutions.join(", "));
            }
            if r.arcanes.iter().any(|a| a != "none") {
                println!("    arcanes: [{}]", r.arcanes.join(", "));
            }
            // Written only where there is one, the same rule the two lines above
            // follow — a board of ordinary weapons is byte-for-byte what it was.
            if !r.valence.is_empty() {
                println!("    valence: {}", r.valence);
            }
            // THE RIVEN, where there is one — and both halves of it. The SHAPE is
            // what a player acts on ("roll this weapon for these stats"); the ROLLS
            // are what the number rests on, and what lets opening this row build
            // the riven on the reader's own machine.
            if let Some(rv) = &r.riven {
                println!("    riven:");
                println!("      bonuses: [{}]", rv.bonuses.join(", "));
                if let Some(m) = &rv.malus {
                    println!("      malus: {m}");
                }
                println!(
                    "      rolls: [{}]",
                    rv.rolls
                        .iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
        }
    }
}

/// The ROLLS a previous pass settled on, from the same shard files
/// [`load_scores`] reads. Same key, same benchmark guard, same merge rule.
fn load_rolls(spec: Option<String>, bench_id: &str) -> std::collections::HashMap<String, Vec<f64>> {
    let mut out: std::collections::HashMap<String, Vec<f64>> = Default::default();
    let Some(dir) = spec else { return out };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for e in entries.flatten() {
        let Ok(text) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        let Ok(file) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if file.get("benchmark").and_then(Value::as_str) != Some(bench_id) {
            continue;
        }
        let Some(rolls) = file.get("rolls").and_then(Value::as_object) else {
            continue;
        };
        for (k, v) in rolls {
            let Some(a) = v.as_array() else { continue };
            out.insert(k.clone(), a.iter().filter_map(Value::as_f64).collect());
        }
    }
    out
}

/// HOW MANY RUNS A CORNER PROBE GETS. Only ever used to pick BETWEEN corners,
/// never to publish: the winner is then measured at the ruler's own count.
///
/// The corners of a roll band are far apart — a stat at 0.9 against the same
/// stat at 1.1 — so choosing between them does not need the precision the
/// published number does, and paying for it sixteen times would make a riven
/// row cost sixteen plain ones on a board that already takes an hour.
///
/// A HUNDRED, not sixty: the board is the reference and a
/// corner search that decides which card to tell people to go and get should
/// read like one. It is still a fraction of the ruler's own 1000, and the
/// sixteen probes are what the two-decision structure buys — the CHOICE is made
/// here and the published NUMBER is measured afterwards at full precision.
///
/// A CORNER THAT THE FIGHT CANNOT SEPARATE COSTS NOTHING EXTRA, because every
/// probe runs under the ruler's own pinned seed: two corners differing only in
/// something this arena ignores return the same f64 bit for bit, and
/// `rivens_data::perfect` then breaks the tie toward the PLAYER rather than
/// toward whichever end noise happened to favour.
const PROBE_RUNS: u32 = 100;

/// The mod id a RIVEN takes in a simulate request. A record spells it `riven`
/// (the endpoint's ids are `[a-z0-9_]`); the request names an ITEM.
const RIVEN_ITEM: &str = "riven:board";

/// One riven, as the `rivens` array of a simulate request.
fn riven_request(spec: &wfsim_engine::rivens_data::RivenSpec) -> Value {
    let stat = |s: &wfsim_engine::rivens_data::RolledStat| json!({ "id": s.id, "roll": s.roll });
    json!([{
        "name": RIVEN_ITEM.trim_start_matches("riven:"),
        "spec": {
            "bonuses": spec.bonuses.iter().map(stat).collect::<Vec<_>>(),
            "malus": spec.malus.as_ref().map(stat),
            "rank": spec.rank,
            "polarity": "madurai",
        }
    }])
}

/// `webapi::simulate_json` under a name that says the crate boundary is
/// deliberate: the scorer runs the SAME entry point the web api runs, so the
/// board cannot drift from what the page computes.
fn wfsim_engine_webapi_simulate(v: &Value) -> Value {
    wfsim_webapi::simulate_json(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A SCORE CROSSES BETWEEN PROCESSES WITHOUT MOVING.
    ///
    /// Every score a shard computes reaches the publish process through a file,
    /// and `serde_json`'s NUMBER parser is not correctly rounding: written as a
    /// JSON number, `1.1070976928071055` comes back one ULP away as `...057`,
    /// and so does about one board value in ten. The board published the moved
    /// number while a reader reproducing the row from the repo — which the board
    /// invites — got the engine's. The three values here are ones it moves.
    #[test]
    fn a_number_crossing_between_processes_is_the_number_that_was_computed() {
        for v in [1.107_097_692_807_105_5_f64, 0.987_342_303_252_504_9, 31.138_033_906_687_234] {
            let wire: Value = serde_json::from_str(
                &serde_json::to_string(&serde_json::json!({ "x": num_out(v) })).unwrap(),
            )
            .unwrap();
            let back = num_in(wire.get("x").unwrap()).expect("a carried number reads back");
            assert_eq!(back.to_bits(), v.to_bits(), "{v} did not survive the carry");
        }
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
                        simulate_request(&scenario, &v, *a),
                        simulate_request(&scenario, &v, *b),
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
            assembly: None,
            forma: 0,
            drain: 0,
        };
        let modes = wfsim_engine::weapons_data::play_modes("ballistica_prime");
        let m = modes
            .iter()
            .find(|m| m.id == "alternate_cycle")
            .expect("alternate_cycle");
        let req = simulate_request(&scenario, &v, *m);
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

    /// **THE PROBE SCREENS THE HOPELESS AND NEVER A CONTENDER.**
    ///
    /// The cut is `PROBE_MARGIN x FLOOR x leader`, so a row has to read a
    /// QUARTER of its group's best before the full measurement is skipped. That
    /// margin is what makes a coarse measurement safe to act on: the probe runs
    /// a tenth of the ruler's iterations, so its standard error is about three
    /// times larger, and 4x is far outside anything that explains.
    ///
    /// ASSERTED ON THE BOUNDARY rather than on a fight, because the boundary is
    /// the decision: everything at or above the floor is measured, and so is
    /// everything between the floor and the cut.
    #[test]
    fn the_probe_screens_only_what_is_far_below_its_group() {
        let leader = 100.0;
        let cut = PROBE_MARGIN * FLOOR * leader;
        assert_eq!(cut, 25.0, "a quarter of the leader");
        // The floor itself is 50: a row there is published, and everything
        // between 25 and 50 is measured even though it will not be listed —
        // being close enough to matter is exactly when precision is worth
        // paying for, because the leader can fall.
        assert!(FLOOR * leader > cut, "the cut must sit BELOW the publication floor");
        for s in [leader, FLOOR * leader, cut + 0.01] {
            assert!(s >= cut, "{s} would have been screened, and it is a contender");
        }
        for s in [0.0, 1.0, cut - 0.01] {
            assert!(s < cut, "{s} is hopeless and should cost a probe, not a measurement");
        }
    }

    fn row(weapon: &str, mode: &str, score: f64) -> Row {
        Row {
            // DISTINCT PER ROW, because the accounting partitions on it: a
            // fixture where every row shared one identity would make the
            // "everything was ranked" assertion pass on a single build.
            identity: format!("{weapon}|{mode}|{score}"),
            probe: false,
            cost_seconds: 0.0,
            weapon: weapon.into(),
            mode: mode.into(),
            score,
            mods: vec![],
            evolutions: vec![],
            arcanes: vec![],
            valence: String::new(),
            exilus: String::new(),
            grip: String::new(),
            loader: String::new(),
            riven: None,
            fp: String::new(),
        }
    }

    /// The same row, carrying a riven — so the floor's groups can be told apart.
    fn riven_row(weapon: &str, mode: &str, score: f64) -> Row {
        Row {
            riven: Some(RowRiven {
                bonuses: vec!["damage".into(), "multishot".into()],
                malus: None,
                rolls: vec![1.1, 1.1],
            }),
            ..row(weapon, mode, score)
        }
    }

    fn scores(rows: &[Row], weapon: &str, mode: &str) -> Vec<f64> {
        rows.iter()
            .filter(|r| r.weapon == weapon && r.mode == mode)
            .map(|r| r.score)
            .collect()
    }

    /// THE FLOOR IS HALF THE GROUP'S LEADER, and the boundary is INCLUSIVE —
    /// exactly half is listed. A cut line drawn with `>` would delete the one
    /// row that is precisely on it, which is the row a reader is most likely to
    /// go looking for.
    #[test]
    fn half_of_the_leader_is_kept_and_less_is_not() {
        let (kept, below) = keep_above_floor(vec![
            row("torid", "cycle", 80.0),
            row("torid", "cycle", 40.0),   // exactly half
            row("torid", "cycle", 39.999), // a hair under
            row("torid", "cycle", 1.0),
        ]);
        assert_eq!(scores(&kept, "torid", "cycle"), vec![80.0, 40.0]);
        assert_eq!(below.len(), 2);
    }

    /// PER WEAPON AND MODE, so a strong group cannot decide what a weak one may
    /// show. A shared reference would have let the Torid's cycle — three times
    /// its base form here — empty the base form's list entirely, which is the
    /// opposite of what the mode dimension is for.
    #[test]
    fn each_group_is_measured_against_its_own_leader() {
        let (kept, below) = keep_above_floor(vec![
            row("torid", "cycle", 90.0),
            row("torid", "cycle", 50.0),
            row("torid", "base", 30.0),
            row("torid", "base", 20.0), // 22% of the cycle's leader, 67% of its own
            row("lex", "base", 10.0),
            row("lex", "base", 9.0),
        ]);
        assert_eq!(scores(&kept, "torid", "base"), vec![30.0, 20.0]);
        assert_eq!(scores(&kept, "lex", "base"), vec![10.0, 9.0]);
        assert_eq!(below.len(), 0);
    }

    /// THERE IS NO CEILING. The count this replaced was a hundred; a group whose
    /// builds are genuinely close keeps every one of them, however many arrive.
    #[test]
    fn a_close_group_keeps_everything() {
        let rows: Vec<Row> = (0..250)
            .map(|i| row("furis", "cycle", 100.0 - i as f64 * 0.1))
            .collect();
        let (kept, below) = keep_above_floor(rows);
        assert_eq!(kept.len(), 250);
        assert_eq!(below.len(), 0);
    }

    /// A LEADER OF ZERO SEPARATES NOTHING. Every row ties it, so the group is
    /// published whole rather than emptied — a ratio has nothing to say when
    /// there is no scale, and deleting a weapon nobody could make kill would
    /// read as a weapon nobody had tried.
    #[test]
    fn a_group_that_scored_nothing_is_not_emptied() {
        let (kept, below) =
            keep_above_floor(vec![row("stug", "base", 0.0), row("stug", "base", 0.0)]);
        assert_eq!(kept.len(), 2);
        assert_eq!(below.len(), 0);
    }

    fn tmpdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("wfsim-board-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("tmpdir");
        d
    }

    /// TWO BOARDS SCORING ONE BUILD PRODUCE THE SAME KEY AND DIFFERENT NUMBERS,
    /// and the publish step is handed ONE directory holding every benchmark's
    /// shards (`.github/workflows/board.yml`: eight shards x every ruler ->
    /// `scores/`, then `--scores scores` once per ruler). Merging them published
    /// one ruler's score under the other's name: the Torid's aimed 28.442 kpm
    /// sat at the top of the NO-AIM board, where that build scores 0.170.
    ///
    /// The merged number also WINS over the board's own correct history, since
    /// `--reuse` only fills where `--scores` left a hole — which is why only the
    /// rows the other ruler happened to rescore that run were wrong, and why it
    /// read as a scenario leak rather than as a file being read twice.
    #[test]
    fn a_score_file_belongs_to_one_board_and_another_boards_is_refused() {
        let d = tmpdir("cross");
        let key = "torid#cycle";
        std::fs::write(
            d.join("single_target-0.json"),
            r#"{"benchmark":"single_target","scores":{"torid#cycle":28.44229348067104}}"#,
        )
        .unwrap();
        std::fs::write(
            d.join("single_target_no_aim-0.json"),
            r#"{"benchmark":"single_target_no_aim","scores":{"torid#cycle":0.17033484369504454}}"#,
        )
        .unwrap();

        let spec = Some(d.to_string_lossy().into_owned());
        let (aimed, _, _, _) = load_scores(spec.clone(), "single_target");
        let (no_aim, _, _, _) = load_scores(spec.clone(), "single_target_no_aim");
        assert_eq!(aimed.get(key).copied(), Some(28.44229348067104));
        assert_eq!(no_aim.get(key).copied(), Some(0.17033484369504454));
        // The sharp one: neither board may see the other's, in EITHER direction
        // — the file order decides which one wins, and it is a sort over names.
        assert_eq!(aimed.len(), 1, "the aimed board read another ruler's file");
        assert_eq!(
            no_aim.len(),
            1,
            "the no-aim board read another ruler's file"
        );

        // A ruler with no file of its own reuses nothing rather than reusing
        // whatever else is in the directory.
        assert!(load_scores(spec, "group_clear").0.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Shards of ONE board still merge, which is the whole point of the file.
    #[test]
    fn shards_of_the_same_board_merge() {
        let d = tmpdir("shards");
        std::fs::write(
            d.join("single_target-0.json"),
            r#"{"benchmark":"single_target","scores":{"a#base":1.0}}"#,
        )
        .unwrap();
        std::fs::write(
            d.join("single_target-1.json"),
            r#"{"benchmark":"single_target","scores":{"b#base":2.0}}"#,
        )
        .unwrap();
        let (got, _, _, _) =
            load_scores(Some(d.to_string_lossy().into_owned()), "single_target");
        assert_eq!(got.len(), 2);
        assert_eq!(got.get("a#base").copied(), Some(1.0));
        assert_eq!(got.get("b#base").copied(), Some(2.0));
        let _ = std::fs::remove_dir_all(&d);
    }
    /// THE FLOOR PARTITIONS BY RIVEN, and the ranking does not.
    ///
    /// A riven build and a plain one compete with each other for nothing, so a
    /// shared reference would let whichever is stronger on this weapon decide
    /// what the other may show — and on most weapons that is the riven build,
    /// which would take the plain ones with it. Those are the builds most
    /// players can actually make.
    #[test]
    fn the_floor_is_drawn_per_riven_ness_and_the_list_is_still_one() {
        // A strong riven leader and a plain group far below it. Every plain row
        // survives on its OWN leader; under one shared reference all three
        // would be gone.
        let (kept, below) = keep_above_floor(vec![
            riven_row("torid", "cycle", 100.0),
            riven_row("torid", "cycle", 60.0),
            riven_row("torid", "cycle", 40.0), // 40% of the riven leader
            row("torid", "cycle", 20.0),
            row("torid", "cycle", 12.0),
            row("torid", "cycle", 11.0), // 55% of the PLAIN leader, and 11% of the riven one
        ]);
        assert_eq!(below.len(), 1, "only the riven row under half its own leader");
        assert_eq!(kept.len(), 5);
        assert_eq!(kept.iter().filter(|r| r.riven.is_some()).count(), 2);
        assert_eq!(kept.iter().filter(|r| r.riven.is_none()).count(), 3);

        // ONE LIST, still sorted by score across both kinds — a riven build
        // does not always beat a plain one, so ranking them apart would publish
        // a comparison the fight does not make.
        let scores: Vec<f64> = kept.iter().map(|r| r.score).collect();
        assert!(scores.windows(2).all(|w| w[0] >= w[1]), "{scores:?}");

        // AND THE PARTITION IS NOT PER WEAPON ONLY: another weapon's riven
        // leader must not set this one's floor either.
        let (kept, _) = keep_above_floor(vec![
            riven_row("laetum", "base", 1000.0),
            riven_row("torid", "cycle", 10.0),
            riven_row("torid", "cycle", 6.0),
        ]);
        assert_eq!(kept.len(), 3);
    }
}

#[cfg(test)]
mod page_row_tests {
    use super::*;

    fn row(riven: Option<RowRiven>) -> Row {
        Row {
            identity: "dual_toxocyst|cycle".into(),
            probe: false,
            cost_seconds: 0.0,
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
            fp: "0123456789abcdef".into(),
        }
    }

    /// **THE RIVEN REACHES THE PAGE**, which is the whole of what went wrong.
    ///
    /// A row wearing one was written to `site/board.json` without it for as
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

    /// THE BACKSTOP REACHES EVERY PRECISION, down to one row.
    ///
    /// It is the button for the case nothing automatic covers, so what it can
    /// NAME is the whole of its worth: a weapon, one of that weapon's modes,
    /// one riven category within a mode, or a single build. Each line below is
    /// one of those four, and the negatives are the ways a looser match would
    /// quietly rescore rows nobody asked for.
    #[test]
    fn a_selector_names_rows_at_every_precision() {
        let felarx = "felarx|galvanized_chamber,serration#cycle";
        let prime = "felarx_prime|galvanized_chamber,serration#cycle";
        let riven = "felarx|riven,serration#cycle";
        let base = "felarx|galvanized_chamber,serration#base";

        let hits = |sel: &str, key: &str| Selector::parse(sel).unwrap().matches(key);

        // A weapon: every mode, every build.
        assert!(hits("felarx", felarx));
        assert!(hits("felarx", base));
        // AND NOT THE PRIME. A prefix that stops mid-component names a weapon
        // whose rows the mechanic never touched, and the operator paying for
        // the rescore has no way to see it happened.
        assert!(!hits("felarx", prime));

        // One mode of it.
        assert!(hits("felarx#cycle", felarx));
        assert!(!hits("felarx#cycle", base));

        // One riven category within that mode.
        assert!(hits("felarx#cycle:plain", felarx));
        assert!(!hits("felarx#cycle:plain", riven));
        assert!(hits("felarx#cycle:riven", riven));
        assert!(!hits("felarx#cycle:riven", felarx));

        // A single build, and a single row of it.
        assert!(hits("felarx|galvanized_chamber,serration", felarx));
        assert!(hits("felarx|galvanized_chamber,serration", base));
        assert!(!hits("felarx|galvanized_chamber,serration#base", felarx));
        assert!(!hits("felarx|galvanized_chamber", felarx));

        // SEPARATED BY `;`, because a mod list is commas: splitting on those
        // would leave the backstop unable to name a build at all.
        let many: Vec<Selector> = "felarx#base;torid#cycle".split(';')
            .filter_map(Selector::parse)
            .collect();
        assert_eq!(many.len(), 2);
        assert!(many.iter().any(|s| s.matches(base)));
    }
}
