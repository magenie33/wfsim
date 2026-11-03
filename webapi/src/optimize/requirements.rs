//! Final-build requirements are containment rules, independent of start slots.
use serde_json::Value;
use wfsim_engine::model::ModDef;

pub(super) fn ids(v: &Value) -> Vec<String> {
    v.pointer("/limits/require/mods").and_then(Value::as_array).into_iter().flatten()
        .filter_map(Value::as_str).map(str::to_string).collect()
}

fn family(m: &ModDef) -> &str {
    m.family.unwrap_or_else(|| m.id.split('@').next().unwrap_or(m.id))
}

fn compatible(cards: &[&ModDef]) -> Result<(), String> {
    for (i, a) in cards.iter().enumerate() {
        for b in &cards[..i] {
            if a.id != b.id && family(a) == family(b) {
                return Err(format!("{} and {} cannot both be required or pinned: the cards are mutually exclusive", a.id, b.id));
            }
        }
    }
    Ok(())
}

pub(super) struct Scope<'a> {
    pub pool: &'a [ModDef],
    pub exilus: &'a [Option<ModDef>],
    pub required: &'a [String],
    pub legacy: &'a [String],
    pub slots: usize,
    pub forbids: &'a [Vec<bool>],
    pub evos: &'a [Vec<String>],
    pub assemblies: &'a [Value],
    pub arcanes: &'a [Vec<String>],
    pub cap: u32,
}

pub(super) fn validate(v: &Value, scope: Scope<'_>) -> Result<(), Value> {
    let Scope { pool, exilus, required, legacy, slots, forbids, evos, assemblies, arcanes, cap } = scope;
    let fail = |s: String| super::err_json(s);
    let excluded = |axis: &str, id: &str| v.get("limits").and_then(|l| l.get("exclude")).and_then(|e| e.get(axis))
        .and_then(Value::as_array).is_some_and(|a| a.iter().any(|x| x.as_str() == Some(id)));
    if let Ok(info) = crate::registry::weapon_of(v) {
        let modes = wfsim_engine::data::weapons::play_modes(&info.id);
        let available: Vec<_> = modes.iter().filter(|m| m.sustainable).collect();
        if !available.is_empty() && available.iter().all(|m| excluded("modes", m.id)) {
            return Err(fail("every playable mode is excluded".into()));
        }
        if wfsim_engine::data::weapons::valence_of(&info.id).is_some_and(|s| s.elements.iter().all(|e| excluded("valence", e))) {
            return Err(fail("every progenitor element is excluded".into()));
        }
    }
    let lookup = |id: &str| pool.iter().chain(exilus.iter().flatten()).find(|m| m.id == id);
    let mut mandatory = Vec::new();
    for id in required.iter().chain(legacy) {
        if v.pointer("/limits/exclude/mods").and_then(Value::as_array).is_some_and(|a| a.iter().any(|x| x.as_str() == Some(id))) {
            return Err(fail(format!("{id} is both required and excluded")));
        }
        let m = lookup(id).ok_or_else(|| fail(format!("required mod {id} is unavailable for this weapon or excluded")))?;
        if m.stance.is_some() && required.contains(id) { return Err(fail(format!("{id} is a stance; choose it in the stance slot"))); }
        if !mandatory.iter().any(|x: &&ModDef| x.id == m.id) { mandatory.push(m); }
    }
    compatible(&mandatory).map_err(fail)?;
    let starts = v.get("starts").and_then(Value::as_array);
    let blank = serde_json::json!({});
    let list: Vec<&Value> = starts.filter(|a| !a.is_empty()).map_or_else(|| vec![&blank], |a| a.iter().collect());
    for (si, s) in list.into_iter().enumerate() {
        let error = |msg: String| fail(format!("start {}: {msg}", si + 1));
        let fixed = |kind: &str, idx: usize| s.get("fixed").and_then(Value::as_array).is_some_and(|a| a.iter().any(|p| p["kind"] == kind && p["idx"] == idx));
        let mut cards = mandatory.clone();
        let mut pinned_main = Vec::new();
        let mut empty_main = 0;
        let mut pinned_exilus = None;
        for i in 0..9 {
            if !fixed("mods", i) { continue; }
            let id = s.get("slots").and_then(|a| a.get(i)).and_then(Value::as_str);
            let Some(id) = id else {
                if i < slots { empty_main += 1; }
                if i == 8 && !exilus.iter().any(Option::is_none) {
                    return Err(error("the pinned empty exilus is outside this scope".into()));
                }
                continue;
            };
            if i < 8 && i >= slots { return Err(error(format!("pinned slot {} exceeds the limit of {slots} mods", i + 1))); }
            let m = lookup(id).ok_or_else(|| error(format!("pinned mod {id} is unavailable or excluded")))?;
            if pinned_main.iter().any(|p: &&ModDef| p.id == id) {
                return Err(error(format!("{id} is pinned in more than one slot")));
            }
            if i == 8 {
                if !exilus.iter().flatten().any(|x| x.id == id) { return Err(error(format!("pinned exilus {id} is unavailable or the exilus is left empty"))); }
                pinned_exilus = Some(m);
            } else { pinned_main.push(m); }
            if !cards.iter().any(|c| c.id == m.id) { cards.push(m); }
        }
        compatible(&cards).map_err(error)?;
        let can_exilus = |m: &&ModDef| !legacy.iter().any(|id| id == m.id) && !pinned_main.iter().any(|p| p.id == m.id)
            && if fixed("mods", 8) { pinned_exilus.is_some_and(|p| p.id == m.id) } else { exilus.iter().flatten().any(|x| x.id == m.id) };
        let main_count = cards.iter().filter(|m| m.stance.is_none()).count() - usize::from(cards.iter().any(can_exilus));
        if main_count + empty_main > slots { return Err(error(format!("required and pinned mods need {main_count} main slots, but only {} are free", slots - empty_main))); }
        let planned: Vec<_> = cards.iter().filter(|m| m.stance.is_none()).map(|m|
            wfsim_engine::rules::capacity::PlannedMod { base_drain: m.base_drain, polarity: m.polarity }).collect();
        if wfsim_engine::rules::capacity::plan_forma(cap, &vec![None; planned.len()], &planned).is_err() {
            return Err(error("required and pinned mods cannot fit the weapon's capacity, even with Forma".into()));
        }
        if !forbids.iter().enumerate().any(|(ei, f)| {
            let pinned_evos = s.get("evolutions").and_then(Value::as_array);
            (!fixed("evo", 0) || pinned_evos.is_none_or(|a| a.iter().filter_map(Value::as_str).all(|id| evos[ei].iter().any(|e| e == id))))
                && cards.iter().all(|m| pool.iter().position(|x| x.id == m.id).is_none_or(|i| !f[i]))
        }) { return Err(error("required or pinned mods conflict with the available evolutions".into())); }
        for (kind, key, axis) in [("mode", "mode", "modes"), ("valence", "valence_element", "valence")] {
            if fixed(kind, 0) && s.get(key).and_then(Value::as_str).is_some_and(|id| v.get(axis).and_then(Value::as_object).is_some_and(|o| !o.contains_key(id))) {
                return Err(error(format!("pinned {kind} is excluded or unavailable")));
            }
        }
        {
            for i in (0..arcanes.first().map_or(0, Vec::len)).filter(|&i| fixed("arcane", i)) {
                let ids = s.get("arcane").and_then(Value::as_array).cloned().unwrap_or_default();
                let ranks = s.get("arcane_rank").and_then(Value::as_array).cloned().unwrap_or_default();
                let id = ids.get(i).and_then(Value::as_str).unwrap_or("none");
                if !arcanes.iter().any(|a| a.get(i).is_some_and(|a| super::quick::arcane_matches(&ids, &ranks, i, a))) {
                    return Err(error(format!("pinned arcane {id} is excluded or its seat is left empty")));
                }
            }
        }
        if (fixed("assembly", 0) || fixed("assembly", 1)) && !assemblies.iter().any(|a| {
            let named = s.get("assembly").unwrap_or(&Value::Null);
            (!fixed("assembly", 0) || a.get("grip") == named.get("grip"))
                && (!fixed("assembly", 1) || a.get("loader").or_else(|| a.get("link")) == named.get("loader").or_else(|| named.get("link")))
        }) { return Err(error("pinned assembly part is unavailable".into())); }
    }
    Ok(())
}

pub(super) fn contains(required: &[String], pool: &[ModDef], exilus: &[Option<ModDef>], c: &wfsim_optimizer::Candidate) -> bool {
    required.iter().all(|id| c.ordered.iter().any(|&i| pool[i].id == id)
        || exilus.get(c.exilus as usize).and_then(Option::as_ref).is_some_and(|m| m.id == id))
}
