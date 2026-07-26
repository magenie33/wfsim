//! Pipeline layer [1]: mod resolution — a chosen mod set becomes panel stats.
//!
//! Buckets (docs/MECHANICS.md, docs/GLOSSARY.md): every relative bonus of one
//! kind sums additively into its bucket, then buckets combine by their real
//! rules (crit chance = base × (1 + Σcc); elemental amount = ModifiedBase ×
//! bonus; reload time = base / (1 + Σreload); …). Elemental entries enter the
//! layer-[2] hierarchy in **mod order** ([`crate::elements`]).
//!
//! Conditional/stacking effects resolve under a [`StackPolicy`] — today only
//! `AssumedMax` (docs/OPTIMIZER.md §3: every stacking buff at full stacks).

use crate::damage::{DamageType, DamageVector};
use crate::elements::{self, ElementalInput};
use crate::mods::Polarity;

/// One resolved effect of a mod at its equipped rank.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModEffect {
    /// Additive base-damage bucket (Hornet Strike).
    BaseDamage(f64),
    /// Additive multishot bucket (total pellets = base × (1 + Σ)).
    Multishot(f64),
    /// Relative crit chance (base_cc × (1 + Σ)).
    CritChance(f64),
    /// Relative crit damage (base_cd × (1 + Σ)).
    CritDamage(f64),
    /// Relative status chance.
    StatusChance(f64),
    /// Relative fire rate (negative for Creeping Bullseye's downside).
    FireRate(f64),
    /// Reload speed bonus (time = base / (1 + Σ)).
    ReloadSpeed(f64),
    /// Status-damage bucket (Pistol Elementalist) — scales status payloads.
    StatusDamage(f64),
    /// Primary element: ModifiedBase × bonus enters the hierarchy at this
    /// mod's position.
    Element(DamageType, f64),
    /// Combined-element mod (Magnetic Might): added outside the hierarchy.
    CombinedElement(DamageType, f64),
    /// Galvanized Diffusion's on-kill multishot stacks.
    OnKillMultishot {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// Condition Overload payload (Galvanized Shot): +per_stack per
    /// status TYPE on the target, per on-kill stack, direct hits only.
    ConditionOverload {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// Galvanized Crosshairs' single refreshable buff: on HEADSHOT,
    /// +bonus relative crit chance (while aiming) for `duration`.
    OnHeadshotCritChance { bonus: f64, duration: f64 },
    /// Galvanized Crosshairs' stacks: on HEADSHOT KILL, +per_stack
    /// relative crit chance; each stack has its OWN duration (per-stack
    /// expiry FIFO — unlike the other Galvanized mods' decay).
    OnHeadshotKillCritChance {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// INDIRECT stat bonus (recoil, accuracy, ammo, projectile speed…):
    /// an additive bucket per stat, resolved into
    /// [`ResolvedPanel::indirect`]. Indirect = excluded from the
    /// theoretical-DPS formula, but a real input to practical combat —
    /// a future shooter model can consume them (recoil/accuracy → hit &
    /// headshot probability; projectile speed → travel time vs moving
    /// targets; ammo/holstered reload → long-fight sustain). Noise and
    /// dodge/roll speed are stealth/survivability, never DPS.
    Indirect(IndirectStat, f64),
    /// Reflex Draw: temporary handling buff on weapon swap-in. Conditional
    /// and handling-only — never a static panel stat.
    OnEquipHandling { recoil: f64, accuracy: f64, duration: f64 },
}

/// Indirect stat targets (each its own additive bucket) — outside the
/// theoretical-DPS formula, inside practical combat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndirectStat {
    Recoil,
    Noise,
    AmmoMax,
    ProjectileSpeed,
    HolsteredReload,
    DodgeSpeed,
    AcrobaticSpeed,
    Accuracy,
}

impl IndirectStat {
    pub fn label(&self) -> &'static str {
        match self {
            IndirectStat::Recoil => "Recoil",
            IndirectStat::Noise => "Noise",
            IndirectStat::AmmoMax => "Ammo Reserve",
            IndirectStat::ProjectileSpeed => "Projectile Speed",
            IndirectStat::HolsteredReload => "Holstered Reload/s",
            IndirectStat::DodgeSpeed => "Dodge Speed",
            IndirectStat::AcrobaticSpeed => "Acrobatic Speed",
            IndirectStat::Accuracy => "Accuracy",
        }
    }
}

/// "+60%" / "−15%" / "+109.5%" from a fraction (true minus sign).
pub fn pct(x: f64) -> String {
    let p = x.abs() * 100.0;
    let s = if (p - p.round()).abs() < 1e-6 {
        format!("{}", p.round() as i64)
    } else {
        format!("{p:.2}").trim_end_matches('0').trim_end_matches('.').to_string()
    };
    if x >= 0.0 { format!("+{s}%") } else { format!("−{s}%") }
}

impl ModEffect {
    /// One display line for this effect — OUR statement of what the model
    /// actually computes (true values; tooltip lies already corrected in
    /// the data). The single source for every effect list in the UI.
    pub fn describe(&self) -> String {
        use ModEffect::*;
        match *self {
            BaseDamage(v) => format!("{} Base Damage", pct(v)),
            Multishot(v) => format!("{} Multishot", pct(v)),
            CritChance(v) => format!("{} Crit Chance", pct(v)),
            CritDamage(v) => format!("{} Crit Damage", pct(v)),
            StatusChance(v) => format!("{} Status Chance", pct(v)),
            FireRate(v) => format!("{} Fire Rate", pct(v)),
            ReloadSpeed(v) => format!("{} Reload Speed", pct(v)),
            StatusDamage(v) => format!("{} Status Damage", pct(v)),
            Element(t, v) => format!("{} {t:?}", pct(v)),
            CombinedElement(t, v) => format!("{} {t:?}", pct(v)),
            OnKillMultishot { per_stack, max_stacks, duration } => {
                format!("On Kill: {} Multishot per stack ×{max_stacks}, {duration}s", pct(per_stack))
            }
            ConditionOverload { per_stack, max_stacks, duration } => {
                format!(
                    "On Kill: {} Damage per status type ×{max_stacks}, {duration}s (direct hits)",
                    pct(per_stack)
                )
            }
            OnHeadshotCritChance { bonus, duration } => {
                format!("On Headshot: {} Crit Chance, {duration}s", pct(bonus))
            }
            OnHeadshotKillCritChance { per_stack, max_stacks, duration } => {
                format!("On Headshot Kill: {} Crit Chance per stack ×{max_stacks}, {duration}s", pct(per_stack))
            }
            Indirect(stat, v) => format!("{} {}", pct(v), stat.label()),
            OnEquipHandling { recoil, accuracy, duration } => {
                format!("On Equip: {} Recoil, {} Accuracy, {duration}s", pct(recoil), pct(accuracy))
            }
        }
    }
}

/// A mod as the resolver sees it (stats at the equipped rank).
#[derive(Debug, Clone)]
pub struct ModDef {
    pub id: &'static str,
    /// Drain at the EQUIPPED (max) rank.
    pub base_drain: u32,
    /// Max rank (drain rises 1/rank from rank 0, so rank-0 drain = base_drain − max_rank).
    pub max_rank: u32,
    pub polarity: Polarity,
    /// Card rarity (frame colour). Display-only — no mechanical effect.
    pub rarity: Rarity,
    /// Exilus (utility) mod: may occupy the exilus slot in addition to
    /// regular slots. Exilus mods are handling/QoL effects with no damage
    /// model, so the optimizer skips them.
    pub exilus: bool,
    /// Mods sharing a family are mutually exclusive (wiki Incompatible).
    pub family: Option<&'static str>,
    pub effects: Vec<ModEffect>,
}

/// Mod card rarity — determines the in-game frame colour (bronze / silver /
/// gold / white). Purely cosmetic; carried for UI display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

impl ModDef {
    /// The primary element this mod adds, if any (position-sensitive).
    pub fn primary_element(&self) -> Option<DamageType> {
        self.effects.iter().find_map(|e| match e {
            ModEffect::Element(t, _) => Some(*t),
            _ => None,
        })
    }
}

/// How stacking/conditional effects are valued (docs/OPTIMIZER.md §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackPolicy {
    /// Full stacks / 100% uptime on every conditional buff.
    AssumedMax,
    /// On-kill stacking buffs start at their configured INITIAL stacks
    /// (full, per user 2026-07-24 correction) and then evolve purely by
    /// mechanics: kills refresh/grant, timeouts decay one stack.
    Emergent,
    /// Sentinel / robotic weapons: Galvanized (and other conditional
    /// on-kill/on-headshot) effects can be EQUIPPED but never trigger — the
    /// companion cannot generate the stacks itself (wiki `Galvanized_Mods`;
    /// user 2026-07-25). Only each mod's unconditional BASE part applies.
    BaseOnly,
}

/// A live on-kill stacking buff spec handed to the sim under
/// [`StackPolicy::Emergent`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackSpec {
    /// Contribution per stack (multishot: already × base pellets; CO:
    /// per-type rate).
    pub per_stack: f64,
    pub max_stacks: u32,
    /// Per-refresh duration; decay = lose ONE stack and reset (the
    /// Galvanized family's graceful decay).
    pub duration: f64,
    /// Stacks at t = 0 (user setting: full by default, 0 for a cold
    /// start; afterwards mechanics rule either way).
    pub initial_stacks: u32,
}

/// How the Condition Overload bonus behaves — PER WEAPON (user,
/// 2026-07-24: "有的武器是独立的加成，有的武器是当基础伤害的，有的还
/// 加成不到"; the wiki CO-mechanic catalog classifies weapons):
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoBehavior {
    /// Joins the base-damage bucket (additive with Hornet Strike):
    /// direct hit × (1 + bd + co × types) / (1 + bd).
    AdditiveWithBaseDamage,
    /// A free-standing final multiplier on direct hits:
    /// direct hit × (1 + co × types).
    Independent,
    /// The bonus simply does not apply on this weapon.
    Inert,
}

/// A weapon's unmodded panel (fixed evolutions folded in — they alter the
/// weapon's BASE stats before mods).
#[derive(Debug, Clone)]
pub struct WeaponBase {
    pub base_vector: DamageVector,
    pub base_crit_chance: f64,
    pub base_crit_damage: f64,
    pub base_status_chance: f64,
    pub base_fire_rate: f64,
    /// Stored pellet count (wiki Multishot).
    pub base_multishot: f64,
    /// Extra additive multishot from non-mod sources at assumed-max
    /// (Fevered Frenzy's 20 stacks = +1.0).
    pub buff_multishot_bonus: f64,
    pub magazine_size: f64,
    pub base_reload: f64,
    /// Unconditional CO rate baked into the weapon config (Carnage
    /// Reign's +33% per status type) — additive with mod CO sources.
    pub innate_co_per_type: f64,
    /// This weapon's Condition Overload behavior class.
    pub co_behavior: CoBehavior,
    /// CO base effectiveness: the CO bonus is computed on the ORIGINAL
    /// base damage, EXCLUDING evolution flat damage (wiki CO catalog:
    /// "CO-bonus does not use base damage increase Evolution"; DT row
    /// "100% or 56%"). = original_base / evolved_base.
    pub co_base_fraction: f64,
    /// Buff-injected elements as RELATIVE bonuses (element, bonus): each
    /// contributes ModifiedBase × bonus at the END of the hierarchy
    /// (rule 8) — Frenzy's +100% Toxin on the base Dual Toxocyst.
    pub injected_elements: Vec<(DamageType, f64)>,
}

/// Dual Toxocyst's ORIGINAL base damage total (both forms), before any
/// evolution flat damage — the base the CO bonus is computed on.
pub const DT_ORIGINAL_BASE_TOTAL: f64 = 75.0;

/// The Evolution II choice — a SEARCH DIMENSION (user, 2026-07-25).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DtEvo2 {
    /// +50 base pro-rata (75→125) + 20 permanent multishot stacks
    /// (+100%, pre-stacked per the user's initial-full setting).
    FeveredFrenzy,
    /// +60 base pro-rata (75→135) + unconditional +33% CO per status
    /// type (adding class; its CO base excludes its own +60; the
    /// energy ≥ 200 requirement is assumed met).
    CarnageReign,
}

impl DtEvo2 {
    fn vector_scale(self) -> f64 {
        match self {
            DtEvo2::FeveredFrenzy => 125.0 / 75.0,
            DtEvo2::CarnageReign => 135.0 / 75.0,
        }
    }

    fn buff_multishot(self) -> f64 {
        match self {
            DtEvo2::FeveredFrenzy => 1.0,
            DtEvo2::CarnageReign => 0.0,
        }
    }

    fn innate_co(self) -> f64 {
        match self {
            DtEvo2::FeveredFrenzy => 0.0,
            DtEvo2::CarnageReign => 0.33,
        }
    }

    fn co_fraction(self) -> f64 {
        DT_ORIGINAL_BASE_TOTAL / (DT_ORIGINAL_BASE_TOTAL * self.vector_scale())
    }
}

impl WeaponBase {
    /// Dual Toxocyst Incarnon Form with the fixed evolutions (Commodore's
    /// Fortune +0.20 into BASE crit chance; Evolved Autoloader is
    /// holstered-only) and the CHOSEN Evolution II (`evo2` — a search
    /// dimension). `frenzy_active`: the passive works while transformed
    /// (user-confirmed) — folds its +100% Toxin injection in.
    pub fn dual_toxocyst_incarnon(frenzy_active: bool, evo2: DtEvo2) -> Self {
        Self {
            base_vector: DamageVector::new()
                .with(DamageType::Impact, 15.0)
                .with(DamageType::Puncture, 37.5)
                .with(DamageType::Slash, 22.5)
                .scale(evo2.vector_scale()),
            base_crit_chance: 0.31, // 11% + Commodore's Fortune 20%
            base_crit_damage: 3.0,
            base_status_chance: 0.43,
            base_fire_rate: 4.5,
            base_multishot: 1.0,
            buff_multishot_bonus: evo2.buff_multishot(),
            magazine_size: 270.0,
            base_reload: 3.35,
            // Wiki CO catalog row: "Adding" class; the CO base EXCLUDES
            // evolution flat damage ("100% or 56%") — derived per evo2.
            innate_co_per_type: evo2.innate_co(),
            co_behavior: CoBehavior::AdditiveWithBaseDamage,
            co_base_fraction: evo2.co_fraction(),
            injected_elements: if frenzy_active {
                vec![(DamageType::Toxin, 1.0)]
            } else {
                Vec::new()
            },
        }
    }

    /// Dual Toxocyst **base form** with the same fixed evolutions and the
    /// chosen Evolution II. `frenzy_active` folds the +100% Toxin
    /// injection in (exact under a Permanent lock).
    pub fn dual_toxocyst_base(frenzy_active: bool, evo2: DtEvo2) -> Self {
        Self {
            base_vector: DamageVector::new()
                .with(DamageType::Impact, 7.5)
                .with(DamageType::Puncture, 60.0)
                .with(DamageType::Slash, 7.5)
                .scale(evo2.vector_scale()),
            base_crit_chance: 0.25, // 5% + Commodore's Fortune 20%
            base_crit_damage: 2.0,
            base_status_chance: 0.37,
            base_fire_rate: 1.0, // semi-auto; Frenzy ×2.5 applies live
            base_multishot: 1.0,
            buff_multishot_bonus: evo2.buff_multishot(),
            magazine_size: 12.0,
            base_reload: 2.35,
            innate_co_per_type: evo2.innate_co(),
            co_behavior: CoBehavior::AdditiveWithBaseDamage,
            co_base_fraction: evo2.co_fraction(),
            injected_elements: if frenzy_active {
                vec![(DamageType::Toxin, 1.0)]
            } else {
                Vec::new()
            },
        }
    }

    /// Verglas Prime — Nautilus Prime's robotic (sentinel) weapon. The purest
    /// test case: no arcane, no Incarnon, no evolutions. Its innate damage is
    /// **100% Cold (32)** — the first innate-elemental weapon in the database
    /// (see the innate-element branch in [`resolve`]). Accepts Rifle Mods.
    /// Wiki `Verglas_Prime` (2026-07-25): cc 14% / cd 2.2× / sc 36% / fire
    /// rate 12 / magazine 80 / reload 1.6 s / multishot 1. Sentinel weapons
    /// resolve their mods under [`StackPolicy::BaseOnly`] — Galvanized and
    /// other on-kill effects are equippable but never trigger.
    pub fn verglas_prime() -> Self {
        Self {
            base_vector: DamageVector::new().with(DamageType::Cold, 32.0),
            base_crit_chance: 0.14,
            base_crit_damage: 2.2,
            base_status_chance: 0.36,
            base_fire_rate: 12.0,
            base_multishot: 1.0,
            buff_multishot_bonus: 0.0,
            magazine_size: 80.0,
            base_reload: 1.6,
            innate_co_per_type: 0.0,
            co_behavior: CoBehavior::Independent,
            co_base_fraction: 1.0,
            injected_elements: Vec::new(),
        }
    }
}

/// The resolved panel: everything the dummy sim needs from layers [1]+[2].
#[derive(Debug, Clone)]
pub struct ResolvedPanel {
    /// Post-hierarchy damage vector (physical × (1+bd) + combined elements).
    pub damage: DamageVector,
    /// ModifiedBase = unmodded total × (1 + Σ base damage) — the base of
    /// every status-payload formula (elemental portions excluded).
    pub modified_base: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
    pub status_chance: f64,
    pub fire_rate: f64,
    pub multishot: f64,
    pub magazine_size: f64,
    pub reload_seconds: f64,
    /// Σ reload-speed bonuses — transitions (Incarnon transmute/revert)
    /// scale by the same formula: time = base / (1 + this).
    pub reload_bonus: f64,
    /// Σ base-damage bonuses (needed live when CO joins this bucket).
    pub base_damage_bonus: f64,
    /// Σ (CO per_stack × stacks) under `AssumedMax` (0 under
    /// `Emergent` — see `co_stack`) — applied per this weapon's
    /// [`CoBehavior`] × `co_base_fraction`, DIRECT HITS ONLY.
    pub co_per_type: f64,
    pub co_behavior: CoBehavior,
    pub co_base_fraction: f64,
    /// Live on-kill CO stacks (Emergent policy).
    pub co_stack: Option<StackSpec>,
    /// Live on-kill multishot stacks (Emergent policy); per_stack is
    /// already × base pellets.
    pub ms_stack: Option<StackSpec>,
    /// Crosshairs' on-headshot buff (Emergent): ABSOLUTE crit chance
    /// (base_cc × bonus) and its duration.
    pub cc_on_headshot: Option<(f64, f64)>,
    /// Crosshairs' on-headshot-kill stacks (Emergent): per_stack is
    /// ABSOLUTE crit chance; per-stack expiry semantics.
    pub cc_stack: Option<StackSpec>,
    /// (1 + Σ status damage) — multiplies status payload values.
    pub status_damage_mult: f64,
    /// Summed INDIRECT buckets (recoil, accuracy, ammo…): outside the
    /// theoretical-DPS math, stated on the panel; a future shooter model
    /// (2D recoil/aim, travel time, ammo sustain) consumes them.
    pub indirect: Vec<(IndirectStat, f64)>,
    /// (element, 1 + Σ that element's bonuses) — the elemental bracket of
    /// DoT tick formulas (only literal same-element mods count).
    pub elem_dot_bonus: Vec<(DamageType, f64)>,
}

/// Resolve a mod set in slot order against a weapon base.
pub fn resolve(base: &WeaponBase, mods: &[&ModDef], policy: StackPolicy) -> ResolvedPanel {
    let (mut bd, mut ms, mut cc, mut cd, mut sc, mut fr, mut rl, mut sd) =
        (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    // Unconditional weapon-level CO (Carnage Reign) seeds the static rate.
    let mut co = base.innate_co_per_type;
    let (mut co_stack, mut ms_stack): (Option<StackSpec>, Option<StackSpec>) = (None, None);
    let mut cc_on_headshot: Option<(f64, f64)> = None;
    let mut cc_stack: Option<StackSpec> = None;
    let mut elem_bonus: Vec<(DamageType, f64)> = Vec::new();
    let mut indirect: Vec<(IndirectStat, f64)> = Vec::new();

    for m in mods {
        for e in &m.effects {
            match *e {
                ModEffect::BaseDamage(v) => bd += v,
                ModEffect::Multishot(v) => ms += v,
                ModEffect::CritChance(v) => cc += v,
                ModEffect::CritDamage(v) => cd += v,
                ModEffect::StatusChance(v) => sc += v,
                ModEffect::FireRate(v) => fr += v,
                ModEffect::ReloadSpeed(v) => rl += v,
                ModEffect::StatusDamage(v) => sd += v,
                ModEffect::Element(t, v) | ModEffect::CombinedElement(t, v) => {
                    if let Some(x) = elem_bonus.iter_mut().find(|(a, _)| *a == t) {
                        x.1 += v;
                    } else {
                        elem_bonus.push((t, v));
                    }
                }
                ModEffect::OnKillMultishot {
                    per_stack,
                    max_stacks,
                    duration,
                } => match policy {
                    StackPolicy::AssumedMax => ms += per_stack * max_stacks as f64,
                    StackPolicy::Emergent => {
                        ms_stack = Some(StackSpec {
                            per_stack: base.base_multishot * per_stack,
                            max_stacks,
                            duration,
                            initial_stacks: max_stacks, // 初始满 (user)
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::ConditionOverload {
                    per_stack,
                    max_stacks,
                    duration,
                } => match policy {
                    StackPolicy::AssumedMax => co += per_stack * max_stacks as f64,
                    StackPolicy::Emergent => {
                        co_stack = Some(StackSpec {
                            per_stack,
                            max_stacks,
                            duration,
                            initial_stacks: max_stacks, // 初始满 (user)
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::OnHeadshotCritChance { bonus, duration } => match policy {
                    StackPolicy::AssumedMax => cc += bonus,
                    StackPolicy::Emergent => {
                        cc_on_headshot = Some((base.base_crit_chance * bonus, duration))
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::OnHeadshotKillCritChance {
                    per_stack,
                    max_stacks,
                    duration,
                } => match policy {
                    StackPolicy::AssumedMax => cc += per_stack * max_stacks as f64,
                    StackPolicy::Emergent => {
                        cc_stack = Some(StackSpec {
                            per_stack: base.base_crit_chance * per_stack,
                            max_stacks,
                            duration,
                            initial_stacks: max_stacks, // 初始满 (user)
                        })
                    }
                    StackPolicy::BaseOnly => {} // sentinel: conditional never fires
                },
                ModEffect::Indirect(stat, v) => {
                    if let Some(x) = indirect.iter_mut().find(|(s, _)| *s == stat) {
                        x.1 += v;
                    } else {
                        indirect.push((stat, v));
                    }
                }
                // Conditional handling buff (Reflex Draw): handling-only and
                // temporary — listed from the card, never a static panel stat.
                ModEffect::OnEquipHandling { .. } => {}
            }
        }
    }

    let modified_base = base.base_vector.total() * (1.0 + bd);

    // Split the (base-damage-scaled) innate vector. Physical IPS stays a fixed
    // component; innate PRIMARY elements (Verglas Prime's Cold, etc.) enter the
    // hierarchy at position 0 — BEFORE any mod element — so mod elements combine
    // with them (innate Cold + a Heat mod -> Blast). Wiki Load Order: innate
    // elementals are placed first and scale with base-damage mods like the rest
    // of the base. (A physical-innate weapon leaves `input` empty here, so this
    // is a no-op for Dual Toxocyst.)
    let scale = 1.0 + bd;
    let mut physical = DamageVector::new();
    let mut input = ElementalInput::default();
    for (t, v) in base.base_vector.iter_nonzero() {
        if t.is_primary_element() {
            input.push(t, v * scale);
        } else {
            physical.add(t, v * scale);
        }
    }

    // Mod-added elements append AFTER the innate ones, in mod order (first
    // placement establishes an element's position; later same-element mods
    // merge there).
    for m in mods {
        for e in &m.effects {
            match *e {
                ModEffect::Element(t, v) => input.push(t, modified_base * v),
                ModEffect::CombinedElement(t, v) => {
                    input.direct_secondary.push((t, modified_base * v))
                }
                _ => {}
            }
        }
    }
    for &(t, bonus) in &base.injected_elements {
        input.injected.push((t, modified_base * bonus));
        // The injection "behaves like a Toxin mod, additive with
        // elemental mods" (frenzy.yaml) — so it ALSO raises that
        // element's DoT tick bracket (1 + element bonuses).
        if let Some(x) = elem_bonus.iter_mut().find(|(a, _)| *a == t) {
            x.1 += bonus;
        } else {
            elem_bonus.push((t, bonus));
        }
    }
    let damage = elements::combine(&physical, &input);

    ResolvedPanel {
        damage,
        modified_base,
        crit_chance: base.base_crit_chance * (1.0 + cc),
        crit_damage: base.base_crit_damage * (1.0 + cd),
        status_chance: base.base_status_chance * (1.0 + sc),
        fire_rate: base.base_fire_rate * (1.0 + fr),
        multishot: base.base_multishot * (1.0 + base.buff_multishot_bonus + ms),
        magazine_size: base.magazine_size,
        reload_seconds: base.base_reload / (1.0 + rl),
        reload_bonus: rl,
        base_damage_bonus: bd,
        co_behavior: base.co_behavior,
        co_base_fraction: base.co_base_fraction,
        co_per_type: co,
        co_stack,
        ms_stack,
        cc_on_headshot,
        cc_stack,
        status_damage_mult: 1.0 + sd,
        elem_dot_bonus: elem_bonus.into_iter().map(|(t, v)| (t, 1.0 + v)).collect(),
        indirect,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use DamageType::*;

    fn m(id: &'static str, effects: Vec<ModEffect>) -> ModDef {
        ModDef {
            id,
            base_drain: 10,
            max_rank: 10,
            polarity: Polarity::Madurai,
            rarity: Rarity::Common,
            exilus: false,
            family: None,
            effects,
        }
    }

    #[test]
    fn dual_toxocyst_panel_resolves_by_hand() {
        // Hornet +220%, Frostbite (cold 60/SC 60), Jolt (elec 60/SC 60),
        // PPG +187% cc, PTC +110% cd, Lethal Torrent (FR 60/MS 60),
        // Galvanized Shot (+80% SC, CO 0.4×3), Galvanized Diffusion
        // (+110% MS, +30%×4 on kill).
        let mods = [
            m("hornet", vec![ModEffect::BaseDamage(2.20)]),
            m(
                "frostbite",
                vec![
                    ModEffect::Element(Cold, 0.60),
                    ModEffect::StatusChance(0.60),
                ],
            ),
            m(
                "jolt",
                vec![
                    ModEffect::Element(Electricity, 0.60),
                    ModEffect::StatusChance(0.60),
                ],
            ),
            m("ppg", vec![ModEffect::CritChance(1.87)]),
            m("ptc", vec![ModEffect::CritDamage(1.10)]),
            m(
                "lt",
                vec![ModEffect::FireRate(0.60), ModEffect::Multishot(0.60)],
            ),
            m(
                "gshot",
                vec![
                    ModEffect::StatusChance(0.80),
                    ModEffect::ConditionOverload {
                        per_stack: 0.40,
                        max_stacks: 3,
                        duration: 14.0,
                    },
                ],
            ),
            m(
                "gdiff",
                vec![
                    ModEffect::Multishot(1.10),
                    ModEffect::OnKillMultishot {
                        per_stack: 0.30,
                        max_stacks: 4,
                        duration: 20.0,
                    },
                ],
            ),
        ];
        let refs: Vec<&ModDef> = mods.iter().collect();
        let p = resolve(
            &WeaponBase::dual_toxocyst_incarnon(false, DtEvo2::FeveredFrenzy),
            &refs,
            StackPolicy::AssumedMax,
        );

        // ModifiedBase: 125 × 3.2 = 400.
        assert!((p.modified_base - 400.0).abs() < 1e-9);
        // Physical keeps its 3.2×; Cold+Electricity (adjacent) -> Magnetic
        // of 2 × 0.6 × 400 = 480; total = 400 + 480 = 880.
        assert!((p.damage.get(Magnetic) - 480.0).abs() < 1e-9);
        assert!((p.damage.get(Puncture) - 200.0).abs() < 1e-9);
        assert!((p.damage.total() - 880.0).abs() < 1e-9);
        // cc 0.31 × 2.87; cd 3 × 2.1; sc 0.43 × 3.0; fr 4.5 × 1.6.
        assert!((p.crit_chance - 0.8897).abs() < 1e-9);
        assert!((p.crit_damage - 6.3).abs() < 1e-9);
        assert!((p.status_chance - 1.29).abs() < 1e-9);
        assert!((p.fire_rate - 7.2).abs() < 1e-9);
        // MS: 1 × (1 + 1.0 Fevered + 0.6 + 1.1 + 1.2 stacks) = 4.9.
        assert!((p.multishot - 4.9).abs() < 1e-9);
        // CO assumed max: 0.4 × 3 = 1.2. No status-damage mods.
        assert!((p.co_per_type - 1.2).abs() < 1e-9);
        assert!((p.status_damage_mult - 1.0).abs() < 1e-9);
        // Elemental DoT brackets: cold 1.6, electricity 1.6.
        assert!(p.elem_dot_bonus.contains(&(Cold, 1.6)));
        assert!(p.elem_dot_bonus.contains(&(Electricity, 1.6)));
    }

    #[test]
    fn injected_toxin_raises_the_toxin_dot_bracket_too() {
        // Frenzy's +100% Toxin behaves like a Toxin mod: with Pistol
        // Pestilence (+60%) the Poison tick bracket is 1 + 0.6 + 1.0.
        let pest = m(
            "pestilence",
            vec![
                ModEffect::Element(Toxin, 0.60),
                ModEffect::StatusChance(0.60),
            ],
        );
        let base = WeaponBase::dual_toxocyst_incarnon(true, DtEvo2::FeveredFrenzy);
        let p = resolve(&base, &[&pest], StackPolicy::AssumedMax);
        assert!(p
            .elem_dot_bonus
            .iter()
            .any(|&(t, v)| t == Toxin && (v - 2.6).abs() < 1e-9));
        // And the injection joined the vector: toxin mod + injection all
        // land as pure Toxin (no partner element): 125 × (0.6 + 1.0).
        assert!((p.damage.get(Toxin) - 200.0).abs() < 1e-9);
    }

    #[test]
    fn element_mod_order_changes_the_combination() {
        let heat = m("scorch", vec![ModEffect::Element(Heat, 0.60)]);
        let cold = m("frostbite", vec![ModEffect::Element(Cold, 0.60)]);
        let tox = m("pestilence", vec![ModEffect::Element(Toxin, 0.60)]);
        let base = WeaponBase::dual_toxocyst_incarnon(false, DtEvo2::FeveredFrenzy);

        // Heat,Cold,Toxin -> Blast + trailing Toxin.
        let p1 = resolve(&base, &[&heat, &cold, &tox], StackPolicy::AssumedMax);
        assert!(p1.damage.get(Blast) > 0.0);
        assert!(p1.damage.get(Toxin) > 0.0);
        // Cold,Toxin,Heat -> Viral + trailing Heat.
        let p2 = resolve(&base, &[&cold, &tox, &heat], StackPolicy::AssumedMax);
        assert!(p2.damage.get(Viral) > 0.0);
        assert!(p2.damage.get(Heat) > 0.0);
        // Totals identical either way.
        assert!((p1.damage.total() - p2.damage.total()).abs() < 1e-9);
    }

    #[test]
    fn verglas_innate_cold_combines_with_mod_elements() {
        // Innate Cold(32) enters the hierarchy at position 0, so a Heat mod
        // pairs with it into Blast (not pure Cold + pure Heat). No base-damage
        // mod -> modified_base 32; Heat mod = 32 × 0.9 = 28.8; Blast = 60.8.
        let heat = m("hellfire", vec![ModEffect::Element(Heat, 0.90)]);
        let p = resolve(&WeaponBase::verglas_prime(), &[&heat], StackPolicy::BaseOnly);
        assert!((p.damage.get(Blast) - 60.8).abs() < 1e-9);
        assert_eq!(p.damage.get(Cold), 0.0);
        assert_eq!(p.damage.get(Heat), 0.0);

        // Innate Cold + a Toxin mod -> Viral.
        let tox = m("infected_clip", vec![ModEffect::Element(Toxin, 0.90)]);
        let pv = resolve(&WeaponBase::verglas_prime(), &[&tox], StackPolicy::BaseOnly);
        assert!((pv.damage.get(Viral) - 60.8).abs() < 1e-9);
    }

    #[test]
    fn sentinel_base_only_ignores_galvanized_conditional() {
        // Galvanized Chamber: base +55% multishot + on-kill +25%×5. On a
        // sentinel only the BASE applies — no live stacks are generated.
        let gchamber = m(
            "galvanized_chamber",
            vec![
                ModEffect::Multishot(0.55),
                ModEffect::OnKillMultishot { per_stack: 0.25, max_stacks: 5, duration: 20.0 },
            ],
        );
        let p = resolve(&WeaponBase::verglas_prime(), &[&gchamber], StackPolicy::BaseOnly);
        assert!((p.multishot - 1.55).abs() < 1e-9);
        assert!(p.ms_stack.is_none());
    }
}
