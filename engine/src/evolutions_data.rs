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
    /// Held is EXACT here, not an approximation, and the timing is why: the
    /// bonus lands the moment an empty reload BEGINS and does not wait for it
    /// to finish (measured in game — user, 2026-08-03; the wiki claims the
    /// opposite and loses, as it does to every measurement). So there is no
    /// gap: the magazine empties, the reload starts, the buff is already back,
    /// and it "lasts indefinitely until a manual reload is initiated while the
    /// magazine is not empty" — which the sim never does. Under the wiki's
    /// reading the buff would instead be DOWN for one reload every cycle, and
    /// holding it would overstate the build.
    ///
    /// **THIS IS THE EXCEPTION, AND THE NAME SAYS SO** (user, 2026-08-03).
    /// The DEFAULT for a reload-triggered effect is that it fires when the
    /// reload COMPLETES; a new one gets its own variant and that default,
    /// rather than reusing this. Two conditions have to hold together here
    /// and neither is the ordinary case:
    ///
    ///   1. the magazine must be EMPTY (a manual reload does not count — it
    ///      is what takes the bonus away);
    ///   2. it fires when the reload STARTS, not when it ends.
    ///
    /// Only Boar Prime's Reified Bane is known to work this way. Whether any
    /// other evolution ever joins it is open, so the variant stays narrow: a
    /// general "on reload" effect is not this one with a flag.
    ///
    /// It stays its own variant rather than being folded into `FlatBaseDamage`
    /// because it is a BUFF: `resolve` turns it into an `EvoBdBuff` so the bar
    /// can show it and a card can scale it back out — opening at ONE stack,
    /// which is the state a default test starts in.
    FlatBaseDamageOnEmptyReload(f64),
    /// A handling / mobility / multi-target stat with no single-target damage
    /// payload — recoil, accuracy, punch through, projectile speed, holstered
    /// reload. It COUNTS: the value lands in the panel's `indirect` bucket
    /// beside the mods' (user, 2026-08-03: "什么后坐力，精准度，我们要纳计算，
    /// 只是目前完全不影响 dps 而已"). Mods were given this treatment on
    /// 2026-08-01; evolutions were still dropping the number on the floor.
    Indirect(crate::loadout::IndirectStat, f64),
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
    /// Prelude of Might: "With Critical Chance below 40%: Increase Critical
    /// Damage Multiplier by +3x". The condition is on the build's OWN RESOLVED
    /// crit chance, so unlike every other `condition:` in this engine it asks
    /// about the panel the mods just produced rather than about the Tenno or
    /// the target — which is why it is a variant and not a gate.
    CritMultiplierBelowCritChance { value: f64, below: f64 },
    /// Headcracker: "On Headshot: +5% Fire Rate for 2s. Stacks up to 10x",
    /// and — from the raw wikitext, which the rendered page's summary drops —
    /// "This effect has a 50% chance of activating."
    StackingFireRateOnHeadshot { per_stack: f64, max_stacks: u32, duration: f64, chance: f64 },
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
    /// THE TRANSFORMATION ITSELF — tier 1 of every Incarnon ladder, naming the
    /// form it unlocks. It changes no stat (the form's own entry carries those)
    /// and it is not a CHOICE: every one of these is `selection: fixed`,
    /// because installing the Genesis is what grants it.
    ///
    /// It was parsed as `Inert("unlocks_weapon")` and the target dropped on the
    /// floor, which left "which evolution unlocks the form" to be guessed from
    /// LADDER POSITION ("tier 1's first option"). Reading it is what lets the
    /// form and the evolution stop being two controls for one fact — asking to
    /// fire the Incarnon form implies the evolution that IS firing it (user,
    /// 2026-08-04).
    UnlocksForm(String),
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
                // A BUFF, not a silent stat: the run holds it from t = 0, but
                // it is earned by an empty reload and the bar has to say so
                // (user, 2026-08-03). Permanent — nothing decays it — and one
                // stack, which is what "on/off" is in this vocabulary.
                EvoEffect::FlatBaseDamageOnEmptyReload(_) => Some(EvoBuffCard {
                    id: "evo_reload_damage",
                    max_stacks: 1,
                    permanent: true,
                }),
                EvoEffect::StackingReloadSpeedOnHeadshot { max_stacks, .. } => Some(EvoBuffCard {
                    id: "on_headshot_reload_speed",
                    max_stacks: *max_stacks,
                    permanent: false,
                }),
                EvoEffect::StackingFireRateOnHeadshot { max_stacks, .. } => Some(EvoBuffCard {
                    id: "on_headshot_fire_rate",
                    max_stacks: *max_stacks,
                    permanent: false,
                }),
                // Static stat changes — nothing to configure at runtime.
                EvoEffect::FlatBaseStatusChanceByForm { .. }
                | EvoEffect::FlatBaseCritMultiplier(_)

                | EvoEffect::Indirect(..)
                | EvoEffect::AmmoMaxSet(_)
                | EvoEffect::FlatBaseDamage(_)
                | EvoEffect::FlatBaseCritChance(_)
                | EvoEffect::FlatBaseStatusChance(_)
                | EvoEffect::FlatBaseMagazine(_)
                | EvoEffect::FieldDurationOnEmptyReload(_)
                | EvoEffect::MultishotOnLastRound(_)
                | EvoEffect::MultishotConsumesAmmo(_)
                | EvoEffect::ConditionOverload { .. }
                | EvoEffect::FireRateBonus(_)
                | EvoEffect::CritMultiplierBelowCritChance { .. }
                | EvoEffect::PostModCritChance(_)
                | EvoEffect::PostModStatusChance(_)
                | EvoEffect::HeadshotDamage(_)
                | EvoEffect::IncarnonChargeRate(_) => None,
                // Rolled per instance, not a buff with an uptime.
                EvoEffect::ChanceDamageOnNoncrit { .. } => None,
                // The transformation grants no CARD: what it unlocks is a
                // FORM, whose own weapon entry carries every stat it brings.
                EvoEffect::UnlocksForm(_) | EvoEffect::Inert(_) => None,
            })
            .collect()
    }
}

impl EvolutionDef {

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

    /// WHAT THIS PERK DOES NOT DO YET — the effects that loaded as `Inert`,
    /// named.
    ///
    /// DERIVED, never declared. An `unmodeled: true` field beside the effects
    /// would be a second copy of the truth, free to disagree with the loader
    /// the moment somebody implements one and forgets the flag. This asks the
    /// loaded effects, so a perk stops confessing the instant it is modelled
    /// and starts the instant a new unknown kind is written.
    ///
    /// Empty means every effect is modelled. It is the honest thing for the
    /// UI to show and the honest thing to grep for (user, 2026-08-06: 如果有
    /// 的东西没做完，得说这个东西未完成 …… 不要隐瞒欺骗自己).
    pub fn unmodeled_effects(&self) -> Vec<&str> {
        self.effects
            .iter()
            .filter_map(|e| match e {
                EvoEffect::Inert(name) => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Does this perk do NOTHING the sim can see? A perk whose every effect is
    /// inert is not a weaker choice, it is not a choice — and the tile you pick
    /// from should say so rather than look like its working tier-mates.
    pub fn fully_unmodeled(&self) -> bool {
        !self.effects.is_empty() && self.unmodeled_effects().len() == self.effects.len()
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
                EvoEffect::FlatBaseDamageOnEmptyReload(v) => format!(
                    "+{v:.0} base damage from the moment an empty reload starts — held all run"
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
                EvoEffect::StackingFireRateOnHeadshot { per_stack, max_stacks, duration, chance } => format!(
                    "+{:.0}% fire rate per stack x{max_stacks} for {duration:.0}s on headshot, \
                     {:.0}% chance each (additive with fire-rate mods)",
                    per_stack * 100.0,
                    chance * 100.0
                ),
                EvoEffect::CritMultiplierBelowCritChance { value, below } => format!(
                    "+{value:.1}x BASE crit multiplier while crit chance stays under {:.0}%                      (crit damage mods multiply it; the check reads the BUILD's crit chance,                      not a live Puncture buff)",
                    below * 100.0
                ),
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
                EvoEffect::UnlocksForm(w) => {
                    format!("unlocks the {w} form — its stats are that form's own")
                }
                EvoEffect::Inert(what) => {
                    format!("{} (no single-target DPS effect)", what.replace('_', " "))
                }
            })
            .collect()
    }
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
                // NAME the payload. "unmodeled payload" told the pinned inert
                // list nothing: two different unmodelled buffs read as the
                // same entry, and neither said what it granted.
                None => EvoEffect::Inert(format!(
                    "stacking_buff {}",
                    v.get("per_stack")
                        .and_then(Value::as_mapping)
                        .and_then(|m| m.keys().next().and_then(|k| k.as_str()).map(str::to_string))
                        .unwrap_or_else(|| "no payload".into())
                )),
            }
        }
        "condition_overload" => EvoEffect::ConditionOverload {
            per_type: f(v, "value").unwrap_or(0.0),
        },
        "fire_rate_bonus" => EvoEffect::FireRateBonus(f(v, "value").unwrap_or(0.0)),
        "on_headshot_fire_rate" => EvoEffect::StackingFireRateOnHeadshot {
            per_stack: f(v, "per_stack").unwrap_or(0.0),
            max_stacks: v.get("max_stacks").and_then(Value::as_u64).unwrap_or(1) as u32,
            duration: f(v, "duration").unwrap_or(0.0),
            // Default 1.0 so a perk that does NOT roll reads as certain rather
            // than as never firing.
            chance: f(v, "chance").unwrap_or(1.0),
        },
        "crit_multiplier_below_crit_chance" => EvoEffect::CritMultiplierBelowCritChance {
            value: f(v, "value").unwrap_or(0.0),
            below: f(v, "below_crit_chance").unwrap_or(0.0),
        },
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
        "unlocks_weapon" => EvoEffect::UnlocksForm(
            v.get("weapon")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
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
                // NOTHING TO APPLY. The form it unlocks is a separate weapon
                // entry with its own stats, so applying anything here would
                // count them twice.
                EvoEffect::UnlocksForm(_) => {}
                EvoEffect::FlatBaseDamage(v) => flat += v,
                // Same bucket as the line above: it is base damage, and the
                // run is modelled holding it (see the variant's note).
                // Into the base like any other flat damage — the buff OPENS
                // FULL — and recorded so the buff card can take it back off.
                EvoEffect::FlatBaseDamageOnEmptyReload(v) => {
                    flat += v;
                    base.reload_damage_buff += v;
                }
                // Into the SAME additive bucket a mod's indirect stat uses;
                // `resolve` seeds the panel from here.
                EvoEffect::Indirect(stat, v) => {
                    match base.indirect.iter_mut().find(|(s, _)| s == stat) {
                        Some(e) => e.1 += v,
                        None => base.indirect.push((*stat, *v)),
                    }
                }
                EvoEffect::AmmoMaxSet(v) => base.ammo_reserve = *v,
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
                // Carried, not applied: `apply` works on the RAW base panel and
                // the condition needs the crit chance the mods produce, which
                // does not exist until `resolve` runs.
                EvoEffect::CritMultiplierBelowCritChance { value, below } => {
                    base.crit_mult_below_cc = Some((*value, *below));
                }
                EvoEffect::StackingFireRateOnHeadshot { per_stack, max_stacks, duration, chance } => {
                    base.stacking_buffs.push(crate::loadout::StackingBuff {
                        id: "on_headshot_fire_rate",
                        trigger: crate::loadout::BuffTrigger::Headshot,
                        grant: crate::loadout::BuffGrant::FireRate,
                        // A FRACTION here; `resolve` turns it into an absolute
                        // rate against the base, which is the bucket it joins.
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        chance: *chance,
                        // EARNED from zero, like every other timed buff.
                        initial_stacks: 0,
                    });
                }
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
                    base.stacking_buffs.push(crate::loadout::StackingBuff {
                        id: "on_plain_hit_damage",
                        trigger: crate::loadout::BuffTrigger::PlainHit,
                        grant: crate::loadout::BuffGrant::BaseDamage,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        chance: 1.0,
                        // EARNED from zero, like every other TIMED buff: it
                        // has a duration, so a lull empties it and the fight
                        // has to fill it again (docs/BUFFS.md).
                        initial_stacks: 0,
                    });
                }
                EvoEffect::StackingReloadSpeedOnHeadshot {
                    per_stack,
                    max_stacks,
                    duration,
                } => {
                    base.stacking_buffs.push(crate::loadout::StackingBuff {
                        id: "on_headshot_reload_speed",
                        trigger: crate::loadout::BuffTrigger::Headshot,
                        grant: crate::loadout::BuffGrant::ReloadSpeed,
                        per_stack: *per_stack,
                        max_stacks: *max_stacks,
                        duration: *duration,
                        chance: 1.0,
                        // EARNED from zero, like every other timed buff.
                        initial_stacks: 0,
                    });
                }
                EvoEffect::Inert(_) => {}
            }
        }
    }
    if flat > 0.0 && original_total > 0.0 {
        let evolved = original_total + flat;
        base.base_vector = base.base_vector.scale(evolved / original_total);
        // ...AND THE EXPLOSION, which is not what this did until 2026-08-05.
        //
        // It was the inconsistency two lines of this function already
        // contradicted: `FlatBaseCritChance` and `FlatBaseStatusChance` reach
        // every attack part because "a base-stat evolution is a WEAPON stat
        // change", and flat base DAMAGE is the same kind of statement. It was
        // marked "INFERENCE, not a citation" because nothing said so outright.
        //
        // The CO catalog now does, for the Burston: its Incarnon radial reads
        // "Attack Damage 55 | CO Damage Bonus at +100% 13 | 24%". The radial's
        // own base is 13 Heat and the ONLY +42 in that Genesis is Evolution
        // II's, so 55 is 13 + 42 — the explosion takes the evolution's flat
        // damage — while 13 is what CO still multiplies, and 13/55 is the 24%
        // the third column prints. One row, and it settles both halves.
        //
        // Isolated when written: the only entries with a radial are the Laetum
        // Incarnon and the Larkspur Prime's charged shot, and neither weapon
        // has a flat-damage evolution at all. So nothing already measured
        // moves.
        if let Some(r) = base.radial.as_mut() {
            let rad_original = r.base_vector.total();
            if rad_original > 0.0 {
                let rad_evolved = rad_original + flat;
                r.base_vector = r.base_vector.scale(rad_evolved / rad_original);
                // The explosion's CO keeps multiplying its UNEVOLVED base. The
                // direct hit's exclusion below is opt-in per perk; this one is
                // not, because the catalog's single radial row is a statement
                // about the radial, and no radial row anywhere says otherwise.
                r.co_base_fraction = rad_original / rad_evolved;
            }
        }
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
impl EvolutionDef {
    /// The form this evolution unlocks, if it is the transformation itself.
    ///
    /// THE TAG the form resolution reads. It replaces "tier 1's first option",
    /// which was a guess from ladder position that happened to hold for the
    /// four Incarnon weapons in the roster and says nothing about the fifth.
    pub fn unlocks_form(&self) -> Option<&str> {
        self.effects.iter().find_map(|e| match e {
            EvoEffect::UnlocksForm(w) => Some(w.as_str()),
            _ => None,
        })
    }
}

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

    /// A broken evolution changes NOTHING — whatever it grants.
    ///
    /// The test above can only be as strong as the data it picks, and no
    /// SHIPPED broken evolution carries an effect `apply` would act on: both
    /// of them resolve to something `apply` ignores anyway, so a regression in
    /// the `currently_broken` filter would not have shown up there. This
    /// builds a synthetic one carrying ONE OF EVERY effect `apply` writes
    /// through, so the guard is on the filter itself rather than on today's
    /// data — including the two write paths added on 2026-08-03
    /// (`Indirect` and `AmmoMaxSet`), which reach fields the old test never
    /// looked at (user: "不要让 broken 的起作用").
    #[test]
    fn a_broken_evolution_changes_nothing_whatever_it_grants() {
        use crate::loadout::{IndirectStat, WeaponBase};
        let everything = |broken: bool| EvolutionDef {
            id: "synthetic".into(),
            name: "Synthetic".into(),
            weapon: "torid".into(),
            tier: 9,
            icon: None,
            description: String::new(),
            currently_broken: broken,
            co_base_excludes_this_evolution: false,
            effects: vec![
                EvoEffect::FlatBaseDamage(100.0),
                EvoEffect::FlatBaseDamageOnEmptyReload(50.0),
                EvoEffect::FlatBaseCritChance(0.5),
                EvoEffect::FlatBaseCritMultiplier(1.5),
                EvoEffect::FlatBaseStatusChance(0.5),
                EvoEffect::FlatBaseStatusChanceByForm { base: 0.4, incarnon: 0.9 },
                EvoEffect::FlatBaseMagazine(30.0),
                EvoEffect::Indirect(IndirectStat::Accuracy, 0.5),
                EvoEffect::AmmoMaxSet(999.0),
            ],
        };
        let base = WeaponBase::from_data("torid", false, &[]);

        let mut broken = base.clone();
        apply(&mut broken, &[&everything(true)]);
        assert!(
            (broken.base_vector.total() - base.base_vector.total()).abs() < 1e-9,
            "a broken evolution moved base damage"
        );
        assert_eq!(broken.base_crit_chance, base.base_crit_chance);
        assert_eq!(broken.base_crit_damage, base.base_crit_damage);
        assert_eq!(broken.base_status_chance, base.base_status_chance);
        assert_eq!(broken.magazine_size, base.magazine_size);
        assert_eq!(broken.ammo_reserve, base.ammo_reserve, "broken set the reserve");
        assert!(broken.indirect.is_empty(), "broken wrote an indirect stat: {:?}", broken.indirect);

        // ...and the SAME evolution unbroken must move every one of them, or
        // this test would pass on an `apply` that does nothing at all.
        let mut live = base.clone();
        apply(&mut live, &[&everything(false)]);
        assert!(live.base_vector.total() > base.base_vector.total());
        assert!(live.base_crit_chance > base.base_crit_chance);
        assert!(live.base_crit_damage > base.base_crit_damage);
        assert!(live.base_status_chance > base.base_status_chance);
        assert!(live.magazine_size > base.magazine_size);
        assert_eq!(live.ammo_reserve, 999.0);
        assert_eq!(live.indirect, vec![(IndirectStat::Accuracy, 0.5)]);
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
            // (The four `unlocks_weapon` tier-1 entries used to live here.
            // They still apply nothing — the form is a separate weapon with
            // its own stats — but they are no longer INERT: `UnlocksForm`
            // carries the form's id, and reading it is what lets a form
            // request imply the evolution that IS that form instead of
            // silently falling back to base (2026-08-04). Inert meant the
            // target was dropped at parse time and "which evolution unlocks
            // the form" had to be guessed from ladder position.)
            // ---- RELOAD CADENCE ----------------------------------------
            // `reload_speed_bonus` is a MODS-loader word this loader has no arm
            // for, and both instances are conditional on an empty reload the
            // sim does not distinguish.
            "boar_ready_retaliation :: reload_speed_bonus",
            // The Burston's copy has been fixed by DE twice on this weapon
            // (37.0.9, 38.5.3), so it wants a measurement and not a reading.
            "burston_ready_retaliation :: reload_speed_bonus",
            "burston_prime_ready_retaliation :: reload_speed_bonus",
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
            // ---- ONE-STACK STACKING BUFFS -------------------------------
            // A "timed buff" is a stacking buff with ONE stack — same trigger,
            // same window — so it uses that vocabulary and lands here when its
            // PAYLOAD is one the engine does not model. The label names the
            // payload, so the two are told apart.
            //
            // Ripper Rounds: punch through, multi-target only. Neurotoxin:
            // "+70% Toxin for 3 s on headshot" — REAL DPS on a weapon played
            // at 100% headshots, and the one genuine gap in this list. It is
            // also `currently_broken` in game (DE's wiki, re-read 2026-08-03:
            // "Currently does not work"), and `apply` skips broken evolutions
            // wholesale, so the two cancel out today. Whoever models a
            // per-type buff payload should check DE fixed the perk first —
            // a mechanic that cannot be measured cannot be verified.
            // REAVER'S RAPTURE — the largest gap in this list, and the only
            // one whose trigger the sim can already see. +20% base damage per
            // COMPLETED BURST to a cap of 5, reset by a reload rather than by
            // a timeout, so a stacking buff with a duration is the wrong
            // shape for it. Holding it at max would overstate a full magazine
            // by 13 points of the base-damage bucket (15 bursts, the first
            // four spent climbing), which is why it is inert instead of
            // approximated. `BurstSpec::count` is carried for whoever fixes
            // this; the weapon yaml has the rest of the rules.
            "burston_reavers_rapture :: stacking_buff base_damage_bonus",
            "burston_prime_reavers_rapture :: stacking_buff base_damage_bonus",
            "dual_toxocyst_neurotoxin :: stacking_buff toxin_damage_bonus",
            "dual_toxocyst_ripper_rounds :: stacking_buff punch_through_m",
            // ---- THE FURIS GENESIS ---------------------------------------
            // Five of its eight perks, and every one is written under a kind
            // this loader does NOT know — deliberately, because the kinds that
            // would have fit all pay out unconditionally:
            //
            //   `flat_base_damage` ignores `condition:`, so Haven Foray's
            //   overshield clause would have loaded a silent +30 on every
            //   build. `flat_base_crit_multiplier` ignores it too, so Prelude
            //   of Might would have granted +3x to everyone. And a
            //   `stacking_buff` carrying a multishot payload becomes
            //   AssumedMaxMultishot whatever trigger sits beside it, so
            //   Stormburst would have handed +1.2 multishot to builds with no
            //   Electricity in them.
            //
            // An unknown kind is the only spelling that means "nothing models
            // this yet" and stays true.
            //
            // TWO LEFT THIS LIST on 2026-08-06. Prelude of Might needed a
            // condition read off the RESOLVED panel, which nothing here did, so
            // it got `CritMultiplierBelowCritChance` and a late hook in
            // `resolve`. Headcracker needed a live stacking buff in the
            // additive fire-rate bucket; `resolve` converts its +5% into the
            // absolute rate that fraction is worth, so the sim never needed an
            // unmodded rate of its own. What the remaining three need:
            // EXECUTIONER'S FORTUNE needs a reload the sim can END rather than
            // scale; STORMBURST needs a stacking buff that can state a TARGET
            // condition, which `AssumedMaxMultishot` cannot; HAVEN FORAY needs
            // a Tenno with overshields, which `TennoCondition` has no room for.
            //
            // Every one of them now says so on its own tile — `unmodeled_effects`
            // is derived from these same variants, so this list and the UI
            // cannot disagree.
            "furis_executioners_fortune :: instant_reload_on_headshot",
            "furis_haven_foray :: flat_base_damage_with_overshields",
            "furis_stormburst :: stacking_multishot_on_electricity_status",
            "mk1_furis_executioners_fortune :: instant_reload_on_headshot",
            "mk1_furis_haven_foray :: flat_base_damage_with_overshields",
            "mk1_furis_stormburst :: stacking_multishot_on_electricity_status",
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
}

/// The two Furis tier-4 perks, both added 2026-08-06, and both from the RAW
/// wikitext rather than the rendered page — which is the point of the pair.
/// Reading the effect column alone gave Headcracker no 50% roll and Prelude of
/// Might no "Base", and each omission makes the perk stronger than the game's.
#[cfg(test)]
mod furis_tier4_tests {
    use super::*;
    use crate::loadout::{resolve, StackPolicy, WeaponBase};

    fn cd_with(mods: &[&str], evos: &[&str]) -> f64 {
        let owned: Vec<String> = evos.iter().map(|s| (*s).to_string()).collect();
        let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
        let base = WeaponBase::from_data("furis_incarnon", true, &refs);
        let pool = crate::mods_data::pool_for_weapon("furis_incarnon");
        let picked: Vec<&crate::loadout::ModDef> = mods
            .iter()
            .map(|id| pool.iter().find(|m| m.id == *id).unwrap_or_else(|| panic!("{id}")))
            .collect();
        resolve(&base, &picked, StackPolicy::AssumedMax).crit_damage
    }

    /// "Increase BASE Critical Damage Multiplier by +3x" — so crit-damage mods
    /// multiply the raised base. Added AFTER the mods instead, a Primed Target
    /// Cracker build reads 10.14x where the game gives 13.44x, and the two only
    /// diverge once a crit-damage mod is on — which is why the word "Base",
    /// present in the wikitext and absent from the summary, decides it.
    #[test]
    fn prelude_of_might_raises_the_base_multiplier_not_the_final_one() {
        let evo = ["furis_evo1_incarnon_form", "furis_prelude_of_might"];
        let bare = ["furis_evo1_incarnon_form"];
        // 3.4 base, +3 = 6.4 with no crit-damage mod either way.
        assert!((cd_with(&[], &evo) - 6.4).abs() < 1e-9, "{}", cd_with(&[], &evo));
        // With +110%: (3.4 + 3.0) x 2.1 = 13.44, NOT 3.4 x 2.1 + 3.0 = 10.14.
        let modded = cd_with(&["primed_target_cracker"], &evo);
        assert!((modded - 13.44).abs() < 1e-6, "expected 13.44x, got {modded}");
        assert!((cd_with(&["primed_target_cracker"], &bare) - 7.14).abs() < 1e-6);
    }

    /// ...and it is CONDITIONAL: the perk pays only while the build's own crit
    /// chance stays under 40%, so taking it means not building crit chance.
    #[test]
    fn prelude_of_might_switches_off_above_the_threshold() {
        let evo = ["furis_evo1_incarnon_form", "furis_prelude_of_might"];
        // The form's own 26% is under the line; Primed Pistol Gambit clears it.
        assert!((cd_with(&[], &evo) - 6.4).abs() < 1e-9);
        let over = cd_with(&["primed_pistol_gambit"], &evo);
        assert!((over - 3.4).abs() < 1e-9, "over 40% crit it must pay nothing, got {over}");
    }

    /// Headcracker is a LIVE buff, so it is asserted on the loaded spec rather
    /// than on a panel: the 50% roll is the half that a summary drops.
    #[test]
    fn headcracker_carries_its_fifty_percent_roll() {
        let e = get("furis_headcracker").expect("furis_headcracker");
        let hit = e.effects.iter().find_map(|x| match x {
            EvoEffect::StackingFireRateOnHeadshot { per_stack, max_stacks, duration, chance } => {
                Some((*per_stack, *max_stacks, *duration, *chance))
            }
            _ => None,
        });
        assert_eq!(
            hit,
            Some((0.05, 10, 2.0, 0.50)),
            "raw wikitext: +5% for 2s, x10, \"This effect has a 50% chance of activating\""
        );
    }
}

/// THE FURIS FAMILY SPLITS ON CONDITION OVERLOAD, and the split is the point.
///
/// One Incarnon Genesis upgrades either weapon, so the tempting move is to give
/// them the same CO treatment. The catalog says otherwise by saying nothing:
/// its row names "Furis" and carries that weapon's numbers, there is no
/// MK1-Furis row, and absence from that table is a positive statement that the
/// attack behaves normally (owner confirmed 2026-08-06 — the MK1 does not have
/// the restriction). Lato Vandal has a row and Lato Prime does not, same family
/// and same Genesis, which is what a per-entry slip in DE's code looks like.
///
/// Pinned in BOTH directions so a later tidy-up cannot quietly align them.
#[cfg(test)]
mod furis_co_split_tests {
    use super::*;

    fn excludes(id: &str) -> bool {
        get(id).unwrap_or_else(|| panic!("{id}")).co_base_excludes_this_evolution
    }

    #[test]
    fn the_furis_tier2_pair_excludes_its_own_base_from_condition_overload() {
        // "100 or 128 (with Evolution II) | 100 | 100% or 78%" — the CO term
        // keeps multiplying the unevolved 100. On the TIER, because the row
        // names "Evolution II" with no perk number and both options grant +28.
        assert!(excludes("furis_haven_foray"));
        assert!(excludes("furis_stormburst"));
    }

    #[test]
    fn the_mk1_tier2_pair_does_not() {
        assert!(!excludes("mk1_furis_haven_foray"));
        assert!(!excludes("mk1_furis_stormburst"));
    }
}

