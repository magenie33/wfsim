//! Declarative Incarnon-evolution loader: `data/evolutions/*.yaml` -> the
//! evolution pool.
//!
//! Evolutions are DATA, not code (same pattern as [`crate::mods_data`] /
//! [`crate::arcanes_data`]): each yaml records the wiki-verified effects;
//! this module parses them into [`EvolutionDef`] and APPLIES a chosen set
//! onto a weapon's raw [`WeaponBase`] — evolutions alter BASE stats before
//! mods (flat base damage scales the vector pro-rata inside ModifiedBase;
//! Commodore's Fortune adds into the BASE crit chance that crit mods then
//! multiply). The engine previously hardcoded these numbers in the
//! `DtEvo2` enum; the enum remains as a selector, the values live here.

use std::sync::OnceLock;

use serde::Deserialize;
use serde_norway::Value;

use crate::loadout::WeaponBase;

#[derive(Debug, Deserialize)]
struct EvoFile {
    id: String,
    name: String,
    weapon: String,
    tier: u32,
    /// Wiki `File:` name for the evolution's icon.
    #[serde(default)]
    icon: Option<String>,
    /// Verbatim in-game/wiki effect text (evolutions have no ranks, so no
    /// X templating).
    #[serde(default)]
    description: Option<String>,
    /// Wiki-flagged non-functional evolutions apply NOTHING.
    #[serde(default)]
    currently_broken: bool,
    /// Does THIS evolution's flat base damage stay out of the weapon's GunCO
    /// term? The CO catalog names the offending perk explicitly, so the flag
    /// belongs to the perk, not to the weapon and not to the CO class.
    #[serde(default)]
    co_base_excludes_this_evolution: bool,
    effects: Vec<Value>,
}

/// One parsed evolution effect (the loader's vocabulary — kinds with no
/// single-target damage payload load as `Inert` so the evolution still
/// resolves and lists).
#[derive(Debug, Clone, PartialEq)]
enum EvoEffect {
    /// Adds to the BASE damage TOTAL, distributed pro-rata across the
    /// vector, BEFORE mods (inside ModifiedBase).
    FlatBaseDamage(f64),
    /// Adds into the BASE crit chance (crit mods multiply the new base).
    FlatBaseCritChance(f64),
    /// Adds into the BASE status chance — the same base-stat layer, so status
    /// mods multiply the new base (Torid's Survivor's Edge and Elemental
    /// Balance both say "Increase Base Status Chance"). NOT the post-mod flat
    /// layer that Elemental Excess occupies.
    FlatBaseStatusChance(f64),
    /// Adds whole rounds to the BASE magazine, before magazine mods (Torid's
    /// Extended Volley: +9 on a base of 5). Explicitly NOT the Incarnon form's
    /// charge-backed magazine — "Does not apply to Incarnon Form's Magazine" —
    /// which is why it lands on the base entry only.
    FlatBaseMagazine(f64),
    /// Renewed Horror: reloading from EMPTY arms a buff that multiplies the
    /// duration of the NEXT shot's lingering field. ✅ measured (M13): x2, so
    /// that field ticks 20 times instead of 10.
    FieldDurationOnEmptyReload(f64),
    /// Final Fusillade: a FLAT multishot add on the last round of the magazine,
    /// BASE FORM ONLY (user, 2026-07-30) — a charge-backed Incarnon magazine
    /// has no "last shot in magazine" to gate on, so `apply` drops it there.
    MultishotOnLastRound(f64),
    /// Plentiful Mayhem: multishot draws its extra rounds from ammo, and the
    /// projectiles it GENERATES deal +v damage as an independent multiplier.
    /// Affects both forms; the sim reads the per-form rule off `continuous`.
    MultishotConsumesAmmo(f64),
    /// A PERMANENT stacking multishot buff (Fevered Frenzy: on-ability-cast
    /// stacks with no timer, cleared only by death — so inside a sim run the
    /// stack count is a static CHOICE, full by default). `total` = the
    /// full-stack bonus (per_stack × max_stacks) that joins the weapon's
    /// buff multishot; `max_stacks` lets the per-buff config rescale it.
    AssumedMaxMultishot { total: f64, max_stacks: u32 },
    /// Unconditional CO rate (Carnage Reign): +v per status TYPE, additive
    /// with mod CO sources. `excludes_evolution_damage`: the GunCO base
    /// excludes evolution flat damage (wiki CO catalog, DT row).
    ConditionOverload { per_type: f64 },
    /// Fire-rate bonus in the ORDINARY additive bucket — the same one the
    /// fire-rate mods feed, so it SUMS with them (Rapid Wrath).
    FireRateBonus(f64),
    /// FLAT crit chance added AFTER mods (Elemental Excess: "Bonuses are
    /// added after mods as a flat value") — NOT the base-stat layer that
    /// Commodore's Fortune occupies.
    PostModCritChance(f64),
    /// FLAT status chance added after mods (Elemental Excess).
    PostModStatusChance(f64),
    /// Additive headshot-damage bonus (Caput Mortuum): joins the headshot
    /// bracket `(1 + Σ)` that multiplies the body-part multiplier.
    HeadshotDamage(f64),
    /// Devouring Attrition: on an instance that did NOT crit, `chance` to
    /// multiply it by `(1 + value)`. An INDEPENDENT multiplier ("multiplicative
    /// to base damage bonuses such as Hornet Strike") that applies to BOTH
    /// attack parts, the radial explosion included.
    ChanceDamageOnNoncrit { chance: f64, value: f64 },
    /// Incarnon gauge fill rate (Incarnon Efficiency): weakpoint hits build
    /// `1 + value` times the charge, so the hits needed to fill divide by it.
    IncarnonChargeRate(f64),
    /// Overwhelming Attrition: a hit that is NEITHER critical NOR applies a
    /// status grants a stack worth `+per_stack` damage for `duration`; on
    /// timeout ONE stack drops and the timer resets (the Galvanized decay,
    /// wiki-verbatim). The bonus is ADDITIVE to the base-damage bucket
    /// ("additive to base damage bonuses such as Hornet Strike") — unlike
    /// [`EvoEffect::ChanceDamageOnNoncrit`], which the same page calls
    /// multiplicative.
    StackingDamageOnPlainHit {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// Lethal Rearmament: every HEADSHOT grants a stack of reload speed
    /// for `duration`, one stack lost per timeout (the Galvanized decay).
    /// Reload speed also scales the Incarnon transmute animations, so this
    /// shortens the whole cycle, not just reloads.
    StackingReloadSpeedOnHeadshot {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// No damage payload here (holstered regen, recoil, timed utility
    /// buffs, the weapon unlock) — kept so the evolution loads and lists.
    Inert(String),
}

/// A parsed Incarnon evolution.
#[derive(Debug, Clone)]
pub struct EvolutionDef {
    pub id: String,
    pub name: String,
    pub weapon: String,
    pub tier: u32,
    /// Wiki `File:` name for the evolution's icon.
    pub icon: Option<String>,
    /// Verbatim effect text — what the cards display (like mods/arcanes).
    pub description: String,
    pub currently_broken: bool,
    /// This evolution's flat base damage does NOT feed the weapon's GunCO
    /// term. Dual Toxocyst's Carnage Reign is the only one: its catalog row
    /// reads "75 or 135 (with Evolution II Perk 1)" against a CO base of a
    /// flat 75. The wiki lists ONLY discrepant cases, so an evolution without
    /// this flag — including Dual Toxocyst's OTHER Evolution II option — feeds
    /// the CO term in full.
    pub co_base_excludes_this_evolution: bool,
    effects: Vec<EvoEffect>,
}

impl EvolutionDef {
    /// Σ flat base damage this evolution adds (0 when broken) — the panel
    /// attributes it as a non-mod source on the Base Damage row.
    pub fn flat_base_damage(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::FlatBaseDamage(v) => Some(*v),
                _ => None,
            })
            .sum()
    }

    /// Σ flat BASE crit chance (Commodore's Fortune; 0 when broken).
    pub fn flat_base_crit_chance(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::FlatBaseCritChance(v) => Some(*v),
                _ => None,
            })
            .sum()
    }

    /// Σ flat BASE status chance (Survivor's Edge, Elemental Balance).
    pub fn flat_base_status_chance(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::FlatBaseStatusChance(v) => Some(*v),
                _ => None,
            })
            .sum()
    }

    /// Σ flat BASE magazine rounds (Extended Volley).
    pub fn flat_base_magazine(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::FlatBaseMagazine(v) => Some(*v),
                _ => None,
            })
            .sum()
    }

    /// Σ assumed-max multishot from permanent stacks (Fevered Frenzy).
    pub fn assumed_multishot(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::AssumedMaxMultishot { total, .. } => Some(*total),
                _ => None,
            })
            .sum()
    }

    /// The permanent stacked-multishot buff, if this evolution grants one:
    /// (full-stack bonus, max stacks). Drives the configurable buff card.
    pub fn ms_buff(&self) -> Option<(f64, u32)> {
        self.active_effects().find_map(|e| match e {
            EvoEffect::AssumedMaxMultishot { total, max_stacks } => Some((*total, *max_stacks)),
            _ => None,
        })
    }
}

/// One configurable buff an evolution grants — everything the Sim's and
/// the Optimizer's buff cards need, with no caller-side knowledge of which
/// effect produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvoBuffCard {
    /// The `apply_buff_config` key this card writes.
    pub id: &'static str,
    pub max_stacks: u32,
    /// PERMANENT stacks (no in-sim trigger, no decay): the count is a
    /// static choice for the run, so the card defaults locked.
    pub permanent: bool,
}

impl EvolutionDef {
    /// EVERY configurable buff this evolution grants.
    ///
    /// The match below is EXHAUSTIVE on purpose: adding an `EvoEffect`
    /// variant fails to compile until someone states whether it is a buff
    /// the user can configure. That is the whole point — a buff that
    /// exists in the engine but not on the cards is invisible, and the
    /// only way to keep the two in step is to make forgetting impossible.
    /// Every stacking buff defaults to FULL stacks (the standing "start
    /// full" decision); only permanent ones default locked.
    pub fn buff_cards(&self) -> Vec<EvoBuffCard> {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::AssumedMaxMultishot { max_stacks, .. } => Some(EvoBuffCard {
                    id: "evo_multishot",
                    max_stacks: *max_stacks,
                    permanent: true,
                }),
                EvoEffect::StackingDamageOnPlainHit { max_stacks, .. } => Some(EvoBuffCard {
                    id: "on_plain_hit_damage",
                    max_stacks: *max_stacks,
                    permanent: false,
                }),
                EvoEffect::StackingReloadSpeedOnHeadshot { max_stacks, .. } => Some(EvoBuffCard {
                    id: "on_headshot_reload_speed",
                    max_stacks: *max_stacks,
                    permanent: false,
                }),
                // Static stat changes — nothing to configure at runtime.
                EvoEffect::FlatBaseDamage(_)
                | EvoEffect::FlatBaseCritChance(_)
                | EvoEffect::FlatBaseStatusChance(_)
                | EvoEffect::FlatBaseMagazine(_)
                | EvoEffect::FieldDurationOnEmptyReload(_)
                | EvoEffect::MultishotOnLastRound(_)
                | EvoEffect::MultishotConsumesAmmo(_)
                | EvoEffect::ConditionOverload { .. }
                | EvoEffect::FireRateBonus(_)
                | EvoEffect::PostModCritChance(_)
                | EvoEffect::PostModStatusChance(_)
                | EvoEffect::HeadshotDamage(_)
                | EvoEffect::IncarnonChargeRate(_) => None,
                // Rolled per instance, not a buff with an uptime.
                EvoEffect::ChanceDamageOnNoncrit { .. } => None,
                EvoEffect::Inert(_) => None,
            })
            .collect()
    }
}

impl EvolutionDef {

    /// The stacking on-plain-hit damage buff (Overwhelming Attrition), if
    /// this evolution grants one. Drives its configurable buff card.
    pub fn plain_hit_buff(&self) -> Option<crate::loadout::PlainHitBuff> {
        self.active_effects().find_map(|e| match e {
            EvoEffect::StackingDamageOnPlainHit {
                per_stack,
                max_stacks,
                duration,
            } => Some(crate::loadout::PlainHitBuff {
                per_stack: *per_stack,
                max_stacks: *max_stacks,
                duration: *duration,
                initial_stacks: *max_stacks,
                pinned: false,
            }),
            _ => None,
        })
    }

    /// Σ unconditional CO rate per status type (Carnage Reign).
    pub fn co_per_type(&self) -> f64 {
        self.active_effects()
            .filter_map(|e| match e {
                EvoEffect::ConditionOverload { per_type } => Some(*per_type),
                _ => None,
            })
            .sum()
    }

    fn active_effects(&self) -> impl Iterator<Item = &EvoEffect> {
        // Broken evolutions contribute nothing (same rule as `apply`).
        self.effects
            .iter()
            .filter(move |_| !self.currently_broken)
    }

    /// One display line per effect — what the model computes (broken
    /// evolutions state the zero honestly at the call site, not here).
    pub fn describe(&self) -> Vec<String> {
        self.effects
            .iter()
            .map(|e| match e {
                EvoEffect::FlatBaseDamage(v) => {
                    format!("+{v:.0} base damage (pro-rata, before mods)")
                }
                EvoEffect::FlatBaseCritChance(v) => {
                    format!("+{:.0}% BASE crit chance (crit mods multiply it)", v * 100.0)
                }
                EvoEffect::FlatBaseStatusChance(v) => format!(
                    "+{:.0}% BASE status chance (status mods multiply it)",
                    v * 100.0
                ),
                EvoEffect::FlatBaseMagazine(v) => {
                    format!("+{v:.0} base magazine (magazine mods multiply it)")
                }
                EvoEffect::FieldDurationOnEmptyReload(v) => format!(
                    "On reload from empty: x{v:.0} lingering-field duration on the next shot"
                ),
                EvoEffect::MultishotOnLastRound(v) => {
                    format!("+{v:.0} multishot on the last round of the magazine (base form only)")
                }
                EvoEffect::MultishotConsumesAmmo(v) => format!(
                    "+{:.0}% damage on multishot-generated projectiles; multishot consumes ammo",
                    v * 100.0
                ),
                EvoEffect::AssumedMaxMultishot { total, max_stacks } => format!(
                    "+{:.0}% multishot ({max_stacks} on-ability-cast stacks, full by default)",
                    total * 100.0
                ),
                EvoEffect::ConditionOverload { per_type } => format!(
                    "+{:.0}% direct damage per status type on the target",
                    per_type * 100.0
                ),
                EvoEffect::FireRateBonus(v) => format!("+{:.0}% fire rate", v * 100.0),
                EvoEffect::PostModCritChance(v) => format!(
                    "{}{:.0}% crit chance, flat AFTER mods",
                    if *v >= 0.0 { "+" } else { "" },
                    v * 100.0
                ),
                EvoEffect::PostModStatusChance(v) => format!(
                    "{}{:.0}% status chance, flat AFTER mods",
                    if *v >= 0.0 { "+" } else { "" },
                    v * 100.0
                ),
                EvoEffect::HeadshotDamage(v) => {
                    format!("+{:.0}% headshot damage (direct hits only)", v * 100.0)
                }
                EvoEffect::StackingReloadSpeedOnHeadshot {
                    per_stack,
                    max_stacks,
                    duration,
                } => format!(
                    "+{:.0}% reload speed per headshot ({max_stacks} stacks, {duration:.0}s) — shortens the transmutes too",
                    per_stack * 100.0
                ),
                EvoEffect::ChanceDamageOnNoncrit { chance, value } => format!(
                    "{:.0}% chance of +{:.0}% damage on a NON-crit instance (own multiplier, radial included)",
                    chance * 100.0,
                    value * 100.0
                ),
                EvoEffect::IncarnonChargeRate(v) => format!(
                    "weakpoint hits build +{:.0}% Incarnon charge",
                    v * 100.0
                ),
                EvoEffect::StackingDamageOnPlainHit {
                    per_stack,
                    max_stacks,
                    duration,
                } => format!(
                    "+{:.0}% damage per stack ({max_stacks} max, {duration:.0} s) on a hit that neither crits nor procs",
                    per_stack * 100.0
                ),
                EvoEffect::Inert(what) => {
                    format!("{} (no single-target DPS effect)", what.replace('_', " "))
                }
            })
            .collect()
    }
}

fn f(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}

fn effect(v: &Value) -> Option<EvoEffect> {
    let kind = v.get("kind").and_then(Value::as_str)?;
    Some(match kind {
        "flat_base_damage" => EvoEffect::FlatBaseDamage(f(v, "value").unwrap_or(0.0)),
        "flat_base_crit_chance" => EvoEffect::FlatBaseCritChance(f(v, "value").unwrap_or(0.0)),
        "flat_base_status_chance" => {
            EvoEffect::FlatBaseStatusChance(f(v, "value").unwrap_or(0.0))
        }
        "flat_base_magazine" => EvoEffect::FlatBaseMagazine(f(v, "value").unwrap_or(0.0)),
        "field_duration_on_empty_reload" => {
            EvoEffect::FieldDurationOnEmptyReload(f(v, "value").unwrap_or(1.0))
        }
        "multishot_on_last_round" => {
            EvoEffect::MultishotOnLastRound(f(v, "value").unwrap_or(0.0))
        }
        "multishot_consumes_ammo" => {
            EvoEffect::MultishotConsumesAmmo(f(v, "value").unwrap_or(0.0))
        }
        "stacking_buff" => {
            // Only the multishot payload is modeled (Fevered Frenzy);
            // other stacking payloads load inert until needed.
            let per = v
                .get("per_stack")
                .and_then(|p| p.get("multishot_bonus"))
                .and_then(Value::as_f64);
            let max = v.get("max_stacks").and_then(Value::as_u64).unwrap_or(0);
            match per {
                Some(p) => EvoEffect::AssumedMaxMultishot {
                    total: p * max as f64,
                    max_stacks: max as u32,
                },
                None => EvoEffect::Inert("stacking_buff (unmodeled payload)".into()),
            }
        }
        "condition_overload" => EvoEffect::ConditionOverload {
            per_type: f(v, "value").unwrap_or(0.0),
        },
        "fire_rate_bonus" => EvoEffect::FireRateBonus(f(v, "value").unwrap_or(0.0)),
        "flat_crit_chance_after_mods" => {
            EvoEffect::PostModCritChance(f(v, "value").unwrap_or(0.0))
        }
        "flat_status_chance_after_mods" => {
            EvoEffect::PostModStatusChance(f(v, "value").unwrap_or(0.0))
        }
        "headshot_damage" => EvoEffect::HeadshotDamage(f(v, "value").unwrap_or(0.0)),
        "chance_damage_on_noncrit" => EvoEffect::ChanceDamageOnNoncrit {
            chance: f(v, "chance").unwrap_or(0.0),
            value: f(v, "value").unwrap_or(0.0),
        },
        "incarnon_charge_rate" => EvoEffect::IncarnonChargeRate(f(v, "value").unwrap_or(0.0)),
        "stacking_damage_on_plain_hit" => EvoEffect::StackingDamageOnPlainHit {
            per_stack: f(v, "per_stack").unwrap_or(0.0),
            max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
            duration: f(v, "duration").unwrap_or(0.0),
        },
        "on_headshot_reload_speed" => EvoEffect::StackingReloadSpeedOnHeadshot {
            per_stack: f(v, "per_stack").unwrap_or(0.0),
            max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
            duration: f(v, "duration").unwrap_or(0.0),
        },
        other => EvoEffect::Inert(other.to_string()),
    })
}

/// Apply a chosen evolution set onto a weapon's RAW base panel.
///
/// Order-independent: flat base damage sums first, then the vector scales
/// pro-rata ONCE; `co_base_fraction` = original / evolved total — the wiki
/// CO-catalog rule that every GunCO source computes on the pre-evolution
/// base ("CO-bonus does not use base damage increase Evolution").
/// `currently_broken` evolutions apply nothing.
pub fn apply(base: &mut WeaponBase, evos: &[&EvolutionDef]) {
    let original_total = base.base_vector.total();
    let mut flat = 0.0;
    for e in evos.iter().filter(|e| !e.currently_broken) {
        for eff in &e.effects {
            match eff {
                EvoEffect::FlatBaseDamage(v) => flat += v,
                // A base-stat evolution is a WEAPON stat change, so it lands
                // on EVERY attack part, not just the direct hit. That is the
                // same reading `resolve` already applies to Elemental Excess's
                // post-mod layer ("a WEAPON stat change, so the explosion takes
                // it too"), and the base layer is the more clearly weapon-wide
                // of the two.
                //
                // INFERENCE, not a citation: no source states whether Torid's
                // Commodore's Fortune / Survivor's Edge / Elemental Balance
                // reach its Toxin cloud. It matters — the cloud is most of that
                // weapon's damage — so it is called out here and in MECHANICS.
                // Nothing else in the roster is affected: only Dual Toxocyst
                // (no radial, no field) and the Torid have base-stat
                // evolutions at all.
                EvoEffect::FlatBaseCritChance(v) => {
                    base.base_crit_chance += v;
                    if let Some(r) = base.radial.as_mut() {
                        r.base_crit_chance += v;
                    }
                    if let Some(f) = base.lingering.as_mut() {
                        f.base_crit_chance += v;
                    }
                }
                EvoEffect::FlatBaseStatusChance(v) => {
                    base.base_status_chance += v;
                    if let Some(r) = base.radial.as_mut() {
                        r.base_status_chance += v;
                    }
                    if let Some(f) = base.lingering.as_mut() {
                        f.base_status_chance += v;
                    }
                }
                // BASE FORM ONLY, and the gate is load-bearing: an Incarnon
                // form's `magazine_size` IS its charge pool (the pseudo-reload
                // rounds), so an ungated `+=` handed Extended Volley's +9 to
                // the 170-round gauge as well — "Does not apply to Incarnon
                // Form's Magazine" (wiki), and that magazine is outside the
                // ammo system entirely (user, 2026-07-30: it uses max charges).
                EvoEffect::FlatBaseMagazine(v) => {
                    if base.incarnon.is_none() {
                        base.magazine_size += v;
                    }
                }
                EvoEffect::FieldDurationOnEmptyReload(v) => {
                    base.field_duration_on_empty_reload = *v;
                }
                // BASE FORM ONLY: `incarnon.is_some()` marks the charge-backed
                // form, whose magazine is the gauge's round pool rather than a
                // reloaded magazine — nothing there is "the last round".
                EvoEffect::MultishotOnLastRound(v) => {
                    if base.incarnon.is_none() {
                        base.multishot_on_last_round = *v;
                    }
                }
                // "Affects both modes" — unlike Final Fusillade this one lands
                // on the charge-backed form too; what differs is the RULE, and
                // the sim picks that off `continuous`, not off the form id.
                EvoEffect::MultishotConsumesAmmo(v) => base.multishot_ammo_bonus = *v,
                EvoEffect::AssumedMaxMultishot { total, max_stacks } => {
                    base.buff_multishot_bonus += total;
                    base.buff_ms_max_stacks = base.buff_ms_max_stacks.max(*max_stacks);
                }
                EvoEffect::ConditionOverload { per_type } => {
                    base.innate_co_per_type += per_type;
                }
                EvoEffect::FireRateBonus(v) => base.evo_fire_rate_bonus += v,
                EvoEffect::PostModCritChance(v) => base.post_mod_crit_chance += v,
                EvoEffect::PostModStatusChance(v) => base.post_mod_status_chance += v,
                EvoEffect::HeadshotDamage(v) => base.headshot_damage_bonus += v,
                EvoEffect::ChanceDamageOnNoncrit { chance, value } => {
                    base.noncrit_bonus = Some((*chance, *value));
                }
                EvoEffect::IncarnonChargeRate(v) => {
                    if let Some(i) = base.incarnon.as_mut() {
                        i.charge_rate += v;
                    }
                }
                EvoEffect::StackingDamageOnPlainHit {
                    per_stack,
                    max_stacks,
                    duration,
                } => {
                    base.plain_hit_bonus = Some(crate::loadout::PlainHitBuff {
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        // Start FULL, like every other stacking buff
                        // (StackSpec, the arcanes): a build is read at the
                        // uptime it plays at, and the sim runs decay from
                        // there. The buff card overrides both knobs.
                        initial_stacks: *max_stacks,
                        pinned: false,
                    });
                }
                EvoEffect::StackingReloadSpeedOnHeadshot {
                    per_stack,
                    max_stacks,
                    duration,
                } => {
                    base.reload_on_headshot = Some(crate::loadout::HeadshotReloadBuff {
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        // Start FULL like every other stacking buff.
                        initial_stacks: *max_stacks,
                        pinned: false,
                    });
                }
                EvoEffect::Inert(_) => {}
            }
        }
    }
    if flat > 0.0 && original_total > 0.0 {
        let evolved = original_total + flat;
        base.base_vector = base.base_vector.scale(evolved / original_total);
        // The CO term keeps using the FULL evolved base — including a perk's
        // flat damage is the normal behaviour (user, 2026-07-30), and the Torid
        // counts its Incarnon perks in full.
        //
        // The exclusion belongs to the PERK, because that is the granularity
        // the CO catalog names: its Dual Toxocyst row reads "75 or 135 (with
        // Evolution II **Perk 1**)". The catalog lists only DISCREPANT cases,
        // so Perk 2 — Fevered Frenzy, which also raises base damage — is not
        // discrepant and feeds the CO term in full. Keying this off the weapon,
        // or off the Adding behaviour class, would silently dock Perk 2 too.
        if evos
            .iter()
            .any(|e| !e.currently_broken && e.co_base_excludes_this_evolution)
        {
            base.co_base_fraction = original_total / evolved;
        }
    }
}

/// Every embedded yaml under data/evolutions (cached).
pub fn pool() -> &'static Vec<EvolutionDef> {
    static POOL: OnceLock<Vec<EvolutionDef>> = OnceLock::new();
    POOL.get_or_init(|| {
        let mut out = Vec::new();
        for (path, text) in crate::data::files_under("evolutions/") {
            // The directory IS the table (data/README.md conventions):
            // everything under evolutions/ must parse as an evolution.
            let ef = serde_norway::from_str::<EvoFile>(text)
                .unwrap_or_else(|e| panic!("parse {path}: {e}"));
            // NAMING CONTRACT, enforced at load (user, 2026-07-29: full
            // weapon names, no abbreviations — long but unambiguous):
            //   id = "<weapon>_<evolution>"  and  filename = "<id>.yaml".
            // Scoping is NOT redundant with the `weapon:` field: evolution
            // NAMES repeat across weapons with different values (Marksman's
            // Hand is −50% recoil on Dual Toxocyst, −40% on Laetum), so the
            // id must carry the weapon. Deriving both the file name and the
            // prefix from it means the three can never drift apart.
            let stem = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".yaml");
            assert!(
                ef.id == stem,
                "{path}: id '{}' must match the filename",
                ef.id
            );
            assert!(
                ef.id.strip_prefix(&ef.weapon).is_some_and(|r| r.starts_with('_')),
                "{path}: id '{}' must start with the weapon id '{}_'",
                ef.id,
                ef.weapon
            );
            let effects = ef.effects.iter().filter_map(effect).collect();
            out.push(EvolutionDef {
                id: ef.id,
                name: ef.name,
                weapon: ef.weapon,
                tier: ef.tier,
                icon: ef.icon,
                description: ef.description.unwrap_or_default(),
                currently_broken: ef.currently_broken,
                co_base_excludes_this_evolution: ef.co_base_excludes_this_evolution,
                effects,
            });
        }
        out
    })
}

/// Look up an evolution by id.
pub fn get(id: &str) -> Option<&'static EvolutionDef> {
    pool().iter().find(|e| e.id == id)
}

/// A weapon's choosable options at a tier (the web picker's rows).
pub fn options(weapon: &str, tier: u32) -> Vec<&'static EvolutionDef> {
    pool()
        .iter()
        .filter(|e| e.weapon == weapon && e.tier == tier)
        .collect()
}

/// How many evolution tiers this weapon's data declares — the tier count
/// is per weapon (Dual Toxocyst has 4, Laetum has 5), so callers must
/// never assume a fixed range.
pub fn tier_count(weapon: &str) -> u32 {
    pool()
        .iter()
        .filter(|e| e.weapon == weapon)
        .map(|e| e.tier)
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_dt_evolution_pool() {
        let dt: Vec<_> = pool().iter().filter(|e| e.weapon == "dual_toxocyst").collect();
        assert!(dt.len() >= 9, "expected the 9 DT evolutions, got {}", dt.len());
        assert_eq!(options("dual_toxocyst", 2).len(), 2); // the EVO II choice
        // Broken evolutions carry the wiki flag.
        assert!(get("dual_toxocyst_ready_retaliation").unwrap().currently_broken);
        assert!(get("dual_toxocyst_neurotoxin").unwrap().currently_broken);
    }

    #[test]
    fn fevered_and_carnage_parse_their_wiki_values() {
        let fe = get("dual_toxocyst_fevered_frenzy").unwrap();
        assert!(fe.effects.contains(&EvoEffect::FlatBaseDamage(50.0)));
        assert!(fe
            .effects
            .contains(&EvoEffect::AssumedMaxMultishot { total: 1.0, max_stacks: 20 }));
        let ca = get("dual_toxocyst_carnage_reign").unwrap();
        assert!(ca.effects.contains(&EvoEffect::FlatBaseDamage(60.0)));
        assert!(ca.effects.contains(&EvoEffect::ConditionOverload { per_type: 0.33 }));
        let cf = get("dual_toxocyst_commodores_fortune").unwrap();
        assert!(cf.effects.contains(&EvoEffect::FlatBaseCritChance(0.20)));
    }

    #[test]
    fn broken_evolutions_apply_nothing() {
        use crate::loadout::WeaponBase;
        let with = WeaponBase::from_data("dual_toxocyst", false, &["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", "dual_toxocyst_fevered_frenzy"]);
        let mut probe = with.clone();
        apply(&mut probe, &[get("dual_toxocyst_ready_retaliation").unwrap()]);
        assert!((probe.base_vector.total() - with.base_vector.total()).abs() < 1e-9);
        assert_eq!(probe.base_crit_chance, with.base_crit_chance);
    }

    /// Final Fusillade is BASE FORM ONLY (user, 2026-07-30). Both forms load
    /// the SAME evolution id — the gate has to be the form, not the id, so this
    /// pins that the charge-backed form comes out with nothing.
    #[test]
    fn final_fusillades_last_round_multishot_skips_the_incarnon_form() {
        use crate::loadout::WeaponBase;
        let evos = ["torid_final_fusillade"];
        let base = WeaponBase::from_data("torid", false, &evos);
        let inc = WeaponBase::from_data("torid_incarnon", false, &evos);
        assert!(
            (base.multishot_on_last_round - 3.0).abs() < 1e-9,
            "base form got {}",
            base.multishot_on_last_round
        );
        assert_eq!(
            inc.multishot_on_last_round, 0.0,
            "a charge-backed magazine has no last round to gate on"
        );
        // The flat base damage on the same evolution DOES reach both forms —
        // otherwise this test would pass on a build that dropped the whole
        // evolution rather than just its conditional half.
        let bare = WeaponBase::from_data("torid_incarnon", false, &[]);
        assert!(inc.base_vector.total() > bare.base_vector.total());
    }

    /// Extended Volley: "Does not apply to Incarnon Form's Magazine", and that
    /// form uses max charges rather than a magazine (user, 2026-07-30). The
    /// gate is load-bearing because an Incarnon form's `magazine_size` IS its
    /// charge pool — an ungated `+=` quietly made it 179 rounds.
    #[test]
    fn extended_volley_leaves_the_charge_pool_alone() {
        use crate::loadout::WeaponBase;
        let evos = ["torid_extended_volley"];
        let base = WeaponBase::from_data("torid", false, &evos);
        let inc = WeaponBase::from_data("torid_incarnon", false, &evos);
        assert!((base.magazine_size - 14.0).abs() < 1e-9, "5 + 9 = {}", base.magazine_size);
        assert!(
            (inc.magazine_size - 170.0).abs() < 1e-9,
            "the charge pool must stay 170, got {}",
            inc.magazine_size
        );
    }
}
