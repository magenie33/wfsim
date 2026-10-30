// SPDX-License-Identifier: AGPL-3.0-or-later
//! Permanent weapon attributes for Build Library, resolved without a fight.

use serde_json::{json, Value};
use wfsim_engine::build::loadout::resolve_for;
use wfsim_engine::model::{ModEffect, StackPolicy};
use crate::registry::{base_for, weapon_of};
use crate::request::err_json;
use crate::rivens::{mod_pool_with_rivens, riven_stat_ids_ok};

/// Bounded batches preserve input order; an unreadable build has no attributes.
pub fn static_stats_json(v: &Value) -> Value {
    let Some(builds) = v.get("builds").and_then(Value::as_array) else {
        return err_json("builds must be an array");
    };
    if builds.len() > 128 {
        return err_json("at most 128 builds per batch");
    }
    json!({ "ok": true, "stats": builds.iter().map(permanent_stats).collect::<Vec<_>>() })
}

fn permanent_stats(v: &Value) -> Value {
    let info = match weapon_of(v) { Ok(i) => i, Err(e) => return e };
    if let Err(e) = riven_stat_ids_ok(v, info) { return err_json(e); }
    let played = match crate::fight::played_mode(v, info) { Ok(m) => m, Err(e) => return e };
    let evos: Vec<&str> = played.evos.iter().map(String::as_str).collect();
    let form = if played.cycle_from.is_some() {
        crate::registry::incarnon_id(info).unwrap_or(played.single_form)
    } else { played.single_form };
    let mut base = base_for(v, form, &evos);
    // State gates and scope zoom are conditional even when a mode aims by default.
    base.gated.clear();
    base.scope.clear();
    base.tenno_scaled.clear();
    base.crit_multiplier_below_crit_chance = None;
    let mut tenno = wfsim_engine::data::tenno::default_tenno().clone();
    tenno.state.aiming = false;
    let pool = mod_pool_with_rivens(v, info, &evos);
    let mut mods = Vec::new();
    for id in v.get("mods").and_then(Value::as_array).into_iter().flatten() {
        let Some(id) = id.as_str() else { return err_json("mod ids must be strings") };
        let Some(m) = pool.iter().find(|m| m.id == id) else { return err_json(format!("unknown mod: {id}")) };
        let mut m = m.clone();
        // Keep the plain half of mixed cards; BaseOnly suppresses their triggered half.
        m.effects.retain(|e| !matches!(e, ModEffect::WhileTenno(..) | ModEffect::TennoScaled { .. }));
        mods.push(m);
    }
    if mods.len() > 10 { return err_json("at most 10 mod slots"); }
    let refs: Vec<_> = mods.iter().collect();
    let p = resolve_for(&base, &refs, StackPolicy::BaseOnly, &tenno);
    json!({ "ok": true, "form": form, "values": {
        "crit_chance": p.crit_chance,
        "crit_damage": p.crit_damage,
        "status_chance": p.status_chance,
        "multishot": p.multishot,
        "fire_rate": p.fire_rate,
        "magazine_size": p.magazine_size,
    } })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stats(mut b: Value) -> Value {
        b["evolutions_as_given"] = json!(true);
        permanent_stats(&b)
    }
    fn n(v: &Value, key: &str) -> f64 { v["values"][key].as_f64().unwrap() }
    #[test]
    fn mixed_mod_keeps_plain_multishot_without_kill_stacks() {
        let bare = stats(json!({"weapon":"laetum", "mode":"base"}));
        let mixed = stats(json!({"weapon":"laetum", "mode":"base", "mods":["galvanized_diffusion"]}));
        assert!((n(&mixed,"multishot") / n(&bare,"multishot") - 2.1).abs() < 1e-9);
    }
    #[test]
    fn aim_and_hit_buffs_do_not_enter_static_crit_chance() {
        let bare = stats(json!({"weapon":"laetum", "mode":"base"}));
        let gated = stats(json!({"weapon":"laetum", "mode":"base", "mods":["galvanized_crosshairs"],
            "arcanes":["secondary_enervate"],"aiming":true,"assume_max":true}));
        assert_eq!(gated["values"]["crit_chance"],bare["values"]["crit_chance"]);
        let plain = stats(json!({"weapon":"laetum", "mode":"base", "mods":["pistol_gambit@0"]}));
        assert!((n(&plain,"crit_chance") / n(&bare,"crit_chance") - 1.2).abs() < 1e-9);
    }
    #[test]
    fn permanent_evolution_and_mod_penalty_remain() {
        let bare = stats(json!({"weapon":"torid", "mode":"base"}));
        let evolved = stats(json!({"weapon":"torid", "mode":"base", "evolutions":["torid_commodores_fortune"]}));
        assert!((n(&evolved,"crit_chance") - n(&bare,"crit_chance") - 0.2).abs() < 1e-9);
        let penalized = stats(json!({"weapon":"torid", "mode":"base", "mods":["critical_delay"]}));
        assert!(n(&penalized,"crit_chance") > n(&bare,"crit_chance"));
        assert!(n(&penalized,"fire_rate") < n(&bare,"fire_rate"));
    }
    #[test]
    fn batch_keeps_missing_values_distinct_from_zero() {
        let r=static_stats_json(&json!({"builds":[{"weapon":"laetum","evolutions_as_given":true},{"weapon":"missing"}]}));
        assert_eq!(r["stats"][0]["ok"],json!(true));
        assert_eq!(r["stats"][1]["ok"],json!(false));
        assert!(r["stats"][1].get("values").is_none());
    }
    #[test]
    fn mode_selects_its_attack_form_without_a_fight() {
        let base = permanent_stats(&json!({"weapon":"laetum", "mode":"base"}));
        let cycle = permanent_stats(&json!({"weapon":"laetum", "mode":"cycle"}));
        assert_eq!(base["form"],json!("laetum"));
        assert_eq!(cycle["form"],json!("laetum_incarnon"));
    }
}
