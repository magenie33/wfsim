//! THE SEARCH — one path at every scope.
//!
//! The optimizer used to enumerate the whole space and rank it. That works
//! while the space fits: a 22-mod pool is 571,569 candidates and a browser can
//! walk it. It does not scale, because the space is superexponential
//! (30 mods → 9.2 million, the full 60-mod pool → ~10⁹–10¹⁰) while one
//! evaluation costs a full simulated engagement (~150/s single-threaded in
//! wasm). At that ratio a search can afford ~10⁴ evaluations against 10⁹
//! candidates, so BEING CUT SHORT IS THE NORMAL CASE — and the old
//! enumeration, being depth-first, left a lexicographic corner behind when it
//! was cut rather than a sample of the space (docs/OPTIMIZER.md).
//!
//! This module keeps the two halves of the problem apart, because they are
//! different problems and only one of them was ever solved here:
//!
//! - **Which builds to look at** — the search. That is this file.
//! - **Which of them is best under noise** — the funnel, unchanged. Measured
//!   against ground truth it culls 22,316 jobs to 10 for 1.5% of the flat cost
//!   and loses nothing, so it is not what needed replacing.
//!
//! ## One loop, both regimes
//!
//! The search walks [`Shuffle`], a pseudorandom bijection on the subset
//! space's index range. Walking it to the end visits every subset exactly once
//! — the search IS exhaustive, provably, for any scope the budget can finish.
//! Stopping early leaves a uniform sample WITHOUT REPLACEMENT. There is no
//! mode to pick and no threshold to cross (user, 2026-08-03: 绝不分大小); which
//! one a run turned out to be is just whether it reached the end, and
//! [`SearchStats::exhaustive`] says which.
//!
//! ## Sampling alone is not enough, so the budget is split
//!
//! 10⁴ uniform samples of 10⁹ builds find a build at about the 1-in-10⁴
//! quantile, which is not an answer anyone wants. So once the explore share of
//! the budget is spent, the rest goes to the NEIGHBOURHOOD of what the samples
//! found: swap one mod, add one, drop one. Build quality is largely modular
//! with a few strong interactions (elements, status thresholds, Condition
//! Overload), which is exactly the landscape a 1-swap neighbourhood climbs
//! well. Every proposal is deduplicated against everything already tried, so
//! exploitation never re-buys what exploration already paid for.
//!
//! ## Why a 1-run screen is allowed to steer
//!
//! Measured, not assumed: over a 64,796-job scope, ranking every job on ONE
//! Monte-Carlo run and keeping the top sixth drops **0 of the true top 100**
//! (docs/OPTIMIZER.md). Kill progress over a 300 s engagement is a low-variance
//! statistic. So the cheap screen is a sound gradient, and the expensive
//! precision belongs where it always did — the final rounds.

use std::collections::HashSet;
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
    /// Share of `max_evals` spent SAMPLING before the neighbourhood takes
    /// over. Ignored when the sample exhausts the space first, which is the
    /// whole point: a scope that fits is never sampled twice.
    pub explore_frac: f64,
    /// How many screened jobs survive into the funnel.
    pub keep: usize,
    pub seed: u64,
    /// Monte-Carlo runs per screen evaluation. 1 — see the module note.
    pub runs: u32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        // 0.3 is MEASURED, not chosen (14-mod Verglas scope, 22,316 jobs,
        // Thrax Lv 9999 SP, graded against ground truth at 500 and 1000
        // screen evaluations):
        //
        //   frac  500 evals            1000 evals
        //   0.15  rank 1, recall 60%   rank 1, recall  90%
        //   0.30  rank 1, recall 60%   rank 1, recall 100%
        //   0.45  rank 8, regret 2.3%  rank 1, recall  80%
        //
        // 0.15 and 0.30 both find the optimum where 0.45 does not; 0.30 wins
        // on recall and keeps twice the exploration, which is the safer side
        // of a landscape whose ruggedness has been measured on one scope.
        SearchConfig { max_evals: 0, explore_frac: 0.3, keep: 65_536, seed: 0xDEAD_BEEF, runs: 1 }
    }
}

/// What a run actually did — the honest report a truncated search owes.
#[derive(Debug, Clone, Copy, Default)]
pub struct SearchStats {
    /// Indices the space holds.
    pub space: u128,
    /// Indices consumed from the shuffled order.
    pub sampled: u128,
    /// Subsets actually built (the rest were family collisions).
    pub subsets: u64,
    /// Subsets proposed by the neighbourhood rather than the sample.
    pub neighbours: u64,
    /// Candidates expanded from those subsets.
    pub candidates: u64,
    /// Screen evaluations spent.
    pub evals: u64,
    /// Did the sample reach the end of the space? Then this was not a search
    /// at all — it was an enumeration, and its winner is THE winner.
    pub exhaustive: bool,
}

impl SearchStats {
    /// Share of the space the sample covered, in `0..=1`. Exact, because the
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
    /// depends on nothing but the search's own deterministic order.
    seq: usize,
}

fn key_of(subset: &[usize]) -> u64 {
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

/// Run the search. Returns the screened survivors (best first) and what the
/// run covered.
#[allow(clippy::too_many_arguments)]
pub fn search(
    space: &SubsetSpace,
    expand: &Expand<'_>,
    arcanes: &[wfsim_engine::arcanes_data::ArcaneFx],
    scenario: &Scenario,
    cfg: &SearchConfig,
    state: Option<&FunnelState>,
    on_board: Option<&crate::ScreenBoardFn<'_>>,
) -> (Vec<ScreenedJob>, SearchStats) {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let mut stats = SearchStats { space: space.len(), ..Default::default() };
    let mut top: BinaryHeap<Reverse<Scored>> = BinaryHeap::new();
    let mut tried: HashSet<u64> = HashSet::new();
    let shuffle = Shuffle::new(space.len(), cfg.seed);
    let mut seq = 0usize;
    let mut k: u128 = 0; // position in the shuffled order
    // The climb's frontier: elites whose neighbourhood has been generated, and
    // the neighbours still waiting for a slot in a batch.
    let mut expanded: HashSet<u64> = HashSet::new();
    let mut pending: Vec<Vec<usize>> = Vec::new();

    let stopped = |st: Option<&FunnelState>| {
        st.is_some_and(|s| {
            s.cancel.load(Ordering::Relaxed) || s.stop_enumeration.load(Ordering::Relaxed)
        })
    };
    let batch_size = crate::batch_width();

    loop {
        if stopped(state) {
            break;
        }
        if cfg.max_evals > 0 && stats.evals >= cfg.max_evals {
            break;
        }
        // EXPLORE while the shuffled order still has ground and the explore
        // share is unspent; EXPLOIT after that. `stop_explore` lets a host
        // whose budget is a clock rather than a count make the same switch.
        let explore_spent = (cfg.max_evals > 0
            && stats.evals as f64 >= cfg.explore_frac * cfg.max_evals as f64)
            || state.is_some_and(|s| s.stop_explore.load(Ordering::Relaxed));
        let exploring = k < space.len() && !explore_spent;

        // A batch must not overrun the phase it belongs to. Batches are wide
        // (4 per worker) so every core stays fed, and a small budget was
        // therefore spent entirely inside the FIRST one: with 120 evaluations
        // and a batch of 104 subsets, the explore share ended after the budget
        // did and the climb never ran at all. Trim the batch to what is left of
        // the current limit, in SUBSETS — a subset costs several evaluations
        // (its element orders, exilus options and evolution sets), so the
        // conversion uses the rate this run has actually been paying.
        let per_subset = if stats.subsets > 0 {
            (stats.evals as f64 / stats.subsets as f64).max(1.0)
        } else {
            1.0
        };
        let room = |limit: f64| -> usize {
            if !limit.is_finite() {
                return batch_size;
            }
            let left = limit - stats.evals as f64;
            if left <= 0.0 {
                return 1;
            }
            ((left / per_subset).ceil() as usize).clamp(1, batch_size)
        };
        let total_limit = if cfg.max_evals > 0 { cfg.max_evals as f64 } else { f64::INFINITY };
        let batch_size = if exploring {
            let explore_limit = if cfg.max_evals > 0 {
                cfg.explore_frac * cfg.max_evals as f64
            } else {
                f64::INFINITY
            };
            room(explore_limit)
        } else {
            room(total_limit)
        };

        let mut batch: Vec<Proposal> = Vec::with_capacity(batch_size);
        if exploring {
            let mut buf = Vec::new();
            while batch.len() < batch_size && k < space.len() {
                let i = shuffle.at(k);
                k += 1;
                stats.sampled += 1;
                if !space.nth(i, &mut buf) {
                    continue; // family collision: an index with no subset
                }
                if !tried.insert(key_of(&buf)) {
                    continue;
                }
                batch.push(Proposal { subset: buf.clone(), seq });
                seq += 1;
            }
        } else {
            // CLIMB, best-first and EXHAUSTIVELY. The neighbourhood of one
            // build is small — swaps are k(n-k), plus n-k adds and k drops, so
            // 62 subsets for 8-of-14 — and enumerating all of it is both
            // cheaper and far better than sampling it at random: a random
            // mutation mostly re-draws moves already seen, while the full
            // neighbourhood is a complete local improvement step. It is also
            // deterministic, which the sampling version was not.
            while pending.is_empty() {
                let Some(next) = best_unexpanded(&top, &expanded) else { break };
                expanded.insert(key_of(&next));
                pending = neighbourhood(space, &next)
                    .into_iter()
                    .filter(|n| !tried.contains(&key_of(n)))
                    .collect();
            }
            if pending.is_empty() {
                break; // every elite expanded and every neighbour already seen
            }
            while batch.len() < batch_size {
                let Some(next) = pending.pop() else { break };
                if !tried.insert(key_of(&next)) {
                    continue;
                }
                batch.push(Proposal { subset: next, seq });
                seq += 1;
                stats.neighbours += 1;
            }
            if batch.is_empty() {
                continue;
            }
        }
        if batch.is_empty() {
            // Explore ran the shuffled order out without filling a batch —
            // the space is exhausted. Fall through to exploitation only if
            // there is budget left AND ground it has not covered; there is
            // not, so the run is over and it is an EXHAUSTIVE one.
            if k >= space.len() {
                break;
            }
            continue;
        }

        // ---- evaluate the batch ----
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

    stats.exhaustive = k >= space.len();
    let mut out: Vec<Scored> = top.into_iter().map(|r| r.0).collect();
    out.sort_by(|a, b| b.cmp(a));
    (
        out.into_iter()
            .map(|s| ScreenedJob { cand: s.cand, ai: s.ai, summary: s.summary })
            .collect(),
        stats,
    )
}

fn push_elite(
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

fn snapshot(
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

/// The best elite whose neighbourhood has not been generated yet — where the
/// climb goes next. Best-first, so the budget is spent around the strongest
/// build found rather than spread evenly over a pool most of which is known to
/// be worse.
fn best_unexpanded(
    top: &std::collections::BinaryHeap<std::cmp::Reverse<Scored>>,
    expanded: &HashSet<u64>,
) -> Option<Vec<usize>> {
    let mut v: Vec<&Scored> = top.iter().map(|r| &r.0).collect();
    v.sort_by(|a, b| b.cmp(a));
    for s in v {
        let mut sub = s.cand.ordered.clone();
        sub.sort_unstable();
        if !expanded.contains(&key_of(&sub)) {
            return Some(sub);
        }
    }
    None
}

/// EVERY 1-move neighbour of a subset: swap one member for one non-member,
/// add one, drop one. Canonical (ascending) and filtered to what this space
/// accepts, so the caller can treat them as ordinary proposals.
///
/// Swap is the move that carries the information — it holds the build size
/// still, so it compares two builds that differ in exactly one slot. Add and
/// drop exist so the size can move at all when the scope allows a range.
fn neighbourhood(space: &SubsetSpace, from: &[usize]) -> Vec<Vec<usize>> {
    let pool = space.choosable();
    let sizes = space.sizes();
    let droppable: Vec<usize> = from
        .iter()
        .copied()
        .filter(|i| !space.required().contains(i))
        .collect();
    let mut out = Vec::new();
    let mut push = |v: Vec<usize>| {
        if space.legal(&v) {
            out.push(v);
        }
    };
    for &inn in pool {
        if from.contains(&inn) {
            continue;
        }
        for &out_i in &droppable {
            let mut v: Vec<usize> = from.iter().copied().filter(|&x| x != out_i).collect();
            v.push(inn);
            v.sort_unstable();
            push(v);
        }
        if from.len() < *sizes.end() {
            let mut v = from.to_vec();
            v.push(inn);
            v.sort_unstable();
            push(v);
        }
    }
    if from.len() > *sizes.start() {
        for &out_i in &droppable {
            let v: Vec<usize> = from.iter().copied().filter(|&x| x != out_i).collect();
            push(v);
        }
    }
    out
}

type Evaluated = (
    usize,
    usize,
    Vec<(std::sync::Arc<Candidate>, usize, wfsim_engine::dummy::Summary)>,
);

/// Expand and screen a whole batch. The batch is fixed before any of it runs,
/// and every job's seed comes from its own `(seq, arcane)`, so the result does
/// not depend on the thread count or on who finished first — a wasm run
/// reproduces a native one exactly.
#[cfg(not(target_arch = "wasm32"))]
fn evaluate_proposals(
    batch: &[Proposal],
    expand: &Expand<'_>,
    arcanes: &[wfsim_engine::arcanes_data::ArcaneFx],
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
    arcanes: &[wfsim_engine::arcanes_data::ArcaneFx],
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
    arcanes: &[wfsim_engine::arcanes_data::ArcaneFx],
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
