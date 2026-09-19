// SPDX-License-Identifier: AGPL-3.0-or-later
//! One build under one arcane, scored by the engine's Monte Carlo — and a
//! batch of them across the worker threads.

use std::sync::atomic::Ordering;

use wfsim_engine::arena::Arena;
use wfsim_engine::fight::{monte_carlo, BuffConfig, BuffLock, FightParams, LockMode, Summary};
use wfsim_engine::model::StackPolicy;

use crate::{Candidate, FunnelState, RoundBoardFn};
#[cfg(not(target_arch = "wasm32"))]
use crate::threads::{deprioritize_current_thread, worker_threads};
#[cfg(target_arch = "wasm32")]
use crate::{tick, BOARD_TOP};

/// The benchmark engagement: an [`Arena`] plus what the SEARCH needs on top of
/// it. The arcane is a SEARCH DIMENSION — passed per
/// evaluation job, not fixed here.
#[derive(Clone)]
pub struct Scenario {
    /// The fight itself — both actors and how long they are at it. EMBEDDED,
    /// not restated: the optimizer scores a build under the same arena the
    /// simulator will replay it in, and a lookalike of the four fields is how
    /// the two drift a field at a time.
    pub arena: Arena,
    /// Run the REAL Incarnon two-form cycle (full gauge start → dump →
    /// revert → rebuild 9 weakpoint charges → transmute → …) instead of
    /// the locked-gauge pseudo-reload model. Needs candidates enumerated
    /// with a second form.
    pub incarnon_cycle: bool,
    /// Frenzy's per-buff lock setting for the base-form phase.
    pub frenzy_lock: LockMode,
    /// The same setting for a SINGLE-form run, where the lock is a vector on
    /// the params rather than something baked into the cycle. Frenzy is the
    /// weapon's passive and persists across its forms, so a build scored in
    /// one form keeps it — a cycle is not what grants it.
    pub frenzy_locks: Vec<BuffLock>,
    /// Does the weapon under search carry the Frenzy passive? It is a
    /// per-weapon perk, not a constant — see data::weapons::has_perk.
    pub frenzy: bool,
    /// Per-buff configured policy applied to every evaluated build (same id
    /// scheme as the web Sim panel). Empty = the emergent default.
    pub buff_cfg: BuffConfig,
    /// WHICH TRIGGERS FIRE NO BUFF HERE (`engine::buff_events`), off the
    /// scenario like every other term — skipping it would rank builds by stacks
    /// the simulator then refuses them.
    pub denied_buff_triggers: Vec<String>,
    /// INFINITE RESERVE — the simulator's own scenario knob, which the
    /// optimizer READS. Ignoring it SEARCHES a weapon with a finite reserve
    /// (Larkspur Prime) running dry while the simulator replays it resupplied,
    /// and the search then reports half the number for the same build.
    pub infinite_ammo: bool,
    /// How conditional buffs are valued. NOT a constant: a SENTINEL weapon
    /// resolves under `BaseOnly` — this arena fires one weapon, so nothing on
    /// the field can trigger a companion gun's conditionals — and hardcoding
    /// `Emergent` handed it buffs the simulator refuses it.
    pub policy: StackPolicy,
}

/// Evaluate one candidate with a given arcane: engine Monte Carlo only.
pub fn evaluate(
    c: &Candidate,
    arcane: &wfsim_engine::data::arcanes::ArcaneFx,
    s: &Scenario,
    runs: u32,
    seed: u64,
) -> Summary {
    // The cycle needs a form to transform INTO, and whether this candidate
    // has one is the candidate's own question: the Incarnon form is unlocked
    // by an evolution and evolutions are a search dimension, so one scope can
    // hold both sets that transform and sets that cannot. A candidate
    // enumerated without a second form is fired in the one form it has.
    let mut params = match (s.incarnon_cycle, c.base_panel.as_ref()) {
        (true, Some(base)) => {
            let mut p = FightParams::incarnon_cycle_from_panels(
                &c.panel,
                base,
                s.frenzy,
                s.frenzy_lock,
                &s.arena,
                arcane,
            );
            // The cycle reports the form it transforms INTO, so its reserve is
            // that form's — the same line `simulate_json` runs.
            p.infinite_reserve = c.panel.reserve_is_infinite(s.infinite_ammo);
            p
        }
        _ => {
            let mut d = FightParams::from_panel(&c.panel, &s.arena, arcane);
            // The scenario's ammo rule, exactly as `simulate_json` applies it.
            d.infinite_reserve = c.panel.reserve_is_infinite(s.infinite_ammo);
            // Frenzy is the WEAPON's passive: it rides whichever form is
            // fired (the Sim's rule). Dropping it here scored a base-form
            // Dual Toxocyst without its own x2.5 fire rate.
            d.frenzy = s.frenzy;
            d.locked_buffs = if s.frenzy { s.frenzy_locks.clone() } else { Vec::new() };
            d
        }
    };
    // NOT `params.arcane = arcane` any more: `from_panel` took the arcane
    // above, because a build and an arcane meet in exactly one place — this is
    // where Primary Compression learns which radius it is compressing and where
    // a stat lock silences an arcane's buff.
    // Per-buff configured policy (weapon-scoped; recurses into the cycle base
    // form). Empty cfg = no-op → the emergent default.
    if !s.buff_cfg.is_empty() {
        params.apply_buff_config(&s.buff_cfg);
    }
    // …then what the fight refuses, in the order the simulator applies them:
    // the cards say where a run OPENS, this says what it can EARN.
    params.deny_buff_triggers(&s.denied_buff_triggers);
    monte_carlo(&params, runs, seed)
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
    arcanes: &[wfsim_engine::data::arcanes::ArcaneFx],
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
                        &arcanes[ai],
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
    arcanes: &[wfsim_engine::data::arcanes::ArcaneFx],
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
            &arcanes[ai],
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
