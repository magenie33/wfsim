// SPDX-License-Identifier: AGPL-3.0-or-later
use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};

use serde_json::{json, Value};
use wfsim_optimizer::Candidate;

use super::Replay;

const BUILD_BATCH: usize = 128;

#[derive(Default)]
struct HeldBuilds {
    tried: HashSet<(Vec<usize>, u32, u32, usize)>,
    seen: HashSet<String>,
    pending: VecDeque<(u64, Value)>,
    next: u64,
    acknowledged: u64,
    closed: bool,
}

/// Complete builds encountered by a search, retained until its reader
/// acknowledges a persisted batch. This channel carries no measured scores.
#[derive(Clone, Default)]
pub struct BuildStream {
    enabled: Arc<AtomicBool>,
    held: Arc<Mutex<HeldBuilds>>,
}

pub(super) struct BuildProducer(BuildStream);

impl Drop for BuildProducer {
    fn drop(&mut self) {
        self.0.close();
    }
}

impl BuildStream {
    pub(super) fn enable(&self) {
        if !self.held.lock().unwrap_or_else(|e| e.into_inner()).closed {
            self.enabled.store(true, Ordering::Relaxed);
        }
    }

    pub(super) fn producer(&self) -> BuildProducer {
        BuildProducer(self.clone())
    }

    /// Whether records still wait for their reader's persistence acknowledgement.
    pub fn has_pending(&self) -> bool {
        !self.held.lock().unwrap_or_else(|e| e.into_inner()).pending.is_empty()
    }

    fn close(&self) {
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        held.closed = true;
        self.enabled.store(false, Ordering::Relaxed);
        held.tried = HashSet::new();
        held.seen = HashSet::new();
        if held.pending.is_empty() { held.pending = VecDeque::new(); }
    }

    /// Acknowledge records through `after`, then read the next transport batch.
    /// Reading twice with the same cursor returns the same unacknowledged records.
    pub fn batch(&self, after: u64) -> Value {
        let mut held = self.held.lock().expect("encountered builds");
        held.acknowledged = held.acknowledged.max(after.min(held.next));
        while held.pending.front().is_some_and(|(seq, _)| *seq <= held.acknowledged) {
            held.pending.pop_front();
        }
        if held.closed && held.pending.is_empty() { held.pending = VecDeque::new(); }
        let rows: Vec<&Value> = held.pending.iter().take(BUILD_BATCH).map(|(_, row)| row).collect();
        let next = held.pending.iter().take(BUILD_BATCH).next_back().map_or(held.acknowledged, |(seq, _)| *seq);
        json!({ "builds": rows, "builds_next": next, "builds_more": held.pending.len() > BUILD_BATCH })
    }

    pub(super) fn take_all(&self) -> Vec<Value> {
        let mut held = self.held.lock().expect("encountered builds");
        held.acknowledged = held.next;
        let rows = held.pending.drain(..).map(|(_, row)| row).collect();
        if held.closed { held.pending = VecDeque::new(); }
        rows
    }
}

impl Replay {
    /// A candidate's ordinary library record, admitted by the board's own door.
    fn admitted_build(&self, c: &Candidate, ai: usize) -> Option<(String, Value)> {
        let mut row = self.build_axes(c, ai);
        let mods: Vec<&str> = c.ordered.iter().map(|&i| self.mods[i]).collect();
        let riven = mods.iter().copied().find(|id| id.starts_with("riven:"));
        let shape = match riven {
            Some(id) => Some(self.riven_shapes.get(id)?),
            None => None,
        };
        let (arcanes, _) = self.arcane(ai);
        row["weapon"] = self.base.get("weapon")?.clone();
        row["mods"] = json!(mods.iter().copied().map(|id| if id.starts_with("riven:") { "riven" } else { id }).collect::<Vec<_>>());
        row["arcanes"] = json!(arcanes);
        if let Some(shape) = shape {
            row["riven_pos"] = json!(shape.bonuses);
            row["riven_neg"] = json!(shape.malus);
        }
        if let Some(wielder) = crate::tenno::wielder_build_of(&self.base) {
            row["wielder"] = serde_json::to_value(wielder).ok()?;
        }
        let assembly = row["assembly"].as_object().cloned();
        let object = row.as_object_mut()?;
        for name in ["arcane", "arcane_rank", "assembly"] { object.remove(name); }
        if let Some(parts) = assembly { object.extend(parts); }
        if object.get("exilus").and_then(Value::as_str) == Some("none") { object.remove("exilus"); }
        let build = crate::board_rows::library_build(&row).ok()?;
        let admitted = wfsim_engine::board::benchmarks::all().iter().any(|bench| {
            wfsim_engine::board::builds::admit_for_board(&bench.id, &build.weapon, &build).is_ok()
        });
        if !admitted { return None; }
        let key = wfsim_engine::board::builds::build_id(&build);
        let mut record = crate::board_rows::canonical_record(&build);
        record["mode"] = row["mode"].clone();
        Some((key, record))
    }

    pub(super) fn encounter(&self, c: &Candidate, ai: usize) -> bool {
        if !self.builds.enabled.load(Ordering::Relaxed) { return false; }
        {
            let mut held = self.builds.held.lock().expect("encountered builds");
            if held.closed || !held.tried.insert((c.ordered.clone(), c.variant, c.exilus, ai)) { return false; }
        }
        let Some((key, row)) = self.admitted_build(c, ai) else { return false };
        let mut held = self.builds.held.lock().expect("encountered builds");
        if held.closed || !held.seen.insert(key) { return false; }
        held.next += 1;
        let seq = held.next;
        held.pending.push_back((seq, row));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse_optimize, run_optimize};
    use wfsim_optimizer::FunnelState;

    const MODS: [&str; 8] = ["serration", "split_chamber", "point_strike", "vital_sense",
        "hellfire", "infected_clip", "stormbringer", "speed_trigger"];

    fn request(mods: &[&str]) -> Value {
        let marks: serde_json::Map<String, Value> = mods.iter().map(|id| ((*id).to_string(), json!("fixed"))).collect();
        json!({
            "weapon": "acceltra_prime", "enemy": "thrax_centurion", "level": 100,
            "duration": 1, "runs": 1, "candidate_runs": 10, "finalists": 1,
            "strategy": "quick", "build_size": mods.len(), "build_min": mods.len(), "threads": 1,
            "mods": marks, "arcanes": { "primary_deadhead": "fixed" }, "modes": { "base": "fixed" },
            "starts": [{ "slots": mods, "fixed": (0..mods.len()).map(|i| json!({ "kind": "mods", "idx": i })).collect::<Vec<_>>() }],
        })
    }

    fn run(req: &Value) -> Value {
        let plan = parse_optimize(req).unwrap_or_else(|e| panic!("{e}"));
        let out = run_optimize(plan, &FunnelState::default(), |_, _| {}, None);
        assert_eq!(out["ok"], true, "{out}");
        out
    }

    fn short(req: &Value) -> Value {
        let mut leader = req.clone();
        leader["quick_fleet"] = json!({ "lead": true, "fresh": true, "scores": [] });
        let lead = run(&leader);
        let pending = lead["pending"].as_array().expect("one pinned full build waits");
        let build = pending[0].get("build").unwrap_or(&pending[0]);
        let mut scorer = req.clone();
        scorer["quick_fleet"] = json!({ "score": [{ "build": build, "runs": 2 }] });
        run(&scorer)
    }

    fn candidate(plan: &crate::OptimizePlan) -> Candidate {
        let base = wfsim_engine::model::WeaponBase::from_data("acceltra_prime", false, &[]);
        let mut candidates = Vec::new();
        wfsim_optimizer::expand_one(&plan.pool, &base, None, 0, plan.cap,
            &crate::registry::innate_slots_for("acceltra_prime"), &[None], &(0..plan.pool.len()).collect::<Vec<_>>(),
            wfsim_engine::data::tenno::default_tenno(), wfsim_engine::model::StackPolicy::Emergent, &mut candidates);
        candidates.remove(0)
    }

    #[test]
    fn short_only_measurements_export_every_admitted_element_order() {
        let out = short(&request(&MODS));
        assert_eq!(out["scores"][0]["rough"], true);
        let orders = out["scores"][0]["shards"].as_array().unwrap();
        let builds = out["builds"].as_array().unwrap();
        assert!(orders.len() > 1, "the fixture must have distinct element combinations");
        assert_eq!(builds.len(), orders.len(), "every measured order is an ordinary build");
        for record in builds {
            let build = crate::board_rows::library_build(record).unwrap();
            assert!(wfsim_engine::board::builds::admit_for_board("standard_single_target", &build.weapon, &build).is_ok());
            for forbidden in ["score", "kill_progress", "replay", "rivens", "riven_rolls", "seed", "enemy"] {
                assert!(record.get(forbidden).is_none(), "{forbidden} is not a submitted build axis");
            }
        }
    }

    #[test]
    fn board_admission_refuses_partial_mods_and_missing_arcanes() {
        assert!(short(&request(&MODS[..7]))["builds"].as_array().unwrap().is_empty());
        let mut req = request(&MODS);
        req["arcanes"] = json!({ "none": "fixed" });
        assert!(short(&req)["builds"].as_array().unwrap().is_empty());
    }

    #[test]
    fn encountered_rivens_export_shapes_at_the_librarys_card_ranks() {
        let mut ids = MODS;
        ids[7] = "riven:local";
        let mut req = request(&ids);
        req["arcanes"] = json!({ "primary_deadhead@1": "fixed" });
        req["rivens"] = json!([{ "id": "local", "spec": {
            "bonuses": [{ "id": "damage", "roll": 1.03 }, { "id": "critical_chance", "roll": 0.94 }],
            "rank": 8,
        } }]);
        let out = short(&req);
        let builds = out["builds"].as_array().unwrap();
        assert!(!builds.is_empty(), "the complete riven builds must be admitted");
        for record in builds {
            assert_eq!(record["riven_pos"], json!(["critical_chance", "damage"]));
            assert!(record["mods"].as_array().unwrap().iter().any(|id| id == "riven"));
            assert_eq!(record["arcanes"], json!(["primary_deadhead"]));
            assert!(record.get("riven_rolls").is_none());
        }
    }

    #[test]
    fn a_native_subscriber_keeps_the_same_ranking_and_all_orders() {
        let req = request(&MODS);
        let expected = run(&req);
        let plan = parse_optimize(&req).unwrap();
        let builds = plan.build_stream();
        let observed = run_optimize(plan, &FunnelState::default(), |_, _| {}, None);
        assert_eq!(observed, expected, "collecting builds cannot alter the search result");
        {
            let held = builds.held.lock().unwrap();
            assert!(held.closed, "returning from the search must close its producer");
            assert_eq!((held.tried.capacity(), held.seen.capacity()), (0, 0));
        }
        assert!(builds.take_all().len() > 1, "native searches expose every element order too");
        assert_eq!(builds.held.lock().unwrap().pending.capacity(), 0);
    }

    #[test]
    fn every_search_strategy_uses_the_same_build_stream() {
        for strategy in ["descent", "exhaust"] {
            let mut req = request(&MODS);
            req["strategy"] = json!(strategy);
            let plan = parse_optimize(&req).unwrap();
            let builds = plan.build_stream();
            let out = run_optimize(plan, &FunnelState::default(), |_, _| {}, None);
            assert_eq!(out["ok"], true, "{strategy}: {out}");
            assert!(builds.take_all().len() > 1, "{strategy} must export its distinct element orders");
        }
    }

    #[test]
    fn a_generated_build_is_exported_before_its_fight_runs() {
        let plan = parse_optimize(&request(&MODS)).unwrap();
        let stream = plan.build_stream();
        assert!(stream.batch(0)["builds"].as_array().unwrap().is_empty());
        assert!((plan.scenario.params)(&candidate(&plan), 0).is_some());
        assert_eq!(stream.batch(0)["builds"].as_array().unwrap().len(), 1,
            "cancelling the upcoming fight cannot discard its already generated build");
    }

    #[test]
    fn active_acknowledgements_keep_promotion_deduplication() {
        let plan = parse_optimize(&request(&MODS)).unwrap();
        let stream = plan.build_stream();
        let producer = stream.producer();
        let build = candidate(&plan);
        assert!((plan.scenario.params)(&build, 0).is_some());
        let cursor = stream.batch(0)["builds_next"].as_u64().unwrap();
        assert!(stream.batch(cursor)["builds"].as_array().unwrap().is_empty());
        assert_eq!(stream.held.lock().unwrap().seen.len(), 1);
        assert!((plan.scenario.params)(&build, 0).is_some());
        assert!(stream.batch(cursor)["builds"].as_array().unwrap().is_empty(), "promotion cannot re-emit an acknowledged build");
        drop(producer);
        let held = stream.held.lock().unwrap();
        assert_eq!((held.seen.capacity(), held.tried.capacity(), held.pending.capacity()), (0, 0, 0));
    }

    #[test]
    fn closing_releases_keys_and_keeps_unacknowledged_records() {
        let plan = parse_optimize(&request(&MODS)).unwrap();
        let stream = plan.build_stream();
        let producer = stream.producer();
        assert!((plan.scenario.params)(&candidate(&plan), 0).is_some());
        let pending = stream.batch(0);
        drop(producer);
        assert_eq!(stream.batch(0), pending, "producer completion cannot spend pending records");
        {
            let held = stream.held.lock().unwrap();
            assert!(held.closed);
            assert_eq!((held.tried.capacity(), held.seen.capacity()), (0, 0));
            assert!(!held.pending.is_empty());
        }
        let done = stream.batch(pending["builds_next"].as_u64().unwrap());
        assert!(done["builds"].as_array().unwrap().is_empty());
        assert_eq!(stream.held.lock().unwrap().pending.capacity(), 0);
        stream.enable();
        assert!(!stream.enabled.load(Ordering::Relaxed), "a completed producer cannot reopen");
    }

    #[test]
    fn producer_drop_recovers_a_poisoned_stream_without_panicking() {
        let stream = BuildStream::default();
        let producer = stream.producer();
        let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _held = stream.held.lock().unwrap();
            panic!("producer failure");
        }));
        assert!(failure.is_err());
        drop(producer);
        assert!(stream.held.lock().unwrap_or_else(|e| e.into_inner()).closed);
    }

    #[test]
    fn equal_scoring_equipment_exports_distinct_builds() {
        let mut ids = MODS;
        ids[7] = "magazine_warp";
        let mut req = request(&ids);
        req["mods"]["magazine_warp"] = json!("search");
        req["mods"]["fast_hands"] = json!("search");
        let plan = parse_optimize(&req).unwrap();
        let index = |id| plan.pool.iter().position(|m| m.id == id).unwrap();
        let magazine = index("magazine_warp");
        let reload = index("fast_hands");
        let mut leader = req.clone();
        leader["quick_fleet"] = json!({ "lead": true, "fresh": true, "scores": [] });
        let pending = run(&leader)["pending"].as_array().unwrap().clone();
        let build = pending[0].get("build").unwrap_or(&pending[0]).clone();
        let mut other = build.clone();
        let slot = build.as_array().unwrap()[..8].iter().position(|v| v.as_u64() == Some(magazine as u64)).unwrap();
        other[slot] = json!(reload);
        req["quick_fleet"] = json!({ "score": [{ "build": build, "runs": 2 }, { "build": other, "runs": 2 }] });
        let out = run(&req);
        assert_eq!(out["scores"][0]["score"], out["scores"][1]["score"], "the fixture must tie");
        let records = out["builds"].as_array().unwrap();
        for card in ["magazine_warp", "fast_hands"] {
            assert!(records.iter().any(|r| r["mods"].as_array().unwrap().iter().any(|id| id == card)), "{card} must survive its equal score");
        }
    }

    #[test]
    fn build_batches_survive_repeated_reads_until_acknowledged() {
        let stream = BuildStream::default();
        {
            let mut held = stream.held.lock().unwrap();
            for i in 1..=BUILD_BATCH as u64 + 2 { held.pending.push_back((i, json!({ "n": i }))); }
            held.next = BUILD_BATCH as u64 + 2;
        }
        let first = stream.batch(0);
        assert_eq!(stream.batch(0), first, "reading cannot spend a batch");
        assert_eq!(first["builds"].as_array().unwrap().len(), BUILD_BATCH);
        assert_eq!(first["builds_more"], true);
        let next = first["builds_next"].as_u64().unwrap();
        let last = stream.batch(next);
        assert_eq!(last["builds"].as_array().unwrap().len(), 2, "pagination has no total cap");
        assert_eq!(stream.batch(next), last);
        let done = stream.batch(last["builds_next"].as_u64().unwrap());
        assert!(done["builds"].as_array().unwrap().is_empty());
        assert_eq!(done["builds_more"], false);
    }
}
