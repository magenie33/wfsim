use super::*;

/// What a build gives up to Primary Compression, before the arcane's own two
/// per-metre ramps are applied to it.
#[derive(Debug, Clone, Copy)]
pub struct Compression {
    /// Metres of blast radius surrendered while aiming — the MODDED radius
    /// times this weapon's row, times the four fifths the arcane takes.
    pub radius_lost_m: f64,
    /// The row's Stacking Behavior: true = the bonus joins the base-damage
    /// bucket, false = it multiplies beside it.
    pub adds: bool,
}

/// What Primary Compression LEAVES of a blast radius while aiming: *"x0.2
/// explosion radius"*. Everything else about the arcane is per-weapon; this
/// fifth is not.
pub const COMPRESSION_RADIUS_KEPT: f64 = 0.2;


/// A TOME'S RECHARGE METER after mod resolution — see
/// [`crate::model::MeterSpec`].
///
/// UNMODDED, every field. No bucket in this engine reaches a recharge clock and
/// the wiki names none for it: fire rate does not shorten it (*"Tick rate is
/// not affected by Fire Rate"* is the same weapon saying the same thing about
/// its other clock), and no mod in the pistol or tome pools claims it. It is
/// resolved rather than read straight off the spec so that the day one does,
/// there is a place for it to land.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedMeter {
    pub seconds_to_fill: f64,
    pub seconds_per_hit: f64,
    pub seconds_per_ammo_pickup: f64,
}

/// A DEPLOYED ORB after mod resolution — see
/// [`crate::model::OrbSpec`]. Geometry and a clock; what it DEALS is the
/// attack's own parts, which are resolved beside this.
///
/// `Copy`, like every other resolved part, because the sim carries one per orb
/// in the air.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedOrb {
    pub fuse_seconds: f64,
    pub strike_interval_seconds: f64,
    /// Reach, AFTER the blast-radius bucket — Fulmination (Primed) enlarges
    /// what an orb can touch, which the owner confirms.
    pub strike_radius_m: f64,
    pub launch_speed_mps: f64,
    pub speed_after_contact_mps: f64,
    /// TOTAL bodies one strike reaches, the struck one included —
    /// `floor(spec x multishot)`, measured (M63). Multishot buys chain targets
    /// on this weapon rather than more orbs, so it is spent here.
    pub chain_bodies: u32,
    /// A hop's reach — UNMODDED, and the one distance here that is. A range
    /// mod widens what the orb can touch and what its detonation covers, and
    /// leaves the jump between two bodies at the six metres the page states.
    pub chain_range_m: f64,
    pub chain_damage_per_hop: f64,
    /// THE THROW AND ITS RECOVERY, already divided by the fire-rate bucket —
    /// see [`crate::model::OrbSpec::throw_seconds`]. A live fire-rate
    /// buff rescales them again in the fight, the same reciprocal trick a
    /// charge draw and a burst delay use.
    pub throw_seconds: f64,
    pub recovery_seconds: f64,
    /// WHERE A STRIKE LANDS — see
    /// [`crate::data::weapons::AttackSpec::unaimed_headshot_chance`], resolved
    /// onto the ORB rather than left on the attack.
    ///
    /// It is declared once, on the attack, because that is where a reader looks
    /// for it; it is CARRIED here because a Tome's cycle fires two forms at
    /// once and they disagree. You point the primary fire and its pellets take
    /// the scenario's aim; the orb picks its own body, and its strikes are the
    /// only thing that reads this.
    pub unaimed_headshot_chance: Option<f64>,
}

/// The lingering field after mod resolution.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResolvedLingering {
    pub damage: DamageVector,
    pub modified_base: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
    pub status_chance: f64,
    /// The field's own UNMODDED stats — the bases its RELATIVE live buffs
    /// multiply (same rule as the radial: a bucket scales whichever base it is
    /// applied to). Torid's cloud is 15% / 2.0x / 25%, none of which match its
    /// grenade impact's 15% / 2.0x / 23%.
    pub base_crit_chance: f64,
    pub base_crit_damage: f64,
    pub base_status_chance: f64,
    pub tick_rate: f64,
    pub duration_seconds: f64,
    /// See [`LingeringBase::first_tick_delay_seconds`]. Unmodded: nothing in
    /// the game moves when a field starts, only how long it lasts.
    pub first_tick_delay_seconds: f64,
    /// See [`LingeringBase::forced_procs`].
    pub forced_procs: crate::rules::damage::ForcedProcs,
    /// Geometry, carried through unmodded — single-target stands at the
    /// epicentre, but the panel states it (and Firestorm enlarges it in game).
    pub radius_m: f64,
    pub falloff_start_m: f64,
    pub falloff_reduction: f64,
    pub stacking: FieldStacking,
    /// See [`LingeringBase::takes_condition_overload`] — CO on an AoE part is
    /// the exception, not the default.
    pub takes_condition_overload: bool,
}

impl ResolvedLingering {
    /// WHAT A BODY `d` METRES FROM THE CLOUD'S CENTRE TAKES, as a fraction —
    /// the same shape as [`ResolvedRadial::falloff_at`], because a cloud falls
    /// off the same way an explosion does and nothing about it being persistent
    /// changes that.
    ///
    /// Nothing outside the cloud, and the full amount before the window opens.
    pub fn falloff_at(&self, d: f64) -> f64 {
        if d >= self.radius_m {
            return 0.0;
        }
        if d <= self.falloff_start_m {
            return 1.0;
        }
        let span = self.radius_m - self.falloff_start_m;
        if span <= 0.0 {
            return 1.0;
        }
        (1.0 - self.falloff_reduction * ((d - self.falloff_start_m) / span)).max(0.0)
    }
}

/// The radial part after mod resolution.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResolvedRadial {
    /// Where this explosion goes off — see
    /// [`crate::model::BlastKind`]. `Contact` on every weapon but the
    /// handful whose round bores on through and detonates behind what it hit.
    pub blast_kind: crate::model::BlastKind,
    pub damage: DamageVector,
    pub modified_base: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
    /// The explosion's UNMODDED crit stats — the bases a RELATIVE live crit
    /// buff multiplies. A weapon may give its explosion different crit stats
    /// from its direct hit (Laetum Incarnon happens to use 22%/2.2x for both),
    /// which is why those bonuses stay relative until the sim knows which
    /// stage it is resolving.
    pub base_crit_chance: f64,
    pub base_crit_damage: f64,
    pub status_chance: f64,
    /// The explosion's UNMODDED status chance — the base a RELATIVE live
    /// status-chance buff (Primary Crux) multiplies. It differs from the
    /// direct hit's, which is why the arcane grant stays relative until the
    /// sim knows which attack part it is resolving.
    pub base_status_chance: f64,
    /// Blast geometry, carried through unmodded — the sim's single target
    /// stands at the epicentre, but the PANEL states it: a reader needs the
    /// radius to know what the explosion is worth beyond one enemy.
    pub radius_m: f64,
    pub falloff_start_m: f64,
    pub falloff_reduction: f64,
    /// See [`RadialBase::forced_procs`].
    pub forced_procs: crate::rules::damage::ForcedProcs,
    /// See [`RadialBase::takes_condition_overload`] — CO on an explosion is the
    /// exception, not the default.
    pub takes_condition_overload: bool,
    /// See [`RadialBase::takes_multishot`].
    pub takes_multishot: bool,
    /// See [`RadialBase::co_base_pair`].
    pub co_base: CoBase,
}

/// The bomblets after mod resolution — see [`ClusterBase`].
#[derive(Debug, Clone, Copy)]
pub struct ResolvedCluster {
    pub count: f64,
    pub contact: ResolvedRadial,
    pub blast: ResolvedRadial,
}

impl ResolvedRadial {
    /// The share the CO term reads, for a reader — the damage path takes the
    /// pair in [`Self::co_base`] instead.
    pub fn co_base_fraction(&self) -> f64 {
        self.co_base.fraction()
    }

    /// What a body `d` metres from the EPICENTRE takes, as a fraction.
    ///
    /// Full inside `falloff_start_m`, decaying linearly to
    /// `1 − falloff_reduction` at the rim, and NOTHING past the radius — the
    /// blast radius IS the falloff's end distance, so the two are one number.
    ///
    /// `falloff_reduction` is the amount REMOVED (the Laetum's 0.2 leaves 80%
    /// at the rim) — DE's own `Reduction`, the same reading the direct hit's
    /// `FalloffSpec::reduction` has.
    pub fn falloff_at(&self, d: f64) -> f64 {
        if d >= self.radius_m {
            return 0.0;
        }
        if d <= self.falloff_start_m {
            return 1.0;
        }
        let span = self.radius_m - self.falloff_start_m;
        if span <= 0.0 {
            return 1.0 - self.falloff_reduction;
        }
        1.0 - self.falloff_reduction * ((d - self.falloff_start_m) / span)
    }
}

/// The resolved panel: everything the dummy sim needs from layers [1]+[2].
#[derive(Debug, Clone)]
pub struct ResolvedPanel {
    /// WHICH WEAPON THIS IS. The panel is everything the sim needs from the
    /// build, and until 2026-08-21 that did not include the weapon's own name —
    /// which is fine while every question is about resolved numbers and stops
    /// being fine the moment something asks a question about the WEAPON. The
    /// Amp auras are the first: Rifle Amp pays a rifle and nothing else, so
    /// somebody has to know the class.
    /// **WHICH FORM THIS PANEL IS** — the entry's own `form:`, carried through
    /// so the fight knows which PRESS it is played on (`data::apl`). The panel
    /// is everything the sim needs from the build, and the input is part of it:
    /// a Magistar's slide attack and its block combo resolve to different
    /// numbers AND to different buttons.
    pub form: crate::model::FormKind,
    pub class: &'static str,
    /// …AND THE POOLS IT DRAWS, because three of the four amps ask THAT
    /// rather than the class: Rifle Amp "also affects bows, sniper rifles
    /// and launchers", which is the `rifle` pool and not any one class.
    pub mod_pools: &'static [&'static str],
    /// …AND THE EQUIPMENT SLOT, which is narrower than the class and wider than
    /// a pool. It is the gate an ARCHON SHARD names: Crimson's "Primary Status
    /// Chance" pays a bow and a shotgun alike.
    pub slot: &'static str,
    /// PUNCH-THROUGH DEPTH, in metres of material — the weapon's own plus every
    /// mod, riven and evolution that grants one, and ZERO on an attack that
    /// cannot use it. See [`crate::rules::space::BODY_MATERIAL_M`].
    pub punch_through_m: f64,
    /// How wide the projectile is — see [`WeaponBase::projectile_width_m`]. No
    /// mod moves it, so it arrives here unchanged.
    pub projectile_width_m: f64,
    /// HOW FAR THIS ATTACK REACHES, metres — `INFINITY` when the weapon
    /// declares none. See [`crate::data::weapons::AttackSpec::range_m`]: past it
    /// there is nothing, which is a different fact from `falloff`, a ramp.
    ///
    /// SCALED BY BEAM RANGE AND BY MELEE REACH, and by nothing else. Beam Range
    /// was an `IndirectStat` — reported on the card, not applied — until a
    /// weapon that takes one was transcribed with a measurement behind it;
    /// melee's Reach pair is the same quantity said the other way, in metres.
    pub range_m: f64,
    /// Post-hierarchy damage vector (physical × (1+base_damage) + combined elements).
    pub damage: DamageVector,
    /// The resolved radial (AoE) part, when the weapon has one.
    pub radial: Option<ResolvedRadial>,
    /// The resolved bomblets, when the weapon's explosion throws any.
    pub cluster: Option<ResolvedCluster>,
    /// THE CONE, accuracy mods applied. A zero-width one lands on the reticle;
    /// `None` = this entry's spread is not transcribed, so no shot of it is
    /// allowed to miss and the entry admits that.
    pub spread: Option<Spread>,
    /// DIRECT-hit damage falloff, when this attack lists one — the shotgun's,
    /// and the range the Arsenal prints. `None` = full damage at any distance.
    pub falloff: Option<Falloff>,
    /// The resolved lingering FIELD, when the weapon leaves one.
    pub lingering: Option<ResolvedLingering>,
    /// CONTINUOUS (beam) weapon — see [`WeaponBase::continuous`].
    pub continuous: bool,
    /// Renewed Horror's field-duration multiplier on the shot after an empty
    /// reload (1.0 = none).
    pub field_duration_on_empty_reload: f64,
    /// Lone Enforcer, carried rather than folded: `(fraction of base multishot,
    /// metres)`. `resolve` cannot settle it because it never sees the arena —
    /// `FightParams::from_panel` does, and that is where it is paid.
    pub multishot_beyond_range: Option<(f64, f64)>,
    /// Final Fusillade's flat multishot add on the magazine's last round
    /// (0.0 = none). NOT folded into `multishot`: it is conditional on the
    /// magazine position, which only the sim can evaluate.
    pub multishot_on_last_round: f64,
    /// The same window, in the OTHER BRACKET: "+5 **Base** Multishot on final
    /// magazine burst", which the wiki notes is "added before mods, and is
    /// thus multiplied by multishot bonuses". So it raises what the weapon's
    /// base pellet count IS for that burst, and every relative bonus — the mod
    /// bucket, a Galvanized stack, an arcane's grant — reads the raised number.
    pub base_multishot_on_last_round: f64,
    /// Plentiful Mayhem's damage bonus on multishot-GENERATED projectiles
    /// (0.0 = none), which also makes multishot spend ammo. Not folded into any
    /// damage bucket: it is an independent multiplier on part of the pellets.
    pub multishot_ammo_bonus: f64,
    /// The Incarnon transformation economy of THIS form, carried through
    /// so the cycle model reads it from data instead of hardcoding one
    /// weapon's numbers.
    pub gauge_form: Option<GaugeForm>,
    /// Beam geometry with `damage_radius_m` already scaled by Blast Range mods.
    /// Firestorm (Primed) enlarges the impact sphere — the one thing a
    /// single-target panel can honestly report about it, since the sphere adds
    /// no damage to a target the beam already struck.
    pub beam: Option<BeamGeometry>,
    /// Ricochet geometry — untouched by mods, unlike the beam's sphere: the
    /// page gives no mod that changes how far or how often a projectile
    /// bounces, and Firestorm reaches the EXPLOSION each bounce sets off
    /// through the radial's own radius rather than through this.
    /// See [`crate::data::weapons::AttackSpec::unaimed_headshot_chance`] — a
    /// property of the ATTACK, unmodded, read by every instance it produces.
    pub unaimed_headshot_chance: Option<f64>,
    /// See [`crate::data::weapons::AttackSpec::windup_seconds`], after the
    /// fire-rate bucket.
    pub windup_seconds: f64,
    // ---- MELEE ----------------------------------------------------------
    /// The melee combo script, delays already shortened by attack speed.
    pub combo_script: Vec<crate::model::ComboHit>,
    /// `FT^(n-1)` — what the n-th body a swing reaches takes.
    pub follow_through: Option<f64>,
    /// THE WEAPON'S OWN SLAM, resolved: what a combo swing that ends on one
    /// detonates. `None` on everything else.
    pub slam: Option<ResolvedRadial>,
    /// THE CLASS'S HEAVY ATTACK — its multiplier, and its wind-up already
    /// shortened by the wind-up bucket. What a Tennokai swing fires.
    pub heavy: Option<crate::model::HeavyAttack>,
    /// TENNOKAI, resolved from whichever of the seven cards are equipped.
    pub tennokai: Tennokai,
    /// Does a swing of this form spend the combo counter?
    pub spends_combo: bool,
    /// Seconds the combo counter survives untouched, mods included.
    pub combo_duration_seconds: f64,
    /// THE WINDOW THIS BUILD'S MELEE INCARNON IS ON FOR — see
    /// [`WeaponBase::melee_incarnon`]. `None` on everything without a Genesis.
    pub melee_incarnon: Option<MeleeIncarnon>,
    /// …AND WHETHER THE COUNTER IS STOPPED ALTOGETHER. *"A zero or negative
    /// combo duration prevents increasing the combo counter"* (wiki), which is
    /// a harder stop than the 0.1 s floor beside it: the clock is not short,
    /// the counter never rises. It is its own field because the floor has
    /// already been applied by the time anyone reads the seconds.
    pub combo_frozen: bool,
    /// The floor the counter returns to, in points, mods and evolutions
    /// included.
    pub initial_combo: f64,
    /// Fraction of the counter a heavy attack does NOT spend, clamped to the
    /// wiki's 0.9 cap.
    pub heavy_attack_efficiency: f64,
    /// Blood Rush's per-combo-tier crit chance.
    pub crit_chance_per_combo: f64,
    /// Weeping Wounds' per-combo-tier status chance.
    pub status_chance_per_combo: f64,
    /// Chance of an EXTRA combo point per landed hit (Quickening, True
    /// Punishment). Each whole 1.0 repeats the hit's points; the rest rolls for
    /// one point per base point (MEASUREMENTS M96, M97).
    pub combo_count_chance: f64,
    /// …AND WHAT A LIFTED TARGET ADDS TO IT (Enduring Strike).
    pub combo_count_chance_on_lifted: f64,
    /// Chance to Gain Combo Count: 0, or a riven's malus — a gate each base
    /// combo point survives with `1 + this` (MEASUREMENTS M97).
    pub combo_gain_chance: f64,
    /// COMBO POINTS PER BODY THE SLAM REACHED (Shockwave Synergy), before the
    /// combo count chance that scales them.
    pub combo_count_on_slam_hit: f64,
    /// Relative status chance a LIFTED target adds (Enduring Affliction) — the
    /// bracket Weeping Wounds is in.
    pub status_chance_on_lifted: f64,
    /// `+X%` damage on a heavy attack alone (Killing Blow).
    pub heavy_attack_damage: f64,
    /// `+X%` damage on a slam alone (Seismic Wave).
    pub slam_damage: f64,
    /// See [`crate::data::weapons::AttackSpec::no_magazine`] — carried through
    /// unmodded, because no bucket can give a weapon a clip it does not have.
    pub no_magazine: bool,
    /// See [`ResolvedOrb`]. `Some` means this attack settles no collision and
    /// no explosion at the impact — the orb delivers both, later and elsewhere.
    pub orb: Option<ResolvedOrb>,
    /// See [`ResolvedMeter`] — the recharge clock this form is gated behind.
    pub meter: Option<ResolvedMeter>,
    pub ricochet: Option<Ricochet>,
    /// Additive headshot-damage bonus from evolutions (Caput Mortuum).
    pub headshot_damage_bonus: f64,
    /// Devouring Attrition's (chance, bonus) on non-crit instances.
    pub noncrit_bonus: Option<(f64, f64)>,
    /// Overwhelming Attrition's stacking damage buff.
/// Every stacking buff, with [`BuffGrant::FireRate`] already converted from
    /// a fraction to an absolute rate. See [`StackingBuff`].
    pub stacking_buffs: Vec<StackingBuff>,
    /// ModifiedBase = unmodded total × (1 + Σ base damage) — the base of
    /// every status-payload formula (elemental portions excluded).
    pub modified_base: f64,
    pub crit_chance: f64,
    pub crit_damage: f64,
    /// PRELUDE OF MIGHT, unresolved on purpose: `(how much of `crit_damage`
    /// this perk is, the crit-chance threshold it has to stay under)`. A panel
    /// is the OPTIMISTIC half of the condition — the wiki's note says the
    /// threshold is read against a crit chance the panel cannot see — so the
    /// perk is granted here and the SIM takes it back on any hit that has
    /// climbed over the line. `None` when the perk is not installed, or when
    /// the panel alone already fails the condition and there is nothing to
    /// take back. See [`WeaponBase::crit_multiplier_below_crit_chance`].
    pub crit_multiplier_below_crit_chance: Option<(f64, f64)>,
    pub status_chance: f64,
    /// UNMODDED crit and status stats of the DIRECT part — the bases a
    /// RELATIVE live buff multiplies, the counterpart of `base_multishot`.
    pub base_crit_chance: f64,
    pub base_crit_damage: f64,
    pub base_status_chance: f64,
    pub fire_rate: f64,
    /// MODDED charge time (bows) — `base / (1 + fire-rate bonuses)`, the same
    /// factor `fire_rate` is multiplied by, so the two never disagree about
    /// what a fire-rate mod did. `Some` means the sim paces on this instead of
    /// `1 / fire_rate`.
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
    /// THE FIELD'S OWN BASE, whether it came from the WEAPON or from a MOD.
    ///
    /// The panel prints a `base -> final` pair for every part, and for the field
    /// it read `WeaponBase::lingering` — which is `None` when a mod granted the
    /// field, so Nightwatch Napalm's fire was resolved, simulated, and drawn
    /// nowhere at all. A part that pays most of a build's damage and appears on
    /// no card is the worst kind of invisible.
    pub lingering_base: Option<LingeringBase>,
    pub battery: Option<crate::model::Battery>,
    /// ROUNDS A SECOND under Pax Charge — carried from the weapon so
    /// `FightParams::from_panel` can build the battery, which is where the
    /// ARCANE is finally in hand. The rate is the only part of that mechanic
    /// the weapon owns.
    pub recharge_per_second: Option<f64>,
    /// Carried from the weapon so the fight can read it — see
    /// `data::weapons::WeaponSpec::echo_multiplier`.
    pub echo_multiplier: f64,
    /// Ammo per shot — a WEAPON constant, so no mod bucket touches it.
    pub ammo_cost: f64,
    /// See `data::weapons::WeaponSpec::headshot_bonus_multiplicative`.
    pub headshot_bonus_multiplicative: bool,
    pub charge_cadence: crate::model::ChargeCadence,
    /// The burst shape with its DELAY already modded — the same treatment
    /// `charge_seconds` gets, and for the same reason: a fire-rate bonus is
    /// spent here rather than re-derived in the sim.
    pub burst: Option<crate::model::BurstSpec>,
    pub multishot: f64,
    /// The weapon's UNMODDED pellet count — the base a relative multishot
    /// buff (Conjunction Voltage) multiplies when it joins the bucket live.
    pub base_multishot: f64,
    pub magazine_size: f64,
    /// Reserve rounds after mods (Ammo Chain, a riven's Ammo Maximum…), and
    /// whether the sim is allowed to spend them. Both travel together: a
    /// number without the flag is a panel figure, not a limit.
    pub ammo_reserve: f64,
    pub has_reserve: bool,
    /// See [`WeaponBase::ammo_pickup`].
    pub ammo_pickup: f64,
    /// THE MUTATION MOD'S SHARE — *"Converts Secondary ammo pickups to X% of
    /// Ammo Pick Up"*. 0 with no such card, 0.92 with a maxed Primed one.
    pub ammo_conversion: f64,
    pub no_resupply: bool,
    /// Untouched by mods — the passive's numbers are the weapon's own.
    pub super_crit_on_status: Option<crate::model::SuperCritSpec>,
    /// See [`WeaponBase::weakpoint_stacks`].
    pub weakpoint_stacks: Option<crate::model::WeakpointStacksSpec>,
    /// See [`WeaponBase::spawn_on_kill`].
    pub spawn_on_kill: Option<crate::model::SpawnOnKillSpec>,
    /// See [`WeaponBase::kill_streak_summon`].
    pub kill_streak_summon: Option<crate::model::KillStreakSummonSpec>,
    /// See `data::weapons::WeaponSpec::beam_ramp_floor`. No mod moves it.
    pub beam_ramp_floor: f64,
    /// Does this weapon apply MICROWAVE? See `fight::DebuffState::microwave`.
    pub applies_microwave: bool,
    /// See `data::weapons::WeaponSpec::independent_procs`.
    pub independent_procs: &'static [&'static str],
    /// Forced procs, carried through unmodded — no mod grants or removes one.
    pub forced_procs: Vec<DamageType>,
    /// ONE RESOLVED VECTOR PER PROJECTILE, in firing order — `(direct, radial)`
    /// — for a weapon whose missiles carry different innate elements. EMPTY on
    /// every other weapon, and the fight reads `damage` as it always did.
    ///
    /// Resolved by running the whole panel once per element rather than by
    /// retyping a finished vector, because an innate element enters the
    /// elemental hierarchy and a finished vector has already forgotten where
    /// its Blast came from. It costs six resolves at BUILD time and nothing in
    /// the fight.
    pub pellet_damage: Vec<(DamageVector, DamageVector)>,
    /// See `data::weapons::AttackSpec::multishot_adds_damage`.
    pub multishot_adds_damage: bool,
    /// The field the attack plants, unmodded for the same reason: no mod in
    /// the roster lengthens it. See `data::weapons::AttackSpec`.
    pub attractor_seconds: Option<f64>,
    /// Untouched by mods: the tendril cap is the weapon's.
    pub tendril_max: u32,
    /// How far a tendril reaches and how far off the reticle it will take a
    /// body — see [`crate::data::weapons::TendrilSpec`]. Both zero on every
    /// weapon that has no tendrils.
    pub tendril_range_m: f64,
    pub tendril_acquire_deg: f64,
    /// THE SHOT COMBO COUNTER, or `None` — and `None` is what a sniper fired
    /// from the hip resolves to, because *"building combo and benefiting from
    /// its multiplier requires being scoped in"* (wiki `Sniper Rifle`). That is
    /// the whole gate: it is answered once, here, so the simulator and the
    /// optimizer get the same answer for a hip-fired scenario without either of
    /// them knowing what a sniper is.
    pub sniper_combo: Option<crate::model::SniperCombo>,
    /// Sentient Surge: crit chance added PER ACTIVE TENDRIL, relative to the
    /// unmodded base — "Additive to other crit chance and status chance mods"
    /// (wiki), so it joins the same bucket Pistol Gambit does rather than
    /// forming one of its own.
    pub crit_chance_per_tendril: f64,
    /// Its status half, same bucket rule.
    pub sc_per_tendril: f64,
    /// HATA-SATYA under Emergent: the rate per hit and the ceiling on what it
    /// is worth, spent in the sim because the pile's size is a fact about the
    /// fight. `None` under the other policies — AssumedMax has already folded
    /// it into `crit_chance`, and BaseOnly refuses conditionals.
    pub crit_chance_per_hit: Option<CritPerHit>,
    /// ACID SHELLS' corpse explosion, when the build carries the augment.
    pub acid_shells: Option<AcidShells>,
    /// Fraction of the magazine returned on each kill, from the reserve.
    pub magazine_refill_on_kill: f64,
    /// The syndicate radial this build's augment grants, if any.
    pub syndicate_radial: Option<crate::data::syndicates::SyndicateDef>,
    pub reload_seconds: f64,
    /// Σ reload-speed bonuses — transitions (Incarnon transmute/revert)
    /// scale by the same formula: time = base / (1 + this).
    pub reload_bonus: f64,
    /// Σ base-damage bonuses (needed live when CO joins this bucket).
    pub base_damage_bonus: f64,
    /// See [`WeaponBase::base_damage_below_half_health`] — carried through unchanged,
    /// already corrected for the granting perk's own flat base damage.
    pub base_damage_below_half_health: f64,
    /// See [`WeaponBase::crit_chance_on_undamaged`] — converted to the post-mod number.
    pub crit_chance_on_undamaged: f64,
    /// See [`WeaponBase::crit_damage_on_undamaged`] — converted to the post-mod number.
    pub crit_damage_on_undamaged: f64,
    /// Σ (CO per_stack × stacks) under `AssumedMax` (0 under
    /// `Emergent` — see `co_stack`) — applied per this weapon's
    /// [`CoBehavior`] × `co_base_fraction`, DIRECT HITS ONLY.
    pub co_per_type: f64,
    pub co_behavior: CoBehavior,
    /// WHAT PRIMARY COMPRESSION HAS TO WORK WITH on this build — the metres of
    /// blast radius it gives up while aiming, and which bracket it pays into.
    /// `None` = the weapon has no row (nothing to compress) or the fight's
    /// Tenno is not aiming, which is the same answer: the arcane is worth
    /// nothing.
    ///
    /// The arcane's own two ramps are NOT spent here. They are per METRE and
    /// this is the metres, so the multiplication happens where a build meets an
    /// arcane — `FightParams::from_panel` — and that is one place rather than
    /// three. It cannot be this one: the optimizer resolves a panel ONCE and
    /// pairs it with every arcane in the search, so a panel that had already
    /// spent an arcane would have to be re-resolved per job.
    ///
    /// PER FORM. The Torid's cloud pays +240% and its Incarnon beam pays
    /// nothing, so one arcane has two answers inside one cycle.
    pub compression: Option<Compression>,
    /// See [`WeaponBase::co_base_pair`] — the absolute and its denominator.
    pub co_base: CoBase,
    /// See [`WeaponBase::unswung_fraction`] — the share of this build's vector
    /// an attack's own multiplier must leave alone.
    pub unswung_fraction: f64,
    /// Live on-kill CO stacks (Emergent policy).
    pub co_stack: Option<StackSpec>,
    /// Live on-kill multishot stacks (Emergent policy); per_stack is
    /// already × base pellets.
    pub multishot_stack: Option<StackSpec>,
    /// Crosshairs' on-headshot buff (Emergent): ABSOLUTE crit chance
    /// (base_cc × bonus) as a timed buff (starts active).
    pub crit_chance_on_headshot: Option<TimedBuff>,
    /// LEADED GAS: the element and the status chance a weak-point hit turns on.
    pub on_weakpoint: Option<crate::model::WeakpointBuff>,
    /// Crosshairs' on-headshot-kill stacks (Emergent): per_stack is
    /// ABSOLUTE crit chance; per-stack expiry semantics.
    pub crit_chance_stack: Option<StackSpec>,
    /// (1 + Σ status damage) — multiplies status payload values.
    pub status_damage_multiplier: f64,
    /// (1 + Σ status duration) — scales status-effect DoT durations.
    pub status_duration_multiplier: f64,
    /// Σ chance for a CRITICAL hit to apply a Slash status (Hunter
    /// Munitions), rolled per pellet, independent of status chance.
    pub slash_on_crit: f64,
    /// MOD SET bonus: chance for a hit that ALREADY crit to move up one
    /// critical tier (Vigilante). Scales per equipped member with no
    /// threshold — see [`crate::data::mod_sets`]. 0.0 = no set equipped.
    pub crit_tier_upgrade_chance: f64,
    /// Summed INDIRECT buckets (recoil, accuracy, ammo…): outside the
    /// theoretical-DPS math, stated on the panel; a future shooter model
    /// (2D recoil/aim, travel time, ammo sustain) consumes them.
    pub indirect: Vec<(IndirectStat, f64)>,
    /// (element, 1 + Σ that element's bonuses) — the elemental bracket of
    /// DoT tick formulas (only literal same-element mods count).
    pub elem_dot_bonus: Vec<(DamageType, f64)>,
    /// (faction, Σ bonus) — faction-damage bucket (Bane/Expel), ADDITIVE
    /// within a faction. Applied at sim time only vs a matching-faction
    /// target (×2 on DoT ticks); shown on the panel as a conditional row.
    pub faction_damage: Vec<(Faction, f64)>,
    /// Σ LISTED Weak Point damage (Acuity). Sim: +1.5× this on the part
    /// multiplier of true weak points, before the headshot bracket.
    pub weakpoint_damage: f64,
    /// The weapon's own headshot multiplier where it overrules the enemy body
    /// part's — see `data::weapons::WeaponSpec::headshot_multiplier`. `None` on
    /// every weapon whose head is worth what the body part says.
    pub headshot_multiplier: Option<f64>,
    /// ABSOLUTE crit chance added on weak-point hits only (base_cc × Σ
    /// relative weak-point CC bonuses); part-conditional, all policies.
    pub weakpoint_crit_chance_relative: f64,
    /// King's Gambit's other half: a MULTIPLIER on a non-weak-point pellet's
    /// crit chance, applied after everything else. 1.0 = ordinary.
    pub bodyshot_crit_chance_multiplier: f64,
    /// WISEMAN'S REGARD, AS A LIVE SPEC: `(rate, cap, what the panel already
    /// folded in)`, the first two ALREADY multiplied by the status-chance mods
    /// because the card grants BASE status chance.
    ///
    /// "30% of CURRENT Critical Chance" is current at the moment of the shot,
    /// not at the arsenal: the row names Secondary Outburst, Cascadia
    /// Overcharge, Secondary Enervate and Galvanized Crosshairs among the
    /// sources that feed it, and all four are live. The panel still shows the
    /// static answer — that is what a panel can say — so the sim subtracts the
    /// third number and adds what the shot actually earns.
    pub derived_status_from_crit: Option<(f64, f64, f64)>,
    /// The mirror (the Dera's High Ground), same shape: `(rate, cap, folded)`
    /// with the first two multiplied by the CRIT-chance mods.
    pub derived_crit_from_status: Option<(f64, f64, f64)>,
    /// Galvanic Reload: `(status, chance, rounds)`, rolled ONCE PER SHOT.
    /// Double Tap: `(per stack, max stacks, seconds)` — its OWN multiplier,
    /// counted per trigger pull. See [`ModEffect::ConsecutiveHitDamage`].
    pub consecutive_hit_damage: Option<(f64, u32, f64)>,
    /// See [`crate::data::weapons::AttackSpec::consecutive_hit_radial_only`].
    pub consecutive_hit_radial_only: bool,
    /// SYNTH CHARGE's multiplier for the magazine's LAST round — see
    /// [`ModEffect::LastRoundDamage`]. Zero on a continuous weapon and on an
    /// Incarnon form, resolved here because only this layer knows the form.
    pub last_round_damage: f64,
    /// THE CHAMBER FAMILY's summed multiplier for the magazine's FIRST round —
    /// see [`ModEffect::FirstRoundDamage`]. Unlike its last-round twin nothing
    /// is switched off here: the Incarnon exemption is Synth Charge's own card
    /// text, and the Vectis Incarnon's per-shot Primed Chamber was a BUG DE
    /// fixed (wiki, ver 43.5) rather than a form that pays nothing.
    pub first_round_damage: f64,
    pub round_restore_on_status: Option<(crate::rules::damage::DamageType, f64, f64)>,
    /// Exact Penance: the chance a KILL — from anywhere — reloads instantly.
    pub instant_reload_on_kill: Option<f64>,
    /// Resonant Restore: `(per stack, max stacks)`, the per-stack value ALREADY
    /// scaled by the magazine mods — the card says "Base Magazine Capacity", so
    /// a Magazine Warp build gets more out of every stack. Same units
    /// conversion `BuffGrant::FlatBaseDamage` and `FireRate` take, and for the
    /// same reason: the sim adds it to a number the mods are already inside.
    pub magazine_growth_on_empty_reload: Option<(f64, u32)>,
    /// Sharpened Bullets under Emergent: ABSOLUTE crit-damage add as a timed
    /// buff (starts inactive), granted/refreshed on every kill.
    pub crit_damage_on_kill: Option<TimedBuff>,
    /// Pressurized Magazine under Emergent: ABSOLUTE fire-rate add as a timed
    /// buff (starts inactive), granted on every reload.
    pub fire_rate_on_reload: Option<TimedBuff>,
    /// Deadly Efficiency's window — see [`ModEffect::OnReloadDamage`]. Its
    /// `value` is the RELATIVE bonus, because it joins the base-damage bucket
    /// rather than replacing a rate.
    pub base_damage_on_reload: Option<TimedBuff>,
    /// EXIMUS ADVANTAGE's window — see [`ModEffect::OnEximusWeakpointDamage`].
    /// Its `value` is RELATIVE, joining the base-damage bucket beside Hornet
    /// Strike's, which is what the card's "Stacks additively with base damage
    /// bonuses" says it should do.
    pub base_damage_on_eximus_weakpoint: Option<TimedBuff>,
    /// READY RETALIATION's window — see
    /// [`crate::model::EvoEffect::ReloadSpeedOnEmptyReload`]. It joins
    /// the reload bucket the mods and `evo_reload_bonus` feed, but only while
    /// open, and only a reload FROM EMPTY opens it.
    /// READY RETALIATION as the sim holds it: a buff with NO DURATION that is
    /// simply up or down (0.0 = the weapon does not have the perk).
    ///
    /// The magazine running out puts it up; a reload completing, or either
    /// Incarnon transform completing, takes it down — because all three refill
    /// the magazine. Nothing ASKS whether the moment is a reload: the value is
    /// summed into the live reload-speed total wherever that total is needed,
    /// and only the removal events are reasoned about.
    pub rs_on_reload: f64,
    /// Flensing Spikes' rate — see [`WeaponBase::armor_strip_per_puncture`].
    pub armor_strip_per_puncture: f64,
    /// JAHU CANTICLE — see [`ModEffect::StripOnKillInRange`]. `(fraction,
    /// radius_m)`, `None` with the card off the build.
    pub strip_on_kill_in_range: Option<(f64, f64)>,
    /// VOME INVOCATION: what the build adds to the FIGHT's Ability Strength,
    /// at the cap. Zero without the card.
    pub ability_strength_bonus: f64,
    /// RIS INVOCATION: the same for Ability Duration.
    pub ability_duration_bonus: f64,
    /// EXECUTIONER'S FORTUNE — see [`InstantReload`]. Carried straight to the
    /// sim under every policy: it is an EVENT, and there is no panel stat an
    /// assumed-max reading could spend it into (a magazine that refills itself
    /// is not a bigger magazine).
    pub instant_reload: Option<InstantReload>,
    /// LINGERING JUDGEMENT — see [`HeadshotStreak`]. Carried to the sim under
    /// every policy: whether the streak ever arms is a property of the FIGHT
    /// (a body-shot engagement never does), not something a panel can assume.
    pub headshot_streak: Option<HeadshotStreak>,
    /// SPITEFUL DEFILEMENT — see [`WeaponBase::crit_damage_below_status_count`]. Also
    /// carried: its condition is the TARGET's live status count.
    pub crit_damage_below_status_count: Option<(u32, f64)>,
    /// Hemorrhage's status-conversion roll (an event mechanic — active under
    /// every policy; contributes no static panel stat).
    pub proc_conversion: Option<ProcConv>,
    /// The evolution's PERMANENT stacked multishot (Fevered Frenzy), if any:
    /// its FULL contribution is already inside `multishot`; the per-buff
    /// config rescales via this spec (no in-sim trigger, no decay — the
    /// stack count is a static choice, full by default).
    pub evo_multishot: Option<EvoMsBuff>,
    /// The evolution's PERMANENT flat base damage (Reified Bane), if any.
    pub evo_base_damage: Option<EvoBdBuff>,
    /// Stats an equipped mod has LOCKED at the weapon's default (`disables`):
    /// `multishot` for the Acuity pair, `fire_rate` for the Cannonades.
    ///
    /// Stated on the panel because the panel is not the last word on either.
    /// A lock is absolute — "set to its default ignoring other bonuses, even
    /// negative effects" — and the sim owns the live sources that never reach
    /// this struct's arithmetic: an arcane's multishot stacks, the weapon's
    /// Frenzy passive. They read this rather than each re-deriving it.
    pub locked: Vec<&'static str>,
}

impl ResolvedPanel {
    /// The share the CO term reads, for a reader — the damage path takes the
    /// pair in [`Self::co_base`] instead.
    pub fn co_base_fraction(&self) -> f64 {
        self.co_base.fraction()
    }

    /// Is the reserve effectively bottomless for this weapon, under this
    /// scenario's Infinite-ammo setting?
    ///
    /// THE ONE PLACE THE RULE IS WRITTEN. It lives on the panel rather than in
    /// a caller because two callers wrote the same wrong version of it
    /// (`infinite_ammo || !finite_reserve`): the simulator is the truth and the
    /// optimizer obeys it, so the optimizer must CALL this rather than restate.
    ///
    /// IT TAKES THE FIGHT'S ANSWER, NOT THE READER'S BOX. A resupply half
    /// spelled here as `&& !self.no_resupply` is a second spelling of
    /// `build::scenario::Capability::CanResupply`, and `build::scenario::resolve` is the one
    /// that survives because it is the only one that can hear a SCENARIO argue
    /// — a fight may declare "in here, Arch-Guns have infinite ammo". So two
    /// facts meet here instead of three:
    ///
    /// - `has_reserve` — is there a pool behind the magazine at all? A sentinel
    ///   weapon has none, so nothing can make it run out.
    /// - `ammo_is_infinite` — the fight's answer for THIS weapon.
    pub fn reserve_is_infinite(&self, ammo_is_infinite: bool) -> bool {
        !self.has_reserve || ammo_is_infinite
    }
}

/// A permanent flat-BASE-DAMAGE buff on the resolved panel, sibling to
/// [`EvoMsBuff`] and rescaled the same way: the panel already carries the full
/// contribution, and the buff card scales it back out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EvoBdBuff {
    /// The flat base damage the buff contributes at full stacks (+14).
    pub full: f64,
    /// The base TOTAL without it — the denominator for scaling back. The
    /// bonus rides the whole mod chain multiplicatively (flat base damage is
    /// added pro-rata BEFORE mods), so removing it is one ratio on the
    /// resolved vector rather than a re-resolve.
    pub without: f64,
    pub max_stacks: u32,
    pub stacks: u32,
}

/// A permanent stacked multishot buff on the resolved panel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EvoMsBuff {
    /// FINAL multishot contributed at full stacks (base pellets × Σ bonus).
    pub full: f64,
    pub max_stacks: u32,
    /// The count actually in play. PERMANENT stacks never move during a run,
    /// so this is a static choice — but the replay still has to be able to say
    /// what it was, and `multishot` has already absorbed it by then.
    pub stacks: u32,
}

/// A resolved status-conversion roll (Hemorrhage).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcConv {
    pub from: DamageType,
    pub to: DamageType,
    pub chance: f64,
    /// Chance ×`low_rate_multiplier` while LIVE fire rate < this (strictly).
    pub low_rate_threshold: f64,
    pub low_rate_multiplier: f64,
}
