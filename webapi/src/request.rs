// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reading a request's fields and shaping the small parts of a response every
//! endpoint shares: the error payload, rounding, a display name from an id.

use serde_json::{json, Value};

/// One and three decimals — a replay ships 600 frames per series, and full
/// f64 text triples the payload for digits no chart can draw.
pub(crate) fn r1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}
pub(crate) fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// mod-id → Title Case display name ("primed_target_cracker" → "Primed Target Cracker").
pub(crate) fn prettify(id: &str) -> String {
    id.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn get_str<'a>(v: &'a Value, key: &str, default: &'a str) -> &'a str {
    v.get(key).and_then(|x| x.as_str()).unwrap_or(default)
}
pub(crate) fn get_f64(v: &Value, key: &str, default: f64) -> f64 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or(default)
}
pub(crate) fn get_u32(v: &Value, key: &str, default: u32) -> u32 {
    v.get(key)
        .and_then(|x| x.as_u64())
        .map(|n| n as u32)
        .unwrap_or(default)
}
pub(crate) fn get_bool(v: &Value, key: &str, default: bool) -> bool {
    v.get(key).and_then(|x| x.as_bool()).unwrap_or(default)
}

pub fn err_json(msg: impl Into<String>) -> Value {
    json!({ "ok": false, "error": msg.into() })
}
