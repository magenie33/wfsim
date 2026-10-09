// SPDX-License-Identifier: AGPL-3.0-or-later
//! One row's fight: the riven card it is measured with, and the runs paid for
//! in as many sittings as the clock allows.

use serde_json::Value;

use super::facts::identity_of;

/// THE CARD A BUILD NAMES — `board_rows::card_of`, the page's producer's too.
pub(crate) use wfsim_webapi::board_rows::card_of;

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
pub(crate) const CHUNK_RUNS: u32 = 1;

/// `webapi::simulate_json` under a name that says the crate boundary is
/// deliberate: the scorer runs the SAME entry point the web api runs, so the
/// board cannot drift from what the page computes.
pub(crate) fn wfsim_engine_webapi_simulate(v: &Value) -> Value {
    wfsim_webapi::simulate_json(v)
}

/// WHAT A ROW HAS PAID FOR SO FAR, when it ran out of clock partway.
///
/// A row is not always one measurement, and the clock has to be able to
/// interrupt it between the pieces rather than only before the last one.
///
/// ONE CURSOR, BECAUSE ONE SUB-MEASUREMENT IS IN FLIGHT AT A TIME: whatever the
/// clock interrupts is the only thing partway, and everything before it is a
/// number this run has already paid for.
#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct Partial {
    /// Sub-measurements this row has finished, by label — `measure` is the
    /// ruler's own, and a label is free to be added beside it.
    #[serde(default)]
    pub(crate) priced: std::collections::BTreeMap<String, f64>,
    /// The one that is partway: its label, how many runs are banked, and what
    /// they contributed.
    #[serde(default)]
    pub(crate) cursor: Option<(String, u32, Value)>,
}

/// Run `want` runs of `req`, resuming and pausing on the clock.
///
/// `None` means the budget ran out and `part` now carries the progress —
/// **including the runs this call paid for**, which is the whole point: a row
/// that cannot finish inside one run of the board still finishes, across as
/// many as it takes. Every run's dice are a pure function of `(seed, index)`,
/// so the pieces merge into exactly what one call over the range produces
/// (`wfsim_webapi::simulate_shard_json`, `fight::tests::formation_and_spread::eight_shards_are_one_run`).
///
/// THE FIRST CHUNK IS ONE RUN, and the rest are sized from what it cost. A
/// fixed chunk is wrong in both directions here: the rows this exists for are
/// seconds a run, and the median row is milliseconds — one chunk of a thousand
/// would blow any budget, and a thousand chunks of one would pay to build a
/// 361-body arena a thousand times.
///
/// Beside the report, the WORK the runs held (`Shard::work`) — what a client
/// filling the same row as a compute order is held to.
pub(crate) fn run_budgeted(
    part: &mut Partial,
    label: &str,
    req: &Value,
    want: u32,
    deadline: Option<std::time::Instant>,
) -> Option<(Value, u64)> {
    let mut acc = wfsim_engine::fight::Shard::default();
    let mut done = 0u32;
    if let Some((who, at, shard)) = part.cursor.take() {
        if who == label {
            if let Ok(s) = serde_json::from_value::<wfsim_engine::fight::Shard>(shard.clone()) {
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
        // THE FOLD IS `board_rows::fold_runs`, one run a piece — the page's
        // producer folds with the same function, so its number is this one.
        if wfsim_webapi::board_rows::fold_runs(req, &mut acc, done, CHUNK_RUNS).is_err() {
            // A shard that will not parse is not a slow row, it is a broken
            // one; the caller's `ok` check answers it the way it always did.
            return Some((wfsim_engine_webapi_simulate(req), 0));
        }
        done += CHUNK_RUNS;
        if done < want && deadline.is_some_and(|d| std::time::Instant::now() >= d) {
            let shard = serde_json::to_value(&acc).unwrap_or(Value::Null);
            part.cursor = Some((label.to_string(), done, shard));
            return None;
        }
    }
    Some((wfsim_webapi::board_rows::measured(req, &acc), acc.work()))
}

/// BANK WHERE THIS ROW STOPPED, and leave it for the next run.
///
/// A paused row is a FOURTH outcome beside listed, held and refused: the build
/// reached no row on this board and is not lost either. `deferred_ids` is what
/// tells the accounting below so, because a run that quietly dropped a build
/// would look exactly like one that ranked it.
pub(crate) fn pause_row(
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
    use super::*;
    use serde_json::json;

    /// A COMPUTE ORDER IS THE SCORER'S ROW, bit for bit.
    ///
    /// A client is handed the library's record and a (ruler, mode), asks
    /// `/api/board/order` for the fight, folds its runs through
    /// `/api/board/fold` — the accumulator carried between calls AS TEXT, the
    /// way the page holds it — and ends with `/api/board/score`. That has to
    /// name the scorer's fight and end on the scorer's number and metric. The
    /// run count is cut to keep a debug build quick; the claim is the path.
    #[test]
    fn a_compute_order_is_the_scorers_row() {
        let sent = json!({
            "weapon": "braton_prime",
            "mods": ["vital_sense", "hellfire", "speed_trigger", "primary_acuity",
                     "galvanized_aptitude", "hammer_shot", "vigilante_fervor", "magnetic_capacity"],
            "evolutions": ["braton_prime_evo1_incarnon_form", "braton_prime_daring_reverie",
                           "braton_prime_voids_guidance", "braton_prime_prelude_of_might"],
            "arcanes": ["primary_deadhead"],
        });
        let lib = wfsim_webapi::board_rows::library_build(&sent).expect("legal");
        let record = wfsim_webapi::board_rows::canonical_record(&lib);
        assert!(order_is_the_scorers(&record) >= 2, "an Incarnon weapon has a mode per form");
    }

    /// …AND A RIVEN CORNER'S IS TOO: the order carries the corner's ROLLS, so
    /// two corners of one shape are handed out as two different fights.
    #[test]
    fn a_riven_corners_order_is_the_scorers_row_with_its_rolls() {
        let corner = |rolls: [f64; 3]| json!({
            "weapon": "furis",
            "mods": ["primed_convulsion", "pathogen_rounds", "galvanized_diffusion", "primed_target_cracker",
                     "galvanized_shot", "gunslinger", "magnetic_might", "riven"],
            "evolutions": ["furis_evo1_incarnon_form", "furis_stormburst", "furis_extended_volley", "furis_headcracker"],
            "arcanes": ["secondary_enervate"],
            "exilus": "eject_magazine",
            "riven_pos": ["damage", "multishot"],
            "riven_neg": "weapon_recoil",
            "riven_rolls": rolls,
        });
        let god = corner([1.1, 1.1, 0.9]);
        assert!(order_is_the_scorers(&god) >= 1);
        let order = |rec: &Value| wfsim_webapi::board_rows::board_order_json(&json!({
            "record": rec, "ruler": "standard_single_target", "mode": "base",
        }))["request"].clone();
        assert_ne!(order(&god), order(&corner([0.9, 1.1, 0.9])), "the rolls never reached the fight");
    }

    /// The order for every mode `record` owes under one ruler names the
    /// scorer's fight and ends on its number; how many modes it checked.
    fn order_is_the_scorers(record: &Value) -> usize {
        let ruler = "standard_single_target";
        let bench = wfsim_engine::board::benchmarks::get(ruler).expect("ruler");
        let v = wfsim_webapi::board_rows::scored_build(record, ruler).expect("complete");
        let scenario = serde_json::to_value(&bench.scenario).unwrap();
        let scorer = wfsim_webapi::board_rows::row_requests(&v, &scenario);
        let modes = scorer.len();
        for (played, want) in scorer {
            let mode = if played.id.is_empty() { "base" } else { played.id };
            let order = wfsim_webapi::board_rows::board_order_json(&json!({
                "record": record, "ruler": ruler, "mode": mode,
            }));
            assert_eq!(order["request"], want, "the order names a different {mode} fight");

            let mut req = want.clone();
            req["runs"] = json!(9);
            let (banked, banked_work) = run_budgeted(&mut Partial::default(), "measure", &req, 9, None).unwrap();
            let mut acc = Value::Null;
            for from in (0..9).step_by(4) {
                let step = wfsim_webapi::board_rows::board_fold_json(&json!({
                    "request": req, "acc": acc, "from": from, "count": (9 - from).min(4),
                }));
                acc = serde_json::from_str(&step["acc"].to_string()).unwrap();
            }
            // …AND SO IS THE SAME ROW FOUGHT ON MANY LANES: its runs fought out
            // of order as unmerged shards, then folded as `pieces` in run order.
            let mut shards: Vec<Value> = Vec::new();
            for from in [6u32, 3, 0] {
                let got = wfsim_webapi::board_rows::board_runs_json(&json!({
                    "request": req, "from": from, "count": 3,
                }));
                let mut got: Vec<Value> = serde_json::from_str(&got["shards"].to_string()).unwrap();
                got.extend(shards);
                shards = got;
            }
            let mut lanes_acc = Value::Null;
            for chunk in shards.chunks(4) {
                let step = wfsim_webapi::board_rows::board_fold_json(&json!({
                    "request": req, "acc": lanes_acc, "pieces": chunk,
                }));
                lanes_acc = serde_json::from_str(&step["acc"].to_string()).unwrap();
            }
            assert_eq!(lanes_acc, acc, "the {mode} runs folded from lanes are not the runs folded in one");
            let scored = wfsim_webapi::board_rows::board_score_json(&json!({
                "ruler": ruler, "request": req, "acc": acc,
            }));
            assert_eq!(
                scored["score"].as_f64().unwrap().to_bits(),
                wfsim_webapi::board_rows::row_score(bench, &banked).to_bits(),
                "the order's {mode} number is not the scorer's",
            );
            assert_eq!(scored["metric"], json!(bench.metric().id), "the order's metric is not the ruler's");
            // …AND ITS WORK IS THE SCORER'S, however the page cut the pieces:
            // it is what a client is credited (docs/BOARD.md §"Contribution").
            assert!(scored["work"].as_u64().is_some_and(|w| w > 0), "the order's {mode} fight counted no work");
            assert_eq!(scored["work"], json!(banked_work), "the order's {mode} work is not the scorer's");
        }
        modes
    }

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
    /// `fight::tests::formation_and_spread::eight_shards_are_one_run` uses one.
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
        let (unbounded, unbounded_work) = run_budgeted(&mut whole, "measure", &req, 40, None)
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
            serde_json::to_string(&resumed.0).unwrap(),
            serde_json::to_string(&one).unwrap(),
            "a resumed row answered differently from an uninterrupted one",
        );
        assert_eq!(resumed.1, unbounded_work, "a resumed row counted other work than an uninterrupted one");
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
}
