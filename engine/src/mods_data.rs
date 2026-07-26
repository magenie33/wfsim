//! Declarative mod loader: `data/mods/<class>/*.yaml` -> the mod pool.
//!
//! Mods are DATA, not code. Each `data/mods/<class>/<id>.yaml` describes a mod
//! (drain,
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
use crate::loadout::{CondBucket, Faction, IndirectStat, ModDef, ModEffect, Rarity};
use crate::mods::Polarity;

#[derive(Debug, Deserialize)]
struct ModFile {
    id: String,
    #[allow(dead_code)]
    name: String,
    // Kept for future per-class pool filtering (rifle vs pistol).
    #[allow(dead_code)]
    mod_type: String,
    polarity: String,
    rarity: String,
    base_drain: u32,
    #[allow(dead_code)]
    max_rank: u32,
    #[serde(default)]
    exilus: bool,
    #[serde(default)]
    family: Option<String>,
    /// Weapon trait required for the mod to apply (calc-layer gate).
    #[serde(default)]
    requires: Option<String>,
    /// Stats this mod locks from being modified.
    #[serde(default)]
    disables: Vec<String>,
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

fn rarity(name: &str) -> Rarity {
    match name {
        "common" => Rarity::Common,
        "uncommon" => Rarity::Uncommon,
        "rare" => Rarity::Rare,
        "legendary" => Rarity::Legendary,
        other => panic!("unknown rarity: {other}"),
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
        "base_damage_bonus" => ModEffect::BaseDamage(max("rankMax")),
        "multishot_bonus" => ModEffect::Multishot(max("rankMax")),
        "crit_chance_bonus" => ModEffect::CritChance(max("rankMax")),
        "crit_damage_bonus" => ModEffect::CritDamage(max("rankMax")),
        "status_chance_bonus" => ModEffect::StatusChance(max("rankMax")),
        "status_damage_bonus" => ModEffect::StatusDamage(max("rankMax")),
        "fire_rate_bonus" => ModEffect::FireRate(max("rankMax")),
        "reload_speed_bonus" => ModEffect::ReloadSpeed(max("rankMax")),
        "magazine_capacity_bonus" => ModEffect::MagazineCapacity(max("rankMax")),
        "status_duration_bonus" => ModEffect::StatusDuration(max("rankMax")),
        // Faction damage (Bane/Expel): +max total damage vs the named faction.
        // An unrecognized faction (Unknown) drops the effect (mod still loads).
        "faction_damage_bonus" => {
            let fac = Faction::from_name(v.get("faction").and_then(Value::as_str)?);
            if fac == Faction::Unknown {
                return None;
            }
            ModEffect::FactionDamage(fac, max("rankMax"))
        }
        "elemental_damage_bonus" | "combined_element_bonus" | "physical_damage_bonus" => {
            let e = element(v.get("element").and_then(Value::as_str)?)?;
            // Physical (IPS) types are a DIFFERENT mechanic from elements: they
            // scale the base of that type and never combine — route to Physical
            // regardless of the kind name.
            match e {
                DamageType::Impact | DamageType::Puncture | DamageType::Slash => {
                    ModEffect::Physical(e, max("rankMax"))
                }
                _ if e.is_primary_element() => ModEffect::Element(e, max("rankMax")),
                _ => ModEffect::CombinedElement(e, max("rankMax")),
            }
        }
        // Unified declarative TRIGGERED BUFF (BUFFS.md model): a held perk
        // grants a buff on `trigger` (+ optional `condition`), contributing
        // `grants` (a bucket) per stack; `rank0`/`rankMax` are the per-stack
        // value. Maps to the modeled buff variants at max rank; triggers not yet
        // modeled keep their (uniform) data but resolve to a no-op.
        "buff" => {
            let trigger = v.get("trigger").and_then(Value::as_str)?;
            let grants = v.get("grants").and_then(Value::as_str)?;
            let per = max("rankMax"); // per-stack value at max rank
            let stacks = u(v, "max_stacks");
            let dur = f(v, "duration").unwrap_or(0.0);
            match (trigger, grants) {
                ("on_kill", "multishot") => {
                    ModEffect::OnKillMultishot { per_stack: per, max_stacks: stacks, duration: dur }
                }
                ("on_kill", "condition_overload") => {
                    ModEffect::ConditionOverload { per_stack: per, max_stacks: stacks, duration: dur }
                }
                ("on_headshot", "crit_chance") => {
                    ModEffect::OnHeadshotCritChance { bonus: per, duration: dur }
                }
                ("on_headshot_kill", "crit_chance") => {
                    ModEffect::OnHeadshotKillCritChance { per_stack: per, max_stacks: stacks, duration: dur }
                }
                // Any other trigger (on_ability_cast / on_reload / on_hit / …):
                // contribute at the assumed-max total via CondBuff when the grant
                // maps to a DPS bucket. Indirect grants (accuracy/recoil) → None.
                _ => {
                    let bucket = match grants {
                        "base_damage" | "damage" => CondBucket::BaseDamage,
                        "multishot" => CondBucket::Multishot,
                        "crit_chance" => CondBucket::CritChance,
                        "crit_damage" => CondBucket::CritDamage,
                        "status_chance" => CondBucket::StatusChance,
                        "status_damage" => CondBucket::StatusDamage,
                        "fire_rate" => CondBucket::FireRate,
                        _ => return None,
                    };
                    ModEffect::CondBuff(bucket, per * stacks.max(1) as f64)
                }
            }
        }
        // INDIRECT stats: outside the theoretical-DPS formula, but real
        // panel buckets a future shooter model consumes (aim, travel,
        // ammo sustain) — the panel states every bonus.
        "recoil_reduction" => ModEffect::Indirect(IndirectStat::Recoil, max("rankMax")),
        "noise_reduction" => ModEffect::Indirect(IndirectStat::Noise, max("rankMax")),
        "ammo_max_bonus" => ModEffect::Indirect(IndirectStat::AmmoMax, max("rankMax")),
        "projectile_speed_bonus" => ModEffect::Indirect(IndirectStat::ProjectileSpeed, max("rankMax")),
        "holstered_reload" => ModEffect::Indirect(IndirectStat::HolsteredReload, max("rankMax")),
        "dodge_speed_bonus" => ModEffect::Indirect(IndirectStat::DodgeSpeed, max("rankMax")),
        "acrobatic_speed_bonus" => ModEffect::Indirect(IndirectStat::AcrobaticSpeed, max("rankMax")),
        "punch_through_bonus" => ModEffect::Indirect(IndirectStat::PunchThrough, max("rankMax")),
        "zoom_bonus" => ModEffect::Indirect(IndirectStat::Zoom, max("rankMax")),
        "accuracy_bonus" => ModEffect::Indirect(IndirectStat::Accuracy, max("rankMax")),
        // Reflex Draw: on swap-in, −recoil/+accuracy for a few seconds.
        "on_equip_buff" => ModEffect::OnEquipHandling {
            recoil: -max("rankMax").abs(),
            accuracy: max("rankMax").abs(),
            duration: f(v, "duration_seconds").unwrap_or(0.0),
        },
        // Scoping markers (weapon_scoped) or an effect not yet modeled:
        // load the mod without this effect.
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
        rarity: rarity(&mf.rarity),
        exilus: mf.exilus,
        family: mf.family.map(|s| &*Box::leak(s.into_boxed_str())),
        requires: mf.requires.map(|s| &*Box::leak(s.into_boxed_str())),
        disables: mf
            .disables
            .into_iter()
            .map(|s| &*Box::leak(s.into_boxed_str()))
            .collect(),
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

/// Root of the per-class mod tree: `data/mods/<class>/*.yaml`. Each weapon
/// class (pistol, rifle, …) gets its own subfolder so the flat pool doesn't
/// get muddled as the mod count grows.
fn mods_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../data/mods")
}

/// The secondary/pistol mod pool — `data/mods/pistol/*.yaml` (Dual Toxocyst's
/// pool). Cached (leaks id/family strings once); cloned so callers own it.
pub fn pistol_pool() -> Vec<ModDef> {
    static POOL: OnceLock<Vec<ModDef>> = OnceLock::new();
    POOL.get_or_init(|| load_from_dir(&mods_root().join("pistol"))).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_pistol_pool_from_yaml() {
        let mods = load_from_dir(&mods_root().join("pistol"));
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
        // Faction-damage mod loads with the right faction + bonus (Expel Orokin
        // → Corrupted; +30% at max rank).
        assert!(by("expel_grineer").effects.iter().any(|e| matches!(e, ModEffect::FactionDamage(Faction::Grineer, v) if (*v - 0.30).abs() < 1e-9)));
        assert!(by("expel_orokin").effects.iter().any(|e| matches!(e, ModEffect::FactionDamage(Faction::Corrupted, _))));
    }
}
