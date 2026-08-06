//! Enemy data loader: `data/enemies/**.yaml` → engine types.
//!
//! This is the first slice of the data layer (devlog 2026-07-24 plan). Custom
//! enemies are just additional YAML files (mark them `synthetic: true`), which
//! makes arbitrary saved target types cheap — see `data/enemies/custom/`.
//!
//! Rigor rule: **impossible combinations are rejected at construction**, not
//! silently accepted — an Eximus of a unit that has no Eximus variant in-game
//! (Thrax units, the Acolytes: absent from the wiki `Eximus/Compatibilities`
//! table) is an error, never a silently-granted overguard pool.

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
    /// Base affinity this unit is worth. Every enemy file has carried one
    /// since they were written and nothing read it — serde dropped the key, so
    /// it was prose in a data field, which data/README.md forbids. A syndicate
    /// radial arms on affinity, so it is consumed now.
    #[serde(default)]
    pub affinity: f64,
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
    /// Portrait file name, served from our own origin at `/img/<name>`.
    /// Wiki-hosted (WFCD's export carries no enemy art), so it is declared
    /// here rather than in `data/assets.yaml` — see the yaml's comment.
    /// Absent → the UI draws no picture, never a broken one.
    #[serde(default)]
    pub image: Option<String>,
    pub scaling_faction: ScalingFaction,
    /// Combat faction for faction-damage mods (Bane/Expel). Optional and
    /// SEPARATE from `scaling_faction`: e.g. Zariman Thrax scale as
    /// Unaffiliated but are combat-faction "Unknown" (no faction mod applies).
    /// Absent → `Faction::Unknown`. Values: grineer/corpus/infested/corrupted
    /// (aka orokin)/murmur/sentient (wiki `Faction_Damage_Bonus`).
    ///
    /// The yaml key is `faction:`, mirroring the wiki module's own `Faction`
    /// field. It was only ever `combat_faction:` here, so every existing file's
    /// `faction:` line was silently discarded — harmless for a unit that
    /// resolves to Unknown either way, and a wrong answer the moment a
    /// Corrupted unit needs Bane of Orokin to land.
    #[serde(default, alias = "faction")]
    pub combat_faction: Option<String>,
    /// Redirects ONLY the damage-type vulnerability column (System B), never
    /// the faction a Bane mod matches.
    ///
    /// The wiki enemy module's own optional field, not ours, and its schema
    /// (`Module:Enemies/data/doc`) states the restriction outright: "Override
    /// for enemies with different **faction resistance value** instead of that
    /// usually matches their faction." 34 module entries carry one — Zariman
    /// ×12, Grineer ×5, The Murmur ×2, Corpus ×1. Thrax Centurion is the one
    /// we ship: `Faction = "Unknown"` (no faction mod ever applies) with
    /// `FactionDamageOverride = "Zariman"` (Void ×1.5).
    ///
    /// It was in our yaml and NOT in this struct until 2026-08-03, so serde
    /// discarded it. Nothing read the column at all back then, so no fight was
    /// scored wrong by it — but the field could not have worked the moment one
    /// did, which is what this is.
    #[serde(default)]
    pub faction_damage_override: Option<String>,
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
    /// What a run against this unit does NOT account for — short phrases, in
    /// English like every other source string, shown on the target card.
    /// A known gap the reader cannot see is indistinguishable from a wrong
    /// number, and the promise this product makes is the opposite of that.
    #[serde(default)]
    pub unmodeled: Vec<String>,
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

    /// Which column of `data/factions/damage_modifiers.yaml` this unit's
    /// damage-type vulnerabilities come from: the OVERRIDE if it has one,
    /// otherwise its faction. A key the table does not name — "stalker",
    /// "unknown", most of the wildlife — has no column, and that is the
    /// answer: it takes every damage type as written.
    pub fn damage_column_key(&self) -> &str {
        self.faction_damage_override
            .as_deref()
            .or(self.combat_faction.as_deref())
            .unwrap_or("unknown")
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
        // The damage table's fifteen columns are the whole system; a faction it
        // does not name takes every type as written, so this cannot fail.
        let type_mods = crate::factions_data::columns_for(self.damage_column_key());
        Ok(TargetParams {
            name: self.name.clone(),
            base_level: self.stats.base_level,
            level,
            base_health: self.stats.health,
            base_armor: self.stats.armor,
            base_overguard: self.stats.overguard,
            base_affinity: self.stats.affinity,
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
            type_mods,
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

    /// The two faction systems have DIFFERENT keys, and the Thrax is the unit
    /// that proves it: no faction mod ever matches it, and it still takes Void
    /// ×1.5. `faction_damage_override:` was in the yaml but not in this struct,
    /// so the whole column was being dropped on the floor.
    #[test]
    fn the_override_redirects_the_column_without_touching_the_faction() {
        let spec = EnemySpec::load(&data_enemies().join("thrax_centurion.yaml")).unwrap();
        assert_eq!(spec.damage_column_key(), "zariman");
        let t = spec
            .target_params(100, false, false, TargetMode::InstantRespawn)
            .unwrap();
        assert_eq!(t.faction, crate::loadout::Faction::Unknown, "no Bane applies");
        assert_eq!(t.type_mods.faction.get(crate::damage::DamageType::Void), 1.5);
        // …and nothing else: the override selects ONE column, it does not add
        // the Zariman column on top of a faction one.
        assert_eq!(t.type_mods.faction.listed().len(), 1);

        // The plain case: no override, so the column follows the faction.
        let g = EnemySpec::load(&data_enemies().join("corrupted_heavy_gunner.yaml")).unwrap();
        assert_eq!(g.damage_column_key(), "orokin");
        let gt = g
            .target_params(100, false, false, TargetMode::InstantRespawn)
            .unwrap();
        assert_eq!(
            gt.type_mods.faction.get(crate::damage::DamageType::Puncture),
            1.5
        );
        assert_eq!(
            gt.type_mods.faction.get(crate::damage::DamageType::Radiation),
            0.5
        );
    }

    #[test]
    fn thrax_eximus_is_rejected_as_nonexistent() {
        let spec = EnemySpec::load(&data_enemies().join("thrax_centurion.yaml")).unwrap();
        let err = spec
            .target_params(100, false, true, TargetMode::InstantRespawn)
            .unwrap_err();
        assert!(err.contains("cannot be an Eximus"), "err: {err}");
    }

    /// The six Acolytes are ONE unit six times over as far as a build is
    /// concerned: same pools, same caps, same 1x head. The files are separate
    /// (each target stands on its own), so the invariant is asserted here
    /// rather than trusted to six copy-edits.
    #[test]
    fn the_six_acolytes_share_one_defensive_statline() {
        let acolytes: Vec<EnemySpec> = all()
            .into_iter()
            .filter(|s| {
                ["angst", "malice", "mania", "misery", "torment", "violence"].contains(&&*s.id)
            })
            .collect();
        assert_eq!(acolytes.len(), 6, "all six Acolytes must be in the library");
        for a in &acolytes {
            assert_eq!(a.stats.health, 2500.0, "{}", a.id);
            assert_eq!(a.stats.shield, 1500.0, "{}", a.id);
            assert_eq!(a.stats.armor, 50.0, "{}", a.id);
            assert_eq!(a.stats.overguard, 0.0, "{}", a.id);
            assert_eq!(a.stats.base_level, 1, "{}", a.id);
            // DE U27.3, extended to the Acolytes in U29.5.4.
            let caps = a.status_stack_caps.unwrap_or_else(|| panic!("{}", a.id));
            assert_eq!((caps.general, caps.impact), (4, 6), "{}", a.id);
            // Multis "Head: 1.0x" — no headshot damage, so no crit headshot.
            let head = a.body_parts.iter().find(|p| p.is_head).unwrap();
            assert_eq!(head.multiplier, 1.0, "{}", a.id);
            assert!(!head.crit_bonus, "{}", a.id);
            assert!(!a.can_be_eximus && !a.mercy_eligible, "{}", a.id);
            // The attenuation constants are unpublished, so nothing is set —
            // and the gap is stated on the card instead of left implicit.
            assert!(a.attenuation.is_none(), "{}", a.id);
            assert!(!a.unmodeled.is_empty(), "{}", a.id);
        }
    }

    /// The benchmark target — and the one enemy so far whose faction has to
    /// REACH the engine: `faction: orokin` is what makes Bane of Orokin land.
    #[test]
    fn corrupted_heavy_gunner_carries_its_faction_into_the_fight() {
        let spec = EnemySpec::load(&data_enemies().join("corrupted_heavy_gunner.yaml")).unwrap();
        assert_eq!(spec.stats.health, 700.0);
        assert_eq!(spec.stats.armor, 500.0);
        assert_eq!(spec.stats.base_level, 8);
        assert_eq!(spec.scaling_faction, ScalingFaction::Corrupted);
        assert!(spec.can_be_eximus && spec.mercy_eligible);
        let t = spec
            .target_params(100, false, true, TargetMode::InstantRespawn)
            .unwrap();
        assert_eq!(t.faction, crate::loadout::Faction::Corrupted);
    }

    #[test]
    fn loads_the_enemy_library() {
        // Thrax, the six Acolytes, the Corrupted Heavy Gunner; `synthetic:
        // true` (custom enemies) stays a supported flag, covered inline below.
        let specs = all();
        assert!(specs.iter().any(|s| s.id == "thrax_centurion"));
        assert!(specs.iter().any(|s| s.id == "corrupted_heavy_gunner"));
        assert!(specs.iter().any(|s| s.id == "angst"));
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
