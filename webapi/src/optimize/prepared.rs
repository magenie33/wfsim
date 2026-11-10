// SPDX-License-Identifier: AGPL-3.0-or-later
use std::cell::RefCell;
use std::sync::Arc;

use serde_json::Value;

use super::{BuildStream, OptimizePlan, Prepared, Replay};

thread_local! {
    static LAST: RefCell<Option<(Value, OptimizePlan)>> = const { RefCell::new(None) };
}

// One request per worker; large scopes run normally without a retained copy.
const MAX_KEY_BYTES: usize = 65_536;
const MAX_OPTIONS: usize = 1024;

pub(super) fn parse(v: &Value) -> Result<OptimizePlan, Value> {
    if v["strategy"] != "quick" || !v["quick_fleet"].is_object() {
        return super::parse_uncached(v);
    }
    let key: Value = v.as_object().expect("quick request").iter()
        .filter(|(k, _)| k.as_str() != "quick_fleet")
        .map(|(k, value)| (k.clone(), value.clone())).collect();
    let hit = LAST.with(|last| last.borrow().as_ref()
        .filter(|(held, _)| held == &key).map(|(_, plan)| plan.clone()));
    let mut plan = match hit {
        Some(plan) => plan,
        None => {
            LAST.with(|last| *last.borrow_mut() = None);
            let plan = super::parse_uncached(&key)?;
            if plan.pool.len() <= MAX_OPTIONS && plan.variants.len() <= MAX_OPTIONS
                && plan.arcanes.len() <= MAX_OPTIONS && serde_json::to_vec(&key).is_ok_and(|s| s.len() <= MAX_KEY_BYTES)
            {
                LAST.with(|last| *last.borrow_mut() = Some((key, plan.clone())));
            }
            plan
        }
    };
    plan.fleet = v["quick_fleet"].clone();
    Ok(plan)
}

pub(super) fn params(scoring: Arc<Replay>) -> Arc<wfsim_optimizer::ParamsFn> {
    let prepared: Prepared = Default::default();
    Arc::new(move |c, ai| {
        let key = (c.variant, ai);
        let held = prepared.lock().ok()?.get(&key).cloned();
        let entrant = match held {
            Some(e) => e?,
            None => {
                let e = crate::simulate::Entrant::new(&scoring.shape(c.variant, ai), false).ok().map(Arc::new);
                prepared.lock().ok()?.insert(key, e.clone());
                e?
            }
        };
        let refs = entrant.seat(&scoring.mod_ids(c)).ok()?;
        let (_, params) = entrant.params(&refs).ok()?;
        if scoring.encounter(c, ai) { wfsim_optimizer::tick(); }
        Some(params)
    })
}

// A clone owns its build stream and entrant preparation. Only immutable tables
// and the lazily resolved weapon bases can outlive one invocation.
impl Clone for OptimizePlan {
    fn clone(&self) -> Self {
        let replay = Arc::new(Replay { builds: BuildStream::default(), ..(*self.replay).clone() });
        let mut scenario = self.scenario.clone();
        scenario.params = params(replay.clone());
        Self {
            weapon_id: self.weapon_id.clone(), pool: self.pool.clone(), stances: self.stances.clone(), cap: self.cap,
            constraints: self.constraints.clone(), min_slots: self.min_slots, build_size: self.build_size,
            evo_sets: self.evo_sets.clone(), variant_forbids: self.variant_forbids.clone(),
            exilus_defs: self.exilus_defs.clone(), arcanes: self.arcanes.clone(), arcane_sets: self.arcane_sets.clone(),
            valences: self.valences.clone(), valence_bonus: self.valence_bonus, scenario,
            final_runs: self.final_runs, finalists: self.finalists, headshot_pct: self.headshot_pct,
            duration: self.duration, target_name: self.target_name.clone(), level: self.level, steel_path: self.steel_path,
            modes: self.modes.clone(), variants: self.variants.clone(), assemblies: self.assemblies.clone(),
            final_required: self.final_required.clone(), threads: self.threads, max_evals: self.max_evals,
            strategy: self.strategy.clone(), starts: self.starts.clone(), candidate_runs: self.candidate_runs,
            fleet: self.fleet.clone(), shard: self.shard, shards: self.shards,
            replay_base: self.replay_base.clone(), replay, forms: self.forms.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
