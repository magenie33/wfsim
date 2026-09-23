// SPDX-License-Identifier: AGPL-3.0-or-later
//! What a run is asked to measure, which shard pays for each row, and the rows
//! nothing has asked for yet.

use serde_json::{json, Value};

use super::entry::{park_under_entry_line, Who};
use super::facts::CrossFact;

/// WHAT A ROW COSTS WHEN NOTHING HAS MEASURED IT — THIS RULER'S OWN MEAN.
///
/// PRICED FROM THE RULER AND NOT FROM THE BOARD. A ruler is its own
/// environment and its rows cost what its fight costs: on one published board
/// the mean row ran 5.8 s under `demolisher` against 25.3 s under
/// `standard_multi_target`. One figure across all of them prices the expensive
/// ruler at a quarter, so a backlog that tilts onto it is provisioned a quarter
/// of the shards it needs and every one of them runs out of clock with the
/// queue still full.
///
/// THE MEAN AND NOT THE MEDIAN, because what is being sized is a SUM. These
/// distributions are right-skewed — a 3.8 s median against a 6.1 s mean — so
/// summing medians under-counts a slice by a third before any ruler is
/// mispriced at all.
pub(crate) fn unmeasured_row_seconds(facts: &super::facts::Facts) -> f64 {
    let paid: Vec<f64> = facts
        .values()
        .map(|f| f.cost_seconds)
        .filter(|c| *c > 0.0)
        .collect();
    if paid.is_empty() {
        return NO_HISTORY_ROW_SECONDS;
    }
    paid.iter().sum::<f64>() / paid.len() as f64
}

/// WHAT A RULER NOBODY HAS RUN CHARGES — the mean row across every ruler that
/// HAS been run: 10.07 s over 70,840 of them on a published board.
///
/// IT IS CHARGED TO A RULER'S FIRST RUN AND NEVER AGAIN, because that run
/// measures every row it takes and the next one prices from those. So what it
/// has to be is the best figure available with nothing measured, which is the
/// board's own mean and not a round number chosen to look like one.
const NO_HISTORY_ROW_SECONDS: f64 = 10.07;

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
pub(crate) fn charge(load: &mut [f64], cost: f64) -> usize {
    let mine = load
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map_or(0, |(i, _)| i);
    load[mine] += cost;
    mine
}

/// EVERY BUILD WITH A ROW OWED ON ANY RULER — the queue read whole, for the one
/// question `load_queue`'s per-ruler view cannot answer.
///
/// A build nothing has measured is owed ONE fight, and the row intake asked for
/// is it. Asked of this ruler alone, a build whose arrival named ANOTHER board
/// would look like a build nobody has asked about, and the reconciliation would
/// hand it every row it could have.
pub(crate) fn builds_with_a_row_owed(spec: Option<String>) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    let Some(path) = spec else { return out };
    let Ok(text) = std::fs::read_to_string(&path) else { return out };
    for line in text.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if let Some(id) = v.get("build_id").and_then(Value::as_str) {
            out.insert(id.to_string());
        }
    }
    out
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
pub(crate) fn load_queue(spec: Option<String>, bench_id: &str) -> Option<Vec<(String, String)>> {
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

/// WHAT NOTHING HAS ASKED FOR YET, written and flushed before anything reads
/// the file. The reconciliation is the reason a hand-written queue cannot
/// quietly lose a row, so the count is said out loud on every run: a number
/// that is not zero after the first pass is a writer that is forgetting to
/// enqueue.
///
/// AND THE ENTRY LINE IS ASKED HERE, WHERE A ROW COSTS A FIGHT rather than
/// where it costs a line in a file. A build that reaches a tenth of some
/// group's leader keeps earning every row it is owed; one that reaches it
/// nowhere keeps the facts it has and stops being asked for more.
#[allow(clippy::too_many_arguments)]
pub(crate) fn write_missing(
    mut missing_out: Option<std::io::BufWriter<std::fs::File>>,
    mut pending_missing: Vec<(String, String)>,
    gate: bool,
    cross: &[CrossFact],
    who: &std::collections::HashMap<String, Who>,
    already_owed: &std::collections::BTreeSet<String>,
    corner_of: &std::collections::HashMap<String, String>,
    bench_id: &str,
) {
    let mut missing = 0usize;
    let parked = if gate {
        park_under_entry_line(&mut pending_missing, cross, who, already_owed, corner_of).len()
    } else {
        0
    };
    use std::io::Write;
    if let Some(out) = missing_out.as_mut() {
        for (id, mode) in &pending_missing {
            let line = json!({ "build_id": id, "ruler": bench_id, "mode": mode });
            let _ = writeln!(out, "{line}");
            missing += 1;
        }
        let _ = out.flush();
    }
    eprintln!("queue-missing: {missing} row(s) nothing has asked for on {bench_id}");
    if gate {
        eprintln!(
            "entry line: {parked} build(s) under {:.0}% of every group's leader — no row asked for",
            wfsim_engine::data::boards::KEEP_LEADER_SHARE * 100.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **AN UNMEASURED ROW IS PRICED BY ITS OWN RULER.** A figure carried over
    /// from a cheaper ruler under-charges the whole backlog, and the run is
    /// sized from that total: the split then provisions a fraction of the
    /// shards the work needs and every one of them runs out of clock.
    ///
    /// THE NUMBERS ARE A PUBLISHED BOARD'S, one ruler each — a `demolisher`
    /// row against a `standard_multi_target` row. What the test denies is any
    /// price between them, which is what one figure for both must produce.
    #[test]
    fn an_unmeasured_row_is_priced_by_its_own_ruler() {
        let facts = |costs: &[f64]| -> super::super::facts::Facts {
            costs
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    (
                        format!("row{i}"),
                        super::super::facts::Fact {
                            score: 1.0,
                            cost_seconds: *c,
                            started_at: String::new(),
                            finished_at: String::new(),
                        },
                    )
                })
                .collect()
        };

        let cheap = unmeasured_row_seconds(&facts(&[4.0, 6.0, 8.0, 5.0]));
        let dear = unmeasured_row_seconds(&facts(&[10.0, 25.0, 60.0, 6.0]));
        assert!((cheap - 5.75).abs() < 1e-9, "{cheap}");
        assert!((dear - 25.25).abs() < 1e-9, "{dear}");

        // A ruler nobody has run is the ONE case with no measurement to read,
        // and it must not silently borrow the other ruler's.
        assert_eq!(
            unmeasured_row_seconds(&Default::default()),
            NO_HISTORY_ROW_SECONDS
        );

        // A ROW BANKED WITH NO COST IS NOT A FREE ROW. Every board written
        // before costs were recorded holds zeroes, and averaging them in
        // prices the backlog at a fraction of what it takes.
        assert!((unmeasured_row_seconds(&facts(&[0.0, 0.0, 4.0, 6.0])) - 5.0).abs() < 1e-9);
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
