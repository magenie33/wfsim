//! Final constraints merge with every start before validation or search.
use serde_json::{json, Value};

fn pins(s: &Value) -> Vec<Value> {
    s["fixed"].as_array().cloned().unwrap_or_default()
}
fn same(p: &Value, q: &Value) -> bool {
    p["kind"] == q["kind"] && p["idx"] == q["idx"]
}
fn slot_pin(p: &Value) -> bool {
    p["placement"] == "slot"
}
fn at(s: &Value, key: &str, i: usize) -> Value {
    s.get(key)
        .and_then(|a| a.get(i))
        .cloned()
        .unwrap_or(Value::Null)
}
fn arcane(s: &Value, i: usize) -> (String, u64) {
    let id = at(s, "arcane", i).as_str().unwrap_or("none").to_string();
    let max = wfsim_engine::data::arcanes::secondary(&id).map_or(0, |a| a.max_rank);
    (
        id,
        at(s, "arcane_rank", i).as_u64().unwrap_or(u64::from(max)),
    )
}
fn set_at(s: &mut Value, key: &str, i: usize, v: Value) {
    let mut a = s[key].as_array().cloned().unwrap_or_default();
    a.resize(a.len().max(i + 1), Value::Null);
    a[i] = v;
    s[key] = json!(a);
}

fn merge(s: &mut Value, final_: &Value) -> Result<(), String> {
    let mut fixed = pins(s);
    for p in pins(final_) {
        let kind = p["kind"].as_str().unwrap_or("");
        let i = p["idx"]
            .as_u64()
            .filter(|&i| i < 10)
            .ok_or("unknown final constraint position")? as usize;
        if kind == "mods" && i < 8 {
            if !slot_pin(&p) {
                continue;
            }
            let value = at(final_, "slots", i);
            let mut seated = s["slots"].as_array().cloned().unwrap_or_default();
            seated.resize(seated.len().max(9), Value::Null);
            if fixed
                .iter()
                .any(|q| same(q, &p) && (slot_pin(q) || seated[i].is_null()))
                && seated[i] != value
            {
                return Err(format!(
                    "slot {} is fixed to different mods in the start and final requirements",
                    i + 1
                ));
            }
            for (j, seat) in seated.iter_mut().enumerate().take(8) {
                if j != i && !value.is_null() && *seat == value {
                    if fixed
                        .iter()
                        .any(|q| q["kind"] == "mods" && q["idx"] == j && slot_pin(q))
                    {
                        return Err("the same MOD is fixed to different slots in the start and final requirements".into());
                    }
                    *seat = Value::Null;
                    fixed.retain(|q| !(q["kind"] == "mods" && q["idx"] == j));
                }
            }
            if seated[i] != value && fixed.iter().any(|q| same(q, &p)) {
                let Some(j) = (0..8)
                    .filter(|&j| {
                        j != i && !fixed.iter().any(|q| q["kind"] == "mods" && q["idx"] == j)
                    })
                    .min_by_key(|&j| !seated[j].is_null())
                else {
                    return Err(format!(
                        "no free slot for the mod required by the start at slot {}",
                        i + 1
                    ));
                };
                seated[j] = seated[i].clone();
                for q in fixed.iter_mut().filter(|q| same(q, &p)) {
                    q["idx"] = json!(j);
                }
            }
            seated[i] = value;
            s["slots"] = json!(seated);
        } else {
            let keys: Vec<&str> = match kind {
                "mods" if i >= 8 => vec!["slots"],
                "arcane" => vec!["arcane", "arcane_rank"],
                "mode" => vec!["mode"],
                "valence" => vec!["valence_element"],
                "evo" => vec!["evolutions"],
                "assembly" if i < 2 => vec![if i == 0 {
                    "grip"
                } else if final_["assembly"].get("link").is_some() {
                    "link"
                } else {
                    "loader"
                }],
                _ => return Err("unknown final constraint position".into()),
            };
            let conflict = if kind == "arcane" {
                arcane(s, i) != arcane(final_, i)
            } else {
                keys.iter().any(|&k| {
                    if kind == "assembly" {
                        s["assembly"][k] != final_["assembly"][k]
                    } else if kind == "mods" {
                        at(s, k, i) != at(final_, k, i)
                    } else {
                        s[k] != final_[k]
                    }
                })
            };
            if conflict && fixed.iter().any(|q| same(q, &p)) {
                let label = match kind {
                    "assembly" => keys[0].to_string(),
                    "arcane" => format!("arcane seat {}", i + 1),
                    "mods" => if i == 9 { "stance slot" } else { "exilus slot" }.to_string(),
                    "valence" => "progenitor element".to_string(),
                    "evo" => "evolutions".to_string(),
                    _ => kind.to_string(),
                };
                return Err(format!("{label} conflicts with the final requirements"));
            }
            for key in keys {
                if kind == "assembly" {
                    if !s["assembly"].is_object() {
                        s["assembly"] = json!({});
                    }
                    s["assembly"][key] = final_["assembly"][key].clone();
                } else if kind == "mods" || kind == "arcane" {
                    set_at(s, key, i, at(final_, key, i));
                } else {
                    s[key] = final_[key].clone();
                }
            }
        }
        fixed.retain(|q| !same(q, &p));
        fixed.push(p);
    }
    s["fixed"] = json!(fixed);
    Ok(())
}

pub(super) fn apply(v: &Value) -> Result<Value, Value> {
    let mut out = v.clone();
    let Some(final_) = v.get("final") else {
        return Ok(out);
    };
    if pins(final_).is_empty() {
        return Ok(out);
    }
    if v["strategy"] != "quick" {
        return Err(super::err_json(
            "final requirements need the quick strategy",
        ));
    }
    let mut required = super::requirements::ids(v);
    for p in pins(final_)
        .iter()
        .filter(|p| p["kind"] == "mods" && p["idx"].as_u64().is_some_and(|i| i < 8))
    {
        if let Some(id) = p["idx"]
            .as_u64()
            .and_then(|i| final_["slots"].get(i as usize))
            .and_then(Value::as_str)
        {
            if !required.iter().any(|r| r == id) {
                required.push(id.to_string());
            }
        }
    }
    if !out["limits"].is_object() {
        out["limits"] = json!({});
    }
    if !out["limits"]["require"].is_object() {
        out["limits"]["require"] = json!({});
    }
    out["limits"]["require"]["mods"] = json!(required);
    let mut starts = v["starts"]
        .as_array()
        .cloned()
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| vec![json!({"slots":[]})]);
    for (i, s) in starts.iter_mut().enumerate() {
        merge(s, final_).map_err(|e| super::err_json(format!("start {}: {e}", i + 1)))?;
    }
    out["starts"] = json!(starts);
    Ok(out)
}

pub(super) fn check(v: &Value) -> Value {
    let starts = v["starts"]
        .as_array()
        .cloned()
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| vec![json!({"slots":[]})]);
    let error = match super::parse_optimize(v) {
        Ok(_) => {
            return json!({"ok":true,"compatible":true,"starts":(0..starts.len()).map(|start| json!({"start":start,"compatible":true})).collect::<Vec<_>>()})
        }
        Err(error) => error,
    };
    let rows: Vec<Value> = starts
        .into_iter()
        .enumerate()
        .map(|(i, s)| {
            let mut one = v.clone();
            one["starts"] = json!([s]);
            match super::parse_optimize(&one) {
                Ok(_) => json!({"start":i,"compatible":true}),
                Err(e) => {
                    let error = e["error"].as_str().unwrap_or("incompatible search conditions");
                    json!({"start":i,"compatible":false,"error":error.strip_prefix("start 1: ").unwrap_or(error)})
                }
            }
        })
        .collect();
    json!({"ok":true,"compatible":false,"starts":rows,"error":error["error"]})
}
