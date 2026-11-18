// SPDX-License-Identifier: AGPL-3.0-or-later
use std::cell::{Cell, RefCell};

use serde_json::{json, Value};
use wasm_bindgen::prelude::*;
use wfsim_webapi::{FinalRound, FinalScore};

thread_local! {
    static GENERATION: Cell<u32> = const { Cell::new(0) };
    static ROUND: RefCell<Option<(u32, FinalRound)>> = const { RefCell::new(None) };
}

pub(super) fn clear() {
    ROUND.with(|r| r.borrow_mut().take());
}

pub(super) fn dispatch(round: FinalRound) -> Value {
    let id = GENERATION.with(|g| {
        let next = g.get().wrapping_add(1);
        g.set(next);
        next
    });
    let runs = round.replays().next().expect("multiple finalists")["runs"]
        .as_u64()
        .expect("replay runs");
    let requests: Vec<String> = round.replays().map(Value::to_string).collect();
    ROUND.with(|r| r.replace(Some((id, round))));
    json!({"ok":true,"verify":requests,"generation":id,"runs":runs})
}

#[wasm_bindgen]
pub fn verify_final(
    generation: u32,
    index: u32,
    replay: &str,
    progress: &js_sys::Function,
) -> String {
    let replay: Value = match serde_json::from_str(replay) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let mut last = u32::MAX;
    let score = FinalScore::simulate(&replay, &mut |done, total| {
        let pct = done.saturating_mul(100).checked_div(total).unwrap_or(100);
        if last != pct || done == total {
            last = pct;
            let _ = progress.call1(&JsValue::NULL, &JsValue::from(done));
        }
    });
    serde_json::to_string(&(generation, index, score)).expect("finalist receipt")
}

fn collect(generation: u32, count: usize, receipts: &str) -> Option<Vec<FinalScore>> {
    let raw: Vec<String> = serde_json::from_str(receipts).ok()?;
    if raw.len() != count {
        return None;
    }
    let mut slots: Vec<_> = (0..count).map(|_| None).collect();
    for receipt in raw {
        let (id, index, score): (u32, usize, FinalScore) = serde_json::from_str(&receipt).ok()?;
        if id != generation || slots.get_mut(index)?.replace(score).is_some() {
            return None;
        }
    }
    slots.into_iter().collect()
}

#[wasm_bindgen]
pub fn finish_finals(generation: u32, receipts: &str) -> String {
    ROUND
        .with(|r| {
            let mut r = r.borrow_mut();
            if r.as_ref().is_none_or(|(id, _)| *id != generation) {
                return wfsim_webapi::err_json("stale finalist round");
            }
            let (_, round) = r.take().expect("checked round");
            let count = round.replays().len();
            match collect(generation, count, receipts) {
                Some(scores) => round.measured(scores),
                None => wfsim_webapi::err_json("invalid or incomplete finalist receipts"),
            }
        })
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipts_keep_original_order_and_reject_missing_duplicate_or_stale_work() {
        let row = |g, i, x| serde_json::to_string(&(g, i, FinalScore::Refused(json!(x)))).unwrap();
        let a = row(7, 0, 1);
        let b = row(7, 1, 2);
        let take =
            |v: Value| collect(7, 2, &v.to_string()).map(|s| serde_json::to_value(s).unwrap());
        assert_eq!(
            take(json!([b, a])),
            Some(json!([{"Refused":1},{"Refused":2}]))
        );
        for bad in [
            json!([a]),
            json!([a, a]),
            json!([a, row(6, 1, 2)]),
            json!([a, row(7, 2, 2)]),
            json!([a, "bad"]),
        ] {
            assert_eq!(take(bad), None);
        }
    }
}
