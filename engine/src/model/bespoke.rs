// SPDX-License-Identifier: AGPL-3.0-or-later
//! DATA-DRIVEN WEAPON PASSIVE AND SIGNATURE RULES (`WeaponSpec::bespoke`).

use crate::model::{BuffDecay, BuffGrant, BuffTrigger, ClearedBy, StackingBuff};
use crate::rules::damage::DamageType;
use serde::Deserialize;

/// Consecutive-shot pellet and ammo spool (Kohm family).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PelletSpoolSpec {
    pub initial_pellets: u32,
    pub max_pellets: u32,
    pub ammo_divisor: u32,
}

#[wfsim_gas_macro::metered]
impl PelletSpoolSpec {
    pub fn pellets_at(self, consecutive_shots: f64) -> u32 {
        let step = consecutive_shots.floor().max(0.0) as u32;
        self.initial_pellets.saturating_add(step).min(self.max_pellets.max(1))
    }

    pub fn ammo_cost_at(self, consecutive_shots: f64) -> f64 {
        let pellets = self.pellets_at(consecutive_shots);
        let divisor = self.ammo_divisor.max(1);
        f64::from(pellets.div_ceil(divisor))
    }
}

/// Active reload window granting elemental Heat rounds for the fresh magazine.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerfectReloadSpec {
    pub window_pct: f64,
    pub heat_damage_bonus: f64,
    #[serde(default)]
    pub forces_heat_proc: bool,
}

/// Scoped shot-combo counter declared on a non-sniper weapon (Buzlok).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopedComboSpec {
    pub min: u32,
    pub duration_seconds: f64,
    #[serde(default)]
    pub resets_completely: bool,
}

/// Critical hit roll that sets the next landing pellet's status chance.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperStatusSpec {
    pub chance: f64,
    pub status_chance: f64,
}

/// Spores spread to nearby bodies when a hit enemy dies.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SporeOnKillSpec {
    pub targets: u32,
    pub range_m: f64,
    pub duration_seconds: f64,
    pub damage_multiplier: f64,
}

/// Non-refreshing frenzy granted on inflicting any status effect (Lesion).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusFrenzySpec {
    pub duration_seconds: f64,
    pub attack_speed_multiplier: f64,
    pub flat_status_chance: f64,
    pub toxin_damage_bonus: f64,
}

/// Kill-streak charge granting follow-through and flat status to the next hits.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VendettaSpec {
    pub kills_required: u32,
    pub enhanced_attacks: u32,
    pub follow_through_bonus: f64,
    pub flat_status_chance: f64,
}

/// Heavy-attack kill buff whose duration scales with spent combo tier.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeavyKillSpeedSpec {
    pub fire_rate_bonus: f64,
    pub base_duration_seconds: f64,
    pub duration_per_combo_tier_seconds: f64,
}

/// Non-slam kills charge the next slam's damage and blast radius.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlamCapacitorSpec {
    pub max_stacks: u32,
    pub damage_bonus_per_stack: f64,
    pub radius_per_stack_m: f64,
}

/// Periodic six-sided roll granted every N landed melee hits (Amanata).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiceRollSpec {
    pub hits_per_roll: u32,
    pub crit_chance_bonus: f64,
    pub status_chance_bonus: f64,
    pub status_damage_bonus: f64,
    pub melee_range_m: f64,
    pub fire_rate_bonus: f64,
    #[serde(default)]
    pub signature_warframe: Option<String>,
}

/// Warframe-gated signature weapon bonus.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SignatureSpec {
    pub warframes: Vec<String>,
    pub projectile_speed_bonus: f64,
    pub heavy_windup_speed_bonus: f64,
    pub flat_base_magazine: f64,
    pub flat_ammo_max: f64,
    pub ammo_max_bonus: f64,
    pub reload_speed_bonus: f64,
    pub multishot_bonus: f64,
    pub punch_through_m: f64,
    pub combo_duration_seconds: f64,
    pub extra_combo_point_chance: f64,
    pub headshot_instant_reload_chance: f64,
}

#[wfsim_gas_macro::metered]
impl SignatureSpec {
    pub fn matches(&self, tenno_id: &str) -> bool {
        self.warframes.iter().any(|w| w == tenno_id)
    }
}

/// Weapon-declared trigger for a stacking combat buff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaponBuffTrigger {
    Headshot,
    HeadshotKill,
    Kill,
    Firing,
    DirectHit,
    Crit,
    StatusApplied,
    GasStatusOnTarget,
    ReloadComplete,
    ReloadFromEmpty,
}

/// Weapon-declared grant for a stacking combat buff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeaponBuffGrant {
    BaseDamage,
    MeleeDamage,
    FireRate,
    ReloadSpeed,
    Multishot,
    AmmoEfficiency,
    CritChance,
    StatusChance,
    MeleeRange,
    StatusDamage,
}

/// Weapon-declared stacking combat buff converted into [`StackingBuff`].
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponStackingBuffSpec {
    pub trigger: WeaponBuffTrigger,
    pub grant: WeaponBuffGrant,
    pub per_stack: f64,
    pub max_stacks: u32,
    pub duration_seconds: f64,
    #[serde(default = "crate::model::one")]
    pub chance: f64,
}

#[wfsim_gas_macro::metered]
impl WeaponStackingBuffSpec {
    pub fn to_stacking_buff(&self) -> Option<StackingBuff> {
        macro_rules! bespoke_table {
            ($(($tv:ident, $tp:literal, $te:expr)),* $(,)?; $(($gv:ident, $gp:literal, $ge:expr)),* $(,)?) => {{
                bespoke_table!(@outer [$(($tv, $tp, $te)),*] [$(($gv, $gp, $ge)),*])
            }};
            (@outer [$(($tv:ident, $tp:literal, $te:expr)),*] $grants:tt) => {
                match self.trigger {
                    $(WeaponBuffTrigger::$tv => bespoke_table!(@inner $tp, $te, $grants),)*
                }
            };
            (@inner $tp:literal, $te:expr, [$(($gv:ident, $gp:literal, $ge:expr)),*]) => {
                match self.grant {
                    $(WeaponBuffGrant::$gv => (concat!("bespoke_", $tp, "_", $gp), $te, $ge),)*
                }
            };
        }
        let (id, trigger, grant) = bespoke_table!(
            (Headshot, "headshot", BuffTrigger::Headshot),
            (HeadshotKill, "headshot_kill", BuffTrigger::HeadshotKill),
            (Kill, "kill", BuffTrigger::Kill),
            (Firing, "firing", BuffTrigger::Firing),
            (DirectHit, "hit", BuffTrigger::Hit),
            (Crit, "crit", BuffTrigger::Crit),
            (StatusApplied, "status_applied", BuffTrigger::StatusApplied),
            (GasStatusOnTarget, "gas_status", BuffTrigger::HitEnemyWithStatus(DamageType::Gas)),
            (ReloadComplete, "reload_complete", BuffTrigger::ReloadComplete),
            (ReloadFromEmpty, "reload_from_empty", BuffTrigger::ReloadFromEmpty);
            (BaseDamage, "base_damage", BuffGrant::BaseDamage),
            (MeleeDamage, "melee_damage", BuffGrant::MeleeDamage),
            (FireRate, "fire_rate", BuffGrant::FireRate),
            (ReloadSpeed, "reload_speed", BuffGrant::ReloadSpeed),
            (Multishot, "multishot", BuffGrant::Multishot),
            (AmmoEfficiency, "ammo_efficiency", BuffGrant::AmmoEfficiency),
            (CritChance, "crit_chance", BuffGrant::CritChance),
            (StatusChance, "status_chance", BuffGrant::StatusChance),
            (MeleeRange, "melee_range", BuffGrant::MeleeRange),
            (StatusDamage, "status_damage", BuffGrant::StatusDamage),
        );
        Some(StackingBuff {
            id,
            trigger,
            grant,
            per_stack: self.per_stack,
            max_stacks: self.max_stacks,
            duration: self.duration_seconds,
            chance: self.chance,
            decay: BuffDecay::AllAtOnce,
            initial_stacks: 0,
            stacks_per_trigger: 1,
            per_shell: false,
            cleared_by: ClearedBy::Nothing,
            precombat: false,
        })
    }
}

/// Weapon-declared bespoke combat mechanics, boxed so ordinary weapons pay one pointer.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BespokeSpec {
    pub spindown_seconds: f64,
    pub burst_empties_magazine: bool,
    pub radial_random_nonrepeating_primary_proc: bool,
    pub pellet_spool: Option<PelletSpoolSpec>,
    pub perfect_reload: Option<PerfectReloadSpec>,
    pub scoped_combo: Option<ScopedComboSpec>,
    pub beacon_duration_seconds: Option<f64>,
    pub beacon_crit_chance_bonus: f64,
    pub deployed_fire_rate_bonus: f64,
    pub innate_initial_combo: f64,
    pub combo_points_per_tier: f64,
    pub combo_max_multiplier: f64,
    pub slash_duration_multiplier: f64,
    pub aimed_headshot_crit_chance_bonus: f64,
    pub hip_ammo_efficiency_bonus: f64,
    pub clip_explosion_bonus_per_round: f64,
    pub flat_crit_chance_per_status_type: f64,
    pub extra_tau_on_status_chance: f64,
    pub heavy_hit_flat_status_chance: f64,
    pub heavy_hit_flat_status_duration_seconds: f64,
    pub super_status_on_crit: Option<SuperStatusSpec>,
    pub spore_on_kill: Option<SporeOnKillSpec>,
    pub status_frenzy: Option<StatusFrenzySpec>,
    pub vendetta: Option<VendettaSpec>,
    pub heavy_kill_speed: Option<HeavyKillSpeedSpec>,
    pub slam_capacitor: Option<SlamCapacitorSpec>,
    pub dice_roll: Option<DiceRollSpec>,
    pub signature: Option<SignatureSpec>,
    pub stacking_buffs: Vec<WeaponStackingBuffSpec>,
}
