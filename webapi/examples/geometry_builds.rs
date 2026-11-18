// SPDX-License-Identifier: AGPL-3.0-or-later
//! Compare declared build scopes under explicit aim assumptions, retaining replay.
use serde_json::{json, Value};
use wfsim_optimizer::FunnelState;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 2, "geometry_builds calls.json output.json");
    let calls: Vec<Value> = serde_json::from_slice(&std::fs::read(&args[0]).unwrap()).unwrap();
    let mut rows = Vec::new();
    for call in calls {
        let body = &call["body"];
        let start = std::time::Instant::now();
        let truth = wfsim_webapi::grade_optimize(body, 400, 10_000, 0);
        assert_eq!(truth["ok"], true, "{}: {truth}", call["id"]);
        let plan = wfsim_webapi::parse_optimize(body).expect("valid build scope");
        let result = wfsim_webapi::run_optimize(plan, &FunnelState::default(), |_, _| {}, None);
        assert_eq!(result["ok"], true, "{}: {result}", call["id"]);
        let mut replays = Vec::new();
        for row in result["results"].as_array().expect("ranked builds") {
            assert_eq!(row["mode"], body["mode"], "{} selected mode", call["id"]);
            let request = row.get("replay").expect("a ranked build carries replay");
            let output = wfsim_webapi::route_text("/api/simulate", &request.to_string());
            let output: Value = serde_json::from_str(&output).unwrap();
            assert_eq!(output["ok"], true, "{output}");
            for (ranked, simulated) in [
                ("kill_progress", "score"),
                ("dps", "dps"),
                ("kills", "kills"),
            ] {
                assert_eq!(
                    row[ranked], output[simulated],
                    "{} replay {ranked}",
                    call["id"]
                );
            }
            replays.push(json!({"request":request,"result":output}));
        }
        eprintln!(
            "{}: {:.2}s, reference settled {}, search rank {}",
            call["id"],
            start.elapsed().as_secs_f64(),
            truth["reference"]["settled"],
            truth["search"]["rank"]
        );
        rows.push(
            json!({"id":call["id"],"request":body,"truth":truth,"search":result,"replays":replays}),
        );
        std::fs::write(&args[1], serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
    }
}
