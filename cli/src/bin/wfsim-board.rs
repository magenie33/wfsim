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
//! a browser as well as natively (measured 2026-08-04: wasm and native agree to
//! the last digit). A forged submission cannot forge a rank; the worst it can
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
}

/// THE FLOOR: a row must score at least half its group's leader to be listed
/// (owner, 2026-08-20). It replaces a COUNT — the top hundred per ruler and
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
    let mut leader: std::collections::BTreeMap<(String, String), f64> = Default::default();
    let mut kept = Vec::new();
    let mut below = 0usize;
    for r in rows {
        let top = *leader.entry((r.weapon.clone(), r.mode.clone())).or_insert(r.score);
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
/// 1000 runs (measured 2026-08-11) — and it is embarrassingly parallel: every
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
        // the NO-AIM board, where that build actually scores 0.5 (2026-08-12).
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
fn reuse_prior(
    path: &str,
    engine_fp: &str,
    bench_id: &str,
) -> Result<std::collections::HashMap<String, f64>, String> {
    if engine_fp.is_empty() {
        return Err("no engine fingerprint given".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let prior = wfsim_engine::boards_data::parse(&text).map_err(|e| format!("{path}: {e}"))?;
    if prior.engine != engine_fp {
        return Err(format!(
            "engine moved ({} -> {engine_fp})",
            if prior.engine.is_empty() { "unrecorded" } else { &prior.engine }
        ));
    }
    if family(&prior.benchmark) != family(bench_id) {
        return Err(format!("{path} is {}'s board", prior.benchmark));
    }
    let mut out = std::collections::HashMap::new();
    for e in &prior.entries {
        let Ok(v) = wfsim_engine::builds::validate_for_board(
            bench_id, &e.weapon, &e.mods, &e.evolutions, &e.arcanes, &e.valence,
        ) else {
            continue;
        };
        let mode = if e.mode.is_empty() { "base" } else { e.mode.as_str() };
        out.insert(format!("{}#{}", wfsim_engine::builds::identity(&v), mode), e.score);
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
    // A COOLDOWN WOULD BE THE WRONG AXIS (owner asked about one, 2026-08-11).
    // Time is not an input: an untouched score is valid forever, and a score
    // whose engine moved is wrong immediately, not in an hour.
    let engine_fp = flag("--engine").unwrap_or_default();
    let mut known = load_scores(flag("--scores"), &bench_id);
    let mut reused = 0usize;
    if let Some(path) = flag("--reuse") {
        match reuse_prior(&path, &engine_fp, &bench_id) {
            Ok(map) => {
                reused = map.len();
                // The shards' own scores win: they were computed by THIS run.
                for (k, v) in map {
                    known.entry(k).or_insert(v);
                }
            }
            Err(why) => eprintln!("full rescore: {why}"),
        }
    }
    let emit_to = flag("--emit-scores");
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
        // MATCHED BY FAMILY, which today is a MIGRATION SHIM and nothing more.
        // Benchmarks carry no version (owner, 2026-08-04) — but records already
        // in the store were submitted against `single_target_v1`, and those are
        // builds like any other. Stripping the suffix is what lets them keep
        // competing under the id that replaced it, which is the same rule as
        // everywhere else here: a changed standard RE-SCORES rather than asking
        // anyone to resubmit: a row stays on the board and is overtaken by a
        // better one rather than being retired by a rule change.
        //
        // A different ruler entirely — `group_clear` — is a different family and
        // keeps its own board.
        if family(s.get("benchmark").and_then(Value::as_str).unwrap_or("")) != family(&bench_id) {
            continue;
        }
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
        // build (2026-08-05). A submission that is merely legal is refused
        // here and simply never scored.
        // THE REASON IS PRINTED, not counted. "2 refused" is a number that
        // tells nobody anything — including me, on the day two complete-looking
        // Dual Toxocyst builds were turned away and the log said only that they
        // were (2026-08-05). A board that refuses in silence cannot be debugged
        // by the person whose build it refused, either.
        // AN ADVERSARY WEAPON'S PROGENITOR ELEMENT is part of the submission,
        // like its mods and its evolutions — a different element is a different
        // build, not a weaker one. `builds::validate` refuses one the weapon
        // cannot have and refuses a MISSING one on a weapon that always has
        // one, so neither can arrive by omission — a legality rule rather than
        // a ruler's, since a build without an element is not a build a ruler
        // declines, it is not a build.
        let valence = s.get("valence").and_then(Value::as_str).unwrap_or("");
        let v = match wfsim_engine::builds::validate_for_board(
            &bench_id, &weapon, &mods, &evos, &arcs, valence,
        ) {
            Ok(v) => v,
            Err(e) => {
                // THE BUILD, not just the weapon. "refused burston_prime:
                // needs 64 of 60" says a build was turned away and leaves
                // "which one, and was it really impossible?" unanswerable —
                // which is the question asked of this log the first time
                // somebody's submission went missing (owner, 2026-08-14). The
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
        // which is exactly what the benchmarks used to pin as `form: default`.
        // So every row already on a board keeps its score to the last digit
        // while gaining the dimension.
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

        let mut req = scenario.clone();
        if let Some(o) = req.as_object_mut() {
            o.insert("weapon".into(), json!(v.weapon));
            o.insert("mods".into(), json!(v.mods));
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
        // because it now decides whether there is one to run. It used to be
        // computed after, since dedup was all it was for.
        //
        // The endpoint stores what was submitted, verbatim — it has no mod pool
        // and cannot tell an elemental mod from any other — so two spellings of
        // one fight arrive as two records and are collapsed HERE, where
        // `validate` has already put both into the same canonical form. The
        // MODE is part of that identity: one build played two ways is two
        // entrants, and collapsing them would keep whichever arrived first.
        let key = format!("{}#{}", wfsim_engine::builds::identity(&v), played.id);
        if !seen_ids.insert(key.clone()) {
            continue;
        }
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
                // on screen for a build that kills 11.05 a minute over 300 s
                // (user, 2026-08-04). Ranking is unaffected either way — this
                // is a linear rescale — but the number people read is not a
                // ranking.
                let s = match metric.as_str() {
                    "dps" => out.get("dps").and_then(Value::as_f64).unwrap_or(0.0),
                    _ => raw * 60.0 / duration,
                };
                computed.insert(key.clone(), s);
                s
            }
        };
        rows.push(Row {
            weapon: v.weapon,
            mode: played.id.to_string(),
            score,
            mods: v.mods,
            evolutions: v.evolutions,
            arcanes: v.arcanes,
            valence: v.valence,
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
        "{seen} submissions, {refused} refused, {} rows ({reused} reused, {} scored here, {below} below the floor){}",
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
            by_weapon.entry(r.weapon.clone()).or_default().push(json!({
                "benchmark": bench_id,
                "mode": r.mode,
                "source": "submissions",
                "score": r.score,
                // The number stays EXACT and the string beside it is what the
                // page prints. Formatting lives in `boards_data::format_score`,
                // so "four significant figures, four decimals" is one rule in
                // one language rather than a Rust copy and a JS copy that drift.
                "shown": wfsim_engine::boards_data::format_score(r.score),
                "mods": r.mods,
                "evolutions": r.evolutions,
                "arcanes": r.arcanes,
                "valence": r.valence,
            }));
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
    }
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
    /// sat at the top of the NO-AIM board, where that build scores 0.170
    /// (measured 2026-08-12, digit for digit on both boards).
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
}
