// SPDX-License-Identifier: AGPL-3.0-or-later
//! The successive-halving funnel: its schedule, its live progress, and the
//! rounds that cull a field down to the finalists.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

use wfsim_engine::fight::Summary;

use crate::{evaluate_batch, Candidate, Job, Scenario};

/// Live progress of a running funnel, shared between the worker threads and
/// an observer (the web UI's status endpoint). Counters are plain atomics so
/// per-job updates stay lock-free; finished-round summaries land in `notes`
/// under a mutex. Setting `cancel` stops the funnel between jobs: the
/// in-flight round is discarded and `run_funnel` returns the last COMPLETED
/// round's leaderboard.
#[derive(Default)]
pub struct FunnelState {
    /// Monte-Carlo runs completed / planned across ALL rounds. The plan is
    /// exact (the schedule fixes every round's field × runs up front), so
    /// `done / planned` is a true overall percentage.
    pub sims_done: AtomicU64,
    pub sims_planned: AtomicU64,
    /// 1-based round in progress, and the total round count.
    pub round: AtomicUsize,
    pub rounds: AtomicUsize,
    /// The in-progress round's field size and per-job run count.
    pub round_jobs: AtomicUsize,
    pub round_runs: AtomicU32,
    /// Observer → funnel: request a stop (checked before each job AND
    /// inside candidate enumeration — a huge scope must stay cancellable).
    pub cancel: AtomicBool,
    /// Enumeration BUDGET exhausted — stop walking, keep what was found.
    ///
    /// Deliberately NOT `cancel`: cancelling means "the user asked to stop",
    /// and the caller answers it with an empty result. This means "the scope
    /// is bigger than the time allowed", and the caller must answer it with
    /// the best builds found so far. A full mod pool is C(72,8) ~ 1.1e10
    /// subsets; the legal ones run out early but the walk still has to grind
    /// the illegal remainder to prove it, which no browser tab should be asked
    /// to sit through.
    pub stop_enumeration: AtomicBool,
    /// Candidates emitted so far by a running enumeration (progress for
    /// the "enumerating" phase, where sims_done is still 0).
    pub enumerated: AtomicU64,
    /// A resumed screen is re-walking to its saved cut. Enumeration is cheap
    /// here (the rejected candidates are skipped, not evaluated), so this is
    /// NOT the phase the enumeration budget is meant to bound — a host that
    /// counted it would cut the run short of even catching up.
    pub rewalking: AtomicBool,
    /// One entry per FINISHED round.
    pub notes: Mutex<Vec<RoundNote>>,
}

/// A finished funnel round, for progress display.
#[derive(Debug, Clone, Copy)]
pub struct RoundNote {
    pub round: usize,
    pub jobs: usize,
    pub runs: u32,
    pub by_kills: bool,
    pub kept: usize,
    /// Best score under the round's own metric (kill progress on kill
    /// rounds, mean effective damage on screen rounds).
    pub best: f64,
    pub multishot: u64,
}

/// Self-scaling successive-halving schedule with the historical defaults
/// (final = 1024 runs × 24 finalists). See [`schedule_to`].
pub fn schedule(n_jobs: usize) -> Vec<(u32, usize, bool)> {
    schedule_to(n_jobs, 1024, 24)
}

/// Successive-halving schedule honoring the user's FINAL-ROUND CONTRACT: the last round is guaranteed to evaluate EXACTLY
/// `finalists` candidates at `final_runs` runs each — everything before it
/// only whittles the field down to that size.
///
/// The cadence is AUTO-PLANNED from the inputs ("derive
/// the elimination rhythm from N directly"):
/// - round count: k = ceil(log₈(N/F)) — "cull at most ×8 per round" is the
///   pace anchor and decides ONLY how many rounds exist;
/// - per-round cull ratio: ρ = (N/F)^(1/k), spread EVENLY in log space so
///   the last cut lands exactly on `finalists` (no floor-clamped tail
///   rounds; a small N/F gets proportionally gentler cuts);
/// - per-round runs: rᵢ = (ρ/2)^i — derived from a halving cost budget
///   (each round costs about half the previous; ρ ≤ 8 keeps growth ≤ ×4),
///   capped at `final_runs / 4` so the final stays a real step up.
///
/// EVERY round ranks by kill score (mean kill progress — just as
/// continuous as the old effective-damage screen, and it IS the
/// objective). The plan is an upper bound, not a promise of work:
/// [`run_funnel`] adapts it both ways at runtime (3σ racing cuts deeper,
/// tie amnesty keeps up to 2×, empty rounds are skipped).
pub fn schedule_to(n_jobs: usize, final_runs: u32, finalists: usize) -> Vec<(u32, usize, bool)> {
    let finalists = finalists.max(1);
    let final_runs = final_runs.max(1);
    let mut rounds = Vec::new();
    if n_jobs > finalists {
        let ratio_total = n_jobs as f64 / finalists as f64;
        let k = (ratio_total.ln() / 8f64.ln()).ceil().max(1.0) as usize;
        let rho = ratio_total.powf(1.0 / k as f64);
        let growth = (rho / 2.0).max(1.0);
        let cap = (final_runs / 4).max(1);
        let mut field = n_jobs as f64;
        let mut runs_f = 1.0f64;
        for i in 0..k {
            let keep = if i + 1 == k {
                finalists
            } else {
                ((field / rho).round() as usize).max(finalists)
            };
            rounds.push(((runs_f.round() as u32).clamp(1, cap), keep, true));
            field = keep as f64;
            runs_f *= growth;
        }
    }
    rounds.push((final_runs, finalists, true));
    rounds
}

/// HOW MANY OF A RANKED FIELD SURVIVE A CUT AFTER `planned`: every one below
/// the line whose kill progress still TIES the line's, within ±3·SE — SE from
/// the field's POOLED per-run σ of kill progress at `runs` (at 1 run no σ
/// exists anywhere, so a 5% relative gap stands in) — capped at twice the plan.
/// Rank order AT the line is noise, so a hard cut would gamble contenders away.
/// `field` is `(mean_kill_progress, std_kill_progress)`, best first.
pub fn tied_at_the_line(field: &[(f64, f64)], planned: usize, runs: u32) -> usize {
    let planned = planned.min(field.len());
    if planned == 0 || field.len() == planned {
        return planned;
    }
    let cap = (planned * 2).min(field.len());
    let cut_score = field[planned - 1].0;
    let tol = if runs >= 2 {
        // THE SPREAD OF THE STATISTIC BEING RANKED: `std_kills` has no partial
        // credit, so a build that never finishes its second kill got a
        // zero-width band from it (docs/OPTIMIZER.md, "The RANKING statistic").
        let pooled = (field.iter().map(|&(_, sd)| sd * sd).sum::<f64>() / field.len() as f64).sqrt();
        3.0 * pooled / f64::from(runs).sqrt()
    } else {
        cut_score.abs() * 0.05
    };
    let mut k = planned;
    while k < cap && field[k].0 >= cut_score - tol {
        k += 1;
    }
    k
}

/// One streamed job that survived the screen: the candidate (shared — the
/// same build may survive with several arcanes), its arcane index and the
/// screen-run summary.
pub struct ScreenedJob {
    pub cand: std::sync::Arc<Candidate>,
    pub ai: usize,
    pub summary: Summary,
}

/// Screen ordering: kill progress, then effective damage, then earliest
/// (seq, arcane) — a STRICT total order, so the surviving top-K set is
/// unique regardless of worker interleaving.
pub(crate) struct Scored {
    pub(crate) kp: f64,
    pub(crate) eff: f64,
    pub(crate) seq: usize,
    pub(crate) ai: usize,
    pub(crate) cand: std::sync::Arc<Candidate>,
    pub(crate) summary: Summary,
}
impl PartialEq for Scored {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == std::cmp::Ordering::Equal
    }
}
impl Eq for Scored {}
impl PartialOrd for Scored {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Scored {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.kp
            .total_cmp(&o.kp)
            .then(self.eff.total_cmp(&o.eff))
            .then(o.seq.cmp(&self.seq)) // earlier candidate wins exact ties
            .then(o.ai.cmp(&self.ai))
    }
}

/// Round wall-clock, compiled out on wasm32: `std::time::Instant` does not
/// exist on wasm32-unknown-unknown (it would panic at runtime), so there
/// `multishot()` reports 0 — progress display simply shows no round timing.
struct RoundTimer {
    #[cfg(not(target_arch = "wasm32"))]
    t: std::time::Instant,
}

impl RoundTimer {
    fn start() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            t: std::time::Instant::now(),
        }
    }
    fn multishot(&self) -> u64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.t.elapsed().as_millis() as u64
        }
        #[cfg(target_arch = "wasm32")]
        {
            0
        }
    }
}

/// The funnel's checkpoint sink: `(next_round, the round's ranked survivors)`.
/// Scores travel with the identities because the same snapshot has to serve
/// two purposes — resuming the search, and being the best-so-far leaderboard
/// a cancel shows.
pub type CheckpointFn<'a> = dyn Fn(usize, &[(Job, Summary)]) + 'a;

/// Best-so-far sink for the SCREEN, which is one long phase with no rounds in
/// it. Fires every `BOARD_EVERY` candidates with the current top slice.
///
/// It exists because a browser cancel is a KILL: the page terminates the
/// worker, so anything the worker has not already pushed out is gone (a
/// 20-minute run that showed nothing,). Native cancel is
/// cooperative — `stream_screen` returns its own best-so-far — so nothing
/// there depends on this.
pub type ScreenBoardFn<'a> = dyn Fn(&[ScreenedJob]) + 'a;

/// Best-so-far INSIDE a round, fired every `BOARD_EVERY` jobs evaluated.
///
/// A round is ONE blocking `evaluate_batch` call, and round 1 is by far the
/// longest — it is the whole field, before any culling. Nothing left the
/// worker between its start and its end, so a cancel inside it had nothing to
/// show. That holds in both
/// regimes: round 1 is the materialized field, or the screen's survivors.
/// Round boundaries alone are not a fine enough heartbeat to answer a cancel.
pub type RoundBoardFn<'a> = dyn Fn(&[(Job, Summary)]) + 'a;

/// How many entries a best-so-far snapshot carries. Comfortably above the
/// 20 finalists the UI shows, so the board never runs short of rows.
pub const BOARD_TOP: usize = 64;

/// Drive the multi-round funnel: for each `(runs, keep, by_kills)` round,
/// evaluate the surviving jobs, sort (kill-progress on kill rounds, effective
/// damage on screen rounds), and cull to `keep`. Returns the final sorted,
/// truncated leaderboard — `[0]` is the winner, `[..10]` the top-10. `verbose`
/// prints per-round progress (the CLI wants it; the web endpoint reads the
/// same numbers off `state` instead). `state` (optional) receives live
/// progress and carries the cancel flag: on cancel the in-flight round is
/// discarded and the last COMPLETED round's leaderboard is returned.
///
/// `on_round` (optional) fires after every COMPLETED round: single-threaded
/// wasm cannot poll `state` from outside a busy worker, so the callback is
/// where progress leaves the funnel. Native callers pass `None` and poll.
///
/// `start_round` RESUMES: rounds before it are skipped and `alive` is taken as
/// that round's incoming field. Seeds key off the ABSOLUTE round index, so a
/// resumed run draws exactly the numbers an uninterrupted one would.
///
/// `on_checkpoint(next_round, alive)` fires after each completed round with the
/// survivors, so a reload costs one round instead of the whole search — a
/// browser worker does not survive a page reload, and nor does a SharedWorker.
#[allow(clippy::too_many_arguments)] // search-config surface, like enumerate_candidates
pub fn run_funnel(
    cands: &[Candidate],
    arcanes: &[wfsim_engine::data::arcanes::ArcaneFx],
    scenario: &Scenario,
    mut alive: Vec<Job>,
    rounds: &[(u32, usize, bool)],
    seed_base: u64,
    verbose: bool,
    state: Option<&FunnelState>,
    on_round: Option<&dyn Fn()>,
    start_round: usize,
    on_checkpoint: Option<&CheckpointFn<'_>>,
    on_round_board: Option<&RoundBoardFn<'_>>,
) -> Vec<(Job, Summary)> {
    if let Some(st) = state {
        st.rounds.store(rounds.len(), Ordering::Relaxed);
        // The schedule fixes every round's field size up front (truncate is a
        // no-op when the field is already below `keep`), so the total sim
        // count is exact — done/planned is a true percentage.
        // On a RESUME the finished rounds are not going to run again, so they
        // must not sit in the denominator — otherwise the bar would open at
        // some fraction it can never reach.
        let mut field = alive.len() as u64;
        let mut sims = 0u64;
        for &(runs, keep, _) in rounds.iter().skip(start_round) {
            sims += field * runs as u64;
            field = field.min(keep as u64);
        }
        st.sims_planned.store(sims, Ordering::Relaxed);
    }
    let mut last: Vec<(Job, Summary)> = Vec::new();
    let floor = rounds.last().map_or(1, |&(_, f, _)| f);
    for (round, &(runs, keep, by_kills)) in rounds.iter().enumerate() {
        // Already done in a previous session: `alive` came in as this round's
        // output, so replaying it would only cost time and change nothing.
        if round < start_round {
            continue;
        }
        // Racing/amnesty may reach the finalists count early — remaining
        // intermediate rounds have nothing left to cut; jump to the final.
        if round + 1 < rounds.len() && alive.len() <= floor {
            continue;
        }
        if let Some(st) = state {
            if st.cancel.load(Ordering::Relaxed) {
                break;
            }
            st.round.store(round + 1, Ordering::Relaxed);
            st.round_jobs.store(alive.len(), Ordering::Relaxed);
            st.round_runs.store(runs, Ordering::Relaxed);
        }
        let t = RoundTimer::start();
        let started = alive.len();
        let summaries = evaluate_batch(
            cands,
            &alive,
            arcanes,
            scenario,
            runs,
            seed_base + round as u64,
            state,
            on_round_board,
            by_kills,
        );
        if state.is_some_and(|st| st.cancel.load(Ordering::Relaxed)) {
            // Cancelled mid-round. The previous COMPLETED round's leaderboard
            // is preferred (uniform estimates) — but when no round ever
            // finished (a huge round 1), rank whatever DID evaluate: a rough
            // best-so-far beats returning nothing.
            if last.is_empty() {
                let mut partial: Vec<(Job, Summary)> = alive
                    .iter()
                    .copied()
                    .zip(summaries)
                    .filter_map(|(j, s)| s.map(|s| (j, s)))
                    .collect();
                partial.sort_by(|a, b| b.1.mean_kill_progress.total_cmp(&a.1.mean_kill_progress));
                partial.truncate(keep);
                last = partial;
            }
            break;
        }
        let mut scored: Vec<(Job, Summary)> = alive
            .iter()
            .copied()
            .zip(summaries)
            .map(|(j, s)| (j, s.expect("uncancelled batch evaluates every job")))
            .collect();
        // Kill rounds rank by kill PROGRESS (kills + depleted fraction of the
        // final target's pool); screen rounds by mean effective damage.
        scored.sort_by(|a, b| {
            let ka = if by_kills {
                a.1.mean_kill_progress
            } else {
                a.1.mean_effective_damage
            };
            let kb = if by_kills {
                b.1.mean_kill_progress
            } else {
                b.1.mean_effective_damage
            };
            kb.total_cmp(&ka)
        });
        // SOFT cut line: the planned 1/8 keep stays as the BUDGET SKELETON, and
        // what ties the line gets amnesty (`tied_at_the_line`). The final round
        // never extends (its field is the contract), but the round FEEDING it
        // may — ties with the last finalist deserve the full-runs final.
        let planned = keep.min(scored.len());
        let keep_n = if round + 1 < rounds.len() {
            let field: Vec<(f64, f64)> =
                scored.iter().map(|(_, s)| (s.mean_kill_progress, s.std_kill_progress)).collect();
            tied_at_the_line(&field, planned, runs)
        } else {
            planned
        };
        scored.truncate(keep_n);
        // Adaptive racing cull ("reduce cleverly before
        // the final"): beyond the planned 1/8, drop every survivor whose 3σ
        // upper confidence bound still misses the finalists boundary's 3σ
        // lower bound — statistically hopeless candidates never see another
        // (4× more expensive) round. Needs runs ≥ 4 for a usable per-job σ;
        // the final round's field is untouchable.
        if round + 1 < rounds.len() && runs >= 4 && scored.len() > floor {
            // …and the same statistic here, for the same reason: the bound is
            // compared against `mean_kill_progress` on both sides of it.
            let se3 = |s: &Summary| 3.0 * s.std_kill_progress / f64::from(runs).sqrt();
            let cut = {
                let b = &scored[floor - 1].1;
                b.mean_kill_progress - se3(b)
            };
            let mut i = 0usize;
            scored.retain(|(_, s)| {
                let keep_it = i < floor || s.mean_kill_progress + se3(s) >= cut;
                i += 1;
                keep_it
            });
        }
        if verbose {
            println!(
                "[round {}] {} jobs x {} runs ({}) -> keep {} in {:.1}s; best {}",
                round + 1,
                started,
                runs,
                if by_kills { "kills" } else { "eff dmg" },
                scored.len(),
                t.multishot() as f64 / 1000.0,
                if by_kills {
                    format!("{:.2} kill score", scored[0].1.mean_kill_progress)
                } else {
                    format!("{:.3e} eff", scored[0].1.mean_effective_damage)
                }
            );
        }
        if let Some(st) = state {
            let best = if by_kills {
                scored[0].1.mean_kill_progress
            } else {
                scored[0].1.mean_effective_damage
            };
            st.notes.lock().unwrap().push(RoundNote {
                round: round + 1,
                jobs: started,
                runs,
                by_kills,
                kept: scored.len(),
                best,
                multishot: t.multishot(),
            });
        }
        alive = scored.iter().map(|(j, _)| *j).collect();
        // A completed round is the natural checkpoint: the field is settled and
        // the next round only needs THIS list.
        if let Some(cp) = on_checkpoint {
            cp(round + 1, &scored);
        }
        last = scored;
        if let Some(st) = state {
            // Replan the remaining work: adaptive culls shrink every later
            // round (and rounds the early-exit will skip cost nothing), so
            // done/planned stays a true percentage.
            let mut field = alive.len() as u64;
            let mut sims = st.sims_done.load(Ordering::Relaxed);
            for (idx2, &(r2, k2, _)) in rounds.iter().enumerate().skip(round + 1) {
                if idx2 + 1 == rounds.len() || field > floor as u64 {
                    sims += field * u64::from(r2);
                }
                field = field.min(k2 as u64);
            }
            st.sims_planned.store(sims, Ordering::Relaxed);
        }
        if let Some(cb) = on_round {
            cb();
        }
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{enumerate_candidates_observed, pool, rebuild_candidate, Constraints};
    use wfsim_engine::arena::Arena;
    use wfsim_engine::fight::LockMode;
    use wfsim_engine::model::StackPolicy;

    #[test]
    fn a_resumed_funnel_lands_on_the_same_leaderboard() {
        use wfsim_engine::target::BodyPart;
        let pool = pool();
        let base = wfsim_engine::model::WeaponBase::from_data("dual_toxocyst", true, &[]);
        let innate = wfsim_engine::data::weapons::innate_slots("dual_toxocyst");
        let (cands, _stats, _c) = enumerate_candidates_observed(
            &pool, &base, None, 0, 8, 8, 60, &innate,
            &Constraints::default(), &[None], None, 400,
            wfsim_engine::data::tenno::default_tenno(), StackPolicy::Emergent,
        );
        assert!(cands.len() > 40, "need a field to cut, got {}", cands.len());
        let arcanes = vec![wfsim_engine::data::arcanes::ArcaneFx::none()];
        let scenario = Scenario {
            also_acting: Vec::new(),
            arena: Arena {
                body_parts: vec![BodyPart {
                    name: "body".into(), aim_weight: 1.0, multiplier: 1.0,
                    is_head: false,
                    is_weak_point: false, crit_bonus: false,
                }],
                duration_seconds: 2.0,
                ..Arena::training(2.0)
            },
            incarnon_cycle: false,
            frenzy_lock: LockMode::Initial(0),
            frenzy_locks: Vec::new(),
            frenzy: false,
            buff_cfg: Default::default(),
            denied_buff_triggers: Vec::new(),
            infinite_ammo: true,
            policy: StackPolicy::Emergent,
        };
        let jobs: Vec<Job> = (0..cands.len()).map(|i| (i, 0)).collect();
        let rounds = schedule_to(jobs.len(), 8, 4);
        assert!(rounds.len() >= 3, "need several rounds, got {}", rounds.len());

        // (a) straight through.
        let whole = run_funnel(
            &cands, &arcanes, &scenario, jobs.clone(), &rounds, 0xDEAD_BEEF,
            false, None, None, 0, None, None,
        );

        // (b) stop after round 1, keep only the identities, rebuild, continue.
        let saved = std::cell::RefCell::new(None);
        let cp = |next: usize, alive: &[(Job, Summary)]| {
            if saved.borrow().is_none() {
                let ids: Vec<(Vec<usize>, u32, u32, usize)> = alive
                    .iter()
                    .map(|&((ci, ai), _)| (cands[ci].ordered.clone(), cands[ci].variant, cands[ci].exilus, ai))
                    .collect();
                *saved.borrow_mut() = Some((next, ids));
            }
        };
        run_funnel(
            &cands, &arcanes, &scenario, jobs, &rounds, 0xDEAD_BEEF,
            false, None, None, 0, Some(&cp), None,
        );
        let (next_round, ids) = saved.into_inner().expect("round 1 checkpointed");
        assert!(next_round >= 1);

        let mut rebuilt: Vec<Candidate> = Vec::new();
        let mut rjobs: Vec<Job> = Vec::new();
        for (ordered, variant, exilus, ai) in &ids {
            let c = rebuild_candidate(
                &pool, &base, None, &innate, 60, &scenario.arena.tenno, scenario.policy,
                ordered, *variant, *exilus, &[None],
            )
            .expect("a checkpointed build is still legal");
            rebuilt.push(c);
            rjobs.push((rebuilt.len() - 1, *ai));
        }
        let resumed = run_funnel(
            &rebuilt, &arcanes, &scenario, rjobs, &rounds, 0xDEAD_BEEF,
            false, None, None, next_round, None, None,
        );

        // Same builds, same order, same numbers.
        assert_eq!(whole.len(), resumed.len(), "leaderboard length");
        for (i, ((jw, sw), (jr, sr))) in whole.iter().zip(resumed.iter()).enumerate() {
            assert_eq!(
                cands[jw.0].ordered, rebuilt[jr.0].ordered,
                "rank {i}: different build"
            );
            assert_eq!(jw.1, jr.1, "rank {i}: different arcane");
            assert!(
                (sw.mean_kill_progress - sr.mean_kill_progress).abs() < 1e-12,
                "rank {i}: {} vs {}", sw.mean_kill_progress, sr.mean_kill_progress
            );
        }
    }

    #[test]
    fn schedule_plans_the_cadence_from_the_inputs() {
        // ~2M jobs, defaults (1024 × 24): k = ceil(log8(N/F)) rounds, even
        // log-space culls ending EXACTLY on the finalists, runs from a
        // halving cost budget, every round by kill score.
        let s = schedule(1_950_192);
        let n = s.len();
        assert_eq!(n, 7, "k = ceil(log8(81258)) = 6 intermediates + final");
        assert_eq!(s[0].0, 1, "the screen starts at 1 run");
        assert!(s.windows(2).all(|w| w[1].0 >= w[0].0 && w[1].1 <= w[0].1));
        assert!(
            s.iter().all(|&(_, _, by_kills)| by_kills),
            "kill score everywhere"
        );
        assert!(
            s[..n - 1].iter().all(|&(r, _, _)| r <= 256),
            "intermediates ≤ final/4"
        );
        assert_eq!(s.last(), Some(&(1024, 24, true)));
        // The round BEFORE the final already reaches the finalists count —
        // the final evaluates exactly that field (the contract).
        assert_eq!(s[n - 2].1, 24);
        // Cull ratios are even: every intermediate cut is ≤ ×8 and the
        // ratios stay within rounding of each other.
        let mut field = 1_950_192usize;
        for &(_, keep, _) in &s[..n - 1] {
            let r = field as f64 / keep as f64;
            assert!(r <= 8.01, "cull ratio {r}");
            field = keep;
        }
        // Total sims stays well below flat evaluation (halving budget ≈ 2N).
        let mut field = 1_950_192usize;
        let mut sims = 0usize;
        for &(runs, keep, _) in &s {
            sims += field * runs as usize;
            field = field.min(keep);
        }
        assert!(sims < 3 * 1_950_192, "sims {sims}");
        // A small N/F gap plans proportionally GENTLER cuts (no ÷8 overshoot
        // straight into the floor).
        let small = schedule(500);
        assert_eq!(small.len(), 3, "two intermediates + final");
        assert_eq!(small.first(), Some(&(1, 110, true)));
        assert_eq!(small[1].1, 24);
        assert_eq!(small.last(), Some(&(1024, 24, true)));

        // The user's contract verbatim: final = 10_000 runs × 20 finalists.
        let big = schedule_to(437_000, 10_000, 20);
        assert_eq!(big.last(), Some(&(10_000, 20, true)));
        assert_eq!(big[big.len() - 2].1, 20);
        assert!(big[..big.len() - 1].iter().all(|&(r, _, _)| r <= 2500));
        // Tiny fields skip straight to the final.
        assert_eq!(schedule_to(15, 10_000, 20), vec![(10_000, 20, true)]);
    }
}
