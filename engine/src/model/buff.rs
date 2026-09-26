// SPDX-License-Identifier: AGPL-3.0-or-later
//! WHAT A BUFF IS: the trigger that grants it, what it grants, how it stacks,
//! decays and is cleared, and the gates on the Tenno that open it.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimedBuff {
    /// The ABSOLUTE bonus this buff contributes while active.
    pub value: f64,
    /// Window length; each trigger refreshes it to `now + duration`.
    /// [`NO_TIMEOUT`] when the buff card is locked, so the window a trigger
    /// opens never closes again.
    pub duration: f64,
    /// Active at t = 0? (per-buff seed — crit_chance_on_headshot starts active, the
    /// on-kill/on-reload buffs start inactive under today's defaults).
    pub initial_active: bool,
}

/// ONE GRANT THE PLAYER'S STATE GATES, carried until `resolve_for` has a Tenno
/// to ask.
///
/// `into_co` is the second sum [`WeaponBase::add_flat_base_damage`] takes, and
/// it rides HERE because the question it answers — [`crate::data::evolutions::
/// EvolutionDef::excludes_co_base`] — is the granting perk's, and the perk is
/// gone by the time the gate is opened. A gated flat add that decided this in
/// `resolve_for` instead would answer it differently from the ungated add on
/// the very same card (MEASUREMENTS M83).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GatedTerm {
    pub gate: TennoCondition,
    pub grant: GatedGrant,
    pub value: f64,
    /// How much of `value` the GunCO term's base grows by — 0 on every grant
    /// that is not [`GatedGrant::FlatBaseDamage`].
    pub into_co: f64,
}

/// A BONUS THE PLAYER'S OWN STATS DECIDE, carried on the panel until a fight
/// says who is holding the gun. `base.gated`'s scaled sibling: that one answers
/// yes or no, this one answers HOW MUCH.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TennoScaledTerm {
    pub stat: crate::model::TennoStat,
    pub above: f64,
    pub unit: f64,
    pub per_unit: f64,
    pub cap: f64,
    pub grant: crate::model::ArcGrant,
}

/// WHAT A GATED PERK GRANTS. One arm per bracket, and each keeps its own —
/// the same rule [`BuffGrant`] follows, and for the same reason: a multishot
/// bonus and a crit-damage one are not interchangeable numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GatedGrant {
    /// Per status type, into the Condition Overload rate.
    ConditionOverload,
    /// METRES OF PUNCH THROUGH, gated on a Warframe stat — Fortress Salvo's
    /// "With Armor Over 450: +4 Punch Through". It joins the same bucket a
    /// punch-through MOD writes to, so the class rule that refuses those on an
    /// area-of-effect attack refuses this too, and for the same reason.
    PunchThrough,
    /// A fraction of the BASE fire rate, additive with fire-rate mods.
    FireRate,
    /// A fraction of the base multishot, into the multishot bucket.
    Multishot,
    /// Added to the weapon's base crit multiplier, so crit-damage mods
    /// multiply it (the card says "Base Critical Damage Multiplier").
    BaseCritDamage,
    /// A fraction, into the projectile-speed indirect bucket.
    ProjectileSpeed,
    /// A fraction, into the ACCURACY indirect bucket — which narrows the cone
    /// a pellet draws inside (`spread`). It was worth nothing until the arena
    /// had a distance, which is why the three Hunter's Mantra cards that grant
    /// it were declared out of scope until 2026-08-15.
    Accuracy,
    /// An ABSOLUTE add to the weapon's base damage, folded exactly as an
    /// ungated one is — [`WeaponBase::add_flat_base_damage`] is the one
    /// implementation, so "+40 with overshields" and a plain "+40" cannot come
    /// out as different panels.
    FlatBaseDamage,
    /// An ABSOLUTE add to the weapon's base MAGAZINE — Lone Gun's "+14 Base
    /// Magazine Capacity" beside its "+40 Base Damage".
    ///
    /// Folded into `magazine_size` BEFORE the magazine mods multiply, which is
    /// where `apply` puts the ungated spelling, so a gated +14 and a plain +14
    /// are the same weapon down to the by-shell reload and a charged form's
    /// ammo ratio. "Increased Base Magazine Capacity does not affect Incarnon
    /// Form" is the same `incarnon.is_none()` guard `apply` uses.
    FlatBaseMagazine,
}

/// WHAT TRIGGERS A STACKING BUFF. One arm per trigger, forever — a new buff
/// that fires on an event already listed here costs no engine code at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuffTrigger {
    /// Overwhelming Attrition: a hit that neither crits nor applies a status.
    PlainHit,
    /// Lethal Rearmament, Headcracker: any weak-point hit.
    Headshot,
    /// Prolific Perforation: a pellet that went THROUGH the body it hit into
    /// another. One per landing pellet, and nothing at all against a lone
    /// target — the mechanic, not an admission.
    PunchThrough,
    /// Stormburst: a hit on a target that ALREADY carries this status. The
    /// condition is on the TARGET, not on the shot — which is why it could not
    /// be expressed as a static `AssumedMaxMultishot` (that would grant the
    /// buff to a build with no Electricity in it at all) and can be expressed
    /// here: a live buff is bumped inside the fight, where the target's
    /// debuffs are in hand.
    HitEnemyWithStatus(crate::rules::damage::DamageType),
    /// Mounting Momentum: a completed RELOAD, not a shot. The first trigger in
    /// this vocabulary that is not something the weapon does to a target — and
    /// the first that grants more than one stack at a time, because what it
    /// grants is one per SHELL loaded (see [`StackingBuff::stacks_per_trigger`]).
    ReloadComplete,
    /// Fresh Havoc, Mauler's Magazine: "On Reload From Empty".
    ///
    /// IN THIS ARENA THAT IS ALMOST — not quite — every reload. The loop only
    /// reloads when it cannot fire, so both reload sites are from-empty by
    /// construction and this trigger would be [`BuffTrigger::ReloadComplete`]
    /// under another name. The exception is what earns it a variant: entering
    /// the Incarnon form FULLY RELOADS the base magazine whether or not it was
    /// empty, and the Soma's card says "Switching to Incarnon Form from empty
    /// will ALSO trigger the buff" — so the transform pays this only when the
    /// base magazine had run out, where `ReloadComplete` would pay every cycle.
    ReloadFromEmpty,
    /// Reaver's Rapture: a COMPLETED BURST, every round of it landing.
    ///
    /// THE MOMENT IS THE LAST ROUND OF THE BURST, so the burst that earns the
    /// stack does not carry it — the next one does. The wiki's own qualifiers
    /// all point the same way and all of them are already true here: "not
    /// affected by multishot or punch through" (it is one count per burst, not
    /// per pellet), "counts object hits", and "activates even if the first hit
    /// of a burst kills the target" — this arena has one target that respawns,
    /// every round hits it, so every completed burst is a full burst hit.
    ///
    /// A magazine that does not divide by the burst count leaves a partial
    /// burst at the end, and a partial burst is not one: the count restarts
    /// with the magazine, so those rounds earn nothing.
    FullBurst,
    /// Crimson Overture: A KILL, wherever it came from.
    ///
    /// Counted off `RunResult::kills` rather than bumped at the six places a
    /// kill can happen (a direct hit, a DoT tick, a field tick, …), because
    /// "remember to also bump it here" is how five of six get done. The loop
    /// already reads kills this way for Sentient Surge's refill and the Ocucur's
    /// tendrils; this is the same mark-and-diff.
    ///
    /// A consequence that matches every other trigger here: the kill is seen at
    /// the START of the next shot, so the shot that earned the stack does not
    /// carry it.
    Kill,
    /// Blazing Barrel: FIRING — the round leaving the barrel, whether or not
    /// it hits and whatever it hits.
    ///
    /// The first trigger here that asks nothing of the target at all, which is
    /// why it is counted where the round is SPENT rather than in the pellet
    /// loop: one shot is one stack however many pellets it threw, and a
    /// shotgun is the family this perk is on.
    ///
    /// THE SHOT THAT EARNS THE STACK DOES NOT CARRY IT — its multishot was
    /// rolled before the round was spent. Same moment Reaver's Rapture uses,
    /// for the same reason.
    Firing,
    /// Paragon Essence: a STATUS EFFECT landing on the target, of any type.
    ///
    /// Distinct from [`BuffTrigger::HitEnemyWithStatus`], which asks what the
    /// target is ALREADY carrying: this one fires on the proc itself, so a
    /// build that lands nothing never earns it however long the fight runs.
    StatusApplied,
    /// Striking Succession: ANY hit — a pellet reaching the target, crit or
    /// not, status or not.
    ///
    /// The permissive sibling of [`BuffTrigger::PlainHit`], which fires only on
    /// a hit that did NEITHER. Two cards, two sentences, and reading one as the
    /// other is worth several stacks a second on a high-crit build.
    Hit,
    /// Well Rehearsed: CONSECUTIVE weak-point hits — the only trigger here that
    /// can be UNDONE by the next shot.
    ///
    /// VERBATIM (wiki, Sybaris Incarnon Genesis): *"The stack resets after
    /// reloading … It also resets after bodyshots"*. So a body hit takes the
    /// whole pile, which is why this cannot be `Headshot` with a clock: the
    /// pile's life is decided by what you hit next, not by time.
    ///
    /// With a headshot rate below 100% the cap is a real target rather than a
    /// given — three in a row at 50% is one run in eight.
    ConsecutiveHeadshot,
}

/// WHAT A STACKING BUFF FEEDS. One arm per grant, and each keeps its own
/// bracket — that is the part which cannot be generalised and must not be:
/// fire rate is additive on the BASE rate, reload speed scales the reload, and
/// base damage joins the damage bucket.
/// WHAT A WEAK-POINT HIT TURNS ON, for a while — Leaded Gas, and the shape any
/// card granting an ELEMENT on a trigger would take.
///
/// THE ELEMENT IS NOT A BUCKET. Every other grant in [`BuffGrant`] is a number
/// added to a bracket; this one adds a share of the modified base to the damage
/// VECTOR and to every tick of that element's status, which is why it is its
/// own type rather than a variant there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeakpointBuff {
    pub element: crate::rules::damage::DamageType,
    /// The share of the modified base the element is worth, and the relative
    /// status chance — ONE number, because the card prints one.
    pub bonus: f64,
    pub duration: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuffGrant {
    BaseDamage,
    ReloadSpeed,
    FireRate,
    /// Stormburst: "+0.4 Multishot", a FLAT add rather than a percentage of
    /// the weapon's base — so it joins `ms_eff` beside Final Fusillade's, not
    /// the multishot BUCKET.
    FlatMultishot,
    /// Blazing Barrel on the Strun family: "+0.05 **Base** Multishot".
    ///
    /// "Base" is the whole difference and the wiki spells out what it buys on
    /// the neighbouring perk (Forceful Finality, "+5 BASE Multishot"): it is
    /// "added before mods, and is thus multiplied by multishot bonuses". So a
    /// build carrying Hell's Chamber gets 0.05 x that bucket a stack, where
    /// [`BuffGrant::FlatMultishot`] would have given it a flat 0.05.
    BaseMultishot,
    /// The RELATIVE crit-damage bucket — Organ Shatter's, not a base grant.
    /// Galvanized Steel's on-kill `+30% Critical Damage` is this one, and the
    /// difference matters: a BASE grant is multiplied by the crit-damage mods
    /// at resolve and this one is one of them.
    CritDamage,
    /// Prolific Perforation: a RELATIVE crit chance, into the bracket its card
    /// names — "additive to other sources … such as Pistol Gambit".
    CritChance,
    /// The RELATIVE status-chance bucket. Galvanized Elementalist's on-kill
    /// `+30% Status Chance`.
    StatusChance,
    /// Blazing Barrel on the Sybaris and the Stug: "+5% Multishot" — a
    /// PERCENTAGE of the weapon's base, which is what every multishot MOD
    /// grants, so it joins their bucket rather than either flat bracket.
    ///
    /// Three brackets for one stat reads like over-modelling until the cards
    /// are laid side by side: the same perk NAME grants a flat base add on one
    /// family and a percentage on another, and they are different numbers on
    /// any build that carries a multishot mod.
    Multishot,
    /// Striking Succession: *"Increase Base Damage by +15"* — an ABSOLUTE add
    /// to the weapon's base, not a share of the base-damage bucket.
    ///
    /// The difference is Serration: a bucket bonus is diluted by every other
    /// bonus in that bucket and a base add is not, because it raises the number
    /// the bucket multiplies. They are the same only on an unmodded weapon.
    ///
    /// `per_stack` therefore CHANGES UNITS at `resolve`, the way
    /// [`BuffGrant::FireRate`]'s does: the flat number on [`WeaponBase`], and
    /// on [`ResolvedPanel`] the share of the bucket that is worth exactly the
    /// same — `flat * (1 + base_damage) / base`, which the mods make a constant. That
    /// keeps ONE live-base-damage path in the sim instead of a second bracket
    /// that would have to be kept in step with it.
    FlatBaseDamage,
    /// Mauler's Magazine: *"Increase Base Critical Damage Multiplier by +1x"* —
    /// the BASE multiplier, so the crit-damage MODS multiply the grant, exactly
    /// as they multiply Prelude of Might's.
    ///
    /// `per_stack` therefore changes units at `resolve` the way
    /// [`BuffGrant::FlatBaseDamage`]'s does — `+1x` leaves as `1 * (1 + cd)`,
    /// the post-mod multiplier it is worth — which keeps ONE crit-damage sum in
    /// the sim rather than a live bracket that would have to be kept in step
    /// with the static one.
    BaseCritDamage,
    /// Sequential Skullbuster: *"On Consecutive Weakpoint Hits: +30% Headshot
    /// Damage"*. Joins the ADDITIVE headshot bracket — the same `(1 + Σ)` that
    /// Primary Deadhead and Lingering Judgement land in, since the wiki lists
    /// every innate headshot source there but Cernos Prime's.
    HeadshotDamage,
    /// Galvanized Reflex: *"On Melee Kill: +20 Initial Combo for 20s. Stacks
    /// up to 4x"* — COMBO POINTS, not a percentage of anything.
    ///
    /// It raises the FLOOR the counter returns to, so on a heavy mode it is
    /// the build: four stacks are +80 points, which is four tiers of heavy
    /// attack that cost nothing to hold and refill at 40 a second after every
    /// swing that spends them.
    InitialCombo,
    /// Spring-Loaded Blade: *"+1 Range for 24s"* — FLAT METRES on the reach,
    /// "additive to other range mods", read at the swing so a stack earned
    /// mid-fight reaches the bodies it brings into range.
    MeleeRange,
}

impl BuffGrant {
    /// THE GRANT'S ONE SPELLING — a data file's `grants:`. A bare stat is its
    /// relative bucket (`multishot` is the mods' +%), `flat_` adds to the
    /// number itself and `base_` to the weapon's base before the mods multiply.
    pub fn id(self) -> &'static str {
        match self {
            BuffGrant::BaseDamage => "base_damage",
            BuffGrant::FlatBaseDamage => "flat_base_damage",
            BuffGrant::BaseMultishot => "base_multishot",
            BuffGrant::Multishot => "multishot",
            BuffGrant::FlatMultishot => "flat_multishot",
            BuffGrant::ReloadSpeed => "reload_speed",
            BuffGrant::FireRate => "fire_rate",
            BuffGrant::BaseCritDamage => "base_crit_damage",
            BuffGrant::CritDamage => "crit_damage",
            BuffGrant::CritChance => "crit_chance",
            BuffGrant::StatusChance => "status_chance",
            BuffGrant::HeadshotDamage => "headshot_damage",
            BuffGrant::InitialCombo => "initial_combo",
            BuffGrant::MeleeRange => "melee_range",
        }
    }

    /// The grant a data file names.
    pub fn from_id(id: &str) -> Option<Self> {
        use BuffGrant as G;
        [
            G::BaseDamage, G::FlatBaseDamage, G::BaseMultishot, G::Multishot, G::FlatMultishot, G::ReloadSpeed,
            G::FireRate, G::BaseCritDamage, G::CritDamage, G::CritChance, G::StatusChance, G::HeadshotDamage,
            G::InitialCombo, G::MeleeRange,
        ]
        .into_iter()
        .find(|g| g.id() == id)
    }
}

impl BuffGrant {
    /// The `disables:` key this grant feeds — the SAME vocabulary a locking
    /// mod writes ("multishot", "fire_rate"). Derived rather than listed: a
    /// lock says "set to its default ignoring other bonuses, even negative
    /// effects" (MEASUREMENTS M30), and a live buff is a bonus like any other,
    /// so the stat is the only thing the two have to agree on. Adding a grant
    /// without answering this is a compile error, which is the point — the
    /// FireRate arm was once the only one that knew about locks, and Stormburst
    /// went on paying +1.2 multishot under Secondary Acuity because Multishot
    /// had simply never been added beside it.
    pub fn locked_stat(self) -> &'static str {
        match self {
            BuffGrant::BaseDamage | BuffGrant::FlatBaseDamage => "base_damage",
            BuffGrant::BaseMultishot | BuffGrant::Multishot => "multishot",
            BuffGrant::ReloadSpeed => "reload_speed",
            BuffGrant::FireRate => "fire_rate",
            BuffGrant::FlatMultishot => "multishot",
            BuffGrant::BaseCritDamage | BuffGrant::CritDamage => "crit_damage",
            BuffGrant::CritChance => "crit_chance",
            BuffGrant::StatusChance => "status_chance",
            BuffGrant::HeadshotDamage => "headshot_damage",
            BuffGrant::InitialCombo => "initial_combo",
            BuffGrant::MeleeRange => "melee_range",
        }
    }

    /// The stat this grant feeds, for a card line. Distinct from
    /// [`Self::locked_stat`] on purpose: that one is a `disables:` KEY and
    /// collapses three multishot brackets into one word, where a reader of a
    /// mod's effect list has to be told WHICH bracket they are buying.
    pub fn label(self) -> &'static str {
        match self {
            BuffGrant::BaseDamage => "Base Damage",
            BuffGrant::FlatBaseDamage => "flat Base Damage",
            BuffGrant::BaseMultishot => "Base Multishot",
            BuffGrant::Multishot => "Multishot",
            BuffGrant::FlatMultishot => "flat Multishot",
            BuffGrant::ReloadSpeed => "Reload Speed",
            BuffGrant::FireRate => "Fire Rate",
            BuffGrant::BaseCritDamage => "Base Critical Damage",
            BuffGrant::CritDamage => "Critical Damage",
            BuffGrant::CritChance => "Crit Chance",
            BuffGrant::StatusChance => "Status Chance",
            BuffGrant::HeadshotDamage => "Headshot Damage",
            BuffGrant::InitialCombo => "Initial Combo",
            BuffGrant::MeleeRange => "Range",
        }
    }
}

impl BuffTrigger {
    /// What EARNS a stack, for a card line — the trigger stated in the words
    /// the mod's own text uses.
    pub fn label(self) -> &'static str {
        match self {
            BuffTrigger::PlainHit => "on a hit that neither crits nor procs",
            BuffTrigger::Headshot => "on a weak-point hit",
            BuffTrigger::PunchThrough => "on a punch-through hit",
            BuffTrigger::HitEnemyWithStatus(_) => "on hitting a target already carrying the status",
            BuffTrigger::ReloadComplete => "on a completed reload",
            BuffTrigger::ReloadFromEmpty => "on a reload from empty",
            BuffTrigger::FullBurst => "on a completed burst",
            BuffTrigger::Kill => "on a kill",
            BuffTrigger::Firing => "on firing",
            BuffTrigger::StatusApplied => "on a status landing",
            BuffTrigger::Hit => "on a hit",
            BuffTrigger::ConsecutiveHeadshot => "on consecutive weak-point hits",
        }
    }
}

/// HOW A STACK LEAVES. `docs/BUFFS.md` has named these three since the buff
/// vocabulary was written; two were implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuffDecay {
    /// The Galvanized family: on timeout ONE stack drops and the timer
    /// RESTARTS, so any new stack refreshes the whole pile. One hit per
    /// window holds every stack.
    LoseOneAndReset,
    /// Each stack carries its OWN clock and expires on it, oldest first —
    /// FIFO. Strictly harsher: holding N stacks needs N hits per window, not
    /// one. Stormburst is the roster's first.
    PerStackExpiry,
    /// THE WHOLE PILE GOES AT ONCE. Split Flights is the roster's first, and
    /// its page states both halves of the rule in consecutive lines:
    /// *"Subsequent hits refresh all stacks' duration"* and *"Stacks expire all
    /// at once after 2 seconds without a hit"*.
    ///
    /// One window separates it from [`Self::LoseOneAndReset`], which is the
    /// same shape until the hits STOP: a full pile of four drains over four
    /// windows there and vanishes in one here. Neither is the harsher of the
    /// two while you keep hitting — which is exactly why the difference has to
    /// be data rather than a default nobody re-read.
    AllAtOnce,
}

/// ONE STACKING BUFF, and one place its identity is written.
///
/// It replaced three structs with identical fields — `PlainHitBuff`,
/// `HeadshotReloadBuff`, `HeadshotFireRateBuff` — that differed only in what
/// triggered them and what they fed. The duplication was not the real cost:
/// each buff's IDENTITY had to be repeated in four places (the evolution's
/// buff card, the sim's replay roster, the config reader, the stack sampler),
/// those four could disagree, and one of them is a control the player sees.
/// Headcracker shipped with the card and none of the other three, so the panel
/// offered a stacks/lock control that did nothing.
///
/// The shape is not invented: `ArcBuffSpec` already carries exactly this
/// (owner + trigger + grant + the four numbers) in a `Vec`, and `ArcRuntime`
/// already bumps by trigger and totals by grant. This is that pattern, applied
/// to the buffs a WEAPON grants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackingBuff {
    /// The buff-card id — the SINGLE source of this buff's identity. The
    /// roster, the config and the sampler all key on it, so they cannot drift.
    pub id: &'static str,
    pub trigger: BuffTrigger,
    pub grant: BuffGrant,
    /// Per stack. For [`BuffGrant::FireRate`] this is a FRACTION of the base
    /// rate on [`WeaponBase`] and the ABSOLUTE rate it is worth on
    /// [`ResolvedPanel`] — `resolve` converts it, because the sim adds it
    /// beside `fire_rate` inside the bracket fire-rate mods live in and
    /// carries no unmodded rate to re-derive it from.
    pub per_stack: f64,
    pub max_stacks: u32,
    pub duration: f64,
    /// Rolled per trigger. 1.0 unless the perk says otherwise — Headcracker's
    /// "This effect has a 50% chance of activating" is the only 0.5 so far.
    pub chance: f64,
    /// See [`BuffDecay`]. Defaults to the Galvanized family because that is
    /// what every buff here did before the third one was implemented.
    pub decay: BuffDecay,
    /// Stacks at t = 0 — the buff card's other knob, the first being
    /// `duration` ([`NO_TIMEOUT`] when it is locked).
    pub initial_stacks: u32,
    /// HOW MANY STACKS ONE TRIGGER GRANTS. One, for every buff written before
    /// Mounting Momentum — and that perk grants one per SHELL LOADED, so the
    /// count is a property of the weapon (its modded magazine) rather than of
    /// the card.
    ///
    /// It is resolved the same way `per_stack` is: `WeaponBase` carries the
    /// RULE (0 = "one per shell") and `resolve` turns it into the number,
    /// because the modded magazine does not exist until the mods are in. That
    /// is also what makes the trade-off real — a magazine mod buys stacks and
    /// pays for them in reload time (`by_round_reload`).
    pub stacks_per_trigger: u32,
    /// Does this buff count SHELLS rather than reloads?
    ///
    /// The same fact `stacks_per_trigger: 0` states on [`WeaponBase`], kept
    /// after `resolve` has turned it into a number — because by then "13" and
    /// "one per shell" are indistinguishable, and the Incarnon route needs the
    /// difference. Entering the form is one reload that loads several shells,
    /// so a shell-counting buff gets one per shell and a reload-counting buff
    /// gets one, and nothing else can tell them apart.
    pub per_shell: bool,
    /// AN EVENT THAT TAKES THE WHOLE PILE, for a buff that has no clock.
    ///
    /// Mounting Momentum is cleared the instant the magazine reaches zero —
    /// not when the reload finishes, and not on a timer. It changes what the perk IS: firing a magazine dry and
    /// reloading it earns one magazine's worth and no more, and the only
    /// way to the 99-stack cap is to keep topping up a magazine that never
    /// empties.
    pub cleared_by: ClearedBy,
    /// THE CARD OPENS AT ITS CAP — a DECISION, not a derivation.
    ///
    /// Every buff in this app opens EARNED at zero, because the modelled fight
    /// is one you have been in a while without contact for the last few seconds
    /// (docs/BUFFS.md). A short, CLOSED list is exempted, because keeping those
    /// up is not something a player thinks about and a fight that opens without
    /// them is the less realistic of the two.
    ///
    /// IT IS A JUDGEMENT ABOUT PLAYING, NOT A CLAIM OFF THE CARD: a card can
    /// say "lasts the mission" for a pile that takes a hundred kills to fill,
    /// which is exactly what should NOT open full. So nothing here derives it —
    /// the shape it would derive from is the DEFAULT for a buff that states
    /// neither, and twenty carry it.
    ///
    /// ONLY THE OPENING MOVES. The trigger still fires, the clear still clears
    /// and the decay still decays, so a buff on this list that DOES get taken
    /// away in a fight is taken away in the sim. And it is a default rather
    /// than a rule: the card's stack stepper sets any count you like, so a
    /// reader who disagrees says so in their own scenario.
    pub card_opens_full: bool,
}

/// See [`StackingBuff::cleared_by`]. A buff with a duration needs none of
/// this — its clock is what ends it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClearedBy {
    /// Only its own clock, which is every buff written before this one.
    #[default]
    Nothing,
    /// The magazine reaching zero.
    EmptyMagazine,
    /// THE MAGAZINE BEING REFILLED — a reload completing, or either Incarnon
    /// transform completing, because swapping either way fully reloads the base
    /// form's magazine (wiki).
    ///
    /// One rule rather than a list of events, and the same one Ready
    /// Retaliation is spent by. Reaver's Rapture states it as three separate
    /// sentences — "resets on Reload", "resets when activating incarnon" — and
    /// they are one fact.
    ///
    /// THE MOMENT IS THE COMPLETION, not the start: a reload that has begun has
    /// not refilled anything yet.
    MagazineRefilled,
    /// A RELOAD — the action, not the refill, and the difference is one event.
    ///
    /// VERBATIM (wiki, Strun Incarnon Genesis, Blazing Barrel): *"resets
    /// entirely upon reloading. Entering Incarnon Form counts as reloading but
    /// exiting does not."* Swapping OUT refills the base form's magazine, so a
    /// buff keyed on the refill dies there — and this one is stated not to.
    ///
    /// Kept as a second variant rather than folded into the one above because
    /// the two disagree on exactly one of the four events that end a magazine,
    /// and picking either as "close enough" is a stack count nobody can
    /// reproduce.
    Reload,
    /// A RELOAD THAT WAS NOT FROM EMPTY — Mauler's Magazine: *"Stacks are shown
    /// as a buff, which are lost when reloading from a partial magazine"*. At
    /// the reload's start, on the same from-empty reading every such card uses.
    PartialReload,
}

/// What event grants/refreshes a stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcTrigger {
    /// Any kill (Secondary Merciless).
    Kill,
    /// Direct-pellet headshot kill (Secondary Deadhead's precision boundary).
    HeadshotKill,
    /// Melee kill — the sim has no melee: the buff starts full (user
    /// setting) and only decays (Secondary Dexterity).
    MeleeKill,
    /// A Heat status this weapon applies (Cascadia Flare).
    HeatStatus,
    /// An Electricity status this weapon applies (Conjunction Voltage).
    ElectricityStatus,
    /// A Toxin status this weapon applies (Primary Blight). Blight is
    /// stricter than the other on-status arcanes — wiki: "stacking the
    /// Blight buff requires the Toxin proc to be inflicted by using the
    /// attached primary weapon" — which is exactly what the sim can see.
    ToxinStatus,
    /// A Cold status this weapon applies (Primary Frostbite).
    ColdStatus,
    /// Nothing grants it — it is simply ON. A Tenno-scaled arcane (Primary
    /// Bulwark, Primary Overcharge) reads a Warframe stat that does not change
    /// during the fight, so its buff starts at its one stack, is pinned there,
    /// and no event has to fire. It rides the buff machinery rather than a new
    /// static bucket because the GRANTS are the same ones the on-kill arcanes
    /// already feed correctly.
    Passive,
    /// A direct-pellet hit on a natural weak point (Primary Crux) — a HIT, not
    /// a kill, and PER PELLET: "Multiple individual pellets from a single shot
    /// (either innate to the weapon or generated via Multishot) can build
    /// stacks" (wiki). Weak spots created by Banshee's Sonar do NOT count,
    /// which is also exactly what `BodyPart::is_head` means here.
    WeakpointHit,
}

/// EVENTS A CARD NAMES THAT NO FIGHT HERE RAISES — an ability cast, a roll, a
/// weapon swap. They are words of the same vocabulary as [`BuffTrigger`]'s, so
/// a file spells them the same way; the card is read at its assumed maximum or
/// stays inert, and says which (docs/UNMODELLED.md).
pub const UNSIMULATED_EVENTS: &[&str] =
    &["equip", "ability_cast", "bullet_jump_land", "swap_consume_combo", "roll", "per_tendril", "puncture_status"];

// ---- THE TRIGGER VOCABULARY -----------------------------------------------
//
// ONE SPELLING per event, and it is this id: a data file's `trigger:`, a
// scenario's switch, a share link and a benchmark all carry it. A file that
// spells one any other way does not load (`every_trigger_in_the_data_is_a_word`).

impl BuffTrigger {
    /// The wire id of a data-declared trigger — what a scenario, a share link and a
    /// benchmark carry.
    ///
    /// EXHAUSTIVE ON PURPOSE — no `_` arm, so a trigger added to [`BuffTrigger`]
    /// cannot compile until it is named here, where a default would leave the next
    /// card with no switch and nothing to notice it by.
    pub fn id(self) -> &'static str {
        match self {
            BuffTrigger::Kill => "kill",
            BuffTrigger::Hit => "hit",
            BuffTrigger::PlainHit => "plain_hit",
            BuffTrigger::Headshot => "headshot",
            BuffTrigger::ConsecutiveHeadshot => "consecutive_headshot",
            BuffTrigger::PunchThrough => "punch_through",
            BuffTrigger::StatusApplied => "status_applied",
            // The element is not part of the id: the condition is "the target
            // already carries this status", and a fight handing out none of them
            // hands out none of any type.
            BuffTrigger::HitEnemyWithStatus(_) => "hit_enemy_with_status",
            BuffTrigger::ReloadComplete => "reload_complete",
            BuffTrigger::ReloadFromEmpty => "reload_from_empty",
            BuffTrigger::FullBurst => "full_burst",
            BuffTrigger::Firing => "firing",
        }
    }

    /// The trigger a data file names. An event carrying a payload
    /// (`HitEnemyWithStatus`) is never named by a file on its own.
    pub fn from_id(id: &str) -> Option<Self> {
        use BuffTrigger as T;
        [
            T::Kill, T::Hit, T::PlainHit, T::Headshot, T::ConsecutiveHeadshot, T::PunchThrough,
            T::StatusApplied, T::ReloadComplete, T::ReloadFromEmpty, T::FullBurst, T::Firing,
        ]
        .into_iter()
        .find(|t| t.id() == id)
    }
}

impl ArcTrigger {
    /// The same for an arcane's own vocabulary. `None` for [`ArcTrigger::Passive`]:
    /// nothing grants it, so no switch may take it away.
    pub fn id(self) -> Option<&'static str> {
        Some(match self {
            ArcTrigger::Kill => "kill",
            ArcTrigger::HeadshotKill => "headshot_kill",
            ArcTrigger::MeleeKill => "melee_kill",
            ArcTrigger::WeakpointHit => "weakpoint_hit",
            ArcTrigger::HeatStatus => "heat_status",
            ArcTrigger::ElectricityStatus => "electricity_status",
            ArcTrigger::ToxinStatus => "toxin_status",
            ArcTrigger::ColdStatus => "cold_status",
            ArcTrigger::Passive => return None,
        })
    }

    /// The trigger an arcane's file names.
    pub fn from_id(id: &str) -> Option<Self> {
        use ArcTrigger as T;
        [
            T::Kill, T::HeadshotKill, T::MeleeKill, T::HeatStatus, T::ElectricityStatus, T::ToxinStatus,
            T::ColdStatus, T::WeakpointHit,
        ]
        .into_iter()
        .find(|t| t.id() == Some(id))
    }
}

impl BuffDecay {
    /// The decay a data file's `decay:` names; absent is the Galvanized
    /// family's, which is what every buff written before the other two did.
    pub fn from_id(id: Option<&str>) -> Self {
        match id {
            Some("per_stack_expiry") => BuffDecay::PerStackExpiry,
            Some("all_at_once") => BuffDecay::AllAtOnce,
            _ => BuffDecay::LoseOneAndReset,
        }
    }
}

impl ClearedBy {
    /// What a data file's `cleared_by:` names; absent is the buff's own clock.
    pub fn from_id(id: Option<&str>) -> Self {
        match id {
            Some("reload") => ClearedBy::Reload,
            Some("magazine_refilled") => ClearedBy::MagazineRefilled,
            Some("empty_magazine") => ClearedBy::EmptyMagazine,
            Some("partial_reload") => ClearedBy::PartialReload,
            _ => ClearedBy::Nothing,
        }
    }
}
