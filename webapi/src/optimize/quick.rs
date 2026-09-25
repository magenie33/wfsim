// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE PLANNED OPTIMIZER'S SPACE over one plan's tables (docs/OPTIMIZER.md,
//! "PLANNED — the descent is the quick calc, repeated").
//!
//! A build is indices into the plan: eight mod slots, an exilus option, an
//! arcane set, an evolution set, a mode and a valence. That makes "inside the
//! scope" a lookup — a candidate the quick calc offers is kept when it maps onto
//! the plan's tables and dropped when it does not. The candidates themselves are
//! `/api/candidates`'; a build fits when `rebuild_candidate` can plan its Forma;
//! it scores through the funnel's own `evaluate`, the fight the replay runs.

use serde_json::{json, Value};
use wfsim_engine::model::ModDef;
use wfsim_optimizer::quick::{Position, QuickSpace, QuickStart, Score};
use wfsim_optimizer::Candidate;

/// A build as indices into the plan's tables.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct QBuild {
    pub(crate) mods: [Option<usize>; 8],
    pub(crate) exilus: usize,
    pub(crate) arcane: usize,
    pub(crate) evo: usize,
    pub(crate) mode: usize,
    pub(crate) val: usize,
}

pub(crate) struct QuickCtx<'a> {
    pub(crate) weapon_id: &'a str,
    pub(crate) pool: &'a [ModDef],
    /// Per pool index: not forbidden by the scope.
    pub(crate) usable: Vec<bool>,
    pub(crate) required: Vec<usize>,
    pub(crate) forms: &'a [(wfsim_engine::model::WeaponBase, Option<wfsim_engine::model::WeaponBase>)],
    pub(crate) variants: &'a [(usize, usize, usize)],
    pub(crate) variant_forbids: &'a [Vec<bool>],
    pub(crate) evo_sets: &'a [Vec<String>],
    pub(crate) mode_ids: Vec<String>,
    pub(crate) valences: &'a [String],
    pub(crate) valence_bonus: f64,
    pub(crate) arcanes: &'a [wfsim_engine::data::arcanes::ArcaneFx],
    pub(crate) arcane_sets: &'a [Vec<String>],
    pub(crate) exilus_defs: &'a [Option<ModDef>],
    pub(crate) exilus_refs: &'a [Option<&'a ModDef>],
    pub(crate) innate: &'a [Option<wfsim_engine::rules::capacity::Polarity>],
    pub(crate) cap: u32,
    pub(crate) scenario: &'a wfsim_optimizer::Scenario,
    /// The request, for what a candidate list needs that the plan does not
    /// hold: the visitor's rivens, the pinned assembly and stance.
    pub(crate) request: &'a Value,
    pub(crate) runs: u32,
    pub(crate) seed: u64,
}

fn split_rank(s: &str) -> (&str, Option<u64>) {
    match s.rsplit_once('@') {
        Some((c, r)) if r.bytes().all(|b| b.is_ascii_digit()) => (c, r.parse().ok()),
        _ => (s, None),
    }
}

impl QuickCtx<'_> {
    fn variant(&self, b: &QBuild) -> Option<usize> {
        self.variants.iter().position(|&(m, e, l)| m == b.mode && e == b.evo && l == b.val)
    }

    /// The build as the funnel scores it, or `None` when it cannot exist:
    /// no such variant, a card its evolutions refuse, or Forma cannot fit it.
    pub(crate) fn materialize(&self, b: &QBuild) -> Option<Candidate> {
        let vi = self.variant(b)?;
        let ordered: Vec<usize> = b.mods.iter().flatten().copied().collect();
        let forbids = &self.variant_forbids[self.variants[vi].1];
        if ordered.iter().any(|&i| forbids[i]) {
            return None;
        }
        let (base, second) = &self.forms[vi];
        wfsim_optimizer::rebuild_candidate(
            self.pool,
            base,
            second.as_ref(),
            self.innate,
            self.cap,
            &self.scenario.arena.tenno,
            self.scenario.policy,
            &ordered,
            vi as u32,
            b.exilus as u32,
            self.exilus_refs,
        )
    }

    fn has_required(&self, b: &QBuild) -> bool {
        self.required.iter().all(|r| b.mods.contains(&Some(*r)))
    }

    /// The build as `/api/candidates` reads it.
    fn candidate_request(&self, b: &QBuild, p: Position) -> Value {
        let mut slots: Vec<Value> = b.mods.iter().map(|m| m.map_or(Value::Null, |i| json!(self.pool[i].id))).collect();
        slots.push(self.exilus_defs[b.exilus].as_ref().map_or(Value::Null, |m| json!(m.id)));
        slots.push(match self.request.get("stance").and_then(Value::as_str) {
            Some(s) if !s.is_empty() => json!(s),
            _ => Value::Null,
        });
        let mut evo_sel = serde_json::Map::new();
        for id in &self.evo_sets[b.evo] {
            if let Some(e) = wfsim_engine::data::evolutions::get(id) {
                evo_sel.insert(e.tier.to_string(), json!(id));
            }
        }
        let set = &self.arcane_sets[b.arcane];
        let (ids, ranks): (Vec<Value>, Vec<Value>) = set
            .iter()
            .map(|s| {
                let (id, r) = split_rank(s);
                (json!(id), r.map_or(Value::Null, |r| json!(r)))
            })
            .unzip();
        json!({
            "weapon": self.weapon_id,
            "slots": slots,
            "evo_sel": evo_sel,
            "arcane": ids,
            "arcane_rank": ranks,
            "mode": self.mode_ids.get(b.mode),
            "valence_element": self.valences.get(b.val).cloned().unwrap_or_default(),
            "valence_bonus": self.valence_bonus,
            "assembly": self.request.get("assembly").cloned().unwrap_or(Value::Null),
            "rivens": self.request.get("rivens").cloned().unwrap_or(Value::Null),
            "axis": { "kind": p.kind, "idx": p.idx },
        })
    }

    /// The seat list a candidate names, spelled the way the plan's arcane sets
    /// are: `id@rank` below the card's max, `none` for an empty seat.
    fn arcane_set_of(&self, ids: &[Value], ranks: &[Value]) -> Option<usize> {
        let spelled: Vec<String> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let id = id.as_str().unwrap_or("none");
                let rank = ranks.get(i).and_then(Value::as_u64);
                match (wfsim_engine::data::arcanes::secondary(id), rank) {
                    (Some(a), Some(r)) if r < u64::from(a.max_rank) => format!("{id}@{r}"),
                    _ => id.to_string(),
                }
            })
            .collect();
        self.arcane_sets.iter().position(|s| *s == spelled)
    }

    /// The plan's default for every axis a start does not name.
    fn blank(&self) -> QBuild {
        QBuild {
            mods: [None; 8],
            exilus: self.exilus_defs.iter().position(Option::is_none).unwrap_or(0),
            arcane: 0,
            evo: 0,
            mode: 0,
            val: 0,
        }
    }

    /// The starts the plan was given, or one per primary element — each a
    /// real build holding that element's strongest card and nothing else,
    /// which is what a player gets by making those builds by hand.
    pub(crate) fn starts(
        &self,
        given: &[wfsim_optimizer::descent::Start],
        seeds: Vec<wfsim_optimizer::descent::Start>,
    ) -> Vec<QuickStart<QBuild>> {
        let list = if given.is_empty() { seeds } else { given.to_vec() };
        let arcane_seats: Vec<Position> = (0..self.arcane_sets.first().map_or(0, Vec::len))
            .map(|idx| Position { kind: "arcane", idx })
            .collect();
        list.into_iter()
            .map(|s| {
                let mut b = self.blank();
                let mut order: Vec<usize> = self.required.clone();
                order.extend(s.mods.iter().copied().filter(|m| !self.required.contains(m)));
                let mut fixed = Vec::new();
                for (slot, m) in order.into_iter().take(8).enumerate() {
                    b.mods[slot] = Some(m);
                    if self.required.contains(&m) || s.locked.contains(&m) {
                        fixed.push(Position { kind: "mods", idx: slot });
                    }
                }
                if let Some(a) = s.arcane {
                    b.arcane = a;
                    if s.lock_arcane {
                        fixed.extend(arcane_seats.iter().copied());
                    }
                }
                QuickStart { build: b, fixed }
            })
            .collect()
    }
}

impl QuickSpace for QuickCtx<'_> {
    type Build = QBuild;

    fn positions(&self, _: &QBuild) -> Vec<Position> {
        let mut out: Vec<Position> = Vec::new();
        if self.arcane_sets.len() > 1 {
            out.extend((0..self.arcane_sets[0].len()).map(|idx| Position { kind: "arcane", idx }));
        }
        out.extend((0..8).map(|idx| Position { kind: "mods", idx }));
        if self.exilus_defs.len() > 1 {
            out.push(Position { kind: "mods", idx: 8 });
        }
        if self.evo_sets.len() > 1 {
            out.push(Position { kind: "evo", idx: 0 });
        }
        if self.mode_ids.len() > 1 {
            out.push(Position { kind: "mode", idx: 0 });
        }
        if self.valences.len() > 1 {
            out.push(Position { kind: "valence", idx: 0 });
        }
        out
    }

    fn empty(&self, b: &QBuild) -> Vec<Position> {
        (0..8).filter(|&i| b.mods[i].is_none()).map(|idx| Position { kind: "mods", idx }).collect()
    }

    fn candidates(&self, b: &QBuild, p: Position) -> Vec<QBuild> {
        let res = crate::candidates::candidates_json(&self.candidate_request(b, p));
        let list = res.get("candidates").and_then(Value::as_array).cloned().unwrap_or_default();
        let mut out = Vec::new();
        for c in list {
            let id = c.get("id").and_then(Value::as_str).unwrap_or("");
            let payload = c.get("payload").cloned().unwrap_or(Value::Null);
            let mut n = b.clone();
            let kept = match p.kind {
                "mods" if p.idx < 8 => {
                    match self.pool.iter().position(|m| m.id == id) {
                        Some(pi) if self.usable[pi] => {
                            n.mods[p.idx] = Some(pi);
                            true
                        }
                        _ => false,
                    }
                }
                "mods" => match self.exilus_defs.iter().position(|x| x.as_ref().is_some_and(|m| m.id == id)) {
                    Some(xi) => {
                        n.exilus = xi;
                        true
                    }
                    None => false,
                },
                "arcane" => {
                    let ids = payload.get("arcane").and_then(Value::as_array).cloned().unwrap_or_default();
                    let ranks = payload.get("arcane_rank").and_then(Value::as_array).cloned().unwrap_or_default();
                    match self.arcane_set_of(&ids, &ranks) {
                        Some(a) => {
                            n.arcane = a;
                            true
                        }
                        None => false,
                    }
                }
                "evo" => {
                    let list: Vec<String> = payload
                        .get("evolutions")
                        .and_then(Value::as_array)
                        .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                        .unwrap_or_default();
                    match self.evo_sets.iter().position(|s| *s == list) {
                        Some(e) => {
                            n.evo = e;
                            true
                        }
                        None => false,
                    }
                }
                "mode" => match self.mode_ids.iter().position(|m| m == id) {
                    Some(m) => {
                        n.mode = m;
                        true
                    }
                    None => false,
                },
                "valence" => match self.valences.iter().position(|v| v == id) {
                    Some(v) => {
                        n.val = v;
                        true
                    }
                    None => false,
                },
                _ => false,
            };
            if kept && self.has_required(&n) {
                out.push(n);
            }
        }
        out
    }

    fn legal(&self, b: &QBuild) -> bool {
        self.has_required(b) && self.materialize(b).is_some()
    }

    fn score(&self, bs: &[QBuild]) -> Vec<Score> {
        let cands: Vec<Option<Candidate>> = bs.iter().map(|b| self.materialize(b)).collect();
        let jobs: Vec<(&Candidate, usize)> = cands
            .iter()
            .zip(bs)
            .filter_map(|(c, b)| c.as_ref().map(|c| (c, b.arcane)))
            .collect();
        let sums = wfsim_optimizer::descent::evaluate_paired(&jobs, self.arcanes, self.scenario, self.runs, self.seed);
        let mut it = sums.into_iter();
        cands
            .iter()
            .map(|c| {
                c.as_ref()
                    .and_then(|_| it.next())
                    .map(|s| (s.mean_kill_progress.max(0.0), s.mean_effective_damage))
            })
            .collect()
    }

    fn key(&self, b: &QBuild) -> String {
        format!("{b:?}")
    }

    /// The canonical form: the same cards whatever their slots, the same
    /// combined elements, the same exilus, arcane and variant.
    fn identity(&self, b: &QBuild) -> String {
        let Some(c) = self.materialize(b) else { return self.key(b) };
        let mut cards: Vec<&str> = b.mods.iter().flatten().map(|&i| self.pool[i].id).collect();
        cards.sort_unstable();
        let elements: Vec<String> = c
            .panel
            .damage
            .iter_nonzero()
            .map(|(t, v)| format!("{t:?}:{v:.3}"))
            .collect();
        format!("{cards:?}|{elements:?}|{}|{}|{:?}", b.exilus, b.arcane, self.variant(b))
    }
}

/// Descend from this shard's share of the starts (`index % shards`) and hand
/// back each answer as a screened job, for the funnel to rank at the final
/// run count.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_quick(
    ctx: &QuickCtx<'_>,
    starts: Vec<QuickStart<QBuild>>,
    max_evals: u64,
    swap_width: u32,
    state: Option<&wfsim_optimizer::FunnelState>,
    shard: u32,
    shards: u32,
    space: u128,
) -> (Vec<wfsim_optimizer::ScreenedJob>, wfsim_optimizer::search::SearchStats) {
    let shards = shards.max(1) as usize;
    let mine: Vec<QuickStart<QBuild>> = starts
        .into_iter()
        .enumerate()
        .filter(|(i, _)| i % shards == shard as usize % shards)
        .map(|(_, s)| s)
        .collect();
    let stop: Vec<&std::sync::atomic::AtomicBool> =
        state.map(|s| vec![&s.cancel, &s.stop_enumeration]).unwrap_or_default();
    let cfg = wfsim_optimizer::quick::QuickConfig { max_evals, swap_width, stop };
    let (answers, _failures, qs) = wfsim_optimizer::quick::quick_descent(ctx, &mine, &cfg);
    let built: Vec<(Candidate, usize)> =
        answers.iter().filter_map(|a| ctx.materialize(&a.build).map(|c| (c, a.build.arcane))).collect();
    let jobs: Vec<(&Candidate, usize)> = built.iter().map(|(c, a)| (c, *a)).collect();
    let sums = wfsim_optimizer::descent::evaluate_paired(&jobs, ctx.arcanes, ctx.scenario, ctx.runs, ctx.seed);
    let screened = built
        .into_iter()
        .zip(sums)
        .map(|((c, ai), summary)| wfsim_optimizer::ScreenedJob { cand: std::sync::Arc::new(c), ai, summary })
        .collect();
    let stats = wfsim_optimizer::search::SearchStats {
        space,
        sampled: u128::from(qs.evals),
        subsets: qs.evals,
        candidates: qs.evals,
        evals: qs.evals,
        exhaustive: false,
        starts: qs.starts,
        cut: qs.cut,
    };
    (screened, stats)
}
