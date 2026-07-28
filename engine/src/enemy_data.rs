//! Enemy data loader: `data/enemies/**.yaml` → engine types.
//!
//! This is the first slice of the data layer (devlog 2026-07-24 plan). Custom
//! enemies are just additional YAML files (mark them `synthetic: true`), which
//! makes arbitrary saved target types cheap — see `data/enemies/custom/`.
//!
//! Rigor rule: **impossible combinations are rejected at construction**, not
//! silently accepted. Examples: an Eximus of a unit that has no Eximus variant
//! in-game (Thrax units — absent from the wiki `Eximus/Compatibilities`
//! table), or a shielded enemy (shield pools are not implemented yet).

use serde::Deserialize;
use std::fs;
use std::path::Path;

use crate::dummy::{Attenuation, BodyPart, StackCaps, TargetMode, TargetParams};
use crate::scaling;

/// Which faction's scaling curves an enemy uses (wiki `Enemy_Level_Scaling`).
/// This is about *stat scaling*, not faction damage bonuses — Thrax units are
/// faction "Unknown" with Zariman damage modifiers but scale as Unaffiliated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScalingFaction {
    Grineer,
    Corpus,
    Infested,
    Corrupted,
    /// Also Murmur and Sentient (same curves), and Zariman/Thrax units.
    Unaffiliated,
    Techrot,
}

impl ScalingFaction {
    pub fn health_curve(self) -> scaling::Curve {
        match self {
            Self::Grineer => scaling::health::GRINEER,
            Self::Corpus => scaling::health::CORPUS,
            Self::Infested => scaling::health::INFESTED,
            Self::Corrupted => scaling::health::CORRUPTED,
            Self::Unaffiliated => scaling::health::UNAFFILIATED,
            Self::Techrot => scaling::health::TECHROT,
        }
    }

    /// Shield curves (wiki table: Grineer + Sentient share one row;
    /// shielded Infested/Unaffiliated units use the Grineer/Sentient
    /// curve as the closest documented family).
    pub fn shield_curve(self) -> scaling::Curve {
        match self {
            Self::Corpus => scaling::shield::CORPUS,
            Self::Corrupted => scaling::shield::CORRUPTED,
            Self::Techrot => scaling::shield::TECHROT,
            Self::Grineer | Self::Infested | Self::Unaffiliated => scaling::shield::GRINEER,
        }
    }
}

/// Boss-type damage attenuation parameters from the data file. The wiki
/// documents the STRUCTURE (per-instance and per-second caps proportional
/// to Max Health, per player) but not the constants - data files record
/// current-belief estimates.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct AttenuationSpec {
    pub max_instance_fraction_of_health: f64,
    pub max_dps_fraction_of_health: f64,
}

/// Per-unit status stack caps (Acolytes: any status 4, Impact 3).
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct StackCapsSpec {
    pub general: usize,
    pub impact: usize,
}

/// Base stats at `base_level` (mirrors the wiki enemy data module fields).
#[derive(Debug, Clone, Deserialize)]
pub struct StatsSpec {
    pub base_level: u32,
    pub health: f64,
    #[serde(default)]
    pub shield: f64,
    #[serde(default)]
    pub armor: f64,
    #[serde(default)]
    pub overguard: f64,
}

/// One body part as stored in enemy data (aim weights are *scenario* state,
/// not enemy state — supply them when building a run).
#[derive(Debug, Clone, Deserialize)]
pub struct BodyPartSpec {
    pub name: String,
    pub multiplier: f64,
    #[serde(default)]
    pub is_head: bool,
    #[serde(default)]
    pub crit_bonus: bool,
}

/// An enemy entry from `data/enemies/`. Unknown YAML fields (source,
/// mechanics, notes, ...) are ignored by the loader.
#[derive(Debug, Clone, Deserialize)]
pub struct EnemySpec {
    pub id: String,
    pub name: String,
    /// True for hand-made test targets that do not exist in-game.
    #[serde(default)]
    pub synthetic: bool,
    pub scaling_faction: ScalingFaction,
    /// Combat faction for faction-damage mods (Bane/Expel). Optional and
    /// SEPARATE from `scaling_faction`: e.g. Zariman Thrax scale as
    /// Unaffiliated but are combat-faction "Unknown" (no faction mod applies).
    /// Absent → `Faction::Unknown`. Values: grineer/corpus/infested/corrupted
    /// (aka orokin)/murmur/sentient (wiki `Faction_Damage_Bonus`).
    #[serde(default)]
    pub combat_faction: Option<String>,
    /// Whether an Eximus variant of this unit exists in-game (wiki
    /// `Eximus/Compatibilities`). Defaults to false: unknown units must not
    /// silently allow impossible combinations.
    #[serde(default)]
    pub can_be_eximus: bool,
    /// Whether this unit is in the Parazon Mercy heavy-unit list (wiki
    /// `Parazon` §Mercy). Defaults to false.
    #[serde(default)]
    pub mercy_eligible: bool,
    pub stats: StatsSpec,
    /// Damage attenuation (boss types); absent = none.
    #[serde(default)]
    pub attenuation: Option<AttenuationSpec>,
    /// Per-unit status stack caps; absent = normal caps.
    #[serde(default)]
    pub status_stack_caps: Option<StackCapsSpec>,
    pub body_parts: Vec<BodyPartSpec>,
}

impl EnemySpec {
    /// Parse from YAML and reject unsupported/impossible data.
    pub fn from_yaml_str(yaml: &str) -> Result<Self, String> {
        let spec: EnemySpec = serde_norway::from_str(yaml).map_err(|e| e.to_string())?;
        if spec.body_parts.is_empty() {
            return Err(format!(
                "{}: an enemy needs at least one body part",
                spec.id
            ));
        }
        Ok(spec)
    }

    /// Load one enemy YAML file from disk (native tooling only — the CLI and
    /// optimizer point at `data/enemies/` paths; the embedded set in [`all`]
    /// is the source everything else uses).
    pub fn load(path: &Path) -> Result<Self, String> {
        let yaml = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_yaml_str(&yaml)
    }

    /// Build the simulation target. Fails on combinations that do not exist
    /// in-game (e.g. `eximus` for a unit with no Eximus variant).
    pub fn target_params(
        &self,
        level: u32,
        steel_path: bool,
        eximus: bool,
        mode: TargetMode,
    ) -> Result<TargetParams, String> {
        if eximus && !self.can_be_eximus {
            return Err(format!(
                "{} cannot be an Eximus: no such unit exists in-game \
                 (wiki Eximus/Compatibilities)",
                self.name
            ));
        }
        Ok(TargetParams {
            name: self.name.clone(),
            base_level: self.stats.base_level,
            level,
            base_health: self.stats.health,
            base_armor: self.stats.armor,
            base_overguard: self.stats.overguard,
            base_shield: self.stats.shield,
            health_curve: self.scaling_faction.health_curve(),
            shield_curve: self.scaling_faction.shield_curve(),
            attenuation: self.attenuation.map(|a| Attenuation {
                instance_frac: a.max_instance_fraction_of_health,
                dps_frac: a.max_dps_fraction_of_health,
            }),
            stack_caps: self.status_stack_caps.map(|c| StackCaps {
                general: c.general,
                impact: c.impact,
            }),
            steel_path,
            eximus,
            can_be_eximus: self.can_be_eximus,
            status_immunities: Vec::new(),
            faction: self
                .combat_faction
                .as_deref()
                .map(crate::loadout::Faction::from_name)
                .unwrap_or(crate::loadout::Faction::Unknown),
            mode,
        })
    }

    /// Body parts with explicit aim weights, matched by part name. Every
    /// weight must name an existing part (typos are errors, not 0% aim).
    pub fn aim_parts(&self, weights: &[(&str, f64)]) -> Result<Vec<BodyPart>, String> {
        weights
            .iter()
            .map(|(name, w)| {
                let p = self
                    .body_parts
                    .iter()
                    .find(|p| p.name == *name)
                    .ok_or_else(|| format!("{} has no body part named '{name}'", self.name))?;
                Ok(BodyPart {
                    name: p.name.clone(),
                    aim_weight: *w,
                    multiplier: p.multiplier,
                    is_head: p.is_head,
                    crit_bonus: p.crit_bonus,
                })
            })
            .collect()
    }
}

/// The full embedded enemy library — every `data/enemies/**.yaml` including
/// `custom/` — the "saved target types" library. Panics on malformed data:
/// the set is fixed at compile time and covered by tests.
pub fn all() -> Vec<EnemySpec> {
    crate::data::files_under("enemies/")
        .map(|(path, text)| {
            EnemySpec::from_yaml_str(text).unwrap_or_else(|e| panic!("parse {path}: {e}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn data_enemies() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/enemies")
    }

    #[test]
    fn loads_thrax_centurion_from_data() {
        let spec = EnemySpec::load(&data_enemies().join("thrax_centurion.yaml")).unwrap();
        assert_eq!(spec.id, "thrax_centurion");
        assert_eq!(spec.stats.health, 3600.0);
        assert_eq!(spec.stats.armor, 200.0);
        assert_eq!(spec.stats.overguard, 15.0);
        assert!(!spec.synthetic);
        assert!(!spec.can_be_eximus);
        assert!(!spec.mercy_eligible);
        assert_eq!(spec.scaling_faction, ScalingFaction::Unaffiliated);
        let head = spec.body_parts.iter().find(|p| p.is_head).unwrap();
        assert_eq!(head.multiplier, 3.0);
    }

    #[test]
    fn thrax_eximus_is_rejected_as_nonexistent() {
        let spec = EnemySpec::load(&data_enemies().join("thrax_centurion.yaml")).unwrap();
        let err = spec
            .target_params(100, false, true, TargetMode::InstantRespawn)
            .unwrap_err();
        assert!(err.contains("cannot be an Eximus"), "err: {err}");
    }

    #[test]
    fn loads_the_enemy_library() {
        // The shipped library is Thrax-only for now; `synthetic: true`
        // (custom enemies) stays a supported flag, covered inline below.
        let specs = all();
        assert!(specs.iter().any(|s| s.id == "thrax_centurion"));
        let yaml = r#"
id: test_dummy
name: Test Dummy
synthetic: true
scaling_faction: unaffiliated
stats: { base_level: 1, health: 1000 }
body_parts: [ { name: body, multiplier: 1.0 } ]
"#;
        let spec = EnemySpec::from_yaml_str(yaml).unwrap();
        assert!(spec.synthetic);
    }

    #[test]
    fn unknown_aim_part_is_an_error() {
        let spec = EnemySpec::load(&data_enemies().join("thrax_centurion.yaml")).unwrap();
        assert!(spec.aim_parts(&[("hed", 1.0)]).is_err());
        let parts = spec.aim_parts(&[("body", 0.5), ("head", 0.5)]).unwrap();
        assert_eq!(parts.len(), 2);
    }

    #[test]
    fn shielded_enemies_load_with_attenuation_and_stack_caps() {
        let yaml = r#"
id: shielded
name: Shielded
scaling_faction: corpus
stats: { base_level: 1, health: 100, shield: 50 }
attenuation:
  max_instance_fraction_of_health: 0.05
  max_dps_fraction_of_health: 0.5
status_stack_caps: { general: 4, impact: 3 }
body_parts: [ { name: body, multiplier: 1.0 } ]
"#;
        let spec = EnemySpec::from_yaml_str(yaml).unwrap();
        let t = spec
            .target_params(1, false, false, TargetMode::InstantRespawn)
            .unwrap();
        assert_eq!(t.base_shield, 50.0);
        assert!((t.max_shield() - 50.0).abs() < 1e-9);
        let a = t.attenuation.unwrap();
        assert!((a.instance_frac - 0.05).abs() < 1e-12);
        let c = t.stack_caps.unwrap();
        assert_eq!((c.general, c.impact), (4, 3));
    }
}
