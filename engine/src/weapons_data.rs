//! Weapon data loader: `data/weapons/*.yaml` → [`WeaponBase`] + registry
//! metadata (CORE.md §2.3: weapon numbers are DATA; the engine only holds
//! rules). The yamls are the source of record — `loadout`'s per-weapon
//! constructors delegate here, and the web registry derives its weapon list,
//! tags, polarities and form descriptors from the same specs.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;

use crate::damage::{DamageType, DamageVector};
use crate::loadout::{
    ChargeOn, CoBehavior, FieldStacking, IncarnonForm, LingeringBase, RadialBase, WeaponBase,
};
use crate::mods::Polarity;

/// What one deployment changes about a weapon. Every field is optional: a
/// column states only where it differs from the entry's own.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DeploymentSpec {
    #[serde(default)]
    pub reload_seconds: Option<f64>,
    #[serde(default)]
    pub magazine: Option<f64>,
    #[serde(default)]
    pub ammo_max: Option<f64>,
    #[serde(default)]
    pub no_resupply: Option<bool>,
}

/// Apply a DEPLOYMENT's overrides to a resolved base, in place.
///
/// A no-op for the weapon's own deployment (its fields already are that
/// column) and for a name it does not have — an unknown environment leaves the
/// weapon alone rather than half-applying something.
pub fn apply_deployment(base: &mut WeaponBase, id: &str, deployment: &str) {
    let Some(s) = spec(id) else { return };
    if s.deployment.as_deref() == Some(deployment) {
        return;
    }
    let Some(d) = s.deployments.get(deployment) else { return };
    if let Some(v) = d.reload_seconds {
        base.base_reload = v;
    }
    if let Some(v) = d.magazine {
        base.magazine_size = v;
    }
    if let Some(v) = d.ammo_max {
        base.ammo_reserve = v;
    }
    if let Some(v) = d.no_resupply {
        base.no_resupply = v;
    }
}

/// Every deployment this weapon has, its OWN first. Fewer than two means the
/// axis does not exist for it and nothing should offer a choice.
pub fn deployments_of(id: &str) -> Vec<String> {
    let Some(s) = spec(id) else { return Vec::new() };
    let Some(own) = s.deployment.clone() else { return Vec::new() };
    let mut out = vec![own];
    out.extend(s.deployments.keys().cloned());
    out
}

#[derive(Debug, Clone, Deserialize)]
pub struct AttackSpec {
    pub trigger: String,
    /// Ammo spent per SHOT (per tick on a continuous weapon). Default 1.
    ///
    /// Read at last (2026-08-01). It sat in every weapon file unread while the
    /// sim spent a flat 1.0, which was harmless only while no weapon disagreed
    /// with it — the Larkspur Prime disagrees on BOTH of its modes: "Alt-fire
    /// consumes 10 ammo per shot" against "0.5 per primary tick" (wiki).
    #[serde(default = "one")]
    pub ammo_cost: f64,
    #[serde(default)]
    pub shot_type: Option<String>,
    pub fire_rate: f64,
    /// The DRAW before the shot (the module's per-attack `ChargeTime`), and
    /// with it the weapon's real cadence. VERBATIM (wiki Fire Rate), the bow
    /// formula and the one it excludes bows from:
    ///
    /// - *"Effective Fire Rate = 1 / (Modded Charge Time + Modded Reload
    ///   Time)"* — "Calculation for true fire rate for **bow** weapons."
    /// - *"1 / (Modded Charge Time + 1 / Modded Fire Rate)"* — "for charge
    ///   weapons **with the exception of bows**, Epitaph, and Lanka."
    ///
    /// So a BOW's cadence carries no fire-rate term at all: draw plus nock.
    /// Every bow attack states this — **`0.0` for the tapped shot**, which is
    /// not an absence but the statement that releasing early costs no draw, so
    /// the nock alone paces it. Fire-rate bonuses DIVIDE it (*"Charge Time =
    /// Base Charge Time / (1 + Mod Bonus)"*), which is why `fire_rate` stays
    /// the listed stat: it is the number the fire-rate GATES read.
    ///
    /// The roster has no non-bow charge weapon yet. When one arrives it needs
    /// the other formula — `1 / fire_rate` in place of the reload — which is a
    /// second cadence rule, not a tweak to this one.
    #[serde(default)]
    pub charge_seconds: Option<f64>,
    #[serde(default = "one")]
    pub multishot: f64,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub status_chance: f64,
    pub damage: BTreeMap<String, f64>,
    #[serde(default)]
    pub ricochet: Option<RicochetSpec>,
    /// A radial (AoE) part fired with every projectile of this attack.
    #[serde(default)]
    pub radial: Option<RadialSpec>,
    /// A LINGERING FIELD left by every landed projectile (Torid's cloud).
    #[serde(default)]
    pub lingering: Option<LingeringSpec>,
    /// Continuous-beam geometry (Torid Incarnon). Shape, not a damage part.
    #[serde(default)]
    pub beam: Option<BeamSpec>,
}

/// A lingering damage FIELD — MECHANICS §7 "Lingering damage FIELDS". Unlike
/// the radial this is not one instance at impact: it persists and TICKS.
#[derive(Debug, Clone, Deserialize)]
pub struct LingeringSpec {
    pub damage: BTreeMap<String, f64>,
    /// Ticks per second (the data module's per-attack `FireRate`).
    pub tick_rate: f64,
    /// Field lifetime in seconds (`EffectDuration`).
    pub duration_s: f64,
    pub radius_m: f64,
    #[serde(default)]
    pub crit_chance: Option<f64>,
    #[serde(default)]
    pub crit_multiplier: Option<f64>,
    #[serde(default)]
    pub status_chance: Option<f64>,
    #[serde(default)]
    pub falloff_start_m: Option<f64>,
    #[serde(default)]
    pub falloff_reduction: Option<f64>,
    /// Does this field take Condition Overload? Default NO: the mods say CO
    /// boosts DIRECT hits, so an AoE part getting it is the exception the CO
    /// catalog spells out per weapon.
    #[serde(default)]
    pub takes_condition_overload: bool,
    /// `stack` (default) or `refresh`. The Torid STACKS — ✅ measured
    /// (MEASUREMENTS M13) — but this stays weapon DATA rather than a constant:
    /// the branch is per weapon, and a future one may refresh.
    #[serde(default = "stack")]
    pub stacking: String,
}

fn stack() -> String {
    "stack".to_string()
}

/// A continuous BEAM's geometry — range, its impact sphere, and the chain.
///
/// Deliberately NOT `RadialSpec`: a radial is a second damage INSTANCE, and
/// the wiki forbids that reading here ("the damage radius is not a separate
/// damage instance from the beam"). This is shape, not a damage part.
///
/// The single-target arena consumes none of it except `damage_radius_m`, which
/// Firestorm scales and the panel states. The rest is the multi-target model's
/// input, kept as values rather than prose per data/README.md.
#[derive(Debug, Clone, Deserialize)]
pub struct BeamSpec {
    pub range_m: f64,
    pub damage_radius_m: f64,
    /// The sphere does NOT take multishot; only the directly-hit target does.
    #[serde(default)]
    pub radius_takes_multishot: bool,
    pub chain: ChainSpec,
}

/// The chain a beam propagates through enemies.
#[derive(Debug, Clone, Deserialize)]
pub struct ChainSpec {
    /// Hops in ONE chain — a sequence, each at `damage_per_hop` of the last.
    pub hops: u32,
    pub range_m: f64,
    pub damage_per_hop: f64,
    /// Which targets start a chain (`radius_targets`: every enemy the sphere
    /// catches starts its own).
    pub origin: String,
    #[serde(default)]
    pub takes_multishot: bool,
    /// Does every chain NODE carry a sphere too, or only the beam's contact
    /// point? UNVERIFIED — a user call on in-game experience against four
    /// pieces of circumstantial evidence and no citation either way
    /// (MEASUREMENTS M15). A data switch so it costs one line to flip.
    #[serde(default)]
    pub nodes_have_radius: bool,
}

/// The radial (explosion) part of an attack — MECHANICS §7. Crit/status
/// default to the direct part's when the data does not state them.
#[derive(Debug, Clone, Deserialize)]
pub struct RadialSpec {
    pub damage: BTreeMap<String, f64>,
    pub radius_m: f64,
    #[serde(default)]
    pub crit_chance: Option<f64>,
    #[serde(default)]
    pub crit_multiplier: Option<f64>,
    #[serde(default)]
    pub status_chance: Option<f64>,
    #[serde(default)]
    pub falloff_start_m: Option<f64>,
    /// Fraction of damage REMOVED at maximum distance (Laetum: 0.2 → 80%).
    #[serde(default)]
    pub falloff_reduction: Option<f64>,
    /// Does this explosion take Condition Overload? Default NO — the mods say
    /// direct hits only, so an AoE part receiving it is a per-entry exception
    /// the CO catalog lists (the Zylok's Incarnon radial has such a row).
    #[serde(default)]
    pub takes_condition_overload: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RicochetSpec {
    pub targets: u32,
    pub range_m: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IncarnonSpec {
    pub gauge: GaugeSpec,
    /// Transition animations, unmodded; both scale by the reload formula.
    pub transmute_in_seconds: f64,
    pub transmute_out_seconds: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GaugeSpec {
    pub max_rounds: f64,
    /// Hits needed to fill the gauge (DT 9, Laetum 12, Torid 5).
    pub charges_to_fill: f64,
    /// WHICH hits count — `weakpoint_hits` (the Zariman default) or
    /// `direct_hits` (Torid). A REAL field: it was documentation-only in the
    /// yaml before, so every weapon silently charged off weak-point hits.
    #[serde(default = "weakpoint_hits")]
    pub charge_on: String,
}

fn weakpoint_hits() -> String {
    "weakpoint_hits".to_string()
}

/// Which of the wiki's two charge-weapon cadence formulas applies.
///
/// - `DrawOnly` — bows: "Effective Fire Rate = 1 / Modded Charge Time".
/// - `DrawThenRate` — everything else: "1 / (Modded Charge Time + 1 / Modded
///   Fire Rate)". The listed rate is the cadence AFTER the charge, not the
///   whole cycle, so the two add.
///
/// Fire-rate bonuses shorten the charge in both ("Charge Time = Base Charge
/// Time / (1 + Mod Bonus)").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeCadence {
    DrawOnly,
    DrawThenRate,
}

/// The locked-gauge magazine/reload reduction of an Incarnon form.
#[derive(Debug, Clone, Deserialize)]
pub struct PseudoReloadSpec {
    pub magazine: f64,
    pub reload_seconds: f64,
}

/// A perk definition — either a `data/perks/*.yaml` entry or an inline
/// block in a weapon's `perks:` list. Both modes register the perk in the
/// GLOBAL namespace: define once anywhere, reference by bare id from
/// everywhere else (data/README.md).
#[derive(Debug, Clone, Deserialize)]
pub struct PerkSpec {
    pub id: String,
    #[serde(default)]
    pub grants: Option<GrantsSpec>,
}

/// One entry of a weapon's `perks:` list: a bare id string (a reference —
/// resolved against the table AND every inline definition) or a full
/// inline definition.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PerkRef {
    Id(String),
    Inline(PerkSpec),
}

impl PerkRef {
    /// Resolve to the perk definition (global-namespace lookup for ids).
    pub fn resolve(&self) -> &PerkSpec {
        match self {
            PerkRef::Inline(p) => p,
            PerkRef::Id(id) => {
                perk(id).unwrap_or_else(|| panic!("missing perk yaml: {id}"))
            }
        }
    }

    pub fn id(&self) -> &str {
        match self {
            PerkRef::Id(id) => id,
            PerkRef::Inline(p) => &p.id,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GrantsSpec {
    #[serde(default)]
    pub injected_element: Option<InjectedElementSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InjectedElementSpec {
    #[serde(rename = "type")]
    pub element: String,
    pub amount: f64,
}

/// The CLOSED vocabulary of attack FORMS. Weapons are operated differently one
/// from the next, but the handful of MODES they are operated in is shared —
/// so a form is a kind from this list, and every weapon entry REGISTERS which
/// one it is (`form:` in its yaml). Nothing may name a form the engine does
/// not know: an unknown string is a hard error, not a silent fallback.
///
/// Adding a kind is one arm here plus one in [`FormKind::parse`] — that is the
/// whole extension point. `alt_fire` is the obvious next one; it is NOT
/// declared until a weapon in `data/` needs it, because a kind nothing
/// registers is a kind nothing tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    /// The ordinary attack — what almost every weapon has, and by definition
    /// the UNCHARGED one. Verglas Prime has only this; the Torid, Laetum and
    /// Dual Toxocyst carry it as the form their Incarnon transforms out of.
    Base,
    /// A charge-trigger weapon's fully drawn shot (Cernos Prime). Its own kind
    /// because the draw REPLACES the fire-rate cadence and, on a bow, the
    /// damage is the uncharged base times the charge multiplier.
    Charged,
    /// The gauge-backed transformed form (Incarnon Genesis).
    Incarnon,
}

impl FormKind {
    /// The stable id — the wire value in an API request and in a saved preset,
    /// so these strings are durable names, not labels.
    pub fn id(self) -> &'static str {
        match self {
            FormKind::Base => "base",
            FormKind::Charged => "charged",
            FormKind::Incarnon => "incarnon",
        }
    }

    /// English display name (the i18n overlay translates from this).
    pub fn label(self) -> &'static str {
        match self {
            FormKind::Base => "Base Form",
            FormKind::Charged => "Charged Shot",
            FormKind::Incarnon => "Incarnon Form",
        }
    }

    /// Does ENTERING this form cost a gated transition — a resource meter to
    /// fill and an animation to play — rather than being a per-shot choice?
    ///
    /// This is what separates the two ways a weapon switches form, and it is a
    /// property of the KIND, not of the weapon: an Incarnon form is always
    /// paid for with a gauge and two transmute animations, while charged vs
    /// uncharged is chosen freely on every trigger pull. Only a gauge-switched
    /// form gives the sim a CYCLE to run.
    pub fn is_gauge_switched(self) -> bool {
        matches!(self, FormKind::Incarnon)
    }

    pub fn parse(s: &str) -> FormKind {
        match s {
            "base" => FormKind::Base,
            "charged" => FormKind::Charged,
            "incarnon" => FormKind::Incarnon,
            other => panic!("unknown form kind in weapon data: {other}"),
        }
    }
}

/// One registered form of a weapon: which yaml ENTRY provides it, what kind it
/// is, and whether it is the one the weapon is normally fired in.
#[derive(Debug, Clone, Copy)]
pub struct FormRef {
    /// The weapon entry id backing this form — the key `base_panel` takes.
    pub weapon_id: &'static str,
    pub kind: FormKind,
    /// The arsenal's form: the group's roster entry (the wiki module says it
    /// per weapon with `_TooltipAttackDisplay`).
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WeaponSpec {
    pub id: String,
    pub name: String,
    pub slot: String,
    pub class: String,
    /// Which DEPLOYMENT the fields on this entry describe (Arch-Guns:
    /// "atmosphere"). `None` = the weapon has only one.
    #[serde(default)]
    pub deployment: Option<String>,
    /// The OTHER deployments, by name, each stating only what it overrides.
    /// An Arch-Gun is the same weapon on the ground and in Archwing — same
    /// damage, same mods, same riven — and only its sustain differs, so this
    /// is a SCENARIO axis rather than a second weapon (user, 2026-08-01).
    #[serde(default)]
    pub deployments: BTreeMap<String, DeploymentSpec>,
    /// Does this weapon's INNATE headshot bonus multiply the additive bracket
    /// instead of joining it? A PER-WEAPON anomaly, not a class rule: the wiki
    /// lists innate bonuses (Kuva Chakkhurr) among the ADDITIVE sources and
    /// then singles one out — "Cernos Prime's headshot bonus is unique and
    /// stacks multiplicatively with Primary Deadhead's headshot bonus".
    #[serde(default)]
    pub headshot_bonus_multiplicative: bool,
    /// Which FORM of its weapon this entry is — a kind from the closed
    /// [`FormKind`] vocabulary. REQUIRED: a form is registered, never guessed,
    /// so a new entry cannot quietly inherit someone else's mode.
    pub form: String,
    /// Is this the form the weapon is normally fired in — the arsenal's, which
    /// the wiki module names per weapon with `_TooltipAttackDisplay`? Exactly
    /// one entry per transform group declares it, and that entry is the
    /// weapon's roster row. Declared rather than inferred from
    /// `transforms_from`, because two forms need not be a transformation:
    /// tapping a bow instead of drawing it switches form for free.
    #[serde(default)]
    pub default_form: bool,
    /// The mod POOLS this weapon draws from, as a union — `data/mods/<pool>/`.
    /// A weapon is not served by one list: a launcher takes both the
    /// primary-wide pool and the rifle class pool, and takes no assault-rifle
    /// or bow mods, which is why those are pools of their own.
    #[serde(default)]
    pub mod_pools: Vec<String>,
    /// Riven disposition — the multiplier every riven stat on this weapon is
    /// scaled by. It belongs to the WEAPON, not to the riven, which is why
    /// one riven reads differently on two guns.
    #[serde(default)]
    pub disposition: Option<f64>,
    /// The weapon's own rank ceiling — 30 for almost everything, 40 for the
    /// Kuva/Tenet/Coda families and the Paracesis. It decides CAPACITY, since
    /// capacity "correlates to their Rank" (wiki `Mod Capacity`) and a rank-40
    /// weapon climbs two ranks per Forma to reach it.
    ///
    /// The data has carried it since the roster was written and nothing read
    /// it: capacity was the literal 60 in four places instead.
    #[serde(default = "rank_30")]
    pub max_rank: u32,
    #[serde(default)]
    pub polarities: Vec<String>,
    #[serde(default)]
    pub exilus_polarity: Option<String>,
    #[serde(default)]
    pub magazine: Option<f64>,
    /// Reserve rounds outside the magazine — the wiki's "Ammo Max".
    ///
    /// Present on nearly every weapon and, until now, read by nobody: the sim
    /// treats reserves as INFINITE by default (decision 2026-07-24) because it
    /// does not model ammo PICKUPS, and a weapon that can be resupplied mid
    /// fight would otherwise run dry for a reason the game does not have.
    #[serde(default)]
    pub ammo_max: Option<f64>,
    /// Can this weapon NOT be refilled mid-fight? A ground Arch-Gun is the
    /// case this exists for: "Archguns only have a limited amount of ammo",
    /// and when it is gone the weapon is removed for a five-minute cooldown
    /// (wiki Arch-Gun). Everything else is resupplied from ammo pickups, which
    /// is what the Infinite-ammo default stands in for.
    ///
    /// THIS IS NOT "has a reserve" — that one is DERIVED from `ammo_max`, and
    /// the two were one flag until 2026-08-04. Conflating them meant the
    /// Infinite-ammo box was ticked AND DISABLED on every weapon but one,
    /// because "cannot be resupplied" was being read as "has no reserve at
    /// all" (owner: "只有 sentinel 是真的无限弹药"). A Torid has 60 rounds
    /// behind its magazine; what it also has is a way to get more.
    #[serde(default)]
    pub no_resupply: bool,
    #[serde(default)]
    pub reload_seconds: Option<f64>,
    #[serde(default)]
    pub co_behavior: Option<String>,
    /// How much of the base the CO term computes on (the catalog's "CO Damage
    /// Bonus Relative To Base Damage" column) when the WEAPON, not one of its
    /// evolutions, is what narrows it. A bow's charged shot: 0.5 — "CO-bonus
    /// only applies to base (uncharged) damage; bows have innate 2x damage
    /// multiplier when fully charged" (CO catalog, Cernos Prime row).
    /// Unset = 1.0, the normal case.
    #[serde(default)]
    pub co_base_fraction: Option<f64>,
    /// Innate additive headshot-damage bonus — the module's per-attack
    /// `ExtraHeadshotDmg` (Cernos Prime: 0.5). Joins the same additive
    /// headshot bracket the arcane and evolution bonuses use.
    #[serde(default)]
    pub headshot_damage_bonus: Option<f64>,
    #[serde(default)]
    pub transform_group: Option<String>,
    #[serde(default)]
    pub transforms_from: Option<String>,
    #[serde(default)]
    pub transforms_to: Option<String>,
    pub attack: AttackSpec,
    #[serde(default)]
    pub incarnon: Option<IncarnonSpec>,
    #[serde(default)]
    pub pseudo_reload: Option<PseudoReloadSpec>,
    /// The weapon's perks: id references into `data/perks/` or inline
    /// one-off definitions (each entry of a transform group lists its own —
    /// Frenzy is active in both Dual Toxocyst forms).
    #[serde(default)]
    pub perks: Vec<PerkRef>,
}

fn one() -> f64 {
    1.0
}

/// Every weapon entry in `data/weapons/` (embedded), parsed once.
pub fn all() -> &'static [WeaponSpec] {
    static SPECS: OnceLock<Vec<WeaponSpec>> = OnceLock::new();
    SPECS.get_or_init(|| {
        crate::data::files_under("weapons/")
            .filter(|(p, _)| p.ends_with(".yaml"))
            .map(|(p, text)| {
                serde_norway::from_str::<WeaponSpec>(text)
                    .unwrap_or_else(|e| panic!("parse {p}: {e}"))
            })
            .collect()
    })
}

pub fn spec(id: &str) -> Option<&'static WeaponSpec> {
    all().iter().find(|s| s.id == id)
}

/// Every perk entry in `data/perks/` (embedded), parsed once.
pub fn perks() -> &'static [PerkSpec] {
    static PERKS: OnceLock<Vec<PerkSpec>> = OnceLock::new();
    PERKS.get_or_init(|| {
        crate::data::files_under("perks/")
            .filter(|(p, _)| p.ends_with(".yaml"))
            .map(|(p, text)| {
                let spec = serde_norway::from_str::<PerkSpec>(text)
                    .unwrap_or_else(|e| panic!("parse {p}: {e}"));
                // Convention (data/README.md): the id matches the filename.
                let stem = p.rsplit('/').next().unwrap_or(p).trim_end_matches(".yaml");
                assert!(spec.id == stem, "{p}: id '{}' != filename", spec.id);
                spec
            })
            .collect()
    })
}

/// Find an inline perk definition among the given weapon specs.
fn inline_perk_in<'a>(
    id: &str,
    specs: impl Iterator<Item = &'a WeaponSpec>,
) -> Option<&'a PerkSpec> {
    specs
        .flat_map(|w| w.perks.iter())
        .find_map(|pr| match pr {
            PerkRef::Inline(p) if p.id == id => Some(p),
            _ => None,
        })
}

/// Perk lookup over the GLOBAL namespace: the `data/perks/` table first,
/// then every weapon's inline definitions. Defining a perk inline registers
/// it globally — any other entry may reference it by bare id (uniqueness is
/// enforced by the engine test suite, so a bare id is never ambiguous).
pub fn perk(id: &str) -> Option<&'static PerkSpec> {
    perks()
        .iter()
        .find(|p| p.id == id)
        .or_else(|| inline_perk_in(id, all().iter()))
}

/// Does this weapon carry a given perk? Weapon PASSIVES are per weapon —
/// Dual Toxocyst lists `frenzy`, the Laetum lists none — so anything that
/// applies a passive must ask, never assume. A transform group's second
/// form lists its own perks (Frenzy is active in both DT forms).
pub fn has_perk(weapon_id: &str, perk_id: &str) -> bool {
    spec(weapon_id).is_some_and(|s| s.perks.iter().any(|p| p.id() == perk_id))
}

/// Registry view: the SELECTABLE weapons — one row per weapon, which is the
/// entry that declares itself the DEFAULT form. Every other form (an Incarnon
/// form, a bow's tapped shot) is a form of that weapon, not its own row.
pub fn roster() -> impl Iterator<Item = &'static WeaponSpec> {
    all().iter().filter(|s| s.default_form)
}

impl WeaponSpec {
    pub fn form_kind(&self) -> FormKind {
        FormKind::parse(&self.form)
    }

    /// The transform group this entry belongs to — its own id when it is a
    /// group of one (Verglas Prime: one weapon, one form).
    pub fn group(&self) -> &str {
        self.transform_group.as_deref().unwrap_or(&self.id)
    }
}

/// The forms a weapon REGISTERS, default first.
///
/// A weapon's forms are the entries of its transform group: the two-weapons
/// model (decision 2026-07-24) gives every form its own yaml entry, and this
/// is the view that reads them back as ONE weapon with several modes. A
/// weapon with a single entry has exactly one form — that is the common case,
/// and it is a real registration (`form: base`), not an absence.
pub fn forms_of(weapon_id: &str) -> Vec<FormRef> {
    let Some(spec) = spec(weapon_id) else { return Vec::new() };
    let group = spec.group();
    let mut out: Vec<FormRef> = all()
        .iter()
        .filter(|s| s.group() == group)
        .map(|s| FormRef {
            weapon_id: &s.id,
            kind: s.form_kind(),
            is_default: s.default_form,
        })
        .collect();
    // Default first; the rest keep the vocabulary's order so two weapons never
    // list the same forms differently.
    out.sort_by_key(|f| (!f.is_default, f.kind as u8));
    out
}

/// Does this weapon have a form you TRANSFORM into (a gauge and two transmute
/// animations)? Only such a weapon has a cycle to simulate — anything else is
/// fired in one form at a time, whatever forms it registers.
pub fn has_gauge_switched_form(weapon_id: &str) -> bool {
    forms_of(weapon_id).iter().any(|f| f.kind.is_gauge_switched())
}

fn damage_type(name: &str) -> DamageType {
    match name {
        "impact" => DamageType::Impact,
        "puncture" => DamageType::Puncture,
        "slash" => DamageType::Slash,
        "heat" => DamageType::Heat,
        "cold" => DamageType::Cold,
        "electricity" => DamageType::Electricity,
        "toxin" => DamageType::Toxin,
        // Innate COMBINED elements (Laetum Incarnon's radial is 300
        // Radiation). They do not re-enter the elemental hierarchy — an
        // innate combined element stays as it is and mod elements combine
        // among themselves (wiki Damage/Elemental combination).
        "blast" => DamageType::Blast,
        "corrosive" => DamageType::Corrosive,
        "gas" => DamageType::Gas,
        "magnetic" => DamageType::Magnetic,
        "radiation" => DamageType::Radiation,
        "viral" => DamageType::Viral,
        "true" => DamageType::True,
        "void" => DamageType::Void,
        other => panic!("unknown damage type in weapon data: {other}"),
    }
}

fn rank_30() -> u32 {
    30
}

pub fn polarity(name: &str) -> Polarity {
    match name {
        "madurai" => Polarity::Madurai,
        "naramon" => Polarity::Naramon,
        "vazarin" => Polarity::Vazarin,
        "umbra" => Polarity::Umbra,
        other => panic!("unknown polarity in weapon data: {other}"),
    }
}

/// The arcane pools this weapon SEATS, in slot order.
///
/// Keyed on the equipment slot, which is what the game keys it on — an Arch-Gun
/// seats a primary AND a secondary arcane, a sentinel weapon seats none, and
/// everything else seats one of its own slot. A category rule, not per-weapon
/// data, which is why it is computed rather than declared.
///
/// It lived in `webapi` until 2026-08-05, when `builds` needed it too: "every
/// arcane seat filled" is part of what a complete build means, and a second
/// copy of this rule in the validator is how the page and the board come to
/// disagree about how many seats a weapon has.
pub fn arcane_pools(weapon: &str) -> Vec<&'static str> {
    let Some(s) = spec(weapon) else { return Vec::new() };
    if s.class.contains("sentinel") {
        return Vec::new();
    }
    match s.slot.as_str() {
        "archgun" => vec!["primary", "secondary"],
        "primary" => vec!["primary"],
        "secondary" => vec!["secondary"],
        "melee" => vec!["melee"],
        _ => Vec::new(),
    }
}

/// Innate MAIN-slot polarities as an 8-slot layout (exilus excluded — the
/// UI/optimizer model treats the exilus slot separately).
pub fn innate_slots(id: &str) -> [Option<Polarity>; 8] {
    let mut out = [None; 8];
    if let Some(s) = spec(id) {
        for (i, p) in s.polarities.iter().take(8).enumerate() {
            out[i] = Some(polarity(p));
        }
    }
    out
}

/// The exilus slot's innate polarity, if the weapon has one (wiki panel's
/// "Exilus Polarity"; Dual Toxocyst: Naramon).
pub fn exilus_polarity(id: &str) -> Option<Polarity> {
    spec(id)?.exilus_polarity.as_deref().map(polarity)
}

/// Weapon behavior traits consumed by arcane/mod `requires` gates. Traits
/// describe the WEAPON (its base form's trigger family), so both forms of a
/// transform group report the base entry's trigger.
/// What a `requires:` gate on a mod or arcane is checked against.
///
/// TWO KINDS, and only one of them existed until 2026-08-05: the firing
/// TRIGGER (`semi_auto`, `auto`), and the weapon CLASS (`shotgun`, `bow`,
/// `dual_pistols`, …). The class was missing, so `requires: dual_pistols` could
/// never be satisfied and Akimbo Slip Shot was silently inert on every dual
/// pistol in the game — the arcane equipped, contributed nothing, and said
/// nothing. Its unit test passed `&["dual_pistols"]` by hand, so it proved the
/// gate worked and never asked whether anything produced the trait.
///
/// The trigger comes from the BASE entry of a transform group (an Incarnon form
/// does not get its own), while the class is the weapon's own — the two halves
/// of one weapon share a class by construction.
fn traits_for(s: &WeaponSpec) -> &'static [&'static str] {
    static CACHE: OnceLock<Mutex<BTreeMap<String, &'static [&'static str]>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut g = cache.lock().expect("weapon traits cache");
    if let Some(t) = g.get(&s.id) {
        return t;
    }
    let base = s
        .transforms_from
        .as_deref()
        .and_then(spec)
        .unwrap_or(s);
    let mut out: Vec<&'static str> = Vec::new();
    match base.attack.trigger.as_str() {
        "semi_auto" => out.push("semi_auto"),
        "auto" => out.push("auto"),
        _ => {}
    }
    // Leaked because the class is data-driven and the caller wants a 'static
    // slice; the set is one entry per weapon and never grows at runtime.
    out.push(Box::leak(s.class.clone().into_boxed_str()));
    let leaked: &'static [&'static str] = Box::leak(out.into_boxed_slice());
    g.insert(s.id.clone(), leaked);
    leaked
}

/// Build the RAW (no evolutions, no mods) [`WeaponBase`] panel for a weapon
/// entry. `frenzy_active` folds passive-granted element injections in
/// (resolved from the transform group's base entry, where passives live).
pub fn base_panel(id: &str, frenzy_active: bool) -> WeaponBase {
    let s = spec(id).unwrap_or_else(|| panic!("unknown weapon id: {id}"));

    let mut vector = DamageVector::new();
    for (name, amount) in &s.attack.damage {
        vector = vector.with(damage_type(name), *amount);
    }

    let injected_elements = if frenzy_active {
        s.perks
            .iter()
            .filter_map(|p| p.resolve().grants.as_ref())
            .filter_map(|g| g.injected_element.as_ref())
            .map(|inj| (damage_type(&inj.element), inj.amount))
            .collect()
    } else {
        Vec::new()
    };

    // EVERY spelling is named, and anything else is a LOAD ERROR.
    //
    // `_ => Independent` used to swallow both a typo and an omission, and it
    // swallowed one: Boar Prime shipped `co_behavior: additive` — a spelling
    // that exists nowhere — and silently became Independent, i.e. the wiki's
    // "Multiplying", the EXCEPTION class, on a weapon the CO catalog does not
    // list at all (user, 2026-08-03). `independent` itself was never matched
    // either; it worked only because it fell through to the same arm.
    //
    // The default it implied is also backwards. The catalog "lists only
    // discrepant attacks. Anything not listed should be assumed to be Additive"
    // (docs/MECHANICS.md §Condition Overload), so the class an unlisted weapon
    // takes is ADDITIVE — there is no defensible default that is the exception.
    // Hence: state it, or fail.
    let co_behavior = match s.co_behavior.as_deref() {
        Some("additive_with_base_damage") => CoBehavior::AdditiveWithBaseDamage,
        Some("independent") => CoBehavior::Independent,
        Some("inert") => CoBehavior::Inert,
        other => panic!(
            "weapon {}: co_behavior must be additive_with_base_damage / independent / inert, got {other:?}.              The wiki CO catalog lists only DISCREPANT attacks — a weapon it does not list is              additive_with_base_damage, never independent.",
            s.id
        ),
    };

    // An Incarnon form's rounds are charge-backed; the locked-gauge
    // pseudo-reload supplies the sim's magazine/reload reduction.
    let (magazine_size, base_reload) = match &s.pseudo_reload {
        Some(pr) => (pr.magazine, pr.reload_seconds),
        None => (
            s.magazine.unwrap_or_else(|| panic!("{id}: magazine missing")),
            s.reload_seconds.unwrap_or_else(|| panic!("{id}: reload_seconds missing")),
        ),
    };

    let incarnon = s.incarnon.as_ref().map(|inc| IncarnonForm {
        max_charges: inc.gauge.max_rounds,
        charge_on: match inc.gauge.charge_on.as_str() {
            "weakpoint_hits" => ChargeOn::WeakpointHits,
            "direct_hits" => ChargeOn::DirectHits,
            other => panic!("{id}: unknown incarnon charge_on: {other}"),
        },
        charges_to_fill: inc.gauge.charges_to_fill,
        transmute_in: inc.transmute_in_seconds,
        transmute_out: inc.transmute_out_seconds,
        charge_rate: 0.0, // raised by evolutions (Incarnon Efficiency)
    });

    // The radial (AoE) attack part, when the weapon data declares one.
    let radial = s.attack.radial.as_ref().map(|r| {
        let mut v = DamageVector::new();
        for (t, val) in &r.damage {
            v.add(damage_type(t), *val);
        }
        RadialBase {
            base_vector: v,
            // Each stat falls back to the direct part's when unstated.
            base_crit_chance: r.crit_chance.unwrap_or(s.attack.crit_chance),
            base_crit_damage: r.crit_multiplier.unwrap_or(s.attack.crit_multiplier),
            base_status_chance: r.status_chance.unwrap_or(s.attack.status_chance),
            radius_m: r.radius_m,
            falloff_start_m: r.falloff_start_m.unwrap_or(0.0),
            falloff_reduction: r.falloff_reduction.unwrap_or(0.0),
            takes_condition_overload: r.takes_condition_overload,
        }
    });

    // The lingering FIELD (Torid's Toxin cloud). Each stat falls back to the
    // direct part's when unstated, same rule as the radial.
    let lingering = s.attack.lingering.as_ref().map(|f| {
        let mut v = DamageVector::new();
        for (t, val) in &f.damage {
            v.add(damage_type(t), *val);
        }
        LingeringBase {
            base_vector: v,
            base_crit_chance: f.crit_chance.unwrap_or(s.attack.crit_chance),
            base_crit_damage: f.crit_multiplier.unwrap_or(s.attack.crit_multiplier),
            base_status_chance: f.status_chance.unwrap_or(s.attack.status_chance),
            tick_rate: f.tick_rate,
            duration_s: f.duration_s,
            radius_m: f.radius_m,
            falloff_start_m: f.falloff_start_m.unwrap_or(0.0),
            falloff_reduction: f.falloff_reduction.unwrap_or(0.0),
            takes_condition_overload: f.takes_condition_overload,
            stacking: match f.stacking.as_str() {
                "stack" => FieldStacking::Stack,
                "refresh" => FieldStacking::Refresh,
                other => panic!("{id}: unknown lingering stacking: {other}"),
            },
        }
    });

    WeaponBase {
        // Empty until an evolution writes into it (evolutions_data::apply).
        indirect: Vec::new(),
        reload_damage_buff: 0.0,
        base_vector: vector,
        base_crit_chance: s.attack.crit_chance,
        base_crit_damage: s.attack.crit_multiplier,
        base_status_chance: s.attack.status_chance,
        base_fire_rate: s.attack.fire_rate,
        headshot_bonus_multiplicative: s.headshot_bonus_multiplicative,
        // Straight through: what a shot COSTS is a weapon constant, and no mod
        // in the roster changes it (ammo EFFICIENCY is its own, separate term).
        ammo_cost: s.attack.ammo_cost,
        // A BOW paces on draw + nock, every form of it (wiki Fire Rate's
        // bow-specific formula — see `AttackSpec::charge_seconds`), so a bow
        // states the draw even when it is 0.0 and anything else must not.
        // Silence here would mean falling back to the `1 / fire_rate` cadence,
        // which for a bow is the one reading the wiki rules out.
        charge_seconds: match (s.class.as_str(), s.attack.charge_seconds) {
            ("bow", Some(c)) => {
                // A zero draw makes the nock the whole cycle, which only reads
                // as a cadence while the magazine is the one nocked arrow.
                assert!(
                    c > 0.0 || s.magazine == Some(1.0),
                    "{id}: a 0.0 draw paces on the reload, so the magazine must be 1"
                );
                Some(c)
            }
            ("bow", None) => panic!("{id}: a bow's cadence is draw + nock — state charge_seconds"),
            // Every OTHER charge weapon uses the wiki's general formula,
            // which is a different sentence: "Effective Fire Rate =
            // 1 / (Modded Charge Time + 1 / Modded Fire Rate)". The listed
            // rate is not the whole cycle there — it is what happens AFTER
            // the charge completes — so the two are added, and the guard
            // that used to refuse this case now routes it (Larkspur Prime's
            // alt-fire: 0.5 s draw + 1/2.0 s = 1.0 s per shot).
            (_, Some(c)) => {
                assert!(c > 0.0, "{id}: a 0.0 charge outside a bow is just a fire rate");
                Some(c)
            }
            (_, None) => None,
        },
        // WHICH of the wiki's two charge formulas paces this weapon. A bow's
        // draw IS its cycle; everything else pays the draw and then the
        // listed rate's interval. Carried as a fact about the weapon rather
        // than re-derived from the class in the sim, which has no business
        // knowing what a bow is.
        charge_cadence: if s.class == "bow" {
            ChargeCadence::DrawOnly
        } else {
            ChargeCadence::DrawThenRate
        },
        // Does a FIRE-RATE bonus shorten the draw as well as the interval?
        //
        // The wiki's general charge formula says yes — "Charge Time = Base
        // Charge Time / (1 + Mod Bonus)". An ARCH-GUN is the exception (owner,
        // 2026-08-01): its fire rate governs only the interval between shots,
        // and the draw answers to a stat of its own. The mod cards are the
        // visible half of that split — Shell Rush is "+50% Charge Rate" where
        // Automatic Trigger is "+X% Fire Rate", and Archgun Ace grants
        // "Fire/Charge Rate", naming two things a single card would not.
        //
        // CHARGE-rate bonuses shorten the draw on every weapon; this flag is
        // only about fire rate. Kept beside `charge_cadence` because it is the
        // same kind of fact — how a weapon's draw and its rate compose — and
        // the sim has no business knowing what an Arch-Gun is.
        fire_rate_shortens_draw: s.class != "archgun",
        // "(x2 for Bows)" — the clause every fire-rate mod card carries
        // (Shred, Primed Shred, Speed Trigger, Vile Acceleration, Vigilante
        // Fervor, and the two DRAWBACK mods Critical Delay / Vile Precision,
        // so it doubles a penalty just as literally). It is keyed on the
        // weapon CLASS, not on a per-weapon flag: the cards say "for Bows",
        // and the wiki's rank tables carry a whole "Fire Rate (Bows)" column
        // at exactly twice the rifle one.
        fire_rate_mod_multiplier: if s.class == "bow" { 2.0 } else { 1.0 },
        base_multishot: s.attack.multishot,
        buff_multishot_bonus: 0.0,
        buff_ms_max_stacks: 0,
        magazine_size,
        // The reserve the sim may spend, and the two facts about it. HAVING
        // one is `ammo_max` — derived, because a weapon that states a reserve
        // has a reserve and there is nothing to declare twice. Being able to
        // REFILL it is the weapon's own business and is declared.
        ammo_reserve: s.ammo_max.unwrap_or(0.0),
        has_reserve: s.ammo_max.is_some_and(|a| a > 0.0),
        no_resupply: s.no_resupply,
        base_reload,
        innate_co_per_type: 0.0,
        co_behavior,
        // 1.0 = the CO term uses the FULL base, evolution damage included,
        // which is the normal case. An evolution that declares itself excluded
        // narrows it later (evolutions_data::apply); a WEAPON-level narrowing
        // (a bow's charged shot computing CO off the uncharged base) is
        // declared here, and no weapon has both.
        co_base_fraction: s.co_base_fraction.unwrap_or(1.0),
        injected_elements,
        traits: traits_for(s),
        incarnon,
        radial,
        lingering,
        // The data module's Trigger for a beam. Not cosmetic: it decides
        // whether `fire_rate` means shots or TICKS and whether multishot merges.
        continuous: s.attack.trigger == "held",
        beam: s.attack.beam.as_ref().map(|b| crate::loadout::BeamGeometry {
            range_m: b.range_m,
            damage_radius_m: b.damage_radius_m,
            radius_takes_multishot: b.radius_takes_multishot,
            chain_hops: b.chain.hops,
            chain_range_m: b.chain.range_m,
            chain_damage_per_hop: b.chain.damage_per_hop,
            chain_takes_multishot: b.chain.takes_multishot,
            chain_nodes_have_radius: b.chain.nodes_have_radius,
        }),
        field_duration_on_empty_reload: 1.0, // raised by Renewed Horror
        multishot_on_last_round: 0.0,        // raised by Final Fusillade
        multishot_ammo_bonus: 0.0,           // raised by Plentiful Mayhem
        // Raised by evolutions, never by the raw weapon data.
        evo_fire_rate_bonus: 0.0,
        post_mod_crit_chance: 0.0,
        post_mod_status_chance: 0.0,
        // Evolutions ADD to this (Caput Mortuum); a weapon's innate share is
        // the module's `ExtraHeadshotDmg`.
        headshot_damage_bonus: s.headshot_damage_bonus.unwrap_or(0.0),
        noncrit_bonus: None,
        plain_hit_bonus: None,
        reload_on_headshot: None,
    }
}

#[cfg(test)]
mod tests {
    /// The Larkspur Prime is the first weapon that can RUN OUT, and this is
    /// the whole data path end to end: YAML -> spec -> base -> panel -> sim.
    ///
    /// "On Reload From Empty" opens when the RELOAD COMPLETES.
    ///
    /// Not when the magazine runs out (owner, 2026-08-01) — the difference is
    /// the reload itself, 2.5 s of a 17 s window on this weapon. The test that
    /// can see it is the FIRST magazine: nothing has reloaded yet, so Deadly
    /// Efficiency must be worth exactly nothing.
    ///
    /// Before 2026-08-01 it was worth nothing for the whole run: `on_reload`
    /// granting damage fell through to a `CondBuff`, which contributes only
    /// under AssumedMax — so the panel showed +220% and the sim showed none.
    #[test]
    fn a_reload_from_empty_buff_is_worth_nothing_until_the_first_reload() {
        use crate::dummy::{monte_carlo, DummyParams};
        use crate::loadout::{resolve, StackPolicy, WeaponBase};

        let base = WeaponBase::from_data("larkspur_prime", true, &[]);
        let mods = crate::mods_data::pool_for_weapon("larkspur_prime");
        let de = mods.iter().find(|m| m.id == "primed_deadly_efficiency").expect("archgun pool");

        // 200 beam ticks at 12/s is 16.7 s of firing before the magazine is
        // empty (100 rounds at 0.5 each), so 10 s cannot have reloaded.
        let run = |with: bool, secs: f64| {
            let refs: Vec<&crate::loadout::ModDef> = if with { vec![de] } else { Vec::new() };
            let panel = resolve(&base, &refs, StackPolicy::Emergent);
            let mut p = DummyParams::from_panel(&panel, &crate::arena::Arena::training(secs));
            p.arcane = crate::arcanes_data::ArcaneFx::none();
            p.infinite_reserve = true;
            let s = monte_carlo(&p, 1, 7);
            (s.mean_damage, s.median_run.reloads)
        };
        let (bare, r0) = run(false, 10.0);
        let (armed, r1) = run(true, 10.0);
        assert_eq!((r0, r1), (0, 0), "10 s cannot reach a reload");
        assert!(
            (armed - bare).abs() < 1e-6,
            "no reload has completed, so the buff cannot be up: {bare} vs {armed}"
        );

        // Over a run that DOES reload, it is worth a great deal — the check
        // that the window opens at all, so the assertion above is not passing
        // because the mod does nothing anywhere.
        let (long_bare, _) = run(false, 120.0);
        let (long_armed, rl) = run(true, 120.0);
        assert!(rl >= 1, "120 s reloads");
        assert!(
            long_armed > long_bare * 2.0,
            "+220% base damage at high uptime: {long_bare} vs {long_armed}"
        );
    }

    /// AMMO EFFICIENCY REACHES A WEAPON BUILT FROM ITS PANEL.
    ///
    /// `DummyParams::from_panel` hardcoded `ammo_efficiency_applies: false`,
    /// so every weapon the API simulates had ammo efficiency switched off
    /// entirely — Primary Crux's +60% did nothing at all. Nothing caught it
    /// because every test of the mechanic builds `DummyParams` by hand, where
    /// the field defaults to `true`; this one goes through the panel, which is
    /// the path a request takes (2026-08-01).
    ///
    /// The flag is a real distinction, not a nuisance: a CHARGE-BACKED form is
    /// "not affected by Ammo Efficiency" (wiki, Torid Incarnon). So the test
    /// has two halves — it must reach the Larkspur and it must NOT reach the
    /// Torid's Incarnon form.
    #[test]
    fn ammo_efficiency_survives_the_trip_through_the_panel() {
        use crate::dummy::DummyParams;
        use crate::loadout::{resolve, StackPolicy, WeaponBase};

        let of = |id: &str| {
            let base = WeaponBase::from_data(id, true, &[]);
            let panel = resolve(&base, &[], StackPolicy::Emergent);
            DummyParams::from_panel(&panel, &crate::arena::Arena::training(60.0))
            .ammo_efficiency_applies
        };
        assert!(of("larkspur_prime"), "an ordinary weapon spends real ammo");
        assert!(of("verglas_prime"), "so does a sentinel weapon");
        assert!(of("cernos_prime"), "and a bow");
        assert!(
            !of("torid_incarnon"),
            "a charge-backed form is outside the ammo economy (wiki)"
        );
    }

    /// EVERYTHING THE WEAPON CAN FIRE IS `magazine + reserve`, and the two
    /// mods move different halves of it (owner, 2026-08-01).
    ///
    /// A magazine mod raises the TOTAL, not just how long between reloads:
    /// the loaded magazine is ammo you have, and nothing draws it out of the
    /// reserve. An ammo-maximum mod raises the reserve alone. Stated because
    /// the opposite is a natural thing to assume — that the magazine is the
    /// first slice of the reserve — and it would make a magazine mod free.
    ///
    /// The Larkspur Prime is the only weapon that can show it, being the only
    /// finite reserve in the roster. Counted in ROUNDS: its primary is a beam
    /// and spends 0.5 per tick, so the tick count is double.
    #[test]
    fn total_ammo_is_the_magazine_plus_the_reserve() {
        use crate::dummy::{monte_carlo, DummyParams};
        use crate::loadout::{resolve, StackPolicy, WeaponBase};

        let base = WeaponBase::from_data("larkspur_prime", true, &[]);
        let pool = crate::mods_data::pool_for_weapon("larkspur_prime");
        let by = |id: &str| pool.iter().find(|m| m.id == id).expect("archgun pool");

        // An hour is far more than any of these can sustain, so what stops the
        // run is always the ammo.
        let rounds = |ids: &[&str]| {
            let refs: Vec<&crate::loadout::ModDef> = ids.iter().map(|i| by(i)).collect();
            let panel = resolve(&base, &refs, StackPolicy::Emergent);
            let mut p = DummyParams::from_panel(&panel, &crate::arena::Arena::training(3600.0));
            p.arcane = crate::arcanes_data::ArcaneFx::none();
            (panel.magazine_size, panel.ammo_reserve, monte_carlo(&p, 1, 11).mean_shots * 0.5)
        };

        let (m, r, fired) = rounds(&[]);
        assert_eq!((m, r), (100.0, 400.0), "the ground column");
        assert!((fired - 500.0).abs() < 1e-9, "magazine + reserve: {fired}");

        // +60% magazine: the RESERVE is untouched and the total still grew.
        let (m, r, fired) = rounds(&["magazine_extension"]);
        assert_eq!((m, r), (160.0, 400.0), "only the magazine moved");
        assert!((fired - 560.0).abs() < 1e-9, "{fired}");

        // +165% ammo maximum: the MAGAZINE is untouched.
        let (m, r, fired) = rounds(&["primed_ammo_chain"]);
        assert_eq!((m, r), (100.0, 1060.0), "only the reserve moved");
        assert!((fired - 1160.0).abs() < 1e-9, "{fired}");

        // Together they simply add: 160 + 1060.
        let (m, r, fired) = rounds(&["magazine_extension", "primed_ammo_chain"]);
        assert_eq!((m, r), (160.0, 1060.0));
        assert!((fired - 1220.0).abs() < 1e-9, "{fired}");
    }

    /// AN ARCH-GUN'S FIRE RATE DOES NOT SHORTEN ITS DRAW.
    ///
    /// The two are separate stats on this weapon class (owner, 2026-08-01),
    /// and the mod cards show the split: Shell Rush is "+50% Charge Rate"
    /// where Automatic Trigger is "+60% Fire Rate", and Archgun Ace grants
    /// "Fire/Charge Rate" — two names one card would not carry if they were
    /// one stat. The wiki's general charge formula does divide the draw by
    /// fire rate; the Arch-Gun is the exception, which is why the fact rides
    /// on the weapon next to `charge_cadence` instead of being assumed.
    ///
    /// The cycle is draw, shot, then an interval of 1/rate — the wiki's own
    /// "Effective Fire Rate = 1 / (Modded Charge Time + 1/Modded Fire Rate)".
    #[test]
    fn an_archgun_charge_answers_to_charge_rate_and_its_interval_to_fire_rate() {
        use crate::loadout::{resolve, ModEffect, StackPolicy, WeaponBase};
        let base = WeaponBase::from_data("larkspur_prime_charged", true, &[]);
        assert!(!base.fire_rate_shortens_draw, "an arch-gun keeps them apart");

        let with = |e: Vec<ModEffect>| {
            let m = crate::loadout::ModDef {
                unmodeled: false,
            out_of_scope: false,
                id: "t",
                name: "t",
                base_drain: 0,
                max_rank: 0,
                polarity: crate::mods::Polarity::Madurai,
                rarity: crate::loadout::Rarity::Common,
                exilus: false,
                family: None,
                requires_weapon: None,
                excludes_weapon: Vec::new(),
                set: None,
                requires: None,
                disables: Vec::new(),
                effects: e,
            };
            let p = resolve(&base, &[&m], StackPolicy::AssumedMax);
            (p.charge_seconds.expect("a charged form draws"), p.fire_rate)
        };
        let (d0, r0) = with(Vec::new());
        assert!((d0 - 0.5).abs() < 1e-9 && (r0 - 2.0).abs() < 1e-9, "{d0} {r0}");

        // Fire rate moves the INTERVAL only.
        let (d1, r1) = with(vec![ModEffect::FireRate(0.60)]);
        assert!((d1 - 0.5).abs() < 1e-9, "the draw is untouched: {d1}");
        assert!((r1 - 3.2).abs() < 1e-9, "2.0 x 1.6: {r1}");

        // Charge rate moves the DRAW only.
        let (d2, r2) = with(vec![ModEffect::ChargeRate(0.50)]);
        assert!((d2 - 0.5 / 1.5).abs() < 1e-9, "0.5 / 1.5: {d2}");
        assert!((r2 - 2.0).abs() < 1e-9, "the rate is untouched: {r2}");

        // Together: 0.3333 draw + 0.3125 interval = 0.6458 s per shot.
        let (d3, r3) = with(vec![ModEffect::FireRate(0.60), ModEffect::ChargeRate(0.50)]);
        assert!((d3 - 0.5 / 1.5).abs() < 1e-9, "{d3}");
        assert!((d3 + 1.0 / r3 - 0.645833333).abs() < 1e-6, "cycle: {}", d3 + 1.0 / r3);
    }

    /// Ground Arch-Gun: 100 in the magazine, 400 behind it, and no way to
    /// resupply (wiki Arch-Gun). 500 ROUNDS and then the weapon is gone —
    /// inside a 120 s engagement, so the clock is not what stops it.
    ///
    /// Rounds, not ticks: the primary is a BEAM and a beam tick costs 0.5
    /// ("Beam Weapons consume 0.5 ammo per trace", and the Larkspur Prime page
    /// repeats it for this weapon), so 500 rounds is 1000 ticks. This read 500
    /// until 2026-08-01, when `ammo_cost` was read at last — the weapon had
    /// been running dry in half the time the wiki gives it.
    #[test]
    fn the_larkspur_runs_out_where_a_primary_would_not() {
        use crate::dummy::{monte_carlo, DummyParams};
        use crate::loadout::{resolve, StackPolicy, WeaponBase};

        let base = WeaponBase::from_data("larkspur_prime", true, &[]);
        assert!((base.ammo_reserve - 400.0).abs() < 1e-9, "the Atmosphere column");
        assert!(base.has_reserve, "400 rounds is a reserve");
        assert!(base.no_resupply, "a ground Arch-Gun cannot be resupplied");

        let panel = resolve(&base, &[], StackPolicy::Emergent);
        let mut p = DummyParams::from_panel(&panel, &crate::arena::Arena::training(120.0));
        p.arcane = crate::arcanes_data::ArcaneFx::none();
        let s = monte_carlo(&p, 1, 3);
        assert!(
            (s.mean_shots - 1000.0).abs() < 1e-9,
            "500 rounds at 0.5 per beam tick, exactly: {}",
            s.mean_shots
        );

        // The clock did not stop it: 120 s at 12 rounds/second is far more.
        assert!(120.0 * panel.fire_rate > 900.0);

        // The alt-fire form draws from the SAME pool — one weapon, one supply.
        let alt = WeaponBase::from_data("larkspur_prime_charged", true, &[]);
        assert!((alt.ammo_reserve - 400.0).abs() < 1e-9);
        assert!(alt.has_reserve && alt.no_resupply);

        // And a Primary with the same shape does NOT run out: the Torid
        // states a 60-round reserve and keeps firing, because ammo pickups
        // exist and the sim does not model them.
        let torid = WeaponBase::from_data("torid", true, &[]);
        let tp = resolve(&torid, &[], StackPolicy::Emergent);
        let mut q = DummyParams::from_panel(&tp, &crate::arena::Arena::training(120.0));
        q.arcane = crate::arcanes_data::ArcaneFx::none();
        assert!(monte_carlo(&q, 1, 3).mean_shots > 60.0, "a Primary is resupplied");
    }

    /// HAVING A RESERVE AND BEING ABLE TO REFILL IT ARE TWO FACTS, and they
    /// were one field until 2026-08-04 — which is why the Infinite-ammo control
    /// was disabled on every weapon but the Arch-Gun (owner: "只有 sentinel 是
    /// 真的无限弹药"). `has_reserve` is derived from `ammo_max` and is what
    /// "truly infinite" means; `no_resupply` is the Arch-Gun's own problem.
    #[test]
    fn a_reserve_and_a_resupply_are_two_different_facts() {
        use crate::loadout::WeaponBase;
        // A Primary HAS a reserve — 60 rounds, the wiki's Ammo Max — and can
        // also refill it. So the setting is the player's to make.
        let torid = WeaponBase::from_data("torid", true, &[]);
        assert!((torid.ammo_reserve - 60.0).abs() < 1e-9, "wiki Ammo Max 60");
        assert!(torid.has_reserve, "60 rounds is a reserve");
        assert!(!torid.no_resupply, "a Primary is resupplied from pickups");

        let laetum = WeaponBase::from_data("laetum", true, &[]);
        assert!((laetum.ammo_reserve - 210.0).abs() < 1e-9);
        assert!(laetum.has_reserve && !laetum.no_resupply);

        // A sentinel weapon states no reserve AT ALL — the one case where
        // infinite is not a stand-in for anything. This is the only shape
        // that leaves the control with nothing to decide.
        let verglas = WeaponBase::from_data("verglas_prime", true, &[]);
        assert!((verglas.ammo_reserve - 0.0).abs() < 1e-9);
        assert!(!verglas.has_reserve);
    }

    /// The scenario's setting stands in for PICKUPS, so it cannot give ammo to
    /// a weapon that can receive none. One rule, on the panel, called by both
    /// the web api and the optimizer.
    #[test]
    fn the_infinite_ammo_setting_cannot_resupply_an_arch_gun() {
        use crate::loadout::{resolve, StackPolicy, WeaponBase};
        let p = |id| resolve(&WeaponBase::from_data(id, true, &[]), &[], StackPolicy::Emergent);

        // Sentinel: infinite either way, nothing to decide.
        assert!(p("verglas_prime").reserve_is_infinite(true));
        assert!(p("verglas_prime").reserve_is_infinite(false));

        // Primary: the setting decides, which is the point.
        assert!(p("torid").reserve_is_infinite(true));
        assert!(!p("torid").reserve_is_infinite(false));

        // Ground Arch-Gun: finite either way — 400 rounds is the engagement.
        assert!(!p("larkspur_prime").reserve_is_infinite(true));
        assert!(!p("larkspur_prime").reserve_is_infinite(false));
    }


    /// THE CYCLE DRAWS FROM THE SAME RESERVE. Both forms are one weapon with
    /// one supply, but every draw inside the cycle was free until 2026-08-04 —
    /// so a finite reserve was ignored on every Incarnon weapon, which is most
    /// of the roster (owner: the Infinite-ammo setting has to be adjustable).
    #[test]
    fn an_incarnon_cycle_runs_dry_like_anything_else() {
        use crate::dummy::{monte_carlo, DummyParams, LockMode};
        use crate::loadout::{resolve, StackPolicy, WeaponBase};
        let arena = crate::arena::Arena::training(300.0);
        let panel = |id| resolve(&WeaponBase::from_data(id, true, &[]), &[], StackPolicy::Emergent);
        let inc = panel("boar_prime_incarnon");
        let base = panel("boar_prime");
        let mk = |infinite| {
            let mut p = DummyParams::incarnon_cycle_from_panels(
                &inc, &base, false, LockMode::Initial(0), &arena);
            p.arcane = crate::arcanes_data::ArcaneFx::none();
            p.infinite_reserve = infinite;
            p
        };
        let free = monte_carlo(&mk(true), 1, 3).mean_shots;
        let dry = monte_carlo(&mk(false), 1, 3).mean_shots;
        assert!(dry < free, "a finite reserve must stop the cycle: {dry} vs {free}");
    }

    use super::*;

    #[test]
    fn loads_the_weapon_roster() {
        assert!(spec("dual_toxocyst").is_some());
        assert!(spec("dual_toxocyst_incarnon").is_some());
        // The roster lists base entries only.
        assert!(roster().all(|s| s.transforms_from.is_none()));
    }

    /// The Torid is the first PRIMARY weapon and the first weapon with a
    /// lingering FIELD, so this pins what the loader must produce for it — every
    /// number cross-checked wiki data module == WFCD.
    #[test]
    fn torid_loads_both_forms_with_its_field_and_direct_hit_gauge() {
        use crate::loadout::{ChargeOn, FieldStacking};
        let b = base_panel("torid", false);
        assert!((b.base_vector.get(DamageType::Toxin) - 100.0).abs() < 1e-9);
        assert!((b.base_crit_chance - 0.15).abs() < 1e-9);
        assert!((b.base_crit_damage - 2.0).abs() < 1e-9);
        assert!((b.base_status_chance - 0.23).abs() < 1e-9);
        assert!((b.base_fire_rate - 1.5).abs() < 1e-9);
        assert!((b.magazine_size - 5.0).abs() < 1e-9);
        assert!((b.base_reload - 1.7).abs() < 1e-9);
        // "Multiplying" in the CO catalog = an INDEPENDENT multiplier, the
        // opposite of the Laetum's "Adding". Both BASE-form rows say so
        // (Main-fire and Toxin AoE Cloud) — but the INCARNON form does not,
        // which is checked below.
        assert_eq!(b.co_behavior, CoBehavior::Independent);
        // The FIELD, with its own stats: note status 25% where the impact is 23%.
        let f = b.lingering.as_ref().expect("torid leaves a cloud");
        assert!((f.base_vector.get(DamageType::Toxin) - 40.0).abs() < 1e-9);
        assert!((f.tick_rate - 1.0).abs() < 1e-9);
        assert!((f.duration_s - 10.0).abs() < 1e-9);
        assert!((f.base_status_chance - 0.25).abs() < 1e-9);
        assert!((f.radius_m - 3.0).abs() < 1e-9);
        // To ZERO at the rim, unlike the Laetum radial's 0.2.
        assert!((f.falloff_reduction - 1.0).abs() < 1e-9);
        assert_eq!(f.stacking, FieldStacking::Stack, "measured (M13)");
        assert!(b.radial.is_none(), "the cloud is a field, not an explosion");

        // The Incarnon form: a continuous beam, ONE attack part (its 2.3 m
        // radius is explicitly not a separate instance), charged by DIRECT hits.
        let i = base_panel("torid_incarnon", false);
        assert!((i.base_vector.get(DamageType::Toxin) - 51.0).abs() < 1e-9);
        assert!((i.base_crit_chance - 0.29).abs() < 1e-9);
        assert!((i.base_crit_damage - 3.1).abs() < 1e-9);
        assert!((i.base_status_chance - 0.39).abs() < 1e-9);
        assert!((i.base_fire_rate - 8.0).abs() < 1e-9, "ticks per second");
        assert!(i.radial.is_none(), "the damage radius is not its own instance");
        assert!(i.lingering.is_none(), "no cloud in Incarnon form");
        let g = i.incarnon.as_ref().expect("torid_incarnon has a gauge");
        assert_eq!(g.charge_on, ChargeOn::DirectHits);
        assert!((g.charges_to_fill - 5.0).abs() < 1e-9);
        assert!((g.max_charges - 170.0).abs() < 1e-9);
        assert!((g.transmute_in - 1.7).abs() < 1e-9, "= the base reload");
        // Charge-backed magazine, so the pseudo-reload supplies the sim's.
        assert!((i.magazine_size - 170.0).abs() < 1e-9);
        assert!((i.base_reload - 2.7).abs() < 1e-9);
        // CO class is per FORM, not per weapon — ✅ measured (user,
        // 2026-07-30). The base form's two catalog rows are "Multiplying"
        // (asserted above); the Incarnon form is ordinary ADDITIVE. This used
        // to be inferred from those rows and the inference was wrong.
        assert_eq!(i.co_behavior, CoBehavior::AdditiveWithBaseDamage);

        // BEAM GEOMETRY — shape, not a damage part. Pinned because it is data
        // now rather than prose, and because one of these values is a decision
        // rather than a citation.
        let bm = i.beam.expect("the incarnon form is a beam");
        assert!((bm.range_m - 37.0).abs() < 1e-9);
        assert!((bm.damage_radius_m - 2.3).abs() < 1e-9);
        assert!(!bm.radius_takes_multishot, "the sphere never takes multishot");
        assert_eq!(bm.chain_hops, 5);
        assert!((bm.chain_range_m - 7.0).abs() < 1e-9);
        assert!((bm.chain_damage_per_hop - 0.75).abs() < 1e-9);
        assert!(!bm.chain_takes_multishot);
        // UNVERIFIED (MEASUREMENTS M15) — a user call on in-game experience,
        // against four pieces of circumstantial evidence and no citation
        // either way. One data line flips it.
        assert!(bm.chain_nodes_have_radius, "user, 2026-07-30: every node spheres");
        // The base form is not a beam.
        assert!(b.beam.is_none());
    }

    /// Every weapon REGISTERS its form, and the registration has to agree with
    /// the entry's own mechanics — a form is a claim about how the weapon is
    /// operated, so a `charge` trigger filed as `base` is a data error, not a
    /// stylistic one. This is the check that keeps the vocabulary honest as
    /// weapons are added.
    #[test]
    fn every_weapon_registers_a_form_that_matches_its_mechanics() {
        for s in all() {
            let kind = s.form_kind(); // panics on a name outside the vocabulary
            let charge_trigger = s.attack.trigger == "charge";
            assert_eq!(
                charge_trigger,
                kind == FormKind::Charged,
                "{}: a charge trigger IS the charged form, and nothing else is",
                s.id
            );
            // A gauge-switched form is the one that carries the gauge economy.
            assert_eq!(
                s.incarnon.is_some(),
                kind.is_gauge_switched(),
                "{}: the gauge and the incarnon form are the same claim",
                s.id
            );
            // The entry reached BY a transform is never the default form.
            assert_eq!(
                s.transforms_from.is_some(),
                kind.is_gauge_switched(),
                "{}: only a transformed-into form has a form to come from",
                s.id
            );
        }
        // A group never registers one kind twice — otherwise a form id could
        // not name a form.
        for s in roster() {
            let forms = forms_of(&s.id);
            let mut kinds: Vec<u8> = forms.iter().map(|f| f.kind as u8).collect();
            kinds.sort_unstable();
            let n = kinds.len();
            kinds.dedup();
            assert_eq!(kinds.len(), n, "{}: duplicate form kind in one group", s.id);
            assert_eq!(
                forms.iter().filter(|f| f.is_default).count(),
                1,
                "{}: exactly one default form",
                s.id
            );
        }
    }

    /// What the registry reads back: one weapon, its forms, default first.
    #[test]
    fn a_weapons_forms_are_its_transform_group() {
        // Two forms, and only the Incarnon one is transformed INTO — which is
        // what decides whether there is a cycle to simulate.
        let torid = forms_of("torid");
        assert_eq!(torid.len(), 2);
        assert_eq!(torid[0].kind, FormKind::Base);
        assert!(torid[0].is_default && torid[0].weapon_id == "torid");
        assert_eq!(torid[1].kind, FormKind::Incarnon);
        assert!(!torid[1].is_default && torid[1].weapon_id == "torid_incarnon");
        assert!(has_gauge_switched_form("torid"));
        // Asking from the non-default entry gives the SAME group.
        assert_eq!(forms_of("torid_incarnon").len(), 2);

        // One form is a registration, not an absence — and a beam weapon has
        // nothing to transform into.
        let verglas = forms_of("verglas_prime");
        assert_eq!(verglas.len(), 1);
        assert_eq!(verglas[0].kind, FormKind::Base);
        assert!(verglas[0].is_default);
        assert!(!has_gauge_switched_form("verglas_prime"));

        // TWO forms with NO transformation between them: a bow is drawn or
        // tapped, and switching costs nothing but a shorter press. So it has
        // no cycle to simulate even though it has more than one form — which
        // is the whole reason "does it have two forms" and "does it transform"
        // are separate questions.
        let bow = forms_of("cernos_prime");
        assert_eq!(bow.len(), 2);
        assert_eq!(bow[0].kind, FormKind::Charged, "the arsenal's form comes first");
        assert!(bow[0].is_default && bow[0].weapon_id == "cernos_prime");
        assert_eq!(bow[1].kind, FormKind::Base, "the tapped shot is the uncharged form");
        assert!(!bow[1].is_default && bow[1].weapon_id == "cernos_prime_uncharged");
        assert!(!has_gauge_switched_form("cernos_prime"));
        // Asking from either entry gives the same weapon's forms.
        assert_eq!(forms_of("cernos_prime_uncharged").len(), 2);

        // Wire ids are stable: they are what a saved preset stores.
        assert_eq!(FormKind::Base.id(), "base");
        assert_eq!(FormKind::Incarnon.id(), "incarnon");
        assert_eq!(FormKind::Charged.id(), "charged");
    }

    /// The Cernos Prime is the first CHARGE-trigger weapon, so this pins the
    /// three things a bow brings that no other roster entry has: a draw that
    /// replaces the fire-rate cadence, an innate headshot bonus, and a CO term
    /// computed off the UNCHARGED base. Every number is wiki data module ==
    /// WFCD (joined on internal name), except `co_base_fraction`, which is the
    /// CO catalog's own column.
    #[test]
    fn cernos_prime_loads_as_a_charged_bow() {
        let b = base_panel("cernos_prime", false);
        // PER ARROW — 3 x 184 = the 552 the page quotes for the whole shot.
        assert!((b.base_vector.get(DamageType::Impact) - 165.6).abs() < 1e-9);
        assert!((b.base_vector.get(DamageType::Puncture) - 9.2).abs() < 1e-9);
        assert!((b.base_vector.get(DamageType::Slash) - 9.2).abs() < 1e-9);
        assert!((b.base_vector.total() - 184.0).abs() < 1e-9);
        assert!((b.base_multishot - 3.0).abs() < 1e-9, "innate 3 arrows");
        assert!((b.base_crit_chance - 0.35).abs() < 1e-9);
        assert!((b.base_crit_damage - 2.0).abs() < 1e-9);
        assert!((b.base_status_chance - 0.30).abs() < 1e-9);
        assert!((b.magazine_size - 1.0).abs() < 1e-9, "one nocked arrow");
        assert!((b.base_reload - 0.65).abs() < 1e-9);
        // The listed stat stays the listed stat; the DRAW is what paces it.
        assert!((b.base_fire_rate - 1.0).abs() < 1e-9);
        assert_eq!(b.charge_seconds, Some(0.5));
        // "(x2 for Bows)" — the clause on every fire-rate mod card.
        assert!((b.fire_rate_mod_multiplier - 2.0).abs() < 1e-9);
        // ExtraHeadshotDmg 0.5 on both attacks ("Deals 50% bonus damage on
        // headshots"), into the additive headshot bracket.
        assert!((b.headshot_damage_bonus - 0.5).abs() < 1e-9);
        // CO catalog: 552 | 276 | 50% | Adding. The 50% is the charge
        // multiplier sitting OUTSIDE the additive bracket, not an evolution
        // exclusion — this weapon has no evolutions.
        assert_eq!(b.co_behavior, CoBehavior::AdditiveWithBaseDamage);
        assert!((b.co_base_fraction - 0.5).abs() < 1e-9);
        // A bow is neither a beam nor an AoE weapon.
        assert!(!b.continuous);
        assert!(b.radial.is_none() && b.lingering.is_none() && b.beam.is_none());
        // Charge is its own trigger family: a mod gated on `semi_auto`
        // (Semi-Rifle Cannonade) is inert on it, which is the in-game rule. The
        // CLASS is still there, and it is what Longbow Sharpshot needs.
        assert_eq!(b.traits, &["bow"]);
    }

    /// The TAPPED shot: same weapon, half the damage per arrow, and a cadence
    /// of pure nock. Wiki Fire Rate gives bows their own effective-fire-rate
    /// formula — `1 / (charge + reload)`, no fire-rate term — so a shot with
    /// no draw to pay is paced by the 0.65 s nock alone.
    #[test]
    fn the_tapped_bow_shot_is_the_same_weapon_at_half_damage() {
        let charged = base_panel("cernos_prime", false);
        let tapped = base_panel("cernos_prime_uncharged", false);
        // "bows have innate 2x damage multiplier when fully charged" — the CO
        // catalog's words, and the two entries hold both sides of it.
        assert!((tapped.base_vector.total() * 2.0 - charged.base_vector.total()).abs() < 1e-9);
        assert!((tapped.base_vector.total() - 92.0).abs() < 1e-9);
        // The draw buys damage, speed and punch through — not crit or status.
        assert_eq!(tapped.base_crit_chance, charged.base_crit_chance);
        assert_eq!(tapped.base_status_chance, charged.base_status_chance);
        assert_eq!(tapped.base_multishot, charged.base_multishot);
        // The innate headshot bonus is the WEAPON's: both attacks carry
        // ExtraHeadshotDmg 0.5 in the module.
        assert!((tapped.headshot_damage_bonus - 0.5).abs() < 1e-9);
        // No charge multiplier to leave out, so CO computes on the full base
        // here — the catalog lists no row for this attack, and absence there
        // is a positive statement.
        assert!((tapped.co_base_fraction - 1.0).abs() < 1e-9);
        assert!((charged.co_base_fraction - 0.5).abs() < 1e-9);
        // ZERO draw is a cadence statement, not a missing value: 1 / 0.65 =
        // 1.54 shots/s against the charged form's 1 / 1.15 = 0.87.
        assert_eq!(tapped.charge_seconds, Some(0.0));
        assert!(tapped.fire_rate_mod_multiplier > 1.9, "still a bow");
    }

    /// The draw, not the fire-rate stat, is what a fire-rate mod shortens —
    /// and on a bow it is shortened by DOUBLE the printed bonus.
    #[test]
    fn fire_rate_mods_halve_a_bow_charge_at_double_value() {
        use crate::loadout::{resolve, ModEffect, StackPolicy, WeaponBase};
        let base = WeaponBase::from_data("cernos_prime", false, &[]);
        let bare = resolve(&base, &[], StackPolicy::AssumedMax);
        assert_eq!(bare.charge_seconds, Some(0.5));
        assert!((bare.fire_rate - 1.0).abs() < 1e-9);

        // Shred: +30% on the card, +60% here.
        let shred = crate::mods_data::class_pool("rifle")
            .into_iter()
            .find(|m| m.id == "shred")
            .expect("shred is in the rifle pool");
        assert!(shred.effects.iter().any(|e| matches!(e, ModEffect::FireRate(v) if (v - 0.30).abs() < 1e-9)));
        let p = resolve(&base, &[&shred], StackPolicy::AssumedMax);
        assert!((p.fire_rate - 1.6).abs() < 1e-9, "1.0 x (1 + 2 x 0.30)");
        // 0.5 / 1.6 = 0.3125 s of draw — the reciprocal of the same bucket.
        assert!((p.charge_seconds.expect("still a bow") - 0.3125).abs() < 1e-9);

        // A non-bow spends the bucket the ordinary way: one x, on the rate.
        let torid = WeaponBase::from_data("torid", false, &[]);
        let t = resolve(&torid, &[&shred], StackPolicy::AssumedMax);
        assert!((t.fire_rate - 1.5 * 1.30).abs() < 1e-9);
        assert!(t.charge_seconds.is_none());
    }

    /// Firestorm reaches the beam's sphere — "The 2.3 meter damage radius from
    /// the point of impact CAN benefit from Firestorm (Primed)." It buys no
    /// single-target damage (a struck target is hit once), which is why the
    /// panel states the radius rather than a DPS delta.
    #[test]
    fn blast_range_mods_enlarge_the_beam_sphere() {
        use crate::loadout::{resolve, StackPolicy, WeaponBase};
        let base = WeaponBase::from_data("torid_incarnon", false, &[]);
        let bare = resolve(&base, &[], StackPolicy::AssumedMax);
        assert!((bare.beam.expect("beam").damage_radius_m - 2.3).abs() < 1e-9);

        let pool = crate::mods_data::class_pool("rifle");
        let pf: Vec<&crate::loadout::ModDef> =
            pool.iter().filter(|m| m.id == "primed_firestorm").collect();
        let modded = resolve(&base, &pf, StackPolicy::AssumedMax);
        // +44% Blast Range at max rank.
        assert!(
            (modded.beam.expect("beam").damage_radius_m - 2.3 * 1.44).abs() < 1e-9,
            "expected 3.312 m, got {}",
            modded.beam.unwrap().damage_radius_m
        );
    }

    /// The CO term uses the FULL base including a perk's flat damage. The CO
    /// catalog "lists only discrepant attacks", so the ONE exclusion in the
    /// roster is the one it names: Dual Toxocyst's Evolution II **Perk 1**
    /// (Carnage Reign). Perk 2 raises base damage too and is absent from the
    /// table, so it feeds CO in full — which is why the flag lives on the perk
    /// rather than on the weapon or on the Adding behaviour class.
    #[test]
    fn only_dual_toxocyst_excludes_its_evolution_damage_from_the_co_term() {
        use crate::loadout::WeaponBase;
        // Torid + Final Fusillade (+51 on a 140 base): the base scales, the CO
        // fraction does NOT.
        let bare = WeaponBase::from_data("torid", false, &[]);
        let evolved = WeaponBase::from_data("torid", false, &["torid_final_fusillade"]);
        assert!(
            (evolved.base_vector.total() - (bare.base_vector.total() + 51.0)).abs() < 1e-9,
            "the evolution still scales the base"
        );
        assert!(
            (evolved.co_base_fraction - 1.0).abs() < 1e-9,
            "the Torid's catalog rows stay 100%, got {}",
            evolved.co_base_fraction
        );
        // Plentiful Mayhem (+31) likewise.
        let pm = WeaponBase::from_data("torid", false, &["torid_plentiful_mayhem"]);
        assert!((pm.co_base_fraction - 1.0).abs() < 1e-9);

        // Dual Toxocyst + Carnage Reign (Perk 1, +60 on a 75 base) = the
        // catalog's "100% or 56%" row: a +100% CO adds 75, never 135.
        for form in ["dual_toxocyst", "dual_toxocyst_incarnon"] {
            let dt = WeaponBase::from_data(form, false, &["dual_toxocyst_carnage_reign"]);
            assert!(
                (dt.co_base_fraction - 75.0 / 135.0).abs() < 1e-9,
                "{form}: expected 75/135 = 0.5556, got {}",
                dt.co_base_fraction
            );
        }
        // …and Perk 2 does NOT. Fevered Frenzy also raises base damage (+50),
        // so keying the exclusion off the WEAPON — or off its Adding CO class —
        // would dock it to 75/125 = 0.6. The catalog does not list it, and the
        // table lists only discrepancies, so it feeds the CO term in full.
        let perk2 =
            WeaponBase::from_data("dual_toxocyst", false, &["dual_toxocyst_fevered_frenzy"]);
        assert!(
            (perk2.base_vector.total() - 125.0).abs() < 1e-9,
            "the +50 still reaches the base, got {}",
            perk2.base_vector.total()
        );
        assert!(
            (perk2.co_base_fraction - 1.0).abs() < 1e-9,
            "Perk 2 is not a listed discrepancy; expected 1.0, got {}",
            perk2.co_base_fraction
        );
    }

    /// The roster is data-driven: dropping in `data/weapons/primary/` publishes
    /// a primary weapon with no code change, and its mod pools and arcane slot
    /// follow from `mod_pools` and `slot`.
    /// A weapon's pool is the union of the pools it draws. The Torid sees the
    /// primary-wide mods AND the rifle class pool; Verglas Prime, a sentinel
    /// weapon, sees only the rifle pool — it is not a Primary weapon, so it
    /// does not claim mods DE tags PRIMARY.
    /// A compat tag is not the whole restriction. Sinister Reach and
    /// Combustion Beam are tagged PRIMARY and still cannot go on the Torid
    /// (user, 2026-07-31) — they need a CONTINUOUS weapon, and the Torid is a
    /// semi-auto grenade launcher. Its INCARNON form is a beam and that
    /// changes nothing: modding is decided on the base form.
    #[test]
    fn a_beam_only_mod_needs_a_continuous_weapon_to_be_offered_at_all() {
        use crate::mods_data::{pool_for_weapon, pool_union};
        let beam_only = ["sinister_reach", "combustion_beam"];
        let torid = pool_for_weapon("torid");
        for id in beam_only {
            assert!(
                pool_union(&["primary".to_string()]).iter().any(|m| m.id == id),
                "{id} is in the primary pool"
            );
            assert!(
                !torid.iter().any(|m| m.id == id),
                "{id} must not be offered on the Torid"
            );
        }
        // The rest of the primary pool still reaches it.
        assert!(torid.iter().any(|m| m.id == "hunter_munitions"));
        assert!(torid.iter().any(|m| m.id == "vigilante_armaments"));
        // Verglas Prime IS continuous (wiki: Continuous Weapons category), so
        // the gate would pass — it just draws the rifle pool, where these are
        // not, which is a separate question this test does not decide.
        assert_eq!(
            spec("verglas_prime").unwrap().attack.trigger,
            "held",
            "continuous, per the wiki category"
        );
    }

    #[test]
    fn a_weapons_pool_is_the_union_of_the_pools_it_draws() {
        use crate::mods_data::{class_pool, pool_union};
        let torid = pool_union(&spec("torid").unwrap().mod_pools);
        let verglas = pool_union(&spec("verglas_prime").unwrap().mod_pools);
        let rifle = class_pool("rifle").len();
        let primary = class_pool("primary").len();
        assert!(primary > 0, "data/mods/primary/ exists");
        assert_eq!(torid.len(), rifle + primary, "union of both, no overlap");
        assert_eq!(verglas.len(), rifle, "sentinel: rifle only");
        assert!(torid.iter().any(|m| m.id == "vigilante_armaments"));
        assert!(!verglas.iter().any(|m| m.id == "vigilante_armaments"));
        // A mod in two pools would still appear once.
        let mut ids: Vec<&str> = torid.iter().map(|m| m.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "the union deduplicates by id");
    }

    #[test]
    fn the_primary_slot_needed_no_code() {
        let t = spec("torid").expect("torid");
        assert_eq!(t.slot, "primary");
        // A UNION, widest first: primary-wide mods AND the rifle class pool.
        assert_eq!(t.mod_pools, ["primary", "rifle"]);
        assert!(roster().any(|s| s.id == "torid"), "selectable");
        assert!(
            !roster().any(|s| s.id == "torid_incarnon"),
            "a form is not its own roster row"
        );
        assert!(
            !crate::mods_data::class_pool("rifle").is_empty(),
            "the rifle pool has to exist for it to be equippable"
        );
    }

    #[test]
    fn base_panels_match_the_wiki_values() {
        let b = base_panel("dual_toxocyst", true);
        assert!((b.base_vector.get(DamageType::Puncture) - 60.0).abs() < 1e-9);
        assert!((b.base_crit_chance - 0.05).abs() < 1e-9);
        assert!((b.magazine_size - 12.0).abs() < 1e-9);
        assert_eq!(b.injected_elements, vec![(DamageType::Toxin, 1.0)]);
        // TRIGGER *AND* CLASS (2026-08-05). The class half is what makes a
        // `requires: dual_pistols` gate satisfiable at all — without it Akimbo
        // Slip Shot equipped and did nothing, on every dual pistol.
        assert_eq!(b.traits, &["semi_auto", "dual_pistols"]);
        assert!(b.incarnon.is_none());

        let i = base_panel("dual_toxocyst_incarnon", false);
        assert!((i.base_crit_damage - 3.0).abs() < 1e-9);
        assert!((i.magazine_size - 270.0).abs() < 1e-9);
        assert!((i.base_reload - 3.35).abs() < 1e-9);
        let inc = i.incarnon.expect("incarnon block");
        assert!((inc.max_charges - 270.0).abs() < 1e-9);
        assert!((inc.transmute_in - 2.35).abs() < 1e-9);
        assert!((inc.transmute_out - 1.0).abs() < 1e-9);
        assert!(i.injected_elements.is_empty());
        // The TRIGGER comes from the transform group's BASE entry; the CLASS
        // is the form's own, and both halves of a pair share it anyway.
        assert_eq!(i.traits, &["semi_auto", "dual_pistols"]);
    }

    /// data/README.md's promotion rule, enforced: perk ids are GLOBALLY
    /// unique. A table perk may be referenced by many items; an inline perk
    /// may exist in exactly ONE item and must not shadow a table id — two
    /// carriers means it should have been promoted to data/perks/.
    #[test]
    fn perk_ids_are_globally_unique_across_table_and_inlines() {
        use std::collections::HashMap;
        let mut home: HashMap<&str, String> = HashMap::new();
        for p in perks() {
            let prev = home.insert(&p.id, format!("data/perks/{}.yaml", p.id));
            assert!(prev.is_none(), "duplicate table perk id: {}", p.id);
        }
        for w in all() {
            for pr in &w.perks {
                if let PerkRef::Inline(p) = pr {
                    if let Some(other) = home.get(p.id.as_str()) {
                        panic!(
                            "inline perk '{}' in weapon '{}' collides with {} — \
                             promote it to data/perks/ and reference the id",
                            p.id, w.id, other
                        );
                    }
                    home.insert(&p.id, format!("inline in weapon '{}'", w.id));
                }
            }
        }
        // Every bare-id reference must resolve (no dangling perk refs —
        // caught here at test time instead of a runtime panic mid-sim).
        for w in all() {
            for pr in &w.perks {
                if let PerkRef::Id(id) = pr {
                    assert!(
                        perk(id).is_some(),
                        "weapon '{}' references unknown perk '{}'",
                        w.id, id
                    );
                }
            }
        }
    }

    #[test]
    fn perks_accept_both_reference_and_inline_forms() {
        let yaml = r#"
id: test_gun
name: Test Gun
slot: secondary
class: pistols
form: base
magazine: 10
reload_seconds: 1.0
attack: { trigger: auto, fire_rate: 5.0, crit_chance: 0.1, crit_multiplier: 2.0, status_chance: 0.1, damage: { toxin: 10.0 } }
perks:
  - frenzy
  - id: one_off
    grants: { injected_element: { type: heat, amount: 0.5 } }
"#;
        let s: WeaponSpec = serde_norway::from_str(yaml).unwrap();
        assert_eq!(s.perks[0].id(), "frenzy");
        assert!(s.perks[0].resolve().grants.is_some()); // table lookup works
        assert_eq!(s.perks[1].id(), "one_off");
        let g = s.perks[1].resolve().grants.as_ref().unwrap();
        let inj = g.injected_element.as_ref().unwrap();
        assert_eq!(inj.element, "heat");
        // An inline definition registers in the perk namespace: a bare-id
        // reference from ANOTHER entry finds it.
        let found = inline_perk_in("one_off", std::iter::once(&s)).expect("inline registered");
        assert!(found.grants.is_some());
        assert!(inline_perk_in("nope", std::iter::once(&s)).is_none());
    }

    #[test]
    fn innate_slots_come_from_the_yaml_polarities() {
        let s = innate_slots("dual_toxocyst");
        assert_eq!(s[0], Some(Polarity::Madurai));
        assert_eq!(s[1], Some(Polarity::Naramon));
        assert_eq!(s[2], None);
    }
}

#[cfg(test)]
mod laetum_tests {
    use super::*;

    #[test]
    fn laetum_incarnon_carries_its_radial_part() {
        let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
        let r = b.radial.as_ref().expect("laetum_incarnon declares a radial part");
        assert_eq!(r.base_vector.total(), 300.0, "300 Radiation");
        assert_eq!(r.radius_m, 2.0);
        assert_eq!(r.falloff_reduction, 0.2);
        // The direct part is pure Impact 100.
        assert_eq!(b.base_vector.total(), 100.0);
    }

    #[test]
    fn the_sim_actually_applies_the_radial() {
        use crate::dummy::{monte_carlo, DummyParams};
        let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
        let p = crate::loadout::resolve(&b, &[], crate::loadout::StackPolicy::AssumedMax);
        let parts = vec![crate::dummy::BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            crit_bonus: false,
        }];
        let params =
            DummyParams::from_panel(&p, &crate::arena::Arena { body_parts: parts, ..crate::arena::Arena::training(10.0) });
        assert!(params.radial.is_some(), "params carry the radial");
        let s = monte_carlo(&params, 30, 7);
        assert!(
            s.source_damage.radial > 0.0,
            "the radial must land damage, got {:?}",
            s.source_damage
        );
        // 300 Radiation vs 100 Impact: the radial dominates.
        assert!(
            s.source_damage.radial > s.source_damage.direct,
            "radial {} should exceed direct {}",
            s.source_damage.radial,
            s.source_damage.direct
        );
    }

    /// Two-stage damage: the direct hit lands first, then the explosion,
    /// both on the SAME enemy (user, 2026-07-29). With Laetum's 100 Impact
    /// direct and 300 Radiation radial, and no body-part multiplier on the
    /// explosion, a body-only engagement must settle at radial ~ 3x direct.
    #[test]
    fn direct_then_radial_lands_at_the_declared_ratio() {
        use crate::dummy::{monte_carlo, DummyParams};
        let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
        let p = crate::loadout::resolve(&b, &[], crate::loadout::StackPolicy::AssumedMax);
        let specs = crate::enemy_data::all();
        let spec = specs.iter().find(|e| e.id == "thrax_centurion").unwrap();
        let target = spec
            .target_params(1, false, false, crate::dummy::TargetMode::InstantRespawn)
            .unwrap();
        let parts = vec![crate::dummy::BodyPart {
            name: "body".into(), aim_weight: 1.0, multiplier: 1.0,
            is_head: false, crit_bonus: false,
        }];
        let params = DummyParams::from_panel(&p, &crate::arena::Arena { target, body_parts: parts, ..crate::arena::Arena::training(30.0) });
        let s = monte_carlo(&params, 40, 3);
        let d = s.source_damage.direct;
        let r = s.source_damage.radial;
        let ratio = r / d;
        assert!(
            (ratio - 3.0).abs() < 0.25,
            "radial/direct should be ~3 (300 vs 100), got {ratio:.2} (direct {d:.0}, radial {r:.0})"
        );
    }

    /// The cycle economy is DATA, not a hardcoded weapon: Laetum fills in
    /// 12 weakpoint hits and both transitions cost its 2.0 s reload, while
    /// Incarnon Efficiency (+50% charge) drops the fill to 8 hits.
    #[test]
    fn the_cycle_reads_its_economy_from_the_weapon_data() {
        let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
        let f = b.incarnon.expect("incarnon economy");
        assert_eq!(f.charges_to_fill, 12.0);
        assert_eq!(f.max_charges, 216.0);
        assert_eq!(f.transmute_in, 2.0);
        // Reverts are a uniform 1 s across every weapon until measured.
        assert_eq!(f.transmute_out, 1.0);
        assert_eq!(f.charge_rate, 0.0);

        let eff = WeaponBase::from_data("laetum_incarnon", true, &["laetum_incarnon_efficiency"]);
        let g = eff.incarnon.expect("incarnon economy");
        assert_eq!(g.charge_rate, 0.5);
        // 12 / 1.5 = 8 hits (wiki).
        assert_eq!((g.charges_to_fill / (1.0 + g.charge_rate)).ceil() as u32, 8);

        // Dual Toxocyst keeps its own numbers.
        let dt = WeaponBase::from_data("dual_toxocyst_incarnon", true, &[]);
        let d = dt.incarnon.expect("incarnon economy");
        assert_eq!(d.charges_to_fill, 9.0);
        assert_eq!(d.transmute_in, 2.35);
    }

    /// Overwhelming Attrition earns its stacks in-run: a plain hit (no
    /// crit, no status) grants one, the buff multiplies later instances,
    /// and a timeout drops ONE stack rather than the whole buff.
    #[test]
    fn overwhelming_attrition_earns_and_pays_out() {
        use crate::dummy::{monte_carlo, DummyParams};
        let parts = vec![crate::dummy::BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            crit_bonus: false,
        }];
        let run = |evos: &[&str]| {
            let b = WeaponBase::from_data("laetum_incarnon", true, evos);
            let p = crate::loadout::resolve(&b, &[], crate::loadout::StackPolicy::AssumedMax);
            let params =
                DummyParams::from_panel(&p, &crate::arena::Arena { body_parts: parts.clone(), ..crate::arena::Arena::training(20.0) });
            (params.plain_hit_bonus.is_some(), monte_carlo(&params, 40, 11).mean_effective_damage)
        };
        let (has_none, without) = run(&[]);
        let (has_buff, with) = run(&["laetum_overwhelming_attrition"]);
        assert!(!has_none && has_buff, "the evolution must carry the buff");
        assert!(
            with > without * 1.5,
            "the earned stacks must show up: {without:.0} -> {with:.0}"
        );
    }

    /// Lethal Rearmament used to load INERT. It is an on-headshot stacking
    /// reload-speed buff, and reload speed also scales the Incarnon
    /// transmute animations — so on a weapon whose whole cycle is
    /// reload-bound it must buy back real time.
    #[test]
    fn lethal_rearmament_shortens_the_cycle_not_just_reloads() {
        use crate::dummy::{monte_carlo, DummyParams};
        // 100% headshots so the trigger fires on every landed pellet.
        let parts = vec![crate::dummy::BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: false,
        }];
        let run = |evos: &[&str]| {
            let b = WeaponBase::from_data("laetum_incarnon", true, evos);
            let p = crate::loadout::resolve(&b, &[], crate::loadout::StackPolicy::Emergent);
            let params =
                DummyParams::from_panel(&p, &crate::arena::Arena { body_parts: parts.clone(), ..crate::arena::Arena::training(60.0) });
            let m = monte_carlo(&params, 24, 7);
            (params.reload_on_headshot.is_some(), m.mean_effective_damage)
        };
        let (has_none, without) = run(&["laetum_evo1_incarnon_form"]);
        let (has_buff, with) = run(&["laetum_evo1_incarnon_form", "laetum_lethal_rearmament"]);
        assert!(!has_none, "no evolution, no buff");
        assert!(has_buff, "the evolution must carry the buff (it loaded INERT before)");
        assert!(
            with > without,
            "less time reloading and transmuting = more damage in the same 60 s:              {without:.0} -> {with:.0}"
        );
    }

    /// M10 (in-game, 2026-07-30): a reload-speed buff is live in BOTH
    /// forms; it does NOT touch the gauge, but it DOES shorten transmute
    /// in and out. The observable consequence in a fixed engagement is
    /// that more cycles fit — the gauge requirement staying put.
    #[test]
    fn a_reload_buff_shortens_the_transmutes_but_never_the_gauge() {
        use crate::dummy::{run_once, DummyParams};
        use crate::rng::Rng;
        let parts = vec![crate::dummy::BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: false,
        }];
        let params = |evos: &[&str], pin: bool| {
            let inc = WeaponBase::from_data("laetum_incarnon", true, evos);
            let base = WeaponBase::from_data("laetum", true, evos);
            let pol = crate::loadout::StackPolicy::Emergent;
            let pi = crate::loadout::resolve(&inc, &[], pol);
            let pb = crate::loadout::resolve(&base, &[], pol);
            let mut d = DummyParams::incarnon_cycle_from_panels(
                &pi,
                &pb,
                false,
                crate::dummy::LockMode::Initial(0),
                &crate::arena::Arena { body_parts: parts.clone(), ..crate::arena::Arena::training(300.0) },
            );
            if !pin {
                if let Some(b) = d.reload_on_headshot.as_mut() {
                    b.initial_stacks = 0;
                }
            } else if let Some(b) = d.reload_on_headshot.as_mut() {
                // Full AND never expiring — the two knobs are separate,
                // and this test wants both held for the whole run.
                b.initial_stacks = b.max_stacks;
                b.duration = crate::loadout::NO_TIMEOUT;
            }
            d
        };
        let cycle_charges = |d: &DummyParams| {
            d.cycle.as_ref().expect("the incarnon cycle").charges_to_fill
        };

        let off = params(&["laetum_evo1_incarnon_form"], false);
        let on = params(
            &["laetum_evo1_incarnon_form", "laetum_lethal_rearmament"],
            true,
        );
        assert_eq!(
            cycle_charges(&off),
            cycle_charges(&on),
            "reload speed must NOT shorten gauge building (M10)"
        );

        // Less downtime in a fixed engagement = more shots fired. (Transform
        // COUNT is gauge-bound, not animation-bound, so it barely moves.)
        let s_off = run_once(&off, &mut Rng::new(5)).shots;
        let s_on = run_once(&on, &mut Rng::new(5)).shots;
        assert!(
            s_on > s_off,
            "shorter transmutes = more shots in the same 300 s: {s_off} -> {s_on}"
        );
    }

    /// The two tier-5 Attritions sit in DIFFERENT brackets, and the wiki
    /// says so explicitly: Overwhelming is "additive to base damage bonuses
    /// such as Hornet Strike", Devouring is "multiplicative" to them. The
    /// observable difference is DILUTION — an additive bonus loses relative
    /// value as the base-damage bucket grows, a multiplicative one does not.
    #[test]
    fn overwhelming_attrition_is_diluted_by_base_damage_mods() {
        use crate::dummy::{monte_carlo, DummyParams};
        let parts = vec![crate::dummy::BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            crit_bonus: false,
        }];
        let pool = crate::mods_data::pistol_pool();
        let hornet: Vec<&crate::loadout::ModDef> =
            pool.iter().filter(|m| m.id == "hornet_strike").collect();
        let gain = |evos: &[&str], mods: &[&crate::loadout::ModDef]| {
            let run = |e: &[&str]| {
                let b = WeaponBase::from_data("laetum_incarnon", true, e);
                let p = crate::loadout::resolve(&b, mods, crate::loadout::StackPolicy::AssumedMax);
                let params =
                    DummyParams::from_panel(&p, &crate::arena::Arena { body_parts: parts.clone(), ..crate::arena::Arena::training(20.0) });
                monte_carlo(&params, 60, 5).mean_effective_damage
            };
            run(evos) / run(&[])
        };
        let bare = gain(&["laetum_overwhelming_attrition"], &[]);
        let modded = gain(&["laetum_overwhelming_attrition"], &hornet);
        assert!(
            bare > modded * 1.5,
            "an ADDITIVE bonus must lose relative value once Hornet Strike              fills the same bucket: bare {bare:.2}x vs modded {modded:.2}x"
        );
        // …and the MULTIPLICATIVE sibling must NOT be diluted at all.
        let d_bare = gain(&["laetum_devouring_attrition"], &[]);
        let d_modded = gain(&["laetum_devouring_attrition"], &hornet);
        let drift = (d_bare / d_modded - 1.0).abs();
        assert!(
            drift < 0.15,
            "a MULTIPLICATIVE bonus keeps its relative value: bare {d_bare:.2}x              vs modded {d_modded:.2}x (drift {drift:.2})"
        );
    }

    /// Rapid Wrath's +20% fire rate joins the ORDINARY fire-rate bucket —
    /// the same additive one the mods feed (user, 2026-07-29). Additive with
    /// Gunslinger's +72%: 6.67 x (1 + 0.72 + 0.20) = 12.81, NOT the
    /// multiplicative 6.67 x 1.72 x 1.20 = 13.77.
    #[test]
    fn rapid_wrath_is_additive_with_fire_rate_mods() {
        let pool = crate::mods_data::pistol_pool();
        let gunslinger: Vec<&crate::loadout::ModDef> =
            pool.iter().filter(|m| m.id == "gunslinger").collect();
        let fr = |evos: &[&str], mods: &[&crate::loadout::ModDef]| {
            let b = WeaponBase::from_data("laetum_incarnon", true, evos);
            crate::loadout::resolve(&b, mods, crate::loadout::StackPolicy::AssumedMax).fire_rate
        };
        let base = fr(&[], &[]);
        assert!((base - 6.67).abs() < 1e-9, "base fire rate {base}");
        assert!((fr(&["laetum_rapid_wrath"], &[]) - 6.67 * 1.20).abs() < 1e-9);
        assert!((fr(&[], &gunslinger) - 6.67 * 1.72).abs() < 1e-9);
        let both = fr(&["laetum_rapid_wrath"], &gunslinger);
        assert!(
            (both - 6.67 * 1.92).abs() < 1e-9,
            "must be ADDITIVE (12.81), got {both} — multiplicative would be {}",
            6.67 * 1.72 * 1.20
        );
    }

    /// The wiki CO catalog rates BOTH Laetum forms "Adding" at a 100% base
    /// fraction, and notes the bonus "multiplies properly with Devouring
    /// Attrition". Two consequences the engine must honour: CO reaches the
    /// DIRECT hit only (never the 300 Radiation radial), and Devouring — its
    /// own multiplicative bracket — multiplies the already-CO-boosted value.
    #[test]
    fn condition_overload_is_adding_direct_only_and_devouring_stacks_on_top() {
        use crate::dummy::{monte_carlo, DummyParams};
        let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
        assert_eq!(b.co_behavior, crate::loadout::CoBehavior::AdditiveWithBaseDamage);
        // 160/160 and 100/100 in the catalog: the whole base feeds the bonus.
        assert!((b.co_base_fraction - 1.0).abs() < 1e-9);

        let pool = crate::mods_data::pistol_pool();
        let co: Vec<&crate::loadout::ModDef> =
            pool.iter().filter(|m| m.id == "galvanized_shot").collect();
        let parts = vec![crate::dummy::BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            crit_bonus: false,
        }];
        let sources = |evos: &[&str], mods: &[&crate::loadout::ModDef]| {
            let b = WeaponBase::from_data("laetum_incarnon", true, evos);
            let p = crate::loadout::resolve(&b, mods, crate::loadout::StackPolicy::AssumedMax);
            let params =
                DummyParams::from_panel(&p, &crate::arena::Arena { body_parts: parts.clone(), ..crate::arena::Arena::training(20.0) });
            let s = monte_carlo(&params, 60, 17).source_damage;
            (s.direct, s.radial)
        };
        // The radial is INDIFFERENT to a CO mod; the direct hit is not.
        let (d0, r0) = sources(&[], &[]);
        let (d1, r1) = sources(&[], &co);
        assert!(d1 > d0 * 1.05, "CO must lift the direct hit: {d0:.0} -> {d1:.0}");
        assert!(
            (r1 / r0 - 1.0).abs() < 0.05,
            "CO must NOT reach the radial: {r0:.0} -> {r1:.0}"
        );
    }

    #[test]
    fn resolving_keeps_the_radial() {
        let b = WeaponBase::from_data("laetum_incarnon", true, &[]);
        let p = crate::loadout::resolve(&b, &[], crate::loadout::StackPolicy::AssumedMax);
        let r = p.radial.expect("resolved panel keeps the radial");
        assert!((r.damage.total() - 300.0).abs() < 1e-9, "got {}", r.damage.total());
    }
}

#[cfg(test)]
mod passive_tests {
    use super::*;

    /// A weapon PASSIVE belongs to the weapon that lists it. Frenzy is Dual
    /// Toxocyst's (both forms); the Laetum has none. Hardcoding it handed
    /// DT's x2.5-on-headshot fire rate to every transform weapon.
    #[test]
    fn frenzy_belongs_only_to_the_weapon_that_lists_it() {
        assert!(has_perk("dual_toxocyst", "frenzy"));
        assert!(has_perk("dual_toxocyst_incarnon", "frenzy"));
        assert!(!has_perk("laetum", "frenzy"));
        assert!(!has_perk("laetum_incarnon", "frenzy"));
        assert!(!has_perk("dual_toxocyst", "no_such_perk"));
    }
}
