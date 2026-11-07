// SPDX-License-Identifier: AGPL-3.0-or-later
//! Research-only benchmark; not a supported prepared-data interface.
use serde_json::Value;
use wasm_bindgen::prelude::*;
use wfsim_webapi::PreparedSimulation;

#[path = "../../../webapi/examples/prepared_cost/row.rs"]
mod fold;

thread_local! {
    static CASE: std::cell::RefCell<Option<(Value, PreparedSimulation)>> = const { std::cell::RefCell::new(None) };
}

#[wasm_bindgen]
pub fn install_data(body: &str) -> String {
    wfsim_webapi::install_pack(body)
}

#[wasm_bindgen]
pub fn install(body: &str) {
    let request: Value = serde_json::from_str(body).unwrap();
    let prepared = PreparedSimulation::new(&request).unwrap();
    CASE.with(|c| *c.borrow_mut() = Some((request, prepared)));
}

#[wasm_bindgen]
pub fn prepare(body: &str, count: u32) {
    let request: Value = serde_json::from_str(body).unwrap();
    for _ in 0..count {
        std::hint::black_box(PreparedSimulation::new(std::hint::black_box(&request)).unwrap());
    }
}

#[wasm_bindgen]
pub fn run(width: u32, retain: bool) -> String {
    CASE.with(|c| {
        let c = c.borrow();
        let (request, prepared) = c.as_ref().unwrap();
        fold::row(request, prepared, width, retain).to_string()
    })
}
