// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE QUICK CALC, REPEATED — the page's search (docs/OPTIMIZER.md,
//! "The quick descent — the quick calc, repeated").
//!
//! From each start: one pass over the positions in order that fills what the
//! start left empty or did not name, and keeps what it did — then sweep the
//! positions in order, run the quick calc on each, keep the best
//! legal candidate when it beats the build, and start the sweep over — until no
//! position offers anything better. Each start answers with ONE build; starts
//! that settle on the same build merge.
//!
//! The loop knows nothing about Warframe. What a position is, what may replace
//! it, whether a build fits and what it scores are the [`QuickSpace`]'s — the
//! caller wires them to the quick calc's own candidate list, the auto-Forma
//! planner and the simulator's fight, so none of those rules exists twice.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

/// A position of a build: an axis kind and an index within it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Position {
    pub kind: &'static str,
    pub idx: usize,
}

/// A score: the objective, then the tie-break. `None` = cannot be scored.
pub type Score = Option<(f64, f64)>;

fn better(a: Score, b: Score) -> bool {
    match (a, b) {
        (Some(_), None) => true,
        (Some((ak, ae)), Some((bk, be))) => ak.total_cmp(&bk).then(ae.total_cmp(&be)).is_gt(),
        _ => false,
    }
}

/// Everything the loop asks of the world.
pub trait QuickSpace: Sync {
    type Build: Clone + Send + Sync;
    /// Every position a sweep visits, in order — the order the fill takes too.
    fn positions(&self, b: &Self::Build) -> Vec<Position>;
    /// The positions a start leaves empty.
    fn empty(&self, b: &Self::Build) -> Vec<Position>;
    /// What may replace `p`: whole builds, one change from `b`, already inside
    /// the scope. Capacity is NOT asked here.
    fn candidates(&self, b: &Self::Build, p: Position) -> Vec<Self::Build>;
    /// Does the build fit (the auto-Forma planner, and every equip rule)?
    fn legal(&self, b: &Self::Build) -> bool;
    /// Score builds on ONE random stream, so every comparison is paired.
    fn score(&self, bs: &[Self::Build]) -> Vec<Score>;
    /// The build as written: the cache's key.
    fn key(&self, b: &Self::Build) -> String;
    /// What makes two answers ONE build (the canonical form: the same cards in
    /// any slots, the same combined elements, …). Defaults to the key.
    fn identity(&self, b: &Self::Build) -> String {
        self.key(b)
    }
    /// The last `score` left builds for someone else to score. The start that
    /// asked PAUSES — it stops where it is, its scores uncached — and the run
    /// moves on to the next start, so one run collects every start's next
    /// batch. A caller that sees any pause discards the answers and runs again.
    fn pending(&self) -> bool {
        false
    }
}

/// Where a descent begins, what it may never change, and what it did not name.
#[derive(Debug, Clone)]
pub struct QuickStart<B> {
    pub build: B,
    pub fixed: Vec<Position>,
    /// Positions holding only their axis's default because the start named
    /// nothing there: the fill chooses them, keeping the default only when no
    /// candidate beats it.
    pub unnamed: Vec<Position>,
}

/// One start's answer.
#[derive(Debug, Clone)]
pub struct QuickAnswer<B> {
    pub build: B,
    pub score: Score,
    /// Indices of the starts that settled here.
    pub starts: Vec<usize>,
    /// The first start's score before its descent, per start in `starts`.
    pub from: Vec<Score>,
    /// Accepted changes, per start in `starts`.
    pub moves: Vec<u32>,
}

/// A start that produced no answer, and why.
#[derive(Debug, Clone)]
pub struct QuickFailure {
    pub start: usize,
    pub why: &'static str,
}

/// Where one start is: filling or sweeping (which round), and at which of how
/// many positions — what the page shows while a descent runs, since how many
/// rounds it takes is found by taking them.
#[derive(Debug, Clone, Copy, Default)]
pub struct StartProgress {
    pub settled: bool,
    /// 0 = filling; n = the n-th sweep.
    pub round: u32,
    pub at: usize,
    pub of: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct QuickStats {
    pub evals: u64,
    pub starts: u32,
    /// The budget stopped some start before it settled.
    pub cut: bool,
}

/// How far a run may go.
pub struct QuickConfig<'a> {
    /// Screen evaluations; 0 = no cap.
    pub max_evals: u64,
    /// Any of these set stops the run: a cancel, a clock.
    pub stop: Vec<&'a AtomicBool>,
}

struct Run<'a, S: QuickSpace> {
    space: &'a S,
    cfg: &'a QuickConfig<'a>,
    cache: HashMap<String, Score>,
    stats: QuickStats,
    now: StartProgress,
}

impl<S: QuickSpace> Run<'_, S> {
    fn out_of_budget(&self) -> bool {
        (self.cfg.max_evals > 0 && self.stats.evals >= self.cfg.max_evals)
            || self.cfg.stop.iter().any(|s| s.load(Ordering::Relaxed))
    }

    /// Scores for `bs`, evaluating only builds not seen before. `None` = the
    /// budget ran out first.
    fn score(&mut self, bs: &[S::Build]) -> Option<Vec<Score>> {
        let keys: Vec<String> = bs.iter().map(|b| self.space.key(b)).collect();
        let mut fresh: Vec<usize> = Vec::new();
        for (i, k) in keys.iter().enumerate() {
            if !self.cache.contains_key(k) && !fresh.iter().any(|&j| keys[j] == *k) {
                fresh.push(i);
            }
        }
        if !fresh.is_empty() {
            if self.out_of_budget() {
                return None;
            }
            let batch: Vec<S::Build> = fresh.iter().map(|&i| bs[i].clone()).collect();
            let got = self.space.score(&batch);
            if self.space.pending() {
                return None;
            }
            self.stats.evals += batch.len() as u64;
            for (&i, s) in fresh.iter().zip(got) {
                self.cache.insert(keys[i].clone(), s);
            }
        }
        Some(keys.iter().map(|k| self.cache[k]).collect())
    }

    /// The best LEGAL candidate of `alts`. Legality is asked before anything is
    /// simulated: dropping first and taking the best of the rest is the same
    /// answer as simulating all and walking down the ranking, for less.
    fn best(&mut self, alts: Vec<S::Build>) -> Option<Option<(S::Build, Score)>> {
        let alts: Vec<S::Build> = alts.into_iter().filter(|b| self.space.legal(b)).collect();
        if alts.is_empty() {
            return Some(None);
        }
        let scores = self.score(&alts)?;
        let mut best: Option<(S::Build, Score)> = None;
        for (b, s) in alts.iter().zip(scores) {
            if best.as_ref().is_none_or(|(_, x)| better(s, *x)) {
                best = Some((b.clone(), s));
            }
        }
        Some(best)
    }

    /// Fill, then sweep to a fixed point. `Err` = the start has no answer.
    fn descend(
        &mut self,
        start: &QuickStart<S::Build>,
    ) -> Result<(S::Build, Score, Score, u32, bool), &'static str> {
        let fixed = |p: &Position| start.fixed.contains(p);
        let mut cur = start.build.clone();
        let mut moves = 0u32;
        // FILL: ONE PASS, in the sweep's order, so what shapes the weapon (its
        // mode, its evolutions, its arcane) is settled before any card is
        // chosen for it. An EMPTY position takes its best legal candidate
        // whether or not the bare slot scored higher — no candidate is
        // "empty". An UNNAMED one holds only a default, and keeps it only when
        // no candidate beats it. What the start NAMED is not touched here:
        // judged on a half-empty build, Primed Cryo Rounds lost to Hellfire on
        // Burston Prime and the answer lost Viral with it.
        let empty = self.space.empty(&cur);
        let fill: Vec<Position> = self
            .space
            .positions(&cur)
            .into_iter()
            .filter(|p| !fixed(p) && (empty.contains(p) || start.unnamed.contains(p)))
            .collect();
        self.now = StartProgress { settled: false, round: 0, at: 0, of: fill.len() };
        for (k, p) in fill.into_iter().enumerate() {
            self.now.at = k;
            let Some(found) = self.best(self.space.candidates(&cur, p)) else {
                return Ok((cur, None, None, moves, true));
            };
            let Some((b, s)) = found else { continue };
            let take = empty.contains(&p) || {
                if !self.space.legal(&cur) {
                    true
                } else {
                    let Some(now) = self.score(std::slice::from_ref(&cur)) else {
                        return Ok((cur, None, None, moves, true));
                    };
                    better(s, now[0])
                }
            };
            if take {
                cur = b;
                moves += 1;
            }
        }
        if !self.space.legal(&cur) {
            return Err("the start itself does not fit, and nothing it can be filled with does");
        }
        let Some(s) = self.score(std::slice::from_ref(&cur)) else {
            return Ok((cur, None, None, moves, true));
        };
        let from = s[0];
        let mut cur_score = from;
        'sweep: loop {
            let positions: Vec<Position> =
                self.space.positions(&cur).into_iter().filter(|p| !fixed(p)).collect();
            self.now = StartProgress { settled: false, round: self.now.round + 1, at: 0, of: positions.len() };
            for (k, &p) in positions.iter().enumerate() {
                self.now.at = k;
                match self.best(self.space.candidates(&cur, p)) {
                    None => return Ok((cur, cur_score, from, moves, true)),
                    Some(Some((b, s))) if better(s, cur_score) => {
                        cur = b;
                        cur_score = s;
                        moves += 1;
                        continue 'sweep;
                    }
                    Some(_) => {}
                }
            }
            self.now.settled = true;
            return Ok((cur, cur_score, from, moves, false));
        }
    }
}

/// What a run hands back: the answers best first, the starts that found none,
/// what it spent, and where each start stood when it stopped.
pub type QuickRun<B> = (Vec<QuickAnswer<B>>, Vec<QuickFailure>, QuickStats, Vec<StartProgress>);

/// Descend from every start; answers merged by key, best first.
pub fn quick_descent<S: QuickSpace>(
    space: &S,
    starts: &[QuickStart<S::Build>],
    cfg: &QuickConfig<'_>,
) -> QuickRun<S::Build> {
    let mut run =
        Run { space, cfg, cache: HashMap::new(), stats: QuickStats::default(), now: StartProgress::default() };
    let mut answers: Vec<QuickAnswer<S::Build>> = Vec::new();
    let mut failures = Vec::new();
    let mut progress = Vec::with_capacity(starts.len());
    for (i, start) in starts.iter().enumerate() {
        run.stats.starts += 1;
        let out = run.descend(start);
        progress.push(StartProgress { settled: out.is_err() || run.now.settled, ..run.now });
        match out {
            Err(why) => failures.push(QuickFailure { start: i, why }),
            Ok((build, score, from, moves, cut)) => {
                run.stats.cut |= cut;
                let id = space.identity(&build);
                match answers.iter_mut().find(|a| space.identity(&a.build) == id) {
                    Some(a) => {
                        a.starts.push(i);
                        a.from.push(from);
                        a.moves.push(moves);
                    }
                    None => answers.push(QuickAnswer { build, score, starts: vec![i], from: vec![from], moves: vec![moves] }),
                }
            }
        }
    }
    answers.sort_by(|a, b| {
        if better(a.score, b.score) {
            std::cmp::Ordering::Less
        } else if better(b.score, a.score) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Equal
        }
    });
    (answers, failures, run.stats, progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A toy: three slots, each takes a digit 0–9; the score is the digits'
    /// sum, except that 9 in slot 0 is illegal. The descent must reach 8+9+9,
    /// never seat the illegal 9, and merge two starts that settle there.
    struct Digits;
    impl QuickSpace for Digits {
        type Build = [Option<u8>; 3];
        fn positions(&self, _: &Self::Build) -> Vec<Position> {
            (0..3).map(|idx| Position { kind: "d", idx }).collect()
        }
        fn empty(&self, b: &Self::Build) -> Vec<Position> {
            (0..3).filter(|&i| b[i].is_none()).map(|idx| Position { kind: "d", idx }).collect()
        }
        fn candidates(&self, b: &Self::Build, p: Position) -> Vec<Self::Build> {
            (0..10u8)
                .filter(|d| b[p.idx] != Some(*d))
                .map(|d| {
                    let mut n = *b;
                    n[p.idx] = Some(d);
                    n
                })
                .collect()
        }
        fn legal(&self, b: &Self::Build) -> bool {
            b[0] != Some(9)
        }
        fn score(&self, bs: &[Self::Build]) -> Vec<Score> {
            bs.iter()
                .map(|b| Some((b.iter().flatten().map(|&d| f64::from(d)).sum(), 0.0)))
                .collect()
        }
        fn key(&self, b: &Self::Build) -> String {
            format!("{b:?}")
        }
    }

    #[test]
    fn a_descent_takes_the_best_legal_and_merges_what_settles_together() {
        let starts = vec![
            QuickStart { build: [None, None, None], fixed: vec![], unnamed: vec![] },
            QuickStart { build: [Some(1), Some(2), Some(3)], fixed: vec![], unnamed: vec![] },
            QuickStart { build: [Some(0), None, None], fixed: vec![Position { kind: "d", idx: 0 }], unnamed: vec![] },
        ];
        let cfg = QuickConfig { max_evals: 0, stop: Vec::new() };
        let (answers, failures, _, _) = quick_descent(&Digits, &starts, &cfg);
        assert!(failures.is_empty());
        assert_eq!(answers[0].build, [Some(8), Some(9), Some(9)]);
        assert_eq!(answers[0].starts, vec![0, 1], "two starts settle on one build");
        assert_eq!(answers[1].build, [Some(0), Some(9), Some(9)], "a fixed position never moves");
    }
}
