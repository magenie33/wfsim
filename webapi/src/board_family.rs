// SPDX-License-Identifier: AGPL-3.0-or-later
//! A library arrival as one build-shaped search start for each sibling weapon.

use serde_json::{json, Value};
use wfsim_engine::board::{benchmarks, builds};
use wfsim_engine::data::{mods, weapons};

/// No fight runs at intake. The target's existing quick search drops unavailable
/// choices, fills their positions and descends from the retained configuration.
pub fn transfers(record: &Value, ruler: &str, mode: &str) -> Vec<Value> {
    let Ok(source) = crate::board_rows::board_build(record) else { return Vec::new() };
    let Some(spec) = weapons::spec(&source.weapon) else { return Vec::new() };
    let Some(family) = spec.riven_family.as_deref().filter(|f| !f.is_empty()) else { return Vec::new() };
    let Some(bench) = benchmarks::all().iter().find(|b| benchmarks::family(&b.id) == benchmarks::family(ruler))
        .or_else(|| benchmarks::all().iter().find(|b| b.id == "standard_single_target")) else { return Vec::new() };
    let presets = mods::search_presets();
    let preset = presets.presets.iter().find(|p| p.riven == source.riven.is_some()).expect("search preset");
    let source_pool = mods::pool_for_weapon(&source.weapon);
    let stance = source.mods.iter().find(|id| source_pool.iter().any(|m| m.id == id.as_str() && m.stance.is_some()));
    weapons::roster()
        .filter(|w| w.id != source.weapon)
        .filter(|w| weapons::spec(&w.id).is_some_and(|s| s.riven_family.as_deref() == Some(family) && s.slot == spec.slot))
        .filter_map(|w| {
            let modes = weapons::play_modes(&w.id);
            let played = modes.iter().find(|m| m.sustainable && m.id == mode)
                .or_else(|| modes.iter().find(|m| m.sustainable))?;
            let mut target = source.clone();
            target.weapon = w.id.clone();
            let target_spec = weapons::spec(&w.id)?;
            target.assembly = source.assembly.as_ref().and_then(|a| weapons::assembly::repaired(target_spec,
                a.grip(), if a.component_key() == "loader" { a.component() } else { "" },
                if a.component_key() == "link" { a.component() } else { "" }));
            let pool = mods::pool_for_weapon(&w.id);
            let stance = stance.filter(|id| pool.iter().any(|m| m.id == id.as_str()))
                .map(String::as_str).or(target_spec.fixed_stance.as_deref())
                .or_else(|| pool.iter().find(|m| m.stance.is_some()).map(|m| m.id));
            let scenario = serde_json::to_value(&bench.scenario).expect("benchmark scenario");
            let mut request = crate::simulate::simulate_request(&scenario, &target, *played);
            let mut slots: Vec<Value> = source.mods.iter()
                .filter(|id| !source_pool.iter().any(|m| m.id == id.as_str() && m.stance.is_some()))
                .map(|m| if m == builds::RIVEN_SLOT { json!(crate::simulate::RIVEN_ITEM) }
                    else if pool.iter().any(|card| card.id == mods::split_rank(m).0) { json!(m) }
                    else { Value::Null }).collect();
            slots.resize(8, Value::Null);
            slots.push(json!(source.exilus.as_ref().filter(|id| pool.iter().any(|m| m.id == mods::split_rank(id).0 && m.exilus))));
            let fixed: Vec<Value> = slots.iter().enumerate().filter(|(_, id)| id.as_str() == Some(crate::simulate::RIVEN_ITEM))
                .map(|(idx, _)| json!({ "kind": "mods", "idx": idx })).collect();
            let evos: Vec<&String> = source.evolutions.iter().filter(|id|
                wfsim_engine::data::evolutions::for_weapon(&w.id).iter().any(|e| e.id == **id)).collect();
            let start = json!({ "slots": slots, "arcane": source.arcanes, "evolutions": evos,
                "mode": played.id, "valence_element": source.valence, "fixed": fixed });
            let shape = source.riven.as_ref().map(|s| json!({ "riven_pos": s.bonuses, "riven_neg": s.malus }));
            if let Some(s) = &source.riven {
                let class = wfsim_engine::build::rivens::class_for_weapon(&w.id)?;
                let riven = wfsim_engine::build::rivens::god_roll(s, class);
                request["rivens"] = crate::simulate::riven_request(&riven);
            }
            let obj = request.as_object_mut()?;
            // Scope comes from the target's own candidate door, including its
            // exclusive cards. Only the riven is pinned to preserve its shape.
            for key in ["mods", "evolutions", "arcane", "arcane_rank"] { obj.remove(key); }
            obj.insert("strategy".into(), json!("quick"));
            obj.insert("starts".into(), json!([start]));
            obj.insert("finalists".into(), json!(1));
            obj.insert("candidate_runs".into(), json!(preset.candidate_runs));
            obj.insert("stance".into(), json!(stance.unwrap_or("")));
            if let Some(wielder) = &source.wielder { obj.insert("wielder".into(), json!(wielder)); }
            let mut context = json!({ "benchmark": bench.id, "weapon": w.id, "mode": played.id,
                "valence": source.valence, "wielder": source.wielder, "stance": stance,
                "rivens": shape.map(|s| json!({ crate::simulate::RIVEN_ITEM: s })).unwrap_or_else(|| json!({})) });
            if let Some(assembly) = &target.assembly {
                context.as_object_mut()?.extend(assembly.parts().as_object()?.clone());
            }
            Some(json!({ "source": builds::build_id(&source), "weapon": w.id, "ruler": bench.id, "request": request, "context": context }))
        }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn burston() -> Value {
        let mods = ["serration", "split_chamber", "point_strike", "vital_sense", "hellfire", "infected_clip", "stormbringer", "gilded_truth"];
        json!({ "weapon": "burston_prime", "mods": mods, "arcanes": ["primary_merciless"] })
    }

    #[test]
    fn sibling_start_keeps_the_configuration_and_uses_the_target_scope() {
        let jobs = transfers(&burston(), "standard_single_target", "cycle");
        assert_eq!(jobs.len(), 1);
        let job = &jobs[0];
        assert_eq!(job["weapon"], "burston");
        assert!(job["request"].get("mods").is_none());
        let slots = job["request"]["starts"][0]["slots"].as_array().unwrap();
        assert!(slots.contains(&Value::Null));
        assert!(!slots.contains(&json!("gilded_truth")));
        let elements: Vec<&str> = slots.iter().filter_map(Value::as_str)
            .filter(|id| ["hellfire", "infected_clip", "stormbringer"].contains(id)).collect();
        assert_eq!(elements, ["hellfire", "infected_clip", "stormbringer"]);
        assert!(crate::parse_optimize(&job["request"]).is_ok(), "{}", job["request"]);
        assert_eq!(crate::board_rows::owed_rows(&burston()).len(),
            benchmarks::all().len() * wfsim_engine::board::groups::board_modes("burston_prime").len());
    }

    #[test]
    fn a_riven_keeps_its_shape_and_is_pinned_in_the_target_start() {
        let mut record = burston();
        record["mods"][0] = json!("riven");
        record["riven_pos"] = json!(["critical_damage", "multishot"]);
        let jobs = transfers(&record, "heavy_gunner", "base");
        assert_eq!(jobs.len(), 1);
        let job = &jobs[0];
        let fixed = job["request"]["starts"][0]["fixed"].as_array().unwrap();
        assert_eq!(fixed.len(), 1);
        let idx = fixed[0]["idx"].as_u64().unwrap() as usize;
        assert_eq!(job["request"]["starts"][0]["slots"][idx], crate::simulate::RIVEN_ITEM);
        assert_eq!(job["context"]["rivens"][crate::simulate::RIVEN_ITEM]["riven_pos"], record["riven_pos"]);
        assert!(crate::parse_optimize(&job["request"]).is_ok());
    }

    #[test]
    fn an_unknown_weapon_or_a_family_without_siblings_has_no_transfers() {
        assert!(transfers(&json!({ "weapon": "missing" }), "standard_single_target", "base").is_empty());
        let mut record = burston();
        record["weapon"] = json!("torid");
        record["mods"][7] = json!("speed_trigger");
        assert!(transfers(&record, "standard_single_target", "base").is_empty());
    }

    #[test]
    fn the_existing_search_fills_an_exclusive_mods_hole_with_a_legal_target_card() {
        let mut request = transfers(&burston(), "standard_single_target", "base").remove(0)["request"].clone();
        let allowed = ["serration", "split_chamber", "point_strike", "vital_sense", "hellfire", "infected_clip", "stormbringer", "speed_trigger"];
        let excluded: Vec<&str> = mods::pool_for_weapon("burston").iter().filter(|m| !allowed.contains(&m.id)).map(|m| m.id).collect();
        request["limits"] = json!({ "exclude": { "mods": excluded, "modes": ["cycle", "transformed"] }, "exilus": false, "arcane_seats": [false] });
        request["duration"] = json!(2);
        request["runs"] = json!(2);
        request["candidate_runs"] = json!(1);
        let plan = crate::parse_optimize(&request).expect("target plan");
        let out = crate::run_optimize(plan, &wfsim_optimizer::FunnelState::default(), |_, _| {}, None);
        assert_eq!(out["ok"], true, "{out}");
        let best = &out["results"][0];
        let chosen = best["mods"].as_array().unwrap();
        assert_eq!(chosen.len(), 8, "{best}");
        assert!(chosen.contains(&json!("speed_trigger")), "{best}");
        assert!(!chosen.contains(&json!("gilded_truth")), "{best}");
        assert_eq!(best["replay"]["weapon"], "burston");
    }
}
