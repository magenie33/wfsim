// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE TARGET — who is being shot, as the fight sees them: pools scaled to a
//! level, the faction and its vulnerability columns, the body parts a shot can
//! land on, and how the fight treats a death.
//!
//! A catalog entry (`data::enemies`) becomes one of these; the fight, the arena
//! and the formation all hold one. It is the input, never the fight's state —
//! what a body has LEFT mid-fight is `fight::TargetState`.

use crate::rules::damage::{DamageType, DamageVector};
use crate::rules::scaling;

/// The simulated target: base stats + level, scaled via [`scaling`].
///
/// Prefer building this through `data::enemies::EnemySpec::target_params`, which
/// rejects combinations that do not exist in-game (e.g. an Eximus of a unit
/// with no Eximus variant). Hand-built values are re-checked at spawn.
///
/// NOT `TargetParams`: `target` meant two things — this, and the BODY being
/// aimed at (`Run::target`, `Arena::target_at`) — and only one of them can
/// keep the word. `foe` is what thirty-six call sites already named their
/// parameter, so the type is catching up with them rather than inventing a
/// fourth word for an enemy.
///
/// The pair is `data::enemies::EnemySpec` — what the roster file says — and
/// this, the same shape as `WeaponBase` against a resolved panel: the spec,
/// and the spec brought to a level and ready to be fought.
#[derive(Debug, Clone)]
pub struct Foe {
    pub name: String,
    pub base_level: u32,
    pub level: u32,
    pub base_health: f64,
    pub base_armor: f64,
    pub base_overguard: f64,
    /// What this unit is worth in affinity BEFORE level scaling — the enemy
    /// file's own number. Scaled by `rules::scaling::affinity_multiplier` at the kill.
    pub base_affinity: f64,
    /// Base shields (mitigation order: Overguard → Shield → Health;
    /// Toxin bypasses shields but NOT overguard).
    pub base_shield: f64,
    pub health_curve: scaling::Curve,
    pub shield_curve: scaling::Curve,
    /// Boss-type damage attenuation (Acolytes etc.); `None` = none.
    pub attenuation: Option<Attenuation>,
    /// Per-unit status stack caps; `None` = the normal per-status caps.
    pub stack_caps: Option<StackCaps>,
    /// Cold never converts on this target — see
    /// [`crate::data::enemies::EnemySpec::cannot_be_frozen`]. The stacks climb to
    /// the ordinary ten-stack cap and STAY there, so the Cold crit-damage bonus
    /// is up for the whole fight instead of being spent every tenth proc.
    pub cannot_be_frozen: bool,
    /// THE SECOND HALF OF A THRAX'S DEATH, when the FIGHT asked for it —
    /// `data::enemies::SpectralForm`. `None` is a unit that dies once, which is
    /// every other enemy and every fight that leaves the box unticked.
    pub spectral: Option<crate::model::SpectralForm>,
    /// Steel Path: health ×2.5 (armor and overguard untouched). The +100 level
    /// shift is a mission-spawn effect — pick `level` accordingly.
    pub steel_path: bool,
    /// Eximus variant: boosted base health + overguard (wiki `Eximus`). Only
    /// legal when `can_be_eximus` — the combination is validated at spawn.
    pub eximus: bool,
    /// Whether this unit has an Eximus variant in-game (wiki
    /// `Eximus/Compatibilities`; Thrax units do not).
    pub can_be_eximus: bool,
    /// Unit-level status immunities: these types are EXCLUDED from the proc
    /// draw (weights renormalize — wiki `Status_Effect` §Immunity
    /// Interactions). Mechanic states (Frozen, Overguard suppression) are NOT
    /// immunities: those procs are drawn normally and nulled on landing.
    pub status_immunities: Vec<DamageType>,
    /// Combat faction — the match key for faction-damage mods (Bane/Expel).
    /// `Unknown` (the default for hand-made targets) means no faction mod
    /// applies. Set from the enemy YAML `combat_faction:` field.
    pub faction: crate::model::Faction,
    /// The post-U36 damage-type vulnerability columns (System B): this unit's
    /// own, keyed by `FactionDamageOverride ?? Faction`, plus the Overguard
    /// pool's. A per-COMPONENT multiplier, independent of `faction` above —
    /// the two systems have different keys and stack multiplicatively
    /// (docs/MECHANICS.md §8).
    pub type_mods: crate::data::factions::Columns,
    /// A FLAT MULTIPLIER THIS UNIT APPLIES INSIDE THE FACTION BRACKET, 1.0 on
    /// everything that does not declare one. It is NOT attenuation and not a
    /// vulnerability column: it rides `faction_at_time`, so it is re-applied at
    /// every derivation step exactly as a Bane is — ×m on a hit, ×m² on a
    /// status that hit applied (`faction_at`). 0.8 therefore reads ×0.8 and
    /// ×0.64, which is the shape it was measured in (MEASUREMENTS M89).
    pub faction_bracket_multiplier: f64,
    pub mode: TargetMode,
}

/// The SHAPE of one damage instance — what fraction of it is each type.
///
/// The defence side reads the shape twice: Toxin bypasses shields, and the
/// vulnerability column is per component. Passing a bare `toxin_frac` down
/// answers one of them and makes the next per-type question a growing list of
/// scalars, so the shape travels as one value.
#[derive(Debug, Clone, Copy, Default)]
pub struct TypeShares([f64; DamageType::ALL.len()]);

/// WHAT THE WIELDER TOOK FROM THEIR OWN BUILD, by damage type.
///
/// **NOBODY DIES HERE AND NOTHING IS APPLIED.** This arena has no Tenno to
/// damage — `nobody_shoots_back` is one of its declared classes — so self
/// damage is COUNTED and never paid. That is not a shrug: a card or a weapon
/// that charges the wielder is making a trade, and a model that drops the cost
/// prices the trade wrong even when it cannot simulate the consequence.
///
/// BY TYPE rather than a total, because the types are not interchangeable to a
/// reader: Heat over six seconds and one lump of Blast are different problems
/// and a single number cannot be either.
///
/// Truth's Flame is the first source — 100 Heat a second for six, every time a
/// Tennokai attack fails its kill — and it will not be the last: a self-damaging
/// launcher lands in the same accumulator with no new machinery.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SelfDamage([f64; DamageType::ALL.len()]);

impl SelfDamage {
    pub fn add(&mut self, t: DamageType, v: f64) {
        self.0[t as usize] += v;
    }
    pub fn of(&self, t: DamageType) -> f64 {
        self.0[t as usize]
    }
    pub fn total(&self) -> f64 {
        self.0.iter().sum()
    }
    /// Every type that took something, biggest first — what a reader is shown.
    pub fn parts(&self) -> Vec<(DamageType, f64)> {
        let mut v: Vec<(DamageType, f64)> = DamageType::ALL
            .iter()
            .copied()
            .filter(|&t| self.0[t as usize] > 0.0)
            .map(|t| (t, self.0[t as usize]))
            .collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v
    }
    pub fn merge(&mut self, o: &Self) {
        for (a, b) in self.0.iter_mut().zip(o.0.iter()) {
            *a += b;
        }
    }
    pub fn scale(&self, k: f64) -> Self {
        let mut out = *self;
        for a in &mut out.0 {
            *a *= k;
        }
        out
    }
}

impl TypeShares {
    /// THE BIGGEST SHARE — which type a mixed instance reads AS, for DISPLAY
    /// only. Same contract as `DamageVector::dominant`, and nothing in the
    /// damage path reads either.
    pub fn dominant(&self) -> DamageType {
        let mut best = 0;
        for i in 1..self.0.len() {
            if self.0[i] > self.0[best] {
                best = i;
            }
        }
        DamageType::ALL[best]
    }

    /// An instance that is entirely one type — a DoT tick, a Blast
    /// detonation, an arcane's flat instance.
    pub fn single(t: DamageType) -> Self {
        let mut s = [0.0; DamageType::ALL.len()];
        s[t as usize] = 1.0;
        TypeShares(s)
    }

    /// A hit's shape, from the vector it was quantized to. A zero vector has
    /// no shape and no damage; its multipliers come out 1.0 either way.
    pub fn of(v: &DamageVector) -> Self {
        let total = v.total();
        let mut s = [0.0; DamageType::ALL.len()];
        if total > 0.0 {
            for t in DamageType::ALL {
                s[t as usize] = v.get(t) / total;
            }
        }
        TypeShares(s)
    }

    /// The Toxin share — what bypasses shields straight to health.
    pub fn toxin(&self) -> f64 {
        self.0[DamageType::Toxin as usize]
    }

    /// ONE TYPE'S SHARE of the instance — what Melee Influence copies, since
    /// it spreads *"that element's damage from the original attack"* and not
    /// the attack.
    pub fn share(&self, t: DamageType) -> f64 {
        self.0[t as usize]
    }

    /// Does this instance have a shape at all? A shapeless one (nothing but
    /// zeros) is treated as one untyped, neutral lump rather than as damage
    /// that vanishes — the shares are bookkeeping, never a gate on damage.
    pub(crate) fn shaped(&self) -> bool {
        self.0.iter().sum::<f64>() > 0.0
    }

    /// The whole instance's multiplier under a column. Each component takes
    /// its own factor, so with shares summing to 1 this is the weighted mean.
    pub(crate) fn whole(&self, col: &crate::data::factions::Column) -> f64 {
        if !self.shaped() {
            return 1.0;
        }
        DamageType::ALL
            .iter()
            .map(|&t| self.0[t as usize] * col.get(t))
            .sum()
    }

    /// The part that does NOT bypass shields, already column-scaled — a
    /// PORTION of the instance, not a multiplier on it.
    pub(crate) fn non_toxin_portion(&self, col: &crate::data::factions::Column) -> f64 {
        if !self.shaped() {
            return 1.0;
        }
        self.whole(col) - self.toxin_portion(col)
    }

    /// The Toxin part, already column-scaled. Goes straight to health.
    pub(crate) fn toxin_portion(&self, col: &crate::data::factions::Column) -> f64 {
        self.toxin() * col.get(DamageType::Toxin)
    }

    /// THE BIGGEST SHARE THAT A SHIELD ACTUALLY STOPS — what the non-Toxin
    /// half of a split instance reads as. Same contract as [`Self::dominant`]
    /// and, like it, display only. Falls back to the whole instance's dominant
    /// where there is no non-Toxin share to speak of.
    pub(crate) fn dominant_non_toxin(&self) -> DamageType {
        let mut best: Option<usize> = None;
        for i in 0..self.0.len() {
            if DamageType::ALL[i] == DamageType::Toxin {
                continue;
            }
            if best.is_none_or(|b| self.0[i] > self.0[b]) {
                best = Some(i);
            }
        }
        best.filter(|&b| self.0[b] > 0.0)
            .map_or_else(|| self.dominant(), |b| DamageType::ALL[b])
    }
}

impl Foe {
    /// A plain training dummy: no defenses, never dies. Damage passes through
    /// unmitigated, which keeps raw-damage calibration runs simple.
    pub fn training_dummy() -> Self {
        Self {
            name: "training dummy".into(),
            // A DUMMY DIES ONCE, like everything but a Thrax.
            spectral: None,
            base_level: 1,
            level: 1,
            base_health: 1.0,
            base_armor: 0.0,
            base_overguard: 0.0,
            base_affinity: 0.0,
            base_shield: 0.0,
            health_curve: scaling::health::UNAFFILIATED,
            shield_curve: scaling::shield::GRINEER, // unused at 0 shields
            attenuation: None,
            stack_caps: None,
            cannot_be_frozen: false,
            steel_path: false,
            eximus: false,
            can_be_eximus: false,
            status_immunities: Vec::new(),
            faction: crate::model::Faction::Unknown,
            // A training dummy has no faction and takes damage as written.
            type_mods: crate::data::factions::Columns::NEUTRAL,
            faction_bracket_multiplier: 1.0,
            mode: TargetMode::InfiniteHealth,
        }
    }

    /// Impossible-combination check (see `data::enemies` for the rigor rule).
    pub fn validate(&self) -> Result<(), String> {
        if self.eximus && !self.can_be_eximus {
            return Err(format!(
                "{} cannot be an Eximus: no such unit exists in-game",
                self.name
            ));
        }
        Ok(())
    }

    /// Effective base health: Eximus units replace theirs with the boosted
    /// level-dependent value before the faction curve applies.
    pub(crate) fn effective_base_health(&self) -> f64 {
        if self.eximus {
            scaling::eximus_base_health(self.base_health, self.level, self.base_armor > 0.0)
        } else {
            self.base_health
        }
    }

    /// …AND THE SAME NUMBER WITHOUT THE MODE'S GIFT, which one mechanic reads
    /// and nothing else does.
    ///
    /// VERBATIM (Acid Shells): *"The Blast damage bonus does not account for
    /// the bonus Health given to enemies in The Steel Path, Archon Hunt, Deep
    /// Archimedea, and similar modes."* So a percentage of "maximum Health"
    /// there means the unit's own scaled maximum before the multiplier the
    /// MODE applies — which is this, and it is `max_health()` on any fight that
    /// is not one of those modes.
    pub fn max_health_before_steel_path(&self) -> f64 {
        let delta = self.level.saturating_sub(self.base_level) as f64;
        self.effective_base_health() * self.health_curve.multiplier(delta)
    }

    /// Scaled max health at `level` (Steel Path ×2.5 applied).
    pub fn max_health(&self) -> f64 {
        let delta = self.level.saturating_sub(self.base_level) as f64;
        let sp = if self.steel_path {
            scaling::STEEL_PATH_HEALTH_MULT
        } else {
            1.0
        };
        self.effective_base_health() * self.health_curve.multiplier(delta) * sp
    }

    /// Scaled armor (spawn minimum 200, cap 2,700; Steel Path does not touch
    /// armor since U36).
    pub fn armor(&self) -> f64 {
        scaling::armor_at(self.base_armor, self.level, self.base_level)
    }

    /// Scaled max shields (Steel Path ×2.5, like health).
    pub fn max_shield(&self) -> f64 {
        let delta = self.level.saturating_sub(self.base_level) as f64;
        let sp = if self.steel_path {
            scaling::STEEL_PATH_SHIELD_MULT
        } else {
            1.0
        };
        self.base_shield * self.shield_curve.multiplier(delta) * sp
    }

    /// Scaled overguard (uses `level − 1`; no Steel Path bonus documented).
    /// Eximus base overguard is 12; no in-game unit combines innate overguard
    /// with Eximus status, so the max() is only a defensive guess.
    pub fn overguard(&self) -> f64 {
        let base = if self.eximus {
            scaling::EXIMUS_BASE_OVERGUARD.max(self.base_overguard)
        } else {
            self.base_overguard
        };
        scaling::overguard_at(base, self.level)
    }
}

/// What happens when the target's health reaches zero.
///
/// These are simulator conveniences for calibration (the real Simulacrum has
/// neither an enemy-invincibility nor an instant-respawn toggle):
/// - `InfiniteHealth`: pools never deplete — measure steady per-shot damage
///   against a fixed defensive state.
/// - `InstantRespawn`: the target dies and instantly respawns in place at full
///   pools; overkill damage is lost. **Decision: no on-death
///   transformation is modeled** (e.g. Thrax spectral forms are skipped — the
///   respawned target is always the physical form).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetMode {
    InfiniteHealth,
    InstantRespawn,
}

/// Damage attenuation (wiki U40 STRUCTURE: the enemy caps the damage it
/// can take per INSTANCE and per SECOND, both proportional to Max
/// Health, measured per player). The exact constants are UNPUBLISHED —
/// these fractions are recorded estimates pending in-game calibration
/// (the data file marks them as such).
#[derive(Debug, Clone, Copy)]
pub struct Attenuation {
    /// Max effective damage per damage instance / max health.
    pub instance_fraction: f64,
    /// Max effective damage per second / max health.
    pub dps_fraction: f64,
}

/// Per-unit status stack caps (Acolytes: any status 4, Impact 3).
#[derive(Debug, Clone, Copy)]
pub struct StackCaps {
    pub general: usize,
    pub impact: usize,
}

/// One aimable location on the target (wiki `Enemy_Body_Parts`).
#[derive(Debug, Clone)]
pub struct BodyPart {
    pub name: String,
    /// Relative probability of a shot landing here (weights are normalized).
    pub aim_weight: f64,
    /// Location damage multiplier.
    pub multiplier: f64,
    /// True head: fires on-headshot effects (`Hit::headshot`). Other weak
    /// spots never trigger headshot conditions.
    pub is_head: bool,
    /// Eligible for the critical-location bonus (the `2*cd` fold-in). False
    /// for e.g. MOA fanny packs and helmeted Corpus heads; locations at 1x
    /// never get the bonus regardless of this flag.
    pub crit_bonus: bool,
}

/// WHICH POOL a portion of a damage instance landed in.
///
/// The game pops ONE NUMBER PER POOL, which is why this is carried rather than
/// summed away: Toxin bypasses a shield while its siblings do not, so a single
/// pellet on a shielded Corpus unit shows two numbers side by side. An engine
/// that reports their sum cannot be checked against a recording.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Pool {
    Overguard,
    Shield,
    /// THE DEFAULT, because it is the pool every fight ends in and the only one
    /// a target is guaranteed to have.
    #[default]
    Health,
}

impl Pool {
    pub fn name(self) -> &'static str {
        match self {
            Pool::Overguard => "overguard",
            Pool::Shield => "shield",
            Pool::Health => "health",
        }
    }
}

impl BodyPart {
    /// A generic humanoid: body 1x, head 3x (headshot-triggering, crit-bonus
    /// eligible), aimed at 50/50.
    pub fn humanoid() -> Vec<BodyPart> {
        vec![
            BodyPart {
                name: "body".into(),
                aim_weight: 0.5,
                multiplier: 1.0,
                is_head: false,
                crit_bonus: false,
            },
            BodyPart {
                name: "head".into(),
                aim_weight: 0.5,
                multiplier: 3.0, // humanoid head (wiki: Enemy_Body_Parts)
                is_head: true,
                crit_bonus: true,
            },
        ]
    }
}

/// A CATALOG ENTRY BECOMES A TARGET here, not in the catalog: the enemy file
/// says what a unit is, and only the layer that holds `Foe` says what
/// the fight makes of it.
impl crate::data::enemies::EnemySpec {
    /// Build the simulation target. Fails on combinations that do not exist
    /// in-game (e.g. `eximus` for a unit with no Eximus variant).
    pub fn target_params(
        &self,
        level: u32,
        steel_path: bool,
        eximus: bool,
        mode: TargetMode,
    ) -> Result<Foe, String> {
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
            crate::data::factions::Columns {
                faction: crate::data::factions::Column::from_multipliers(&self.inline_column()?),
                overguard: crate::data::factions::overguard_column(),
            }
        } else {
            crate::data::factions::columns_for(self.damage_column_key())
        };
        Ok(Foe {
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
            faction_bracket_multiplier: self.faction_bracket_multiplier,
            stack_caps: self.status_stack_caps.map(|c| StackCaps {
                general: c.general,
                impact: c.impact,
            }),
            cannot_be_frozen: self.cannot_be_frozen,
            // OFF UNLESS THE FIGHT ASKS. `spectral_form` is what the second
            // half of this unit's death WOULD be; a fight that has never heard
            // of the switch cannot get it, which is why this is `None` here and
            // filled by the caller rather than read off the spec.
            spectral: None,
            steel_path,
            eximus,
            can_be_eximus: self.can_be_eximus,
            type_mods,
            status_immunities: self
                .status_immunities
                .iter()
                .map(|k| {
                    crate::rules::damage::DamageType::from_name(k)
                        .ok_or_else(|| format!("{}: no damage type named '{k}'", self.name))
                })
                .collect::<Result<Vec<_>, _>>()?,
            faction: self
                .combat_faction
                .as_deref()
                .map(crate::model::Faction::from_name)
                .unwrap_or(crate::model::Faction::Unknown),
            mode,
        })
    }

    /// This enemy's inline column, resolved against the damage-type names.
    ///
    /// A NAME IT DOES NOT KNOW IS AN ERROR rather than a silently ignored
    /// entry: the whole point of the field is to state something unusual, and
    /// "heatt: 0" that quietly does nothing is a target the reader believes is
    /// immune and is not.
    fn inline_column(&self) -> Result<Vec<(crate::rules::damage::DamageType, f64)>, String> {
        let m = match &self.damage_modifiers {
            Some(m) => m,
            None => return Ok(Vec::new()),
        };
        m.iter()
            .map(|(k, v)| {
                let t = crate::rules::damage::DamageType::from_name(k)
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
