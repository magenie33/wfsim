//! Entity inputs shared by task identity and execution; deployment metadata is separate.
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(crate) fn replace_ids(v: &Value, aliases: &BTreeMap<String, String>) -> Value {
    match v {
        Value::String(s) => json!(aliases.get(s).unwrap_or(s)),
        Value::Array(a) => Value::Array(a.iter().map(|v| replace_ids(v, aliases)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (aliases.get(k).unwrap_or(k).clone(), replace_ids(v, aliases)))
                .collect(),
        ),
        _ => v.clone(),
    }
}

pub(crate) fn normalize(
    kind: &str,
    input: &Value,
) -> Result<(Value, BTreeMap<String, String>), Value> {
    if !["simulation", "optimization"].contains(&kind) {
        return Err(crate::err_json("unknown calculation kind"));
    }
    let mut request = input
        .as_object()
        .cloned()
        .ok_or_else(|| crate::err_json("calculation input must be an object"))?;
    request.retain(|key, _| {
        !key.starts_with("__") && !["engine", "release", "review", "input_schema"].contains(&key.as_str())
    });
    let initial = Value::Object(request);
    let fight = crate::fight::parse_fight(&initial)?;
    let mut aliases = BTreeMap::new();
    let mut rivens = BTreeMap::new();
    let equipped = crate::simulate::mod_ids(&initial);
    let mut riven_shapes = BTreeMap::new();
    for (old, spec) in crate::rivens::riven_specs_from(&initial, fight.info) {
        if kind == "simulation" && !equipped.contains(&old) {
            continue;
        }
        let stat =
            |s: &wfsim_engine::build::rivens::RolledStat| json!({ "id": s.id, "roll": s.roll });
        let polarity = match spec.polarity {
            wfsim_engine::rules::capacity::Polarity::Vazarin => "vazarin",
            wfsim_engine::rules::capacity::Polarity::Naramon => "naramon",
            _ => "madurai",
        };
        let entity = json!({ "bonuses": spec.bonuses.iter().map(stat).collect::<Vec<_>>(),
            "malus": spec.malus.as_ref().map(stat), "rank": spec.rank, "polarity": polarity });
        let name = wfsim_engine::board::builds::identity_id(&entity.to_string());
        let id = format!("riven:{name}");
        aliases.insert(old.to_string(), id.clone());
        riven_shapes.insert(
            id,
            wfsim_engine::build::rivens::RivenShape {
                bonuses: spec.bonuses.iter().map(|stat| stat.id.clone()).collect(),
                malus: spec.malus.as_ref().map(|stat| stat.id.clone()),
            },
        );
        let row = json!({ "id": name, "spec": entity });
        if rivens.get(&name).is_some_and(|old| old != &row) {
            return Err(crate::err_json("riven identity collision"));
        }
        rivens.insert(name, row);
    }
    let mut request = replace_ids(&initial, &aliases);
    let out = request.as_object_mut().expect("object");
    out.insert("weapon".into(), json!(fight.info.id));
    // A missing mode retains its legacy constructor path; explicit mode
    // selection also affects base construction outside the fight parser.
    if initial.get("mode").is_some() {
        out.insert("mode".into(), json!(fight.mode));
        out.remove("form");
    }
    out.insert("metric".into(), json!(fight.metric.id));
    out.insert("level".into(), json!(fight.level));
    out.insert("steel_path".into(), json!(fight.steel_path));
    out.insert("eximus".into(), json!(fight.eximus));
    out.insert("headshot_pct".into(), json!(fight.headshot_pct));
    out.insert("infinite_ammo".into(), json!(fight.infinite_ammo));
    out.insert("ammo_drops".into(), json!(fight.ammo_drops));
    if fight.pickup_range_m.is_finite() {
        out.insert("pickup_range_m".into(), json!(fight.pickup_range_m));
    } else {
        out.remove("pickup_range_m");
    }
    out.insert("landscape".into(), json!(fight.landscape));
    out.insert("duration".into(), json!(fight.duration));
    out.insert("runs".into(), json!(fight.runs));
    out.insert("seed".into(), json!(fight.seed));
    out.insert(
        "rivens".into(),
        Value::Array(rivens.into_values().collect()),
    );
    if kind == "simulation" {
        out.insert("evolutions".into(), json!(fight.evos));
        out.remove("evo2");
        let mods: Vec<String> = crate::simulate::mod_ids(&initial)
            .into_iter()
            .map(|id| aliases.get(id).cloned().unwrap_or_else(|| id.to_string()))
            .collect();
        let main_len = mods.len().min(8);
        let riven = mods[..main_len]
            .iter()
            .find_map(|id| riven_shapes.get(id).map(|shape| (id.clone(), shape)));
        let main: Vec<_> = mods[..main_len]
            .iter()
            .map(|id| {
                if riven_shapes.contains_key(id) {
                    "riven".to_string()
                } else {
                    id.clone()
                }
            })
            .collect();
        let mut canonical = wfsim_engine::board::builds::canonical_mods_with(
            &fight.info.id,
            &main,
            riven.as_ref().map(|(_, shape)| *shape),
        );
        if let Some((id, _)) = riven {
            for card in &mut canonical {
                if card == "riven" {
                    *card = id.clone();
                }
            }
        }
        canonical.extend_from_slice(&mods[main_len..]);
        let mods = canonical;
        out.insert("mods".into(), json!(mods));
        crate::simulate::PreparedSimulation::new(&request)?;
    } else if kind == "optimization" {
        request = crate::optimize::task_request(&request)?;
    } else {
        return Err(crate::err_json("unknown calculation kind"));
    }
    Ok((request, aliases))
}

pub(crate) fn input_json(v: &Value) -> Value {
    let kind = v.get("kind").and_then(Value::as_str).unwrap_or("");
    match normalize(kind, &v["request"]) {
        Ok((request, aliases)) => {
            json!({ "ok": true, "schema": 1, "request": request, "context": replace_ids(&v["context"], &aliases), "aliases": aliases })
        }
        Err(error) => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_inputs_and_management_fields_share_one_materialization() {
        let raw = json!({ "weapon": "braton", "mods": ["serration", "split_chamber"], "duration": 2, "runs": 1 });
        let (plain, _) = normalize("simulation", &raw).unwrap();
        let mut changed = plain.clone();
        changed["__board_upload"] = json!(true);
        changed["__retention_window"] = json!(17);
        changed["engine"] = json!("another-engine");
        assert_eq!(normalize("simulation", &changed).unwrap().0, plain);
        assert_eq!(normalize("simulation", &plain).unwrap().0, plain);
        let before = crate::simulate_json(&raw);
        let after = crate::simulate_json(&plain);
        assert_eq!(before["dps"], after["dps"]);
    }
    #[test]
    fn actual_inputs_and_element_pairing_remain_distinct() {
        let raw = json!({ "weapon": "braton", "mods": ["hellfire", "cryo_rounds", "stormbringer"], "duration": 2, "runs": 1 });
        let first = normalize("simulation", &raw).unwrap().0;
        let mut second = raw.clone();
        second["mods"] = json!(["hellfire", "stormbringer", "cryo_rounds"]);
        assert_ne!(first, normalize("simulation", &second).unwrap().0);
        for (key, value) in [
            ("seed", json!(99)),
            ("runs", json!(2)),
            ("duration", json!(3)),
        ] {
            let mut next = raw.clone();
            next[key] = value;
            assert_ne!(first, normalize("simulation", &next).unwrap().0);
        }
    }
    #[test]
    fn riven_names_are_aliases_for_the_same_entity() {
        let raw = json!({ "weapon":"braton", "mods":["riven:personal-name"], "duration":2, "runs":1,
            "rivens":[{"id":"personal-name","spec":{"bonuses":[{"id":"damage","roll":1.0},{"id":"critical_chance","roll":1.0}]}}] });
        let mut other = raw.clone();
        other["mods"] = json!(["riven:another-name"]);
        other["rivens"][0]["id"] = json!("another-name");
        let (left, aliases) = normalize("simulation", &raw).unwrap();
        assert_eq!(left, normalize("simulation", &other).unwrap().0);
        assert!(aliases.contains_key("riven:personal-name"));
    }
}
