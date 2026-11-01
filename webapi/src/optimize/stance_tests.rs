// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

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
