// SPDX-License-Identifier: AGPL-3.0-or-later
//! WHAT A WEAPON IS before any card touches it: its base stats, its forms and
//! cadences, and the per-weapon mechanics a yaml can declare.

use super::*;
use crate::rules::damage::{DamageType, DamageVector};
use serde::Deserialize;

/// Direct-hit damage falloff: full damage inside `start_m`, decreasing
/// linearly until `reduction` of it is GONE at `end_m`, and flat beyond.
///
/// `reduction` is DE's own field, `Module:Weapons/data`'s `Reduction`, and it
/// is the fraction REMOVED — the same reading as [`RadialSpec::falloff_reduction`].
/// Hek's is 0.8 and its page says *"from 100% to 20% from 10m to 20m"* — read
/// as the share kept, it would keep 80% where the game keeps 20%.
/// `build::loadout::Falloff::keep` is its complement.
#[derive(Debug, Clone, Deserialize)]
pub struct FalloffSpec {
    /// Metres out to which damage is full.
    pub start_m: f64,
    /// Metres past which damage stops dropping.
    pub end_m: f64,
    /// Fraction of damage REMOVED at `end_m` and beyond.
    pub reduction: f64,
}

/// THE CONE AN ATTACK FIRES INTO — degrees from the reticle, per ATTACK.
///
/// **THE PRIMARY VALUE, and `accuracy` is the derived one.** The Arsenal's
/// Accuracy is `100 / average spread in degrees`; the thing the game has is
/// this cone — *"spread is internally represented as an angle in degrees from
/// the reticle"*, with a minimum (**Deviation With Aim**) and a maximum (**Max
/// Deviation**) per weapon (wiki `Accuracy` §Spread).
///
/// Deriving the cone back out of the scalar loses the min/max and the FORM:
/// `Module:Weapons/data` carries these per ATTACK, so the Torid's grenade is
/// `0 / 0` while its Incarnon beam is `1.0 / 1.5`. Transcribed by
/// `scripts/intake_spread.py`, which refuses any attack it cannot identify by
/// an exact multi-field match.
///
/// **WHAT IS NOT MODELLED IS THE BLOOM.** The min is the FIRST SHOT and the
/// max is where sustained fire takes it — *"the faster a weapon fires, the
/// larger the size of the 'cone'"* — and the ramp is published nowhere. A
/// pellet draws uniformly across the window instead, which has the published
/// average (`(min + max) / 2`) and invents no rate.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct SpreadSpec {
    /// Deviation With Aim — the first shot's cone.
    pub min_deg: f64,
    /// Max Deviation — where sustained fire takes it.
    pub max_deg: f64,
}

/// A TOME'S RECHARGE METER — a clock you spend, and the third way this roster
/// gates a form.
///
/// The other two are a MAGAZINE and an INCARNON GAUGE. This is neither:
/// *"Requires a fully filled meter beneath the reticle in order to fire. The
/// meter takes 45 seconds to completely recharge. Hitting enemies with the
/// primary fire reduces recharge time by 1 second per hit"* (wiki `Grimoire`).
///
/// SO IT IS A COUNTDOWN, modelled as a gauge filling at one unit a second
/// because a gauge is what the engine speaks; the FIELD NAMES stay in the
/// page's own units. ITS OWN TYPE, DELIBERATELY — it could be bent onto
/// [`GaugeSpec`], and the mechanics differ in every particular: weak-point hits
/// against seconds, a magazine against a single throw, emptied by firing
/// against by one shot.
///
/// WHAT THE HITS ARE: *"Multishot will count as an additional hit"* and
/// *"Radial damage does not count an additional hit"*, so it is one second per
/// landing PELLET and the explosion adds nothing — both distinctions the engine
/// already makes, which is why neither needs a field.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct MeterSpec {
    /// How long it takes to refill with nothing else happening.
    pub seconds_to_fill: f64,
    /// What one landing pellet of the OTHER form takes off it.
    pub seconds_per_hit: f64,
    /// What one ammo pickup takes off it. *"Picking up secondary or universal
    /// ammo"* — a PRIMARY pickup does nothing, which is why the drop model
    /// behind it has to know which kind fell (`rules::ammo::SECONDARY_PICKUP_ON_KILL`).
    pub seconds_per_ammo_pickup: f64,
}

/// A DEPLOYED ORB — an entity with a POSITION, a clock and a reach, none of
/// which this engine had before it.
///
/// IT IS NOT A FIELD: a [`LingeringSpec`] is an AREA and everyone in it burns,
/// while an orb strikes exactly ONE body inside its reach — *"Orb will shock 1
/// enemy within 6 meters of it every 1 second"* (wiki `Grimoire`) — and MOVES
/// between them. Not a projectile either: it lives out a fuse striking as it
/// drifts, so its attack settles no collision and no explosion at the impact.
///
/// WHAT THE ORB DEALS IS THE ATTACK'S OWN — its strike is the attack's
/// `damage`, crit, status and forced procs, its detonation the attack's
/// `radial` moved to where the orb was. This block is GEOMETRY AND CLOCK.
///
/// THE STRIKE CLOCK RUNS FROM THE THROW rather than from a contact, which
/// reproduces the measured count: six ticks over a six second fuse, a tick with
/// nobody in reach spent on nobody. A throw taking 2.5 s lands four strikes and
/// one at contact always six — `ceil(6 - flight)` out of the geometry (M63).
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
    /// blast-radius bucket.
    pub strike_radius_m: f64,
    /// How fast it leaves the muzzle, and what it slows to once it has touched
    /// a body — 6 m/s then 2 m/s.
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
    /// BODIES ONE STRIKE REACHES PER POINT OF MULTISHOT — the struck one
    /// included. The whole count is `floor(this x multishot)`.
    ///
    /// ✅ MEASURED, by counting Invocation stacks: those
    /// four mods gain a stack per hit, so a strike's body count is readable off
    /// the buff rather than guessed at. Six points, and the formula reproduces
    /// every one:
    ///
    /// ```text
    ///   multishot   1.0   1.6   2.1   2.7   3.6   3.9
    ///   measured      3     4     6     8    10    11
    ///   3 x ms      3.0   4.8   6.3   8.1  10.8  11.7   -> floor
    /// ```
    ///
    /// A FLOOR, not a coin. The count was `multishot + 2` for an afternoon,
    /// read off *"chains to an additional 2 enemies"* plus *"Number of chains
    /// is affected by Multishot"* with the remainder rolled — and the wiki's
    /// two sentences are consistent with both readings at x1.0, which is
    /// exactly where they agree and nowhere else: at x2.1 the sum gives 5 and
    /// the product gives 6. The measurement separates them.
    pub chain_bodies_per_multishot: f64,
    /// How far a chain hop may reach, body to body — and it is the one
    /// distance on this attack that a RANGE MOD does not move.
    ///
    /// The orb's reach and its detonation radius both take the blast-radius
    /// bucket; the jump between two bodies stays at what the page gives it. So the two sixes below are the same number by
    /// coincidence rather than by construction, and only one of them grows.
    pub chain_range_m: f64,
    /// THE THROW ANIMATION, in seconds, before the orb leaves — a wind-up like
    /// any thrown weapon's.
    ///
    /// BOTH HALVES ARE SHORTENED BY FIRE RATE, which is why they are here and
    /// not a constant: a fire-rate mod speeds the throw up exactly as it speeds
    /// a trigger pull up.
    pub throw_seconds: f64,
    /// …and the RECOVERY after it, before the weapon can do anything else
    /// (0.85 s). Together they are the second a throw costs, which on
    /// this weapon is also its listed fire rate of 1 — the animation IS the
    /// cadence.
    ///
    /// It is what a CYCLE pays: the primary fire stops for this long every time
    /// an orb goes out, and that is the only price the cycle has beyond the
    /// meter itself.
    pub recovery_seconds: f64,
    /// What a hop deals relative to the hop before it.
    ///
    /// 1.0 — UNDILUTED — for the Grimoire, which chains the way a beam chain with
    /// no falloff does. It is also what the page supports on
    /// its own (it names a count and no reduction). Per entry rather than a
    /// constant, because a chain's falloff is per weapon everywhere else in
    /// this roster — the Atomos compounds at 0.75 and the Kuva Nukor does not
    /// compound at all.
    #[serde(default = "one")]
    pub chain_damage_per_hop: f64,
}

/// THE CLASS'S HEAVY ATTACK, as the wiki's per-weapon-type table states it.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct HeavyAttack {
    /// The class multiplier — 6.0 for a Hammer, 5.0 for a Sword.
    pub multiplier: f64,
    /// The charge before it, in seconds at 1.0x wind-up speed. Attack speed
    /// does not shorten it (wiki, Melee).
    pub windup_seconds: f64,
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

/// The CLOSED vocabulary of attack FORMS. Weapons are operated differently one
/// from the next, but the handful of MODES they are operated in is shared —
/// so a form is a kind from this list, and every weapon entry REGISTERS which
/// one it is (`form:` in its yaml). Nothing may name a form the engine does
/// not know: an unknown string is a hard error, not a silent fallback.
///
/// Adding a kind is one arm here plus one in [`FormKind::parse`] — the whole
/// extension point — and a kind is added when an entry registers it, never in
/// advance: a kind nothing registers is a kind nothing tests.
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
    /// Its own kind rather than [`FormKind::Charged`]: a thrown spear is not a
    /// drawn bow, the id is what a saved preset and a share link carry, and a
    /// wrong word there is wrong forever.
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
    // ---- MELEE. Seven ways to swing one weapon, and each of them is a
    // BUILD --------------------------------------------
    //
    // A melee player picks one loop and runs it for the whole fight, which is
    // the definition of a MODE in this file — so these are forms for exactly
    // the reason a Kuva Hind's three triggers are, and they reach the board as
    // seven independent rows for exactly the reason its three do.
    //
    // THE FIRST FOUR ARE THE STANCE'S GROUND COMBOS, named for their INPUT
    // rather than for the combo. A stance names them itself — Crushing Ruin
    // calls the neutral one "Raging Whirlwind" — so a name here would be one
    // stance's name baked into a durable id that every saved preset and share
    // link carries. The input is what does not change.
    /// Stationary combo (melee alone). The arsenal's own, so it is the
    /// weapon's default form and its mode is `base`.
    Neutral,
    /// Forward + melee.
    Forward,
    /// Block + melee.
    Block,
    /// Block + forward + melee.
    BlockForward,
    /// Nothing but heavy attacks. Its own mode because it is its own build:
    /// the combo counter is SPENT rather than accumulated, so Blood Rush and
    /// Weeping Wounds are worth nothing in it and initial combo is worth
    /// everything.
    Heavy,
    /// Nothing but slide attacks.
    Slide,
    /// Nothing but heavy slams — the one melee mode that needs no stance at
    /// all, because a slam is the same attack whatever is in the slot.
    HeavySlam,
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
            FormKind::Neutral => "neutral",
            FormKind::Forward => "forward",
            FormKind::Block => "block",
            FormKind::BlockForward => "block_forward",
            FormKind::Heavy => "heavy",
            FormKind::Slide => "slide",
            FormKind::HeavySlam => "heavy_slam",
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
            // THE LABEL IS THE FALLBACK, not the name a reader sees. A stance
            // names its own combos and `/api/meta` fills that in; this is what
            // is drawn when no stance is equipped, where the input IS the only
            // true thing to say.
            FormKind::Neutral => "Neutral Combo",
            FormKind::Forward => "Forward Combo",
            FormKind::Block => "Block Combo",
            FormKind::BlockForward => "Block Forward Combo",
            FormKind::Heavy => "Heavy Attack",
            FormKind::Slide => "Slide Attack",
            FormKind::HeavySlam => "Heavy Slam",
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
    /// arrived: its alt-fire is bought with five kills and
    /// is a gauge-fed form of an ordinary Arch-Gun, with no adapter anywhere.
    /// "Does entering this cost a meter" is [`WeaponSpec::has_gauge`], which
    /// reads what the weapon DECLARES instead of inferring it from a name.
    pub fn is_adapter_form(self) -> bool {
        matches!(self, FormKind::Incarnon)
    }

    /// Is this one of the seven ways to swing a melee weapon?
    ///
    /// Asked where a mode id is chosen and where a stance's own combo name is
    /// filled in. It is a property of the KIND rather than of the weapon's
    /// slot because that is what the two call sites have in hand.
    pub fn is_melee(self) -> bool {
        matches!(
            self,
            FormKind::Neutral
                | FormKind::Forward
                | FormKind::Block
                | FormKind::BlockForward
                | FormKind::Heavy
                | FormKind::Slide
                | FormKind::HeavySlam
        )
    }

    /// Does this form SPEND the combo counter to swing?
    ///
    /// The two heavy kinds do, and it is what makes them different builds
    /// rather than different animations: a heavy loop reads the combo counter
    /// as a multiplier and empties it, so Blood Rush — which reads the same
    /// counter as a crit bracket — is worth nothing there.
    pub fn is_heavy(self) -> bool {
        matches!(self, FormKind::Heavy | FormKind::HeavySlam)
    }

    pub fn parse(s: &str) -> FormKind {
        match s {
            "base" => FormKind::Base,
            "charged" => FormKind::Charged,
            "incarnon" => FormKind::Incarnon,
            "alt_fire" => FormKind::AltFire,
            "semi_auto" => FormKind::SemiAuto,
            "auto" => FormKind::Auto,
            "neutral" => FormKind::Neutral,
            "forward" => FormKind::Forward,
            "block" => FormKind::Block,
            "block_forward" => FormKind::BlockForward,
            "heavy" => FormKind::Heavy,
            "slide" => FormKind::Slide,
            "heavy_slam" => FormKind::HeavySlam,
            other => panic!("unknown form kind in weapon data: {other}"),
        }
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
pub struct SpawnOnKillSpec {
    /// How long one of them stands, seconds.
    pub seconds: f64,
    /// How far from the wielder a kill still leaves one, metres.
    pub range_m: f64,
}

/// WEAK-POINT STACKS — the Knell family's "Death Knell".
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct WeakpointStacksSpec {
    pub max_stacks: u32,
    /// ONE CLOCK, from the last weak-point hit: when it falls due one stack is
    /// lost and it restarts for the rest — the Galvanized family's decay.
    pub duration_seconds: f64,
    /// Per stack, added to the FINISHED multiplier — the page writes the
    /// bracket itself: `2 x (1 + Crit Damage Mods) + 0.5 x Number of Stacks`.
    #[serde(default)]
    pub crit_multiplier: f64,
    /// Per stack, added to the FINISHED status chance.
    #[serde(default)]
    pub status_chance: f64,
    /// While ANY stack is up. 1.0 is the round being free.
    #[serde(default)]
    pub ammo_efficiency: f64,
}

/// A status-triggered crit-chance lock — Gotva Prime's.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct SuperCritSpec {
    /// Per pellet that applied at least one status. Several statuses on one
    /// hit do not raise it (wiki).
    pub chance: f64,
    /// The chance the next landing pellet uses, verbatim. 3.0 = 300%.
    pub crit_chance: f64,
}

/// A BURST trigger: one pull fires `count` rounds `delay_seconds` apart, and
/// the weapon's listed `fire_rate` is BURSTS per second, not rounds.
///
/// VERBATIM (wiki Fire Rate): *"Effective Fire Rate = Burst Count / [1/Fire
/// Rate + [(Burst Count−1)⋅Burst Delay]]"*, *"Fire Rate bonuses affect both the
/// speed of the burst as well as the time between bursts"*, and *"Burst Delay
/// is not affected by net negative Fire Rate bonuses."* The second makes this
/// cheap — a bonus scales BOTH terms, so a positive-bonus build is an auto
/// weapon at the effective rate — and the third is where burst stops being a
/// relabelling (`build::loadout::resolve`).
///
/// PRIMARY COMPRESSION's per-weapon row — see docs/CATALOGS.md §2.
///
/// The arcane trades explosion RADIUS for damage, so its worth is a property
/// of the WEAPON and the wiki publishes one row per weapon ATTACK, two of whose
/// columns cannot be derived from anything else the weapon knows:
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
/// The Shedu's battery, and the roster's first: *"ammo regenerates over time.
/// Has a 1 second delay before ammo begins to regenerate; if there are still
/// rounds left, the delay is 0.4 seconds instead. Ammo regenerates at 28 rounds
/// per second"* (wiki Shedu).
///
/// The listed "Reload Time" is therefore not a reload — it is
/// `delay + magazine/rate`, and both published numbers fall out of it: 1.25 s
/// is the wiki's empty battery, 0.65 s WFCD's partial one.
///
/// WHAT IT CHANGES that a plain reload does not: the battery refills BETWEEN
/// SHOTS, for the part of the gap exceeding the delay, so it breaks even at
///
///   `1 / fire_rate  >=  delay_partial + ammo_cost / regen_per_second`
///
/// — on the Shedu **2.295 rounds a second**, 8.2% below its listed 2.50. Below
/// that the battery NEVER EMPTIES, so a nine percent fire-rate penalty removes
/// the reload entirely and Vile Precision alone crosses it.
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
/// Both directions, one field — six in the roster and five go UP:
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
/// shots to optimal — reconciling exactly on all five risers, which is what
/// `over_shots` is derived from and why it is not always an integer (the
/// Gorgon's 10.667% IS 0.8/7.5). The climb is LINEAR. Not `beam_ramp_floor`, a
/// continuous weapon's DAMAGE ramp in seconds; the Phantasma has both.
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

/// A KILL STREAK SUMMONS A SECOND GUN — Pyrana Prime: "3 kills each within 2
/// seconds of the previous kill summons a second Pyrana Prime for 6 seconds,
/// doubling its magazine size and increasing its fire rate by 1.4x" (wiki).
/// Kills while it is up do not refresh it; it arrives with a modded magazine's
/// worth of rounds, and "when the ethereal Pyrana disappears, the magazine is
/// reduced to the modded magazine size".
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct KillStreakSummonSpec {
    pub kills: u32,
    pub kill_window_seconds: f64,
    pub duration_seconds: f64,
    pub magazine_multiplier: f64,
    pub fire_rate_multiplier: f64,
}

impl KillStreakSummonSpec {
    /// Its card, its replay row and its entry on the buff bar.
    pub const BUFF_ID: &'static str = "kill_streak_summon";
    /// The streak that buys it: a stack per kill on one clock of
    /// `kill_window_seconds`, which the next kill restarts.
    pub const STREAK_BUFF_ID: &'static str = "kill_streak";
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
/// integer in binary.
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

/// A weapon's unmodded panel (fixed evolutions folded in — they alter the
/// weapon's BASE stats before mods).
#[derive(Debug, Clone)]
pub struct WeaponBase {
    /// WHICH FORM this panel is, from the yaml entry's own `form:`.
    ///
    /// A two-weapons pair is two entries and two panels, and until now neither
    /// knew which of the two it was — every evolution applied to both. Eleven
    /// evolutions say *"Does not affect Incarnon Form"* and this is what lets
    /// them be obeyed rather than transcribed and ignored.
    /// PUNCH-THROUGH DEPTH the WEAPON brings, in metres of material — see
    /// [`crate::data::weapons::AttackSpec::punch_through_m`].
    pub punch_through_m: f64,
    /// [`crate::data::weapons::AttackSpec::projectile_width_m`] — 0 is a ray.
    pub projectile_width_m: f64,
    /// [`crate::data::weapons::AttackSpec::range_m`] — `INFINITY` when the
    /// weapon declares none, which is what every weapon did before 2026-08-19.
    pub range_m: f64,
    /// Whether this attack takes punch-through MODS — see
    /// [`crate::data::weapons::AttackSpec::punch_through_mods`]. `None` means
    /// the class rule decides.
    pub punch_through_mods: Option<bool>,
    pub form: crate::model::FormKind,
    /// Indirect stats the WEAPON itself brings, before any mod — today only
    /// EVOLUTIONS write here (Practiced Grip's +50% accuracy, Marksman's
    /// Hand's recoil, Swift Deliverance's projectile speed). `resolve` seeds
    /// the panel's `indirect` from this and mods add into the same buckets, so
    /// an evolution's handling stat lands exactly where a mod's does.
    pub indirect: Vec<(IndirectStat, f64)>,
    pub base_vector: DamageVector,
    pub base_crit_chance: f64,
    pub base_crit_damage: f64,
    pub base_status_chance: f64,
    pub base_fire_rate: f64,
    /// CHARGE trigger (bows): the draw before the shot, unmodded. `Some` moves
    /// the cadence off `1 / fire_rate` — a charged weapon fires once its draw
    /// completes, and fire-rate bonuses shorten THAT. `base_fire_rate` stays
    /// the listed stat (Cernos Prime: 1.0), which is what reads it as a stat
    /// (Hemorrhage's below-2.5 gate) still sees.
    pub charge_seconds: Option<f64>,
    /// See [`crate::data::weapons::AttackSpec::charge_ammo_per_second`]. Set,
    /// the charge spends the magazine and the damage rides on it.
    pub charge_ammo_per_second: Option<f64>,
    /// See [`crate::model::SustainedFireRate`]. It rides through the mod
    /// layer UNTOUCHED — it is a fraction of whatever rate the build ends up
    /// with, so a fire-rate mod raises the ceiling and the floor together.
    pub sustained_fire_rate: Option<crate::model::SustainedFireRate>,
    /// See [`crate::model::Battery`]. Untouched by the mod layer too:
    /// the regen rate is the weapon's and a magazine mod changes only how many
    /// rounds it has to refill.
    pub battery: Option<crate::model::Battery>,
    /// Ammo spent per shot / per beam tick (weapon data `attack.ammo_cost`).
    pub ammo_cost: f64,
    /// See `data::weapons::WeaponSpec::headshot_bonus_multiplicative`.
    pub headshot_bonus_multiplicative: bool,
    /// Does a fire-rate bonus shorten the DRAW? False for Arch-Guns, whose
    /// fire rate paces only the interval — see `data::weapons`.
    pub fire_rate_shortens_draw: bool,
    /// Which charge formula paces it — see [`crate::model::ChargeCadence`].
    pub charge_cadence: crate::model::ChargeCadence,
    /// A BURST trigger's shape, unmodded — see [`crate::model::BurstSpec`].
    pub burst: Option<crate::model::BurstSpec>,
    /// What a fire-rate MOD's bonus is multiplied by on this weapon — 2.0 for
    /// bows, whose cards all print "(x2 for Bows)". It reaches the mod bucket
    /// only: a mod-granted BUFF (Pressurized Magazine's on-reload fire rate)
    /// carries no such clause, so it is not doubled. UNVERIFIED for buffs; no
    /// bow-eligible fire-rate buff is in the roster to measure it with.
    pub fire_rate_mod_multiplier: f64,
    /// Stored pellet count (wiki Multishot).
    pub base_multishot: f64,
    /// See [`crate::data::weapons::AttackSpec::unaimed_headshot_chance`].
    pub unaimed_headshot_chance: Option<f64>,
    /// See [`crate::data::weapons::AttackSpec::windup_seconds`] — unmodded here;
    /// `resolve` divides it by the fire-rate bucket.
    pub windup_seconds: f64,
    /// See [`crate::data::weapons::AttackSpec::no_magazine`].
    pub no_magazine: bool,
    // ---- MELEE, as the ENTRY states it -----------------------------------
    /// See [`crate::data::weapons::AttackSpec::combo_script`] — the swings a
    /// melee form loops, unmodded. `resolve` shortens the delays by the
    /// attack-speed bucket and nothing else touches them.
    pub combo_script: Vec<crate::model::ComboHit>,
    /// See [`crate::data::weapons::AttackSpec::follow_through`].
    pub follow_through: Option<f64>,
    /// See [`crate::data::weapons::AttackSpec::slam`] — the weapon's own slam,
    /// unmodded, fired by a combo swing that ends on one.
    pub slam: Option<RadialBase>,
    /// See [`crate::data::weapons::AttackSpec::heavy`] — the class's heavy
    /// attack, stated on every melee form because Tennokai turns a LIGHT swing
    /// into one.
    pub heavy: Option<crate::model::HeavyAttack>,
    /// See [`crate::data::weapons::AttackSpec::spends_combo`].
    pub spends_combo: bool,
    /// See [`crate::data::weapons::WeaponSpec::combo_duration_seconds`] —
    /// unmodded; `resolve` adds the Drifting Contact bucket and floors it.
    pub combo_duration_seconds: f64,
    /// See [`crate::model::OrbSpec`] — unmodded; `resolve` scales the
    /// two radii and adds multishot to the body count.
    pub orb: Option<crate::model::OrbSpec>,
    /// See [`crate::model::MeterSpec`] — the clock this form is gated
    /// behind, unmodded.
    pub meter: Option<crate::model::MeterSpec>,
    /// Extra additive multishot from non-mod sources at assumed-max
    /// (Fevered Frenzy's 20 stacks = +1.0).
    /// Flat BASE damage an evolution grants through a PERMANENT buff rather
    /// than unconditionally — Boar Prime's Reified Bane, "On Reload From
    /// Empty: +14 Base Damage". `base_vector` already carries it (the buff
    /// starts full, like Fevered Frenzy's multishot); this records how much of
    /// it is the buff's, so a buff card can take it back off.
    pub reload_damage_buff: f64,
    pub buff_multishot_bonus: f64,
    /// Stack count behind `buff_multishot_bonus` (Fevered Frenzy: 20). The
    /// stacks are PERMANENT (no timer, cleared only by death) and their
    /// trigger (ability cast) cannot fire in the sim — so the count is a
    /// static per-buff CHOICE, full by default. 0 = no such buff.
    pub buff_multishot_max_stacks: u32,
    pub magazine_size: f64,
    /// Base reserve rounds (wiki "Ammo Max"), before mods.
    pub ammo_reserve: f64,
    /// Has this weapon a reserve behind its magazine at all? Derived from
    /// `ammo_max`: false only where the weapon states none, which today is
    /// every sentinel weapon ("Ammo Max: ∞ / Ammo Type: None").
    pub has_reserve: bool,
    /// [`crate::data::weapons::WeaponSpec::ammo_pickup`] — rounds one pickup
    /// gives. No mod in this roster moves it (the Scavenger auras would).
    pub ammo_pickup: f64,
    /// Gotva Prime's passive: a status-triggered crit-chance SET. See
    /// `data::weapons::SuperCritSpec`.
    pub super_crit_on_status: Option<crate::model::SuperCritSpec>,
    /// [`crate::data::weapons::WeaponSpec::weakpoint_stacks`] — no mod moves it.
    pub weakpoint_stacks: Option<crate::model::WeakpointStacksSpec>,
    /// [`crate::data::weapons::WeaponSpec::spawn_on_kill`] — counted, no more.
    pub spawn_on_kill: Option<crate::model::SpawnOnKillSpec>,
    /// [`crate::model::KillStreakSummonSpec`] — no mod moves it.
    pub kill_streak_summon: Option<crate::model::KillStreakSummonSpec>,
    /// Where this weapon's beam ramp starts (0.20 unless it says otherwise).
    pub beam_ramp_floor: f64,
    /// Does this weapon apply MICROWAVE? See `fight::DebuffState::microwave`.
    pub applies_microwave: bool,
    /// See `data::weapons::WeaponSpec::independent_procs`. No mod adds or
    /// removes one — it is what the weapon DOES, not what the build asks for.
    pub independent_procs: &'static [&'static str],
    /// Damage types forced on every DIRECT hit — see
    /// `data::weapons::AttackSpec::forced_procs`.
    pub forced_procs: Vec<DamageType>,
    /// ONE PULL, ONE ELEMENT EACH — see
    /// `data::weapons::AttackSpec::pellet_elements`. Empty on every weapon
    /// whose projectiles share an element, which is all but one of them.
    pub pellet_elements: Vec<DamageType>,
    /// See `data::weapons::AttackSpec::multishot_adds_damage`.
    pub multishot_adds_damage: bool,
    /// See `data::weapons::AttackSpec::attractor_seconds`.
    pub attractor_seconds: Option<f64>,
    /// How many tendrils this weapon can hold up (0 = it has none). See
    /// `data::weapons::TendrilSpec` for why the COUNT is modelled and the
    /// tendrils' own damage is not.
    pub tendril_max: u32,
    /// How far a tendril reaches and how far off the reticle it will take a
    /// body — see [`crate::data::weapons::TendrilSpec`]. Both zero on every
    /// weapon that has no tendrils.
    pub tendril_range_m: f64,
    pub tendril_acquire_deg: f64,
    /// The sniper's Shot Combo Counter, before `resolve` asks whether the
    /// Tenno is aiming — see `data::weapons::SniperCombo`.
    pub sniper_combo: Option<crate::model::SniperCombo>,
    /// ...and the scope's headshot bonus at its top zoom level, likewise
    /// unspent until `resolve` (0.0 = no scope).
    pub scope_headshot_damage: f64,
    /// ...or the scope's CRIT bonuses, for the weapons whose zoom grants those
    /// instead. Spent by `resolve`, like the headshot one, and only while
    /// aiming. See `data::weapons::ScopeSpec`.
    pub scope_crit_chance: f64,
    pub scope_crit_multiplier: f64,
    /// The Lanka's kind — see `data::weapons::ScopeSpec::crit_chance_post_mod`.
    pub scope_crit_chance_post_mod: f64,
    /// ...and can it NOT be refilled mid-fight? See `WeaponSpec::no_resupply`.
    /// Separate from the above on purpose — most weapons have a reserve AND a
    /// way to top it up.
    pub no_resupply: bool,
    pub base_reload: f64,
    /// ROUNDS A SECOND under Pax Charge — a MODULAR weapon's chamber states it,
    /// and it is `None` everywhere else. The arcane is what turns the magazine
    /// into a battery; this is the only part of the mechanic the weapon owns,
    /// because the DELAY is the reload and the reload is the loader's.
    pub recharge_per_second: Option<f64>,
    /// See `data::weapons::WeaponSpec::echo_multiplier` — a MEASURED,
    /// unexplained coefficient on Secondary Irradiate's echo, 1.0 everywhere
    /// but the Laetum's Incarnon form.
    pub echo_multiplier: f64,
    /// A BY-ROUND reload, as `(start, per shell, end)` seconds. `None` = the
    /// ordinary one-block reload. See `WeaponSpec::reload_style`: the whole
    /// point is that the magazine size is IN the reload time, so a magazine
    /// mod on a Strun or a Felarx costs what the game charges for it.
    pub by_round_reload: Option<(f64, f64, f64)>,
    /// Bonuses the PLAYER's stats decide, folded in `resolve_for` for the same
    /// reason `gated` is: they are read off the raw weapon and the Tenno is not
    /// there yet.
    pub tenno_scaled: Vec<TennoScaledTerm>,
    /// Unconditional CO rate baked into the weapon config (Carnage
    /// Reign's +33% per status type) — additive with mod CO sources.
    pub innate_co_per_type: f64,
    /// The same thing, waiting on the PLAYER: an evolution's Condition Overload
    /// that states a sprint speed. It joins `innate_co_per_type` in
    /// `resolve_for`, where the Tenno exists, or contributes nothing.
    /// EVERY GRANT THIS WEAPON MAKES CONDITIONAL ON THE PLAYER, as
    /// `(gate, grant, value)`. Carried rather than spent because `apply` works
    /// on the raw weapon and the Tenno is not there; folded in `resolve_for`,
    /// which has both.
    pub gated: Vec<GatedTerm>,
    /// King's Gambit: a MULTIPLIER on crit chance for a hit that did NOT land on
    /// a weak point. 1.0 = ordinary.
    ///
    /// VERBATIM (Sicarus_Incarnon_Genesis): "x0 Critical Chance on Bodyshots,
    /// +150% Critical Chance on Weakpoint Hits", with the note that settles the
    /// bracket — "Bodyshot modifier is MULTIPLICATIVE with all sources of
    /// Critical Chance, effectively making non-headshot critical hits
    /// impossible". Its other half is additive and already has a home:
    /// "Weakpoint modifier is ADDITIVE with mods such as Pistol Gambit", which
    /// is `weakpoint_crit_chance_relative`.
    pub bodyshot_crit_chance_multiplier: f64,
    /// GALVANIC RELOAD: `(status, chance, rounds)` — "On hitting a target
    /// affected by an Electricity status, 40% chance to restore 1 round in the
    /// magazine from ammo pool".
    ///
    /// ONCE PER SHOT, not per pellet: the card says "The bonus can only apply
    /// once per enemy hit", and this is a shotgun family where the difference is
    /// tenfold. The rounds come FROM THE AMMO POOL, so a dry reserve restores
    /// nothing — and a refill is not a reload, the same rule
    /// `magazine_refill_on_kill` follows.
    pub round_restore_on_status: Option<(crate::rules::damage::DamageType, f64, f64)>,
    /// Exact Penance: the chance a KILL — from anywhere, including a status
    /// kill — reloads instantly. See the ResolvedPanel field for why it is not
    /// `instant_reload_on_headshot`.
    pub instant_reload_on_kill: Option<f64>,
    /// THIS FORM CANNOT AIM DOWN SIGHTS — see
    /// [`crate::data::weapons::WeaponSpec::cannot_zoom`]. `resolve_for` answers
    /// the aim question FALSE for it whatever the scenario says, so every
    /// `aiming` mod, arcane and evolution pays nothing here.
    pub cannot_zoom: bool,
    /// RESONANT RESTORE: `(per stack, max stacks)` — "On Reload From Empty:
    /// Increase Base Magazine Capacity by +N. Stacks up to Nx", in the card's
    /// own units so `resolve` can scale it: the card says BASE capacity, which
    /// is the number a magazine mod multiplies.
    pub magazine_growth_on_empty_reload: Option<(f64, u32)>,
    /// King's Gambit's weak-point half, held on the WEAPON so it can seed the
    /// same bucket the mods write to — "Weakpoint modifier is additive with
    /// mods such as Pistol Gambit". Same shape as `evo_reload_bonus`.
    pub evo_weakpoint_crit_chance_relative: f64,
    /// Double Tap: `(per stack, max stacks, seconds)`. See
    /// [`ModEffect::ConsecutiveHitDamage`].
    pub consecutive_hit_damage: Option<(f64, u32, f64)>,
    /// See [`crate::data::weapons::AttackSpec::consecutive_hit_radial_only`].
    pub consecutive_hit_radial_only: bool,
    /// Wiseman's Regard: `(rate, cap)` — "Increase Base Status Chance by 30% of
    /// current Critical Chance, up to 40%".
    ///
    /// "CURRENT" is the MODDED value, and the wiki's own arithmetic proves it:
    /// the Dera's mirror perk notes that "+366.7% Status Chance is needed to max
    /// out the Critical Chance bonus", and 0.30 x (1 + 3.667) is exactly the
    /// 1.40 that a 35% cap at 25% a point demands. "BASE" is where the grant
    /// LANDS, so the stat's own mods multiply it afterwards.
    pub base_status_from_crit: Option<(f64, f64)>,
    /// High Ground: the mirror — base crit chance from current status chance.
    pub base_crit_from_status: Option<(f64, f64)>,
    /// THE SECOND perk to ask about the player's sprint speed, and the second
    /// grant to be carried rather than spent in `apply`: Deadly Pace's "With
    /// Sprint Speed 1.2 or Higher: +80% Fire Rate".
    ///

    /// Feigned Retreat / Swift Conclusion: a share of the BASE-DAMAGE BUCKET
    /// that applies only while the target is under half health.
    ///
    /// VERBATIM (wiki, Sicarus Incarnon Genesis): *"Bonus damage is additive
    /// with mods such as Hornet Strike but does not take into account the Base
    /// Damage increase from this perk."* Both halves of that sentence are
    /// obeyed: it joins the bucket Serration feeds, and the rate stored here is
    /// already scaled so that the perk's OWN flat base damage is excluded from
    /// what it multiplies — resolved at the end of `apply`, which is the first
    /// moment the evolved base exists.
    pub base_damage_below_half_health: f64,
    /// Vicious Promise: "+40% Base Critical Chance / +2x Base Critical Damage
    /// Multiplier ON UNDAMAGED ENEMIES".
    ///
    /// A condition on the TARGET like the one above, and the first that asks
    /// whether the fight has started rather than how far it has got. Both are
    /// BASE grants, so `resolve` converts each into the post-mod number worth
    /// the same — `flat x (1 + mods)` — for the same reason the flat
    /// base-damage buff does: the panel resolves crit once, and a live grant
    /// has to land in the bracket the card names.
    pub crit_chance_on_undamaged: f64,
    /// The other half of the same card. See above.
    pub crit_damage_on_undamaged: f64,
    /// THE VALENCE FRACTION THIS COPY CAME OUT OF ITS LICH WITH, once applied.
    ///
    /// `apply_valence` spends it on the base vector and the radial and then it
    /// is gone — which is fine while everything it can reach is on the weapon,
    /// and is not once a MOD grants an attack part. Nightwatch Napalm's fire is
    /// the first: it arrives during `resolve_for`, long after the valence was
    /// applied, and its base is "increased by Kuva weapon's bonus damage stat"
    /// (wiki) — measured at 150 to 240 on a 60% roll.
    ///
    /// Zero on every weapon that is not an adversary weapon, and on one whose
    /// element was never named.
    pub valence_bonus: f64,
    /// This weapon's Condition Overload behavior class.
    pub co_behavior: CoBehavior,
    /// CO base effectiveness = `original_base / evolved_base`, i.e. how much of
    /// the CO term the weapon's own evolutions dilute.
    ///
    /// **1.0 on every weapon but Dual Toxocyst.** Including a perk's flat base
    /// THE ORIGINAL BASE — the damage the GunCO term computes on, in the same
    /// units as `base_vector.total()`.
    ///
    /// AN ABSOLUTE, NOT A FRACTION. A ratio recomputed as `original / evolved`
    /// describes the ARITHMETIC of one loadout instead of the FACT underneath —
    /// that a weapon has an original base, and some things add to it while
    /// others only add to what it prints. What it cannot express: TWO SOURCES
    /// THAT DISAGREE (a weapon carrying two flat-damage perks, one feeding the
    /// term and one not, has no single ratio), and A NEW MECHANIC, which under
    /// an absolute simply says whether it feeds this.
    ///
    /// A weapon may DECLARE a starting value below its own base
    /// (`co_base_fraction` in the yaml, 0.5 on a bow's charged entry); that is
    /// the only place a fraction is still written down, because that is how the
    /// catalog prints it.
    pub co_base: f64,
    /// THE PART OF THE BASE AN ATTACK'S OWN MULTIPLIER DOES NOT SCALE, in the
    /// same units as `base_vector.total()` — a flat base-damage add, and
    /// nothing else.
    ///
    /// A stance's combo multiplier, a slam's and a heavy slam's scale the
    /// WEAPON's base and this rides beside it rather than inside it:
    /// `mods x (base x swing + flat)` (MEASUREMENTS M79). An ABSOLUTE for the
    /// reason [`Self::co_base`] is one, and zero on every weapon that carries
    /// no flat add — which is all but the Incarnon Genesis perks.
    pub unswung_base: f64,
    /// Buff-injected elements as RELATIVE bonuses (element, bonus): each
    /// contributes ModifiedBase × bonus at the END of the hierarchy
    /// (rule 8) — Frenzy's +100% Toxin on the base Dual Toxocyst.
    pub injected_elements: Vec<(DamageType, f64)>,
    /// Weapon traits a mod's `requires` is checked against (e.g. `semi_auto`,
    /// `beam`). A mod requiring a trait the weapon lacks is inert.
    pub traits: &'static [&'static str],
    /// THE WEAPON'S CLASS — `rifle`, `pistol`, `shotgun`, `sniper`. A trait
    /// answers "how does it fire"; this answers "what IS it", and the Amp auras
    /// are the first thing to ask: Rifle Amp pays a rifle and nothing else.
    pub class: &'static str,
    /// …AND THE POOLS IT DRAWS, because three of the four amps ask THAT
    /// rather than the class: Rifle Amp "also affects bows, sniper rifles
    /// and launchers", which is the `rifle` pool and not any one class.
    pub mod_pools: &'static [&'static str],
    /// …AND THE EQUIPMENT SLOT, which is narrower than the class and wider than
    /// a pool. It is the gate an ARCHON SHARD names: Crimson's "Primary Status
    /// Chance" pays a bow and a shotgun alike.
    pub slot: &'static str,
    /// Incarnon-form transformation economy. `Some` marks this form's
    /// magazine as CHARGE-BACKED (a fixed "Max Charges" resource fed by the
    /// weakpoint gauge, entirely outside the ammo system): magazine mods and
    /// ammo efficiency are INERT on it. There is no reload; instead two
    /// transition times (transmute in = the base form's reload; transmute
    /// out = the officially-unnamed revert), each scaled by the reload
    /// formula `base / (1 + reload bonus)`. `magazine_size` / `base_reload`
    /// still carry the pseudo-reload (270 / 3.35) the plain sim consumes.
    pub gauge_form: Option<GaugeForm>,
    /// Evolution-granted additive fire rate (Rapid Wrath) — joins the
    /// fire-rate-mod bucket.
    pub evo_fire_rate_bonus: f64,
    // ---- WHAT A MELEE GENESIS GRANTS ------------------------------------
    //
    // Their own channels rather than the mod buckets, for the reason
    // `evo_fire_rate_bonus` has one: an evolution is applied to the WeaponBase
    // before `resolve` runs, and a stat that is locked by an Acuity has to be
    // able to refuse the evolution's share separately from the mods'.
    /// A relative damage bonus from an evolution — the Pressure Point bucket.
    /// `+100% Melee Damage` is the Magistar Incarnon Form's whole first line.
    pub evo_base_damage_bonus: f64,
    /// Points the melee combo counter opens at, from an evolution.
    pub evo_initial_combo: f64,
    /// Combo points per body a slam reached, from an evolution (Shockwave
    /// Synergy). Zero everywhere else.
    pub evo_combo_count_on_slam_hit: f64,
    /// Metres of melee reach, from an evolution (Orokin Reach).
    pub evo_melee_range_m: f64,
    /// THE WINDOW THIS BUILD'S MELEE INCARNON IS ON FOR, when one is installed.
    ///
    /// `None` on every gun and on a melee weapon with no Genesis. A GUN's
    /// Incarnon is not this: it is a second weapon entry with its own attack,
    /// and it ends when its charge magazine does.
    pub melee_incarnon: Option<MeleeIncarnon>,
    /// A relative change to Follow Through, from an evolution.
    pub evo_follow_through_bonus: f64,
    /// A relative change to a slam's radius, from an evolution.
    pub evo_slam_radius_bonus: f64,
    /// A relative change to HEAVY ATTACK WIND UP SPEED, from an evolution
    /// (the Magistar's Incarnon Form is +50% and its Swift Break another +30%).
    pub evo_heavy_windup_speed: f64,
    /// A proc conversion granted by an evolution — `(from, to, chance)`.
    /// See [`ModEffect::ProcConversion`], whose runtime this shares.
    pub evo_proc_conversion: Option<(crate::rules::damage::DamageType, crate::rules::damage::DamageType, f64)>,
    /// Reload-speed bonus from evolutions, into the same bucket the mods feed.
    pub evo_reload_bonus: f64,
    /// READY RETALIATION's window — see
    /// [`crate::model::EvoEffect::ReloadSpeedOnEmptyReload`]. Same
    /// bucket as `evo_reload_bonus`, but only while the window is open.
    /// READY RETALIATION: *"On Reload From Empty: +100% Reload Speed"*, as a
    /// plain bonus rather than a timed buff.
    ///
    /// IT IS SCOPED TO THE RELOAD ACTION — it arrives when the reload starts
    /// and is gone when the reload ends. So it cannot
    /// lapse halfway through, and it cannot spill onto anything that is
    /// not that reload.
    ///
    /// That is also what makes the perk loadable at all on the other eleven
    /// weapons that have it. Only the Phenmor's page publishes a window (6 s),
    /// and the rest state the bonus and nothing else — which read as missing
    /// data while the model was a timer, and reads as "there is nothing to
    /// state" once the window is the action.
    pub rs_on_empty_reload: f64,
    /// FLENSING SPIKES: *"Remove 20% of enemy Armor per Puncture Status"*, as a
    /// fraction per live Weakened stack (0.0 = the weapon does not have it).
    ///
    /// A THIRD ARMOUR-STRIP SOURCE. The engine had two — Corrosive and Heat,
    /// the two the game itself strips with — and this is a weapon PERK doing it
    /// off a status that strips nothing on its own. It multiplies with the
    /// other two the same way they multiply with each other, and at Puncture's
    /// five-stack cap 20% a stack is the whole of the armour.
    pub armor_strip_per_puncture: f64,
    /// EXECUTIONER'S FORTUNE — see [`InstantReload`].
    pub instant_reload_on_headshot: Option<InstantReload>,
    /// LINGERING JUDGEMENT — see [`HeadshotStreak`].
    pub headshot_streak: Option<HeadshotStreak>,
    /// SPITEFUL DEFILEMENT: `(threshold, bonus)` — add `bonus` to the crit
    /// DAMAGE, after mods and flat, while the target carries fewer than
    /// `threshold` distinct status types.
    pub crit_damage_below_status_count: Option<(u32, f64)>,
    /// Prelude of Might: `(bonus, threshold)` — add `bonus` to the crit damage
    /// MULTIPLIER while the resolved crit chance stays under `threshold`.
    /// Resolved late for that reason: it is the only evolution whose condition
    /// reads the panel rather than the fight.
    pub crit_multiplier_below_crit_chance: Option<(f64, f64)>,
    /// FLAT crit/status chance added AFTER mods (Elemental Excess) — a
    /// different layer from the base-stat one `base_crit_chance` carries.
    pub post_mod_crit_chance: f64,
    pub post_mod_status_chance: f64,
    /// Additive headshot-damage bonus (Caput Mortuum), inside the headshot
    /// bracket `(1 + Σ)`. Direct hits only — a radial never headshots.
    pub headshot_damage_bonus: f64,
    /// The WEAPON's own headshot multiplier where it overrules the enemy body
    /// part's — see `data::weapons::WeaponSpec::headshot_multiplier`. A weapon
    /// stat and never a mod's, so it rides the base rather than the buckets.
    pub headshot_multiplier: Option<f64>,
    /// Devouring Attrition: `(chance, bonus)` — on an instance that did
    /// NOT crit, `chance` to multiply it by `(1 + bonus)`. Its own
    /// multiplier, applied to the direct hit and the radial alike.
    pub noncrit_bonus: Option<(f64, f64)>,
    /// Overwhelming Attrition's stacking damage buff.
/// Every stacking buff this weapon grants — see [`StackingBuff`]. A Vec
    /// rather than one field per buff, so the roster, the config reader and the
    /// stack sampler can each walk it instead of naming buffs one at a time.
    pub stacking_buffs: Vec<StackingBuff>,
    /// A RADIAL (AoE) attack part fired alongside the direct hit — the
    /// Laetum Incarnon's 300 Radiation explosion. Separate damage vector,
    /// crit and status stats; the directly-hit enemy takes both parts.
    /// See MECHANICS §7 "Radial (AoE) attack parts" for the rule set.
    pub radial: Option<RadialBase>,
    /// The bomblets this attack's explosion throws out — see [`ClusterBase`].
    pub cluster: Option<ClusterBase>,
    /// THE CONE this attack fires into, as the data states it and before
    /// accuracy mods — see [`crate::model::SpreadSpec`]. `None` = not
    /// transcribed, and the entry admits it.
    pub spread: Option<crate::model::SpreadSpec>,
    /// DIRECT-hit damage falloff as the weapon data states it, unscaled — see
    /// [`Falloff`], which is this after Projectile Speed has moved the window.
    pub falloff: Option<crate::model::FalloffSpec>,
    /// This attack's row in Primary Compression's per-weapon table — see
    /// [`crate::model::CompressionSpec`] and docs/CATALOGS.md §2. `None`
    /// means the weapon has no AoE for the arcane to compress, so it is worth
    /// nothing rather than unknown (the catalog rule).
    pub compression: Option<crate::model::CompressionSpec>,
    /// A LINGERING FIELD left by every landed projectile of this attack — the
    /// Torid's Toxin cloud. Grenades STICK, so a directly-hit enemy takes the
    /// impact AND every tick. MECHANICS §7 "Lingering damage FIELDS".
    pub lingering: Option<LingeringBase>,
    /// CONTINUOUS (beam) weapon — trigger "Held". Two rules change, both wiki:
    /// `fire_rate` is TICKS per second, and multishot beams hitting one target
    /// MERGE into a single instance.
    pub continuous: bool,
    /// Renewed Horror: the multiplier a reload-from-EMPTY applies to the next
    /// shot's field duration (1.0 = the evolution is not installed).
    pub field_duration_on_empty_reload: f64,
    /// Lone Enforcer: `(fraction of base multishot, metres)`, paid only when
    /// the target is standing further away than that. Settled against the arena
    /// in `FightParams::from_panel` — see [`EvoEffect::MultishotBeyondRange`].
    pub multishot_beyond_range: Option<(f64, f64)>,
    /// Continuous-beam geometry, when this form is one.
    pub beam: Option<BeamGeometry>,
    /// Ricochet geometry, when this form's projectile bounces.
    pub ricochet: Option<Ricochet>,
    /// Final Fusillade: a FLAT multishot add on the LAST round of the magazine
    /// (0.0 = not installed). Base form only — the evolution loader drops it on
    /// a charge-backed form, so this is always 0.0 there.
    pub multishot_on_last_round: f64,
    /// The same window, in the OTHER BRACKET: "+5 **Base** Multishot on final
    /// magazine burst", which the wiki notes is "added before mods, and is
    /// thus multiplied by multishot bonuses". So it raises what the weapon's
    /// base pellet count IS for that burst, and every relative bonus — the mod
    /// bucket, a Galvanized stack, an arcane's grant — reads the raised number.
    pub base_multishot_on_last_round: f64,
    /// Plentiful Mayhem: multishot spends ammo, and what it GENERATES deals
    /// +v damage (0.0 = not installed). Both forms carry it; the rule differs
    /// by form and the sim reads that off `continuous`.
    pub multishot_ammo_bonus: f64,
}
