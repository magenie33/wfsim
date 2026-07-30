//! Declarative arcane loader: `data/arcanes/<slot>/*.yaml` -> the arcane pool.
//!
//! Arcanes are DATA, not code (same pattern as [`crate::mods_data`]). Each
//! YAML records the wiki-verified schema (X-templated description, effects
//! with `rank0`/`rankMax`, triggered buffs as `kind: buff`, an explicit
//! `ranks:` list only where per-rank values are non-linear). This module
//! parses them into [`ArcaneDef`] and resolves a def AT A RANK into the flat
//! [`ArcaneFx`] the simulator consumes.
//!
//! Policy handling mirrors mods (docs/OPTIMIZER.md §3): triggers the timeline
//! actually fires (kills, headshot kills, own Heat/Electricity procs, target
//! state) run EMERGENTLY; triggers outside the sim's world (rolls, ability
//! casts, weapon swaps, overshields) contribute their assumed-max value ONLY
//! under [`StackPolicy::AssumedMax`] — under `Emergent` they are honest
//! no-ops until the configured-buff policy lands (devlog 2026-07-27).

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;
use serde_norway::Value;

use crate::loadout::{count_x, fill_x, pct, Rarity, StackPolicy};

#[derive(Debug, Deserialize)]
struct ArcaneFile {
    id: String,
    name: String,
    rarity: String,
    max_rank: u32,
    /// Weapon trait required for the effects to apply (Akimbo Slip Shot →
    /// `dual_pistols`); calc-layer gate like a mod's `requires`.
    #[serde(default)]
    requires: Option<String>,
    /// Custom perk implementation id (Secondary Enervate's ramp/reset lives
    /// in `engine::perks`, not in the declarative effect vocabulary).
    #[serde(default)]
    perk: Option<String>,
    /// Verbatim in-game text, rank-varying numbers as `X` (schema).
    #[serde(default)]
    description: Option<String>,
    effects: Vec<Value>,
}

/// A per-rank value: `rank0`→`rankMax` linear unless an explicit non-linear
/// `ranks:` table overrides it (Kinship, Outburst, Cryogenic).
#[derive(Debug, Clone, PartialEq)]
pub struct Scale {
    rank0: Option<f64>,
    rank_max: f64,
    ranks: Option<Vec<f64>>,
}

impl Scale {
    fn at(&self, rank: u32, max_rank: u32) -> f64 {
        if let Some(t) = &self.ranks {
            return t[(rank as usize).min(t.len().saturating_sub(1))];
        }
        let r0 = match self.rank0 {
            Some(v) => v,
            None => return self.rank_max,
        };
        if max_rank == 0 {
            return self.rank_max;
        }
        r0 + (self.rank_max - r0) * rank.min(max_rank) as f64 / max_rank as f64
    }
}

/// What an emergent arcane stacking buff adds per stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcGrant {
    /// Joins the Hornet Strike bracket live (scales ModifiedBase too).
    BaseDamage,
    /// Additive multishot (already an absolute pellet-count bonus × base? —
    /// no: a RELATIVE bonus; the sim multiplies by base pellets itself).
    Multishot,
    /// Joins the reload-speed bucket (time = base / (1 + Σ)).
    ReloadSpeed,
    /// Joins the crit-DAMAGE bucket — a RELATIVE bonus on the weapon's base
    /// crit damage, like the crit-damage mods (Primary Blight/Frostbite).
    CritDamage,
    /// Joins the status-chance bucket (Primary Crux). VERBATIM (wiki):
    /// "Status Chance bonus is additive to mods like Rifle Aptitude", so it is
    /// a RELATIVE bonus on the attack part's base status chance.
    ///
    /// Unlike `CritDamage` this is NOT resolved to an absolute value in
    /// [`ArcaneDef::fx`]: the direct hit and the explosion carry DIFFERENT
    /// base status chances, so only the sim — which knows which attack part it
    /// is resolving — can multiply it out.
    StatusChance,
    /// Additive ammo efficiency, i.e. the refunded fraction of a round
    /// (Primary Crux's second grant). Wiki: "additive with other sources of
    /// Ammo Efficiency", the same bucket Frenzy feeds.
    AmmoEfficiency,
}

/// What event grants/refreshes a stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcTrigger {
    /// Any kill (Secondary Merciless).
    Kill,
    /// Direct-pellet headshot kill (Secondary Deadhead's precision boundary).
    HeadshotKill,
    /// Melee kill — the sim has no melee: the buff starts full (user
    /// setting) and only decays (Secondary Dexterity).
    MeleeKill,
    /// A Heat status this weapon applies (Cascadia Flare).
    HeatStatus,
    /// An Electricity status this weapon applies (Conjunction Voltage).
    ElectricityStatus,
    /// A Toxin status this weapon applies (Primary Blight). Blight is
    /// stricter than the other on-status arcanes — wiki: "stacking the
    /// Blight buff requires the Toxin proc to be inflicted by using the
    /// attached primary weapon" — which is exactly what the sim can see.
    ToxinStatus,
    /// A Cold status this weapon applies (Primary Frostbite).
    ColdStatus,
    /// A direct-pellet hit on a natural weak point (Primary Crux) — a HIT, not
    /// a kill, and PER PELLET: "Multiple individual pellets from a single shot
    /// (either innate to the weapon or generated via Multishot) can build
    /// stacks" (wiki). Weak spots created by Banshee's Sonar do NOT count,
    /// which is also exactly what `BodyPart::is_head` means here.
    WeakpointHit,
}

/// One emergent stacking buff, resolved at a rank.
#[derive(Debug, Clone, PartialEq)]
pub struct ArcBuffSpec {
    pub grant: ArcGrant,
    pub trigger: ArcTrigger,
    pub per_stack: f64,
    pub max_stacks: u32,
    pub duration: f64,
    /// true = ALL stacks drop on timeout (the on-status family: Cascadia
    /// Flare, Conjunction Voltage); false = lose ONE stack and reset the
    /// timer (the kill family: Merciless/Deadhead/Dexterity).
    pub all_drop: bool,
    /// Stacks at t = 0 (arcane stacking buffs start FULL — user setting).
    pub initial_stacks: u32,
    /// AssumedMax: the stack count is pinned at max regardless of events.
    pub pinned: bool,
}

/// The flat arcane parameter block the simulator consumes — one arcane,
/// resolved at a rank under a stack policy. `Default` = no arcane (the
/// multiplier fields default to their 1.0 identity, NOT zero).
#[derive(Debug, Clone, PartialEq)]
pub struct ArcaneFx {
    pub id: String,
    /// Emergent stacking buffs (kill/status families).
    pub buffs: Vec<ArcBuffSpec>,
    /// Secondary Enervate: run the ramp/reset perk at this rank.
    pub enervate_rank: Option<u8>,
    /// Deadhead rank 5: joins the additive headshot bracket that multiplies
    /// the part multiplier.
    pub headshot_mult_bonus: f64,
    /// Static reload-speed bucket addition (Merciless rank 5).
    pub reload_bonus: f64,
    /// ABSOLUTE crit-chance add (assumed-max conditionals: Overcharge,
    /// Outburst) — base_cc × Σ relative bonuses.
    pub cc_abs: f64,
    /// ABSOLUTE crit-damage add (Outburst) — base_cd × Σ relative bonuses.
    pub cd_abs: f64,
    /// ABSOLUTE crit chance on weak-point hits only (Cascadia Accuracy,
    /// assumed-max).
    pub weakpoint_cc_abs: f64,
    /// Final damage multiplier on direct hits (Secondary Surge assumed-max:
    /// the cap, multiplicative with Hornet Strike). 1.0 = none.
    pub final_mult: f64,
    /// Secondary Shiver: +per per ACTIVE Cold status on the target (cap
    /// `cold_cap`), a GunCO-family source — applied per the weapon's
    /// CoBehavior bracket alongside Condition Overload.
    pub per_cold_bd: f64,
    pub cold_cap: u32,
    /// Cascadia Empowered: each status proc adds a flat damage instance of
    /// the proc's type (unaffected by mods/crit/parts; faction once; enemy
    /// mitigation applies).
    pub flat_damage_on_status: f64,
    /// Secondary Encumber: chance that a proc-carrying trigger pull adds ONE
    /// extra status of a uniformly random type (13-type pool, wiki), at most
    /// once per instant (= per trigger pull here).
    pub encumber_chance: f64,
    /// Secondary Cryogenic: Cold statuses applied to the target on each
    /// Puncture status (single-target: the radius burst collapses onto the
    /// main target, which the wiki confirms is also hit).
    pub cold_burst_on_puncture: u32,
    /// Secondary Fortifier: total damage multiplier while the target still
    /// has Overguard (x3..x8 in-game → stored as the multiplier). 1.0 = none.
    pub overguard_mult: f64,
    /// Akimbo Slip Shot under assumed-max (sliding/aim-gliding not simmed):
    /// added to BuffBar ammo efficiency. Gated on the `dual_pistols` trait.
    pub ammo_efficiency: f64,
}

impl Default for ArcaneFx {
    fn default() -> Self {
        Self {
            id: String::new(),
            buffs: Vec::new(),
            enervate_rank: None,
            headshot_mult_bonus: 0.0,
            reload_bonus: 0.0,
            cc_abs: 0.0,
            cd_abs: 0.0,
            weakpoint_cc_abs: 0.0,
            final_mult: 1.0,
            per_cold_bd: 0.0,
            cold_cap: 0,
            flat_damage_on_status: 0.0,
            encumber_chance: 0.0,
            cold_burst_on_puncture: 0,
            overguard_mult: 1.0,
            ammo_efficiency: 0.0,
        }
    }
}

impl ArcaneFx {
    pub fn none() -> Self {
        Self::default()
    }
}

/// A parsed arcane definition (rank-parameterized).
#[derive(Debug, Clone)]
pub struct ArcaneDef {
    pub id: String,
    pub name: String,
    pub rarity: Rarity,
    pub max_rank: u32,
    pub requires: Option<String>,
    /// Verbatim in-game text with rank-varying numbers as `X`.
    pub description: String,
    perk: Option<String>,
    effects: Vec<ArcEffect>,
}

/// One rank-parameterized arcane effect (the loader's vocabulary — every
/// structured kind in data/arcanes; kinds with no single-target sim payload
/// load as `Inert` so the arcane still resolves).
#[derive(Debug, Clone, PartialEq)]
enum ArcEffect {
    Buff {
        trigger: ArcTrigger,
        grant: ArcGrant,
        scale: Scale,
        max_stacks: u32,
        duration: f64,
        all_drop: bool,
    },
    /// Relative crit chance under a non-simmed condition (Overcharge).
    CondCritChance(Scale),
    /// Outburst: relative CC and CD per combo tier consumed (assumed-max =
    /// full 12x combo), duration-limited buff on a non-simmed trigger.
    CondCritChanceStacked { scale: Scale, max_stacks: u32 },
    CondCritDamageStacked { scale: Scale, max_stacks: u32 },
    /// Cascadia Accuracy: relative crit chance on weak-point hits (on-roll
    /// buff — assumed-max only).
    WeakpointCritChance(Scale),
    /// Surge: final damage-multiplier cap (stored as bonus; assumed-max).
    FinalDamageCap(Scale),
    /// Fractalized Reset: reload speed on a trigger the arena cannot fire (an
    /// ability cast). The GRANT is modeled, so it follows the house policy for
    /// non-simmed triggers — assumed-max only, a no-op under `Emergent`.
    CondReloadSpeed(Scale),
    HeadshotMultiplier { value: f64, unlocks_at: u32 },
    ReloadSpeed { value: f64, unlocks_at: u32 },
    PerColdDamage { scale: Scale, max_stacks: u32 },
    FlatDamageOnStatus(Scale),
    EncumberChance(Scale),
    ColdBurst { scale: Scale, radius0: f64, radius1: f64 },
    OverguardDamage(Scale),
    AmmoEfficiency(Scale),
    /// Kinship: per ally-affecting buff — team context, uncapped: inert in
    /// the sim, but its per-rank value still renders in the description.
    PerAllyCritChance(Scale),
    /// Irradiate: % of the hit damage echoed in a radius — AoE, inert in
    /// the single-target sim; values render in the description.
    AoeEcho { scale: Scale, radius0: f64, radius1: f64 },
    /// `kind: unmodeled` — an effect whose payload is OUT OF THE SIM'S WORLD
    /// (Warframe armor/energy, enemy behaviour, a mechanic still to be built).
    /// No sim payload, but it OWNS a description `X`: its per-rank value still
    /// has to render, or the config page shows a literal "X". The yaml `note`
    /// is what the panel says instead of a computed line.
    Unmodeled { note: String, scale: Scale },
    /// No single-target sim payload and NO description number of its own
    /// (recoil, combo duration, overguard-on-damage: the description states
    /// those literally). Kept so the arcane loads.
    Inert(String),
}

fn f(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}
fn u(v: &Value, k: &str) -> u32 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0) as u32
}
fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn scale(v: &Value) -> Scale {
    let ranks = v.get("ranks").and_then(Value::as_sequence).map(|seq| {
        seq.iter().filter_map(Value::as_f64).collect::<Vec<f64>>()
    });
    Scale {
        rank0: f(v, "rank0"),
        rank_max: f(v, "rankMax").unwrap_or(0.0),
        ranks,
    }
}

fn effect(v: &Value) -> Option<ArcEffect> {
    let kind = s(v, "kind")?;
    let inert = |why: &str| Some(ArcEffect::Inert(why.to_string()));
    Some(match kind {
        "buff" => {
            let trigger = s(v, "trigger")?;
            let grants = s(v, "grants")?;
            let all_drop = s(v, "decay") == Some("all_drop_on_timeout");
            let trig = match trigger {
                "on_kill" => ArcTrigger::Kill,
                "on_precision_headshot_kill" => ArcTrigger::HeadshotKill,
                "on_melee_kill" => ArcTrigger::MeleeKill,
                "on_heat_status" => ArcTrigger::HeatStatus,
                "on_electricity_status" => ArcTrigger::ElectricityStatus,
                "on_toxin_status" => ArcTrigger::ToxinStatus,
                "on_cold_status" => ArcTrigger::ColdStatus,
                "on_weakpoint_hit" => ArcTrigger::WeakpointHit,
                // Non-simmed triggers with modeled grants:
                "on_swap_consume_combo" => {
                    return Some(match grants {
                        "crit_chance" => ArcEffect::CondCritChanceStacked {
                            scale: scale(v),
                            max_stacks: u(v, "max_stacks"),
                        },
                        "crit_damage" => ArcEffect::CondCritDamageStacked {
                            scale: scale(v),
                            max_stacks: u(v, "max_stacks"),
                        },
                        other => ArcEffect::Inert(format!("on_swap grant {other}")),
                    })
                }
                "on_roll" if grants == "weakpoint_crit_chance" => {
                    return Some(ArcEffect::WeakpointCritChance(scale(v)))
                }
                "on_ability_cast" if grants == "final_damage" => {
                    return Some(ArcEffect::FinalDamageCap(scale(v)))
                }
                "on_ability_cast" if grants == "reload_speed" => {
                    return Some(ArcEffect::CondReloadSpeed(scale(v)))
                }
                // Enervate's on_hit buff is implemented by its perk.
                "on_hit" => return inert("perk-implemented (on_hit)"),
                other => return inert(&format!("trigger {other}")),
            };
            let grant = match grants {
                "base_damage" => ArcGrant::BaseDamage,
                "multishot" => ArcGrant::Multishot,
                "reload_speed" => ArcGrant::ReloadSpeed,
                "crit_damage" => ArcGrant::CritDamage,
                "status_chance" => ArcGrant::StatusChance,
                "ammo_efficiency" => ArcGrant::AmmoEfficiency,
                other => return inert(&format!("grant {other}")),
            };
            ArcEffect::Buff {
                trigger: trig,
                grant,
                scale: scale(v),
                max_stacks: u(v, "max_stacks"),
                duration: f(v, "duration").unwrap_or(0.0),
                all_drop,
            }
        }
        // Kinship carries `per: ally_buff` — team-context, uncapped: inert
        // in the sim; the value still renders in the description.
        "crit_chance_bonus" if s(v, "per").is_some() => ArcEffect::PerAllyCritChance(scale(v)),
        "crit_chance_bonus" => ArcEffect::CondCritChance(scale(v)),
        "headshot_multiplier_bonus" => ArcEffect::HeadshotMultiplier {
            value: f(v, "rankMax").unwrap_or(0.0),
            unlocks_at: u(v, "unlocks_at_rank"),
        },
        "reload_speed_bonus" => ArcEffect::ReloadSpeed {
            value: f(v, "rankMax").unwrap_or(0.0),
            unlocks_at: u(v, "unlocks_at_rank"),
        },
        "per_status_damage_bonus" => ArcEffect::PerColdDamage {
            scale: scale(v),
            max_stacks: u(v, "max_stacks"),
        },
        "flat_damage_on_status" => ArcEffect::FlatDamageOnStatus(scale(v)),
        "proc_conversion" => ArcEffect::EncumberChance(scale(v)),
        "proc_burst" => ArcEffect::ColdBurst {
            scale: scale(v),
            radius0: f(v, "radius_rank0").unwrap_or(0.0),
            radius1: f(v, "radius_rankMax").unwrap_or(0.0),
        },
        "aoe_echo" => ArcEffect::AoeEcho {
            scale: scale(v),
            radius0: f(v, "radius_rank0").unwrap_or(0.0),
            radius1: f(v, "radius_rankMax").unwrap_or(0.0),
        },
        "overguard_damage_bonus" => ArcEffect::OverguardDamage(scale(v)),
        "ammo_efficiency" => ArcEffect::AmmoEfficiency(scale(v)),
        "unmodeled" => ArcEffect::Unmodeled {
            note: s(v, "note").unwrap_or_default().to_string(),
            scale: scale(v),
        },
        other => return inert(other),
    })
}

impl ArcaneDef {
    /// Resolve this arcane at `rank` into the sim parameter block.
    ///
    /// `base_cc`/`base_cd` are the WEAPON's unmodded crit stats — the
    /// arcane's relative crit bonuses join the mod buckets, so their
    /// absolute value is base × bonus (same rule as Galvanized Crosshairs).
    /// `traits` gates `requires` (calc-layer inert, like mods).
    pub fn fx(
        &self,
        rank: u32,
        policy: StackPolicy,
        base_cc: f64,
        base_cd: f64,
        traits: &[&str],
    ) -> ArcaneFx {
        let rank = rank.min(self.max_rank);
        let mut fx = ArcaneFx {
            id: self.id.clone(),
            ..ArcaneFx::none()
        };
        if let Some(req) = &self.requires {
            if !traits.contains(&req.as_str()) {
                return fx; // required trait absent: effects inert, id kept
            }
        }
        if self.perk.as_deref() == Some("secondary_enervate") {
            fx.enervate_rank = Some(rank as u8);
        }
        let assumed = policy == StackPolicy::AssumedMax;
        for e in &self.effects {
            match e {
                ArcEffect::Buff { trigger, grant, scale, max_stacks, duration, all_drop } => {
                    if policy == StackPolicy::BaseOnly {
                        continue; // sentinel: conditional never fires
                    }
                    // A crit-damage grant is RELATIVE to the weapon's base
                    // crit damage (it joins that bucket), so it is resolved
                    // to an ABSOLUTE per-stack value here — the same
                    // treatment the static crit bonuses get below. Every
                    // other grant is already absolute or a plain ratio.
                    let per_stack = scale.at(rank, self.max_rank)
                        * match grant {
                            ArcGrant::CritDamage => base_cd,
                            _ => 1.0,
                        };
                    fx.buffs.push(ArcBuffSpec {
                        grant: *grant,
                        trigger: *trigger,
                        per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        all_drop: *all_drop,
                        initial_stacks: *max_stacks, // start full (user decision)
                        pinned: assumed,
                    });
                }
                ArcEffect::CondCritChance(sc) => {
                    if assumed {
                        fx.cc_abs += base_cc * sc.at(rank, self.max_rank);
                    }
                }
                ArcEffect::CondCritChanceStacked { scale, max_stacks } => {
                    if assumed {
                        fx.cc_abs += base_cc * scale.at(rank, self.max_rank) * *max_stacks as f64;
                    }
                }
                ArcEffect::CondCritDamageStacked { scale, max_stacks } => {
                    if assumed {
                        fx.cd_abs += base_cd * scale.at(rank, self.max_rank) * *max_stacks as f64;
                    }
                }
                ArcEffect::WeakpointCritChance(sc) => {
                    if assumed {
                        fx.weakpoint_cc_abs += base_cc * sc.at(rank, self.max_rank);
                    }
                }
                ArcEffect::FinalDamageCap(sc) => {
                    if assumed {
                        fx.final_mult = 1.0 + sc.at(rank, self.max_rank);
                    }
                }
                ArcEffect::CondReloadSpeed(sc) => {
                    if assumed {
                        fx.reload_bonus += sc.at(rank, self.max_rank);
                    }
                }
                ArcEffect::HeadshotMultiplier { value, unlocks_at } => {
                    if rank >= *unlocks_at {
                        fx.headshot_mult_bonus += value;
                    }
                }
                ArcEffect::ReloadSpeed { value, unlocks_at } => {
                    if rank >= *unlocks_at {
                        fx.reload_bonus += value;
                    }
                }
                ArcEffect::PerColdDamage { scale, max_stacks } => {
                    fx.per_cold_bd = scale.at(rank, self.max_rank);
                    fx.cold_cap = *max_stacks;
                }
                ArcEffect::FlatDamageOnStatus(sc) => {
                    fx.flat_damage_on_status = sc.at(rank, self.max_rank);
                }
                ArcEffect::EncumberChance(sc) => {
                    fx.encumber_chance = sc.at(rank, self.max_rank);
                }
                ArcEffect::ColdBurst { scale, .. } => {
                    fx.cold_burst_on_puncture = scale.at(rank, self.max_rank).round() as u32;
                }
                // Team-context / AoE / out-of-scope — no sim payload.
                ArcEffect::PerAllyCritChance(_)
                | ArcEffect::AoeEcho { .. }
                | ArcEffect::Unmodeled { .. } => {}
                ArcEffect::OverguardDamage(sc) => {
                    fx.overguard_mult = 1.0 + sc.at(rank, self.max_rank);
                }
                ArcEffect::AmmoEfficiency(sc) => {
                    if assumed {
                        fx.ammo_efficiency += sc.at(rank, self.max_rank);
                    }
                }
                ArcEffect::Inert(_) => {}
            }
        }
        fx
    }

    /// The verbatim in-game DESCRIPTION with its `X` placeholders filled at
    /// `rank` — what the config page shows (docs: description-X schema).
    ///
    /// `X`s map to the effects' display values in yaml order. Two derived
    /// cases close the gaps: adjacent effects sharing one number collapse
    /// (Outburst's cc+cd "by X% per Combo"), and a trailing "Stacks up to
    /// X%" cap is per_stack × max_stacks (Cascadia Flare).
    pub fn desc_at(&self, rank: u32) -> String {
        let rank = rank.min(self.max_rank);
        let at = |sc: &Scale| sc.at(rank, self.max_rank);
        let lerp = |a: f64, b: f64| a + (b - a) * rank as f64 / self.max_rank.max(1) as f64;
        let mut vals: Vec<f64> = Vec::new();
        if self.perk.as_deref() == Some("secondary_enervate") {
            vals.push((rank + 1) as f64); // "Resets after X Big Critical Hit"
        }
        for e in &self.effects {
            match e {
                ArcEffect::Buff { scale, .. }
                | ArcEffect::CondCritChance(scale)
                | ArcEffect::CondCritChanceStacked { scale, .. }
                | ArcEffect::CondCritDamageStacked { scale, .. }
                | ArcEffect::WeakpointCritChance(scale)
                | ArcEffect::PerColdDamage { scale, .. }
                | ArcEffect::FlatDamageOnStatus(scale)
                | ArcEffect::EncumberChance(scale)
                | ArcEffect::AmmoEfficiency(scale)
                | ArcEffect::PerAllyCritChance(scale)
                | ArcEffect::CondReloadSpeed(scale)
                // Out of the sim's world, but it still owns its `X`.
                | ArcEffect::Unmodeled { scale, .. }
                // Multiplier kinds ("xX"): stored as the bonus — fill_x's
                // xX rule renders the +1.
                | ArcEffect::FinalDamageCap(scale)
                | ArcEffect::OverguardDamage(scale) => vals.push(at(scale)),
                ArcEffect::ColdBurst { scale, radius0, radius1 }
                | ArcEffect::AoeEcho { scale, radius0, radius1 } => {
                    vals.push(at(scale));
                    vals.push(lerp(*radius0, *radius1));
                }
                ArcEffect::HeadshotMultiplier { .. }
                | ArcEffect::ReloadSpeed { .. }
                | ArcEffect::Inert(_) => {}
            }
        }
        let xs = count_x(&self.description);
        // Outburst: cc + cd share the single "by X% per Combo" number.
        while vals.len() > xs {
            let before = vals.len();
            vals.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
            if vals.len() == before {
                break;
            }
        }
        // Cascadia Flare: the trailing stack cap is per_stack × max_stacks.
        if vals.len() + 1 == xs && self.description.contains("Stacks up to X%") {
            if let Some(ArcEffect::Buff { scale, max_stacks, .. }) = self
                .effects
                .iter()
                .find(|e| matches!(e, ArcEffect::Buff { .. }))
            {
                vals.push(at(scale) * *max_stacks as f64);
            }
        }
        fill_x(&self.description, &vals)
    }

    /// Display lines at a rank — OUR statement of what the model computes
    /// (mirrors [`crate::loadout::ModEffect::describe`]).
    pub fn describe_at(&self, rank: u32) -> Vec<String> {
        let rank = rank.min(self.max_rank);
        let mut out = Vec::new();
        if self.perk.as_deref() == Some("secondary_enervate") {
            out.push("On Hit: +10% flat Crit Chance per stack".to_string());
            out.push(format!(
                "Resets after {} big crit{}",
                rank + 1,
                if rank == 0 { "" } else { "s" }
            ));
        }
        for e in &self.effects {
            let at = |sc: &Scale| sc.at(rank, self.max_rank);
            match e {
                ArcEffect::Buff { trigger, grant, scale, max_stacks, duration, all_drop } => {
                    let what = match grant {
                        ArcGrant::BaseDamage => "Base Damage",
                        ArcGrant::Multishot => "Multishot",
                        ArcGrant::ReloadSpeed => "Reload Speed",
                        ArcGrant::CritDamage => "Critical Damage",
                        ArcGrant::StatusChance => "Status Chance",
                        ArcGrant::AmmoEfficiency => "Ammo Efficiency",
                    };
                    let when = match trigger {
                        ArcTrigger::Kill => "On Kill",
                        ArcTrigger::HeadshotKill => "On Precision Headshot Kill",
                        ArcTrigger::MeleeKill => "On Melee Kill",
                        ArcTrigger::HeatStatus => "On Heat Status",
                        ArcTrigger::ElectricityStatus => "On Electricity Status",
                        ArcTrigger::ToxinStatus => "On Toxin Status",
                        ArcTrigger::ColdStatus => "On Cold Status",
                        ArcTrigger::WeakpointHit => "On Weak Point Hit",
                    };
                    let decay = if *all_drop { "all drop on timeout" } else { "lose one on timeout" };
                    out.push(format!(
                        "{when}: {} {what} per stack ×{max_stacks}, {duration}s ({decay})",
                        pct(at(scale))
                    ));
                }
                ArcEffect::CondCritChance(sc) => {
                    out.push(format!("{} Crit Chance (conditional)", pct(at(sc))));
                }
                ArcEffect::CondCritChanceStacked { scale, max_stacks } => out.push(format!(
                    "{} Crit Chance per combo consumed ×{max_stacks} (on swap)",
                    pct(at(scale))
                )),
                ArcEffect::CondCritDamageStacked { scale, max_stacks } => out.push(format!(
                    "{} Crit Damage per combo consumed ×{max_stacks} (on swap)",
                    pct(at(scale))
                )),
                ArcEffect::WeakpointCritChance(sc) => out.push(format!(
                    "On Roll: {} Crit Chance on weak-point hits",
                    pct(at(sc))
                )),
                ArcEffect::FinalDamageCap(sc) => out.push(format!(
                    "On Ability Cast: next shot ×{:.0} damage cap (0.5%/energy)",
                    1.0 + at(sc)
                )),
                ArcEffect::CondReloadSpeed(sc) => out.push(format!(
                    "On Ability Cast: {} Reload Speed (conditional)",
                    pct(at(sc))
                )),
                ArcEffect::HeadshotMultiplier { value, unlocks_at } => {
                    if rank >= *unlocks_at {
                        out.push(format!("{} to Headshot Multiplier", pct(*value)));
                    }
                }
                ArcEffect::ReloadSpeed { value, unlocks_at } => {
                    if rank >= *unlocks_at {
                        out.push(format!("{} Reload Speed", pct(*value)));
                    }
                }
                ArcEffect::PerColdDamage { scale, max_stacks } => out.push(format!(
                    "{} Damage per Cold status on the target (×{max_stacks})",
                    pct(at(scale))
                )),
                ArcEffect::FlatDamageOnStatus(sc) => out.push(format!(
                    "On Status: +{:.0} flat damage of the proc's type",
                    at(sc)
                )),
                ArcEffect::EncumberChance(sc) => out.push(format!(
                    "On Status: {} chance of one extra random status",
                    pct(at(sc))
                )),
                ArcEffect::ColdBurst { scale, .. } => out.push(format!(
                    "On Puncture: apply {:.0} Cold stacks",
                    at(scale)
                )),
                ArcEffect::PerAllyCritChance(sc) => out.push(format!(
                    "{} Crit Chance per ally-affecting buff (team context)",
                    pct(at(sc))
                )),
                ArcEffect::AoeEcho { scale, .. } => out.push(format!(
                    "{} of the hit damage echoed to nearby enemies (AoE)",
                    pct(at(scale))
                )),
                ArcEffect::OverguardDamage(sc) => out.push(format!(
                    "×{:.0} damage to Overguard",
                    1.0 + at(sc)
                )),
                ArcEffect::AmmoEfficiency(sc) => out.push(format!(
                    "{} ammo efficiency while sliding/aim gliding (Dual Pistols)",
                    pct(at(sc))
                )),
                // Say so, rather than silently listing nothing: the panel's
                // job is to state what the model does, and "this one is out of
                // scope" is part of that.
                ArcEffect::Unmodeled { note, .. } => {
                    out.push(format!("not modeled: {note}"));
                }
                ArcEffect::Inert(_) => {}
            }
        }
        out
    }
}

fn rarity(name: &str) -> Rarity {
    match name {
        "common" => Rarity::Common,
        "uncommon" => Rarity::Uncommon,
        "rare" => Rarity::Rare,
        "legendary" => Rarity::Legendary,
        other => panic!("unknown arcane rarity: {other}"),
    }
}

/// Load every embedded arcane yaml under a `data/` prefix (e.g.
/// `"arcanes/secondary/"`) into arcane definitions (sorted by id).
pub fn load_pool(prefix: &str) -> Vec<ArcaneDef> {
    let mut out = Vec::new();
    for (path, text) in crate::data::files_under(prefix) {
        let af: ArcaneFile =
            serde_norway::from_str(text).unwrap_or_else(|e| panic!("parse {path}: {e}"));
        let effects = af
            .effects
            .iter()
            .filter_map(effect)
            .collect();
        out.push(ArcaneDef {
            id: af.id,
            name: af.name,
            rarity: rarity(&af.rarity),
            max_rank: af.max_rank,
            requires: af.requires,
            description: af.description.unwrap_or_default(),
            perk: af.perk,
            effects,
        });
    }
    out
}

/// Every arcane SLOT present in the data — one per `data/arcanes/<slot>/`
/// directory, sorted. Same discovery rule as the mod classes: dropping in
/// `data/arcanes/primary/` publishes primary arcanes with no code change.
pub fn slots() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = crate::data::files_under("arcanes/")
        .filter_map(|(p, _)| p.strip_prefix("arcanes/")?.split('/').next())
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The arcane pool of one slot — `data/arcanes/<slot>/*.yaml`. Cached per
/// slot (each entry leaks once).
pub fn slot_pool(slot: &str) -> &'static [ArcaneDef] {
    static POOLS: OnceLock<Mutex<BTreeMap<String, &'static [ArcaneDef]>>> = OnceLock::new();
    let cache = POOLS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut g = cache.lock().expect("arcane pool cache");
    g.entry(slot.to_string())
        .or_insert_with(|| {
            Box::leak(load_pool(&format!("arcanes/{slot}/")).into_boxed_slice())
        })
}

/// Which slot an arcane id belongs to, if any.
pub fn slot_of(id: &str) -> Option<&'static str> {
    slots()
        .into_iter()
        .find(|s| slot_pool(s).iter().any(|a| a.id == id))
}

/// The secondary-arcane pool — `data/arcanes/secondary/*.yaml`.
pub fn secondary_pool() -> &'static [ArcaneDef] {
    slot_pool("secondary")
}

/// Look up an arcane by id across EVERY slot (ids are globally unique).
pub fn secondary(id: &str) -> Option<&'static ArcaneDef> {
    slots()
        .into_iter()
        .find_map(|s| slot_pool(s).iter().find(|a| a.id == id))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_TRAITS: &[&str] = &[];

    #[test]
    fn loads_all_18_secondary_arcanes() {
        let pool = secondary_pool();
        assert_eq!(pool.len(), 18, "expected the full 18-arcane pool");
    }

    #[test]
    fn loads_all_14_primary_arcanes() {
        let pool = slot_pool("primary");
        assert_eq!(pool.len(), 14, "expected the full 14-arcane primary pool");
        // The slot registry discovers directories, so primary must show up
        // next to secondary with no code change.
        assert!(slots().contains(&"primary"), "slots(): {:?}", slots());
    }

    /// Primary Blight is the Torid-relevant one: a Toxin weapon feeds it
    /// constantly. Two grants on ONE trigger, 40 stacks — the same shape as
    /// Conjunction Voltage — and the crit-damage grant must resolve to an
    /// ABSOLUTE value against the weapon's base crit damage.
    #[test]
    fn primary_blight_stacks_crit_damage_and_multishot_on_toxin() {
        let a = slot_pool("primary")
            .iter()
            .find(|x| x.id == "primary_blight")
            .expect("primary_blight");
        let base_cd = 3.1; // Torid Incarnon's crit damage
        let fx = a.fx(a.max_rank, StackPolicy::Emergent, 0.29, base_cd, NO_TRAITS);
        assert_eq!(fx.buffs.len(), 2, "crit damage + multishot");
        for b in &fx.buffs {
            assert_eq!(b.trigger, ArcTrigger::ToxinStatus);
            assert_eq!(b.max_stacks, 40);
            assert!(b.all_drop, "on-status family: ALL stacks drop on timeout");
        }
        let cd = fx
            .buffs
            .iter()
            .find(|b| b.grant == ArcGrant::CritDamage)
            .expect("crit damage grant");
        // +3.6% per stack is RELATIVE to base crit damage, so one stack is
        // worth 0.036 x 3.1 of absolute crit damage.
        assert!(
            (cd.per_stack - 0.036 * base_cd).abs() < 1e-9,
            "per stack {} vs {}",
            cd.per_stack,
            0.036 * base_cd
        );
        let ms = fx
            .buffs
            .iter()
            .find(|b| b.grant == ArcGrant::Multishot)
            .expect("multishot grant");
        assert!((ms.per_stack - 0.018).abs() < 1e-9, "multishot stays a ratio");
    }

    /// Primary Crux: two grants on a weak-point HIT, 10 stacks, all-drop.
    /// Both per-stack values stay plain RATIOS — the status-chance one is
    /// relative to the ATTACK PART's base, which only the sim knows (the
    /// explosion's differs), unlike `CritDamage`, resolved absolute here.
    #[test]
    fn primary_crux_grants_status_chance_and_ammo_efficiency_on_weakpoint_hits() {
        let a = secondary("primary_crux").expect("primary_crux");
        let fx = a.fx(a.max_rank, StackPolicy::Emergent, 0.29, 3.1, NO_TRAITS);
        assert_eq!(fx.buffs.len(), 2, "status chance + ammo efficiency");
        for b in &fx.buffs {
            assert_eq!(b.trigger, ArcTrigger::WeakpointHit);
            assert_eq!((b.max_stacks, b.all_drop), (10, true));
            assert!((b.duration - 10.0).abs() < 1e-9);
        }
        let g = |grant| fx.buffs.iter().find(|b| b.grant == grant).map(|b| b.per_stack);
        assert_eq!(g(ArcGrant::StatusChance), Some(0.3));
        assert_eq!(g(ArcGrant::AmmoEfficiency), Some(0.06));
        // Rank 0 (both ramps are linear from a fifth of max).
        let fx0 = a.fx(0, StackPolicy::Emergent, 0.29, 3.1, NO_TRAITS);
        assert!((fx0.buffs[0].per_stack - 0.05).abs() < 1e-9);
        assert!((fx0.buffs[1].per_stack - 0.01).abs() < 1e-9);
        // The static `ammo_efficiency` field belongs to the assumed-max
        // conditionals (Akimbo Slip Shot) — Crux's is a live buff, not that.
        assert_eq!(fx.ammo_efficiency, 0.0);
        assert_eq!(
            a.desc_at(5),
            "On Weak Point Hit: Gain +30% Status Chance and +6% Ammo Efficiency for 10s. Stacks up to 10x."
        );
    }

    /// The X-fill invariant, for the primary pool too (it holds for the
    /// secondary pool above): every rank of every arcane renders.
    #[test]
    fn primary_desc_at_fills_every_x() {
        for a in slot_pool("primary") {
            for r in 0..=a.max_rank {
                let d = a.desc_at(r);
                assert_eq!(
                    crate::loadout::count_x(&d),
                    0,
                    "{} rank {r}: unfilled X in {d:?}",
                    a.id
                );
            }
        }
    }

    #[test]
    fn merciless_resolves_kill_family_buff_plus_rank5_reload() {
        let a = secondary("secondary_merciless").unwrap();
        let fx = a.fx(5, StackPolicy::Emergent, 0.05, 2.0, NO_TRAITS);
        let b = &fx.buffs[0];
        assert_eq!(b.trigger, ArcTrigger::Kill);
        assert_eq!(b.grant, ArcGrant::BaseDamage);
        assert!((b.per_stack - 0.30).abs() < 1e-9);
        assert_eq!(b.max_stacks, 12);
        assert!(!b.all_drop); // kill family: lose one + reset
        assert!((fx.reload_bonus - 0.30).abs() < 1e-9);
        // Rank 4: no reload passive; per-stack 25% (linear).
        let fx4 = a.fx(4, StackPolicy::Emergent, 0.05, 2.0, NO_TRAITS);
        assert_eq!(fx4.reload_bonus, 0.0);
        assert!((fx4.buffs[0].per_stack - 0.25).abs() < 1e-9);
    }

    #[test]
    fn deadhead_and_flare_match_the_historical_hardcoded_specs() {
        let d = secondary("secondary_deadhead")
            .unwrap()
            .fx(5, StackPolicy::Emergent, 0.05, 2.0, NO_TRAITS);
        let b = &d.buffs[0];
        assert_eq!(b.trigger, ArcTrigger::HeadshotKill);
        assert!((b.per_stack - 1.20).abs() < 1e-9);
        assert_eq!((b.max_stacks, b.all_drop), (3, false));
        assert!((b.duration - 24.0).abs() < 1e-9);
        assert!((d.headshot_mult_bonus - 0.30).abs() < 1e-9);

        let fl = secondary("cascadia_flare")
            .unwrap()
            .fx(5, StackPolicy::Emergent, 0.05, 2.0, NO_TRAITS);
        let b = &fl.buffs[0];
        assert_eq!(b.trigger, ArcTrigger::HeatStatus);
        assert!((b.per_stack - 0.12).abs() < 1e-9);
        assert_eq!((b.max_stacks, b.all_drop), (40, true));
    }

    #[test]
    fn nonlinear_ranks_use_the_explicit_table() {
        // Kinship is inert (team context), but Cryogenic's stack table is
        // 1,1,2,2,3,3 — rank 3 must be 2, not a lerp of 1..3.
        let c = secondary("secondary_cryogenic").unwrap();
        assert_eq!(
            c.fx(3, StackPolicy::Emergent, 0.05, 2.0, NO_TRAITS).cold_burst_on_puncture,
            2
        );
    }

    #[test]
    fn assumed_max_only_conditionals_are_emergent_noops() {
        let o = secondary("cascadia_overcharge").unwrap();
        let em = o.fx(5, StackPolicy::Emergent, 0.10, 2.0, NO_TRAITS);
        assert_eq!(em.cc_abs, 0.0);
        let am = o.fx(5, StackPolicy::AssumedMax, 0.10, 2.0, NO_TRAITS);
        assert!((am.cc_abs - 0.10 * 3.0).abs() < 1e-9);

        // Surge: ×8 cap under AssumedMax, no-op under Emergent.
        let s = secondary("secondary_surge").unwrap();
        assert_eq!(s.fx(5, StackPolicy::Emergent, 0.1, 2.0, NO_TRAITS).final_mult, 1.0);
        assert!((s.fx(5, StackPolicy::AssumedMax, 0.1, 2.0, NO_TRAITS).final_mult - 8.0).abs() < 1e-9);
    }

    #[test]
    fn requires_gates_akimbo_on_the_dual_pistols_trait() {
        let a = secondary("akimbo_slip_shot").unwrap();
        let off = a.fx(5, StackPolicy::AssumedMax, 0.1, 2.0, NO_TRAITS);
        assert_eq!(off.ammo_efficiency, 0.0);
        let on = a.fx(5, StackPolicy::AssumedMax, 0.1, 2.0, &["dual_pistols"]);
        assert!((on.ammo_efficiency - 0.65).abs() < 1e-9);
    }

    #[test]
    fn shiver_fortifier_encumber_empowered_cryogenic_resolve() {
        let sh = secondary("secondary_shiver")
            .unwrap()
            .fx(5, StackPolicy::Emergent, 0.05, 2.0, NO_TRAITS);
        assert!((sh.per_cold_bd - 0.45).abs() < 1e-9);
        assert_eq!(sh.cold_cap, 10);
        let ft = secondary("secondary_fortifier")
            .unwrap()
            .fx(5, StackPolicy::Emergent, 0.05, 2.0, NO_TRAITS);
        assert!((ft.overguard_mult - 8.0).abs() < 1e-9);
        let en = secondary("secondary_encumber")
            .unwrap()
            .fx(5, StackPolicy::Emergent, 0.05, 2.0, NO_TRAITS);
        assert!((en.encumber_chance - 0.24).abs() < 1e-9);
        let em = secondary("cascadia_empowered")
            .unwrap()
            .fx(5, StackPolicy::Emergent, 0.05, 2.0, NO_TRAITS);
        assert!((em.flat_damage_on_status - 750.0).abs() < 1e-9);
        // Voltage: two status-family buffs sharing the 40-stack pool.
        let cv = secondary("conjunction_voltage")
            .unwrap()
            .fx(5, StackPolicy::Emergent, 0.05, 2.0, NO_TRAITS);
        assert_eq!(cv.buffs.len(), 2);
        assert!(cv.buffs.iter().all(|b| b.all_drop && b.max_stacks == 40));
    }

    #[test]
    fn desc_at_fills_every_x_for_the_whole_pool() {
        for a in secondary_pool() {
            for r in 0..=a.max_rank {
                let d = a.desc_at(r);
                assert_eq!(
                    crate::loadout::count_x(&d),
                    0,
                    "{} rank {r}: unfilled X in {d:?}",
                    a.id
                );
            }
        }
    }

    #[test]
    fn desc_at_spot_checks() {
        let d = |id: &str, r: u32| secondary(id).unwrap().desc_at(r);
        // Linear percent fill.
        assert_eq!(d("secondary_merciless", 5), "On Kill:\n+30% Damage for 4s. Stacks up to 12x.\n+30% Reload Speed");
        assert_eq!(d("secondary_merciless", 0), "On Kill:\n+5% Damage for 4s. Stacks up to 12x.\n+30% Reload Speed");
        // Flare: per-stack AND the derived stack cap (per × 40).
        assert_eq!(d("cascadia_flare", 0), "On Heat Status Effect:\n+2% Damage for 10s. Stacks up to 80%.");
        assert_eq!(d("cascadia_flare", 5), "On Heat Status Effect:\n+12% Damage for 10s. Stacks up to 480%.");
        // Voltage: two Xs from the two buffs, in order.
        assert_eq!(d("conjunction_voltage", 5), "On Electricity Status Effect:\n+1.5% Reload Speed and +3% Multishot for 12s. Stacks up to 40x.");
        // Outburst: cc + cd collapse onto the single X (non-linear table).
        assert_eq!(d("secondary_outburst", 3), "On swapping to Secondary Weapon, consume all Combo Multipliers to increase Secondary Weapon Critical Chance and Critical Damage by 12% per Combo consumed for 30s.");
        // Cryogenic: non-linear stack table + radius lerp ("X ... Xm").
        assert_eq!(d("secondary_cryogenic", 2), "On Puncture: Apply 2 Cold stacks on targets within 12m.");
        // Multiplier form xX: stored bonus renders as the multiplier.
        assert_eq!(d("secondary_surge", 5), "On Ability Cast: Next shot gains a Damage Multiplier for every 200 current Energy, up to x8.");
        assert_eq!(d("secondary_fortifier", 0), "Gain 1 Overguard for every 100 Damage dealt to an enemy's Overguard.\nDeals x3 Extra Damage to Overguard.");
        // Enervate: the perk's reset threshold is the only varying number.
        assert_eq!(d("secondary_enervate", 3), "On Hit: Increase Critical Chance by 10%. Resets after 4 Big Critical Hit.");
        // Flat (non-%) fill.
        assert_eq!(d("cascadia_empowered", 5), "On Status Effect:\nDeals +750 Damage matching the Damage Type of the Status Effect");
    }

    #[test]
    fn enervate_runs_as_the_perk() {
        let e = secondary("secondary_enervate")
            .unwrap()
            .fx(3, StackPolicy::Emergent, 0.05, 2.0, NO_TRAITS);
        assert_eq!(e.enervate_rank, Some(3));
        assert!(e.buffs.is_empty()); // the on_hit buff is perk-implemented
    }
}

#[cfg(test)]
mod slot_tests {
    use super::*;

    /// Arcane pools are DISCOVERED from `data/arcanes/<slot>/`, mirroring the
    /// mod classes: adding `data/arcanes/primary/` is a data change, not a
    /// code change. Ids stay globally unique, so a lookup never needs a slot.
    #[test]
    fn slots_come_from_the_data_tree() {
        let ss = slots();
        assert!(ss.contains(&"secondary"), "expected the secondary slot, got {ss:?}");
        for s in &ss {
            assert!(!slot_pool(s).is_empty(), "slot {s} has no arcanes");
        }
        assert!(slot_pool("no_such_slot").is_empty());
        assert_eq!(slot_of("secondary_merciless"), Some("secondary"));
        assert_eq!(slot_of("no_such_arcane"), None);
        // Every id is unique across slots — what makes `secondary(id)` safe.
        let mut ids: Vec<&str> = ss.iter().flat_map(|s| slot_pool(s).iter().map(|a| a.id.as_str())).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "arcane ids collide across slots");
    }
}
