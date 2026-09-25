// SPDX-License-Identifier: AGPL-3.0-or-later
//! COORDINATE DESCENT — the search that starts from the element rules.
//!
//! [`crate::search::search`] samples the subset space and climbs from what it
//! drew, and every subset it looks at is paid for under EVERY arcane: the axes
//! multiply. Here they add. One build is held, and each position — a mod slot,
//! an empty slot, the arcane — is swept against every legal alternative with
//! the rest held still, so one sweep costs the SUM of the option counts and the
//! whole pool stays searchable.
//!
//! 1. STARTS: the player's own, each a partial build. Without any, one per
//!    primary element, one card each ([`seeds`]). A start is where the descent
//!    begins, not a constraint: it may swap any of it out (a card that must
//!    stay is the scope's `fixed` mark).
//! 2. FILL: from the start, add the best card until the build is full.
//! 3. SWEEP, arcane → each mod → an empty slot → the variant (evolution set,
//!    mode, valence). Any accepted change restarts at the arcane, because a
//!    change anywhere moves what every other position wants. A full sweep with
//!    no change is that start's answer.
//!
//! Every build is scored on the SAME random stream, so two builds are compared
//! on paired runs and a score is a deterministic function of the build. That
//! is also what makes the loop terminate: each accepted move strictly raises a
//! fixed score over a finite set.
//!
//! What it returns is the same as the sampler's — every job it scored, best
//! first — so the funnel, the replay and the grader cannot tell them apart.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use wfsim_engine::fight::Summary;
use wfsim_engine::model::{ModDef, ModEffect};
use wfsim_engine::rules::damage::DamageType;

use crate::search::{key_of, push_elite, snapshot, Expand, SearchConfig, SearchStats};
use crate::space::SubsetSpace;
use crate::{evaluate, job_seed, Candidate, FunnelState, Scenario, ScreenedJob, Scored};

const PRIMARY: [DamageType; 4] =
    [DamageType::Cold, DamageType::Electricity, DamageType::Heat, DamageType::Toxin];

/// A build's score: the best of the candidates its point expands to, and the
/// variant that candidate fires. `None` = it expands to nothing (Forma cannot
/// fit it, or the variant cannot equip a card).
type Score = Option<(f64, f64, u32)>;

fn better(a: Score, b: Score) -> bool {
    match (a, b) {
        (Some(_), None) => true,
        (Some((ak, ae, _)), Some((bk, be, _))) => {
            ak.total_cmp(&bk).then(ae.total_cmp(&be)).is_gt()
        }
        _ => false,
    }
}

/// One build under evaluation: a canonical (ascending) subset, an arcane, and
/// the variant — `None` only while a start is being scored, which asks every
/// variant at once so the fill begins under the one that suits it.
#[derive(Clone)]
struct Point {
    subset: Vec<usize>,
    ai: usize,
    vi: Option<u32>,
}

impl Point {
    fn key(&self) -> (u64, usize, Option<u32>) {
        (key_of(&self.subset), self.ai, self.vi)
    }
}

/// Where one descent begins, and what it may never change.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Start {
    /// Pool indices the start holds.
    pub mods: Vec<usize>,
    /// Pool indices the descent never swaps out — a pin for THIS start only,
    /// where the scope's `fixed` mark pins every start.
    pub locked: Vec<usize>,
    /// Index into the arcane list to begin under; `None` = the one the start
    /// scores best with.
    pub arcane: Option<usize>,
    /// The arcane never changes.
    pub lock_arcane: bool,
}

struct Run<'a> {
    space: &'a SubsetSpace,
    /// The current start's pins.
    locked: Vec<usize>,
    lock_arcane: bool,
    expand: &'a Expand<'a>,
    arcanes: &'a [wfsim_engine::data::arcanes::ArcaneFx],
    scenario: &'a Scenario,
    cfg: &'a SearchConfig,
    state: Option<&'a FunnelState>,
    on_board: Option<&'a crate::ScreenBoardFn<'a>>,
    cache: HashMap<(u64, usize, Option<u32>), Score>,
    top: BinaryHeap<Reverse<Scored>>,
    stats: SearchStats,
    seq: usize,
}

impl Run<'_> {
    fn out_of_budget(&self) -> bool {
        (self.cfg.max_evals > 0 && self.stats.evals >= self.cfg.max_evals)
            || self.state.is_some_and(|s| {
                s.cancel.load(Ordering::Relaxed) || s.stop_enumeration.load(Ordering::Relaxed)
            })
    }

    /// Score every point, evaluating only the ones not seen before. `None` =
    /// the budget ran out before the batch could run; the caller stops.
    fn score(&mut self, points: &[Point]) -> Option<Vec<Score>> {
        let fresh: Vec<&Point> = {
            let mut seen = std::collections::HashSet::new();
            points
                .iter()
                .filter(|p| {
                    let k = p.key();
                    !self.cache.contains_key(&k) && seen.insert(k)
                })
                .collect()
        };
        if !fresh.is_empty() {
            if self.out_of_budget() {
                return None;
            }
            let (expand, arcanes, scenario, cfg, state) =
                (self.expand, self.arcanes, self.scenario, self.cfg, self.state);
            let results = par_map(&fresh, |p| eval_point(p, expand, arcanes, scenario, cfg, state));
            for (p, res) in fresh.iter().zip(results) {
                self.stats.subsets += 1;
                self.stats.candidates += res.len() as u64;
                self.stats.evals += res.len() as u64;
                let mut best: Score = None;
                for (cand, s) in res {
                    let sc =
                        Some((s.mean_kill_progress.max(0.0), s.mean_effective_damage, cand.variant));
                    if better(sc, best) {
                        best = sc;
                    }
                    push_elite(
                        &mut self.top,
                        Scored {
                            kp: s.mean_kill_progress.max(0.0),
                            eff: s.mean_effective_damage,
                            seq: self.seq,
                            ai: p.ai,
                            cand,
                            summary: s,
                        },
                        self.cfg.keep,
                    );
                    self.seq += 1;
                }
                self.cache.insert(p.key(), best);
            }
            self.stats.sampled = u128::from(self.stats.subsets);
            if let Some(st) = self.state {
                st.enumerated.store(self.stats.subsets, Ordering::Relaxed);
                st.sims_done.store(self.stats.evals, Ordering::Relaxed);
            }
            if let Some(b) = self.on_board {
                b(&snapshot(&self.top, crate::BOARD_TOP));
            }
        }
        Some(points.iter().map(|p| self.cache[&p.key()]).collect())
    }

    /// The mods a move may take out: neither required by the scope nor locked
    /// by the start.
    fn swappable(&self, subset: &[usize]) -> Vec<usize> {
        subset
            .iter()
            .copied()
            .filter(|i| !self.space.required().contains(i) && !self.locked.contains(i))
            .collect()
    }

    /// The arcanes a move may switch to — none while the start locks it.
    fn other_arcanes(&self, ai: usize) -> Vec<usize> {
        if self.lock_arcane {
            return Vec::new();
        }
        (0..self.arcanes.len()).filter(|&a| a != ai).collect()
    }

    /// The best of `alts`, if it beats `cur`.
    fn best_move(&mut self, cur: Score, alts: Vec<Point>) -> Option<Option<(Point, Score)>> {
        if alts.is_empty() {
            return Some(None);
        }
        let scores = self.score(&alts)?;
        let mut best: Option<(Point, Score)> = None;
        for (p, s) in alts.into_iter().zip(scores) {
            if best.as_ref().is_none_or(|(_, b)| better(s, *b)) {
                best = Some((p, s));
            }
        }
        Some(best.filter(|(_, s)| better(*s, cur)))
    }

    /// Fill a start to size, then sweep it to a fixed point. `false` = the
    /// budget ran out first; everything scored so far is already in `top`.
    fn descend(&mut self, start: Start) -> bool {
        let space = self.space;
        let sizes = space.sizes();
        self.locked = start.locked.clone();
        self.lock_arcane = start.lock_arcane && start.arcane.is_some();
        let seed = start.mods;
        let with = |sub: &[usize], add: usize, drop: Option<usize>| -> Vec<usize> {
            let mut v: Vec<usize> = sub.iter().copied().filter(|&x| Some(x) != drop).collect();
            v.push(add);
            v.sort_unstable();
            v
        };

        // The arcane and variant the start wants. The sweep revisits both once
        // the build is full; this only keeps the fill from being chosen under
        // a bad pair.
        let tried: Vec<usize> = match start.arcane {
            Some(a) => vec![a],
            None => (0..self.arcanes.len()).collect(),
        };
        let at_start: Vec<Point> =
            tried.iter().map(|&ai| Point { subset: seed.clone(), ai, vi: None }).collect();
        let Some(scores) = self.score(&at_start) else { return false };
        let mut cur = Point { subset: seed, ai: tried[0], vi: None };
        let mut cur_score = None;
        for (&ai, s) in tried.iter().zip(scores) {
            if better(s, cur_score) {
                cur_score = s;
                cur.ai = ai;
            }
        }
        let Some((_, _, vi)) = cur_score else { return true };
        cur.vi = Some(vi);
        let Some(s) = self.score(std::slice::from_ref(&cur)) else { return false };
        cur_score = s[0];

        // FILL. Below the minimum a card is added even if it costs; above it,
        // only while one pays.
        while cur.subset.len() < *sizes.end() {
            let alts: Vec<Point> = space
                .choosable()
                .iter()
                .filter(|i| !cur.subset.contains(i))
                .map(|&i| with(&cur.subset, i, None))
                .filter(|v| space.legal_upto(v))
                .map(|subset| Point { subset, ai: cur.ai, vi: cur.vi })
                .collect();
            let floor = if cur.subset.len() < *sizes.start() { None } else { cur_score };
            match self.best_move(floor, alts) {
                None => return false,
                Some(Some((p, s))) => {
                    cur = p;
                    cur_score = s;
                }
                Some(None) => break,
            }
        }
        if !space.legal(&cur.subset) {
            return true; // the pool cannot reach the minimum from this seed
        }

        // SWEEP. Positions in a fixed order — the arcane, each mod the build
        // holds, an empty slot, the variant — restarting at the first after
        // any accepted move.
        'sweep: loop {
            let held = self.swappable(&cur.subset);
            let mut positions: Vec<Vec<Point>> = vec![self
                .other_arcanes(cur.ai)
                .into_iter()
                .map(|ai| Point { subset: cur.subset.clone(), ai, vi: cur.vi })
                .collect()];
            for &m in &held {
                let mut alts: Vec<Point> = space
                    .choosable()
                    .iter()
                    .filter(|i| !cur.subset.contains(i))
                    .map(|&i| with(&cur.subset, i, Some(m)))
                    .filter(|v| space.legal(v))
                    .map(|subset| Point { subset, ai: cur.ai, vi: cur.vi })
                    .collect();
                let dropped: Vec<usize> = cur.subset.iter().copied().filter(|&x| x != m).collect();
                if space.legal(&dropped) {
                    alts.push(Point { subset: dropped, ai: cur.ai, vi: cur.vi });
                }
                positions.push(alts);
            }
            if cur.subset.len() < *sizes.end() {
                positions.push(
                    space
                        .choosable()
                        .iter()
                        .filter(|i| !cur.subset.contains(i))
                        .map(|&i| with(&cur.subset, i, None))
                        .filter(|v| space.legal(v))
                        .map(|subset| Point { subset, ai: cur.ai, vi: cur.vi })
                        .collect(),
                );
            }
            let mut variants: Vec<u32> =
                (self.expand)(&cur.subset).iter().map(|c| c.variant).collect();
            variants.sort_unstable();
            variants.dedup();
            positions.push(
                variants
                    .into_iter()
                    .filter(|&v| Some(v) != cur.vi)
                    .map(|v| Point { subset: cur.subset.clone(), ai: cur.ai, vi: Some(v) })
                    .collect(),
            );
            for alts in positions {
                match self.best_move(cur_score, alts) {
                    None => return false,
                    Some(Some((p, s))) => {
                        cur = p;
                        cur_score = s;
                        continue 'sweep;
                    }
                    Some(None) => {}
                }
            }
            // Fixed point at width 1. Wider moves are tried only here, one
            // width at a time, and any gain sends the sweep back to width 1.
            for w in 2..=self.cfg.swap_width.max(1) as usize {
                match self.wide(&cur, cur_score, w) {
                    None => return false,
                    Some(Some((p, s))) => {
                        cur = p;
                        cur_score = s;
                        continue 'sweep;
                    }
                    Some(None) => {}
                }
            }
            return true;
        }
    }

    /// Every move that changes exactly `w` positions at once — `m` mods
    /// replaced, plus the arcane and/or the variant — scored in chunks; the
    /// first chunk holding a build better than `cur` returns its best. A
    /// width-1 sweep cannot cross a valley two changes wide (a card that pays
    /// only under another evolution); this can, at C(held, m)·C(free, m) cost.
    fn wide(&mut self, cur: &Point, cur_score: Score, w: usize) -> Option<Option<(Point, Score)>> {
        const CHUNK: usize = 1024;
        let space = self.space;
        let held = self.swappable(&cur.subset);
        let free: Vec<usize> =
            space.choosable().iter().copied().filter(|i| !cur.subset.contains(i)).collect();
        let arcs = self.other_arcanes(cur.ai);
        let mut vars: Vec<Option<u32>> =
            (self.expand)(&cur.subset).iter().map(|c| Some(c.variant)).collect();
        vars.sort_unstable();
        vars.dedup();
        vars.retain(|&v| v != cur.vi);

        let mut buf: Vec<Point> = Vec::new();
        for change_arcane in [false, true] {
            for change_variant in [false, true] {
                let fixed = usize::from(change_arcane) + usize::from(change_variant);
                if (change_arcane && arcs.is_empty()) || (change_variant && vars.is_empty()) {
                    continue;
                }
                let Some(m) = w.checked_sub(fixed) else { continue };
                if m > held.len() || m > free.len() || (m == 0 && fixed < 2) {
                    continue;
                }
                let ais = if change_arcane { arcs.clone() } else { vec![cur.ai] };
                let vis = if change_variant { vars.clone() } else { vec![cur.vi] };
                let ins = combos(&free, m);
                for out in combos(&held, m) {
                    for inn in &ins {
                        let mut subset: Vec<usize> =
                            cur.subset.iter().copied().filter(|x| !out.contains(x)).collect();
                        subset.extend_from_slice(inn);
                        subset.sort_unstable();
                        if !space.legal(&subset) {
                            continue;
                        }
                        for &ai in &ais {
                            for &vi in &vis {
                                buf.push(Point { subset: subset.clone(), ai, vi });
                            }
                        }
                        if buf.len() >= CHUNK {
                            if let Some(hit) = self.best_move(cur_score, std::mem::take(&mut buf))? {
                                return Some(Some(hit));
                            }
                        }
                    }
                }
            }
        }
        self.best_move(cur_score, buf)
    }
}

/// Every `k`-element subset of `items`, in lexicographic order.
fn combos(items: &[usize], k: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut pick = Vec::with_capacity(k);
    fn rec(items: &[usize], k: usize, from: usize, pick: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if pick.len() == k {
            out.push(pick.clone());
            return;
        }
        for i in from..items.len() {
            if items.len() - i < k - pick.len() {
                break;
            }
            pick.push(items[i]);
            rec(items, k, i + 1, pick, out);
            pick.pop();
        }
    }
    rec(items, k, 0, &mut pick, &mut out);
    out
}

/// The default starts: one per primary element the scope can field, holding
/// its strongest carrier (ties to pool order). The fill picks the partner, so
/// four starts reach every pair that six pair-starts would, for less: graded,
/// Verglas 1,092 evals against 1,535 and Boar Prime 601 against 971, same
/// answer. ONE start holding all four elements is the wrong shape — it stuck
/// at 49% regret, because shedding an element costs its combination first.
/// Required mods ride in every start; an element one of them carries is not
/// added twice.
pub fn seeds(space: &SubsetSpace, pool: &[ModDef]) -> Vec<Start> {
    let strength = |i: usize, t: DamageType| -> Option<f64> {
        pool[i].effects.iter().find_map(|e| match e {
            ModEffect::Element(et, v) if *et == t => Some(*v),
            _ => None,
        })
    };
    let carrier = |t: DamageType| -> Option<usize> {
        if let Some(&r) = space.required().iter().find(|&&r| strength(r, t).is_some()) {
            return Some(r);
        }
        let mut best: Option<(usize, f64)> = None;
        for &i in space.choosable() {
            if let Some(v) = strength(i, t) {
                if best.is_none_or(|(_, b)| v > b) {
                    best = Some((i, v));
                }
            }
        }
        best.map(|(i, _)| i)
    };
    let mut out: Vec<Vec<usize>> = Vec::new();
    for e in PRIMARY {
        let Some(x) = carrier(e) else { continue };
        let mut v = space.required().to_vec();
        if !v.contains(&x) {
            v.push(x);
        }
        v.sort_unstable();
        if space.legal_upto(&v) && !out.contains(&v) {
            out.push(v);
        }
    }
    if out.is_empty() {
        // No element in the scope: start from what is required.
        let mut v = space.required().to_vec();
        v.sort_unstable();
        out.push(v);
    }
    out.into_iter().map(|mods| Start { mods, ..Default::default() }).collect()
}

/// The player's starts, canonical: required and locked mods added, and a
/// start outside the scope or colliding in a family dropped rather than
/// guessed at.
pub fn starts_in(space: &SubsetSpace, starts: &[Start]) -> Vec<Start> {
    let mut out: Vec<Start> = Vec::new();
    for s in starts {
        let mut v = space.required().to_vec();
        for &i in s.mods.iter().chain(&s.locked) {
            if !v.contains(&i) {
                v.push(i);
            }
        }
        v.sort_unstable();
        let mut locked = s.locked.clone();
        locked.sort_unstable();
        locked.dedup();
        let start = Start { mods: v, locked, ..s.clone() };
        if space.legal_upto(&start.mods) && !out.contains(&start) {
            out.push(start);
        }
    }
    out
}

/// Run the descent from every start this shard owns (`index % shards ==
/// shard`) — the player's `starts`, or one start per element when there are
/// none. Returns every scored job, best first, and what it spent.
#[allow(clippy::too_many_arguments)]
pub fn descent(
    space: &SubsetSpace,
    pool: &[ModDef],
    starts: &[Start],
    expand: &Expand<'_>,
    arcanes: &[wfsim_engine::data::arcanes::ArcaneFx],
    scenario: &Scenario,
    cfg: &SearchConfig,
    state: Option<&FunnelState>,
    on_board: Option<&crate::ScreenBoardFn<'_>>,
) -> (Vec<ScreenedJob>, SearchStats) {
    let mut run = Run {
        space,
        locked: Vec::new(),
        lock_arcane: false,
        expand,
        arcanes,
        scenario,
        cfg,
        state,
        on_board,
        cache: HashMap::new(),
        top: BinaryHeap::new(),
        stats: SearchStats { space: space.len(), ..Default::default() },
        seq: 0,
    };
    let shards = cfg.shards.max(1) as usize;
    let shard = cfg.shard as usize % shards;
    let starts = if starts.is_empty() { seeds(space, pool) } else { starts_in(space, starts) };
    for (i, seed) in starts.into_iter().enumerate() {
        if i % shards != shard {
            continue;
        }
        run.stats.starts += 1;
        if run.out_of_budget() || !run.descend(seed) {
            run.stats.cut = true;
        }
    }
    run.stats.exhaustive = false;
    let mut out: Vec<Scored> = run.top.into_iter().map(|r| r.0).collect();
    out.sort_by(|a, b| b.cmp(a));
    (
        out.into_iter()
            .map(|s| ScreenedJob { cand: s.cand, ai: s.ai, summary: s.summary })
            .collect(),
        run.stats,
    )
}

/// Every candidate one point expands to, scored under its arcane. The seed is
/// the candidate's position within its subset and nothing else, so every build
/// runs on the same random stream as every other.
/// Score candidates on ONE random stream — every build gets the same seed — so
/// any two are compared on paired runs, across threads natively.
pub fn evaluate_paired(
    jobs: &[(&Candidate, usize)],
    arcanes: &[wfsim_engine::data::arcanes::ArcaneFx],
    scenario: &Scenario,
    runs: u32,
    seed: u64,
) -> Vec<Summary> {
    let seed = job_seed(seed, 0, 0);
    par_map(jobs, |(c, ai)| evaluate(c, &arcanes[*ai], scenario, runs, seed))
}

fn eval_point(
    p: &Point,
    expand: &Expand<'_>,
    arcanes: &[wfsim_engine::data::arcanes::ArcaneFx],
    scenario: &Scenario,
    cfg: &SearchConfig,
    state: Option<&FunnelState>,
) -> Vec<(Arc<Candidate>, Summary)> {
    let mut out = Vec::new();
    let cands = expand(&p.subset).into_iter().filter(|c| p.vi.is_none_or(|v| c.variant == v));
    for (ci, c) in cands.enumerate() {
        if state.is_some_and(|s| s.cancel.load(Ordering::Relaxed)) {
            break;
        }
        let s = evaluate(&c, &arcanes[p.ai], scenario, cfg.runs, job_seed(cfg.seed, ci, 0));
        out.push((Arc::new(c), s));
    }
    out
}

#[cfg(not(target_arch = "wasm32"))]
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = crate::worker_threads().min(items.len()).max(1);
    let chunk = items.len().div_ceil(threads).max(1);
    let mut parts: Vec<Vec<R>> = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|part| {
                let f = &f;
                scope.spawn(move || {
                    crate::deprioritize_current_thread();
                    part.iter().map(f).collect::<Vec<R>>()
                })
            })
            .collect();
        parts = handles.into_iter().map(|h| h.join().expect("descent worker")).collect();
    });
    parts.into_iter().flatten().collect()
}

#[cfg(target_arch = "wasm32")]
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    items
        .iter()
        .map(|x| {
            let r = f(x);
            crate::tick();
            r
        })
        .collect()
}
