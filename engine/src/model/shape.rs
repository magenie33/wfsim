// SPDX-License-Identifier: AGPL-3.0-or-later
//! WHAT A WEAPON'S DAMAGE LOOKS LIKE IN SPACE: fields, radials, clusters,
//! ricochets, beams, gauges and the melee combo script.

use super::*;
use crate::rules::damage::DamageVector;
use serde::Deserialize;

/// What happens when a second field lands on a target that already has one.
///
/// Weapon DATA, not a global rule — the Torid STACKS (✅ measured, MEASUREMENTS
/// M13) but a future weapon may refresh, and the answer is worth up to ~5x
/// sustained single-target DPS on a 5-round magazine, so it is not a constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldStacking {
    /// N concurrent tick streams, one per grenade — what the Torid does.
    #[default]
    Stack,
    /// One field, re-armed: a second grenade resets duration instead of adding
    /// a stream.
    Refresh,
}

/// A weapon's LINGERING FIELD attack part — an area that persists and TICKS
/// rather than landing once (Torid's Toxin cloud), unmodded. MECHANICS §7.
#[derive(Debug, Clone, PartialEq)]
pub struct LingeringBase {
    pub base_vector: DamageVector,
    pub base_crit_chance: f64,
    pub base_crit_damage: f64,
    pub base_status_chance: f64,
    /// Ticks per second (the data module's `FireRate` for the part: Torid 1).
    pub tick_rate: f64,
    /// How long the field lives (`EffectDuration`: Torid 10 s), from its own
    /// first tick.
    pub duration_seconds: f64,
    /// See [`crate::data::weapons::LingeringSpec::first_tick_delay_seconds`].
    pub first_tick_delay_seconds: f64,
    /// See [`crate::data::weapons::LingeringSpec::forced_procs`] — the field's
    /// OWN, never the direct part's.
    pub forced_procs: crate::rules::damage::ForcedProcs,
    pub radius_m: f64,
    pub falloff_start_m: f64,
    /// Torid's cloud is `reduction 1.0` — damage falls to ZERO at the rim,
    /// unlike the Laetum radial's 0.2.
    pub falloff_reduction: f64,
    pub stacking: FieldStacking,
    /// Does this field take Condition Overload? **Default NO** — the normal
    /// rule is what the mods say on the tin: CO boosts DIRECT hits only, and an
    /// AoE part should get nothing. The Torid's cloud is an ANOMALY that the CO
    /// catalog records with a row of its own ("in theory it
    /// would not get it, but the programmer let it"), so the weapon declares it
    /// rather than the engine assuming every field behaves that way.
    pub takes_condition_overload: bool,
    /// See [`crate::data::weapons::LingeringSpec::elemental_mods_apply`].
    pub elemental_mods_apply: bool,
    /// See [`crate::data::weapons::LingeringSpec::can_crit`].
    pub can_crit: bool,
    /// See [`crate::data::weapons::LingeringSpec::status_mods_apply`].
    pub status_mods_apply: bool,
}

/// A weapon's radial (explosion) attack part, unmodded.
#[derive(Debug, Clone)]
pub struct RadialBase {
    pub base_vector: DamageVector,
    pub base_crit_chance: f64,
    pub base_crit_damage: f64,
    pub base_status_chance: f64,
    /// Where this explosion goes off — see
    /// [`crate::model::BlastKind`]. `Contact` on every weapon but the
    /// handful whose round bores on through and detonates behind what it hit.
    pub blast_kind: crate::model::BlastKind,
    /// Blast radius = the falloff `end` distance.
    pub radius_m: f64,
    /// See [`crate::data::weapons::RadialSpec::takes_blast_radius_mods`].
    pub takes_blast_radius_mods: bool,
    /// Linear falloff window and the fraction of damage REMOVED at max
    /// distance: `mult(d) = 1 − reduction × clamp((d−start)/(end−start))`.
    /// Only bites once the sim has targets away from the epicentre.
    pub falloff_start_m: f64,
    pub falloff_reduction: f64,
    /// See [`crate::data::weapons::RadialSpec::forced_procs`] — the EXPLOSION's
    /// own, which is not the direct part's.
    pub forced_procs: crate::rules::damage::ForcedProcs,
    /// Does this explosion take Condition Overload? **Default NO** — the mods
    /// say CO boosts DIRECT hits, so an AoE part is not supposed to receive it
    /// at all. Some entries do anyway, and the CO catalog lists them one at a
    /// time: the Zylok's Incarnon radial has a row reading "Radial hit only
    /// receives CO bonus on target directly hit by bullet", which the sim's
    /// single-target arena always is. Declared per weapon because it is a
    /// per-entry quirk, never a rule (MECHANICS §6).
    pub takes_condition_overload: bool,
    /// See [`crate::data::weapons::RadialSpec::takes_multishot`].
    pub takes_multishot: bool,
    /// THE ORIGINAL BASE of this explosion — the radial's own [`WeaponBase::co_base`],
    /// and it needs its own because an evolution can raise what the explosion
    /// DEALS without raising what its CO term reads.
    pub co_base: f64,
}

/// THE BOMBLETS AN EXPLOSION THROWS OUT, unmodded — see
/// [`crate::data::weapons::ClusterSpec`].
///
/// BOTH HALVES ARE A [`RadialBase`] so they resolve through exactly the mod
/// buckets an explosion does, and the CONTACT hit is the one that needs saying:
/// its radius is [`crate::rules::space::BODY_RADIUS_M`], the smallest sphere that
/// means "the body this bomblet touched and nobody else". A radius of ZERO
/// would mean nobody at all — `falloff_at` is exclusive at the edge.
#[derive(Debug, Clone)]
pub struct ClusterBase {
    /// How many bomblets one detonation releases.
    pub count: f64,
    /// The contact hit, per bomblet.
    pub contact: RadialBase,
    /// The bomblet's own explosion.
    pub blast: RadialBase,
}

/// THE GRENADES A RELOAD THROWS — see
/// [`crate::data::weapons::ReloadGrenadeSpec`]. Two throws, from empty and
/// partial, each a contact hit and an explosion per grenade.
#[derive(Debug, Clone)]
pub struct ReloadGrenadeBase {
    pub count: u32,
    pub fan_deg: f64,
    pub from_empty: GrenadeThrowBase,
    pub partial: GrenadeThrowBase,
}

/// One throw's grenade, the bomblet's shape.
#[derive(Debug, Clone)]
pub struct GrenadeThrowBase {
    pub contact: RadialBase,
    pub blast: RadialBase,
}

/// THE CO TERM'S BASE, AND THE BASE IT IS A SHARE OF — carried together.
///
/// The FACT is `absolute`: an evolution's flat add leaves it alone, a valence
/// bonus scales it, and it never moves for any other reason. The damage math
/// wants a RATIO, and a ratio is only correct against the base it was derived
/// from — so the two travel as one value and the division happens here rather
/// than at four call sites. A stage whose base differs (a melee swing landing
/// on half the vector, an explosion an evolution raised) says so with
/// [`Self::against`] instead of recomputing a fraction of its own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoBase {
    absolute: f64,
    of: f64,
    stage: CoStage,
}

/// WHICH PART OF AN ATTACK A CO BASE BELONGS TO.
///
/// A stage that reads its OWN base is the rule; one that reads another's is an
/// exception the catalog states per weapon, and [`CoBase::borrowed_for`] is
/// where it gets written down. The tag is checked against the stage actually
/// being resolved, so a fifth stage added by copying the line above it fires in
/// the test suite instead of shipping a silently wrong denominator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoStage {
    /// The bullet.
    Direct,
    /// The explosion, which carries its own base — an evolution can raise what
    /// it DEALS without raising what its CO term reads.
    Radial,
    /// A lingering field. The catalog puts the Torid's cloud on the same base
    /// as its main fire, so this one borrows.
    Field,
}

impl Default for CoBase {
    /// READS THE WHOLE BASE, which is what a weapon with no catalog row does.
    fn default() -> Self {
        Self { absolute: 1.0, of: 1.0, stage: CoStage::Direct }
    }
}

impl CoBase {
    pub fn new(absolute: f64, of: f64, stage: CoStage) -> Self {
        Self { absolute, of, stage }
    }

    /// A term that reads the whole of whatever base it lands on — the direct
    /// hit, which is what a weapon with no catalog row has.
    pub fn whole() -> Self {
        Self::default()
    }

    /// The same, for a part that is not the bullet.
    pub fn whole_for(stage: CoStage) -> Self {
        Self { stage, ..Self::default() }
    }

    /// The share of the base the CO term multiplies. 1 when there is no base
    /// to divide by, which is the same answer an unstated catalog row gives.
    ///
    /// CLAMPED AT THE WHOLE BASE. No catalog row states a term that reads more
    /// than the attack it is on, and a denominator narrowed by [`Self::against`]
    /// is the one place arithmetic could push it past 1.
    pub fn fraction(self) -> f64 {
        if self.of > 0.0 { (self.absolute / self.of).min(1.0) } else { 1.0 }
    }

    /// The base this share is taken of — for a stage that must re-aim it.
    pub fn of(self) -> f64 {
        self.of
    }

    /// THE SAME ABSOLUTE, AGAINST A DIFFERENT DENOMINATOR — for a stage whose
    /// base is not the one this was built from.
    pub fn against(self, of: f64) -> Self {
        Self { of, ..self }
    }

    /// Which stage this base belongs to.
    pub fn stage(self) -> CoStage {
        self.stage
    }

    /// DELIBERATELY READ BY ANOTHER STAGE — the catalog exception, written
    /// down. Only for a part whose row puts it on a base that is not its own;
    /// a part that has its own builds it with [`Self::new`] instead.
    pub fn borrowed_for(self, stage: CoStage) -> Self {
        Self { stage, ..self }
    }
}

impl RadialBase {
    /// See [`WeaponBase::co_base_pair`] — paired, never stored.
    pub fn co_base_pair(&self) -> CoBase {
        CoBase::new(self.co_base, self.base_vector.total(), CoStage::Radial)
    }

    /// The share alone, for a reader with no stage to pair it with.
    pub fn co_base_fraction(&self) -> f64 {
        self.co_base_pair().fraction()
    }
}

/// THE CONE, resolved: degrees from the reticle, accuracy mods applied.
///
/// `min` is the first shot's and `max` is where sustained fire takes it — see
/// [`crate::model::SpreadSpec`], which is also where the bloom between
/// them is written down as unmodelled.
#[derive(Debug, Clone, Copy)]
pub struct Spread {
    pub min_deg: f64,
    pub max_deg: f64,
}

impl Spread {
    /// Can this attack miss at all? A weapon whose AIMED cone is zero cannot —
    /// the Torid's grenade is `0 / 0` and its page says "Pinpoint accuracy" in
    /// words, and every sniper in the roster is `0 / 15`: the first shot goes
    /// exactly where the reticle is, which is what a sniper IS.
    pub fn is_pinpoint(&self) -> bool {
        self.min_deg <= 0.0
    }

    /// The deviation ONE pellet drew, in degrees, from a uniform `u` in [0,1).
    ///
    /// UNIFORM INSIDE THE AIMED CONE, i.e. `[0, min_deg)`. Two readings of the
    /// wiki decide that and both are quoted at [`crate::data::weapons::
    /// SpreadSpec`]: spread is *"an angle in degrees from the reticle"*, so the
    /// stat is the cone's RADIUS and a shot lands somewhere inside it rather
    /// than on its rim; and `min` is named *"Deviation With Aim"*, which is the
    /// state this arena is permanently in (the rulers pin `aiming: true`).
    ///
    /// SO `max_deg` IS CARRIED AND NOT CONSUMED. It is where SUSTAINED FIRE
    /// takes the cone — *"the faster a weapon fires, the larger the size of the
    /// 'cone'"* — and the ramp between the two is published nowhere, so
    /// modelling it would mean inventing a bloom rate for 224 entries. What
    /// that costs is stated rather than hidden: a weapon held on the trigger
    /// is more accurate here than in game, most visibly on the ones whose
    /// window is widest (every sniper is `0 / 15`). docs/UNMODELLED.md §2.
    ///
    /// Drawing across `[min, max]` instead was tried first and is refutable
    /// from the data: it makes a Rubico — pinpoint on its first shot, in a
    /// weapon class defined by that — miss about half of them.
    pub fn draw(&self, u: f64) -> f64 {
        self.min_deg * u
    }
}

/// DIRECT-hit damage falloff, resolved: full damage inside `start_m`, decaying
/// linearly to `keep` of it at `end_m` and flat beyond.
///
/// `keep` is the fraction KEPT, and it is the COMPLEMENT of DE's `Reduction`,
/// which is the fraction removed: Hek's 0.8 is the page's *"100% to 20%"*. The
/// data states DE's number, as every radial's `falloff_reduction` does, and this
/// is the one place it is turned round; see [`crate::model::FalloffSpec`].
///
/// THE WINDOW IS SCALED BY PROJECTILE SPEED, which is the first thing that
/// bucket has ever been worth. Wiki (`Projectile Speed`), verbatim: *"Mods
/// including Rivens that have positive or negative Projectile speeds will
/// affect a weapon's entire Damage Falloff range accordingly"* — and, from the
/// other side, *"Hitscan weapons that do not list Damage Falloff values in
/// their UI are completely unaffected by Projectile Speed modifications"*. So a
/// weapon without a falloff takes nothing from the stat, which is exactly this
/// struct being `None`.
#[derive(Debug, Clone, Copy)]
pub struct Falloff {
    pub start_m: f64,
    pub end_m: f64,
    /// Fraction of damage KEPT at `end_m` and beyond.
    pub keep: f64,
}

impl Falloff {
    /// The multiplier on a direct hit that travelled `d` metres.
    pub fn factor(&self, d: f64) -> f64 {
        if d <= self.start_m {
            return 1.0;
        }
        if d >= self.end_m || self.end_m <= self.start_m {
            return self.keep;
        }
        let t = (d - self.start_m) / (self.end_m - self.start_m);
        1.0 - (1.0 - self.keep) * t
    }
}

/// A BOUNCING PROJECTILE'S GEOMETRY — shape, not a damage part, exactly like
/// [`BeamGeometry`]. What a bounce DEALS is the attack's own collision and the
/// attack's own radial arriving again; this says only how many and how far.
#[derive(Debug, Clone, Copy)]
pub struct Ricochet {
    /// Bounces after the first collision — one on the Drakgoon, whose page
    /// gives *"Shots bounce once"*.
    pub bounces: u32,
    /// The chance a bounce lands on a head. A bounce is not aimed, so the
    /// scenario's `headshot_pct` does not decide it.
    pub headshot_chance: f64,
    /// How far a bounce may reach. `f64::INFINITY` when the data states none —
    /// the count is then the only limit, which is the only limit the page has.
    pub range_m: f64,
}

/// A continuous beam's GEOMETRY — shape, not a damage part. Carried so
/// Firestorm has a radius to scale and the multi-target model has its inputs;
/// the single-target arena reads none of it.
#[derive(Debug, Clone, Copy)]
pub struct BeamGeometry {
    pub range_m: f64,
    /// The impact sphere. Firestorm (Primed) enlarges it.
    pub damage_radius_m: f64,
    /// The sphere does NOT take multishot; only the direct target does.
    pub radius_takes_multishot: bool,
    pub chain_hops: u32,
    pub chain_range_m: f64,
    /// Each hop deals this fraction of the hop before it — or of the MAIN beam
    /// when `chain_compounds` is false, which is the Kuva Nukor's shape.
    pub chain_damage_per_hop: f64,
    pub chain_compounds: bool,
    pub chain_takes_multishot: bool,
    /// Does every chain NODE carry a sphere too? UNVERIFIED (MEASUREMENTS
    /// M15) — one line of weapon data so a measurement flips it.
    pub chain_nodes_have_radius: bool,
    /// BEAMS THAT AIM THEMSELVES — `rules::chain::Acquire`. 1 is an ordinary weapon
    /// and the aimed body is one of the count, never extra to it.
    pub beams_count: u32,
    pub beams_acquire_deg: f64,
    pub beams_range_m: f64,
}

/// The Incarnon form's charge economy, for the panel's stat display (see
/// [`WeaponBase::incarnon`]). All times are UNMODDED bases.
#[derive(Debug, Clone, Copy)]
pub struct GaugeForm {
    /// Fixed charge capacity ("Max Charges") — magazine mods are inert.
    pub max_charges: f64,
    /// WHAT fills the gauge. Not cosmetic: the Zariman pistols count weak-point
    /// hits, the Torid counts plain direct hits — "Angstrum Incarnon Genesis
    /// and Torid Incarnon Genesis are instead charged through direct hits"
    /// (wiki Incarnon). Either way it is PER PELLET: "Individual Multishot
    /// bullets can build charges."
    pub charge_on: ChargeOn,
    /// Hits of `charge_on` needed to fill the gauge, UNMODIFIED by evolutions
    /// (Dual Toxocyst 9, Laetum 12, Torid 5). `charge_rate` below shortens it.
    pub charges_to_fill: f64,
    /// Transmute IN (enter the form) = the base form's reload time.
    pub transmute_in: f64,
    /// Transmute OUT (revert to the base form; officially unnamed) — an
    /// estimate, also shortened by reload-speed bonuses.
    pub transmute_out: f64,
    /// Extra gauge fill rate from evolutions (Incarnon Efficiency: +0.5).
    /// Weakpoint hits build `1 + charge_rate` times the charge, so the
    /// hits needed to fill the gauge divide by that factor.
    pub charge_rate: f64,
}

/// Which hits build an Incarnon gauge (weapon data, never assumed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChargeOn {
    /// Weak-point hits only — the Zariman weapons (Dual Toxocyst, Laetum).
    /// A radial/field instance can never contribute: it has no hit location.
    #[default]
    WeakpointHits,
    /// Any direct hit on an enemy (Torid: 5 fill it). Its Toxin cloud is NOT a
    /// direct hit and does not charge — the wiki says so outright ("Torid's
    /// poison cloud does not build charges") and a 37.0 patch note fixed the
    /// case where it did.
    DirectHits,
    /// Kills, not hits — the Mausolon and the Cortege, whose alt-fire is a
    /// thing you EARN rather than hold: "Getting 5 kills with the Mausolon's
    /// primary fire will unlock an Alternate Fire", and after firing it
    /// "additional kills are needed to recharge the laser" (wiki Mausolon;
    /// the Cortege page carries the same sentence).
    ///
    /// This is the first gauge whose source is not a hit, and it is why the
    /// count is a MARK rather than a per-shot delta: a hit lands inside the
    /// shot that caused it, a kill can land on a status tick between two.
    Kills,
}

/// WHERE AN EXPLOSION GOES OFF, which is the difference between a weapon that
/// may be given punch through and one that may not.
///
/// The owner named the problem: a Burston Prime Incarnon carries a
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
    /// past it and worse. `rules::space::dissipation_point` is the geometry.
    Terminal,
    /// A GROUND SLAM: the sphere is centred on the WIELDER'S OWN FEET, not on
    /// anything the attack touched.
    ///
    /// Its own kind rather than a flag because it answers the same question the
    /// other two do — where is the epicentre — and answering it with a third
    /// variant is what keeps `detonation` a total function of the kind. Nothing
    /// is aimed at, nothing flies, and nothing has to be hit for it to go off,
    /// which is also why a slam is the one melee mode that works at any range.
    Slam,
}

/// ONE SWING OF A STANCE COMBO.
///
/// A stance publishes a SEQUENCE — Crushing Ruin's neutral combo is
/// `400% -> 200% -> 300% -> 500% -> 100%` — and every gun in this roster fires
/// one identical shot on a loop, so this is the first attack in the file whose
/// damage and cadence both move from swing to swing.
///
/// TWO NUMBERS THE WIKI PUBLISHES, AND A THIRD IT IMPLIES. It gives the
/// per-swing multiplier and a per-combo "average damage per second", so the
/// combo's own DURATION is `sum(multipliers) / average` — 1500% at 466.7%/s is
/// 3.214 s for Raging Whirlwind. That is derived rather than guessed, which is
/// the only reason this can be transcribed at all: nothing published states a
/// swing's animation length directly.
#[derive(Debug, Clone, Deserialize)]
pub struct ComboHit {
    /// The stance damage multiplier, as a FRACTION (400% -> 4.0).
    ///
    /// DAMAGE ONLY. What the swing earns the counter is `combo_points`, a
    /// separate number the game sets per attack.
    pub multiplier: f64,
    /// Seconds from this swing to the next, at 1.0x attack speed.
    ///
    /// The ANIMATION, and attack speed shortens it. It does NOT include the
    /// wind-up below, which is a separate clock scaled by a separate bucket.
    pub delay_seconds: f64,
    /// THE CHARGE BEFORE A HEAVY SWING, in seconds at 1.0x wind-up speed.
    ///
    /// A SEPARATE CLOCK, because DE says so: *"Increasing melee attack speed
    /// does not reduce the wind-up time; rather, it reduces the interval
    /// between heavy attacks"* (wiki, Melee). Four cards move it and none of
    /// them moves attack speed — Killing Blow, Amalgam Organ Shatter, and two
    /// of the Magistar's own evolutions — so a model with one delay could not
    /// pay any of them without also shortening every light swing.
    ///
    /// Zero on every light swing and on every gun.
    #[serde(default)]
    pub windup_seconds: f64,
    /// Does it reach every body in range rather than the one in front?
    ///
    /// `Types = { "360" }` in the wiki's own `Module:Stances/data`. It is a
    /// SPATIAL fact and the only thing that makes a combo mode worth anything
    /// in a crowd, since Follow Through walks a line and this does not.
    ///
    /// THE MODULE HAS FIVE TYPES AND THIS MODELS TWO OF THEM. `"360"` is here;
    /// `"Sweep"`, `"Thrust"` and the empty string all become the forward
    /// 90-degree arc (`fight::MELEE_ARC_DEG`), which is wide for a thrust and
    /// narrow for a sweep; `"Ranged"` and `"Slam"` are different mechanics and
    /// have their own fields. Declared on every melee entry.
    #[serde(default)]
    pub all_around: bool,
    /// HOW MANY TIMES THIS SWING LANDS.
    ///
    /// `Hits = { 1, 2 }` in the module: Crushing Ruin's forward combo lands its
    /// second 100% TWICE, and Shattered Village lands two 50% spins per attack.
    /// Each is a separate instance — its own crit roll, its own status roll,
    /// its own combo point — which is why this is a count rather than a
    /// multiplier on the damage.
    #[serde(default = "one_hit")]
    pub hits: u32,
    /// A BONUS TO THE IMPACT COMPONENT of this swing alone.
    ///
    /// `ImpactMultiplier = { 1.5 }` in the module — Crushing Ruin marks three
    /// of its swings — and it is a different thing from a forced Knockback
    /// proc, which several of the same swings ALSO carry. On a Magistar (168 of
    /// 210 Impact) a 1.5 takes the swing to 1.4x overall.
    ///
    /// IMPACT NEVER COMBINES, so scaling the finished vector's Impact component
    /// is exact rather than an approximation: no elemental hierarchy can have
    /// consumed it on the way.
    #[serde(default = "one")]
    pub impact_multiplier: f64,
    /// A BONUS TO THE SLASH COMPONENT of this swing alone.
    ///
    /// `SlashMultiplier = { 1.25 }` in the module — Sovereign Outcast marks one
    /// swing of Villain Rule — and it is the same mechanism as the Impact bonus
    /// above on the other physical type. Exact for the same reason: Slash never
    /// combines into an element either, so nothing can have consumed it on the
    /// way.
    #[serde(default = "one")]
    pub slash_multiplier: f64,
    /// …AND THE SLAM SOME COMBOS END ON, as a multiple of the weapon's own.
    ///
    /// `Types = { "", "Slam" }` with `Dmg = { 500, 100 }`: the last attack of
    /// three of Crushing Ruin's four combos is a swing AND a slam, and the slam
    /// is the only thing a combo mode has that reaches past the weapon's reach.
    /// `None` on an ordinary swing.
    #[serde(default)]
    pub slam_multiplier: Option<f64>,
    /// WHAT THIS SWING APPLIES WHATEVER THE ROLL SAYS.
    ///
    /// ONE LIST FOR TWO MECHANISMS, because the stance table is one column:
    /// Crushing Ruin marks its first swing forced Impact and its last forced
    /// Knockdown, and a reader transcribing the table should not have to know
    /// that one of those is a DAMAGE TYPE competing for the proc roll and the
    /// other is an INDEPENDENT proc that never does. The split is made where
    /// the swing lands (`ComboHit::split_forced`), which is the one place that
    /// has both machines in front of it.
    #[serde(default)]
    pub forced_procs: Vec<String>,
    /// HOW MANY OF `hits` CARRY `forced_procs`, when not all of them do.
    /// `Procs = { "Slash", "", "Slash", "", "Stagger" }` on Hysteria's heavy is
    /// two of five; the row lands at one moment, so which two is no question.
    #[serde(default)]
    pub forced_hits: Option<u32>,
    /// COMBO POINTS ONE INSTANCE OF THIS SWING EARNS — its own number, set beside
    /// its damage and REQUIRED, because the game sets it per attack: Hysteria's
    /// follow no rule of the multiplier (MEASUREMENTS M95). An unmeasured row is
    /// filled from the wiki's rule (see notes: combo_points_from_multiplier).
    pub combo_points: f64,
    /// HOW MANY OF THOSE POINTS ARE BASE POINTS — the unit every combo chance
    /// acts on (MEASUREMENTS M97). An ordinary weapon's stance: all of them. An
    /// Exalted stance: one per hit, the rest riding along. Zero on a row of a
    /// form that spends the counter, which earns nothing.
    pub combo_points_base: f64,
}

fn one_hit() -> u32 {
    1
}

impl ComboHit {
    /// The two kinds of forced proc this swing carries, told apart by name.
    ///
    /// A name the engine does not know is a LOUD failure rather than a silent
    /// drop: a stance table transcribed with a typo would otherwise ship a
    /// swing that forces nothing and reads as merely weak.
    pub fn split_forced(&self) -> (Vec<crate::rules::damage::DamageType>, Vec<&'static str>) {
        let mut types = Vec::new();
        let mut independent = Vec::new();
        for p in &self.forced_procs {
            if let Some(ty) = crate::rules::damage::DamageType::from_name(p) {
                types.push(ty);
            } else {
                match p.as_str() {
                    "lifted" => independent.push("lifted"),
                    "knockdown" => independent.push("knockdown"),
                    other => panic!(
                        "combo swing forces `{other}`, which is neither a damage type nor an                          independent proc the engine implements (`lifted`, `knockdown`)"
                    ),
                }
            }
        }
        (types, independent)
    }
}

/// A continuous weapon's damage RAMP — wiki Continuous_Weapon, verbatim:
/// "Initial damage starts at a lower percentage, and ramps up to 100% of its
/// damage over 0.6 seconds of hitting a target. 0.8 seconds after the weapon
/// stops hitting a target, the damage decays back to its initial point over 2
/// seconds. For most weapons, this lower percentage is 20%."
///
/// The floor is PER WEAPON — the same page lists exceptions (Convectrix 60/80%,
/// Phage 70%, Embolist 30%), and the roster now has one: Phantasma Prime ramps
/// "from 15% to 100%", not from 20%. So this constant is the DEFAULT the
/// sentence gives ("for most weapons"), and a weapon that disagrees says so in
/// its own file (`beam_ramp_floor`).
pub const BEAM_RAMP_FLOOR: f64 = 0.20;
