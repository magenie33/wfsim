// SPDX-License-Identifier: AGPL-3.0-or-later
//! Remove unequippable rows from a committed snapshot without reading scores.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{value::RawValue, Value};

use super::publish::INDEX_STEM;

fn strings(row: &Value, key: &str) -> Vec<String> {
    row.get(key).and_then(Value::as_array).map(|a| {
        a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect()
    }).unwrap_or_default()
}

pub(super) fn legal(weapon: &str, row: &Value) -> Result<(), String> {
    let text = |key| row.get(key).and_then(Value::as_str).unwrap_or("");
    if wfsim_engine::board::benchmarks::get(text("benchmark")).is_none() {
        return Err("unknown benchmark".into());
    }
    let riven = row.get("riven").map(|rv| wfsim_engine::build::rivens::RivenShape {
        bonuses: strings(rv, "bonuses"),
        malus: rv.get("malus").and_then(Value::as_str).map(str::to_owned),
    });
    let assembly = wfsim_engine::data::weapons::assembly::of(
        weapon, text("grip"), text("loader"), text("link"));
    let mods = strings(row, "mods");
    let build = wfsim_engine::board::builds::validate_for_board_with(
        text("benchmark"), weapon, &mods, &strings(row, "evolutions"), &strings(row, "arcanes"),
        text("valence"), riven.as_ref(), row.get("exilus").and_then(Value::as_str), assembly.as_ref(),
        row.get("wielder").and_then(|v| serde_json::from_value(v.clone()).ok()).as_ref(),
    )?;
    if build.mods.len() != mods.len() {
        return Err("a published mod was lost".into());
    }
    Ok(())
}

/// Preserve row bytes, including scores and measurement provenance. Indexes
/// are rebuilt from survivors; no measurement or missing row is invented.
pub(super) fn repair(root: &Path, state: Option<&Path>) {
    let mut counts = BTreeMap::new();
    repair_dir(root, &mut counts);
    if let Some(path) = state {
        let mut stored = super::state::stored_state(path);
        for (id, row) in &mut stored {
            row.listed = *counts.get(id).unwrap_or(&0);
        }
        super::state::write_state(path, &stored);
    }
}

fn repair_dir(root: &Path, counts: &mut BTreeMap<String, usize>) {
    let mut index = BTreeMap::new();
    let mut removed = 0;
    for entry in std::fs::read_dir(root).expect("read board directory") {
        let entry = entry.expect("read board entry");
        let path = entry.path();
        if entry.file_type().expect("board file type").is_dir() {
            repair_dir(&path, counts);
            continue;
        }
        if path.extension().is_none_or(|x| x != "json") || path.file_stem().is_some_and(|x| x == INDEX_STEM) {
            continue;
        }
        let weapon = path.file_stem().expect("weapon file").to_str().expect("weapon id");
        let source = std::fs::read_to_string(&path).expect("read weapon rows");
        let rows: Vec<Box<RawValue>> = serde_json::from_str(&source).expect("parse weapon rows");
        let mut keep = Vec::new();
        let before = rows.len();
        for raw in rows {
            let row: Value = serde_json::from_str(raw.get()).expect("parse row");
            if let Err(error) = legal(weapon, &row) {
                eprintln!("{weapon}: removed illegal snapshot row: {error}");
                removed += 1;
            } else {
                *counts.entry(row["benchmark"].as_str().expect("benchmark").to_string()).or_default() += 1;
                keep.push(raw);
            }
        }
        let leaders = super::assemble::leaders(&keep.iter().map(|r| r.as_ref()).collect::<Vec<_>>());
        if !leaders.is_empty() {
            index.insert(weapon.to_string(), leaders);
        }
        if before != keep.len() {
            std::fs::write(&path, serde_json::to_string(&keep).expect("rows json")).expect("write repaired rows");
        }
    }
    std::fs::write(root.join(format!("{INDEX_STEM}.json")), serde_json::to_string(&index).expect("index json"))
        .expect("write repaired index");
    eprintln!("{}: removed {removed} illegal row(s); measurement clocks retained", root.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_keeps_exact_measurements_and_rebuilds_the_index() {
        let path = std::env::temp_dir().join(format!("wfsim-snapshot-repair-{}", std::process::id()));
        std::fs::create_dir_all(path.join("scene")).unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../site/board/braton.json");
        let rows: Vec<Box<RawValue>> = serde_json::from_str(&std::fs::read_to_string(source).unwrap()).unwrap();
        let valid = rows.iter().find(|r| legal("braton", &serde_json::from_str(r.get()).unwrap()).is_ok()).expect("legal published fixture");
        let mut invalid: Value = serde_json::from_str(valid.get()).unwrap();
        invalid["mods"] = serde_json::json!(["missing_mod"]);
        assert!(legal("braton", &invalid).is_err());
        let input = format!("[{},{}]", valid.get(), invalid);
        for dir in [&path, &path.join("scene")] {
            std::fs::write(dir.join("braton.json"), &input).unwrap();
            std::fs::write(dir.join("index.json"), "{}").unwrap();
        }
        let state = path.join("state.yaml");
        let bench = serde_json::from_str::<Value>(valid.get()).unwrap()["benchmark"].as_str().unwrap().to_string();
        std::fs::write(&state, format!("boards:\n  {bench}:\n    submissions: 99\n    listed: 999\n    scored_at_epoch_seconds: 123\n")).unwrap();
        repair(&path, Some(&state));
        let stored = super::super::state::stored_state(&state);
        assert_eq!(stored[&bench].submissions, 99);
        assert_eq!(stored[&bench].scored_at, 123);
        assert_eq!(stored[&bench].listed, 2);
        for dir in [&path, &path.join("scene")] {
            let kept: Vec<Box<RawValue>> = serde_json::from_str(&std::fs::read_to_string(dir.join("braton.json")).unwrap()).unwrap();
            assert_eq!(kept.len(), 1);
            assert_eq!(kept[0].get(), valid.get(), "every byte of the measurement survives");
            let index: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("index.json")).unwrap()).unwrap();
            assert!(!index["braton"].as_array().unwrap().is_empty());
        }
        let before = std::fs::read(path.join("braton.json")).unwrap();
        repair(&path, Some(&state));
        assert_eq!(std::fs::read(path.join("braton.json")).unwrap(), before);
        std::fs::remove_dir_all(path).unwrap();
    }
}
