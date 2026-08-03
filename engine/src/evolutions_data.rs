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

use crate::damage::DamageType;
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
    /// The same layer, but the two FORMS get different numbers. Boar's
    /// Elemental Balance reads "+12% per projectile" and "+96% for Incarnon
    /// Form" as two separate statements, not as a sum — a shotgun's pellet
    /// carries a twelfth of the status a beam tick does, so one number cannot
    /// serve both. Picked by `base.incarnon.is_some()`, the same gate
    /// `FlatBaseMagazine` uses.
    FlatBaseStatusChanceByForm { base: f64, incarnon: f64 },
    /// Adds into the BASE crit MULTIPLIER (Boar's Critical Parallel: "+0.5x").
    /// Base-stat layer like the crit-chance one above, so crit-damage mods
    /// multiply the new base.
    FlatBaseCritMultiplier(f64),
    /// Flat BASE damage that an empty reload turns on and nothing turns off —
    /// Boar's Reified Bane, "On Reload From Empty: Increase Base Damage by
    /// +14". It is applied UNCONDITIONALLY, i.e. the run is modelled as
    /// holding it from t = 0 (user, 2026-08-03: "我们也开头是1").
    ///
    /// That is the honest reading rather than a shortcut. The wiki states the
    /// bonus "lasts indefinitely until a manual reload is initiated while the
    /// magazine is not empty", and a sustained engagement empties the magazine
    /// every time — so the buff is up for all of a fight worth measuring. It
    /// is also the only shape this layer can take: base-damage evolutions are
    /// baked into `WeaponBase` BEFORE mods, while a runtime buff is applied to
    /// `DummyParams` after, so a configurable version would have to re-derive
    /// the panel mid-run.
    ///
    /// It stays its own variant rather than being folded into
    /// `FlatBaseDamage` so the card can say what it assumed, and so the day
    /// the sim can toggle it there is one place to change.
    FlatBaseDamageOnEmptyReload(f64),
    /// A handling / mobility / multi-target stat with no single-target damage
    /// payload — recoil, accuracy, punch through, projectile speed, holstered
    /// reload. It COUNTS: the value lands in the panel's `indirect` bucket
    /// beside the mods' (user, 2026-08-03: "什么后坐力，精准度，我们要纳计算，
    /// 只是目前完全不影响 dps 而已"). Mods were given this treatment on
    /// 2026-08-01; evolutions were still dropping the number on the floor.
    Indirect(crate::loadout::IndirectStat, f64),
    /// A buff a TRIGGER turns on for a while — the shape `kind: timed_buff`
    /// describes. It is PARSED and DESCRIBED here; whether it is also APPLIED
    /// depends on the payload, and `apply` says which for each.
    ///
    /// It used to be an anonymous `Inert("timed_buff")`, which meant the card
    /// showed nothing at all: the trigger, the window and the number were all
    /// read past. A perk the player picked has to say what it does even where
    /// the arena cannot price it.
    TimedBuff {
        trigger: TimedTrigger,
        duration: f64,
        payload: TimedPayload,
    },
    /// Sets the ammo RESERVE outright (Mercenary Chamber: "Increase Base Ammo
    /// Capacity to 195") — a set, not an add, so it cannot ride the additive
    /// indirect bucket.
    AmmoMaxSet(f64),
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
    /// `permanent` is the ONE thing this has to get right: a permanent buff
    /// has no trigger and no decay, so it survives a lull and starts full,
    /// while every timed buff starts EARNED at zero (docs/BUFFS.md).
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
                EvoEffect::FlatBaseStatusChanceByForm { .. }
                | EvoEffect::FlatBaseCritMultiplier(_)
                | EvoEffect::FlatBaseDamageOnEmptyReload(_)
                | EvoEffect::Indirect(..)
                | EvoEffect::AmmoMaxSet(_)
                // A timed buff IS configurable in principle, but neither of
                // today's two is APPLIED (see `apply`), and a card for a buff
                // that changes nothing is worse than no card.
                | EvoEffect::TimedBuff { .. }
                | EvoEffect::FlatBaseDamage(_)
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
                initial_stacks: 0, // EARNED — docs/BUFFS.md §Activation policy
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
                EvoEffect::FlatBaseStatusChanceByForm { base, incarnon } => format!(
                    "+{:.0}% BASE status chance ({:.0}% in Incarnon Form)",
                    base * 100.0,
                    incarnon * 100.0
                ),
                EvoEffect::FlatBaseCritMultiplier(v) => {
                    format!("+{v:.2}x BASE crit multiplier (crit damage mods multiply it)")
                }
                EvoEffect::Indirect(stat, v) => {
                    // Percent for the fractional stats, a bare number for the
                    // ones measured in their own unit (punch through: metres).
                    if matches!(stat, crate::loadout::IndirectStat::PunchThrough) {
                        format!("{:+.1} m {}", v, stat.label())
                    } else {
                        format!("{:+.0}% {}", v * 100.0, stat.label())
                    }
                }
                EvoEffect::AmmoMaxSet(v) => format!("ammo reserve set to {v:.0}"),
                EvoEffect::TimedBuff { trigger, duration, payload } => {
                    let what = match payload {
                        TimedPayload::PunchThrough(m) => format!("+{m:.1} m punch through"),
                        TimedPayload::TypeDamage(t, v) => {
                            format!("+{:.0}% {} damage", v * 100.0, t.name())
                        }
                    };
                    format!("{}: {what} for {duration:.0} s", trigger.label())
                }
                EvoEffect::FlatBaseDamageOnEmptyReload(v) => format!(
                    "+{v:.0} base damage after an empty reload — held for the whole run"
                ),
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

/// What turns a [`EvoEffect::TimedBuff`] on. Closed, so an unknown trigger is
/// a load error rather than a buff that silently never fires.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimedTrigger {
    OnKill,
    OnHeadshot,
    OnReload,
    OnEquip,
}

impl TimedTrigger {
    fn label(self) -> &'static str {
        match self {
            TimedTrigger::OnKill => "on kill",
            TimedTrigger::OnHeadshot => "on headshot",
            TimedTrigger::OnReload => "on reload",
            TimedTrigger::OnEquip => "on equip",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "on_kill" => TimedTrigger::OnKill,
            "on_headshot" => TimedTrigger::OnHeadshot,
            "on_reload" => TimedTrigger::OnReload,
            "on_equip" => TimedTrigger::OnEquip,
            _ => return None,
        })
    }
}

/// What a timed buff GRANTS. One arm per `modifiers:` key the data uses —
/// deliberately not a free-form map, so a misspelled modifier falls through to
/// `Inert` and the pinned test catches it instead of the buff silently
/// granting nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimedPayload {
    /// Punch-through depth in metres (Ripper Rounds).
    PunchThrough(f64),
    /// A fractional bonus to ONE damage type (Neurotoxin: +70% Toxin).
    TypeDamage(DamageType, f64),
}

/// `stat:` names an [`IndirectStat`]. Deliberately EXPLICIT rather than a
/// fuzzy match: an unknown name falls through to `Inert(...)` and the pinned
/// inert test then fails, which is how a typo announces itself instead of
/// silently contributing nothing.
fn indirect_stat(name: &str) -> Option<crate::loadout::IndirectStat> {
    use crate::loadout::IndirectStat as I;
    Some(match name {
        "recoil" => I::Recoil,
        "accuracy" => I::Accuracy,
        "punch_through" => I::PunchThrough,
        "projectile_speed" => I::ProjectileSpeed,
        "holstered_reload_per_second" => I::HolsteredReload,
        "movement_speed_aiming" => I::MovementSpeed,
        "ammo_max" => I::AmmoMax,
        "zoom" => I::Zoom,
        "range" => I::Range,
        "beam_range" => I::BeamRange,
        "noise" => I::Noise,
        _ => return None,
    })
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
        "flat_base_status_chance_by_form" => EvoEffect::FlatBaseStatusChanceByForm {
            base: f(v, "base").unwrap_or(0.0),
            incarnon: f(v, "incarnon").unwrap_or(0.0),
        },
        "flat_base_crit_multiplier" => {
            EvoEffect::FlatBaseCritMultiplier(f(v, "value").unwrap_or(0.0))
        }
        "flat_base_damage_on_empty_reload" => {
            EvoEffect::FlatBaseDamageOnEmptyReload(f(v, "value").unwrap_or(0.0))
        }
        // The handling family. `indirect` names its target in `stat:`; the
        // rest are named kinds that predate it and keep their spelling so the
        // yaml still reads like the card.
        "indirect" => match v.get("stat").and_then(Value::as_str).and_then(indirect_stat) {
            Some(st) => EvoEffect::Indirect(st, f(v, "value").unwrap_or(0.0)),
            None => EvoEffect::Inert(format!(
                "indirect ({})",
                v.get("stat").and_then(Value::as_str).unwrap_or("no stat")
            )),
        },
        "punch_through_bonus" => {
            EvoEffect::Indirect(crate::loadout::IndirectStat::PunchThrough, f(v, "value").unwrap_or(0.0))
        }
        "accuracy_bonus" => {
            EvoEffect::Indirect(crate::loadout::IndirectStat::Accuracy, f(v, "value").unwrap_or(0.0))
        }
        // NEGATIVE means less recoil, the same convention the MODS carry
        // (Primed Stabilizer ramps -0.15 -> -0.9). A positive value here would
        // read as more recoil, which no evolution grants.
        "recoil_reduction" => {
            EvoEffect::Indirect(crate::loadout::IndirectStat::Recoil, f(v, "value").unwrap_or(0.0))
        }
        "holstered_magazine_regen" => EvoEffect::Indirect(
            crate::loadout::IndirectStat::HolsteredReload,
            f(v, "value").unwrap_or(0.0),
        ),
        "ammo_reserve_set" => EvoEffect::AmmoMaxSet(f(v, "value").unwrap_or(0.0)),
        "timed_buff" => {
            let trig = v.get("trigger").and_then(Value::as_str).and_then(TimedTrigger::parse);
            let dur = f(v, "duration_seconds").unwrap_or(0.0);
            let mods = v.get("modifiers").and_then(Value::as_mapping);
            let payload = mods.and_then(|m| {
                m.iter().find_map(|(k, val)| {
                    let (k, x) = (k.as_str()?, val.as_f64()?);
                    Some(match k {
                        "punch_through_m" => TimedPayload::PunchThrough(x),
                        other => TimedPayload::TypeDamage(
                            DamageType::from_name(other.strip_suffix("_damage_bonus")?)?,
                            x,
                        ),
                    })
                })
            });
            match (trig, payload) {
                (Some(trigger), Some(payload)) => EvoEffect::TimedBuff { trigger, duration: dur, payload },
                _ => EvoEffect::Inert(format!(
                    "timed_buff ({})",
                    v.get("trigger").and_then(Value::as_str).unwrap_or("no trigger")
                )),
            }
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
                // Same bucket as the line above: it is base damage, and the
                // run is modelled holding it (see the variant's note).
                EvoEffect::FlatBaseDamageOnEmptyReload(v) => flat += v,
                // Into the SAME additive bucket a mod's indirect stat uses;
                // `resolve` seeds the panel from here.
                EvoEffect::Indirect(stat, v) => {
                    match base.indirect.iter_mut().find(|(s, _)| s == stat) {
                        Some(e) => e.1 += v,
                        None => base.indirect.push((*stat, *v)),
                    }
                }
                EvoEffect::AmmoMaxSet(v) => base.ammo_reserve = *v,
                // PARSED AND DESCRIBED, NOT APPLIED — and the reason differs
                // per payload, which is why they are matched apart rather than
                // waved through together:
                //
                // - PunchThrough: a CONDITIONAL indirect. Adding it to the
                //   flat bucket would make the panel claim an unconditional
                //   stat the build does not have, which is the rule the mods
                //   already follow (see data/mods/rifle/twitch.yaml: "it must
                //   not read as an unconditional stat change in the builder
                //   panel"). It is also multi-target only, so nothing is lost.
                //
                // - TypeDamage: a live per-type multiplier is a MECHANIC the
                //   sim does not have — the damage vector is baked at resolve.
                //   Adding one is a new mechanic, and AGENTS requires an
                //   in-game measurement for those. Its only user, Dual
                //   Toxocyst's Neurotoxin, is confirmed still broken in game
                //   (wiki, re-read 2026-08-03: "Currently does not work"), so
                //   it CANNOT be measured — an unmeasurable damage path is
                //   exactly the faithful-looking implementation the repo
                //   forbids. `apply` skips broken evolutions anyway.
                EvoEffect::TimedBuff { .. } => {}
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
                EvoEffect::FlatBaseStatusChanceByForm { base: b, incarnon } => {
                    // The Incarnon entry is the one carrying the `incarnon:`
                    // block — the same gate `FlatBaseMagazine` uses to keep a
                    // magazine evolution off the charge pool.
                    let v = if base.incarnon.is_some() { *incarnon } else { *b };
                    base.base_status_chance += v;
                    if let Some(r) = base.radial.as_mut() {
                        r.base_status_chance += v;
                    }
                    if let Some(f) = base.lingering.as_mut() {
                        f.base_status_chance += v;
                    }
                }
                EvoEffect::FlatBaseCritMultiplier(v) => {
                    base.base_crit_damage += v;
                    if let Some(r) = base.radial.as_mut() {
                        r.base_crit_damage += v;
                    }
                    if let Some(f) = base.lingering.as_mut() {
                        f.base_crit_damage += v;
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
                        // EARNED from zero, like every other TIMED buff: it
                        // has a duration, so a lull empties it and the fight
                        // has to fill it again (docs/BUFFS.md).
                        initial_stacks: 0,
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
                        // EARNED from zero, like every other timed buff.
                        initial_stacks: 0,
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
    /// EVERY evolution effect that loads INERT, pinned.
    ///
    /// An inert effect is a legitimate answer — "+50% Accuracy" decides
    /// nothing in an arena with no geometry — but it is indistinguishable at a
    /// glance from a MISSPELLED `kind:`, which also lands in `Inert(other)`
    /// and silently contributes nothing. That is the failure this exists for:
    /// the Boar's evolutions were written against a loader that had no
    /// crit-multiplier arm, and only reading the loader by hand caught it.
    ///
    /// So the set is written down. Adding an evolution whose effect does not
    /// load fails here until someone states which it is — a mechanic the arena
    /// cannot express, or a typo.
    #[test]
    fn the_inert_evolution_effects_are_the_ones_we_meant() {
        let mut found: Vec<String> = Vec::new();
        for def in pool() {
            for e in &def.effects {
                if let EvoEffect::Inert(what) = e {
                    found.push(format!("{} :: {what}", def.id));
                }
            }
        }
        found.sort();
        // Each line is a DECISION, and the reason is beside the effect in its
        // own yaml. Kept as a flat list so a diff here is readable.
        let expected: Vec<&str> = vec![
            // ---- the form is a WEAPON, not perk payload ----------------
            // Tier 1 unlocks the second weapon entry; there is nothing for
            // this loader to apply.
            "boar_prime_evo1_incarnon_form :: unlocks_weapon",
            "dual_toxocyst_evo1_incarnon_form :: unlocks_weapon",
            "laetum_evo1_incarnon_form :: unlocks_weapon",
            "torid_evo1_incarnon_form :: unlocks_weapon",
            // ---- RELOAD CADENCE ----------------------------------------
            // `reload_speed_bonus` is a MODS-loader word this loader has no arm
            // for, and both instances are conditional on an empty reload the
            // sim does not distinguish.
            "boar_prime_ready_retaliation :: reload_speed_bonus",
            "dual_toxocyst_ready_retaliation :: reload_speed_bonus",
            // ---- AMMO EFFICIENCY, and it is CONDITIONAL -----------------
            // Not an indirect stat: efficiency is real DPS the moment a
            // reserve runs dry. But one is gated on a movement state and one
            // on a headshot window, and applying either unconditionally would
            // overstate the build. They also land on the Laetum's Incarnon
            // magazine, which is charge-backed and takes no efficiency at all.
            "laetum_feather_of_justice :: indirect (ammo_efficiency_conditional)",
            "laetum_reapers_plenty :: indirect (ammo_efficiency_on_headshot)",
            // (`timed_buff` LOADS now — see
            // `the_parsed_but_unapplied_effects_are_the_ones_we_meant`.)
        ];
        let expected: Vec<String> = expected.into_iter().map(str::to_string).collect();
        let missing: Vec<&String> = expected.iter().filter(|e| !found.contains(e)).collect();
        let extra: Vec<&String> = found.iter().filter(|f| !expected.contains(f)).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "the inert set moved.
  NEW (implement it, or add it here with a reason): {extra:#?}
  GONE (drop it from the list): {missing:#?}"
        );
    }
    /// An evolution's HANDLING stats reach the resolved panel.
    ///
    /// They have no single-target damage payload, which is exactly why they
    /// used to be dropped — and dropping them meant the evolution equipped and
    /// its number vanished (user, 2026-08-03: 我们要纳计算). This asserts the
    /// whole path: yaml -> loader -> `WeaponBase.indirect` -> `resolve`'s
    /// bucket, in the same place a mod's would land.
    #[test]
    fn an_evolutions_handling_stats_reach_the_panel() {
        use crate::loadout::{resolve, IndirectStat, StackPolicy};
        let of = |weapon: &str, evo: &str| -> Vec<(IndirectStat, f64)> {
            let base = crate::loadout::WeaponBase::from_data(weapon, true, &[evo]);
            resolve(&base, &[], StackPolicy::Emergent).indirect
        };
        let find = |v: &[(IndirectStat, f64)], want: IndirectStat| {
            v.iter().find(|(s, _)| *s == want).map(|(_, x)| *x)
        };

        // Practiced Grip: "+50% Accuracy".
        let grip = of("boar_prime", "boar_prime_practiced_grip");
        assert_eq!(find(&grip, IndirectStat::Accuracy), Some(0.50), "{grip:?}");

        // Fortress Salvo: "+4 Punch Through" (metres), alongside its +16 base
        // damage — a mixed evolution must deliver BOTH halves.
        let salvo = of("boar_prime", "boar_prime_fortress_salvo");
        assert_eq!(find(&salvo, IndirectStat::PunchThrough), Some(4.0), "{salvo:?}");

        // Marksman's Hand: "-50% Recoil". NEGATIVE, like the mods'.
        let hand = of("dual_toxocyst", "dual_toxocyst_marksmans_hand");
        assert_eq!(find(&hand, IndirectStat::Recoil), Some(-0.50), "{hand:?}");

        // Swift Deliverance: "+50% Projectile Speed", which was `unmodeled`.
        let swift = of("torid", "torid_swift_deliverance");
        assert_eq!(find(&swift, IndirectStat::ProjectileSpeed), Some(0.50), "{swift:?}");

        // Mercenary Chamber SETS the reserve rather than adding to a bucket.
        let base = crate::loadout::WeaponBase::from_data(
            "boar_prime", true, &["boar_prime_mercenary_chamber"],
        );
        assert_eq!(base.ammo_reserve, 195.0);
    }
    /// Effects that LOAD but are deliberately not APPLIED.
    ///
    /// A companion to the inert list, and the reason it exists: once
    /// `timed_buff` got a real arm, its two users stopped being `Inert` and
    /// would have vanished from every list — parsed, described, contributing
    /// nothing, and no longer visible anywhere. "Loaded" and "applied" are
    /// different claims and each needs its own pin.
    #[test]
    fn the_parsed_but_unapplied_effects_are_the_ones_we_meant() {
        let mut found: Vec<String> = Vec::new();
        for def in pool() {
            for e in &def.effects {
                if let EvoEffect::TimedBuff { .. } = e {
                    found.push(def.id.clone());
                }
            }
        }
        found.sort();
        found.dedup();
        // Sorted, so this list reads alphabetically rather than by argument.
        let expected = vec![
            // A live PER-TYPE multiplier is a mechanic the sim does not have
            // (the damage vector is baked at resolve). New mechanics need an
            // in-game measurement, and this one cannot be measured: DE's own
            // wiki still says "Currently does not work" (re-read 2026-08-03),
            // which is also why the evolution carries `currently_broken` and
            // `apply` skips it wholesale.
            "dual_toxocyst_neurotoxin".to_string(),
            // CONDITIONAL indirect. Flattening it would make the panel claim
            // an unconditional +3 m punch through; the mods already refuse
            // that (data/mods/rifle/twitch.yaml). Multi-target only anyway.
            "dual_toxocyst_ripper_rounds".to_string(),
        ];
        assert_eq!(found, expected, "the parsed-but-unapplied set moved");
    }
}
