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
        "movement_speed" => IndirectStat::MovementSpeed,
        "sprint_speed" => IndirectStat::SprintSpeed,
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

fn effect(v: &Value) -> Option<ModEffect> {
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
                low_rate_mult: mult,
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
        // 2D groundwork (2026-08-01): these were `kind: unmodeled`, i.e. the
        // mod equipped and the number was thrown away. They carry no
        // SINGLE-TARGET damage, which is what `Indirect` is for.
        "range_bonus" => ModEffect::Indirect(IndirectStat::Range, max("rankMax")),
        "beam_range_bonus" => ModEffect::Indirect(IndirectStat::BeamRange, max("rankMax")),
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
    let effects = mf.effects.iter().filter_map(effect).collect();
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
/// exactly the line [`FormKind::is_gauge_switched`] already draws, and it is why
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
                .filter(|e| effect(e).is_none())
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
    /// game refuses (user, 2026-08-03: "半自动野猪是装不了的").
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

    /// THE WHOLE ROSTER, SPELLED OUT — which weapon is offered which Cannonade
    /// (user, 2026-08-04: "semi 系列是检查是否不对的武器也可以装").
    ///
    /// Written as an explicit table rather than re-derived from the trigger,
    /// because a check that recomputes the rule agrees with a wrong rule. Every
    /// entry is the wiki's own answer: the mod is a Rifle / Pistol / Shotgun mod
    /// AND the weapon's listed trigger is Semi-Auto. A new weapon fails this
    /// until someone writes down which of the three it takes.
    #[test]
    fn every_weapon_in_the_roster_gets_the_right_cannonade() {
        const CANNONADES: [&str; 3] =
            ["semi_rifle_cannonade", "semi_pistol_cannonade", "semi_shotgun_cannonade"];
        // (weapon, listed trigger, the Cannonades it may equip bare)
        const EXPECTED: [(&str, &str, &[&str]); 49] = [
            // Arch-Gun: the Cannonades are rifle/pistol/shotgun mods and an
            // Arch-Gun draws neither pool, so the trigger never comes up.
            ("larkspur_prime", "held", &[]),
            ("boar", "auto", &[]),                             // full-auto shotgun
            ("boar_prime", "auto", &[]),                       // ...and its Prime
            ("cernos_prime", "charge", &[]),                   // a bow is not semi-auto
            // THE BULK INTAKE (2026-08-08), batch 1. Every answer here is the
            // weapon's LISTED trigger and nothing subtler: Burst and Auto take
            // no Cannonade, Semi-Auto rifles take the rifle one and semi-auto
            // pistols the pistol one.
            ("sybaris", "burst", &[]),
            ("dex_sybaris", "burst", &[]),
            ("sybaris_prime", "burst", &[]),
            ("dera", "auto", &[]),
            ("dera_vandal", "auto", &[]),
            ("lato", "semi_auto", &["semi_pistol_cannonade"]),
            ("lato_vandal", "semi_auto", &["semi_pistol_cannonade"]),
            ("lato_prime", "semi_auto", &["semi_pistol_cannonade"]),
            ("lex", "semi_auto", &["semi_pistol_cannonade"]),
            ("lex_prime", "semi_auto", &["semi_pistol_cannonade"]),
            // Batch 2. The Bronco is a shotgun SIDEARM and still draws the
            // pistol pool, so Semi-Pistol Cannonade is the one it sees — the
            // trigger decides, not the shot pattern. The Kunai is thrown and
            // listed Auto, so no Cannonade at all.
            ("vasto", "semi_auto", &["semi_pistol_cannonade"]),
            ("vasto_prime", "semi_auto", &["semi_pistol_cannonade"]),
            ("bronco", "semi_auto", &["semi_pistol_cannonade"]),
            ("bronco_prime", "semi_auto", &["semi_pistol_cannonade"]),
            // Batch 3. The Sicarus family is BURST — its own trigger family, so
            // no Cannonade, the same answer the Burston gets. The Atomos is a
            // chaining BEAM and reads Held.
            ("sicarus", "burst", &[]),
            ("sicarus_prime", "burst", &[]),
            ("cestra", "auto", &[]),
            ("despair", "auto", &[]),
            ("atomos", "held", &[]),
            ("kunai", "auto", &[]),
            ("mk1_kunai", "auto", &[]),
            ("torid", "semi_auto", &["semi_rifle_cannonade"]), // semi-auto launcher, rifle pool
            // THE ASSAULT RIFLES (2026-08-05). Semi-Pistol/Shotgun Cannonade
            // are pistol and shotgun mods, so a rifle never sees them; the
            // RIFLE one turns on the listed trigger, which is the whole point
            // of having both an auto and a semi-auto rifle in the batch.
            ("gotva_prime", "auto", &[]),                      // full-auto rifle
            // THE BRATON FAMILY (2026-08-08). Full-auto in both forms, so no
            // Cannonade fits any of the four — and that is the point of listing
            // them: one adapter, four weapons, and the trigger answer is the
            // weapon's rather than the adapter's.
            // THE BOLTOR FAMILY (2026-08-08). Full-auto nail guns, so no Cannonade
            // — and the Incarnon form is a pseudo-shotgun, which changes the
            // multishot and not the trigger.
            ("boltor", "auto", &[]),
            ("telos_boltor", "auto", &[]),
            ("boltor_prime", "auto", &[]),
            ("braton", "auto", &[]),
            ("mk1_braton", "auto", &[]),
            ("braton_vandal", "auto", &[]),
            ("braton_prime", "auto", &[]),
            ("karak_wraith", "auto", &[]),                     // full-auto rifle
            ("prisma_grinlok", "semi_auto", &["semi_rifle_cannonade"]),
            // THE LATRON FAMILY (2026-08-08). Semi-auto marksman rifles, and
            // they STAY semi-auto through the transformation — the Incarnon
            // form trades hit-scan for a ricochet projectile at a lower rate,
            // not for a different trigger. So the mod fits and keeps fitting,
            // which is the opposite of the Phenmor's answer and the reason both
            // are listed.
            ("latron", "semi_auto", &["semi_rifle_cannonade"]),
            ("latron_wraith", "semi_auto", &["semi_rifle_cannonade"]),
            ("latron_prime", "semi_auto", &["semi_rifle_cannonade"]),
            // A BEAM shotgun, and its alt-fire does not change the answer: a
            // CHARGED form is not a second firing mode (`is_gauge_switched`
            // draws that line), so the weapon's listed trigger is Held and no
            // Cannonade fits.
            ("phantasma_prime", "held", &[]),
            // A NATURAL Incarnon whose two forms disagree about the trigger:
            // Semi in the hand, Auto once transmuted. The listed trigger is
            // the base form's, so the mod FITS — and installing tier 1 takes
            // it off again, because the rule is asked of every firing mode and
            // the Incarnon form is one. Exactly the Dual Toxocyst's shape, on
            // the rifle side of the pool for the first time.
            ("phenmor", "semi_auto", &["semi_rifle_cannonade"]),
            // BURST is its own trigger family, and this is where that claim
            // is cashed: the Semi-* mods gate on the LISTED trigger, the wiki
            // lists the Burston as "Burst", so it takes no Cannonade at all —
            // not even the rifle one, which every other rifle here argues
            // about. Firing three rounds a pull is not being semi-auto.
            ("burston", "burst", &[]),
            ("burston_prime", "burst", &[]),
            ("dual_toxocyst", "semi_auto", &["semi_pistol_cannonade"]),
            ("laetum", "semi_auto", &["semi_pistol_cannonade"]),
            // A BEAM pistol: held, so no Cannonade — the roster's first
            // weapon to make `continuous` and the PISTOL pool meet.
            ("ocucor", "held", &[]),
            // FULL-AUTO pistols, so no Cannonade — Semi-Pistol Cannonade gates
            // on the listed Semi-Auto trigger and these are Auto. They are the
            // first pair in the roster that differs only in NUMBERS: same
            // magazine, reserve, reload, accuracy and crit, and a status
            // chance of 12% against 1%.
            ("furis", "auto", &[]),
            ("mk1_furis", "auto", &[]),
            ("verglas_prime", "held", &[]),                    // continuous sentinel weapon
        ];
        let roster: Vec<&str> =
            crate::weapons_data::roster().map(|s| s.id.as_str()).collect();
        assert_eq!(
            roster.len(),
            EXPECTED.len(),
            "a weapon joined the roster and nobody said which Cannonade it takes: {roster:?}"
        );
        for (id, trigger, want) in EXPECTED {
            assert!(roster.contains(&id), "{id} is in the roster");
            // The trigger is half the claim, so it is pinned too — the table
            // would otherwise pass by agreeing with changed data.
            assert_eq!(
                crate::weapons_data::spec(id).unwrap().attack.trigger,
                trigger,
                "{id}'s listed trigger"
            );
            let got: Vec<&str> = pool_for_weapon(id)
                .iter()
                .map(|m| m.id)
                .filter(|m| CANNONADES.contains(m))
                .collect();
            assert_eq!(got, want, "{id}");
        }
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
        const PVE_LEGAL: [&str; 12] = [
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
            ModEffect::ProcConversion { from: DamageType::Impact, to: DamageType::Slash, chance, low_rate_threshold, low_rate_mult }
                if (*chance - 0.35).abs() < 1e-9 && (*low_rate_threshold - 2.5).abs() < 1e-9 && (*low_rate_mult - 2.0).abs() < 1e-9)));
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
}
