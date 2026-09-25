// SPDX-License-Identifier: AGPL-3.0-or-later
//! The rivens a request carries, and `/api/riven`.

use serde_json::{json, Value};
use wfsim_engine::model::WeaponBase;
use wfsim_engine::model::ModDef;
use crate::registry::{WeaponInfo, default_weapon_id, mod_pool_for, weapon};
use crate::request::{get_str, get_u32};

/// A mod id that must outlive the request. Riven ids are made from a name the
/// visitor typed, so they cannot be `&'static` on their own — interning keeps
/// one copy per distinct id instead of leaking a fresh one per keystroke.
fn intern(s: String) -> &'static str {
    use std::sync::{Mutex, OnceLock};
    static POOL: OnceLock<Mutex<std::collections::HashSet<&'static str>>> = OnceLock::new();
    let set = POOL.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    let mut g = set.lock().expect("intern");
    if let Some(x) = g.get(s.as_str()) {
        return x;
    }
    let leaked: &'static str = Box::leak(s.into_boxed_str());
    g.insert(leaked);
    leaked
}

pub(crate) fn riven_stat_ids_ok(v: &Value, info: &WeaponInfo) -> Result<(), String> {
    let class = riven_class(info);
    let pool = wfsim_engine::build::rivens::pool(&class);
    let known = |x: &Value| -> Result<(), String> {
        let Some(id) = x.get("id").and_then(|i| i.as_str()) else { return Ok(()) };
        if id.is_empty() || pool.iter().any(|s| s.id == id) {
            return Ok(());
        }
        Err(format!("unknown riven stat id: {id} (pool: {class})"))
    };
    for r in v.get("rivens").and_then(|a| a.as_array()).into_iter().flatten() {
        let Some(s) = r.get("spec") else { continue };
        for b in s
            .get("bonuses")
            .or_else(|| s.get("positives"))
            .and_then(|x| x.as_array())
            .into_iter()
            .flatten()
        {
            known(b)?;
        }
        if let Some(c) = s.get("malus").or_else(|| s.get("curse")) {
            known(c)?;
        }
    }
    Ok(())
}

/// The rivens a request carries, as ordinary [`ModDef`]s — the visitor's own
/// items, built against THIS weapon's disposition, so they travel with the
/// request and join the pool only for the build being resolved.
///
/// An INCOMPLETE riven is fine and resolves to whatever it does say. An
/// UNKNOWN stat id is an ERROR: `resolved_slots` drops a stat it cannot find,
/// so a typo left unrefused equips a riven that occupies a slot, drains
/// capacity and grants nothing, with the card still naming the stats.
pub(crate) fn rivens_from(v: &Value, info: &WeaponInfo) -> Vec<ModDef> {
    use wfsim_engine::build::rivens::{RivenSpec, RolledStat};
    let class = riven_class(info);
    let rolled = |x: &Value| -> Option<RolledStat> {
        if !x.is_object() {
            return None;
        }
        Some(RolledStat {
            id: x.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string(),
            roll: x.get("roll").and_then(|r| r.as_f64()).unwrap_or(1.0),
        })
    };
    v.get("rivens")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    // THE ITEM ID IS BUILT FROM `id`, and from `name` only when
                    // there is none. A card's identity is its own key, not its
                    // label — a rename must not move what a build points at —
                    // and a share link written before identities existed
                    // carries the label alone, so both still open.
                    let name = r.get("id").and_then(Value::as_str)
                        .or_else(|| r.get("name").and_then(Value::as_str))?;
                    let s = r.get("spec")?;
                    let spec = RivenSpec {
                        class: class.clone(),
                        // Both spellings: a riven saved before Bonus/Malus
                        // still equips (see `riven_json`).
                        bonuses: s
                            .get("bonuses")
                            .or_else(|| s.get("positives"))
                            .and_then(|x| x.as_array())
                            .map(|x| x.iter().filter_map(rolled).collect())
                            .unwrap_or_default(),
                        malus: s.get("malus").or_else(|| s.get("curse")).and_then(rolled),
                        rank: s.get("rank").and_then(|x| x.as_u64()).unwrap_or(8) as u32,
                        polarity: match s.get("polarity").and_then(|x| x.as_str()).unwrap_or("madurai") {
                            "vazarin" => wfsim_engine::rules::capacity::Polarity::Vazarin,
                            "naramon" => wfsim_engine::rules::capacity::Polarity::Naramon,
                            _ => wfsim_engine::rules::capacity::Polarity::Madurai,
                        },
                    };
                    Some(spec.to_mod_def(intern(format!("riven:{name}")), info.disposition))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Which riven stat pool a weapon draws from: the NARROWEST of its mod pools
/// that actually has one.
///
/// It is derived rather than declared because a weapon's riven class is not
/// always its mod class. A bow's mod pool is `bow`, and there is no bow riven
/// — the wiki's table has no bow column and its RIFLE row is the one that
/// reads "(x2 for Bows)". Walking outward from the narrowest finds `rifle`
/// for a bow today, and would find a real bow pool the day one exists,
/// without a weapon having to name it.
pub(crate) fn riven_class(info: &WeaponInfo) -> String {
    wfsim_engine::build::rivens::class_for_weapon(&info.id).unwrap_or_default().to_string()
}

/// The build's pool PLUS the request's own rivens.
pub(crate) fn mod_pool_with_rivens(v: &Value, info: &WeaponInfo, evos: &[&str]) -> Vec<ModDef> {
    let mut p = mod_pool_for(&info.id, evos);
    // …AND EVERY LOWER RANK THE REQUEST NAMES, as a list (a build) or as the
    // keys of a scope (a search), in whichever block carries mods.
    let mut named: Vec<&str> = Vec::new();
    for key in ["mods", "exilus", "stance"] {
        match v.get(key) {
            Some(Value::String(id)) => named.push(id),
            Some(Value::Array(a)) => named.extend(a.iter().filter_map(Value::as_str)),
            Some(Value::Object(o)) => named.extend(o.keys().map(String::as_str)),
            _ => {}
        }
    }
    wfsim_engine::data::mods::with_ranks(&mut p, named);
    p.extend(rivens_from(v, info));
    p
}

pub fn riven_json(v: &Value) -> Value {
    use wfsim_engine::build::rivens::{RivenSpec, RolledStat};
    let info = weapon(get_str(v, "weapon", default_weapon_id()));
    let class = riven_class(info);
    // A slot may carry a `roll` OR a `value`. `value` is what you type off a
    // riven you already own; it is turned into the roll it implies and
    // clamped into the legal band, so a number from anywhere lands legal
    // instead of being refused. An empty id is a slot not filled in yet.
    let rolled = |x: &Value| -> Option<RolledStat> {
        // A null malus is NO malus — the shape depends on it, so reading one
        // as an empty slot made every riven resolve as if it had one.
        if !x.is_object() {
            return None;
        }
        Some(RolledStat {
            id: x.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string(),
            roll: x.get("roll").and_then(|r| r.as_f64()).unwrap_or(1.0),
        })
    };
    // The wiki's words are Bonus and Malus and so are ours. A riven saved
    // before the rename still arrives spelled the old way, and a stored riven
    // is the visitor's own — it outlives our vocabulary.
    let field = |a: &str, b: &str| v.get(a).or_else(|| v.get(b));
    let spec = RivenSpec {
        class: class.clone(),
        bonuses: field("bonuses", "positives")
            .and_then(|a| a.as_array())
            .map(|a| a.iter().filter_map(rolled).collect())
            .unwrap_or_default(),
        malus: field("malus", "curse").and_then(rolled),
        rank: get_u32(v, "rank", wfsim_engine::build::rivens::MAX_RANK),
        polarity: match get_str(v, "polarity", "madurai") {
            "vazarin" => wfsim_engine::rules::capacity::Polarity::Vazarin,
            "naramon" => wfsim_engine::rules::capacity::Polarity::Naramon,
            _ => wfsim_engine::rules::capacity::Polarity::Madurai,
        },
    };
    let evo_refs: Vec<&str> = Vec::new();
    let base = WeaponBase::from_data(&info.id, true, &evo_refs);
    let disposition = info.disposition;
    let n_pos = spec.bonuses.len();
    // A typed VALUE overrides the roll, once the stat is known.
    let mut spec = spec;
    let want_value = |arr: Option<&Value>, i: usize| -> Option<f64> {
        arr?.as_array()?.get(i)?.get("value")?.as_f64()
    };
    let p = wfsim_engine::build::rivens::pool(&class);
    // A typed value arrives in the units the CARD shows — "200" means 200%,
    // "0.59" on a faction stat means a x0.59 multiplier. `from_shown` is the
    // engine's own inverse of what it printed, so a number copied off a real
    // riven means what it says instead of being clamped to an end.
    for i in 0..spec.bonuses.len() {
        let Some(v) = want_value(field("bonuses", "positives"), i) else { continue };
        let id = spec.bonuses[i].id.clone();
        if let Some(def) = p.iter().find(|x| x.id == id) {
            spec.bonuses[i].roll = spec.roll_for_value(def, true, disposition, def.from_shown(v));
        }
    }
    if let (Some(c), Some(v)) = (
        spec.malus.clone(),
        field("malus", "curse").and_then(|c| c.get("value")).and_then(|x| x.as_f64()),
    ) {
        if let Some(def) = p.iter().find(|x| x.id == c.id) {
            let r = spec.roll_for_value(def, false, disposition, def.from_shown(v));
            if let Some(cc) = spec.malus.as_mut() { cc.roll = r; }
        }
    }
    let stats: Vec<Value> = spec
        .resolved_slots(disposition)
        .into_iter()
        .map(|(slot, def, value)| {
            let bonus = slot < n_pos;
            let (lo, hi) = spec.bounds_of(def, bonus, disposition);
            let roll = if bonus { spec.bonuses[slot].roll } else { spec.malus.as_ref().map_or(1.0, |c| c.roll) };
            json!({
                // The SLOT, because a half-described card still has real
                // numbers and skipping the empty slots would slide the rest.
                "slot": if bonus { slot.to_string() } else { "malus".to_string() },
                "id": def.id, "text": def.print(value), "value": value,
                "shown": def.shown(value), "roll": roll,
                // The card's precision, so a box cannot offer a decimal the
                // game never showed anyone.
                "decimals": def.decimals(),
                // Where the roll landed in its own band, 0-100 — the one
                // number that compares two stats on one card.
                "percentile": wfsim_engine::build::rivens::percentile(roll),
                // The ends of the roll band, in shown units — what a number
                // box may be typed to without leaving the legal riven.
                "min": def.shown(lo), "max": def.shown(hi),
                // A multiplier has no sign to read, so the box needs to be
                // told the number it holds is not a percentage.
                "unit": match def.shown_as() {
                    wfsim_engine::build::rivens::Shown::Percent => "%",
                    wfsim_engine::build::rivens::Shown::Multiplier => "x",
                    wfsim_engine::build::rivens::Shown::Number => "",
                },
                "bonus": bonus, "modeled": def.kind != "unmodelled",
            })
        })
        .collect();
    json!({
        "ok": true,
        "class": class,
        "disposition": disposition,
        "name": spec.name(disposition),
        "drain": spec.drain(),
        "stats": stats,
        // Every reason at once, so the UI can point at the knob that is wrong
        // instead of only refusing.
        "illegal": spec.illegal_on(&base),
    })
}

#[cfg(test)]
mod riven_stat_id_tests {
    use super::*;
    use crate::simulate::simulate_json;

    fn req(stat: &str) -> Value {
        json!({
            "weapon": "torid",
            "mods": ["riven:Test"],
            "rivens": [{"name": "Test", "spec": {
                "bonuses": [{"id": stat, "roll": 1.1}],
                "rank": 8, "polarity": "madurai"}}],
        })
    }

    /// A typo'd riven stat is an ERROR, not a blank card.
    ///
    /// `resolved_slots` drops a stat whose id is not in the pool, so before
    /// this the riven still equipped, still drained capacity, still showed
    /// its name — and granted nothing. The failure that found it: a request
    /// built with the stats' `kind` names (`multishot_bonus`) instead of
    /// their ids (`multishot`) simulated a full build to the DECIMAL of the
    /// same build with no riven at all.
    #[test]
    fn an_unknown_riven_stat_id_is_rejected() {
        let bad = simulate_json(&req("multishot_bonus"));
        assert_eq!(bad["ok"], json!(false), "typo'd stat id must not simulate");
        assert!(
            bad["error"].as_str().unwrap_or_default().contains("multishot_bonus"),
            "the error must name the offending id: {bad:?}"
        );
        let good = simulate_json(&req("multishot"));
        assert_eq!(good["ok"], json!(true), "the real id still works: {good:?}");
        assert!(
            (good["panel"]["multishot"].as_f64().unwrap_or(0.0) - 1.0).abs() > 0.5,
            "and it actually grants multishot: {:?}",
            good["panel"]
        );
    }
}
