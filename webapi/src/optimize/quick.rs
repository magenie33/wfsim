// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE QUICK DESCENT'S SPACE over one plan's tables (docs/OPTIMIZER.md,
//! "The quick descent — the quick calc, repeated").
//!
//! A build is indices into the plan: eight mod slots, an exilus option, an
//! arcane set, an evolution set, a mode and a valence. That makes "inside the
//! scope" a lookup — a candidate the quick calc offers is kept when it maps onto
//! the plan's tables and dropped when it does not. The candidates themselves are
//! `/api/candidates`'; a build fits when the enumerator can plan its Forma; it
//! scores through the funnel's own `evaluate`, the fight the replay runs.
//!
//! A BUILD SCORES AT ITS BEST ELEMENT ORDER. Slot position decides only what
//! combines, and a candidate seated in the slot being swept cannot move its
//! element behind another: Magnetic + Toxin on Sancti Magistar needs a card
//! swap AND a reorder at once, each worse alone (rank 4, 1.6% short, unmoved
//! at 30 runs and by a reorder move of its own). So each build is scored over
//! its distinct element orders and keeps the best, the enumerator's own rule.

use std::collections::HashMap;

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

impl QBuild {
    /// Indices as they travel between workers: the eight slots (-1 empty),
    /// then exilus, arcane, evolution set, mode, valence. Only a worker that
    /// parsed the same request can read them.
    fn to_json(&self) -> Value {
        let mut v: Vec<i64> = self.mods.iter().map(|m| m.map_or(-1, |i| i as i64)).collect();
        v.extend([self.exilus, self.arcane, self.evo, self.mode, self.val].map(|x| x as i64));
        json!(v)
    }

    fn from_json(v: &Value) -> Option<Self> {
        let a: Vec<i64> = v.as_array()?.iter().map(|x| x.as_i64()).collect::<Option<_>>()?;
        if a.len() != 13 {
            return None;
        }
        let mut mods = [None; 8];
        for (i, m) in mods.iter_mut().enumerate() {
            *m = usize::try_from(a[i]).ok();
        }
        let at = |i: usize| usize::try_from(a[i]).ok();
        Some(QBuild { mods, exilus: at(8)?, arcane: at(9)?, evo: at(10)?, mode: at(11)?, val: at(12)? })
    }
}

/// THE BROWSER FLEET'S SHARED DESCENT (docs/WASM.md, "The quick descent across
/// the fleet"). A worker is one thread and cannot wait for another, so the
/// LEADER runs the descent on the scores it holds; a start whose next builds
/// nobody has scored pauses, and the call hands every start's batch back to the
/// page, which splits it across all workers and returns the scores. It persists
/// between the leader's calls — the scores, and the candidate lists and
/// legality the replay would otherwise recompute at every step.
#[derive(Default)]
pub(crate) struct Fleet {
    /// By key: the score, the best element order, and the per-run σ of kill
    /// progress at that order.
    scores: HashMap<String, (Score, usize, f64)>,
    candidates: HashMap<(QBuild, Position), Vec<QBuild>>,
    legal: HashMap<QBuild, bool>,
    pending: Vec<QBuild>,
    missed: bool,
    /// Where each start stood when the last call ended.
    progress: Value,
}

thread_local! {
    pub(crate) static FLEET: std::cell::RefCell<Fleet> = std::cell::RefCell::new(Fleet::default());
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
    /// Each scored build's best element order, by key.
    pub(crate) best: std::sync::Mutex<std::collections::HashMap<String, Candidate>>,
    /// Each scored build's per-run σ of kill progress at that order, by key —
    /// what the band around the N-th contender is sized from.
    pub(crate) spread: std::sync::Mutex<std::collections::HashMap<String, f64>>,
    /// Engagements simulated: every order of every build, at `runs` each.
    pub(crate) sims: std::sync::atomic::AtomicU64,
    /// Where the page reads progress: builds scored (`enumerated`) and fights
    /// run (`sims_done`), advanced a chunk at a time so a slow host shows it.
    pub(crate) progress: Option<&'a wfsim_optimizer::FunnelState>,
    /// Leading the browser fleet: scores come from [`FLEET`], and a miss
    /// pauses the start that asked rather than being simulated here.
    pub(crate) lead: bool,
    /// How many of the eight main slots a build may fill (`build_size`).
    pub(crate) mod_slots: usize,
}

/// Jobs scored between progress updates: small enough that a single-threaded
/// browser worker reports every few seconds, large enough to keep cores busy.
const PROGRESS_CHUNK: usize = 32;

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

    /// Every distinct element order of the build that can exist — empty when
    /// none can: no such variant, a card its evolutions refuse, or Forma
    /// cannot fit it.
    fn orders(&self, b: &QBuild) -> Vec<Candidate> {
        let Some(vi) = self.variant(b) else { return Vec::new() };
        let mut subset: Vec<usize> = b.mods.iter().flatten().copied().collect();
        let forbids = &self.variant_forbids[self.variants[vi].1];
        if subset.iter().any(|&i| forbids[i]) {
            return Vec::new();
        }
        subset.sort_unstable();
        let (base, second) = &self.forms[vi];
        let mut out = Vec::new();
        wfsim_optimizer::expand_one(
            self.pool,
            base,
            second.as_ref(),
            vi as u32,
            self.cap,
            self.innate,
            self.exilus_refs,
            &subset,
            &self.scenario.arena.tenno,
            self.scenario.policy,
            &mut out,
        );
        out.retain(|c| c.exilus as usize == b.exilus);
        out
    }

    /// The build at its best element order, once scored; before that, the
    /// first order that can exist.
    pub(crate) fn materialize(&self, b: &QBuild) -> Option<Candidate> {
        if let Some(c) = self.best.lock().ok().and_then(|m| m.get(&self.key(b)).cloned()) {
            return Some(c);
        }
        let at = if self.lead { FLEET.with(|f| f.borrow().scores.get(&self.key(b)).map(|x| x.1)) } else { None };
        self.orders(b).into_iter().nth(at.unwrap_or(0))
    }

    /// The per-run σ of kill progress the build scored with.
    fn spread_of(&self, b: &QBuild) -> f64 {
        let k = self.key(b);
        if self.lead {
            return FLEET.with(|f| f.borrow().scores.get(&k).map_or(0.0, |x| x.2));
        }
        self.spread.lock().ok().and_then(|m| m.get(&k).copied()).unwrap_or(0.0)
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

    /// The request's BUILD-SHAPED starts — the page's: ten slots, the axes, and
    /// the positions fixed. What the plan's tables cannot hold falls back to the
    /// plan's default for that axis; the page puts a start's cards into the
    /// scope, so that is a start from an older scope, not a new rule.
    fn build_starts(&self) -> Vec<QuickStart<QBuild>> {
        let list = self.request.get("starts").and_then(Value::as_array).cloned().unwrap_or_default();
        let str_of = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let mut out = Vec::new();
        for s in list.iter().filter(|s| s.get("slots").is_some()) {
            let mut b = self.blank();
            let mut unnamed: Vec<Position> = Vec::new();
            let slots = s.get("slots").and_then(Value::as_array).cloned().unwrap_or_default();
            for (i, id) in slots.iter().take(9).enumerate() {
                let Some(id) = id.as_str() else { continue };
                if i < self.mod_slots {
                    b.mods[i] = self.pool.iter().position(|m| m.id == id).filter(|&p| self.usable[p]);
                } else if i < 8 {
                    // Past the limit's last slot: the page refuses such a start.
                } else if let Some(x) = self.exilus_defs.iter().position(|x| x.as_ref().is_some_and(|m| m.id == id)) {
                    b.exilus = x;
                }
            }
            if slots.get(8).and_then(Value::as_str).is_none() && self.exilus_defs.len() > 1 {
                unnamed.push(Position { kind: "mods", idx: 8 });
            }
            let ids = s.get("arcane").and_then(Value::as_array).cloned().unwrap_or_default();
            let ranks = s.get("arcane_rank").and_then(Value::as_array).cloned().unwrap_or_default();
            match self.arcane_set_of(&ids, &ranks) {
                Some(a) => b.arcane = a,
                None => unnamed.extend((0..self.arcane_sets[0].len()).map(|idx| Position { kind: "arcane", idx })),
            }
            // What the limits exclude is in no set, so a start holding it has
            // not named that tier: the fill chooses it.
            let evos: Vec<String> = s
                .get("evolutions")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<String>>())
                .unwrap_or_default()
                .into_iter()
                .filter(|id| self.evo_sets.iter().any(|set| set.contains(id)))
                .collect();
            // The set that holds every tier the start names; the tiers it does
            // not name are the fill's.
            if let Some(e) = self.evo_sets.iter().position(|x| evos.iter().all(|id| x.contains(id))) {
                b.evo = e;
            }
            let named: Vec<usize> = evos
                .iter()
                .filter_map(|id| wfsim_engine::data::evolutions::get(id).map(|e| e.tier as usize))
                .collect();
            unnamed.extend(
                self.evo_tiers().into_iter().filter(|t| !named.contains(t)).map(|idx| Position { kind: "evo", idx }),
            );
            match str_of(s, "mode").and_then(|m| self.mode_ids.iter().position(|x| *x == m)) {
                Some(m) => b.mode = m,
                None => unnamed.push(Position { kind: "mode", idx: 0 }),
            }
            match str_of(s, "valence_element").and_then(|e| self.valences.iter().position(|x| *x == e)) {
                Some(v) => b.val = v,
                None => unnamed.push(Position { kind: "valence", idx: 0 }),
            }
            // Required cards ride in every start, in the first free slots.
            for r in &self.required {
                if !b.mods.contains(&Some(*r)) {
                    if let Some(slot) = b.mods.iter().position(Option::is_none) {
                        b.mods[slot] = Some(*r);
                    }
                }
            }
            let mut fixed: Vec<Position> = s
                .get("fixed")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|p| {
                    let kind: &'static str = match p.get("kind").and_then(Value::as_str)? {
                        "mods" => "mods",
                        "arcane" => "arcane",
                        "evo" => "evo",
                        "mode" => "mode",
                        "valence" => "valence",
                        _ => return None,
                    };
                    Some(Position { kind, idx: p.get("idx").and_then(Value::as_u64)? as usize })
                })
                .collect();
            for (slot, m) in b.mods.iter().enumerate() {
                if m.is_some_and(|m| self.required.contains(&m)) {
                    fixed.push(Position { kind: "mods", idx: slot });
                }
            }
            // The page pins the evolution BLOCK; each tier is a position here.
            if fixed.iter().any(|p| p.kind == "evo") {
                fixed.retain(|p| p.kind != "evo");
                fixed.extend(self.evo_tiers().into_iter().map(|idx| Position { kind: "evo", idx }));
            }
            out.push(QuickStart { build: b, fixed, unnamed });
        }
        out
    }

    /// The evolution tiers the scope ranges over, in ladder order.
    fn evo_tiers(&self) -> Vec<usize> {
        let mut t: Vec<usize> = self
            .evo_sets
            .iter()
            .flatten()
            .filter_map(|id| wfsim_engine::data::evolutions::get(id).map(|e| e.tier as usize))
            .collect();
        t.sort_unstable();
        t.dedup();
        t
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
        let builds = self.build_starts();
        if !builds.is_empty() {
            return builds;
        }
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
                QuickStart { build: b, fixed, unnamed: Vec::new() }
            })
            .collect()
    }
}

impl QuickSpace for QuickCtx<'_> {
    type Build = QBuild;

    /// WHAT SHAPES THE WEAPON FIRST: how it is played, its evolutions tier by
    /// tier, its element, its arcane — then the cards, then the exilus. A card
    /// chosen before the mode or the evolutions is chosen for another weapon:
    /// on Burston Prime the cycle doubled the score and half the work before it
    /// had been spent on the base form.
    fn positions(&self, _: &QBuild) -> Vec<Position> {
        let mut out: Vec<Position> = Vec::new();
        if self.mode_ids.len() > 1 {
            out.push(Position { kind: "mode", idx: 0 });
        }
        if self.evo_sets.len() > 1 {
            out.extend(self.evo_tiers().into_iter().map(|idx| Position { kind: "evo", idx }));
        }
        if self.valences.len() > 1 {
            out.push(Position { kind: "valence", idx: 0 });
        }
        if self.arcane_sets.len() > 1 {
            out.extend((0..self.arcane_sets[0].len()).map(|idx| Position { kind: "arcane", idx }));
        }
        out.extend((0..self.mod_slots).map(|idx| Position { kind: "mods", idx }));
        if self.exilus_defs.len() > 1 {
            out.push(Position { kind: "mods", idx: 8 });
        }
        out
    }

    fn empty(&self, b: &QBuild) -> Vec<Position> {
        let mut out: Vec<Position> = (0..self.mod_slots)
            .filter(|&i| b.mods[i].is_none())
            .map(|idx| Position { kind: "mods", idx })
            .collect();
        if self.exilus_defs.len() > 1 && self.exilus_defs[b.exilus].is_none() {
            out.push(Position { kind: "mods", idx: 8 });
        }
        out
    }

    fn candidates(&self, b: &QBuild, p: Position) -> Vec<QBuild> {
        if self.lead {
            if let Some(c) = FLEET.with(|f| f.borrow().candidates.get(&(b.clone(), p)).cloned()) {
                return c;
            }
            let c = self.candidates_now(b, p);
            FLEET.with(|f| f.borrow_mut().candidates.insert((b.clone(), p), c.clone()));
            return c;
        }
        self.candidates_now(b, p)
    }

    fn legal(&self, b: &QBuild) -> bool {
        if self.lead {
            if let Some(l) = FLEET.with(|f| f.borrow().legal.get(b).copied()) {
                return l;
            }
            let l = self.legal_now(b);
            FLEET.with(|f| f.borrow_mut().legal.insert(b.clone(), l));
            return l;
        }
        self.legal_now(b)
    }

    /// Each build at its best element order: every order is scored on the one
    /// paired stream and the best is kept, and remembered for the answer. The
    /// fleet's leader reads the scores it was handed instead.
    fn score(&self, bs: &[QBuild]) -> Vec<Score> {
        if !self.lead {
            return self.score_orders(bs).into_iter().map(|(s, _, _)| s).collect();
        }
        FLEET.with(|f| {
            let mut f = f.borrow_mut();
            f.missed = false;
            let mut out = Vec::with_capacity(bs.len());
            for b in bs {
                match f.scores.get(&self.key(b)) {
                    Some(&(s, _, _)) => out.push(s),
                    None => {
                        f.missed = true;
                        if !f.pending.contains(b) {
                            f.pending.push(b.clone());
                        }
                        out.push(None);
                    }
                }
            }
            out
        })
    }

    fn pending(&self) -> bool {
        self.lead && FLEET.with(|f| f.borrow().missed)
    }

    /// ORDER-BLIND: the cards as a set, since a build scores at its best order.
    fn key(&self, b: &QBuild) -> String {
        let mut mods: Vec<&str> = b.mods.iter().flatten().map(|&i| self.pool[i].id).collect();
        mods.sort_unstable();
        format!("{mods:?} x{} a{} e{} m{} v{}", b.exilus, b.arcane, b.evo, b.mode, b.val)
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

impl QuickCtx<'_> {
    fn candidates_now(&self, b: &QBuild, p: Position) -> Vec<QBuild> {
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
                "evo" if wfsim_engine::data::evolutions::get(id).is_none_or(|e| e.tier as usize != p.idx) => false,
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

    fn legal_now(&self, b: &QBuild) -> bool {
        self.has_required(b) && !self.orders(b).is_empty()
    }

    /// Every build's score at its best element order, and WHICH order that is
    /// (its index in [`Self::orders`]) — what a fleet worker hands back.
    fn score_orders(&self, bs: &[QBuild]) -> Vec<(Score, usize, f64)> {
        let orders: Vec<Vec<Candidate>> = bs.iter().map(|b| self.orders(b)).collect();
        let jobs: Vec<(&Candidate, usize)> = orders
            .iter()
            .zip(bs)
            .flat_map(|(os, b)| os.iter().map(move |c| (c, b.arcane)))
            .collect();
        use std::sync::atomic::Ordering::Relaxed;
        self.sims.fetch_add(jobs.len() as u64 * u64::from(self.runs), Relaxed);
        // One seed for every chunk, so the chunks stay one paired stream.
        let mut sums = Vec::with_capacity(jobs.len());
        for chunk in jobs.chunks(PROGRESS_CHUNK) {
            sums.extend(wfsim_optimizer::descent::evaluate_paired(chunk, self.arcanes, self.scenario, self.runs, self.seed));
            if let Some(p) = self.progress {
                p.sims_done.fetch_add(chunk.len() as u64 * u64::from(self.runs), Relaxed);
            }
            wfsim_optimizer::tick();
        }
        if let Some(p) = self.progress {
            p.enumerated.fetch_add(bs.len() as u64, Relaxed);
        }
        let mut it = sums.into_iter();
        let mut out = Vec::with_capacity(bs.len());
        for (b, os) in bs.iter().zip(&orders) {
            let mut best: Option<(usize, (f64, f64), f64)> = None;
            for (oi, _) in os.iter().enumerate() {
                let Some(s) = it.next() else { break };
                let sc = (s.mean_kill_progress.max(0.0), s.mean_effective_damage);
                if best.is_none_or(|(_, x, _)| sc.0.total_cmp(&x.0).then(sc.1.total_cmp(&x.1)).is_gt()) {
                    best = Some((oi, sc, s.std_kill_progress));
                }
            }
            if let (Some((oi, _, _)), Ok(mut m)) = (best, self.best.lock()) {
                m.insert(self.key(b), os[oi].clone());
            }
            let sd = best.map_or(0.0, |(_, _, sd)| sd);
            if let Ok(mut m) = self.spread.lock() {
                m.insert(self.key(b), sd);
            }
            out.push((best.map(|(_, s, _)| s), best.map_or(0, |(oi, _, _)| oi), sd));
        }
        out
    }

    /// A fleet worker's share of a batch: each build's score and best order.
    pub(crate) fn score_json(&self, builds: &[Value]) -> Value {
        let bs: Vec<QBuild> = builds.iter().filter_map(QBuild::from_json).collect();
        let before = self.sims.load(std::sync::atomic::Ordering::Relaxed);
        let got = self.score_orders(&bs);
        let fights = self.sims.load(std::sync::atomic::Ordering::Relaxed) - before;
        let rows: Vec<Value> = bs
            .iter()
            .zip(got)
            .map(|(b, (s, oi, sd))| {
                json!({ "key": self.key(b), "score": s.map(|(k, e)| json!([k, e])), "order": oi, "sd": sd })
            })
            .collect();
        json!({ "ok": true, "scores": rows, "fights": fights })
    }

    /// The leader takes the scores the fleet returned; `fresh` begins a new
    /// search and forgets the last one's.
    pub(crate) fn lead_take(scores: &[Value], fresh: bool) {
        FLEET.with(|f| {
            let mut f = f.borrow_mut();
            if fresh {
                *f = Fleet::default();
            }
            f.pending.clear();
            for r in scores {
                let Some(key) = r.get("key").and_then(Value::as_str) else { continue };
                let s = r.get("score").and_then(Value::as_array).and_then(|a| Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?)));
                let oi = r.get("order").and_then(Value::as_u64).unwrap_or(0) as usize;
                let sd = r.get("sd").and_then(Value::as_f64).unwrap_or(0.0);
                f.scores.insert(key.to_string(), (s, oi, sd));
            }
        });
    }

    /// The batch the paused starts wait for, or `None` when every start ran to
    /// the end on scores the leader already holds.
    pub(crate) fn lead_pending() -> Option<Value> {
        FLEET.with(|f| {
            let f = f.borrow();
            (!f.pending.is_empty()).then(|| {
                json!({
                    "ok": true,
                    "pending": f.pending.iter().map(QBuild::to_json).collect::<Vec<_>>(),
                    "scored": f.scores.len(),
                    "progress": f.progress,
                })
            })
        })
    }
}

/// Where each answer came from, how each other contender differs from the
/// nearest answer, and the starts that reached none — keyed by the job, since
/// the funnel re-ranks contenders and rows are drawn from its list.
#[derive(Default)]
pub(crate) struct QuickReport {
    pub(crate) from_starts: std::collections::HashMap<super::JobIdentity, serde_json::Value>,
    pub(crate) near: std::collections::HashMap<super::JobIdentity, serde_json::Value>,
    pub(crate) failures: Vec<serde_json::Value>,
}

impl QuickCtx<'_> {
    /// What turns `from` into `to`, one entry per changed position: a card
    /// swapped (either side `null` when the counts differ), the exilus, an
    /// arcane seat, an evolution tier, the mode, the valence.
    fn changes(&self, from: &QBuild, to: &QBuild) -> Vec<Value> {
        let mut out = Vec::new();
        let cards = |b: &QBuild| -> Vec<&str> {
            let mut v: Vec<&str> = b.mods.iter().flatten().map(|&i| self.pool[i].id).collect();
            v.sort_unstable();
            v
        };
        let (mut gone, mut came) = (cards(from), cards(to));
        let both: Vec<&str> = gone.iter().filter(|id| came.contains(id)).copied().collect();
        for id in both {
            if let (Some(i), Some(j)) = (gone.iter().position(|x| *x == id), came.iter().position(|x| *x == id)) {
                gone.remove(i);
                came.remove(j);
            }
        }
        for k in 0..gone.len().max(came.len()) {
            out.push(json!({ "axis": "mod", "from": gone.get(k), "to": came.get(k) }));
        }
        let exilus = |b: &QBuild| self.exilus_defs[b.exilus].as_ref().map(|m| m.id);
        if exilus(from) != exilus(to) {
            out.push(json!({ "axis": "exilus", "from": exilus(from), "to": exilus(to) }));
        }
        for (a, b) in self.arcane_sets[from.arcane].iter().zip(&self.arcane_sets[to.arcane]) {
            if a != b {
                out.push(json!({ "axis": "arcane", "from": a, "to": b }));
            }
        }
        let tier = |b: &QBuild, t: usize| {
            self.evo_sets[b.evo]
                .iter()
                .find(|id| wfsim_engine::data::evolutions::get(id).is_some_and(|e| e.tier as usize == t))
                .cloned()
        };
        for t in self.evo_tiers() {
            if tier(from, t) != tier(to, t) {
                out.push(json!({ "axis": "evolution", "from": tier(from, t), "to": tier(to, t) }));
            }
        }
        if from.mode != to.mode {
            out.push(json!({ "axis": "mode", "from": self.mode_ids.get(from.mode), "to": self.mode_ids.get(to.mode) }));
        }
        if from.val != to.val {
            out.push(json!({ "axis": "valence", "from": self.valences.get(from.val), "to": self.valences.get(to.val) }));
        }
        out
    }
}

/// Descend from this shard's share of the starts (`index % shards`) and hand
/// back the CONTENDERS as screened jobs, for the funnel to rank at the final
/// run count: the best `finalists` whole builds the sweeps scored, and every
/// one below them that ties the N-th (`tied_at_the_line`, the funnel's own
/// cut). Start numbers in the report are the REQUEST's, not the shard's.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_quick(
    ctx: &QuickCtx<'_>,
    starts: Vec<QuickStart<QBuild>>,
    max_evals: u64,
    state: Option<&wfsim_optimizer::FunnelState>,
    shard: u32,
    shards: u32,
    space: u128,
    finalists: usize,
) -> (Vec<wfsim_optimizer::ScreenedJob>, wfsim_optimizer::search::SearchStats, QuickReport) {
    let shards = shards.max(1) as usize;
    let global = |j: usize| j * shards + shard as usize % shards;
    let mine: Vec<QuickStart<QBuild>> = starts
        .into_iter()
        .enumerate()
        .filter(|(i, _)| i % shards == shard as usize % shards)
        .map(|(_, s)| s)
        .collect();
    let stop: Vec<&std::sync::atomic::AtomicBool> =
        state.map(|s| vec![&s.cancel, &s.stop_enumeration]).unwrap_or_default();
    let cfg = wfsim_optimizer::quick::QuickConfig { max_evals, stop };
    let wfsim_optimizer::quick::QuickRun { answers, failures, stats: qs, progress, pool } =
        wfsim_optimizer::quick::quick_descent(ctx, &mine, &cfg);
    if ctx.lead {
        let rows: Vec<Value> = progress
            .iter()
            .map(|p| json!({ "settled": p.settled, "round": p.round, "at": p.at, "of": p.of }))
            .collect();
        FLEET.with(|f| f.borrow_mut().progress = json!(rows));
    }
    let mut report = QuickReport {
        failures: failures.iter().map(|f| json!({ "start": global(f.start), "why": f.why })).collect(),
        ..Default::default()
    };
    let scored: Vec<&(QBuild, Score)> = pool.iter().filter(|(_, s)| s.is_some()).collect();
    let field: Vec<(f64, f64)> =
        scored.iter().map(|(b, s)| (s.map_or(0.0, |(kp, _)| kp), ctx.spread_of(b))).collect();
    let keep = wfsim_optimizer::tied_at_the_line(&field, finalists, ctx.runs);
    let contenders: Vec<&QBuild> = scored[..keep].iter().map(|(b, _)| b).collect();
    let answer_of = |b: &QBuild| {
        let id = ctx.identity(b);
        answers.iter().position(|a| ctx.identity(&a.build) == id)
    };
    // A runner-up is described against an answer ON THE LIST, so the row it
    // names is one the reader can see. The best answer always is: a sweep that
    // scored a better build would have moved to it.
    let listed: Vec<usize> = contenders.iter().filter_map(|b| answer_of(b)).collect();
    let mut built: Vec<(Candidate, usize)> = Vec::new();
    for &b in &contenders {
        let Some(c) = ctx.materialize(b) else { continue };
        let job = (c.ordered.clone(), c.variant, c.exilus, b.arcane);
        match answer_of(b) {
            Some(ai) => {
                let a = &answers[ai];
                let lanes: Vec<Value> = a
                    .starts
                    .iter()
                    .enumerate()
                    .map(|(k, &s)| {
                        json!({
                            "start": global(s),
                            "from": a.from.get(k).copied().flatten().map(|(kp, _)| kp),
                            "moves": a.moves.get(k).copied().unwrap_or(0),
                        })
                    })
                    .collect();
                report.from_starts.insert(job, json!(lanes));
            }
            None => {
                let near: Vec<usize> = if listed.is_empty() { (0..answers.len()).collect() } else { listed.clone() };
                if let Some((a, changes)) =
                    near.into_iter().map(|ai| (&answers[ai], ctx.changes(&answers[ai].build, b))).min_by_key(|(_, c)| c.len())
                {
                    let starts: Vec<usize> = a.starts.iter().map(|&s| global(s)).collect();
                    report.near.insert(job, json!({ "starts": starts, "changes": changes }));
                }
            }
        }
        built.push((c, b.arcane));
    }
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
        // SIMULATED ENGAGEMENTS, which is what the walk's count means: every
        // order of every build scored, at `runs` each.
        evals: ctx.sims.load(std::sync::atomic::Ordering::Relaxed),
        exhaustive: false,
        starts: qs.starts,
        cut: qs.cut,
    };
    (screened, stats, report)
}

/// THE QUICK CALC'S WHOLE SCOPE, for a quick request that names none: every
/// card, exilus card, arcane, evolution, mode and element the weapon takes, and
/// each lower rank the every-rank lists ask for — what `/api/candidates` offers
/// at each position, so the descent's candidates are the quick calc's — less
/// what the request's `limits` rule out: an excluded option on any axis (a
/// card by its exact id, so `card@2` is that card at rank 2 alone; an arcane
/// at every rank), at most `mods` cards, the exilus or an arcane seat left
/// empty (docs/OPTIMIZER.md, "Limits").
/// `None` when the request is not quick or names its own scope (the grader's).
pub(crate) fn whole_scope(v: &Value) -> Option<Value> {
    if v.get("strategy").and_then(Value::as_str) != Some("quick") || v.get("mods").is_some() {
        return None;
    }
    let info = crate::registry::weapon(v.get("weapon").and_then(Value::as_str).unwrap_or(""));
    let every = wfsim_engine::data::mods::every_rank();
    let every_list = |k: &str, dflt: &[String]| -> Vec<String> {
        v.get("every_rank")
            .and_then(|e| e.get(k))
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_else(|| dflt.to_vec())
    };
    let every_mods = every_list("mods", &every.mods);
    let every_arcanes = every_list("arcanes", &every.arcanes);
    let ranked = |id: &str, max: u32, every: &[String]| -> Vec<String> {
        let mut ids = vec![id.to_string()];
        if every.iter().any(|e| e == id) {
            ids.extend((0..max).map(|r| format!("{id}@{r}")));
        }
        ids
    };
    let limits = v.get("limits").cloned().unwrap_or(Value::Null);
    let out_of = |axis: &str| -> Vec<String> {
        limits
            .get("exclude")
            .and_then(|e| e.get(axis))
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default()
    };
    let (no_mods, no_arcanes, no_evos, no_modes, no_valence) =
        (out_of("mods"), out_of("arcanes"), out_of("evolutions"), out_of("modes"), out_of("valence"));
    let pool = crate::rivens::mod_pool_with_rivens(v, info, &[]);
    let mut mods = serde_json::Map::new();
    let mut exilus = serde_json::Map::new();
    for m in pool.iter().filter(|m| m.stance.is_none()) {
        for id in ranked(m.id, m.max_rank, &every_mods).into_iter().filter(|id| !no_mods.contains(id)) {
            mods.insert(id.clone(), json!("search"));
            if m.exilus {
                exilus.insert(id, json!("search"));
            }
        }
    }
    // The exilus left empty: its one option is the empty slot.
    if limits.get("exilus").and_then(Value::as_bool) == Some(false) {
        exilus = serde_json::Map::from_iter([("none".to_string(), json!("fixed"))]);
    }
    let seat_filled = |i: usize| -> bool {
        limits.get("arcane_seats").and_then(|a| a.get(i)).and_then(Value::as_bool) != Some(false)
    };
    let mut arcanes = serde_json::Map::new();
    for (i, seat) in info.arcane_pools.iter().enumerate() {
        if !seat_filled(i) {
            arcanes.insert(format!("none:{seat}"), json!("fixed"));
            continue;
        }
        for a in wfsim_engine::data::arcanes::pool_for_weapon(&info.id, seat) {
            if a.id != "none" && !no_arcanes.contains(&a.id) {
                for id in ranked(&a.id, a.max_rank, &every_arcanes) {
                    arcanes.insert(id, json!("search"));
                }
            }
        }
    }
    let group = crate::registry::evo_group(info);
    let mut evolutions = serde_json::Map::new();
    for t in 1..=wfsim_engine::data::evolutions::tier_count(group) {
        let ids: Vec<Value> = wfsim_engine::data::evolutions::options(group, t)
            .iter()
            .filter(|o| !no_evos.contains(&o.id))
            .map(|o| json!(o.id))
            .collect();
        if !ids.is_empty() {
            evolutions.insert(t.to_string(), json!(ids));
        }
    }
    let modes: serde_json::Map<String, Value> = wfsim_engine::data::weapons::play_modes(&info.id)
        .iter()
        .filter(|m| m.sustainable && !no_modes.iter().any(|x| x == m.id))
        .map(|m| (m.id.to_string(), json!("search")))
        .collect();
    let valence: serde_json::Map<String, Value> = wfsim_engine::data::weapons::valence_of(&info.id)
        .map(|s| s.elements.iter().filter(|e| !no_valence.iter().any(|x| x == *e)).map(|e| (e.to_string(), json!("search"))).collect())
        .unwrap_or_default();
    let most = limits.get("mods").and_then(Value::as_u64).unwrap_or(8).min(8);
    let mut out = v.clone();
    let o = out.as_object_mut()?;
    o.insert("mods".into(), Value::Object(mods));
    o.insert("exilus".into(), Value::Object(exilus));
    o.insert("arcanes".into(), Value::Object(arcanes));
    o.insert("evolutions".into(), Value::Object(evolutions));
    if !modes.is_empty() {
        o.insert("modes".into(), Value::Object(modes));
    }
    if !valence.is_empty() {
        o.insert("valence".into(), Value::Object(valence));
    }
    o.insert("build_size".into(), json!(most));
    o.insert("build_min".into(), json!(0));
    Some(out)
}
