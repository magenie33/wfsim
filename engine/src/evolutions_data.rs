//! Declarative Incarnon-evolution loader: `data/evolutions/*.yaml` -> the
//! evolution pool.
//!
//! Evolutions are DATA, not code (same pattern as [`crate::mods_data`] /
//! [`crate::arcanes_data`]): each yaml records the wiki-verified effects;
//! this module parses them into [`EvolutionDef`] and APPLIES a chosen set
//! onto a weapon's raw [`WeaponBase`] — evolutions alter BASE stats before
//! mods (flat base damage scales the vector pro-rata inside ModifiedBase;
//! Commodore's Fortune adds into the BASE crit chance that crit mods then
//! multiply). The engine previously hardcoded these numbers in the
//! `DtEvo2` enum; the enum remains as a selector, the values live here.

use std::sync::OnceLock;

use serde::Deserialize;
use serde_norway::Value;

use crate::loadout::WeaponBase;

#[derive(Debug, Deserialize)]
struct EvoFile {
    id: String,
    name: String,
    #[allow(dead_code)]
    kind: String,
    weapon: String,
    tier: u32,
    /// Wiki `File:` name for the evolution's icon.
    #[serde(default)]
    icon: Option<String>,
    /// Verbatim in-game/wiki effect text (evolutions have no ranks, so no
    /// X templating).
    #[serde(default)]
    description: Option<String>,
    /// Wiki-flagged non-functional evolutions apply NOTHING.
    #[serde(default)]
    currently_broken: bool,
    effects: Vec<Value>,
}

/// One parsed evolution effect (the loader's vocabulary — kinds with no
/// single-target damage payload load as `Inert` so the evolution still
/// resolves and lists).
#[derive(Debug, Clone, PartialEq)]
enum EvoEffect {
    /// Adds to the BASE damage TOTAL, distributed pro-rata across the
    /// vector, BEFORE mods (inside ModifiedBase).
    FlatBaseDamage(f64),
    /// Adds into the BASE crit chance (crit mods multiply the new base).
    FlatBaseCritChance(f64),
    /// A PERMANENT stacking multishot buff (Fevered Frenzy: on-ability-cast
    /// stacks with no timer, cleared only by death — so inside a sim run the
    /// stack count is a static CHOICE, full by default). `total` = the
    /// full-stack bonus (per_stack × max_stacks) that joins the weapon's
    /// buff multishot; `max_stacks` lets the per-buff config rescale it.
    AssumedMaxMultishot { total: f64, max_stacks: u32 },
    /// Unconditional CO rate (Carnage Reign): +v per status TYPE, additive
    /// with mod CO sources. `excludes_evolution_damage`: the GunCO base
    /// excludes evolution flat damage (wiki CO catalog, DT row).
    ConditionOverload { per_type: f64 },
    /// No damage payload here (holstered regen, recoil, timed utility
    /// buffs, the weapon unlock) — kept so the evolution loads and lists.
    Inert(String),
}

/// A parsed Incarnon evolution.
#[derive(Debug, Clone)]
pub struct EvolutionDef {
    pub id: String,
    pub name: String,
    pub weapon: String,
    pub tier: u32,
    /// Wiki `File:` name for the evolution's icon.
    pub icon: Option<String>,
    /// Verbatim effect text — what the cards display (like mods/arcanes).
    pub description: String,
    pub currently_broken: bool,
    effects: Vec<EvoEffect>,
}

impl EvolutionDef {
    /// Σ flat base damage this evolution adds (0 when broken) — the panel
    /// attributes it as a non-mod source on the Base Damage row.
    pub fn flat_base_damage(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::FlatBaseDamage(v) => Some(*v),
                _ => None,
            })
            .sum()
    }

    /// Σ flat BASE crit chance (Commodore's Fortune; 0 when broken).
    pub fn flat_base_crit_chance(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::FlatBaseCritChance(v) => Some(*v),
                _ => None,
            })
            .sum()
    }

    /// Σ assumed-max multishot from permanent stacks (Fevered Frenzy).
    pub fn assumed_multishot(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::AssumedMaxMultishot { total, .. } => Some(*total),
                _ => None,
            })
            .sum()
    }

    /// The permanent stacked-multishot buff, if this evolution grants one:
    /// (full-stack bonus, max stacks). Drives the configurable buff card.
    pub fn ms_buff(&self) -> Option<(f64, u32)> {
        self.active_effects().find_map(|e| match e {
            EvoEffect::AssumedMaxMultishot { total, max_stacks } => Some((*total, *max_stacks)),
            _ => None,
        })
    }

    /// Σ unconditional CO rate per status type (Carnage Reign).
    pub fn co_per_type(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::ConditionOverload { per_type } => Some(*per_type),
                _ => None,
            })
            .sum()
    }

    fn active_effects(&self) -> impl Iterator<Item = &EvoEffect> {
        // Broken evolutions contribute nothing (same rule as `apply`).
        self.effects
            .iter()
            .filter(move |_| !self.currently_broken)
    }

    /// One display line per effect — what the model computes (broken
    /// evolutions state the zero honestly at the call site, not here).
    pub fn describe(&self) -> Vec<String> {
        self.effects
            .iter()
            .map(|e| match e {
                EvoEffect::FlatBaseDamage(v) => {
                    format!("+{v:.0} base damage (pro-rata, before mods)")
                }
                EvoEffect::FlatBaseCritChance(v) => {
                    format!("+{:.0}% BASE crit chance (crit mods multiply it)", v * 100.0)
                }
                EvoEffect::AssumedMaxMultishot { total, max_stacks } => format!(
                    "+{:.0}% multishot ({max_stacks} on-ability-cast stacks, full by default)",
                    total * 100.0
                ),
                EvoEffect::ConditionOverload { per_type } => format!(
                    "+{:.0}% direct damage per status type on the target",
                    per_type * 100.0
                ),
                EvoEffect::Inert(what) => {
                    format!("{} (no single-target DPS effect)", what.replace('_', " "))
                }
            })
            .collect()
    }
}

fn f(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}

fn effect(v: &Value) -> Option<EvoEffect> {
    let kind = v.get("kind").and_then(Value::as_str)?;
    Some(match kind {
        "flat_base_damage" => EvoEffect::FlatBaseDamage(f(v, "value").unwrap_or(0.0)),
        "flat_base_crit_chance" => EvoEffect::FlatBaseCritChance(f(v, "value").unwrap_or(0.0)),
        "stacking_buff" => {
            // Only the multishot payload is modeled (Fevered Frenzy);
            // other stacking payloads load inert until needed.
            let per = v
                .get("per_stack")
                .and_then(|p| p.get("multishot_bonus"))
                .and_then(Value::as_f64);
            let max = v.get("max_stacks").and_then(Value::as_u64).unwrap_or(0);
            match per {
                Some(p) => EvoEffect::AssumedMaxMultishot {
                    total: p * max as f64,
                    max_stacks: max as u32,
                },
                None => EvoEffect::Inert("stacking_buff (unmodeled payload)".into()),
            }
        }
        "condition_overload" => EvoEffect::ConditionOverload {
            per_type: f(v, "value").unwrap_or(0.0),
        },
        other => EvoEffect::Inert(other.to_string()),
    })
}

/// Apply a chosen evolution set onto a weapon's RAW base panel.
///
/// Order-independent: flat base damage sums first, then the vector scales
/// pro-rata ONCE; `co_base_fraction` = original / evolved total — the wiki
/// CO-catalog rule that every GunCO source computes on the pre-evolution
/// base ("CO-bonus does not use base damage increase Evolution").
/// `currently_broken` evolutions apply nothing.
pub fn apply(base: &mut WeaponBase, evos: &[&EvolutionDef]) {
    let original_total = base.base_vector.total();
    let mut flat = 0.0;
    for e in evos.iter().filter(|e| !e.currently_broken) {
        for eff in &e.effects {
            match eff {
                EvoEffect::FlatBaseDamage(v) => flat += v,
                EvoEffect::FlatBaseCritChance(v) => base.base_crit_chance += v,
                EvoEffect::AssumedMaxMultishot { total, max_stacks } => {
                    base.buff_multishot_bonus += total;
                    base.buff_ms_max_stacks = base.buff_ms_max_stacks.max(*max_stacks);
                }
                EvoEffect::ConditionOverload { per_type } => {
                    base.innate_co_per_type += per_type;
                }
                EvoEffect::Inert(_) => {}
            }
        }
    }
    if flat > 0.0 && original_total > 0.0 {
        let evolved = original_total + flat;
        base.base_vector = base.base_vector.scale(evolved / original_total);
        base.co_base_fraction = original_total / evolved;
    }
}

/// Every embedded yaml under data/evolutions (cached).
pub fn pool() -> &'static Vec<EvolutionDef> {
    static POOL: OnceLock<Vec<EvolutionDef>> = OnceLock::new();
    POOL.get_or_init(|| {
        let mut out = Vec::new();
        for (_, text) in crate::data::files_under("evolutions/") {
            // The kind tag guards against a stray non-evolution yaml.
            let Ok(ef) = serde_norway::from_str::<EvoFile>(text) else { continue };
            if ef.kind != "incarnon_evolution" {
                continue;
            }
            let effects = ef.effects.iter().filter_map(effect).collect();
            out.push(EvolutionDef {
                id: ef.id,
                name: ef.name,
                weapon: ef.weapon,
                tier: ef.tier,
                icon: ef.icon,
                description: ef.description.unwrap_or_default(),
                currently_broken: ef.currently_broken,
                effects,
            });
        }
        out
    })
}

/// Look up an evolution by id.
pub fn get(id: &str) -> Option<&'static EvolutionDef> {
    pool().iter().find(|e| e.id == id)
}

/// A weapon's choosable options at a tier (the web picker's rows).
pub fn options(weapon: &str, tier: u32) -> Vec<&'static EvolutionDef> {
    pool()
        .iter()
        .filter(|e| e.weapon == weapon && e.tier == tier)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_dt_evolution_pool() {
        let dt: Vec<_> = pool().iter().filter(|e| e.weapon == "dual_toxocyst").collect();
        assert!(dt.len() >= 9, "expected the 9 DT evolutions, got {}", dt.len());
        assert_eq!(options("dual_toxocyst", 2).len(), 2); // the EVO II choice
        // Broken evolutions carry the wiki flag.
        assert!(get("dt_ready_retaliation").unwrap().currently_broken);
        assert!(get("dt_neurotoxin").unwrap().currently_broken);
    }

    #[test]
    fn fevered_and_carnage_parse_their_wiki_values() {
        let fe = get("dt_fevered_frenzy").unwrap();
        assert!(fe.effects.contains(&EvoEffect::FlatBaseDamage(50.0)));
        assert!(fe
            .effects
            .contains(&EvoEffect::AssumedMaxMultishot { total: 1.0, max_stacks: 20 }));
        let ca = get("dt_carnage_reign").unwrap();
        assert!(ca.effects.contains(&EvoEffect::FlatBaseDamage(60.0)));
        assert!(ca.effects.contains(&EvoEffect::ConditionOverload { per_type: 0.33 }));
        let cf = get("dt_commodores_fortune").unwrap();
        assert!(cf.effects.contains(&EvoEffect::FlatBaseCritChance(0.20)));
    }

    #[test]
    fn broken_evolutions_apply_nothing() {
        use crate::loadout::WeaponBase;
        let with = WeaponBase::from_data("dual_toxocyst", false, &["dt_commodores_fortune", "dt_evolved_autoloader", "dt_fevered_frenzy"]);
        let mut probe = with.clone();
        apply(&mut probe, &[get("dt_ready_retaliation").unwrap()]);
        assert!((probe.base_vector.total() - with.base_vector.total()).abs() < 1e-9);
        assert_eq!(probe.base_crit_chance, with.base_crit_chance);
    }
}
