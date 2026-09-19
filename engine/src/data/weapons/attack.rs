use super::*;

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
/// just an `f64`.
///
/// Absence has to keep meaning unlimited — 121 entries have never been
/// transcribed and must go on working — but it now means "nobody has looked"
/// rather than "there is no limit", and the two are separable by a script.
/// `data::weapons`'s own ratchet counts the entries that say NEITHER, so the
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

/// METRES OF MATERIAL, OR THE WORD FOR ANY AMOUNT OF IT.
///
/// `punch_through_m: infinite` is the wiki's own class — *"weapons that shoot
/// wide projectiles or a stream of particles … pierce an unlimited amount of
/// enemies, but not level geometry"* — and it is a STATEMENT where a big
/// number is a guess someone has to re-derive. Held as
/// [`crate::rules::space::INFINITE_BODY_PUNCH_THROUGH_M`] rather than as a true
/// infinity, because a budget that survives every body is spent as FLIGHT
/// (`rules::space::dissipation_point`) and `0.0 * f64::INFINITY` is NaN.
pub(super) fn punch_through_metres<'de, D>(d: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    match serde_norway::Value::deserialize(d)? {
        serde_norway::Value::Number(n) => n.as_f64().ok_or_else(|| D::Error::custom("not a number")),
        // Loud rather than lenient, the rule `RangeSpec` follows: an
        // unrecognised word is a data error, and the alternative is a weapon
        // that silently reaches through everything.
        serde_norway::Value::String(w) if w == "infinite" => {
            Ok(crate::rules::space::INFINITE_BODY_PUNCH_THROUGH_M)
        }
        v => Err(D::Error::custom(format!("punch_through_m: a number or `infinite`, got {v:?}"))),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AttackSpec {
    pub trigger: String,
    /// Ammo spent per SHOT (per tick on a continuous weapon). Default 1.
    ///
    /// READ, not assumed: a flat 1.0 is harmless only while no weapon
    /// disagrees with it, and the Larkspur Prime disagrees on BOTH of its
    /// modes — "Alt-fire consumes 10 ammo per shot" against "0.5 per primary
    /// tick" (wiki).
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
    /// A NON-BOW charge weapon (Tombfinger's primary) pays the draw AND the
    /// listed rate's interval — [`ChargeCadence::DrawThenRate`], a second
    /// cadence rule rather than a tweak to this one.
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
    /// weapon, and the only one in the roster where it is. `build::loadout::resolve`
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
    /// outright, so
    /// a per-part spelling would be six chances to make them differ.
    ///
    /// Its value here is ASSUMED rather than measured (0.1) and the
    /// weapon says so on its own page.
    #[serde(default)]
    pub unaimed_headshot_chance: Option<f64>,
    /// THIS WEAPON HAS NO MAGAZINE, so it never reloads.
    ///
    /// A TOME is the case: the module gives the Grimoire a magazine of ZERO,
    /// which is a fact about the class — there is no clip and no reload in the
    /// sense every gun here has one. The entry writes `magazine: 1` because
    /// this sim cannot fire a magazine of zero, and until 2026-08-28 that stood
    /// alone: the loop emptied the one round, RELOADED for zero seconds, and
    /// did it again on the next shot.
    ///
    /// A ZERO-SECOND RELOAD COSTS NO TIME AND IS STILL AN EVENT. Every
    /// reload-triggered buff in the game therefore fired on EVERY SHOT and
    /// stayed up for the whole engagement — Pressurized Magazine took the
    /// Grimoire from 1,409 DPS to 2,699, on a weapon that never reloads, and
    /// would have won every build search run on it.
    ///
    /// Declaring it is what makes the loop skip the reload rather than perform
    /// a free one, and skipping the EVENT is what makes every effect keyed to
    /// it — the buff triggers, the fire-rate window, the base-damage window,
    /// the instant reloads — inert without any of them being listed here.
    #[serde(default)]
    pub no_magazine: bool,
    /// SECONDS BETWEEN THE TRIGGER AND THE ROUND LEAVING.
    ///
    /// Zero on every gun in this roster and 0.1 s on the Grimoire's primary
    /// fire, which is what makes it a field rather than a
    /// constant: every other gun here fires at 0 s, and this one at 0.1 s.
    ///
    /// IT DOES NOT CHANGE THE CADENCE. The interval between shots is the fire
    /// rate's, exactly — so a sustained engagement fires the same number of
    /// rounds and this is not a DPS penalty. What it moves is WHEN each of them
    /// lands: shot `k` resolves at `windup + k / rate` rather than at `k /
    /// rate`, so the first damage of the fight is late by this much and every
    /// number after it is too.
    ///
    /// THAT IS WORTH MODELLING even though the mean does not move: the combat
    /// record's whole claim is that a row can be laid beside a recording and
    /// checked, and a stream whose timestamps are all 0.1 s early fails that
    /// test. It also reaches time-to-first-kill and the DPS curve's opening.
    ///
    /// SHORTENED BY FIRE RATE, like the throw animation it is the sibling of.
    #[serde(default)]
    pub windup_seconds: f64,
    /// THE SWINGS THIS FORM LOOPS, in order. Empty on every gun.
    ///
    /// A melee form IS its combo: the four ground combos differ in nothing else
    /// (same weapon, same mods, same target) and each is a separate board row,
    /// so the sequence is the entry rather than a decoration on it.
    ///
    /// `damage:` above stays the weapon's own per-swing base, and each entry
    /// scales it — which is what makes the four combos share one transcription
    /// of the weapon and differ only where they actually differ.
    #[serde(default)]
    pub combo_script: Vec<ComboHit>,
    /// WHAT A SECOND BODY IN THE SAME SWING TAKES.
    ///
    /// *"Proportion of weapon damage = FT^(n-1)"* (wiki, Melee) where `n` is
    /// the order the swing reached it in. A hammer is 0.4, so the third body a
    /// swing crosses takes 16%.
    ///
    /// It is the melee answer to punch through and it is NOT one: nothing is
    /// spent, there is no budget, and the wiki names the two things it does not
    /// touch — *"Follow Through does not affect: (Heavy) Slam Attacks, Any
    /// attack that shoots projectiles or deals AoE."*
    #[serde(default)]
    pub follow_through: Option<f64>,
    /// THE WEAPON CLASS'S HEAVY ATTACK MULTIPLIER, and its wind-up.
    ///
    /// `(6.0, 1.2)` for a Hammer — the wiki's per-weapon-type table, which is a
    /// property of the CLASS rather than of the stance. It is stated on EVERY
    /// melee form, including the light ones, because TENNOKAI turns one of
    /// their swings into a heavy attack: a 15% roll on a direct hit opens a
    /// window in which a heavy costs no combo, and a light form with no heavy
    /// multiplier of its own could not fire the swing the window buys.
    #[serde(default)]
    pub heavy: Option<HeavyAttack>,
    /// THE WEAPON'S OWN SLAM, fired by any swing whose `slam_multiplier` says
    /// so — the trailing hit of three of Crushing Ruin's four combos.
    ///
    /// ITS OWN FIELD RATHER THAN `radial`, because they are two different
    /// attacks with different numbers: a normal slam is `2x` in Impact over 9 m
    /// falling to 50%, and a HEAVY slam is `3x` in Blast over 10 m falling to
    /// 70%. The heavy slam is a whole MODE and carries its explosion in
    /// `radial:` like any other AoE attack; this is the one a light combo ends
    /// on, and it is stated at 100% so a swing's own multiplier scales it.
    #[serde(default)]
    pub slam: Option<RadialSpec>,
    /// Does swinging this form SPEND the melee combo counter?
    ///
    /// True on the two heavy forms. The fraction is not here: it is
    /// `1 - heavy_attack_efficiency`, which is a BUILD number and lives with
    /// the mods.
    #[serde(default)]
    pub spends_combo: bool,
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
    /// fight picks by pellet index — see `build::loadout::ResolvedPanel::pellet_damage`.
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
    /// Void field without dealing Void (the field IS the
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
    /// Arsenal lists as a range in metres. The fight applies it at each body's
    /// own distance (`build::loadout::Falloff`).
    ///
    /// The RIVEN pool reads it too. Wiki (`Projectile Speed`),
    /// verbatim: *"Mods including Rivens that have positive or negative
    /// Projectile speeds will affect a weapon's entire Damage Falloff range
    /// accordingly"* and *"Hitscan weapons that do **not** list Damage Falloff
    /// values in their UI are completely unaffected by Projectile Speed
    /// modifications"*. So listing a falloff is precisely what gives the
    /// Projectile Speed stat something to act on when nothing flies — it is
    /// why a shotgun rolls it, and why the Furis does (its Incarnon form
    /// falls off from 10 m to 16 m) while the Latron does not.
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
    /// WHAT IT COSTS is [`crate::rules::space::BODY_MATERIAL_M`] per body crossed.
    /// `999.0` is how INFINITE BODY punch-through is written (the Fluctus, the
    /// Phantasma): the page's qualifier on it — *"innate punch through does not
    /// apply to surfaces"* — separates bodies from geometry, and this arena has
    /// no geometry, so unlimited through bodies is the whole of it here.
    ///
    /// AN AoE ATTACK IGNORES THIS AND EVERY MOD, which is the punch-through
    /// page's own catalog rule and is applied in `build::loadout::resolve` rather than
    /// here: *"weapon projectiles with an area of effect (AoE) component will
    /// not Punch Through enemies or level geometry at all. Instead the
    /// projectile will explode on first contact"*, and *"Projectile AoE weapons
    /// cannot have their Punch Through stat modified"*.
    #[serde(default, deserialize_with = "punch_through_metres")]
    pub punch_through_m: f64,
    /// HOW WIDE THE PROJECTILE IS, in metres — 0 is a ray, which is every
    /// weapon that has not been measured. The class is the punch-through
    /// page's own ("weapons that shoot wide projectiles"), the width of one is
    /// published NOWHERE, and an entry that states a number says where it came
    /// from.
    #[serde(default)]
    pub projectile_width_m: f64,
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
    /// `Some(false)` is a WEAPON PAGE overruling that, which is why the shape
    /// alone does not decide: the Torid's Incarnon form says *"Punch Through
    /// mods have no effect on the behavior of the beam"* and carries neither
    /// `radial:` nor `lingering:`. Nor does the FAMILY decide — the sentence
    /// grouping it with the Ignis for Primary Compression names a weapon on the
    /// punch-through page's EXCEPTION list.
    ///
    /// `Some(true)` is the other direction, for an AoE attack a page says DOES
    /// take them. Nothing in the roster needs it yet.
    #[serde(default)]
    pub punch_through_mods: Option<bool>,
    /// DOUBLE TAP ON AN ATTACK THAT EXPLODES, the Latron Incarnon's way
    /// (MEASUREMENTS M102): only the `radial:` takes the bonus — the direct hit
    /// reads the same with the pile full as with it empty — and each projectile
    /// that lands counts TWO hits, its collision and its explosion. A bounce
    /// counts none.
    #[serde(default)]
    pub consecutive_hit_radial_only: bool,
    /// A radial (AoE) part fired with every projectile of this attack.
    #[serde(default)]
    pub radial: Option<RadialSpec>,
    /// The BOMBLETS this attack's explosion throws out — see [`ClusterSpec`].
    #[serde(default)]
    pub cluster: Option<ClusterSpec>,
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
    /// THE METER THAT GATES THIS FORM — see [`MeterSpec`].
    ///
    /// Declaring one makes this form GAUGE-FED, which `play_modes` reads off
    /// the entry rather than off its name: the form stops being a sustainable
    /// `alternate` a ruler may rank and becomes a `transformed` mode the
    /// builder shows for its own numbers, plus a `cycle` that is how the weapon
    /// is actually played. None of that is decided here — it falls out of the
    /// declaration, which is what that machinery was built for.
    #[serde(default)]
    pub meter: Option<MeterSpec>,
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
    /// at the instant the collision already settled one.
    #[serde(default)]
    pub first_tick_delay_seconds: f64,
    /// Damage types this field's tick applies regardless of status chance —
    /// its OWN, exactly as [`RadialSpec::forced_procs`] is the explosion's.
    ///
    /// A cloud declares none, which is why this defaults to empty. The
    /// Grimoire's orb declares Electricity: measured, its pulses force it and
    /// its final explosion does not — one attack answering the question both
    /// ways, and the reason the two lists are separate rather than shared.
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
    /// cannot crit crit for 2.2x (owner asked; found by the test
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

pub(super) fn lingering_default_true() -> bool {
    true
}

pub(super) fn stack() -> String {
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
    /// Beams that pick their OWN target — absent for every weapon that fires
    /// where it is pointed, which is almost all of them.
    #[serde(default)]
    pub beams: Option<BeamsSpec>,
}

/// AN ATTACK THAT AIMS ITSELF — `rules::chain::Acquire`, as a data file states it.
///
/// The Boar Incarnon: *"can fire up to 3 beams that automatically target
/// enemies within 10° of the reticle"*. This is NOT multishot: multishot puts
/// more instances on one body, and this puts one instance on more bodies. The
/// difference is a factor of three against a crowd and nothing at all against
/// one target, which is exactly backwards from what multishot 3 would do.
#[derive(Debug, Clone, Deserialize)]
pub struct BeamsSpec {
    /// Beams in total, THE AIMED ONE INCLUDED.
    pub count: u32,
    /// Half-angle off the reticle inside which a beam will take a body.
    pub acquire_deg: f64,
    /// How far a beam reaches. Ordinarily the same as the beam's own range,
    /// stated again because the page states it about the beams.
    pub range_m: f64,
}

pub(super) fn chain_compounds_default() -> bool {
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
    /// **MOST AoE TAKES MULTISHOT**, by the mechanism rather than a table: a
    /// pellet lands and detonates, so several pellets are several detonations.
    /// Default YES.
    ///
    /// THE EXCEPTION IS A WEAPON WHOSE BLAST IS TIED TO THE SHOT rather than to
    /// the projectile — the Burston Incarnon, where the wiki states it outright
    /// ("The Radial Attack does not benefit from Multishot bonuses") and the
    /// explosion counts PULLS. WHAT SETTLES A DOUBTFUL ENTRY: a field is left
    /// by a rocket that LANDED, so two napalm fields is two arrivals, and an
    /// Ogris reading `false` costs a Split Chamber 43% of its worth.
    ///
    /// **73 ENTRIES STILL DECLARE `false` AND ONLY TWO HAVE A SOURCE** — the
    /// two Burstons. The rest carry the shape of a bulk intake applying it as a
    /// rule, so a `false` on anything else is unverified rather than stated.
    ///
    /// Declared per entry, never inferred.
    #[serde(default = "yes")]
    pub takes_multishot: bool,
}

/// THE BOMBLETS AN EXPLOSION THROWS OUT — docs/MECHANICS.md §7.1.
///
/// A third damage layer under the attack's own: the shell detonates, and its
/// detonation releases `count` child projectiles that each land a contact hit
/// and an explosion of their own. It is a SHAPE the roster has five of (both
/// Zarrs, the Kuva Bramma, the Kulstar and both Phantasmas) and the reason
/// each of them read as a floor until it existed.
///
/// WHERE THEY GO OFF IS THE EPICENTRE. They are seeking projectiles that fan
/// out and come back down on what the shell landed on, so this arena detonates
/// them where the shell detonated — which is the arrangement a player sees and
/// the only one it can answer. A bomblet that found a body the shell missed is
/// geometry this plane does not hold.
///
/// THE COUNT IS THE BOMB'S, and multishot does not raise it: multishot adds
/// PROJECTILES, and a bomblet is not one — it is something the projectile
/// released. Nothing published states this either way.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterSpec {
    /// How many bomblets one detonation releases.
    pub count: f64,
    /// THE CONTACT HIT, per bomblet. Laid out like the attack's own because a
    /// bomblet IS a little attack: damage and stats here, its explosion under
    /// `radial:`.
    pub damage: BTreeMap<String, f64>,
    #[serde(default)]
    pub crit_chance: Option<f64>,
    #[serde(default)]
    pub crit_multiplier: Option<f64>,
    #[serde(default)]
    pub status_chance: Option<f64>,
    /// The CONTACT hit's forced procs — the bomblet explosion's are its own,
    /// the same split every `radial:` states.
    #[serde(default)]
    pub forced_procs: Vec<String>,
    /// The bomblet's own explosion.
    pub radial: RadialSpec,
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
    /// field.
    #[serde(default = "standard_transmute_out")]
    pub transmute_out_seconds: f64,
}

/// See [`GaugeFormSpec::transmute_out_seconds`]. Changing this changes every
/// Incarnon cycle in the roster, which is the point of it being one number.
pub(super) fn standard_transmute_out() -> f64 {
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

pub(super) fn weakpoint_hits() -> String {
    "weakpoint_hits".to_string()
}

/// The locked-gauge magazine/reload reduction of an Incarnon form.
#[derive(Debug, Clone, Deserialize)]
pub struct PseudoReloadSpec {
    pub magazine: f64,
    pub reload_seconds: f64,
}
