// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

fn quick_request() -> Value {
    json!({"weapon":"magistar", "strategy":"quick", "mods":{}, "build_size":0,
        "modes":{"base":"fixed"}, "starts":[{"slots":[]}],
        "enemy":"thrax_centurion", "level":100, "duration":5, "runs":2,
        "candidate_runs":2, "finalists":10, "threads":1})
}

fn run(request: &Value) -> Value {
    let out = run_optimize(parse_optimize(request).unwrap(), &FunnelState::default(), |_, _| {}, None);
    assert_eq!(out["ok"], true, "{out}");
    out
}

#[test]
fn an_unpinned_empty_start_searches_stances_and_replays_the_selected_combos() {
    let request = quick_request();
    let plan = parse_optimize(&request).unwrap();
    let choices: Vec<_> = plan.stances.iter().flatten().map(|&i| plan.pool[i].id).collect();
    assert!(choices.contains(&"crushing_ruin") && choices.contains(&"shattering_storm"), "{choices:?}");
    assert!(plan.stances.contains(&None));
    let out = run(&request);
    let results = out["results"].as_array().unwrap();
    assert!(results.iter().any(|row| row["mods"].as_array().unwrap().iter().any(|id|
        matches!(id.as_str(), Some("crushing_ruin" | "shattering_storm")))), "{out}");
    for row in results {
        let sim = crate::simulate::simulate_json(&row["replay"]);
        assert_eq!(sim["ok"], true, "{sim}");
        assert_eq!(sim["score"], row["kill_progress"]);
        assert_eq!(sim["dps"], row["dps"]);
        assert_eq!(row["mods"], row["replay"]["mods"]);
    }
}

#[test]
fn stance_pins_keep_each_starts_choice_and_allow_a_pinned_empty_slot() {
    for choice in [json!("crushing_ruin"), json!("shattering_storm"), Value::Null] {
        let mut request = quick_request();
        let mut slots = vec![Value::Null; 10];
        slots[9] = choice.clone();
        request["starts"] = json!([{"slots":slots, "fixed":[{"kind":"mods", "idx":9}]}]);
        let out = run(&request);
        let rows = out["results"].as_array().unwrap();
        assert!(!rows.is_empty(), "{out}");
        for row in rows {
            let mods = row["mods"].as_array().unwrap();
            assert_eq!(mods.len(), usize::from(!choice.is_null()), "{row}");
            if !choice.is_null() { assert!(mods.contains(&choice), "{row}"); }
        }
    }
    let mut request = quick_request();
    let start = |id: &str| {
        let mut slots = vec![Value::Null; 10]; slots[9] = json!(id);
        json!({"slots":slots, "fixed":[{"kind":"mods", "idx":9}]})
    };
    request["starts"] = json!([start("crushing_ruin"), start("shattering_storm")]);
    let out = run(&request);
    for row in out["results"].as_array().unwrap() {
        for from in row["from_starts"].as_array().unwrap() {
            let expected = if from["start"] == 0 { "crushing_ruin" } else { "shattering_storm" };
            assert!(row["mods"].as_array().unwrap().contains(&json!(expected)), "{row}");
        }
    }
}

#[test]
fn stance_scope_and_final_pins_refuse_unavailable_or_incompatible_choices() {
    let mut request = quick_request();
    let mut slots = vec![Value::Null; 10]; slots[9] = json!("crushing_ruin");
    request["starts"] = json!([{"slots":slots, "fixed":[{"kind":"mods", "idx":9}]}]);
    request["limits"] = json!({"exclude":{"mods":["crushing_ruin"]}});
    assert!(parse_optimize(&request).is_err());
    request["limits"] = json!({});
    slots[9] = json!("shattering_storm");
    request["final"] = json!({"slots":slots, "fixed":[{"kind":"mods", "idx":9}]});
    assert!(parse_optimize(&request).err().unwrap()["error"].as_str().unwrap().contains("stance slot"));
    request["starts"] = json!([{"slots":[]}]);
    assert!(run(&request)["results"].as_array().unwrap().iter().all(|row|
        row["mods"].as_array().unwrap().contains(&json!("shattering_storm"))));
    request.as_object_mut().unwrap().remove("final");
    request["weapon"] = json!("valkyr_talons");
    let plan = parse_optimize(&request).unwrap();
    assert_eq!(plan.stances.len(), 1);
    assert_eq!(plan.pool[plan.stances[0].unwrap()].id, "hysteria");
    request["stance"] = json!("");
    assert_eq!(parse_optimize(&request).unwrap().stances.len(), 1);
}

#[test]
fn quick_calc_keeps_stances_in_their_own_slot() {
    for idx in [0, 8, 9] {
        let out = crate::candidates::candidates_json(&json!({"weapon":"magistar", "slots":[], "axis":{"kind":"mods", "idx":idx}}));
        let ids: Vec<_> = out["candidates"].as_array().unwrap().iter().filter_map(|c| c["id"].as_str()).collect();
        assert_eq!(ids.contains(&"crushing_ruin"), idx == 9, "{idx}: {out}");
        if idx == 9 { assert!(!ids.contains(&"primed_pressure_point"), "{out}"); }
    }
}

#[test]
fn stance_capacity_is_planned_per_candidate_outside_the_eight_main_slots() {
    let ids = ["primed_pressure_point", "sacrificial_steel", "primed_fever_strike", "primed_fury",
        "reach", "smite_grineer", "blood_rush", "condition_overload"];
    let mut request = quick_request();
    request["mods"] = json!(ids.iter().map(|id| ((*id).to_string(), json!("fixed"))).collect::<serde_json::Map<_, _>>());
    request["build_size"] = json!(8);
    request["exilus"] = json!({"conditions_perfection":"fixed"});
    request["modes"] = json!({"heavy":"fixed"});
    let out = run(&request);
    let rows = out["results"].as_array().unwrap();
    assert!(!rows.is_empty(), "{out}");
    for row in rows {
        assert_eq!(row["mods"].as_array().unwrap().len(), 9, "eight main cards plus stance: {row}");
        assert!(row["mods"].as_array().unwrap().contains(&json!("shattering_storm")), "{row}");
        assert!(row["forma"]["total_drain"].as_u64().unwrap() > 60, "the stance's grant is necessary: {row}");
        let sim = crate::simulate::simulate_json(&row["replay"]);
        assert_eq!(sim["ok"], true, "{sim}");
    }
}

#[test]
fn bare_search_modes_follow_the_simulators_combo_availability() {
    for weapon in ["magistar", "praedos"] {
        for mode in ["base", "forward", "block", "block_forward", "slide", "heavy", "heavy_slam"] {
            let request = json!({ "weapon": weapon, "modes": { mode: "fixed" }, "mods": {} });
            let plan = parse_optimize(&request).unwrap_or_else(|e| panic!("{weapon}/{mode}: {e}"));
            assert_eq!(plan.modes.len(), 1, "{weapon}/{mode}");
            assert_eq!(plan.modes[0].id, mode);
            let mut sim = request;
            sim["mode"] = json!(mode);
            sim["mods"] = json!([]);
            sim["runs"] = json!(1);
            sim["duration"] = json!(1);
            let result = crate::simulate::simulate_json(&sim);
            assert_eq!(result["ok"], true, "{weapon}/{mode}: {result}");
        }
    }
    for mode in ["block", "block_forward"] {
        let request = json!({ "weapon": "corufell_prime", "modes": { mode: "fixed" }, "mods": {} });
        let error = parse_optimize(&request).err().expect("an incomplete bare input is refused");
        assert_eq!(error["error"], wfsim_engine::data::weapons::STANCELESS_COMBO);
    }
    let supported = parse_optimize(&json!({
        "weapon": "corufell_prime", "mods": {},
        "modes": { "heavy": "search", "block": "search" },
    })).expect("the supported mode remains searchable");
    assert!(supported.modes.iter().all(|m| m.id != "block"));
    assert!(!supported.modes.is_empty());
}

#[test]
fn a_full_bare_quick_winner_replays_and_passes_the_board_door() {
    let ids = ["primed_pressure_point", "sacrificial_steel", "organ_shatter", "primed_reach",
        "primed_fever_strike", "north_wind", "shocking_touch", "weeping_wounds"];
    let mods: serde_json::Map<String, Value> = ids.iter().map(|id| ((*id).into(), json!("fixed"))).collect();
    let request = json!({
        "weapon": "praedos", "stance": "", "mods": mods,
        "modes": { "base": "fixed" }, "arcanes": { "melee_influence": "fixed" },
        "build_size": 8, "build_min": 8, "strategy": "quick",
        "enemy": "thrax_centurion", "level": 100, "duration": 3,
        "runs": 2, "candidate_runs": 2, "finalists": 1, "threads": 1,
    });
    let out = run_optimize(parse_optimize(&request).unwrap(), &FunnelState::default(), |_, _| {}, None);
    assert_eq!(out["ok"], true, "{out}");
    let row = &out["results"][0];
    let mods: Vec<String> = serde_json::from_value(row["mods"].clone()).expect("winner mods");
    assert_eq!(mods.len(), 8, "{row}");
    assert!(ids.iter().all(|id| mods.iter().any(|m| m == id)), "{mods:?}");
    let sim = crate::simulate::simulate_json(&row["replay"]);
    assert_eq!(sim["ok"], true, "{sim}");
    assert_eq!(sim["score"], row["kill_progress"]);
    assert_eq!(sim["dps"], row["dps"]);
    assert!(sim["dps"].as_f64().is_some_and(|d| d > 0.0));
    let evos: Vec<String> = serde_json::from_value(row["evolutions"].clone()).unwrap();
    let arcs: Vec<String> = serde_json::from_value(row["arcane"].clone()).unwrap();
    let admitted = wfsim_engine::board::builds::validate_for_board(
        "standard_single_target", "praedos", &mods, &evos, &arcs, "",
    );
    assert!(admitted.is_ok(), "{admitted:?}");
}

#[test]
fn bare_board_orders_only_ask_for_supported_inputs() {
    let record = json!({
        "weapon": "corufell_prime",
        "mods": ["primed_pressure_point", "sacrificial_steel", "organ_shatter", "primed_reach",
            "primed_fever_strike", "north_wind", "shocking_touch", "weeping_wounds"],
        "arcanes": ["melee_influence"],
    });
    let build = crate::board_rows::board_build(&record).unwrap();
    let owed = crate::board_rows::owed_rows(&record);
    assert!(!owed.is_empty(), "a complete bare build owes supported fights");
    assert!(owed.iter().all(|(_, m)| !matches!(m.as_str(), "block" | "block_forward")), "{owed:?}");
    let refused = crate::board_rows::board_order_json(&json!({
        "record": record, "ruler": "standard_single_target", "mode": "block",
    }));
    assert_eq!(refused["ok"], false, "{refused}");
    assert!(refused["error"].as_str().is_some_and(|s| s.contains("supported")), "{refused}");
    let requests = crate::board_rows::row_requests(&build, &json!({ "duration": 1, "runs": 1 }));
    assert!(!requests.is_empty());
    for (mode, request) in requests {
        assert!(!matches!(mode.id, "block" | "block_forward"));
        assert!(owed.iter().any(|(_, m)| m == mode.id), "{} owes its supported mode", mode.id);
        let output = crate::simulate::simulate_json(&request);
        assert_eq!(output["ok"], true, "{}: {output}", mode.id);
    }
}
