// SPDX-License-Identifier: AGPL-3.0-or-later
//! Stance choices share the simulator's scripts and occupy their own slot.
use serde_json::Value;
use wfsim_engine::model::ModDef;

pub(super) fn choices(v: &Value, full: &[ModDef], weapon: &str) -> Result<Vec<Option<String>>, Value> {
    let fail = super::err_json;
    let fixed = wfsim_engine::data::weapons::spec(weapon).and_then(|s| s.fixed_stance.as_deref());
    let valid = |id: &str| full.iter().any(|m| m.id == id && m.stance.is_some());
    let excluded = |id: &str| v.pointer("/limits/exclude/mods").and_then(Value::as_array)
        .is_some_and(|a| a.iter().any(|s| s.as_str() == Some(id)));
    let named = v.get("stance").and_then(Value::as_str);
    if let Some(id) = named.filter(|id| !id.is_empty()) {
        if !valid(id) || excluded(id) { return Err(fail(format!("stance {id} is unavailable or excluded"))); }
    }
    if let Some(id) = fixed {
        if excluded(id) { return Err(fail(format!("required stance {id} is excluded"))); }
        if named.is_some_and(|s| !s.is_empty() && s != id) {
            return Err(fail(format!("{weapon} requires stance {id}")));
        }
        return Ok(vec![Some(id.to_string())]);
    }
    // Explicit legacy requests retain their fixed stance, including an empty one.
    if let Some(id) = named {
        return Ok(vec![(!id.is_empty()).then(|| id.to_string())]);
    }
    if v["strategy"] != "quick" && !v["stances"].is_object() { return Ok(vec![None]); }
    let mut out = Vec::new();
    if let Some(scope) = v["stances"].as_object() {
        let fixed: Vec<_> = scope.iter().filter(|(_, mark)| mark.as_str() == Some("fixed")).collect();
        if fixed.len() > 1 { return Err(fail("only one stance can be fixed".to_string())); }
        for (id, mark) in scope {
            if !matches!(mark.as_str(), Some("fixed" | "search")) || (!fixed.is_empty() && mark != "fixed") { continue; }
            if id != "none" && (!valid(id) || excluded(id)) { return Err(fail(format!("stance {id} is unavailable or excluded"))); }
            out.push((id != "none").then(|| id.clone()));
        }
    } else {
        out.push(None);
        out.extend(full.iter().filter(|m| m.stance.is_some() && !excluded(m.id)).map(|m| Some(m.id.to_string())));
    }
    if out.is_empty() { return Err(fail("no stances in search scope".to_string())); }
    Ok(out)
}

pub(super) fn capacity(weapon: &str, stance: Option<&ModDef>) -> u32 {
    60 + stance.map_or(0, |m| wfsim_engine::rules::capacity::stance_capacity(
        m.polarity, wfsim_engine::data::weapons::stance_polarity(weapon)))
}

pub(super) fn supported(weapon: &str, form: wfsim_engine::model::FormKind, stance: Option<&ModDef>) -> bool {
    wfsim_engine::data::weapons::combo_error(weapon, form, stance.and_then(|m| m.stance)).is_none()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn expand(
    pool: &[ModDef], weapon: &str, stances: &[Option<usize>],
    base: &wfsim_engine::model::WeaponBase, variant: u32,
    forbidden: &[bool], innate: &[Option<wfsim_engine::rules::capacity::Polarity>],
    exilus: &[Option<&ModDef>], subset: &[usize],
    scenario: &wfsim_optimizer::Scenario, out: &mut Vec<wfsim_optimizer::Candidate>,
) {
    for &stance in stances {
        let seated = stance.map(|i| &pool[i]);
        if stance.is_some_and(|i| forbidden[i]) || !supported(weapon, base.form, seated) { continue; }
        let mut cards = subset.to_vec();
        cards.extend(stance);
        wfsim_optimizer::expand_one(pool, base, None, variant, capacity(weapon, seated), innate,
            exilus, &cards, &scenario.arena.tenno, scenario.policy, out);
    }
}
