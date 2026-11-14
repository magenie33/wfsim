// SPDX-License-Identifier: AGPL-3.0-or-later
//! Bounded, stateless engine operations for the durable relational projection.

use super::{derive, par_map, publication_row, roster, Ruler};
use crate::board::entry::{earning, publication_clears, publication_default, publication_floor_key, publication_shape, Earning};
use crate::board::publish::{page_row, published, Row};
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
    let mut publication = serde_json::to_value(publication_row(&b,"",0.0,String::new(),String::new())).map_err(|e|e.to_string())?;
    publication.as_object_mut().expect("row object").retain(|key,value| canonical.get(key) != Some(value));
    Ok(json!({"id":id,"candidate":b.identity,"bucket":[b.weapon,b.riven.is_some()],
        "admit":format!("{:016x}",b.admit),"modes":b.modes.iter().map(|m| if m.is_empty() {"base"} else {m}).collect::<Vec<_>>(),
        "default":wfsim_engine::board::builds::default_corner(&b.v),"record":canonical,
        "publication":publication,
        "publication_modes":b.modes.iter().enumerate().map(|(i,m)|json!({"id":if m.is_empty(){"base"}else{m},"mode":m,"ordinal":i})).collect::<Vec<_>>()}))
}

fn render(item: Value, rulers: &[Ruler]) -> Result<Value, String> {
    let bench = item.get("benchmark").and_then(Value::as_str).ok_or("publication ruler required")?;
    if !rulers.iter().any(|r| r.id == bench) { return Err("unknown publication ruler".into()); }
    let mut row: Row = serde_json::from_value(item.get("row").cloned().ok_or("publication row required")?)
        .map_err(|e| e.to_string())?;
    row.mode = item.get("mode").and_then(Value::as_str).ok_or("publication mode required")?.to_string();
    let fact = item.get("fact").ok_or("publication fact required")?;
    row.score = fact.get("score").and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok())).ok_or("invalid publication score")?;
    if !row.score.is_finite() { return Err("non-finite publication score".into()); }
    let at = fact.get("finished_at").and_then(Value::as_str).unwrap_or_default();
    row.measured_at = at.get(..16).filter(|_| at.as_bytes()[10] == b'T').unwrap_or_default().to_string();
    row.measured_by = fact.get("measured_by").and_then(Value::as_str).unwrap_or_default().to_string();
    let shape = match publication_shape(&row) {
        Some(shape) => json!(["shape",shape]), None => json!(["row",row.identity,row.mode]),
    };
    Ok(json!({"shape":serde_json::to_string(&shape).map_err(|e|e.to_string())?,
        "floor":serde_json::to_string(&publication_floor_key(&row)).map_err(|e|e.to_string())?,
        "default":publication_default(&row),"score":row.score,"riven":row.riven.is_some(),
        "page":serde_json::to_string(&page_row(bench,&row)).map_err(|e|e.to_string())?}))
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
        "render" => request.items.into_iter().map(|item|render(item,rulers)).collect(),
        "floor" => request.items.into_iter().map(|item| {
            let score=item.get("score").and_then(Value::as_f64).ok_or("publication score required")?;
            let leader=item.get("leader").and_then(Value::as_f64).ok_or("publication maximum required")?;
            if !score.is_finite() || !leader.is_finite() { return Err("non-finite publication maximum".into()); }
            Ok(json!({"keep":publication_clears(score,leader)}))
        }).collect(),
        "carry" => request.items.into_iter().map(|item| {
            let raw=item.get("raw").and_then(Value::as_str).ok_or("publication bytes required")?;
            let dir=item.get("dir").and_then(Value::as_str).ok_or("publication directory required")?;
            let raw: Box<serde_json::value::RawValue>=serde_json::from_str(raw).map_err(|e|e.to_string())?;
            let value=published(&raw);
            let keep=value.as_ref().is_none_or(|p| {
                let family=wfsim_engine::board::benchmarks::family(p.benchmark);
                rulers.iter().any(|r|wfsim_engine::board::benchmarks::family(&r.id)==family)
                    && !rulers.iter().any(|r|r.dir==dir&&wfsim_engine::board::benchmarks::family(&r.id)==family)
            });
            Ok(json!({"keep":keep,"index":value.as_ref().map(|p|json!({"benchmark":p.benchmark,
                "mode":p.mode.unwrap_or("base"),"riven":p.riven.is_some(),
                "score":p.score.get().trim_matches('"').parse::<f64>().unwrap_or(f64::NEG_INFINITY)}))}))
        }).collect(),
        "state" => request.items.into_iter().map(|item| {
            let path=std::path::Path::new(item.get("path").and_then(Value::as_str).ok_or("publication state path required")?);
            if !path.is_absolute() || std::fs::metadata(path).map_err(|e|e.to_string())?.len()>1024*1024 {
                return Err("invalid publication state path".into());
            }
            let boards: Vec<(String,usize,usize)>=serde_json::from_value(item.get("boards").cloned().ok_or("publication state counts required")?)
                .map_err(|e|e.to_string())?;
            if boards.len()!=rulers.len() || boards.iter().zip(rulers).any(|(b,r)|b.0!=r.id) {
                return Err("publication state rulers changed".into());
            }
            crate::board::state::record_states(path,&boards);
            Ok(json!({"ok":true}))
        }).collect(),
        _ => Err("unknown inventory operation".into()),
    }
}

pub(in crate::board) fn serve() {
    let rulers = roster();
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    serde_json::to_writer(&mut out, &json!({"v":1,"ready":true,"max_items":MAX_ITEMS,"operators":["describe","facts","gate","render","floor","carry","state"],
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
