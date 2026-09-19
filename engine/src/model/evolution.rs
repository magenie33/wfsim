// SPDX-License-Identifier: AGPL-3.0-or-later
//! WHAT AN INCARNON EVOLUTION'S CARD SAYS — the loader's vocabulary for an
//! evolution perk, beside [`super::ModEffect`] for a mod and
//! [`super::ArcEffect`] for an arcane.

/// One parsed evolution effect (the loader's vocabulary — kinds with no
/// single-target damage payload load as `Inert` so the evolution still
/// resolves and lists).
#[derive(Debug, Clone, PartialEq)]
pub enum EvoEffect {
    /// Adds to the BASE damage TOTAL, distributed pro-rata across the
    /// vector, BEFORE mods (inside ModifiedBase).
    FlatBaseDamage(f64),
    /// A RELATIVE base-damage bonus — the Pressure Point / Serration bucket.
    ///
    /// Its own variant beside `FlatBaseDamage` because DE writes the two
    /// differently and means different things by them: a gun's Genesis says
    /// `+14 Base Damage` and a melee one says `+100% Melee Damage`, and the
    /// second is a bracket term while the first is a number added to the
    /// weapon's own. The Magistar's Incarnon Form carries both, one in EVO1 and
    /// one in EVO2, which is what makes the distinction unavoidable.
    BaseDamageBonus(f64),
    /// POINTS THE MELEE COMBO COUNTER OPENS AT and returns to.
    InitialCombo(f64),
    /// COMBO POINTS PER BODY A SLAM REACHED (Shockwave Synergy's `+4`).
    ///
    /// THE ONE THING THAT EARNS COMBO ON A HEAVY MODE. A heavy attack adds
    /// nothing to the counter, so a heavy-slam loop can only spend what its
    /// initial-combo floor regenerates — until this, which pays per body in the
    /// slam's radius and turns a crowd into the counter's engine.
    ///
    /// COMBO COUNT CHANCE SCALES IT: *"True Punishment affects Shockwave
    /// Synergy, effectively doubling the Combo Count gain from 4 to 8"* (wiki,
    /// Praedos), so the grant is `value x (1 + chance)` rather than a roll.
    ComboCountOnSlamHit(f64),
    /// METRES OF MELEE REACH (Orokin Reach's `+1.4 Range`).
    MeleeRange(f64),
    /// THE WINDOW A MELEE INCARNON IS ON FOR, and what opens it — *"Reach 6x
    /// Combo and then Heavy Attack to activate Incarnon Form for 180 seconds"*.
    ///
    /// It carries no stat of its own: the tier that states it also states the
    /// numbers, and this is what makes those numbers TIMED rather than the
    /// weapon's. Both halves are optional because two cards can speak — the
    /// Praedos's Swift Transmute lowers the arming to 3x and states no window —
    /// and the merge is the obvious one: the LOWEST arming any selected
    /// evolution asks for, and the window whichever one names it.
    IncarnonWindow { arm_at_combo: Option<f64>, seconds: Option<f64> },
    /// A RELATIVE change to FOLLOW THROUGH — what a second body in the same
    /// swing takes (Crushing Verdict's `+40% Follow Through`).
    FollowThroughBonus(f64),
    /// A RELATIVE change to a SLAM's radius (Seismic Slam's `+100% Slam
    /// Radius`).
    SlamRadiusBonus(f64),
    /// A RELATIVE change to HEAVY ATTACK WIND UP SPEED — the charge before a
    /// heavy swing, which attack speed does not touch.
    HeavyWindUpSpeed(f64),
    /// A PROC THAT ROLLS ANOTHER PROC — Flashing Bleed's `+50% Chance of a
    /// Slash Status Effect on an Impact Status Effect`.
    ///
    /// The same mechanic Hemorrhage carries on the mod side, and the same
    /// runtime: one roll per damage instance, never alongside another `to` proc
    /// in the same instance. It matters on this weapon more than on most —
    /// the Magistar is 80% Impact and all four of its combos force an Impact
    /// proc somewhere in the sequence.
    ProcConversion { from: crate::rules::damage::DamageType, to: crate::rules::damage::DamageType, chance: f64 },
    /// Adds into the BASE crit chance (crit mods multiply the new base).
    FlatBaseCritChance(f64),
    /// A flat addition to BASE multishot — the Braton family's Munitions Grit
    /// is +0.20, and its tier-mate's +60% multishot has nothing to act on
    /// without it. The yaml said so in a comment while loading inert
    /// ("with no multishot source the +60% applies to nothing"), which is the
    /// disclosure working and not a reason to leave it.
    FlatBaseMultishot(f64),
    /// Mounting Momentum: every SHELL loaded is +10% fire rate, and nothing
    /// takes the stacks away short of holstering.
    ///
    /// The first buff in the roster whose per-trigger gain is a weapon stat.
    /// It is what makes the perk a real choice: a magazine mod buys stacks and
    /// pays for them in reload time, and the two only trade off because the
    /// by-round reload is modelled (`WeaponSpec::reload_style`). Implementing
    /// one without the other would have handed the optimizer free fire rate.
    StackingFireRatePerShellReloaded { per_stack: f64, max_stacks: u32 },
    /// Adds into the BASE status chance — the same base-stat layer, so status
    /// mods multiply the new base (Torid's Survivor's Edge and Elemental
    /// Balance both say "Increase Base Status Chance"). NOT the post-mod flat
    /// layer that Elemental Excess occupies.
    FlatBaseStatusChance(f64),
    /// The same layer, but the two FORMS get different numbers. Boar's
    /// Elemental Balance reads "+12% per projectile" and "+96% for Incarnon
    /// Form" as two separate statements, not as a sum — a shotgun's pellet
    /// carries a twelfth of the status a beam tick does, so one number cannot
    /// serve both. Picked by `base.gauge_form.is_some()`, the same gate
    /// `FlatBaseMagazine` uses.
    FlatBaseStatusChanceByForm { base: f64, incarnon: f64 },
    /// Adds into the BASE crit MULTIPLIER (Boar's Critical Parallel: "+0.5x").
    /// Base-stat layer like the crit-chance one above, so crit-damage mods
    /// multiply the new base.
    FlatBaseCritMultiplier(f64),
    /// Flat BASE damage that an empty reload turns on and nothing turns off —
    /// Boar's Reified Bane, "On Reload From Empty: Increase Base Damage by
    /// +14". It is applied UNCONDITIONALLY, i.e. the run is modelled as
    /// holding it from t = 0.
    ///
    /// Held is EXACT rather than an approximation, and the timing is why: the
    /// bonus lands the moment an empty reload BEGINS (measured in game; the
    /// wiki claims the opposite), so there is no gap — the magazine empties,
    /// the reload starts, the buff is already back, and it "lasts indefinitely
    /// until a manual reload is initiated while the magazine is not empty",
    /// which the sim never does.
    ///
    /// **THIS IS THE EXCEPTION, AND THE NAME SAYS SO.** The DEFAULT for a
    /// reload-triggered effect is that it fires when the reload COMPLETES; two
    /// unusual conditions hold together here — the magazine must be EMPTY, and
    /// it fires when the reload STARTS — so the variant stays narrow.
    ///
    /// Its own variant rather than part of `FlatBaseDamage` because it is a
    /// BUFF: `resolve` turns it into an `EvoBdBuff` so the bar can show it and
    /// a card can scale it back out.
    FlatBaseDamageOnEmptyReload(f64),
    /// A handling / mobility / multi-target stat with no single-target damage
    /// payload — recoil, accuracy, punch through, projectile speed, holstered
    /// reload. It COUNTS: the value lands in the panel's `indirect` bucket
    /// beside the mods'. Mods were given this treatment
    /// on 2026-08-01; evolutions were still dropping the number on the
    /// floor.
    Indirect(crate::model::IndirectStat, f64),
    /// Sets the ammo RESERVE outright (Mercenary Chamber: "Increase Base Ammo
    /// Capacity to 195") — a set, not an add, so it cannot ride the additive
    /// indirect bucket.
    AmmoMaxSet(f64),
    /// Adds whole rounds to the BASE magazine, before magazine mods (Torid's
    /// Extended Volley: +9 on a base of 5). Explicitly NOT the Incarnon form's
    /// charge-backed magazine — "Does not apply to Incarnon Form's Magazine" —
    /// which is why it lands on the base entry only.
    FlatBaseMagazine(f64),
    /// Renewed Horror: reloading from EMPTY arms a buff that multiplies the
    /// duration of the NEXT shot's lingering field. ✅ measured (M13): x2, so
    /// that field ticks 20 times instead of 10.
    FieldDurationOnEmptyReload(f64),
    /// A multishot bonus that pays only PAST a distance — Lone Enforcer's
    /// "+25% Multishot if no enemies are within 5m", as `(fraction, metres)`.
    ///
    /// It cannot be a [`GatedGrant`] like the rest: those are opened by the
    /// TENNO's state and `resolve` never sees the arena, while this asks where
    /// the two of them are standing. So it rides the panel and is settled in
    /// `FightParams::from_panel`, which is the one place the build and the
    /// fight are both in scope — the same seam Primary Compression already
    /// uses (the panel brings the metres, the arena brings the answer).
    MultishotBeyondRange { value: f64, metres: f64 },
    /// Final Fusillade: a FLAT multishot add on the last round of the magazine,
    /// BASE FORM ONLY — a charge-backed Incarnon magazine
    /// has no "last shot in magazine" to gate on, so `apply` drops it there.
    /// A flat multishot add on the magazine's last round, and WHICH BRACKET it
    /// lands in — the `bool` is the card's own word "Base".
    ///
    /// The two perks that grant this do not grant the same thing, and the wiki
    /// says so on the row rather than in a general rule:
    ///
    /// - Torid, Final Fusillade: *"+3 Multishot on last shot in magazine"* —
    ///   flat, on top of everything, `false`.
    /// - Burston, Forceful Finality: *"+5 **Base** Multishot on final magazine
    ///   burst"*, with a note attached to that row: *"Multishot bonus is added
    ///   before mods, and is thus multiplied by multishot bonuses"* — `true`.
    ///
    /// The note exists BECAUSE it is unusual, and the difference is not small:
    /// on a Burston Prime carrying Split Chamber and Vigilante Armaments the
    /// same +5 is 11 pellets rather than 5.
    MultishotOnLastRound { value: f64, base: bool },
    /// Flensing Spikes: armour removed per live Puncture status, as a
    /// fraction. A third strip source beside Corrosive and Heat, and the first
    /// that a WEAPON grants rather than a status carrying it.
    ArmorStripPerPunctureStatus(f64),
    /// Reaver's Rapture: +X base damage per COMPLETED BURST, reset when the
    /// magazine is refilled. No duration — it is held until something takes it.
    BaseDamagePerFullBurst { per_stack: f64, max_stacks: u32 },
    /// Plentiful Mayhem: multishot draws its extra rounds from ammo, and the
    /// projectiles it GENERATES deal +v damage as an independent multiplier.
    /// Affects both forms; the sim reads the per-form rule off `continuous`.
    MultishotConsumesAmmo(f64),
    /// A PERMANENT stacking multishot buff (Fevered Frenzy: on-ability-cast
    /// stacks with no timer, cleared only by death — so inside a sim run the
    /// stack count is a static CHOICE, full by default). `total` = the
    /// full-stack bonus (per_stack × max_stacks) that joins the weapon's
    /// buff multishot; `max_stacks` lets the per-buff config rescale it.
    AssumedMaxMultishot { total: f64, max_stacks: u32 },
    /// Unconditional CO rate (Carnage Reign): +v per status TYPE, additive
    /// with mod CO sources. `excludes_evolution_damage`: the GunCO base
    /// excludes evolution flat damage (wiki CO catalog, DT row).
    /// Condition Overload granted by an evolution, and the SPRINT SPEED the
    /// player needs for it. `min_sprint` is 0 when the card states no
    /// condition; the Latron family's Swift Punishment states 1.2.
    ConditionOverload { per_type: f64, min_sprint: f64 },
    /// Fire-rate bonus in the ORDINARY additive bucket — the same one the
    /// fire-rate mods feed, so it SUMS with them (Rapid Wrath).
    /// A FIRE-RATE (melee: ATTACK SPEED) bonus, with the two conditions the
    /// roster's cards state: a sprint floor, and whether the melee weapon has
    /// to be DRAWN — *"With Melee Weapon Equipped"*, which a quick-melee swing
    /// does not satisfy.
    FireRateBonus { value: f64, min_sprint: f64, needs_melee_equipped: bool },
    /// "+X% Damage to enemies below half Health" — a bucket bonus with a
    /// condition on the TARGET rather than on the weapon or the player.
    /// "+X% Damage to enemies below half Health". `excludes_own_flat` is the
    /// Sicarus's note — *"does not take into account the Base Damage increase
    /// from this perk"* — and it is per CARD rather than a rule: the Kunai's
    /// page says the opposite about the same weapon's base increase ("CO-bonus
    /// DOES use base damage increase Evolution"), so applying the correction
    /// everywhere would dock a perk the wiki never docked.
    BaseDamageBelowHalfHealth { rate: f64, excludes_own_flat: bool },
    /// RESONANT RESTORE — "On Reload From Empty: Increase Base Magazine
    /// Capacity by +N. Stacks up to Nx".
    ///
    /// Not a `StackingGrant`, because what it grants is not a term in any
    /// bracket: it is the magazine CAPACITY, which every other line of the sim
    /// loop reads. "BASE" capacity, so the magazine mods multiply each stack.
    MagGrowthOnEmptyReload { per_stack: f64, max_stacks: u32 },
    /// EXACT PENANCE — "On Kill: 50% chance for Instant Reload".
    ///
    /// Distinct from `InstantReloadOnHeadshot` because of the card's own note:
    /// "Kills from status effects can also trigger the effect." That one asks
    /// for a weak-point direct hit; this one asks only that something died, so
    /// it is read off the kill counter.
    InstantReloadOnKill { chance: f64 },
    /// GALVANIC RELOAD — a magazine restore the TARGET's state gates.
    ///
    /// VERBATIM (Strun_Incarnon_Genesis): "On hitting a target affected by an
    /// Electricity status, 40% chance to restore 1 round in the magazine from
    /// ammo pool", with three notes, each of which decides something:
    ///   *The status effect may originate from any source.
    ///   *The bonus can only apply once per enemy hit.
    ///   *The bonus does not affect the Incarnon form.
    /// The second is why it is per SHOT and not per pellet — this is a shotgun
    /// family — and the third is the card's `base_form_only`.
    RoundRestoreOnStatusHit { status: crate::rules::damage::DamageType, chance: f64, rounds: f64 },
    /// KING'S GAMBIT — one bullet, two brackets, and the wiki names both.
    ///
    /// VERBATIM (Sicarus_Incarnon_Genesis): "x0 Critical Chance on Bodyshots,
    /// +150% Critical Chance on Weakpoint Hits", under which:
    ///   *Bodyshot modifier is multiplicative with all sources of Critical
    ///    Chance, effectively making non-headshot critical hits impossible.
    ///   *Weakpoint modifier is additive with mods such as Pistol Gambit
    ///
    /// So `bodyshot_multiplier` MULTIPLIES a body pellet's chance and
    /// `weakpoint_bonus` joins `weakpoint_crit_chance_relative`, the same relative bracket
    /// Pistol Acuity uses. Both are per-PELLET, which is what keeps them out of
    /// the panel's crit chance — and that is what makes the same page's other
    /// note true for free: Wiseman's Regard, which reads "current Critical
    /// Chance", is "**Not** affected by the King's Gambit Evolution II perk".
    CritChanceByBodyPart { bodyshot_multiplier: f64, weakpoint_bonus: f64 },
    /// ONE STAT FROM THE OTHER, capped. `from_crit` says which way round:
    /// Wiseman's Regard reads crit and pays status, High Ground the mirror.
    DerivedStat { from_crit: bool, rate: f64, cap: f64 },
    /// A grant the PLAYER's state switches on — "With Armor Over 450: +80%
    /// Multishot", "With Energy Max Over 700: +1x Base Critical Damage
    /// Multiplier", "With Sprint Speed 1.2 or Higher: +60% Projectile Speed".
    ///
    /// One variant for all of them: the gate and the bracket are both data, so
    /// the next perk that asks about the player is a yaml block.
    GatedByTenno {
        gate: crate::model::TennoCondition,
        grant: crate::model::GatedGrant,
        value: f64,
    },
    /// Vicious Promise: crit chance and crit multiplier while the target has
    /// taken no damage. One variant for both halves, because the card grants
    /// them together and the condition is one sentence.
    CritOnUndamaged { crit_chance: f64, crit_multiplier: f64 },
    /// A RELOAD-SPEED bonus, into the same bucket the mods feed.
    ///
    /// The most common perk in the whole Incarnon set — Rapid Reinforcement is
    /// on 14 guns by docs/INCARNON.md's count, more than any other name — and it
    /// sat inert for all of them because this loader had no arm while the MODS
    /// loader did. One arm removes a slot from half the remaining program, which
    /// is what the intake kept demonstrating four rows at a time.
    ///
    /// UNCONDITIONAL ONLY. Ready Retaliation's "On Reload from Empty: +100%
    /// Reload Speed" is a different perk and stays inert: `condition:` is not
    /// read here, and granting a conditional bonus unconditionally is the one
    /// mistake worse than not granting it.
    ReloadSpeedBonus(f64),
    /// EXECUTIONER'S FORTUNE — a headshot has a chance to fill the magazine
    /// outright, no reload played.
    ///
    /// Two weapons word it two ways and `needs_kill` is the whole difference:
    /// the Furis pair pay on any headshot ("On Headshot: 10% chance for Instant
    /// Reload"), the Phenmor only on one that KILLS ("On Headshot kill: 20%
    /// chance to instant Reload"), which is far rarer against a single target
    /// that has to be worn down.
    InstantReloadOnHeadshot { chance: f64, needs_kill: bool },
    /// LINGERING JUDGEMENT — `hits` headshots inside `within` seconds open
    /// `value` extra headshot damage for `duration`.
    ///
    /// The bonus joins the ADDITIVE headshot bracket: "Headshot damage bonus
    /// stacks additively with Primary Deadhead's headshot damage bonus" (wiki,
    /// supplied measured 2026-08-10). That is the same bucket the arcane
    /// feeds, so the two sum before the bracket is spent rather than
    /// multiplying — which is what makes the perk worth much less on a Deadhead
    /// build than the card's +50% suggests.
    HeadshotDamageOnStreak { hits: u32, within: f64, value: f64, duration: f64 },
    /// SPITEFUL DEFILEMENT — `value` extra crit DAMAGE while the target carries
    /// fewer than `threshold` distinct status types.
    ///
    /// Two clauses decide where it lands and both are the wiki's, verbatim:
    /// "Bonus is added after mods as a flat value" puts it in the same
    /// after-mods bucket Cold's received crit-damage bonus uses, NOT in the
    /// weapon's base crit damage; and "Multiple instances of the same status
    /// effect are not counted separately, e.g. having 5 corrosive and 5
    /// radiation status effects on a target will not disable this buff" makes
    /// the counter DISTINCT TYPES — which is exactly Condition Overload's
    /// bucket, so the two read the same number and cannot disagree.
    ///
    /// It is therefore the anti-CO perk: the third status TYPE turns it off,
    /// and the third status type is where CO starts paying.
    CritDamageBelowStatusCount { threshold: u32, value: f64 },
    /// READY RETALIATION — reload speed, armed by STARTING a reload from empty
    /// and lasting a while after.
    ///
    /// THE TRIGGER IS THE RELOAD ACTION, NOT ITS COMPLETION, so the reload
    /// that arms it is the first thing it speeds up. That one word is most of
    /// the perk's value: on a weapon that always reloads from empty — which is
    /// every weapon in this sim — it behaves like a permanent reload mod rather
    /// than a bonus that has to be caught in time.
    ///
    /// The window still matters for what comes AFTER the reload: a transmute,
    /// or a second reload, inside the remaining seconds.
    ///
    /// An ordinary reload-speed bonus in every other respect. The Phenmor's
    /// page adds *"Can affect transition into Incarnon form with a well-timed
    /// manual reload. Does not affect transition from Incarnon back to base
    /// form."* and the last clause is WRONG: nothing about the buff knows which
    /// direction an animation is going, so the revert takes it too.
    /// Ready Retaliation. NO DURATION: the buff is scoped to the reload action,
    /// arriving when it starts and gone when it ends, so there is no window to
    /// state and nothing that can lapse halfway through.
    ReloadSpeedOnEmptyReload { value: f64 },
    /// Prelude of Might: "With Critical Chance below 40%: Increase Base
    /// Critical Damage Multiplier by +3x", carrying the wiki's note on the same
    /// row — "Condition is affected by the critical chance increase effect of
    /// Puncture status".
    ///
    /// So the condition asks about the crit chance THE HIT HAS, which is
    /// neither of the two things a `condition:` can express: not the Tenno, not
    /// the target, but the panel the mods produced PLUS every live bonus on top
    /// of it — a target-side one included. That is why it is a variant and not
    /// a gate, and why it is settled in two places: `resolve` grants it against
    /// the panel (the optimistic half) and the sim takes it back per shot.
    CritMultiplierBelowCritChance { value: f64, below: f64 },
    /// Headcracker: "On Headshot: +5% Fire Rate for 2s. Stacks up to 10x",
    /// and — from the raw wikitext, which the rendered page's summary drops —
    /// "This effect has a 50% chance of activating."
    StackingFireRateOnHeadshot {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
        chance: f64,
        /// HEADCRACKER IS FIFO, and it is data rather than a constant because
        /// this is the second perk to want it and neither shape is the rule.
        /// See [`crate::model::BuffDecay`].
        decay: crate::model::BuffDecay,
    },
    /// Stormburst: "On hitting an enemy affected by Electricity: +0.4
    /// Multishot for 2s. Stacks up to 3x."
    /// Blazing Barrel: *"On Firing: +X Multishot. Stacks up to Nx."*
    ///
    /// `base` is which bracket the card names, and is why one perk name needs
    /// one variant rather than two: the Strun family reads "+0.05 BASE
    /// Multishot" and the Sybaris family "+5% Multishot". NO DURATION — neither
    /// page states one and both state the reset instead, so the stacks stand
    /// until a reload (`ClearedBy::Reload`).
    /// A STACKING BUFF, stated entirely in data: what triggers it, what it
    /// grants, how much, how many, how long, and what takes it.
    ///
    /// The vocabulary the sim runs on — [`crate::model::BuffTrigger`],
    /// [`crate::model::BuffGrant`], [`crate::model::ClearedBy`],
    /// [`crate::model::BuffDecay`] — is expressive enough on its own; what
    /// was missing was a way for a yaml to NAME a combination of it, so a perk
    /// whose trigger and grant both exist is a yaml block and no Rust at all.
    /// The older single-purpose variants are kept where they carry reasoning a
    /// generic one cannot (Ready Retaliation's arming, Reaver's Rapture's burst
    /// arithmetic).
    StackingGrant {
        trigger: crate::model::BuffTrigger,
        grant: crate::model::BuffGrant,
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
        chance: f64,
        decay: crate::model::BuffDecay,
        cleared_by: crate::model::ClearedBy,
        /// The card's own claim that a mission never takes it — see
        /// [`crate::model::StackingBuff::card_opens_full`].
        card_opens_full: bool,
    },
    StackingMultishotOnFiring {
        per_stack: f64,
        max_stacks: u32,
        base: bool,
    },
    StackingMultishotOnStatus {
        status: crate::rules::damage::DamageType,
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// FLAT crit chance added AFTER mods (Elemental Excess: "Bonuses are
    /// added after mods as a flat value") — NOT the base-stat layer that
    /// Commodore's Fortune occupies.
    PostModCritChance(f64),
    /// FLAT status chance added after mods (Elemental Excess).
    PostModStatusChance(f64),
    /// Additive headshot-damage bonus (Caput Mortuum): joins the headshot
    /// bracket `(1 + Σ)` that multiplies the body-part multiplier.
    HeadshotDamage(f64),
    /// Devouring Attrition: on an instance that did NOT crit, `chance` to
    /// multiply it by `(1 + value)`. An INDEPENDENT multiplier ("multiplicative
    /// to base damage bonuses such as Hornet Strike") that applies to BOTH
    /// attack parts, the radial explosion included.
    ChanceDamageOnNoncrit { chance: f64, value: f64 },
    /// Incarnon gauge fill rate (Incarnon Efficiency): weakpoint hits build
    /// `1 + value` times the charge, so the hits needed to fill divide by it.
    IncarnonChargeRate(f64),
    /// Overwhelming Attrition: a hit that is NEITHER critical NOR applies a
    /// status grants a stack worth `+per_stack` damage for `duration`; on
    /// timeout ONE stack drops and the timer resets (the Galvanized decay,
    /// wiki-verbatim). The bonus is ADDITIVE to the base-damage bucket
    /// ("additive to base damage bonuses such as Hornet Strike") — unlike
    /// [`EvoEffect::ChanceDamageOnNoncrit`], which the same page calls
    /// multiplicative.
    StackingDamageOnPlainHit {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// Lethal Rearmament: every HEADSHOT grants a stack of reload speed
    /// for `duration`, one stack lost per timeout (the Galvanized decay).
    /// Reload speed also scales the Incarnon transmute animations, so this
    /// shortens the whole cycle, not just reloads.
    StackingReloadSpeedOnHeadshot {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// No damage payload here (holstered regen, recoil, timed utility
    /// buffs, the weapon unlock) — kept so the evolution loads and lists.
    /// THE TRANSFORMATION ITSELF — tier 1 of every Incarnon ladder, naming the
    /// form it unlocks. It changes no stat (the form's own entry carries those)
    /// and it is not a CHOICE: every one of these is `selection: fixed`,
    /// because installing the Genesis is what grants it.
    ///
    /// It was parsed as `Inert("unlocks_weapon")` and the target dropped on the
    /// floor, which left "which evolution unlocks the form" to be guessed from
    /// LADDER POSITION ("tier 1's first option"). Reading it is what lets the
    /// form and the evolution stop being two controls for one fact — asking to
    /// fire the Incarnon form implies the evolution that IS firing it.
    UnlocksForm(String),
    Inert(String),
    /// NOT A TODO — AN EDGE. The clause is understood and cannot pay out in
    /// this simulator, and one of `docs/UNMODELLED.md`'s classes says why.
    ///
    /// The mods have had this distinction since 2026-08-05 (`not_modeled` vs
    /// `out_of_scope`) for a reason the evolutions inherited without the fix:
    /// printing "not modelled yet" over both is what made the whole app look
    /// unfinished. A perk waiting on work someone can do and a perk waiting on
    /// a second body in the arena are different sentences to a player deciding
    /// what to equip.
    ///
    /// It stays OFF the ratchet in `unmodeled_effects` — nothing about this
    /// engine will ever close it — and ON the page, which is where the
    /// difference is for.
    OutOfScope { clause: String, reason: Scope },
    /// THE GAME DOES NOT DO IT. The card states a clause, the clause pays
    /// nothing when measured, and a hotfix restores it.
    ///
    /// A THIRD PROMISE, and the only one that is not a shortfall of ours
    /// (`live_bugs:` on an arcane has said the same thing since Primary
    /// Debilitate). [`EvoEffect::Inert`] is work someone can do and
    /// [`EvoEffect::OutOfScope`] is the edge of what a single-target damage
    /// simulator is; this one says the model is RIGHT and reality is broken.
    /// Reporting it as either of the others would tell a reader to wait for
    /// us, when what they should do is not pick the perk.
    ///
    /// The `clause` is the effect's own kind, so the line names WHICH half of
    /// a two-clause perk is dead — Carnage Reign's +60 base damage works and
    /// its "+33% per Status Type" does not (MEASUREMENTS M49).
    LiveBug { clause: String, note: String },
    /// A clause that QUALIFIES a neighbouring effect rather than being one —
    /// "Stacks up to 4x" on a card whose stacking bonus is the effect above it.
    ///
    /// It is not a gap and it must not be counted as one. All 51 of these sit
    /// in a perk that ALREADY declares a real gap (the conditional bonus they
    /// cap is itself inert), so counting them said "partly modelled" twice for
    /// one thing and put a third of the roster's inert total on a fragment of
    /// a sentence. `a_qualifier_never_stands_alone` is what keeps that true:
    /// the day one appears beside a working effect, it IS a gap — the cap goes
    /// unenforced — and the test fails so somebody looks.
    Qualifier(String),
}

/// WHY a clause can never pay out here — `docs/UNMODELLED.md`'s classes, as a
/// closed set, so a new gap either fits a reason already written down or is a
/// reason nobody has thought about yet (which that file says is itself worth
/// knowing).
///
/// EVERY MEMBER IS A PROPERTY OF THE ARENA and none is a property of the FIGHT
/// ON SCREEN — a reason a reader can invalidate by editing their own scenario
/// does not belong in this set. There is no `one_target` among them for exactly
/// that reason: the arena holds a formation, so a clause about a second body is
/// either computed or is work (`notes: one_target_is_not_an_edge`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    NoMovement,
    NoHolster,
    InfiniteAmmo,
    NobodyShootsBack,
    WarframeAbilities,
}

impl Scope {
    pub fn parse(s: &str) -> Option<Scope> {
        Some(match s {
            "no_movement" => Scope::NoMovement,
            "no_holster" => Scope::NoHolster,
            "infinite_ammo" => Scope::InfiniteAmmo,
            "nobody_shoots_back" => Scope::NobodyShootsBack,
            "warframe_abilities" => Scope::WarframeAbilities,
            _ => return None,
        })
    }

    /// The sentence a player reads. English is the source; the i18n overlay
    /// translates it like any other UI string.
    pub fn why(self) -> &'static str {
        match self {
            Scope::NoMovement => "the player does not move or aim by hand here",
            Scope::NoHolster => "this weapon is never holstered during the fight",
            Scope::InfiniteAmmo => "ammo reserves are unlimited, so nothing runs dry",
            Scope::NobodyShootsBack => "nothing damages the player in this fight",
            Scope::WarframeAbilities => "no Warframe ability is cast during the fight",
        }
    }
}
