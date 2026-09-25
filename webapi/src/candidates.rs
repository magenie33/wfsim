// SPDX-License-Identifier: AGPL-3.0-or-later
//! `/api/candidates` — what one position of a build may be swapped for.
//!
//! THE QUICK CALC'S CANDIDATES, WRITTEN ONCE. The quick calc ranks every
//! candidate of one position of the build on screen; the optimizer is that
//! same scan applied until nothing improves (docs/OPTIMIZER.md, "The quick
//! descent").
//! Both ask this, so a legality rule or an axis added here reaches both.
//!
//! A position is `{ kind, idx }`: a slot (`mods`, idx 0–7 main, 8 exilus,
//! 9 stance), an arcane seat (`arcane`), every open evolution tier at once
//! (`evo`), `mode`, `valence` or `assembly`. A candidate is an id plus the
//! fields to OVERRIDE on the build's own simulate request to try it.
//!
//! Capacity is not asked here: the quick calc never does, and the optimizer
//! checks it as a separate step.

use serde_json::{json, Map, Value};
use wfsim_engine::model::ModDef;

use crate::registry::{evo_forbids, evo_group, form_unlock_evo, weapon, WeaponInfo};

const EXILUS: usize = 8;

/// The build a position belongs to, as the page holds it: slots rather than
/// the wire's flat mod list, because which slot is the exilus is a fact the
/// flat list cannot carry.
struct Build<'a> {
    info: &'static WeaponInfo,
    /// Ten slot ids (ranked `card@r` or a riven's), `None` = empty.
    slots: Vec<Option<String>>,
    /// Tier → chosen evolution, tiers in order.
    evo: Vec<(u32, Option<String>)>,
    arcane: Vec<String>,
    arcane_rank: Vec<Value>,
    mode: String,
    valence_element: String,
    valence_bonus: Value,
    assembly: Map<String, Value>,
    every_mods: Vec<String>,
    every_arcanes: Vec<String>,
    req: &'a Value,
}

fn card_of(id: &str) -> &str {
    // `card@rank` names a card below its max rank; a riven id has no rank.
    match id.rsplit_once('@') {
        Some((c, r)) if !id.starts_with("riven") && r.bytes().all(|b| b.is_ascii_digit()) => c,
        _ => id,
    }
}

fn parse(v: &Value) -> Build<'_> {
    let info = weapon(v.get("weapon").and_then(Value::as_str).unwrap_or(""));
    let strs = |k: &str| -> Vec<String> {
        v.get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|x| x.as_str().unwrap_or("none").to_string()).collect())
            .unwrap_or_default()
    };
    let slots: Vec<Option<String>> = (0..10)
        .map(|i| {
            v.get("slots")
                .and_then(|s| s.get(i))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .collect();
    let tiers = wfsim_engine::data::evolutions::tier_count(evo_group(info));
    let evo = (1..=tiers)
        .map(|t| {
            let id = v
                .get("evo_sel")
                .and_then(|e| e.get(t.to_string()))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            (t, id)
        })
        .collect();
    let every = wfsim_engine::data::mods::every_rank();
    let every_list = |k: &str, dflt: &[String]| -> Vec<String> {
        v.get("every_rank")
            .and_then(|e| e.get(k))
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_else(|| dflt.to_vec())
    };
    Build {
        info,
        slots,
        evo,
        arcane: strs("arcane"),
        arcane_rank: v.get("arcane_rank").and_then(Value::as_array).cloned().unwrap_or_default(),
        mode: v.get("mode").and_then(Value::as_str).unwrap_or("base").to_string(),
        valence_element: v.get("valence_element").and_then(Value::as_str).unwrap_or("").to_string(),
        valence_bonus: v.get("valence_bonus").cloned().unwrap_or(Value::Null),
        assembly: v.get("assembly").and_then(Value::as_object).cloned().unwrap_or_default(),
        every_mods: every_list("mods", &every.mods),
        every_arcanes: every_list("arcanes", &every.arcanes),
        req: v,
    }
}

/// The mod ids a set of evolutions takes off the weapon — the union of each
/// one's own answer, which is how the builder's picker reads `evo_forbids`.
fn forbidden(forbids: &Map<String, Value>, evo: &[(u32, Option<String>)]) -> Vec<String> {
    evo.iter()
        .filter_map(|(_, id)| id.as_deref())
        .flat_map(|id| forbids.get(id).and_then(Value::as_array).cloned().unwrap_or_default())
        .filter_map(|x| x.as_str().map(str::to_string))
        .collect()
}

fn mods(b: &Build, idx: usize) -> Vec<Value> {
    let forbids = evo_forbids(b.info);
    let no = forbidden(&forbids, &b.evo);
    let mut every: Vec<ModDef> = wfsim_engine::data::mods::pool_for_weapon(&b.info.id);
    every.extend(crate::rivens::rivens_from(b.req, b.info));
    let pool: Vec<&ModDef> = every.iter().filter(|m| !no.iter().any(|n| n == m.id)).collect();
    // A card is its own family, so one card at two ranks is refused too.
    let fam = |id: &str| -> String {
        let card = card_of(id);
        every
            .iter()
            .find(|m| m.id == card)
            .and_then(|m| m.family)
            .unwrap_or(card)
            .to_string()
    };
    let cur: Vec<Option<String>> = b.slots.clone();
    let mut out = Vec::new();
    for m in &pool {
        // The card at max rank, then each lower rank the every-rank list asks for.
        let mut ids = vec![m.id.to_string()];
        if b.every_mods.iter().any(|e| e == m.id) {
            ids.extend((0..m.max_rank).map(|r| format!("{}@{}", m.id, r)));
        }
        for id in ids {
            if cur.iter().any(|c| c.as_deref() == Some(id.as_str())) {
                continue;
            }
            if idx == EXILUS && !m.exilus {
                continue;
            }
            let mut next = cur.clone();
            next[idx] = Some(id.clone());
            let list: Vec<String> = next.into_iter().flatten().collect();
            let mut fams: Vec<String> = list.iter().map(|x| fam(x)).collect();
            let n = fams.len();
            fams.sort();
            fams.dedup();
            if fams.len() != n {
                continue;
            }
            out.push(json!({ "id": id, "payload": { "mods": list } }));
        }
    }
    out
}

fn arcanes(b: &Build, idx: usize) -> Vec<Value> {
    let Some(seat) = b.info.arcane_pools.get(idx) else { return Vec::new() };
    let cur_id = b.arcane.get(idx).cloned().unwrap_or_else(|| "none".into());
    // The seat's own id as the page spells it: ranked below its max.
    let here = match (
        wfsim_engine::data::arcanes::secondary(&cur_id),
        b.arcane_rank.get(idx).and_then(Value::as_u64),
    ) {
        (Some(a), Some(r)) if r < u64::from(a.max_rank) => format!("{}@{}", a.id, r),
        _ => cur_id.clone(),
    };
    let mut out = Vec::new();
    for a in wfsim_engine::data::arcanes::pool_for_weapon(&b.info.id, seat) {
        if a.id == "none" {
            continue;
        }
        let mut rows: Vec<(String, Option<u32>)> = vec![(a.id.clone(), None)];
        if b.every_arcanes.contains(&a.id) {
            rows.extend((0..a.max_rank).map(|r| (format!("{}@{}", a.id, r), Some(r))));
        }
        for (id, rank) in rows {
            if id == here {
                continue;
            }
            let mut next = b.arcane.clone();
            let mut ranks = b.arcane_rank.clone();
            while next.len() <= idx {
                next.push("none".into());
            }
            while ranks.len() <= idx {
                ranks.push(Value::Null);
            }
            next[idx] = a.id.clone();
            ranks[idx] = rank.map_or(Value::Null, |r| json!(r));
            out.push(json!({ "id": id, "payload": { "arcane": next, "arcane_rank": ranks } }));
        }
    }
    out
}

fn evolutions(b: &Build) -> Vec<Value> {
    let group = evo_group(b.info);
    let forbids = evo_forbids(b.info);
    // The ladder: tier N opens once N-1 is filled.
    let mut open_to = 0;
    for (t, id) in &b.evo {
        if id.is_none() {
            break;
        }
        open_to = *t;
    }
    let open_to = open_to + 1;
    let equipped: Vec<&str> = b.slots.iter().flatten().map(|s| card_of(s)).collect();
    let mut out = Vec::new();
    for (t, chosen) in b.evo.iter().filter(|(t, _)| *t <= open_to) {
        for o in wfsim_engine::data::evolutions::options(group, *t) {
            if chosen.as_deref() == Some(o.id.as_str()) {
                continue;
            }
            let next: Vec<(u32, Option<String>)> = b
                .evo
                .iter()
                .map(|(tt, id)| (*tt, if tt == t { Some(o.id.clone()) } else { id.clone() }))
                .collect();
            // An evolution that would evict an equipped card is that swap plus
            // an eviction, not a one-step change.
            let no = forbidden(&forbids, &next);
            if equipped.iter().any(|m| no.iter().any(|n| n == m)) {
                continue;
            }
            let list: Vec<String> = next.into_iter().filter_map(|(_, id)| id).collect();
            out.push(json!({ "id": o.id, "payload": { "evolutions": list } }));
        }
    }
    out
}

fn modes(b: &Build) -> Vec<Value> {
    let all = wfsim_engine::data::weapons::play_modes(&b.info.id);
    let ids: Vec<&str> = all.iter().filter(|m| m.sustainable).map(|m| m.id).collect();
    if ids.len() < 2 {
        return Vec::new();
    }
    // A card that takes the Incarnon form off the weapon leaves every cycle
    // with nothing to transform into.
    let cost: Vec<String> = form_unlock_evo(b.info)
        .and_then(|u| evo_forbids(b.info).get(u).and_then(Value::as_array).cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|x| x.as_str().map(str::to_string))
        .collect();
    let blocked = b.slots.iter().flatten().any(|s| cost.iter().any(|c| c == card_of(s)));
    let is_cycle = |id: &str| id == "cycle" || id.ends_with("_cycle");
    ids.into_iter()
        .filter(|id| *id != b.mode && !(blocked && is_cycle(id)))
        .map(|id| json!({ "id": id, "payload": { "mode": id } }))
        .collect()
}

fn valence(b: &Build) -> Vec<Value> {
    let Some(spec) = wfsim_engine::data::weapons::valence_of(&b.info.id) else { return Vec::new() };
    spec.elements
        .iter()
        .filter(|e| **e != b.valence_element)
        .map(|e| json!({ "id": e, "payload": { "valence_element": e, "valence_bonus": b.valence_bonus } }))
        .collect()
}

fn assembly(b: &Build) -> Vec<Value> {
    let spec = crate::meta::assembly_meta(&b.info.id);
    if spec.is_null() || b.assembly.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (part, key) in [("grip", "grips"), ("loader", "loaders")] {
        for it in spec.get(key).and_then(Value::as_array).into_iter().flatten() {
            let Some(id) = it.get("id").and_then(Value::as_str) else { continue };
            if b.assembly.get(part).and_then(Value::as_str) == Some(id) {
                continue;
            }
            let mut next = b.assembly.clone();
            next.insert(part.to_string(), json!(id));
            out.push(json!({ "id": format!("{part}:{id}"), "payload": { "assembly": next } }));
        }
    }
    out
}

/// Every candidate for `axis` on the build in `v`, as `{ id, payload }`.
pub fn candidates_json(v: &Value) -> Value {
    let b = parse(v);
    let axis = v.get("axis").cloned().unwrap_or(Value::Null);
    let idx = axis.get("idx").and_then(Value::as_u64).unwrap_or(0) as usize;
    let list = match axis.get("kind").and_then(Value::as_str).unwrap_or("mods") {
        "arcane" => arcanes(&b, idx),
        "evo" => evolutions(&b),
        "mode" => modes(&b),
        "valence" => valence(&b),
        "assembly" => assembly(&b),
        _ if idx < 10 => mods(&b, idx),
        _ => Vec::new(),
    };
    json!({ "ok": true, "candidates": list })
}
