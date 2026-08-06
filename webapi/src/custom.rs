//! Visitor-authored custom content: weapons and mods.
//!
//! A custom weapon's panel IS the request — no data entry behind it, no
//! hidden passives, no evolutions. A custom mod is a card the visitor
//! defines by hand (arbitrary effects, repeatable, unlike a riven's one-per-
//! weapon limit). Both travel inline with every request (`custom_weapon` /
//! `custom_mods`); nothing is stored server-side.
//!
//! The rest of `webapi` consumes these through the SAME paths a roster
//! weapon or mod uses: `weapons()` chains [`custom_weapon_infos`], so
//! `weapon()`/`meta_json`/`riven_class` answer for `custom:*` ids without
//! their own branches, and `mod_pool_with_custom` (lib.rs) appends the
//! request's cards to the ordinary pool.

use serde_json::Value;
use wfsim_engine::damage::{DamageType, DamageVector};
use wfsim_engine::loadout::{
    CondBucket, Faction, IndirectStat, ModDef, ModEffect, Rarity, TennoCondition, WeaponBase,
};
use wfsim_engine::mods::Polarity;

/// The five equipment slots a custom weapon can be, and the only ids under
/// `custom:` the app recognizes. `custom_slot_of` is the inverse of
/// `custom:primary` → `"primary"`, and the slot drives the mod pool and the
/// riven class the same way a roster weapon's data would.
pub const CUSTOM_SLOTS: [&str; 5] = ["primary", "secondary", "shotgun", "archgun", "sentinel"];

pub fn is_custom_id(id: &str) -> bool {
    id.starts_with("custom:") && custom_slot_of(id).is_some()
}

pub fn custom_slot_of(id: &str) -> Option<&'static str> {
    let slot = id.strip_prefix("custom:")?;
    CUSTOM_SLOTS.iter().find(|s| **s == slot).copied()
}

/// The weapon's trigger, with its trait consequences: `semi_auto` needs the
/// Semi-Auto mods' `requires`, `held` is a beam (`continuous`) like the
/// roster's held weapons, anything else is a plain automatic.
pub fn custom_trigger(v: &Value) -> Result<String, String> {
    let t = v
        .get("custom_weapon")
        .and_then(|c| c.get("trigger"))
        .and_then(|x| x.as_str())
        .unwrap_or("auto");
    match t {
        "auto" | "semi_auto" | "held" => Ok(t.to_string()),
        other => Err(format!(
            "custom weapon: unknown trigger {other:?} (auto/semi_auto/held)"
        )),
    }
}

fn traits_of(trigger: &str) -> &'static [&'static str] {
    match trigger {
        "semi_auto" => &["semi_auto"],
        "held" => &["continuous"],
        _ => &["auto"],
    }
}

/// The visitor's riven disposition for the custom weapon (0.5..=1.55, the
/// game's own five-档 band). It rides the request — the static entry in
/// [`custom_weapon_infos`] only carries the neutral 1.0 for meta display.
pub fn custom_disposition(v: &Value) -> Result<f64, String> {
    let d = match v
        .get("custom_weapon")
        .and_then(|c| c.get("disposition"))
    {
        // Absent = the neutral 1.0; present but not a number = a typo.
        None => 1.0,
        Some(x) => x
            .as_f64()
            .ok_or("custom weapon: disposition must be a number 0.5..=1.55")?,
    };
    if !d.is_finite() || !(0.5..=1.55).contains(&d) {
        return Err("custom weapon: disposition must be 0.5..=1.55".into());
    }
    Ok(d)
}

/// The disposition a riven calculation must use for THIS weapon: the
/// visitor's own for a custom weapon, the data's for the roster.
pub fn disposition_of(v: &Value, info: &crate::WeaponInfo) -> f64 {
    if is_custom_id(&info.id) {
        custom_disposition(v).unwrap_or(1.0) // validated earlier at every entry
    } else {
        info.disposition
    }
}

/// The mod pool(s) a custom weapon of this slot draws from — the SAME union
/// the roster's representative weapons declare, so the picker, the optimizer
/// and the riven class all answer for it without custom-specific branches.
pub fn custom_pool_for(slot: &str, trigger: &str) -> Vec<ModDef> {
    let pools: &[&str] = match slot {
        "secondary" => &["pistol"],
        "shotgun" => &["primary", "shotgun"],
        "archgun" => &["archgun"],
        "sentinel" => &["rifle"],
        _ => &["primary", "rifle"],
    };
    // The same equip rules the roster's pool_for_build enforces, asked of the
    // visitor's chosen trigger: Sinister Reach only on held/continuous,
    // Cannonades only on semi-auto, unknown requirements hide the mod, and
    // Amalgam-class mods never equip on a sentinel (wiki "This mod cannot be
    // equipped on Sentinel weapons").
    let continuous = trigger == "held";
    let semi_auto = trigger == "semi_auto";
    wfsim_engine::mods_data::pool_union(&pools.iter().map(|s| s.to_string()).collect::<Vec<_>>())
        .into_iter()
        .filter(|m| match m.requires_weapon {
            None => true,
            Some("continuous") => continuous,
            Some("semi_auto") => semi_auto,
            Some(_) => false,
        })
        .filter(|m| !(slot == "sentinel" && m.excludes_weapon.contains(&"sentinel_weapon")))
        .collect()
}

/// True when a mod does NOTHING but raise ammo capacity — the only shape the
/// "no ammo pool" rule drops (a dual-stat keeps its other half).
pub fn only_ammo_max(m: &ModDef) -> bool {
    !m.effects.is_empty()
        && m.effects.iter().all(|e| {
            matches!(
                e,
                wfsim_engine::loadout::ModEffect::Indirect(
                    wfsim_engine::loadout::IndirectStat::AmmoMax,
                    _
                )
            )
        })
}

/// The five static entries the app treats as weapons: `custom:primary` …
/// `custom:sentinel`. Appended to `weapons()` so `weapon()`, `meta_json`
/// and `riven_class` answer for them with no branches of their own.
pub fn custom_weapon_infos() -> &'static [crate::WeaponInfo] {
    use std::sync::OnceLock;
    static C: OnceLock<Vec<crate::WeaponInfo>> = OnceLock::new();
    C.get_or_init(|| {
        CUSTOM_SLOTS
            .iter()
            .map(|slot| {
                let pools: Vec<String> = match *slot {
                    "secondary" => vec!["pistol".into()],
                    "shotgun" => vec!["primary".into(), "shotgun".into()],
                    "archgun" => vec!["archgun".into()],
                    "sentinel" => vec!["rifle".into()],
                    _ => vec!["primary".into(), "rifle".into()],
                };
                crate::WeaponInfo {
                    id: format!("custom:{slot}"),
                    name: format!("Custom {slot}"),
                    mod_pools: pools,
                    // The BASE form's trigger decides beam-ness; the default
                    // form is auto, and the visitor's `trigger` may make it
                    // held when the panel says so (meta reads this statically).
                    continuous: false,
                    disposition: 1.0,
                    subtype: "Custom".into(),
                    sentinel: *slot == "sentinel",
                    forms: vec![("Custom", "Custom".to_string(), true)],
                    has_cycle: false,
                    uses_arcane: false,
                    slot: slot.to_string(),
                    arcane_pools: Vec::new(),
                    uses_evo2: false,
                }
            })
            .collect()
    })
}

/// Build the weapon the request describes. Every number is the visitor's own
/// stated contract — a typo is heard, not absorbed — so validation is
/// explicit and fail-fast: base damage positive, every value finite and
/// bounded, fire rate strictly positive (a ≤0 rate sends the engagement
/// loop's pacing negative and it never ends).
pub fn custom_weapon_from(v: &Value) -> Result<WeaponBase, String> {
    let c = v
        .get("custom_weapon")
        .ok_or("custom weapon: missing custom_weapon object")?;
    let p = c.get("panel").ok_or("custom weapon: missing panel")?;
    let num = |k: &str, dflt: f64| -> Result<f64, String> {
        match p.get(k) {
            Some(x) => {
                let n = x
                    .as_f64()
                    .ok_or_else(|| format!("custom weapon panel: {k} must be a number"))?;
                if !n.is_finite() || n.abs() > 1e9 {
                    return Err(format!(
                        "custom weapon panel: {k} must be finite and |x| <= 1e9"
                    ));
                }
                Ok(n)
            }
            None => Ok(dflt),
        }
    };
    let mut base_vector = DamageVector::new();
    for (k, t) in [
        ("impact", DamageType::Impact),
        ("puncture", DamageType::Puncture),
        ("slash", DamageType::Slash),
        ("heat", DamageType::Heat),
        ("cold", DamageType::Cold),
        ("electricity", DamageType::Electricity),
        ("toxin", DamageType::Toxin),
        // Compound elements a weapon can CARRY innately (a Kuva weapon's
        // native bonus, or the visitor's own hypothetical). They enter the
        // base vector, skip the combination hierarchy, and print as-is.
        ("blast", DamageType::Blast),
        ("corrosive", DamageType::Corrosive),
        ("gas", DamageType::Gas),
        ("magnetic", DamageType::Magnetic),
        ("radiation", DamageType::Radiation),
        ("viral", DamageType::Viral),
    ] {
        let x = num(k, 0.0)?;
        if x > 0.0 {
            base_vector.add(t, x);
        }
    }
    if base_vector.total() <= 0.0 {
        return Err("custom weapon panel: base damage must be positive".into());
    }
    let trigger = custom_trigger(v)?;
    // The riven disposition is part of the stated contract: out of the
    // game's band is an error, not a silently-clamped guess.
    custom_disposition(v)?;
    let base_fire_rate = num("fire_rate", 1.0)?;
    if !(base_fire_rate > 0.0 && base_fire_rate <= 1000.0) {
        return Err("custom weapon panel: fire_rate must be in (0, 1000]".into());
    }
    let base_multishot = num("multishot", 1.0)?;
    if !(base_multishot > 0.0 && base_multishot <= 100.0) {
        return Err("custom weapon panel: multishot must be in (0, 100]".into());
    }
    let magazine_size = num("magazine", 50.0)?;
    if magazine_size <= 0.0 {
        return Err("custom weapon panel: magazine must be > 0".into());
    }
    let base_reload = num("reload", 2.0)?;
    if base_reload <= 0.0 {
        return Err("custom weapon panel: reload must be > 0".into());
    }
    let ammo_cost = num("ammo_cost", 1.0)?;
    if ammo_cost < 0.0 {
        return Err("custom weapon panel: ammo_cost must be >= 0".into());
    }
    let ammo_reserve = num("ammo_reserve", 0.0)?;
    Ok(WeaponBase {
        indirect: Vec::new(),
        base_vector,
        base_crit_chance: num("crit_chance", 0.05)?,
        base_crit_damage: num("crit_damage", 2.0)?,
        base_status_chance: num("status_chance", 0.05)?,
        base_fire_rate,
        charge_seconds: None,
        ammo_cost,
        headshot_bonus_multiplicative: false,
        fire_rate_shortens_draw: true,
        charge_cadence: Default::default(),
        fire_rate_mod_multiplier: 1.0,
        base_multishot,
        reload_damage_buff: 0.0,
        buff_multishot_bonus: 0.0,
        buff_ms_max_stacks: 0,
        magazine_size,
        ammo_reserve,
        has_reserve: ammo_reserve > 0.0,
        no_resupply: false,
        base_reload,
        innate_co_per_type: 0.0,
        co_behavior: Default::default(),
        co_base_fraction: 1.0,
        injected_elements: Vec::new(),
        traits: traits_of(&trigger),
        incarnon: None,
        evo_fire_rate_bonus: 0.0,
        post_mod_crit_chance: 0.0,
        post_mod_status_chance: 0.0,
        headshot_damage_bonus: 0.0,
        noncrit_bonus: None,
        plain_hit_bonus: None,
        reload_on_headshot: None,
        radial: None,
        lingering: None,
        continuous: trigger == "held",
        field_duration_on_empty_reload: 0.0,
        beam: None,
        multishot_on_last_round: 0.0,
        multishot_ammo_bonus: 0.0,
    })
}

// ---- custom mods ----------------------------------------------------------

/// Parse the request's `custom_mods` array into cards. Every failure names
/// the card and the field — a broken card is an error, never a silent drop.
pub fn custom_mods_from(v: &Value) -> Result<Vec<ModDef>, String> {
    let Some(arr) = v.get("custom_mods").and_then(|x| x.as_array()) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(arr.len());
    // Two cards under one name are one id — refuse instead of letting the
    // pool carry two definitions for the same slot reference.
    let mut names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, card) in arr.iter().enumerate() {
        let name = card
            .get("name")
            .and_then(|x| x.as_str())
            .ok_or_else(|| format!("custom_mods[{i}]: missing name"))?
            .to_string();
        if !names.insert(name.clone()) {
            return Err(format!("custom_mods[{i}] ({name}): duplicate card name"));
        }
        let effects_raw = card
            .get("effects")
            .and_then(|x| x.as_array())
            .ok_or_else(|| format!("custom_mods[{i}] ({name}): missing effects array"))?;
        if effects_raw.is_empty() {
            return Err(format!("custom_mods[{i}] ({name}): at least one effect is required"));
        }
        let polarity = match card
            .get("polarity")
            .and_then(|x| x.as_str())
            .unwrap_or("madurai")
        {
            "vazarin" => Polarity::Vazarin,
            "naramon" => Polarity::Naramon,
            "umbra" => Polarity::Umbra,
            "madurai" => Polarity::Madurai,
            other => {
                return Err(format!(
                    "custom_mods[{i}] ({name}): unknown polarity {other:?} (madurai/vazarin/naramon/umbra)"
                ))
            }
        };
        let base_drain = match card.get("base_drain").and_then(|x| x.as_u64()) {
            Some(d) => d as u32,
            // A non-integer (negative, float, string) is a typo — name it,
            // rather than silently costing 10 like the note at the top says
            // every failure is heard.
            None => {
                return Err(format!(
                    "custom_mods[{i}] ({name}): base_drain must be an integer 0..=100"
                ))
            }
        };
        if base_drain > 100 {
            return Err(format!("custom_mods[{i}] ({name}): base_drain must be <= 100"));
        }
        let exilus = card.get("exilus").and_then(|x| x.as_bool()).unwrap_or(false);
        let mut trigger_seen: Vec<&'static str> = Vec::new();
        let mut effects = Vec::with_capacity(effects_raw.len());
        for e in effects_raw {
            effects.push(
                custom_effect(e, &mut trigger_seen)
                    .map_err(|err| format!("custom_mods[{i}] ({name}): {err}"))?,
            );
        }
        out.push(ModDef {
            unmodeled: false,
            out_of_scope: false,
            id: crate::intern(format!("custom:{name}")),
            name: crate::intern(name.clone()),
            base_drain,
            max_rank: 0,
            polarity,
            rarity: Rarity::Legendary,
            exilus,
            family: None,
            requires_weapon: None,
            excludes_weapon: Vec::new(),
            set: None,
            requires: None,
            disables: Vec::new(),
            effects,
        });
    }
    Ok(out)
}

/// One custom-card effect as a `ModEffect`. `trigger_seen` tracks the
/// singleton trigger kinds (their emergent resolution overwrites, so a
/// duplicate would silently resolve to the last one) — at most one per card.
fn custom_effect(v: &Value, trigger_seen: &mut Vec<&'static str>) -> Result<ModEffect, String> {
    let kind = v
        .get("kind")
        .and_then(|x| x.as_str())
        .ok_or("effect missing kind")?;
    let ratio = |what: &str| custom_ratio(v.get(what).unwrap_or(&Value::Null), what);
    let mut single = |name: &'static str| -> Result<(), String> {
        if trigger_seen.contains(&name) {
            return Err(format!(
                "{name} is a singleton trigger effect — at most one per card"
            ));
        }
        trigger_seen.push(name);
        Ok(())
    };
    let dmg = || -> Result<DamageType, String> {
        v.get("type")
            .and_then(|x| x.as_str())
            .and_then(custom_damage_type)
            .ok_or("type must be impact/puncture/slash/heat/cold/electricity/toxin".to_string())
    };
    // Each element kind accepts only its own family — a "physical" slot must
    // not smuggle an element into the combine step (elements::combined_of
    // asserts two primaries; Impact+Heat panics), and an element must not
    // pretend to be physical damage.
    let elem = || -> Result<DamageType, String> {
        v.get("type").and_then(|x| x.as_str()).and_then(|s| match s {
            "heat" => Some(DamageType::Heat),
            "cold" => Some(DamageType::Cold),
            "electricity" => Some(DamageType::Electricity),
            "toxin" => Some(DamageType::Toxin),
            _ => None,
        })
        .ok_or("element type must be heat/cold/electricity/toxin".to_string())
    };
    let phys = || -> Result<DamageType, String> {
        v.get("type").and_then(|x| x.as_str()).and_then(|s| match s {
            "impact" => Some(DamageType::Impact),
            "puncture" => Some(DamageType::Puncture),
            "slash" => Some(DamageType::Slash),
            _ => None,
        })
        .ok_or("physical type must be impact/puncture/slash".to_string())
    };
    let combined = || -> Result<DamageType, String> {
        v.get("type").and_then(|x| x.as_str()).and_then(|s| match s {
            "blast" => Some(DamageType::Blast),
            "corrosive" => Some(DamageType::Corrosive),
            "gas" => Some(DamageType::Gas),
            "magnetic" => Some(DamageType::Magnetic),
            "radiation" => Some(DamageType::Radiation),
            "viral" => Some(DamageType::Viral),
            _ => None,
        })
        .ok_or("combined_element type must be blast/corrosive/gas/magnetic/radiation/viral".to_string())
    };
    Ok(match kind {
        // ---- ratio-valued (repeatable; the buckets are additive) ----
        "base_damage" => ModEffect::BaseDamage(ratio("value")?),
        "multishot" => ModEffect::Multishot(ratio("value")?),
        "crit_chance" => ModEffect::CritChance(ratio("value")?),
        "crit_damage" => ModEffect::CritDamage(ratio("value")?),
        "status_chance" => ModEffect::StatusChance(ratio("value")?),
        "fire_rate" => ModEffect::FireRate(ratio("value")?),
        "charge_rate" => ModEffect::ChargeRate(ratio("value")?),
        "reload_speed" => ModEffect::ReloadSpeed(ratio("value")?),
        "status_damage" => ModEffect::StatusDamage(ratio("value")?),
        "status_duration" => ModEffect::StatusDuration(ratio("value")?),
        "magazine_capacity" => ModEffect::MagazineCapacity(ratio("value")?),
        "slash_on_crit" => ModEffect::SlashOnCrit(ratio("value")?),
        "blast_radius" => ModEffect::BlastRadius(ratio("value")?),
        "weakpoint_damage" => ModEffect::WeakpointDamage(ratio("value")?),
        "weakpoint_crit_chance" => ModEffect::WeakpointCritChance(ratio("value")?),
        "physical" => ModEffect::Physical(phys()?, ratio("value")?),
        "element" => ModEffect::Element(elem()?, ratio("value")?),
        "combined_element" => ModEffect::CombinedElement(combined()?, ratio("value")?),
        "faction_damage" => {
            let f = Faction::from_name(
                v.get("faction").and_then(|x| x.as_str()).unwrap_or(""),
            );
            if f == Faction::Unknown {
                return Err(
                    "faction_damage: unknown faction (grineer/corpus/infested/corrupted/murmur/sentient)"
                        .into(),
                );
            }
            ModEffect::FactionDamage(f, ratio("value")?)
        }
        "cond_buff" => {
            let bucket = match v.get("bucket").and_then(|x| x.as_str()) {
                Some("base_damage") => CondBucket::BaseDamage,
                Some("multishot") => CondBucket::Multishot,
                Some("crit_chance") => CondBucket::CritChance,
                Some("crit_damage") => CondBucket::CritDamage,
                Some("status_chance") => CondBucket::StatusChance,
                Some("status_damage") => CondBucket::StatusDamage,
                Some("fire_rate") => CondBucket::FireRate,
                Some("reload_speed") => CondBucket::ReloadSpeed,
                _ => {
                    return Err(
                        "cond_buff: unknown bucket (base_damage/multishot/crit_chance/crit_damage/status_chance/status_damage/fire_rate/reload_speed)"
                            .into(),
                    )
                }
            };
            ModEffect::CondBuff(bucket, ratio("value")?)
        }
        "indirect" => {
            let stat = match v.get("stat").and_then(|x| x.as_str()) {
                Some("recoil") => IndirectStat::Recoil,
                Some("noise") => IndirectStat::Noise,
                Some("ammo_max") => IndirectStat::AmmoMax,
                Some("projectile_speed") => IndirectStat::ProjectileSpeed,
                Some("holstered_reload") => IndirectStat::HolsteredReload,
                Some("dodge_speed") => IndirectStat::DodgeSpeed,
                Some("acrobatic_speed") => IndirectStat::AcrobaticSpeed,
                Some("accuracy") => IndirectStat::Accuracy,
                Some("punch_through") => IndirectStat::PunchThrough,
                Some("zoom") => IndirectStat::Zoom,
                Some("range") => IndirectStat::Range,
                Some("beam_range") => IndirectStat::BeamRange,
                Some("movement_speed") => IndirectStat::MovementSpeed,
                Some("sprint_speed") => IndirectStat::SprintSpeed,
                Some("ammo_conversion") => IndirectStat::AmmoConversion,
                Some("stagger_resist") => IndirectStat::StaggerResist,
                Some("self_stagger") => IndirectStat::SelfStagger,
                Some("double_jump") => IndirectStat::DoubleJump,
                Some("kill_explosion") => IndirectStat::KillExplosion,
                _ => {
                    return Err("indirect: unknown stat (recoil/noise/ammo_max/projectile_speed/holstered_reload/dodge_speed/acrobatic_speed/accuracy/punch_through/zoom/range/beam_range/movement_speed/sprint_speed/ammo_conversion/stagger_resist/self_stagger/double_jump/kill_explosion)".into())
                }
            };
            ModEffect::Indirect(stat, ratio("value")?)
        }
        // ---- singleton triggers (at most one of each kind per card) ----
        "on_kill_multishot" => {
            single("on_kill_multishot")?;
            ModEffect::OnKillMultishot {
                per_stack: ratio("per_stack")?,
                max_stacks: custom_stacks(
                    v.get("max_stacks").unwrap_or(&Value::Null),
                    "max_stacks",
                )?,
                duration: custom_duration(v.get("duration").unwrap_or(&Value::Null), "duration")?,
            }
        }
        "condition_overload" => {
            single("condition_overload")?;
            ModEffect::ConditionOverload {
                per_stack: ratio("per_stack")?,
                max_stacks: custom_stacks(
                    v.get("max_stacks").unwrap_or(&Value::Null),
                    "max_stacks",
                )?,
                duration: custom_duration(v.get("duration").unwrap_or(&Value::Null), "duration")?,
            }
        }
        "on_headshot_crit_chance" => {
            single("on_headshot_crit_chance")?;
            ModEffect::OnHeadshotCritChance {
                bonus: ratio("bonus")?,
                duration: custom_duration(v.get("duration").unwrap_or(&Value::Null), "duration")?,
            }
        }
        "on_headshot_kill_crit_chance" => {
            single("on_headshot_kill_crit_chance")?;
            ModEffect::OnHeadshotKillCritChance {
                per_stack: ratio("per_stack")?,
                max_stacks: custom_stacks(
                    v.get("max_stacks").unwrap_or(&Value::Null),
                    "max_stacks",
                )?,
                duration: custom_duration(v.get("duration").unwrap_or(&Value::Null), "duration")?,
            }
        }
        "on_kill_crit_damage" => {
            single("on_kill_crit_damage")?;
            ModEffect::OnKillCritDamage {
                bonus: ratio("bonus")?,
                duration: custom_duration(v.get("duration").unwrap_or(&Value::Null), "duration")?,
            }
        }
        "on_reload_fire_rate" => {
            single("on_reload_fire_rate")?;
            ModEffect::OnReloadFireRate {
                bonus: ratio("bonus")?,
                duration: custom_duration(v.get("duration").unwrap_or(&Value::Null), "duration")?,
            }
        }
        "on_reload_damage" => {
            single("on_reload_damage")?;
            ModEffect::OnReloadDamage {
                bonus: ratio("bonus")?,
                duration: custom_duration(v.get("duration").unwrap_or(&Value::Null), "duration")?,
            }
        }
        "proc_conversion" => {
            single("proc_conversion")?;
            ModEffect::ProcConversion {
                from: dmg()?,
                to: dmg()?,
                chance: custom_chance(v.get("chance").unwrap_or(&Value::Null), "chance")?,
                low_rate_threshold: ratio("low_rate_threshold")?,
                low_rate_mult: ratio("low_rate_mult")?,
            }
        }
        // ---- wrappers ----
        "on_equip_handling" => ModEffect::OnEquipHandling {
            recoil: ratio("recoil")?,
            accuracy: ratio("accuracy")?,
            duration: custom_duration(v.get("duration").unwrap_or(&Value::Null), "duration")?,
        },
        "while_tenno" => {
            let cond = match v.get("condition").and_then(|x| x.as_str()) {
                Some("aiming") => TennoCondition::Aiming,
                Some("invisible") => TennoCondition::Invisible,
                Some("airborne") => TennoCondition::Airborne,
                _ => return Err("while_tenno: condition must be aiming/invisible/airborne".into()),
            };
            let inner = v.get("inner").ok_or("while_tenno: missing inner effect")?;
            if inner.get("kind").and_then(|x| x.as_str()) == Some("while_tenno") {
                return Err("while_tenno cannot nest another while_tenno".into());
            }
            ModEffect::WhileTenno(cond, Box::new(custom_effect(inner, trigger_seen)?))
        }
        other => return Err(format!("unknown effect kind: {other}")),
    })
}

fn custom_damage_type(s: &str) -> Option<DamageType> {
    Some(match s {
        "impact" => DamageType::Impact,
        "puncture" => DamageType::Puncture,
        "slash" => DamageType::Slash,
        "heat" => DamageType::Heat,
        "cold" => DamageType::Cold,
        "electricity" => DamageType::Electricity,
        "toxin" => DamageType::Toxin,
        _ => return None,
    })
}

/// `CUSTOM_MAX_RATIO = 100.0` is the whole "±10000%" band, and the visitor
/// asks for more by repeating the effect — each card's effects add into the
/// same bucket, so ten copies of +1000% ARE +10000%, spelled ten times.
fn custom_ratio(v: &Value, what: &str) -> Result<f64, String> {
    let x = v
        .as_f64()
        .ok_or_else(|| format!("{what} must be a number"))?;
    if !x.is_finite() || x.abs() > 100.0 {
        return Err(format!(
            "{what} must be finite and |value| <= 100 (×100 = +10000%; more by repeating the effect)"
        ));
    }
    Ok(x)
}

fn custom_chance(v: &Value, what: &str) -> Result<f64, String> {
    let x = v
        .as_f64()
        .ok_or_else(|| format!("{what} must be a number"))?;
    if !x.is_finite() || !(0.0..=1.0).contains(&x) {
        return Err(format!("{what} must be 0..=1"));
    }
    Ok(x)
}

fn custom_duration(v: &Value, what: &str) -> Result<f64, String> {
    let x = v
        .as_f64()
        .ok_or_else(|| format!("{what} must be a number"))?;
    if !x.is_finite() || !(0.0..=1_000_000.0).contains(&x) {
        return Err(format!("{what} must be 0..=1000000"));
    }
    Ok(x)
}

fn custom_stacks(v: &Value, what: &str) -> Result<u32, String> {
    let x = v
        .as_u64()
        .ok_or_else(|| format!("{what} must be an integer"))?;
    if x > 1_000_000 {
        return Err(format!("{what} must be <= 1000000"));
    }
    Ok(x as u32)
}
