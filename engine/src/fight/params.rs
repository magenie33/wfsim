use super::*;
use crate::target::BodyPart;

/// WHAT ARMS THE SECOND SET OF NUMBERS — the one thing a gun's Incarnon and a
/// melee's genuinely disagree about, said as data instead of as two mechanisms.
///
/// A melee Incarnon is NOT A FORM and this does not make it one: it unlocks no
/// weapon entry and changes no animation (`unlocks_weapon` stays absent on
/// every melee Genesis). What it changes is NUMBERS, part-way through the
/// fight — which is what [`IncarnonCycle`] already is, and the second panel
/// here is the same weapon resolved twice rather than a second entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Arms {
    /// A GAUGE, filled by hits of one kind (Zariman: weak-point; Torid: any
    /// direct hit). The gun Incarnon's.
    Gauge {
        charge_on: crate::model::ChargeOn,
        /// Hits of `charge_on` to fill it (Dual Toxocyst 9, Torid 5).
        charges_to_fill: u32,
    },
    /// A HEAVY ATTACK TAKEN AT OR ABOVE A COMBO MULTIPLIER — *"Reach 6x Combo
    /// and then Heavy Attack to activate Incarnon Form"*. The melee Incarnon's,
    /// and it needs no gauge because the counter it reads is one the fight
    /// already keeps.
    HeavyAtCombo(f64),
}

/// …AND WHAT ENDS IT.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Ends {
    /// The CHARGE MAGAZINE runs dry. The gun Incarnon's, and the reason its
    /// cycle is written around a magazine at all.
    ChargeMagazine,
    /// A CLOCK, from the moment it armed. The melee Incarnon's — and the whole
    /// of what separates a Genesis worth the engagement (180 s) from one worth
    /// half of it (the Praedos's 90 s).
    After(f64),
}

/// The real Incarnon combat cycle: the run STARTS IN THE BASE FORM WITH AN
/// EMPTY GAUGE and earns its way in — weakpoint hits (each multishot pellet
/// counts) fill it, transmute (`transmute_seconds`), spend the charge
/// magazine, revert (`transmute_out_seconds`), repeat. Swapping either way fully reloads the
/// base form's magazine (wiki side effect). Frenzy EXISTS in the Incarnon
/// Form too: the buff persists across
/// transforms and headshots keep triggering it in both forms.
#[derive(Debug, Clone)]
pub struct IncarnonCycle {
    /// The base form's full engagement params (its own panel; target/aim/
    /// duration fields are ignored — the outer params' are shared).
    pub base_form: Box<FightParams>,
    /// WHAT PUTS THE WEAPON IN ITS SECOND SET OF NUMBERS.
    pub arms: Arms,
    /// …AND WHAT TAKES IT BACK OUT.
    pub ends: Ends,
    /// Incarnon → base transition (already reload-speed scaled).
    pub transmute_out_seconds: f64,
    /// Base → Incarnon transition (already reload-speed scaled).
    pub transmute_seconds: f64,
    /// The reload-speed bucket the two times above were divided by. A
    /// LIVE bonus (Lethal Rearmament) rescales them by
    /// `(1 + bucket) / (1 + bucket + live)`.
    pub reload_bucket: f64,
    /// Does the engagement OPEN already transformed, with a full charge
    /// magazine?
    ///
    /// False, and that is the fight the benchmark runs. A full gauge is a
    /// CONSUMABLE resource, and this project's own rule for those is that they
    /// start at zero and are earned in the fight (docs/BUFFS.md) — the cycle
    /// opening transformed was an exception to a rule already written down, and
    /// it handed every Incarnon weapon a magazine it had not paid for.
    ///
    /// It matters most where the gauge cannot be refilled: on a board with no
    /// weak-point hits, eight of the nine Incarnon forms can never charge, so
    /// a free opening magazine was the only Incarnon damage they would ever
    /// deal and it was pure gift.
    ///
    /// True is still a real way to play — you walk into the room having charged
    /// on the last one — which is why it is a field rather than a deletion.
    pub starts_primed: bool,
}

/// Parameters of the dummy engagement.
#[derive(Debug, Clone)]
pub struct FightParams {
    /// WHICH RUN IS THE BENCHMARK FIGHT: runs are ranked by this and the one at
    /// `len / 2` is replayed. The scenario's metric decides it.
    pub sample_by: crate::rules::metrics::RunStat,
    /// WHAT THE WARFRAME BRINGS, resolved from the Tenno's auras and shards.
    /// Computed once in `from_panel` rather than re-derived per shot, and read
    /// wherever a squad effect lands — the armour multiplier at mitigation, the
    /// stack ceiling at the proc.
    pub squad: crate::data::tenno::SquadEffects,
    /// The weapon's (modded) base damage vector. Quantized once per run for
    /// dealing damage and proc-type weighting.
    pub damage: DamageVector,
    /// RESOLVED (modded) crit chance of the DIRECT part — the name is
    /// historical; `unmodded_crit_chance` below is the real base.
    pub base_crit_chance: f64,
    /// MOD SET bonus: chance for a hit that ALREADY crit to move up one
    /// critical tier (Vigilante). 0.0 = no set member equipped.
    pub crit_tier_upgrade_chance: f64,
    /// Hunter Munitions: chance for a CRITICAL hit to apply a Slash status,
    /// rolled per pellet and independent of status chance.
    pub slash_on_crit: f64,
    pub crit_multiplier: f64,
    /// PRELUDE OF MIGHT's live gate — `(the part of `crit_multiplier` this
    /// perk is, the crit-chance threshold)`. Carried rather than settled
    /// because the condition is read at the moment of the hit; see
    /// [`crate::build::loadout::ResolvedPanel::crit_multiplier_below_crit_chance`]. Per FORM, which
    /// is what a cycle needs: the Furis's base form sits at 5% and its
    /// Incarnon form at 26%, so the same Weakened stacks push one over the
    /// 40% line and leave the other under it.
    pub crit_multiplier_below_crit_chance: Option<(f64, f64)>,
    /// UNMODDED crit stats of the DIRECT part — the bases a RELATIVE live crit
    /// buff multiplies (the radial carries its own pair on `ResolvedRadial`).
    pub unmodded_crit_chance: f64,
    pub unmodded_crit_damage: f64,
    /// Listed status chance per hit (may exceed 1.0).
    pub status_chance: f64,
    /// UNMODDED status chance — the base a RELATIVE live status-chance buff
    /// (Primary Crux) multiplies, exactly like `base_multishot`.
    pub base_status_chance: f64,
    /// Forced procs on every hit (weapon data, per attack part).
    pub forced_procs: Vec<DamageType>,
    /// Seconds of Bullet Attractor this attack PLANTS on the target — see
    /// `data::weapons::AttackSpec::attractor_seconds`. `None` for every weapon
    /// but the thrown spearguns.
    pub attractor_seconds: Option<f64>,
    /// Status duration multiplier (1.0 = unmodded).
    pub status_duration_multiplier: f64,
    /// Base fire rate; multiplied live by BuffBar fire-rate multipliers
    /// (Frenzy x2.5) to schedule the next shot.
    pub fire_rate: f64,
    /// CHARGE trigger (bows): the modded draw before the shot. When `Some` it
    /// REPLACES `1 / fire_rate` as the interval between pulls — the weapon
    /// fires the moment the draw finishes — and live fire-rate buffs divide it
    /// by the same factor they would have multiplied the rate by. `fire_rate`
    /// stays the listed stat, which is what Hemorrhage's below-2.5 gate reads.
    ///
    /// A bow's magazine is 1, so the reload lands between two draws either way
    /// and the cycle is `charge + reload` however the two are ordered.
    pub charge_seconds: Option<f64>,
    /// Which charge formula paces the shot — see `ChargeCadence`.
    pub charge_cadence: crate::model::ChargeCadence,
    /// A RATE THAT FALLS WHILE THE TRIGGER IS HELD — see
    /// [`crate::model::SustainedFireRate`]. It scales the CADENCE and
    /// nothing else: `fire_rate` stays the listed stat, which is what
    /// Hemorrhage's below-2.5 gate reads and what the panel prints, exactly as
    /// on a charge weapon.
    pub sustained_fire_rate: Option<crate::model::SustainedFireRate>,
    /// A MAGAZINE THAT REFILLS ITSELF — see [`crate::model::Battery`].
    /// The EMPTY case is already `reload_seconds` (delay + a full refill); what
    /// this adds is the BETWEEN-SHOTS one, which is where the mechanic stops
    /// being a differently-spelled reload.
    pub battery: Option<crate::model::Battery>,
    /// SECONDARY IRRADIATE'S ECHO, TIMES THIS — a MEASURED coefficient with no
    /// explanation behind it. 1.0 for every entry in the roster but one; see
    /// [`crate::data::weapons::WeaponSpec::echo_multiplier`].
    pub echo_multiplier: f64,
    /// A BURST trigger's modded shape — see [`crate::model::BurstSpec`].
    pub burst: Option<crate::model::BurstSpec>,
    /// Whether the weapon's Frenzy passive is equipped (Dual Toxocyst base
    /// form). Wired: fire-rate x2.5 on true headshots (3 s, refreshable).
    /// NOT yet wired: +100% Toxin injection (needs the element layer) and
    /// ammo efficiency (ammo is infinite here anyway).
    pub frenzy: bool,
    /// Stats an equipped mod has LOCKED at the weapon's default — the panel's
    /// [`crate::build::loadout::ResolvedPanel::locked`], carried in because the panel's
    /// arithmetic is not the whole of the stat.
    ///
    /// "Equipping this mod will set weapon's Fire Rate to its default ignoring
    /// other bonuses, even negative effects" (wiki, the Cannonades); the Acuity
    /// pair says it of Multishot. `resolve` handles what it can see; the live
    /// sources are HERE — an arcane's multishot stacks and the Frenzy passive's
    /// x2.5 — and a lock that stopped at the mod bucket left them paying.
    pub locked_stats: Vec<&'static str>,
    /// Buff-lock settings (see [`LockMode`]).
    pub locked_buffs: Vec<BuffLock>,
    /// The real Incarnon two-form cycle; `None` = single-phase run.
    pub cycle: Option<IncarnonCycle>,
    /// The RADIAL (AoE) attack part, fired by every projectile that lands
    /// (Laetum Incarnon: 300 Radiation beside the 100 Impact direct hit).
    /// The directly-hit enemy takes both (MECHANICS §7). It carries its own
    /// crit stats, never takes a body-part multiplier and never feeds
    /// Condition Overload.
    ///
    /// It is resolved as a SEPARATE damage instance — wiki (Laetum):
    /// "Initial hit and explosion apply status separately" — so it rolls
    /// its own crit tier and draws its own procs from its own damage
    /// vector. Those procs land on the same target and therefore DO feed
    /// Condition Overload on subsequent direct hits.
    pub radial: Option<crate::build::loadout::ResolvedRadial>,
    /// THE BOMBLETS this attack's explosion throws out, when it throws any —
    /// see [`crate::data::weapons::ClusterSpec`]. Each one is TWO more instances
    /// on top of the explosion, resolved where the explosion was.
    pub cluster: Option<crate::build::loadout::ResolvedCluster>,
    /// DIRECT-hit damage falloff, when this attack lists one. Read against the
    /// distance the shot travelled; `None` = full damage wherever it lands.
    ///
    /// The DIRECT part only. The explosion has a falloff of its own and it is
    /// measured from the EPICENTRE, which — for a projectile that hit the
    /// target — is on the target, so the radial keeps taking full damage and
    /// its `unmodeled:` line still says so.
    pub falloff: Option<crate::model::Falloff>,
    /// THE CONE this attack fires into, accuracy mods applied — what decides
    /// whether a pellet lands on the target or beside it. `None` = this entry's
    /// spread is not transcribed, so nothing of it may miss.
    pub spread: Option<crate::model::Spread>,
    /// The LINGERING FIELD every landed projectile leaves (Torid's Toxin
    /// cloud). A third kind of attack part: it persists and TICKS instead of
    /// landing once, and each tick is a full damage instance — own crit roll,
    /// own status draw ("Toxin clouds can proc Hunter Munitions on each tick
    /// of damage"), the weapon's mod buckets, and Condition Overload live off
    /// the target's current status count. MECHANICS §7.
    pub lingering: Option<crate::build::loadout::ResolvedLingering>,
    /// CONTINUOUS (beam) weapon: `fire_rate` is ticks per second, and multishot
    /// beams on one target MERGE into a single damage instance.
    pub continuous: bool,
    /// Renewed Horror: what a reload-from-EMPTY does to the NEXT shot's field
    /// duration (1.0 = none). ✅ measured x2 (M13).
    pub field_duration_on_empty_reload: f64,
    /// Final Fusillade: FLAT multishot added on the magazine's last round only
    /// (0.0 = none). Base form only — the evolution loader already dropped it
    /// on a charge-backed form, so this field just carries what survived.
    pub multishot_on_last_round: f64,
    /// The same window in the BASE bracket — see
    /// [`crate::build::loadout::ResolvedPanel::base_multishot_on_last_round`]. It is
    /// carried separately rather than folded into the panel's `multishot`
    /// because it is conditional on the magazine position, which only the sim
    /// can evaluate.
    pub base_multishot_on_last_round: f64,
    /// Plentiful Mayhem: +v damage on multishot-GENERATED projectiles, and
    /// multishot spends ammo to make them (0.0 = none). See `run_once` for the
    /// two per-form rules this drives.
    pub multishot_ammo_bonus: f64,
    /// Evolution headshot-damage bonus (Caput Mortuum) — joins the
    /// headshot bracket. Direct hits only; a radial never headshots.
    pub headshot_damage_bonus: f64,
    /// The weapon's innate headshot bonus MULTIPLIES the additive bracket
    /// rather than joining it (wiki, Cernos Prime — a per-weapon anomaly).
    pub headshot_bonus_multiplicative: bool,
    /// Devouring Attrition: (chance, bonus) rolled on every instance that
    /// did NOT crit — its own multiplier, on the direct hit AND the radial.
    pub noncrit_bonus: Option<(f64, f64)>,
    /// Overwhelming Attrition: a hit that neither crits nor applies a
    /// status grants a stack worth `+per_stack` damage; on timeout ONE
    /// stack drops and the timer resets. The buff multiplies subsequent
    /// instances, the radial part included.
    /// Every stacking buff this build grants, keyed by its own id — see
    /// [`crate::model::StackingBuff`]. The roster, the config reader and the
    /// sampler all walk THIS, which is what stops a buff from existing in one
    /// of them and not the others.
    pub stacking_buffs: Vec<crate::model::StackingBuff>,
    /// Magazine size; when it runs dry a reload (below) blocks firing.
    pub magazine_size: f64,
    pub reload_seconds: f64,
    /// Default: infinite reserve ammo. Toggle off to
    /// simulate finite reserves - firing stops when magazine + reserve are
    /// both dry (DoTs keep ticking).
    pub infinite_reserve: bool,
    /// Ammo a shot COSTS (a beam tick included). 1.0 for almost everything;
    /// the wiki states 0.5 per trace for a beam and 10 for the Larkspur
    /// Prime's alt-fire. Multiplies the magazine spend, so it changes how
    /// often a weapon reloads even when the reserve is infinite.
    pub ammo_cost: f64,
    /// Reserve pool, consumed by reloads when `infinite_reserve` is off.
    pub reserve_ammo: f64,
    /// DO THE BODIES DROP AMMO? The fight's own switch (`engine::ammo` rolls
    /// it per kill). It decides nothing while `infinite_reserve` is on, which
    /// is the state the rulers are scored under.
    pub ammo_drops: bool,
    /// Rounds one pickup gives this weapon — `ResolvedPanel::ammo_pickup`.
    pub ammo_pickup: f64,
    /// A mutation mod's share of the above for the OTHER class's packs.
    pub ammo_conversion: f64,
    /// HOW FAR A PACK IS COLLECTED FROM, in metres, measured from where the
    /// body fell. Infinite by default, which is the arena this engine has:
    /// the Tenno does not walk (docs/UNMODELLED.md), so a finite reach is a
    /// wall rather than a delay. The game's own numbers are 3 m on foot and
    /// 13.5 m with a maxed Vacuum or Fetch.
    pub pickup_range_m: f64,
    /// IS THIS A LANDSCAPE? Open-world bodies drop more (`engine::ammo`).
    pub landscape: bool,
    /// Which class this weapon's reserve takes. `None` on a weapon outside the
    /// two classes a body drops (an Arch-Gun takes HEAVY, whose drop is a
    /// per-enemy table this engine does not model).
    pub ammo_class: Option<crate::rules::ammo::Pickup>,
    /// Whether BuffBar ammo efficiency (Frenzy's +100%) reduces consumption.
    /// False for charge-backed magazines (Incarnon) - they are outside the
    /// ammo economy entirely.
    pub ammo_efficiency_applies: bool,
    /// Multishot: pellets per trigger pull = floor + fractional chance
    /// (wiki Multishot). Each pellet is an independent damage instance
    /// (own crit roll, own part, own status roll); ammo cost and Hit
    /// events stay per pull (hitscan pellets are not separate Hits).
    pub multishot: f64,
    /// UNMODDED pellet count — the base a relative arcane multishot buff
    /// (Conjunction Voltage) multiplies live.
    pub base_multishot: f64,
    /// Σ base-damage bonuses on the panel — needed live when CO joins
    /// this bucket (the vector already includes it; only the CO ratio
    /// reads it).
    pub base_damage_bonus: f64,
    /// Condition Overload payload (assumed-max Σ per_stack × stacks),
    /// applied per `co_behavior`, direct hits only.
    pub co_per_type: f64,
    /// PER-WEAPON CO class: additive with base damage,
    /// an independent multiplier, or inert on this weapon.
    pub co_behavior: crate::model::CoBehavior,
    /// CO base effectiveness (wiki: the CO bonus excludes evolution flat
    /// damage — DT with Fevered = 75/125 = 0.6), paired with the base it is a
    /// share of — see [`crate::model::CoBase`].
    pub co_base: crate::model::CoBase,
    /// See [`crate::model::WeaponBase::unswung_fraction`] — the share of the
    /// vector a stance's, a slam's or a heavy's own multiplier leaves alone.
    pub unswung_fraction: f64,
    /// The evolution's PERMANENT stacked multishot (Fevered Frenzy): its
    /// full contribution is already inside `multishot`; `apply_buff_config`
    /// rescales it by the configured stacks ("evo_multishot"). No live
    /// machinery — the trigger (ability cast) cannot fire in the sim and the
    /// stacks never decay, so the count is static for the whole run.
    pub evo_multishot: Option<crate::build::loadout::EvoMsBuff>,
    /// The evolution's PERMANENT flat base damage (Reified Bane): the vector
    /// already carries it, and `apply_buff_config` scales it back out.
    pub evo_base_damage: Option<crate::build::loadout::EvoBdBuff>,
    /// Live on-kill CO stacks, live per StackSpec (Emergent policy).
    pub co_stack: Option<crate::model::StackSpec>,
    /// Live on-kill multishot stacks, earned from zero.
    pub multishot_stack: Option<crate::model::StackSpec>,
    /// Crosshairs on-headshot buff: absolute crit chance as a timed buff.
    pub crit_chance_on_headshot: Option<crate::model::TimedBuff>,
    /// LEADED GAS: the ELEMENT and the status chance a weak-point hit turns on
    /// together. The element is a share of the modified base added to the
    /// vector and to that element's DoT bracket, which is what makes this card
    /// reach the gas clouds it is named for.
    pub on_weakpoint: Option<crate::model::WeakpointBuff>,
    /// …AND WHAT A CARD SAID ABOUT IT: `(open at t = 0, locked)`. A locked
    /// window never shuts, which is how a reader asks "what is this worth while
    /// it is up" without also asking how often they land a weak point.
    pub weakpoint_open: Option<(bool, bool)>,
    /// Crosshairs on-headshot-kill stacks: absolute cc per stack,
    /// per-stack expiry (FIFO), NOT the lose-one-reset decay.
    pub crit_chance_stack: Option<crate::model::StackSpec>,
    /// (1 + status-damage bonuses): scales every status payload value.
    pub status_damage_multiplier: f64,
    /// (element, 1 + Σ its bonuses) brackets for elemental DoT ticks.
    pub elem_dot_bonus: Vec<(DamageType, f64)>,
    /// (1 + Σ faction bonuses matching THIS target's faction) — the resolved
    /// faction-damage multiplier (System A). 1.0 vs a non-matching / Unknown
    /// faction. Applied ×1 on direct hits, ×2 (squared) on DoT/status ticks
    /// (the wiki "double dip"). Computed in `from_panel` from the panel bucket
    /// + the target's faction.
    pub faction_multiplier: f64,
    /// WARFRAME ABILITY BUFFS running in this fight (`data/abilities/`).
    ///
    /// A property of the FIGHT, not of the build: it arrives on the Arena and
    /// is copied here, so the optimizer scores its candidates under the same
    /// Roar the replay will run (the house rule — the simulator is the truth).
    ///
    /// Each carries its own end time, so they are read AT `t` rather than
    /// folded into a scalar: [`FightParams::faction_at_time`],
    /// [`FightParams::ability_final_at`] and
    /// [`FightParams::ability_element_at`] are the three reads, one per effect
    /// kind, and there is no fourth.
    pub abilities: Vec<crate::data::abilities::ActiveAbility>,
    /// ModifiedBase for status-payload formulas (base × (1 + damage mods),
    /// elemental portions excluded). `None` = the vector total (correct
    /// for purely physical vectors).
    pub dot_modified_base: Option<f64>,
    /// Σ reload-speed bonuses on the panel — needed live when arcane
    /// reload buffs (Merciless r5, Conjunction Voltage stacks) join the
    /// bucket: time = base_reload / (1 + this + arcane additions).
    pub reload_bonus: f64,
    /// Σ LISTED Weak Point damage (Pistol Acuity): +1.5× this on the part
    /// multiplier of true weak points, before the headshot bracket.
    pub weakpoint_damage: f64,
    /// The weapon's own headshot multiplier where it overrules the enemy body
    /// part's — see `data::weapons::WeaponSpec::headshot_multiplier`. `None` on
    /// every weapon whose head is worth what the body part says.
    pub headshot_multiplier: Option<f64>,
    /// ABSOLUTE crit chance added on weak-point pellets only (Acuity).
    pub weakpoint_crit_chance_relative: f64,
    /// King's Gambit: MULTIPLIES a non-weak-point pellet's crit chance.
    /// 1.0 = ordinary; the card's x0 makes a body crit impossible.
    pub bodyshot_crit_chance_multiplier: f64,
    /// Wiseman's Regard, live: `(rate, cap, what the panel already folded in)`,
    /// the first two in POST-MOD units. See the ResolvedPanel field.
    pub derived_status_from_crit: Option<(f64, f64, f64)>,
    /// The mirror (High Ground): same shape, against the live STATUS chance.
    pub derived_crit_from_status: Option<(f64, f64, f64)>,
    /// Galvanic Reload: `(status, chance, rounds)` — a magazine restore rolled
    /// ONCE PER SHOT when the target carries that status.
    /// Double Tap: `(per stack, max stacks, seconds)`. Its OWN multiplier, and
    /// counted per TRIGGER PULL. See `ModEffect::ConsecutiveHitDamage`.
    pub consecutive_hit_damage: Option<(f64, u32, f64)>,
    /// See [`crate::data::weapons::AttackSpec::consecutive_hit_radial_only`].
    pub consecutive_hit_radial_only: bool,
    /// SYNTH CHARGE — see [`crate::model::ModEffect::LastRoundDamage`].
    pub last_round_damage: f64,
    /// THE CHAMBERS — see [`crate::model::ModEffect::FirstRoundDamage`].
    pub first_round_damage: f64,
    pub round_restore_on_status: Option<(DamageType, f64, f64)>,
    /// Exact Penance: the chance a KILL reloads instantly. Rolled off the kill
    /// COUNTER, so a status kill counts — which the card requires.
    pub instant_reload_on_kill: Option<f64>,
    /// Resonant Restore: `(per stack, max stacks)` — the magazine GROWS on each
    /// reload from empty, up to the cap. `per_stack` arrives already scaled by
    /// the magazine mods.
    pub magazine_growth_on_empty_reload: Option<(f64, u32)>,
    /// Sharpened Bullets (Emergent): ABSOLUTE crit-damage add as a timed buff
    /// (starts inactive), granted/refreshed on every kill.
    pub crit_damage_on_kill: Option<crate::model::TimedBuff>,
    /// Pressurized Magazine (Emergent): ABSOLUTE fire-rate add as a timed buff
    /// (starts inactive), granted on every reload.
    pub fire_rate_on_reload: Option<crate::model::TimedBuff>,
    /// Deadly Efficiency: a RELATIVE base-damage bonus whose window opens when
    /// the reload COMPLETES, not when the magazine empties.
    pub base_damage_on_reload: Option<crate::model::TimedBuff>,
    /// EXIMUS ADVANTAGE: a RELATIVE base-damage bonus whose window is opened
    /// by a weak-point hit on an EXIMUS and refreshed by the next one. The
    /// target-side half of the trigger is read HERE rather than at `resolve`,
    /// because the panel has no target to ask — see
    /// [`crate::model::ModEffect::OnEximusWeakpointDamage`].
    pub base_damage_on_eximus_weakpoint: Option<crate::model::TimedBuff>,
    /// ACID SHELLS: the corpse explosion every kill by this weapon sets off —
    /// see [`crate::model::AcidShells`]. It rides on the params rather than
    /// on the target, because it is a fact about the BUILD that a death reads.
    pub acid_shells: Option<crate::model::AcidShells>,
    /// HATA-SATYA: relative crit chance per HIT, and the CEILING ON WHAT THE
    /// PILE IS WORTH rather than on how deep it gets — see
    /// [`crate::model::CritPerHit`]. The pile has no clock — a RELOAD is what
    /// takes it — so it is a rate and a cap here rather than a `TimedBuff`, the
    /// same shape `crit_chance_per_tendril` carries one field down.
    pub crit_chance_per_hit: Option<crate::model::CritPerHit>,
    // ---- MELEE: the combo counter and what reads it ----------------------
    //
    // ONE COUNTER, THREE READERS, and they want opposite things from it. A
    // HEAVY form spends it as a damage multiplier; Blood Rush and Weeping
    // Wounds read it as a bracket term and never spend it. That is why the
    // seven melee forms are seven builds rather than seven animations.
    /// The swings this form loops — see [`crate::model::ComboHit`].
    /// Empty on every gun, and its emptiness is what keeps this loop unchanged
    /// for them.
    pub combo_script: Vec<crate::model::ComboHit>,
    /// `FT^(n-1)`, the share a swing has left for the n-th body it reaches.
    /// `None` on anything that is not a melee swing.
    pub follow_through: Option<f64>,
    /// THE WEAPON'S OWN SLAM, fired by a combo swing that ends on one — three
    /// of Crushing Ruin's four combos do. `None` everywhere else.
    pub slam: Option<crate::build::loadout::ResolvedRadial>,
    /// THE CLASS'S HEAVY ATTACK, on every melee form — what a TENNOKAI swing
    /// fires when the window is open on a light combo.
    pub heavy: Option<crate::model::HeavyAttack>,
    /// TENNOKAI, resolved. `enabled` false on every build carrying none of its
    /// seven cards, which is the game's own answer.
    pub tennokai: crate::model::Tennokai,
    /// Does a swing SPEND the combo counter? True on the two heavy forms.
    pub spends_combo: bool,
    /// SECONDS THE COUNTER SURVIVES with nothing added to it. 5.0 on almost
    /// every melee weapon; the wiki names the handful that differ (Guandao
    /// Prime 6, Pulmonars 9, Vitrica 10) and the mods that extend it.
    pub combo_duration_seconds: f64,
    /// …AND WHETHER IT IS STOPPED ALTOGETHER — see
    /// [`crate::build::loadout::ResolvedWeapon::combo_frozen`].
    pub combo_frozen: bool,
    /// THE FLOOR THE COUNTER RETURNS TO, in points.
    ///
    /// *"Heavy attacks spend initial combo, which regenerates at a rate of 40
    /// combo points per second"* (wiki, Melee Combo). It is the whole of the
    /// pure-heavy build: a Magistar in Incarnon Form carries +30, which refills
    /// in 0.75 s against a 1.07 s cycle, so every heavy lands at 2x rather
    /// than at 1x.
    pub initial_combo: f64,
    /// Fraction of the counter a heavy attack does NOT spend.
    ///
    /// *"40% heavy attack efficiency will change the amount spent to 60% combo
    /// points"*, *"stacks additively and is capped at 90%"* (wiki, Melee
    /// Combo) — so this is clamped to 0.9 where it is resolved, not here.
    pub heavy_attack_efficiency: f64,
    /// BLOOD RUSH: `crit = base x [1 + mods + this x (combo - 1)] + flat`.
    ///
    /// The bracket is the one Point Strike is already in — the wiki writes the
    /// formula that way itself — so this joins `crit_chance_relative` rather
    /// than getting a layer of its own.
    pub crit_chance_per_combo: f64,
    /// WEEPING WOUNDS, the same shape on the status side:
    /// `status = base x [1 + mods + this x (combo - 1)]`.
    pub status_chance_per_combo: f64,
    /// CHANCE OF AN EXTRA COMBO POINT per landed hit (Quickening, True
    /// Punishment, Enduring Strike). Each whole 1.0 repeats the hit's points;
    /// the rest rolls for one point per base point (MEASUREMENTS M96, M97).
    pub combo_count_chance: f64,
    /// …AND WHAT A LIFTED TARGET ADDS TO IT (Enduring Strike), plus the status
    /// bracket's own Lifted card (Enduring Affliction). A CONDITION ABOUT THE
    /// TARGET IS SIMULATED: `Lifted` is a status this engine tracks, forced by
    /// every heavy slam and by a heavy attack.
    pub combo_count_chance_on_lifted: f64,
    /// Chance to Gain Combo Count — see `build::loadout::ResolvedPanel::combo_gain_chance`.
    pub combo_gain_chance: f64,
    /// COMBO POINTS PER BODY THE SLAM REACHED (Shockwave Synergy), before the
    /// combo count chance that scales them. Zero on every other weapon.
    pub combo_count_on_slam_hit: f64,
    pub status_chance_on_lifted: f64,
    /// `+X%` on a HEAVY attack alone (Killing Blow). Read only where
    /// `spends_combo` is true, which is what "heavy attack" means here.
    pub heavy_attack_damage: f64,
    /// `+X%` on a SLAM alone (Seismic Wave). Read only where the form's
    /// explosion is `BlastKind::Slam`, which is what "slam" means here.
    pub slam_damage: f64,
    /// The stacks Hata-Satya's card OPENS with, and whether an event may take
    /// them — the same two knobs the tendrils carry, and for the same reason:
    /// a pile that costs hits is unmeasurable against a target that dies before
    /// it builds, so the card is where a player states what they walk in with.
    pub crit_chance_per_hit_initial_stacks: u32,
    pub crit_chance_per_hit_held: bool,
    /// READY RETALIATION's window: STARTING a reload from empty opens
    /// `duration` seconds of `value` extra reload speed, and the reload that
    /// opened it is the first thing that spends it — the trigger is the reload
    /// ACTION, not its completion.
    ///
    /// It reaches the transmute animations too, in BOTH directions. The
    /// Phenmor's page says it "does not affect transition from Incarnon back to
    /// base form"; that is wrong and nothing here could
    /// implement it anyway — this is an ordinary reload-speed bonus and the
    /// revert is an ordinary reload-speed-scaled animation.
    /// READY RETALIATION, as a bonus rather than a window (0.0 = none).
    ///
    /// It is scoped to the RELOAD ACTION — it arrives when the reload starts
    /// and is gone when it ends — so there is no expiry to
    /// carry, nothing to lapse mid-reload, and nothing left over afterwards for
    /// a transmute animation to pick up. A reload from empty is simply faster.
    pub rs_on_reload: f64,
    /// JAHU CANTICLE — see
    /// [`crate::model::ModEffect::StripOnKillInRange`]. `(fraction,
    /// radius_m)`; every kill takes that share off the armour of every body
    /// inside the radius OF THE PLAYER.
    pub strip_on_kill_in_range: Option<(f64, f64)>,
    /// FLENSING SPIKES: armour removed per live Puncture status (0.0 = none).
    /// A third strip source beside Corrosive and Heat, multiplying with them.
    pub armor_strip_per_puncture: f64,
    /// EXECUTIONER'S FORTUNE — see [`crate::model::InstantReload`]. Rolled by
    /// the PELLET that headshots, because only there is it known whether the
    /// hit landed in a head and whether it killed.
    pub instant_reload: Option<crate::model::InstantReload>,
    /// LINGERING JUDGEMENT — see [`crate::model::HeadshotStreak`]. Counted
    /// per PELLET that lands in a head, like every other on-hit trigger here.
    pub headshot_streak: Option<crate::model::HeadshotStreak>,
    /// SPITEFUL DEFILEMENT: `(threshold, bonus)` — see
    /// [`crate::build::loadout::ResolvedPanel::crit_damage_below_status_count`].
    pub crit_damage_below_status_count: Option<(u32, f64)>,
    /// A syndicate augment's radial (Gilded Truth grants Truth) — armed by
    /// AFFINITY this weapon earns, fired on its own cooldown.
    pub syndicate_radial: Option<crate::data::syndicates::SyndicateDef>,
    /// Where a continuous weapon's damage ramp STARTS, as a fraction of full.
    /// 0.20 "for most weapons" (wiki); Phantasma Prime is 0.15.
    pub beam_ramp_floor: f64,
    /// Does this weapon apply MICROWAVE? See [`DebuffState::microwave`].
    pub applies_microwave: bool,
    /// See `data::weapons::WeaponSpec::independent_procs` — status effects this
    /// attack lands on its own, outside the damage-type draw.
    pub independent_procs: &'static [&'static str],
    /// ONE RESOLVED VECTOR PER PROJECTILE, `(direct, radial)`, for a weapon
    /// whose missiles carry different innate elements. EMPTY on every other
    /// weapon, and then the loop reads `damage` as it always did.
    pub pellet_damage: Vec<(crate::rules::damage::DamageVector, crate::rules::damage::DamageVector)>,
    /// See `data::weapons::AttackSpec::multishot_adds_damage`.
    pub multishot_adds_damage: bool,
    /// THE SHOT COMBO COUNTER, or `None` — which is what a sniper fired from
    /// the hip already resolved to, so nothing here asks about aiming.
    pub sniper_combo: Option<crate::model::SniperCombo>,
    /// The counter the run OPENS with. Seeded like every other stack count,
    /// and it matters more than most: the counter costs LANDING HITS and a
    /// sniper fires slowly, so a fight short enough to be worth measuring can
    /// end before the multiplier a player actually plays at ever appears.
    pub combo_initial: u32,
    /// ...and the card's "no timeout": the counter never decays.
    pub combo_held: bool,
    /// The Ocucor's tendril cap (0 = no tendrils). Their own damage is not
    /// modelled and should not be — see `data::weapons::TendrilSpec`; the COUNT
    /// is what Sentient Surge reads.
    pub tendril_max: u32,
    /// How far a tendril reaches, and how far off the reticle it will take a
    /// body — see [`crate::data::weapons::TendrilSpec`].
    /// THE AIMED BODY'S NAME (`arena::Arena::target_id`) — every other body's
    /// is on its own `formation::FoeSpec`.
    pub target_id: String,
    /// PUNCH-THROUGH DEPTH in metres of material — innate plus mods, and 0 on
    /// an attack that cannot use it. See [`crate::rules::space::BODY_MATERIAL_M`].
    pub punch_through_m: f64,
    /// How wide the projectile is — see `build::loadout::WeaponBase::projectile_width_m`.
    pub projectile_width_m: f64,
    /// HOW FAR THIS WEAPON REACHES, metres — `INFINITY` when it declares none.
    /// See [`crate::data::weapons::AttackSpec::range_m`].
    pub range_m: f64,
    pub tendril_range_m: f64,
    pub tendril_acquire_deg: f64,
    /// Sentient Surge's crit chance per ACTIVE tendril, relative to the
    /// unmodded base (it joins the crit-chance bucket).
    pub crit_chance_per_tendril: f64,
    /// ...and its status half, same bucket.
    pub sc_per_tendril: f64,
    /// The tendrils the run OPENS with — Sentient Surge's buff card, seeded
    /// exactly like every other buff's stack count. Without it the mod is
    /// unmeasurable in the fights it is played in: a tendril costs a kill and
    /// a reload takes every one back, so at a level where kills are slow the
    /// weapon's only augment contributes nothing and there was no way to say
    /// otherwise.
    pub tendrils_initial: u32,
    /// ...and the card's "no timeout". A tendril has no clock — what ENDS it
    /// is the magazine event — so locking it means that event no longer
    /// clears them. Same reading as everywhere else: the count still starts
    /// where the card sets it and still climbs on every kill.
    pub tendrils_held: bool,
    /// ...and the fraction of the magazine a kill puts back.
    pub magazine_refill_on_kill: f64,
    /// GOTVA PRIME'S PASSIVE: a pellet that lands a status has `chance` to arm
    /// the NEXT landing pellet's crit chance to `crit_chance`, exactly — the
    /// modded value and every crit bonus are ignored, because the card says
    /// "Set Critical Chance ignores all other modifiers".
    pub super_crit_on_status: Option<crate::model::SuperCritSpec>,
    /// See `build::loadout::WeaponBase::weakpoint_stacks` — the Knell's Death Knell.
    pub weakpoint_stacks: Option<crate::model::WeakpointStacksSpec>,
    /// See `build::loadout::WeaponBase::spawn_on_kill` — the Ballistica's ghosts.
    pub spawn_on_kill: Option<crate::model::SpawnOnKillSpec>,
    /// Pyrana Prime's second gun — see [`crate::model::KillStreakSummonSpec`].
    pub kill_streak_summon: Option<crate::model::KillStreakSummonSpec>,
    /// ...and its card: whether it is already up when the fight opens.
    pub kill_streak_summon_opens_active: bool,
    /// ...and the streak's card: the kills already banked when it opens.
    pub kill_streak_opens_at: u32,
    /// Hemorrhage's status-conversion roll (per damage instance, max one).
    pub proc_conversion: Option<crate::build::loadout::ProcConv>,
    /// The equipped secondary arcane, resolved at its rank from
    /// data/arcanes/secondary (fixed equipment per scenario; the optimizer
    /// compares scenarios per arcane). `ArcaneFx::none()` = empty slot.
    pub arcane: ArcaneFx,
    /// Primary Compression's damage bonus when this weapon's row `multiplies`
    /// (1.0 = none) — a FINAL multiplier on the instance, the same slot
    /// Secondary Surge occupies: *"the damage bonus is multiplicative to other
    /// damage bonus sources"*. An `adds` row never reaches here; it is already
    /// inside the base-damage bucket the panel resolved.
    ///
    /// PER FORM. The Torid's cloud pays +240% and its Incarnon beam pays
    /// nothing, so the cycle's two `FightParams` disagree on purpose.
    pub compression_multiplier: f64,
    /// The same arcane's `adds` row: a flat addition to the base-damage
    /// bracket, beside a live buff's. Per form, for the same reason.
    pub compression_base_damage: f64,
    /// See [`crate::build::loadout::ResolvedPanel::base_damage_below_half_health`]. Per ATTACK
    /// PART, like every other bracket here, so a radial that the catalog
    /// exempts can carry a different number than the direct hit.
    pub base_damage_below_half_health: f64,
    /// See [`crate::build::loadout::ResolvedPanel::crit_chance_on_undamaged`].
    pub crit_chance_on_undamaged: f64,
    /// See [`crate::build::loadout::ResolvedPanel::crit_damage_on_undamaged`].
    pub crit_damage_on_undamaged: f64,
    /// Secondary Enervate's stack count at t = 0. Its own field because the
    /// ramp lives in a PERK rather than in `arcane.buffs`, so the ordinary
    /// per-buff seeding never reached it — the arcane simply always started
    /// from nothing, with no card to say so.
    ///
    /// UNCAPPED, like the mechanic: a hit adds a stack with no ceiling until a
    /// big crit wipes the pile. There is no maximum to clamp to and inventing
    /// one would be the model disagreeing with the card.
    pub enervate_stacks: u32,
    /// MELEE INFLUENCE'S WINDOW, when the reader has set it rather than earned
    /// it — `Some(end)` opens it at the start of the fight, and infinity is the
    /// lock. `None` leaves the roll to do its own work.
    pub influence_open: Option<f64>,
    /// RAGE AS THE READER SET IT — the share the fight opens at, and whether
    /// the meter is held. `None` builds it from zero (`crate::data::rage`).
    pub rage_open: Option<(f64, bool)>,
    pub body_parts: Vec<BodyPart>,
    /// The TARGET — one of the fight's two actors.
    pub target: TargetParams,
    /// THE REST OF THE FORMATION — see [`crate::arena::Arena::others`]. Empty
    /// for every fight this engine has run, and nothing below reads it when it
    /// is.
    pub others: Vec<crate::formation::FoeSpec>,
    /// WHERE THE WEAPON POINTS — see [`crate::arena::Arena::aim_at`]. `None`
    /// is "at the target", and every fight this engine ran before a formation
    /// existed is that.
    pub aim_at: Option<crate::rules::space::Vec2>,
    /// The beam's own geometry, when this attack is one: the damage radius that
    /// SEEDS the chains and the chain's three constants. `None` for everything
    /// that is not a chaining beam, which is the whole roster but one form.
    pub beam: Option<crate::model::BeamGeometry>,
    /// A PROJECTILE THAT BOUNCES — the Latron family's Incarnon form. Every
    /// bounce is this attack's collision and this attack's explosion arriving
    /// again, at a body it has not hit yet.
    pub ricochet: Option<crate::model::Ricochet>,
    /// See [`crate::data::weapons::AttackSpec::unaimed_headshot_chance`] — this
    /// attack is not pointed at anything, so where each of its instances lands
    /// is a flat chance of its own rather than the scenario's `headshot_pct`.
    pub unaimed_headshot_chance: Option<f64>,
    /// See [`crate::build::loadout::ResolvedPanel::windup_seconds`] — how long after
    /// the trigger a round actually leaves.
    pub windup_seconds: f64,
    /// See [`crate::data::weapons::AttackSpec::no_magazine`]. The loop tops the
    /// magazine up silently instead of reloading, so a reload never happens and
    /// nothing keyed to one ever fires.
    pub no_magazine: bool,
    /// A DEPLOYED ORB'S geometry and clock — see [`crate::build::loadout::ResolvedOrb`].
    ///
    /// `Some` changes what a SHOT IS: it deploys rather than arrives, so the
    /// pellet loop settles no collision and no explosion, and one orb goes out
    /// however much multishot is on the build.
    pub orb: Option<crate::build::loadout::ResolvedOrb>,
    /// WHAT ONE OF ITS STRIKES DEALS — the attack's own hit, in the shape a
    /// timed instance is resolved from. Built in [`Self::from_panel`] rather
    /// than declared in the data, because it IS the attack's `damage:` and
    /// writing it twice is how the two come to disagree.
    pub orb_strike: Option<crate::build::loadout::ResolvedLingering>,
    /// …AND WHAT ITS FUSE ENDS IN: the attack's own `radial:`, in the same
    /// shape, fired from wherever the orb had got to.
    pub orb_blast: Option<crate::build::loadout::ResolvedLingering>,
    /// THE RECHARGE METER THAT GATES THE ORB — see
    /// [`crate::build::loadout::ResolvedMeter`].
    ///
    /// `Some` moves the throw off the TRIGGER and onto the clock: the shot loop
    /// stops deploying, and an orb goes out whenever the meter fills. That is
    /// the whole difference between "what this form is worth if you could hold
    /// it" — which is what this weapon reported before the meter existed, and
    /// an enormous overstatement — and what it is worth in a fight.
    pub meter: Option<crate::build::loadout::ResolvedMeter>,
    /// HOW BIG THE SQUAD IS, for the ammo drop table — see [`crate::rules::ammo`].
    /// One, because this arena has one player.
    pub squad_size: u32,
    /// The TENNO — the other one. Who is holding this weapon, and what they
    /// are doing: `resolve` has already asked its state which conditional mods
    /// pay, and the arcanes that scale off Warframe armor or energy read its
    /// stats. It rides on the params rather than being resolved away so the
    /// fight can be replayed, reported and shared as what it was: somebody,
    /// shooting somebody.
    pub tenno: crate::data::tenno::Tenno,
    /// WHERE THE TWO OF THEM STAND — straight off the arena, in metres
    /// (`crate::rules::space`). Points rather than the distance between them, so that
    /// the thing a damage instance asks — "how far did this travel to get where
    /// it went off" — keeps the same shape when the answer is no longer always
    /// the target's position (an explosion's epicentre, a second body).
    pub player_at: crate::rules::space::Vec2,
    pub target_at: crate::rules::space::Vec2,
    pub duration_seconds: f64,
}
