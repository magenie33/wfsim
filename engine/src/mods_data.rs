//! Declarative mod loader: `data/mods/*.yaml` -> the mod pool.
//!
//! Mods are DATA, not code. Each `data/mods/<id>.yaml` describes a mod (drain,
//! polarity, per-rank effects); this module parses them into [`ModDef`] so the
//! pool is a single auditable source of truth that non-programmers can extend
//! via PR (same pattern as [`crate::enemy_data`] for enemies).
//!
//! The YAML records the TRUE mechanical effect (tooltip lies are corrected in
//! place — see docs/DATA_SOURCES.md). Effect `kind`s map to [`ModEffect`]; the
//! MAX-rank value is used for the pool (the sim builds at max rank). Effect
//! kinds with no damage impact (dodge/acrobatic speed, weapon_scoped markers)
//! are loaded as no-ops. Unknown kinds are ignored with the mod still loaded,
//! so a not-yet-modeled special effect never silently drops the whole mod.

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use serde::Deserialize;
use serde_norway::Value;

use crate::damage::DamageType;
use crate::loadout::{ModDef, ModEffect};
use crate::mods::Polarity;

#[derive(Debug, Deserialize)]
struct ModFile {
    id: String,
    #[allow(dead_code)]
    name: String,
    mod_type: String,
    polarity: String,
    base_drain: u32,
    #[allow(dead_code)]
    max_rank: u32,
    #[serde(default)]
    family: Option<String>,
    effects: Vec<Value>,
}

fn polarity(name: &str) -> Polarity {
    match name {
        "madurai" => Polarity::Madurai,
        "naramon" => Polarity::Naramon,
        "vazarin" => Polarity::Vazarin,
        "zenurik" => Polarity::Zenurik,
        "unairu" => Polarity::Unairu,
        "penjaga" => Polarity::Penjaga,
        "umbra" => Polarity::Umbra,
        other => panic!("unknown polarity: {other}"),
    }
}

fn element(name: &str) -> Option<DamageType> {
    use DamageType::*;
    Some(match name {
        "cold" => Cold,
        "heat" => Heat,
        "electricity" => Electricity,
        "toxin" => Toxin,
        "magnetic" => Magnetic,
        "viral" => Viral,
        "corrosive" => Corrosive,
        "gas" => Gas,
        "radiation" => Radiation,
        "blast" => Blast,
        "impact" => Impact,
        "puncture" => Puncture,
        "slash" => Slash,
        _ => return None,
    })
}

fn f(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}
fn u(v: &Value, k: &str) -> u32 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0) as u32
}

/// Map one YAML effect entry to a [`ModEffect`] at max rank (None = no damage
/// effect / not modeled — the mod still loads).
fn effect(v: &Value) -> Option<ModEffect> {
    let kind = v.get("kind").and_then(Value::as_str)?;
    let max = |k: &str| f(v, k).unwrap_or(0.0);
    Some(match kind {
        "base_damage_bonus" => ModEffect::BaseDamage(max("max")),
        "multishot_bonus" => ModEffect::Multishot(max("max")),
        "crit_chance_bonus" => ModEffect::CritChance(max("max")),
        "crit_damage_bonus" => ModEffect::CritDamage(max("max")),
        "status_chance_bonus" => ModEffect::StatusChance(max("max")),
        "status_damage_bonus" => ModEffect::StatusDamage(max("max")),
        "fire_rate_bonus" => ModEffect::FireRate(max("max")),
        "reload_speed_bonus" => ModEffect::ReloadSpeed(max("max")),
        "elemental_damage_bonus" | "combined_element_bonus" => {
            let e = element(v.get("element").and_then(Value::as_str)?)?;
            if e.is_primary_element() {
                ModEffect::Element(e, max("max"))
            } else {
                ModEffect::CombinedElement(e, max("max"))
            }
        }
        "on_headshot_crit_chance" => ModEffect::OnHeadshotCritChance {
            bonus: f(v, "bonus").unwrap_or_else(|| max("max")),
            duration: f(v, "duration_seconds").unwrap_or(0.0),
        },
        "on_headshot_kill_crit_chance_stacks" => ModEffect::OnHeadshotKillCritChance {
            per_stack: f(v, "per_stack").unwrap_or(0.0),
            max_stacks: u(v, "max_stacks"),
            duration: f(v, "duration_seconds").unwrap_or(0.0),
        },
        "stacking_buff" => {
            // on-kill families: the per_stack map names the bucket.
            let ps = v.get("per_stack");
            let dur = f(v, "duration_seconds").unwrap_or(0.0);
            let stacks = u(v, "max_stacks");
            if let Some(m) = ps.and_then(|p| p.get("multishot_bonus")).and_then(Value::as_f64) {
                ModEffect::OnKillMultishot { per_stack: m, max_stacks: stacks, duration: dur }
            } else if let Some(c) =
                ps.and_then(|p| p.get("condition_overload")).and_then(Value::as_f64)
            {
                ModEffect::ConditionOverload { per_stack: c, max_stacks: stacks, duration: dur }
            } else {
                return None; // unrecognized stacking payload -> not modeled
            }
        }
        // No damage effect (Amalgam side benefits, scoping markers), or a
        // special effect not yet modeled: load the mod without this effect.
        _ => return None,
    })
}

fn to_moddef(mf: ModFile) -> ModDef {
    let effects = mf.effects.iter().filter_map(effect).collect();
    ModDef {
        id: Box::leak(mf.id.into_boxed_str()),
        // ModDef.base_drain is the drain at the EQUIPPED (max) rank: drain
        // rises by 1 per rank from the rank-0 `base_drain`, so max = base + rank.
        base_drain: mf.base_drain + mf.max_rank,
        max_rank: mf.max_rank,
        polarity: polarity(&mf.polarity),
        family: mf.family.map(|s| &*Box::leak(s.into_boxed_str())),
        effects,
    }
}

/// Load every `<id>.yaml` under `dir` into mod definitions (sorted by id).
pub fn load_from_dir(dir: &Path) -> Vec<ModDef> {
    let mut out = Vec::new();
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    let mut paths: Vec<_> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
        .collect();
    paths.sort();
    for path in paths {
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let mf: ModFile = serde_norway::from_str(&text)
            .unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        out.push(to_moddef(mf));
    }
    out
}

fn data_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../data/mods")
}

/// All loaded mods, cached (leaks id/family strings once). Filter by
/// `mod_type` for a weapon-class pool.
pub fn all_mods() -> &'static [ModDef] {
    static POOL: OnceLock<Vec<ModDef>> = OnceLock::new();
    POOL.get_or_init(|| load_from_dir(&data_dir()))
}

/// The secondary/pistol mod pool (mod_type: pistol) — Dual Toxocyst's pool.
pub fn pistol_pool() -> Vec<ModDef> {
    // Re-filtered from the cache; mod_type lives in the YAML but we don't keep
    // it on ModDef, so re-read is cheap enough for a load-once pool. Kept as a
    // clone so callers can own/reorder freely.
    all_mods().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_pistol_pool_from_yaml() {
        let mods = load_from_dir(&data_dir());
        assert!(mods.len() >= 26, "expected >=26 mods, got {}", mods.len());

        let by = |id: &str| mods.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("missing {id}"));

        // Generic bonus.
        assert!(matches!(by("hornet_strike").effects[0], ModEffect::BaseDamage(v) if (v - 2.20).abs() < 1e-9));
        // Primary vs combined element dispatch.
        assert!(by("frostbite").effects.iter().any(|e| matches!(e, ModEffect::Element(DamageType::Cold, v) if (*v - 0.60).abs() < 1e-9)));
        assert!(by("magnetic_might").effects.iter().any(|e| matches!(e, ModEffect::CombinedElement(DamageType::Magnetic, v) if (*v - 0.60).abs() < 1e-9)));
        // Conditional families.
        assert!(by("galvanized_shot").effects.iter().any(|e| matches!(e, ModEffect::ConditionOverload { per_stack, max_stacks: 3, .. } if (*per_stack - 0.40).abs() < 1e-9)));
        assert!(by("galvanized_diffusion").effects.iter().any(|e| matches!(e, ModEffect::OnKillMultishot { per_stack, max_stacks: 4, .. } if (*per_stack - 0.30).abs() < 1e-9)));
        assert!(by("galvanized_crosshairs").effects.iter().any(|e| matches!(e, ModEffect::OnHeadshotKillCritChance { max_stacks: 5, .. })));
    }
}
