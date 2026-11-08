//! Canonical Warframe and companion builds admitted to the public library.

use serde_json::{json, Value};
use crate::data::{companions as comp, warframes as wf};
use crate::rules::capacity::{slot_drain, Polarity};

fn rank(pick: &mut wf::SlotPick, max: u32) -> Result<(), String> {
    if pick.rank.is_some_and(|r| r > max) {
        return Err(format!("invalid rank for {}", pick.id));
    }
    pick.rank = None;
    pick.stacks = None;
    Ok(())
}

fn full_forma_drain(drain: u32) -> u32 {
    slot_drain(drain, Polarity::Madurai, Some(Polarity::Madurai))
}

/// Full primary loadouts only; derived equipment and personal links are absent.
pub fn canonical(input: &Value) -> Result<Value, String> {
    let build = input.get("build").ok_or("missing build")?;
    match input["kind"].as_str() {
        Some("warframe") => {
            let mut b: wf::Build = serde_json::from_value(build.clone()).map_err(|e| e.to_string())?;
            if b.mods.len() != 8 || b.exilus.is_none() || b.aura.is_none()
                || b.arcanes.len() != 2 || b.shards.len() != 5 {
                return Err("needs 10 mods, 2 arcanes and 5 shards".into());
            }
            b.operator = None;
            let mut drain = 0;
            for p in b.mods.iter_mut().chain(b.exilus.iter_mut()).chain(b.aura.iter_mut()) {
                let m = wf::mod_by_id(&p.id).ok_or_else(|| format!("unknown mod: {}", p.id))?;
                rank(p, m.max_rank)?;
                if !m.aura { drain += full_forma_drain(m.base_drain + p.rank.unwrap_or(m.max_rank)); }
            }
            let aura = b.aura.as_ref().unwrap();
            let m = wf::mod_by_id(&aura.id).unwrap();
            let capacity = 60 + wf::aura_capacity(m.base_drain + aura.rank.unwrap_or(m.max_rank), &m.polarity, Some(&m.polarity));
            if drain > capacity { return Err("exceeds capacity with full Forma".into()); }
            for p in &mut b.arcanes {
                let a = wf::arcane_by_id(&p.id).ok_or_else(|| format!("unknown arcane: {}", p.id))?;
                rank(p, a.max_rank)?;
            }
            for shard in &mut b.shards { shard.tauforged = true; }
            let resolved = wf::resolve(&b)?;
            if !resolved.refused.is_empty() { return Err(resolved.refused.join("; ")); }
            b.mods.sort_by(|a, b| {
                let (a, b) = (wf::mod_by_id(&a.id).unwrap(), wf::mod_by_id(&b.id).unwrap());
                b.base_drain.cmp(&a.base_drain).then_with(|| a.name.cmp(&b.name)).then_with(|| a.id.cmp(&b.id))
            });
            b.arcanes.sort_by(|a,b| a.id.cmp(&b.id));
            b.shards.sort_by(|a,b| (&a.shard, &a.effect).cmp(&(&b.shard, &b.effect)));
            Ok(json!({"kind":"warframe", "equipment":b.frame, "build":b}))
        }
        Some("companion") => {
            let mut b: comp::CompanionBuild = serde_json::from_value(build.clone()).map_err(|e| e.to_string())?;
            if b.mods.len() != comp::COMPANION_MOD_SLOTS { return Err("needs 10 mods".into()); }
            if !comp::companions().iter().any(|c| c.id == b.companion) { return Err("unknown companion".into()); }
            if !b.gilded && comp::companion(&b.companion).is_some_and(|c| c.assembly.is_some()) {
                return Err("modular companion library builds must be gilded".into());
            }
            let resolved = comp::resolve(&b)?;
            if !resolved.refused.is_empty() { return Err(resolved.refused.join("; ")); }
            b.assembly = resolved.assembly;
            b.gild_polarity = resolved.gild_polarity;
            let mut seen = std::collections::BTreeSet::new();
            let mut drain = 0;
            for p in &mut b.mods {
                let m = comp::seatable_by(&b.companion).find(|m| m.id == p.id)
                    .ok_or_else(|| format!("cannot seat {} on {}", p.id, b.companion))?;
                if !seen.insert(p.id.clone()) { return Err(format!("duplicate mod: {}", p.id)); }
                rank(p, m.max_rank)?;
                drain += full_forma_drain(m.base_drain + p.rank.unwrap_or(m.max_rank));
            }
            if drain > 60 { return Err("exceeds capacity with full Forma".into()); }
            b.mods.sort_by(|a,b| {
                let (a,b) = (comp::companion_mods().iter().find(|m| m.id == a.id).unwrap(),
                    comp::companion_mods().iter().find(|m| m.id == b.id).unwrap());
                b.base_drain.cmp(&a.base_drain).then_with(|| a.name.cmp(&b.name)).then_with(|| a.id.cmp(&b.id))
            });
            Ok(json!({"kind":"companion", "equipment":b.companion, "build":b}))
        }
        _ => Err("unknown equipment kind".into()),
    }
}

/// The record must already be canonical; intake derives it independently.
pub fn build_id(record: &Value) -> String {
    super::builds::identity_id(&record.to_string())
}
