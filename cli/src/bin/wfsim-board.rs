//! THE SCORER — turns submitted builds into a board.
//!
//! Reads a JSON array of submissions on stdin, writes the board YAML on
//! stdout. That shape is deliberate: fetching from KV and committing the
//! result are the workflow's job (`.github/workflows/board.yml`), and neither
//! needs the engine. What needs the engine is the only thing here — running
//! each build under the benchmark and reading the number off.
//!
//! WHY A SUBMISSION CARRIES NO SCORE, restated because this binary is where it
//! becomes true: nobody's number is trusted because nobody's number is asked
//! for. A row's score is produced HERE, by this engine, under the benchmark's
//! own pinned seed — so anyone with the repo can reproduce any row exactly, in
//! a browser as well as natively. A forged submission cannot forge a rank; the worst it can
//! do is submit a build that scores badly.
//!
//! It also means an engine change re-scores everything instead of migrating
//! anything: the builds are still builds. Nobody is ever asked to resubmit.
//!
//! TWO OUTPUTS, because the board has two readers with different needs. The
//! YAML on stdout is the CANONICAL record, committed and diffable. The JSON is
//! what the PAGE fetches at runtime — and it exists because a board that
//! changes hourly must not require rebuilding a 2.5 MB wasm to reach anyone.
//! Compiling it in made every board update a full site rebuild: install
//! wasm-bindgen, fetch 300 images, recompile — to change a few numbers.
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
    row
}

/// THE FLOOR: a row must score at least half its group's leader to be listed. It replaces a COUNT — the top hundred per ruler and
/// mode, itself raised from ten on 2026-08-08.
///
/// A COUNT AND A FLOOR BOUND DIFFERENT THINGS. The count bounded how LONG the
/// list could get and said nothing about whether the hundredth row was worth
/// reading. On the board of 2026-08-19 the three groups that reached the cap
/// had a hundredth row at 18.6%, 25.9% and 25.4% of their leader — so the list
/// had stopped being about builds anybody would pick long before the cap cut
/// it, and the cap was trimming the wrong end.
///
/// WHAT IT REMOVES IS NOT THE CHEAP BUILD. That was the objection, and this
/// board refutes it: the rows below the line carry 8 of 8 mods exactly like the
/// rows above, and they differ by taking the WORSE arcane (Merciless where
/// Deadhead wins) or by spending slots on mods this fight cannot pay — Magazine
/// Extension, Parallax Scope, Quick Reload, which docs/UNMODELLED.md already
/// says are worth nothing against one standing target. Of 86 groups, three have
/// ever held a row with no arcane at all, and in each of them it was the leader.
///
/// IT IS MECHANICAL, and that is the decision. The seed is pinned and a score
/// reproduces to the last digit, so 50.3% and 49.5% are two different NUMBERS
/// rather than two estimates of one — a board whose rows are exact has no tie
/// band to grant, and the ruler separating two builds is what the ruler is for.
///
/// FIFTY IS A CUT LINE, not a measurement, and the file says so rather than
/// implying otherwise. The pooled distribution of score-as-a-fraction-of-leader
/// has no knee to sit on (the largest gap anywhere below 90% is 1.2 points), so
/// the data cannot pick the number; what it can say is that the number is not
/// fragile — about 12 of 1274 rows per point, so 45 or 55 would cost a few per
/// cent rather than a shape. Against the sports that draw the same kind of line
/// (F1's 107% rule, cycling's 3-20% time limit) half the leader is very
/// generous, which is the intent: it marks where a build stops being a
/// DIFFERENT answer, not where it stops being the best one.
///
/// THERE IS NO CEILING NOW, so a group whose builds are genuinely close keeps
/// every one of them. A group whose leader scores zero keeps all of its rows
/// too — every row ties it, and a ratio has nothing to separate.
const FLOOR: f64 = 0.5;

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
fn keep_above_floor(mut rows: Vec<Row>) -> (Vec<Row>, usize) {
    rows.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut leader: std::collections::BTreeMap<(String, String, bool), f64> = Default::default();
    let mut kept = Vec::new();
    let mut below = 0usize;
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
            below += 1;
        }
    }
    (kept, below)
}

/// A flag's value, `--name value` anywhere after the positionals.
fn flag(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
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
fn load_scores(spec: Option<String>, bench_id: &str) -> std::collections::HashMap<String, f64> {
    let mut out = std::collections::HashMap::new();
    let Some(spec) = spec else { return out };
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    let p = std::path::Path::new(&spec);
    if p.is_dir() {
        if let Ok(rd) = std::fs::read_dir(p) {
            files.extend(rd.flatten().map(|e| e.path()).filter(|f| {
                f.extension().is_some_and(|e| e == "json")
            }));
        }
    } else {
        files.push(p.to_path_buf());
    }
    files.sort();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        let Ok(file) = serde_json::from_str::<Value>(&text) else { continue };
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
        let Some(scores) = file.get("scores").and_then(Value::as_object) else { continue };
        for (k, v) in scores {
            if let Some(n) = v.as_f64() {
                out.insert(k.clone(), n);
            }
        }
    }
    out
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
    /// Rows whose OWN data moved. Printed rather than counted silently: it is
    /// the number that says how much a change actually cost.
    stale: usize,
}

fn reuse_prior(path: &str, code_fp: &str, bench_id: &str) -> Result<Prior, String> {
    if code_fp.is_empty() {
        return Err("no engine fingerprint given".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let prior = wfsim_engine::boards_data::parse(&text).map_err(|e| format!("{path}: {e}"))?;
    if prior.engine != code_fp {
        return Err(format!(
            "engine code moved ({} -> {code_fp})",
            if prior.engine.is_empty() { "unrecorded" } else { &prior.engine }
        ));
    }
    if family(&prior.benchmark) != family(bench_id) {
        return Err(format!("{path} is {}'s board", prior.benchmark));
    }
    let mut out = Prior::default();
    for e in &prior.entries {
        let Ok(v) = wfsim_engine::builds::validate_for_board_with(
            bench_id, &e.weapon, &e.mods, &e.evolutions, &e.arcanes, &e.valence,
            e.riven.as_ref().map(wfsim_engine::boards_data::BoardRiven::shape).as_ref(),
            Some(e.exilus.as_str()).filter(|x| !x.is_empty()),
        ) else {
            continue;
        };
        // THE ROW'S OWN DATA, recomputed from the row rather than trusted — the
        // same reason its identity is recomputed one line down. A row written
        // before per-row fingerprints existed carries an empty one and is
        // rescored, which is the safe direction and the only one available.
        let want = wfsim_engine::data_fingerprint::row_fingerprint(
            bench_id, &v.weapon, &v.mods, &v.arcanes, &v.evolutions, v.exilus.as_deref(),
        );
        if e.fp != want {
            out.stale += 1;
            continue;
        }
        let key = wfsim_engine::builds::board_key(&v, &e.mode);
        if let Some(rv) = &e.riven {
            if !rv.rolls.is_empty() {
                out.rolls.insert(key.clone(), rv.rolls.clone());
            }
        }
        out.scores.insert(key, e.score);
    }
    Ok(out)
}

fn main() {
    let bench_id = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: wfsim-board <benchmark-id> [board.json] [--shard i/n] \
                   [--scores <file|dir>] [--emit-scores <file>]  (submissions on stdin)");
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
            (i.parse::<usize>().unwrap_or(0), n.parse::<usize>().unwrap_or(1).max(1))
        }
        None => (0, 1),
    };
    // THE ENGINE FINGERPRINT — a hash of everything a score depends on that is
    // not the build: `engine/`, `webapi/`, `cli/`, and `data/` minus the boards
    // themselves. The workflow computes it from the index and hands it in.
    //
    // It is what makes reuse EXACT rather than a guess. A score is a pure
    // function of (build, the ruler's terms, this code and this data), so if
    // the fingerprint is unchanged the stored number is not merely probably
    // still right — it is the same number this run would compute, and running
    // the fight again would be spending an hour to reproduce it.
    //
    // A COOLDOWN WOULD BE THE WRONG AXIS.
    // Time is not an input: an untouched score is valid forever, and a score
    // whose engine moved is wrong immediately, not in an hour.
    // WHAT `--engine` IS NOW: the CODE, and only the code (`engine`, `webapi`,
    // `cli`). Hashing `data/` in as well makes adding a weapon — a file no
    // existing row reads — invalidate every stored score and buy a full
    // rescore: about an hour of wall clock and thirty of CPU to reproduce
    // numbers that could not have moved. The data half is asked PER ROW, from
    // the files that row actually reads.
    let engine_fp = flag("--engine").unwrap_or_default();
    let mut known = load_scores(flag("--scores"), &bench_id);
    let mut reused = 0usize;
    let mut stale = 0usize;
    let mut prior_rolls: std::collections::HashMap<String, Vec<f64>> = Default::default();
    if let Some(path) = flag("--reuse") {
        match reuse_prior(&path, &engine_fp, &bench_id) {
            Ok(p) => {
                reused = p.scores.len();
                stale = p.stale;
                prior_rolls = p.rolls;
                // The shards' own scores win: they were computed by THIS run.
                for (k, v) in p.scores {
                    known.entry(k).or_insert(v);
                }
            }
            Err(why) => eprintln!("full rescore: {why}"),
        }
    }
    let emit_to = flag("--emit-scores");
    let mut computed: std::collections::HashMap<String, f64> = Default::default();
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
    let metric = scenario
        .get("metric")
        .and_then(Value::as_str)
        .unwrap_or("kpm")
        .to_string();
    let duration = scenario.get("duration").and_then(Value::as_f64).unwrap_or(300.0);
    assert!(
        matches!(metric.as_str(), "kpm" | "dps"),
        "unknown benchmark metric {metric:?} — a row published in units nobody          named is worse than no row"
    );

    let mut rows: Vec<Row> = Vec::new();
    // …and how many arrived with NO mode. Those are records written before the
    // endpoint stored one, and they are read by the fallback below rather than
    // by their submitter's choice — so the count is how much of this board is
    // still a guess. It goes to zero on its own as players resubmit; printing
    // it is what makes that visible instead of permanent.
    let mut legacy = 0usize;
    let (mut seen, mut refused) = (0usize, 0usize);
    let mut seen_ids: std::collections::HashSet<String> = Default::default();
    for (idx, s) in subs.iter().enumerate() {
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
        let weapon = s.get("weapon").and_then(Value::as_str).unwrap_or("").to_string();
        let get = |k: &str| -> Vec<String> {
            s.get(k)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
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
            let malus = s.get("riven_neg").and_then(Value::as_str).filter(|x| !x.is_empty());
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
        let exilus = s.get("exilus").and_then(Value::as_str).filter(|x| !x.is_empty());
        let v = match wfsim_engine::builds::validate_for_board_with(
            &bench_id, &weapon, &mods, &evos, &arcs, valence, shape.as_ref(), exilus,
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

        // THE MODE THIS ENTRY WAS SUBMITTED FOR, and the fight it implies.
        //
        // A submission with no mode is played the way the arsenal plays it,
        // so a row carrying no mode keeps its score to the last digit while
        // gaining the dimension.
        let modes = wfsim_engine::weapons_data::play_modes(&v.weapon);
        let want = s.get("mode").and_then(Value::as_str);
        let played = match want {
            Some(id) => match modes.iter().find(|m| m.id == id) {
                Some(m) => *m,
                None => {
                    eprintln!("refused {weapon}: it has no `{id}` mode");
                    refused += 1;
                    continue;
                }
            },
            // No mode named: however this weapon is normally played — the cycle
            // where there is one, its arsenal form where there is not.
            None => {
                legacy += 1;
                *modes
                    .iter()
                    .find(|m| m.mode == wfsim_engine::weapons_data::PlayMode::Cycle)
                    .or_else(|| modes.first())
                    .expect("every weapon has a base mode")
            }
        };
        if !played.sustainable {
            // "Always Incarnon" is not a way to play for three hundred seconds,
            // and a board may not rank a fight nobody can hold. Derived, so no
            // benchmark has to carry a list of what it will not take.
            eprintln!("refused {weapon}: `{}` cannot be sustained for an engagement", played.id);
            refused += 1;
            continue;
        }

        // THE RIVEN'S SLOT IS SPELLED DIFFERENTLY ON THE WIRE. A record carries
        // the bare `riven` because the endpoint's ids are `[a-z0-9_]`; a
        // simulate request names the riven ITEM, which is `riven:<name>`. The
        // translation is one line and lives here so neither protocol has to
        // bend for the other.
        let mut req = scenario.clone();
        if let Some(o) = req.as_object_mut() {
            o.insert("weapon".into(), json!(v.weapon));
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
            // THE VALENCE, at the ruler's own terms: the element the entrant
            // named, and the roll's MAXIMUM whatever they said it was. Every
            // player can fuse to 60%, so ranking a lower roll would be ranking
            // how many duplicates someone farmed — the same reason every row
            // here is scored at full Forma.
            if !v.valence.is_empty() {
                o.insert("valence_element".into(), json!(v.valence));
                let max = wfsim_engine::weapons_data::valence_of(&v.weapon)
                    .map_or(0.0, |s| s.max);
                o.insert("valence_bonus".into(), json!(max));
            }
            // The one place a MODE becomes a FORM.
            o.insert("form".into(), json!(played.form()));
        }
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
        let score = match known.get(&key) {
            // A SIBLING SHARD OF THIS RUN ALREADY PAID FOR IT. Not a cache: the
            // map only ever travels between processes built from one commit.
            Some(&s) => s,
            None => {
                // Not this shard's slice: another one is simulating it right
                // now, and publishing a row for it here would mean scoring it
                // twice and ranking it once.
                if shards > 1 && idx % shards != shard {
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
                    let cls = wfsim_engine::rivens_data::class_for_weapon(&v.weapon)
                        .unwrap_or("");
                    let best = wfsim_engine::rivens_data::perfect(shape, cls, |sp| {
                        let mut probe = req.clone();
                        if let Some(o) = probe.as_object_mut() {
                            o.insert("rivens".into(), riven_request(sp));
                            o.insert("runs".into(), json!(PROBE_RUNS));
                        }
                        wfsim_engine_webapi_simulate(&probe)
                            .get("score")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0)
                    });
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
                        out.get("error").and_then(Value::as_str).unwrap_or("scored zero")
                    );
                    refused += 1;
                    continue;
                }
                // IN THE BENCHMARK'S OWN METRIC. `score` off the wire is kill
                // PROGRESS — kills plus the fraction of the current target
                // depleted — over the whole engagement. The benchmark says
                // `metric: kpm`, so publishing the raw figure labelled "kill
                // rate" overstated every row by the length of the fight: 55.26
                // on screen for a build that kills 11.05 a minute over 300 s. Ranking is unaffected either way — this
                // is a linear rescale — but the number people read is not a
                // ranking.
                let s = match metric.as_str() {
                    "dps" => out.get("dps").and_then(Value::as_f64).unwrap_or(0.0),
                    _ => raw * 60.0 / duration,
                };
                computed.insert(key.clone(), s);
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
        // WHAT THIS ROW READS, hashed from the CANONICAL build rather than from
        // the submission — the same object `identity` is taken from, so the
        // next run recomputes the identical hash off its own stored row.
        let fp = wfsim_engine::data_fingerprint::row_fingerprint(
            &bench_id, &v.weapon, &v.mods, &v.arcanes, &v.evolutions, v.exilus.as_deref(),
        );
        let exilus_for_row = v.exilus.clone().unwrap_or_default();
        rows.push(Row {
            weapon: v.weapon,
            mode: played.id.to_string(),
            score,
            mods: v.mods,
            evolutions: v.evolutions,
            arcanes: v.arcanes,
            valence: v.valence,
            exilus: exilus_for_row,
            riven: row_riven,
            fp,
        });
    }

    let (mut kept, below) = keep_above_floor(rows);
    kept.sort_by(|a, b| {
        a.weapon
            .cmp(&b.weapon)
            .then(b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal))
    });

    // HOW MUCH OF THIS BOARD WAS KEPT rather than recomputed, said out loud. A
    // run that reuses everything and a run that scored everything look
    // identical from the outside, and the difference is an hour.
    //
    // AND HOW MANY THE FLOOR TOOK. A build below the line is stored, scored and
    // then not listed, which from the submitter's side is indistinguishable
    // from a submission that was lost — the failure this repo has already paid
    // for twice. This log is the maintainer's half of saying so; the panel
    // that states the rule to the player is the other.
    eprintln!(
        "{seen} submissions, {refused} refused, {} rows ({reused} reused, {stale} rescored for a data change, {} scored here, {below} below the floor){}",
        kept.len(),
        computed.len(),
        // ONLY WHEN THERE ARE ANY. A board whose every row carries its
        // submitter's own choice should say nothing here — the line exists to
        // report a migration in progress, and a zero printed forever is noise
        // that trains you to skip the line that matters.
        if legacy > 0 {
            format!(" ({legacy} with no mode — read by the fallback)")
        } else {
            String::new()
        }
    );

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
        let text = serde_json::to_string(&serde_json::json!({
            "benchmark": bench_id,
            "scores": computed,
            // Only the rows this shard actually searched; a plain build has no
            // entry, so an ordinary board's shard file is what it always was.
            "rolls": computed
                .keys()
                .filter_map(|k| rolls.get(k).map(|r| (k.clone(), r.clone())))
                .collect::<std::collections::HashMap<_, _>>(),
        }))
        .expect("scores");
        std::fs::write(path, text).unwrap_or_else(|e| panic!("write {path}: {e}"));
        eprintln!("shard {shard}/{shards}: scored {} rows -> {path}", computed.len());
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
                                    let b = r.get("benchmark").and_then(Value::as_str).unwrap_or("");
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
            by_weapon.entry(r.weapon.clone()).or_default().push(page_row(&bench_id, r));
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
    if !engine_fp.is_empty() {
        println!("engine: {engine_fp}");
    }
    println!("entries:");
    for r in kept {
        println!("  - weapon: {}", r.weapon);
        println!("    mode: {}", r.mode);
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
        if !r.exilus.is_empty() {
            println!("    exilus: {}", r.exilus);
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
                rv.rolls.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ")
            );
        }
    }
}

/// The ROLLS a previous pass settled on, from the same shard files
/// [`load_scores`] reads. Same key, same benchmark guard, same merge rule.
fn load_rolls(
    spec: Option<String>,
    bench_id: &str,
) -> std::collections::HashMap<String, Vec<f64>> {
    let mut out: std::collections::HashMap<String, Vec<f64>> = Default::default();
    let Some(dir) = spec else { return out };
    let Ok(entries) = std::fs::read_dir(&dir) else { return out };
    for e in entries.flatten() {
        let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
        let Ok(file) = serde_json::from_str::<Value>(&text) else { continue };
        if file.get("benchmark").and_then(Value::as_str) != Some(bench_id) {
            continue;
        }
        let Some(rolls) = file.get("rolls").and_then(Value::as_object) else { continue };
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

    fn row(weapon: &str, mode: &str, score: f64) -> Row {
        Row {
            weapon: weapon.into(),
            mode: mode.into(),
            score,
            mods: vec![],
            evolutions: vec![],
            arcanes: vec![],
            valence: String::new(),
            exilus: String::new(),
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
        assert_eq!(below, 2);
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
        assert_eq!(below, 0);
    }

    /// THERE IS NO CEILING. The count this replaced was a hundred; a group whose
    /// builds are genuinely close keeps every one of them, however many arrive.
    #[test]
    fn a_close_group_keeps_everything() {
        let rows: Vec<Row> = (0..250).map(|i| row("furis", "cycle", 100.0 - i as f64 * 0.1)).collect();
        let (kept, below) = keep_above_floor(rows);
        assert_eq!(kept.len(), 250);
        assert_eq!(below, 0);
    }

    /// A LEADER OF ZERO SEPARATES NOTHING. Every row ties it, so the group is
    /// published whole rather than emptied — a ratio has nothing to say when
    /// there is no scale, and deleting a weapon nobody could make kill would
    /// read as a weapon nobody had tried.
    #[test]
    fn a_group_that_scored_nothing_is_not_emptied() {
        let (kept, below) = keep_above_floor(vec![
            row("stug", "base", 0.0),
            row("stug", "base", 0.0),
        ]);
        assert_eq!(kept.len(), 2);
        assert_eq!(below, 0);
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
        let aimed = load_scores(spec.clone(), "single_target");
        let no_aim = load_scores(spec.clone(), "single_target_no_aim");
        assert_eq!(aimed.get(key).copied(), Some(28.44229348067104));
        assert_eq!(no_aim.get(key).copied(), Some(0.17033484369504454));
        // The sharp one: neither board may see the other's, in EITHER direction
        // — the file order decides which one wins, and it is a sort over names.
        assert_eq!(aimed.len(), 1, "the aimed board read another ruler's file");
        assert_eq!(no_aim.len(), 1, "the no-aim board read another ruler's file");

        // A ruler with no file of its own reuses nothing rather than reusing
        // whatever else is in the directory.
        assert!(load_scores(spec, "group_clear").is_empty());
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
        let got = load_scores(Some(d.to_string_lossy().into_owned()), "single_target");
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
        assert_eq!(below, 1, "only the riven row under half its own leader");
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
            weapon: "dual_toxocyst".into(),
            mode: "cycle".into(),
            score: 139.28,
            mods: vec!["galvanized_diffusion".into()],
            evolutions: vec!["dual_toxocyst_evo1_incarnon_form".into()],
            arcanes: vec!["secondary_deadhead".into()],
            valence: String::new(),
            exilus: String::new(),
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
        let v = page_row("single_target", &row(Some(RowRiven {
            bonuses: vec!["critical_chance".into(), "multishot".into()],
            malus: Some("zoom".into()),
            rolls: vec![1.1, 1.1, 0.9],
        })));
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
        for k in ["benchmark", "mode", "source", "score", "shown", "mods",
                  "evolutions", "arcanes", "valence"] {
            assert!(v.get(k).is_some(), "{k} is missing");
        }
    }
}
