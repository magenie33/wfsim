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

/// Map one YAML effect entry to a [`ModEffect`] at max rank (None = no damage
/// effect / not modeled — the mod still loads).
fn effect(v: &Value) -> Option<ModEffect> {
    let kind = v.get("kind").and_then(Value::as_str)?;
    let max = |k: &str| f(v, k).unwrap_or(0.0);
    // `condition: while_aiming` gates ANY effect, not only a triggered one.
    // Critical Focus is a flat crit bonus that simply does not exist unless
    // you are aiming — there is no event to wait for, so `kind: buff` (which
    // requires a trigger) cannot say it. The wrapper already existed; only
    // the data path was missing. The `buff` arm reads the same key itself,
    // for the effect it builds, and is skipped here so nothing double-wraps.
    let aim_gated = kind != "buff"
        && v.get("condition").and_then(Value::as_str) == Some("while_aiming");
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
            // `condition: while_aiming` wraps whatever this buff resolves to,
            // so the scenario can switch it off (loadout::resolve_with).
            let aim_gated = v.get("condition").and_then(Value::as_str) == Some("while_aiming");
            let per = max("rankMax"); // per-stack value at max rank
            let stacks = u(v, "max_stacks");
            let dur = f(v, "duration").unwrap_or(0.0);
            let wrap = |e: ModEffect| {
                if aim_gated {
                    ModEffect::WhileAiming(Box::new(e))
                } else {
                    e
                }
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
                ("on_kill", "crit_damage") => {
                    ModEffect::OnKillCritDamage { bonus: per, duration: dur }
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
                        _ => return None,
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
    Some(if aim_gated { ModEffect::WhileAiming(Box::new(out)) } else { out })
}

fn to_moddef(mf: ModFile) -> ModDef {
    let effects = mf.effects.iter().filter_map(effect).collect();
    ModDef {
        id: Box::leak(mf.id.into_boxed_str()),
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

/// The pool a weapon can actually EQUIP: its pools unioned, minus mods whose
/// equip requirement the weapon does not meet.
///
/// The compat tag is not the whole rule. Sinister Reach and Combustion Beam
/// are tagged PRIMARY and still cannot go on the Torid (user, 2026-07-31):
/// they need a CONTINUOUS weapon. The Torid is the case that shows where the
/// line falls — its Incarnon form IS a continuous beam and it still cannot
/// take them, because modding is decided on the BASE form, a semi-auto
/// grenade launcher.
pub fn pool_for_weapon(weapon_id: &str) -> Vec<ModDef> {
    let Some(spec) = crate::weapons_data::spec(weapon_id) else {
        return Vec::new();
    };
    // The BASE form's trigger, which is what `WeaponBase::continuous` reads.
    let continuous = spec.attack.trigger == "held";
    pool_union(&spec.mod_pools)
        .into_iter()
        .filter(|m| match m.requires_weapon {
            None => true,
            Some("continuous") => continuous,
            // An unknown requirement hides the mod rather than ignoring the
            // restriction — a mod offered where it cannot go is the worse bug.
            Some(_) => false,
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
            let mut xvals: Vec<(f64, f64)> = Vec::new();
            let mut ei: Option<usize> = None; // the effect the sentence is on
            for kind in crate::loadout::x_kinds(&desc) {
                use crate::loadout::XKind;
                // Seek forward to an effect that can answer this placeholder;
                // a `Value` always moves on, the others stay put once the
                // sentence has an effect to describe.
                let seek = |from: usize, pick: &dyn Fn(&Value) -> bool| {
                    (from..mf.effects.len()).find(|&i| pick(&mf.effects[i]))
                };
                let next = match kind {
                    XKind::Value => {
                        ei = seek(ei.map_or(0, |i| i + 1), &|e| varying(e).is_some());
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
    /// The authority is the wiki's `Rifle_Mods` / `Pistol_Mods` tables, which
    /// tag the genuinely restricted ones "Exclusive to PvP". This pins the
    /// survivors as an explicit allowlist, so neither mistake can be made
    /// silently: a new PvP-path mod fails until someone checks that table.
    #[test]
    fn only_pve_legal_conclave_mods_are_in_the_pools() {
        const PVE_LEGAL: [&str; 4] = ["agile_aim", "twitch", "eject_magazine", "reflex_draw"];
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
            ModEffect::WhileAiming(inner)
                if matches!(**inner, ModEffect::OnHeadshotKillCritChance { max_stacks: 5, .. }))));
        assert!(by("galvanized_crosshairs").effects.iter().all(|e| matches!(e, ModEffect::WhileAiming(_))),
            "every Galvanized Crosshairs effect is while-aiming");
        // ... and a mod with no condition is NOT wrapped.
        assert!(by("galvanized_diffusion").effects.iter().all(|e| !matches!(e, ModEffect::WhileAiming(_))));
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
            ModEffect::WhileAiming(inner)
                if matches!(**inner, ModEffect::OnKillCritDamage { bonus, duration }
                    if (bonus - 0.75).abs() < 1e-9 && (duration - 9.0).abs() < 1e-9))));
        assert!(by("pressurized_magazine").effects.iter().any(|e| matches!(e,
            ModEffect::WhileAiming(inner)
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
}
