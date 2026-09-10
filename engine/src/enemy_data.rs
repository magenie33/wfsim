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
    /// AN INLINE DAMAGE-TYPE COLUMN, for an enemy nobody published — the one
    /// thing a player-made target needs that the roster's own units never do.
    ///
    /// A published unit names a faction and the table answers with a column
    /// (`damage_column_key`). A custom one may want a column that is in no
    /// table: "immune to Heat", "double from Void". So it may carry the column
    /// itself, and it OUTRANKS the faction key — which stays meaningful, since
    /// `combat_faction` is also what a Bane mod matches, and those are two
    /// different questions about one enemy (MECHANICS §8).
    ///
    /// Keys are damage-type names as `data/factions/damage_modifiers.yaml`
    /// spells them; a type left out takes damage as written. An IMMUNITY is
    /// simply 0.0 — the game has no third state between a multiplier and
    /// nothing getting through.
    #[serde(default)]
    pub damage_modifiers: Option<std::collections::BTreeMap<String, f64>>,
    /// STATUS IMMUNITY, which is a DIFFERENT MECHANIC from taking no damage of
    /// a type, and the difference is not a detail.
    ///
    /// The wiki states both halves in one paragraph (`Status_Effect` §Status
    /// Immunity Interactions): *"Proc type chances are not altered by enemy
    /// resistances or weaknesses to the damage components used in their
    /// computation; however, they are modified by enemy status immunities. When
    /// an attack procs a status effect on an enemy which is immune to a
    /// particular proc type, the respective damage type is EXCLUDED from proc
    /// type chance calculations for that enemy."*
    ///
    /// So the two are independent, "regardless of whether that enemy is also
    /// immune to Corrosive damage": `damage_modifiers` x0 changes what a hit
    /// DEALS and leaves the proc distribution alone, while a status immunity
    /// changes what it PROCS by removing that type from the denominator so the
    /// rest RENORMALIZE — the worked example takes Corrosive out of
    /// 20/5/10/25/50 and the other four go from 18.18/4.55/9.09/22.73% to
    /// 33.33/8.33/16.67/41.67%.
    #[serde(default)]
    pub status_immunities: Vec<String>,
    /// THE OTHER KIND, and it is a different arithmetic: the proc LANDS and its
    /// EFFECT does nothing.
    ///
    /// A status immunity above removes the type from the proc DRAW, so the
    /// others renormalise onto the roll and each becomes MORE likely. A
    /// nullified effect does not: the type still takes its share of the rolls,
    /// the status is still applied and still counts as a type for Condition
    /// Overload — only what it DOES is gone.
    ///
    /// The wiki says which is which by how it words them. A Demolisher's codex
    /// lists "Proc Immunity: Radiation" — that one cannot land. The Demolisher
    /// page lists the CROWD CONTROL it ignores — "Confusion, Knockdown, Lifted,
    /// Stagger, Stun" — and those are effects: Impact still procs on it and
    /// still does not move it.
    #[serde(default)]
    pub nullified_status_effects: Vec<String>,
    /// It cannot be FROZEN. Cold's ladder keeps climbing and never converts, so
    /// the stacks sit at their cap instead of being consumed every tenth proc —
    /// which means the Cold bonus is up for the WHOLE fight rather than in
    /// bursts around a 3-second Frozen window.
    ///
    /// It is the same kind of fact as `nullified_status_effects` — the proc
    /// lands, one part of what it does is missing — but Frozen is a STATE
    /// rather than a type, so it says so on its own.
    #[serde(default)]
    pub cannot_be_frozen: bool,
    /// HOW MUCH HEALTH A SQUAD ADDS, indexed by `squad_size - 1`, so `[0]` is
    /// solo and is always 0. Empty on every unit whose health is what it is
    /// however many people are shooting it — which is all of them but a
    /// Demolisher.
    ///
    /// A PROPERTY OF THE UNIT, spent by the FIGHT: `scenario.squad_size` is
    /// what picks the entry, and everything that does not name one stays solo
    /// by construction rather than by remembering to pass a 1.
    #[serde(default)]
    pub squad_health_bonus: Vec<f64>,
    /// A DEMOLISHER'S NULLIFYING PULSE. VERBATIM (wiki, Disruption):
    /// *"Demolysts and Demolishers will pulse out a red aura every 5 seconds
    /// with a radius of 6.5 meters, immediately dispelling and disabling all
    /// Warframe abilities within range and on itself, similar to a Nullifier
    /// Crewman's bubble."*
    ///
    /// It reaches THIS engine because the Warframe ability BUFFS are the one
    /// thing here a Warframe does — Roar, Eclipse, Nourish and the elemental
    /// augments ride on the Arena (`data/abilities/`). Against a unit that
    /// carries this, they are not up.
    ///
    /// No distance in this sim, so "within range" is always true; and no
    /// ability CASTING, so nothing re-applies between pulses. Both simplify the
    /// same way — the buffs are off for the whole fight — and both are stated
    /// on the target card rather than left to be discovered in a number.
    #[serde(default)]
    pub nullifies_warframe_abilities: bool,
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

    /// WHAT A SQUAD OF `squad` DOES TO THIS UNIT'S HEALTH — 1.0 unless the
    /// unit carries a ladder for it.
    ///
    /// CLAMPED AT THE LADDER'S END rather than extrapolated: the table is four
    /// entries because a Warframe squad is four, and a fifth number would be
    /// invented rather than read.
    pub fn squad_health_multiplier(&self, squad: u32) -> f64 {
        if self.squad_health_bonus.is_empty() {
            return 1.0;
        }
        let i = (squad.max(1) as usize - 1).min(self.squad_health_bonus.len() - 1);
        1.0 + self.squad_health_bonus[i]
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
        // A CUSTOM ENEMY MAY BRING ITS OWN COLUMN, and then it is the answer —
        // see `damage_modifiers`. The Overguard pool keeps the table's own
        // column either way: Overguard is a layer over the enemy rather than
        // part of it, and a player inventing a target does not get to invent
        // that.
        let type_mods = if self.damage_modifiers.is_some() {
            crate::factions_data::Columns {
                faction: crate::factions_data::Column::from_multipliers(&self.inline_column()?),
                overguard: crate::factions_data::overguard_column(),
            }
        } else {
            crate::factions_data::columns_for(self.damage_column_key())
        };
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
                instance_fraction: a.max_instance_fraction_of_health,
                dps_fraction: a.max_dps_fraction_of_health,
            }),
            stack_caps: self.status_stack_caps.map(|c| StackCaps {
                general: c.general,
                impact: c.impact,
            }),
            cannot_be_frozen: self.cannot_be_frozen,
            steel_path,
            eximus,
            can_be_eximus: self.can_be_eximus,
            type_mods,
            status_immunities: self
                .status_immunities
                .iter()
                .map(|k| {
                    crate::damage::DamageType::from_name(k)
                        .ok_or_else(|| format!("{}: no damage type named '{k}'", self.name))
                })
                .collect::<Result<Vec<_>, _>>()?,
            faction: self
                .combat_faction
                .as_deref()
                .map(crate::loadout::Faction::from_name)
                .unwrap_or(crate::loadout::Faction::Unknown),
            mode,
        })
    }

    /// This enemy's inline column, resolved against the damage-type names.
    ///
    /// A NAME IT DOES NOT KNOW IS AN ERROR rather than a silently ignored
    /// entry: the whole point of the field is to state something unusual, and
    /// "heatt: 0" that quietly does nothing is a target the reader believes is
    /// immune and is not.
    fn inline_column(&self) -> Result<Vec<(crate::damage::DamageType, f64)>, String> {
        let m = match &self.damage_modifiers {
            Some(m) => m,
            None => return Ok(Vec::new()),
        };
        m.iter()
            .map(|(k, v)| {
                let t = crate::damage::DamageType::from_name(k)
                    .ok_or_else(|| format!("{}: no damage type named '{k}'", self.name))?;
                if !(0.0..=100.0).contains(v) {
                    return Err(format!("{}: {k} x{v} is not a damage multiplier", self.name));
                }
                Ok((t, *v))
            })
            .collect()
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
    /// THE NULLIFY GATE ITSELF, kept under test while no unit turns it on.
    ///
    /// A flag nothing sets is a flag that rots. This asserts the two halves
    /// that matter — the field survives a round trip through the loader, and
    /// `parse_fight` is the one place that reads it — so the day a Demolisher
    /// (or a Nullifier) switches it on, the path is not being written for the
    /// first time.
    #[test]
    fn a_target_that_nullifies_takes_the_players_buffs() {
        let spec: EnemySpec = serde_norway::from_str(
            r#"
id: synthetic_nullifier
name: Synthetic Nullifier
faction: grineer
scaling_faction: grineer
type: melee
nullifies_warframe_abilities: true
stats: { base_level: 1, health: 100, shield: 0, armor: 0, overguard: 0, affinity: 0 }
body_parts:
  - { name: body, multiplier: 1.0, is_head: false, crit_bonus: false }
source: { url: "synthetic" }
"#,
        )
        .expect("the field parses");
        assert!(spec.nullifies_warframe_abilities);

        // …and the default is OFF, so an ordinary unit never silently strips a
        // player's buffs.
        let roster = all();
        let on: Vec<&str> = roster
            .iter()
            .filter(|e| e.nullifies_warframe_abilities)
            .map(|e| e.id.as_str())
            .collect();
        assert!(on.is_empty(), "no unit turns it on today: {on:?}");
    }

    /// A DEMOLISHER'S RESTRICTIONS, and NOT ONE OF THEM STOPS A PROC LANDING.
    ///
    /// The wiki's list is *"Confusion, Knockdown, Lifted, Stagger, Stun
    /// EFFECTS"*, and the last word decides it: an effect is what a proc DOES.
    /// Two of the five have a weapon status behind them — Confusion is
    /// Radiation's and Stagger is Impact's — and both land, take their share of
    /// the rolls and count as a status type for Condition Overload.
    ///
    /// READING EITHER AS A PROC IMMUNITY IS WRONG TWICE: the draw renormalises
    /// without that type so every other status gets rarer, and a Condition
    /// Overload build loses a type it actually has. Radiation was in the wrong
    /// list here, which is exactly the failure the Impact note warned about.
    ///
    /// THE ONE PROC IMMUNITY THE PAGE STATES IS SOMEBODY ELSE'S: Viral, on the
    /// INFESTED Demolishers. This unit is Grineer.
    ///
    /// IT IS NEVER FROZEN, which is the third kind again — the Cold proc lands
    /// and one thing it does is missing, so the ladder climbs to its cap and
    /// stays instead of spending itself every tenth proc.
    #[test]
    fn a_demolishers_three_restrictions_are_three_different_mechanics() {
        let roster = all();
        let d = roster
            .iter()
            .find(|e| e.id == "demolisher_devourer")
            .expect("the roster carries it");

        // THE PULSE IS NOT TURNED ON, and that is a CHOICE rather than a gap in
        // what the sim can do — the same shape as the Thrax's spectral form. The machinery is built and covered by the test
        // below; the unit says so where a player reads it.
        assert!(!d.nullifies_warframe_abilities, "not applied to this unit");
        assert!(
            d.unmodeled.iter().any(|u| u.contains("nullifying pulse")),
            "…and the card says so: {:?}",
            d.unmodeled
        );
        // NOTHING IS DROPPED FROM THE DRAW. An entry here would make every
        // other status likelier, which is a change to every status build's
        // numbers made by a data file nobody re-read.
        assert!(d.status_immunities.is_empty(), "{:?} cannot land", d.status_immunities);
        assert_eq!(
            d.nullified_status_effects, ["radiation", "impact"],
            "both land and both do nothing"
        );
        assert!(d.cannot_be_frozen);
        assert!(
            d.damage_modifiers.is_none(),
            "none of this is a x0 column: the damage is untouched"
        );

        // BOTH NULLIFIED TYPES STILL TAKE THEIR SHARE OF THE ROLLS, which is
        // the whole difference between the two lists and the thing that was
        // wrong here.
        let immune: Vec<crate::damage::DamageType> = d
            .status_immunities
            .iter()
            .filter_map(|s| crate::damage::DamageType::from_name(s))
            .collect();
        let share = |ty: crate::damage::DamageType, immune: &[crate::damage::DamageType]| {
            let v = crate::damage::DamageVector::new()
                .with(ty, 75.0)
                .with(crate::damage::DamageType::Slash, 25.0);
            let mut rng = crate::rng::Rng::new(0xD3E0);
            (0..4000)
                .filter(|_| crate::status::draw_proc_type(&v, immune, &mut rng) == Some(ty))
                .count()
        };
        for ty in [crate::damage::DamageType::Impact, crate::damage::DamageType::Radiation] {
            let n = share(ty, &immune);
            assert!(
                (2800..3200).contains(&n),
                "{ty:?} still takes its 3/4 of the rolls on this unit: {n} of 4000"
            );
        }

        // …AND WHAT A PROC IMMUNITY WOULD HAVE DONE, on a list this unit does
        // NOT have. The mechanism is real and belongs to the units that do have
        // one; asserting it against a fixture is what keeps the difference
        // visible after the day Radiation was taken out of the real list.
        let pretend = [crate::damage::DamageType::Radiation];
        assert_eq!(
            share(crate::damage::DamageType::Radiation, &pretend), 0,
            "an immune type is dropped from the draw, not merely made rarer"
        );
    }

    /// **A SQUAD MAKES THE DEMOLISHER FATTER AND NOTHING ELSE.**
    ///
    /// VERBATIM (wiki, Disruption): *"Demolysts and Demolishers gain increased
    /// health based on squad size. Squad Size 1: +0% health / Squad Size 2:
    /// +50% health / Squad Size 3: +100% health / Squad Size 4: +200% health"*.
    ///
    /// THE LADDER IS THE UNIT'S AND THE RUNG IS THE FIGHT'S, which is the whole
    /// reason this is a multiplier a caller asks for rather than a stat baked
    /// into the block: the same Demolisher is the one a lone player meets and
    /// the one a full squad meets.
    ///
    /// AND EVERY OTHER UNIT ANSWERS 1. A ladder nobody else carries must not
    /// become a silent scale on the roster — the Thrax boards would have moved
    /// under the change that introduced it.
    #[test]
    fn a_squad_scales_a_demolishers_health_and_no_one_elses() {
        let of = |id: &str| {
            super::all()
                .iter()
                .find(|e| e.id == id)
                .unwrap_or_else(|| panic!("no enemy {id}"))
                .clone()
        };
        let demo = of("demolisher_devourer");
        for (squad, want) in [(1u32, 1.0), (2, 1.5), (3, 2.0), (4, 3.0)] {
            let got = demo.squad_health_multiplier(squad);
            assert!((got - want).abs() < 1e-9, "squad {squad}: {got} against {want}");
        }
        // PAST THE LADDER'S END IT CLAMPS rather than extrapolating: a squad is
        // four, and a fifth number would be invented rather than read.
        assert!((demo.squad_health_multiplier(9) - 3.0).abs() < 1e-9);
        // …AND A SQUAD OF ZERO IS A SQUAD OF ONE, so no caller can divide the
        // roster by accident.
        assert!((demo.squad_health_multiplier(0) - 1.0).abs() < 1e-9);

        // THE NEGATIVE CONTROL, and it is the one that matters: every other
        // unit on the roster is unmoved at every squad size.
        for e in super::all().iter().filter(|e| e.id != "demolisher_devourer") {
            for squad in 1..=4 {
                assert!(
                    (e.squad_health_multiplier(squad) - 1.0).abs() < 1e-9,
                    "{} moved at squad {squad}", e.id
                );
            }
        }
    }

    use super::*;
    use std::path::PathBuf;

    /// STATUS IMMUNITY IS NOT DAMAGE IMMUNITY, and the wiki says so in as many
    /// words — "regardless of whether that enemy is also immune to Corrosive
    /// damage". One changes what a hit DEALS, the other what it PROCS, so a
    /// spec may carry either, both, or the same type in both.
    #[test]
    fn a_status_immunity_is_not_a_damage_immunity() {
        let yaml = r#"
id: t
name: T
scaling_faction: grineer
status_immunities: [slash]
damage_modifiers: { heat: 0.0 }
stats: { base_level: 1, health: 100 }
body_parts: [{ name: body, multiplier: 1.0 }]
"#;
        let s = EnemySpec::from_yaml_str(yaml).expect("parses");
        let t = s
            .target_params(1, false, false, TargetMode::InstantRespawn)
            .expect("builds");
        assert_eq!(t.status_immunities, vec![crate::damage::DamageType::Slash]);
        // …the Slash it cannot BLEED from still lands its damage in full, and
        // the Heat it takes none of is still drawn for procs.
        assert_eq!(t.type_mods.faction.get(crate::damage::DamageType::Slash), 1.0);
        assert_eq!(t.type_mods.faction.get(crate::damage::DamageType::Heat), 0.0);
    }

    /// A NAME THE ENGINE DOES NOT KNOW IS AN ERROR, not a silently ignored
    /// entry: an immunity that quietly does nothing is a target the reader
    /// believes is immune and is not.
    #[test]
    fn an_unknown_status_immunity_is_an_error() {
        let yaml = r#"
id: t
name: T
scaling_faction: grineer
status_immunities: [slashh]
stats: { base_level: 1, health: 100 }
body_parts: [{ name: body, multiplier: 1.0 }]
"#;
        let s = EnemySpec::from_yaml_str(yaml).expect("parses");
        assert!(s
            .target_params(1, false, false, TargetMode::InstantRespawn)
            .is_err());
    }


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
        assert!((a.instance_fraction - 0.05).abs() < 1e-12);
        let c = t.stack_caps.unwrap();
        assert_eq!((c.general, c.impact), (4, 3));
    }
}
