// SPDX-License-Identifier: AGPL-3.0-or-later
//! Measure the zero-transfer upper bound of reusing immutable preparation.

use serde_json::{json, Value};
use std::hint::black_box;
use std::time::Instant;
use wfsim_webapi::PreparedSimulation;

#[path = "prepared_cost/row.rs"]
mod fold;
use fold::row;

#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    assert!(args.len() >= 2, "requests.json output.json [repeats]");
    let requests: Vec<Value> = serde_json::from_slice(&std::fs::read(&args[0]).unwrap()).unwrap();
    let repeats: usize = args.get(2).map_or(3, |n| n.parse().unwrap());
    let mut output = Vec::new();
    for item in requests {
        let req = &item["request"];
        let bytes = req.to_string();
        let prepared = PreparedSimulation::new(req).unwrap();
        let start = Instant::now();
        for _ in 0..1000 {
            black_box(serde_json::from_str::<Value>(black_box(&bytes)).unwrap());
        }
        let parse_ms = start.elapsed().as_secs_f64();
        let start = Instant::now();
        for _ in 0..200 {
            black_box(PreparedSimulation::new(black_box(req)).unwrap());
        }
        let prepare_ms = start.elapsed().as_secs_f64() * 1000.0 / 200.0;
        let expected = wfsim_webapi::board_rows::board_fold_json(
            &json!({"request": req, "from": 0, "count": 40}),
        );
        assert_eq!(expected["ok"], true, "{}", item["id"]);
        let mut variants = Vec::new();
        for width in [1u32, 7, 40] {
            for retain in [false, true] {
                assert_eq!(
                    row(req, &prepared, width, retain),
                    expected,
                    "{} width={width} retain={retain}",
                    item["id"]
                );
            }
            let mut current_ms = Vec::new();
            let mut retained_ms = Vec::new();
            for rep in 0..repeats {
                for retain in if rep % 2 == 0 {
                    [false, true, true, false]
                } else {
                    [true, false, false, true]
                } {
                    let start = Instant::now();
                    let result = row(req, &prepared, width, retain);
                    let ms = start.elapsed().as_secs_f64() * 1000.0;
                    assert_eq!(result, expected);
                    if retain {
                        retained_ms.push(ms);
                    } else {
                        current_ms.push(ms);
                    }
                }
            }
            variants.push(
                json!({"width": width, "current_ms": current_ms, "retained_ms": retained_ms}),
            );
        }
        eprintln!(
            "{}: prepare={prepare_ms:.5}ms, request={}bytes",
            item["id"],
            bytes.len()
        );
        output.push(
            json!({"id": item["id"], "request_bytes": bytes.len(), "parse_ms": parse_ms,
            "prepare_ms": prepare_ms, "variants": variants, "result": expected}),
        );
    }
    std::fs::write(&args[1], serde_json::to_vec(&output).unwrap()).unwrap();
}
