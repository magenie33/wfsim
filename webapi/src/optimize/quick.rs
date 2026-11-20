// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE QUICK DESCENT'S SPACE over one plan's tables (docs/OPTIMIZER.md,
//! "The quick descent — the quick calc, repeated").
//!
//! A build is indices into the plan: eight mod slots, exilus and stance options, an
//! arcane set, an evolution set, a mode, a valence and an assembly. That makes "inside the
//! scope" a lookup — a candidate the quick calc offers is kept when it maps onto
//! the plan's tables and dropped when it does not. The candidates themselves are
//! `/api/candidates`'; a build fits when the enumerator can plan its Forma; it
//! scores through the funnel's own `evaluate`, the fight the replay runs.
//!
//! A BUILD SCORES AT ITS BEST ALLOWED ELEMENT ORDER. Slot position decides what
//! combines, and a candidate seated in the slot being swept cannot move its
//! element behind another: Magnetic + Toxin on Sancti Magistar needs a card
//! swap AND a reorder at once, each worse alone (rank 4, 1.6% short, unmoved
//! at 30 runs and by a reorder move of its own). So each build is scored over
//! its distinct allowed element orders and keeps the best.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use super::{carry::Carry, warm, Replay};

use serde_json::{json, Value};
use wfsim_engine::model::ModDef;
use wfsim_optimizer::quick::{Position, QuickSpace, QuickStart, Score};
use wfsim_engine::fight::{Shard, Summary};
use wfsim_optimizer::Candidate;

/// A build as indices into the plan's tables.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct QBuild {
    pub(crate) mods: [Option<usize>; 8],
    pub(crate) exilus: usize,
    pub(crate) stance: usize,
    pub(crate) arcane: usize,
    pub(crate) evo: usize,
    pub(crate) mode: usize,
    pub(crate) val: usize,
    pub(crate) assembly: usize,
    pub(crate) fixed_slots: u8,
}

pub(crate) struct SearchBranch {
    start: QuickStart<QBuild>,
    origin: usize,
    element: Option<String>,
}

impl QBuild {
    /// Indices as they travel between workers: the eight slots (-1 empty),
    /// then exilus, arcane, evolution set, mode, valence, assembly and the
    /// fixed-slot mask, then the stance. Legacy arrays omit the final stance.
    /// Only a worker that
    /// parsed the same request can read them.
    fn to_json(&self) -> Value {
        let mut v: Vec<i64> = self.mods.iter().map(|m| m.map_or(-1, |i| i as i64)).collect();
        v.extend([self.exilus, self.arcane, self.evo, self.mode, self.val, self.assembly].map(|x| x as i64));
        v.push(i64::from(self.fixed_slots));
        v.push(self.stance as i64);
        json!(v)
    }

    fn from_json(v: &Value) -> Option<Self> {
        let a: Vec<i64> = v.as_array()?.iter().map(|x| x.as_i64()).collect::<Option<_>>()?;
        if !matches!(a.len(), 15 | 16) {
            return None;
        }
        let mut mods = [None; 8];
        for (i, m) in mods.iter_mut().enumerate() {
            *m = usize::try_from(a[i]).ok();
        }
        let at = |i: usize| usize::try_from(a[i]).ok();
        Some(QBuild { mods, exilus: at(8)?, stance: if a.len() == 16 { at(15)? } else { 0 }, arcane: at(9)?, evo: at(10)?, mode: at(11)?, val: at(12)?, assembly: at(13)?, fixed_slots: u8::try_from(a[14]).ok()? })
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
    /// Builds waiting for a full score, with their keys.
    pending: Vec<(QBuild, String)>,
    pending_keys: HashSet<String>,
    /// The step screen's short scores, by key, and the builds still waiting
    /// for one — each handed out with its own run count, so the page passes
    /// the batch through without knowing a screen exists.
    rough: HashMap<String, Score>,
    pending_rough: Vec<(QBuild, u32)>,
    pending_rough_keys: HashSet<String>,
    /// The screen's runs per element order, by key, as a worker sent them —
    /// handed out again with the build's full measurement.
    screened_runs: HashMap<String, Vec<Option<Carry>>>,
    /// Each fully scored build's shard at its best order, by key.
    best_runs: HashMap<String, Carry>,
    missed: bool,
    /// Where each start stood when the last call ended.
    progress: Value,
}

thread_local! {
    pub(crate) static FLEET: std::cell::RefCell<Fleet> = std::cell::RefCell::new(Fleet::default());
    // A replay revisits the same builds. Keep this separate: score() already borrows FLEET.
    static LEAD_MEMO: std::cell::RefCell<LeadMemo> = std::cell::RefCell::new(LeadMemo::default());
}

#[derive(Default)]
struct LeadMemo {
    scope: std::sync::Weak<ScopeForms>,
    keys: HashMap<QBuild, String>,
    preferences: HashMap<QBuild, std::rc::Rc<wfsim_engine::board::builds::Preference<'static>>>,
}

type ScopeForms = std::sync::OnceLock<Vec<wfsim_engine::model::WeaponBase>>;

impl LeadMemo {
    fn bind(&mut self, scope: &Arc<ScopeForms>, fresh: bool) {
        let scope = Arc::downgrade(scope);
        if fresh || !self.scope.ptr_eq(&scope) { *self = Self { scope, ..Self::default() }; }
    }
}

// As in the Simfleet leader, a miss after eviction only recomputes metadata, never a fight.
fn memoized<T: Clone>(map: &mut HashMap<QBuild, T>, b: &QBuild, make: impl FnOnce() -> T) -> T {
    if let Some(v) = map.get(b) { return v.clone(); }
    if map.len() >= 4096 { map.clear(); }
    let value = make();
    map.insert(b.clone(), value.clone());
    value
}

pub(crate) struct QuickCtx<'a> {
    pub(crate) weapon_id: &'a str,
    pub(crate) pool: &'a [ModDef],
    pub(crate) stances: &'a [Option<usize>],
    /// Per pool index: not forbidden by the scope.
    pub(crate) usable: Vec<bool>,
    pub(crate) required: Vec<usize>,
    pub(crate) final_required: &'a [String],
    pub(crate) forms: &'a [wfsim_engine::model::WeaponBase],
    pub(crate) variants: &'a [(usize, usize, usize, usize)],
    pub(crate) assemblies: &'a [Value],
    pub(crate) variant_forbids: &'a [Vec<bool>],
    pub(crate) evo_sets: &'a [Vec<String>],
    pub(crate) mode_ids: Vec<String>,
    pub(crate) valences: &'a [String],
    pub(crate) valence_bonus: f64,
    pub(crate) arcane_sets: &'a [Vec<String>],
    pub(crate) exilus_defs: &'a [Option<ModDef>],
    pub(crate) exilus_refs: &'a [Option<&'a ModDef>],
    pub(crate) innate: &'a [Option<wfsim_engine::rules::capacity::Polarity>],
    pub(crate) scenario: &'a wfsim_optimizer::Scenario,
    /// The request, for what a candidate list needs that the plan does not
    /// hold: the visitor's rivens and simulator settings.
    pub(crate) request: &'a Value,
    pub(crate) runs: u32,
    pub(crate) seed: u64,
    /// Each scored build's best element order, by key.
    pub(crate) best: std::sync::Mutex<std::collections::HashMap<String, usize>>,
    /// Each scored build's per-run σ of kill progress at that order, by key —
    /// what the band around the N-th contender is sized from.
    pub(crate) spread: std::sync::Mutex<std::collections::HashMap<String, f64>>,
    /// Engagements simulated: every order of every build, at `runs` each.
    pub(crate) sims: std::sync::atomic::AtomicU64,
    /// …and the work those fights took (`Summary::work`), which a fleet worker
    /// hands back beside them so a search run as volunteer work can be credited.
    pub(crate) work: std::sync::atomic::AtomicU64,
    /// Where the page reads progress: builds scored (`enumerated`) and fights
    /// run (`sims_done`), advanced a chunk at a time so a slow host shows it.
    pub(crate) progress: Option<&'a wfsim_optimizer::FunnelState>,
    /// Leading the browser fleet: scores come from [`FLEET`], and a miss
    /// pauses the start that asked rather than being simulated here.
    pub(crate) lead: bool,
    /// How many of the eight main slots a build may fill (`build_size`).
    pub(crate) mod_slots: usize,
    /// THE SCREEN: a step's new candidates first fight `screen_ratio × runs`,
    /// and the best `keep_ratio` of them go on to `runs`. 0 = no screen.
    pub(crate) screen_ratio: f64,
    pub(crate) keep_ratio: f64,
    /// Each build's element orders, by key: legality asks for them and so does
    /// every measurement.
    pub(crate) orders_of: std::sync::Mutex<OrderCache>,
    /// The screen's runs, per element order, by key — what the full
    /// measurement continues rather than fights again.
    pub(crate) screened_runs: std::sync::Mutex<HashMap<String, Vec<Option<Shard>>>>,
    /// The full measurement's shard at the best element order, by key: a
    /// contender's summary, and what a fleet worker hands its leader for it.
    pub(crate) best_runs: std::sync::Mutex<HashMap<String, Shard>>,
}

/// THE STEP SCREEN (docs/OPTIMIZER.md, "Each step is screened"): a step's new
/// candidates first fight this share of `candidate_runs`, and this share of
/// them is measured in full. Over five weapons at 0.2 / 0.2 every answer was
/// the unscreened search's own, for a quarter to a third of its full
/// evaluations.
pub(crate) const SCREEN_RATIO: f64 = 0.2;
pub(crate) const KEEP_RATIO: f64 = 0.2;

/// Jobs scored between progress updates: small enough that a single-threaded
/// browser worker reports every few seconds, large enough to keep cores busy.
const PROGRESS_CHUNK: usize = 32;

type OrderScore = (Score, usize, f64);

struct MeasuredOrders {
    score: OrderScore,
    shards: Vec<Option<Shard>>,
}

/// Legality is cheap to retain for the whole solve. Resolved panels are large:
/// keep the last two batches of orders rather than every panel ever examined.
#[derive(Default)]
pub(crate) struct OrderCache {
    legal: HashMap<String, bool>,
    recent: HashMap<String, Arc<Vec<Candidate>>>,
    previous: HashMap<String, Arc<Vec<Candidate>>>,
    panels: usize,
}

impl OrderCache {
    const GENERATION: usize = 512;

    fn get(&self, key: &str) -> Option<Arc<Vec<Candidate>>> {
        self.recent.get(key).or_else(|| self.previous.get(key)).cloned()
    }

    fn insert(&mut self, key: &str, orders: Arc<Vec<Candidate>>) {
        self.legal.insert(key.to_owned(), !orders.is_empty());
        let size = orders.len().max(1);
        if size > Self::GENERATION { return; }
        if self.panels + size > Self::GENERATION {
            std::mem::swap(&mut self.recent, &mut self.previous);
            self.recent.clear();
            self.panels = 0;
        }
        self.panels += size;
        self.recent.insert(key.to_owned(), orders);
    }
}

fn split_rank(s: &str) -> (&str, Option<u64>) {
    match s.rsplit_once('@') {
        Some((c, r)) if r.bytes().all(|b| b.is_ascii_digit()) => (c, r.parse().ok()),
        _ => (s, None),
    }
}

impl QuickCtx<'_> {
    fn variant(&self, b: &QBuild) -> Option<usize> {
        self.variants.iter().position(|&(m, e, l, a)| m == b.mode && e == b.evo && l == b.val && a == b.assembly)
    }

    /// Every distinct element order of the build that can exist — empty when
    /// none can: no such variant, a card its evolutions refuse, or Forma
    /// cannot fit it.
    fn orders_by_key(&self, b: &QBuild, key: &str) -> Arc<Vec<Candidate>> {
        if let Some(os) = self.orders_of.lock().ok().and_then(|m| m.get(key)) {
            return os;
        }
        let os = Arc::new(self.orders_now(b));
        if let Ok(mut m) = self.orders_of.lock() {
            m.insert(key, os.clone());
        }
        os
    }

    fn orders_now(&self, b: &QBuild) -> Vec<Candidate> {
        let Some(vi) = self.variant(b) else { return Vec::new() };
        let Some(&stance) = self.stances.get(b.stance) else { return Vec::new() };
        let base = &self.forms[vi];
        if !super::stance::supported(self.weapon_id, base.form, stance.map(|i| &self.pool[i])) { return Vec::new(); }
        let mut subset: Vec<usize> = b.mods.iter().flatten().copied().collect();
        subset.extend(stance);
        let forbids = &self.variant_forbids[self.variants[vi].1];
        if subset.iter().any(|&i| forbids[i]) {
            return Vec::new();
        }
        subset.sort_unstable();
        let cap = super::stance::capacity(self.weapon_id, stance.map(|i| &self.pool[i]));
        // THE BUILD'S OWN EXILUS OPTION ONLY: every other one is another build,
        // planned and resolved for nothing.
        let exilus = match self.exilus_refs.get(b.exilus) {
            Some(x) => std::slice::from_ref(x),
            None if self.exilus_refs.is_empty() && b.exilus == 0 => &[],
            None => return Vec::new(),
        };
        let mut out = Vec::new();
        if b.fixed_slots != 0 {
            let x = exilus.first().copied().flatten();
            out = wfsim_optimizer::expand_with_slots(self.pool, base, vi as u32, cap,
                self.innate, x, &subset, &b.mods, b.fixed_slots, &self.scenario.arena.tenno, self.scenario.policy);
            for c in &mut out { c.exilus = b.exilus as u32; }
            return out;
        }
        wfsim_optimizer::expand_one(
            self.pool,
            base,
            None,
            vi as u32,
            cap,
            self.innate,
            exilus,
            &subset,
            &self.scenario.arena.tenno,
            self.scenario.policy,
            &mut out,
        );
        for c in &mut out {
            c.exilus = b.exilus as u32;
        }
        out
    }

    /// The build at its best element order, once scored; before that, the
    /// first order that can exist.
    pub(crate) fn materialize(&self, b: &QBuild) -> Option<Candidate> {
        let (orders, index) = self.selected_order(b);
        orders.get(index).cloned()
    }

    fn selected_order(&self, b: &QBuild) -> (Arc<Vec<Candidate>>, usize) {
        let key = self.key(b);
        let at = if self.lead { FLEET.with(|f| f.borrow().scores.get(&key).map(|x| x.1)) }
            else { self.best.lock().ok().and_then(|m| m.get(&key).copied()) };
        (self.orders_by_key(b, &key), at.unwrap_or(0))
    }

    /// The runs a full score fought at the build's best order, wherever they
    /// were fought.
    fn kept_runs(&self, key: &str) -> Option<Shard> {
        if self.lead {
            return FLEET.with(|f| f.borrow().best_runs.get(key).cloned()?.open());
        }
        self.best_runs.lock().ok().and_then(|m| m.get(key).cloned())
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
        self.required.iter().all(|r| b.mods.contains(&Some(*r))) && self.final_required.iter().all(|id|
            b.mods.iter().flatten().any(|&i| self.pool[i].id == id)
                || self.exilus_defs[b.exilus].as_ref().is_some_and(|m| m.id == id))
    }

    /// The build as `/api/candidates` reads it.
    fn candidate_request(&self, b: &QBuild, p: Position) -> Value {
        let mut slots: Vec<Value> = b.mods.iter().map(|m| m.map_or(Value::Null, |i| json!(self.pool[i].id))).collect();
        slots.push(self.exilus_defs[b.exilus].as_ref().map_or(Value::Null, |m| json!(m.id)));
        slots.push(self.stances[b.stance].map_or(Value::Null, |i| json!(self.pool[i].id)));
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
            "assembly": self.assemblies[b.assembly],
            "rivens": self.request.get("rivens").cloned().unwrap_or(Value::Null),
            "axis": { "kind": p.kind, "idx": p.idx },
        })
    }

    /// The seat list a candidate names, spelled the way the plan's arcane sets
    /// are: `id@rank` below the card's max, `none` for an empty seat.
    fn arcane_set_of(&self, ids: &[Value], ranks: &[Value]) -> Option<usize> {
        self.arcane_sets.iter().position(|set| set.iter().enumerate().all(|(i, a)| arcane_matches(ids, ranks, i, a)))
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
                if i < 8 {
                    b.mods[i] = self.pool.iter().position(|m| m.id == id).filter(|&p| self.usable[p]);
                } else if let Some(x) = self.exilus_defs.iter().position(|x| x.as_ref().is_some_and(|m| m.id == id)) {
                    b.exilus = x;
                }
            }
            if let Some(id) = slots.get(9).and_then(Value::as_str) {
                if let Some(i) = self.stances.iter().position(|m| m.is_some_and(|i| self.pool[i].id == id)) { b.stance = i; }
            } else if self.stances.len() > 1 {
                unnamed.push(Position { kind: "mods", idx: 9 });
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
            for p in s.get("fixed").and_then(Value::as_array).into_iter().flatten() {
                if p["kind"] == "mods" {
                    if let Some(i) = p["idx"].as_u64().filter(|&i| i < 8) {
                        if p["placement"] == "slot" || b.mods[i as usize].is_none() { b.fixed_slots |= 1 << i; }
                    }
                }
            }
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
                        "assembly" => "assembly",
                        _ => return None,
                    };
                    Some(Position { kind, idx: p.get("idx").and_then(Value::as_u64)? as usize })
                })
                .collect();
            if let Some((i, _)) = self.arcane_sets.iter().enumerate().filter(|(_, set)| set.iter().enumerate()
                .all(|(i, a)| !fixed.contains(&Position { kind: "arcane", idx: i }) || arcane_matches(&ids, &ranks, i, a)))
                .max_by_key(|(i, set)| (set.iter().enumerate().filter(|(i, a)| arcane_matches(&ids, &ranks, *i, a)).count(), std::cmp::Reverse(*i))) { b.arcane = i; }
            if let Some(a) = s.get("assembly") {
                let matches = |x: &Value| {
                    (!fixed.contains(&Position { kind: "assembly", idx: 0 }) || x.get("grip") == a.get("grip"))
                        && (!fixed.contains(&Position { kind: "assembly", idx: 1 }) || x.get("loader").or_else(|| x.get("link")) == a.get("loader").or_else(|| a.get("link")))
                };
                if let Some((i, _)) = self.assemblies.iter().enumerate().filter(|(_, x)| matches(x)).min_by_key(|(_, x)|
                    usize::from(x.get("grip") != a.get("grip")) + usize::from(x.get("loader").or_else(|| x.get("link")) != a.get("loader").or_else(|| a.get("link")))) { b.assembly = i; }
            }
            self.seed_required(&mut b, &fixed);
            if !fixed.iter().any(|p| p.kind == "evo") && b.mods.iter().flatten().any(|&i| self.variant_forbids[b.evo][i]) {
                let hard = |slot: usize, i: usize| fixed.contains(&Position { kind: "mods", idx: slot })
                    || self.required.contains(&i) || self.final_required.iter().any(|r| r == self.pool[i].id);
                if let Some(e) = self.variant_forbids.iter().position(|f| b.mods.iter().enumerate()
                    .all(|(slot, m)| m.is_none_or(|i| !hard(slot, i) || !f[i]))) { b.evo = e; }
            }
            for (slot, m) in b.mods.iter_mut().enumerate() {
                if !fixed.contains(&Position { kind: "mods", idx: slot }) && m.is_some_and(|i|
                    self.variant_forbids[b.evo][i] && !self.required.contains(&i) && !self.final_required.iter().any(|r| r == self.pool[i].id)) { *m = None; }
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

    fn seed_required(&self, b: &mut QBuild, fixed: &[Position]) {
        let ids: Vec<&str> = self.required.iter().filter(|&&i| self.pool[i].stance.is_none()).map(|&i| self.pool[i].id)
            .chain(self.final_required.iter().map(String::as_str)).collect();
        let pinned = |i| fixed.contains(&Position { kind: "mods", idx: i });
        while b.mods.iter().flatten().count() > self.mod_slots {
            let Some(i) = (0..8).rev().find(|&i| !pinned(i) && b.mods[i].is_some_and(|m| !ids.contains(&self.pool[m].id))) else { break };
            b.mods[i] = None;
        }
        for id in &ids {
            if b.mods.iter().flatten().any(|&i| self.pool[i].id == *id)
                || self.exilus_defs[b.exilus].as_ref().is_some_and(|m| m.id == *id) { continue; }
            if !pinned(8) && !self.required.iter().any(|&i| self.pool[i].id == *id)
                && !self.exilus_defs[b.exilus].as_ref().is_some_and(|m| ids.contains(&m.id)) {
                if let Some(x) = self.exilus_defs.iter().position(|m| m.as_ref().is_some_and(|m| m.id == *id)) {
                    b.exilus = x;
                    continue;
                }
            }
            let pi = self.pool.iter().position(|m| m.id == *id);
            // Displace an unfixed family conflict before trying a free main slot.
            if let Some(pi) = pi {
                for i in 0..8 {
                    if !pinned(i) && b.mods[i].is_some_and(|m| self.clashes_with(&self.pool[pi], &self.pool[m])) {
                        b.mods[i] = None;
                    }
                }
            }
            let slot = (0..8).filter(|&i| !pinned(i))
                .filter(|&i| b.mods[i].is_some() || b.mods.iter().flatten().count() < self.mod_slots)
                .filter(|&i| b.mods[i].is_none_or(|m| !ids.contains(&self.pool[m].id)))
                .min_by_key(|&i| b.mods[i].is_some());
            match (pi, slot) {
                (Some(pi), Some(i)) => b.mods[i] = Some(pi),
                _ if !pinned(8) => {
                    if let Some(x) = self.exilus_defs.iter().position(|m| m.as_ref().is_some_and(|m| m.id == *id)) { b.exilus = x; }
                }
                _ => {}
            }
        }
    }

    fn clashes_with(&self, a: &ModDef, b: &ModDef) -> bool {
        a.family.unwrap_or_else(|| a.id.split('@').next().unwrap_or(a.id))
            == b.family.unwrap_or_else(|| b.id.split('@').next().unwrap_or(b.id))
    }

    /// The evolution tiers the scope ranges over, in tier order.
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
            stance: self.stances.iter().position(Option::is_none).unwrap_or(0),
            arcane: 0,
            evo: 0,
            mode: 0,
            val: 0,
            assembly: 0,
            fixed_slots: 0,
        }
    }

    /// User builds, legacy subset starts, or the selected strategy's blank input.
    fn initial_starts(
        &self,
        given: &[wfsim_optimizer::descent::Start],
        seeds: Vec<wfsim_optimizer::descent::Start>,
    ) -> Vec<QuickStart<QBuild>> {
        let builds = self.build_starts();
        if !builds.is_empty() {
            return builds;
        }
        let list = if !given.is_empty() { given.to_vec() }
            else if self.request["search_strategy"] == "four_elements" {
                vec![wfsim_optimizer::descent::Start::default()]
            } else { seeds };
        let arcane_seats: Vec<Position> = (0..self.arcane_sets.first().map_or(0, Vec::len))
            .map(|idx| Position { kind: "arcane", idx })
            .collect();
        list.into_iter()
            .map(|s| {
                let mut b = self.blank();
                let mut order: Vec<usize> = self.required.iter().copied().filter(|&i| self.pool[i].stance.is_none()).collect();
                let mut fixed = Vec::new();
                if let Some(i) = s.mods.iter().find(|&&i| self.pool[i].stance.is_some()) {
                    if let Some(si) = self.stances.iter().position(|m| *m == Some(*i)) {
                        b.stance = si;
                        if s.locked.contains(i) { fixed.push(Position { kind: "mods", idx: 9 }); }
                    }
                }
                order.extend(s.mods.iter().copied().filter(|m| !self.required.contains(m) && self.pool[*m].stance.is_none()));
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
                self.seed_required(&mut b, &fixed);
                let unnamed = if self.stances.len() > 1 { vec![Position { kind: "mods", idx: 9 }] } else { Vec::new() };
                QuickStart { build: b, fixed, unnamed }
            })
            .collect()
    }

    pub(crate) fn starts(
        &self,
        given: &[wfsim_optimizer::descent::Start],
        seeds: Vec<wfsim_optimizer::descent::Start>,
    ) -> Vec<SearchBranch> {
        let selected = self.request["search_strategy"] == "four_elements";
        let mut branches = Vec::new();
        for (origin, start) in self.initial_starts(given, seeds).into_iter().enumerate() {
            let mut seen = HashSet::from([self.key(&start.build)]);
            branches.push(SearchBranch {
                start: QuickStart { build: start.build.clone(), fixed: start.fixed.clone(), unnamed: start.unnamed.clone() },
                origin, element: selected.then(|| "original".into()),
            });
            if selected {
                for row in &wfsim_engine::data::mods::search_presets().element_starts {
                    let Some(card) = row.iter().find_map(|id| self.pool.iter().enumerate()
                        .find(|(i, m)| m.id == id && self.usable[*i]).map(|(i, _)| i)) else { continue };
                    let element = self.pool[card].effects.iter().find_map(|e| match e {
                        wfsim_engine::model::ModEffect::Element(t, _) => Some(format!("{t:?}").to_lowercase()),
                        _ => None,
                    });
                    if start.build.mods.contains(&Some(card)) { continue; }
                    let protected = |slot: usize| start.fixed.contains(&Position { kind: "mods", idx: slot })
                        || start.build.mods[slot].is_some_and(|m| self.required.contains(&m)
                            || self.final_required.iter().any(|id| id == self.pool[m].id));
                    let conflicts: Vec<usize> = (0..8).filter(|&i| start.build.mods[i]
                        .is_some_and(|m| self.clashes_with(&self.pool[card], &self.pool[m]))).collect();
                    if conflicts.iter().any(|&i| protected(i)) { continue; }
                    let mut base = start.build.clone();
                    for &i in &conflicts { base.mods[i] = None; }
                    let open = (0..8).find(|&i| !protected(i) && base.mods[i].is_none());
                    let slots: Vec<usize> = if base.mods.iter().flatten().count() < self.mod_slots {
                        open.into_iter().collect()
                    } else {
                        (0..8).filter(|&i| !protected(i) && base.mods[i].is_some()).collect()
                    };
                    for slot in slots {
                        let mut b = base.clone();
                        b.mods[slot] = Some(card);
                        if self.clashes(&b, slot, &self.pool[card]) { continue; }
                        if b.mods.iter().flatten().any(|&i| self.variant_forbids[b.evo][i]) {
                            if start.fixed.iter().any(|p| p.kind == "evo") { continue; }
                            let Some(evo) = self.variant_forbids.iter().position(|f| b.mods.iter().flatten().all(|&i| !f[i]))
                                else { continue };
                            b.evo = evo;
                        }
                        if seen.insert(self.key(&b)) {
                            branches.push(SearchBranch { origin, element: element.clone(),
                                start: QuickStart { build: b, fixed: start.fixed.clone(), unnamed: start.unnamed.clone() } });
                        }
                    }
                }
            }
        }
        branches
    }
}

impl QuickSpace for QuickCtx<'_> {
    type Build = QBuild;

    /// WHAT SHAPES THE WEAPON FIRST: its parts, how it is played, its evolutions tier by
    /// tier, its element, its arcane — then the cards, then the exilus. A card
    /// chosen before the mode or the evolutions is chosen for another weapon:
    /// on Burston Prime the cycle doubled the score and half the work before it
    /// had been spent on the base form.
    fn positions(&self, _: &QBuild) -> Vec<Position> {
        let mut out: Vec<Position> = Vec::new();
        if self.assemblies.len() > 1 {
            out.extend((0..2).map(|idx| Position { kind: "assembly", idx }));
        }
        if self.stances.len() > 1 {
            out.push(Position { kind: "mods", idx: 9 });
        }
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
        out.extend((0..8).map(|idx| Position { kind: "mods", idx }));
        if self.exilus_defs.len() > 1 {
            out.push(Position { kind: "mods", idx: 8 });
        }
        out
    }

    /// WHAT SHAPES THE WEAPON — mode, evolution tiers, valence, arcane seats:
    /// a handful of options each, so a pair of them costs about one card
    /// slot's sweep. Two card slots at once would be C(8,2) x 85^2 builds.
    fn coupled(&self, b: &QBuild) -> Vec<Position> {
        self.positions(b).into_iter().filter(|p| p.kind != "mods" || p.idx == 9).collect()
    }

    fn empty(&self, b: &QBuild) -> Vec<Position> {
        let mut out: Vec<Position> = (0..8)
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
                let key = self.key(b);
                match f.scores.get(&key) {
                    Some(&(s, _, _)) => out.push(s),
                    None => {
                        f.missed = true;
                        // ONE ITEM A KEY: the same cards in another slot order are
                        // the same build, and two items of it split work by slicing.
                        if f.pending_keys.insert(key.clone()) {
                            f.pending.push((b.clone(), key));
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

    fn defer_results(&self) -> bool {
        self.lead && FLEET.with(|f| {
            let f = f.borrow();
            !f.pending.is_empty() || !f.pending_rough.is_empty()
        })
    }

    /// Below two fights there is no spread to rank by, and at `runs` or more
    /// the screen is the full measurement — either way, no screen.
    fn screen(&self, bs: &[QBuild]) -> Option<Vec<Score>> {
        let short = (self.screen_ratio * f64::from(self.runs)).ceil() as u32;
        if self.keep_ratio <= 0.0 || short < 2 || short >= self.runs {
            return None;
        }
        if !self.lead {
            return Some(self.score_orders_at(bs, short).into_iter().map(|(s, _, _)| s).collect());
        }
        FLEET.with(|f| {
            let mut f = f.borrow_mut();
            f.missed = false;
            let mut out = Vec::with_capacity(bs.len());
            for b in bs {
                let key = self.key(b);
                match f.rough.get(&key) {
                    Some(&s) => out.push(s),
                    None => {
                        f.missed = true;
                        if f.pending_rough_keys.insert(key) {
                            f.pending_rough.push((b.clone(), short));
                        }
                        out.push(None);
                    }
                }
            }
            Some(out)
        })
    }

    fn keep_ratio(&self) -> f64 {
        self.keep_ratio
    }

    /// The board's own order (`wfsim_engine::board::builds::preferred`), then
    /// the mode and the valence as the plan lists them.
    fn prefer(&self, a: &QBuild, b: &QBuild) -> std::cmp::Ordering {
        let order = if self.lead {
            LEAD_MEMO.with_borrow_mut(|memo| {
                let mut pref = |q| memoized(&mut memo.preferences, q, || std::rc::Rc::new(self.preference(q)));
                wfsim_engine::board::builds::preferred(&pref(a), &pref(b))
            })
        } else {
            wfsim_engine::board::builds::preferred(&self.preference(a), &self.preference(b))
        };
        order.then(a.mode.cmp(&b.mode)).then(a.val.cmp(&b.val))
    }

    /// Each free card slot, costliest card first, takes the costliest card
    /// that makes the build fit — or nothing — and the next is lowered only
    /// while it still does not. One card per drain is tried, since every card
    /// scored as one there.
    fn repair(&self, b: &QBuild, free: &[Position]) -> Option<QBuild> {
        let mut slots: Vec<usize> = free.iter().filter(|p| p.kind == "mods" && p.idx < 9).map(|p| p.idx).collect();
        slots.retain(|&i| {
            let m = if i < 8 { b.mods[i].map(|m| self.pool[m].id) } else { self.exilus_defs[b.exilus].as_ref().map(|m| m.id) };
            m.is_none_or(|id| !self.final_required.iter().any(|r| r == id))
        });
        slots.sort_by_key(|&i| std::cmp::Reverse(self.slot_drain(b, i)));
        let mut cur = b.clone();
        for i in slots {
            let now = self.slot_drain(&cur, i);
            if now == 0 {
                continue;
            }
            let mut tried = std::collections::HashSet::new();
            let mut lower: Vec<QBuild> = Vec::new();
            for (pi, m) in self.pool.iter().enumerate() {
                if i < 8 && self.usable[pi] && m.base_drain < now && !self.clashes(&cur, i, m) {
                    let mut n = cur.clone();
                    n.mods[i] = Some(pi);
                    lower.push(n);
                }
            }
            if i == 8 {
                for (xi, m) in self.exilus_defs.iter().enumerate() {
                    if m.as_ref().is_some_and(|m| m.base_drain < now && !self.clashes(&cur, 8, m)) {
                        let mut n = cur.clone();
                        n.exilus = xi;
                        lower.push(n);
                    }
                }
            }
            lower.sort_by(|x, y| self.prefer(x, y));
            lower.retain(|n| tried.insert(self.slot_drain(n, i)));
            let mut bare = cur.clone();
            if i < 8 {
                bare.mods[i] = None;
            } else {
                bare.exilus = self.exilus_defs.iter().position(Option::is_none).unwrap_or(cur.exilus);
            }
            lower.push(bare);
            match lower.into_iter().find(|n| self.legal(n)) {
                Some(n) => return Some(n),
                None => {
                    if i < 8 {
                        cur.mods[i] = None;
                    } else {
                        cur.exilus = self.exilus_defs.iter().position(Option::is_none).unwrap_or(cur.exilus);
                    }
                }
            }
        }
        None
    }

    /// Cards as a set, with prescribed physical slots kept in the identity.
    fn key(&self, b: &QBuild) -> String {
        let make = || {
            let mut mods: Vec<&str> = b.mods.iter().flatten().map(|&i| self.pool[i].id).collect();
            mods.sort_unstable();
            let slots: Vec<_> = (0..8).filter(|&i| b.fixed_slots & (1 << i) != 0).map(|i| (i, b.mods[i])).collect();
            format!("{mods:?} x{} a{} e{} m{} v{} p{} t{} s{slots:?}", b.exilus, b.arcane, b.evo, b.mode, b.val, b.assembly, b.stance)
        };
        if self.lead { LEAD_MEMO.with_borrow_mut(|m| memoized(&mut m.keys, b, make)) } else { make() }
    }

    /// The canonical form: the same cards whatever their slots, the same
    /// combined elements, the same exilus, arcane and variant.
    fn identity(&self, b: &QBuild) -> String {
        // Without a pinned slot the sorted cards and axes identify the whole order table,
        // including its shared winning index. No other table can share this identity.
        if b.fixed_slots == 0 { return self.key(b); }
        let (orders, index) = self.selected_order(b);
        let Some(c) = orders.get(index) else { return self.key(b) };
        let mut cards: Vec<&str> = b.mods.iter().flatten().map(|&i| self.pool[i].id).collect();
        cards.sort_unstable();
        let elements: Vec<String> = c
            .panel
            .damage
            .iter_nonzero()
            .map(|(t, v)| format!("{t:?}:{v:.3}"))
            .collect();
        format!("{cards:?}|{elements:?}|{}|{}|{}|{:?}|{:?}", b.exilus, b.stance, b.arcane, self.variant(b), c.slots)
    }
}

impl QuickCtx<'_> {
    /// What the board's order reads of a build.
    fn preference(&self, b: &QBuild) -> wfsim_engine::board::builds::Preference<'static> {
        let cards: Vec<&ModDef> = b
            .mods
            .iter()
            .flatten()
            .map(|&i| &self.pool[i])
            .chain(self.stances[b.stance].map(|i| &self.pool[i]))
            .chain(self.exilus_defs.get(b.exilus).and_then(Option::as_ref))
            .collect();
        let arcanes = self.arcane_sets.get(b.arcane).map_or_else(Vec::new, |s| {
            s.iter()
                .filter_map(|id| {
                    let card = id.split('@').next().unwrap_or(id);
                    let slot = wfsim_engine::data::arcanes::slot_of(card)?;
                    wfsim_engine::data::arcanes::for_slot(slot, card).map(|a| a.name.as_str())
                })
                .collect()
        });
        let mut evolutions: Vec<(u32, u32)> = self.evo_sets.get(b.evo).map_or_else(Vec::new, |s| {
            s.iter()
                .filter_map(|id| wfsim_engine::data::evolutions::get(id).map(|e| (e.tier, e.position_in_tier)))
                .collect()
        });
        evolutions.sort_unstable();
        wfsim_engine::board::builds::Preference {
            drain: cards.iter().map(|m| m.base_drain).sum(),
            cards: cards.iter().map(|m| m.name).collect(),
            arcanes,
            evolutions: evolutions.into_iter().map(|(_, p)| p).collect(),
        }
    }

    /// The drain of the card in slot `i` (8 = the exilus).
    fn slot_drain(&self, b: &QBuild, i: usize) -> u32 {
        let m = if i < 8 { b.mods[i].map(|pi| &self.pool[pi]) } else { self.exilus_defs.get(b.exilus).and_then(Option::as_ref) };
        m.map_or(0, |m| m.base_drain)
    }

    /// Would `m` in slot `i` repeat a card or a family the build holds elsewhere?
    fn clashes(&self, b: &QBuild, i: usize, m: &ModDef) -> bool {
        let fam = |m: &ModDef| m.family.unwrap_or_else(|| m.id.split('@').next().unwrap_or(m.id));
        let mine = fam(m);
        let others = b
            .mods
            .iter()
            .enumerate()
            .filter(|&(j, _)| j != i)
            .filter_map(|(_, x)| x.map(|pi| &self.pool[pi]))
            .chain(if i == 8 { None } else { self.exilus_defs.get(b.exilus).and_then(Option::as_ref) });
        others.map(fam).any(|f| f == mine)
    }

    fn candidates_now(&self, b: &QBuild, p: Position) -> Vec<QBuild> {
        if p.kind == "mods" && p.idx < 8 && b.mods[p.idx].is_none()
            && b.mods.iter().flatten().count() >= self.mod_slots { return Vec::new(); }
        let res = crate::candidates::candidates_json(&self.candidate_request(b, p));
        let mut out = Vec::new();
        for c in res["candidates"].as_array().into_iter().flatten() {
            let id = c.get("id").and_then(Value::as_str).unwrap_or("");
            let payload = &c["payload"];
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
                "mods" if p.idx == 9 => match self.stances.iter().position(|x| x.map_or(id == "none", |i| self.pool[i].id == id)) {
                    Some(i) => { n.stance = i; true }
                    None => false,
                },
                "mods" => match self.exilus_defs.iter().position(|x| x.as_ref().is_some_and(|m| m.id == id)) {
                    Some(xi) => {
                        n.exilus = xi;
                        true
                    }
                    None => false,
                },
                "arcane" => {
                    let ids = payload["arcane"].as_array().map_or(&[][..], Vec::as_slice);
                    let ranks = payload["arcane_rank"].as_array().map_or(&[][..], Vec::as_slice);
                    match self.arcane_set_of(ids, ranks) {
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
                "assembly" => {
                    let key = if p.idx == 0 { "grip" } else if self.assemblies[b.assembly].get("link").is_some() { "link" } else { "loader" };
                    match payload.get("assembly").filter(|a| a.get(key) != self.assemblies[b.assembly].get(key))
                        .and_then(|a| self.assemblies.iter().position(|x| x == a)) {
                        Some(a) => { n.assembly = a; true }
                        None => false,
                    }
                }
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
        if b.mods.iter().flatten().count() > self.mod_slots || !self.has_required(b) { return false; }
        let key = self.key(b);
        let held = self.orders_of.lock().ok().and_then(|m| m.legal.get(&key).copied());
        held.unwrap_or_else(|| !self.orders_by_key(b, &key).is_empty())
    }

    /// Every build's score at its best element order, and WHICH order that is
    /// (its index in [`Self::orders_by_key`]) — what a fleet worker hands back.
    fn score_orders(&self, bs: &[QBuild]) -> Vec<OrderScore> {
        self.score_orders_at(bs, self.runs)
    }

    /// [`Self::score_orders`] at `runs` fights a candidate, on the same seed —
    /// the screen's short count is the first `runs` of the full paired stream.
    fn score_orders_at(&self, bs: &[QBuild], runs: u32) -> Vec<OrderScore> {
        let keys: Vec<_> = bs.iter().map(|b| self.key(b)).collect();
        let orders: Vec<_> = bs.iter().zip(&keys).map(|(b,k)| self.orders_by_key(b,k)).collect();
        // A FULL MEASUREMENT KEEPS THE SCREEN'S RUNS: they are the first runs
        // of the same stream, so continuing them is the full count to the bit.
        let screened = runs != self.runs;
        let priors: Vec<Vec<Option<Shard>>> = keys
            .iter()
            .zip(&orders)
            .map(|(key, os)| {
                let kept = if screened { None } else { self.screened_runs.lock().ok().and_then(|mut m| m.remove(key)) };
                kept.filter(|k| k.len() == os.len()).unwrap_or_else(|| vec![None; os.len()])
            })
            .collect();
        let measured = self.measure_orders(bs, &orders, &priors, runs, false);
        keys.into_iter().zip(measured).map(|(key, MeasuredOrders { score, mut shards })| {
            let (s, oi, sd) = score;
            if let (Some(_), Ok(mut m)) = (s, self.best.lock()) {
                m.insert(key.clone(), oi);
            }
            if screened {
                if let Ok(mut m) = self.screened_runs.lock() {
                    m.insert(key.clone(), shards);
                }
            } else if let Some(sh) = shards.get_mut(oi).and_then(Option::take) {
                if let Ok(mut m) = self.best_runs.lock() {
                    m.insert(key.clone(), sh);
                }
            }
            if let Ok(mut m) = self.spread.lock() {
                m.insert(key, sd);
            }
            score
        }).collect()
    }

    fn measure_orders(
        &self,
        bs: &[QBuild],
        orders: &[Arc<Vec<Candidate>>],
        priors: &[Vec<Option<Shard>>],
        runs: u32,
        warm: bool,
    ) -> Vec<MeasuredOrders> {
        let jobs: Vec<(&Candidate, usize, Option<&Shard>)> = orders
            .iter()
            .zip(bs)
            .zip(priors)
            .flat_map(|((os, b), ps)| os.iter().zip(ps).map(move |(c, p)| (c, b.arcane, p.as_ref())))
            .collect();
        use std::sync::atomic::Ordering::Relaxed;
        // One seed for every chunk, so the chunks stay one paired stream.
        let mut sums = Vec::with_capacity(jobs.len());
        for chunk in jobs.chunks(PROGRESS_CHUNK) {
            let got = wfsim_optimizer::descent::evaluate_paired_onto(chunk, self.scenario, runs, self.seed);
            // WHAT THIS CALL FOUGHT, not what the summaries hold: a kept run
            // was counted when it was fought.
            let (mut fights, mut work) = (0u64, 0u64);
            for ((_, _, prior), (s, sh)) in chunk.iter().zip(&got) {
                if warm && sh.is_none() { continue; }
                fights += u64::from(runs - prior.map_or(0, |p| p.runs.min(runs)));
                work = work.saturating_add(s.work.saturating_sub(prior.map_or(0, |p| p.work())));
            }
            self.sims.fetch_add(fights, Relaxed);
            self.work.fetch_add(work, Relaxed);
            sums.extend(got);
            if let Some(p) = self.progress {
                p.sims_done.fetch_add(fights, Relaxed);
            }
            wfsim_optimizer::tick();
        }
        if let Some(p) = self.progress {
            p.enumerated.fetch_add(bs.len() as u64, Relaxed);
        }
        let mut it = sums.into_iter();
        let mut out = Vec::with_capacity(bs.len());
        for os in orders {
            let mut best: Option<(usize, (f64, f64), f64)> = None;
            let mut shards: Vec<Option<Shard>> = Vec::with_capacity(os.len());
            for (oi, _) in os.iter().enumerate() {
                let Some((s, sh)) = it.next() else { break };
                shards.push(sh);
                let sc = (s.mean_kill_progress.max(0.0), s.mean_effective_damage);
                if best.is_none_or(|(_, x, _)| sc.0.total_cmp(&x.0).then(sc.1.total_cmp(&x.1)).is_gt()) {
                    best = Some((oi, sc, s.std_kill_progress));
                }
            }
            let score = best.map_or((None, 0, 0.0), |(oi, score, sd)| (Some(score), oi, sd));
            out.push(MeasuredOrders { score, shards });
        }
        out
    }

    fn warm_identities(&self, b: &QBuild, orders: &[Candidate], replay: &Replay) -> Vec<warm::OrderIdentity> {
        orders.iter().map(|c| replay.warm_identity(c, b.arcane, self.weapon_id, self.valence_bonus, self.seed)).collect()
    }

    fn warm_keys(&self, b: &QBuild, orders: &[Candidate], replay: &Replay) -> Vec<String> {
        self.warm_identities(b, orders, replay).into_iter().map(|id| id.key).collect()
    }

    fn score_row(key: &str, (s, oi, sd): OrderScore) -> Value {
        json!({ "key": key, "score": s.map(|(k, e)| json!([k, e])), "order": oi, "sd": sd })
    }

    fn score_warm_at(&self, bs: &[QBuild], items: &[&Value], runs: u32, replay: &Replay, pack: bool) -> Vec<Value> {
        let keys: Vec<_> = bs.iter().map(|b| self.key(b)).collect();
        let orders: Vec<_> = bs.iter().zip(&keys).map(|(b, k)| self.orders_by_key(b, k)).collect();
        let identities: Vec<_> = bs.iter().zip(&orders).map(|(b, os)| self.warm_identities(b, os, replay)).collect();
        let priors: Vec<_> = identities.iter().zip(items).map(|(ids, item)| {
            let warm = item["warm_shards"].as_array();
            let carried = item["shards"].as_array().filter(|rows| rows.len() == ids.len());
            ids.iter().enumerate().map(|(i, id)| {
                warm::prefix(warm.and_then(|a| a.get(i)), carried.and_then(|a| a.get(i)), id, self.seed, runs)
            }).collect()
        }).collect();
        let measured = self.measure_orders(bs, &orders, &priors, runs, true);
        keys.into_iter().zip(identities).zip(measured).map(|((key, ids), MeasuredOrders { score, mut shards })| {
            let mut row = Self::score_row(&key, score);
            row["warm_shards"] = json!(ids.iter().zip(&shards).map(|(id, s)| {
                s.clone().map(|s| warm::pack(id, s))
            }).collect::<Vec<_>>());
            if runs != self.runs {
                row["rough"] = json!(true);
                row["shards"] = json!(shards.into_iter().map(|s| s.map(|s| Carry::new(s, pack))).collect::<Vec<_>>());
            } else if let Some(sh) = shards.get_mut(score.1).and_then(Option::take) {
                row["shard"] = json!(Carry::new(sh, pack));
            }
            row
        }).collect()
    }

    /// A fleet worker's share of a batch: each build's score and best order.
    /// An item is a build, scored at `runs`, or `{build, runs}` — a step
    /// screen's short measurement, answered with `rough: true` and its runs
    /// per element order (`shards`) — or `{build, shards}`, a full measurement
    /// that continues them, answered with its best order's runs (`shard`).
    pub(crate) fn score_json(&self, builds: &[Value], stream: &super::BuildStream, pack: bool, warm: Option<&Replay>) -> Value {
        stream.enable();
        let mut groups: Vec<(u32, Vec<QBuild>, Vec<&Value>)> = Vec::new();
        for item in builds {
            let (b, runs) = match item.get("build") {
                Some(b) => (b, item.get("runs").and_then(Value::as_u64).map_or(self.runs, |r| r as u32)),
                None => (item, self.runs),
            };
            let Some(b) = QBuild::from_json(b) else { continue };
            if warm.is_some() && item.get("runs").is_some_and(|r| {
                !r.as_u64().is_some_and(|n| (1..=u64::from(self.runs)).contains(&n))
            }) {
                return crate::err_json("invalid warm search run count");
            }
            if let Some(v) = item.get("shards").filter(|_| warm.is_none()) {
                let kept = Carry::list(v).and_then(|rows| rows.into_iter().map(|s| match s {
                    Some(s) => s.open().map(Some), None => Some(None),
                }).collect::<Option<Vec<_>>>());
                if let Some(kept) = kept {
                    if let Ok(mut m) = self.screened_runs.lock() { m.insert(self.key(&b), kept); }
                } else if v.as_array().is_some_and(|a| a.iter().any(Value::is_string)) {
                    return crate::err_json("invalid carried search shard");
                }
            }
            let input = warm.map(|_| item);
            match groups.iter_mut().find(|(r, _, _)| *r == runs) {
                Some((_, g, items)) => { g.push(b); items.extend(input); }
                None => groups.push((runs, vec![b], input.into_iter().collect())),
            }
        }
        let before = self.sims.load(std::sync::atomic::Ordering::Relaxed);
        let work_before = self.work.load(std::sync::atomic::Ordering::Relaxed);
        let mut rows: Vec<Value> = Vec::new();
        for (runs, bs, items) in &groups {
            if let Some(replay) = warm {
                rows.extend(self.score_warm_at(bs, items, *runs, replay, pack));
                continue;
            }
            let got = self.score_orders_at(bs, *runs);
            rows.extend(bs.iter().zip(got).map(|(b, score)| {
                let key = self.key(b);
                let mut row = Self::score_row(&key, score);
                if *runs != self.runs {
                    row["rough"] = json!(true);
                    if let Some(kept) = self.screened_runs.lock().ok().and_then(|mut m| m.remove(&key)) {
                        row["shards"] = json!(kept.into_iter().map(|s| s.map(|s| Carry::new(s, pack))).collect::<Vec<_>>());
                    }
                } else if let Some(sh) = self.best_runs.lock().ok().and_then(|mut m| m.remove(&key)) {
                    row["shard"] = json!(Carry::new(sh, pack));
                }
                row
            }));
        }
        let fights = self.sims.load(std::sync::atomic::Ordering::Relaxed) - before;
        let work = self.work.load(std::sync::atomic::Ordering::Relaxed) - work_before;
        json!({ "ok": true, "scores": rows, "fights": fights, "work": work,
            "builds": stream.take_all() })
    }

    /// The leader takes the scores the fleet returned; `fresh` begins a new
    /// search and forgets the last one's.
    pub(crate) fn lead_take(scores: &[Value], fresh: bool, forms: &Arc<ScopeForms>) {
        LEAD_MEMO.with_borrow_mut(|m| m.bind(forms, fresh));
        FLEET.with(|f| {
            let mut f = f.borrow_mut();
            if fresh {
                *f = Fleet::default();
            }
            f.pending.clear();
            f.pending_keys.clear();
            f.pending_rough.clear();
            f.pending_rough_keys.clear();
            for r in scores {
                let Some(key) = r.get("key").and_then(Value::as_str) else { continue };
                let s = r.get("score").and_then(Value::as_array).and_then(|a| Some((a.first()?.as_f64()?, a.get(1)?.as_f64()?)));
                if r.get("rough").and_then(Value::as_bool) == Some(true) {
                    f.rough.insert(key.to_string(), s);
                    if let Some(kept) = r.get("shards").and_then(Carry::list) {
                        f.screened_runs.insert(key.to_string(), kept);
                    }
                    continue;
                }
                f.screened_runs.remove(key);
                if let Some(sh) = r.get("shard").and_then(Carry::read) {
                    f.best_runs.insert(key.to_string(), sh);
                }
                let oi = r.get("order").and_then(Value::as_u64).unwrap_or(0) as usize;
                let sd = r.get("sd").and_then(Value::as_f64).unwrap_or(0.0);
                f.scores.insert(key.to_string(), (s, oi, sd));
            }
        });
    }

    /// The batch the paused starts wait for, or `None` when every start ran to
    /// the end on scores the leader already holds.
    pub(crate) fn lead_pending(&self, warm: Option<&Replay>) -> Option<Value> {
        FLEET.with(|f| {
            let f = f.borrow();
            (!f.pending.is_empty() || !f.pending_rough.is_empty()).then(|| {
                // A build the screen fought carries those runs, for whichever
                // worker measures it in full to continue.
                let full = f.pending.iter().map(|(b, key)| {
                    let mut item = match f.screened_runs.get(key) {
                        Some(v) => json!({ "build": b.to_json(), "shards": v }),
                        None if warm.is_some() => json!({ "build": b.to_json() }),
                        None => b.to_json(),
                    };
                    if let Some(replay) = warm {
                        item["warm_keys"] = json!(self.warm_keys(b, &self.orders_by_key(b, key), replay));
                    }
                    item
                });
                let short = f.pending_rough.iter().map(|(b, runs)| {
                    let mut item = json!({ "build": b.to_json(), "runs": runs });
                    if let Some(replay) = warm {
                        item["warm_keys"] = json!(self.warm_keys(b, &self.orders_by_key(b, &self.key(b)), replay));
                    }
                    item
                });
                json!({
                    "ok": true,
                    "pending": full.chain(short).collect::<Vec<_>>(),
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
    pub(crate) from_starts: std::collections::HashMap<QuickJobIdentity, serde_json::Value>,
    pub(crate) near: std::collections::HashMap<QuickJobIdentity, serde_json::Value>,
    pub(crate) failures: Vec<serde_json::Value>,
    pub(crate) branch_failures: Vec<serde_json::Value>,
    pub(crate) branches: u32,
}

type QuickJobIdentity = (super::JobIdentity, Option<[Option<usize>; 8]>);

impl QuickCtx<'_> {
    /// What turns `from` into `to`, one entry per changed position: a card
    /// swapped (either side `null` when the counts differ), the exilus, an
    /// arcane seat, an evolution tier, the mode, the valence.
    fn changes(&self, from: &QBuild, to: &QBuild) -> Vec<Value> {
        let mut out = Vec::new();
        let cards = |b: &QBuild| -> Vec<&str> {
            let mut v: Vec<&str> = b.mods.iter().flatten().copied().chain(self.stances[b.stance]).map(|i| self.pool[i].id).collect();
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
        for key in ["grip", "loader", "link"] {
            if self.assemblies[from.assembly].get(key) != self.assemblies[to.assembly].get(key) {
                out.push(json!({"axis": "assembly", "from": self.assemblies[from.assembly].get(key), "to": self.assemblies[to.assembly].get(key)}));
            }
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
    starts: Vec<SearchBranch>,
    max_evals: u64,
    state: Option<&wfsim_optimizer::FunnelState>,
    shard: u32,
    shards: u32,
    space: u128,
    finalists: usize,
) -> (Vec<wfsim_optimizer::ScreenedJob>, wfsim_optimizer::search::SearchStats, QuickReport) {
    let shards = shards.max(1) as usize;
    let global = |j: usize| j * shards + shard as usize % shards;
    let origins: Vec<(usize, Option<String>)> = starts.iter().map(|b| (b.origin, b.element.clone())).collect();
    let mine: Vec<QuickStart<QBuild>> = starts
        .into_iter()
        .enumerate()
        .filter(|(i, _)| i % shards == shard as usize % shards)
        .map(|(_, s)| s.start)
        .collect();
    let stop: Vec<&std::sync::atomic::AtomicBool> =
        state.map(|s| vec![&s.cancel, &s.stop_enumeration]).unwrap_or_default();
    let cfg = wfsim_optimizer::quick::QuickConfig { max_evals, stop };
    let wfsim_optimizer::quick::QuickRun { answers, failures, stats: qs, progress, pool } =
        wfsim_optimizer::quick::quick_descent(ctx, &mine, &cfg);
    if ctx.lead {
        let rows: Vec<Value> = progress
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut row = json!({ "settled": p.settled, "pass": p.pass, "passes": p.passes, "round": p.round, "at": p.at, "of": p.of });
                if let Some((origin, Some(element))) = origins.get(global(i)) {
                    row["start"] = json!(origin);
                    row["branch"] = json!(element);
                    row["branch_index"] = json!(global(i));
                }
                row
            })
            .collect();
        FLEET.with(|f| f.borrow_mut().progress = json!(rows));
    }
    let stats = wfsim_optimizer::search::SearchStats {
        space,
        sampled: u128::from(qs.evals),
        subsets: qs.evals,
        candidates: qs.evals,
        // SIMULATED ENGAGEMENTS, which is what the walk's count means: every
        // order of every build scored, at `runs` each.
        evals: ctx.sims.load(std::sync::atomic::Ordering::Relaxed),
        exhaustive: false,
        starts: (0..qs.starts as usize).map(|i| origins[global(i)].0).collect::<HashSet<_>>().len() as u32,
        cut: qs.cut,
    };
    if ctx.defer_results() {
        return (Vec::new(), stats, QuickReport::default());
    }
    let failed: HashSet<usize> = failures.iter().map(|f| f.start).collect();
    let mut report = QuickReport {
        branches: qs.starts,
        failures: failures.iter().filter(|f| {
            let (origin, element) = &origins[global(f.start)];
            element.is_none() || (shards == 1
                && !answers.iter().any(|a| a.starts.iter().any(|&s| origins[global(s)].0 == *origin))
                && (0..mine.len()).filter(|&s| origins[global(s)].0 == *origin).all(|s| failed.contains(&s)))
        }).map(|f| json!({ "start": origins[global(f.start)].0, "why": f.why })).collect(),
        branch_failures: failures.iter().filter(|f| origins[global(f.start)].1.is_some())
            .map(|f| json!({ "start": origins[global(f.start)].0,
                "branch": origins[global(f.start)].1, "why": f.why })).collect(),
        ..Default::default()
    };
    let mut reported = HashSet::new();
    report.failures.retain(|f| reported.insert(f["start"].clone()));
    let scored: Vec<&(QBuild, Score)> = pool.iter().filter(|(_, s)| s.is_some()).collect();
    let field: Vec<(f64, f64)> =
        scored.iter().map(|(b, s)| (s.map_or(0.0, |(kp, _)| kp), ctx.spread_of(b))).collect();
    let keep = wfsim_optimizer::tied_at_the_line(&field, finalists, ctx.runs);
    let mut contenders: Vec<&QBuild> = scored[..keep].iter().map(|(b, _)| b).collect();
    let answer_ids: Vec<_> = answers.iter().map(|a| ctx.identity(&a.build)).collect();
    let mut ids: Vec<_> = contenders.iter().map(|b| ctx.identity(b)).collect();
    for (a, id) in answers.iter().zip(&answer_ids).filter(|(a, _)| a.score.is_some()) {
        if !ids.contains(id) { contenders.push(&a.build); ids.push(id.clone()); }
    }
    // A runner-up is described against an answer ON THE LIST, so the row it
    // names is one the reader can see. The best answer always is: a sweep that
    // scored a better build would have moved to it.
    let owners: Vec<_> = ids.iter().map(|id| answer_ids.iter().position(|a| a == id)).collect();
    let listed: Vec<_> = owners.iter().flatten().copied().collect();
    let mut built: Vec<(Candidate, usize)> = Vec::new();
    let mut built_keys: Vec<String> = Vec::new();
    for (&b, owner) in contenders.iter().zip(owners) {
        let Some(c) = ctx.materialize(b) else { continue };
        built_keys.push(ctx.key(b));
        let job = ((c.ordered.clone(), c.variant, c.exilus, b.arcane), c.slots);
        match owner {
            Some(ai) => {
                let a = &answers[ai];
                let lanes: Vec<Value> = a
                    .starts
                    .iter()
                    .enumerate()
                    .map(|(k, &s)| {
                        let mut lane = json!({
                            "start": origins[global(s)].0,
                            "from": a.from.get(k).copied().flatten().map(|(kp, _)| kp),
                            "moves": a.moves.get(k).copied().unwrap_or(0),
                        });
                        if let Some(element) = &origins[global(s)].1 {
                            lane["branch"] = json!(element);
                        }
                        lane
                    })
                    .collect();
                report.from_starts.insert(job, json!(lanes));
            }
            None => {
                let near: Vec<usize> = if listed.is_empty() { (0..answers.len()).collect() } else { listed.clone() };
                if let Some((a, changes)) =
                    near.into_iter().map(|ai| (&answers[ai], ctx.changes(&answers[ai].build, b))).min_by_key(|(_, c)| c.len())
                {
                    let mut starts: Vec<usize> = a.starts.iter().map(|&s| origins[global(s)].0).collect();
                    starts.sort_unstable();
                    starts.dedup();
                    report.near.insert(job, json!({ "starts": starts, "changes": changes }));
                }
            }
        }
        built.push((c, b.arcane));
    }
    // A CONTENDER'S SUMMARY IS ITS FULL SCORE'S RUNS — the same order on the
    // same stream — so only one whose runs nobody kept is fought again.
    let kept: Vec<Option<Summary>> = built
        .iter()
        .zip(&built_keys)
        .map(|((c, ai), k)| {
            let sh = ctx.kept_runs(k).filter(|sh| sh.runs == ctx.runs)?;
            Some(sh.finish(&(ctx.scenario.params)(c, *ai)?, ctx.runs).0)
        })
        .collect();
    let jobs: Vec<(&Candidate, usize)> =
        built.iter().zip(&kept).filter(|(_, k)| k.is_none()).map(|((c, a), _)| (c, *a)).collect();
    let mut fought = wfsim_optimizer::descent::evaluate_paired(&jobs, ctx.scenario, ctx.runs, ctx.seed).into_iter();
    let sums: Vec<Summary> = kept.into_iter().filter_map(|k| k.or_else(|| fought.next())).collect();
    let screened = built
        .into_iter()
        .zip(sums)
        .map(|((c, ai), summary)| wfsim_optimizer::ScreenedJob { cand: std::sync::Arc::new(c), ai, summary })
        .collect();
    (screened, stats, report)
}

/// THE QUICK CALC'S WHOLE SCOPE, for a quick request that names none: every
/// card, exilus card, arcane, evolution, mode, element and part the weapon takes, and
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
    // An unknown weapon is `parse_fight`'s to refuse, right after this.
    let Ok(info) = crate::registry::weapon_of(v) else { return None };
    let every = wfsim_engine::data::mods::every_rank();
    let every_list = |k: &str, dflt: &[String]| -> Vec<String> {
        v.get("every_rank")
            .and_then(|e| e.get(k))
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_else(|| dflt.to_vec())
    };
    let every_mods = every_list("mods", &every.mods);
    let required = super::requirements::ids(v);
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
    let pinned_empty = |kind: &str, idx: usize, field: &str| v.get("starts").and_then(Value::as_array).is_some_and(|a| a.iter().any(|s|
        s.get("fixed").and_then(Value::as_array).is_some_and(|f| f.iter().any(|p| p["kind"] == kind && p["idx"] == idx))
            && s.get(field).and_then(|a| a.get(idx)).and_then(Value::as_str).is_none_or(|id| id == "none")));
    let mut mods = serde_json::Map::new();
    let mut exilus = serde_json::Map::new();
    for m in pool.iter().filter(|m| m.stance.is_none()) {
        let ids = ranked(m.id, m.max_rank, &every_mods).into_iter().chain(required.iter().filter(|id|
            wfsim_engine::data::mods::split_rank(id).0 == m.id).cloned());
        for id in ids.filter(|id| !no_mods.contains(id)) {
            mods.insert(id.clone(), json!("search"));
            if m.exilus {
                exilus.insert(id, json!("search"));
            }
        }
    }
    // The exilus left empty: its one option is the empty slot — and on a
    // weapon that has none (an Arch-Gun, a companion's), the only one there is.
    if limits.get("exilus").and_then(Value::as_bool) == Some(false)
        || !wfsim_engine::data::weapons::has_exilus_slot(&info.id)
    {
        exilus = serde_json::Map::from_iter([("none".to_string(), json!("fixed"))]);
    } else if pinned_empty("mods", 8, "slots") {
        exilus.insert("none".into(), json!("search"));
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
        if pinned_empty("arcane", i, "arcane") {
            arcanes.insert(format!("none:{seat}"), json!("search"));
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
        // Sent even when empty: a tier with every option excluded is refused
        // by `parse_optimize`, never quietly given its default.
        evolutions.insert(t.to_string(), json!(ids));
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
    if let Some(spec) = wfsim_engine::data::weapons::spec(&info.id).filter(|s| wfsim_engine::data::weapons::assembly::is_modular(s)) {
        let meta = crate::meta::assembly_meta(&spec.id);
        let component = meta.get("component").and_then(Value::as_str).unwrap_or("loader");
        let mut parts = Vec::new();
        for g in meta["grips"].as_array().into_iter().flatten() {
            for c in meta[format!("{component}s")].as_array().into_iter().flatten() {
                parts.push(json!({"grip": g["id"], component: c["id"]}));
            }
        }
        o.insert("assemblies".into(), json!(parts));
    }
    o.insert("build_size".into(), json!(most));
    o.insert("build_min".into(), json!(0));
    Some(out)
}

/// Explicit scopes name complete pairs; the ordinary quick scope fills every legal pair.
pub(crate) fn assemblies(v: &Value, weapon: &str) -> Result<Vec<Value>, Value> {
    let fallback = crate::weapon_options::assembly_of(v, weapon).map_or(Value::Null, |a| a.parts());
    let Some(list) = v.get("assemblies").and_then(Value::as_array) else { return Ok(vec![fallback]) };
    if list.is_empty() { return Err(super::err_json("no assemblies in search scope")); }
    let mut out = Vec::new();
    for a in list {
        let r = json!({"assembly": a});
        let actual = crate::weapon_options::assembly_of(&r, weapon).map_or(Value::Null, |a| a.parts());
        if *a != actual { return Err(super::err_json("invalid assembly in search scope")); }
        if !out.contains(a) { out.push(a.clone()); }
    }
    Ok(out)
}

pub(super) fn arcane_matches(ids: &[Value], ranks: &[Value], i: usize, option: &str) -> bool {
    let (id, r) = split_rank(ids.get(i).and_then(Value::as_str).unwrap_or("none"));
    let max = |id: &str| wfsim_engine::data::arcanes::secondary(id).map_or(0, |a| u64::from(a.max_rank));
    let rank = ranks.get(i).and_then(Value::as_u64).or(r).unwrap_or_else(|| max(id));
    let (card, r) = split_rank(option);
    id == card && rank == r.unwrap_or_else(|| max(card))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_panels_do_not_forget_legality() {
        let mut cache = OrderCache::default();
        for i in 0..(2 * OrderCache::GENERATION + 1) { cache.insert(&i.to_string(), Arc::new(Vec::new())); }
        assert!(cache.get("0").is_none());
        assert_eq!(cache.legal.get("0"), Some(&false));
        assert!(cache.get(&OrderCache::GENERATION.to_string()).is_some());
        assert_eq!(cache.recent.len() + cache.previous.len(), OrderCache::GENERATION + 1);
    }

    #[test]
    fn leader_metadata_is_bounded_and_never_reuses_another_scopes_indices() {
        let (old, new) = (Arc::new(ScopeForms::new()), Arc::new(ScopeForms::new()));
        let b = QBuild::from_json(&json!([-1,-1,-1,-1,-1,-1,-1,-1,0,0,0,0,0,0,0,0])).unwrap();
        let mut m = LeadMemo::default();
        m.bind(&old, false);
        for mode in 0..4097 {
            memoized(&mut m.keys, &QBuild { mode, ..b.clone() }, || mode.to_string());
        }
        assert_eq!(m.keys.len(), 1);
        m.bind(&old, false);
        assert_eq!(m.keys.len(), 1);
        m.bind(&new, false);
        assert!(m.keys.is_empty());
        memoized(&mut m.keys, &b, || String::from("new scope"));
        m.bind(&new, true);
        assert!(m.keys.is_empty());
    }

    fn request() -> Value {
        json!({
            "weapon": "braton", "enemy": "thrax_centurion", "level": 100,
            "duration": 1.0, "runs": 2, "candidate_runs": 10, "finalists": 1,
            "strategy": "quick", "build_size": 3,
            "mods": { "hellfire": "search", "infected_clip": "search", "stormbringer": "search" },
            "starts": [{ "slots": ["hellfire", "infected_clip", "stormbringer"],
                "fixed": [{"kind":"mods","idx":0}, {"kind":"mods","idx":1}, {"kind":"mods","idx":2}] }],
        })
    }

    fn run(req: &Value, fleet: Option<Value>) -> Value {
        let mut req = req.clone();
        if let Some(f) = fleet { req["quick_fleet"] = f; }
        let out = super::super::run_optimize(
            super::super::parse_optimize(&req).expect("plan"),
            &wfsim_optimizer::FunnelState::default(), |_, _| {}, None,
        );
        assert_eq!(out["ok"], json!(true), "{out}");
        out
    }

    fn warm_lead(req: &Value) -> Value {
        let out = run(req, Some(json!({"lead": true, "fresh": true, "warm_cache": 1})));
        assert_eq!(out["pending"].as_array().unwrap().len(), 1);
        out["pending"][0].clone()
    }

    #[test]
    fn warm_cache_witness_fixed_holes_preserve_incarnon_fight() {
        let mut req = request();
        req["weapon"] = json!("torid");
        req["mode"] = json!("cycle");
        req["modes"] = json!({"cycle": "fixed"});
        req["mods"] = json!({"hellfire": "search", "infected_clip": "search", "split_chamber": "search"});
        req.as_object_mut().unwrap().remove("starts");
        let plan = super::super::parse_optimize(&req).unwrap();
        let card = |id| plan.pool.iter().position(|m| m.id == id).unwrap();
        let ordered = vec![card("hellfire"), card("infected_clip"), card("split_chamber")];
        let base = wfsim_engine::model::WeaponBase::from_data("torid", false, &[]);
        let spaced = [Some(ordered[0]), None, Some(ordered[1]), None, None, Some(ordered[2]), None, None];
        let packed = [Some(ordered[0]), Some(ordered[1]), Some(ordered[2]), None, None, None, None, None];
        let expand = |slots| wfsim_optimizer::expand_with_slots(
            &plan.pool, &base, 0, plan.cap, &crate::registry::innate_slots_for("torid"),
            None, &ordered, slots, u8::MAX, &plan.scenario.arena.tenno, plan.scenario.policy,
        ).pop().expect("legal fixed layout");
        let (packed, spaced) = (expand(&packed), expand(&spaced));
        assert_ne!(packed.slots, spaced.slots);
        assert_eq!(packed.ordered, spaced.ordered);
        let identity = |c| plan.replay.warm_identity(c, 0, "torid", 0.0, 0xDEAD_BEEF).key;
        assert_eq!(identity(&packed), identity(&spaced));
        let prepared = |c| (plan.scenario.params)(c, 0).expect("prepared fight");
        let (p, s) = (prepared(&packed), prepared(&spaced));
        let cycle = p.cycle.as_ref().expect("Torid Incarnon cycle");
        assert!(cycle.base_form.multishot > 1.0);
        assert_eq!(format!("{p:?}"), format!("{s:?}"));

        let (packed, spaced) = (plan.replay.request(&packed, 0), plan.replay.request(&spaced, 0));
        assert_ne!(packed["mod_slots"], spaced["mod_slots"]);
        assert_eq!(packed["mods"], spaced["mods"]);
        for request in [&packed, &spaced] {
            let replayed = crate::simulate::ready(request, false).expect("replay fight");
            assert_eq!(format!("{p:?}"), format!("{:?}", replayed.params));
        }
    }

    #[test]
    fn warm_cache_keys_follow_order_and_start_ranks_and_rolls() {
        let mut req = request();
        let keys = |req: &Value| warm_lead(req)["warm_keys"].as_array().unwrap().clone();
        let original = keys(&req);
        assert_eq!(original.len(), 3);
        let mut pinned = req.clone();
        pinned["starts"][0]["slots"] = json!(["infected_clip", "stormbringer", "hellfire"]);
        pinned["starts"][0]["fixed"][0]["placement"] = json!("slot");
        let pinned = keys(&pinned);
        assert!(pinned.iter().enumerate().any(|(i, k)| {
            original.iter().position(|other| other == k).is_some_and(|j| i != j)
        }), "a shared actual order can move to a different index: {original:?}, {pinned:?}");

        let differs = |base: &[Value], changed: &Value| {
            assert!(keys(changed).iter().all(|k| !base.contains(k)));
        };
        let mut lower_mod = req.clone();
        lower_mod["mods"]["hellfire@0"] = json!("search");
        lower_mod["starts"][0]["slots"][0] = json!("hellfire@0");
        differs(&original, &lower_mod);

        req["arcanes"] = json!({"primary_merciless@1": "fixed"});
        let arcane = keys(&req);
        differs(&original, &req);
        req["arcanes"] = json!({"primary_merciless@2": "fixed"});
        differs(&arcane, &req);

        req["mods"]["riven:local"] = json!("search");
        req["starts"][0]["slots"][0] = json!("riven:local");
        req["rivens"] = json!([{"id": "local", "spec": {"rank": 8,
            "bonuses": [{"id":"damage", "roll":1.03}, {"id":"critical_chance", "roll":0.94}],
            "malus": {"id":"zoom", "roll":1.0}}}]);
        let riven = keys(&req);
        for i in 0..2 {
            let mut changed = req.clone();
            changed["rivens"][0]["spec"]["bonuses"][i]["roll"] = json!(1.01);
            differs(&riven, &changed);
        }
        for (field, value) in [("rank", json!(7)), ("malus", Value::Null)] {
            let mut changed = req.clone();
            changed["rivens"][0]["spec"][field] = value;
            differs(&riven, &changed);
        }
        let mut duplicate = req["rivens"][0].clone();
        duplicate["spec"]["rank"] = json!(7);
        req["rivens"].as_array_mut().unwrap().push(duplicate);
        assert_eq!(keys(&req), riven);
        req["rivens"].as_array_mut().unwrap().swap(0, 1);
        differs(&riven, &req);
    }

    #[test]
    fn warm_cache_exact_across_screen_full_and_reindexed_pool() {
        let mut req = request();
        req["threads"] = json!(1);
        req["candidate_runs"] = json!(2);
        req["mods"]["fast_hands"] = json!("search");
        let pending = warm_lead(&req);
        let keys = pending["warm_keys"].as_array().unwrap();
        assert_eq!(keys.len(), 3, "keep this test within ten fights");
        assert_eq!(keys.iter().map(Value::as_str).collect::<HashSet<_>>().len(), keys.len());
        let score = |req: &Value, item: Value| run(req, Some(json!({"score": [item], "warm_cache": 1})));
        let cold = score(&req, pending.clone());
        let short = score(&req, json!({"build": pending["build"], "runs": 1}));
        assert_eq!(cold["fights"], 6);
        assert_eq!(short["fights"], 3);
        let full = &cold["scores"][0]["warm_shards"];
        let small = &short["scores"][0]["warm_shards"];

        let mut other = req.clone();
        other["mods"]["fast_hands"] = json!("exclude");
        other["starts"][0]["slots"] = json!(["stormbringer", "hellfire", "infected_clip"]);
        let moved = warm_lead(&other);
        assert_ne!(moved["build"], pending["build"], "exercise different pool indices");
        assert_eq!(moved["warm_keys"], pending["warm_keys"]);
        let carried = &short["scores"][0]["shards"];
        let screened = score(&other, json!({"build": moved["build"], "runs": 1,
            "warm_shards": [full[0], small[1]], "shards": [carried[0], null, carried[2]]}));
        assert_eq!(screened, json!({"ok": true, "scores": short["scores"], "fights": 0,
            "work": 0, "builds": short["builds"]}));

        let mut bad: Value = serde_json::from_str(full[2].as_str().unwrap()).unwrap();
        bad[2]["index"][1]["hi"] = json!(0);
        let mixed = score(&other, json!({"build": moved["build"],
            "warm_shards": [full[0], full[1], bad.to_string()], "shards": carried}));
        assert_eq!(mixed["fights"], 1);
        let saved = Shard::unpack_search_with_identity(full[2].as_str().unwrap(), Some((
            keys[2].as_str().unwrap(), bad[2]["warm_identity"].as_str().unwrap(),
        ))).unwrap();
        let prefix = Carry::read(&carried[2]).unwrap().open().unwrap();
        assert_eq!(mixed["work"], saved.work() - prefix.work());
        assert_eq!(mixed["scores"], cold["scores"]);
        assert_eq!(mixed["builds"], cold["builds"]);

        let hit = score(&other, json!({"build": moved["build"], "warm_shards": full}));
        assert_eq!((hit["fights"].as_u64(), hit["work"].as_u64()), (Some(0), Some(0)));
        assert_eq!(hit["scores"], cold["scores"]);
        assert_eq!(hit["builds"], cold["builds"]);

        let disabled = run(&other, Some(json!({"lead":true, "fresh":true, "warm_cache":true})));
        assert_eq!(disabled["pending"][0], moved["build"], "the opt-in requires numeric 1");
    }

    fn strategy_request() -> Value {
        json!({
            "weapon": "braton", "enemy": "thrax_centurion", "level": 100,
            "duration": 0.1, "runs": 1, "candidate_runs": 1, "finalists": 10,
            "strategy": "quick", "search_strategy": "four_elements", "build_size": 2,
            "mods": { "serration": "search", "split_chamber": "search",
                "thermite_rounds": "search", "rime_rounds": "search",
                "high_voltage": "search", "malignant_force": "search" },
            "starts": [{"slots": ["serration", "split_chamber"],
                "fixed": [{"kind": "mods", "idx": 0, "placement": "slot"}]}],
        })
    }

    #[test]
    fn four_elements_processes_full_builds_without_moving_pins() {
        let req = strategy_request();
        let plan = super::super::parse_optimize(&req).expect("plan");
        let lead = run(&req, Some(json!({"lead": true, "fresh": true, "scores": []})));
        let progress = lead["progress"].as_array().expect("branch progress");
        assert_eq!(progress.len(), 5, "{lead}");
        for row in progress {
            assert_eq!(row["start"], 0);
        }
        let pending = lead["pending"].as_array().unwrap();
        assert_eq!(pending.len(), 5, "{lead}");
        let pinned = plan.pool.iter().position(|m| m.id == "serration").unwrap();
        for row in pending {
            let b = QBuild::from_json(row.get("build").unwrap_or(row)).unwrap();
            assert_eq!(b.mods[0], Some(pinned), "{row}");
            assert_eq!(b.fixed_slots, 1);
            assert_eq!(b.mods.iter().flatten().count(), 2);
        }
        let out = run(&req, None);
        assert_eq!(out["starts"], 1, "{out}");
        assert_eq!(out["branches"], 5, "{out}");
        assert_eq!(out["start_results"].as_array().unwrap().len(), 1, "{out}");
        for lane in out["results"].as_array().unwrap().iter()
            .flat_map(|r| r["from_starts"].as_array().into_iter().flatten()) {
            assert_eq!(lane["start"], 0, "{lane}");
            assert!(lane["branch"].is_string(), "{lane}");
        }
    }

    #[test]
    fn four_elements_keeps_user_starts_and_legacy_requests_distinct() {
        let mut req = strategy_request();
        req["starts"] = json!([{"slots": []}, {"slots": ["serration"]}]);
        let out = run(&req, None);
        assert_eq!(out["starts"], 2, "{out}");
        assert_eq!(out["branches"], 10, "{out}");
        assert_eq!(out["start_results"].as_array().unwrap().len(), 2, "{out}");
        req.as_object_mut().unwrap().remove("search_strategy");
        let legacy = run(&req, None);
        assert_eq!(legacy["starts"], 2);
        assert_eq!(legacy["branches"], 2);
        req["search_strategy"] = json!("regular");
        let mut regular = run(&req, None);
        assert_eq!(regular["starts"], legacy["starts"]);
        assert_eq!(regular["branches"], legacy["branches"]);
        for row in regular["results"].as_array_mut().unwrap() {
            assert_eq!(row["replay"].as_object_mut().unwrap().remove("search_strategy"), Some(json!("regular")));
        }
        for answer in regular["start_results"].as_array_mut().unwrap() {
            if let Some(replay) = answer.get_mut("result").and_then(|row| row.get_mut("replay")).and_then(Value::as_object_mut) {
                replay.remove("search_strategy");
            }
        }
        assert_eq!(regular["results"], legacy["results"]);
        assert_eq!(regular["start_results"], legacy["start_results"]);
        for strategy in [json!("unknown"), json!(null), json!(4)] {
            req["search_strategy"] = strategy;
            assert!(super::super::parse_optimize(&req).is_err());
        }
        req["search_strategy"] = json!("four_elements");
        req["starts"] = json!([]);
        let out = run(&req, None);
        assert_eq!(out["starts"], 1, "{out}");
        assert_eq!(out["branches"], 5, "{out}");
        req["strategy"] = json!("exhaust");
        assert!(super::super::parse_optimize(&req).is_err());
    }

    #[test]
    fn four_elements_respects_exclusions_required_cards_and_pinned_empty_seats() {
        let mut req = strategy_request();
        req["mods"]["serration"] = json!("fixed");
        req["mods"]["rime_rounds"] = json!("exclude");
        req["starts"] = json!([{"slots": ["serration", null, "split_chamber"],
            "fixed": [{"kind": "mods", "idx": 1, "placement": "slot"}]}]);
        let plan = super::super::parse_optimize(&req).expect("plan");
        let lead = run(&req, Some(json!({"lead": true, "fresh": true, "scores": []})));
        assert_eq!(lead["progress"].as_array().unwrap().len(), 4, "{lead}");
        for row in lead["pending"].as_array().unwrap() {
            let b = QBuild::from_json(row.get("build").unwrap_or(row)).unwrap();
            assert!(b.mods[1].is_none(), "{row}");
            assert!(b.mods.iter().flatten().any(|&i| plan.pool[i].id == "serration"));
            assert!(!b.mods.iter().flatten().any(|&i| plan.pool[i].id == "rime_rounds"));
        }
        req["starts"][0]["fixed"] = json!([
            {"kind": "mods", "idx": 0}, {"kind": "mods", "idx": 1, "placement": "slot"},
            {"kind": "mods", "idx": 2}
        ]);
        let out = run(&req, None);
        assert_eq!(out["branches"], 1, "no optional branch can displace protected cards: {out}");
        let mut req = strategy_request();
        req["mods"]["thermite_rounds@0"] = json!("search");
        req["starts"][0]["slots"] = json!(["thermite_rounds@0", "split_chamber"]);
        let out = run(&req, None);
        assert_eq!(out["branches"], 4, "a pinned elemental family needs no second copy: {out}");
        assert!(!out["results"].as_array().unwrap().is_empty(), "{out}");
        for row in out["results"].as_array().unwrap() {
            assert!(row["mods"].as_array().unwrap().contains(&json!("thermite_rounds@0")), "{row}");
            assert!(!row["mods"].as_array().unwrap().contains(&json!("thermite_rounds")), "{row}");
        }
    }

    fn screened(req: &Value) -> (Value, Value) {
        let lead = run(req, Some(json!({ "lead": true, "fresh": true, "scores": [] })));
        let pending = lead["pending"].as_array().expect("a paused start");
        assert_eq!(pending.len(), 1, "one fully pinned start: {lead}");
        let build = pending[0].get("build").unwrap_or(&pending[0]).clone();
        let out = run(req, Some(json!({ "score": [{ "build": build, "runs": 2 }] })));
        assert_eq!(out["scores"][0]["rough"], json!(true), "{out}");
        assert!(out["scores"][0]["shards"].as_array().unwrap().len() > 1, "several element orders");
        (build, out)
    }

    #[test]
    fn cached_screen_shards_preserve_order_nulls_and_seed_halves() {
        let req = request();
        let (_, mut short) = screened(&req);
        let row = &mut short["scores"][0];
        row["shards"][0]["index"][0]["hi"] = json!(u32::MAX);
        row["shards"][0]["index"][0]["lo"] = json!(u32::MAX - 1);
        row["shards"][1] = Value::Null;
        let expected = row["shards"].clone();
        let scores = short["scores"].clone();
        let first = run(&req, Some(json!({ "lead": true, "scores": scores })));
        assert_eq!(first["pending"][0]["shards"], expected);
        let replay = run(&req, Some(json!({ "lead": true, "scores": [] })));
        assert_eq!(replay["pending"][0]["shards"], expected, "another leader replay keeps every order intact");
        assert!(replay["pending"][0]["shards"][1].is_null());
    }

    #[test]
    fn packed_screen_promotes_to_identical_dense_rows_without_refighting() {
        let req = request();
        let (build, dense) = screened(&req);
        let packed = run(&req, Some(json!({"score":[{"build":build,"runs":2}],"compact_shards":1})));
        assert_eq!(dense["work"], packed["work"]);
        assert!(packed["scores"][0]["shards"][0].is_string());
        let promote = |short: &Value| {
            let lead = run(&req, Some(json!({"lead":true,"fresh":true,"scores":short["scores"]})));
            let mut full = run(&req, Some(json!({"score":lead["pending"]})));
            // Independent native scorers can publish their encounter log in either order.
            full["builds"].as_array_mut().unwrap().sort_by_key(Value::to_string);
            let finish = run(&req, Some(json!({"lead":true,"scores":full["scores"]})));
            (full,finish)
        };
        assert_eq!(promote(&packed), promote(&dense));
    }

    #[test]
    fn cached_screen_promotion_preserves_scores_fights_and_work() {
        let req = request();
        let (build, short) = screened(&req);
        let lead = run(&req, Some(json!({ "lead": true, "scores": short["scores"] })));
        assert_eq!(lead["pending"][0]["shards"], short["scores"][0]["shards"]);
        let replay = run(&req, Some(json!({ "lead": true, "scores": [] })));
        assert_eq!(replay["pending"], lead["pending"]);
        let full = run(&req, Some(json!({ "score": replay["pending"] })));
        let fresh = run(&req, Some(json!({ "score": [build] })));
        assert_eq!(full["scores"], fresh["scores"], "continuing the screen preserves every score and the best shard");
        let orders = short["scores"][0]["shards"].as_array().unwrap().len() as u64;
        assert_eq!(short["fights"], json!(2 * orders));
        assert_eq!(full["fights"], json!(8 * orders), "the short runs are not fought again");
        for field in ["fights", "work"] {
            assert_eq!(short[field].as_u64().unwrap() + full[field].as_u64().unwrap(), fresh[field].as_u64().unwrap(), "{field}");
        }
        let answer = run(&req, Some(json!({ "lead": true, "scores": full["scores"] })));
        let key = full["scores"][0]["key"].as_str().unwrap();
        let best = FLEET.with(|f| serde_json::to_value(f.borrow().best_runs.get(key).unwrap()).unwrap());
        assert_eq!(best, full["scores"][0]["shard"], "materialization retains the full best-order shard");
        assert_eq!(answer["results"], run(&req, None)["results"], "the final simulator sees the same contenders");
    }
}
