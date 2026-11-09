// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded, stateless engine operations for the durable relational projection.

use super::{derive, par_map, roster, Ruler};
use crate::board::entry::{earning, Earning};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, Read, Write};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ITEMS: usize = 256;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    op: String,
    items: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Gate {
    share: Option<f64>,
    default_present: bool,
    default_share: Option<f64>,
    owed: bool,
}

fn describe(item: &Value, rulers: &[Ruler]) -> Result<Value, String> {
    let id = item.get("id").and_then(Value::as_str).filter(|s| !s.is_empty() && s.len() <= 128)
        .ok_or("invalid source identity")?;
    let Some(record) = item.get("record").filter(|v| v.is_object()) else {
        return Ok(json!({"id":id,"candidate":null}));
    };
    let raw = serde_json::value::to_raw_value(record).map_err(|e| e.to_string())?;
    let Some(b) = derive(&raw, rulers) else { return Ok(json!({"id":id,"candidate":null})); };
    let canonical = wfsim_webapi::board_rows::canonical_record(&b.v);
    let canonical = if b.v.riven_rolls.is_empty() { canonical }
        else { wfsim_webapi::board_rows::with_rolls(canonical, &b.v.riven_rolls) };
    Ok(json!({"id":id,"candidate":b.identity,"bucket":[b.weapon,b.riven.is_some()],
        "admit":format!("{:016x}",b.admit),"modes":b.modes.iter().map(|m| if m.is_empty() {"base"} else {m}).collect::<Vec<_>>(),
        "default":wfsim_engine::board::builds::default_corner(&b.v),"record":canonical}))
}

fn execute(request: Request, rulers: &[Ruler]) -> Result<Vec<Value>, String> {
    if request.items.len() > MAX_ITEMS { return Err("inventory operation exceeds item bound".into()); }
    match request.op.as_str() {
        "describe" => par_map(&request.items, |item| describe(item, rulers)).into_iter().collect(),
        "facts" => request.items.into_iter().map(|mut item| {
            let ruler = item.get("ruler").and_then(Value::as_str).ok_or("invalid fact ruler")?;
            let family = wfsim_engine::board::benchmarks::family(ruler);
            let known = rulers.iter().any(|r| wfsim_engine::board::benchmarks::family(&r.id) == family);
            let group = known.then(|| family.to_string());
            let scene = wfsim_engine::board::benchmarks::get(family).and_then(|b| b.cell_dir()).unwrap_or_default();
            item["gate_ruler"] = json!(group);
            item["scene"] = json!(scene);
            Ok(item)
        }).collect(),
        "gate" => request.items.into_iter().map(|item| {
            let g: Gate = serde_json::from_value(item).map_err(|e| e.to_string())?;
            if g.share.into_iter().chain(g.default_share).any(|v| !v.is_finite()) {
                return Err("invalid entry share".into());
            }
            let decision = earning(g.share, g.default_present.then_some(g.default_share), g.owed);
            Ok(json!({"eligible":decision == Earning::Eligible,"parked":decision == Earning::Parked}))
        }).collect(),
        _ => Err("unknown inventory operation".into()),
    }
}

pub(in crate::board) fn serve() {
    let rulers = roster();
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    serde_json::to_writer(&mut out, &json!({"v":1,"ready":true,"max_items":MAX_ITEMS,"operators":["describe","facts","gate"],
        "entry_share":wfsim_engine::data::boards::KEEP_LEADER_SHARE,
        "rulers":rulers.iter().map(|r| json!({"id":r.id,"scene":r.dir})).collect::<Vec<_>>()})).expect("operator ready");
    writeln!(out).expect("operator ready"); out.flush().expect("operator ready");
    let mut input = std::io::stdin().lock();
    loop {
        let mut bytes = Vec::new();
        let n = input.by_ref().take(MAX_BYTES + 1).read_until(b'\n', &mut bytes).expect("operator input");
        if n == 0 { break; }
        let result = if n as u64 > MAX_BYTES { Err("inventory operation exceeds byte bound".into()) }
            else { serde_json::from_slice(&bytes).map_err(|e| e.to_string()).and_then(|r| execute(r, &rulers)) };
        let response = match result { Ok(items) => json!({"ok":true,"items":items}), Err(error) => json!({"ok":false,"error":error}) };
        serde_json::to_writer(&mut out, &response).expect("operator output");
        writeln!(out).expect("operator output"); out.flush().expect("operator output");
        if n as u64 > MAX_BYTES { break; }
    }
}
