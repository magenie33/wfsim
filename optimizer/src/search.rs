// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE WALK — every subset of a scope, on request.
//!
//! The page's search is [`crate::descent`] on every scope. This answers a
//! tool that asks for the whole space (`"strategy": "exhaust"`): the walk
//! visits every subset exactly once, so its winner is the optimum of
//! everything pooled.
//!
//! It walks [`Shuffle`], a pseudorandom bijection on the subset space's index
//! range, rather than a depth-first descent: a walk cut short by the clock
//! then leaves a uniform sample instead of a lexicographic corner, and
//! [`SearchStats::coverage`] says how much of the space it reached.
//!
//! A 1-RUN SCREEN IS ALLOWED TO STEER, measured rather than assumed: over a
//! 64,796-job scope, ranking every job on ONE Monte-Carlo run and keeping the
//! top sixth drops **0 of the true top 100**, because kill progress over a
//! 300 s engagement is a low-variance statistic.

use std::sync::atomic::Ordering;

use crate::space::{Shuffle, SubsetSpace};
use crate::{evaluate, job_seed, Candidate, FunnelState, Scenario, ScreenedJob, Scored};

/// What the caller turns one subset into: every candidate that subset can
/// produce — element orders, exilus options, evolution sets — resolved and
/// legalized. Those axes stay EXHAUSTIVE inside a subset: there are at most a
/// couple of dozen of them, they are cheap to enumerate, and handing an exact
/// subproblem to a stochastic search is how you lose an answer for no reason.
pub type Expand<'a> = dyn Fn(&[usize]) -> Vec<Candidate> + Sync + 'a;

#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// Screen evaluations the search may spend. 0 = no cap (the host's clock,
    /// via `FunnelState`, is then the only bound).
    pub max_evals: u64,
    /// How many screened jobs survive into the funnel.
    pub keep: usize,
    pub seed: u64,
    /// Monte-Carlo runs per screen evaluation. 1 — see the module note.
    pub runs: u32,
    /// WHICH SHARD this run is, of `shards` total. The browser is
    /// single-threaded, so it buys compute with several Web Workers: the walk
    /// gives each one stride of the shuffled order (`shard`, `shard + shards`,
    /// …), disjoint and together the whole space; the descent gives each its
    /// share of the starts.
    pub shard: u32,
    pub shards: u32,
    /// The DESCENT's widest move: how many positions it may change at once
    /// once single changes stop paying. 1 = single changes only.
    pub swap_width: u32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        SearchConfig {
            max_evals: 0,
            keep: 65_536,
            seed: 0xDEAD_BEEF,
            runs: 1,
            shard: 0,
            shards: 1,
            swap_width: 1,
        }
    }
}

/// What a run actually did — the honest report a search owes.
#[derive(Debug, Clone, Copy, Default)]
pub struct SearchStats {
    /// Indices the space holds.
    pub space: u128,
    /// Indices the walk consumed from the shuffled order.
    pub sampled: u128,
    /// Subsets actually scored.
    pub subsets: u64,
    /// Candidates expanded from those subsets.
    pub candidates: u64,
    /// Screen evaluations spent.
    pub evals: u64,
    /// Did the walk reach the end of the space? Then its winner is THE winner.
    pub exhaustive: bool,
    /// Starts a descent ran from; 0 = this was the walk.
    pub starts: u32,
    /// A descent the budget stopped before every start reached a fixed point.
    pub cut: bool,
}

impl SearchStats {
    /// Share of the space the walk covered, in `0..=1`. Exact, because the
    /// denominator is a counted index range rather than an estimate.
    pub fn coverage(&self) -> f64 {
        if self.space == 0 {
            return 1.0;
        }
        (self.sampled as f64 / self.space as f64).min(1.0)
    }
}

/// One proposal awaiting evaluation.
struct Proposal {
    subset: Vec<usize>,
    /// Global sequence number — the seed source, so a job's random stream
    /// depends on nothing but the walk's own deterministic order.
    seq: usize,
}

pub(crate) fn key_of(subset: &[usize]) -> u64 {
    let mut v = subset.to_vec();
    v.sort_unstable();
    // FNV-1a over the sorted members: a subset is a SET, so its identity must
    // not depend on the order the proposal happened to build it in.
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for x in v {
        for b in (x as u64).to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100_0000_01b3);
        }
    }
    h
}

/// Walk this shard's stride of the space. Returns the screened survivors
/// (best first) and what the walk covered.
pub fn search(
    space: &SubsetSpace,
    expand: &Expand<'_>,
    arcanes: &[wfsim_engine::data::arcanes::ArcaneFx],
    scenario: &Scenario,
    cfg: &SearchConfig,
    state: Option<&FunnelState>,
    on_board: Option<&crate::ScreenBoardFn<'_>>,
) -> (Vec<ScreenedJob>, SearchStats) {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let mut stats = SearchStats { space: space.len(), ..Default::default() };
    let mut top: BinaryHeap<Reverse<Scored>> = BinaryHeap::new();
    let shuffle = Shuffle::new(space.len(), cfg.seed);
    let mut seq = 0usize;
    // How many of THIS SHARD's positions have been consumed; the position
    // itself is `shard + k * shards`.
    let mut k: u128 = 0;
    let shards = u128::from(cfg.shards.max(1));
    let shard = u128::from(cfg.shard) % shards;
    let taken = |k: u128| shard + k * shards;
    let stopped = |st: Option<&FunnelState>| {
        st.is_some_and(|s| {
            s.cancel.load(Ordering::Relaxed) || s.stop_enumeration.load(Ordering::Relaxed)
        })
    };
    let width = crate::batch_width();

    while taken(k) < space.len() {
        if stopped(state) || (cfg.max_evals > 0 && stats.evals >= cfg.max_evals) {
            break;
        }
        // A batch must not overrun the budget: batches are wide (4 per worker)
        // so every core stays fed, and a subset costs several evaluations (its
        // element orders, exilus options and evolution sets) — so the batch is
        // trimmed to what is left, at the rate this run has actually paid.
        let batch_size = if cfg.max_evals > 0 {
            let per_subset = if stats.subsets > 0 {
                (stats.evals as f64 / stats.subsets as f64).max(1.0)
            } else {
                1.0
            };
            let left = (cfg.max_evals - stats.evals) as f64;
            ((left / per_subset).ceil() as usize).clamp(1, width)
        } else {
            width
        };
        let mut batch: Vec<Proposal> = Vec::with_capacity(batch_size);
        let mut buf = Vec::new();
        while batch.len() < batch_size && taken(k) < space.len() {
            let i = shuffle.at(taken(k));
            k += 1;
            stats.sampled += 1;
            if space.nth(i, &mut buf) {
                batch.push(Proposal { subset: buf.clone(), seq });
                seq += 1;
            }
        }
        for (pseq, n_cands, results) in
            evaluate_proposals(&batch, expand, arcanes, scenario, cfg, state)
        {
            stats.subsets += 1;
            stats.candidates += n_cands as u64;
            stats.evals += results.len() as u64;
            for (cand, ai, summary) in results {
                push_elite(
                    &mut top,
                    Scored {
                        kp: summary.mean_kill_progress.max(0.0),
                        eff: summary.mean_effective_damage,
                        seq: pseq,
                        ai,
                        cand,
                        summary,
                    },
                    cfg.keep,
                );
            }
        }
        if let Some(st) = state {
            st.enumerated.store(stats.subsets, Ordering::Relaxed);
            st.sims_done.store(stats.evals, Ordering::Relaxed);
        }
        if let Some(b) = on_board {
            b(&snapshot(&top, crate::BOARD_TOP));
        }
    }

    // This shard walked every position it owns. With one shard that is the
    // whole space; with N it is one stride of it, and the caller ANDs them.
    stats.exhaustive = taken(k) >= space.len();
    let mut out: Vec<Scored> = top.into_iter().map(|r| r.0).collect();
    out.sort_by(|a, b| b.cmp(a));
    (
        out.into_iter()
            .map(|s| ScreenedJob { cand: s.cand, ai: s.ai, summary: s.summary })
            .collect(),
        stats,
    )
}

pub(crate) fn push_elite(
    top: &mut std::collections::BinaryHeap<std::cmp::Reverse<Scored>>,
    item: Scored,
    keep: usize,
) {
    use std::cmp::Reverse;
    if top.len() < keep {
        top.push(Reverse(item));
    } else if top.peek().is_some_and(|Reverse(min)| item > *min) {
        top.pop();
        top.push(Reverse(item));
    }
}

pub(crate) fn snapshot(
    top: &std::collections::BinaryHeap<std::cmp::Reverse<Scored>>,
    n: usize,
) -> Vec<ScreenedJob> {
    let mut v: Vec<&Scored> = top.iter().map(|r| &r.0).collect();
    v.sort_by(|a, b| b.cmp(a));
    v.truncate(n);
    v.into_iter()
        .map(|s| ScreenedJob { cand: s.cand.clone(), ai: s.ai, summary: s.summary })
        .collect()
}

type Evaluated = (
    usize,
    usize,
    Vec<(std::sync::Arc<Candidate>, usize, wfsim_engine::fight::Summary)>,
);

/// Expand and screen a whole batch. The batch is fixed before any of it runs,
/// and every job's seed comes from its own `(seq, arcane)`, so the result does
/// not depend on the thread count or on who finished first — a wasm run
/// reproduces a native one exactly.
#[cfg(not(target_arch = "wasm32"))]
fn evaluate_proposals(
    batch: &[Proposal],
    expand: &Expand<'_>,
    arcanes: &[wfsim_engine::data::arcanes::ArcaneFx],
    scenario: &Scenario,
    cfg: &SearchConfig,
    state: Option<&FunnelState>,
) -> Vec<Evaluated> {
    let threads = crate::worker_threads().min(batch.len().max(1));
    let chunk = batch.len().div_ceil(threads).max(1);
    let mut out: Vec<Vec<Evaluated>> = vec![Vec::new(); batch.len().div_ceil(chunk)];
    std::thread::scope(|scope| {
        for (part, slot) in batch.chunks(chunk).zip(out.iter_mut()) {
            let scenario = scenario.clone();
            scope.spawn(move || {
                crate::deprioritize_current_thread();
                *slot = part
                    .iter()
                    .map(|p| eval_one(p, expand, arcanes, &scenario, cfg, state))
                    .collect();
            });
        }
    });
    out.into_iter().flatten().collect()
}

#[cfg(target_arch = "wasm32")]
fn evaluate_proposals(
    batch: &[Proposal],
    expand: &Expand<'_>,
    arcanes: &[wfsim_engine::data::arcanes::ArcaneFx],
    scenario: &Scenario,
    cfg: &SearchConfig,
    state: Option<&FunnelState>,
) -> Vec<Evaluated> {
    batch
        .iter()
        .map(|p| {
            let r = eval_one(p, expand, arcanes, scenario, cfg, state);
            crate::tick(); // heartbeat: progress leaves the worker mid-batch
            r
        })
        .collect()
}

fn eval_one(
    p: &Proposal,
    expand: &Expand<'_>,
    arcanes: &[wfsim_engine::data::arcanes::ArcaneFx],
    scenario: &Scenario,
    cfg: &SearchConfig,
    state: Option<&FunnelState>,
) -> Evaluated {
    let cands: Vec<std::sync::Arc<Candidate>> =
        expand(&p.subset).into_iter().map(std::sync::Arc::new).collect();
    let n = cands.len();
    let mut results = Vec::with_capacity(n * arcanes.len());
    for (ci, c) in cands.iter().enumerate() {
        for (ai, arc) in arcanes.iter().enumerate() {
            if state.is_some_and(|s| s.cancel.load(Ordering::Relaxed)) {
                return (p.seq, n, results);
            }
            // The candidate index rides in the seed alongside the proposal's
            // own sequence number, so two candidates of one subset are not
            // handed the same random stream.
            let seed = job_seed(cfg.seed, p.seq.wrapping_mul(64).wrapping_add(ci), ai);
            let s = evaluate(c, arc, scenario, cfg.runs, seed);
            results.push((c.clone(), ai, s));
        }
    }
    (p.seq, n, results)
}
