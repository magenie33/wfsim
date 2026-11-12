// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE DOOR'S SECOND HALF — inbox records in, library builds out.
//!
//!   cat inbox.ndjson | wfsim-intake --done done.txt > builds.ndjson
//!   …--deadline 900   # stop taking new records after fifteen minutes
//!
//! WHY THIS EXISTS AT ALL. The endpoint that takes a submission runs on
//! Cloudflare and has no game data, so it cannot say what a build IS: telling
//! two apart needs the mod POOL, because an elemental card enters the element
//! sequence and a plain one does not, and the sequence decides the pairing
//! (Torid, six mods: 12,424 DPS against 46,583). A key derived without it would
//! be a SECOND answer to the one question that must have one — and it would be
//! the half with no evidence. So the door stores the record verbatim and this
//! derives the key, with the engine.
//!
//! AND THE CLIENT MAY NOT DERIVE IT EITHER, though it has the engine: an id
//! that arrives over the wire is an id an attacker chooses, and choosing one is
//! choosing which stored build to overwrite.
//!
//! Legal builds are retained independently of board visibility. Captured plans
//! describe the current boards and family; database admission decides whether
//! the build is new or an exact-environment observation needs correction.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};

use serde_json::{json, Value};
use wfsim_cli::args::flag;

fn id(rec: &Value, key: &str) -> String {
    rec.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

/// Normalize exact encountered builds and capture their current expansion.
/// The database decides first registration, correction and active-ID reuse.
fn intake(
    lines: impl Iterator<Item = String>,
    deadline: Option<std::time::Duration>,
    _orders: Option<&mut Vec<Value>>,
) -> (Vec<Value>, Vec<Value>, Vec<String>, usize, usize) {
    let started = std::time::Instant::now();
    let mut built: BTreeMap<String, Value> = BTreeMap::new();
    let mut done = Vec::new();
    let (mut seen, mut refused, mut left) = (0, 0, 0);
    for line in lines {
        if line.trim().is_empty() { continue; }
        if deadline.is_some_and(|d| started.elapsed() >= d) { left += 1; continue; }
        let Ok(row) = serde_json::from_str::<Value>(&line) else { continue };
        let rec = match row.get("record") {
            Some(Value::String(s)) => serde_json::from_str(s).unwrap_or(Value::Null),
            Some(v) => v.clone(), None => Value::Null,
        };
        seen += 1;
        done.push(id(&row,"id"));
        let normalized = if rec.get("kind").is_some() {
            wfsim_engine::board::equipment::canonical(&rec).map(|record| {
                (wfsim_engine::board::equipment::build_id(&record),record)
            })
        } else {
            wfsim_webapi::board_rows::library_build(&rec).map(|v| {
                let key = wfsim_engine::board::builds::build_id(&v);
                let record = wfsim_webapi::board_rows::canonical_record(&v);
                let record = if v.riven_rolls.is_empty() {record}
                    else {wfsim_webapi::board_rows::with_rolls(record,&v.riven_rolls)};
                (key,record)
            })
        };
        let (key,record) = match normalized {
            Ok(value) => value,
            Err(why) => { refused += 1; eprintln!("refused {}: {why}",id(&rec,"weapon")); continue; }
        };
        let event = format!("{key}|{}|{}",row.get("family_source").unwrap_or(&Value::Null),row.get("observation").unwrap_or(&Value::Null));
        if let Some(entry) = built.get_mut(&event) {
            entry["from"].as_array_mut().unwrap().push(json!(id(&row,"id")));
            if entry["initiator"].is_null() && row["initiator"].is_string() {entry["initiator"]=row["initiator"].clone();}
            continue;
        }
        let mut entry=json!({"id":key,"at":row["at"],"record":record,"from":[id(&row,"id")]});
        if rec.get("kind").is_none() {
            let derived=row.get("family_source").is_some_and(Value::is_string);
            entry["derived"]=json!(derived);
            entry["initiator"]=row["initiator"].clone();
            entry["flow"]=wfsim_webapi::board_flow::plan(&record,&id(&rec,"benchmark"),&id(&rec,"mode"),!derived,row.get("workflow_scope"));
            entry["observation"]=wfsim_webapi::board_flow::observation(&record,row.get("observation"));
            if let Some(source)=row["family_source"].as_str() {
                let parts:Vec<_>=source.split('|').collect();
                if parts.len()==3 && parts[0]=="workflow" {entry["flow_source"]=json!({"workflow":parts[1],"node":parts[2]});}
            }
        }
        built.insert(event,entry);
    }
    if left>0 {eprintln!("intake: {left} record(s) left for the next pass");}
    (built.into_values().collect(),Vec::new(),done,seen,refused)
}

/// QUEUE ROWS THE BOARD DOES NOT ADMIT, read as `{build_id, ruler, mode, record}`
/// one a line and written back in the queue's shape for `purge_orphans.sh`, the
/// reason counted out loud. The door is `asked_row`'s, `owed_rows`, so a row
/// this names is one no client's `/api/board/order` can open.
fn sweep_admission(lines: impl Iterator<Item = String>) -> (Vec<Value>, BTreeMap<String, usize>) {
    let mut refused = Vec::new();
    let mut why: BTreeMap<String, usize> = BTreeMap::new();
    for line in lines {
        let Ok(row) = serde_json::from_str::<Value>(&line) else { continue };
        let (build, ruler, mode) = (id(&row, "build_id"), id(&row, "ruler"), id(&row, "mode"));
        let record = row.get("record").cloned().unwrap_or(Value::Null);
        if wfsim_webapi::board_rows::owed_rows(&record).iter().any(|(r, m)| *r == ruler && *m == mode) {
            continue;
        }
        let reason = match wfsim_webapi::board_rows::scored_build(&record, &ruler) {
            Err(e) => e,
            Ok(_) if wfsim_engine::board::benchmarks::get(&ruler).is_none() => "a ruler the roster does not have".into(),
            Ok(_) => "an unsupported or unsustainable mode".into(),
        };
        *why.entry(reason).or_default() += 1;
        refused.push(json!({ "build_id": build, "ruler": ruler, "mode": mode }));
    }
    (refused, why)
}

fn main() {
    if wfsim_cli::args::has_flag("--sweep-admission") {
        let (refused, why) = sweep_admission(std::io::stdin().lock().lines().map_while(Result::ok));
        for r in &refused {
            println!("{r}");
        }
        for (reason, n) in &why {
            eprintln!("sweep: {n} row(s) refused: {reason}");
        }
        eprintln!("sweep: {} queue row(s) the board does not admit", refused.len());
        return;
    }
    let done_path = flag("--done");
    let asked_path = flag("--asked");
    let orders_path = flag("--orders");
    let mut orders: Vec<Value> = Vec::new();
    // SECONDS, and no flag means no clock — a local run over a file is not the
    // thing this bounds. The workflows pass one; see `intake`.
    let deadline = flag("--deadline")
        .and_then(|s| s.parse::<u64>().ok())
        .map(std::time::Duration::from_secs);
    let (builds, asked, done, seen, refused) =
        intake(
            std::io::stdin().lock().lines().map_while(Result::ok),
            deadline,
            Some(&mut orders),
        );

    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for row in &builds {
        if writeln!(out, "{row}").is_err() {
            break;
        }
    }
    let _ = out.flush();

    if let Some(path) = done_path {
        if let Err(e) = std::fs::write(&path, done.join("\n")) {
            eprintln!("intake: cannot write {path}: {e}");
            std::process::exit(1);
        }
    }
    // THE FILE IS WRITTEN EVEN WHEN IT IS EMPTY. The shipper reads it, and an
    // absent file and an empty one are the same thing to it — but an absent one
    // after a pass that took records in is a pass that lost them.
    if let Some(path) = asked_path {
        let body: String = asked.iter().map(|r| format!("{r}\n")).collect();
        if let Err(e) = std::fs::write(&path, body) {
            eprintln!("intake: cannot write {path}: {e}");
            std::process::exit(1);
        }
    }
    if let Some(path) = orders_path {
        let body: String = orders.iter().map(|r| format!("{r}
")).collect();
        if let Err(e) = std::fs::write(&path, body) {
            eprintln!("intake: cannot write {path}: {e}");
            std::process::exit(1);
        }
    }
    eprintln!(
        "intake: {seen} record(s) in, {refused} refused, {} build(s) out, {} fight(s) asked for",
        builds.len(),
        asked.len(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn arrival(id: &str, extra: Value) -> Value {
        let mut row=json!({"id":id,"at":"2026-11-11","record":{"weapon":"burston_prime",
            "mods":["serration","split_chamber","point_strike","vital_sense","hellfire","infected_clip","stormbringer","gilded_truth"],
            "arcanes":["primary_merciless"]}});
        row.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());row
    }
    fn run(rows: Vec<Value>) -> (Vec<Value>,Vec<Value>,Vec<String>,usize,usize) {
        intake(rows.into_iter().map(|r|r.to_string()),None,None)
    }
    #[test]
    fn canonical_builds_merge_modes_and_equivalent_spellings() {
        let a=arrival("a",json!({}));let mut b=a.clone();b["id"]=json!("b");b["record"]["mode"]=json!("base");
        b["record"]["mods"][0]=json!("split_chamber");b["record"]["mods"][1]=json!("serration");
        let (builds,asked,done,..)=run(vec![a,b]);
        assert_eq!(builds.len(),1);assert_eq!(builds[0]["from"].as_array().unwrap().len(),2);
        assert_eq!(done.len(),2);assert!(asked.is_empty());assert!(builds[0]["record"].get("mode").is_none());
    }
    #[test]
    fn captures_family_once_and_never_recurses_from_derived_output() {
        let a=arrival("a",json!({}));let mut b=a.clone();b["id"]=json!("b");b["family_source"]=json!("workflow|parent|1");
        let (builds,..)=run(vec![a,b]);assert_eq!(builds.len(),2);
        let root=builds.iter().find(|b|b["derived"]==false).unwrap();
        assert!(root["flow"]["nodes"].as_array().unwrap().iter().any(|n|n["kind"]=="optimize"));
        let child=builds.iter().find(|b|b["derived"]==true).unwrap();
        assert!(child["flow"]["nodes"].as_array().unwrap().iter().all(|n|n["kind"]=="measure"));
        assert_eq!(child["flow_source"],json!({"workflow":"parent","node":"1"}));
    }
    #[test]
    fn two_parent_continuations_of_one_build_both_survive() {
        let a=arrival("a",json!({"family_source":"workflow|parent|1"}));
        let b=arrival("b",json!({"family_source":"workflow|parent|2"}));
        let (builds,..)=run(vec![a,b]);assert_eq!(builds.len(),2);assert_eq!(builds[0]["id"],builds[1]["id"]);
        assert_ne!(builds[0]["flow_source"],builds[1]["flow_source"]);
    }
    #[test]
    fn exact_mod_ranks_and_riven_rolls_are_entities() {
        let a=arrival("a",json!({}));let mut b=a.clone();b["record"]["mods"][0]=json!("serration@2");
        let (builds,..)=run(vec![a,b]);assert_eq!(builds.len(),2);assert_ne!(builds[0]["id"],builds[1]["id"]);
        assert!(builds.iter().any(|b|b["record"]["mods"].as_array().unwrap().contains(&json!("serration@2"))));
    }
    #[test]
    fn source_context_is_separate_from_the_build_entity() {
        let (builds,..)=run(vec![arrival("a",json!({"initiator":"user:code"}))]);
        assert_eq!(builds[0]["initiator"],"user:code");assert!(!builds[0]["record"].to_string().contains("user:code"));
    }
    #[test]
    fn legality_and_board_visibility_are_separate() {
        let a=json!({"id":"a","at":"2026-11-11","record":{"weapon":"braton","mods":["serration"]}});
        let bad=json!({"id":"bad","record":{"weapon":"missing","mods":[]}});
        let (builds,asked,done,seen,refused)=run(vec![a,bad]);
        assert_eq!((builds.len(),done.len(),seen,refused),(1,2,2,1));assert!(asked.is_empty());
    }
    #[test]
    fn equipment_keeps_its_own_entity_without_weapon_work() {
        let a=json!({"id":"equipment:a","at":"2026-11-11","record":{"kind":"companion","build":{"companion":"sentinel","mods":wfsim_engine::data::companions::seatable_by("sentinel").take(10).map(|m|json!({"id":m.id})).collect::<Vec<_>>()}}});
        let (builds,asked,..)=run(vec![a]);assert_eq!(builds.len(),1);assert_eq!(builds[0]["record"]["kind"],"companion");
        assert!(builds[0].get("flow").is_none());assert!(asked.is_empty());
    }
}
