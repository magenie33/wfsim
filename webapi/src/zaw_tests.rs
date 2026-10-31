use serde_json::{json, Value};
use wfsim_engine::data::weapons;

#[test]
fn zaw_metadata_candidates_and_library_records_use_links() {
    for w in weapons::roster().filter(|w| w.zaw.is_some()) {
        let meta = crate::meta::assembly_meta(&w.id);
        assert_eq!(meta["kind"], "zaw");
        assert_eq!(meta["grips"].as_array().unwrap().len(), 5);
        assert_eq!(meta["links"].as_array().unwrap().len(), 16);
        assert!(meta.get("loaders").is_none());
        let g = meta["grips"][0]["id"].as_str().unwrap();
        let built = weapons::zaws::stats(w.zaw.as_ref().unwrap(), g, "ekwana_ruhang_ii").unwrap();
        assert_eq!(meta["grip_stats_by_link"]["ekwana_ruhang_ii"][g]["damage"], json!(built.damage.values().sum::<f64>()));
        let req = json!({"weapon":w.id,"axis":{"kind":"assembly"}});
        let out = crate::candidates_json(&req);
        let candidates = out["candidates"].as_array().unwrap();
        assert_eq!(candidates.len(), 19, "four grips and fifteen links: {out}");
        for c in candidates {
            let a = crate::weapon_options::assembly_of(&c["payload"], &w.id).unwrap();
            assert_eq!(a.parts(), c["payload"]["assembly"]);
            assert!(weapons::spec_assembled(w, Some(&a)).is_some());
        }
    }
    let record = json!({"weapon":"sepfahn_one_handed","mods":[],"arcanes":["exodia_triumph","melee_exposure"],
        "grip":"peye","link":"vargeet_ii_jai"});
    let build = crate::board_rows::library_build(&record).unwrap();
    let canonical = crate::board_rows::canonical_record(&build);
    assert_eq!(canonical["grip"], record["grip"]);
    assert_eq!(canonical["link"], record["link"]);
    assert!(canonical.get("loader").is_none());
    assert_eq!(crate::board_rows::canonical_record(&crate::board_rows::library_build(&canonical).unwrap()), canonical);
    for bad in [json!({"grip":"jayap"}), json!({"loader":"bellows"}), json!({"link":"bad"})] {
        let mut rec = record.clone();
        rec.as_object_mut().unwrap().extend(bad.as_object().unwrap().clone());
        assert!(crate::board_rows::library_build(&rec).is_err(), "accepted {rec}");
    }
}

#[test]
fn zaw_builds_replay_with_their_parts_and_all_melee_inputs_fire() {
    let scene = json!({"enemy":"corrupted_heavy_gunner","level":30,"duration":2.0,"runs":2,"seed":17});
    for w in weapons::roster().filter(|w| w.zaw.is_some()) {
        let stance = wfsim_engine::data::mods::pool_for_weapon(&w.id).into_iter().find(|m| m.stance.is_some()).unwrap();
        let hand = &w.zaw.as_ref().unwrap().handedness;
        let grip = if hand == "one_handed" { "peye" } else { "seekalla" };
        let record = json!({"weapon":w.id,"mods":[stance.id],"arcanes":["exodia_valor"],"grip":grip,"link":"vargeet_ii_jai"});
        let build = crate::board_rows::library_build(&record).unwrap();
        let mut modes = 0;
        for mode in weapons::play_modes(&w.id) {
            let replay = crate::simulate_request(&scene, &build, mode);
            assert_eq!(replay["assembly"], json!({"grip":grip,"link":"vargeet_ii_jai"}));
            let out = crate::simulate_json(&replay);
            assert_eq!(out["ok"], true, "{} / {}: {out}", w.id, mode.id);
            assert!(out["dps"].as_f64().unwrap() > 0.0, "{} / {}: {out}", w.id, mode.id);
            modes += 1;
        }
        assert_eq!(modes, 7, "{}", w.id);
        let req = crate::simulate_request(&scene, &build, weapons::play_modes(&w.id)[0]);
        let mut changed = req.clone();
        changed["assembly"] = json!({"grip":grip,"link":"ekwana_ii_ruhang"});
        assert_ne!(crate::panel_json(&req)["forms"], crate::panel_json(&changed)["forms"]);
    }
    let old = json!({"weapon":"catchmoon_secondary","mods":[],"grip":"haymaker","loader":"splat"});
    let build = crate::board_rows::library_build(&old).unwrap();
    let canonical = crate::board_rows::canonical_record(&build);
    assert_eq!(canonical["loader"], "splat");
    assert!(canonical.get("link").is_none());
    let repaired = crate::weapon_options::assembly_of(&json!({"assembly":{"grip":"jayap","link":"vargeet_ii_jai"}}), "sepfahn_one_handed").unwrap();
    assert_eq!(repaired.parts(), json!({"grip":"laka","link":"vargeet_ii_jai"}));
    assert_eq!(crate::weapon_options::assembly_of(&Value::Null, "braton"), None);
}
