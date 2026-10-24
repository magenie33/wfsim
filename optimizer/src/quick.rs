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
    /// A step's SHORT measurement, on the same stream as [`Self::score`];
    /// `None` = no screen, every candidate is measured in full.
    fn screen(&self, _bs: &[Self::Build]) -> Option<Vec<Score>> {
        None
    }
    /// The share of a step's screened candidates measured in full.
    fn keep_ratio(&self) -> f64 {
        1.0
    }
    /// OF TWO BUILDS THAT SCORE AS ONE ([`one_build`]), the one kept: `Less` =
    /// `a` (`wfsim_engine::board::builds::preferred`). The default prefers
    /// neither, which keeps the first one offered.
    fn prefer(&self, _a: &Self::Build, _b: &Self::Build) -> std::cmp::Ordering {
        std::cmp::Ordering::Equal
    }
    /// A BUILD THAT DOES NOT FIT, made to fit by lowering only the parts at
    /// `free` — positions whose part was chosen where every candidate scored as
    /// one, so lowering it moves no number the step could see. `None` = it does
    /// not fit that way either. The repaired build is scored like any other.
    fn repair(&self, _b: &Self::Build, _free: &[Position]) -> Option<Self::Build> {
        None
    }
    /// THE COUPLED POSITIONS — what shapes the build: few options each, and each
    /// moves what the others are worth. Once no single change is better, every
    /// TWO of them changed at once is one more step; and a start's fill is taken
    /// both with them first and with them last. Empty = neither.
    fn coupled(&self, _b: &Self::Build) -> Vec<Position> {
        Vec::new()
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
    /// Screen scores of builds the screen dropped. Never in `cache`, so a
    /// dropped build is never a move and never on the list.
    rough: HashMap<String, Score>,
    /// Every build a SWEEP asked about, by key: whole builds, legal, one change
    /// from where some start stood. The fill's half-empty builds are not here.
    seen: HashMap<String, S::Build>,
    sweeping: bool,
    stats: QuickStats,
    now: StartProgress,
    /// The descent's positions whose part was chosen where every candidate
    /// scored as one: what [`QuickSpace::repair`] may lower.
    free: Vec<Position>,
    /// The last screen dropped nothing that did not score as one with the rest.
    screen_tied: bool,
}

impl<S: QuickSpace> Run<'_, S> {
    /// Does `b` scoring `s` replace `x` scoring `xs`? A tie is the space's
    /// preference, never the last bit of a sum.
    fn wins(&self, b: &S::Build, s: Score, x: &S::Build, xs: Score) -> bool {
        if one_build(s, xs) {
            self.space.prefer(b, x).is_lt()
        } else {
            better(s, xs)
        }
    }

    fn set_free(&mut self, p: Position, free: bool) {
        self.free.retain(|q| *q != p);
        if free {
            self.free.push(p);
        }
    }

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

    /// `alts` less what the screen drops: of the candidates with no full score
    /// yet, the best `keep_ratio` by a short measurement go on. `None` = the
    /// budget ran out first.
    fn screened(&mut self, alts: Vec<S::Build>) -> Option<Vec<S::Build>> {
        let keys: Vec<String> = alts.iter().map(|b| self.space.key(b)).collect();
        let mut fresh: Vec<usize> = Vec::new();
        for (i, k) in keys.iter().enumerate() {
            if !self.cache.contains_key(k) && !fresh.iter().any(|&j| keys[j] == *k) {
                fresh.push(i);
            }
        }
        if fresh.len() < 2 {
            return Some(alts);
        }
        let unseen: Vec<usize> = fresh.iter().copied().filter(|&i| !self.rough.contains_key(&keys[i])).collect();
        if !unseen.is_empty() {
            if self.out_of_budget() {
                return None;
            }
            let batch: Vec<S::Build> = unseen.iter().map(|&i| alts[i].clone()).collect();
            let Some(got) = self.space.screen(&batch) else { return Some(alts) };
            if self.space.pending() {
                return None;
            }
            for (&i, s) in unseen.iter().zip(got) {
                self.rough.insert(keys[i].clone(), s);
            }
        }
        let mut ranked = fresh.clone();
        let head = self.rough[&keys[fresh[0]]];
        self.screen_tied = fresh.iter().all(|&i| one_build(self.rough[&keys[i]], head));
        rank_ties(&mut ranked, |&i| self.rough[&keys[i]], |&a, &b| self.space.prefer(&alts[a], &alts[b]).then(a.cmp(&b)));
        let keep = ((self.space.keep_ratio() * fresh.len() as f64).ceil() as usize).clamp(1, fresh.len());
        let dropped: std::collections::HashSet<&String> = ranked[keep..].iter().map(|&i| &keys[i]).collect();
        Some(alts.iter().zip(&keys).filter(|(_, k)| !dropped.contains(k)).map(|(b, _)| b.clone()).collect())
    }

    /// The best LEGAL candidate of `alts` at `p`, and whether every candidate
    /// scored as one. Legality is asked before anything is simulated: dropping
    /// first and taking the best of the rest is the same answer as simulating
    /// all and walking down the ranking, for less. A candidate that does not
    /// fit is first offered to [`QuickSpace::repair`] over the other free
    /// positions: a riven that only fits once a utility exilus is a smaller
    /// one is that riven, not a refusal.
    fn best(&mut self, alts: Vec<S::Build>, p: Position) -> Option<Option<(S::Build, Score, bool)>> {
        let free: Vec<Position> = self.free.iter().copied().filter(|q| *q != p).collect();
        let mut keys = std::collections::HashSet::new();
        let mut fit: Vec<S::Build> = Vec::new();
        for b in alts {
            let b = if self.space.legal(&b) {
                b
            } else {
                match self.space.repair(&b, &free) {
                    Some(r) if self.space.legal(&r) => r,
                    _ => continue,
                }
            };
            if keys.insert(self.space.key(&b)) {
                fit.push(b);
            }
        }
        if fit.is_empty() {
            return Some(None);
        }
        self.screen_tied = true;
        let alts = self.screened(fit)?;
        let scores = self.score(&alts)?;
        if self.sweeping {
            for b in &alts {
                self.seen.entry(self.space.key(b)).or_insert_with(|| b.clone());
            }
        }
        let mut best: Option<(S::Build, Score)> = None;
        for (b, s) in alts.iter().zip(&scores) {
            if best.as_ref().is_none_or(|(x, xs)| self.wins(b, *s, x, *xs)) {
                best = Some((b.clone(), *s));
            }
        }
        // EVERY CANDIDATE AS ONE, the screen's dropped ones included.
        let tied = keys.len() > 1
            && self.screen_tied
            && best.as_ref().is_some_and(|(_, bs)| scores.iter().all(|s| one_build(*s, *bs)));
        Some(best.map(|(b, s)| (b, s, tied)))
    }

    /// What a start's fill chooses, in order: the sweep's order — what shapes
    /// the build first — or, `shape_last`, the coupled positions after the rest.
    fn fill_of(&self, start: &QuickStart<S::Build>, shape_last: bool) -> Vec<Position> {
        let empty = self.space.empty(&start.build);
        let mut fill: Vec<Position> = self
            .space
            .positions(&start.build)
            .into_iter()
            .filter(|p| !start.fixed.contains(p) && (empty.contains(p) || start.unnamed.contains(p)))
            .collect();
        if shape_last {
            let coupled = self.space.coupled(&start.build);
            fill.sort_by_key(|p| coupled.contains(p));
        }
        fill
    }

    /// Fill, then sweep to a fixed point. `Err` = the start has no answer.
    fn descend(
        &mut self,
        start: &QuickStart<S::Build>,
        shape_last: bool,
    ) -> Result<(S::Build, Score, Score, u32, bool), &'static str> {
        let fixed = |p: &Position| start.fixed.contains(p);
        let mut cur = start.build.clone();
        let mut moves = 0u32;
        // FILL: ONE PASS. An EMPTY position takes its best legal candidate
        // whether or not the bare slot scored higher — no candidate is "empty".
        // An UNNAMED one holds only a default, and keeps it only when no
        // candidate beats it. What the start NAMED is not touched here: judged
        // on a half-empty build, Primed Cryo Rounds lost to Hellfire on Burston
        // Prime and the answer lost Viral with it. The ORDER is the caller's
        // (`fill_of`): what shapes the build chosen before its cards, or after.
        let empty = self.space.empty(&cur);
        let fill = self.fill_of(start, shape_last);
        self.sweeping = false;
        self.now = StartProgress { settled: false, round: 0, at: 0, of: fill.len() };
        for (k, p) in fill.into_iter().enumerate() {
            self.now.at = k;
            let Some(found) = self.best(self.space.candidates(&cur, p), p) else {
                return Ok((cur, None, None, moves, true));
            };
            let Some((b, s, tied)) = found else { continue };
            let take = empty.contains(&p) || {
                if !self.space.legal(&cur) {
                    true
                } else {
                    let Some(now) = self.score(std::slice::from_ref(&cur)) else {
                        return Ok((cur, None, None, moves, true));
                    };
                    self.wins(&b, s, &cur, now[0])
                }
            };
            if take {
                cur = b;
                moves += 1;
                self.set_free(p, tied);
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
        self.sweeping = true;
        self.seen.entry(self.space.key(&cur)).or_insert_with(|| cur.clone());
        // A TIE MOVES ONLY TOWARD THE PREFERRED, but "as one" is a tolerance
        // and not an order, so a build the sweep has stood on is never a move.
        let mut visited = std::collections::HashSet::from([self.space.key(&cur)]);
        'sweep: loop {
            let positions: Vec<Position> =
                self.space.positions(&cur).into_iter().filter(|p| !fixed(p)).collect();
            let coupled: Vec<Position> = self.space.coupled(&cur).into_iter().filter(|p| !fixed(p)).collect();
            let of = positions.len() + usize::from(coupled.len() > 1);
            self.now = StartProgress { settled: false, round: self.now.round + 1, at: 0, of };
            for (k, &p) in positions.iter().enumerate() {
                self.now.at = k;
                match self.best(self.space.candidates(&cur, p), p) {
                    None => return Ok((cur, cur_score, from, moves, true)),
                    Some(Some((b, s, tied)))
                        if self.wins(&b, s, &cur, cur_score) && visited.insert(self.space.key(&b)) =>
                    {
                        cur = b;
                        cur_score = s;
                        moves += 1;
                        self.set_free(p, tied);
                        continue 'sweep;
                    }
                    Some(Some((_, s, tied))) => self.set_free(p, tied && one_build(s, cur_score)),
                    Some(None) => {}
                }
            }
            // NO SINGLE CHANGE IS BETTER: try every TWO coupled positions at
            // once. A Laetum riven's answer wanted Enervate and Overwhelming
            // Attrition together, each worse alone (123 against 155 kpm), and
            // the element starts all differ in a card, never in an arcane.
            if coupled.len() > 1 {
                self.now.at = positions.len();
                let mut pairs: Vec<S::Build> = Vec::new();
                let mut keys = std::collections::HashSet::new();
                for (i, &p) in coupled.iter().enumerate() {
                    for c in self.space.candidates(&cur, p) {
                        for &q in &coupled[i + 1..] {
                            for d in self.space.candidates(&c, q) {
                                if keys.insert(self.space.key(&d)) {
                                    pairs.push(d);
                                }
                            }
                        }
                    }
                }
                match self.best(pairs, coupled[0]) {
                    None => return Ok((cur, cur_score, from, moves, true)),
                    Some(Some((b, s, _)))
                        if self.wins(&b, s, &cur, cur_score) && visited.insert(self.space.key(&b)) =>
                    {
                        cur = b;
                        cur_score = s;
                        moves += 1;
                        continue 'sweep;
                    }
                    _ => {}
                }
            }
            self.now.settled = true;
            return Ok((cur, cur_score, from, moves, false));
        }
    }
}

/// What a run hands back.
pub struct QuickRun<B> {
    /// One per start, merged by identity, best first.
    pub answers: Vec<QuickAnswer<B>>,
    pub failures: Vec<QuickFailure>,
    pub stats: QuickStats,
    /// Where each start stood when it stopped.
    pub progress: Vec<StartProgress>,
    /// EVERY WHOLE BUILD THE SWEEPS SCORED, one per identity and one per score,
    /// best first — the answers and every build one change from where a start
    /// stood. The best N
    /// of a run are drawn from here, not from the answers alone: the runner-up
    /// to an answer is usually that answer with one card swapped.
    pub pool: Vec<(B, Score)>,
}

/// Descend from every start; answers merged by key, best first.
pub fn quick_descent<S: QuickSpace>(
    space: &S,
    starts: &[QuickStart<S::Build>],
    cfg: &QuickConfig<'_>,
) -> QuickRun<S::Build> {
    let mut run = Run {
        space,
        cfg,
        cache: HashMap::new(),
        rough: HashMap::new(),
        seen: HashMap::new(),
        sweeping: false,
        stats: QuickStats::default(),
        now: StartProgress::default(),
        free: Vec::new(),
        screen_tied: true,
    };
    let mut answers: Vec<QuickAnswer<S::Build>> = Vec::new();
    let mut failures = Vec::new();
    let mut progress = Vec::with_capacity(starts.len());
    for (i, start) in starts.iter().enumerate() {
        run.stats.starts += 1;
        // THE FILL BOTH WAYS: what shapes the build chosen before its cards,
        // then after them. Chosen first, on a build with no cards yet, every
        // element start chose Furis the same evolutions and they all settled
        // at 141 kpm; its cards chosen first, the toxin start reached 184 — the
        // board's own #1. A start whose fill reads the same either way is
        // descended once.
        let ways: &[bool] = if run.fill_of(start, false) == run.fill_of(start, true) { &[false] } else { &[false, true] };
        let mut lane = StartProgress::default();
        for &shape_last in ways {
            run.free.clear();
            let out = run.descend(start, shape_last);
            lane = StartProgress { settled: out.is_err() || run.now.settled, round: lane.round + run.now.round, ..run.now };
            match out {
                Err(why) => {
                    if !failures.iter().any(|f: &QuickFailure| f.start == i) {
                        failures.push(QuickFailure { start: i, why });
                    }
                }
                Ok((build, score, from, moves, cut)) => {
                    run.stats.cut |= cut;
                    let id = space.identity(&build);
                    match answers.iter_mut().find(|a| space.identity(&a.build) == id) {
                        Some(a) if a.starts.contains(&i) => {}
                        Some(a) => {
                            a.starts.push(i);
                            a.from.push(from);
                            a.moves.push(moves);
                        }
                        None => answers.push(QuickAnswer { build, score, starts: vec![i], from: vec![from], moves: vec![moves] }),
                    }
                    if cut {
                        break;
                    }
                }
            }
        }
        progress.push(lane);
    }
    rank_ties(&mut answers, |a| a.score, |a, b| space.prefer(&a.build, &b.build));
    // THE SAME SCORE ON THE PAIRED STREAM IS THE SAME BUILD for the list
    // ([`one_build`]): a utility exilus, or an evolution the engine does not
    // load, swapped — Boar Prime's Mercenary Chamber and Practiced Grip filled
    // half its top ten as twins. Re-measured on separate streams, twins also pushed the
    // answer itself off the list. The answer is the one kept, then the first
    // key, so the order never depends on the map's. The cost: an effect that
    // moves no number in `candidate_runs` fights is merged with its absence.
    let answer_ids: std::collections::HashSet<String> = answers.iter().map(|a| space.identity(&a.build)).collect();
    let mut pool: Vec<(String, bool, S::Build, Score)> = run
        .seen
        .into_iter()
        .filter_map(|(k, b)| {
            let s = run.cache.get(&k).copied().flatten()?;
            Some((k, answer_ids.contains(&space.identity(&b)), b, Some(s)))
        })
        .collect();
    rank_ties(&mut pool, |x| x.3, |a, b| b.1.cmp(&a.1).then_with(|| space.prefer(&a.2, &b.2)).then(a.0.cmp(&b.0)));
    let mut ids = std::collections::HashSet::new();
    let mut last: Score = None;
    pool.retain(|(_, _, b, s)| {
        let fresh = !one_build(*s, last) && ids.insert(space.identity(b));
        last = *s;
        fresh
    });
    let pool = pool.into_iter().map(|(_, _, b, s)| (b, s)).collect();
    QuickRun { answers, failures, stats: run.stats, progress, pool }
}

/// ONE BUILD TO THE FIGHT: two scores on one paired stream that agree to a
/// relative 1e-9 on both counts. Not to the bit — the same bonuses summed in
/// another slot order differ in the last bit (Serration and Heavy Caliber,
/// whose accuracy penalty the arena does not read).
pub fn one_build(a: Score, b: Score) -> bool {
    let near = |x: f64, y: f64| (x - y).abs() <= 1e-9 * x.abs().max(y.abs());
    matches!((a, b), (Some((ak, ae)), Some((bk, be))) if near(ak, bk) && near(ae, be))
}

/// BEST FIRST, and each run that scores as one with its first ([`one_build`])
/// in `tie`'s order. Two passes, because "as one" is a tolerance: a comparator
/// built on it is not an order, and a sort may panic on one that is not.
fn rank_ties<T>(v: &mut [T], score: impl Fn(&T) -> Score, tie: impl Fn(&T, &T) -> std::cmp::Ordering) {
    v.sort_by(|a, b| best_first(score(a), score(b)).then_with(|| tie(a, b)));
    let mut at = 0;
    while at < v.len() {
        let head = score(&v[at]);
        let end = (at + 1..v.len()).find(|&j| !one_build(score(&v[j]), head)).unwrap_or(v.len());
        v[at..end].sort_by(&tie);
        at = end;
    }
}

fn best_first(a: Score, b: Score) -> std::cmp::Ordering {
    if better(a, b) {
        std::cmp::Ordering::Less
    } else if better(b, a) {
        std::cmp::Ordering::Greater
    } else {
        std::cmp::Ordering::Equal
    }
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

    /// A SHAPE AND A CARD, scored by a table: [shape 0–1, card 0–2]. On a build
    /// with no card yet shape 0 is better (5 against 4), so a fill that chooses
    /// the shape first settles at (0, 0) = 10, where either change alone is
    /// worse; chosen after its card, the shape reaches (1, 2) = 12.
    struct ShapeAndCard;
    impl QuickSpace for ShapeAndCard {
        type Build = [Option<u8>; 2];
        fn positions(&self, _: &Self::Build) -> Vec<Position> {
            vec![Position { kind: "s", idx: 0 }, Position { kind: "c", idx: 1 }]
        }
        fn empty(&self, b: &Self::Build) -> Vec<Position> {
            self.positions(b).into_iter().filter(|p| b[p.idx].is_none()).collect()
        }
        fn candidates(&self, b: &Self::Build, p: Position) -> Vec<Self::Build> {
            (0..if p.idx == 0 { 2 } else { 3 })
                .filter(|d| b[p.idx] != Some(*d))
                .map(|d| {
                    let mut n = *b;
                    n[p.idx] = Some(d);
                    n
                })
                .collect()
        }
        fn legal(&self, _: &Self::Build) -> bool {
            true
        }
        fn score(&self, bs: &[Self::Build]) -> Vec<Score> {
            let table = |b: &Self::Build| match *b {
                [Some(s), None] => [5.0, 4.0][s as usize],
                [None, Some(c)] => [1.0, 1.0, 3.0][c as usize],
                [Some(s), Some(c)] => [[10.0, 6.0, 7.0], [7.0, 6.0, 12.0]][s as usize][c as usize],
                [None, None] => 0.0,
            };
            bs.iter().map(|b| Some((table(b), 0.0))).collect()
        }
        fn key(&self, b: &Self::Build) -> String {
            format!("{b:?}")
        }
        fn coupled(&self, _: &Self::Build) -> Vec<Position> {
            vec![Position { kind: "s", idx: 0 }]
        }
    }

    #[test]
    fn a_start_is_filled_with_its_shape_last_too() {
        let starts = vec![QuickStart { build: [None, None], fixed: vec![], unnamed: vec![] }];
        let cfg = QuickConfig { max_evals: 0, stop: Vec::new() };
        let QuickRun { answers, progress, .. } = quick_descent(&ShapeAndCard, &starts, &cfg);
        assert_eq!(answers[0].build, [Some(1), Some(2)], "the shape chosen for its card");
        assert_eq!(answers[0].starts, vec![0], "both fills are one start");
        assert_eq!(progress.len(), 1, "…and one lane");
        // A START WITH NOTHING TO FILL is descended once: the same answer, no twin.
        let named = vec![QuickStart { build: [Some(0), Some(0)], fixed: vec![], unnamed: vec![] }];
        let QuickRun { answers, .. } = quick_descent(&ShapeAndCard, &named, &cfg);
        assert_eq!(answers.len(), 1);
    }

    /// TWO SHAPES THAT PAY ONLY TOGETHER: from (0, 0) = 5 either change alone is
    /// 4, both at once 9.
    struct TwoShapes;
    impl QuickSpace for TwoShapes {
        type Build = [Option<u8>; 2];
        fn positions(&self, _: &Self::Build) -> Vec<Position> {
            (0..2).map(|idx| Position { kind: "s", idx }).collect()
        }
        fn empty(&self, b: &Self::Build) -> Vec<Position> {
            self.positions(b).into_iter().filter(|p| b[p.idx].is_none()).collect()
        }
        fn candidates(&self, b: &Self::Build, p: Position) -> Vec<Self::Build> {
            ShapeAndCard.candidates(b, Position { kind: "s", idx: p.idx }).into_iter().filter(|n| n[p.idx] < Some(2)).collect()
        }
        fn legal(&self, _: &Self::Build) -> bool {
            true
        }
        fn score(&self, bs: &[Self::Build]) -> Vec<Score> {
            bs.iter().map(|b| Some(([[5.0, 4.0], [4.0, 9.0]][b[0].unwrap_or(0) as usize][b[1].unwrap_or(0) as usize], 0.0))).collect()
        }
        fn key(&self, b: &Self::Build) -> String {
            format!("{b:?}")
        }
        fn coupled(&self, b: &Self::Build) -> Vec<Position> {
            self.positions(b)
        }
    }

    #[test]
    fn two_coupled_positions_change_together_when_no_single_change_wins() {
        let starts = vec![QuickStart { build: [Some(0), Some(0)], fixed: vec![], unnamed: vec![] }];
        let cfg = QuickConfig { max_evals: 0, stop: Vec::new() };
        let QuickRun { answers, .. } = quick_descent(&TwoShapes, &starts, &cfg);
        assert_eq!(answers[0].build, [Some(1), Some(1)]);
    }

    #[test]
    fn a_descent_takes_the_best_legal_and_merges_what_settles_together() {
        let starts = vec![
            QuickStart { build: [None, None, None], fixed: vec![], unnamed: vec![] },
            QuickStart { build: [Some(1), Some(2), Some(3)], fixed: vec![], unnamed: vec![] },
            QuickStart { build: [Some(0), None, None], fixed: vec![Position { kind: "d", idx: 0 }], unnamed: vec![] },
        ];
        let cfg = QuickConfig { max_evals: 0, stop: Vec::new() };
        let QuickRun { answers, failures, pool, .. } = quick_descent(&Digits, &starts, &cfg);
        assert!(failures.is_empty());
        assert_eq!(pool[0].0, answers[0].build, "the pool's best is the best answer");
        assert!(pool.iter().all(|(b, _)| b.iter().all(Option::is_some)), "the fill's half-empty builds are not in it");
        assert_eq!(pool[1].1, Some((25.0, 0.0)), "a runner-up one change away is, once per score");
        assert_eq!(answers[0].build, [Some(8), Some(9), Some(9)]);
        assert_eq!(answers[0].starts, vec![0, 1], "two starts settle on one build");
        assert_eq!(answers[1].build, [Some(0), Some(9), Some(9)], "a fixed position never moves");
    }

    /// Two slots under a capacity of 10, each digit costing itself: slot 0
    /// moves no number, slot 1 scores its digit. The fill seats a 9 in slot 0
    /// by preference, so slot 1's 9 fits only once slot 0 is lowered — and the
    /// answer keeps the costliest slot 0 that still fits.
    struct Capacity;
    impl QuickSpace for Capacity {
        type Build = [Option<u8>; 2];
        fn positions(&self, _: &Self::Build) -> Vec<Position> {
            (0..2).map(|idx| Position { kind: "d", idx }).collect()
        }
        fn empty(&self, b: &Self::Build) -> Vec<Position> {
            (0..2).filter(|&i| b[i].is_none()).map(|idx| Position { kind: "d", idx }).collect()
        }
        fn candidates(&self, b: &Self::Build, p: Position) -> Vec<Self::Build> {
            Digits.candidates(&[b[0], b[1], None], p).into_iter().map(|n| [n[0], n[1]]).collect()
        }
        fn legal(&self, b: &Self::Build) -> bool {
            b.iter().flatten().map(|&d| u32::from(d)).sum::<u32>() <= 10
        }
        fn score(&self, bs: &[Self::Build]) -> Vec<Score> {
            bs.iter().map(|b| Some((f64::from(b[1].unwrap_or(0)), 0.0))).collect()
        }
        fn key(&self, b: &Self::Build) -> String {
            format!("{b:?}")
        }
        fn prefer(&self, a: &Self::Build, b: &Self::Build) -> std::cmp::Ordering {
            let cost = |x: &Self::Build| x.iter().flatten().map(|&d| u32::from(d)).sum::<u32>();
            cost(b).cmp(&cost(a))
        }
        fn repair(&self, b: &Self::Build, free: &[Position]) -> Option<Self::Build> {
            let p = free.first()?;
            (0..b[p.idx]?).rev().map(|d| {
                let mut n = *b;
                n[p.idx] = Some(d);
                n
            }).find(|n| self.legal(n))
        }
    }

    #[test]
    fn a_tie_keeps_the_preferred_and_a_free_part_is_lowered_to_fit() {
        let starts = vec![QuickStart { build: [None, None], fixed: vec![], unnamed: vec![] }];
        let cfg = QuickConfig { max_evals: 0, stop: Vec::new() };
        let QuickRun { answers, .. } = quick_descent(&Capacity, &starts, &cfg);
        assert_eq!(answers[0].build, [Some(1), Some(9)]);
    }
}
