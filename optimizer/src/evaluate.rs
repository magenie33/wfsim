// SPDX-License-Identifier: AGPL-3.0-or-later
//! One build under one arcane, scored by the engine's Monte Carlo — and a
//! batch of them across the worker threads.

use std::sync::atomic::Ordering;

use wfsim_engine::arena::Arena;
use wfsim_engine::fight::{monte_carlo, shard_onto, FightParams, Shard, Summary};
use wfsim_engine::model::StackPolicy;

use crate::{Candidate, FunnelState, RoundBoardFn};
#[cfg(not(target_arch = "wasm32"))]
use crate::threads::{deprioritize_current_thread, worker_threads};
#[cfg(target_arch = "wasm32")]
use crate::{tick, BOARD_TOP};

/// HOW A CANDIDATE BECOMES A FIGHT: a candidate and an arcane INDEX in, the
/// fight's params out — `None` when the simulator refuses the build.
pub type ParamsFn = dyn Fn(&Candidate, usize) -> Option<FightParams> + Send + Sync;

/// What a search scores in. THE OPTIMIZER BUILDS NO FIGHT: `params` is the
/// simulator's own construction, handed in by the caller (`webapi::simulate::
/// ready`, on the candidate's `replay`), so a build is scored exactly as it is
/// replayed. A fixture with no simulator states its own (`from_panels`).
#[derive(Clone)]
pub struct Scenario {
    pub params: std::sync::Arc<ParamsFn>,
    /// The fight's arena, for what is read off it beside the score: the Tenno a
    /// candidate's panel is resolved for, and how long the fight lasts.
    pub arena: Arena,
    /// How conditional buffs are valued when a candidate's panel is resolved
    /// for the enumeration — the fight's, never a constant.
    pub policy: StackPolicy,
}

impl Scenario {
    /// A FIXTURE'S FIGHT: the candidate's own panel under `arena`, with no
    /// roster and no fight terms — for a test or the CLI, which have no
    /// simulator to call. Never the product's.
    pub fn from_panels(arena: Arena, policy: StackPolicy, arcanes: Vec<wfsim_engine::data::arcanes::ArcaneFx>) -> Self {
        let fight = arena.clone();
        let params = move |c: &Candidate, ai: usize| -> Option<FightParams> {
            let mut p = FightParams::from_panel(&c.panel, &fight, arcanes.get(ai)?);
            p.infinite_reserve = c.panel.reserve_is_infinite(true);
            Some(p)
        };
        Self { params: std::sync::Arc::new(params), arena, policy }
    }
}

/// Score one candidate under one arcane: the simulator's fight, the engine's
/// Monte Carlo. A refused build scores nothing.
pub fn evaluate(c: &Candidate, ai: usize, s: &Scenario, runs: u32, seed: u64) -> Summary {
    match (s.params)(c, ai) {
        Some(p) => monte_carlo(&p, runs, seed),
        None => Summary::refused(s.arena.duration_seconds),
    }
}

/// [`evaluate`] to `runs`, keeping the runs `prior` already fought — the first
/// `prior.runs` of the same stream — and handing back the shard, so a longer
/// measurement can keep these in turn. The summary is [`evaluate`]'s to the bit
/// (`shard_onto`). A refused build has no shard.
pub fn evaluate_onto(
    c: &Candidate,
    ai: usize,
    s: &Scenario,
    runs: u32,
    seed: u64,
    prior: Option<&Shard>,
) -> (Summary, Option<Shard>) {
    let Some(p) = (s.params)(c, ai) else { return (Summary::refused(s.arena.duration_seconds), None) };
    let acc = prior.filter(|x| x.runs <= runs).cloned().unwrap_or_default();
    let from = acc.runs;
    let sh = shard_onto(&p, acc, from, runs - from, seed, false, &mut |_| {});
    (sh.clone().finish(&p, runs).0, Some(sh))
}

/// One evaluation job: a candidate paired with an arcane INDEX into the
/// search's resolved arcane list (the arcane is a search dimension like
/// the mod choice; data-driven `ArcaneFx` is not `Copy`, so jobs carry
/// the index).
pub type Job = (usize, usize);

/// Deterministic per-job seed, mixed per round. One definition for both
/// evaluation strategies: the seed depends only on (candidate, arcane), never
/// on thread count or chunking, so serial wasm evaluation reproduces native
/// results bit-for-bit.
pub(crate) fn job_seed(seed: u64, ci: usize, ai: usize) -> u64 {
    seed ^ (ci as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ ((ai as u64) << 56)
}

/// Evaluate jobs concurrently across all cores (single-threaded on wasm32 —
/// same seeds, same order, identical results, just serial). Returns summaries
/// index-aligned with `jobs`; entries are `None` only when a cancel
/// request stopped the batch before that job ran.
#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)] // search-config surface, like run_funnel
pub fn evaluate_batch(
    cands: &[Candidate],
    jobs: &[Job],
    scenario: &Scenario,
    runs: u32,
    seed: u64,
    state: Option<&FunnelState>,
    // Never fired here: native cancel is cooperative, so `run_funnel` returns
    // the mid-round best-so-far itself, and the status endpoint polls `state`.
    // The browser can do neither — cancel there is a worker kill.
    _on_board: Option<&RoundBoardFn<'_>>,
    _by_kills: bool,
) -> Vec<Option<Summary>> {
    let threads = worker_threads();
    let chunk = jobs.len().div_ceil(threads).max(1);
    let mut results: Vec<Option<Summary>> = vec![None; jobs.len()];
    std::thread::scope(|scope| {
        for (ids, res) in jobs.chunks(chunk).zip(results.chunks_mut(chunk)) {
            let scenario = scenario.clone();
            scope.spawn(move || {
                deprioritize_current_thread();
                for (k, &(ci, ai)) in ids.iter().enumerate() {
                    if state.is_some_and(|st| st.cancel.load(Ordering::Relaxed)) {
                        return;
                    }
                    res[k] = Some(evaluate(
                        &cands[ci],
                        ai,
                        &scenario,
                        runs,
                        job_seed(seed, ci, ai),
                    ));
                    if let Some(st) = state {
                        st.sims_done.fetch_add(runs as u64, Ordering::Relaxed);
                    }
                }
            });
        }
    });
    results
}

/// wasm32 (docs/WASM.md phase 3): no threads in a Web Worker — evaluate the
/// jobs sequentially with the identical per-job seeds.
#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)] // search-config surface, like run_funnel
pub fn evaluate_batch(
    cands: &[Candidate],
    jobs: &[Job],
    scenario: &Scenario,
    runs: u32,
    seed: u64,
    state: Option<&FunnelState>,
    on_board: Option<&RoundBoardFn<'_>>,
    by_kills: bool,
) -> Vec<Option<Summary>> {
    let mut results: Vec<Option<Summary>> = vec![None; jobs.len()];
    // The round's leaders so far, best-first. Ranked the way THIS round ranks,
    // so the snapshot a cancel shows agrees with the cut the round would make.
    let mut best: Vec<(Job, Summary)> = Vec::new();
    let score = |s: &Summary| {
        if by_kills {
            s.mean_kill_progress
        } else {
            s.mean_effective_damage
        }
    };
    for (k, &(ci, ai)) in jobs.iter().enumerate() {
        if state.is_some_and(|st| st.cancel.load(Ordering::Relaxed)) {
            break;
        }
        let s = evaluate(
            &cands[ci],
            ai,
            scenario,
            runs,
            job_seed(seed, ci, ai),
        );
        results[k] = Some(s);
        if let Some(st) = state {
            st.sims_done.fetch_add(runs as u64, Ordering::Relaxed);
        }
        if let Some(b) = on_board {
            if best.len() < BOARD_TOP || score(&s) > score(&best[best.len() - 1].1) {
                let at = best.partition_point(|(_, o)| score(o) >= score(&s));
                best.insert(at, ((ci, ai), s));
                best.truncate(BOARD_TOP);
            }
            if (k + 1).is_multiple_of(BOARD_EVERY) {
                b(&best);
            }
        }
        tick(); // wasm heartbeat: intra-round progress leaves the worker
    }
    results
}

/// How often the SERIAL (wasm) batch publishes one. Native evaluates a round
/// across threads and reports at its boundaries; the browser has one thread and
/// a round can be the whole field, so it publishes mid-round or not at all.
///
/// Native `cargo clippy` cannot see this constant used — the only reader is
/// behind `cfg(target_arch = "wasm32")` — so it reads as dead code there and
/// was deleted once on that advice. Only `scripts/build_site_app.py` (which
/// compiles the wasm target) caught it.
#[cfg(target_arch = "wasm32")]
const BOARD_EVERY: usize = 4096;
