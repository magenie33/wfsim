// SPDX-License-Identifier: AGPL-3.0-or-later
//! What a run is asked to measure, which shard pays for each row, and the rows
//! nothing has asked for yet.

use serde_json::{json, Value};
use std::io::BufRead;

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

type QueueRows = std::collections::HashMap<String, Vec<(String, String)>>;
type QueueParts = (QueueRows, std::collections::BTreeSet<String>);

/// One queue snapshot, preserving its order within each ruler. An unspecified
/// input remains distinct from a supplied empty or unreadable file.
#[derive(Default)]
pub(crate) struct Queue {
    by_ruler: Option<QueueRows>,
    owed: std::collections::BTreeSet<String>,
    failed: Option<String>,
    pub(crate) rows: usize,
}

impl Queue {
    pub(crate) fn load(spec: Option<String>, only_rulers: Option<&[&str]>, track_owed: bool) -> Self {
        Self::read(spec, only_rulers, track_owed, false)
    }

    pub(crate) fn load_inventory(path: String, only_rulers: &[&str]) -> Self {
        Self::read(Some(path), Some(only_rulers), true, true)
    }

    fn read(spec: Option<String>, only_rulers: Option<&[&str]>, track_owed: bool, strict: bool) -> Self {
        let Some(path) = spec else { return Self::default() };
        let mut out = Self { by_ruler: Some(Default::default()), ..Self::default() };
        let Ok(file) = std::fs::File::open(&path) else {
            out.failed = Some(path);
            return out;
        };
        let included = only_rulers.map(|rs| rs.iter().copied().collect::<std::collections::HashSet<_>>());
        for line in std::io::BufReader::new(file).lines() {
            let Ok(line) = line else {
                return Self { by_ruler: Some(Default::default()), failed: Some(path), ..Self::default() };
            };
            if line.trim().is_empty() { continue; }
            let parsed = serde_json::from_str::<Value>(&line);
            if strict && !parsed.as_ref().is_ok_and(|v| ["build_id", "ruler", "mode"].iter()
                .all(|key| v.get(key).and_then(Value::as_str).is_some_and(|s| !s.is_empty())))
            {
                return Self { by_ruler: Some(Default::default()), failed: Some(path), ..Self::default() };
            }
            let Ok(v) = parsed else { continue };
            out.rows += 1;
            let Some(id) = v.get("build_id").and_then(Value::as_str) else { continue };
            // Any queued ruler protects an arrival's first fight, including a
            // row whose ruler is no longer published.
            if track_owed {
                out.owed.insert(id.to_string());
            }
            let Some(ruler) = v.get("ruler").and_then(Value::as_str) else { continue };
            if included.as_ref().is_some_and(|rs| !rs.contains(ruler)) {
                continue;
            }
            let mode = v.get("mode").and_then(Value::as_str).unwrap_or("base");
            out.by_ruler
                .as_mut()
                .unwrap()
                .entry(ruler.to_string())
                .or_default()
                .push((id.to_string(), mode.to_string()));
        }
        out
    }

    pub(crate) fn builds_owed(&self) -> &std::collections::BTreeSet<String> {
        &self.owed
    }

    pub(crate) fn into_parts(self) -> Result<QueueParts, String> {
        if let Some(path) = self.failed { return Err(format!("cannot read queue {path}")); }
        self.by_ruler.map(|rows| (rows, self.owed)).ok_or_else(|| "queue input required".to_string())
    }

    pub(crate) fn for_ruler(&self, bench_id: &str) -> Option<&[(String, String)]> {
        let by_ruler = self.by_ruler.as_ref()?;
        if let Some(path) = &self.failed {
            say!("queue: cannot read {path}");
            return Some(&[]);
        }
        let owed = by_ruler.get(bench_id).map(Vec::as_slice).unwrap_or_default();
        say!("queue: {} row(s) owed on {bench_id}", owed.len());
        Some(owed)
    }
}

/// THE ROWS OWED THAT NO BUILD CAN PAY: queued `(build_id, mode)` pairs the walk
/// never produced. Sorted, so a file is comparable run to run.
///
/// AN EMPTY WALK NAMES NOTHING. A library that read back empty would make every
/// row an orphan, and the purge would empty the queue on a bad read.
pub(crate) fn orphans(
    owed: Option<&std::collections::HashSet<(String, String)>>,
    payable: &std::collections::HashSet<(String, String)>,
    walked: usize,
) -> Vec<(String, String)> {
    if walked == 0 {
        return Vec::new();
    }
    let mut out: Vec<(String, String)> = owed
        .map(|o| o.iter().filter(|k| !payable.contains(*k)).cloned().collect())
        .unwrap_or_default();
    out.sort();
    out
}

/// …written one json per line in the queue's own shape, for
/// `scripts/purge_orphans.sh`, and counted out loud.
pub(crate) fn write_orphans(
    path: &str,
    owed: Option<&std::collections::HashSet<(String, String)>>,
    payable: &std::collections::HashSet<(String, String)>,
    walked: usize,
    bench_id: &str,
) {
    let rows = orphans(owed, payable, walked);
    let text: String = rows
        .iter()
        .map(|(b, m)| format!("{}\n", serde_json::json!({ "build_id": b, "ruler": bench_id, "mode": m })))
        .collect();
    if let Err(e) = std::fs::write(path, text) {
        say!("queue: cannot write {path}: {e}");
        return;
    }
    say!("queue: {} row(s) owed on {bench_id} that no build can pay", rows.len());
}

/// WHETHER A ROW IS OWED AGAIN: it has no fact, or its fact finished before the
/// refresh's instant (`--queue-older`). Facts are ISO-ms UTC, so the text order
/// is the time order.
pub(crate) fn owed_again(finished_at: Option<&str>, older: Option<&str>) -> bool {
    finished_at.is_none_or(|f| older.is_some_and(|t| f < t))
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
        {
            let scene = wfsim_engine::board::benchmarks::get(bench_id).and_then(|b| b.cell_dir());
            park_under_entry_line(&mut pending_missing, scene.as_deref(), cross, who, already_owed, corner_of).len()
        }
    } else {
        0
    };
    use std::io::Write;
    if let Some(out) = missing_out.as_mut() {
        for (id, mode) in &pending_missing {
            // ITS WEAPON TOO, so a scene's first pass can be paced by the
            // weapon's popularity (`scripts/scene_rows.py`); the queue keeps three.
            let weapon = who.get(id).map(|w| w.weapon.as_str());
            let line = json!({ "build_id": id, "ruler": bench_id, "mode": mode, "weapon": weapon });
            let _ = writeln!(out, "{line}");
            missing += 1;
        }
        let _ = out.flush();
    }
    say!("queue-missing: {missing} row(s) nothing has asked for on {bench_id}");
    if gate {
        say!(
            "entry line: {parked} build(s) under {:.0}% of every group's leader — no row asked for",
            wfsim_engine::data::boards::KEEP_LEADER_SHARE * 100.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_queue_snapshot_preserves_order_duplicates_and_global_owed_builds() {
        let path = std::env::temp_dir().join(format!("wfsim-queue-index-{}", std::process::id()));
        let text = concat!(
            "{\"build_id\":\"second\",\"ruler\":\"a\",\"mode\":\"alt\"}\n",
            "{\"build_id\":\"elsewhere\",\"ruler\":\"retired\"}\n",
            "invalid json\n",
            "{\"build_id\":\"first\",\"ruler\":\"a\"}\n",
            "{\"build_id\":\"second\",\"ruler\":\"a\",\"mode\":\"alt\"}\n",
            "{\"build_id\":\"no-ruler\"}\n",
            "{\"ruler\":\"a\",\"mode\":\"base\"}\n",
            "{\"build_id\":\"fallback\",\"ruler\":\"b\",\"mode\":null}\n",
        );
        std::fs::write(&path, text).unwrap();
        let queue = Queue::load(Some(path.to_string_lossy().into_owned()), None, true);
        std::fs::write(&path, "").unwrap();
        std::fs::remove_file(&path).unwrap();
        let legacy = |ruler: &str| {
            text.lines()
                .filter_map(|l| serde_json::from_str::<Value>(l).ok())
                .filter(|v| v.get("ruler").and_then(Value::as_str) == Some(ruler))
                .filter_map(|v| {
                    Some((
                        v.get("build_id")?.as_str()?.to_string(),
                        v.get("mode")
                            .and_then(Value::as_str)
                            .unwrap_or("base")
                            .to_string(),
                    ))
                })
                .collect::<Vec<_>>()
        };
        for ruler in ["a", "b", "retired", "empty"] {
            assert_eq!(queue.for_ruler(ruler), Some(legacy(ruler).as_slice()));
        }
        assert_eq!(queue.for_ruler("a").unwrap().len(), 3);
        assert_eq!(
            queue
                .builds_owed()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["elsewhere", "fallback", "first", "no-ruler", "second"]
        );
    }

    #[test]
    fn no_queue_supplied_is_distinct_from_an_empty_or_unreadable_input() {
        let unspecified = Queue::load(None, None, true);
        assert!(unspecified.for_ruler("a").is_none());
        let path =
            std::env::temp_dir().join(format!("wfsim-empty-queue-index-{}", std::process::id()));
        std::fs::write(&path, "").unwrap();
        let empty = Queue::load(Some(path.to_string_lossy().into_owned()), None, true);
        std::fs::remove_file(&path).unwrap();
        let unreadable = Queue::load(Some(path.to_string_lossy().into_owned()), None, true);
        assert_eq!(empty.for_ruler("a"), Some([].as_slice()));
        assert_eq!(unreadable.for_ruler("a"), Some([].as_slice()));
        assert!(unreadable.builds_owed().is_empty());
    }

    #[test]
    fn a_single_ruler_keeps_only_its_rows_and_the_gate_can_still_see_other_builds() {
        let path =
            std::env::temp_dir().join(format!("wfsim-one-queue-index-{}", std::process::id()));
        std::fs::write(
            &path,
            "{\"build_id\":\"first\",\"ruler\":\"a\"}\n{\"build_id\":\"other\",\"ruler\":\"b\"}\n",
        )
        .unwrap();
        let queue = Queue::load(Some(path.to_string_lossy().into_owned()), Some(&["a"]), true);
        let no_gate = Queue::load(Some(path.to_string_lossy().into_owned()), Some(&["a"]), false);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            queue.for_ruler("a").unwrap(),
            [("first".to_string(), "base".to_string())]
        );
        assert!(queue.for_ruler("b").unwrap().is_empty());
        assert!(queue.builds_owed().contains("other"));
        assert!(no_gate.builds_owed().is_empty());
    }

    #[test]
    fn a_stream_read_error_discards_the_whole_queue_snapshot() {
        let path = std::env::temp_dir().join(format!("wfsim-bad-queue-index-{}", std::process::id()));
        let mut bytes = b"{\"build_id\":\"first\",\"ruler\":\"a\"}\n".to_vec();
        bytes.push(255);
        std::fs::write(&path, bytes).unwrap();
        let queue = Queue::load(Some(path.to_string_lossy().into_owned()), None, true);
        std::fs::remove_file(&path).unwrap();
        assert!(queue.for_ruler("a").unwrap().is_empty());
        assert!(queue.builds_owed().is_empty());
        assert_eq!(queue.rows, 0);
    }

    /// **A ROW IS OWED AGAIN BY ITS OWN AGE.** No fact is owed; a fact is owed
    /// only once a refresh names an instant after it finished.
    #[test]
    fn a_row_is_owed_again_once_its_fact_is_older_than_the_refresh() {
        let t = Some("2026-10-08T00:00:00.000Z");
        assert!(owed_again(None, None));
        assert!(owed_again(None, t));
        assert!(!owed_again(Some("2026-10-01T00:00:00.000Z"), None));
        assert!(owed_again(Some("2026-10-07T23:59:59.999Z"), t));
        assert!(!owed_again(Some("2026-10-08T00:00:00.000Z"), t));
        assert!(!owed_again(Some("2026-10-10T12:00:00.000Z"), t));
    }

    /// **A ROW NO BUILD CAN PAY IS AN ORPHAN, AND ONLY THAT.** The case that
    /// found it: rows queued under a build id the identity rule has since moved
    /// — the build scored under its new id, the old row owed for ever.
    #[test]
    fn an_orphan_is_a_queued_row_the_walk_never_produced() {
        let k = |b: &str, m: &str| (b.to_string(), m.to_string());
        let owed: std::collections::HashSet<_> =
            [k("new", "base"), k("old", "base"), k("new", "lost_mode")].into_iter().collect();
        let payable: std::collections::HashSet<_> = [k("new", "base"), k("new", "cycle")].into_iter().collect();
        assert_eq!(orphans(Some(&owed), &payable, 10), vec![k("new", "lost_mode"), k("old", "base")]);
        // A walk over nothing names nothing: an empty read may not empty the queue.
        assert!(orphans(Some(&owed), &Default::default(), 0).is_empty());
        // No queue read, no orphans.
        assert!(orphans(None, &payable, 10).is_empty());
    }

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
                            measured_by: String::new(),
                            work: 0,
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
