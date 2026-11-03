use super::*;

fn request() -> Value {
    json!({"weapon": "braton_prime", "enemy": "thrax_centurion", "level": 100,
        "duration": 0.5, "runs": 2, "candidate_runs": 1, "finalists": 2, "strategy": "quick",
        "mods": {"serration": "search", "hellfire": "search", "cryo_rounds": "search"},
        "build_size": 2, "starts": [{"slots": ["hellfire", "cryo_rounds"]}, {"slots": []}],
        "limits": {"require": {"mods": ["serration"]}}})
}

fn run(v: &Value) -> Value {
    run_optimize(parse_optimize(v).unwrap(), &FunnelState::default(), |_, _| {}, None)
}

fn fleet(v: &Value) -> Value {
    let with = |fleet: Value| { let mut v=v.clone(); v["quick_fleet"]=fleet; run(&v) };
    let mut scores=Vec::<Value>::new();
    for step in 0..100 {
        let out=with(json!({"lead":true,"fresh":step == 0,"scores":scores}));
        let Some(pending)=out.get("pending") else { return out };
        scores=with(json!({"score":pending}))["scores"].as_array().unwrap().clone();
    }
    panic!("the fleet did not settle");
}

#[test]
fn final_requirements_replace_unfixed_cards_in_every_start_and_replay() {
    let out = run(&request());
    let rows = out["results"].as_array().unwrap();
    assert!(!rows.is_empty(), "{out}");
    assert!(out["failed_starts"].as_array().unwrap().is_empty(), "{out}");
    for row in rows {
        assert!(row["mods"].as_array().unwrap().contains(&json!("serration")), "{row}");
        let sim = crate::simulate::simulate_json(&row["replay"]);
        assert_eq!(sim["score"], row["kill_progress"], "{row}");
    }
    assert_eq!(fleet(&request())["results"], out["results"]);
}

#[test]
fn final_requirement_can_live_in_a_pinned_exilus_with_no_main_slots() {
    let mut v = request();
    v["mods"] = json!({});
    v["exilus"] = json!({"vigilante_supplies": "search"});
    v["build_size"] = json!(0);
    v["limits"]["require"]["mods"] = json!(["vigilante_supplies"]);
    v["starts"] = json!([{"slots": [null,null,null,null,null,null,null,null,"vigilante_supplies"],
        "fixed": [{"kind": "mods", "idx": 8}]}]);
    let out = run(&v);
    assert_eq!(out["results"][0]["exilus"], json!("vigilante_supplies"), "{out}");
}

#[test]
fn a_pinned_empty_seat_stays_empty_in_the_whole_scope() {
    let v = json!({"weapon":"braton", "strategy":"quick", "duration":0.5,"runs":2,"candidate_runs":1,"finalists":1,
        "limits":{"mods":0},"starts":[{"slots":[],"arcane":[],"fixed":[{"kind":"mods","idx":8},{"kind":"arcane","idx":0}]}]});
    let out = run(&v);
    assert_eq!(out["results"][0]["exilus"], "none", "{out}");
    assert_eq!(out["results"][0]["arcane"], json!(["none"]), "{out}");
}

#[test]
fn contradictions_fail_before_search() {
    let mut cases = Vec::new();
    let mut v = request();
    v["limits"]["exclude"]["mods"] = json!(["serration"]);
    cases.push((v, "both required and excluded"));
    let mut v = request();
    v["build_size"] = json!(0);
    cases.push((v, "main slots"));
    let mut v = request();
    v["limits"]["exclude"]["modes"] = json!(wfsim_engine::data::weapons::play_modes("braton_prime").iter().map(|m| m.id).collect::<Vec<_>>());
    cases.push((v, "every playable"));
    let mut v = request();
    v["weapon"] = json!("kuva_bramma");
    v["limits"]["exclude"]["valence"] = json!(wfsim_engine::data::weapons::valence_of("kuva_bramma").unwrap().elements);
    cases.push((v, "every progenitor"));
    let mut v = request();
    v["starts"] = json!([{"slots": ["hellfire", "cryo_rounds"], "fixed": [{"kind":"mods","idx":0},{"kind":"mods","idx":1}]}]);
    cases.push((v, "start 1"));
    let mut v = request();
    v["limits"]["require"]["mods"] = json!(["point_blank"]);
    cases.push((v, "unavailable"));
    let mut v = request();
    v["mods"]["serration@0"] = json!("search");
    v["limits"]["require"]["mods"] = json!(["serration", "serration@0"]);
    cases.push((v, "mutually exclusive"));
    let mut v = request();
    v["mods"]["serration@0"] = json!("search");
    v["starts"] = json!([{"slots":["serration@0"],"fixed":[{"kind":"mods","idx":0}]}]);
    cases.push((v, "mutually exclusive"));
    let mut v = request();
    v["build_size"] = json!(1);
    v["starts"] = json!([{"slots":[null],"fixed":[{"kind":"mods","idx":0}]}]);
    cases.push((v, "main slots"));
    for (v, expected) in cases {
        let e = match parse_optimize(&v) { Err(e) => e, Ok(_) => panic!("accepted contradiction: {v}") };
        assert!(e["error"].as_str().unwrap().contains(expected), "{expected}: {e}");
    }
}

#[test]
fn a_required_exact_rank_survives_a_full_start() {
    let mut v = request();
    v["mods"]["serration@0"] = json!("search");
    v["limits"]["require"]["mods"] = json!(["serration@0"]);
    let out = run(&v);
    assert!(out["failed_starts"].as_array().unwrap().is_empty(), "{out}");
    for row in out["results"].as_array().unwrap() {
        assert!(row["mods"].as_array().unwrap().contains(&json!("serration@0")), "{row}");
    }
}

#[test]
fn adversary_weapon_elements_keep_their_start_pin() {
    for weapon in ["kuva_bramma", "tenet_glaxion", "coda_motovore"] {
        let mut v = request();
        v["weapon"] = json!(weapon);
        v["mods"] = json!({});
        v["build_size"] = json!(0);
        v["limits"] = json!({});
        v["valence"] = json!({"heat":"search","cold":"search"});
        v["valence_element"] = json!("cold");
        v["valence_bonus"] = json!(0.6);
        v["starts"] = json!([{"slots":[],"valence_element":"heat","fixed":[{"kind":"valence","idx":0}]}]);
        let out=run(&v);
        assert!(!out["results"].as_array().unwrap().is_empty(), "{out}");
        for row in out["results"].as_array().unwrap() { assert_eq!(row["valence"], "heat", "{weapon}: {row}"); }
    }
}

#[test]
fn whole_scope_includes_exact_required_rank_and_all_modular_parts() {
    let mut v = json!({"weapon": "braton_prime", "strategy": "quick", "every_rank": {"mods": []},
        "limits": {"require": {"mods": ["serration@0"]}}});
    assert!(parse_optimize(&v).unwrap().pool.iter().any(|m| m.id == "serration@0"));
    for weapon in ["catchmoon_primary", "plague_kripath_two_handed"] {
        v["weapon"] = json!(weapon);
        v["limits"] = json!({});
        let plan = parse_optimize(&v).unwrap();
        assert!(plan.assemblies.len() >= 80, "{weapon}: {}", plan.assemblies.len());
        assert!(plan.replay.variants.iter().any(|x| x.3 != plan.replay.variants[0].3));
    }
}

#[test]
fn modular_parts_search_respects_individual_pins_and_fleet_replay() {
    for weapon in ["catchmoon_primary", "plague_kripath_two_handed"] {
        let meta = crate::meta::assembly_meta(weapon);
        let component = meta["component"].as_str().unwrap();
        let pair = |g: usize, c: usize| json!({"grip": meta["grips"][g]["id"], component: meta[format!("{component}s")][c]["id"]});
        let mut v = request();
        v["weapon"] = json!(weapon);
        v["mods"] = json!({});
        v["build_size"] = json!(0);
        v["limits"] = json!({});
        v["assemblies"] = json!([pair(0,0),pair(0,1),pair(1,0),pair(1,1)]);
        v["assembly"] = pair(1,1);
        v["starts"] = json!([{"slots":[], "assembly":pair(0,0), "fixed":[{"kind":"assembly","idx":0}]}]);
        let alone = run(&v);
        let rows = alone["results"].as_array().unwrap();
        assert!(!rows.is_empty(), "{alone}");
        for row in rows {
            assert_eq!(row["replay"]["assembly"]["grip"], pair(0,0)["grip"], "{row}");
            let sim = crate::simulate::simulate_json(&row["replay"]);
            assert_eq!(sim["score"], row["kill_progress"], "{row}");
        }
        let led = fleet(&v);
        assert_eq!(led["results"], alone["results"], "{weapon}");
        v["starts"][0]["fixed"] = json!([{"kind":"assembly","idx":1}]);
        let out=run(&v);
        for row in out["results"].as_array().unwrap() { assert_eq!(row["assembly"][component], pair(0,0)[component]); }
    }
}

#[test]
fn a_modular_quick_start_keeps_the_stance_out_of_its_eight_main_slots() {
    let mods = ["pressure_point", "true_steel", "organ_shatter", "reach", "fury", "fever_strike", "north_wind", "shocking_touch"];
    let v = json!({"weapon":"plague_kripath_two_handed", "stance":"shimmering_blight",
        "mods": mods.iter().map(|id| (id.to_string(), json!("fixed"))).collect::<serde_json::Map<_,_>>(),
        "strategy":"quick", "duration":0.5, "runs":2, "candidate_runs":1, "finalists":1,
        "starts":[{"slots":mods,"fixed":(0..8).map(|idx| json!({"kind":"mods","idx":idx})).collect::<Vec<_>>()}]});
    let out = run(&v);
    let row = &out["results"][0];
    assert_eq!(row["mods"].as_array().map(Vec::len), Some(9), "{out}");
    assert!(out["failed_starts"].as_array().unwrap().is_empty(), "{out}");
    let sim = crate::simulate::simulate_json(&row["replay"]);
    assert_eq!(sim["score"], row["kill_progress"]);
}
