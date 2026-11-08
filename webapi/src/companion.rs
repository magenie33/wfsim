use serde_json::{json, Value};
use wfsim_engine::data::companions as comp;

use crate::request::err_json;

pub fn companion_panel_json(v: &Value) -> Value {
    let build: comp::CompanionBuild = match serde_json::from_value(v.clone()) {
        Ok(b) => b,
        Err(e) => return err_json(format!("bad companion build: {e}")),
    };
    let resolved = match comp::resolve(&build) {
        Ok(r) => r,
        Err(e) => return err_json(e),
    };
    let mut out = serde_json::to_value(&resolved).expect("companion panel");
    out["polarities"] = json!(resolved.polarities.iter().map(|p| crate::warframe::polarity_label(p)).collect::<Vec<_>>());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weapon_panel_and_fight_enforce_the_same_carrier_and_part_rules() {
        for (weapon, host) in [("deconstructor_prime", "shade"), ("verglas", "hound"), ("smeeta_claws", "adarza_kavat")] {
            let v = json!({"weapon":weapon, "duration":0.1, "runs":1, "distance":0, "wielder":{"companion":host}});
            for response in [crate::panel_json(&v), crate::simulate_json(&v)] {
                assert!(response["error"].as_str().unwrap().contains("cannot carry"), "{response}");
            }
        }
        let v = json!({"weapon":"verglas", "wielder":{"companion":"moa", "assembly":{"core":"adlet_core"}}});
        assert!(crate::panel_json(&v)["error"].as_str().unwrap().contains("not a core"));
        assert!(crate::simulate_json(&v)["error"].as_str().unwrap().contains("not a core"));
    }

    #[test]
    fn concrete_unmodded_holders_and_assemblies_reach_weapon_results() {
        let helios = crate::panel_json(&json!({"weapon":"deconstructor_prime", "wielder":{"companion":"helios"}}));
        assert_eq!(helios["wielder"]["health"], 560.0);
        let body = json!({"companion":"moa", "assembly":{"core":"drex_core", "gyro":"trux_gyro"},
            "mods":[{"id":"enhanced_vitality"}]});
        let own = companion_panel_json(&body);
        let weapon = crate::panel_json(&json!({"weapon":"verglas", "wielder":body}));
        for stat in comp::BODY_STATS { assert_eq!(own["stats"][stat], weapon["wielder"][stat]); }
        let proto = crate::panel_json(&json!({"weapon":"deconstructor_prime"}));
        assert_eq!(proto["wielder"]["name"], "Prototype Helios");
        assert_eq!(proto["wielder"]["health"], 560.0);
    }

    #[test]
    fn companion_library_identity_contains_the_normalized_assembly() {
        use wfsim_engine::board::equipment;
        let mut pool: Vec<_> = comp::seatable_by("moa").filter(|m| !m.precept).collect();
        pool.sort_by_key(|m| m.base_drain + m.max_rank);
        let mut families = std::collections::BTreeSet::new();
        let mods: Vec<_> = pool.into_iter().filter(|m| families.insert(m.family.as_deref().unwrap_or(&m.id)))
            .take(10).map(|m| json!({"id":m.id})).collect();
        let input = json!({"kind":"companion", "build":{"companion":"moa", "assembly":{"core":"drex_core"}, "mods":mods}});
        let first = equipment::canonical(&input).unwrap();
        assert_eq!(first["build"]["assembly"].as_object().unwrap().len(), 4);
        let same = equipment::canonical(&json!({"kind":"companion", "build":first["build"]})).unwrap();
        assert_eq!(equipment::build_id(&first), equipment::build_id(&same));
        let mut changed = input;
        changed["build"]["assembly"]["core"] = json!("alcrom_core");
        let changed = equipment::canonical(&changed).unwrap();
        assert_ne!(equipment::build_id(&first), equipment::build_id(&changed));
    }
}
