// SPDX-License-Identifier: AGPL-3.0-or-later
//! One row's fight: the riven card it is measured with, and the runs paid for
//! in as many sittings as the clock allows.

use serde_json::Value;

use super::facts::identity_of;

/// THE CARD A BUILD NAMES — its own rolls, or the god roll if it names none.
///
/// ONE READER, so a row's card cannot depend on whether the row was fought or
/// reused. The fact carried a copy while a record could state only a shape;
/// nothing made the two agree, and two copies of one truth is a rule about
/// which wins waiting to be needed.
///
/// A RECORD NAMING NO ROLLS is what the library held before `wfsim-intake`
/// resolved them, and the god roll is what it would resolve to today.
pub(crate) fn card_of(
    v: &wfsim_engine::board::builds::ValidBuild,
    shape: &wfsim_engine::build::rivens::RivenShape,
) -> Vec<f64> {
    if !v.riven_rolls.is_empty() {
        return v.riven_rolls.clone();
    }
    let cls = wfsim_engine::build::rivens::class_for_weapon(&v.weapon).unwrap_or("");
    let g = wfsim_engine::build::rivens::god_roll(shape, cls);
    g.bonuses.iter().map(|b| b.roll).chain(g.malus.iter().map(|m| m.roll)).collect()
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
pub(crate) fn run_budgeted(
    part: &mut Partial,
    label: &str,
    req: &Value,
    want: u32,
    deadline: Option<std::time::Instant>,
) -> Option<Value> {
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
        let piece = wfsim_webapi::simulate_shard_json(req, done, CHUNK_RUNS, &mut |_, _| {});
        let Ok(s) = serde_json::from_value::<wfsim_engine::fight::Shard>(piece) else {
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
}
