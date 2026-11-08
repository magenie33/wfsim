use super::*;
use crate::board_rows::{board_fold_json, board_runs_json, fold_runs};
use wfsim_engine::fight::Shard;

#[test]
fn claws_require_an_interval_and_keep_it_across_preparation() {
    let mut request = json!({"weapon":"adarza_claws", "duration":1.01,
        "runs":1, "distance":0, "headshot_pct":0, "mods":["maul", "bite"]});
    assert!(parse_fight(&request).is_err());
    for value in [json!(0), json!(-1), json!("0.2"), json!(3601)] {
        request["attack_interval_seconds"] = value;
        assert!(parse_fight(&request).is_err());
    }
    request["attack_interval_seconds"] = json!(0.2);
    let prepared = PreparedSimulation::new(&request).unwrap();
    assert_eq!(prepared.params.attack_interval_seconds, Some(0.2));
    assert_eq!(prepared.params.refire, wfsim_engine::model::RefireRule::SuppliedInterval);
    assert!(prepared.params.no_magazine);
    assert_eq!(simulate_json(&request)["shots"], json!(6.0));
    assert_eq!(crate::riven_json(&json!({"weapon":"adarza_claws"}))["ok"], json!(false));
    assert!(crate::parse_optimize(&json!({"weapon":"adarza_claws"})).is_err());
    let before = crate::tenno::wielder_from(&request, crate::registry::weapon_of(&request).unwrap());
    request["wielder"] = json!({"companion":"sentinel", "mods":[{"id":"fired_up","rank":5}]});
    let after = crate::tenno::wielder_from(&request, crate::registry::weapon_of(&request).unwrap());
    assert!(before.element_per_hit.is_none() && after.element_per_hit.is_none());
}

fn requests() -> Vec<Value> {
    let calls: Vec<Value> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/simulation-cost.json"
    ))).unwrap();
    calls.into_iter().filter(|c| c["path"] == "/api/board/fold").map(|c| {
        let mut request = c["body"]["request"].clone();
        request["duration"] = json!(8);
        request
    }).collect()
}

#[test]
fn prepared_ranges_match_the_simulator_including_rng_series_and_gas() {
    for mut request in requests() {
        for series in [false, true] {
            request["run_series"] = json!(series);
            let prepared = PreparedSimulation::new(&request).unwrap();
            for (from, count) in [(7, 3), (0, 2), (7, 3), (u32::MAX - 1, 1), (0, 0)] {
                let mut progress = Vec::new();
                let actual = prepared.shard(from, count, &mut |done, total| progress.push((done, total))).unwrap();
                let expected = simulate_shard_json(&request, from, count, &mut |_, _| {});
                assert_eq!(serde_json::to_value(actual).unwrap(), expected, "{request}: {from}+{count}");
                assert_eq!(progress, (1..=count).map(|done| (done, count)).collect::<Vec<_>>());
            }
            assert!(prepared.shard(u32::MAX, 1, &mut |_, _| {}).is_err());
        }
    }
}

#[test]
fn prepared_board_batches_keep_one_shard_per_run_and_the_json_boundary() {
    for request in requests() {
        for (from, count) in [(5, 4), (u32::MAX - 1, 3)] {
            let expected: Vec<Value> = (from..from.saturating_add(count))
                .map(|index| simulate_shard_json(&request, index, 1, &mut |_, _| {})).collect();
            let actual = board_runs_json(&json!({"request": request, "from": from, "count": count}));
            assert_eq!(actual, json!({"ok": true, "shards": expected}));
            let mut acc = Shard::default();
            for shard in &expected {
                acc.merge(&serde_json::from_value(shard.clone()).unwrap());
            }
            let folded = board_fold_json(&json!({"request": request, "from": from, "count": count}));
            assert_eq!(folded, json!({"ok": true, "acc": acc}));
        }
        let first = board_fold_json(&json!({"request": request, "from": 0, "count": 2}));
        let continued = board_fold_json(&json!({"request": request, "from": 2, "count": 3, "acc": first["acc"]}));
        let whole = board_fold_json(&json!({"request": request, "from": 0, "count": 5}));
        assert_eq!(continued, whole);
    }
}

#[test]
fn empty_board_ranges_leave_an_invalid_request_unread() {
    let request = json!({"weapon": "not_a_weapon"});
    for (from, count) in [(0, 0), (u32::MAX, 3)] {
        assert_eq!(board_runs_json(&json!({"request": request, "from": from, "count": count})),
            json!({"ok": true, "shards": []}));
        let mut acc = Shard::default();
        fold_runs(&request, &mut acc, from, count).unwrap();
        assert_eq!(serde_json::to_value(acc).unwrap(), serde_json::to_value(Shard::default()).unwrap());
    }
}

#[test]
fn prepared_refusals_are_the_simulators_refusals() {
    for request in [
        json!({"weapon": "not_a_weapon"}),
        json!({"weapon": "braton", "mods": ["not_a_mod"]}),
        json!({"weapon": "braton", "arcane": ["not_an_arcane"]}),
    ] {
        let expected = simulate_shard_json(&request, 0, 1, &mut |_, _| {});
        assert_eq!(expected["ok"], false);
        let actual = match PreparedSimulation::new(&request) {
            Err(error) => error,
            Ok(_) => panic!("invalid request accepted: {request}"),
        };
        assert_eq!(actual, expected);
        assert_eq!(board_runs_json(&json!({"request": request, "from": 0, "count": 2})), expected);
        let mut acc = Shard::default();
        assert_eq!(fold_runs(&request, &mut acc, 0, 2).unwrap_err(), expected);
    }
}

#[test]
fn a_prepared_request_is_owned_and_runs_share_no_live_state() {
    for mut request in requests() {
        let original = request.clone();
        let prepared = PreparedSimulation::new(&request).unwrap();
        request["seed"] = json!(98317);
        request["mods"] = json!([]);
        let actual = std::thread::scope(|scope| {
            let first = scope.spawn(|| prepared.shard_json(7, 2));
            let second = scope.spawn(|| prepared.shard_json(0, 3));
            [first.join().unwrap(), second.join().unwrap()]
        });
        assert_eq!(actual[0], simulate_shard_json(&original, 7, 2, &mut |_, _| {}));
        assert_eq!(actual[1], simulate_shard_json(&original, 0, 3, &mut |_, _| {}));
    }
}
