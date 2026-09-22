// SPDX-License-Identifier: AGPL-3.0-or-later
//! One pass of the scorer: its flags, the walk over the library, and the
//! phases that publish what the walk measured.

use std::io::Read;

use serde_json::Value;

use crate::args::{flag, has_flag};

use super::entry::{account, apply_entry_line, one_row_per_shape, Who};
use super::facts::{identity_of, load_cross_facts, load_facts, stamp, Fact, FactLog};
use super::measure::{card_of, pause_row, run_budgeted, Partial};
use super::publish::{write_pages, Row, RowRiven};
use super::queue::{
    builds_with_a_row_owed, charge, load_queue, write_missing, DEFAULT_ROW_SECONDS,
};
use super::state::record_state;

pub fn run() {
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
    let missing_out = flag("--queue-missing").and_then(|p| {
        std::fs::File::create(&p)
            .map_err(|e| eprintln!("queue: cannot write {p}: {e}"))
            .ok()
            .map(std::io::BufWriter::new)
    });
    // …AND WHETHER THE ENTRY LINE GETS A SAY IN WHAT IS ASKED FOR.
    //
    // OFF BY DEFAULT, AND THE CALLER TURNS IT ON, because which paths it
    // governs is a decision worth reading in the workflow rather than inferring
    // from the code: the hourly reconciliation and the nightly sweep pass it,
    // and the RESCORE button does not. A model correction is exactly the case
    // where a parked build may deserve another look, and a person asking is the
    // one thing this pipeline lets override a fact.
    //
    // THE SHARE ITSELF IS NOT A FLAG. It is `KEEP_LEADER_SHARE`, and a number
    // on a command line would be a second copy of it.
    let gate = has_flag("--gate");
    let cross = if gate { load_cross_facts(flag("--facts-in")) } else { Vec::new() };
    // …AND WHICH BUILDS SOMEBODY HAS ALREADY ASKED ABOUT, on any ruler. A build
    // with no fact is owed one fight; this is how the run knows whether that
    // fight has already been asked for.
    let already_owed =
        if gate { builds_with_a_row_owed(flag("--queue-in")) } else { Default::default() };
    // WHICH GROUP EACH BUILD'S FACTS FALL IN, filled as the run walks the
    // library. A build illegal under THIS ruler is missing from it, which can
    // only understate another group's leader and so only ever asks for a row
    // that need not have been asked for.
    let mut who: std::collections::HashMap<String, Who> = Default::default();
    // …AND THE ROWS NOTHING HAS ASKED FOR, held until the line can be applied.
    // The gate needs every group's leader, which is known only once the whole
    // library has been walked, so the file is written at the end rather than
    // row by row.
    let mut pending_missing: Vec<(String, String)> = Vec::new();
    // …AND, FOR EACH ALTERNATIVE CORNER OF A RIVEN, THE DEFAULT IT WAITS ON.
    let mut corner_of: std::collections::HashMap<String, String> = Default::default();
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

    let bench = wfsim_engine::board::benchmarks::get(&bench_id).unwrap_or_else(|| {
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
    // here. A ruler names exactly one core and `board::benchmarks` is where that
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
        // build, not a weaker one. `board::builds::validate` refuses one the weapon
        // cannot have and refuses a MISSING one on a weapon that always has
        // one, so neither can arrive by omission — a legality rule rather than
        // a ruler's, since a build without an element is not a build a ruler
        // declines, it is not a build.
        let valence = s.get("valence").and_then(Value::as_str).unwrap_or("");
        // A RIVEN'S SHAPE, when the submission carries one. Two flat lists, the
        // way the endpoint stores them: the ROLLS are never submitted because
        // they are one person's luck — `wfsim-intake` stores the shape's
        // corners and the board ranks them.
        let shape = {
            let bonuses = get("riven_pos");
            let malus = s
                .get("riven_neg")
                .and_then(Value::as_str)
                .filter(|x| !x.is_empty());
            (!bonuses.is_empty()).then(|| wfsim_engine::build::rivens::RivenShape {
                bonuses: {
                    let mut b = bonuses;
                    b.sort();
                    b
                },
                malus: malus.map(String::from),
            })
        };
        // THE EXILUS SLOT'S MOD. Optional as of 2026-08-25 — see
        // `board::benchmarks::BuildRequirement::allows_exilus` — and its own
        // field on the wire because a flat `mods` list cannot say which entry
        // came out of the exilus slot.
        let exilus = s
            .get("exilus")
            .and_then(Value::as_str)
            .filter(|x| !x.is_empty());
        // THE WARFRAME HOLDING IT, and only an Exalted row carries one — its
        // numbers are that frame's ability's, so the record states it where
        // every other row is scored in the ruler's frameless hands.
        let wielder: Option<wfsim_engine::data::warframes::Build> =
            s.get("wielder").and_then(|x| serde_json::from_value(x.clone()).ok());
        // THE PARTS, flat, exactly as the worker stores them and as the page's
        // own door reads them (`webapi::kitgun::board_assembly_of`). The chamber is the
        // weapon's, never the record's.
        let asm = {
            let g = s.get("grip").and_then(Value::as_str).unwrap_or("");
            let l = s.get("loader").and_then(Value::as_str).unwrap_or("");
            (!(g.is_empty() && l.is_empty())).then(|| wfsim_engine::data::weapons::kitguns::Assembly {
                // The chamber's WEAPON id, which is what `Assembly` holds.
                chamber: wfsim_engine::data::weapons::spec(&weapon)
                    .and_then(|sp| sp.kitgun.clone())
                    .and_then(|r| wfsim_engine::data::weapons::kitguns::default_assembly(&r))
                    .map(|d| d.chamber)
                    .unwrap_or_default(),
                grip: g.to_string(),
                loader: l.to_string(),
            })
        };
        let v = match wfsim_engine::board::builds::validate_for_board_with(
            &bench_id,
            &weapon,
            &mods,
            &evos,
            &arcs,
            valence,
            shape.as_ref(),
            exilus,
            asm.as_ref(),
            // THE WARFRAME THE RECORD CARRIES, and it carries one only where a
            // ruler cannot pin it. Absent on every ordinary row, which is why
            // the door drops it there rather than asking for it.
            wielder.as_ref(),
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
        let ident = wfsim_engine::board::builds::build_id(&v);
        scored_ids.insert(ident.clone());
        // …AND WHAT THE ENTRY LINE WILL NEED TO SAY ABOUT IT. Recorded here,
        // beside the identity, because both are facts about the BUILD and
        // neither is a property of the mode the loop below enumerates.
        who.entry(ident).or_insert_with(|| Who {
            weapon: v.weapon.clone(),
            riven: shape.is_some(),
        });
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
        let modes: Vec<wfsim_engine::data::weapons::WeaponPlayMode> =
            wfsim_engine::data::weapons::play_modes(&v.weapon)
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
            let key = wfsim_engine::board::builds::board_key(&v, played.id);
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
                wfsim_engine::board::builds::build_id(&v),
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
                // HELD, NOT WRITTEN. The entry line needs every group's leader
                // and the last of those is known only when the library has
                // been walked; the file is written from this list below.
                //
                // …AND WHICH BUILD THIS CORNER IS AN ALTERNATIVE TO, when it is
                // one. A riven's shape is several builds and only the DEFAULT
                // is asked for on arrival; the line decides whether the rest
                // are ever worth a fight (`board::entry`).
                if let Some(def) = wfsim_engine::board::builds::default_corner(&v) {
                    corner_of.insert(asked.0.clone(), def);
                }
                pending_missing.push(asked.clone());
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
                            wfsim_engine::build::rivens::class_for_weapon(&v.weapon).unwrap_or("");
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
                identity: wfsim_engine::board::builds::build_id(&v),
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

    // EVERY SCORED ROW THAT CLEARS THE ENTRY LINE IS PUBLISHED, and HOW DEEP TO
    // READ IS THE READER'S QUESTION: the page shows builds within half their
    // group's leader by default and will widen to everything the board kept
    // (docs/BOARD.md). Holding back a row a depth would have shown made it
    // indistinguishable from a build that was lost; holding back one no depth
    // would have shown costs a reader nothing and costs a flood its payload.
    let mut kept = rows;
    kept.sort_by(|a, b| {
        a.weapon.cmp(&b.weapon).then(
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });

    // …AND ONE ROW PER RIVEN SHAPE, before the line is applied: the corners of
    // one shape are one piece of advice, and a group's leader is the same
    // number whichever of them carried it.
    one_row_per_shape(&mut kept);

    let floored_ids = apply_entry_line(&mut kept, shards);

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
    // being trusted. Four outcomes and no fifth: refused at the door,
    // published, deferred to the next run by a budget, or under the entry line.
    // A build that fell out of all four
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

    // WHAT NOTHING HAS ASKED FOR YET, written and flushed before anything reads
    // the file. The reconciliation is the reason a hand-written queue cannot
    // quietly lose a row, so the count is said out loud on every run: a number
    // that is not zero after the first pass is a writer that is forgetting to
    // enqueue.
    //
    // AND THE ENTRY LINE IS ASKED HERE, WHERE A ROW COSTS A FIGHT rather than
    // where it costs a line in a file. A build that reaches a tenth of some
    // group's leader keeps earning every row it is owed; one that reaches it
    // nowhere keeps the facts it has and stops being asked for more.
    if missing_out.is_some() {
        write_missing(
            missing_out, pending_missing, gate, &cross, &who, &already_owed, &corner_of, &bench_id,
        );
    }
    if dry {
        eprintln!(
            "dry-run: todo={todo} work={work_seconds:.0} reused={reused} seen={seen}"
        );
        return;
    }

    account(&kept, &scored_ids, &deferred_ids, &floored_ids, shards, refused);

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
        write_pages(std::path::Path::new(&dir), &bench_id, &kept);
    }

    // WHAT THE RUNTIME NEEDS, and only that.
    //
    // The rows are not embedded — `site/board/` is outside `data/` for exactly
    // that reason — so the page's few scalars per board come from a small
    // generated file that is. Merged rather than overwritten: this binary runs
    // once per benchmark.
    record_state(&bench_id, seen, kept.len());
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    /// THE SCORER ASKS FOR A MODE, AND TWO MODES ARE TWO QUESTIONS. A weapon
    /// that can fill its gauge either way sent one request for both cycles, so
    /// the two modes scored to the last digit.
    ///
    /// DERIVED, NOT LISTED: every weapon, every pair of its modes, and two
    /// sharing a form must still differ.
    #[test]
    fn two_modes_sharing_one_form_are_two_requests() {
        let scenario = json!({ "enemy": "thrax_centurion", "level": 9999 });
        let build = |id: &str| wfsim_engine::board::builds::ValidBuild {
            wielder: None,
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
        for w in wfsim_engine::data::weapons::roster() {
            let modes = wfsim_engine::data::weapons::play_modes(&w.id);
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
        for w in wfsim_engine::data::weapons::roster() {
            let v = wfsim_engine::board::builds::ValidBuild {
            wielder: None,
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
            let modes: Vec<_> = wfsim_engine::data::weapons::play_modes(&w.id)
                .into_iter()
                .filter(|m| m.sustainable)
                .collect();
            if modes.len() > 1 {
                multi += 1;
            }
            let mut keys = std::collections::HashSet::new();
            for m in &modes {
                assert!(
                    keys.insert(wfsim_engine::board::builds::board_key(&v, m.id)),
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
        let v = wfsim_engine::board::builds::ValidBuild {
            wielder: None,
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
        let modes = wfsim_engine::data::weapons::play_modes("ballistica_prime");
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
}
