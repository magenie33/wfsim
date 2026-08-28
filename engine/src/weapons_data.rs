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
    ChargeOn, CoBehavior, FieldStacking, GaugeForm, LingeringBase, RadialBase, WeaponBase,
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
    /// THIS COLUMN'S DAMAGE, transcribed from the infobox tab rather than
    /// derived from the entry's.
    ///
    /// VERBATIM (wiki `Archgun`): *"most Heavy Weapons (a.k.a. Archguns when
    /// used via the Archgun Deployer) have had their damage doubled"* — so a
    /// ground Arch-Gun hits for twice what the same weapon does in an Archwing
    /// mission, and the two tabs of its infobox say so field for field.
    ///
    /// THIS FIELD EXISTS BECAUSE ITS ABSENCE WAS A WRONG NUMBER ON THE BOARD.
    /// The deployment axis was built as a SUSTAIN axis — reload, magazine,
    /// reserve, resupply — on a reading of the two-column table that said "same
    /// damage, same crit, same status, only the sustain differs". Crit,
    /// multiplier, status, fire rate and magazine ARE identical; the damage is
    /// not, and the Larkspur Prime was scored on the board at half its ground
    /// damage from the day it was added until 2026-08-14.
    ///
    /// BOTH COLUMNS ARE WRITTEN DOWN, and that is the point of stating a vector
    /// here instead of the `x0.5` that would produce the same numbers (owner,
    /// 2026-08-14). A multiplier is DERIVED: a reader of this file cannot see
    /// what the other tab says and cannot check it against the page, which is
    /// the one thing that would have caught the original error. The doubling is
    /// "most", not all, so the ratio is an observation about a weapon and never
    /// a rule to compute with — `deployment_tests` asserts which weapons
    /// actually follow it and NAMES the ones that do not.
    #[serde(default)]
    pub damage: Option<BTreeMap<String, f64>>,
    /// ...and the same tab's RADIAL, when the attack has one. Required
    /// alongside `damage` by `validate` rather than optional in practice: an
    /// explosion left on the other column is exactly the half-applied
    /// deployment this whole field exists to prevent.
    #[serde(default)]
    pub radial_damage: Option<BTreeMap<String, f64>>,
    /// ...and its lingering FIELD, under the same rule.
    #[serde(default)]
    pub lingering_damage: Option<BTreeMap<String, f64>>,
    /// THE STATS THAT ALSO MOVE. Damage is the field that cost something, but
    /// it is not the only one a tab can change: the Kuva Grattler's critical
    /// multiplier is 2.10x in Archwing and 2.00x on the ground, and nothing
    /// about "a deployment is a sustain axis" would have predicted that either.
    ///
    /// Only the three that reach a number are here. Blast RADIUS, projectile
    /// SPEED and falloff differ too on several Arch-Guns (the Kuva Ayanga's
    /// explosion is 9.0 m in space and 6.0 m on the ground, its grenade 300 m/s
    /// against 55) and none of them changes a number in an arena with one
    /// target and no distance — so those are transcribed into each weapon's
    /// comments and named in its `unmodeled:`, rather than carried as fields
    /// that would move nothing.
    #[serde(default)]
    pub crit_chance: Option<f64>,
    #[serde(default)]
    pub crit_multiplier: Option<f64>,
    #[serde(default)]
    pub status_chance: Option<f64>,
}

/// THE VALENCE BONUS an ADVERSARY weapon carries — a Kuva Lich's, a Sister's, a
/// Coda's — as the weapon declares what it CAN have.
///
/// VERBATIM (wiki, Kuva Weapons §Elemental Bonus): *"The Kuva weapons
/// additionally have bonus damage of one damage type which can either be
/// Impact, Heat, Cold, Electricity, Toxin, Magnetic, or Radiation, ranging from
/// 25-60% of the weapon's base damage determined randomly. … This additional
/// bonus damage applies as weapon base damage, meaning elemental mods and
/// status that scale from base / modified base damage will be affected."*
///
/// So it is not a bucket and not a buff: it is the WEAPON's own base vector,
/// which is why nothing downstream needs to know it exists. An innate element
/// already composes with the mod elements the way MECHANICS §3 rule 2 says, and
/// this arrives as one.
///
/// The SPEC is what a weapon may have; the CHOICE (which element, what
/// percentage) belongs to the build, because it is a property of the copy a
/// player owns rather than of the model — the same shape a riven has.
#[derive(Debug, Clone, Deserialize)]
pub struct ValenceSpec {
    /// The progenitor elements this weapon's bonus can be, in the wiki's order.
    pub elements: Vec<String>,
    /// The roll's floor and ceiling as fractions of base damage (0.25–0.60 on
    /// every Kuva weapon). Both are stated rather than assumed: a Tenet or Coda
    /// entry may differ and the page is the only thing that knows.
    pub min: f64,
    pub max: f64,
}

/// Apply a chosen VALENCE BONUS to a resolved base, in place.
///
/// `bonus` is a fraction of the base TOTAL, added as `element` — merged into
/// that element if the weapon already deals it, which is what a Radiation
/// progenitor on a Radiation weapon does. Written beside `apply_deployment`
/// because it is the same shape of thing: a per-request choice the base cannot
/// carry, applied once, at the one place a request builds its weapon.
///
/// Out of range is CLAMPED rather than refused: the roll's floor and ceiling
/// are the game's, and a request that asks for more gets the ceiling instead of
/// an error nobody can act on. A weapon with no spec is left alone.
pub fn apply_valence(base: &mut WeaponBase, id: &str, element: &str, bonus: f64) {
    let Some(s) = spec(id).and_then(|s| s.valence.as_ref()) else { return };
    if !s.elements.iter().any(|e| e == element) {
        return;
    }
    let Some(ty) = crate::damage::DamageType::from_name(element) else { return };
    let fraction = bonus.clamp(s.min, s.max);
    let total = base.base_vector.total();
    if total <= 0.0 || fraction <= 0.0 {
        return;
    }
    let add = total * fraction;
    base.base_vector = base.base_vector.with(ty, base.base_vector.get(ty) + add);
    // THE RADIAL TOO, on a weapon that has one: the bonus is base damage, and a
    // radial's base is base damage. None of today's adversary weapons in this
    // roster has one, which is exactly when to write the line — before a
    // weapon arrives that would have been silently wrong.
    if let Some(r) = base.radial.as_mut() {
        let rt = r.base_vector.total();
        if rt > 0.0 {
            let radd = rt * fraction;
            r.base_vector = r.base_vector.with(ty, r.base_vector.get(ty) + radd);
        }
    }
    // …AND A LINGERING FIELD, on the same argument and still on no weapon in
    // this roster. A field a MOD grants is the case that is real today, and it
    // cannot be reached from here — see `WeaponBase::valence_bonus`, which
    // `resolve_for` spends when it builds one.
    if let Some(l) = base.lingering.as_mut() {
        let lt = l.base_vector.total();
        if lt > 0.0 {
            let ladd = lt * fraction;
            l.base_vector = l.base_vector.with(ty, l.base_vector.get(ty) + ladd);
        }
    }
    // WHAT IS LEFT FOR A PART THAT DOES NOT EXIST YET.
    base.valence_bonus = fraction;
}

/// The valence spec of a weapon, if it is an adversary weapon at all.
pub fn valence_of(id: &str) -> Option<&'static ValenceSpec> {
    spec(id).and_then(|s| s.valence.as_ref())
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
    // EVERY ATTACK PART, because the infobox states every attack part. The
    // direct vector alone would leave an Arch-Gun's explosion on the other
    // column — which is the half-applied deployment `validate` refuses.
    let vector_of = |m: &std::collections::BTreeMap<String, f64>| {
        let mut v = crate::damage::DamageVector::new();
        for (name, amount) in m {
            v.add(damage_type(name), *amount);
        }
        v
    };
    if let Some(m) = d.damage.as_ref() {
        base.base_vector = vector_of(m);
    }
    if let Some(m) = d.radial_damage.as_ref() {
        if let Some(r) = base.radial.as_mut() {
            r.base_vector = vector_of(m);
        }
    }
    if let Some(m) = d.lingering_damage.as_ref() {
        if let Some(l) = base.lingering.as_mut() {
            l.base_vector = vector_of(m);
        }
    }
    // The radial inherits the direct part's crit and status unless it states
    // its own (RadialSpec), so a column that moves them moves both — which is
    // what the Kuva Grattler's two tabs actually do.
    if let Some(v) = d.crit_chance {
        base.base_crit_chance = v;
    }
    if let Some(v) = d.crit_multiplier {
        base.base_crit_damage = v;
    }
    if let Some(v) = d.status_chance {
        base.base_status_chance = v;
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

/// A RANGE, OR THE WORD FOR NOT HAVING ONE.
///
/// `range_m: 20.0` and `range_m: infinite` are both statements; leaving the
/// field out is not one, and the difference is the whole reason this is not
/// just an `f64` (owner, 2026-08-19: 无限射程应该是特殊的字段，这样才研究).
///
/// Absence has to keep meaning unlimited — 121 entries have never been
/// transcribed and must go on working — but it now means "nobody has looked"
/// rather than "there is no limit", and the two are separable by a script.
/// `weapons_data`'s own ratchet counts the entries that say NEITHER, so the
/// number can only be driven down.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum RangeSpec {
    /// Metres, from the wiki's Range stat.
    Metres(f64),
    /// The literal `infinite`, and nothing else — a typo must not silently
    /// become an unlimited weapon, which is what a bare string field would let
    /// it do.
    Word(String),
}

impl RangeSpec {
    pub fn metres(&self) -> f64 {
        match self {
            RangeSpec::Metres(m) => *m,
            RangeSpec::Word(w) if w == "infinite" => f64::INFINITY,
            // Loud rather than lenient: an unrecognised word is a data error,
            // and the alternative is a weapon that silently reaches forever.
            RangeSpec::Word(w) => panic!("range_m must be a number or `infinite`, got `{w}`"),
        }
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
    /// THIS ATTACK IS NOT AIMED, and this is the flat chance any one of its
    /// damage instances lands on a weak point.
    ///
    /// The scenario's `headshot_pct` is a statement about the PLAYER'S AIM, so
    /// it is the wrong number for an attack the player does not point: the
    /// Grimoire throws an orb that drifts and then *"shock[s] 1 enemy within 6
    /// meters of it every 1 second"* — a random body, of the game's choosing,
    /// six times (wiki `Grimoire`). [`RicochetSpec::headshot_chance`] reached
    /// the same conclusion first, for a bounce; this is the same idea one level
    /// up, where the WHOLE attack is unaimed rather than one of its parts.
    ///
    /// IT IS READ BY EVERY INSTANCE THE ATTACK PRODUCES — the collision and the
    /// `lingering:` field's ticks alike — which is the point of declaring it on
    /// the attack. The orb's six strikes are one mechanic and the owner says so
    /// outright (2026-08-28: the first is a field tick like the other five), so
    /// a per-part spelling would be six chances to make them differ.
    ///
    /// Its value here is ASSUMED rather than measured (owner: 0.1) and the
    /// weapon says so on its own page.
    #[serde(default)]
    pub unaimed_headshot_chance: Option<f64>,
    pub crit_chance: f64,
    pub crit_multiplier: f64,
    pub status_chance: f64,
    pub damage: BTreeMap<String, f64>,
    /// ONE PULL, ONE ELEMENT EACH — the innate element of every projectile
    /// this attack fires, in FIRING ORDER. The Arbucep's six homing missiles
    /// are Blast, Corrosive, Gas, Magnetic, Radiation and Viral, one apiece,
    /// fired together; `damage:` above is what ONE of them carries.
    ///
    /// WHY IT CANNOT BE ONE VECTOR. Six types in a single instance get the
    /// damage right and everything else wrong: a proc is drawn ONCE per
    /// instance weighted by share, so six missiles draw six procs and a
    /// blended one draws a single proc; crit is rolled per instance, so six
    /// rolls collapse into one; and each missile carries its own explosion of
    /// its own element. The panel therefore resolves ONCE PER ELEMENT and the
    /// fight picks by pellet index — see `loadout::ResolvedPanel::pellet_damage`.
    ///
    /// Its length IS the projectile count, so `multishot` beside it must agree.
    #[serde(default)]
    pub pellet_elements: Vec<String>,
    /// MULTISHOT THAT IS NOT MORE PROJECTILES. VERBATIM (wiki `Arbucep`):
    /// *"Multishot increases weapon damage instead of creating additional
    /// projectiles. Damage bonus is multiplicative to other sources of
    /// damage."*
    ///
    /// The count stays the weapon's own and the multishot bucket becomes an
    /// independent damage multiplier instead. Both halves matter: leaving the
    /// count alone is what keeps six elements six, and "multiplicative" is
    /// what keeps the bonus out of the base-damage bucket.
    #[serde(default)]
    pub multishot_adds_damage: bool,
    /// Damage types this attack applies on EVERY hit regardless of status
    /// chance — "Plasma bomb and seeking projectiles have a guaranteed Impact
    /// proc" (Phantasma Prime). Rolled status is unaffected and lands on top.
    ///
    /// DIRECT hits only, which is the engine's existing rule
    /// (`if direct { &ap.forced_procs }`) and is the wiki's too: the Astilla's
    /// direct hit forces Impact and its radial does not.
    #[serde(default)]
    pub forced_procs: Vec<String>,
    /// Seconds of BULLET ATTRACTOR this attack plants on what it hits — the
    /// spearguns' throw, and the one attack in the roster that applies the
    /// Void field without dealing Void (owner, 2026-08-14: the field IS the
    /// Void effect, and only the FIELD dies when the next throw starts —
    /// what it already applied runs its own clock on the enemy).
    ///
    /// Worth exactly one line in the Condition Overload counter, which is all
    /// `DebuffState::attractor` has ever been worth here. What the field is
    /// worth as a HEADSHOT aid is still unmodelled and still wants a measured
    /// rate — docs/UNMODELLED.md §Bullet Attractor.
    #[serde(default)]
    pub attractor_seconds: Option<f64>,
    /// A projectile that BOUNCES off what it hits and keeps going — the Latron
    /// family's Incarnon form, and the roster's only member.
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
    /// 2026-08-08).
    #[serde(default)]
    pub falloff: Option<FalloffSpec>,
    /// THE CONE THIS ATTACK FIRES INTO — see [`SpreadSpec`]. `None` = not
    /// transcribed, and the entry says so in `unmodeled:`.
    #[serde(default)]
    pub spread: Option<SpreadSpec>,
    /// PUNCH-THROUGH DEPTH in metres of material, from the weapon's own infobox.
    ///
    /// Written into every entry by the intake since the roster began and read
    /// by nobody until 2026-08-17, when the arena grew a second body — until
    /// then it changed no number, which is why an unread field was the honest
    /// place for it rather than an invented one.
    ///
    /// WHAT IT COSTS is [`crate::space::BODY_MATERIAL_M`] per body crossed.
    /// `999.0` is how INFINITE BODY punch-through is written (the Fluctus, the
    /// Phantasma): the page's qualifier on it — *"innate punch through does not
    /// apply to surfaces"* — separates bodies from geometry, and this arena has
    /// no geometry, so unlimited through bodies is the whole of it here.
    ///
    /// AN AoE ATTACK IGNORES THIS AND EVERY MOD, which is the punch-through
    /// page's own catalog rule and is applied in `loadout::resolve` rather than
    /// here: *"weapon projectiles with an area of effect (AoE) component will
    /// not Punch Through enemies or level geometry at all. Instead the
    /// projectile will explode on first contact"*, and *"Projectile AoE weapons
    /// cannot have their Punch Through stat modified"*.
    #[serde(default)]
    pub punch_through_m: f64,
    /// HOW FAR THIS ATTACK REACHES, metres — and PAST IT THERE IS NOTHING.
    ///
    /// The wiki's own Range stat, transcribed per weapon. A shot does not
    /// weaken at the end of its range; it stops existing there, so a target
    /// beyond it takes literally zero (Phantasma: *"Limited range of 20
    /// meters"*, and *"No Damage Falloff"* — the two facts are separate, and
    /// this is the first, which the engine had no way to express).
    ///
    /// NOT the same thing as `falloff:`, which is a RAMP over distance and is
    /// already modelled. A weapon can have either, both or neither: the
    /// Phantasma has a hard 20 m and no ramp at all.
    ///
    /// ABSENT MEANS THE PAGE STATES NONE, and 101 of the roster's 224 entries
    /// are in that state — a fact about the wiki rather than a gap in us. Every
    /// entry has had its page opened (`every_entry_has_had_its_range_page_opened`),
    /// so absence is no longer ambiguous and the `beam_range` admission that
    /// stood for it is gone.
    #[serde(default)]
    pub range_m: Option<RangeSpec>,
    /// DOES THIS ATTACK TAKE PUNCH-THROUGH MODS? A CATALOG ANSWER, and absent
    /// means ORDINARY — which for punch through is *yes*.
    ///
    /// `None` falls back to the punch-through page's own CLASS rule, which is
    /// about projectiles: *"With a very few exceptions, weapon projectiles with
    /// an area of effect (AoE) component will not Punch Through enemies or
    /// level geometry at all"*, and *"Projectile AoE weapons cannot have their
    /// Punch Through stat modified"*. So an attack with a `radial:` or a
    /// `lingering:` takes none.
    ///
    /// `Some(false)` is a WEAPON PAGE overruling that, and it is why this field
    /// exists rather than the shape alone deciding. The Torid's Incarnon form
    /// says *"Punch Through mods have no effect on the behavior of the beam"* —
    /// and it is a BEAM with a damage radius, so it carries neither `radial:`
    /// nor `lingering:` and the class rule would have let Shred onto it. The
    /// same wiki sentence that classifies it for Primary Compression names the
    /// family: *"beam attacks with an AoE component. For example, Ignis or
    /// Torid Incarnon Genesis"* — and the Ignis is on the punch-through page's
    /// EXCEPTION list, so the family does not decide it either. Only the entry
    /// does, which is docs/CATALOGS.md's rule generalised once more.
    ///
    /// `Some(true)` is the other direction, for an AoE attack a page says DOES
    /// take them. Nothing in the roster needs it yet.
    #[serde(default)]
    pub punch_through_mods: Option<bool>,
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
    /// A DEPLOYED ORB — see [`OrbSpec`]. An attack that has one settles no
    /// collision and no explosion of its own: the orb delivers both.
    #[serde(default)]
    pub orb: Option<OrbSpec>,
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

/// THE CONE AN ATTACK FIRES INTO — degrees from the reticle, per ATTACK.
///
/// **THE PRIMARY VALUE, and `accuracy` is the derived one** (owner,
/// 2026-08-15). The Arsenal prints an Accuracy that the wiki's own page defines
/// as `100 / average spread in degrees` and then shows as a CATEGORY beside it
/// ("Very High"); the thing the game has is this cone. Wiki `Accuracy` §Spread,
/// verbatim: *"spread is internally represented as an angle in degrees from the
/// reticle"*, with *"each weapon having a defined minimum (first-shot) and
/// maximum spread. Minimum spread is represented by the **Deviation With Aim**
/// stat while maximum spread is represented by the **Max Deviation** stat."*
///
/// Deriving the cone back out of the scalar loses two things that matter: the
/// min/max, and the FORM — `Module:Weapons/data` carries these per ATTACK, so
/// the Torid's grenade is `0 / 0` (pinpoint, and its page says so in words)
/// while its Incarnon beam is `1.0 / 1.5`. One accuracy number per weapon
/// cannot say that, which is why 63 form entries had no accuracy at all.
///
/// Transcribed by `scripts/intake_spread.py`, which refuses any attack it
/// cannot identify by an exact multi-field match.
///
/// **WHAT IS NOT MODELLED IS THE BLOOM.** The min is the FIRST SHOT and the max
/// is where sustained fire takes it — *"the faster a weapon fires, the larger
/// the size of the 'cone'"* — and the ramp between them is published nowhere.
/// A pellet here draws uniformly across the window instead, which has the
/// published average (`(min + max) / 2`, the wiki's own definition of the
/// number Accuracy is computed from) and does not invent a rate.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct SpreadSpec {
    /// Deviation With Aim — the first shot's cone.
    pub min_deg: f64,
    /// Max Deviation — where sustained fire takes it.
    pub max_deg: f64,
}

/// A DEPLOYED ORB — an entity with a POSITION, a clock and a reach, which is
/// none of the three things this engine had before it.
///
/// IT IS NOT A FIELD, and the difference is the whole reason for the type
/// (owner, 2026-08-28). A [`LingeringSpec`] is an AREA: everyone standing in it
/// burns, at their own falloff distance. An orb strikes exactly ONE body inside
/// its reach — *"Orb will shock 1 enemy within 6 meters of it every 1 second"*
/// (wiki `Grimoire`) — every strike deals the same number, and it MOVES between
/// them, so where it is decides who is a candidate.
///
/// It is not a projectile either. A projectile in this engine arrives, deals
/// its collision and its explosion, and is over; this one is thrown, lives out
/// a fuse striking as it drifts, and detonates at the end wherever it has got
/// to. So the attack it belongs to settles NO collision and no explosion at the
/// impact — everything it deals is delivered by the orb.
///
/// WHAT THE ORB DEALS IS THE ATTACK'S OWN. Its strike is the attack's `damage`,
/// crit, status, `forced_procs` and `unaimed_headshot_chance`; its detonation is
/// the attack's `radial`, moved to wherever the orb was when the fuse ran out.
/// This block is the GEOMETRY AND THE CLOCK and nothing else, the same division
/// [`BeamSpec`] makes for a beam.
///
/// THE STRIKE CLOCK RUNS FROM THE THROW, not from a contact, and that is what
/// reproduces the measured count. Six ticks over a six second fuse; a tick with
/// nobody inside the reach strikes nobody and is spent. So a throw that reaches
/// its target in under a second loses nothing, one that takes 2.5 s lands four
/// strikes, and against a body at contact it is always six — which is the
/// owner's `ceil(6 - flight)` arriving out of the geometry rather than being
/// written down (MEASUREMENTS M63).
// All f64, so it travels BY VALUE rather than as a leaked reference: the sim
// carries one per orb in the air and a Copy is cheaper than a pointer chase.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct OrbSpec {
    /// How long the orb lives before it detonates — *"explodes after 6
    /// seconds"*.
    pub fuse_seconds: f64,
    /// Seconds between strikes. *"Tick rate is not affected by Fire Rate"*
    /// (wiki), which this engine gets right by construction: the orb's clock is
    /// its own and no mod bucket reaches it.
    pub strike_interval_seconds: f64,
    /// How far a strike reaches from the orb. Fulmination (Primed) enlarges it
    /// — the owner confirms the reach and the detonation radius both take the
    /// blast-radius bucket (2026-08-28).
    pub strike_radius_m: f64,
    /// How fast it leaves the muzzle, and what it slows to once it has touched
    /// a body — 6 m/s then 2 m/s (owner, 2026-08-28).
    ///
    /// The launch speed is HERE rather than read off the attack's
    /// `projectile_speed_mps`, which this engine has never modelled: that field
    /// is transcribed on every projectile weapon in the roster and read by
    /// nothing, because a shot in this arena arrives the instant it is fired.
    /// An orb is the first thing whose flight actually costs something, so it
    /// states the number it flies at rather than quietly giving a meaning to a
    /// field 224 other entries assume has none.
    pub speed_mps: f64,
    pub speed_after_contact_mps: f64,
    /// EXTRA BODIES ONE STRIKE REACHES, beyond the one it struck — *"Each
    /// enemy hit chains to an additional 2 enemies within 6 meters"*.
    ///
    /// Multishot adds to it rather than adding orbs: *"Number of chains is
    /// affected by Multishot"*, and the owner gives the count as `multishot +
    /// this`, total bodies a strike reaches — three at an unmodded x1.0, and
    /// 4.6 at a panel reading x2.6, which is four for certain and a fifth 60%
    /// of the time (2026-08-28).
    pub chain_bodies: f64,
    /// How far a chain hop may reach, body to body — and it is the one
    /// distance on this attack that a RANGE MOD does not move.
    ///
    /// The orb's reach and its detonation radius both take the blast-radius
    /// bucket; the jump between two bodies stays at what the page gives it
    /// (owner, 2026-08-28). So the two sixes below are the same number by
    /// coincidence rather than by construction, and only one of them grows.
    pub chain_range_m: f64,
    /// What a hop deals relative to the hop before it.
    ///
    /// 1.0 — UNDILUTED — for the Grimoire: *"chain 起来没有衰减的 beam chain
    /// 那种方式"* (owner, 2026-08-28), which is also what the page supports on
    /// its own (it names a count and no reduction). Per entry rather than a
    /// constant, because a chain's falloff is per weapon everywhere else in
    /// this roster — the Atomos compounds at 0.75 and the Kuva Nukor does not
    /// compound at all.
    #[serde(default = "one")]
    pub chain_damage_per_hop: f64,
}

/// A lingering damage FIELD — MECHANICS §7 "Lingering damage FIELDS". Unlike
/// the radial this is not one instance at impact: it persists and TICKS.
#[derive(Debug, Clone, Deserialize)]
pub struct LingeringSpec {
    pub damage: BTreeMap<String, f64>,
    /// Ticks per second (the data module's per-attack `FireRate`).
    pub tick_rate: f64,
    /// Field lifetime in seconds (`EffectDuration`), measured from the field's
    /// OWN first tick rather than from the impact — see
    /// [`LingeringSpec::first_tick_delay_seconds`], which is zero for every
    /// field but one and leaves the two readings identical.
    pub duration_seconds: f64,
    pub radius_m: f64,
    /// HOW LONG AFTER THE IMPACT THE FIRST TICK LANDS.
    ///
    /// Zero for a CLOUD, and that is measured: the Torid's first tick lands
    /// with the impact number (M13), and reading the wiki's "Clouds do not
    /// instantly do damage" as a delayed first tick cost a tenth of the
    /// field's damage.
    ///
    /// The Grimoire's orb is the other shape. Its contact is a DIRECT hit —
    /// the attack's own damage part — and the pulses that follow are on a one
    /// second clock from there, so its field must not settle a second number
    /// at the instant the collision already settled one (owner, 2026-08-28).
    #[serde(default)]
    pub first_tick_delay_seconds: f64,
    /// Damage types this field's tick applies regardless of status chance —
    /// its OWN, exactly as [`RadialSpec::forced_procs`] is the explosion's.
    ///
    /// A cloud declares none, which is why this defaults to empty and why the
    /// tick path passed a literal `&[]` before this existed. The Grimoire's
    /// orb declares Electricity: the owner measured the pulses forcing it and
    /// the final explosion NOT forcing it (2026-08-28), which is one attack
    /// answering the question both ways and the reason the two lists are
    /// separate rather than shared.
    #[serde(default)]
    pub forced_procs: Vec<String>,
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
    /// DO ELEMENTAL MODS REACH THIS FIELD? Default YES, which is the Torid's
    /// cloud and what "through the SAME mod buckets" has always meant here.
    ///
    /// Nightwatch Napalm is the exception and the wiki states it as a closed
    /// list: *"Damage output can only be increased by base damage mods (e.g.
    /// Serration, Heavy Caliber, Semi-Rifle Cannonade), and faction mods"* — so
    /// its 150 Heat is multiplied by the base-damage bucket and by the faction
    /// bonus at fire time, and a Cryo Rounds on the same build does nothing to
    /// it.
    #[serde(default = "lingering_default_true")]
    pub elemental_mods_apply: bool,
    /// CAN THIS FIELD CRIT AT ALL?
    ///
    /// Not the same question as "what is its crit chance". Nightwatch Napalm's
    /// fire states a zero, and the wiki states the REASON in stronger words:
    /// it cannot crit *"via any means"*. A zero alone does not survive a build
    /// that is trying — Vital Sense multiplies the crit DAMAGE bucket, taking a
    /// field's 1.0 to 2.2, and a post-mod ADDITIVE crit-chance source (Arcane
    /// Avenger) lifts the chance off zero. Together those make a fire that
    /// cannot crit crit for 2.2x (owner asked, 2026-08-23; found by the test
    /// the question prompted).
    ///
    /// So this is declared rather than inferred from a zero: a field with 0%
    /// base crit and no such sentence on its page SHOULD take a flat crit-chance
    /// source, exactly as a 0% weapon does.
    #[serde(default = "lingering_default_true")]
    pub can_crit: bool,
    /// DO STATUS-CHANCE MODS REACH IT? Default YES, same reasoning.
    ///
    /// Nightwatch Napalm's is pinned: *"Napalm has 68% chance to proc Heat …
    /// Status chance is not affected by mods."*
    #[serde(default = "lingering_default_true")]
    pub status_mods_apply: bool,
}

fn lingering_default_true() -> bool {
    true
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

fn chain_compounds_default() -> bool {
    true
}

/// The chain a beam propagates through enemies.
#[derive(Debug, Clone, Deserialize)]
pub struct ChainSpec {
    /// Hops in ONE chain — a sequence, each at `damage_per_hop` of the last.
    pub hops: u32,
    pub range_m: f64,
    pub damage_per_hop: f64,
    /// Does `damage_per_hop` COMPOUND along the path, or does every hop deal
    /// the same share of the main beam?
    ///
    /// Compounding is the common shape and the default — the Atomos is
    /// *"0.75^n times the main beam's damage, where n is the chain number"*,
    /// and the Torid, Larkspur and Boar all read the same way ("of the previous
    /// chain's damage"). The Kuva Nukor does NOT: *"chain up to 2 nearby
    /// enemies … each doing 50% of the main beam's damage"* — both hops at 50%,
    /// not 50% and 25%. It is one word's difference on the page and a factor of
    /// two on the second hop.
    #[serde(default = "chain_compounds_default")]
    pub compounds: bool,
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

/// WHERE AN EXPLOSION GOES OFF, which is the difference between a weapon that
/// may be given punch through and one that may not.
///
/// The owner named the problem (2026-08-20): a Burston Prime Incarnon carries a
/// blast on the card and *"actually punches through"* in game, its round going
/// off BEHIND the enemy it passed — and its blast takes no multishot either, so
/// he called it a FAKE AoE and asked for it to be a type rather than a pile of
/// exceptions. This is that type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum BlastKind {
    /// Detonates on the first thing it touches — a grenade, a rocket, a
    /// speargun's spear. This is a TRUE area-of-effect attack, so the punch
    /// through page's class rule applies to it and no mod may give it any.
    #[default]
    Contact,
    /// Bores through bodies while its punch-through budget lasts and detonates
    /// where the FLIGHT ends — so it is not an area-of-effect projectile in the
    /// sense that rule means, and punch-through mods work on it normally.
    ///
    /// TWO CONSEQUENCES, and the second is the surprising one. Punch through
    /// buys more DIRECT hits, as on any other weapon; and it moves the
    /// explosion DOWN THE LINE — onto whichever body the round cannot get out
    /// of, which in a crowd is deeper and better and against a lone enemy is
    /// past it and worse. `space::dissipation_point` is the geometry.
    Terminal,
}

/// The radial (explosion) part of an attack — MECHANICS §7. Crit/status
/// default to the direct part's when the data does not state them.
#[derive(Debug, Clone, Deserialize)]
pub struct RadialSpec {
    pub damage: BTreeMap<String, f64>,
    pub radius_m: f64,
    /// See [`BlastKind`] — `contact` unless the entry says otherwise.
    #[serde(default)]
    pub blast_kind: BlastKind,
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
    /// Damage types this EXPLOSION applies on every hit regardless of status
    /// chance — its OWN, not the direct part's.
    ///
    /// The two are different questions and the roster has both answers: the
    /// Astilla's direct hit forces Impact and its radial does not, while the
    /// Scourge pair's page says "Guaranteed Impact proc" of the SPEAR EXPLOSION
    /// and nothing of the throw. One shared list would have made the Scourge
    /// force a proc on a hit the game does not force one on.
    #[serde(default)]
    pub forced_procs: Vec<String>,
    /// Does this explosion take Condition Overload? Default NO — the mods say
    /// direct hits only, so an AoE part receiving it is a per-entry exception
    /// the CO catalog lists (the Zylok's Incarnon radial has such a row).
    #[serde(default)]
    pub takes_condition_overload: bool,
    /// Does the explosion fire once PER PELLET, or once per trigger pull?
    ///
    /// **MOST AoE TAKES MULTISHOT** (owner, 2026-08-23), and the reason is the
    /// mechanism rather than a table: a pellet lands and detonates, so several
    /// pellets are several detonations. Default YES.
    ///
    /// THE EXCEPTION IS A WEAPON WHOSE BLAST IS TIED TO THE SHOT rather than to
    /// the projectile. The Burston Incarnon is the one the roster has and the
    /// wiki states it outright — "The Radial Attack does not benefit from
    /// Multishot bonuses" — and what makes it exceptional is that its explosion
    /// counts PULLS: extra pellets ride along and add nothing to it.
    ///
    /// HOW IT WAS FOUND: both Ogrises read `false` with no source behind it,
    /// which cost a Split Chamber 43% of what it is worth on that weapon — the
    /// second rocket arrived, left its napalm fire, and did no blast damage at
    /// all. "Two warheads is two fields" was the reading that settled it: a
    /// field is left by a rocket that LANDED, so two fields is two arrivals, and
    /// an arrival detonates.
    ///
    /// **73 ENTRIES STILL DECLARE `false` AND ONLY TWO HAVE A SOURCE** — the two
    /// Burstons. The rest carry the shape of a bulk intake that applied it as a
    /// rule, which is the one thing the line below forbids. Nobody has surveyed
    /// them against the wiki yet; until somebody does, a `false` on any entry
    /// other than a Burston should be read as unverified rather than as stated.
    ///
    /// Declared per entry, never inferred.
    #[serde(default = "yes")]
    pub takes_multishot: bool,
}

/// A PROJECTILE THAT DEFLECTS OFF WHAT IT HITS and keeps going.
///
/// Verbatim, from the Latron Incarnon Genesis page: *"a traveling projectile
/// that can ricochet off enemies and terrain, exploding up to 6 times with a 4
/// meter radius, dealing damage once for any collision on enemies, and again
/// for the explosion"*.
///
/// NO ATTENUATION PER BOUNCE. The page names none, and it names the one thing
/// that does change — *"Each ricochet will cause the projectile to slow
/// down"* — so every bounce deals this attack's collision and this attack's
/// explosion in full. The slowing is what ends the projectile in game and is
/// declared as a gap on the weapons that have it.
///
/// It was `{ targets, range_m }` and unread by anything, in any file, since it
/// was written. Rewritten rather than joined, so the roster has one spelling.
#[derive(Debug, Clone, Deserialize)]
pub struct RicochetSpec {
    /// Bounces AFTER the first collision.
    ///
    /// SIX EXPLOSIONS IS FIVE BOUNCES: *"exploding up to 6 times"*, and the
    /// first of the six is the shot arriving, which the ordinary pipeline
    /// already pays for.
    pub bounces: u32,
    /// The chance a bounce lands on a head.
    ///
    /// A bounce is NOT AIMED, so the scenario's `headshot_pct` — a statement
    /// about the player's aim — says nothing about where one lands. Owner,
    /// 2026-08-18: 0.5.
    pub headshot_chance: f64,
    /// How far a bounce may travel to find its next body. Absent = the nearest
    /// body it has not already hit, however far, which leaves the bounce COUNT
    /// as the only limit — and the count is the only limit the page states.
    #[serde(default)]
    pub range_m: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GaugeFormSpec {
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
    /// field (owner, 2026-08-08).
    #[serde(default = "standard_transmute_out")]
    pub transmute_out_seconds: f64,
}

/// See [`GaugeFormSpec::transmute_out_seconds`]. Changing this changes every
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
/// whole extension point, and `alt_fire` went in the day the Scourge pair
/// needed it rather than in advance, because a kind nothing registers is a kind
/// nothing tests.
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
    /// A SECOND TRIGGER, chosen freely and costing no meter — the Scourge pair
    /// throws the weapon itself, and its numbers, its element split and its
    /// explosion are all its own.
    ///
    /// Its own kind rather than [`FormKind::Charged`], which was the only
    /// non-gauge alternate the vocabulary had: a thrown spear is not a drawn
    /// bow, the id is what a saved preset and a share link carry, and a wrong
    /// word there is wrong forever (owner, 2026-08-14).
    AltFire,
    /// …AND THE THIRD ONE, for a weapon that CYCLES more than two triggers.
    ///
    /// A form is identified by its KIND — `forms_of` keys on it and `/api/meta`
    /// looks a form up by `kind.id()` — so a group cannot hold two `AltFire`
    /// entries: the second would be unreachable. The Kuva Hind has three
    /// togglable modes ("5-round burst, semi-auto, and full-auto", cycled with
    /// Alternate Fire), which is one more than the vocabulary had.
    ///
    /// NAMED FOR THE TRIGGER, because on this weapon the trigger IS the whole
    /// difference — three blocks that share every weapon-level stat and differ
    /// in cadence, crit and damage. The alternative was `alt_fire_2`, which
    /// says nothing and would be in every saved preset and share link forever
    /// (the reason `AltFire` itself exists rather than borrowing `Charged`).
    ///
    /// So the Hind reads base / semi_auto / auto: `Base` is the arsenal's
    /// burst, and these two are the other pulls.
    SemiAuto,
    /// See [`FormKind::SemiAuto`] — the fully automatic member of the same set.
    Auto,
}

impl FormKind {
    /// The stable id — the wire value in an API request and in a saved preset,
    /// so these strings are durable names, not labels.
    pub fn id(self) -> &'static str {
        match self {
            FormKind::Base => "base",
            FormKind::Charged => "charged",
            FormKind::Incarnon => "incarnon",
            FormKind::AltFire => "alt_fire",
            FormKind::SemiAuto => "semi_auto",
            FormKind::Auto => "auto",
        }
    }

    /// English display name (the i18n overlay translates from this).
    pub fn label(self) -> &'static str {
        match self {
            FormKind::Base => "Base Form",
            FormKind::Charged => "Charged Shot",
            FormKind::Incarnon => "Incarnon Form",
            FormKind::AltFire => "Alternate Fire",
            FormKind::SemiAuto => "Semi-Auto",
            FormKind::Auto => "Full-Auto",
        }
    }

    /// Does this form exist only because an ADAPTER was installed?
    ///
    /// A property of the KIND, and only the Incarnon form has it: the form is
    /// not in the arsenal until a Genesis is fitted and a tier-1 evolution
    /// chosen, which is why a riven pool skips it and why the form list hides
    /// it until the unlock is in the build.
    ///
    /// IT IS NOT THE GAUGE QUESTION. Those were one method until the Mausolon
    /// arrived (owner, 2026-08-15): its alt-fire is bought with five kills and
    /// is a gauge-fed form of an ordinary Arch-Gun, with no adapter anywhere.
    /// "Does entering this cost a meter" is [`WeaponSpec::has_gauge`], which
    /// reads what the weapon DECLARES instead of inferring it from a name.
    pub fn is_adapter_form(self) -> bool {
        matches!(self, FormKind::Incarnon)
    }

    pub fn parse(s: &str) -> FormKind {
        match s {
            "base" => FormKind::Base,
            "charged" => FormKind::Charged,
            "incarnon" => FormKind::Incarnon,
            "alt_fire" => FormKind::AltFire,
            "semi_auto" => FormKind::SemiAuto,
            "auto" => FormKind::Auto,
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
    /// A FORM INHERITS ITS WEAPON. The id of the entry this one is a form of —
    /// every WEAPON-LEVEL field this entry does not state is filled in from
    /// there, and only what actually DIFFERS is written down here.
    ///
    /// WHY IT EXISTS. 88 of the roster's entries are form siblings rather than
    /// weapons, and before this they each restated their weapon's mastery
    /// rank, disposition, polarities, riven family, internal name, magazine,
    /// reload and the rest. An audit on 2026-08-15 counted 313 identical values
    /// written twice and 1,004 (group, field) pairs where some siblings carried
    /// a field and others did not — and it found a real error inside that
    /// noise: the ordinary Larkspur's alt-fire carried its BASE form's
    /// accuracy while its Prime's carried the alt-fire's. Nothing could catch
    /// it, because nothing knew the two entries were the same weapon.
    ///
    /// With this, a difference is the only thing on the page.
    ///
    /// Applies to [`INHERITED`] and to nothing else: the ATTACK is never
    /// inherited (it is the entire reason a form is a separate entry), and
    /// neither is `co_behavior`, which the catalog gives PER ATTACK — the
    /// Mandonel's two forms take different classes from two different rows.
    #[serde(default)]
    pub inherits: Option<String>,
    /// EVERY ADMISSION, STRUCTURED — filled in at load time beside the rendered
    /// `unmodeled:` strings, so a localized page can re-render one instead of
    /// looking up the whole English sentence.
    ///
    /// A weapon never writes this; it writes `unmodeled:` and this is derived.
    #[serde(default)]
    pub unmodeled_parts: Vec<UnmodelledPart>,
    pub id: String,
    pub name: String,
    /// THIS ENTRY CANNOT AIM DOWN SIGHTS, so nothing gated on aiming pays.
    ///
    /// On the wiki "Zoom" IS the word for aiming — its page opens "Zoom (or
    /// aiming, aiming down sights (ADS))", and the Galvanized mods link the
    /// word as `[[Zoom|aiming]]`. So the Vasto's "cannot Zoom" is a statement
    /// about the aim STATE, not about magnification.
    ///
    /// DE settled what that costs, in a patch note about Mesa's Regulators:
    /// "Removed ability to unintentionally equip Hydraulic Crosshairs and
    /// Sharpened Bullets on Mesa's Regulators. Although the buff appeared to
    /// trigger, it never actually applied due to the 'on aim' criteria not
    /// being fulfilled."
    ///
    /// PER FORM, not per weapon: the Vasto aims fine and its Incarnon form does
    /// not, and each form is its own entry.
    #[serde(default)]
    pub cannot_zoom: bool,
    /// WHAT THIS ENTRY DOES NOT MODEL, in the reader's own language, one
    /// sentence per gap.
    ///
    /// The enemy files have carried this since the target card was written, and
    /// weapons had nowhere to put it: a yaml COMMENT is honest to whoever opens
    /// the file and invisible to everyone else. The bulk Incarnon intake made
    /// that expensive — a weapon whose base attack has parts this entry does not
    /// carry (a bow's uncharged shot, the Angstrum's explosion, the Stug's
    /// blobs) reads as a complete weapon, and its number is not the weapon's
    /// number (owner, 2026-08-08).
    ///
    /// Prose, deliberately, and the ONE place in a weapon file where prose is a
    /// value rather than a comment — the same exception `enemies/` already
    /// carries, for the same reason: it is shown to a reader verbatim.
    #[serde(default)]
    pub unmodeled: Vec<String>,
    /// WHAT THIS ENTRY DOES THAT NOBODY CAN EXPLAIN, and the engine reproduces
    /// anyway — the weapon's half of the `live_bugs:` an arcane, an ability and
    /// an enemy already carry.
    ///
    /// IT IS NOT AN `unmodeled:` LINE and must not be filed as one: that banner
    /// says "the number below is a FLOOR", and this says the opposite — the
    /// number is right, it was measured, and the reason is unknown. A player
    /// building around it is owed both facts.
    ///
    /// The case it was added for is the Laetum's Incarnon form, whose Secondary
    /// Irradiate echo measures 3.6x the hit where the arcane's own card says
    /// 1.8x and every pure single-target weapon delivers 1.8x (MEASUREMENTS
    /// M59). The engine carries it as `echo_multiplier` and said so nowhere a
    /// reader could see, while the arcane card beside it printed 180%
    /// (owner asked where it was shown, 2026-08-25).
    ///
    /// Prose, for the same reason `unmodeled:` is: it is shown verbatim.
    #[serde(default)]
    pub live_bugs: Vec<String>,
    /// DE's ACCURACY stat, as the Arsenal prints it. REFERENCE ONLY — the
    /// model reads [`AttackSpec::spread`], which is the primary value.
    ///
    /// The yaml has carried this on 144 entries since the intake and NOTHING
    /// deserialized it, so serde dropped every one of them: a number in the
    /// repo that no code could see (2026-08-15).
    ///
    /// It is DERIVED and it is fuzzy (owner, 2026-08-15): the wiki defines it
    /// as `100 / (average spread in degrees)` and prints it as a CATEGORY
    /// ("Very High"), so it is one rounded scalar standing in for a window —
    /// and it is a WEAPON-level field, which cannot describe a form at all.
    /// The engine therefore does not read it; `AttackSpec::spread` is what the
    /// aim model uses and it comes per attack from the same wiki module.
    #[serde(default)]
    pub accuracy: Option<f64>,
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
    /// An ADVERSARY weapon's valence bonus — what it CAN have. See
    /// [`ValenceSpec`]; absent on every weapon that is not one.
    #[serde(default)]
    pub valence: Option<ValenceSpec>,
    /// Does this weapon apply MICROWAVE — the Nukor family's own invisible
    /// status? See `dummy::DebuffState::microwave`. Two weapons in the game
    /// have it and the wiki names both.
    #[serde(default)]
    pub applies_microwave: bool,
    /// INDEPENDENT PROCS this attack lands — status effects that come from a
    /// specific weapon rather than from the damage-type draw
    /// (`data/debuffs/independent_procs.yaml`).
    ///
    /// A LIST OF IDS, not a flag per effect. `applies_microwave` above is the
    /// older shape and the reason this one is not: an effect that arrives with
    /// the next weapon should cost a row in a table, not a field on every
    /// weapon in the roster (owner, 2026-08-15). The engine implements the ids
    /// it knows and panics on one it does not, which is the same contract
    /// `charge_on` has.
    ///
    /// The only member so far is `lifted` — the Mausolon's alt-fire explosion.
    /// It carries no damage; what it carries is a status TYPE, and Condition
    /// Overload counts it (`Status_Effect` §Independent from Damage, and
    /// MECHANICS §"Condition Overload").
    #[serde(default)]
    pub independent_procs: Vec<String>,
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
    /// THE CHAMBER RECORD this entry is assembled from — `tombfinger_secondary`
    /// in `data/kitguns/chambers/`. Its presence is what makes the weapon
    /// MODULAR, and it is the whole of that difference.
    ///
    /// A Kitgun has no published stat line: every number on this entry is the
    /// chamber's own `base` PREVIEW, which is the module's no-grip row and is
    /// not any grip's answer. [`spec_assembled`] composes the real ones over
    /// the top the moment a build names an assembly, exactly as an evolution
    /// overrides a panel — so nothing downstream of `base_panel` has to learn
    /// what a Kitgun is.
    #[serde(default)]
    pub kitgun: Option<String>,
    /// ROUNDS A SECOND under Pax Charge, filled in by [`spec_assembled`] from
    /// the chamber. Not written in any weapon yaml: it is the chamber's, and a
    /// roster entry that restated it would be the same number written twice.
    #[serde(skip)]
    pub recharge_per_second: Option<f64>,
    /// Riven disposition — the multiplier every riven stat on this weapon is
    /// scaled by. It belongs to the WEAPON, not to the riven, which is why
    /// **AN UNEXPLAINED, MEASURED COEFFICIENT ON SECONDARY IRRADIATE'S ECHO.**
    ///
    /// The arcane's echo is `1.8 × the hit` at max rank, and on a pure
    /// single-target weapon that is exactly what it deals — the owner measured
    /// several. On the LAETUM'S INCARNON FORM it deals **3.6×**, twice as much,
    /// and nobody knows why (owner, 2026-08-24, M59):
    ///
    /// ```text
    /// base form      1536 direct  ->  2764.8 echo   = 1.80x   (ordinary)
    /// Incarnon form   320 direct  ->  1152   echo   = 3.60x
    ///                 960 direct  ->  3456   echo   = 3.60x
    /// ```
    ///
    /// IT IS NOT THE AoE TRIGGERING IT A SECOND TIME. The same session
    /// established that only a DIRECT hit ever triggers this echo and the
    /// radial never does — which this engine already had right, since
    /// `spread_from_echo` is called from the direct path alone. So the
    /// explanation is not two triggers, and we do not have another.
    ///
    /// WHAT IS SUSPECTED AND NOT ESTABLISHED: the two forms differ in that the
    /// Incarnon one carries a radial, and the owner reports "other weapons with
    /// an AoE seem to have a bit of this problem". That is a lead, not a rule —
    /// so this is a per-ENTRY number rather than a rule about AoE weapons, and
    /// it stays that way until somebody measures a second one. The same
    /// discipline `docs/CATALOGS.md` states for Condition Overload: transcribe
    /// the row for the entry it names, never generalise it to a class.
    ///
    /// NOT INHERITED (it is absent from `INHERITED`), because the base Laetum
    /// measures 1.8 and only its Incarnon form does not.
    #[serde(default = "one")]
    pub echo_multiplier: f64,
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
    pub reload_start_seconds: Option<f64>,
    #[serde(default)]
    pub reload_per_shell_seconds: Option<f64>,
    #[serde(default)]
    pub reload_end_seconds: Option<f64>,
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
    /// all" (owner). A Torid has 60 rounds behind its magazine;
    /// what it also has is a way to get more.
    #[serde(default)]
    pub no_resupply: bool,
    /// A status-triggered crit-chance LOCK — Gotva Prime's passive, and the
    /// first of its kind in the roster.
    #[serde(default)]
    pub super_crit_on_status: Option<SuperCritSpec>,
    /// The Ocucor's tendrils — see [`TendrilSpec`].
    #[serde(default)]
    pub tendrils: Option<TendrilSpec>,
    /// The sniper's Shot Combo Counter — see [`SniperCombo`]. `None` on every
    /// weapon that is not a sniper rifle, which is what the mechanic is keyed
    /// on in game: it is not a class-wide rule the engine could infer from
    /// `class: sniper`, because the Minimum Combo is per weapon.
    #[serde(default)]
    pub sniper_combo: Option<SniperCombo>,
    /// ...and its scope's own buff — see [`ScopeSpec`].
    #[serde(default)]
    pub scope: Option<ScopeSpec>,
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
    /// THE WEAPON'S OWN HEADSHOT MULTIPLIER, which REPLACES the body part's.
    ///
    /// A head is worth what the ENEMY's body part says it is worth, on every
    /// weapon but a handful: the Tenet Arca Plasmor's page states *"1x headshot
    /// multiplier"* under its disadvantages and then *"It is exceedingly easy to
    /// perform headshots with this weapon. Although it has a 1x headshot
    /// multiplier (meaning it does no extra damage), this can be increased using
    /// Primary Deadhead."* So the head is still a HEAD — the shot counts as one
    /// and Deadhead pays on it — and only the enemy's own multiplier is
    /// overruled.
    ///
    /// NOT `headshot_damage_bonus`, which is the additive bracket beside
    /// Deadhead's and which the module states this weapon's value in:
    /// `ExtraHeadshotDmg = -2`, on seven primaries (both Alternoxes, both Arca
    /// Plasmors, both Fulmins' semi-auto mode, Nataruk's Perfect Shot). Put
    /// through that bracket `1 + (-2)` is NEGATIVE and a headshot would heal the
    /// target — so the datamined figure encodes a multiplier DE applies
    /// somewhere this engine does not, and the wiki's own sentence, "1x", is
    /// what gets transcribed. The two fields compose: a weapon may state this
    /// multiplier and still take every additive bonus on top of it.
    ///
    /// It also silences the CRITICAL HEADSHOT doubling, which the engine gates
    /// on a part multiplier above 1x — correctly, since the wiki's rule is about
    /// a weak point worth more than 1x and this weapon's head is not one.
    #[serde(default)]
    pub headshot_multiplier: Option<f64>,
    #[serde(default)]
    pub transform_group: Option<String>,
    #[serde(default)]
    pub transforms_from: Option<String>,
    #[serde(default)]
    pub transforms_to: Option<String>,
    pub attack: AttackSpec,
    #[serde(default)]
    pub gauge_form: Option<GaugeFormSpec>,
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

/// ONE ADMISSION, as the page needs it.
///
/// `text` is the finished English. `template` and `params` are present when the
/// admission named a REASON, and they are what lets a locale translate the
/// sentence once rather than once per set of numbers.
#[derive(Debug, Clone, Deserialize)]
pub struct UnmodelledPart {
    pub text: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReasonDef {
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ReasonFile {
    reasons: BTreeMap<String, ReasonDef>,
}

/// The reason table — `data/unmodelled/reasons.yaml`, parsed once.
fn reasons() -> &'static BTreeMap<String, String> {
    static R: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    R.get_or_init(|| {
        let mut out = BTreeMap::new();
        for (p, text) in crate::data::files_under("unmodelled/") {
            if !p.ends_with(".yaml") {
                continue;
            }
            let f = serde_norway::from_str::<ReasonFile>(text)
                .unwrap_or_else(|e| panic!("parse {p}: {e}"));
            for (k, v) in f.reasons {
                out.insert(k, v.text);
            }
        }
        out
    })
}

/// Substitute `{named}` holes. Anything the params do not name is LEFT ALONE
/// rather than blanked — a template with a hole nobody filled should read as
/// obviously broken on the page, not as a sentence with a gap in it.
pub fn fill_template(tpl: &str, params: &BTreeMap<String, String>) -> String {
    let mut out = tpl.to_string();
    for (k, v) in params {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// WHAT A FORM INHERITS FROM ITS WEAPON — the fields that describe the
/// WEAPON rather than the shot.
///
/// The line is drawn at "would the arsenal print this once for the gun, or
/// once per firing mode". Mastery, disposition, polarities and the riven
/// family are the gun's. Magazine and reload are the gun's TOO, even though a
/// form may override them: the Scourge's throw really does hold one round
/// against the primary fire's forty, and stating that override is exactly what
/// this mechanism makes visible.
///
/// NOT HERE, deliberately:
///   - everything under `attack:` — a form IS its attack;
///   - `co_behavior`, which the Condition Overload catalog gives per ATTACK
///     (the Mandonel's uncharged shot is Multiplying and its charged one
///     Adding, from two different rows);
///   - `form`, `default_form`, `transform_group`, `transforms_to/from`,
///     `incarnon`, `id`, `name` — the entry's own identity;
///   - `source`, because a form that shares a page still says so itself.
const INHERITED: [&str; 20] = [
    "slot", "class", "mod_pools", "mastery_rank", "max_rank", "accuracy",
    "disposition", "polarities", "exilus_polarity", "riven_family",
    "internal_name", "noise", "magazine", "reload_seconds", "ammo_type",
    "ammo_max", "ammo_pickup", "traits", "deployment", "no_resupply",
];

/// ...and `deployments` and `valence`, which are MAPS and would need a deep
/// merge to inherit partially. They are all-or-nothing: a form that states
/// neither takes its weapon's whole block.
const INHERITED_BLOCKS: [&str; 4] = ["deployments", "valence", "sniper_combo", "scope"];

/// Every weapon entry in `data/weapons/` (embedded), parsed once.
pub fn all() -> &'static [WeaponSpec] {
    static SPECS: OnceLock<Vec<WeaponSpec>> = OnceLock::new();
    SPECS.get_or_init(|| {
        // TWO PASSES, and the merge happens on the YAML rather than on the
        // struct: `WeaponSpec`'s fields have defaults, so once it is
        // deserialized "absent" and "stated at the default" are the same
        // thing, and a form could not inherit a `false` or a `0`.
        use serde_norway::Value;
        let raw: Vec<(&str, Value)> = crate::data::files_under("weapons/")
            .filter(|(p, _)| p.ends_with(".yaml"))
            .map(|(p, text)| {
                (p, serde_norway::from_str::<Value>(text)
                    .unwrap_or_else(|e| panic!("parse {p}: {e}")))
            })
            .collect();
        let by_id: std::collections::HashMap<String, Value> = raw
            .iter()
            .filter_map(|(_, v)| {
                v.get("id").and_then(Value::as_str).map(|i| (i.to_string(), v.clone()))
            })
            .collect();
        raw.into_iter()
            .map(|(p, mut v)| {
                if let Some(parent) = v.get("inherits").and_then(Value::as_str) {
                    let up = by_id
                        .get(parent)
                        .unwrap_or_else(|| panic!("{p}: inherits unknown id `{parent}`"));
                    let m = v.as_mapping_mut().unwrap_or_else(|| panic!("{p}: not a mapping"));
                    for k in INHERITED.iter().chain(INHERITED_BLOCKS.iter()) {
                        let key = Value::String((*k).to_string());
                        if !m.contains_key(&key) {
                            if let Some(val) = up.get(*k) {
                                m.insert(key, val.clone());
                            }
                        }
                    }
                    // ADMISSIONS ARE NOT INHERITED, and that is deliberate.
                    // A form's `unmodeled:` is about THAT form — the Lanka's
                    // full draw says "the partial charge is a separate entry",
                    // which is nonsense printed on the partial charge. The
                    // shared lines are the class's rather than the weapon's
                    // anyway (every Arch-Gun repeats the Deployer cooldown),
                    // and de-duplicating THOSE is a different job: they want a
                    // reason id and a template, not a parent.
                }
                render_admissions(&mut v, p);
                serde_norway::from_value::<WeaponSpec>(v)
                    .unwrap_or_else(|e| panic!("parse {p}: {e}"))
            })
            .collect()
    })
}

/// Turn `unmodeled:` into finished English AND a structured list, in place.
///
/// An entry is either a STRING — prose, for a gap that happens once — or a
/// mapping naming a `reason:` from `data/unmodelled/reasons.yaml` with its
/// parameters. Both end as a sentence in `unmodeled`; only the second can be
/// re-rendered in another language, which is the whole point.
fn render_admissions(v: &mut serde_norway::Value, path: &str) {
    use serde_norway::Value;
    let Some(m) = v.as_mapping_mut() else { return };
    let key = Value::String("unmodeled".to_string());
    let Some(list) = m.get(&key).and_then(Value::as_sequence).cloned() else { return };
    let mut text: Vec<Value> = Vec::with_capacity(list.len());
    let mut parts: Vec<Value> = Vec::with_capacity(list.len());
    for one in list {
        let mut part = serde_norway::Mapping::new();
        match &one {
            Value::String(s) => {
                text.push(Value::String(s.clone()));
                part.insert(Value::String("text".into()), Value::String(s.clone()));
            }
            Value::Mapping(mm) => {
                let rid = mm
                    .get(Value::String("reason".into()))
                    .and_then(Value::as_str)
                    .unwrap_or_else(|| panic!("{path}: an admission mapping needs `reason:`"))
                    .to_string();
                let tpl = reasons().get(&rid).unwrap_or_else(|| {
                    panic!("{path}: unknown unmodelled reason `{rid}` — add it to data/unmodelled/reasons.yaml")
                });
                let params: BTreeMap<String, String> = mm
                    .iter()
                    .filter(|(k, _)| k.as_str() != Some("reason"))
                    .map(|(k, val)| {
                        let ks = k.as_str().unwrap_or_default().to_string();
                        let vs = match val {
                            Value::String(s) => s.clone(),
                            other => serde_norway::to_string(other)
                                .unwrap_or_default()
                                .trim()
                                .trim_start_matches("---")
                                .trim()
                                .to_string(),
                        };
                        (ks, vs)
                    })
                    .collect();
                let rendered = fill_template(tpl, &params);
                text.push(Value::String(rendered.clone()));
                part.insert(Value::String("text".into()), Value::String(rendered));
                part.insert(Value::String("reason".into()), Value::String(rid));
                part.insert(Value::String("template".into()), Value::String(tpl.clone()));
                let pm: serde_norway::Mapping = params
                    .into_iter()
                    .map(|(k, val)| (Value::String(k), Value::String(val)))
                    .collect();
                part.insert(Value::String("params".into()), Value::Mapping(pm));
            }
            other => panic!("{path}: an admission is a string or a mapping, got {other:?}"),
        }
        parts.push(Value::Mapping(part));
    }
    m.insert(key, Value::Sequence(text));
    m.insert(Value::String("unmodeled_parts".into()), Value::Sequence(parts));
}

impl WeaponSpec {
    /// Does entering this form cost a METER the fight has to fill?
    ///
    /// The one question `has_gauge_switched_form` and the sim's cycle both ask,
    /// and it is answered by what the entry DECLARES — never by its form kind.
    /// An Incarnon adapter is one way to get here; five kills with a Mausolon
    /// is another.
    pub fn has_gauge(&self) -> bool {
        self.gauge_form.is_some()
    }
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
    /// `gauge_cycle` runs the cycle, and anything else names a single form
    /// to fire. So a mode is translated at that boundary and nowhere else —
    /// which is what lets "played without ever transmuting" be ASKED FOR at
    /// all, where `form: default` could only ever mean "however it is normally
    /// played" and resolved to the cycle behind your back.
    pub fn form(&self) -> &'static str {
        match self.mode {
            // NOT `incarnon_cycle`: the policy is "fill a gauge in one form,
            // spend it in the other, come back", and an Incarnon adapter is
            // one thing that produces it. The Mausolon earns its alt-fire with
            // KILLS and has no adapter (owner, 2026-08-15). The old spelling is
            // still ACCEPTED by the parser — it never reached a saved build or
            // a board row, but it costs one `||` to be sure.
            PlayMode::Cycle => "gauge_cycle",
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
        let gauged = spec(alt.weapon_id).is_some_and(|s| s.gauge_form.is_some());
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
        // …AND A THIRD FORM NEEDS A THIRD NAME. This is the same collision the
        // Paris caused in 2026-08-08 — two alternates both emitting
        // `id: "alternate"`, so a build naming a mode named neither — arriving
        // from the other side: the Kuva Hind's two extra triggers are both FREE,
        // so `Transformed` does not tell them apart either.
        //
        // A MODE IS NAMED FOR ITS FORM, but only where it has to be. Every kind
        // that existed before keeps `"alternate"`, because a mode id is what a
        // saved preset, a share link and a board row carry, and renaming one
        // would orphan every stored build. Only the two kinds that could not
        // have been stored yet take their own name.
        let id = match (gauged, alt.kind) {
            (true, _) => mode.id(),
            (false, FormKind::SemiAuto | FormKind::Auto) => alt.kind.id(),
            (false, _) => PlayMode::Alternate.id(),
        };
        out.push(WeaponPlayMode {
            id,
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

/// Does this weapon have a form you TRANSFORM into? Only such a weapon has a
/// cycle to simulate — anything else is fired in one form at a time, whatever
/// forms it registers.
///
/// DECLARED, NOT INFERRED. This used to ask the form's KIND, which made "has a
/// gauge" and "is the Incarnon form" the same sentence; the Mausolon's
/// kill-fed alt-fire is the counter-example, and it is a `charged` form.
pub fn has_gauge_switched_form(weapon_id: &str) -> bool {
    forms_of(weapon_id)
        .iter()
        .any(|f| spec(f.weapon_id).is_some_and(WeaponSpec::has_gauge))
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
        // TAU, the Sentient type — "neutral to all health types, meaning its
        // damage is neither increased or decreased against any target"
        // (wiki `Damage/Tau Damage`), which is the Void's rule and is why the
        // enum has carried the variant since before a weapon dealt it. The
        // Haalvu is the first player weapon that does. Its STATUS is Status
        // Chance Vulnerability (+10% received status a stack, ten stacks, 8 s)
        // and this engine has no debuff for it — the weapon's own card says so.
        "tau" => DamageType::Tau,
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
        // THE WHOLE ENUM, because a weapon may ship any of them and this
        // panicked on four it could not spell. The Haalvu is the one that found
        // it: its EXILUS slot is Universal — the module says so and the page
        // does not mention the slot at all — and this roster had copied the
        // weapon's own Madurai into it, so the exilus mod was charged full
        // drain unless it happened to be Madurai (2026-08-21).
        "zenurik" => Polarity::Zenurik,
        "unairu" => Polarity::Unairu,
        "penjaga" => Polarity::Penjaga,
        // A SLOT polarity only — no mod carries it, and the enum spells it `Omni`
        // after the Forma that grants it. `mods::slot_drain` already
        // knew about it; only this parser did not.
        "universal" => Polarity::Omni,
        // …and the other slot-only one, which matches nothing at all.
        "aura" => Polarity::Aura,
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

    // A BLAST THAT LANDS BEHIND WHAT YOU SHOT. Said out loud for the same
    // reason the headshot line below is: it is the other way a number comes out
    // SMALLER than a reader expects, and an unexplained drop reads as a bug in
    // the sim rather than as the weapon. Both halves are on the line, because
    // the trade is the whole point — punch through buys bodies and costs the
    // explosion (MEASUREMENTS M53).
    if s.attack.radial.as_ref().map(|r| r.blast_kind) == Some(BlastKind::Terminal) {
        out.push(
            "Its round bores THROUGH what it hits and explodes where it finally stops, so unlike other explosive weapons it takes punch-through mods normally — and they cut both ways: in a crowd the blast lands on whichever enemy the round could not get out of, deeper down the line, while against a lone enemy it is carried PAST the target and away from it."
                .to_string(),
        );
    }

    // A HEAD THIS WEAPON DOES NOT CARE ABOUT. Said out loud because it is the
    // one weapon stat that makes a number SMALLER than the reader expects, and
    // an unexplained small number reads as a bug in the sim rather than as the
    // weapon. Both halves are on the line: the multiplier is the weapon's, and
    // a headshot mod still pays — which is what keeps Primary Deadhead worth
    // fitting on a gun whose own head bonus is nothing.
    if let Some(m) = s.headshot_multiplier {
        out.push(format!(
            "A headshot with this weapon is worth {m:.0}x rather than the enemy body part's own multiplier, so aiming for the head buys nothing by itself. Headshot mods still apply on top of it, and a critical headshot does not double its critical damage."
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

    // THE SHOT COMBO COUNTER and THE SCOPE, both stated because both are
    // silent otherwise: neither is a stat on the panel and neither is a mod, so
    // a player reading a sniper's damage has no way to see that two of its
    // factors are the weapon's own.
    if let Some(c) = s.sniper_combo {
        out.push(format!(
            "Scoped in, consecutive hits build a Shot Combo Counter: {} landing {} multiply damage by 1.5x, and every threefold count past that adds another 0.5x. It drops by one for every {:.0} s without a hit, and it pays nothing at all from the hip.",
            c.min,
            if c.min == 1 { "hit" } else { "hits" },
            c.seconds
        ));
    }
    if let Some(z) = s.scope {
        // THE SENTENCE NAMES WHAT THE SCOPE ACTUALLY PAYS. It printed
        // `headshot_damage` whatever the grant was, so eight of the ten scoped
        // weapons in the roster read "+0% headshot damage" on a scope granting
        // +50% critical damage (2026-08-20). A scope grants exactly ONE of the
        // four fields — that is why they are four fields — so the first
        // non-zero one is the grant.
        let (fraction, granted) = if z.headshot_damage != 0.0 {
            (z.headshot_damage, "headshot damage, additive with headshot mods")
        } else if z.crit_multiplier != 0.0 {
            (z.crit_multiplier, "critical damage")
        } else if z.crit_chance != 0.0 {
            (z.crit_chance, "critical chance, relative to the unmodded base")
        } else {
            (
                z.crit_chance_post_mod,
                "critical chance, applied after mods",
            )
        };
        // A MAGNIFICATION IS ONLY QUOTED WHEN THE PAGE PUBLISHES ONE. The
        // Vesper 77's aim bonus rides a laser sight and its page states no zoom
        // level at all, so the clause about trading field of view for
        // magnification has nothing to be about.
        out.push(match z.magnification {
            Some(magnification) => format!(
                "Its scope's top zoom ({magnification:.1}x) grants +{:.0}% {granted} while aiming. This arena has no field of view to trade for magnification, so the scope is always at that level.",
                fraction * 100.0
            ),
            None => format!(
                "Aiming grants +{:.0}% {granted}. The page publishes no zoom level for it, and this arena aims by default.",
                fraction * 100.0
            ),
        });
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

    // NOT `no_resupply`. It was listed here and taken out (owner, 2026-08-05):
    // every ground Arch-Gun is removed when its reserve runs out, so it says
    // nothing about THIS weapon. A line that is true of a whole class does not
    // belong on the entry for one member of it — it reads as a distinguishing
    // feature and distinguishes nothing.
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
    pub delay_empty_seconds: f64,
    /// …and with rounds still in it (0.4 s).
    pub delay_partial_seconds: f64,
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
    /// How far a tendril reaches, metres. Twenty on the Ocucor, and Ruinous
    /// Extension extends it — a mod this engine has never had anything for.
    #[serde(default = "tendril_range_default")]
    pub range_m: f64,
    /// How far off the reticle a tendril will ACQUIRE a body, degrees. Forty on
    /// the Ocucor; it then holds to sixty, which nothing here needs because
    /// nobody moves.
    #[serde(default = "tendril_cone_default")]
    pub acquire_deg: f64,
}

fn tendril_range_default() -> f64 {
    20.0
}
fn tendril_cone_default() -> f64 {
    40.0
}

/// THE SHOT COMBO COUNTER — a sniper rifle's own damage multiplier, and the one
/// mechanic in the game that is a WEAPON's and not a build's.
///
/// VERBATIM (wiki `Sniper Rifle` §Shot Combo Counter): *"Each Sniper Rifle
/// requires a minimum number of shots, referred to as Minimum Combo, before the
/// Shot Combo Counter activates, starting with a damage bonus of 1.5x. Another
/// 0.5x damage is added to the counter each time the Shot Combo Counter reaches
/// a number of hits three times the amount needed for the previous damage bonus
/// milestone"* — so the thresholds are `min * 3^k` and the multiplier
/// `1.5 + 0.5k`, which [`SniperCombo::multiplier`] walks rather than computing
/// through a logarithm: `log3` of an exact power of three is not exactly an
/// integer in binary, and the floor of it is off by one wherever it lands
/// short.
///
/// *"The Shot Combo Counter will be reduced by 1 after a short period of time
/// that no successful hits have been made, or if the player misses a shot. All
/// sniper rifles have a 2 second combo duration, with the exception of the
/// Lanka, which has a 6 second combo duration."* It DECAYS one at a time; it
/// does not reset, which is why a sniper that keeps firing never loses it and
/// one interrupted for a second still has most of it.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct SniperCombo {
    /// "Minimum Combo": landing hits before the counter pays anything at all.
    /// 1 on the Vectis, 5 on the Vectis Prime — the wiki's own table.
    pub min: u32,
    /// Seconds without a landing hit before the counter drops by ONE.
    #[serde(default = "combo_seconds_default")]
    pub seconds: f64,
}

fn combo_seconds_default() -> f64 {
    // "All sniper rifles have a 2 second combo duration, with the exception of
    // the Lanka" — so 2 is the rule and the Lanka states its own.
    2.0
}

impl SniperCombo {
    /// The damage multiplier at `hits`. 1.0 below Minimum Combo — the counter
    /// exists there and pays nothing.
    pub fn multiplier(self, hits: u32) -> f64 {
        if self.min == 0 || hits < self.min {
            return 1.0;
        }
        let mut threshold = u64::from(self.min);
        let mut mult = 1.5;
        while u64::from(hits) >= threshold * 3 {
            threshold *= 3;
            mult += 0.5;
        }
        mult
    }
}

/// THE SCOPE, as the only part of zoom that is a damage number.
///
/// A sniper's zoom levels each carry a buff (wiki `Sniper Rifle` §Zoom Buffs),
/// and the arena models the BUFF while modelling none of the optics: it has no
/// distance and no field of view (docs/UNMODELLED.md), so nothing here is
/// traded for the magnification and the highest level is not a choice — it is
/// strictly better and free. The scope therefore sits at its top level
/// whenever the Tenno is aiming, and that is stated on the weapon's card.
///
/// Only the headshot-damage kind is declared, because it is the only kind the
/// roster's snipers grant. The Lanka's and Komorex's are called out by the same
/// section as exceptions to the additive rule, so a weapon that grants crit
/// chance or a critical multiplier gets its own field when one is added rather
/// than this one reinterpreted.
///
/// *"These zoom buffs, which are intrinsic to the weapon and cannot be
/// modified, generally stack additively with similar buffs from mods"* — so it
/// joins the headshot bracket rather than multiplying it.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct ScopeSpec {
    /// The top zoom level's magnification, carried for the card's sentence.
    ///
    /// `None` where the page publishes none — the Vesper 77's laser sight is an
    /// aim bonus with no stated zoom level, and inventing a 1.0 for it would put
    /// a number on the card that no source ever wrote.
    #[serde(default)]
    pub magnification: Option<f64>,
    /// ...and its headshot-damage bonus at that level, as a fraction. The
    /// Vectis family's kind.
    #[serde(default)]
    pub headshot_damage: f64,
    /// ...or a CRITICAL CHANCE bonus, which is what the Lanka's scope grants
    /// (+20/+30/+50% across its three levels). Relative to the unmodded base,
    /// like every other crit-chance bucket entry.
    #[serde(default)]
    pub crit_chance: f64,
    /// ...or a CRITICAL MULTIPLIER bonus — the Rubico family's kind, and the
    /// Perigale's (+35/+50% and +20/+40%).
    ///
    /// A scope grants exactly ONE of these three in the published table, so the
    /// two it does not grant stay zero. They are separate fields rather than a
    /// kind + value because each lands in a DIFFERENT bucket, and a single
    /// value with a tag would just move the match somewhere less obvious.
    #[serde(default)]
    pub crit_multiplier: f64,
    /// ...or a FLAT critical chance applied AFTER mods, which is the Lanka's
    /// and the reason the mechanic page calls its zoom bonus an exception:
    /// *"The zoom bonus adds a flat +20/30/50 critical chance, applied after
    /// mods"* (wiki `Lanka`). That is a different layer from `crit_chance`
    /// above — a relative bucket term is multiplied by the weapon's base, and
    /// this is added to the finished number — so it needs its own field or the
    /// Lanka's +50% would be worth 50% of 25% instead of 50 points.
    #[serde(default)]
    pub crit_chance_post_mod: f64,
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
    let own = match s.slot.as_str() {
        // "Archguns possess two Arcane Enhancement slots to equip one Primary
        // Arcane and one Secondary Arcane" (wiki, Arch-Gun).
        "archgun" => vec!["primary", "secondary"],
        "primary" => vec!["primary"],
        "secondary" => vec!["secondary"],
        "melee" => vec!["melee"],
        _ => return Vec::new(),
    };
    // A KITGUN SEATS ONE OF ITS OWN AS WELL, and the wiki states it as an
    // "as well" rather than an "instead": *"These can be installed
    // simultaneously with Secondary/Primary arcanes"* (`Kitgun` §Kitgun
    // Arcanes). Filing Pax and Residual under the weapon's own slot made the
    // two compete for one seat, so the page asked the reader to choose between
    // a Pax Charge and a Primary Merciless — a choice the game never puts to
    // them (owner, 2026-08-23).
    //
    // FIRST, because it is the seat this weapon has that no other weapon does:
    // the ordinary one is the same seat every gun in the roster carries, and
    // putting the distinctive one after it reads as an afterthought.
    if s.kitgun.is_some() {
        let mut out = vec!["kitgun"];
        out.extend(own);
        return out;
    }
    own
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
/// The `independent_procs:` ids this entry declares, as a static slice.
///
/// Leaked through a cache exactly like [`traits_for`], and for the same reason:
/// a `WeaponBase` is built per request and the ids come from a yaml the loader
/// owns for the life of the process. The set of legal ids is validated HERE, so
/// a typo fails at load with the whole roster's names rather than silently
/// applying nothing in a fight.
fn independent_procs_for(s: &WeaponSpec) -> &'static [&'static str] {
    static CACHE: OnceLock<Mutex<BTreeMap<String, &'static [&'static str]>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut g = cache.lock().expect("independent proc cache");
    if let Some(t) = g.get(&s.id) {
        return t;
    }
    let out: Vec<&'static str> = s
        .independent_procs
        .iter()
        .map(|p| match p.as_str() {
            "lifted" => "lifted",
            other => panic!(
                "{}: unknown independent proc `{other}` — the engine implements `lifted`;                  add the effect to dummy::DebuffState before declaring it",
                s.id
            ),
        })
        .collect();
    let leaked: &'static [&'static str] = Box::leak(out.into_boxed_slice());
    g.insert(s.id.clone(), leaked);
    leaked
}

/// The traits a weapon has, for an EQUIP rule — [`traits_for`], public.
pub fn traits_of(s: &WeaponSpec) -> &'static [&'static str] {
    traits_for(s)
}

fn traits_for(s: &WeaponSpec) -> &'static [&'static str] {
    static CACHE: OnceLock<Mutex<BTreeMap<String, &'static [&'static str]>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut g = cache.lock().expect("weapon traits cache");
    if let Some(t) = g.get(&s.id) {
        return t;
    }
    // THE TRIGGER IS THE WEAPON'S, WHICH IS THE GROUP'S DEFAULT FORM — the same
    // entry `mods_data::triggers_of` reads, and for the same reason: a mod's
    // `requires:` is an EQUIP rule, decided once for the weapon, and a mod the
    // weapon may legally wear has to keep working on every form of it.
    //
    // It used to follow `transforms_from`, which reaches only GAUGE-FED forms
    // (an entry may not carry that field without a gauge), so a free alternate
    // fire reported its OWN trigger — and the Tenet Detron found the hole: its
    // primary fire is Semi-Auto and its Mag Burst is not, so Semi-Pistol
    // Cannonade was offered on the weapon, went inert on the alternate form,
    // AND TOOK ITS FIRE-RATE LOCK WITH IT. A mod that locks a stat on one form
    // and not the other is the worst of the three possible answers.
    //
    // The narrow blast radius is what makes this safe: three mods in the whole
    // data set gate on a trigger (the Cannonades, `semi_auto`), and the only
    // entries whose answer moves are ones whose DEFAULT form is semi-auto and
    // whose alternate is not — the Tenet Detron and the Tenet Plinx. Everywhere
    // else `pool_for_build` had already refused the mod, so nothing could reach
    // this gate to change.
    let group = s.transform_group.as_deref().unwrap_or(&s.id);
    let base = all()
        .iter()
        .find(|x| x.transform_group.as_deref().unwrap_or(&x.id) == group && x.default_form)
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
    // MODULAR, which no class can say. A primary Tombfinger is a `rifle` and a
    // secondary one a `pistol` — the same two classes as a Braton and a Lex —
    // and the eight Pax and Residual arcanes go on neither. DERIVED from the
    // one field that makes the weapon modular rather than written on the entry
    // beside it, so a chamber transcribed tomorrow cannot arrive without it.
    if s.kitgun.is_some() {
        out.push("modular");
    }
    let leaked: &'static [&'static str] = Box::leak(out.into_boxed_slice());
    g.insert(s.id.clone(), leaked);
    leaked
}

/// Build the RAW (no evolutions, no mods) [`WeaponBase`] panel for a weapon
/// entry. `frenzy_active` folds passive-granted element injections in
/// (resolved from the transform group's base entry, where passives live).
/// THE SPEC A BUILD ACTUALLY FIRES, with a KITGUN's assembly composed into it.
///
/// Everything that is not modular comes back untouched, and so does a modular
/// weapon with no assembly named — whose numbers are the chamber's `base`
/// PREVIEW, which its own entry says out loud.
///
/// WHY IT REWRITES THE SPEC RATHER THAN THE PANEL. `base_panel` derives a great
/// deal from `s.attack` on the way past — the cone, the falloff, the CO class,
/// the punch-through budget, the radial's own crit and status defaults — so a
/// panel patched afterwards would have to re-derive every one of them, and the
/// ones nobody remembered would silently keep the preview's answer. Composing
/// one layer earlier means NOTHING downstream of this function has to learn
/// what a Kitgun is; it is the same reason an evolution overrides a panel
/// rather than the sim.
///
/// `None` when the assembly names parts that do not compose — a grip from the
/// other slot, a loader that does not exist. A Kitgun that is not assembled has
/// no numbers, and inventing some would be worse than saying so.
pub fn spec_assembled<'a>(
    s: &'a WeaponSpec,
    a: Option<&crate::kitguns_data::Assembly>,
) -> Option<std::borrow::Cow<'a, WeaponSpec>> {
    let Some(chamber_id) = s.kitgun.as_deref() else {
        return Some(std::borrow::Cow::Borrowed(s));
    };
    // A MODULAR WEAPON IS NEVER FIRED AS ITS PREVIEW. The `base` row is the
    // module's no-grip preview and is a stat line no player can reproduce, so
    // a caller that names no assembly gets the DEFAULT one rather than that —
    // which means no path anywhere can produce a preview-based panel by
    // forgetting to pass parts. Six call sites in `webapi` build a base for a
    // request and every one of them would otherwise have had to remember;
    // a shared helper is not enough when the DECISION around it is the thing
    // that goes missing (AGENTS.md, `parse_fight`).
    let owned;
    let a = match a {
        Some(a) => a,
        None => {
            owned = crate::kitguns_data::default_assembly(chamber_id)?;
            &owned
        }
    };
    let built = crate::kitguns_data::assemble(a)?;
    // THE ASSEMBLY MUST BE THIS ENTRY'S. The grip picks the slot and the slot
    // picks the chamber record, so a secondary grip on the primary entry
    // composes a real weapon that is the WRONG one — which is exactly the kind
    // of mismatch that reads as a working build.
    if built.chamber_record_id != chamber_id {
        return None;
    }

    let mut out = s.clone();
    // THE FORM DECIDES WHICH EXPLOSION. A Tombfinger primary explodes
    // differently on a quick shot and on a full charge, and this entry is one
    // of the two; the chamber keys its blasts by the same form ids.
    let blast = built.blasts.get(&s.form);
    if built.blasts.is_empty() != out.attack.radial.is_none() {
        // A chamber that explodes on an entry with no `radial:` (or the other
        // way round) is a roster entry and a parts file that disagree about
        // what the weapon IS, and neither one can be the answer.
        return None;
    }

    // THE DIRECT HIT IS WHAT THE EXPLOSION LEAVES — the whole shot where the
    // explosion is ADDED beside it, and short of it by the carve where it is
    // taken out of it. One field either way, so the two shapes need no branch.
    out.attack.damage = match blast {
        Some(b) => b.direct.clone(),
        None => built.damage.clone(),
    };
    if let (Some(r), Some(b)) = (out.attack.radial.as_mut(), blast) {
        r.damage = b.damage.clone();
        r.radius_m = b.radius_m;
        r.falloff_start_m = Some(0.0);
        r.falloff_reduction = Some(1.0 - b.falloff_to);
    }

    out.attack.fire_rate = built.fire_rate;
    // A CHARGE ONLY WHERE THE CHAMBER HAS ONE, and it is the GRIP's — the one
    // trigger in the roster whose charge belongs to a part rather than to the
    // weapon.
    if built.charge_seconds.is_some() {
        out.attack.charge_seconds = built.charge_seconds;
    }
    out.attack.crit_chance = built.crit_chance;
    out.attack.crit_multiplier = built.crit_multiplier;
    out.attack.status_chance = built.status_chance;
    out.attack.multishot = built.multishot;
    out.attack.ammo_cost = built.ammo_cost;
    out.attack.punch_through_m = built.punch_through_m.unwrap_or(0.0);
    out.attack.spread = Some(SpreadSpec {
        min_deg: built.spread.min_deg,
        max_deg: built.spread.max_deg,
    });
    out.recharge_per_second = built.recharge_per_second;
    out.magazine = Some(built.magazine);
    out.reload_seconds = Some(built.reload_seconds);
    out.ammo_max = Some(built.ammo_max);
    // THE DISPOSITION IS THE ENTRY'S, not the assembly's: it is per chamber AND
    // per slot, and this entry already is one chamber in one slot. Restating it
    // from the parts would be the same number written twice.
    Some(std::borrow::Cow::Owned(out))
}

pub fn base_panel(id: &str, frenzy_active: bool) -> WeaponBase {
    base_panel_assembled(id, frenzy_active, None)
}

/// [`base_panel`], for a MODULAR weapon whose numbers are its assembly's.
///
/// Panics on an assembly that does not compose, the way this function already
/// panics on an unknown weapon id: both are a caller handing over a weapon that
/// does not exist, and a panel invented for one is a build nobody can reproduce.
pub fn base_panel_assembled(
    id: &str,
    frenzy_active: bool,
    assembly: Option<&crate::kitguns_data::Assembly>,
) -> WeaponBase {
    let raw = spec(id).unwrap_or_else(|| panic!("unknown weapon id: {id}"));
    let composed = spec_assembled(raw, assembly).unwrap_or_else(|| {
        panic!("weapon {id}: the assembly {assembly:?} does not compose into it")
    });
    let s = &*composed;

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
            let start = s.reload_start_seconds.unwrap_or(0.0);
            let end = s.reload_end_seconds.unwrap_or(0.0);
            let per = s.reload_per_shell_seconds.unwrap_or_else(|| {
                ((base_reload - start - end) / magazine_size.max(1.0)).max(0.0)
            });
            (start, per, end)
        });

    let gauge_form = s.gauge_form.as_ref().map(|inc| GaugeForm {
        max_charges: inc.gauge.max_rounds,
        charge_on: match inc.gauge.charge_on.as_str() {
            "weakpoint_hits" => ChargeOn::WeakpointHits,
            "direct_hits" => ChargeOn::DirectHits,
            "kills" => ChargeOn::Kills,
            other => panic!("{id}: unknown gauge charge_on: {other}"),
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
            blast_kind: r.blast_kind,
            // Each stat falls back to the direct part's when unstated.
            base_crit_chance: r.crit_chance.unwrap_or(s.attack.crit_chance),
            base_crit_damage: r.crit_multiplier.unwrap_or(s.attack.crit_multiplier),
            base_status_chance: r.status_chance.unwrap_or(s.attack.status_chance),
            radius_m: r.radius_m,
            takes_blast_radius_mods: r.takes_blast_radius_mods,
            falloff_start_m: r.falloff_start_m.unwrap_or(0.0),
            falloff_reduction: r.falloff_reduction.unwrap_or(0.0),
            forced_procs: crate::damage::ForcedProcs::from_types(
                r.forced_procs.iter().map(|t| damage_type(t)),
            ),
            takes_condition_overload: r.takes_condition_overload,
            takes_multishot: r.takes_multishot,
            // AN EXPLOSION STARTS WITH ITS OWN BASE as the CO base, and stays
            // there — see `WeaponBase::add_flat_base_damage`, where the radial's
            // never grows.
            co_base: v.total(),
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
            duration_seconds: f.duration_seconds,
            first_tick_delay_seconds: f.first_tick_delay_seconds,
            forced_procs: crate::damage::ForcedProcs::from_types(
                f.forced_procs.iter().map(|t| damage_type(t)),
            ),
            radius_m: f.radius_m,
            falloff_start_m: f.falloff_start_m.unwrap_or(0.0),
            falloff_reduction: f.falloff_reduction.unwrap_or(0.0),
            takes_condition_overload: f.takes_condition_overload,
            elemental_mods_apply: f.elemental_mods_apply,
            can_crit: f.can_crit,
            status_mods_apply: f.status_mods_apply,
            stacking: match f.stacking.as_str() {
                "stack" => FieldStacking::Stack,
                "refresh" => FieldStacking::Refresh,
                other => panic!("{id}: unknown lingering stacking: {other}"),
            },
        }
    });

    WeaponBase {
        // LEAKED ONCE so the panel can answer "what is this" and "what does it
        // draw" without a lookup — the two questions the Amp auras ask.
        class: Box::leak(s.class.clone().into_boxed_str()),
        slot: Box::leak(s.slot.clone().into_boxed_str()),
        // Filled in by `apply_valence`; zero until then, and zero forever on a
        // weapon that never came out of a Lich.
        valence_bonus: 0.0,
        recharge_per_second: s.recharge_per_second,
        echo_multiplier: s.echo_multiplier,
        mod_pools: Box::leak(
            s.mod_pools
                .iter()
                .map(|p| &*Box::leak(p.clone().into_boxed_str()))
                .collect::<Vec<&'static str>>()
                .into_boxed_slice(),
        ),
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
        //
        // A CROSSBOW IS A BOW FOR THIS ONE RULE. DE files it as its own class
        // and the Attica's and the Zhuge's pages both say, word for word,
        // "Counts as a bow in regards to fire rate mods, doubling the fire rate
        // bonus" — so the match is a SET rather than an equality, and the rest
        // of what `class == "bow"` decides (the draw-only charge cadence) stays
        // the bows' alone, since a crossbow does not charge (2026-08-20).
        independent_procs: independent_procs_for(s),
        fire_rate_mod_multiplier: match s.class.as_str() {
            "bow" | "crossbow" => 2.0,
            _ => 1.0,
        },
        base_multishot: s.attack.multishot,
        buff_multishot_bonus: 0.0,
        buff_multishot_max_stacks: 0,
        magazine_size,
        // The reserve the sim may spend, and the two facts about it. HAVING
        // one is `ammo_max` — derived, because a weapon that states a reserve
        // has a reserve and there is nothing to declare twice. Being able to
        // REFILL it is the weapon's own business and is declared.
        ammo_reserve: s.ammo_max.unwrap_or(0.0),
        has_reserve: s.ammo_max.is_some_and(|a| a > 0.0),
        super_crit_on_status: s.super_crit_on_status,
        tendril_max: s.tendrils.map_or(0, |t| t.max),
        tendril_range_m: s.tendrils.as_ref().map_or(0.0, |t| t.range_m),
        tendril_acquire_deg: s.tendrils.as_ref().map_or(0.0, |t| t.acquire_deg),
        sniper_combo: s.sniper_combo,
        // The scope's bonus rides HERE unconditionally and is spent by
        // `resolve`, which is the only layer that knows whether the Tenno is
        // aiming — and the only one that knows a form cannot zoom.
        scope_headshot_damage: s.scope.map_or(0.0, |z| z.headshot_damage),
        scope_crit_chance: s.scope.map_or(0.0, |z| z.crit_chance),
        scope_crit_multiplier: s.scope.map_or(0.0, |z| z.crit_multiplier),
        scope_crit_chance_post_mod: s.scope.map_or(0.0, |z| z.crit_chance_post_mod),
        // The DEFAULT lives with the ramp it belongs to, so "most weapons"
        // is stated once rather than copied into a second file that is free
        // to drift from it.
        beam_ramp_floor: s.beam_ramp_floor.unwrap_or(crate::dummy::BEAM_RAMP_FLOOR),
        applies_microwave: s.applies_microwave,
        battery: s.battery,
        forced_procs: s.attack.forced_procs.iter().map(|t| damage_type(t)).collect(),
        pellet_elements: s.attack.pellet_elements.iter().map(|t| damage_type(t)).collect(),
        multishot_adds_damage: s.attack.multishot_adds_damage,
        attractor_seconds: s.attack.attractor_seconds,
        no_resupply: s.no_resupply,
        base_reload,
        by_round_reload,
        innate_co_per_type: 0.0,
        gated: Vec::new(),
        tenno_scaled: Vec::new(),
        cannot_zoom: s.cannot_zoom,
        consecutive_hit_damage: None,
        bodyshot_crit_chance_multiplier: 1.0,
        round_restore_on_status: None,
        instant_reload_on_kill: None,
        magazine_growth_on_empty_reload: None,
        evo_weakpoint_crit_chance_relative: 0.0,
        base_status_from_crit: None,
        base_crit_from_status: None,
        base_damage_below_half_health: 0.0,
        crit_chance_on_undamaged: 0.0,
        crit_damage_on_undamaged: 0.0,
        co_behavior,
        // THE ORIGINAL BASE, absolute. A weapon may DECLARE a fraction of its
        // own base here (0.5 on a bow's charged entry) and that is the only
        // place a fraction is written down, because it is how the catalog
        // prints it — everything downstream carries the absolute.
        co_base: vector.total() * s.co_base_fraction.unwrap_or(1.0),
        injected_elements,
        traits: traits_for(s),
        gauge_form,
        radial,
        spread: s.attack.spread,
        // Only an EVOLUTION grants one (Lone Enforcer); no weapon declares it.
        multishot_beyond_range: None,
        falloff: s.attack.falloff.clone(),
        punch_through_m: s.attack.punch_through_m,
        // ONE FACT, TWO SPELLINGS, resolved here rather than left to a reader
        // to notice. `beam.range_m` has carried a beam's reach since the block
        // existed — the Torid Incarnon's 37 m is asserted in this file's own
        // tests — and was read by nothing, so it recorded the number and
        // changed no answer. `attack.range_m` is the general form, for the
        // weapons that have a range and no `beam:` block (the Phantasma says
        // so in its own comment: *"What is lost by omitting it is the RANGE"*).
        // The explicit one wins; a beam's own is the fallback.
        range_m: s
            .attack
            .range_m
            .as_ref()
            .map(RangeSpec::metres)
            .or_else(|| s.attack.beam.as_ref().map(|b| b.range_m))
            .unwrap_or(f64::INFINITY),
        punch_through_mods: s.attack.punch_through_mods,
        compression: s.attack.compression.clone(),
        lingering,
        // The data module's Trigger for a beam. Not cosmetic: it decides
        // whether `fire_rate` means shots or TICKS and whether multishot merges.
        continuous: s.attack.trigger == "held",
        // A BOUNCE IS NOT SCALED BY ANYTHING, so it comes across as written.
        unaimed_headshot_chance: s.attack.unaimed_headshot_chance,
        orb: s.attack.orb,
        ricochet: s.attack.ricochet.as_ref().map(|r| crate::loadout::Ricochet {
            bounces: r.bounces,
            headshot_chance: r.headshot_chance,
            // NO RANGE IS NOT ZERO RANGE — it is the page stating no limit but
            // the count, so an absent field means "the nearest body it has not
            // hit yet, however far".
            range_m: r.range_m.unwrap_or(f64::INFINITY),
        }),
        beam: s.attack.beam.as_ref().map(|b| crate::loadout::BeamGeometry {
            range_m: b.range_m,
            damage_radius_m: b.damage_radius_m,
            radius_takes_multishot: b.radius_takes_multishot,
            chain_hops: b.chain.hops,
            chain_range_m: b.chain.range_m,
            chain_damage_per_hop: b.chain.damage_per_hop,
            chain_compounds: b.chain.compounds,
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
        crit_damage_below_status_count: None,
        // Set by Prelude of Might at `evolutions_data::apply`, read in `resolve`.
        crit_multiplier_below_crit_chance: None,
        // Set by Headcracker at `evolutions_data::apply`.
        // Filled by `evolutions_data::apply`; see `StackingBuff`.
        stacking_buffs: Vec::new(),
        post_mod_crit_chance: 0.0,
        post_mod_status_chance: 0.0,
        // Evolutions ADD to this (Caput Mortuum); a weapon's innate share is
        // the module's `ExtraHeadshotDmg`.
        headshot_damage_bonus: s.headshot_damage_bonus.unwrap_or(0.0),
        headshot_multiplier: s.headshot_multiplier,
        noncrit_bonus: None,
    }
}

#[cfg(test)]
mod inheritance_tests {
    use super::*;

    /// A FORM NEVER RESTATES ITS WEAPON. This is the guard the Larkspur bug
    /// needed and did not have.
    ///
    /// Its alt-fire carried its BASE form's accuracy while its Prime's alt-fire
    /// carried the alt-fire's — one weapon, two entries, two answers, and no
    /// way to notice because nothing knew the two were the same gun. An audit
    /// found it (2026-08-15) among 313 identical values written twice.
    ///
    /// The rule that closes it is not "inherit everything", which would be
    /// wrong — a form legitimately overrides its magazine (the Scourge's throw
    /// holds one round against forty) and its accuracy (a scoped alt-fire is
    /// not a hip-fired beam). The rule is that a form may not state a value
    /// IDENTICAL to its weapon's: a restatement carries no information and is
    /// the only way the two can drift apart.
    #[test]
    fn a_form_states_only_what_differs_from_its_weapon() {
        use serde_norway::Value;
        // Read the FILES rather than the merged specs — after the merge the
        // inherited value and a restated one are the same thing, which is
        // exactly the distinction this asserts.
        let raw: Vec<(&str, Value)> = crate::data::files_under("weapons/")
            .filter(|(p, _)| p.ends_with(".yaml"))
            .map(|(p, text)| (p, serde_norway::from_str::<Value>(text).expect(p)))
            .collect();
        let by_id: std::collections::HashMap<&str, &Value> = raw
            .iter()
            .filter_map(|(_, v)| v.get("id").and_then(Value::as_str).map(|i| (i, v)))
            .collect();

        let mut echoed: Vec<String> = Vec::new();
        for (p, v) in &raw {
            let Some(parent) = v.get("inherits").and_then(Value::as_str) else { continue };
            let up = by_id[parent];
            for k in INHERITED.iter().chain(INHERITED_BLOCKS.iter()) {
                if let (Some(mine), Some(theirs)) = (v.get(*k), up.get(*k)) {
                    if mine == theirs {
                        echoed.push(format!("{p}: `{k}` is its weapon's own value"));
                    }
                }
            }
        }
        assert!(
            echoed.is_empty(),
            "a form restated a value it already inherits — drop the line, and if it              is meant to DIFFER, the value is what is wrong:
  {}",
            echoed.join("
  ")
        );
    }

    /// ...and every form sibling that CAN inherit, does. A weapon whose form
    /// carries a full copy of its metadata is the state this was written to
    /// leave, so a new one is a regression rather than a style choice.
    #[test]
    fn a_form_that_copies_its_weapon_declares_the_inheritance() {
        use serde_norway::Value;
        let raw: Vec<(&str, Value)> = crate::data::files_under("weapons/")
            .filter(|(p, _)| p.ends_with(".yaml"))
            .map(|(p, text)| (p, serde_norway::from_str::<Value>(text).expect(p)))
            .collect();
        // group -> the entry that is the arsenal's form
        let mut head: std::collections::HashMap<&str, &Value> = std::collections::HashMap::new();
        for (_, v) in &raw {
            if v.get("default_form").and_then(Value::as_bool) == Some(true) {
                if let Some(g) = v.get("transform_group").and_then(Value::as_str) {
                    head.insert(g, v);
                }
            }
        }
        let mut copiers: Vec<String> = Vec::new();
        for (p, v) in &raw {
            if v.get("inherits").is_some()
                || v.get("default_form").and_then(Value::as_bool) == Some(true)
            {
                continue;
            }
            let Some(g) = v.get("transform_group").and_then(Value::as_str) else { continue };
            let Some(up) = head.get(g) else { continue };
            let same = INHERITED
                .iter()
                .filter(|k| v.get(**k).is_some() && v.get(**k) == up.get(**k))
                .count();
            // A handful of shared fields is a form describing itself; a dozen
            // is a copy of the weapon.
            if same >= 6 {
                copiers.push(format!("{p}: {same} fields identical to `{g}`"));
            }
        }
        assert!(
            copiers.is_empty(),
            "these forms copy their weapon instead of inheriting it — add              `inherits:` and delete the copies:
  {}",
            copiers.join("
  ")
        );
    }
}

#[cfg(test)]
mod pellet_element_tests {
    use super::*;

    /// SIX MISSILES, SIX ELEMENTS, AND SIX RESOLVES.
    ///
    /// The Arbucep fires six homing missiles at once, each carrying a
    /// different combined element. One blended vector would get the damage
    /// right and everything else wrong — a proc is drawn once per instance, so
    /// six missiles draw six and a blend draws one — so the panel resolves per
    /// element and the fight picks by pellet index.
    #[test]
    fn each_projectile_resolves_its_own_element() {
        let base = crate::loadout::WeaponBase::from_data("arbucep", false, &[]);
        let refs: Vec<&crate::loadout::ModDef> = Vec::new();
        let p = crate::loadout::resolve(&base, &refs, crate::loadout::StackPolicy::Emergent);
        assert_eq!(p.pellet_damage.len(), 6, "six missiles, six vectors");

        use crate::damage::DamageType::*;
        for (i, want) in [Blast, Corrosive, Gas, Magnetic, Radiation, Viral].iter().enumerate() {
            let (direct, radial) = &p.pellet_damage[i];
            assert!(direct.get(*want) > 0.0, "missile {i} carries {want:?}: {direct:?}");
            assert!(radial.get(*want) > 0.0, "...and so does its explosion: {radial:?}");
            // …and ONLY it, unmodded: each missile IS its element rather than
            // a blend containing it.
            assert!(
                (direct.total() - direct.get(*want)).abs() < 1e-9,
                "missile {i} is nothing but {want:?}: {direct:?}"
            );
        }
        // The published per-missile numbers, ground column.
        assert!((p.pellet_damage[0].0.total() - 32.0).abs() < 1e-9);
        assert!((p.pellet_damage[0].1.total() - 228.0).abs() < 1e-9);

        // NOTHING ELSE IN THE ROSTER HAS THEM, which is what keeps six resolves
        // off every other weapon's build.
        let with: Vec<&str> = all()
            .iter()
            .filter(|w| !w.attack.pellet_elements.is_empty())
            .map(|w| w.id.as_str())
            .collect();
        assert_eq!(with, ["arbucep"]);
    }

    /// THE LIST IS THE PROJECTILE COUNT. `multishot` and `pellet_elements`
    /// describe the same six missiles from two directions, and a weapon whose
    /// two disagree would cycle its elements against its own pellet count.
    #[test]
    fn the_element_list_is_as_long_as_the_volley() {
        for w in all() {
            if w.attack.pellet_elements.is_empty() {
                continue;
            }
            assert_eq!(
                w.attack.pellet_elements.len(),
                w.attack.multishot.round() as usize,
                "{}: {} elements against {} projectiles",
                w.id,
                w.attack.pellet_elements.len(),
                w.attack.multishot
            );
            // …and a weapon that lists them is one whose multishot pays in
            // DAMAGE, or the seventh projectile would have no element.
            assert!(
                w.attack.multishot_adds_damage,
                "{}: lists per-projectile elements but lets multishot add projectiles",
                w.id
            );
        }
    }
}

#[cfg(test)]
mod deployment_tests {
    use super::*;

    /// A DEPLOYMENT CHANGES THE DAMAGE. VERBATIM (wiki `Archgun`): *"most Heavy
    /// Weapons (a.k.a. Archguns when used via the Archgun Deployer) have had
    /// their damage doubled"*.
    ///
    /// The axis was built as a SUSTAIN axis on a reading of the two-column
    /// infobox that said "same damage, same crit, same status — only the
    /// sustain differs". Crit, multiplier and status ARE identical in both
    /// columns, which is why the wrong half went unnoticed: three of the four
    /// stats checked out, and the Larkspur Prime posted 112 board rows at half
    /// its ground damage (2026-08-14).
    #[test]
    fn a_deployment_moves_the_damage_and_the_sustain() {
        let ground = |id: &str| crate::loadout::WeaponBase::from_data(id, false, &[]);
        let space = |id: &str| {
            let mut b = ground(id);
            apply_deployment(&mut b, id, "archwing");
            b
        };
        // The entry's own column is Atmosphere, so the ground build is the file
        // and the Archwing one is the override.
        let (g, s) = (ground("larkspur_prime"), space("larkspur_prime"));
        assert_eq!(g.base_vector.total(), 180.0, "the ground column is 20 + 160");
        assert_eq!(s.base_vector.total(), 90.0, "and Archwing is half of it");
        // The sustain half, which was right all along.
        assert_eq!(g.base_reload, 2.5);
        assert_eq!(s.base_reload, 4.5);
        assert!(g.no_resupply && !s.no_resupply, "a ground Arch-Gun cannot be resupplied");
        // What DOES NOT move — and the reason the wrong reading survived.
        assert_eq!(g.base_crit_chance, s.base_crit_chance);
        assert_eq!(g.base_status_chance, s.base_status_chance);

        // EVERY ATTACK PART. The alt-fire form doubles its impact AND its
        // explosion; a multiplier that reached only the bullet would leave the
        // explosion at half of what the same infobox prints beside it.
        let (gc, sc) = (ground("larkspur_prime_charged"), space("larkspur_prime_charged"));
        assert_eq!(gc.base_vector.total(), 840.0);
        assert_eq!(sc.base_vector.total(), 420.0);
        let rad = |b: &crate::loadout::WeaponBase| b.radial.as_ref().expect("it explodes").base_vector.total();
        assert_eq!(rad(&gc), 1600.0);
        assert_eq!(rad(&sc), 800.0);

        // ...and a weapon with one deployment is untouched by the axis, which
        // is what keeps this off the rest of the roster.
        let mut torid = ground("torid");
        let before = torid.base_vector.total();
        apply_deployment(&mut torid, "torid", "archwing");
        assert_eq!(torid.base_vector.total(), before);
    }

    /// NO HALF-APPLIED COLUMN. A deployment that restates the direct damage
    /// must restate every OTHER attack part the entry has, or the explosion is
    /// left on the tab the bullet just left — which is the same shape of error
    /// as the one that started this, one level down.
    ///
    /// Checked over the whole roster rather than on the two entries that have
    /// a deployment today, because the next Arch-Gun with an explosion is the
    /// one that would get it wrong.
    #[test]
    fn a_deployment_restates_every_attack_part_or_none() {
        for w in all() {
            for (name, d) in &w.deployments {
                if d.damage.is_none() {
                    continue;
                }
                assert_eq!(
                    w.attack.radial.is_some(),
                    d.radial_damage.is_some(),
                    "{}/{name}: the entry has a radial and this column does not restate it",
                    w.id
                );
                assert_eq!(
                    w.attack.lingering.is_some(),
                    d.lingering_damage.is_some(),
                    "{}/{name}: the entry has a lingering field and this column does not restate it",
                    w.id
                );
            }
        }
    }

    /// THE DOUBLING IS NOT A RULE, so this does not assert it.
    ///
    /// The wiki says *"MOST Heavy Weapons … have had their damage doubled"*, and
    /// the roster proves "most" is doing real work: the Phaedra is x2.071, the
    /// Dual Decurion x1.727, and the Cyngas doubles its TOTAL while changing its
    /// split (39.6/39.6/40.8 becomes an even 80/80/80, so Slash goes x1.961 and
    /// the other two x2.020). The Prisma Dual Decurions is exactly x2 off the
    /// SAME Archwing vector its ordinary is x1.727 off. No single multiplier
    /// expresses this class, which is why both columns are transcribed per
    /// entry (owner, 2026-08-14).
    ///
    /// What IS worth asserting is that a transcription slip cannot pass. A
    /// dropped digit, a factor of ten, a column pasted into the wrong file: all
    /// of those land far outside the band the game's own numbers occupy, and a
    /// ground column that came out SMALLER than its Archwing one would be the
    /// original bug with the two tabs swapped.
    #[test]
    fn every_stated_column_is_in_the_band_the_game_uses() {
        let mut seen = 0;
        for w in all() {
            let Some(d) = w.deployments.get("archwing") else { continue };
            let Some(m) = d.damage.as_ref() else { continue };
            let space: f64 = m.values().sum();
            let ground: f64 = w.attack.damage.values().sum();
            assert!(space > 0.0, "{}: a stated column with no damage in it", w.id);
            let r = ground / space;
            // THE OBSERVED RANGE, after reading all twenty pages: 1.000 (Corvas
            // Prime, whose two tabs are identical) through 1.489 (Kuva
            // Grattler) and 1.5 (Grattler, Mausolon) up to 2.071 (Phaedra).
            // The invariant worth asserting is only the DIRECTION — a ground
            // column is never weaker than its Archwing one — plus a ceiling
            // that a dropped digit or a factor of ten cannot pass.
            assert!(
                (1.0..=2.5).contains(&r),
                "{}: ground is x{r:.3} of Archwing ({ground} against {space}).                  Under 1.0 means the two tabs are swapped; over 2.5 is a                  transcription slip. The game's observed range is 1.000-2.071.",
                w.id
            );
            seen += 1;
        }
        assert!(seen >= 8, "every Arch-Gun states both columns: only {seen} did");
    }
}

#[cfg(test)]
mod sniper_tests {
    use super::*;

    /// THE WIKI'S OWN TABLE, transcribed off the Vectis Prime page:
    ///
    /// | tier | multiplier | hits |
    /// |------|-----------|------|
    /// | 1 | 1.5x | 5    |
    /// | 2 | 2.0x | 15   |
    /// | 3 | 2.5x | 45   |
    /// | 4 | 3.0x | 135  |
    /// | 5 | 3.5x | 405  |
    /// | 6 | 4.0x | 1215 |
    ///
    /// Its last two rows are 3675 and 11025, which are NOT `5 * 3^k` (3645 and
    /// 10935) — the page's table disagrees with the page's own formula from
    /// tier 7 up. The formula is implemented, because it is the rule and the
    /// table looks like an arithmetic slip; the divergence is recorded here
    /// rather than in a comment nobody runs, and it is unreachable either way
    /// (3645 landing hits is over half an hour of a two-round magazine).
    #[test]
    fn the_combo_ladder_is_the_wikis() {
        let vp = SniperCombo { min: 5, seconds: 2.0 };
        for (hits, want) in [
            (0u32, 1.0),
            (4, 1.0),
            (5, 1.5),
            (14, 1.5),
            (15, 2.0),
            (44, 2.0),
            (45, 2.5),
            (135, 3.0),
            (405, 3.5),
            (1215, 4.0),
        ] {
            assert!(
                (vp.multiplier(hits) - want).abs() < 1e-12,
                "{hits} hits: {}, want {want}",
                vp.multiplier(hits)
            );
        }
        // The Vectis pays from the FIRST hit — the smallest minimum in the
        // game, and the reason the ordinary gun is not simply the worse one.
        let v = SniperCombo { min: 1, seconds: 2.0 };
        assert_eq!(v.multiplier(0), 1.0);
        assert!((v.multiplier(1) - 1.5).abs() < 1e-12);
        assert!((v.multiplier(3) - 2.0).abs() < 1e-12);
        assert!((v.multiplier(9) - 2.5).abs() < 1e-12);
        // A power of three is the case a `log3` implementation gets wrong: the
        // floor lands one short wherever the division rounds down.
        // 3^10, and 1.5 + 0.5*10 = 6.5.
        assert!((v.multiplier(59_049) - 6.5).abs() < 1e-12);
    }

    /// The roster's snipers carry both mechanics, with the wiki's numbers, and
    /// nothing else in the roster carries either — a mechanic keyed on a class
    /// the engine does not know is a mechanic that leaks.
    #[test]
    fn the_roster_declares_them_where_the_wiki_does() {
        let combo = |id: &str| spec(id).and_then(|w| w.sniper_combo);
        assert_eq!(combo("vectis").map(|c| c.min), Some(1));
        assert_eq!(combo("vectis_prime").map(|c| c.min), Some(5));
        // 2 s unless the weapon says otherwise (the Lanka, which is not here).
        assert_eq!(combo("vectis_prime").map(|c| c.seconds), Some(2.0));
        let scope = |id: &str| spec(id).and_then(|w| w.scope);
        assert_eq!(scope("vectis").map(|z| z.headshot_damage), Some(0.5));
        assert_eq!(scope("vectis_prime").map(|z| z.headshot_damage), Some(0.6));
        // THE COMBO IS THE SNIPER'S; THE SCOPE IS NOT. A Shot Combo Counter is
        // keyed on the class in game — the wiki's rule opens "Scoped in" and
        // the mechanic exists on no other family — so a combo outside the class
        // is a leak. A `scope:` is keyed on the fight's AIMING state and on
        // nothing else, which is why the Vesper 77's laser sight (+40%
        // critical damage while aiming, no published magnification) is one on a
        // pistol: same bucket, same gate, no scope on the gun (2026-08-20).
        for w in all() {
            if w.sniper_combo.is_some() {
                assert_eq!(w.class, "sniper", "{} is not a sniper rifle", w.id);
            }
        }
        // …and every scope is either a sniper's or names itself in prose, so a
        // scope that turns up on an ordinary weapon by accident is still caught.
        for w in all() {
            if let Some(z) = w.scope {
                assert!(
                    w.class == "sniper" || z.magnification.is_none(),
                    "{}: a non-sniper scope must be an aim bonus with no published zoom",
                    w.id
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {

    /// EVERY ENTRY'S RANGE PAGE HAS BEEN OPENED, and this is what says so.
    ///
    /// It began as a ratchet counting how many entries said nothing — 209 of
    /// 224 — and a shrinking number is a poor guarantee: it cannot tell "we
    /// read the page and it states no range" from "nobody has looked". Now that
    /// all 224 have been read, the invariant is the stronger one and it holds
    /// for weapons added tomorrow: an entry either STATES a range or is
    /// RECORDED in `data/surveys/weapon_range.yaml` as having been read.
    ///
    /// ABSENCE STILL MEANS UNLIMITED at runtime — 101 entries' pages really do
    /// state no reach, which is a fact about the wiki rather than a gap in us,
    /// and `infinite` is a claim the page has to make before we write it. What
    /// is no longer possible is an entry nobody has checked.
    ///
    /// The worksheet is read HERE and by nothing else, which is the same shape
    /// `data/rivens/pools.yaml` has: the rules decide, the survey checks.
    /// EVERY `internal_name` IS ONE THE EXPORT ACTUALLY HOLDS.
    ///
    /// It is the only join between this data and its cross-check source —
    /// `internal_name` == WFCD's `uniqueName`, never the display name, because
    /// the export carries stale duplicates sharing one (data/README.md). Every
    /// weapon yaml written since then opens with "cross-checked against WFCD
    /// warframe-items — 0 disagreements", and a key that resolves to NOTHING
    /// produces exactly that sentence out of a comparison that never ran.
    ///
    /// The Hema is why this exists: it shipped
    /// `/Lotus/Weapons/Infested/InfWFAccompanyingPri/...` against DE's
    /// `/Lotus/Weapons/Infested/LongGuns/InfWFAccompanyingPri/...` — one path
    /// segment short — so every sweep since the roster began skipped that
    /// weapon in silence (2026-08-20). No earlier check could see it, because
    /// each of them asked about the weapons it could FIND.
    ///
    /// The survey is generated by `scripts/survey_internal_names.py` and cannot
    /// record a key that joins to nothing — it REFUSES to write instead. So the
    /// test is the other half: every entry that states a key must be IN the
    /// survey, which is what makes a newly-mistyped one fail here rather than
    /// disappear from a file nobody re-reads. An entry with no key at all is a
    /// FORM, which inherits its weapon's and states only what differs.
    #[test]
    fn every_internal_name_resolves_in_the_export() {
        let raw = crate::data::file("surveys/internal_names.yaml")
            .expect("data/surveys/internal_names.yaml — run scripts/survey_internal_names.py");
        let doc: serde_norway::Value = serde_norway::from_str(raw).expect("the survey parses");
        let resolved = doc
            .get("resolved")
            .and_then(|m| m.as_mapping())
            .expect("the survey has a `resolved:` mapping");
        assert!(resolved.len() >= 200, "the survey looks empty: {} rows", resolved.len());

        // THE RAW YAML, not the spec: `internal_name` is metadata a FORM
        // inherits through the raw merge and no `WeaponSpec` field holds it, so
        // reading the spec would find nothing to check.
        let mut stated = 0usize;
        let mut unsurveyed: Vec<String> = Vec::new();
        let mut disagreed: Vec<String> = Vec::new();
        let mut seen: std::collections::BTreeSet<String> = Default::default();
        for (path, text) in crate::data::files_under("weapons/") {
            let doc: serde_norway::Value =
                serde_norway::from_str(text).unwrap_or_else(|e| panic!("{path}: {e}"));
            let Some(key) = doc.get("internal_name").and_then(|v| v.as_str()) else {
                continue;
            };
            let id = doc
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| panic!("{path} has no id"))
                .to_string();
            stated += 1;
            match resolved
                .get(serde_norway::Value::String(id.clone()))
                .and_then(|v| v.as_str())
            {
                None => unsurveyed.push(format!("{id} -> {key}")),
                Some(surveyed) if surveyed != key => {
                    disagreed.push(format!("{id}: yaml {key} / survey {surveyed}"))
                }
                Some(_) => {}
            }
            seen.insert(id);
        }
        assert!(
            unsurveyed.is_empty(),
            "{} entry/entries state an `internal_name` the survey does not hold — either \
             the key is a typo (the survey refuses to record one that joins to nothing) or \
             the survey is stale: run scripts/survey_internal_names.py\n  {}",
            unsurveyed.len(),
            unsurveyed.join("\n  ")
        );
        assert!(
            disagreed.is_empty(),
            "{} entry/entries disagree with the survey — the key moved and the survey did \
             not, or the other way round:\n  {}",
            disagreed.len(),
            disagreed.join("\n  ")
        );
        // …and the survey may not keep rows for entries that no longer state a
        // key, which is the direction that would otherwise let a deleted row
        // vouch for a weapon nobody has looked at.
        let orphans: Vec<&str> = resolved
            .iter()
            .filter_map(|(k, _)| k.as_str())
            .filter(|k| !seen.contains(*k))
            .collect();
        assert!(orphans.is_empty(), "the survey names entries the roster lost: {orphans:?}");
        assert_eq!(stated, resolved.len(), "survey row count vs entries that state a key");
    }

    #[test]
    fn every_entry_has_had_its_range_page_opened() {
        let raw = crate::data::file("surveys/weapon_range.yaml")
            .expect("data/surveys/weapon_range.yaml");
        let doc: serde_norway::Value = serde_norway::from_str(raw).expect("the worksheet parses");
        let checked = doc
            .get("checked")
            .and_then(serde_norway::Value::as_mapping)
            .expect("a `checked:` mapping");
        let silent: Vec<&str> = super::all()
            .iter()
            .filter(|s| {
                let stated = s.attack.range_m.is_some()
                    || s.attack.beam.as_ref().is_some_and(|b| b.range_m.is_finite());
                let read = checked.contains_key(serde_norway::Value::String(s.id.clone()));
                !stated && !read
            })
            .map(|s| s.id.as_str())
            .collect();
        assert!(
            silent.is_empty(),
            "{} entries have neither a range nor a line in              data/surveys/weapon_range.yaml. Open the wiki page, write the number              into the weapon, or record that the page states none:
  {}",
            silent.len(),
            silent.join("
  "),
        );
    }

    /// EVERY CO ANOMALY IN THE ROSTER IS ON THIS LIST, and the list is the
    /// catalog. Nothing else may be anything but ordinary.
    ///
    /// The rule (owner, 2026-08-12) Ordinary has a definition — direct
    /// hits only, 100% of the base, added to the base-damage bucket — and
    /// a shared Genesis does not make one weapon.
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
            // Rocket Impact; the base Akarius has no row and stays ordinary.
            ("akarius_prime", "independent", 1.0),
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
            // THE KUVA BATCH (2026-08-19). Four of the sixteen have a row and
            // the rest are ordinary; each is transcribed for the entry the
            // catalog NAMES, never generalised to the family.
            //   Kuva Seer | Projectile Impact | Projectile | 131 | 131 | 100% | Multiplying
            ("kuva_seer", "independent", 1.0),
            //   Kuva Drakgoon | Charged Attack | Projectile | 460 | 230 | 50% | Adding
            //   "CO-bonus only applies to base (uncharged) damage; uses bows
            //    mechanics; bows have innate 2x damage multiplier when fully
            //    charged" — which is why the fraction is exactly a half and the
            //    catalog's own two damage columns are 460 and 230. The TAPPED
            //    shot has no row and stays ordinary.
            ("kuva_drakgoon", "additive_with_base_damage", 0.5),
            // THE TENET AND CODA BATCH (2026-08-20). Twenty weapons, and the
            // two catalog tables name SEVEN of their attacks between them —
            // five that are anomalies and reach this list, and two that the
            // catalog checked and called ordinary (both Tenet Detron rows,
            // `Adding` at 100%), which is a row worth transcribing into the
            // weapon and worth nothing here.
            //   Coda Bassocyst | Normal Attack | Projectile | 808 | 808 | 100% | Multiplying
            // …and its ALT-FIRE has the opposite row — `303 | 0 | 0% | N/A,
            // "Does not apply"` — which would be `inert` and has no entry,
            // because that alt fire is a Mercy-finisher tool rather than a way
            // to fight and is recorded as a gap instead.
            // THE NINETEEN BASE WEAPONS behind the adversary families
            // (2026-08-20). Four of them have rows and the rest are ordinary,
            // and the pattern does NOT follow the family: on the Arca Plasmor
            // and the Hema both variants are named, on the Bubonico only the
            // BASE is, and on the Ferrox only the TENET. A row is transcribed
            // for the entry the catalog names, every time.
            //   Arca Plasmor | Normal Attack | Projectile | 600 | 600 | 100% | Multiplying
            ("arca_plasmor", "independent", 1.0),
            //   Hema | Normal Attack | Projectile | 47 | 47 | 100% | Multiplying
            ("hema", "independent", 1.0),
            //   Bubonico | Main-fire | Projectile | 287 | 287 | 100% | Multiplying
            //   Bubonico | Alt-fire  | Projectile |   9 |   9 | 100% | Multiplying
            // …and the CODA Bubonico has neither, which is the asymmetry.
            ("bubonico", "independent", 1.0),
            ("bubonico_burst", "independent", 1.0),
            // AND TWO THE CATALOG NAMES THAT NO ENTRY CAN TAKE:
            //   Tysis | Normal Attack | Projectile | 49 | 49 | 100% | Adding
            //     — Adding at 100% IS ordinary, so it is not an anomaly.
            //   Pox   | DoT Cloud     | AoE        | 20 | 50 | 250% | Adding
            //     — about the CLOUD, whose term reads 50 against its own 20.
            //     There is no per-part CO fraction, so the cloud takes the
            //     ordinary 100% and the weapon's `unmodeled:` says so.
            //   Catabolyst | Partial Reload Impact    | 11 | 11 | 100% | Multiplying
            //   Catabolyst | Reload From Empty Impact | 11 | 11 | 100% | Multiplying
            //     — both name the thrown grenade's contact hit, which the
            //     engine cannot fire because it happens on a RELOAD.
            ("coda_bassocyst", "independent", 1.0),
            //   Coda Hema | Normal Attack | Projectile | 52 | 52 | 100% | Multiplying
            ("coda_hema", "independent", 1.0),
            //   Tenet Arca Plasmor | Normal Attack | Projectile | 760 | 760 | 100% | Multiplying
            // The ORDINARY Arca Plasmor has no row and stays Additive.
            ("tenet_arca_plasmor", "independent", 1.0),
            //   Tenet Plinx | Alt Fire Impact | Projectile | 1000 | 1000 | 100%
            //     | Multiplying | "Scales properly with magazine size"
            // The Base Damage cell is 1000 rather than the infobox's 100, which
            // is the catalog independently confirming that this attack's damage
            // is multiplied by the magazine. The PRIMARY fire has no row.
            ("tenet_plinx_charged", "independent", 1.0),
            //   Tenet Spirex | Slug Impact | Projectile | 120 | 120 | 100% | Multiplying
            // The cell is the slug's own 120, which is what tells the row apart
            // from the 80 explosion beside it.
            ("tenet_spirex", "independent", 1.0),
            // AND ONE THE CATALOG NAMES THAT THIS ROSTER CANNOT TAKE:
            //   Tenet Ferrox | Hitscan AoE Direct | AoE | 60 | 200 | 333%
            //     | Adding | "Radial hit receives CO bonus on direct hit only"
            // It is about the RADIAL, whose term reads the DIRECT hit's base of
            // 200 against its own 60. `co_base_fraction` is one number per
            // ENTRY and that entry's direct hit is ordinary, so the radial takes
            // no Condition Overload at all — the conservative reading, stated in
            // the weapon's own `unmodeled:`.
            ("shedu", "independent", 1.0),
            // The row is `Blob Impact | 0% | Does not apply`, and its unmodded 4
            // names the BASE form (the Incarnon deals 50).
            ("stug", "inert", 1.0),
            ("torid", "independent", 1.0),
            // THE SPEARGUNS' THROW, and the row's Attack Name cell is what
            // scopes it: `Throw` on both, so the PRIMARY FIRE entries have no
            // row and stay ordinary. The Unmodded Damage cells are each throw's
            // own total — 150 and 200 — which is what tells the row apart from
            // the explosion beside it.
            ("scourge_thrown", "independent", 1.0),
            ("scourge_prime_thrown", "independent", 1.0),
            // THE ARCH-GUNS. Four rows in the catalog reach the roster, and
            // three of the four are about telling near-identical entries apart:
            //   Grattler       | Normal attack | Projectile | 100% | Multiplying
            //   Larkspur Prime | Alt-fire      | Projectile | 100% | Multiplying
            // The Grattler's row names the ORDINARY weapon, so the Kuva
            // Grattler has none and stays additive — a shared Genesis does not
            // make a family one weapon. The Larkspur Prime's row names its
            // ALT-FIRE, so the Prime's normal fire has none, and the ordinary
            // Larkspur has none on EITHER form. Both asymmetries are data.
            ("grattler", "independent", 1.0),
            // Arbucep | Direct Hit | Projectile | 100% | Multiplying, with the
            // note "Consistent on all 6 projectiles … Does not apply to the
            // 228 damage AoE" — the second half is the engine's standing rule.
            ("arbucep", "independent", 1.0),
            ("larkspur_prime_charged", "independent", 1.0),
            // THE CHARGE ARCH-GUNS. The catalog carries a row per FORM here,
            // which is what makes the Mandonel the sharpest entry in it:
            //   Velocitus    | Uncharged attack | Projectile | 100% | Multiplying
            //   Velocitus    | Charged attack   | Projectile | 100% | Multiplying
            //   Corvas Prime | Uncharged Attack | Projectile | 100% | Multiplying
            //   Corvas Prime | Charged Attack   | Projectile | 100% | Multiplying
            //   Mandonel     | Uncharged Attack | Projectile | 100% | Multiplying
            //   Mandonel     | Charged attack   | Hitscan    | 100% | ADDING
            // One weapon, two rows, two answers — so the Mandonel's charged
            // form is ordinary and does not appear here, while its uncharged
            // one does. The ORDINARY Corvas is named on neither row and is
            // ordinary on both forms.
            ("velocitus", "independent", 1.0),
            ("velocitus_uncharged", "independent", 1.0),
            ("corvas_prime", "independent", 1.0),
            ("corvas_prime_uncharged", "independent", 1.0),
            ("mandonel_uncharged", "independent", 1.0),

            // THE 2026-08-20 SWEEP, and the reason it found so many at once:
            // every one of these was filed ORDINARY because the check for a row
            // had been run against docs/CATALOGS.md — our own transcription,
            // which by construction only ever carried the rows the roster already
            // had. Reading the WIKI PAGE instead turned up thirty-five entries the
            // Attack Catalog names and the roster contradicted, a third of them
            // weapons that had been here for months (the Lanka at 38%, both Laser
            // Rifles, the Cernos family at 50%).
            //
            // A "Multiplying" row is `independent`; the relative column is
            // `co_base_fraction`, and 100% leaves the field off.
            ("acceltra", "additive_with_base_damage", 0.743),
            ("aeolak", "independent", 1.0),
            ("aeolak_alt", "independent", 1.0),
            ("alternox", "independent", 1.0),
            ("alternox_prime", "independent", 1.0),
            ("basmu", "independent", 1.0),
            ("battacor", "independent", 1.0),
            ("buzlok", "independent", 1.0),
            ("buzlok_beacon", "independent", 1.0),
            ("cernos", "additive_with_base_damage", 0.5),
            ("cinta", "independent", 1.0),
            ("cinta_charged", "independent", 1.0),
            ("daikyu_prime", "additive_with_base_damage", 0.5),
            ("drakgoon", "additive_with_base_damage", 0.57),
            ("epitaph", "independent", 1.0),
            ("evensong", "additive_with_base_damage", 0.65),
            ("exergis", "independent", 1.0),
            ("fulmin_semi", "independent", 1.0),
            ("harpak_harpoon", "independent", 1.0),
            ("javlok", "independent", 1.0),
            ("lanka", "additive_with_base_damage", 0.38),
            ("laser_rifle", "independent", 1.0),
            ("mutalist_cernos", "additive_with_base_damage", 0.5),
            ("mutalist_cernos_uncharged", "independent", 1.0),
            ("nataruk_perfect", "independent", 1.0),
            ("paracyst_harpoon", "independent", 1.0),
            ("prime_laser_rifle", "independent", 1.0),
            ("quellor_alt", "independent", 1.0),
            ("rakta_cernos", "additive_with_base_damage", 0.5),
            ("seer", "independent", 1.0),
            ("stahlta", "independent", 1.0),
            ("stahlta_charged", "independent", 1.0),
            ("steflos", "independent", 1.0),
            ("tenet_envoy", "independent", 1.0),
            ("trumna_grenade", "independent", 1.0),
            //
            // AND A SECOND PASS THE SAME DAY. The first reconciliation matched a
            // row to a form through a short list of attack NAMES, and the
            // catalog names an attack the way that WEAPON's page does — so
            // "Projectile Impact", "Direct Hit", "Lock-On Mode", "Slug Impact"
            // and "Reload From Empty Impact" all missed silently. Nine more:
            ("aegrit", "independent", 1.0),
            ("catabolyst", "independent", 1.0),
            ("cyanex", "independent", 1.0),
            ("cyanex_burst", "independent", 1.0),
            ("epitaph_uncharged", "independent", 1.0),
            ("sepulcrum", "independent", 1.0),
            ("sepulcrum_lockon", "independent", 1.0),
            ("tenet_diplos_lock_on", "independent", 1.0),
            // …and one that takes NO Condition Overload at all: "Sonicor |
            // Projectile Impact | 150 | 0 | 0% | N/A | Does not apply". The Stug
            // has carried the same row since it was written.
            ("sonicor", "inert", 1.0),
            // …and the CASTANAS FAMILY, whose every attack carries the same
            // 0% row: "Castanas | Normal Attack | AoE | 160 | 0 | 0% | N/A |
            // Does not apply", and the Sancti's two detonations likewise. On a
            // mine whose damage IS the blast that was the whole weapon taking a
            // term the game does not give it. The Talons has NO row, and
            // absence means ordinary.
            ("castanas", "inert", 1.0),
            ("sancti_castanas", "inert", 1.0),
            // THE ONE ENTRY HERE THAT NO CATALOG ROW NAMES, and it is not an
            // exception to the rule above — it is the AoE rule below, reaching
            // a part this engine has no other slot for.
            //
            // The Grimoire's orb pulses are RANGE DIRECT HITS: each lands on
            // everything within six metres, which makes them an area attack
            // wearing a direct hit's other properties (owner, 2026-08-28, M63).
            // "AN AoE PART TAKES NO CO unless its own row says so" is therefore
            // the whole answer, and the wiki's catalog was re-read on the PAGE
            // the same day with no Grimoire row of any kind — absence meaning
            // ORDINARY, and ordinary for an area attack is nothing.
            //
            // It has to be said HERE because the contact pulse is filed as the
            // attack's own `damage:`, the only slot an engine that fires a
            // field off an impact has for it; the five that follow are the
            // `lingering:` field and the final blast is the `radial:`, and both
            // of those take no CO by default. So this line is what makes the
            // three halves of one attack agree.
            ("grimoire_active", "inert", 1.0),
        ];

        let mut unexpected = Vec::new();
        let mut wrong = Vec::new();
        for s in all() {
            let beh = s.co_behavior.as_deref().unwrap_or("additive_with_base_damage");
            let fraction = s.co_base_fraction.unwrap_or(1.0);
            let ordinary = beh == "additive_with_base_damage" && (fraction - 1.0).abs() < 1e-9;
            match NAMED.iter().find(|(id, ..)| *id == s.id) {
                None if !ordinary => unexpected.push(format!("{} = {beh} x{fraction}", s.id)),
                Some((_, b, f)) if beh != *b || (fraction - f).abs() > 1e-9 => {
                    wrong.push(format!("{}: {beh} x{fraction}, catalog says {b} x{f}", s.id));
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
            // THE 2026-08-20 SWEEP. Five more radials the catalog gives their
            // own row and this roster had at `false` — which is not the
            // fraction being off, it is the WHOLE CO term missing from an AoE
            // that is most of the weapon. Each carries its relative column in
            // its own comment and admits the fraction it cannot hold:
            //   Ambassador  75%  | Ferrox      350% | Tenet Ferrox 333%
            //   Opticor    250%  | Opt. Vandal 200% | Trumna       164%
            "ambassador_charged", "ferrox", "tenet_ferrox",
            "opticor", "opticor_vandal", "trumna",
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

        // …and the CLOUDS, which are FIELDS rather than explosions — each with
        // its own catalog row and its own flag. The distinction matters here
        // because it is why this roster has more AoE parts taking CO than it
        // has radials.
        //
        //   Torid           | Toxin AoE Cloud         | 40 |  40 |  100% | Multiplying
        //   Pox             | DoT Cloud               | 20 |  50 |  250% | Adding
        //   Mutalist Cernos | Charged AoE Toxin Cloud |  5 | 205 | 4100% | Adding
        //
        // THE POX'S 250% IS NOT EXPRESSIBLE and the weapon says so: its term
        // reads 50 against a cloud whose own base is 20, and `co_base_fraction`
        // is one number per ENTRY whose THROW is ordinary. It takes the
        // ordinary 100% here, which understates a status-stacking build.
        let field_co: Vec<&str> = all()
            .iter()
            .filter(|s| s.attack.lingering.as_ref().is_some_and(|f| f.takes_condition_overload))
            .map(|s| s.id.as_str())
            .collect();
        // THE MUTALIST CERNOS JOINED THEM ON 2026-08-20, and its 4100% is the
        // most extreme relative column in the catalog: a cloud whose own base
        // is 5 and whose CO term reads 205. Same shape as the Pox's 250% and
        // the same admission — the field takes the term at 100% of its own
        // base, which understates a status-stacking build enormously.
        let mut field_co: Vec<&str> = field_co;
        field_co.sort_unstable();
        assert_eq!(
            field_co, ["mutalist_cernos", "pox", "torid"],
            "a lingering field likewise"
        );
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
    /// was disabled on every weapon but the Arch-Gun (owner). `has_reserve`
    /// is derived from `ammo_max` and is what "truly infinite" means;
    /// `no_resupply` is the Arch-Gun's own problem.
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
    /// a weapon that can receive none.
    ///
    /// IT ASSERTS THE WHOLE CHAIN, not the last method in it (2026-08-27). The
    /// resupply rule moved out of `reserve_is_infinite` and into
    /// `scenario::Capability::CanResupply`, where a scenario can also argue
    /// with it — so a test that called the method with a raw box value would
    /// now be testing half a rule and would go green on an Arch-Gun that had
    /// silently become bottomless. It resolves first, exactly as `parse_fight`
    /// does.
    #[test]
    fn the_infinite_ammo_setting_cannot_resupply_an_arch_gun() {
        use crate::loadout::{resolve, StackPolicy, WeaponBase};
        use crate::scenario::{self, AxisValue};
        let ammo = scenario::axis("infinite_ammo").unwrap();
        let run = |id: &str, ticked: bool| {
            let panel =
                resolve(&WeaponBase::from_data(id, true, &[]), &[], StackPolicy::Emergent);
            let v = scenario::resolve(ammo, id, None)
                .value(AxisValue::Flag(ticked))
                .as_flag()
                .unwrap();
            panel.reserve_is_infinite(v)
        };

        // Sentinel: infinite either way, nothing to decide.
        assert!(run("verglas_prime", true));
        assert!(run("verglas_prime", false));

        // Primary: the setting decides, which is the point.
        assert!(run("torid", true));
        assert!(!run("torid", false));

        // Ground Arch-Gun: finite either way — 400 rounds is the engagement.
        assert!(!run("larkspur_prime", true));
        assert!(!run("larkspur_prime", false));

        // …UNLESS THE FIGHT ITSELF SAYS OTHERWISE, which is the one thing a
        // scenario is allowed to argue with. Same weapon, same ticked box, a
        // class rule that says Arch-Guns are resupplied in here.
        let panel = resolve(
            &WeaponBase::from_data("larkspur_prime", true, &[]),
            &[],
            StackPolicy::Emergent,
        );
        let ruled = scenario::resolve(ammo, "larkspur_prime", Some(AxisValue::Flag(true)))
            .value(AxisValue::Flag(true))
            .as_flag()
            .unwrap();
        assert!(panel.reserve_is_infinite(ruled));
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
        assert!((f.duration_seconds - 10.0).abs() < 1e-9);
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
        let g = i.gauge_form.as_ref().expect("torid_incarnon has a gauge");
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
            // AN ADAPTER FORM IS EXEMPT, and only that. It carries its own
            // kind — the form vocabulary answers "which form of this weapon",
            // and `incarnon` already answers it — so a form that happens to
            // draw is not thereby the charged form. The Dread's Incarnon form
            // draws for 0.6 s and is not what the arsenal means by "charged
            // Dread". Everything else still has to agree: a `charge` trigger
            // filed as `base` is a data error. NOT the gauge: the Mausolon's
            // alt-fire has one and IS the charged form, because a charge is
            // exactly how it is fired.
            assert_eq!(
                charge_trigger && !kind.is_adapter_form(),
                kind == FormKind::Charged,
                "{}: a charge trigger IS the charged form, and nothing else is",
                s.id
            );
            // AN IMPLICATION, NOT AN EQUIVALENCE. An adapter form is always
            // bought with a gauge, so one without the economy is a half-written
            // entry — but the converse stopped holding when the Mausolon landed
            // a gauge on a `charged` form (owner, 2026-08-15), and asserting it
            // both ways is what made a real weapon look like a data error.
            assert!(
                !kind.is_adapter_form() || s.gauge_form.is_some(),
                "{}: an Incarnon form is entered by filling a gauge, so it has to declare one",
                s.id
            );
            // The entry reached BY a transform is never the default form.
            // THE GAUGE AGAIN, not the adapter: a form you transform INTO is
            // exactly a form you must pay a meter to reach, and naming where
            // it comes from is how the cycle finds its other end.
            assert_eq!(
                s.transforms_from.is_some(),
                s.has_gauge(),
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

    /// AN EXPLOSION'S FORCED PROC IS ITS OWN, and the Scourge pair is why the
    /// field stopped being the attack's alone.
    ///
    /// The two are different questions and the roster holds both answers. The
    /// Astilla's DIRECT hit forces Impact and its radial does not; the Scourge
    /// pair's page says "Guaranteed Impact proc" of the SPEAR EXPLOSION and
    /// nothing of the throw. One shared list can only be right for one of them,
    /// and putting the Scourge's on the attack would have forced a proc on a
    /// hit the game does not force one on.
    ///
    /// Asserted in BOTH directions on the same weapon, because either alone
    /// passes on a flag that is simply always set.
    #[test]
    fn an_explosions_forced_proc_is_its_own_and_not_the_direct_hits() {
        use crate::damage::DamageType;
        let mut buf = [DamageType::Impact; DamageType::ALL.len()];

        for id in ["scourge_thrown", "scourge_prime_thrown"] {
            let base = WeaponBase::from_data(id, false, &[]);
            assert!(
                base.forced_procs.is_empty(),
                "{id}: the THROW forces nothing — the page says it of the explosion"
            );
            let r = base.radial.as_ref().expect("the spear explodes");
            let n = r.forced_procs.fill(&mut buf);
            assert_eq!(&buf[..n], &[DamageType::Impact], "{id}: the EXPLOSION forces Impact");

            // …and it survives the mod layer, which is where the attack's own
            // list was silently dropped before anything filled it.
            let panel = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::AssumedMax);
            let rr = panel.radial.as_ref().expect("resolved");
            let n = rr.forced_procs.fill(&mut buf);
            assert_eq!(&buf[..n], &[DamageType::Impact], "{id}: still Impact after resolution");
        }

        // THE OTHER DIRECTION, on the primary fire of the same weapons: an
        // explosion that forces nothing must still force nothing.
        for id in ["scourge", "scourge_prime"] {
            let base = WeaponBase::from_data(id, false, &[]);
            let r = base.radial.as_ref().expect("the plasma shot explodes");
            assert!(
                r.forced_procs.is_empty(),
                "{id}: no guaranteed proc is claimed of the primary fire's explosion"
            );
        }
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
        assert!((b.co_base_fraction() - 0.5).abs() < 1e-9);
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
        assert!((tapped.co_base_fraction() - 1.0).abs() < 1e-9);
        assert!((charged.co_base_fraction() - 0.5).abs() < 1e-9);
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
    fn an_evolutions_flat_damage_stays_out_of_the_co_term_by_default() {
        use crate::loadout::WeaponBase;
        // THE TORID IS WHERE THIS TEST TURNED AROUND. It asserted 1.0 on both
        // tier-2 perks, on the reading that the weapon's catalog rows say
        // "100 | 100%" — until the owner measured the Incarnon form and every
        // reading off BOTH perks solved to a CO base of ~51, the unevolved
        // value, off panels of 102 and 82 (MEASUREMENTS M50). The rows say
        // 100% of an UNEVOLVED 100, which is true by construction.
        let bare = WeaponBase::from_data("torid", false, &[]);
        let evolved = WeaponBase::from_data("torid", false, &["torid_final_fusillade"]);
        assert!(
            (evolved.base_vector.total() - (bare.base_vector.total() + 51.0)).abs() < 1e-9,
            "the evolution still scales the base"
        );
        // …ON THE FORM THAT WAS MEASURED, which is the Incarnon one. The base
        // form is `Multiplying` and stays at 1.0 until somebody measures a
        // Multiplying entry — see `EvolutionDef::excludes_co_base`.
        assert!(
            (evolved.co_base_fraction() - 1.0).abs() < 1e-9,
            "the Multiplying base form is unmeasured, got {}",
            evolved.co_base_fraction()
        );
        for (perk, panel) in [("torid_final_fusillade", 102.0), ("torid_plentiful_mayhem", 82.0)] {
            let inc = WeaponBase::from_data("torid_incarnon", false, &[perk]);
            assert!((inc.base_vector.total() - panel).abs() < 1e-9);
            // ONE CO BASE, TWO PANELS — the shape the measurement turned on.
            assert!(
                (inc.co_base_fraction() * panel - 51.0).abs() < 1e-9,
                "{perk}: solves to a CO base of {}",
                inc.co_base_fraction() * panel
            );
        }

        // Dual Toxocyst + Carnage Reign (Perk 1, +60 on a 75 base) = the
        // catalog's "100% or 56%" row: a +100% CO adds 75, never 135.
        for form in ["dual_toxocyst", "dual_toxocyst_incarnon"] {
            let frame_seconds = WeaponBase::from_data(form, false, &["dual_toxocyst_carnage_reign"]);
            assert!(
                (frame_seconds.co_base_fraction() - 75.0 / 135.0).abs() < 1e-9,
                "{form}: expected 75/135 = 0.5556, got {}",
                frame_seconds.co_base_fraction()
            );
        }
        // …AND SO DOES PERK 2, WHICH THE CATALOG DOES NOT LIST. Fevered Frenzy
        // also raises base damage (+50), and this assertion read 1.0 until it
        // was measured: at the 125 panel, Galvanized Shot at 3 stacks against 2
        // status types gives 305, and a CO term on the full 125 would give 425
        // (MEASUREMENTS M49, owner 2026-08-16).
        //
        // WHAT IT COST TO LEARN, and why the flag is still per PERK. The
        // catalog's ABSENCE-MEANS-ORDINARY rule produced the old number, and
        // the rule is not repealed — it holds for every other row and the
        // negative controls below still assert it. What is now known is that
        // this weapon's exclusion is the WEAPON's rather than one perk's, so
        // both tier-2 options carry the flag. The Despair is why the
        // granularity stays per perk regardless: one of its two is excluded
        // and the other measurably is not.
        let perk2 =
            WeaponBase::from_data("dual_toxocyst", false, &["dual_toxocyst_fevered_frenzy"]);
        assert!(
            (perk2.base_vector.total() - 125.0).abs() < 1e-9,
            "the +50 still reaches the base, got {}",
            perk2.base_vector.total()
        );
        assert!(
            (perk2.co_base_fraction() - 75.0 / 125.0).abs() < 1e-9,
            "Perk 2 was measured to exclude its own +50 too; expected 75/125, got {}",
            perk2.co_base_fraction()
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
        assert!(b.gauge_form.is_none());

        let i = base_panel("dual_toxocyst_incarnon", false);
        assert!((i.base_crit_damage - 3.0).abs() < 1e-9);
        assert!((i.magazine_size - 270.0).abs() < 1e-9);
        assert!((i.base_reload - 3.35).abs() < 1e-9);
        let inc = i.gauge_form.expect("incarnon block");
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
        let f = b.gauge_form.expect("incarnon economy");
        assert_eq!(f.charges_to_fill, 12.0);
        assert_eq!(f.max_charges, 216.0);
        assert_eq!(f.transmute_in, 2.0);
        // Reverts are a uniform 1 s across every weapon until measured.
        assert_eq!(f.transmute_out, 1.0);
        assert_eq!(f.charge_rate, 0.0);

        let eff = WeaponBase::from_data("laetum_incarnon", true, &["laetum_incarnon_efficiency"]);
        let g = eff.gauge_form.expect("incarnon economy");
        assert_eq!(g.charge_rate, 0.5);
        // 12 / 1.5 = 8 hits (wiki).
        assert_eq!((g.charges_to_fill / (1.0 + g.charge_rate)).ceil() as u32, 8);

        // Dual Toxocyst keeps its own numbers.
        let frame_seconds = WeaponBase::from_data("dual_toxocyst_incarnon", true, &[]);
        let d = frame_seconds.gauge_form.expect("incarnon economy");
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
        assert!((b.co_base_fraction() - 1.0).abs() < 1e-9);

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

    /// THE BRATON'S RADIAL CO BASE, the same shape the Burston's row has and
    /// the same arithmetic: 70 + 4 = 74, and 70/74 is the 95% the catalog's
    /// third column prints. The explosion TAKES the tier-2 evolution's flat
    /// damage and does not take it into the base its CO term multiplies.
    ///
    /// Both tier-2 options land it, at their own values — the row's note reads
    /// "Listed values for Braton Prime with inactive Daring Reverie", i.e. that
    /// perk's unconditional +4 is in the 74 and its conditional half is not.
    /// A BURST TRIGGER DECLARES ITS BURST. Without the block the sim reads
    /// `fire_rate` as rounds per second when it counts PULLS, and the weapon is
    /// understated by its whole burst count — the Sybaris by 2x, the Sicarus by
    /// 3x, the Vasto's Incarnon form by 6x.
    ///
    /// It also decides a TRIGGER: `BuffTrigger::FullBurst` asks "every count-th
    /// round", and with no block the count is 1, so every round completes a
    /// burst and Reaver's Rapture stacks at the wrong rate.
    ///
    /// Twelve entries shipped without one until 2026-08-12, which is why this
    /// is a test rather than a habit.
    #[test]
    fn every_burst_weapon_declares_how_many_rounds_a_pull_fires() {
        let missing: Vec<&str> = all()
            .iter()
            .filter(|s| s.attack.trigger == "burst" && s.attack.burst.is_none())
            .map(|s| s.id.as_str())
            .collect();
        assert!(
            missing.is_empty(),
            "burst trigger with no `burst:` block — `fire_rate` counts PULLS, so these are              understated by their burst count: {missing:?}"
        );
        // …and a block never claims a burst of one, which would be a weapon
        // that is not a burst weapon wearing the trigger.
        for s in all() {
            if let Some(b) = s.attack.burst {
                assert!(b.count >= 2, "{}: a burst of {} is not a burst", s.id, b.count);
                // A ZERO DELAY IS A REAL VALUE, and the Morgha is why this is
                // `>= 0.0` instead of `> 0.0`: its page gives Burst Count 2 and
                // Burst Delay 0.0 s, which means both rounds leave together.
                // It is still a BURST and not multishot — a burst of two spends
                // two rounds, and on an Arch-Gun's finite ground reserve that
                // is the difference between 160 shots and 320.
                assert!(b.delay_seconds >= 0.0, "{}: a negative burst delay", s.id);
            }
        }
    }

    /// THE AKARIUS PAIR: two damage instances a rocket, two rockets a pull, and
    /// the CO row that names one of them.
    ///
    /// The burst is the half a reader gets wrong: the listed fire rate counts
    /// PULLS, so reading 3.667 as rounds per second halves the weapon. The
    /// module carries `BurstCount = 2` and the wiki's Notes name it in words —
    /// "the guaranteed 2-round burst".
    #[test]
    fn the_akarius_fires_two_rockets_a_pull_and_each_one_explodes() {
        for (id, blast, radius, cc) in [
            ("akarius", 419.0, 7.2, 0.06),
            ("akarius_prime", 509.0, 7.8, 0.18),
        ] {
            let b = WeaponBase::from_data(id, true, &[]);
            assert_eq!(b.base_vector.total(), 68.0, "{id}: Rocket Impact is 68 Impact");
            let burst = b.burst.expect("{id} declares its burst");
            assert_eq!(burst.count, 2, "{id}: two rockets a pull");

            let r = b.radial.as_ref().unwrap_or_else(|| panic!("{id} declares Rocket Detonation"));
            assert!((r.base_vector.total() - blast).abs() < 1e-9, "{id} blast");
            assert!((r.radius_m - radius).abs() < 1e-9, "{id} radius");
            assert!((r.base_crit_chance - cc).abs() < 1e-9, "{id}: the explosion crits like the impact");
            // "Rocket Detonation | 0% | Does not apply" on the Prime's row, and
            // no row at all on the base — ordinary either way for an AoE.
            assert!(!r.takes_condition_overload, "{id}: the explosion takes no CO");
        }

        // The CO row names the PRIME's Rocket Impact and nothing else.
        assert_eq!(
            spec("akarius_prime").unwrap().co_behavior.as_deref(),
            Some("independent")
        );
        // The base has no row. Written out rather than left blank, so the
        // assertion is on the VALUE — absence and the ordinary value are the
        // same statement and a file may make either.
        assert_eq!(
            spec("akarius").unwrap().co_behavior.as_deref().unwrap_or("additive_with_base_damage"),
            "additive_with_base_damage",
            "no row, so ordinary"
        );
    }

    #[test]
    fn the_bratons_radial_co_base_is_the_catalogs_ninety_five_percent() {
        let b = WeaponBase::from_data("braton_prime_incarnon", true, &["braton_prime_daring_reverie"]);
        let r = b.radial.as_ref().expect("the radial survives an evolution");
        assert!((r.base_vector.total() - 74.0).abs() < 1e-9, "{}", r.base_vector.total());
        assert!(
            (r.co_base_fraction() - 70.0 / 74.0).abs() < 1e-9,
            "the explosion's CO base stays 70/74 = {:.1}%, got {}",
            70.0 / 74.0 * 100.0,
            r.co_base_fraction()
        );
    }

    /// THE ZYLOK'S ROW MIXES ITS TWO VARIANTS, so its printed 90% is the one
    /// figure in the catalog this engine does not reproduce — and should not.
    ///
    /// The row reads `776 || 700 || 90%` with the note "Listed Values for Zylok
    /// Prime". 700 IS the Prime's radial; the +76 that makes 776 is the base
    /// Zylok's Precision's Payoff, which the evolution table prints per variant
    /// as X = 76 (Zylok) and X = 30 (Zylok Prime). 700/776 is therefore one
    /// weapon's explosion under the other's perk.
    ///
    /// Each variant is self-consistent here, and the per-variant evolution
    /// table is the more specific source.
    #[test]
    fn the_zyloks_two_variants_each_carry_their_own_radial_co_base() {
        let prime = WeaponBase::from_data("zylok_prime_incarnon", true, &["zylok_prime_precisions_payoff"]);
        let pr = prime.radial.as_ref().expect("radial");
        assert!((pr.base_vector.total() - 730.0).abs() < 1e-9, "700 + 30");
        assert!((pr.co_base_fraction() - 700.0 / 730.0).abs() < 1e-9);

        let plain = WeaponBase::from_data("zylok_incarnon", true, &["zylok_precisions_payoff"]);
        let cr = plain.radial.as_ref().expect("radial");
        assert!((cr.base_vector.total() - 676.0).abs() < 1e-9, "600 + 76");
        assert!((cr.co_base_fraction() - 600.0 / 676.0).abs() < 1e-9);
    }

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
                (r.co_base_fraction() - 13.0 / 55.0).abs() < 1e-9,
                "{evo}: the explosion's CO base stays 13/55, got {}",
                r.co_base_fraction()
            );
            // …AND SO DOES THE DIRECT HIT — MEASURED (owner, 2026-08-16),
            // which reversed this assertion. It read `co_base_fraction == 1.0`
            // on the reasoning that the catalog's row names the RADIAL and the
            // direct hit is therefore not discrepant. It is: at one and two
            // Galvanized Aptitude stacks against one status type the game gives
            // 181 and 196 where the full-base reading gives 231 and 261, and
            // both solve to 13/55. The exclusion belongs to the PERK, so it
            // reaches wherever the +42 landed. MEASUREMENTS M48.
            assert!((b.base_vector.total() - 55.0).abs() < 1e-9);
            assert!(
                (b.co_base_fraction() - 13.0 / 55.0).abs() < 1e-9,
                "{evo}: the direct hit's CO base is 13/55 too, got {}",
                b.co_base_fraction()
            );
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
        let multishot = sim(&[split]);
        assert!(
            multishot.direct > bare.direct * 1.5,
            "+90% multishot must grow the direct hit: {} -> {}",
            bare.direct,
            multishot.direct
        );
        assert!(
            (multishot.radial / bare.radial - 1.0).abs() < 0.1,
            "the explosion must not follow it: {} -> {}",
            bare.radial,
            multishot.radial
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
            let multishot = play_modes(&w.id);
            let base = multishot.iter().find(|m| m.mode == PlayMode::Base);
            let base = base.unwrap_or_else(|| panic!("{}: no base mode", w.id));
            assert!(base.sustainable, "{}: its own arsenal form is not rankable", w.id);
            assert_eq!(
                multishot.iter().filter(|m| m.mode == PlayMode::Base).count(),
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
            let multishot = play_modes(&w.id);
            let alt = forms.iter().find(|f| !f.is_default);
            let Some(alt) = alt else {
                assert_eq!(multishot.len(), 1, "{}: one form, so one mode", w.id);
                continue;
            };
            let _ = alt;
            // EVERY alternate form gets its own mode, and a weapon may have
            // more than one: a bow with an adapter has a tapped shot and an
            // Incarnon form.
            let alts: Vec<_> = forms.iter().filter(|f| !f.is_default).collect();
            let gauged = |f: &FormRef| spec(f.weapon_id).is_some_and(|s| s.gauge_form.is_some());
            let any_gauged = alts.iter().any(|f| gauged(f));
            let has = |m: PlayMode| multishot.iter().any(|x| x.mode == m);
            let rankable = |m: PlayMode| multishot.iter().any(|x| x.mode == m && x.sustainable);

            assert_eq!(
                multishot.len(), 1 + alts.len() + usize::from(any_gauged),
                "{}: {} forms should give base + one mode each + a cycle: {:?}",
                w.id, forms.len(), multishot.iter().map(|m| m.id).collect::<Vec<_>>()
            );
            assert_eq!(has(PlayMode::Cycle), any_gauged, "{}: cycle iff gauge", w.id);
            assert_eq!(has(PlayMode::Transformed), any_gauged, "{}: gauge-fed mode iff gauge", w.id);
            assert_eq!(
                has(PlayMode::Alternate), alts.iter().any(|f| !gauged(f)),
                "{}: a free second form is an alternate", w.id
            );
            // …and the ids are DISTINCT, which is the whole reason the gauged
            // one is its own mode: a build names a mode by id.
            let mut ids: Vec<&str> = multishot.iter().map(|m| m.id).collect();
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
        assert_eq!(f("torid", PlayMode::Cycle), Some("gauge_cycle"));
        // TRANSFORMED, not Alternate: being in the Incarnon form for a
        // whole engagement is a thing the builder can show and not a
        // thing a ruler ranks.
        assert_eq!(f("torid", PlayMode::Transformed), Some("incarnon"));
        assert_eq!(f("torid", PlayMode::Alternate), None, "the Torid has no free second form");
        // A bow with an adapter has BOTH, which is why they are two modes.
        assert_eq!(f("paris", PlayMode::Base), Some("charged"));
        assert_eq!(f("paris", PlayMode::Alternate), Some("base"), "the tapped shot");
        assert_eq!(f("paris", PlayMode::Transformed), Some("incarnon"));
        assert_eq!(f("paris", PlayMode::Cycle), Some("gauge_cycle"));
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

    /// A GAUGE WITHOUT AN ADAPTER — the Mausolon, and the case that used to be
    /// unrepresentable.
    ///
    /// Its alt-fire is bought with five kills ("Getting 5 kills with the
    /// Mausolon's primary fire will unlock an Alternate Fire", wiki), so it is
    /// the same POLICY as a Zariman weapon's and none of the same hardware:
    /// no Genesis, no tier-1 unlock, and a `charged` form rather than an
    /// `incarnon` one. Every one of those three used to be how the roster
    /// recognised a gauge (owner, 2026-08-15).
    ///
    /// Three claims, and each fails on a different half of that:
    ///   * the cycle EXISTS, and is sustainable — kills keep coming;
    ///   * the alt-fire alone is NOT, because five kills buy one laser;
    ///   * the form needs no adapter, so it is not hidden behind an evolution.
    #[test]
    fn the_mausolon_earns_its_alt_fire_without_an_adapter() {
        let all: Vec<&'static str> = play_modes("mausolon").iter().map(|m| m.id).collect();
        assert_eq!(all, vec!["base", "cycle", "transformed"]);
        let on: Vec<&'static str> = play_modes("mausolon")
            .into_iter()
            .filter(|m| m.sustainable)
            .map(|m| m.mode.id())
            .collect();
        assert_eq!(on, vec!["base", "cycle"]);

        let c = play_modes("mausolon")
            .into_iter()
            .find(|m| m.mode == PlayMode::Cycle)
            .expect("five kills is a gauge");
        assert_eq!(c.weapon_id, "mausolon");
        assert_eq!(c.other_id, Some("mausolon_charged"));

        let alt = spec("mausolon_charged").expect("the alt-fire is a form entry");
        assert!(alt.has_gauge());
        // THE GAUGE IS DECLARED AND THE ADAPTER IS NOT INFERRED FROM IT. This
        // is the pair that was one method until this weapon arrived.
        assert!(!alt.form_kind().is_adapter_form());
        assert_eq!(alt.form_kind(), FormKind::Charged);
        assert!(has_gauge_switched_form("mausolon"));

        let g = alt.gauge_form.as_ref().expect("checked above");
        assert_eq!(g.gauge.charge_on, "kills");
        assert!((g.gauge.charges_to_fill - 5.0).abs() < 1e-9);
        // ONE LASER PER FILL, and no transition either way — the charge IS the
        // shot, so a house-standard 1 s transmute would invent downtime.
        assert!((g.gauge.max_rounds - 1.0).abs() < 1e-9);
        assert!((g.transmute_in_seconds).abs() < 1e-9);
        assert!((g.transmute_out_seconds).abs() < 1e-9);
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
            // …AND IT NO LONGER ADMITS ANYTHING (2026-08-15). This assertion
            // used to read the other way round — every falloff weapon had to
            // SAY it was not modelled — and the direction flipped with the
            // arena's 2D layer. An admission that outlives the gap it names is
            // worse than none: it tells a player to distrust a number that is
            // now right, and the page is where they would read it.
            //
            // The RADIAL's own falloff is a different gap and still open, so a
            // weapon may still carry `radial_falloff` — this only forbids the
            // direct-hit line.
            assert!(
                !w.unmodeled_parts
                    .iter()
                    .any(|u| u.reason.as_deref() == Some("damage_falloff")),
                "{} still admits a direct-hit falloff the engine now models",
                w.id
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
            // THE SPEARGUNS, whose two rows are the same weapon read twice —
            // 1.7 m on the primary fire and 7.0 m on the throw, ordinary in
            // every column. The Prime shares both rows: the catalog's Weapon
            // cell is literally "Scourge (Scourge Prime)".
            ("scourge", 1.36),
            ("scourge_prime", 1.36),
            ("scourge_thrown", 5.60),
            ("scourge_prime_thrown", 5.60),
            // THE ARCH-GUNS. Four rows reach the roster and two of them are a
            // tested ZERO — "Doesn't Work" is a stronger statement than an
            // absent row, so it is carried rather than left to be inferred.
            //   Mausolon | Main-fire Radial | 100% | Adds | Stolen   1.8 m
            //   Mausolon | Alt-fire Radial  | 100% | Adds | Snapshot 8.0 m
            //   Cortege  | Primary Fire+AoE |   0% | Doesn't Work
            //   Cortege  | Alt-Fire + AoE   |   0% | Doesn't Work
            //   Kuva Ayanga | Primary+AoE   |   0% | Doesn't Work
            ("mausolon", 1.44),
            ("mausolon_charged", 6.40),
            ("cortege", 0.0),
            ("cortege_alt", 0.0),
            ("kuva_ayanga", 0.0),
            ("arbucep", 0.0),
            // THE TENET AND CODA BATCH (2026-08-20). Six rows, and the last is
            // the third tested ZERO in the catalog.
            //   Tenet Envoy   | Primary Fire + AoE | 100% | Multiplies | 8.0 m | +640%
            //   Tenet Tetra   | Alt-Fire + AoE     | 100% | Multiplies | 8.0 m | +640%
            //   Tenet Ferrox  | Primary Fire + AoE | 100% | ADDS       | 4.0 m | +320%
            //   Tenet Quanta  | Alt-Fire + AoE     | 100% / 8% | Multiplies | 0.5 m | +40%
            //   Coda Bubonico | Alt-Fire + AoE     | 100% | Multiplies | 7.0 m | +560%
            //   Tenet Ferrox  | Throw + AoE        |   0% | Doesn't Work
            // The Quanta's two effectiveness figures are its cube's TWO
            // explosions — 100% on the 0.5 m contact blast this roster fires,
            // 8% on the 6 m one a player shoots loose, which is unmodelled. The
            // base-radius column agrees with the first, so the single figure is
            // right for what the entry carries.
            ("tenet_envoy", 6.40),
            ("tenet_tetra_grenade", 6.40),
            ("tenet_ferrox", 3.20),
            ("tenet_quanta_cube", 0.40),
            ("coda_bubonico_burst", 5.60),
            ("tenet_ferrox_thrown", 0.0),
            // THE NINETEEN BASE WEAPONS (2026-08-20). The Ferrox row is ONE row
            // covering both variants — its base-radius cell reads
            // "3.6 m (4.0 m)", the parenthetical being the Tenet's — which is
            // the opposite of the CO table's rule and safe only because the
            // cell says so.
            //   Bubonico       | Alt-Fire + AoE     | 100% | Multiplies | 7.0 m | +560%
            //   Ferrox         | Primary Fire + AoE | 100% | ADDS       | 3.6 m | +288%
            //   Ferrox         | Throw + AoE        |   0% | Doesn't Work
            //   Quanta         | Alt-Fire + AoE     | 100% / 8% | Multiplies | 0.5 m | +40%
            //   Quanta Vandal  | Alt-Fire + AoE     | 100% / 8% | Multiplies | 0.5 m | +40%
            //   Glaxion Vandal | Primary Fire + AoE |   0% | Doesn't Work | 2.0 m
            // The Glaxion Vandal's is the fourth tested ZERO in the catalog and
            // the general exclusion applied to a real radius: a beam attack
            // with an AoE component.
            ("bubonico_burst", 5.60),
            ("ferrox", 2.88),
            ("ferrox_thrown", 0.0),
            ("quanta_cube", 0.40),
            ("quanta_vandal_cube", 0.40),
            ("glaxion_vandal", 0.0),

            // THE 2026-08-20 SWEEP. The published table named FIFTY-NINE more
            // roster attacks than the roster had transcribed — most of them from
            // this month's intake, and a dozen that had been here far longer. An
            // attack with no `compression:` pays the arcane NOTHING (
            // `loadout::resolve` reads `Some(c)` or nothing at all), so every one
            // of them was silently worth zero to a build carrying it.
            //
            // Each figure below is OUR radius x 0.8, which is what the arcane
            // takes. Where that disagrees with the table's own Max Damage Bonus
            // column the line says so, and there are exactly three:
            //
            //   lenz / prisma_lenz — 7.2 m x 0.8 is 5.76 and the table rounds its
            //     own arithmetic to +575%.
            //   secura_penta — the table gives the three Pentas ONE row at 4.0 m,
            //     and this weapon's own module row is 6.0 m. The weapon wins.
            //   battacor_charged — the table's radius column says 3.4 m and its
            //     bonus column says +208%, which is 2.6 m. The table disagrees
            //     with ITSELF there; ours follows its radius column.
            ("acceltra", 3.20),
            ("acceltra_prime", 4.00),
            ("aeolak_alt", 5.60),
            ("afentis", 2.40),
            ("afentis_prime", 4.40),
            ("alternox_alt", 4.80),
            ("alternox_prime_alt", 4.80),
            ("ambassador_charged", 4.80),
            ("astilla", 1.92),
            ("astilla_prime", 1.92),
            ("basmu", 1.36),
            ("battacor_charged", 2.72),   // table prints +208%
            ("carmine_penta", 3.20),
            ("cedo_alt", 4.80),
            ("cedo_prime_alt", 4.80),
            ("coda_sporothrix", 1.60),
            ("corinth_airburst", 7.52),
            ("corinth_prime_airburst", 7.84),
            ("enkaus_alt", 0.0),
            ("evensong", 3.20),
            ("grattler", 0.0),
            ("ignis", 0.0),
            ("ignis_wraith", 0.0),
            ("javlok", 1.92),
            ("javlok_throw", 4.80),
            ("komorex", 0.0),
            ("kuva_bramma", 6.64),
            ("kuva_chakkhurr", 2.32),
            ("kuva_grattler", 0.0),
            ("kuva_ogris", 6.32),
            ("kuva_tonkor", 5.60),
            ("kuva_zarr", 5.60),
            ("larkspur_charged", 0.0),
            ("larkspur_prime_charged", 0.0),
            ("lenz", 5.76),   // table prints +575%
            ("morgha", 0.0),
            ("morgha_alt", 0.0),
            ("mutalist_cernos", 0.0),
            ("mutalist_quanta_orb", 3.52),
            ("ogris", 5.68),
            ("opticor", 4.80),
            ("opticor_quick", 4.80),
            ("opticor_vandal", 3.68),
            ("opticor_vandal_quick", 3.68),
            ("panthera_prime", 1.28),
            ("penta", 3.20),
            ("prisma_lenz", 5.76),   // table prints +575%
            ("proboscis_cernos", 5.60),
            ("secura_penta", 4.80),   // table prints +320%
            ("simulor", 4.00),
            ("sporothrix", 1.36),
            ("stahlta_charged", 0.0),
            ("synoid_simulor", 4.00),
            ("tonkor", 5.60),
            ("trumna", 1.28),
            ("trumna_prime", 1.28),
            ("vadarya_prime", 0.0),
            ("zarr", 3.92),
            ("zhuge_prime", 2.08),
        ];
        // At rank 5 a metre is worth +100%, so the bonus IS the metres lost.
        let fx = crate::arcanes_data::for_slot("primary", "primary_compression")
            .expect("the arcane is in the primary pool")
            .fx(5, crate::loadout::StackPolicy::Emergent, &[], crate::tenno_data::default_tenno());
        assert_eq!(fx.compression_damage_per_m, 1.0, "+100% per metre at max rank");
        for (id, expected) in table {
            let base = crate::loadout::WeaponBase::from_data(id, true, &[]);
            let p = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent);
            let bonus = p.compression.map_or(0.0, |c| c.radius_lost_m) * fx.compression_damage_per_m;
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

    /// A THROW PAYS FOR ITS OWN RELOAD, so the wind-up is not the cycle.
    ///
    /// The spearguns' alt-fire is wind-up → release → reload, every throw
    /// (owner, 2026-08-14) — the reload is unconditional rather than a magazine
    /// running dry. That is a `magazine: 1` weapon, the same shape as a bow's
    /// nock, and it is worth pinning because the entry carried the PRIMARY
    /// FIRE's 40 rounds for two days: the sim then threw 40 times between
    /// reloads and the mode read 59% faster than it is.
    ///
    /// The sharp half is the second assertion. With one magazine per throw the
    /// reload is a FLOOR the wind-up cannot cross, so a fire-rate bonus buys
    /// only the wind-up's share of the cycle — under a 40-round magazine it
    /// bought the whole thing, which is what made a fire-rate build the
    /// obvious one on a weapon where it is not.
    #[test]
    fn a_thrown_speargun_paces_on_wind_up_plus_reload() {
        use crate::dummy::{monte_carlo, DummyParams};
        const DURATION: f64 = 180.0;
        // Both entries: the Prime is not a different mechanic.
        for id in ["scourge_thrown", "scourge_prime_thrown"] {
            assert_eq!(
                spec(id).unwrap().magazine,
                Some(1.0),
                "{id}: one throw is one magazine — the 40 rounds are the primary fire's"
            );
            let run = |mods: &[&crate::loadout::ModDef]| {
                let b = crate::loadout::WeaponBase::from_data(id, true, &[]);
                let p = crate::loadout::resolve(&b, mods, crate::loadout::StackPolicy::Emergent);
                let params = DummyParams::from_panel(
                    &p,
                    &crate::arena::Arena::training(DURATION),
                    &crate::arcanes_data::ArcaneFx::none(),
                );
                // The cycle the data describes, against the one the sim ran:
                // the first throw costs no wind-up, so the count is one more
                // than the cycles that fit.
                let cycle = 1.0 / params.fire_rate + params.reload_seconds;
                let shots = monte_carlo(&params, 8, 5).mean_shots;
                let want = (DURATION / cycle).floor() + 1.0;
                assert!(
                    (shots - want).abs() < 1e-9,
                    "{id}: {shots} throws in {DURATION}s, but a {cycle:.3}s cycle fits {want}"
                );
                (params.fire_rate, shots)
            };
            // Bare: 1.0 s of wind-up + 0.6 s of reload.
            let (rate, shots) = run(&[]);
            assert!((rate - 1.0).abs() < 1e-9, "{id}: the wind-up is 1 / {rate}");

            // …AND A FIRE-RATE MOD CANNOT BUY THE RELOAD. Stated as the
            // inequality rather than as a figure, so it holds whatever the
            // card is worth: throughput rises, and by strictly less than the
            // fire rate did.
            let vile = crate::mods_data::class_pool("rifle")
                .into_iter()
                .find(|m| m.id == "vile_acceleration")
                .expect("vile acceleration is in the rifle pool");
            let (fast_rate, fast_shots) = run(&[&vile]);
            assert!(fast_rate > rate, "{id}: the mod must raise the rate");
            assert!(
                fast_shots > shots && fast_shots / shots < fast_rate / rate - 1e-9,
                "{id}: x{:.3} fire rate bought x{:.3} throws — the reload is a floor",
                fast_rate / rate,
                fast_shots / shots
            );
        }
    }

    /// THE THROW PLANTS A FIELD, and the field outlives the throw after it.
    ///
    /// The Bullet Attractor is the Void effect (owner, 2026-08-14), so it is
    /// worth one line in the Condition Overload counter and nothing else here.
    /// What makes it worth a test is the ARITHMETIC of the two clocks: 4.7 s
    /// on the target against a 1.6 s throw cycle, so from the second throw on
    /// it is simply up — and a new throw destroying the OLD FIELD does not
    /// take back what that field already applied.
    ///
    /// Measured through the CO count rather than through the debuff, because
    /// the count is the only thing the field is worth: a build with a CO mod
    /// must be worth more on the throw than the same build is without the
    /// field, and the gap must be exactly one status type's share.
    #[test]
    fn a_thrown_speargun_plants_a_bullet_attractor_that_counts() {
        use crate::dummy::{monte_carlo, DummyParams};
        for id in ["scourge_thrown", "scourge_prime_thrown"] {
            let b = crate::loadout::WeaponBase::from_data(id, true, &[]);
            assert_eq!(b.attractor_seconds, Some(4.7), "{id}: the wiki's 4.7 s");
            let p = crate::loadout::resolve(&b, &[], crate::loadout::StackPolicy::Emergent);
            let mut params = DummyParams::from_panel(
                &p,
                &crate::arena::Arena::training(60.0),
                &crate::arcanes_data::ArcaneFx::none(),
            );
            assert_eq!(params.attractor_seconds, Some(4.7), "{id}: through the panel");
            // The whole claim, stated as damage: a Condition Overload build
            // that counts the field beats the same build that cannot see it.
            params.co_per_type = 0.8;
            params.co_behavior = crate::loadout::CoBehavior::Independent;
            let with = monte_carlo(&params, 24, 7).mean_effective_damage;
            let without = DummyParams { attractor_seconds: None, ..params.clone() };
            let without = monte_carlo(&without, 24, 7).mean_effective_damage;
            assert!(
                with > without * 1.02,
                "{id}: the field must reach the CO count — {without:.0} -> {with:.0}"
            );
        }
        // NEGATIVE CONTROL: nobody else plants one. The debuff has exactly two
        // sources — this attack and Xata's Whisper's Void instance — and a
        // field granted to a weapon that has none would be invisible in every
        // other test here.
        let planters: Vec<&str> = all()
            .iter()
            .filter(|w| w.attack.attractor_seconds.is_some())
            .map(|w| w.id.as_str())
            .collect();
        assert_eq!(planters, vec!["scourge_prime_thrown", "scourge_thrown"]);
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
        assert!(aimed.compression.is_some_and(|c| c.radius_lost_m > 5.27));
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
        assert!(rows >= 80, "only {rows} rows transcribed");
        // THE MINORITY IS REAL, and it is the half of the table most likely to
        // be flattened by a copy. Eighteen rows print "Adds": every Braton and
        // Burston Incarnon (six), BOTH Mausolon radials — its two rows are the
        // only Arch-Gun ones in the table and both print it — both Ferroxes,
        // and the eight the 2026-08-20 sweep brought in (the Ambassador's
        // charge, the Battacor's, both Opticors in both forms, and both
        // Trumnas' primary fire).
        assert_eq!(
            adds, 18,
            "the Adds minority moved — count it against the table before changing this"
        );
        // …and every one of them is named, so a row that quietly changes
        // bracket is a failure rather than a number that still adds up.
        let adders: std::collections::BTreeSet<&str> = all()
            .iter()
            .filter(|w| w.attack.compression.as_ref().is_some_and(|c| c.stacking == "adds"))
            .map(|w| w.id.as_str())
            .collect();
        assert_eq!(
            adders,
            [
                "ambassador_charged", "battacor_charged",
                "braton_incarnon", "braton_prime_incarnon", "braton_vandal_incarnon",
                "burston_incarnon", "burston_prime_incarnon",
                "ferrox", "tenet_ferrox",
                "mausolon", "mausolon_charged",
                "mk1_braton_incarnon",
                "opticor", "opticor_quick", "opticor_vandal", "opticor_vandal_quick",
                "trumna", "trumna_prime",
            ]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>()
        );
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
            // "Fire rate ramps from 20% baseline, increasing by 20% per shot",
            // so it is full from the FIFTH round: 0.20 + 4 x 0.20 = 1.00. The
            // page's Disadvantages also say "Requires 5-12 shot spool before
            // optimal performance" — a RANGE because two things spool at
            // different speeds on this weapon: the fire rate over 5 shots and
            // the PELLET COUNT over 12. Only the first is modelled, which is
            // what the weapon's own admission says.
            ("kuva_kohm", 0.20, 1.00, 4.0, 0.20, 5),
            // "Requires a spool-up of 7 shots before optimal fire rate is
            // achieved", and "Fire rate starts at 30% of the listed value, and
            // increases by 11.67% per shot" — 0.70 / 6 = 11.67% a shot, full
            // from the 7th, so the two published sentences reconcile exactly.
            ("coda_bubonico", 0.30, 1.00, 6.0, 0.11667, 7),
            // "Requires a spool-up of 5 shots before optimal fire rate is
            // achieved", and "fire rate starts at 10% of the listed value, and
            // increases by 22.5% per shot" — 0.90 / 4 = 22.5% a shot, full from
            // the 5th. The lowest opening rate in the roster.
            ("supra", 0.10, 1.00, 4.0, 0.225, 5),
            // "…a spool-up of 4 shots", "starts at 40% … increases by 20% per
            // shot" — 0.60 / 3 = 20% a shot, full from the 4th.
            ("supra_vandal", 0.40, 1.00, 3.0, 0.20, 4),
            // "Primary fire requires a spool-up of 9 shots before optimal fire
            // rate is achieved", and "fire rate starts at 40% of the listed
            // value, and increases by 7.5% per shot" — 0.60 / 8 = 7.5% a shot,
            // full from the 9th. The Prime's page prints the same two numbers.
            ("tenora", 0.40, 1.00, 8.0, 0.075, 9),
            ("tenora_prime", 0.40, 1.00, 8.0, 0.075, 9),
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

#[cfg(test)]
mod valence_tests {
    use super::*;
    use crate::damage::DamageType;
    use crate::loadout::WeaponBase;

    /// THE VALENCE BONUS IS BASE DAMAGE, and the arithmetic is the wiki's own
    /// sentence: *"ranging from 25-60% of the weapon's base damage … This
    /// additional bonus damage applies as weapon base damage, meaning elemental
    /// mods and status that scale from base / modified base damage will be
    /// affected."*
    ///
    /// The Kuva Nukor's 21 Radiation is the whole fixture: a Toxin progenitor
    /// at 60% adds 12.6 Toxin BESIDE it, and a Radiation one at 60% MERGES into
    /// it for 33.6 — the two cases that a naive "push a new element" would get
    /// half right.
    #[test]
    fn a_valence_bonus_is_base_damage_and_merges_with_the_element_it_matches() {
        let bare = WeaponBase::from_data("kuva_nukor", true, &[]);
        assert!((bare.base_vector.total() - 21.0).abs() < 1e-9, "the fixture moved");

        let mut toxin = bare.clone();
        apply_valence(&mut toxin, "kuva_nukor", "toxin", 0.60);
        assert!((toxin.base_vector.get(DamageType::Radiation) - 21.0).abs() < 1e-9);
        assert!((toxin.base_vector.get(DamageType::Toxin) - 12.6).abs() < 1e-9);
        assert!((toxin.base_vector.total() - 33.6).abs() < 1e-9);

        // …AND THE SAME ELEMENT MERGES rather than appearing twice.
        let mut rad = bare.clone();
        apply_valence(&mut rad, "kuva_nukor", "radiation", 0.60);
        assert!((rad.base_vector.get(DamageType::Radiation) - 33.6).abs() < 1e-9);
        assert!((rad.base_vector.total() - 33.6).abs() < 1e-9);

        // THE ROLL'S RANGE IS THE GAME'S, so a request outside it is clamped
        // rather than obeyed — 100% is not a bonus a Lich can hand out.
        let mut over = bare.clone();
        apply_valence(&mut over, "kuva_nukor", "heat", 1.0);
        assert!((over.base_vector.get(DamageType::Heat) - 21.0 * 0.60).abs() < 1e-9);
        let mut under = bare.clone();
        apply_valence(&mut under, "kuva_nukor", "heat", 0.0);
        assert!((under.base_vector.get(DamageType::Heat) - 21.0 * 0.25).abs() < 1e-9);

        // AN ELEMENT THE SPEC DOES NOT OFFER IS REFUSED. A Kuva bonus is never
        // Puncture or Slash, and a request that says so leaves the weapon
        // alone rather than inventing a progenitor group.
        let mut slash = bare.clone();
        apply_valence(&mut slash, "kuva_nukor", "slash", 0.60);
        assert!((slash.base_vector.total() - 21.0).abs() < 1e-9, "slash is not a progenitor element");

        // …AND A WEAPON WITH NO SPEC CANNOT BE HANDED ONE.
        let mut torid = WeaponBase::from_data("torid", true, &[]);
        let before = torid.base_vector.total();
        apply_valence(&mut torid, "torid", "heat", 0.60);
        assert!((torid.base_vector.total() - before).abs() < 1e-9);
        assert!(valence_of("torid").is_none());
        assert!(valence_of("kuva_nukor").is_some());
    }
}

#[cfg(test)]
mod condition_overload_catalog_tests {
    /// THE CO CATALOG'S DAMAGE COLUMN IS A FREE CROSS-CHECK OF THE SHOT.
    ///
    /// "Attack Unmodded Damage" is the whole SHOT — every pellet of it — while
    /// a weapon yaml carries the per-projectile damage and the pellet count
    /// separately. So `base_vector.total() x base_multishot` has to reproduce
    /// it, and a lost pellet count shows up here and nowhere else: the damage
    /// per projectile stays right, the panel stays plausible, and the weapon
    /// quietly deals a fraction of its shot.
    ///
    /// That is exactly how the Bronco was found (2026-08-12). Both Incarnon
    /// entries had `multishot: 1.0` where the base forms had 7, so the Incarnon
    /// Bronco dealt ONE SEVENTH of its shot — 22 against 154, and 34 against
    /// 238 on the Prime.
    ///
    /// Rows transcribed from `Condition_Overload_(Mechanic)?action=raw`. Only
    /// the entries the roster carries, and only the DIRECT attack of each,
    /// because a radial's own damage is not in this column.
    #[test]
    fn every_catalog_row_reproduces_our_shot_damage() {
        // (entry, the catalog's Attack Unmodded Damage)
        let rows: &[(&str, f64)] = &[
            ("angstrum_incarnon", 30.0),
            ("atomos_incarnon", 100.0),
            ("ballistica", 100.0),
            ("ballistica_prime", 304.0),        // "76" is per projectile; 4 bolts
            ("ballistica_prime_incarnon", 830.0),
            ("bronco_prime_incarnon", 238.0),   // 34 x 7 — the row that caught the bug
            ("cernos_prime", 552.0),
            ("cestra", 26.0),
            ("cestra_incarnon", 50.0),
            ("despair_incarnon", 60.0),
            ("dread", 336.0),
            ("dread_incarnon", 400.0),
            ("dual_toxocyst_incarnon", 75.0),
            ("felarx", 760.0),
            ("felarx_incarnon", 600.0),
            ("furis_incarnon", 100.0),
            ("kunai", 46.0),
            // PER PROJECTILE, like its MK1 below — the catalog says 40 and this
            // form fires two. See the note there.
            ("kunai_incarnon", 80.0),
            ("laetum", 160.0),
            ("laetum_incarnon", 100.0),
            ("lato_vandal_incarnon", 152.0),
            ("latron_incarnon", 50.0),
            ("lex_prime_incarnon", 1200.0),
            ("miter", 500.0),
            ("miter_incarnon", 60.0),
            // THE KUNAI FAMILY'S ROWS ARE PER PROJECTILE, and the catalog's
            // own Lato Vandal row is what proves the others are not: 152 is
            // that form's 76 damage TIMES its 2 multishot, while these two
            // carry 24 and 40, which are the damage alone. Three sources say
            // the multishot is 2 — the module's attack row, the shared Genesis
            // page ("2 base Multishot"), and the fact that this catalog is
            // community-sourced by its own header, with the CLASS and the
            // RELATIVE column being what this repo transcribes from it.
            // Doubled here rather than in the data, because the data is right
            // (2026-08-21).
            ("mk1_kunai_incarnon", 48.0),
            ("mk1_paris", 230.0),
            ("paris", 320.0),
            ("paris_incarnon", 460.0),          // the catalog's 520 is the PRIME's
            ("paris_prime", 360.0),
            ("paris_prime_incarnon", 520.0),
            ("rakta_ballistica", 300.0),
            ("shedu", 71.0),
            ("stug", 4.0),
            ("torid", 100.0),
            ("vasto_prime_incarnon", 420.0),
            ("zylok_incarnon", 400.0),
            ("zylok_prime_incarnon", 500.0),
        ];
        for (id, want) in rows {
            let b = crate::loadout::WeaponBase::from_data(id, false, &[]);
            let got = b.base_vector.total() * b.base_multishot.max(1.0);
            assert!(
                (got - want).abs() < 0.51,
                "{id}: the catalog's shot is {want}, ours is {:.1} x {:.0} pellets = {got:.1}",
                b.base_vector.total(), b.base_multishot
            );
        }
    }

    /// …AND THE RADIALS the catalog names, which are a separate column entry.
    /// Their listed number INCLUDES the flat-damage evolution, so the check is
    /// against the unevolved base the third column ("Relative To Base Damage")
    /// is computed from.
    #[test]
    fn every_catalog_radial_row_reproduces_our_explosion() {
        let rows: &[(&str, f64)] = &[
            ("braton_prime_incarnon", 70.0),    // catalog 74 = 70 + Daring Reverie's +4
            ("burston_prime_incarnon", 13.0),   // catalog 55 = 13 + 42
            ("zylok_prime_incarnon", 700.0),    // catalog 776 mixes in the base Zylok's +76
            ("akarius_prime", 509.0),
        ];
        for (id, want) in rows {
            let b = crate::loadout::WeaponBase::from_data(id, false, &[]);
            let r = b.radial.as_ref().unwrap_or_else(|| panic!("{id} has no radial"));
            assert!((r.base_vector.total() - want).abs() < 0.51,
                "{id}: the catalog's explosion is {want}, ours is {}", r.base_vector.total());
        }
    }
    /// "CO-BONUS DOES NOT USE BASE DAMAGE INCREASE EVOLUTION" — all eleven rows,
    /// checked by the catalog's OWN ARITHMETIC.
    ///
    /// Each row prints two damage figures and a percentage: "100 or 124 (with
    /// Evolution II)" against "100% or 81%". The second percentage is
    /// unmodded/evolved, which is exactly what `co_base_fraction` becomes when
    /// the named perk is applied — so the row checks itself, and the check
    /// fails if the flag is MISSING, on the WRONG PERK, or on a perk whose flat
    /// damage does not match.
    ///
    /// That third failure is not hypothetical. This list is the group that has
    /// gone wrong twice: eight weapons were missing the flag entirely
    /// (2026-08-12), and the Vasto Prime was still missing it when the CO
    /// mechanism was audited later the same day.
    ///
    /// A row that names "Evolution II Perk 1" or "Perk 2" means ONLY that perk
    /// is discrepant — its tier-mate feeds the CO term in full even when it
    /// raises base damage by the same amount, which is true of the Vasto Prime
    /// (Lone Gun and Deathtrap Trigger are both +24) and of the Dual Toxocyst.
    #[test]
    fn the_eleven_evolution_exclusion_rows_reproduce_their_own_percentages() {
        // (entry, perk, catalog unmodded, catalog with-evolution)
        let rows: &[(&str, &str, f64, f64)] = &[
            ("atomos_incarnon", "atomos_hoplite_virtue", 100.0, 124.0),
            ("atomos_incarnon", "atomos_paladin_virtue", 100.0, 124.0),
            ("bronco_prime_incarnon", "bronco_prime_speeding_bullet", 238.0, 448.0),
            ("cestra", "cestra_fortress_salvo", 26.0, 36.0),
            ("cestra", "cestra_steadfast_grit", 26.0, 36.0),
            ("cestra_incarnon", "cestra_fortress_salvo", 50.0, 60.0),
            ("cestra_incarnon", "cestra_steadfast_grit", 50.0, 60.0),
            ("despair_incarnon", "despair_stalkers_vendetta", 60.0, 120.0),
            ("dual_toxocyst_incarnon", "dual_toxocyst_carnage_reign", 75.0, 135.0),
            ("furis_incarnon", "furis_haven_foray", 100.0, 128.0),
            ("furis_incarnon", "furis_stormburst", 100.0, 128.0),
            // THE ONE ROW THE CATALOG CONTRADICTS ITSELF ON, so it carries our
            // number and the row's, and a note rather than a silent choice.
            //
            // The Lato Vandal's Incarnon form is 2 pellets of 76 (wiki infobox:
            // "Total Damage 152 ... Multishot 2 (76.00 damage per projectile)")
            // and Haven Foray adds +22. The row prints "152 or 174", i.e. the
            // +22 landing ONCE on the shot — but every other multi-pellet row in
            // the same table is PER PELLET: the Bronco Prime's 238 -> 448 is
            // 7 x 30 and the Vasto Prime's 420 -> 564 is 6 x 24, both exact.
            //
            // A flat base-damage evolution raises the BASE DAMAGE stat, which a
            // multishot weapon lists per projectile — so per pellet is what the
            // engine does, it agrees with the catalog on both of the other
            // multi-pellet rows, and two cards (the Vasto's Lone Gun, the Soma's
            // Fresh Havoc) say "applied per pellet in Incarnon Form" outright.
            // NEEDS AN IN-GAME MEASUREMENT to settle; until then the row is the
            // outlier, not the engine.
            ("lato_vandal_incarnon", "lato_vandal_haven_foray", 76.0, 98.0),
            ("lex_prime_incarnon", "lex_prime_hoplite_virtue", 1200.0, 1220.0),
            ("lex_prime_incarnon", "lex_prime_trusty_sidearm", 1200.0, 1220.0),
            ("vasto_prime_incarnon", "vasto_prime_deathtrap_trigger", 420.0, 564.0),
            ("zylok_prime_incarnon", "zylok_prime_maulers_magazine", 500.0, 530.0),
            ("zylok_prime_incarnon", "zylok_prime_precisions_payoff", 500.0, 530.0),
        ];
        for (entry, perk, unmodded, evolved) in rows {
            let b = crate::loadout::WeaponBase::from_data(entry, false, &[perk]);
            let want = unmodded / evolved;
            assert!((b.co_base_fraction() - want).abs() < 1e-6,
                "{entry} + {perk}: the catalog says CO computes on {unmodded} of {evolved}                  ({:.1}%), our co_base_fraction is {:.4}", want * 100.0, b.co_base_fraction());
        }

        // …AND THE TIER-MATES THE CATALOG DOES NOT NAME feed the term in full.
        // Absence from the table is a positive statement, so a perk that raises
        // base damage by the same number as its named sibling is still ordinary.
        //
        // THIS HALF IS THE ONE UNDER PRESSURE. The wiki's Math section lists
        // "Base Damage increases from Incarnon Genesis Evolutions" among the
        // things Adding CO ignores, with no "some" on it — and read as a law it
        // would flip all 107 of these. It is not a law (owner, 2026-08-12), and
        // the page argues that side itself: the same list's "Bow charging"
        // bullet is enumerated by ~15 catalog rows that disagree with each
        // other (50%, 40%, 38%, 57%, 65%, 25%) and contains outright
        // counter-examples — the Cinta and Nataruk are charged bows at 100%
        // Multiplying and the Balefire Charger is 0%. See docs/CATALOGS.md.
        // THIS LOOP TURNED AROUND, and the comment is the record of it.
        //
        // Every entry here is an ADDING perk the catalog does not list, and
        // each was asserted to compute CO on its FULL evolved base because the
        // table "lists only discrepant attacks". Four perks were measured on
        // 2026-08-16 and all four came back excluded — the Dual Toxocyst's two
        // (M49) and the Torid Incarnon's two (M50) — and the reading of the
        // table that produced this loop did not survive it: its eleven
        // double-valued rows are the only ones that ever measured an evolved
        // weapon, all eleven exclude, and the "100%" rows print an UNEVOLVED
        // base in their own damage column, which is true by construction and
        // answers a different question. 15 to 0.
        //
        // THE FLIP IS ADDING-ONLY (owner, 2026-08-16). Nothing has measured a
        // Multiplying entry's evolved CO base, so those are untouched and the
        // Torid's base form is the open experiment — its own test pins it at
        // 1.0. These five are kept, pointing the other way, because they are
        // still the set that distinguishes the two defaults: a measurement
        // finding an INCLUDED Adding perk is what would edit this loop.
        for (entry, perk) in [
            ("vasto_prime_incarnon", "vasto_prime_lone_gun"),
            ("bronco_prime_incarnon", "bronco_prime_infused_shots"),
            ("despair_incarnon", "despair_fatal_affliction"),
            ("lato_vandal_incarnon", "lato_vandal_reified_bane"),
            ("vasto_incarnon", "vasto_deathtrap_trigger"),
        ] {
            let b = crate::loadout::WeaponBase::from_data(entry, false, &[perk]);
            let bare = crate::loadout::WeaponBase::from_data(entry, false, &[]);
            let f = bare.base_vector.total() / b.base_vector.total();
            assert!(f < 0.999, "{entry} + {perk} raises no base damage");
            assert_eq!(b.co_behavior, crate::loadout::CoBehavior::AdditiveWithBaseDamage);
            assert!((b.co_base_fraction() - f).abs() < 1e-9,
                "{entry} + {perk}: an Adding entry computes CO on the UNEVOLVED base \
                 by default — expected {f:.4}, got {:.4}", b.co_base_fraction());
        }
    }

    /// THE OTHER HALF, ROSTER-WIDE: NO EVOLUTION DILUTES A `Multiplying` ENTRY.
    ///
    /// The loop above is `Adding`, where the term reads the UNEVOLVED base. The
    /// Torid's base form measured the opposite answer on the other class (M51):
    /// the same two tier-2 perks, +51 and +31, and the CO multiplier came back
    /// 1.40 and 1.80 under BOTH — so a `Multiplying` term reads the FULL
    /// evolved base and the two classes disagree.
    ///
    /// GENERALISED TO ALL 26 ENTRIES ON ONE WEAPON'S READING (owner,
    /// 2026-08-16), deliberately ahead of the catalog: the wiki prints a
    /// fraction for a minority of attacks, this rule beats that table, and a
    /// measurement that contradicts it edits ONE weapon's yaml rather than this.
    ///
    /// It asserts the PROPERTY rather than the 26 numbers, which is what makes
    /// it hold for a weapon nobody has entered yet: the fraction with the whole
    /// evolution ladder installed equals the fraction with none of it. A future
    /// perk declaring `co_base_excludes_this_evolution` without scoping it to
    /// the form it was measured on fails HERE — which is exactly the reach the
    /// Torid's own pair would have had without `co_base_excludes_only_form`.
    ///
    /// The flat 1.0 is asserted SEPARATELY and only as a snapshot of today's
    /// roster: it is what the reserved `co_base_fraction:` slot reads on every
    /// Multiplying entry, and the day one weapon declares otherwise on evidence
    /// that half moves while the invariant above does not.
    #[test]
    fn no_evolution_dilutes_a_multiplying_co_base() {
        let mut checked = 0;
        for spec in crate::weapons_data::all() {
            let bare = crate::loadout::WeaponBase::from_data(&spec.id, false, &[]);
            if bare.co_behavior != crate::loadout::CoBehavior::Independent {
                continue;
            }
            // The GROUP owns the evolutions, not the form.
            let group = spec.transform_group.as_deref().unwrap_or(&spec.id);
            let ids: Vec<&str> = (1..=crate::evolutions_data::tier_count(group))
                .flat_map(|t| crate::evolutions_data::options(group, t))
                .map(|o| o.id.as_str())
                .collect();
            if ids.is_empty() {
                continue;
            }
            checked += 1;
            let loaded = crate::loadout::WeaponBase::from_data(&spec.id, false, &ids);
            assert!(
                loaded.base_vector.total() >= bare.base_vector.total(),
                "{}: the ladder lowered the panel", spec.id
            );
            assert!(
                (loaded.co_base_fraction() - bare.co_base_fraction()).abs() < 1e-9,
                "{}: an evolution diluted a Multiplying CO base ({:.4} -> {:.4}); \
                 a Multiplying term reads the FULL evolved base (M51)",
                spec.id, bare.co_base_fraction(), loaded.co_base_fraction()
            );
            // TODAY'S ROSTER — the reserved slot, unexercised everywhere.
            assert!(
                (loaded.co_base_fraction() - 1.0).abs() < 1e-9,
                "{}: no Multiplying entry declares a CO base fraction yet, got {:.4}",
                spec.id, loaded.co_base_fraction()
            );
        }
        assert!(checked >= 8, "only {checked} Multiplying entries with a ladder checked");
    }

}

/// MODULAR WEAPONS — a Kitgun's parts reaching the panel a fight reads.
///
/// Its own module rather than a corner of the CO catalog's: what it is about is
/// [`spec_assembled`], and a test's home is part of what it says.
#[cfg(test)]
mod echo_tests {
    /// **THE LAETUM'S INCARNON FORM DOUBLES SECONDARY IRRADIATE'S ECHO**, and
    /// its base form does not (owner, 2026-08-24, M59). 1.8x on a pure
    /// single-target weapon, 3.6x here.
    ///
    /// The pair is the whole point: a test asserting only that the Incarnon
    /// form is 2.0 passes just as well on a build that applied it to the entire
    /// weapon, which the base form's own measurement contradicts.
    #[test]
    fn the_laetums_incarnon_form_doubles_the_echo_and_its_base_form_does_not() {
        let m = |id: &str| super::spec(id).unwrap_or_else(|| panic!("{id}")).echo_multiplier;
        assert_eq!(m("laetum_incarnon"), 2.0);
        assert_eq!(m("laetum"), 1.0, "the base form measures the ordinary 1.8x");
        // …AND IT REACHES THE FIGHT. A number in a yaml that no panel carries
        // is a number nothing computes.
        let base = crate::loadout::WeaponBase::from_data("laetum_incarnon", false, &[]);
        let refs: Vec<&crate::loadout::ModDef> = Vec::new();
        let panel = crate::loadout::resolve(&base, &refs, crate::loadout::StackPolicy::Emergent);
        assert_eq!(panel.echo_multiplier, 2.0);
    }

    /// **IT IS THE ONLY ONE, AND THAT IS ASSERTED RATHER THAN ASSUMED.** The
    /// owner's reading is that the game counts the attack's damage components —
    /// a direct hit and a radial give 1.8 + 1.8 — which would make every
    /// direct+radial weapon in the roster a candidate. NONE of the others has
    /// been measured, so none of them carries the field, and generalising one
    /// measurement to a class is what `docs/CATALOGS.md` forbids.
    ///
    /// This test is the note to come back to: the day somebody measures a
    /// second weapon it fails, names both, and forces the decision to be made
    /// on purpose rather than by a default.
    #[test]
    fn only_the_measured_entry_carries_an_echo_coefficient() {
        let odd: Vec<&str> = super::all()
            .iter()
            .filter(|s| (s.echo_multiplier - 1.0).abs() > 1e-9)
            .map(|s| s.id.as_str())
            .collect();
        assert_eq!(odd, ["laetum_incarnon"], "an unmeasured entry gained a coefficient");
    }
}

#[cfg(test)]
mod url_tests {
    /// **A WEAPON HAS A URL OF ITS OWN.** `/weapons/<Wiki_Name>` is built from
    /// the English display name with a parenthesised qualifier stripped — the
    /// qualifier is OURS rather than the page's ("Larkspur Prime (Atmosphere)"
    /// is one wiki page with two stat columns and we ship the ground one), so
    /// it never reaches a URL. `build_site_app.py::wiki_name` and `wikiSlug`
    /// in app.js do exactly this, and the site PRERENDERS one directory per
    /// roster weapon from it.
    ///
    /// TWO ENTRIES ON ONE SLUG IS SILENT DATA LOSS, in both directions: the
    /// route resolves to whichever the lookup finds first, so the other weapon
    /// is unreachable by link, by bare URL and by crawler — and the prerendered
    /// `site/weapons/<Wiki_Name>/index.html` of one simply overwrites the
    /// other's, with its title, description, canonical and OG card.
    ///
    /// It was found by `check_pages`, which reports it as three `WRONG WEAPON`
    /// lines after fifty-five minutes of browser sweep (2026-08-25). This says
    /// the same thing in a millisecond, which is what makes it a check somebody
    /// runs.
    ///
    /// TOMBFINGER IS THE FIRST OF A KIND, not a one-off: a kitgun chamber is
    /// TWO roster entries because the SLOT is the weapon (it decides the mod
    /// pool), and the wiki gives a chamber ONE page. Catchmoon's chamber data
    /// is already here, so the next dual-slot kitgun to reach the roster
    /// collides the same way.
    ///
    /// SO IT IS A RATCHET RATHER THAN A FLAT ASSERTION. The fix is a URL
    /// DECISION — which entry keeps the bare name and how the other is spelled
    /// — and it changes what an already posted link means, which is the
    /// owner's call and not something a test may guess at. What a test CAN do
    /// is stop the next one arriving in silence, so the known collision is
    /// written down and everything else fails.
    ///
    /// `KNOWN_URL_CLASHES` MAY ONLY SHRINK, the same way `naming::FROZEN` may
    /// — an entry removed is a bug fixed, an entry added is the bug spreading,
    /// so growing it needs the same deliberate act as re-freezing a manifest.
    /// The non-breaking shape of the fix, for whoever takes it, and it is an
    /// OBSERVATION rather than a preference: `check_pages` reports the loser by
    /// name — six lines reading `tombfinger_secondary WRONG WEAPON
    /// tombfinger_primary`, in both languages — so the bare slug already means
    /// the PRIMARY. Giving the qualifier to the SECONDARY therefore leaves
    /// every link already posted meaning exactly what it means today, and makes
    /// the unreachable entry reachable.
    const KNOWN_URL_CLASHES: &[&str] = &[
        // The kitgun chamber built into both Gunsmith slots. One wiki page,
        // two roster entries, and `/weapons/Tombfinger` can only be one.
        "/weapons/Tombfinger <- tombfinger_primary, tombfinger_secondary",
    ];

    #[test]
    fn no_two_weapons_want_the_same_url() {
        use std::collections::BTreeMap;
        let mut by_slug: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        for w in super::roster() {
            let slug = w.name.split(" (").next().unwrap_or(&w.name).replace(' ', "_");
            by_slug.entry(slug).or_default().push(&w.id);
        }
        let clashes: Vec<String> = by_slug
            .iter()
            .filter(|(_, ids)| ids.len() > 1)
            .map(|(slug, ids)| format!("/weapons/{slug} <- {}", ids.join(", ")))
            .collect();
        let fresh: Vec<&String> =
            clashes.iter().filter(|c| !KNOWN_URL_CLASHES.contains(&c.as_str())).collect();
        assert!(
            fresh.is_empty(),
            "two roster entries want one URL, so one of them is unreachable \
             and its prerendered page is overwritten:\n{}",
            fresh.iter().map(|c| c.as_str()).collect::<Vec<_>>().join("\n")
        );
        // AND THE LIST MAY ONLY SHRINK. A fixed collision left written down
        // here would silently re-admit the next one that spells itself the
        // same way, which is the exact failure this exists to end.
        let stale: Vec<&&str> =
            KNOWN_URL_CLASHES.iter().filter(|k| !clashes.iter().any(|c| c == *k)).collect();
        assert!(
            stale.is_empty(),
            "these URL collisions are FIXED — delete them from \
             KNOWN_URL_CLASHES:\n{}",
            stale.iter().map(|s| **s).collect::<Vec<_>>().join("\n")
        );
    }
}

#[cfg(test)]
mod modular_tests {
    use super::{spec, spec_assembled};

    /// A KITGUN'S ASSEMBLY REACHES THE PANEL — the whole point of
    /// `spec_assembled`, asserted on the numbers a fight actually reads rather
    /// than on the spec it was composed from.
    ///
    /// The roster entry carries the chamber's `base` PREVIEW, so the test is
    /// that naming an assembly MOVES every number it should and moves them to
    /// the parts' own values.
    #[test]
    fn an_assembly_composes_all_the_way_into_a_panel() {
        use crate::kitguns_data::Assembly;
        // NAMING NO ASSEMBLY IS THE DEFAULT ONE, never the chamber's preview —
        // so no path can produce a preview-based panel by forgetting to pass
        // parts. Asserted against the default composed by hand, because the
        // whole point is that the two agree without the caller knowing.
        let unnamed = crate::loadout::WeaponBase::from_data("tombfinger_secondary", false, &[]);
        let dflt = crate::kitguns_data::default_assembly("tombfinger_secondary").unwrap();
        assert_eq!(dflt.grip, "ulnaris", "the grip nearest the `base` preview");
        assert_eq!(dflt.loader, "bellows", "the first loader that changes nothing");
        let named = crate::loadout::WeaponBase::from_data_assembled(
            "tombfinger_secondary",
            false,
            &[],
            Some(&dflt),
        );
        assert_eq!(unnamed.base_vector, named.base_vector);
        assert_eq!(unnamed.base_fire_rate, named.base_fire_rate);
        assert_eq!(unnamed.magazine_size, named.magazine_size);
        // …and it is NOT the preview: the module's no-grip row totals 84 and
        // Ulnaris totals 100.01. The panel's own vector is the DIRECT hit, so
        // the shot is that plus the explosion — which is the carve holding.
        let whole = unnamed.base_vector.total()
            + unnamed.radial.as_ref().map_or(0.0, |r| r.base_vector.total());
        assert!((whole - 100.01).abs() < 1e-6, "{whole}");
        let haymaker = Assembly {
            chamber: "tombfinger".into(),
            grip: "haymaker".into(),
            loader: "thunderdrum".into(),
        };
        let built = crate::loadout::WeaponBase::from_data_assembled(
            "tombfinger_secondary",
            false,
            &[],
            Some(&haymaker),
        );

        // THE DIRECT HIT IS WHAT THE EXPLOSION LEAVES. Haymaker is 32 Impact +
        // 25 Puncture + 123 Radiation, and 19.5% of the Radiation stays here.
        let d = &built.base_vector;
        assert!((d.get(crate::damage::DamageType::Impact) - 32.0).abs() < 1e-9, "{d:?}");
        assert!((d.get(crate::damage::DamageType::Puncture) - 25.0).abs() < 1e-9, "{d:?}");
        assert!(
            (d.get(crate::damage::DamageType::Radiation) - 123.0 * 0.195).abs() < 1e-9,
            "{d:?}"
        );
        // …AND THE OTHER 80.5% IS THE EXPLOSION.
        let r = built.radial.as_ref().expect("the secondary explodes");
        assert!(
            (r.base_vector.get(crate::damage::DamageType::Radiation) - 123.0 * 0.805).abs() < 1e-9,
            "{:?}",
            r.base_vector
        );
        assert!((r.radius_m - 1.9).abs() < 1e-9);

        // EVERY OTHER AXIS THE ASSEMBLY OWNS MOVED, and moved to the part's own
        // number: the grip's fire rate, the loader's magazine class and reload,
        // and crit and status as the loader's additive deltas on the chamber.
        assert!((built.base_fire_rate - 2.17).abs() < 1e-9, "{}", built.base_fire_rate);
        assert_ne!(built.base_fire_rate, unnamed.base_fire_rate);
        // Thunderdrum is -4% crit chance, -0.1 crit damage, +7% status, the
        // `highest` magazine class (29 rounds on this chamber) and a 2.1 s
        // reload. TWO OF THE THREE DELTAS ARE NEGATIVE, which is the whole
        // reason they are additive and not a multiplier.
        assert!((built.base_crit_chance - 0.20).abs() < 1e-9, "{}", built.base_crit_chance);
        assert!((built.base_crit_damage - 1.9).abs() < 1e-9, "{}", built.base_crit_damage);
        assert!((built.base_status_chance - 0.31).abs() < 1e-9, "{}", built.base_status_chance);
        assert_eq!(built.magazine_size, 29.0);
        assert!((built.base_reload - 2.1).abs() < 1e-9, "{}", built.base_reload);

        // AND A GRIP FROM THE OTHER SLOT DOES NOT COMPOSE. It is a real weapon
        // and it is the wrong one, which is the mismatch that reads as working.
        let tremor = Assembly {
            chamber: "tombfinger".into(),
            grip: "tremor".into(),
            loader: "thunderdrum".into(),
        };
        assert!(
            spec_assembled(spec("tombfinger_secondary").unwrap(), Some(&tremor)).is_none(),
            "a primary grip composed into the secondary entry"
        );
        // …and the same grip on the PRIMARY entry does.
        assert!(
            spec_assembled(spec("tombfinger_primary").unwrap(), Some(&tremor)).is_some()
        );
    }

    /// A KITGUN'S ROSTER ENTRY AND ITS PARTS FILE MUST AGREE ABOUT WHAT THE
    /// WEAPON IS, and every entry that names a chamber must name one that
    /// exists. Both are the kind of mismatch that composes into a plausible
    /// weapon rather than into an error.
    #[test]
    fn every_modular_entry_matches_its_chamber() {
        for s in super::all() {
            let Some(k) = s.kitgun.as_deref() else { continue };
            let c = crate::kitguns_data::chambers()
                .iter()
                .find(|c| c.id == k)
                .unwrap_or_else(|| panic!("{}: no chamber record {k}", s.id));
            assert_eq!(c.slot, s.slot, "{}: slot", s.id);
            assert_eq!(
                c.blast.is_some(),
                s.attack.radial.is_some(),
                "{}: the chamber explodes {} and the entry {}",
                s.id,
                c.blast.is_some(),
                s.attack.radial.is_some()
            );
            // THE ENTRY'S FORM MUST BE ONE THE CHAMBER PUBLISHES AN EXPLOSION
            // FOR. A form the parts file has never heard of composes to nothing
            // at all, and the panel that would have said so panics.
            if let Some(b) = &c.blast {
                assert!(
                    b.forms.contains_key(&s.form),
                    "{}: form `{}` has no explosion; the chamber states {:?}",
                    s.id,
                    s.form,
                    b.forms.keys().collect::<Vec<_>>()
                );
            }
        }
    }
    /// PAX CHARGE REMOVES THE RELOAD, and this is that end to end: the arcane
    /// grants nothing but a reload-speed bonus and a flag, the CHAMBER states
    /// the rate, and the sim's own battery — written for the Shedu — does the
    /// rest. Asserted on the fight rather than on the panel, because a flag
    /// that reaches a card and not the loop is exactly what this is for.
    #[test]
    fn pax_charge_turns_the_magazine_into_a_battery() {
        use crate::loadout::StackPolicy;
        let base = crate::loadout::WeaponBase::from_data("tombfinger_secondary", false, &[]);
        assert_eq!(base.recharge_per_second, Some(50.0), "the chamber states its rate");

        // ITS OWN SEAT, and NOT the weapon's. *"These can be installed
        // simultaneously with Secondary/Primary arcanes"* (wiki, `Kitgun`), so
        // a Kitgun holds one of each and the two never compete.
        let arc = crate::arcanes_data::for_slot("kitgun", "pax_charge")
            .expect("pax charge is offered in the Kitgun seat");
        for seat in ["primary", "secondary"] {
            assert!(
                crate::arcanes_data::for_slot(seat, "pax_charge").is_none(),
                "{seat}: a Kitgun arcane is competing with the ordinary pool"
            );
        }
        // …AND ON NOTHING ELSE: the equip rule is a TRAIT, since no class can
        // say "Kitgun" — a secondary Tombfinger is a `pistol` exactly like a Lex.
        for w in ["lex", "braton_prime"] {
            assert!(
                crate::arcanes_data::pool_for_weapon(w, "kitgun").is_empty(),
                "{w} is offered a Kitgun arcane"
            );
        }
        // All eight, on both entries, in the Kitgun seat and nowhere else.
        for w in ["tombfinger_primary", "tombfinger_secondary"] {
            let kit = crate::arcanes_data::pool_for_weapon(w, "kitgun");
            assert_eq!(kit.len(), 8, "{w}: the four Pax and four Residual arcanes");
            let own = crate::weapons_data::arcane_pools(w);
            assert_eq!(own[0], "kitgun", "{w}: the distinctive seat comes first");
            assert_eq!(own.len(), 2, "{w}: a Kitgun holds one of each");
            assert!(
                !crate::arcanes_data::pool_for_weapon(w, own[1])
                    .iter()
                    .any(|a| a.id.starts_with("pax_") || a.id.starts_with("residual_")),
                "{w}: a Kitgun arcane leaked into the ordinary seat"
            );
        }
        // A NON-MODULAR WEAPON IS UNCHANGED — one seat, its own slot's.
        assert_eq!(crate::weapons_data::arcane_pools("lex"), vec!["secondary"]);
        assert_eq!(crate::weapons_data::arcane_pools("braton_prime"), vec!["primary"]);

        let tenno = crate::tenno_data::default_tenno();
        let fx = arc.fx(arc.max_rank, StackPolicy::Emergent, &["modular"], tenno);
        // MAX RANK IS +50% RECHARGE DELAY REDUCTION, joining the reload bucket.
        assert!((fx.reload_bonus - 0.50).abs() < 1e-9, "{}", fx.reload_bonus);
        assert!(fx.rechargeable_magazine);

        // THE FIGHT. Same weapon, same everything, with and without the arcane.
        let arena = crate::arena::Arena::training(12.0);
        let panel = crate::loadout::resolve_for(&base, &[], StackPolicy::Emergent, tenno);
        let plain = crate::dummy::DummyParams::from_panel(
            &panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        let charged = crate::dummy::DummyParams::from_panel(&panel, &arena, &fx);
        assert!(plain.battery.is_none(), "an ordinary Kitgun has no battery");
        let b = charged.battery.expect("pax charge installs one");
        assert_eq!(b.regen_per_second, 50.0);
        // THE DELAY IS THE RELOAD, shortened by the arcane's own bonus: the
        // default assembly's Bellows loader reloads in 2.1 s, and 2.1 / 1.5 is
        // 1.4 s — which is the worked example on the arcane's own page.
        assert!((b.delay_empty_seconds - 1.4).abs() < 1e-6, "{}", b.delay_empty_seconds);
        assert_eq!(b.delay_partial_seconds, b.delay_empty_seconds);
    }

}
