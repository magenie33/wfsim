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

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

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
    polarity: String,
    rarity: String,
    base_drain: u32,
    max_rank: u32,
    /// Verbatim in-game text, rank-varying numbers as `X` (schema).
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    exilus: bool,
    #[serde(default)]
    family: Option<String>,
    /// Mod SET membership — the bonus itself lives in `data/mod_sets/`.
    #[serde(default)]
    set: Option<String>,
    /// Weapon property required to EQUIP this mod ("continuous").
    #[serde(default)]
    requires_weapon: Option<String>,
    /// WEAPON IDS this mod may be equipped on, and nothing else. Distinct from
    /// `requires_weapon`, which names a PROPERTY several weapons can share:
    /// this names the weapons themselves, because some mods are written for
    /// exactly one ("Can equip the Ocucor-exclusive Sentient Surge mod").
    #[serde(default)]
    exclusive_to: Vec<String>,
    /// DE's own INCOMPATIBILITY tags, lowercased ("sentinel_weapon",
    /// "power_weapon") — the mirror of `requires_weapon`. NOT the existing
    /// `incompatible_with:` key, which names other MODS and duplicates
    /// `family`; this one names weapon KINDS.
    #[serde(default)]
    excludes_weapon: Vec<String>,
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
/// Any YAML number, integer or float. `as_f64` alone returns None for a plain
/// integer scalar, which is why `duration: 9` silently read as absent and left
/// a literal "X" in the rendered description.
fn n(v: &Value, k: &str) -> Option<f64> {
    let x = v.get(k)?;
    x.as_f64().or_else(|| x.as_i64().map(|i| i as f64))
}

/// A buff's `grants:` naming an INDIRECT stat rather than a damage bucket.
///
/// Both spellings of recoil are here because the data has both: a standalone
/// `kind: recoil_reduction` and a buff granting `recoil`. They mean the same
/// stat and the same sign convention — a reduction is stored NEGATIVE, which
/// every recoil mod in `data/` already does.
fn indirect_grant(grants: &str) -> Option<IndirectStat> {
    Some(match grants {
        "recoil" | "recoil_reduction" => IndirectStat::Recoil,
        "accuracy" => IndirectStat::Accuracy,
        "noise" => IndirectStat::Noise,
        "zoom" => IndirectStat::Zoom,
        "ammo_max" => IndirectStat::AmmoMax,
        "projectile_speed" => IndirectStat::ProjectileSpeed,
        "holstered_reload" => IndirectStat::HolsteredReload,
        "dodge_speed" => IndirectStat::DodgeSpeed,
        "acrobatic_speed" => IndirectStat::AcrobaticSpeed,
        "punch_through" => IndirectStat::PunchThrough,
        "range" => IndirectStat::Range,
        "beam_range" => IndirectStat::BeamRange,
        "beam_range_percent" => IndirectStat::BeamRangePercent,
        "movement_speed" => IndirectStat::MovementSpeed,
        "sprint_speed" => IndirectStat::SprintSpeed,
        // TOME MODS — see the enum for why each is its own bucket.
        "ability_strength" => IndirectStat::AbilityStrength,
        "ability_duration" => IndirectStat::AbilityDuration,
        "ability_efficiency" => IndirectStat::AbilityEfficiency,
        "energy_regen" => IndirectStat::EnergyRegen,
        "ally_buff" => IndirectStat::AllyBuff,
        "strip_on_kill" => IndirectStat::StripOnKill,
        "orb_drop" => IndirectStat::OrbDrop,
        _ => return None,
    })
}

/// Map one YAML effect entry to a [`ModEffect`] at max rank (None = no damage
/// effect / not modeled — the mod still loads).
/// `condition:` values that name a PLAYER STATE. Each maps to a
/// [`TennoCondition`], which resolve asks of the fight's Tenno — so the mod
/// pays exactly when the player is doing the thing. `while_aiming` is one of
/// these, not a case beside them: a card gates on aim the same way it gates on
/// invisibility, and there is one place to look for either (user, 2026-08-02).
///
/// An unrecognised string gates nothing, which the mod-condition test catches
/// as "the card states a condition, the model has none".
fn tenno_condition(cond: Option<&str>) -> Option<crate::loadout::TennoCondition> {
    match cond? {
        "while_aiming" => Some(crate::loadout::TennoCondition::Aiming),
        "while_invisible" => Some(crate::loadout::TennoCondition::Invisible),
        "while_airborne" => Some(crate::loadout::TennoCondition::Airborne),
        _ => None,
    }
}

/// A buff's `trigger:` naming an EVENT the sim already fires. One line per
/// trigger, and adding one here is the whole cost of a mod that stacks on it —
/// see [`crate::loadout::ModEffect::GrantsStackingBuff`].
fn buff_trigger(name: &str) -> Option<crate::loadout::BuffTrigger> {
    use crate::loadout::BuffTrigger as T;
    Some(match name {
        "on_hit" => T::Hit,
        "on_plain_hit" => T::PlainHit,
        "on_headshot" => T::Headshot,
        "on_consecutive_headshot" => T::ConsecutiveHeadshot,
        "on_kill" => T::Kill,
        "on_firing" => T::Firing,
        "on_status_applied" => T::StatusApplied,
        "on_full_burst" => T::FullBurst,
        "on_reload" => T::ReloadComplete,
        "on_reload_from_empty" => T::ReloadFromEmpty,
        _ => return None,
    })
}

/// A buff's `grants:` naming a BRACKET. The multishot spellings are three
/// because the brackets are three — see [`crate::loadout::BuffGrant`].
fn buff_grant(name: &str) -> Option<crate::loadout::BuffGrant> {
    use crate::loadout::BuffGrant as G;
    Some(match name {
        "multishot" => G::MultishotPercent,
        "flat_multishot" => G::Multishot,
        "base_multishot" => G::BaseMultishot,
        "base_damage" | "damage" => G::BaseDamage,
        "flat_base_damage" => G::FlatBaseDamage,
        "base_crit_damage" => G::BaseCritDamage,
        "headshot_damage" => G::HeadshotDamage,
        "fire_rate" => G::FireRate,
        "reload_speed" => G::ReloadSpeed,
        _ => return None,
    })
}

fn buff_decay(name: Option<&str>) -> crate::loadout::BuffDecay {
    use crate::loadout::BuffDecay as D;
    match name {
        Some("all_at_once") => D::AllAtOnce,
        Some("per_stack_expiry") => D::PerStackExpiry,
        // The Galvanized family's, which is what every buff written before the
        // third decay model was implemented does.
        _ => D::LoseOneAndReset,
    }
}

fn effect(id: &str, v: &Value) -> Option<ModEffect> {
    let kind = v.get("kind").and_then(Value::as_str)?;
    let max = |k: &str| f(v, k).unwrap_or(0.0);
    // `condition:` gates ANY effect, not only a triggered one. `while_aiming`
    // has its own wrapper (it predates the Tenno); every other player state is
    // a `TennoCondition`, asked of `data/tenno/` at resolve time.
    // Critical Focus is a flat crit bonus that simply does not exist unless
    // you are aiming — there is no event to wait for, so `kind: buff` (which
    // requires a trigger) cannot say it. The wrapper already existed; only
    // the data path was missing. The `buff` arm reads the same key itself,
    // for the effect it builds, and is skipped here so nothing double-wraps.
    let cond = v.get("condition").and_then(Value::as_str);
    // A `kind: buff` reads its own condition below (it wraps what the trigger
    // resolves to); every other kind wraps here.
    let tenno_cond = if kind == "buff" { None } else { tenno_condition(cond) };
    let out = match kind {
        "base_damage_bonus" => ModEffect::BaseDamage(max("rankMax")),
        "multishot_bonus" => ModEffect::Multishot(max("rankMax")),
        "crit_chance_bonus" => ModEffect::CritChance(max("rankMax")),
        "crit_damage_bonus" => ModEffect::CritDamage(max("rankMax")),
        "status_chance_bonus" => ModEffect::StatusChance(max("rankMax")),
        "status_damage_bonus" => ModEffect::StatusDamage(max("rankMax")),
        // Hunter Munitions / Internal Bleeding: a Slash status rolled off a
        // CRITICAL hit, independently of status chance.
        "slash_on_crit" => ModEffect::SlashOnCrit(max("rankMax")),
        // DOUBLE TAP. The rank table moves BOTH halves — 5%/80x at rank 0 and
        // 20%/20x at rank 3 — and the product is +400% at every rank, so the
        // per-stack value and the cap are read together and neither alone.
        "consecutive_hit_damage" => ModEffect::ConsecutiveHitDamage {
            per_stack: max("rankMax"),
            max_stacks: v.get("max_stacks").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
            duration: v.get("duration_seconds").and_then(|x| x.as_f64()).unwrap_or(0.0),
        },
        // HATA-SATYA. Read together for the same reason Double Tap's pair is:
        // the cap is what the rate is worth, not a separate fact. Here the
        // cap is the one thing rank does NOT move — "capped at 500% at all mod
        // ranks" — so the yaml states it and the rate ladders under it.
        //
        // `max_bonus` AND NOT `max_stacks`, which is the difference between a
        // ceiling DE published and one we computed from it: a stack count would
        // have to be re-derived at every rank, and at rank 0 it is 2,500 rather
        // than the 417 the card's own rate suggests.
        "crit_chance_per_hit" => ModEffect::CritChancePerHit(crate::loadout::CritPerHit {
            per_stack: max("rankMax"),
            max_bonus: f(v, "max_bonus").unwrap_or(0.0),
        }),
        // EXIMUS ADVANTAGE. The duration is fixed at every rank, so it is a
        // plain number beside the ladder.
        "eximus_weakpoint_damage" => ModEffect::OnEximusWeakpointDamage {
            bonus: max("rankMax"),
            duration: v.get("duration_seconds").and_then(|x| x.as_f64()).unwrap_or(0.0),
        },
        // SYNTH CHARGE. Its own multiplier on the magazine's last round — see
        // `ModEffect::LastRoundDamage` for the three things that switch it off.
        // A MOD THAT GRANTS A LIVE STACKING BUFF, in the vocabulary the weapon
        // perks already speak. Deliberately its own kind rather than a new arm
        // on `kind: buff`: that one contributes at the ASSUMED MAX through
        // `CondBuff`, which is right for a card whose trigger the sim has no
        // event for and wrong the moment it does — so opting in per mod is what
        // keeps every existing card exactly where it was.
        //
        // The buff's ID is the MOD's, leaked once. It is the key the card, the
        // replay curve, the stack config and the sampler all share, so deriving
        // it is what stops those four from drifting.
        "stacking_buff" => ModEffect::GrantsStackingBuff(crate::loadout::StackingBuff {
            id: Box::leak(id.to_string().into_boxed_str()),
            trigger: buff_trigger(v.get("trigger").and_then(Value::as_str)?)?,
            grant: buff_grant(v.get("grants").and_then(Value::as_str)?)?,
            per_stack: max("rankMax"),
            max_stacks: u(v, "max_stacks").max(1),
            duration: n(v, "duration").unwrap_or(0.0),
            chance: n(v, "chance").unwrap_or(1.0),
            decay: buff_decay(v.get("decay").and_then(Value::as_str)),
            initial_stacks: 0,
            stacks_per_trigger: 1,
            per_shell: false,
            cleared_by: crate::loadout::ClearedBy::Nothing,
            // Read here as well, so a MOD that states it needs no second edit
            // — no mod does today; the two that claim it are evolutions.
            card_opens_full: v
                .get("card_opens_full")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }),
        // DEGREES, not an accuracy fraction — see `ModEffect::AddedSpread`.
        // `max_stacks` multiplies it, the same assumed-max reading a `kind:
        // buff` with an indirect grant already takes: a build carrying this
        // mod is played at its cap.
        "added_spread" => {
            ModEffect::AddedSpread(max("rankMax") * f64::from(u(v, "max_stacks").max(1)))
        }
        "last_round_damage" => ModEffect::LastRoundDamage(max("rankMax")),
        "first_round_damage" => ModEffect::FirstRoundDamage(max("rankMax")),
        "fire_rate_bonus" => ModEffect::FireRate(max("rankMax")),
        "reload_speed_bonus" => ModEffect::ReloadSpeed(max("rankMax")),
        "magazine_capacity_bonus" => ModEffect::MagazineCapacity(max("rankMax")),
        "blast_radius_bonus" => ModEffect::BlastRadius(max("rankMax")),
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
            // The condition wraps whatever this buff resolves to, so the
            // fight's Tenno decides whether it arms at all.
            let tenno_cond = tenno_condition(v.get("condition").and_then(Value::as_str));
            let per = max("rankMax"); // per-stack value at max rank
            let stacks = u(v, "max_stacks");
            let dur = f(v, "duration").unwrap_or(0.0);
            let wrap = |e: ModEffect| match tenno_cond {
                Some(c) => ModEffect::WhileTenno(c, Box::new(e)),
                None => e,
            };
            wrap(match (trigger, grants) {
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
                // Sharpened Bullets / Pressurized Magazine: the sim has kill
                // and reload events, so these run emergently (the while_aiming
                // condition is satisfied — the sim assumes constant aiming).
                // SENTIENT SURGE — one card, three numbers, so one effect.
                // The trigger word is `per_tendril` because that is what the
                // bonus scales with; it is not an EVENT like the others in
                // this table, and calling it `on_kill` would have been the
                // easy lie (kills spawn tendrils, but a reload takes them all
                // away without a kill anywhere).
                ("per_tendril", "crit_and_status") => {
                    ModEffect::PerTendril { crit_chance: per, status_chance: per }
                }
                ("on_kill", "magazine_refill") => ModEffect::MagazineRefillOnKill(per),
                ("on_kill", "crit_damage") => {
                    ModEffect::OnKillCritDamage { bonus: per, duration: dur }
                }
                // "On Reload From Empty: +X% Damage" — its own event, because
                // the window opens when the RELOAD COMPLETES and a CondBuff
                // would have to pretend it is always on.
                ("on_reload", "base_damage") | ("on_reload", "damage") => {
                    ModEffect::OnReloadDamage { bonus: per, duration: dur }
                }
                ("on_reload", "fire_rate") => {
                    ModEffect::OnReloadFireRate { bonus: per, duration: dur }
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
                        "reload_speed" => CondBucket::ReloadSpeed,
                        // An INDIRECT grant used to hit `return None` here,
                        // which threw the number away and left three mods
                        // (Twitch, Reflex Draw, Targeting Subsystem) loading
                        // with no effects at all. `CondBucket` is damage
                        // buckets only, so route these to the indirect
                        // bucket instead — flat, like every other indirect
                        // stat. The trigger stays on the card; a stat with no
                        // damage payload has nothing to gate in this sim, and
                        // the 2D world wants the magnitude either way.
                        // `wrap`, not a bare return: Targeting Subsystem is
                        // `condition: while_aiming`, and skipping the wrapper
                        // would report it on the panel as an unconditional
                        // stat change — the exact thing the buff shape exists
                        // to prevent. The outer `aim_gated` is false for
                        // `kind: buff`, so this cannot double-wrap.
                        _ => {
                            let stat = indirect_grant(grants)?;
                            let v = per * stacks.max(1) as f64;
                            return Some(wrap(ModEffect::Indirect(stat, v)));
                        }
                    };
                    ModEffect::CondBuff(bucket, per * stacks.max(1) as f64)
                }
            })
        }
        // Weak-point effects (Pistol Acuity): conditional on the part hit.
        "weakpoint_damage_bonus" => ModEffect::WeakpointDamage(max("rankMax")),
        "weakpoint_crit_chance_bonus" => ModEffect::WeakpointCritChance(max("rankMax")),
        // Hemorrhage: `trigger` status rolls `rankMax` to also apply the
        // `applies` status; `condition: fire_rate_below_<x>` doubles it.
        "proc_conversion" => {
            let from = element(v.get("trigger").and_then(Value::as_str)?)?;
            let to = element(v.get("applies").and_then(Value::as_str)?)?;
            let (threshold, mult) = match v.get("condition").and_then(Value::as_str) {
                Some(c) if c.starts_with("fire_rate_below_") => (
                    c["fire_rate_below_".len()..].parse().ok()?,
                    f(v, "condition_multiplier").unwrap_or(1.0),
                ),
                _ => (0.0, 1.0),
            };
            ModEffect::ProcConversion {
                from,
                to,
                chance: max("rankMax"),
                low_rate_threshold: threshold,
                low_rate_multiplier: mult,
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
        // TOME MODS. Each carries its real number into the panel and pays
        // nothing, which is what `IndirectStat` is for — see the enum for why
        // the three ability stats are three buckets and not one.
        "ability_strength_bonus" => {
            ModEffect::Indirect(IndirectStat::AbilityStrength, max("rankMax"))
        }
        "ability_duration_bonus" => {
            ModEffect::Indirect(IndirectStat::AbilityDuration, max("rankMax"))
        }
        "ability_efficiency_bonus" => {
            ModEffect::Indirect(IndirectStat::AbilityEfficiency, max("rankMax"))
        }
        "energy_regen_bonus" => ModEffect::Indirect(IndirectStat::EnergyRegen, max("rankMax")),
        "ally_buff" => ModEffect::Indirect(IndirectStat::AllyBuff, max("rankMax")),
        "strip_on_kill" => ModEffect::Indirect(IndirectStat::StripOnKill, max("rankMax")),
        "orb_drop_chance" => ModEffect::Indirect(IndirectStat::OrbDrop, max("rankMax")),
        "zoom_bonus" => ModEffect::Indirect(IndirectStat::Zoom, max("rankMax")),
        "accuracy_bonus" => ModEffect::Indirect(IndirectStat::Accuracy, max("rankMax")),
        // 2D groundwork (2026-08-01): these were `kind: unmodeled`, i.e. the
        // mod equipped and the number was thrown away. They carry no
        // SINGLE-TARGET damage, which is what `Indirect` is for.
        "range_bonus" => ModEffect::Indirect(IndirectStat::Range, max("rankMax")),
        // NIGHTWATCH NAPALM: the mod LEAVES A FIELD. Every number the field
        // needs is stated here rather than borrowed from the weapon, because
        // this fire is not the rocket's — see `ModEffect::GrantsLingering`.
        "grants_lingering" => {
            let mut vector = crate::damage::DamageVector::new();
            for (k, val) in v.get("damage")?.as_mapping()? {
                vector.add(crate::weapons_data::damage_type(k.as_str()?), val.as_f64()?);
            }
            let field = Box::leak(Box::new(crate::loadout::LingeringBase {
                base_vector: vector,
                base_crit_chance: n(v, "crit_chance").unwrap_or(0.0),
                base_crit_damage: n(v, "crit_multiplier").unwrap_or(1.0),
                base_status_chance: n(v, "status_chance").unwrap_or(0.0),
                tick_rate: n(v, "tick_rate")?,
                duration_seconds: n(v, "duration_seconds")?,
                // Overwritten at resolve time from the weapon's own blast.
                radius_m: 0.0,
                falloff_start_m: n(v, "falloff_start_m").unwrap_or(0.0),
                falloff_reduction: n(v, "falloff_reduction").unwrap_or(0.0),
                stacking: match v.get("stacking").and_then(Value::as_str) {
                    Some("refresh") => crate::loadout::FieldStacking::Refresh,
                    _ => crate::loadout::FieldStacking::Stack,
                },
                takes_condition_overload: v
                    .get("takes_condition_overload")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                // A MOD-GRANTED field says so the same way a weapon's does.
                can_crit: v.get("can_crit").and_then(Value::as_bool).unwrap_or(true),
                elemental_mods_apply: v
                    .get("elemental_mods_apply")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                status_mods_apply: v
                    .get("status_mods_apply")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
            }));
            ModEffect::GrantsLingering(field)
        }
        // …and the share of the blast AREA it covers, its own column on the
        // card and therefore its own effect.
        "lingering_area_fraction" => ModEffect::LingeringAreaFraction(max("rankMax")),
        // ACID SHELLS: the corpse explosion. Three numbers off one ladder,
        // read together because none of them means anything alone.
        "acid_shells_flat_damage" => {
            ModEffect::AcidShells(crate::loadout::AcidShellsPart::FlatDamage(max("rankMax")))
        }
        "acid_shells_health_fraction" => {
            ModEffect::AcidShells(crate::loadout::AcidShellsPart::HealthFraction(max("rankMax")))
        }
        "acid_shells_radius_m" => {
            ModEffect::AcidShells(crate::loadout::AcidShellsPart::RadiusM(max("rankMax")))
        }
        // HARKONAR SCOPE: seconds onto the sniper combo's decay window.
        "combo_duration_bonus" => ModEffect::ComboDuration(n(v, "duration_seconds")?),
        "beam_range_bonus" => ModEffect::Indirect(IndirectStat::BeamRange, max("rankMax")),
        // …and the PERCENTAGE half, which is a different bucket because it
        // lands in a different place — see `range_m` in `loadout::resolve`.
        "beam_range_percent" => {
            ModEffect::Indirect(IndirectStat::BeamRangePercent, max("rankMax"))
        }
        "movement_speed_bonus" => ModEffect::Indirect(IndirectStat::MovementSpeed, max("rankMax")),
        "sprint_speed_bonus" => ModEffect::Indirect(IndirectStat::SprintSpeed, max("rankMax")),
        "ammo_conversion" => ModEffect::Indirect(IndirectStat::AmmoConversion, max("rankMax")),
        "stagger_resist_bonus" => ModEffect::Indirect(IndirectStat::StaggerResist, max("rankMax")),
        "self_stagger_reduction" => ModEffect::Indirect(IndirectStat::SelfStagger, max("rankMax")),
        "double_jump_refresh" => ModEffect::Indirect(IndirectStat::DoubleJump, max("rankMax")),
        "explosion_on_kill" => ModEffect::Indirect(IndirectStat::KillExplosion, max("rankMax")),
        // A syndicate augment's radial scale ("+1 Truth"). Its damage is
        // real; its TRIGGER counts affinity, which the sim does not track.
        // A syndicate augment names one of the six effects; its payload lives
        // in data/syndicates/ and is looked up there.
        "syndicate_radial" => ModEffect::SyndicateRadial {
            syndicate: Box::leak(
                v.get("syndicate")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
                    .into_boxed_str(),
            ),
            amount: max("rankMax"),
        },
        "status_spread_chance" => ModEffect::Indirect(IndirectStat::StatusSpread, max("rankMax")),
        // NOT indirect: a CHARGE-rate bonus shortens the draw, and a charged
        // form's cadence IS its draw (`ChargeCadence`), so this is DPS. It is
        // its own bucket rather than `fire_rate_bonus` because Shell Rush says
        // "Charge Rate" — it must not also speed up an uncharged form.
        "charge_rate_bonus" => ModEffect::ChargeRate(max("rankMax")),
        // Reflex Draw: on swap-in, −recoil/+accuracy for a few seconds.
        "on_equip_buff" => ModEffect::OnEquipHandling {
            recoil: -max("rankMax").abs(),
            accuracy: max("rankMax").abs(),
            duration: f(v, "duration_seconds").unwrap_or(0.0),
        },
        // Scoping markers (weapon_scoped) or an effect not yet modeled:
        // load the mod without this effect.
        _ => return None,
    };
    Some(match tenno_cond {
        Some(c) => ModEffect::WhileTenno(c, Box::new(out)),
        None => out,
    })
}

fn to_moddef(mf: ModFile) -> ModDef {
    let effects = mf.effects.iter().filter_map(|e| effect(&mf.id, e)).collect();
    // WHAT WE KNOWINGLY DO NOT MODEL, kept rather than dropped. An `unmodeled`
    // effect returns None from `effect` and vanishes, so a mod carrying only
    // one loads as a mod that does nothing and says nothing — which is exactly
    // how it looks to a player who equips it and sees no change (reported
    // 2026-08-05 about Primary Debilitate; 12 mods and 5 arcanes are in this
    // state). The note travels so the card can admit it.
    let has = |k: &str| {
        mf.effects
            .iter()
            .any(|e| e.get("kind").and_then(Value::as_str) == Some(k))
    };
    let unmodeled = has("unmodeled");
    let out_of_scope = has("out_of_scope");
    ModDef {
        unmodeled,
        out_of_scope,
        id: Box::leak(mf.id.into_boxed_str()),
        name: Box::leak(mf.name.into_boxed_str()),
        // ModDef.base_drain is the drain at the EQUIPPED (max) rank: drain
        // rises by 1 per rank from the rank-0 `base_drain`, so max = base + rank.
        base_drain: mf.base_drain + mf.max_rank,
        max_rank: mf.max_rank,
        polarity: polarity(&mf.polarity),
        rarity: rarity(&mf.rarity),
        exilus: mf.exilus,
        family: mf.family.map(|s| &*Box::leak(s.into_boxed_str())),
        set: mf.set.map(|s| &*Box::leak(s.into_boxed_str())),
        requires_weapon: mf.requires_weapon.map(|s| &*Box::leak(s.into_boxed_str())),
        exclusive_to: Box::leak(
            mf.exclusive_to
                .into_iter()
                .map(|s| &*Box::leak(s.into_boxed_str()))
                .collect::<Vec<&'static str>>()
                .into_boxed_slice(),
        ),
        excludes_weapon: mf
            .excludes_weapon
            .into_iter()
            .map(|s| &*Box::leak(s.into_boxed_str()))
            .collect(),
        requires: mf.requires.map(|s| &*Box::leak(s.into_boxed_str())),
        disables: mf
            .disables
            .into_iter()
            .map(|s| &*Box::leak(s.into_boxed_str()))
            .collect(),
        effects,
    }
}

/// Load a weapon class's embedded mod pool — `data/mods/<class>/*.yaml`
/// (each class gets its own subfolder so the flat pool doesn't get muddled
/// as the mod count grows). Sorted by file path, i.e. by id.
pub fn load_class(class: &str) -> Vec<ModDef> {
    crate::data::files_under(&format!("mods/{class}/"))
        .map(|(path, text)| {
            let mf: ModFile =
                serde_norway::from_str(text).unwrap_or_else(|e| panic!("parse {path}: {e}"));
            to_moddef(mf)
        })
        .collect()
}

/// Every mod CLASS present in the data — one per `data/mods/<class>/`
/// directory, sorted. The registry publishes a pool per class, so adding
/// `data/mods/rifle/` is enough to make rifle mods reachable: no code.
pub fn classes() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = crate::data::files_under("mods/")
        .filter_map(|(p, _)| p.strip_prefix("mods/")?.split('/').next())
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The mod pool of one class — `data/mods/<class>/*.yaml`. Cached per class
/// (each entry leaks its id/family strings once); cloned so callers own it.
pub fn class_pool(class: &str) -> Vec<ModDef> {
    static POOLS: OnceLock<Mutex<BTreeMap<String, &'static [ModDef]>>> = OnceLock::new();
    let cache = POOLS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut g = cache.lock().expect("mod pool cache");
    g.entry(class.to_string())
        .or_insert_with(|| Box::leak(load_class(class).into_boxed_slice()))
        .to_vec()
}

/// The pool a weapon actually sees: the UNION of the named pools, in order,
/// deduplicated by mod id.
///
/// The game's compatibility is not one flat list per weapon. DE tags a mod
/// PRIMARY (fits any primary weapon), Rifle (the class), or narrower still —
/// Assault Rifle, Bow, Sniper — and a weapon draws every tag that applies to
/// it. Collapsing that into a single directory per weapon was right only
/// while every rifle-class weapon in the roster was a launcher.
pub fn pool_union(pools: &[String]) -> Vec<ModDef> {
    let mut out: Vec<ModDef> = Vec::new();
    for p in pools {
        for m in class_pool(p) {
            if !out.iter().any(|x| x.id == m.id) {
                out.push(m);
            }
        }
    }
    out.sort_by_key(|m| m.id);
    out
}

/// The pool a weapon can EQUIP WITH NOTHING INSTALLED: its pools unioned, minus
/// mods whose equip requirement the weapon does not meet. [`pool_for_build`] is
/// the same rule once evolutions are chosen.
///
/// The compat tag is not the whole rule. Sinister Reach and Combustion Beam
/// are tagged PRIMARY and still cannot go on the Torid (user, 2026-07-31):
/// they need a CONTINUOUS weapon. The Torid is the case that shows where the
/// line falls — its Incarnon form IS a continuous beam and it still cannot
/// take them, because its OTHER firing mode is a semi-auto grenade launcher
/// and an equip rule is asked of every mode a weapon has.
pub fn pool_for_weapon(weapon_id: &str) -> Vec<ModDef> {
    pool_for_build(weapon_id, &[])
}

/// Every trigger a BUILD can FIRE: the weapon's own, plus that of any form an
/// installed evolution UNLOCKS.
///
/// A firing MODE is what an equip rule is asked about, and an Incarnon weapon
/// has two of them: "Weapons with an Incarnon mode must have Semi-Auto trigger
/// type for both firing modes in order to equip this mod" (wiki,
/// Semi-Pistol_Cannonade). So Dual Toxocyst — semi-auto, with a full-auto
/// Incarnon form — takes a Cannonade while the Genesis is not installed and
/// refuses it the moment tier 1 is (user, 2026-08-04).
///
/// A CHARGED form is NOT a second firing mode: charged vs uncharged is chosen
/// freely on every trigger pull and the weapon comparison lists ONE trigger for
/// such a weapon (Cernos Prime is "Charge", Larkspur Prime "Held"). That is
/// exactly the line [`FormKind::is_adapter_form`] already draws, and it is why
/// only a form an EVOLUTION unlocks joins this list — the arsenal gains a second
/// trigger when the Genesis goes in, not when you hold the button down.
fn triggers_of(weapon_id: &str, evolutions: &[&str]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    // THE WEAPON'S OWN trigger is its DEFAULT form's — `default_form: true` is
    // "the arsenal's form (module _TooltipAttackDisplay)", i.e. the one the
    // weapon comparison lists a trigger for. Asking the entry directly would
    // make `cernos_prime_uncharged` (semi-auto) a semi-auto WEAPON, when the
    // bow it is a form of is listed "Charge" — a form is not a weapon, and only
    // an Incarnon mode gets a second trigger of its own.
    if let Some(s) = crate::weapons_data::spec(weapon_id) {
        let group = s.transform_group.as_deref().unwrap_or(&s.id);
        let default = crate::weapons_data::all()
            .iter()
            .find(|x| x.transform_group.as_deref().unwrap_or(&x.id) == group && x.default_form)
            .unwrap_or(s);
        out.push(default.attack.trigger.as_str());
    }
    for id in evolutions {
        let Some(form) = crate::evolutions_data::get(id).and_then(|e| e.unlocks_form()) else {
            continue;
        };
        if let Some(s) = crate::weapons_data::spec(form) {
            if !out.contains(&s.attack.trigger.as_str()) {
                out.push(s.attack.trigger.as_str());
            }
        }
    }
    out
}

/// The pool a BUILD can equip: [`pool_for_weapon`]'s rules, resolved against
/// every firing mode the chosen `evolutions` give the weapon.
///
/// `evolutions` empty is the weapon as it comes out of the box — which is what
/// [`pool_for_weapon`] means and why it is this function with nothing installed.
pub fn pool_for_build(weapon_id: &str, evolutions: &[&str]) -> Vec<ModDef> {
    let Some(spec) = crate::weapons_data::spec(weapon_id) else {
        return Vec::new();
    };
    // EVERY firing mode must meet the requirement, not just the one you happen
    // to be in. The Torid is the case that shows where the line falls for
    // `continuous`: its Incarnon form IS a beam and it still cannot take
    // Sinister Reach, because its other firing mode is a grenade launcher.
    let triggers = triggers_of(weapon_id, evolutions);
    let all = |t: &str| !triggers.is_empty() && triggers.iter().all(|x| *x == t);
    // What `WeaponBase::continuous` reads, asked of every mode.
    let continuous = all("held");
    // Same rule, other trigger: the Cannonades state "Only compatible with
    // Semi-Auto Trigger" on the card and DE enforces it at the slot.
    let semi_auto = all("semi_auto");
    // "Mods that affect Ammo Maximum have no effect on Robotic weapon because
    // they already have unlimited ammo reserves" (wiki `Sentinel`). Stated for
    // robotic weapons, true of any weapon with no ammo pool, and read off the
    // one fact that says so — `ammo_max` absent. A mod is dropped only when
    // ammo maximum is ALL it does: a dual-stat keeps its other half, whose
    // ammo share is already inert.
    let no_ammo_pool = spec.ammo_max.is_none();
    let only_ammo_max = |m: &ModDef| {
        !m.effects.is_empty()
            && m.effects.iter().all(|e| {
                matches!(e, ModEffect::Indirect(crate::loadout::IndirectStat::AmmoMax, _))
            })
    };
    pool_union(&spec.mod_pools)
        .into_iter()
        .filter(|m| match m.requires_weapon {
            None => true,
            Some("continuous") => continuous,
            Some("semi_auto") => semi_auto,
            // SYNTH CHARGE's magazine gate, and it reads the BASE magazine
            // rather than the modded one — the wiki says so in both directions:
            // "If the magazine is increased above 6 on a weapon that has below
            // 6, it will still not be usable on that gun. However, if a gun has
            // a magazine above 6 and it is reduced below that, the mod will
            // still function." So it is an equip rule and not a live check:
            // no mod can buy it and no mod can lose it.
            //
            // `magazine` on the spec IS the base magazine — the mod layer never
            // writes it — which is what makes this the right number to read.
            Some("magazine_6") => spec.magazine.is_some_and(|m| m >= 6.0),
            // An unknown requirement hides the mod rather than ignoring the
            // restriction — a mod offered where it cannot go is the worse bug.
            Some(_) => false,
        })
        // A mod written for ONE weapon goes nowhere else. Matched against the
        // transform GROUP as well as the id, so an Incarnon form counts as the
        // weapon its mod was written for rather than as a stranger.
        .filter(|m| {
            m.exclusive_to.is_empty()
                || m.exclusive_to.contains(&weapon_id)
                || spec
                    .transform_group
                    .as_deref()
                    .is_some_and(|g| m.exclusive_to.contains(&g))
        })
        .filter(|m| !(no_ammo_pool && only_ammo_max(m)))
        // DE's INCOMPATIBILITY tags — the mirror of `requires_weapon`, and the
        // reason plain Serration goes on a sentinel weapon while Amalgam
        // Serration does not (user, 2026-08-01). An Amalgam mod's second half
        // buffs the WARFRAME ("+25% Sprint Speed... always applies, regardless
        // of whether or not you are holding the weapon"), and a companion is
        // not the Warframe, so the wiki states it outright: "This mod cannot be
        // equipped on Sentinel weapons", tags `SENTINEL_WEAPON, POWER_WEAPON`.
        // We model no exalted weapon, so `power_weapon` is carried and unused.
        .filter(|m| {
            !(spec.class.contains("sentinel") && m.excludes_weapon.contains(&"sentinel_weapon"))
        })
        .collect()
}

/// The secondary/pistol mod pool — `data/mods/pistol/*.yaml` (Dual Toxocyst's
/// and Laetum's pool).
pub fn pistol_pool() -> Vec<ModDef> {
    class_pool("pistol")
}

/// Display info for a mod's DESCRIPTION at any rank: the X-templated game
/// text plus the (rank0, rankMax) pair of every rank-VARYING effect, in
/// yaml order. The description's `X`s map to these in order (extra varying
/// effects beyond the X count are hidden stats — Amalgam Barrel Diffusion's
/// acrobatic speed — and are correctly left unconsumed at the tail).
#[derive(Debug, Clone)]
pub struct ModDescInfo {
    pub description: String,
    pub xvals: Vec<(f64, f64)>,
    pub max_rank: u32,
}

impl ModDescInfo {
    /// The description with each `X` filled at `rank` (linear rank0→rankMax
    /// — the schema stores real endpoints; regular mods scale linearly).
    pub fn at(&self, rank: u32) -> String {
        let r = rank.min(self.max_rank) as f64;
        let m = self.max_rank.max(1) as f64;
        let vals: Vec<f64> = self.xvals.iter().map(|(a, b)| a + (b - a) * r / m).collect();
        crate::loadout::fill_x(&self.description, &vals)
    }
}

/// Description info by mod id — the VERBATIM in-game text with each `X`
/// filled, which is what the picker and a configured slot display.
///
/// Covers EVERY class. It used to scan `mods/pistol/` alone, from when the
/// rifle pool was hardcoded; the rifle pool has been yaml-driven with a
/// description on every file for a while, so every rifle mod silently fell
/// back to the engine's modeled effect lines. That fallback only states what
/// the ENGINE models, so anything unmodeled on a mod simply vanished from the
/// UI — the card looked like it did less than it does.
///
/// None means the file genuinely has no `description`, and the caller falls
/// back to the effect lines.
/// Where in `hay` this effect is SPOKEN ABOUT, if it is at all.
///
/// A kind reads `<what>_<qualifiers>` and a card names the `<what>`:
/// `life_steal_on_own_damage` is written "Life Steal", `status_chance_bonus` is
/// written "Status Chance". So the longest form is tried first and trailing
/// words are dropped until one is found — never below two words, because a lone
/// word matches too easily to be evidence of anything.
///
/// A syndicate radial is named by its SYNDICATE (Purity, Truth); "syndicate
/// radial" appears on no card.
pub(crate) fn effect_spoken_at(e: &Value, hay: &str) -> Option<usize> {
    let kind = e.get("kind").and_then(Value::as_str)?;
    if kind == "syndicate_radial" {
        let sy = e.get("syndicate").and_then(Value::as_str)?.to_lowercase();
        return hay.find(&sy);
    }
    let words: Vec<&str> = kind
        .trim_end_matches("_bonus")
        .trim_end_matches("_reduction")
        .split('_')
        .collect();
    let floor = if words.len() <= 1 { 1 } else { 2 };
    (floor..=words.len())
        .rev()
        .find_map(|take| hay.find(&words[..take].join(" ")))
}

/// The effect kinds on this mod that the loader DROPPED — what the card must
/// admit it does not do.
///
/// `effect()` is a `filter_map`, so an effect it cannot build simply vanishes
/// and the mod loads as one that silently does less than its card says. Two
/// kinds say so on purpose (`unmodeled`, `out_of_scope`) and the ModDef carries
/// a flag for each; this covers the third case, a mod that is PARTLY modelled.
///
/// Winds of Purity is the one today: its Purity radial lands 1,000 damage a
/// blast and its life steal heals a Tenno this arena does not have. Flagging
/// the whole mod `unmodeled` would say the card does nothing, which is worse
/// than saying nothing — so the disclosure has to be per effect.
///
/// DERIVED, never listed: it re-asks `effect()` the same question the loader
/// asked, so a mod that starts dropping an effect discloses it without anyone
/// noticing they should come back here (memory: derive triggers, don't list
/// them).
pub fn unmodeled_effects(id: &str) -> &'static [String] {
    static MAP: OnceLock<std::collections::HashMap<String, Vec<String>>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map = std::collections::HashMap::new();
        for (_, text) in crate::data::files_under("mods/") {
            let Ok(mf) = serde_norway::from_str::<ModFile>(text) else { continue };
            let dropped: Vec<String> = mf
                .effects
                .iter()
                .filter(|e| effect(&mf.id, e).is_none())
                .filter_map(|e| e.get("kind").and_then(Value::as_str))
                // The two that already have their own flag and their own line
                // on the card.
                .filter(|k| *k != "unmodeled" && *k != "out_of_scope")
                // `life_steal_on_own_damage` -> "life steal on own damage": the
                // kind IS the description, in the vocabulary the yaml chose.
                .map(|k| k.replace('_', " "))
                .collect();
            if !dropped.is_empty() {
                map.insert(mf.id.clone(), dropped);
            }
        }
        map
    })
    .get(id)
    .map_or(&[], |v| v.as_slice())
}

pub fn desc_info(id: &str) -> Option<&'static ModDescInfo> {
    static INFO: OnceLock<std::collections::HashMap<String, ModDescInfo>> = OnceLock::new();
    INFO.get_or_init(|| {
        let mut map = std::collections::HashMap::new();
        for (_, text) in crate::data::files_under("mods/") {
            let Ok(mf) = serde_norway::from_str::<ModFile>(text) else { continue };
            let Some(desc) = mf.description else { continue };
            // The X's in a description are consumed IN ORDER, and an effect
            // can supply more than one: "+X% Multishot for Xs. Stacks up to
            // Xx." is one buff spending three of them.
            //
            // But the values are matched to placeholders BY KIND, not by
            // position, because a description is free to write any of them as
            // a literal — Galvanized Crosshairs spells out its 12s and its 5x
            // and leaves only two X's, both for crit. Feeding values in effect
            // order there put the duration in a crit slot: "+1200% Critical
            // Chance", with everything after it shifted up one.
            //
            // Constants ride as (v, v) so `at(rank)` interpolates them to
            // themselves. A kind with nothing left to give STOPS the fill, so
            // the placeholder stays visible and
            // `desc_info_fills_every_x_across_the_pool` fails, rather than a
            // wrong-kind value quietly taking the slot.
            // Values are matched to placeholders by KIND and by SENTENCE, not by
            // position in a flat queue.
            //
            // A `X%`-style placeholder opens the next effect that has a
            // rank-varying value; "for Xs" and "up to Xx" then describe THAT
            // effect. Position alone put Galvanized Crosshairs' 12-second
            // duration into its crit slot — "+1200% Critical Chance" — because
            // that description spells its duration out and offers no slot for
            // it, so everything after shifted up one. A flat per-kind queue
            // gets Galvanized Scope wrong the same way: its first buff carries
            // `max_stacks: 1` that the text never mentions, and the one "Xx"
            // in the sentence belongs to the second buff.
            //
            // Constants ride as (v, v) so `at(rank)` interpolates them to
            // themselves. A placeholder with nothing to fill it STOPS the fill,
            // so it stays visible and
            // `desc_info_fills_every_x_across_the_pool` fails, rather than a
            // wrong-kind value quietly taking the slot.
            let varying = |e: &Value| match (f(e, "rank0"), f(e, "rankMax")) {
                (Some(a), Some(b)) if (a - b).abs() > 1e-12 => Some((a, b)),
                _ => None,
            };
            // `duration` (buff) and `duration_seconds` (on_equip_buff) are the
            // same slot in the sentence; a mod carries one or neither. A
            // duration that RAMPS with rank (Argon Scope: 2s -> 9s) also states
            // `duration_rank0` — without it the card read "for 9s" at every
            // rank, a rank-varying value shown as a constant.
            let dur = |e: &Value| {
                let d = n(e, "duration").or_else(|| n(e, "duration_seconds"))?;
                Some((n(e, "duration_rank0").unwrap_or(d), d))
            };
            // The card's own lines, lowercased once: a `Value` placeholder asks
            // about the effect its LINE names, and only falls back to position
            // when the line names nothing.
            let lines: Vec<String> = desc.lines().map(str::to_lowercase).collect();
            let x_line = crate::loadout::x_lines(&desc);
            let mut xvals: Vec<(f64, f64)> = Vec::new();
            let mut ei: Option<usize> = None; // the effect the sentence is on
            let mut used: Vec<usize> = Vec::new();
            for (xi, kind) in crate::loadout::x_kinds(&desc).into_iter().enumerate() {
                use crate::loadout::XKind;
                // Seek forward to an effect that can answer this placeholder;
                // a `Value` always moves on, the others stay put once the
                // sentence has an effect to describe.
                let seek = |from: usize, pick: &dyn Fn(&Value) -> bool| {
                    (from..mf.effects.len()).find(|&i| pick(&mf.effects[i]))
                };
                let next = match kind {
                    XKind::Value => {
                        // BY NAME FIRST. Position alone made the yaml's effect
                        // ORDER an unwritten part of the card's meaning, and
                        // Winds of Purity broke it the day it was written: its
                        // radial was listed first while the card says "+X% Life
                        // Steal" first, so the two ladders landed in each
                        // other's slots and it printed "+100% Life Steal /
                        // +0.2 Purity" for the wiki's "+20% / +1". Both wrong,
                        // both the kind of number a mod could have.
                        let named = x_line.get(xi).and_then(|&l| lines.get(l)).and_then(|line| {
                            (0..mf.effects.len()).find(|i| {
                                !used.contains(i)
                                    && varying(&mf.effects[*i]).is_some()
                                    && effect_spoken_at(&mf.effects[*i], line).is_some()
                            })
                        });
                        ei = named.or_else(|| {
                            seek(ei.map_or(0, |i| i + 1), &|e| varying(e).is_some())
                        });
                        if let Some(i) = ei {
                            used.push(i);
                        }
                        ei.and_then(|i| varying(&mf.effects[i]))
                    }
                    XKind::Duration => {
                        if ei.is_none() {
                            ei = seek(0, &|e| dur(e).is_some());
                        }
                        ei.and_then(|i| dur(&mf.effects[i]))
                    }
                    XKind::Stacks => {
                        if ei.is_none() {
                            ei = seek(0, &|e| n(e, "max_stacks").is_some());
                        }
                        // A stack CAP that scales with rank (Aerial Ace's
                        // 1x -> 6x) is a rank-varying value, not a constant.
                        ei.and_then(|i| n(&mf.effects[i], "max_stacks"))
                            .map(|s| (s, s))
                            .or_else(|| {
                                ei = seek(ei.map_or(0, |i| i + 1), &|e| varying(e).is_some());
                                ei.and_then(|i| varying(&mf.effects[i]))
                            })
                    }
                };
                match next {
                    Some(v) => xvals.push(v),
                    None => break,
                }
            }
            map.insert(
                mf.id,
                ModDescInfo { description: desc, xvals, max_rank: mf.max_rank },
            );
        }
        map
    })
    .get(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// NO mod loads with an empty effect list.
    ///
    /// A mod that parses to nothing equips, costs capacity, prints its card —
    /// and does nothing, which the picker and the optimizer cannot see. 14 of
    /// them shipped that way until 2026-08-01, every one a `kind: unmodeled`
    /// the loader dropped on the floor: beam range, movement speed, ammo
    /// conversion, self-stagger, noise, double jumps, kill explosions, status
    /// spread. They carry no SINGLE-TARGET damage, which is what
    /// [`ModEffect::Indirect`] is for — the value now survives into the panel
    /// and the API, where the 2D multi-target model will read it instead of
    /// re-deriving it from card text.
    ///
    /// One of them, Shell Rush's "+50% Charge Rate", was not indirect at all:
    /// a charged form's cadence IS its draw, so that was DPS being discarded.
    #[test]
    fn no_mod_loads_with_nothing() {
        let mut empty: Vec<&str> = Vec::new();
        for class in classes() {
            for m in class_pool(class) {
                if m.effects.is_empty() {
                    empty.push(m.id);
                }
            }
        }
        empty.sort_unstable();
        empty.dedup();
        assert!(
            empty.is_empty(),
            "mods that equip and do nothing: {empty:?} — give the effect a \
             `kind` the loader knows, or an `IndirectStat` if it carries no \
             single-target damage"
        );
    }

    /// Every AMALGAM mod must declare that it cannot go on a sentinel weapon.
    ///
    /// The wiki states it per mod — "This mod cannot be equipped on Sentinel
    /// weapons", infobox tags `SENTINEL_WEAPON, POWER_WEAPON` — and the reason
    /// is structural: an Amalgam mod's second half buffs the WARFRAME, which a
    /// companion is not. DE's own taxonomy names that structure, so the check
    /// can be mechanical: `/Lotus/Upgrades/Mods/DualSource/` is the directory
    /// every Amalgam mod lives in.
    ///
    /// The PATH is the check, not the rule — the wiki tag is the rule, and
    /// each mod's yaml carries it with its citation. This exists so the next
    /// Amalgam mod cannot be added without someone reading that infobox.
    #[test]
    fn every_amalgam_mod_declares_it_cannot_go_on_a_sentinel_weapon() {
        let mut missing: Vec<String> = Vec::new();
        for (p, text) in crate::data::files_under("mods/").filter(|(p, _)| p.ends_with(".yaml")) {
            let dual = text
                .lines()
                .any(|l| l.starts_with("internal_name:") && l.contains("/DualSource/"));
            if dual && !text.contains("sentinel_weapon") {
                missing.push(p.to_string());
            }
        }
        assert!(
            missing.is_empty(),
            "Amalgam (DualSource) mods with no `excludes_weapon: [sentinel_weapon, ...]`: \
             {missing:?} — check the wiki infobox's incompatibility tags"
        );
        // And the rule reaches the pool: plain Serration equips on a sentinel
        // weapon, the Amalgam one does not.
        let ids: Vec<&str> = pool_for_weapon("verglas_prime").iter().map(|m| m.id).collect();
        assert!(ids.contains(&"serration"), "plain Serration is fine on a sentinel weapon");
        assert!(!ids.contains(&"amalgam_serration"), "Amalgam Serration is not: {ids:?}");
        // Ammo Maximum is the wiki's other stated sentinel rule: "Mods that
        // affect Ammo Maximum have no effect on Robotic weapon because they
        // already have unlimited ammo reserves."
        assert!(!ids.contains(&"ammo_drum"), "an infinite reserve takes no ammo mod: {ids:?}");
        // The Torid keeps all three — it is neither a sentinel nor ammo-less.
        let torid: Vec<&str> = pool_for_weapon("torid").iter().map(|m| m.id).collect();
        for id in ["serration", "amalgam_serration", "ammo_drum"] {
            assert!(torid.contains(&id), "the torid keeps {id}");
        }
    }

    /// The Cannonade family states TWO rules on one card line, and all three
    /// members must carry both. The shotgun one carried NEITHER until
    /// 2026-08-03 — it had a bare zero-valued `fire_rate_bonus` where the lock
    /// belongs, which is how "Fire Rate cannot be modified" ends up rendering
    /// as "+0% Fire Rate" while a build stacks fire rate underneath it. Its
    /// twins had been right since M23, which is exactly why a per-family
    /// invariant is worth pinning: the outlier is invisible from either file.
    #[test]
    fn every_cannonade_states_both_of_its_rules() {
        let ids = ["semi_rifle_cannonade", "semi_pistol_cannonade", "semi_shotgun_cannonade"];
        for id in ids {
            let pools: Vec<String> = ["rifle", "pistol", "shotgun"].iter().map(|s| s.to_string()).collect();
            let m = pool_union(&pools)
                .into_iter()
                .find(|m| m.id == id)
                .unwrap_or_else(|| panic!("{id} is in the data"));
            assert_eq!(m.requires_weapon, Some("semi_auto"), "{id} states its EQUIP rule");
            assert_eq!(m.requires, Some("semi_auto"), "{id} states its CALC gate");
            assert!(m.disables.contains(&"fire_rate"), "{id} locks fire rate: {:?}", m.disables);
            // ...and states the lock as a lock, not as a zero-valued bonus.
            assert!(
                !m.effects.iter().any(|e| matches!(e, ModEffect::FireRate(_))),
                "{id} carries a fire-rate EFFECT under a fire-rate LOCK"
            );
        }
    }

    /// The lock BITES, on a real weapon with real mods: a fire-rate mod under
    /// a Cannonade changes nothing, and neither does a fire-rate DRAWBACK —
    /// "cannot be modified" is symmetric, which is why the mod is worth more
    /// on a build carrying a negative, not less.
    #[test]
    fn a_cannonade_locks_fire_rate_both_ways() {
        use crate::loadout::{resolve, StackPolicy, WeaponBase};
        let base = WeaponBase::from_data("torid", false, &[]);
        let pool = pool_for_weapon("torid");
        let pick = |id: &str| {
            pool.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{id} in the torid pool"))
        };
        let cannon = pick("semi_rifle_cannonade");
        let speed = pick("speed_trigger");
        let slow = pick("critical_delay");          // -20% fire rate at max rank

        let fr = |mods: &[&ModDef]| resolve(&base, mods, StackPolicy::Emergent).fire_rate;
        let bare = fr(&[]);
        assert!(fr(&[speed]) > bare * 1.05, "speed trigger moves fire rate on its own");
        assert!(fr(&[slow]) < bare * 0.95, "critical delay moves it the other way");
        for (label, mods) in [
            ("a bonus", vec![cannon, speed]),
            ("a drawback", vec![cannon, slow]),
            ("both at once", vec![cannon, speed, slow]),
        ] {
            assert!(
                (fr(&mods) - bare).abs() < 1e-9,
                "under the lock the weapon keeps its BASE fire rate through {label}: {} vs {bare}",
                fr(&mods)
            );
        }
        // ...and the damage half still pays, so the lock is a lock and not a
        // whole-mod veto.
        let dmg = |mods: &[&ModDef]| resolve(&base, mods, StackPolicy::Emergent).damage.total();
        assert!(dmg(&[cannon]) > dmg(&[]) * 1.5, "the Cannonade still adds its damage");
    }

    /// "Only compatible with Semi-Auto Trigger" is an EQUIP rule, and the pool
    /// is where an equip rule has to bite: the optimizer searches this list,
    /// so a mod left in it is a mod a winning build can carry to a slot the
    /// game refuses (user, 2026-08-03).
    #[test]
    fn the_cannonades_need_a_semi_auto_trigger() {
        let has = |w: &str, m: &str| pool_for_weapon(w).iter().any(|x| x.id == m);

        // Boar Prime is full-auto. This is the case that was wrong.
        assert!(!has("boar_prime", "semi_shotgun_cannonade"), "full-auto takes no Cannonade");
        // ...and so is its Incarnon form, a held beam — both firing modes fail.
        assert!(!has("boar_prime", "semi_rifle_cannonade"), "nor the rifle one");

        // The Torid IS semi-auto, and keeps it — the rule excludes, it does
        // not blanket-hide.
        assert!(has("torid", "semi_rifle_cannonade"), "a semi-auto rifle keeps it");
        for w in ["dual_toxocyst", "laetum"] {
            assert!(has(w, "semi_pistol_cannonade"), "{w} is semi-auto");
        }
        // Cernos Prime CHARGES; its uncharged form is semi-auto and does not
        // decide the pool.
        assert!(!has("cernos_prime", "semi_rifle_cannonade"), "a charge bow is not semi-auto");
    }

    /// A MOD WRITTEN FOR ONE WEAPON GOES NOWHERE ELSE.
    ///
    /// "Can equip the Ocucor-exclusive Sentient Surge mod" (wiki, Ocucor), and
    /// exclusivity is an EQUIP rule: the mod is never offered elsewhere rather
    /// than equipping and sitting inert. Asserted in BOTH directions, because
    /// only one of them is the interesting failure — a gate that hides the mod
    /// everywhere passes any test that only checks it is absent from the
    /// wrong weapons.
    #[test]
    fn an_exclusive_mod_reaches_its_weapon_and_no_other() {
        let has = |w: &str| pool_for_weapon(w).iter().any(|m| m.id == "sentient_surge");
        assert!(has("ocucor"), "the weapon it was written for must be offered it");
        for other in crate::weapons_data::roster().map(|s| s.id.clone()) {
            if other == "ocucor" {
                continue;
            }
            assert!(!has(&other), "{other} was offered an Ocucor-only mod");
        }
        // ...and it is a PISTOL mod, so it is in the pool it would otherwise
        // reach every pistol through. Without this the test above would pass
        // for a mod that simply failed to load.
        assert!(
            pool_union(&["pistol".to_string()]).iter().any(|m| m.id == "sentient_surge"),
            "it should be a pistol mod that exclusivity narrows, not a mod nobody has"
        );

        // GILDED TRUTH SPLITS A FAMILY, which is the harder case: the wiki says
        // it is "exclusive to the Burston Prime" AND "cannot be equipped on the
        // Burston", so one variant takes it and its twin does not — a
        // distinction a rule keyed on class, trigger or riven family could not
        // draw, since the two share all three.
        let gilded = |w: &str| pool_for_weapon(w).iter().any(|m| m.id == "gilded_truth");
        assert!(gilded("burston_prime"), "the Prime is what it was written for");
        assert!(!gilded("burston"), "and the wiki says the base variant cannot take it");
    }

    /// AN EQUIP RULE THE MOD DECLARES DECIDES EVERY POOL — derived, both ways.
    ///
    /// This was a 136-row table, one line per weapon, and its stated reason was
    /// good: "a check that recomputes the rule agrees with a wrong rule". What
    /// changed is where the rule LIVES. It is not in this file and never was —
    /// the mod's own yaml says `requires_weapon: semi_auto`, and
    /// `every_cannonade_states_both_of_its_rules` pins that value against the
    /// card. So this does not recompute anything: it asks the MOD what it
    /// requires, asks the WEAPON what it is, and checks the pool agreed.
    ///
    /// The table cost one edit per weapon — `mods_data.rs` was touched in six
    /// of six weapon-intake commits on 2026-08-15, and only ever for this — and
    /// an audit found it reproduced by the rule 136 times out of 136, with no
    /// exception in it at all. A snapshot with no surprises in it is a snapshot
    /// of a rule.
    ///
    /// BOTH DIRECTIONS, because each alone passes on a different bug: "offered
    /// ⇒ eligible" alone passes on a filter that offers nothing, and "eligible
    /// ⇒ offered" alone passes on one that offers everything.
    ///
    /// It is written over EVERY mod that declares a trigger requirement rather
    /// than over the three Cannonades, so the next such mod is covered by
    /// arriving.
    #[test]
    fn a_declared_trigger_rule_decides_every_pool() {
        // Every trigger a weapon in the roster actually lists — so a
        // requirement naming a trigger nothing has is caught as a typo rather
        // than passing vacuously.
        let triggers: std::collections::BTreeSet<&str> = crate::weapons_data::roster()
            .map(|s| s.attack.trigger.as_str())
            .collect();
        let gated: Vec<ModDef> = pool_union(&["rifle", "pistol", "shotgun", "archgun"]
            .iter().map(|s| s.to_string()).collect::<Vec<_>>())
            .into_iter()
            .filter(|m| m.requires_weapon.is_some_and(|r| triggers.contains(r)))
            .collect();
        assert!(gated.len() >= 3, "the three Cannonades at least: {}", gated.len());

        let mut wrong: Vec<String> = Vec::new();
        for m in &gated {
            let want = m.requires_weapon.expect("filtered on it");
            for w in crate::weapons_data::roster() {
                let offered = pool_for_weapon(&w.id).iter().any(|x| x.id == m.id);
                // ELIGIBLE means the weapon lists that trigger AND draws the
                // pool the mod lives in. The second half is what makes the
                // Fluctus interesting: it is semi-auto and takes none, because
                // an Arch-Gun draws neither the rifle nor the pistol pool.
                let draws = pool_for_weapon(&w.id).iter().any(|x| x.id == m.id)
                    || pool_union(&w.mod_pools).iter().any(|x| x.id == m.id);
                let eligible = w.attack.trigger == want && draws;
                if offered != eligible {
                    wrong.push(format!(
                        "{}: {} offered={offered} but trigger={} draws={draws}",
                        w.id, m.id, w.attack.trigger
                    ));
                }
            }
        }
        assert!(
            wrong.is_empty(),
            "a pool disagreed with a rule the mod DECLARES:\n  {}",
            wrong.join("\n  ")
        );
    }

    /// ...and the answers a reader would get wrong, written down. Not a
    /// roster: three cases, each because something other than the trigger
    /// decides it.
    #[test]
    fn the_cannonade_answers_worth_reading() {
        let has = |w: &str, m: &str| pool_for_weapon(w).iter().any(|x| x.id == m);
        // THE POOL BEATS THE TRIGGER. The Fluctus is semi-auto and takes none:
        // an Arch-Gun draws the archgun pool and the Cannonades are rifle,
        // pistol and shotgun mods.
        assert_eq!(crate::weapons_data::spec("fluctus").unwrap().attack.trigger, "semi_auto");
        assert!(!has("fluctus", "semi_rifle_cannonade"));
        // A SENTINEL WEAPON DOES TAKE ONE, which surprised the 2026-08-15
        // intake: Semi-Rifle Cannonade lives in the RIFLE pool rather than the
        // `primary` one, and a semi-auto companion weapon draws rifle mods.
        assert!(has("stinger", "semi_rifle_cannonade"));
        assert!(has("cryotra", "semi_rifle_cannonade"));
        // AND THE PISTOL ONE IS NOT THE RIFLE ONE. A weapon takes the Cannonade
        // of its own pool and no other.
        assert!(has("lex", "semi_pistol_cannonade"));
        assert!(!has("lex", "semi_rifle_cannonade"));
    }

    /// ...AND THE LOCK BITES, on every weapon that can equip one and in every
    /// FORM of it.
    ///
    /// The table above says who may wear a Cannonade; this says what wearing it
    /// does. "Equipping this mod will set weapon's Fire Rate to its default
    /// ignoring other bonuses, EVEN NEGATIVE EFFECTS" — the case that made the
    /// question worth asking is the negative one, since a build pairs a
    /// Cannonade with a fire-rate-for-crit trade precisely to be handed the
    /// trade for free (owner, 2026-08-08). Same sentence, same test, for the
    /// Acuity twins' Multishot.
    ///
    /// DERIVED: it finds the locking mods by their `disables`, the offending
    /// mods by resolving each one alone and seeing which move that stat, and
    /// the forms from the weapon. A fourth locking mod, or a fifth weapon that
    /// can wear one, is covered without a line here.
    #[test]
    fn a_locking_mod_pins_its_stat_in_every_form_that_can_wear_it() {
        use crate::loadout::{resolve, StackPolicy, WeaponBase};
        let read = |p: &crate::loadout::ResolvedPanel, key: &str| match key {
            "fire_rate" => p.fire_rate,
            "multishot" => p.multishot,
            other => panic!("no reader for the locked stat `{other}`"),
        };
        let mut proven = 0;
        for w in crate::weapons_data::roster() {
            let pool = pool_for_build(&w.id, &[]);
            let lockers: Vec<&ModDef> = pool.iter().filter(|m| !m.disables.is_empty()).collect();
            if lockers.is_empty() {
                continue;
            }
            for f in crate::weapons_data::forms_of(&w.id) {
                let base = WeaponBase::from_data(f.weapon_id, true, &[]);
                let bare = resolve(&base, &[], StackPolicy::Emergent);
                for lock in &lockers {
                    for key in &lock.disables {
                        // Everything in this pool that MOVES the stat on its
                        // own — which is what the lock has to be tested
                        // against, in both directions of movement.
                        for other in pool.iter().filter(|m| m.id != lock.id) {
                            let alone = resolve(&base, &[other], StackPolicy::Emergent);
                            if (read(&alone, key) - read(&bare, key)).abs() < 1e-9 {
                                continue;
                            }
                            let both = resolve(&base, &[other, lock], StackPolicy::Emergent);
                            assert!(
                                (read(&both, key) - read(&bare, key)).abs() < 1e-9,
                                "{} ({}): {} moved {key} from {} to {} under {} — \
                                 a lock that lets a bonus through",
                                w.id,
                                f.weapon_id,
                                other.id,
                                read(&bare, key),
                                read(&both, key),
                                lock.id
                            );
                            proven += 1;
                        }
                    }
                }
            }
        }
        // The three Cannonades and the two Acuities, across their weapons and
        // forms: a collapse here means the walk stopped finding the pairs.
        assert!(proven > 100, "the walk collapsed: only {proven} lock/bonus pairs");
    }

    /// A FORM IS NOT A WEAPON. `cernos_prime_uncharged` fires semi-auto, and
    /// the bow it belongs to is listed "Charge" — asking the form entry its own
    /// trigger would hand a Cannonade to a weapon that cannot hold one. The
    /// weapon's trigger is its DEFAULT form's, which is what the arsenal and
    /// the weapon-comparison table show.
    #[test]
    fn a_form_entry_answers_with_its_weapons_trigger() {
        let has = |w: &str, m: &str| pool_for_build(w, &[]).iter().any(|x| x.id == m);
        assert_eq!(
            crate::weapons_data::spec("cernos_prime_uncharged").unwrap().attack.trigger,
            "semi_auto",
            "the tapped shot really is semi-auto — that is the trap"
        );
        assert!(!has("cernos_prime_uncharged", "semi_rifle_cannonade"), "...but the bow is not");
        // It is the only form entry that can show this: `mod_pools` is declared
        // on the WEAPON, so every other form resolves to an empty pool and has
        // no answer to give either way.
        assert!(
            pool_for_build("dual_toxocyst_incarnon", &[]).is_empty(),
            "a form declares no pool of its own — modding is the weapon's"
        );
    }

    /// INSTALLING THE GENESIS IS WHAT TAKES THE CANNONADE OFF (user,
    /// 2026-08-04). "Weapons with an Incarnon mode must have Semi-Auto trigger
    /// type for both firing modes in order to equip this mod" (wiki,
    /// Semi-Pistol_Cannonade), and the roster's three semi-auto Incarnon
    /// weapons all transform into something that is not: Dual Toxocyst and
    /// Laetum into full-auto, the Torid into a held beam.
    ///
    /// So the pool is a question about the BUILD, not about the weapon: with
    /// tier 1 unpicked the weapon has one firing mode and the mod fits, and the
    /// moment tier 1 goes in it has two and the mod is gone.
    #[test]
    fn an_unlocked_incarnon_form_is_a_second_firing_mode() {
        let has = |w: &str, evos: &[&str], m: &str| {
            pool_for_build(w, evos).iter().any(|x| x.id == m)
        };
        for (w, evo, m) in [
            ("dual_toxocyst", "dual_toxocyst_evo1_incarnon_form", "semi_pistol_cannonade"),
            ("laetum", "laetum_evo1_incarnon_form", "semi_pistol_cannonade"),
            ("torid", "torid_evo1_incarnon_form", "semi_rifle_cannonade"),
        ] {
            assert!(has(w, &[], m), "{w} with nothing installed is pure semi-auto");
            assert!(!has(w, &[evo], m), "{w} with the Incarnon form installed is not");
            // The rest of the pool is untouched — this excludes one mod, it is
            // not a second pool for the transformed weapon.
            assert!(
                has(w, &[evo], "serration") || has(w, &[evo], "hornet_strike"),
                "{w} keeps its ordinary mods with the form unlocked"
            );
        }
        // An evolution that unlocks NOTHING changes nothing: only a form the
        // weapon gains can be a second trigger.
        assert!(
            has("dual_toxocyst", &["dual_toxocyst_carnage_reign"], "semi_pistol_cannonade"),
            "a stat evolution is not a firing mode"
        );
        // ...and a weapon whose Incarnon form is ALSO semi-auto would keep it.
        // The roster has none yet (the wiki names Bronco / Lato / Lex), so the
        // claim is pinned on the data instead: every entry here transforms into
        // a trigger that is not semi-auto, which is why all three drop it.
        for (w, evo) in [
            ("dual_toxocyst", "dual_toxocyst_evo1_incarnon_form"),
            ("laetum", "laetum_evo1_incarnon_form"),
            ("torid", "torid_evo1_incarnon_form"),
        ] {
            let form = crate::evolutions_data::get(evo).and_then(|e| e.unlocks_form()).unwrap();
            assert_ne!(
                crate::weapons_data::spec(form).unwrap().attack.trigger,
                "semi_auto",
                "{w}: the test above only proves the rule while this holds"
            );
        }
    }

    /// PvP-EXCLUSIVE mods must not ship in a PvE pool — they are a separate
    /// balance pass, and offering them makes the picker and the optimizer
    /// propose builds that cannot exist in the mission the sim models.
    ///
    /// The trap is that `/Lotus/Upgrades/Mods/PvPMods/` in the `internal_name`
    /// is an ORIGIN, not a restriction. Update 17.9 made a set of Conclave mods
    /// equippable in PvE, so four of ours legitimately sit under that path.
    /// Deleting on the path alone throws away real content; keeping everything
    /// under it ships six mods that cannot be equipped.
    ///
    /// The authority is the wiki's `Rifle_Mods` / `Pistol_Mods` /
    /// `Shotgun_Mods` tables, which tag the genuinely restricted ones
    /// "Exclusive to PvP". This pins the survivors as an explicit allowlist,
    /// so neither mistake can be made silently: a new PvP-path mod fails until
    /// someone checks that table.
    ///
    /// The SHOTGUN import (2026-08-03) is what this test was written for. The
    /// generator brought 15 mods in under that path; `Shotgun_Mods` tags ten
    /// of them "Exclusive to PvP" — Bounty Hunter, Crash Shot, Flak Shot,
    /// Hydraulic Chamber, Kill Switch, Loaded Capacity, Loose Chamber,
    /// Momentary Pause, Prize Kill, Shred Shot — and they were deleted. The
    /// five below are the ones the table leaves unmarked.
    #[test]
    fn only_pve_legal_conclave_mods_are_in_the_pools() {
        const PVE_LEGAL: [&str; 13] = [
            "agile_aim", "twitch", "eject_magazine", "reflex_draw",
            // Shotgun, from `Shotgun_Mods` (2026-08-03).
            "broad_eye", "double_barrel_drift", "lock_and_load", "snap_shot", "soft_hands",
            // ASSAULT RIFLE, from `Rifle_Mods` (2026-08-05). The RENDERED page
            // is what carries the tags — the raw wikitext is template
            // transclusions and names none of these mods, so a check against
            // `?action=raw` would have found nothing and concluded nothing.
            // Seven mods on that page are tagged "Exclusive to PvP" and two of
            // them are assault-rifle-only (Recover, Vanquished Prey); those
            // were NOT imported. The page's own "Assault rifle-only" list is
            // the positive statement, and it names these three.
            "gun_glide", "overview", "tactical_reload",
            // DOUBLE TAP is the fourth, and its own page states the direction
            // outright: "is a PvE and Conclave Latron, Latron Wraith, and
            // Latron Prime mod". The Bugs block goes further and says the
            // Conclave half is the broken one — "Despite originally being a
            // Conclave only mod, the buff does not actually function in
            // Conclave" — so a PvP path here is where it came FROM, not where
            // it works.
            "double_tap",
        ];
        let mut found: Vec<String> = crate::data::files_under("mods/")
            .filter(|(p, _)| p.ends_with(".yaml"))
            .filter(|(_, text)| {
                text.lines()
                    .any(|l| l.starts_with("internal_name:") && l.contains("/PvPMods/"))
            })
            .map(|(p, _)| {
                p.rsplit('/').next().unwrap_or(p).trim_end_matches(".yaml").to_string()
            })
            .collect();
        found.sort();
        let mut want: Vec<String> = PVE_LEGAL.iter().map(|s| s.to_string()).collect();
        want.sort();
        assert_eq!(
            found, want,
            "every mod under /PvPMods/ must be one the wiki does NOT tag \
             \"Exclusive to PvP\" — check Rifle_Mods / Pistol_Mods before changing this"
        );
    }

    /// Every `X` in a description must be filled. A literal X on a mod card is
    /// a rendering failure — "Stacks up to Xx." is what it looked like — and it
    /// only became visible on the rifle pool once `desc_info` started covering
    /// it, so the pool asserts it rather than waiting to be noticed again.
    #[test]
    fn every_mod_description_fills_all_its_x() {
        // KNOWN GAP, not a tolerance: these carry a parenthetical about BOWS
        // ("(xX for Bows)", and Internal Bleeding's fire-rate clause) whose
        // multiplier is real in-game data we do not hold. Because `fill_x`
        // substitutes positionally, that missing value does not merely leave an
        // X — it SHIFTS every later one, so Shred renders its punch-through
        // (1.2) as the bow multiplier "x2.2" and then has nothing left for
        // "+X Punch Through". Fixing it means adding the datum, not deleting
        // the clause: bows draw from the rifle pool, so the text is relevant.
        const MISSING_BOW_MULTIPLIER: [&str; 7] = [
            "critical_delay", "internal_bleeding", "primed_shred", "shred",
            "speed_trigger", "vile_acceleration", "vile_precision",
        ];
        let mut bad = Vec::new();
        for class in ["pistol", "rifle"] {
            for m in class_pool(class) {
                if MISSING_BOW_MULTIPLIER.contains(&m.id) {
                    continue;
                }
                if let Some(info) = desc_info(m.id) {
                    let s = info.at(info.max_rank);
                    if s.contains('X') {
                        bad.push(format!("{}: {}", m.id, s.replace('\n', " / ")));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "unfilled X placeholders:\n{}", bad.join("\n"));
    }

    #[test]
    fn loads_the_pistol_pool_from_yaml() {
        let mods = load_class("pistol");
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
        // Galvanized Crosshairs is AIM-GATED, so its buffs arrive WRAPPED -
        // asserting the bare variant would pass on a build where the gate had
        // been silently dropped, which is the bug this wrapper exists to stop.
        assert!(by("galvanized_crosshairs").effects.iter().any(|e| matches!(e,
            ModEffect::WhileTenno(crate::loadout::TennoCondition::Aiming, inner)
                if matches!(**inner, ModEffect::OnHeadshotKillCritChance { max_stacks: 5, .. }))));
        assert!(by("galvanized_crosshairs").effects.iter().all(|e| matches!(e, ModEffect::WhileTenno(crate::loadout::TennoCondition::Aiming, _))),
            "every Galvanized Crosshairs effect is while-aiming");
        // ... and a mod with no condition is NOT wrapped.
        assert!(by("galvanized_diffusion").effects.iter().all(|e| !matches!(e, ModEffect::WhileTenno(crate::loadout::TennoCondition::Aiming, _))));
        // Faction-damage mod loads with the right faction + bonus (Expel Orokin
        // → Corrupted; +30% at max rank).
        assert!(by("expel_grineer").effects.iter().any(|e| matches!(e, ModEffect::FactionDamage(Faction::Grineer, v) if (*v - 0.30).abs() < 1e-9)));
        assert!(by("expel_orokin").effects.iter().any(|e| matches!(e, ModEffect::FactionDamage(Faction::Corrupted, _))));
        // The formerly-unmodeled kinds now map to real effects.
        assert!(by("pistol_acuity").effects.iter().any(|e| matches!(e, ModEffect::WeakpointDamage(v) if (*v - 3.50).abs() < 1e-9)));
        assert!(by("pistol_acuity").effects.iter().any(|e| matches!(e, ModEffect::WeakpointCritChance(v) if (*v - 3.50).abs() < 1e-9)));
        assert!(by("hemorrhage").effects.iter().any(|e| matches!(e,
            ModEffect::ProcConversion { from: DamageType::Impact, to: DamageType::Slash, chance, low_rate_threshold, low_rate_multiplier }
                if (*chance - 0.35).abs() < 1e-9 && (*low_rate_threshold - 2.5).abs() < 1e-9 && (*low_rate_multiplier - 2.0).abs() < 1e-9)));
        // Both of these are while-aiming too, so they arrive wrapped.
        assert!(by("sharpened_bullets").effects.iter().any(|e| matches!(e,
            ModEffect::WhileTenno(crate::loadout::TennoCondition::Aiming, inner)
                if matches!(**inner, ModEffect::OnKillCritDamage { bonus, duration }
                    if (bonus - 0.75).abs() < 1e-9 && (duration - 9.0).abs() < 1e-9))));
        assert!(by("pressurized_magazine").effects.iter().any(|e| matches!(e,
            ModEffect::WhileTenno(crate::loadout::TennoCondition::Aiming, inner)
                if matches!(**inner, ModEffect::OnReloadFireRate { bonus, .. }
                    if (bonus - 0.90).abs() < 1e-9))));
    }

    /// A description's numbers are of two kinds and they must not swap places:
    /// some RAMP with rank, some are FIXED. Every case here was wrong when the
    /// values were handed out by position (checked against WFCD `levelStats`,
    /// 2026-07-31).
    #[test]
    fn fixed_and_rank_varying_values_land_in_the_right_slots() {
        // Literal duration and stack cap in the text, so the two X's are both
        // crit. By position the 12-second duration took the second one and
        // printed "+1200% Critical Chance".
        assert_eq!(
            desc_info("galvanized_crosshairs").unwrap().at(10),
            "On Headshot:
+120% Critical Chance when Aiming for 12s
On Headshot Kill:
+40% Critical Chance when Aiming for 12s. Stacks up to 5x."
        );
        // Its rifle twin spells all five out. The first buff carries a
        // `max_stacks: 1` the text never mentions, so a per-kind queue would
        // hand THAT to "Stacks up to Xx" instead of the second buff's 5.
        assert_eq!(
            desc_info("galvanized_scope").unwrap().at(10),
            "On Headshot:
+120% Critical Chance when Aiming for 12s
On Headshot Kill:
+40% Critical Chance when Aiming for 12s. Stacks up to 5x."
        );
        // A duration that RAMPS: 2s at rank 0, 9s at max. Stored as one number
        // it read "for 9s" at every rank.
        let argon = desc_info("argon_scope").unwrap();
        assert_eq!(argon.at(0), "On Headshot:
+22.5% Critical Chance when Aiming for 2s");
        assert_eq!(argon.at(5), "On Headshot:
+135% Critical Chance when Aiming for 9s");
        // A stack CAP that ramps, 1x -> 6x — rank-varying, not fixed.
        assert_eq!(
            desc_info("aerial_ace").unwrap().at(5),
            "On Kill:
Refresh Double Jump up to 6x while Airborne."
        );
        // The bows multiplier is fixed text; the fire rate is not.
        assert_eq!(
            desc_info("shred").unwrap().at(5),
            "+30% Fire Rate (x2 for Bows)
+1.2 Punch Through"
        );
    }

    #[test]
    fn desc_info_fills_every_x_across_the_pool() {
        // EVERY class, not just pistol. The pool this walked was the only one
        // that existed when it was written, so a guard that NAMES a pool stops
        // guarding the moment a second appears — the rifle pool then shipped
        // descriptions whose X count exceeded their values, and Vile
        // Acceleration showed its damage downside as a bare placeholder (user,
        // 2026-07-30). It reads the class registry now.
        //
        // (X count <= varying-effect count; hidden tail stats — Amalgam's
        // acrobatic speed — are legitimately unconsumed.)
        for c in classes() {
            for m in class_pool(c) {
                let info =
                    desc_info(m.id).unwrap_or_else(|| panic!("{} has no description", m.id));
                for r in 0..=info.max_rank {
                    let d = info.at(r);
                    assert_eq!(
                        crate::loadout::count_x(&d),
                        0,
                        "{} rank {r}: unfilled X in {d:?}",
                        m.id
                    );
                }
            }
        }
        // Spot checks: linear fill, the xX faction form, and a flat value.
        assert_eq!(desc_info("hornet_strike").unwrap().at(10), "+220% Damage");
        assert_eq!(desc_info("hornet_strike").unwrap().at(0), "+20% Damage");
        assert_eq!(desc_info("expel_grineer").unwrap().at(5), "x1.3 Damage to Grineer");
        assert_eq!(desc_info("seeker").unwrap().at(5), "+2.1 Punch Through");
        // Signed template + negative stored downside: magnitude only.
        assert_eq!(desc_info("anemic_agility").unwrap().at(5), "+90% Fire Rate\n-15% Damage");
        // Its rifle twin, plus the literal bows clause: the `2` is TEXT, not a
        // value — written as `X` it ate the damage stat and left the last
        // placeholder unfilled.
        assert_eq!(
            desc_info("vile_acceleration").unwrap().at(5),
            "+90% Fire Rate (x2 for Bows)\n-15% Damage"
        );
    }
}

#[cfg(test)]
mod class_tests {
    use super::*;

    /// Mod pools are DISCOVERED from `data/mods/<class>/`, so adding a class
    /// is a data change. Today only `pistol` exists; the moment
    /// `data/mods/rifle/` lands it must appear here with no code edit.
    #[test]
    fn classes_come_from_the_data_tree() {
        let cs = classes();
        assert!(cs.contains(&"pistol"), "expected the pistol class, got {cs:?}");
        for c in &cs {
            assert!(!class_pool(c).is_empty(), "class {c} has no mods");
        }
        // An unknown class is empty, never another class's pool.
        assert!(class_pool("no_such_class").is_empty());
    }

    /// A CONDITION DE PRINTS ON THE CARD MUST EXIST IN THE MODEL.
    ///
    /// Primary Acuity read "+350% Weak Point Damage / +350% Weak Point
    /// Critical Chance" and was modelled as plain `base_damage_bonus` +
    /// `crit_chance_bonus` — every shot collected all of it, whether or not
    /// anything was hit in the head (user, 2026-08-02). Its own pistol twin
    /// had been right the whole time, which is what made one wrong file easy
    /// to miss among a hundred right ones.
    ///
    /// The check reads DE's own `description` beside the effects, so it works
    /// for a mod nobody has thought about yet:
    ///
    ///   · "Weak Point" on the card ⇒ some effect is a `weakpoint_*` kind;
    ///   · "when/while Aiming" ⇒ a DAMAGE effect is wrapped in `while_aiming`
    ///     (a mod whose only payload is movement speed or accuracy is exempt —
    ///     the condition cannot change a number this calculator produces).
    #[test]
    fn a_condition_on_the_card_is_a_condition_in_the_model() {
        const DAMAGE_KINDS: [&str; 10] = [
            "base_damage_bonus", "crit_chance_bonus", "crit_damage_bonus",
            "multishot_bonus", "status_chance_bonus", "fire_rate_bonus",
            "elemental_damage_bonus", "physical_damage_bonus",
            "faction_damage_bonus", "headshot_damage_bonus",
        ];
        let mut bad: Vec<String> = Vec::new();
        for (path, text) in crate::data::files_under("mods/").filter(|(p, _)| p.ends_with(".yaml")) {
            let id = text
                .lines()
                .find_map(|l| l.strip_prefix("id:"))
                .unwrap_or(path)
                .trim();
            let desc = text
                .lines()
                .find_map(|l| l.strip_prefix("description:"))
                .unwrap_or("")
                .to_lowercase();
            // Comments are stripped: a comment naming a trigger must not
            // satisfy a check about what the model does.
            let effects: String = match text.split_once("effects:") {
                Some((_, rest)) => rest
                    .lines()
                    .map(|l| l.split('#').next().unwrap_or(""))
                    .collect::<Vec<_>>()
                    .join("\n"),
                None => String::new(),
            };
            let has_damage = DAMAGE_KINDS.iter().any(|k| effects.contains(k));
            if desc.contains("weak point") && !effects.contains("weakpoint_") {
                bad.push(format!("{id}: card says Weak Point, no weakpoint_* effect"));
            }
            // ANY "while/when <state>" clause, not the two phrases that
            // happened to be known. Spectral Serration reads "+330% Damage
            // while Invisible" and was a flat bonus every build collected —
            // the check knew about aiming and weak points, so it walked past
            // (user, 2026-08-02). A conditional is satisfied by a `condition:`,
            // by a `trigger:` the sim can evaluate, or by resolving to a
            // CondBuff — all three leave the word in the effects block.
            let conditional = effects.contains("condition:") || effects.contains("trigger:");
            if !conditional && has_damage {
                for clause in ["while ", "when "] {
                    if let Some(at) = desc.find(clause) {
                        // "+X% Damage while Airborne" is a condition; "while
                        // Aiming" is too. A sentence that merely CONTAINS the
                        // word later (a note, not a gate) is why this looks
                        // only at what follows it.
                        let tail: String = desc[at..].chars().take(40).collect();
                        bad.push(format!(
                            "{id}: card gates on \"{}\" and no effect is conditional",
                            tail.trim_end()
                        ));
                        break;
                    }
                }
            }
        }
        assert!(bad.is_empty(), "{} mod(s):\n  {}", bad.len(), bad.join("\n  "));
    }
}


/// A CARD'S SENTENCES AND ITS EFFECTS ARE ONE ORDER.
///
/// `desc_info` used to fill the X placeholders by walking the effects forward
/// and nothing else, which made the yaml's effect ORDER an unwritten part of
/// the card's meaning — a contract that was real, undocumented and unchecked,
/// and that Winds of Purity broke the day it was written.
///
/// The filler now asks the LINE first (`effect_spoken_at`), so an effect the
/// card names is found wherever it sits. This rule therefore no longer guards
/// the numbers for a named effect — it guards the two things left: the
/// FALLBACK, which is still positional and is what an unnamed effect gets, and
/// a reader, for whom a yaml listed in a different order than the card it
/// prints is a puzzle with no answer in it.
///
/// The check is derived: it does not know what any mod does. For each effect
/// whose KIND names something the description actually says ("status chance",
/// "fire rate", "life steal", or a syndicate's own word), it takes where that
/// phrase sits in the sentence — and those positions must climb with the
/// effects. An effect whose kind is not spoken in the description is skipped,
/// so this only ever fires on a mismatch it can prove.
#[cfg(test)]
mod card_order_tests {
    use super::*;

    /// Where in the description this effect is spoken about, if it is.
    ///
    /// A kind reads `<what>_<qualifiers>`, and a card names the `<what>`:
    /// `life_steal_on_own_damage` is written "Life Steal", `status_chance_bonus`
    /// is written "Status Chance". So the longest form is tried first and
    /// trailing words are dropped until one is found — never below two words,
    /// because a lone word matches too easily to be evidence of anything.
    fn spoken_at(e: &Value, hay: &str) -> Option<(usize, String)> {
        let kind = e.get("kind").and_then(Value::as_str)?;
        // A syndicate radial is named by its SYNDICATE (Purity, Truth), never
        // by its kind — "syndicate radial" appears on no card.
        if kind == "syndicate_radial" {
            let s = e.get("syndicate").and_then(Value::as_str)?.to_lowercase();
            return hay.find(&s).map(|at| (at, s));
        }
        let words: Vec<&str> = kind
            .trim_end_matches("_bonus")
            .trim_end_matches("_reduction")
            .split('_')
            .collect();
        let floor = if words.len() <= 1 { 1 } else { 2 };
        for take in (floor..=words.len()).rev() {
            let p = words[..take].join(" ");
            if let Some(at) = hay.find(&p) {
                return Some((at, p));
            }
        }
        None
    }

    #[test]
    fn effects_are_listed_in_the_order_the_card_says_them() {
        for (path, text) in crate::data::files_under("mods/") {
            let Ok(mf) = serde_norway::from_str::<ModFile>(text) else { continue };
            let Some(desc) = mf.description.as_ref() else { continue };
            let hay = desc.to_lowercase();
            let mut seen: Vec<(usize, String, usize)> = Vec::new(); // (position, phrase, effect index)
            for (i, e) in mf.effects.iter().enumerate() {
                if let Some((at, p)) = spoken_at(e, &hay) {
                    seen.push((at, p, i));
                }
            }
            for w in seen.windows(2) {
                let (a, b) = (&w[0], &w[1]);
                assert!(
                    a.0 <= b.0,
                    "{path}: the card says `{}` before `{}`, but the effects are listed \
                     the other way round — `desc_info` fills the X's in effect order, so \
                     the two ladders land in each other's slots",
                    b.1, a.1
                );
            }
        }
    }
}


/// THE CARD IS RIGHT WHICHEVER ORDER THE EFFECTS ARE IN.
///
/// The Winds of Purity failure, pinned by its outcome rather than by its cause:
/// the wiki's ladder is life steal 5/10/15/20% and Purity 0.25/0.5/0.75/1, and
/// the card printed "+100% Life Steal / +0.2 Purity" — the two ladders in each
/// other's slots. Both numbers are the kind a mod could have, which is why
/// reading the card could not catch it and why the value is pinned here.
#[cfg(test)]
mod card_values_tests {
    use super::*;

    #[test]
    fn winds_of_purity_prints_the_wikis_ladder() {
        let info = desc_info("winds_of_purity").expect("the mod has a description");
        assert_eq!(info.at(0), "+5% Life Steal\n+0.25 Purity");
        assert_eq!(info.at(info.max_rank), "+20% Life Steal\n+1 Purity");
    }

    /// The filler finds an effect by the words the card uses for it, so a
    /// two-word kind is matched and a lone word is not evidence.
    #[test]
    fn an_effect_is_found_by_the_words_its_card_uses() {
        let steal = serde_norway::from_str::<Value>("kind: life_steal_on_own_damage").unwrap();
        assert!(effect_spoken_at(&steal, "+x% life steal").is_some());
        assert!(effect_spoken_at(&steal, "+x purity").is_none());
        // A syndicate radial answers to its SYNDICATE, never to its kind.
        let radial = serde_norway::from_str::<Value>("kind: syndicate_radial\nsyndicate: purity").unwrap();
        assert!(effect_spoken_at(&radial, "+x purity").is_some());
        assert!(effect_spoken_at(&radial, "+x% life steal").is_none());
    }

    /// EVERY MOD EFFECT THE LOADER DROPS IS ON THE CARD, and the list of them
    /// is short enough to argue about.
    ///
    /// `unmodeled_effects` derives the disclosure, so a mod that starts
    /// dropping one says so without anyone remembering to come back here —
    /// this pins WHICH ones, so a new gap arrives in review rather than only
    /// on a card nobody happens to open. The arcane side carries the same pin
    /// (`an_arcane_that_does_nothing_with_an_effect_says_so`), added after
    /// three of them promised a stat they silently never applied.
    #[test]
    fn a_mod_that_drops_an_effect_says_so_and_the_list_is_argued() {
        let mut found: Vec<String> = Vec::new();
        for (_, text) in crate::data::files_under("mods/") {
            let Ok(mf) = serde_norway::from_str::<ModFile>(text) else { continue };
            for why in unmodeled_effects(&mf.id) {
                found.push(format!("{} :: {why}", mf.id));
            }
        }
        found.sort();
        assert_eq!(
            found,
            [
                // TWO THINGS ACID SHELLS' EXPLOSION DOES THAT THIS ARENA
                // CANNOT. Line of sight is not a simplification here, it is a
                // missing dimension: there are no walls at all, so every body
                // in the radius is in sight of every other. On the group
                // ruler's open grid that is the game's own answer too; in a
                // corridor it is not.
                //
                // The Extra Hit is the other, and it compounds: the explosion
                // triggers Toxic Lash and Xata's Whisper, and "because these
                // effects are considered Sobek's damage, they too can trigger
                // Acid Shells". `fire_extra_hits` is not wired into the area
                // path, so a build running one of those abilities is
                // understated by the extra instances AND by the chain they
                // would start.
                "acid_shells :: a line of sight rule this arena has no walls to enforce",
                "acid_shells :: an extra hit the corpse explosion should also trigger",
                // A HEAL, and nothing damages the Tenno here — the same edge
                // Winds of Purity's life steal sits on.
                "bhisaj_bal :: health restore nothing damages the tenno here",
                // TWO EDGES AT ONCE: no distance and no finishers. The stun is
                // crowd control against a target that never acts.
                "dizzying_rounds :: a stun that opens finishers no distance and no \
                 finishers here",
                // Three clauses that need a SECOND thing in the world — a
                // bubble to hit, an ability to cast, an Incarnon bug to
                // reproduce.
                "double_tap :: a bullet attraction bubble makes each hit count twice",
                "double_tap :: hitting an object counts as a miss and clears the stacks",
                "double_tap :: the latron incarnon bugs this mod only the aoe benefits",
                // Crowd control again, and for the same reason it is worth
                // nothing: the target never acts, so stone changes nothing it
                // TAKES.
                "metamorphic_magazine :: petrify after 20 hits crowd control against a \
                 target that never acts",
                // The card's whole headline needs a Nullifier, and the wiki
                // says outright it "has no effect on any other enemy in
                // Warframe".
                "neutralizing_justice :: destroys a nullifier shield generator no such \
                 enemy in this roster",
                // THE TWO HALVES OF THE NAPALM NOBODY PUBLISHED, and they are
                // different kinds of gap. The tick rate is the whole DPS of the
                // field and NOTHING states it — the page gives damage per tick
                // and a duration in seconds and never joins them, and the
                // Ogris's page, Napalm Grenades and DE's own card text are all
                // silent — so one a second is an assumption a measurement
                // settles rather than something the engine can derive.
                //
                // The Heat proc is the opposite: it is STATED and this engine
                // cannot express it. "ticking for 50% of napalm's damage per
                // second for 3 seconds" — the rate is what every Heat proc here
                // already does; the LENGTH is this mod's own and a per-proc
                // duration is not something a field can carry yet, so the burn
                // runs the standard time and this weapon is overstated by the
                // difference.
                "nightwatch_napalm :: a heat proc that should burn for three seconds and burns for the standard time",
                // A PER-WEAPON CATALOG ROW, of the kind docs/CATALOGS.md is
                // about: the wiki tabulates an ADDITIONAL spread penalty for
                // the Cernos Prime (and, commented out, four crossbows this
                // roster does not carry) on top of the flat ladder every bow
                // takes. One row for one weapon in the roster, so the mod's
                // own number is right for eight of the nine bows and a third
                // of a degree tight on the ninth.
                "split_flights :: the cernos prime takes an additional spread row of its own",
                // TWO THINGS THE PAGE STATES AND THE MODEL CANNOT. One is an
                // open question — whether the bonus reaches a status payload —
                // and the other is a family of alt-fire and reload mechanics
                // that inherit the last round's bonus, of which this roster
                // carries exactly one weapon (the Dual Toxocyst's Frenzy).
                "synth_charge :: the alt fire and reload mechanics that inherit the last rounds bonus",
                "synth_charge :: whether the bonus reaches a status payload is unmeasured",
                // ONE TARGET, so "3 or more enemies with a single projectile"
                // is unreachable — but the crit damage half is modelled and
                // pays to an invisible Tenno, which is why only this line is
                // here and not the whole mod.
                "unseen_dread :: invisibility on striking 3 enemies with one shot only \
                 one target here",
                // Its Purity radial lands 1,000 damage a blast and its life
                // steal heals a Tenno this arena does not have — so the
                // disclosure has to be per effect. Flagging the whole mod
                // would say the card does nothing, which is worse than saying
                // nothing at all.
                "winds_of_purity :: life steal on own damage",
            ],
            "the partly-modelled mod list moved"
        );
    }
}

#[cfg(test)]
mod weapon_exclusive_survey {
    /// EVERY WEAPON-EXCLUSIVE GUN MOD OUR ROSTER CAN EQUIP, and how many of
    /// them are still missing.
    ///
    /// A mod that fits ONE weapon is invisible to every other check we have:
    /// the pools are built from what `data/mods/` holds, so a mod nobody
    /// transcribed is a mod the builder cannot offer and nothing notices. The
    /// Dread's Unseen Dread and the Latron's Double Tap sat missing that way
    /// until someone read a wiki page (owner, 2026-08-13).
    ///
    /// `data/surveys/weapon_exclusive_mods.yaml` is the survey — generated by
    /// `scripts/survey_weapon_mods.py` from WFCD's export, joined on
    /// `compatName` and read by this test and nothing else. It is the FACT:
    /// what exists in game. This is the ratchet that stops the gap growing
    /// quietly, and it only ever goes down.
    ///
    /// Seven of the twenty rows are EXCLUDED on purpose rather than owed: six
    /// Conclave-exclusive mods that `only_pve_legal_conclave_mods_are_in_the_pools`
    /// forbids anyway, and Soaring Truth, which is in DE's files and in no
    /// released game. An exclusion has to carry its reason, so refusing a mod
    /// costs the same sentence as transcribing one and cannot be the quiet
    /// option.
    ///
    /// IT IS AT ZERO (2026-08-13). The last two needed a trigger the engine did
    /// not have — Eximus Advantage's weak-point hit on an EXIMUS, and
    /// Hata-Satya's per-hit stack that the RELOAD clears — and neither could be
    /// approximated: a `CondBuff` applies a buff at its assumed maximum, which
    /// here is +600% base damage against any target and +500% critical chance
    /// on the first shot of every run.
    #[test]
    fn the_weapon_exclusive_mods_we_still_owe_only_goes_down() {
        const OWED: usize = 0;
        let text = crate::data::file("surveys/weapon_exclusive_mods.yaml")
            .expect("data/surveys/weapon_exclusive_mods.yaml — run scripts/survey_weapon_mods.py");
        let mut total = 0usize;
        let mut missing: Vec<&str> = Vec::new();
        let mut excluded: Vec<&str> = Vec::new();
        let mut unreasoned: Vec<&str> = Vec::new();
        let mut name = "";
        for line in text.lines() {
            let l = line.trim();
            if let Some(v) = l.strip_prefix("- name:") {
                name = v.trim();
                total += 1;
            } else if let Some(v) = l.strip_prefix("carried:") {
                match v.trim() {
                    "~" => missing.push(name),
                    "excluded" => {
                        excluded.push(name);
                        unreasoned.push(name);
                    }
                    _ => {}
                }
            } else if l.starts_with("reason:") {
                unreasoned.retain(|n| *n != name);
            }
        }
        assert!(total >= 20, "the survey looks empty: {total} rows");
        // EQUALITY, not a ceiling. The ratchet used to allow drifting below and
        // asked the ceiling to follow; at zero there is nowhere below to drift,
        // so the two directions collapse into one assertion — a mod appearing
        // fails it, and so would a mod being deleted without this line moving.
        assert_eq!(
            missing.len(),
            OWED,
            "{} weapon-exclusive mods missing, ceiling {OWED} — transcribe one or \
             raise this line deliberately:\n  {}",
            missing.len(),
            missing.join("\n  ")
        );
        // A REFUSAL IS NOT A SHORTCUT. `excluded` takes a mod out of the gap
        // count, so it must cost a written reason — otherwise the cheapest way
        // to close the ratchet is to declare everything out of scope.
        assert!(
            unreasoned.is_empty(),
            "excluded without a reason: {}",
            unreasoned.join(", ")
        );
        // Every mod that is neither carried nor excluded is missing, so these
        // three have to add up — a row the parser skipped would otherwise read
        // as one nobody owes.
        let carried = total - missing.len() - excluded.len();
        assert!(carried >= 13, "only {carried} of {total} carried");
    }
}


#[cfg(test)]
mod synth_charge_tests {
    /// SYNTH CHARGE: the LAST ROUND, its own multiplier, and three ways off.
    ///
    /// It shipped as a plain `base_damage_bonus` — +200% on EVERY shot, in the
    /// bucket Hornet Strike is in — which is wrong twice and wrong upward both
    /// times (owner, 2026-08-13).
    #[test]
    fn synth_charge_is_the_last_round_only_and_only_where_it_can_be() {
        use crate::loadout::{resolve, ModEffect, StackPolicy, WeaponBase};
        let pool = crate::mods_data::class_pool("pistol");
        let sc = pool.iter().find(|m| m.id == "synth_charge").expect("synth charge");
        assert!(
            sc.effects.iter().any(|e| matches!(e, ModEffect::LastRoundDamage(v) if (v - 2.0).abs() < 1e-9)),
            "the card is +200% at max rank, on the final shot: {:?}",
            sc.effects
        );

        // THE MAGAZINE GATE reads the BASE magazine, so it is an equip rule.
        // The Bronco (2) and the Angstrum (1) are turned away; the Lex sits
        // exactly on 6 and keeps it.
        let has = |w: &str| crate::mods_data::pool_for_weapon(w).iter().any(|m| m.id == "synth_charge");
        for w in ["lex", "vasto", "vasto_prime", "lato", "laetum"] {
            assert!(has(w), "{w}: base magazine is 6 or more");
        }
        for w in ["bronco", "bronco_prime", "angstrum", "prisma_angstrum"] {
            assert!(!has(w), "{w}: base magazine is under 6");
        }

        // …AND WHERE IT IS EQUIPPABLE IT IS STILL WORTH NOTHING on a continuous
        // weapon or an Incarnon form, both the mod's own words. The Kuva Nukor
        // has a 77-round magazine and is a beam: it may hold the mod and gets
        // nothing from it.
        let val = |w: &str| {
            let base = WeaponBase::from_data(w, true, &[]);
            resolve(&base, &[sc], StackPolicy::Emergent).last_round_damage
        };
        assert!((val("lex") - 2.0).abs() < 1e-9, "an ordinary pistol pays it");
        assert!(has("kuva_nukor"), "77 rounds, so it EQUIPS");
        assert_eq!(val("kuva_nukor"), 0.0, "…and a continuous weapon gets nothing");
        assert_eq!(val("lex_incarnon"), 0.0, "…and neither does an Incarnon fire mode");
    }
}

#[cfg(test)]
mod chamber_tests {
    /// THE CHAMBER FAMILY: two cards, one bracket, and no family tie.
    ///
    /// Both are `Sniper`-tagged, which is a pool this roster had no directory
    /// for at all until 2026-08-18 — fifteen snipers were drawing `[primary,
    /// rifle]` and nothing else, so every sniper-only mod in the game was
    /// invisible to the builder. `scripts/survey_pool_mods.py` is what stops
    /// that happening again.
    #[test]
    fn the_chambers_sum_into_one_first_round_bracket_and_are_not_a_family() {
        use crate::loadout::{resolve, ModEffect, StackPolicy, WeaponBase};
        let pool = crate::mods_data::class_pool("sniper");
        let pick = |id: &str| {
            pool.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{id}")).clone()
        };
        let cc = pick("charged_chamber");
        let pc = pick("primed_chamber");
        assert!(
            cc.effects.iter().any(|e| matches!(e, ModEffect::FirstRoundDamage(v) if (v - 0.4).abs() < 1e-9)),
            "Charged Chamber is +40% at rank 3: {:?}", cc.effects
        );
        assert!(
            pc.effects.iter().any(|e| matches!(e, ModEffect::FirstRoundDamage(v) if (v - 1.0).abs() < 1e-9)),
            "Primed Chamber is +100% at rank 3: {:?}", pc.effects
        );
        // "Despite its name … it is not the 'Primed version' of Charged
        // Chamber, and thus can be equipped alongside it." A shared `family`
        // would have made the pair mutually exclusive, which is the one thing
        // the page goes out of its way to deny.
        assert_ne!(
            (cc.family, pc.family), (Some("chamber"), Some("chamber")),
            "the two chambers are not a mod family"
        );
        assert!(cc.family.is_none() && pc.family.is_none());

        // ONE BRACKET: "Stacks additively with … for up to 140% bonus damage."
        let base = WeaponBase::from_data("vectis_prime", true, &[]);
        let both = resolve(&base, &[&cc, &pc], StackPolicy::Emergent);
        assert!(
            (both.first_round_damage - 1.4).abs() < 1e-9,
            "140%, not 1.4x1.0: {}", both.first_round_damage
        );

        // …AND THE INCARNON FORM KEEPS IT, which is where this card parts
        // company with Synth Charge. "Fixed the Vectis Incarnon Form
        // benefitting from Primed Chamber on every shot" (ver 43.5) says the
        // form pays it once a magazine — a bug in HOW OFTEN, not an exemption.
        let inc = WeaponBase::from_data("vectis_prime_incarnon", true, &[]);
        assert!(
            (resolve(&inc, &[&pc], StackPolicy::Emergent).first_round_damage - 1.0).abs() < 1e-9,
            "an Incarnon fire mode still takes the first-round bonus"
        );
    }

    /// EVERY SNIPER SEES THE SNIPER POOL, and nothing else does.
    #[test]
    fn the_sniper_pool_reaches_snipers_and_only_snipers() {
        let has = |w: &str| {
            crate::mods_data::pool_for_weapon(w).iter().any(|m| m.id == "primed_chamber")
        };
        for w in ["vectis", "vectis_prime", "rubico_prime", "lanka", "vulkar", "komorex"] {
            assert!(has(w), "{w} is a sniper");
        }
        for w in ["braton_prime", "paris_prime", "boar_prime", "lex", "kuva_nukor"] {
            assert!(!has(w), "{w} is not a sniper");
        }
        // A FORM DECLARES NO POOL AT ALL — modding is the WEAPON's, and every
        // form entry resolves to an empty one (see
        // `a_form_entry_answers_with_its_weapons_trigger`). So the pool goes on
        // the weapon and the Incarnon halves need nothing, which is also why
        // this edit touched fifteen files and not thirty.
        assert!(crate::mods_data::pool_for_weapon("vectis_incarnon").is_empty());
    }
}

#[cfg(test)]
mod pool_survey {
    /// EVERY CLASS-TAGGED GUN MOD THE ROSTER'S POOLS CAN HOLD, and how many
    /// are still missing.
    ///
    /// The sibling of `the_weapon_exclusive_mods_we_still_owe_only_goes_down`,
    /// and the half it never covered. That survey joins `compatName` against
    /// WEAPON NAMES, which finds the mod written for one gun; this one joins it
    /// against the POOL TAGS — Rifle, Bow, Sniper, Shotgun, Pistol, Assault
    /// Rifle, PRIMARY, Archgun — which is where the other five hundred live.
    ///
    /// Nothing looked at those, and the failure mode was invisible: a pool a
    /// weapon DECLARES and no directory holds resolves to an empty list, with
    /// no error anywhere. Nine bows had claimed `bow` since the roster began
    /// and `data/mods/bow/` did not exist, so Split Flights — the only
    /// multishot mod a bow can hold — was unreachable; fifteen snipers claimed
    /// no `sniper` pool at all, so both Chambers were (owner, 2026-08-18).
    ///
    /// `scripts/survey_pool_mods.py` refuses to run when a weapon claims a pool
    /// no export tag maps to, which is the check that catches the NEXT one at
    /// the moment somebody writes the tag down rather than months later.
    ///
    /// The ceiling is a RATCHET and starts where the pools stood the day the
    /// survey was written. It is not zero and is not meant to be yet — the 28
    /// are a work list, not a defect: three Thunderbolt entries, the ammo
    /// mutations, Target Acquired, Depleted Reload, Primed Blunderbuss, and a
    /// handful of one-offs.
    ///
    /// 29 -> 28 on 2026-08-25, when the TOME pool arrived complete (8 of 8,
    /// gap 0) — which is what lowering this line is for.
    #[test]
    fn the_pool_mods_we_still_owe_only_goes_down() {
        const OWED: usize = 28;
        let text = crate::data::file("surveys/pool_mods.yaml")
            .expect("data/surveys/pool_mods.yaml — run scripts/survey_pool_mods.py");
        let mut total = 0usize;
        let mut missing: Vec<String> = Vec::new();
        let mut unreasoned: Vec<String> = Vec::new();
        let (mut name, mut pool) = ("", "");
        for line in text.lines() {
            let l = line.trim();
            if let Some(v) = l.strip_prefix("- name:") {
                name = v.trim();
                total += 1;
            } else if let Some(v) = l.strip_prefix("pool:") {
                pool = v.trim();
            } else if let Some(v) = l.strip_prefix("carried:") {
                match v.trim() {
                    "~" => missing.push(format!("{pool}: {name}")),
                    "excluded" => unreasoned.push(format!("{pool}: {name}")),
                    _ => {}
                }
            } else if l.starts_with("reason:") {
                let key = format!("{pool}: {name}");
                unreasoned.retain(|n| *n != key);
            }
        }
        assert!(total >= 400, "the survey looks empty: {total} rows");
        assert_eq!(
            missing.len(),
            OWED,
            "{} class-tagged mods missing, ceiling {OWED} — transcribe one and lower \
             this line, or raise it deliberately:\n  {}",
            missing.len(),
            missing.join("\n  ")
        );
        // A REFUSAL IS NOT A SHORTCUT — the same rule the weapon-exclusive
        // survey holds. `excluded` takes a mod out of the gap count, so it
        // costs a written reason; otherwise the cheapest way to close a ratchet
        // is to declare everything out of scope.
        assert!(
            unreasoned.is_empty(),
            "excluded without a reason: {}",
            unreasoned.join(", ")
        );

        // AND EVERY POOL A WEAPON CLAIMS HOLDS SOMETHING. This is the assertion
        // that bites on the actual bug: `bow` and `sniper` were both legal
        // names carried by real weapons and both resolved to nothing.
        for w in crate::weapons_data::all() {
            for p in &w.mod_pools {
                assert!(
                    !crate::mods_data::class_pool(p).is_empty(),
                    "{}: mod pool `{p}` is empty — every mod tagged for it is unreachable",
                    w.id
                );
            }
        }
    }
}

#[cfg(test)]
mod split_flights_tests {
    /// SPLIT FLIGHTS: a MOD that grants a live stacking buff, which is a route
    /// the engine had no door for until this card.
    ///
    /// Every stacking buff before it came from an evolution or an arcane, so a
    /// mod that stacked on a trigger had to invent a bespoke `ModEffect`
    /// (`OnKillMultishot`, `OnHeadshotKillCritChance`, `ConditionOverload` —
    /// three variants for one idea). This one is a trigger already in
    /// `BuffTrigger` feeding a grant already in `BuffGrant`, so it carries the
    /// whole spec and `resolve` hands it to the panel beside the weapon's own.
    #[test]
    fn split_flights_reaches_the_panel_as_a_live_stacking_buff() {
        use crate::loadout::{resolve, BuffDecay, BuffGrant, BuffTrigger, StackPolicy, WeaponBase};
        let pool = crate::mods_data::class_pool("bow");
        let sf = pool.iter().find(|m| m.id == "split_flights").expect("split flights");

        let base = WeaponBase::from_data("paris_prime", true, &[]);
        let panel = resolve(&base, &[sf], StackPolicy::Emergent);
        let b = panel
            .stacking_buffs
            .iter()
            .find(|b| b.id == "split_flights")
            .expect("the mod's buff reaches the panel");
        assert_eq!(b.trigger, BuffTrigger::Hit, "every landing pellet earns one");
        // The PERCENTAGE bracket, which is where a multishot MOD's bonus goes —
        // not `Multishot` (a flat add) and not `BaseMultishot` (added before
        // mods). Split Chamber's +90% is in the same one, which is also why the
        // two share a family and cannot be equipped together.
        assert_eq!(b.grant, BuffGrant::MultishotPercent);
        assert!((b.per_stack - 1.0).abs() < 1e-9, "+100% a stack at rank 5");
        assert_eq!(b.max_stacks, 4);
        assert!((b.duration - 2.0).abs() < 1e-9);
        assert_eq!(
            b.decay,
            BuffDecay::AllAtOnce,
            "\"Stacks expire all at once after 2 seconds without a hit\""
        );

        // THE PENALTY IS DEGREES OF CONE, outside the accuracy bucket — "Added
        // spread is not affected by bonuses that increase accuracy". The Paris
        // Prime's aimed deviation is 0, so the whole of what this mod costs
        // shows up as widening from nothing rather than as a divided cone.
        let bare = resolve(&base, &[], StackPolicy::Emergent);
        let (b0, s0) = (bare.spread.expect("a bow has a cone"), panel.spread.unwrap());
        assert!(
            (s0.min_deg - b0.min_deg - 7.2).abs() < 1e-6,
            "+1.8 degrees a stack at four stacks: {} -> {}",
            b0.min_deg, s0.min_deg
        );
        assert!((s0.max_deg - b0.max_deg - 7.2).abs() < 1e-6);
    }

    /// …AND ONLY A BOW CAN HOLD IT. The `bow` pool existed as a NAME on nine
    /// weapons and as no directory at all, so this is the first assertion in
    /// the app that the tag reaches anything.
    #[test]
    fn the_bow_pool_reaches_bows_and_only_bows() {
        let has = |w: &str| {
            crate::mods_data::pool_for_weapon(w).iter().any(|m| m.id == "split_flights")
        };
        for w in ["paris", "paris_prime", "mk1_paris", "dread", "cernos_prime"] {
            assert!(has(w), "{w} is a bow");
        }
        for w in ["braton_prime", "vectis_prime", "boar_prime", "lex"] {
            assert!(!has(w), "{w} is not a bow");
        }
    }
}
