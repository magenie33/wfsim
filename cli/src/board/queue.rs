// SPDX-License-Identifier: AGPL-3.0-or-later
//! What a run is asked to measure, which shard pays for each row, and the rows
//! nothing has asked for yet.

use serde_json::{json, Value};

use super::entry::{park_under_entry_line, Who};
use super::facts::CrossFact;

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
pub(crate) const DEFAULT_ROW_SECONDS: f64 = 3.6;

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
