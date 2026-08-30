//! GROUND TRUTH — what the search is graded against.
//!
//! Accuracy is not something a search strategy can assert about itself, so it
//! is MEASURED against an answer obtained another way: take a scope small
//! enough to **exhaust**, evaluate **every** job in it flat at a high run
//! count, and that ranking is the reference. Two things it does NOT assume:
//!
//! 1. **That the truth is a single build.** The objective is a Monte-Carlo
//!    mean with a standard error, and the top of a real scope is usually a
//!    CLUSTER no run count can separate, so demanding rank 1 fails a search
//!    for being unlucky rather than wrong. The target is
//!    [`Truth::indistinguishable`] and [`Verdict::within_noise`] is the
//!    pass/fail.
//! 2. **That the truth is trustworthy by construction.** A reference measured
//!    at too few runs is another noisy ranking wearing a badge, so
//!    [`Truth::agrees_with`] re-measures under a different seed and reports
//!    the overlap.

use wfsim_engine::dummy::Summary;

use crate::{evaluate_batch, Candidate, Job, Scenario};

/// One job's objective, with the uncertainty that comes with a mean.
#[derive(Debug, Clone, Copy)]
pub struct Estimate {
    /// Mean kill progress — the objective the funnel ranks by.
    pub mean: f64,
    /// Standard error of that mean: σ of the per-run statistic over √runs.
    pub se: f64,
}

impl Estimate {
    fn of(s: &Summary, runs: u32) -> Self {
        Estimate {
            mean: s.mean_kill_progress,
            se: s.std_kill_progress / f64::from(runs.max(1)).sqrt(),
        }
    }
    /// Are two estimates separable at `sigmas`? The difference of two means has
    /// the standard error of the two combined in quadrature.
    fn separable(&self, o: &Estimate, sigmas: f64) -> bool {
        (self.mean - o.mean).abs() > sigmas * (self.se * self.se + o.se * o.se).sqrt()
    }
}

/// An exhausted scope, flat-evaluated.
#[derive(Debug, Clone)]
pub struct Truth {
    pub runs: u32,
    /// Index-aligned with the `jobs` slice it was measured from.
    pub est: Vec<Estimate>,
    /// Job indices, best mean first.
    pub order: Vec<usize>,
}

impl Truth {
    /// Evaluate EVERY job flat. No funnel, no culling — that is the point:
    /// the reference must not share a strategy with what it grades.
    pub fn measure(
        cands: &[Candidate],
        jobs: &[Job],
        arcanes: &[wfsim_engine::arcanes_data::ArcaneFx],
        scenario: &Scenario,
        runs: u32,
        seed: u64,
    ) -> Truth {
        let sums = evaluate_batch(cands, jobs, arcanes, scenario, runs, seed, None, None, true);
        let est: Vec<Estimate> = sums
            .iter()
            .map(|s| Estimate::of(s.as_ref().expect("flat evaluation is never cancelled"), runs))
            .collect();
        let mut order: Vec<usize> = (0..est.len()).collect();
        // Ties break on index so the order is a strict function of the input.
        order.sort_by(|&a, &b| est[b].mean.total_cmp(&est[a].mean).then(a.cmp(&b)));
        Truth { runs, est, order }
    }

    pub fn best(&self) -> usize {
        self.order[0]
    }

    /// 1-based position of a job in the reference ranking.
    pub fn rank_of(&self, job: usize) -> usize {
        self.order.iter().position(|&j| j == job).expect("job in scope") + 1
    }

    /// How much objective a choice gives up, as a fraction of the best.
    pub fn regret(&self, job: usize) -> f64 {
        let best = self.est[self.best()].mean;
        if best <= 0.0 {
            return 0.0;
        }
        ((best - self.est[job].mean) / best).max(0.0)
    }

    /// Every job the measurement cannot separate from the best. This is the
    /// ANSWER SET: a search that returns any of these has not made a mistake,
    /// and one that returns something outside it has.
    pub fn indistinguishable(&self, sigmas: f64) -> Vec<usize> {
        let b = self.est[self.best()];
        self.order
            .iter()
            .copied()
            .take_while(|&j| !self.est[j].separable(&b, sigmas))
            .collect()
    }

    /// Fraction of THIS truth's top `k` that `other` also puts in its top `k`.
    /// Re-measure under a different seed and compare: a reference that does not
    /// reproduce itself is not a reference.
    pub fn agrees_with(&self, other: &Truth, k: usize) -> f64 {
        let k = k.min(self.order.len()).min(other.order.len()).max(1);
        let mine: std::collections::HashSet<usize> = self.order[..k].iter().copied().collect();
        let hits = other.order[..k].iter().filter(|j| mine.contains(j)).count();
        hits as f64 / k as f64
    }
}

/// What a search strategy's leaderboard is worth against the reference.
#[derive(Debug, Clone)]
pub struct Verdict {
    /// Where the strategy's WINNER sits in the reference ranking (1-based).
    pub rank: usize,
    /// Objective it gave up, as a fraction of the best.
    pub regret: f64,
    /// The pass/fail: is the winner inside the reference's answer set?
    pub within_noise: bool,
    /// Fraction of the reference's top `k` the strategy's own top `k` contains
    /// — a strategy can find the winner and still be blind to the field.
    pub recall: f64,
    /// Monte-Carlo runs the strategy spent to get there, against the flat
    /// reference's own cost. Accuracy is only interesting next to its price.
    pub sims: u64,
    pub reference_sims: u64,
}

/// Grade a strategy's leaderboard (job indices, best first) against `truth`.
pub fn judge(truth: &Truth, leaderboard: &[usize], k: usize, sims: u64) -> Verdict {
    let winner = *leaderboard.first().expect("a strategy returns at least one build");
    let answer: std::collections::HashSet<usize> =
        truth.indistinguishable(3.0).into_iter().collect();
    let k = k.min(truth.order.len()).max(1);
    let top: std::collections::HashSet<usize> = truth.order[..k].iter().copied().collect();
    let hits = leaderboard.iter().take(k).filter(|j| top.contains(j)).count();
    Verdict {
        rank: truth.rank_of(winner),
        regret: truth.regret(winner),
        within_noise: answer.contains(&winner),
        recall: hits as f64 / k as f64,
        sims,
        reference_sims: truth.est.len() as u64 * u64::from(truth.runs),
    }
}
