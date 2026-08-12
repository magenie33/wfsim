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

/// How a shot ARRIVES — the module's per-attack `ShotType`.
///
/// PARSED, not kept as a string, because the one rule that reads it is an
/// EXCLUSION: a riven rolls Projectile Flight Speed only on a weapon that
/// fires something. A spelling the rule does not recognise therefore reads as
/// "it flies" and silently hands the stat to a weapon DE never rolls it on —
/// which is what `hitscan` (8 files) and `hit_scan` (13) did between them
/// until 2026-08-07. An unknown value is now a parse error at load, so the
/// vocabulary cannot drift again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShotType {
    /// The trace lands the instant the trigger does.
    HitScan,
    /// A continuous trace — instant in the same way, and never a projectile.
    Beam,
    /// Something with a flight speed, which is the stat's whole subject.
    Projectile,
}

impl ShotType {
    /// English display name (the i18n overlay translates from this).
    pub fn label(self) -> &'static str {
        match self {
            ShotType::HitScan => "Hit-Scan",
            ShotType::Beam => "Beam",
            ShotType::Projectile => "Projectile",
        }
    }

    /// Does a shot of this kind take TIME to reach the target?
    pub fn flies(self) -> bool {
        matches!(self, ShotType::Projectile)
    }
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
    pub shot_type: Option<ShotType>,
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
    /// A CHARGE THAT EATS THE MAGAZINE, in ammo per second (the Phantasma's
    /// 11). Present only where charging spends the magazine to buy damage.
    ///
    /// Three facts collapse into this one number, all from the weapon's own
    /// wiki Notes: *"Charging consumes ammo, up to a full magazine on full
    /// charge"*, *"Damage dealt by the plasma bomb is directly proportional to
    /// the amount of ammo consumed during the charge"*, and *"Charge rate
    /// consumes a set 11 ammo per second. Modding to increase magazine capacity
    /// will allow a longer total charge, and thus more damage."*
    ///
    /// So a full charge costs the WHOLE modded magazine, takes
    /// `magazine / rate` seconds, and is worth `magazine / base magazine` times
    /// the listed damage — which makes Magazine Capacity a DAMAGE stat on this
    /// weapon, and the only one in the roster where it is. `loadout::resolve`
    /// does all three; the listed numbers here are a FULL charge of the
    /// unmodded magazine, which is what the arsenal shows.
    #[serde(default)]
    pub charge_ammo_per_second: Option<f64>,
    /// A FIRE RATE THAT FALLS WHILE THE TRIGGER IS HELD — see
    /// [`SustainedFireRate`]. `None` on every weapon that fires at one rate.
    #[serde(default)]
    pub sustained_fire_rate: Option<SustainedFireRate>,
    /// A BURST trigger's shape — the Burston's three-round pull. See
    /// [`BurstSpec`] for the cadence formula and why it is exact here.
    #[serde(default)]
    pub burst: Option<BurstSpec>,
    #[serde(default = "one")]
    pub multishot: f64,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub status_chance: f64,
    pub damage: BTreeMap<String, f64>,
    /// Damage types this attack applies on EVERY hit regardless of status
    /// chance — "Plasma bomb and seeking projectiles have a guaranteed Impact
    /// proc" (Phantasma Prime). Rolled status is unaffected and lands on top.
    ///
    /// DIRECT hits only, which is the engine's existing rule
    /// (`if direct { &ap.forced_procs }`) and is the wiki's too: the Astilla's
    /// direct hit forces Impact and its radial does not.
    #[serde(default)]
    pub forced_procs: Vec<String>,
    #[serde(default)]
    pub ricochet: Option<RicochetSpec>,
    /// DAMAGE FALLOFF on the direct hit — the shotgun's, and the one the
    /// Arsenal lists as a range in metres.
    ///
    /// **NOT MODELLED IN THE FIGHT**, and recorded anyway. This arena has no
    /// distance: every shot lands at point blank, so a weapon with falloff is
    /// simulated at its best case and says so in `unmodeled:`.
    ///
    /// What reads it today is the RIVEN pool. Wiki (`Projectile Speed`),
    /// verbatim: *"Mods including Rivens that have positive or negative
    /// Projectile speeds will affect a weapon's entire Damage Falloff range
    /// accordingly"* and *"Hitscan weapons that do **not** list Damage Falloff
    /// values in their UI are completely unaffected by Projectile Speed
    /// modifications"*. So listing a falloff is precisely what gives the
    /// Projectile Speed stat something to act on when nothing flies — it is
    /// why a shotgun rolls it, and why the Furis does (its Incarnon form
    /// falls off from 10 m to 16 m) while the Latron does not (owner,
    /// 2026-08-08: "会间接影响射程。很多霰弹有这个特性").
    #[serde(default)]
    pub falloff: Option<FalloffSpec>,
    /// A radial (AoE) part fired with every projectile of this attack.
    #[serde(default)]
    pub radial: Option<RadialSpec>,
    /// PRIMARY COMPRESSION's row for this attack — see [`CompressionSpec`].
    /// `None` means the weapon is absent from the table, which is not the same
    /// as 0%: absent is untested or inapplicable (every secondary, since the
    /// arcane is a PRIMARY one), while 0% is a tested "Doesn't Work".
    #[serde(default)]
    pub compression: Option<CompressionSpec>,
    /// A LINGERING FIELD left by every landed projectile (Torid's cloud).
    #[serde(default)]
    pub lingering: Option<LingeringSpec>,
    /// Continuous-beam geometry (Torid Incarnon). Shape, not a damage part.
    #[serde(default)]
    pub beam: Option<BeamSpec>,
}

/// Direct-hit damage falloff: full damage inside `start_m`, decreasing
/// linearly to `reduction` of it at `end_m` and beyond.
///
/// `reduction` is DE's own field and it is the fraction KEPT, not the fraction
/// lost — the Boar keeps 0.5 past 25 m. It reads the opposite way to
/// [`RadialSpec::falloff_reduction`], which is the amount REMOVED, and the two
/// are kept as their sources state them rather than being normalised into a
/// shared spelling that would make one of them a lie about its source.
#[derive(Debug, Clone, Deserialize)]
pub struct FalloffSpec {
    /// Metres out to which damage is full.
    pub start_m: f64,
    /// Metres past which damage stops dropping.
    pub end_m: f64,
    /// Fraction of damage KEPT at `end_m` and beyond.
    pub reduction: f64,
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
    /// Does the BLAST-RADIUS bucket (Firestorm, Fulmination) reach this
    /// explosion? True everywhere but the Shedu and both Trumnas, whose pages
    /// say otherwise — "Explosion cannot benefit from Firestorm (Primed)
    /// despite being area of effect" (wiki Shedu, verbatim).
    ///
    /// It changes no damage while the arena has one target. It changes PRIMARY
    /// COMPRESSION, which pays per metre of radius given up and therefore reads
    /// this number directly — 44% of the Shedu's bonus with Primed Firestorm
    /// equipped. docs/CATALOGS.md §2.
    #[serde(default = "yes")]
    pub takes_blast_radius_mods: bool,
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
    /// Does the explosion fire once PER PELLET, or once per trigger pull?
    ///
    /// Default YES (per pellet) — a radial rides its projectile, so a weapon
    /// that throws two projectiles detonates twice. The Burston's Incarnon is
    /// the exception the wiki states outright: "The Radial Attack does not
    /// benefit from Multishot bonuses". Declared per entry, never inferred.
    #[serde(default = "yes")]
    pub takes_multishot: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RicochetSpec {
    pub targets: u32,
    pub range_m: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IncarnonSpec {
    pub gauge: GaugeSpec,
    /// The transition INTO the form, unmodded. Per weapon: it is that
    /// weapon's reload time, which the page states outright ("an animation
    /// equal to the weapon's reload speed"). Scales by the reload formula.
    pub transmute_in_seconds: f64,
    /// The transition OUT, unmodded — and this one is OUR STANDARD rather
    /// than anyone's published number.
    ///
    /// One second, measured once on the Dual Toxocyst (MEASUREMENTS M9) and
    /// applied to every form since. DE publishes nothing for it and no second
    /// weapon has been measured, so restating it in sixty-nine weapon files
    /// dressed a house convention as sixty-nine facts. It lives here, once,
    /// and a weapon that is ever measured to differ says so by writing the
    /// field (owner, 2026-08-08: "要标准不是官方数据").
    #[serde(default = "standard_transmute_out")]
    pub transmute_out_seconds: f64,
}

/// See [`IncarnonSpec::transmute_out_seconds`]. Changing this changes every
/// Incarnon cycle in the roster, which is the point of it being one number.
fn standard_transmute_out() -> f64 {
    1.0
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
    /// WHAT THIS ENTRY DOES NOT MODEL, in the reader's own language, one
    /// sentence per gap.
    ///
    /// The enemy files have carried this since the target card was written, and
    /// weapons had nowhere to put it: a yaml COMMENT is honest to whoever opens
    /// the file and invisible to everyone else. The bulk Incarnon intake made
    /// that expensive — a weapon whose base attack has parts this entry does not
    /// carry (a bow's uncharged shot, the Angstrum's explosion, the Stug's
    /// blobs) reads as a complete weapon, and its number is not the weapon's
    /// number (owner, 2026-08-08: "没建模的要如实说，因为我自己要看，也给用户看").
    ///
    /// Prose, deliberately, and the ONE place in a weapon file where prose is a
    /// value rather than a comment — the same exception `enemies/` already
    /// carries, for the same reason: it is shown to a reader verbatim.
    #[serde(default)]
    pub unmodeled: Vec<String>,
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
    /// Which RIVEN this weapon takes, by the family's English name — one
    /// riven fits every variant in it, which is why it is a name and not an
    /// id. It is the key `data/rivens/pools.yaml` is surveyed by: DE rolls a
    /// pool per family, so a Boar riven and a Boar Prime riven are one thing.
    #[serde(default)]
    pub riven_family: Option<String>,
    /// `by_round` — the magazine refills a SHELL AT A TIME (Strun, Felarx,
    /// Onos). It is the wiki module's `ReloadStyle`, and it is not cosmetic:
    /// a bigger magazine makes the reload LONGER, so a magazine mod buys
    /// capacity and pays for it in downtime. Modelled as one flat block until
    /// 2026-08-08, which made Ammo Stock read as free capacity on exactly the
    /// weapons the game charges for it (owner, calibrating the Felarx).
    #[serde(default)]
    pub reload_style: Option<String>,
    /// The three parts of a by-round reload, where the weapon's page states
    /// them — the Felarx's are 0.8 s to start, 0.4 s a shell, 0.5 s to end.
    ///
    /// Where they are NOT stated, the engine derives a per-shell time from the
    /// published total and the base magazine and leaves the fixed parts at
    /// zero. That reproduces the published number exactly at a full magazine
    /// and scales correctly with capacity, which is the behaviour that was
    /// missing; it only understates a PARTIAL reload, and this sim reloads
    /// from empty.
    #[serde(default)]
    pub reload_start_s: Option<f64>,
    #[serde(default)]
    pub reload_per_shell_s: Option<f64>,
    #[serde(default)]
    pub reload_end_s: Option<f64>,
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
    /// A status-triggered crit-chance LOCK — Gotva Prime's passive, and the
    /// first of its kind in the roster.
    #[serde(default)]
    pub super_crit_on_status: Option<SuperCritSpec>,
    /// The Ocucor's tendrils — see [`TendrilSpec`].
    #[serde(default)]
    pub tendrils: Option<TendrilSpec>,
    /// Where this CONTINUOUS weapon's damage ramp starts, as a fraction of
    /// full damage. Omitted means the wiki's "for most weapons" 20%; state it
    /// only for a weapon whose page gives a different number.
    #[serde(default)]
    pub beam_ramp_floor: Option<f64>,
    #[serde(default)]
    pub reload_seconds: Option<f64>,
    /// A MAGAZINE THAT REFILLS ITSELF — see [`Battery`]. `None` on every weapon
    /// that reloads.
    #[serde(default)]
    pub battery: Option<Battery>,
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

fn yes() -> bool {
    true
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

/// HOW A WEAPON IS PLAYED FOR A WHOLE ENGAGEMENT — a policy over its forms.
///
/// A FORM is what the weapon is at an instant (base, charged, Incarnon); a MODE
/// is what you do with those forms for three hundred seconds. They were one
/// field for a long time and it could not express the questions worth asking:
/// `form: incarnon_cycle` is a mode wearing a form's name, and `form: default`
/// resolves to one or the other depending on a weapon flag, so a benchmark that
/// may not name a form could only ever ask for "however it is normally played".
/// "The Torid without ever transmuting" was unaskable (owner, 2026-08-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    /// The arsenal's form, all engagement. Every weapon has this one.
    Base,
    /// A FREE other form, all engagement — a bow that never charges, an
    /// Arch-Gun fired on its alt. Nothing is spent to be in it, so it can be
    /// held for a whole engagement and a ruler may rank it.
    Alternate,
    /// A GAUGE-FED other form, all engagement. Same shape as [`Self::Alternate`]
    /// and a different claim: you cannot be in it for a whole engagement, so no
    /// ruler ranks it, and it exists as a mode only so the builder can show the
    /// form's own numbers.
    ///
    /// Split off from `Alternate` in 2026-08-08, when the Paris arrived. A bow
    /// with an adapter has THREE forms — drawn, tapped, Incarnon — and the two
    /// alternates were both emitting `id: "alternate"`, so a build naming a
    /// mode named two of them. A weapon with one alternate never noticed.
    Transformed,
    /// Fill the gauge in the base form, spend it in the other, come back.
    Cycle,
}

impl PlayMode {
    pub fn id(self) -> &'static str {
        match self {
            PlayMode::Base => "base",
            PlayMode::Alternate => "alternate",
            PlayMode::Transformed => "transformed",
            PlayMode::Cycle => "cycle",
        }
    }
}

/// One way this weapon can be played, and whether a ruler may rank it.
#[derive(Debug, Clone, Copy)]
pub struct WeaponPlayMode {
    /// The mode's own id, and the only name a submission or a board row uses.
    pub id: &'static str,
    pub mode: PlayMode,
    /// The form entry this mode fires — for a cycle, the one it returns to.
    pub weapon_id: &'static str,
    /// The other form, for a cycle.
    pub other_id: Option<&'static str>,
    /// May a standard benchmark rank this? See [`play_modes`].
    pub sustainable: bool,
}

/// Every way this weapon can be played, derived from the forms it registers.
///
/// SUSTAINABILITY IS DERIVED, NOT DECLARED. A form entered by filling a gauge
/// that then empties (`auto_transmute_out: on_incarnon_ammo_empty`) cannot be
/// held for a whole engagement — "always Incarnon" is not a playstyle, it is a
/// thing that happens for a few seconds at a time. A form with no gauge is just
/// a trigger pull and can be used forever, which is why a Cernos Prime that
/// never charges IS a way to play it and belongs on a board.
///
/// So nothing has to be marked. The gauge is read off the FORM ENTRY rather
/// than off the form's NAME, which is what keeps this true for the next
/// gauge-switched weapon that is not an Incarnon — Mausolon's alt-fire is
/// charged by kills, and it will get its cycle from declaring a gauge and
/// nothing else (owner, 2026-08-07).
impl WeaponPlayMode {
    /// The `form` a fight request must carry to be played this way.
    ///
    /// The fight parser's vocabulary is form KINDS plus the one policy word:
    /// `incarnon_cycle` runs the cycle, and anything else names a single form
    /// to fire. So a mode is translated at that boundary and nowhere else —
    /// which is what lets "played without ever transmuting" be ASKED FOR at
    /// all, where `form: default` could only ever mean "however it is normally
    /// played" and resolved to the cycle behind your back.
    pub fn form(&self) -> &'static str {
        match self.mode {
            PlayMode::Cycle => "incarnon_cycle",
            // The single form this mode fires, named by its KIND — the Cernos
            // Prime's `base` mode is its CHARGED form, because that is the one
            // the arsenal gives you.
            _ => spec(self.weapon_id).map_or("default", |s| s.form_kind().id()),
        }
    }
}

pub fn play_modes(weapon_id: &str) -> Vec<WeaponPlayMode> {
    let forms = forms_of(weapon_id);
    let Some(default) = forms.iter().find(|f| f.is_default).or(forms.first()) else {
        return Vec::new();
    };
    let mut out = vec![WeaponPlayMode {
        id: PlayMode::Base.id(),
        mode: PlayMode::Base,
        weapon_id: default.weapon_id,
        other_id: None,
        sustainable: true,
    }];
    for alt in forms.iter().filter(|f| f.weapon_id != default.weapon_id) {
        // The GAUGE, not the kind: "does entering this cost a meter you must
        // earn" is the question, and the answer lives on the entry.
        let gauged = spec(alt.weapon_id).is_some_and(|s| s.incarnon.is_some());
        if gauged {
            out.push(WeaponPlayMode {
                id: PlayMode::Cycle.id(),
                mode: PlayMode::Cycle,
                weapon_id: default.weapon_id,
                other_id: Some(alt.weapon_id),
                sustainable: true,
            });
        }
        // ONE MODE PER ALTERNATE FORM, and the two kinds have DIFFERENT IDS —
        // a weapon may have more than one alternate (a bow with an adapter has
        // a tapped shot and an Incarnon form), and a mode id is what a build
        // names, so two of them sharing one id names neither.
        let mode = if gauged { PlayMode::Transformed } else { PlayMode::Alternate };
        out.push(WeaponPlayMode {
            id: mode.id(),
            mode,
            weapon_id: alt.weapon_id,
            other_id: None,
            // A gauge you must fill and then run dry is exactly what cannot be
            // sustained; anything else can.
            sustainable: !gauged,
        });
    }
    out
}

/// Does this weapon have a form you TRANSFORM into (a gauge and two transmute
/// animations)? Only such a weapon has a cycle to simulate — anything else is
/// fired in one form at a time, whatever forms it registers.
pub fn has_gauge_switched_form(weapon_id: &str) -> bool {
    forms_of(weapon_id).iter().any(|f| f.kind.is_gauge_switched())
}

pub(crate) fn damage_type(name: &str) -> DamageType {
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

/// "Status Effects have a X% chance to set the next hit's Critical Chance to
/// Y" — Gotva Prime's passive (wiki, Characteristics).
///
/// A SET and not a bonus: "Set Critical Chance ignores all other modifiers,
/// whether from mods or Warframe abilities". The tier UPGRADE still applies
/// afterwards, which is how Vigilante can carry it to a Tier-4 hit — so the
/// lock binds the chance, not the ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct SuperCritSpec {
    /// Per pellet that applied at least one status. Several statuses on one
    /// hit do not raise it (wiki).
    pub chance: f64,
    /// The chance the next landing pellet uses, verbatim. 3.0 = 300%.
    pub crit_chance: f64,
}

/// WHAT THIS WEAPON DOES THAT ITS STATS DO NOT SAY — its passives, as
/// sentences GENERATED from the data that implements them.
///
/// Generated and not written down, for the same reason a mod's effect lines
/// are: a sentence in the YAML would be prose in a data field, and worse, a
/// second copy of a number that could drift from the one the sim uses. Every
/// line here is built from the values the engine actually reads, so a passive
/// cannot be described as something it is not.
///
/// This exists because a weapon whose passive is invisible reads as an ordinary
/// weapon (owner, 2026-08-05): Gotva Prime's crit set and Dual Toxocyst's
/// Frenzy are most of what those weapons ARE, and nothing on the page said so.
pub fn passive_lines(weapon: &str) -> Vec<String> {
    let Some(s) = spec(weapon) else { return Vec::new() };
    let mut out = Vec::new();

    if let Some(sc) = s.super_crit_on_status {
        out.push(format!(
            "A Status Effect has a {:.0}% chance to SET the next hit's crit chance to {:.0}% — ignoring every other crit bonus. Rolled per pellet.",
            sc.chance * 100.0,
            sc.crit_chance * 100.0
        ));
    }

    // An innate headshot bonus normally joins the additive bracket; this flag
    // marks the weapon whose does not (Cernos Prime, wiki: "unique and stacks
    // MULTIPLICATIVELY with Primary Deadhead's").
    if s.headshot_bonus_multiplicative {
        out.push(
            "Its innate headshot bonus MULTIPLIES the headshot bracket instead of joining it, so it compounds with Deadhead and Target Acquired rather than adding to them."
                .to_string(),
        );
    }

    // THE OCUCOR'S TENDRILS. Says what it is AND what it is worth HERE, because
    // the second half is the surprising one: this is the weapon's whole
    // identity and its damage against a lone enemy is zero. A line that only
    // described the passive would read as a promise the number does not keep.
    if let Some(t) = s.tendrils {
        out.push(format!(
            "Every kill spawns an energy tendril that reaches for ANOTHER enemy, up to {}; a reload or an empty magazine clears them all. Their damage is NOT counted here — a tendril homing on the target you are already shooting is cosmetic (wiki) and this sim fights one enemy — but the count is, because Sentient Surge pays crit chance and status chance per active tendril.",
            t.max
        ));
    }

    // THE SPOOL, which is the one passive that makes the stat above it WRONG
    // rather than incomplete: the panel prints one fire rate and the weapon
    // never fires at it for long. A reader who sees only the printed rate has
    // no way to tell whether the DPS below it is the one they measured, so the
    // line states where the rate starts, where it ends, and when.
    //
    // Each direction is phrased with the number ITS OWN page prints — a faller
    // is given as a span ("over 51 shots", the Phenmor's words), a riser as the
    // shot it is finally full on ("from the 9th", the Gorgon's) — rather than
    // one shape forced onto both. Same field, same arithmetic, two sentences.
    if let Some(sp) = s.attack.sustained_fire_rate {
        out.push(if sp.end < sp.start {
            format!(
                "Its fire rate FALLS while the trigger is held — to {:.0}% of the listed rate over {:.0} shots — and rebuilds the moment you stop firing. This is simulated; the sim holds the trigger until the magazine is dry.",
                sp.end * 100.0,
                sp.over_shots
            )
        } else {
            format!(
                "Its fire rate SPOOLS UP while the trigger is held — from {:.0}% of the listed rate, full from the {}th shot. This is simulated, and it rebuilds after every reload.",
                sp.start * 100.0,
                sp.over_shots.ceil() as i64 + 1
            )
        });
    }

    // NOT `no_resupply`. It was listed here and taken out (owner, 2026-08-05:
    // "这个不是被动...是archgun一类的特性"): every ground Arch-Gun is removed
    // when its reserve runs out, so it says nothing about THIS weapon. A line
    // that is true of a whole class does not belong on the entry for one member
    // of it — it reads as a distinguishing feature and distinguishes nothing.
    //
    // The rule still reaches the player where it is a decision: the scenario's
    // Infinite-ammo control is forced off for such a weapon, and says why.

    // A PERK NAMES ITSELF AND STOPS THERE. `weapons_data::PerkSpec` carries the
    // reference and its element injection; the NUMBERS live in the perk's own
    // module (`perks::frenzy`) and reach the player through its buff card,
    // which already shows the stacks, the duration and what it grants.
    //
    // So this line's job is to tell you the weapon HAS one — which is the whole
    // complaint: nothing on the page said Dual Toxocyst had Frenzy at all, and
    // a passive you do not know to look for is a passive you do not have. The
    // buff card is where its numbers belong and where they already are; a
    // second copy here is a second thing to keep true.
    for p in &s.perks {
        let r = p.resolve();
        let mut line = format!("Weapon passive: {} — see its buff card", pretty_id(&r.id));
        if let Some(inj) = r.grants.as_ref().and_then(|g| g.injected_element.as_ref()) {
            line.push_str(&format!(" (grants +{} {} while active)", inj.amount, inj.element));
        }
        out.push(format!("{line}."));
    }
    out
}

/// `dual_toxocyst_fevered_frenzy` -> "Fevered Frenzy". Display only.
fn pretty_id(id: &str) -> String {
    id.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A BURST trigger: one pull fires `count` rounds `delay_seconds` apart, and
/// the weapon's listed `fire_rate` is BURSTS per second, not rounds.
///
/// VERBATIM (wiki Fire Rate), and the reason a burst weapon is not just a
/// slower auto:
///
/// - *"Effective Fire Rate = Burst Count / [1/Fire Rate + [(Burst Count−1)⋅
///   Burst Delay]]"*
/// - *"Fire Rate bonuses affect both the speed of the burst as well as the
///   time between bursts"*
/// - *"Burst Delay is not affected by net negative Fire Rate bonuses."*
///
/// The middle line is what makes this cheap: because a bonus scales BOTH
/// terms, the effective rate is exactly linear in the fire-rate multiplier, so
/// a positive-bonus build is indistinguishable from an auto weapon listed at
/// the effective rate. The THIRD line is the only place burst stops being a
/// relabelling — a net-negative bonus (Critical Delay, Vile Precision) stops
/// stretching the intra-burst delay while it keeps stretching the gap between
/// bursts, so the weapon loses less rate than the number on the card says.
/// That asymmetry is modelled; see the `.max(1.0)` in `loadout::resolve`.
///
/// PRIMARY COMPRESSION's per-weapon row — see docs/CATALOGS.md §2.
///
/// The arcane trades explosion RADIUS for damage, so what it is worth is a
/// property of the weapon rather than of the arcane, and the wiki publishes a
/// table with one row per weapon ATTACK. Two of its columns cannot be derived
/// from anything else the weapon knows:
///
/// > Weapon | Effectiveness | Base Radius | Max Damage Bonus @ Base Radius |
/// > Stacking Behavior | Notes
#[derive(Debug, Clone, serde::Deserialize)]
pub struct CompressionSpec {
    /// The row's **Compression Effectiveness**, and the page's legend says what
    /// that means: *"how much bigger/smaller the radius Compression considers
    /// compared to how much it should be considering. 100% means 'intended'."*
    ///
    /// So it is a factor on the RADIUS READ, not a discount on the damage —
    /// the arithmetic lands in the same place, but the Vectis pair's 0.04 is
    /// the arcane reading a 0.1 m embed radial instead of the headshot
    /// explosion, and the Trumna alt-fire's 1.27 is a radius counted twice.
    pub effectiveness: f64,
    /// The row's **Stacking Behavior with Damage Bonuses**: `multiplies` (the
    /// common case) or `adds` (Ambassador, Battacor, Ferrox, Opticor, Trumna,
    /// and every Braton and Burston Incarnon). A bracket, not a number.
    pub stacking: String,
    /// The row's **Radius Calculation**, which is a COLUMN and not a note —
    /// it decides WHICH radius the arcane reads on a weapon with more than one
    /// AoE-bearing firing mode. The legend's three, plus one the table uses:
    ///
    /// - `snapshot` — *"uses the ads state when fired, not when AoE occurs"*;
    ///   the ordinary value.
    /// - `stolen` — *"uses another firing mode's radius"* (Mausolon).
    /// - `doesnt_work` — the arcane does not apply to this AoE.
    /// - `constant_check` — the Battacor, and the legend does not list it.
    #[serde(default = "snapshot")]
    pub radius_calculation: String,
    /// The row's **Base Radius**, when it is a radius this weapon's data does
    /// NOT carry. Left out, the arcane reads the attack's own MODDED radius —
    /// which is what makes the table's Primed Firestorm column exactly 1.44x
    /// its base column on every row that takes the mod.
    ///
    /// The Vectis pair are the roster's only override, and they are why this
    /// field exists rather than a second multiplication: their row reads
    /// **0.1 m** where the Incarnon's own explosion is 6.7 m, and 4% of 6.7 is
    /// not 0.1. `effectiveness` is the row's own account of how far off that
    /// is ("worse than expected"); it does not reconstruct the number, so when
    /// this is set it is the whole answer and effectiveness is not applied
    /// again.
    pub reads_radius_m: Option<f64>,
}

fn snapshot() -> String {
    "snapshot".to_string()
}

/// A MAGAZINE THAT REFILLS ITSELF, on a clock rather than on a reload.
///
/// The Shedu's battery, and the roster's first: *"This weapon does not use ammo
/// pickups; ammo regenerates over time. Has a 1 second delay before ammo begins
/// to regenerate; if there are still rounds left, the delay is 0.4 seconds
/// instead. Ammo regenerates at 28 rounds per second"* (wiki Shedu, verbatim).
///
/// The listed "Reload Time" is therefore not a reload at all — it is
/// `delay + magazine/rate`, and BOTH published numbers fall out of it: 1.0 +
/// 7/28 = 1.25 s is the wiki's figure for an empty battery, 0.4 + 7/28 = 0.65 s
/// is WFCD's for a partial one. The two sources never disagreed; each published
/// a different case.
///
/// WHAT IT CHANGES that a plain reload does not: the battery refills BETWEEN
/// SHOTS, and only for the part of the gap that exceeds the delay. So the
/// weapon breaks even when
///
///   `1 / fire_rate  >=  delay_partial + ammo_cost / regen_per_second`
///
/// which on the Shedu is `0.4 + 1/28 = 0.4357 s`, i.e. **2.295 rounds a
/// second** — 8.2% below its listed 2.50. Above that it drains and reloads;
/// below it, every gap returns more than a whole round and the battery NEVER
/// EMPTIES. A fire-rate penalty of nine percent therefore removes the weapon's
/// reload entirely, which is the one thing a `reload_seconds` cannot say.
///
/// The listed rate sits just 0.036 s above break-even, so the effect is not a
/// curiosity: Vile Precision alone crosses it.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct Battery {
    /// Rounds a second, once the delay has passed (28).
    pub regen_per_second: f64,
    /// The wait before regeneration starts with the battery EMPTY (1.0 s).
    /// `reload_seconds` already carries `this + magazine/rate`, so this field
    /// is what the between-shots case needs rather than a second copy of it.
    pub delay_empty_s: f64,
    /// …and with rounds still in it (0.4 s).
    pub delay_partial_s: f64,
}

/// A SPOOL: the rate MOVES the longer the trigger is held, and rebuilds from
/// the start once firing pauses.
///
/// Both directions, one field, because they are one mechanic — six weapons in
/// the roster and five of them go UP:
///
/// | weapon | start | span | full/floor at |
/// | --- | --- | --- | --- |
/// | Phenmor (Incarnon) | 100% | 51 | 60% |
/// | Gorgon | 20% | 7.5 | shot 9 |
/// | Gorgon Wraith | 20% | 5 | shot 6 |
/// | Prisma Gorgon | 20% | 6 | shot 7 |
/// | Soma | 25% | 5 | shot 6 |
/// | Soma Prime | 25% | 2.5 | shot 4 |
///
/// Each page states its spool TWICE — a percentage per shot and a count of
/// shots to optimal — and the two reconcile exactly on all five risers, which
/// is what `over_shots` is derived from and why it is not always an integer
/// (the Gorgon's 10.667% per shot IS 0.8/7.5). VERBATIM, one of each shape:
///
/// > Fire rate starts at 20% of its listed value, and increases by 10.667% per
/// > shot. Requires a spool-up of 9 shots before optimal fire rate is achieved.
/// > Burst firing maintains spool-up.        (wiki Gorgon)
///
/// > Fire rate decreases from 100% to 60% over 51 shots as the trigger is held
/// > … Spool resets once the player stops firing.          (wiki Phenmor)
///
/// The fall/climb is LINEAR — every page gives two ends and a count and nothing
/// between them.
///
/// Not to be confused with `beam_ramp_floor`, which is a continuous weapon's
/// DAMAGE ramp: that one climbs in seconds and holds, this one moves in SHOTS
/// and is a cadence. The Phantasma has both and they are unrelated.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct SustainedFireRate {
    /// Where the rate STARTS, as a fraction of the listed one, on the first
    /// shot after a pause (1.00 on the Phenmor, 0.20 on a Gorgon).
    pub start: f64,
    /// Where it SETTLES (0.60 on the Phenmor, 1.00 on everything that spools
    /// up). `end < start` is a spool-down; `end > start` a spool-up.
    pub end: f64,
    /// The span, in held shots. Shot `n` (0-based, counting from the pause)
    /// sits at `start + (end − start)·min(n, over_shots)/over_shots`, so a
    /// riser is at full from shot `ceil(over_shots) + 1` — which is the number
    /// each page prints.
    pub over_shots: f64,
}

/// WHAT IS NOT MODELLED, stated because it is a real difference: the sim
/// spaces rounds EVENLY at the effective rate instead of clumping them into
/// bursts. Nothing the single-target arena reads can tell the difference —
/// total rounds, ammo, status rolls and reload cadence are all identical over
/// any whole number of bursts — but a buff whose window is shorter than one
/// burst cycle (0.28 s on a Burston Prime) would see a different pattern, and
/// so would a per-burst TRIGGER. The Burston has exactly one of those, Reaver's
/// Rapture ("On Full Burst Hit: +20% Damage"), and it is the reason `count`
/// is carried rather than folded away into an effective rate: whoever models
/// that perk needs "every `count`-th round completes a burst", which this
/// field is, and which an effective rate would have thrown away.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct BurstSpec {
    /// Rounds per pull. Each is a full instance — its own multishot, crit and
    /// status rolls — so this is NOT multishot.
    pub count: u32,
    /// Seconds between rounds WITHIN a burst (the module's `BurstDelay`).
    pub delay_seconds: f64,
}

/// THE OCUCOR'S TENDRILS: a kill spawns an energy tendril, up to `max`, and
/// any magazine event clears them all.
///
/// WHAT IS DELIBERATELY ABSENT: damage. A tendril reaches for a DIFFERENT
/// enemy, and the wiki is explicit about the one that reaches this fight's
/// target — *"Tendrils homing in on the main beam's target are only cosmetic,
/// and don't deal any additional damage or status effects."* So in a
/// single-target arena a tendril's own damage is zero, and modelling it would
/// inflate the weapon by up to four beams that the source says are not there.
///
/// The COUNT still matters, which is why this type exists at all: Sentient
/// Surge scales crit chance and status chance with how many tendrils are up,
/// and those land on the MAIN beam, which is real damage against this target.
/// The passive therefore reaches the fight through the mod and not through
/// itself.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct TendrilSpec {
    /// The cap. Four on the Ocucor.
    pub max: u32,
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
        // A burst trigger is its OWN family, not a semi-auto that fires three
        // times: the wiki lists the Burston's trigger as "Burst", and the
        // Semi-* mods gate on the listed trigger. So a Burston takes no
        // Semi-Rifle Cannonade, which is exactly what the roster's Cannonade
        // table asserts weapon by weapon.
        "burst" => out.push("burst"),
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

    // A BY-ROUND RELOAD, resolved into (start, per shell, end).
    //
    // Where the weapon's page states the three parts, they are used. Where it
    // does not, the per-shell time is DERIVED from the published total and the
    // base magazine with the fixed parts at zero: that reproduces the
    // published number exactly at a full magazine and scales correctly with
    // capacity, which is the behaviour that was missing. It understates a
    // PARTIAL reload by the fixed part, and this sim reloads from empty.
    //
    // A pseudo-reload (an Incarnon form's charge pool) is never by-round: it
    // is not a magazine, it is a resource that runs out.
    let by_round_reload = (s.reload_style.as_deref() == Some("by_round")
        && s.pseudo_reload.is_none())
        .then(|| {
            let start = s.reload_start_s.unwrap_or(0.0);
            let end = s.reload_end_s.unwrap_or(0.0);
            let per = s.reload_per_shell_s.unwrap_or_else(|| {
                ((base_reload - start - end) / magazine_size.max(1.0)).max(0.0)
            });
            (start, per, end)
        });

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
            takes_blast_radius_mods: r.takes_blast_radius_mods,
            falloff_start_m: r.falloff_start_m.unwrap_or(0.0),
            falloff_reduction: r.falloff_reduction.unwrap_or(0.0),
            takes_condition_overload: r.takes_condition_overload,
            takes_multishot: r.takes_multishot,
            // 1.0 until an evolution raises the explosion's base without
            // raising what CO multiplies — see `evolutions_data::apply`.
            co_base_fraction: 1.0,
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
        form: FormKind::parse(&s.form),
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
        charge_ammo_per_second: s.attack.charge_ammo_per_second,
        sustained_fire_rate: s.attack.sustained_fire_rate,
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
        burst: s.attack.burst,
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
        super_crit_on_status: s.super_crit_on_status,
        tendril_max: s.tendrils.map_or(0, |t| t.max),
        // The DEFAULT lives with the ramp it belongs to, so "most weapons"
        // is stated once rather than copied into a second file that is free
        // to drift from it.
        beam_ramp_floor: s.beam_ramp_floor.unwrap_or(crate::dummy::BEAM_RAMP_FLOOR),
        battery: s.battery,
        forced_procs: s.attack.forced_procs.iter().map(|t| damage_type(t)).collect(),
        no_resupply: s.no_resupply,
        base_reload,
        by_round_reload,
        innate_co_per_type: 0.0,
        innate_co_gated: 0.0,
        co_min_sprint: 0.0,
        evo_fire_rate_gated: 0.0,
        fire_rate_min_sprint: 0.0,
        bd_below_half_health: 0.0,
        cc_on_undamaged: 0.0,
        cd_on_undamaged: 0.0,
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
        compression: s.attack.compression.clone(),
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
        multishot_on_last_round: 0.0,
        base_multishot_on_last_round: 0.0,        // raised by Final Fusillade
        multishot_ammo_bonus: 0.0,           // raised by Plentiful Mayhem
        // Raised by evolutions, never by the raw weapon data.
        evo_fire_rate_bonus: 0.0,
        evo_reload_bonus: 0.0,
        rs_on_empty_reload: 0.0,
        armor_strip_per_puncture: 0.0,
        instant_reload_on_headshot: None,
        headshot_streak: None,
        cd_below_status_count: None,
        // Set by Prelude of Might at `evolutions_data::apply`, read in `resolve`.
        crit_mult_below_cc: None,
        // Set by Headcracker at `evolutions_data::apply`.
        // Filled by `evolutions_data::apply`; see `StackingBuff`.
        stacking_buffs: Vec::new(),
        post_mod_crit_chance: 0.0,
        post_mod_status_chance: 0.0,
        // Evolutions ADD to this (Caput Mortuum); a weapon's innate share is
        // the module's `ExtraHeadshotDmg`.
        headshot_damage_bonus: s.headshot_damage_bonus.unwrap_or(0.0),
        noncrit_bonus: None,
    }
}

#[cfg(test)]
mod tests {

    /// EVERY CO ANOMALY IN THE ROSTER IS ON THIS LIST, and the list is the
    /// catalog. Nothing else may be anything but ordinary.
    ///
    /// The rule (owner, 2026-08-12): *"同家族的灵化仍旧视为不同的武器。只要不在
    /// co表上，一律视为正常的只对direct的100%的加算…如果一个武器的普通的在表上，
    /// 而prime没有，那普通特殊处理，prime正常处理，不要擅自家族推广."* Ordinary
    /// has a definition — direct hits only, 100% of the base, added to the
    /// base-damage bucket — and a shared Genesis does not make one weapon.
    ///
    /// A LIST rather than a count, because the failure this exists to stop is
    /// not "someone added an anomaly", it is "someone gave one to the variant
    /// next door". Three entries had been generalised that way, and each of
    /// them looked like a reasonable reading of a row that did not name it.
    /// Adding a weapon whose family has a row now fails here until the row is
    /// checked for that weapon's own name.
    #[test]
    fn the_only_condition_overload_anomalies_are_the_ones_the_catalog_names() {
        // (entry, behaviour, co_base_fraction) — see docs/CATALOGS.md for the
        // verbatim row behind each.
        const NAMED: &[(&str, &str, f64)] = &[
            ("angstrum_incarnon", "independent", 1.0),
            ("prisma_angstrum_incarnon", "independent", 1.0),
            ("ballistica", "additive_with_base_damage", 0.25),
            ("ballistica_prime", "additive_with_base_damage", 0.50),
            ("ballistica_prime_incarnon", "independent", 1.0),
            ("rakta_ballistica", "additive_with_base_damage", 0.25),
            ("cernos_prime", "additive_with_base_damage", 0.5),
            ("dread", "additive_with_base_damage", 0.5),
            ("dread_incarnon", "independent", 1.0),
            ("felarx", "independent", 1.0),
            ("felarx_incarnon", "independent", 1.0),
            ("kunai_incarnon", "independent", 1.0),
            ("mk1_kunai_incarnon", "independent", 1.0),
            ("larkspur_prime_charged", "independent", 1.0),
            ("latron_incarnon", "independent", 1.0),
            ("latron_prime_incarnon", "independent", 1.0),
            ("miter", "additive_with_base_damage", 0.40),
            ("miter_incarnon", "independent", 1.0),
            ("mk1_paris", "additive_with_base_damage", 0.5),
            ("paris", "additive_with_base_damage", 0.5),
            ("paris_incarnon", "independent", 1.0),
            ("paris_prime", "additive_with_base_damage", 0.5),
            ("paris_prime_incarnon", "independent", 1.0),
            ("shedu", "independent", 1.0),
            // The row is `Blob Impact | 0% | Does not apply`, and its unmodded 4
            // names the BASE form (the Incarnon deals 50).
            ("stug", "inert", 1.0),
            ("torid", "independent", 1.0),
        ];

        let mut unexpected = Vec::new();
        let mut wrong = Vec::new();
        for s in all() {
            let beh = s.co_behavior.as_deref().unwrap_or("additive_with_base_damage");
            let frac = s.co_base_fraction.unwrap_or(1.0);
            let ordinary = beh == "additive_with_base_damage" && (frac - 1.0).abs() < 1e-9;
            match NAMED.iter().find(|(id, ..)| *id == s.id) {
                None if !ordinary => unexpected.push(format!("{} = {beh} x{frac}", s.id)),
                Some((_, b, f)) if beh != *b || (frac - f).abs() > 1e-9 => {
                    wrong.push(format!("{}: {beh} x{frac}, catalog says {b} x{f}", s.id));
                }
                _ => {}
            }
        }
        assert!(
            unexpected.is_empty(),
            "CO anomaly on an entry the catalog does not name — check the row for THIS              weapon's own name, or make it ordinary: {unexpected:?}"
        );
        assert!(wrong.is_empty(), "CO anomaly disagrees with the catalog: {wrong:?}");

        // …and every listed entry still EXISTS, so a rename cannot quietly
        // empty this list.
        for (id, ..) in NAMED {
            assert!(all().iter().any(|s| s.id == *id), "no weapon entry {id}");
        }

        // AN AoE PART TAKES NO CO unless its own row says so — "只对direct".
        // Named, not counted, for the same reason the list above is.
        const RADIAL_CO: &[&str] = &[
            // Braton / Mk1 / Prime / Vandal — Incarnon Form Radial Attack
            "braton_incarnon", "mk1_braton_incarnon", "braton_prime_incarnon",
            "braton_vandal_incarnon",
            // Burston / Burston Prime — Incarnon Form Radial Attack
            "burston_incarnon", "burston_prime_incarnon",
            // Zylok / Zylok Prime — Incarnon Form Radial Attack
            "zylok_incarnon", "zylok_prime_incarnon",
        ];
        let mut radial_co: Vec<&str> = all()
            .iter()
            .filter(|s| s.attack.radial.as_ref().is_some_and(|r| r.takes_condition_overload))
            .map(|s| s.id.as_str())
            .collect();
        radial_co.sort_unstable();
        let mut want = RADIAL_CO.to_vec();
        want.sort_unstable();
        assert_eq!(radial_co, want, "a radial takes CO only where the catalog names it");

        // …and the Torid's cloud, which is a FIELD rather than an explosion —
        // its own row ("Toxin AoE Cloud", Multiplying) and its own flag. The
        // distinction matters here because it is why this roster has EIGHT
        // radials taking CO and NINE AoE parts that do.
        let field_co: Vec<&str> = all()
            .iter()
            .filter(|s| s.attack.lingering.as_ref().is_some_and(|f| f.takes_condition_overload))
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(field_co, ["torid"], "a lingering field likewise");
    }
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
            let mut p = DummyParams::from_panel(&panel, &crate::arena::Arena::training(secs), &crate::arcanes_data::ArcaneFx::none());
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
            DummyParams::from_panel(&panel, &crate::arena::Arena::training(60.0), &crate::arcanes_data::ArcaneFx::none())
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
            let mut p = DummyParams::from_panel(&panel, &crate::arena::Arena::training(3600.0), &crate::arcanes_data::ArcaneFx::none());
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
                exclusive_to: &[],
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
        let mut p = DummyParams::from_panel(&panel, &crate::arena::Arena::training(120.0), &crate::arcanes_data::ArcaneFx::none());
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
        let mut q = DummyParams::from_panel(&tp, &crate::arena::Arena::training(120.0), &crate::arcanes_data::ArcaneFx::none());
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
        // 600 s, not 300: the fixture has to actually EXHAUST the reserve to
        // say anything, and after the transform stopped skipping the completing
        // shot's interval (2026-08-10) a 300 s cycle no longer burned the Boar
        // Prime's supply — both runs fired 1951 shots and the assertion below
        // compared a number to itself.
        let arena = crate::arena::Arena::training(600.0);
        let panel = |id| resolve(&WeaponBase::from_data(id, true, &[]), &[], StackPolicy::Emergent);
        let inc = panel("boar_prime_incarnon");
        let base = panel("boar_prime");
        let mk = |infinite| {
            let mut p = DummyParams::incarnon_cycle_from_panels(
                &inc, &base, false, LockMode::Initial(0), &arena, &crate::arcanes_data::ArcaneFx::none());
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
        // STILL UNVERIFIED (MEASUREMENTS M15), but no longer a coin flip:
        // an explosion is a damage instance WITH FALLOFF, and the wiki denies
        // this sphere both ("not a separate damage instance from the beam").
        // A node sphere would need a falloff nothing documents. Flipped from
        // the 2026-07-30 `true` on that argument (user, 2026-08-06); the Y=1
        // protocol in M15 is what would actually close it.
        assert!(!bm.chain_nodes_have_radius, "one sphere, at the beam's contact point");
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
            // A GAUGE-SWITCHED FORM IS EXEMPT, and only that. It carries its
            // own kind — the form vocabulary answers "which form of this
            // weapon", and `incarnon` already answers it — so a form that
            // happens to draw is not thereby the charged form. The Dread's
            // Incarnon form draws for 0.6 s and is not what the arsenal means
            // by "charged Dread". Everything else still has to agree: a
            // `charge` trigger filed as `base` is a data error.
            assert_eq!(
                charge_trigger && !kind.is_gauge_switched(),
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

    /// A WEAPON'S FORCED PROCS REACH THE SIM.
    ///
    /// `DummyParams::forced_procs` has existed since the Astilla was written up
    /// in MECHANICS §6, and the panel filled it with an empty vector — so the
    /// field was real, the sim read it, and no weapon could ever put anything
    /// in it. Phantasma Prime's charged form is the first that needs to:
    /// "Plasma bomb and seeking projectiles have a guaranteed Impact proc."
    ///
    /// Asserted at BOTH ends, because either alone passes on a broken path: the
    /// weapon file says Impact, and the RESOLVED panel still says Impact after
    /// the mod layer has been through it.
    #[test]
    fn a_weapons_forced_procs_survive_resolution() {
        let base = WeaponBase::from_data("phantasma_prime_charged", false, &[]);
        assert_eq!(base.forced_procs, vec![crate::damage::DamageType::Impact]);

        let panel = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::AssumedMax);
        assert_eq!(
            panel.forced_procs,
            vec![crate::damage::DamageType::Impact],
            "a forced proc is the weapon's, so no mod bucket may drop it"
        );

        // ...and the BEAM form forces nothing, so this is not a weapon-wide
        // flag wearing an attack's name.
        assert!(
            WeaponBase::from_data("phantasma_prime", false, &[]).forced_procs.is_empty(),
            "the beam has no guaranteed proc; only the charged bomb does"
        );
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
            DummyParams::from_panel(&p, &crate::arena::Arena { body_parts: parts, ..crate::arena::Arena::training(10.0) }, &crate::arcanes_data::ArcaneFx::none());
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
        let params = DummyParams::from_panel(&p, &crate::arena::Arena { target, body_parts: parts, ..crate::arena::Arena::training(30.0) }, &crate::arcanes_data::ArcaneFx::none());
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
                DummyParams::from_panel(&p, &crate::arena::Arena { body_parts: parts.clone(), ..crate::arena::Arena::training(20.0) }, &crate::arcanes_data::ArcaneFx::none());
            (!params.stacking_buffs.is_empty(), monte_carlo(&params, 40, 11).mean_effective_damage)
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
                DummyParams::from_panel(&p, &crate::arena::Arena { body_parts: parts.clone(), ..crate::arena::Arena::training(60.0) }, &crate::arcanes_data::ArcaneFx::none());
            let m = monte_carlo(&params, 24, 7);
            (!params.stacking_buffs.is_empty(), m.mean_effective_damage)
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
                &crate::arcanes_data::ArcaneFx::none(),
            );
            for b in d.stacking_buffs.iter_mut() {
                if b.grant != crate::loadout::BuffGrant::ReloadSpeed {
                    continue;
                }
                if pin {
                    // Full AND never expiring — the two knobs are separate,
                    // and this test wants both held for the whole run.
                    b.initial_stacks = b.max_stacks;
                    b.duration = crate::loadout::NO_TIMEOUT;
                } else {
                    b.initial_stacks = 0;
                }
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
                    DummyParams::from_panel(&p, &crate::arena::Arena { body_parts: parts.clone(), ..crate::arena::Arena::training(20.0) }, &crate::arcanes_data::ArcaneFx::none());
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
                DummyParams::from_panel(&p, &crate::arena::Arena { body_parts: parts.clone(), ..crate::arena::Arena::training(20.0) }, &crate::arcanes_data::ArcaneFx::none());
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

/// The Burston Incarnon's radial — the roster's only entry in the CO catalog
/// whose row constrains an EVOLUTION, so it is pinned here rather than left to
/// `evolutions_data::apply`'s comment. The row:
///
///   Burston/Burston Prime | Incarnon Form Radial Attack | AoE |
///     Attack Damage 55 | CO Damage Bonus at +100% 13 | 24% | Adding
///     "Radial hit only receives CO bonus on target directly hit by bullet.
///      AoE does not scale off multishot."
#[cfg(test)]
mod burston_incarnon_radial_tests {
    use super::*;

    #[test]
    fn the_incarnon_form_is_two_damage_instances() {
        let b = WeaponBase::from_data("burston_prime_incarnon", true, &[]);
        assert_eq!(b.base_vector.total(), 13.0, "direct hit is 13 Heat");
        let r = b.radial.as_ref().expect("the Incarnon form declares a radial");
        assert_eq!(r.base_vector.total(), 13.0, "the explosion is 13 Heat too");
        assert_eq!(r.radius_m, 2.0);
        assert!(r.takes_condition_overload, "the catalog row grants it");
        assert!(!r.takes_multishot, "\"AoE does not scale off multishot\"");
    }

    /// 55 = 13 + 42, and 13/55 = the 24% the catalog's third column prints:
    /// the explosion TAKES the tier-2 evolution's flat damage but does not
    /// take it into the base its CO term multiplies. Both tier-2 options give
    /// the same +42, so both must land the same radial.
    #[test]
    fn a_flat_damage_evolution_raises_the_explosion_but_not_its_co_base() {
        for evo in ["burston_prime_forceful_finality", "burston_prime_fortress_salvo"] {
            let b = WeaponBase::from_data("burston_prime_incarnon", true, &[evo]);
            let r = b.radial.as_ref().expect("the radial survives an evolution");
            assert!(
                (r.base_vector.total() - 55.0).abs() < 1e-9,
                "{evo}: radial should evolve to 55, got {}",
                r.base_vector.total()
            );
            assert!(
                (r.co_base_fraction - 13.0 / 55.0).abs() < 1e-9,
                "{evo}: the explosion's CO base stays 13/55, got {}",
                r.co_base_fraction
            );
            // The DIRECT hit has no catalog row, so it is not discrepant: its
            // CO computes on the full evolved base, which is the normal rule.
            assert!((b.base_vector.total() - 55.0).abs() < 1e-9);
            assert!((b.co_base_fraction - 1.0).abs() < 1e-9);
        }
    }

    /// Both instances land on the SAME enemy, and multishot moves only one of
    /// them: `takes_multishot: false` means the explosion fires once per pull
    /// while the direct hit fires once per pellet.
    #[test]
    fn multishot_multiplies_the_direct_hit_and_not_the_explosion() {
        use crate::dummy::{monte_carlo, DummyParams};
        let b = WeaponBase::from_data("burston_prime_incarnon", true, &[]);
        let body = || {
            vec![crate::dummy::BodyPart {
                name: "body".into(),
                aim_weight: 1.0,
                multiplier: 1.0,
                is_head: false,
                crit_bonus: false,
            }]
        };
        let pool = crate::mods_data::pool_for_weapon("burston_prime_incarnon");
        let sim = |mods: &[&crate::loadout::ModDef]| {
            let p = crate::loadout::resolve(&b, mods, crate::loadout::StackPolicy::AssumedMax);
            let params = DummyParams::from_panel(
                &p,
                &crate::arena::Arena {
                    body_parts: body(),
                    ..crate::arena::Arena::training(30.0)
                },
                &crate::arcanes_data::ArcaneFx::none(),
            );
            monte_carlo(&params, 30, 11).source_damage
        };
        let bare = sim(&[]);
        assert!(bare.direct > 0.0 && bare.radial > 0.0, "both instances land: {bare:?}");

        let split = pool.iter().find(|m| m.id == "split_chamber").expect("split_chamber");
        let ms = sim(&[split]);
        assert!(
            ms.direct > bare.direct * 1.5,
            "+90% multishot must grow the direct hit: {} -> {}",
            bare.direct,
            ms.direct
        );
        assert!(
            (ms.radial / bare.radial - 1.0).abs() < 0.1,
            "the explosion must not follow it: {} -> {}",
            bare.radial,
            ms.radial
        );
    }
}

/// HOW AN INCARNON GAUGE FILLS, on the real weapons rather than on a fixture.
///
/// `charge_on` is weapon data and `dummy` already tells the two rules apart —
/// but the only test of it built its own `DummyParams` by hand, so it proved
/// the shot loop honours the flag and never asked whether any weapon in the
/// roster carries the right one. The difference is not a matter of speed: at a
/// 0% headshot rate a weakpoint-charged weapon never transforms at all, which
/// is the largest thing an Incarnon weapon can do decided by one field.
#[cfg(test)]
mod incarnon_gauge_tests {
    use super::*;
    use crate::dummy::{monte_carlo, DummyParams};

    /// Transformations in a fixed engagement at a given headshot rate.
    fn transforms(weapon: &str, evo: &str, headshot_pct: f64) -> u32 {
        let base = WeaponBase::from_data(weapon, true, &[evo]);
        let form = crate::weapons_data::spec(weapon)
            .and_then(|s| s.transforms_to.clone())
            .expect("a cycling weapon names its second form");
        let inc = WeaponBase::from_data(&form, true, &[evo]);
        let policy = crate::loadout::StackPolicy::Emergent;
        let p0 = crate::loadout::resolve(&base, &[], policy);
        let p1 = crate::loadout::resolve(&inc, &[], policy);
        // The head takes every shot or none of them, which is what makes this
        // a test of the CHARGE RULE rather than of the aim model.
        let parts = vec![
            crate::dummy::BodyPart {
                name: "head".into(), aim_weight: headshot_pct / 100.0,
                multiplier: 3.0, is_head: true, crit_bonus: true,
            },
            crate::dummy::BodyPart {
                name: "body".into(), aim_weight: 1.0 - headshot_pct / 100.0,
                multiplier: 1.0, is_head: false, crit_bonus: false,
            },
        ];
        let arena = crate::arena::Arena { body_parts: parts, ..crate::arena::Arena::training(120.0) };
        // Frenzy off and earned-from-zero: neither weapon here has the passive,
        // and pinning it keeps this a test of the GAUGE.
        let params = DummyParams::incarnon_cycle_from_panels(
            &p1, &p0, false, crate::dummy::LockMode::Initial(0), &arena,
            &crate::arcanes_data::ArcaneFx::none(),
        );
        monte_carlo(&params, 5, 3).mean_transforms.round() as u32
    }

    #[test]
    fn a_direct_hit_gauge_does_not_need_headshots() {
        // The Torid: "Angstrum Incarnon Genesis and Torid Incarnon Genesis are
        // instead charged through direct hits" (wiki Incarnon).
        let none = transforms("torid", "torid_evo1_incarnon_form", 0.0);
        let all = transforms("torid", "torid_evo1_incarnon_form", 100.0);
        assert!(none > 0, "the Torid must transform with no headshots at all, got {none}");
        assert!(all > 0, "and still does when every shot is a headshot, got {all}");
    }

    #[test]
    fn a_weakpoint_gauge_does() {
        // The control, and the reason the Torid's rule needs its own field: on
        // a Zariman-rule weapon a body-only engagement never transforms.
        let none = transforms("burston_prime", "burston_prime_evo1_incarnon_form", 0.0);
        let all = transforms("burston_prime", "burston_prime_evo1_incarnon_form", 100.0);
        assert_eq!(none, 0, "no headshots, no Incarnon form");
        assert!(all > 0, "and with headshots it gets there, got {all}");
    }
}

/// THE MODE TABLE, DERIVED — and it must stay derived.
///
/// One row per weapon on a board is not enough: a Torid played through its
/// Incarnon cycle and a Torid that never transmutes are two different weapons
/// to hold, and only one of them was ever measurable. What each weapon offers
/// falls out of the forms it registers plus one question about the second one —
/// does entering it cost a meter you have to earn?
#[cfg(test)]
mod play_mode_tests {
    use super::*;

    /// Every weapon can be played in its arsenal form, and that always counts.
    #[test]
    fn every_weapon_has_a_base_mode_and_it_is_always_rankable() {
        for w in roster() {
            let ms = play_modes(&w.id);
            let base = ms.iter().find(|m| m.mode == PlayMode::Base);
            let base = base.unwrap_or_else(|| panic!("{}: no base mode", w.id));
            assert!(base.sustainable, "{}: its own arsenal form is not rankable", w.id);
            assert_eq!(
                ms.iter().filter(|m| m.mode == PlayMode::Base).count(),
                1,
                "{}: more than one base mode", w.id
            );
        }
    }

    /// A GAUGE IS WHAT DECIDES IT, and the two shapes are exactly these.
    ///
    /// A second form you pay a meter for gives a CYCLE that a board can rank
    /// and an always-in-it mode that it cannot — "always Incarnon" is not a
    /// playstyle, it is a few seconds at a time. A second form that is only a
    /// different trigger pull can be held forever, so it is rankable and there
    /// is no cycle to run.
    #[test]
    fn a_gauge_gives_a_cycle_and_costs_the_alternate_its_rank() {
        for w in roster() {
            let forms = forms_of(&w.id);
            let ms = play_modes(&w.id);
            let alt = forms.iter().find(|f| !f.is_default);
            let Some(alt) = alt else {
                assert_eq!(ms.len(), 1, "{}: one form, so one mode", w.id);
                continue;
            };
            let _ = alt;
            // EVERY alternate form gets its own mode, and a weapon may have
            // more than one: a bow with an adapter has a tapped shot and an
            // Incarnon form.
            let alts: Vec<_> = forms.iter().filter(|f| !f.is_default).collect();
            let gauged = |f: &FormRef| spec(f.weapon_id).is_some_and(|s| s.incarnon.is_some());
            let any_gauged = alts.iter().any(|f| gauged(f));
            let has = |m: PlayMode| ms.iter().any(|x| x.mode == m);
            let rankable = |m: PlayMode| ms.iter().any(|x| x.mode == m && x.sustainable);

            assert_eq!(
                ms.len(), 1 + alts.len() + usize::from(any_gauged),
                "{}: {} forms should give base + one mode each + a cycle: {:?}",
                w.id, forms.len(), ms.iter().map(|m| m.id).collect::<Vec<_>>()
            );
            assert_eq!(has(PlayMode::Cycle), any_gauged, "{}: cycle iff gauge", w.id);
            assert_eq!(has(PlayMode::Transformed), any_gauged, "{}: gauge-fed mode iff gauge", w.id);
            assert_eq!(
                has(PlayMode::Alternate), alts.iter().any(|f| !gauged(f)),
                "{}: a free second form is an alternate", w.id
            );
            // …and the ids are DISTINCT, which is the whole reason the gauged
            // one is its own mode: a build names a mode by id.
            let mut ids: Vec<&str> = ms.iter().map(|m| m.id).collect();
            let n = ids.len();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), n, "{}: two modes share an id: {ids:?}", w.id);

            assert!(!rankable(PlayMode::Transformed),
                "{}: a gauge-fed form cannot be held for an engagement", w.id);
            assert_eq!(rankable(PlayMode::Alternate), has(PlayMode::Alternate),
                "{}: a free form can be held for one", w.id);
        }
    }

    /// A MODE IS TRANSLATED AT ONE BOUNDARY, into the fight parser's own
    /// vocabulary — form KINDS, plus the single policy word for the cycle.
    #[test]
    fn each_mode_names_the_form_a_fight_would_have_to_run() {
        let f = |w: &str, m: PlayMode| {
            play_modes(w).into_iter().find(|x| x.mode == m).map(|x| x.form())
        };
        // The gauge weapon: never transmute, or run the cycle.
        assert_eq!(f("torid", PlayMode::Base), Some("base"));
        assert_eq!(f("torid", PlayMode::Cycle), Some("incarnon_cycle"));
        // TRANSFORMED, not Alternate: being in the Incarnon form for a
        // whole engagement is a thing the builder can show and not a
        // thing a ruler ranks.
        assert_eq!(f("torid", PlayMode::Transformed), Some("incarnon"));
        assert_eq!(f("torid", PlayMode::Alternate), None, "the Torid has no free second form");
        // A bow with an adapter has BOTH, which is why they are two modes.
        assert_eq!(f("paris", PlayMode::Base), Some("charged"));
        assert_eq!(f("paris", PlayMode::Alternate), Some("base"), "the tapped shot");
        assert_eq!(f("paris", PlayMode::Transformed), Some("incarnon"));
        assert_eq!(f("paris", PlayMode::Cycle), Some("incarnon_cycle"));
        // The free one, where `base` mode is the CHARGED form because that is
        // what the arsenal hands you — the mode is named for its role, not for
        // the form's own name.
        assert_eq!(f("cernos_prime", PlayMode::Base), Some("charged"));
        assert_eq!(f("cernos_prime", PlayMode::Alternate), Some("base"));
        assert_eq!(f("cernos_prime", PlayMode::Cycle), None);
    }

    /// The two weapons the owner named, spelled out — a derivation is worth
    /// nothing if it derives the wrong table.
    #[test]
    fn torid_and_cernos_prime_each_offer_two() {
        let on = |id: &str| -> Vec<&'static str> {
            play_modes(id).into_iter().filter(|m| m.sustainable).map(|m| m.mode.id()).collect()
        };
        // The gauge one: fill it and spend it, or never transmute at all.
        assert_eq!(on("torid"), vec!["base", "cycle"]);
        // The free one: charge every arrow, or none of them.
        assert_eq!(on("cernos_prime"), vec!["base", "alternate"]);
        // ...and a weapon with one form offers one.
        assert_eq!(on("ocucor"), vec!["base"]);
    }

    /// A cycle names BOTH ends, because it is the only mode that is about two
    /// forms rather than one.
    #[test]
    fn a_cycle_carries_the_form_it_returns_to_and_the_one_it_spends() {
        let c = play_modes("torid")
            .into_iter()
            .find(|m| m.mode == PlayMode::Cycle)
            .expect("the Torid has a cycle");
        assert_eq!(c.weapon_id, "torid");
        assert_eq!(c.other_id, Some("torid_incarnon"));
        // Every other mode is about one form and says so.
        for m in play_modes("cernos_prime") {
            assert_eq!(m.other_id, None, "{} names a second form", m.mode.id());
        }
    }

    /// A weapon that falls off with range is simulated at point blank, and it
    /// has to SAY so — on the page, not only in the file.
    ///
    /// The arena has no distance, so `falloff:` is data nothing in the fight
    /// reads: a Boar's number is its 0-15 m number and a player comparing it
    /// to a rifle is comparing a best case to a flat one. The line is derived
    /// from the field rather than remembered per weapon, so a weapon that
    /// gains a falloff tomorrow cannot gain it quietly.
    #[test]
    fn a_weapon_with_damage_falloff_says_it_is_not_modelled() {
        let mut with = 0;
        for w in all() {
            let Some(f) = &w.attack.falloff else { continue };
            with += 1;
            assert!(f.end_m > f.start_m, "{}: falloff {f:?} does not span", w.id);
            // `reduction` is the fraction KEPT, so a weapon that keeps all of
            // its damage has no falloff and should not be carrying the field.
            assert!(f.reduction < 1.0 && f.reduction > 0.0, "{}: keeps {}", w.id, f.reduction);
            assert!(
                w.unmodeled.iter().any(|u| u.contains("falloff")),
                "{} falls off from {} m and its `unmodeled:` never mentions it",
                w.id, f.start_m
            );
        }
        // The Boar is the shape this exists for: hit-scan, and its damage is
        // halved past 25 m.
        let boar = spec("boar").unwrap().attack.falloff.as_ref().unwrap();
        assert_eq!((boar.start_m, boar.end_m, boar.reduction), (15.0, 25.0, 0.5));
        assert!(with >= 10, "only {with} weapons carry a falloff");
    }

    /// FIRESTORM REACHES EVERY EXPLOSION BUT THE SHEDU'S.
    ///
    /// *"Explosion cannot benefit from Firestorm (Primed) despite being area of
    /// effect"* (wiki Shedu, verbatim) — the roster's only exception, and the
    /// owner asked for it to be confirmed rather than assumed (2026-08-11).
    ///
    /// The same weapon's OTHER AoE goes the other way: its battery discharge
    /// *"is affected by base damage, Faction Damage Bonus, and Firestorm /
    /// Primed Firestorm"*. So the blast-radius bucket reaches exactly the one
    /// radius Primary Compression is forbidden to spend ("cannot use reload
    /// pulse radial"), and the arcane is stuck at the shot's unmoddable 6.6 m.
    ///
    /// It changes no damage while the arena has one target, which is precisely
    /// why it needs a test: nothing else would notice it going wrong.
    #[test]
    fn only_the_shedus_explosion_refuses_the_blast_radius_bucket() {
        let firestorm = crate::mods_data::class_pool("rifle")
            .into_iter()
            .find(|m| m.id == "primed_firestorm")
            .expect("primed firestorm");
        let radius = |id: &str, mods: &[&crate::loadout::ModDef]| {
            let base = crate::loadout::WeaponBase::from_data(id, true, &[]);
            crate::loadout::resolve(&base, mods, crate::loadout::StackPolicy::Emergent)
                .radial
                .map(|r| r.radius_m)
        };
        // THE SHEDU: 6.6 m with the mod and without it.
        assert_eq!(radius("shedu", &[]), Some(6.6));
        assert_eq!(radius("shedu", &[&firestorm]), Some(6.6), "Firestorm must not reach it");
        // THE TORID, the same pool and the same mod, moves: its cloud is a
        // `lingering` rather than a radial, so the DIRECT comparison is another
        // radial weapon that does take the bucket.
        let laetum = radius("laetum_incarnon", &[]);
        assert!(laetum.is_some(), "the Laetum's Incarnon form explodes");
        // …and the flag is declared exactly once in the whole roster.
        let refusing: Vec<&str> = all()
            .iter()
            .filter(|w| w.attack.radial.as_ref().is_some_and(|r| !r.takes_blast_radius_mods))
            .map(|w| w.id.as_str())
            .collect();
        assert_eq!(refusing, ["shedu"], "a second weapon started refusing the bucket");
    }

    /// EVERY COMPRESSION ROW IS TRANSCRIBED, AND EVERY ONE IS SAYABLE.
    ///
    /// The table is the arcane's whole per-weapon behaviour (docs/CATALOGS.md
    /// §2) and it is copied by hand, so this asserts the shape rather than
    /// trusting the copy: a stacking class the engine has no bracket for, or an
    /// effectiveness outside the published range, is a transcription error and
    /// not a weapon that behaves strangely.
    ///
    /// The published range is not [0, 1]: the Vectis pair are 0.04 and the
    /// Trumna's alt-fire is 1.27 ("Merged"), so the bound is generous on
    /// purpose — what it catches is a percent written as 100 instead of 1.0.
    #[test]
    fn the_roster_reproduces_primary_compressions_published_column() {
        // The table's "Max Damage Bonus @ Base Radius" — the wiki's own
        // arithmetic on its own numbers, and a column we never transcribed:
        // it falls out of the radius, the row and the rank ramp. So it is a
        // CROSS-CHECK rather than a restatement — a radius typed wrong, an
        // effectiveness misread, an override invented, and this stops matching.
        let table: &[(&str, f64)] = &[
            ("shedu", 5.28),
            ("torid", 2.40),              // the Toxin cloud
            ("torid_incarnon", 0.0),      // "Doesn't Work" — the beam exclusion
            ("braton_incarnon", 2.40),
            ("braton_prime_incarnon", 2.40),
            ("braton_vandal_incarnon", 2.40),
            ("mk1_braton_incarnon", 2.40),
            ("burston_incarnon", 1.60),
            ("burston_prime_incarnon", 1.60),
            ("gorgon_incarnon", 4.00),
            ("gorgon_wraith_incarnon", 4.00),
            ("prisma_gorgon_incarnon", 4.00),
            ("latron_incarnon", 3.20),
            ("latron_prime_incarnon", 3.20),
            ("miter_incarnon", 2.40),
            ("strun_incarnon", 3.20),
            ("strun_prime_incarnon", 3.20),
            ("strun_wraith_incarnon", 3.20),
            ("phantasma_charged", 3.84),
            ("phantasma_prime_charged", 3.84),
            // THE ONE OVERRIDE: 0.8 x 0.1 m, not 0.8 x 6.7 m x 4%.
            ("vectis_incarnon", 0.08),
            ("vectis_prime_incarnon", 0.08),
        ];
        // At rank 5 a metre is worth +100%, so the bonus IS the metres lost.
        let fx = crate::arcanes_data::for_slot("primary", "primary_compression")
            .expect("the arcane is in the primary pool")
            .fx(5, crate::loadout::StackPolicy::Emergent, &[], crate::tenno_data::default_tenno());
        assert_eq!(fx.compression_dmg_per_m, 1.0, "+100% per metre at max rank");
        for (id, expected) in table {
            let base = crate::loadout::WeaponBase::from_data(id, true, &[]);
            let p = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent);
            let bonus = p.compression.map_or(0.0, |c| c.radius_lost) * fx.compression_dmg_per_m;
            assert!(
                (bonus - expected).abs() < 5e-3,
                "{id}: the table says +{}%, this build pays +{:.1}%",
                expected * 100.0, bonus * 100.0
            );
        }
        // …and every row the roster carries is in the list above, so a new
        // weapon cannot join the catalog without its column being checked.
        // `all()`, not `roster()`: a compression row belongs to an ATTACK, and
        // most of these are Incarnon FORMS, which are entries of their own.
        let carried: Vec<&str> = all()
            .iter()
            .filter(|w| w.attack.compression.is_some())
            .map(|w| w.id.as_str())
            .collect();
        for id in &carried {
            assert!(table.iter().any(|(t, _)| t == id), "{id} has a row and no expected bonus");
        }
        assert_eq!(carried.len(), table.len());
    }

    /// AIMING IS THE WHOLE CONDITION — *"On aim: x0.2 explosion radius"*.
    #[test]
    fn compression_is_worth_nothing_to_a_player_who_is_not_aiming() {
        let base = crate::loadout::WeaponBase::from_data("shedu", true, &[]);
        let mut hipfire = crate::tenno_data::default_tenno().clone();
        hipfire.state.aiming = false;
        let aimed = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent);
        let hip = crate::loadout::resolve_for(
            &base, &[], crate::loadout::StackPolicy::Emergent, &hipfire,
        );
        assert!(aimed.compression.is_some_and(|c| c.radius_lost > 5.27));
        assert!(hip.compression.is_none(), "no aim, no trade, no bonus");
    }

    #[test]
    fn every_compression_row_is_one_the_engine_could_apply() {
        let mut rows = 0;
        let mut adds = 0;
        for w in all() {
            let Some(c) = &w.attack.compression else { continue };
            rows += 1;
            assert!(
                matches!(c.stacking.as_str(), "multiplies" | "adds"),
                "{}: `{}` is not a stacking class",
                w.id, c.stacking
            );
            assert!(
                (0.0..=1.5).contains(&c.effectiveness),
                "{}: effectiveness {} — a percent written as a whole number?",
                w.id, c.effectiveness
            );
            // THE THIRD COLUMN, and the legend's own vocabulary. `Snapshot` is
            // the ordinary value; the other three each mean the arcane reads a
            // radius that is not this attack's own, which is the part a
            // transcription flattens into "it works".
            assert!(
                matches!(
                    c.radius_calculation.as_str(),
                    "snapshot" | "stolen" | "doesnt_work" | "constant_check"
                ),
                "{}: `{}` is not a Radius Calculation",
                w.id, c.radius_calculation
            );
            // An OVERRIDE is only ever the reason a row's radius is not the
            // attack's, so it must not also be at full effectiveness — that
            // pair would be two answers to one question.
            assert!(
                c.reads_radius_m.is_none() || c.effectiveness != 1.0,
                "{}: reads_radius_m with 100% effectiveness — which one is the radius?",
                w.id
            );
            // …and the two that must agree: an effectiveness of zero IS
            // "Doesn't Work", spelled in the other column.
            assert_eq!(
                c.effectiveness == 0.0,
                c.radius_calculation == "doesnt_work",
                "{}: effectiveness {} against radius calculation `{}`",
                w.id, c.effectiveness, c.radius_calculation
            );
            if c.stacking == "adds" {
                adds += 1;
            }
        }
        assert!(rows >= 20, "only {rows} rows transcribed");
        // THE MINORITY IS REAL, and it is the half of the table most likely to
        // be flattened by a copy: every Braton and Burston Incarnon ADDS.
        assert_eq!(adds, 6, "the Braton four and the Burston two add, and nothing else here does");
        // …and a tested ZERO is not the same as an absent row. The Torid's
        // Incarnon form has one, and its base form is 100% — one arcane, two
        // answers, inside one weapon's cycle.
        assert_eq!(spec("torid_incarnon").unwrap().attack.compression.as_ref().unwrap().effectiveness, 0.0);
        assert_eq!(spec("torid").unwrap().attack.compression.as_ref().unwrap().effectiveness, 1.0);
        // The Vectis pair are the reason the range is not a boolean.
        assert_eq!(spec("vectis_incarnon").unwrap().attack.compression.as_ref().unwrap().effectiveness, 0.04);
    }

    /// THE TORID'S CO CATALOG ROWS, pinned — both of them, and both forms.
    ///
    /// The wiki's Condition Overload catalog gives this weapon TWO rows, and
    /// the owner supplied them verbatim (2026-08-10):
    ///
    /// > Torid | Main-fire | Projectile | 100 | 100 | 100% | Multiplying
    /// > Torid | Toxin AoE Cloud | AoE | 40 | 40 | 100% | Multiplying
    ///
    /// Three facts are load-bearing and none of them is the default:
    ///
    /// 1. MULTIPLYING, which is `Independent` here — a free-standing
    ///    `x (1 + co x types)` rather than a share of the base-damage bucket.
    /// 2. THE CLOUD TAKES IT TOO. CO is a direct-hit bonus everywhere else, so
    ///    an AoE part getting it is the anomaly the catalog exists to record.
    /// 3. THE INCARNON FORM DOES NOT. It is ordinary additive, which means one
    ///    weapon's two forms disagree — exactly the shape a refactor flattens
    ///    without anyone noticing, since both would still "have CO".
    ///
    /// The base fractions are 100% on both rows, which is the default, so they
    /// are asserted rather than declared in the files.
    #[test]
    fn the_torid_carries_both_of_its_co_catalog_rows() {
        use crate::loadout::CoBehavior;
        let base = spec("torid").expect("torid");
        let inc = spec("torid_incarnon").expect("torid_incarnon");

        // 1. MAIN FIRE: 100 Toxin, multiplying, on the whole base.
        assert_eq!(base.attack.damage.get("toxin").copied(), Some(100.0));
        assert_eq!(base.co_behavior.as_deref(), Some("independent"));
        assert_eq!(base.co_base_fraction.unwrap_or(1.0), 1.0);

        // 2. THE CLOUD: 40 Toxin, and it takes CO — the anomaly.
        let cloud = base.attack.lingering.as_ref().expect("the Torid's cloud");
        assert_eq!(cloud.damage.get("toxin").copied(), Some(40.0));
        assert!(
            cloud.takes_condition_overload,
            "the catalog gives the cloud its own Multiplying row"
        );

        // 3. THE INCARNON FORM IS ORDINARY, and has no cloud to argue about.
        assert_eq!(inc.co_behavior.as_deref(), Some("additive_with_base_damage"));
        assert!(inc.attack.lingering.is_none(), "the Incarnon form is a beam");

        // …and the two really do resolve to different brackets, which is the
        // claim rather than the spelling.
        let resolved = |id: &str| {
            crate::loadout::resolve(
                &crate::loadout::WeaponBase::from_data(id, true, &[]),
                &[],
                crate::loadout::StackPolicy::Emergent,
            )
            .co_behavior
        };
        assert_eq!(resolved("torid"), CoBehavior::Independent);
        assert_eq!(resolved("torid_incarnon"), CoBehavior::AdditiveWithBaseDamage);
    }

    /// EVERY SPOOL RECONCILES WITH ITS OWN PAGE, and says what it costs.
    ///
    /// The field is small and easy to typo into silence — serde ignores what it
    /// does not know — so every weapon's numbers are asserted by value. The
    /// risers get a second, stronger check: each page states its spool TWICE,
    /// as a percentage per shot and as a count of shots to optimal, and the two
    /// must agree. `over_shots` carries the span (the exact half) and this
    /// re-derives BOTH published figures from it, so a mistyped span cannot
    /// survive — it would have to be wrong in a way that keeps two independent
    /// sentences true.
    #[test]
    fn every_spool_reconciles_with_its_own_page() {
        // (weapon, start, end, over_shots, the page's "% per shot", its
        //  "N shots before optimal" — the last two are what the wiki prints.)
        let published: &[(&str, f64, f64, f64, f64, i64)] = &[
            ("phenmor_incarnon", 1.00, 0.60, 51.0, 0.0, 0),
            ("gorgon", 0.20, 1.00, 7.5, 0.10667, 9),
            ("gorgon_wraith", 0.20, 1.00, 5.0, 0.16, 6),
            ("prisma_gorgon", 0.20, 1.00, 6.0, 0.13333, 7),
            ("soma", 0.25, 1.00, 5.0, 0.15, 6),
            ("soma_prime", 0.25, 1.00, 2.5, 0.30, 4),
        ];
        for (id, start, end, over, per_shot, full_at) in published {
            let s = spec(id)
                .unwrap_or_else(|| panic!("{id}"))
                .attack
                .sustained_fire_rate
                .unwrap_or_else(|| panic!("{id} lost its spool"));
            assert_eq!((s.start, s.end, s.over_shots), (*start, *end, *over), "{id}");
            assert!(s.start > 0.0 && s.end > 0.0, "{id}");
            if *full_at == 0 {
                continue; // the faller: its page gives a span, not a count
            }
            // THE TWO PUBLISHED FIGURES, both from `over_shots`.
            assert!(
                ((s.end - s.start) / s.over_shots - per_shot).abs() < 5e-5,
                "{id}: {} a shot, page says {per_shot}",
                (s.end - s.start) / s.over_shots
            );
            assert_eq!(s.over_shots.ceil() as i64 + 1, *full_at, "{id}: full-at shot");
        }
        // …and every one of them owes the reader the play pattern it assumes.
        let mut with = 0;
        for w in all() {
            if w.attack.sustained_fire_rate.is_none() {
                continue;
            }
            with += 1;
            assert!(
                w.unmodeled.iter().any(|u| u.contains("spool")),
                "{} spools and its `unmodeled:` never says what the sim assumes",
                w.id
            );
        }
        assert_eq!(with, published.len(), "a spool with no published numbers above");
        // A FORM SPOOLS ON ITS OWN. The Phenmor's base form is semi-auto and has
        // no held trigger; the Gorgons' Incarnon forms are Auto Charge and their
        // pages say plainly that they do not spool.
        for id in ["phenmor", "gorgon_incarnon", "soma_incarnon", "prisma_gorgon_incarnon"] {
            assert!(spec(id).unwrap().attack.sustained_fire_rate.is_none(), "{id}");
        }
    }
}
