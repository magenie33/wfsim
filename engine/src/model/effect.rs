// SPDX-License-Identifier: AGPL-3.0-or-later
//! WHAT A CARD DOES: the effects a mod, an arcane or an evolution states, and
//! the conditions and stacking rules they state them under.

use super::*;
use crate::rules::damage::DamageType;
use crate::rules::capacity::Polarity;

/// Combat faction — the key for faction-damage mods (Bane/Expel/Cleanse/Smite,
/// "System A"). Distinct from [`crate::data::enemies::ScalingFaction`] (stat
/// scaling) and from the per-type vulnerability column ("System B"). `Unknown`
/// = no faction mod ever applies (e.g. Zariman Thrax, faction "Unknown").
/// Strict matching: Grineer mods do NOT hit Corrupted/Narmer units. The
/// `Corrupted` variant covers the Void/Orokin enemies the "Expel Orokin"
/// family targets. Wiki `Faction_Damage_Bonus`, docs/MECHANICS.md §2/§8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Faction {
    Grineer,
    Corpus,
    Infested,
    Corrupted,
    Murmur,
    Sentient,
    Unknown,
}

impl Faction {
    /// Map a data string (mod `faction:` field, enemy `combat_faction:`) to a
    /// faction. `orokin` aliases `Corrupted`; unrecognized → `Unknown`.
    pub fn from_name(s: &str) -> Faction {
        match s.trim().to_ascii_lowercase().as_str() {
            "grineer" => Faction::Grineer,
            "corpus" => Faction::Corpus,
            "infested" => Faction::Infested,
            "corrupted" | "orokin" => Faction::Corrupted,
            "murmur" | "the_murmur" | "the murmur" => Faction::Murmur,
            "sentient" => Faction::Sentient,
            _ => Faction::Unknown,
        }
    }
}

/// The stat bucket a conditional buff feeds ([`ModEffect::CondBuff`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CondBucket {
    BaseDamage,
    Multishot,
    CritChance,
    CritDamage,
    StatusChance,
    StatusDamage,
    FireRate,
    /// Archgun Ace's second half. Reload is not a per-hit bucket, so this
    /// contributes only under AssumedMax — under Emergent the sim would have
    /// to hold a live reload-speed timer, which nothing else needs yet.
    ReloadSpeed,
}

impl CondBucket {
    /// Printed on a card, so it is words and not the variant name: a
    /// conditional fire-rate buff must not read "+50% FireRate".
    pub fn label(&self) -> &'static str {
        match self {
            CondBucket::BaseDamage => "Base Damage",
            CondBucket::Multishot => "Multishot",
            CondBucket::CritChance => "Crit Chance",
            CondBucket::CritDamage => "Crit Damage",
            CondBucket::StatusChance => "Status Chance",
            CondBucket::StatusDamage => "Status Damage",
            CondBucket::FireRate => "Fire Rate",
            CondBucket::ReloadSpeed => "Reload Speed",
        }
    }
}

/// A player STATE a mod can be conditional on. One variant per field of
/// [`crate::data::tenno::TennoState`] — the two are meant to be read together.
///
/// `Aiming` is in here rather than beside it: it was a bool threaded through
/// `resolve` while the other states lived on the Tenno, which is two homes for
/// one kind of fact and two places to remember when the third state lands. A card says "while X"; the fight says who is doing what;
/// one enum joins them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TennoCondition {
    Aiming,
    Invisible,
    Airborne,
}

/// One resolved effect of a mod at its equipped rank.
///
/// NOT `Copy`: [`ModEffect::WhileTenno`] nests an effect, which needs
/// indirection. Every arm still binds only `Copy` payloads, so `match *e`
/// works unchanged.
#[derive(Debug, Clone, PartialEq)]
pub enum ModEffect {
    /// DOUBLE TAP: `(per stack, max stacks, seconds)` — a damage bonus that
    /// climbs with CONSECUTIVE HITS and stands on its own multiplier.
    ///
    /// "Multiplicatively stacks with damage bonuses like Serration and Faction
    /// Damage Bonus", so it joins the chain of independent multipliers rather
    /// than the base-damage bucket.
    ///
    /// The count is PER TRIGGER PULL rather than per pellet, pinned by the
    /// card's own arithmetic: "the bonus is applied on hit to all pellets as
    /// damage * 20% * (hits - 1)".
    /// SYNTH CHARGE: *"bonus damage to the final shot in the Magazine"*.
    ///
    /// ITS OWN MULTIPLIER — "Damage stacks multiplicatively with Hornet
    /// Strike, and any area damage the weapon may have is also affected" — a
    /// factor beside Double Tap's, never a base-damage bucket term.
    ///
    /// THREE THINGS SWITCH IT OFF, all the mod's own words: "has no effect on
    /// Continuous Weapons", "does not have an effect on any Incarnon fire
    /// modes", and only EQUIPPABLE at a BASE magazine of 6 or higher. The first
    /// two resolve against the form; the third is an equip rule, because a
    /// magazine mod can neither buy it nor lose it.
    LastRoundDamage(f64),
    /// **A BONUS COMPUTED FROM THE PLAYER**, per unit of one of their stats,
    /// capped — the Basmu's Dreadful Killshot: *"increases Damage and Status
    /// Chance for every 75 Current Warframe Health, up to 360% at all ranks"*.
    ///
    /// The MOD-side twin of `ArcEffect::TennoScaled`, a separate variant
    /// because a mod's effects resolve in a different pass — `base.tenno_scaled`
    /// is read in `resolve_for`, where the Tenno is known.
    ///
    /// ONE PERCENTAGE, TWO BUCKETS, so the card is TWO entries with identical
    /// parameters rather than one granting a pair: the wiki settles that they
    /// are the same number (*"Both the Damage and Status chance bonuses are
    /// additive"*), and two entries keep every grant a single bucket.
    ///
    /// WHICH BUCKET the damage half joins is not a guess: the wiki's own note
    /// is *"the equipped Warframe must have at least 675 current health for the
    /// damage bonus to outdo Serration"*, and 675/75 x 20% = 180% against
    /// Serration's 165%. That comparison is only meaningful inside Serration's
    /// bracket.
    TennoScaled {
        stat: crate::model::TennoStat,
        /// Only the part of the stat ABOVE this counts. Zero for a card that
        /// counts from nothing, which is this one.
        above: f64,
        /// How much of the stat one step is worth — 75 health.
        unit: f64,
        /// What one whole step pays — 0.20 at max rank.
        per_unit: f64,
        /// The card's own ceiling on the whole bonus. 3.6 here, and the SAME at
        /// every rank, which is why it does not scale with rank.
        cap: f64,
        grant: crate::model::ArcGrant,
    },
    /// THE CHAMBER FAMILY: *"+X% Damage on first shot in Magazine"* (Charged
    /// Chamber, Primed Chamber).
    ///
    /// THE GATE IS NOT "THE MAGAZINE WAS FULL", and both pages say so in the
    /// same words: the bonus lands *"as long as the magazine counter is at Max
    /// Magazine - 1 **after** a shot is fired"*, so *"when used alongside 100%
    /// ammo efficiency, make sure one shot is missing from the magazine"*. A
    /// POST-SHOT reading, which `resolve` cannot answer and the sim can, where
    /// the round's real cost is known.
    ///
    /// ONE BRACKET FOR BOTH CARDS: they *"stack additively with each other for
    /// up to 140% bonus damage"*, so the two sum here and the sum is one
    /// factor, never a base-damage bucket term. They are NOT a mod family —
    /// *"Despite its name, Primed Chamber is … not the 'Primed version' of
    /// Charged Chamber, and thus can be equipped alongside it"*.
    ///
    /// IT REACHES STATUS DAMAGE — *"The damage bonus applies to all Multishot
    /// hits and to Status Damage"* — which is the one thing that separates it
    /// from [`ModEffect::LastRoundDamage`], whose own page never says either
    /// way (see `data/mods/pistol/synth_charge.yaml`).
    FirstRoundDamage(f64),
    ConsecutiveHitDamage { per_stack: f64, max_stacks: u32, duration: f64 },
    /// ADDED SPREAD, in DEGREES, and it is NOT an accuracy bonus.
    ///
    /// Split Flights is the roster's first. Its rank ladder is published as a
    /// SPREAD table (+0.3 -> +1.8) beside the card's accuracy wording, and the
    /// page states the bracket outright: *"Added spread is not affected by
    /// bonuses that increase accuracy, such as Twitch or Guided Ordnance"*.
    ///
    /// [`IndirectStat::Accuracy`] is the wrong home for it twice over. That
    /// bucket DIVIDES the cone — accuracy is `100 / spread`, so a +30% card
    /// divides by 1.3 — which means a negative entry there would both scale
    /// instead of adding AND be clawed back by exactly the mods the wiki says
    /// cannot touch it. Applied here, after the divisor, both halves come out
    /// right and the number stays in the unit the source published it in.
    AddedSpread(f64),
    /// A MOD THAT GRANTS A [`StackingBuff`] — the same shape a weapon perk
    /// grants, reaching the sim by the same path.
    ///
    /// With `WeaponBase::stacking_buffs` as the only door, a mod that stacks on
    /// a trigger has to invent a bespoke effect — `OnKillMultishot`,
    /// `OnHeadshotKillCritChance`, `ConditionOverload` are three variants for
    /// one idea. Split Flights is the case that makes a fourth absurd: *"On
    /// Hit: +100% Multishot … Stacks up to 4x"* is a trigger already in
    /// [`BuffTrigger`] feeding a grant already in [`BuffGrant`].
    ///
    /// So the mod carries the whole spec and `resolve` hands it to the panel
    /// beside the weapon's own. Everything downstream — the buff card, the
    /// replay curve, the stack config, the sampler — is keyed by `id` and
    /// already walks that list "by construction", so a second mod on any
    /// trigger/grant pair in those two vocabularies costs no engine code at all.
    GrantsStackingBuff(StackingBuff),
    /// Additive base-damage bucket (Hornet Strike).
    BaseDamage(f64),
    /// Additive multishot bucket (total pellets = base × (1 + Σ)).
    Multishot(f64),
    /// Relative crit chance (base_cc × (1 + Σ)).
    CritChance(f64),
    /// Relative crit damage (base_cd × (1 + Σ)).
    CritDamage(f64),
    /// Relative status chance.
    StatusChance(f64),
    /// Relative fire rate (negative for Creeping Bullseye's downside).
    FireRate(f64),
    /// JAHU CANTICLE: `(fraction, radius_m)` — what one KILL takes off the
    /// armour of every enemy inside Affinity Range.
    ///
    /// AFFINITY RANGE IS CENTRED ON THE PLAYER rather than on the corpse:
    /// *"Affinity will be shared among the squad if they are within a 50-meter
    /// radius"* (wiki `Affinity`), so the range this card names is a distance
    /// from YOU. Which body died therefore does not matter, only that one did —
    /// and that is what makes it a count rather than a position.
    StripOnKillInRange(f64, f64),
    /// THE INVOCATIONS: a stacking bonus to one of the frame's own stats, earned
    /// by the alt fire and capped.
    ///
    /// `(stat, per_stack, max_stacks)`. Taken at the CAP rather than tracked
    /// live, and on this weapon that is very nearly exact rather than a
    /// convention: one thrown orb strikes six times and each strike reaches
    /// `floor(3 × multishot)` bodies, so a bare build lands 18 hits with the
    /// first orb against a 15-stack cap. The cards last 20 s and an orb every
    /// 18 s refreshes them.
    AbilityStat(AbilityStat, f64, u32),
    /// Relative CHARGE rate — shortens the draw ONLY (Shell Rush "+50% Charge
    /// Rate"). Its own bucket rather than `FireRate` because a charge-rate mod
    /// must not also speed up an uncharged form: on the Larkspur Prime that
    /// would hand the hit-scan attack a bonus the card never grants.
    ChargeRate(f64),
    /// Reload speed bonus (time = base / (1 + Σ)).
    ReloadSpeed(f64),
    /// Hunter Munitions / Internal Bleeding: chance for a CRITICAL hit to
    /// apply a Slash status, rolled per pellet and INDEPENDENT of status
    /// chance and of the weapon's damage types (wiki: "not affected by the
    /// weapon's Status Chance, or damage type distribution, besides being
    /// indirectly affected by its Critical Chance").
    SlashOnCrit(f64),
    /// Status-damage bucket (Pistol Elementalist) — scales status payloads.
    StatusDamage(f64),
    /// Primary element: ModifiedBase × bonus enters the hierarchy at this
    /// mod's position.
    Element(DamageType, f64),
    /// Combined-element mod (Magnetic Might): added outside the hierarchy.
    CombinedElement(DamageType, f64),
    /// PHYSICAL damage mod (Impact / Puncture / Slash). Scales the BASE of that
    /// physical type — `base_t × (1 + Σ)` — a SEPARATE multiplier that is
    /// MULTIPLICATIVE with base damage (Serration applies after), and does NOT
    /// enter the elemental hierarchy. No effect on a type the weapon lacks
    /// (wiki Damage/Calculation; MECHANICS.md §2).
    Physical(DamageType, f64),
    /// A CONDITIONAL/triggered buff's contribution to a stat bucket, valued at
    /// its assumed-max total (per_stack × max_stacks). Applied ONLY under
    /// `StackPolicy::AssumedMax` (the panel/optimizer's optimistic view); the
    /// emergent sim leaves it to the timeline. For triggered-buff mods whose
    /// trigger isn't event-modeled (on_ability_cast / on_reload / on_hit / …).
    CondBuff(CondBucket, f64),
    /// Galvanized Diffusion's on-kill multishot stacks.
    OnKillMultishot {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// Condition Overload payload (Galvanized Shot): +per_stack per
    /// status TYPE on the target, per on-kill stack, direct hits only.
    ConditionOverload {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
        /// WHICH SWITCH GRANTS IT (`data::buff_events::ALL`), and `None` when
        /// nothing does.
        ///
        /// `None` IS MELEE'S CONDITION OVERLOAD, which is the original and is
        /// unconditional — nothing to earn, so it opens full and no fight can
        /// take it away. The Galvanized family earns the same payload on a
        /// KILL, so a fight that hands out no kills hands out none of this.
        earned_on: Option<&'static str>,
    },
    /// Galvanized Crosshairs' single refreshable buff: on HEADSHOT,
    /// +bonus relative crit chance (while aiming) for `duration`.
    OnHeadshotCritChance { bonus: f64, duration: f64 },
    /// Galvanized Crosshairs' stacks: on HEADSHOT KILL, +per_stack
    /// relative crit chance; each stack has its OWN duration (per-stack
    /// expiry FIFO — unlike the other Galvanized mods' decay).
    OnHeadshotKillCritChance {
        per_stack: f64,
        max_stacks: u32,
        duration: f64,
    },
    /// INDIRECT stat bonus (recoil, accuracy, ammo, projectile speed…):
    /// an additive bucket per stat, resolved into
    /// [`ResolvedPanel::indirect`]. Indirect = excluded from the
    /// theoretical-DPS formula, but a real input to practical combat —
    /// a future shooter model can consume them (recoil/accuracy → hit &
    /// headshot probability; projectile speed → travel time vs moving
    /// targets; ammo/holstered reload → long-fight sustain). Noise and
    /// dodge/roll speed are stealth/survivability, never DPS.
    Indirect(IndirectStat, f64),
    /// Reflex Draw: temporary handling buff on weapon swap-in. Conditional
    /// and handling-only — never a static panel stat.
    OnEquipHandling { recoil: f64, accuracy: f64, duration: f64 },
    /// An effect gated on the player AIMING (`condition: while_aiming` in the
    /// data: Galvanized Crosshairs / Scope, Argon Scope, Hydraulic Crosshairs,
    /// Sharpened Bullets, Bladed Rounds, Pressurized Magazine, the Catalyzers).
    ///
    /// Satisfying this silently would fire every aim-gated buff whether or not
    /// the scenario implies aiming, which flatters any build carrying one. It
    /// is a SCENARIO knob: resolve with `aiming = false` and the wrapped effect
    /// contributes nothing at all. Wrapping rather than adding a flag to each
    /// variant keeps every other arm of the resolver unaware that aiming
    /// exists.
    /// Gated on what the PLAYER is doing — "while aiming", "while Invisible",
    /// "while Airborne". Asked of [`crate::data::tenno::Tenno`], the fight's
    /// second actor, so a card's condition and the fight's state meet in one
    /// place instead of aiming having its own parameter.
    WhileTenno(TennoCondition, Box<ModEffect>),
    /// Faction damage bonus (Bane/Expel/Cleanse/Smite): +v total damage vs a
    /// MATCHING enemy faction. Its own multiplicative bucket, ADDITIVE with
    /// other faction sources; **double-dips on DoT ticks** (applied twice).
    /// Conditional on the target's faction — no effect vs a non-match.
    FactionDamage(Faction, f64),
    /// Magazine capacity bonus (+v of base magazine, additive; floored to a
    /// whole round). Feeds reload cadence / long-fight sustain.
    MagazineCapacity(f64),
    /// Blast RANGE (+v of base radius) — Firestorm/Fulmination. The mods say
    /// "+X% Blast Range", NOT Blast damage: reading that description as an
    /// element is what had Primed Firestorm inventing +44% Blast damage on
    /// every AoE weapon.
    ///
    /// It scales every part that HAS a radius: the radial explosion and the
    /// lingering field. The field is measured and the wiki says so
    /// too ("Firestorm mods will now affect Torid gas clouds"). No single-target
    /// damage consequence — the target stands at the epicentre either way — but
    /// it is what the panel states, and Primary Compression reads the MODDED
    /// radius (MECHANICS §7).
    BlastRadius(f64),
    /// Status-duration bonus (+v): scales status-effect DoT DURATION (→ more
    /// ticks) and slows Heat's armour-strip ramp. No effect on instant procs.
    StatusDuration(f64),
    /// Weak Point damage (Pistol Acuity). The LISTED value; on a true weak
    /// point (humanoid head) the actual bonus is 1.5× the listed value ADDED
    /// to the part's Weak Point Multiplier, and the sum is MULTIPLICATIVE
    /// with the headshot-multiplier bracket (wiki Pistol_Acuity notes:
    /// Butcher 3x head + rank-10 Acuity = 3 + 3.5 × 1.5 = 8.25x).
    WeakpointDamage(f64),
    /// Weak Point crit chance (Pistol Acuity): a NORMAL relative crit-chance
    /// bonus (additive with Pistol Gambit — the multiplicative-crit behavior
    /// was a bug fixed in 38.5) that is only active on weak-point hits.
    WeakpointCritChance(f64),
    /// Sharpened Bullets: on ANY kill, +bonus relative crit damage (while
    /// aiming — the sim assumes constant aiming) for `duration` seconds.
    OnKillCritDamage { bonus: f64, duration: f64 },
    /// SENTIENT SURGE: crit chance and status chance per ACTIVE TENDRIL.
    ///
    /// The two travel together because they ARE one number — "Status Chance /
    /// Crit Chance Increase" is a single column on the wiki's rank table, and
    /// no rank of this mod raises one without the other. The magazine refill
    /// is its own column there and its own effect here.
    PerTendril { crit_chance: f64, status_chance: f64 },
    /// HATA-SATYA: relative crit chance per HIT, and the RELOAD takes it back.
    ///
    /// The pile has no clock at all — "Resets upon reloading or holstering" —
    /// which is why it is not a [`StackingBuff`] with a duration. It is the
    /// same shape as the Ocucor's tendrils one variant up (earned on an event,
    /// cleared by a magazine event, capped) with the event swapped: a hit
    /// instead of a kill.
    ///
    /// THE CAP IS ON THE BONUS, NOT ON THE STACK COUNT.
    /// "The critical chance bonus is capped at 500% at all mod ranks" is a
    /// ceiling on a NUMBER, so the 417th hit at max rank happens like any
    /// other and the bonus it would carry to 500.4% simply reads 500% —
    /// see [`CritPerHit`].
    CritChancePerHit(CritPerHit),
    /// BLOOD RUSH: crit chance per COMBO TIER above the first.
    ///
    /// `Crit Chance = Weapon Crit Chance x [1 + Mod Crit Bonus + Blood Rush
    /// Bonus x (Combo Multi - 1)] + Static Crit Bonus` (wiki, verbatim), so it
    /// is the bracket Point Strike is already in and never a layer of its own.
    ///
    /// IT IS WHY THE COMBO COUNTER MATTERS TO A LIGHT BUILD AT ALL. The counter
    /// does not multiply a normal swing's damage — *"Melee Combo Multiplier
    /// does not multiply the damage of your normal attacks"* — so this and
    /// Weeping Wounds are the entire payoff of building it.
    CritChancePerCombo(f64),
    /// WEEPING WOUNDS, the same sentence on the status side: `Status Chance =
    /// Weapon Status Chance x [1 + Mod Status Bonus + Weeping Wounds Bonus x
    /// (Combo Multi - 1)]`.
    StatusChancePerCombo(f64),
    /// SECONDS ADDED TO THE MELEE COMBO CLOCK (Drifting Contact, Body Count).
    ///
    /// Flat and additive; the wiki floors the result at 0.1 s. `Melee` is in
    /// the name because [`ModEffect::ComboDuration`] above is the SNIPER
    /// combo's, and the two are unrelated systems that a bare `ComboDuration`
    /// would silently merge.
    MeleeComboDuration(f64),
    /// POINTS THE COUNTER OPENS WITH and returns to (Corrupt Charge, Ready
    /// Steel). Spent by a heavy attack and regenerated at 40 points a second.
    InitialCombo(f64),
    /// FRACTION OF THE COUNTER A HEAVY ATTACK DOES NOT SPEND (Focus Energy,
    /// Reflex Coil). *"stacks additively and is capped at 90%"* — the cap is
    /// applied where the sum is taken, not here.
    HeavyAttackEfficiency(f64),
    /// A RELATIVE change to the combo clock (Corrupt Charge's `-50% Combo
    /// Duration`).
    ///
    /// Its own bucket beside [`ModEffect::MeleeComboDuration`] because the two
    /// cards genuinely say different things — Body Count adds twelve SECONDS
    /// and Corrupt Charge halves whatever is there — and folding one into the
    /// other would make the pair's order matter.
    MeleeComboDurationMultiplier(f64),
    /// METRES OF REACH (Reach, Primed Reach).
    ///
    /// FLAT, not a percentage, and that is DE's own card: *"+2.3 Range"* and
    /// *"+3 Range"*. It was a percentage historically and the wiki's own module
    /// carries the metres now — which matters, because a flat 3 m more than
    /// doubles a hammer's 2.5 and would be a rounding error on a whip.
    MeleeRange(f64),
    /// SLAM DAMAGE (Seismic Wave's `+200% Slam Attack Damage`).
    ///
    /// It pays only on an attack that IS a slam, which in this model is a form
    /// whose explosion is `BlastKind::Slam` — so it is worth exactly nothing in
    /// the five modes that swing and everything in the one that lands.
    SlamDamage(f64),
    /// HEAVY ATTACK DAMAGE (Killing Blow's `+120% Melee Damage on Heavy
    /// Attack`), paid only on a form that spends the combo counter.
    HeavyAttackDamage(f64),
    /// TENNOKAI, and the five knobs its seven cards turn.
    ///
    /// *"Landing direct melee hits has a 15% chance of flashing a sword icon
    /// ... for 2 seconds. Performing a Heavy Attack or Heavy Slam during this
    /// flash increases its Wind Up Speed and does not consume Combo Counter."*
    ///
    /// IT IS A BEHAVIOUR, NOT A STAT, and that is what makes it the only melee
    /// mechanic that changes what the LOOP DOES rather than what a number is:
    /// when the window is open, the next swing of a light combo becomes a heavy
    /// attack. The owner said what to do with it in one clause — use it the
    /// moment it fires — so there is no play pattern to invent.
    ///
    /// A LIGHT BUILD IS WHERE IT PAYS: a heavy attack reads the combo counter
    /// and a Tennokai one does not SPEND it, so a combo mode that has climbed
    /// to 12x fires free 12x heavy attacks between its swings.
    ///
    /// One variant with five fields rather than five variants, because the
    /// seven cards each turn a DIFFERENT subset and a build sums them: a
    /// missing knob is a zero, and `enabled` is what says the mechanic is on
    /// at all (three of the seven cards enable it and nothing else does).
    Tennokai {
        /// Does this card switch the mechanic on? (Every card whose text opens
        /// "Enables Tennokai".)
        enabled: bool,
        /// Added to the 15% base chance (Dreamer's Wrath: +50%).
        chance: f64,
        /// Replaces the roll with a fixed cadence (Discipline's Merit: every 4
        /// melee hits). Zero means "roll".
        every_n_hits: u32,
        /// Seconds the window stays open, if this card sets one (Opportunity's
        /// Reach: 4.0). Zero means the base 2.
        window_seconds: f64,
        /// Damage on a Tennokai attack (Master's Edge: +60%).
        damage: f64,
        /// Crit damage on a Tennokai attack (Dreamer's Wrath: +32%).
        crit_damage: f64,
        /// Status chance on a Tennokai attack (Condition's Perfection: +100%).
        status_chance: f64,
        /// TRUTH'S FLAME'S THREE, and no other card has any of them — see
        /// [`Tennokai`] for what each one means.
        chain_seconds: f64,
        damage_needs_chain: bool,
        curse_resets_combo: bool,
        curse_heat_per_second: f64,
        curse_seconds: f64,
    },
    /// A CRIT-CHANCE CARD THAT READS x2 ON A HEAVY ATTACK.
    ///
    /// `+120% Critical Chance (x2 for Heavy Attacks)` is True Steel's own card,
    /// and Sacrificial Steel and Galvanized Steel say the same. It is a
    /// property of the CARD rather than of the bucket: Blood Rush is in the same
    /// bracket and says nothing of the kind, so doubling the bracket would
    /// double a mod the game does not.
    ///
    /// It is the ordinary crit bucket, added TWICE on a heavy form — which is
    /// what "x2" means for a term inside `base x (1 + this + that)`.
    CritChanceHeavyDoubled(f64),
    /// CRIT CHANCE ON A SLIDE ATTACK ALONE (Maiming Strike's `+150% Critical
    /// Chance for Slide Attack`).
    ///
    /// The melee counterpart of Killing Blow and Seismic Wave — a card that
    /// names an attack — and the seven modes are what make it checkable: it is
    /// worth 150% in `slide` and exactly nothing in the other six.
    CritChanceOnSlide(f64),
    /// HEAVY ATTACK WIND UP SPEED (Killing Blow, Amalgam Organ Shatter).
    ///
    /// Its own bucket rather than the fire-rate one, and the wiki says why in
    /// one sentence: *"Increasing melee attack speed does not reduce the wind-up
    /// time; rather, it reduces the interval between heavy attacks."* Two
    /// clocks, two buckets — and folding them would make every attack-speed mod
    /// a heavy-attack mod and every wind-up card a light-attack one.
    HeavyWindUpSpeed(f64),
    /// ADDITIONAL COMBO COUNT CHANCE (Quickening, True Punishment, Enduring
    /// Strike).
    ///
    /// *"Certain mods award extra combo points on hit/block additively"*. A
    /// CHANCE of one EXTRA point per landed hit rather than a multiplier on the
    /// swing's own points — each whole 100% repeats the hit's points, and what is
    /// left rolls for one point per BASE point (MEASUREMENTS M96, M97).
    ComboCountChance(f64),
    /// CHANCE TO GAIN COMBO COUNT — a riven's malus, and a GATE rather than a
    /// share of the chance above: each base combo point a hit earns survives with
    /// `1 + v`, and a lost one takes its points with it (MEASUREMENTS M97).
    ///
    /// ONE RIVEN AXIS, TWO MECHANICS. On the card it is the malus pole of the
    /// axis [`Self::ComboCountChance`] is the bonus pole of, so it reads like one
    /// signed number — and summing the two is the wrong model the measurement
    /// rules out: they stack without netting.
    ComboGainChance(f64),
    /// …AND THE SAME CHANCE, PAID ONLY ON A LIFTED TARGET (Enduring Strike).
    ///
    /// A CONDITION ABOUT THE TARGET IS SIMULATED: `Lifted` is a status this
    /// engine tracks, forced by every heavy slam and by a heavy attack, so the
    /// gate is read at the swing rather than assumed.
    ComboCountChanceOnLifted(f64),
    /// RELATIVE STATUS CHANCE on a Lifted target (Enduring Affliction), in the
    /// same bracket Weeping Wounds lands in — and gated the same way.
    StatusChanceOnLifted(f64),
    /// ...and that refill: a fraction of the magazine back on every kill,
    /// drawn from the reserve ("This mod does not generate ammo").
    MagazineRefillOnKill(f64),
    /// A SYNDICATE AUGMENT's radial (Gilded Truth grants Truth). The payload
    /// belongs to the SYNDICATE and is looked up by id — six effects shared by
    /// dozens of cards, so the card names one rather than restating it.
    ///
    /// `amount` is the card's own number ("+1 Truth"). The sim does not read
    /// it: the points a gauge needs depend on the mod's RANK and are 1000 at
    /// max, and every mod here simulates at max rank.
    SyndicateRadial { syndicate: &'static str, amount: f64 },
    /// Pressurized Magazine: on reload, +bonus relative fire rate (while
    /// aiming) for `duration` seconds.
    OnReloadFireRate { bonus: f64, duration: f64 },
    /// Deadly Efficiency: "On Reload From Empty: +X% Damage for Xs" — a
    /// relative BASE-damage bonus whose window opens when the reload
    /// COMPLETES, not when the magazine runs out. The distinction is worth a
    /// modelled buff: at rank 10 it is +220% for 17 s, which under Emergent
    /// contributes nothing at all unless the window is modelled.
    OnReloadDamage { bonus: f64, duration: f64 },
    /// EXIMUS ADVANTAGE: a relative BASE-damage window opened by a weak-point
    /// hit on an EXIMUS, and by nothing else.
    ///
    /// Two questions at once, which is what earns it a variant rather than a
    /// `kind: buff` with an `on_headshot` trigger: that would arm on any target
    /// and hand +600% base damage to a build the mod does nothing for. The
    /// target-side half is read live in the sim, where `Target::eximus` is in
    /// hand.
    ///
    /// The trigger is the WEAK POINT, not the head: "Despite the description
    /// specifying headshots, the effect can be trigger on weak-point hits"
    /// (wiki) — the same reading [`BuffTrigger::Headshot`] already carries. It
    /// REFRESHES rather than stacking, so one window with its clock restarted.
    OnEximusWeakpointDamage { bonus: f64, duration: f64 },
    /// NIGHTWATCH NAPALM: a LINGERING FIELD the weapon does not have, granted
    /// by a mod.
    ///
    /// The first of its kind — every other field in the app is a property of an
    /// attack (the Torid's cloud), and this one is bolted onto a rocket that
    /// leaves none. Everything downstream of `ResolvedPanel` is unchanged: the
    /// sim spawns, ticks and expires it exactly as it does a weapon's own,
    /// which is why the mod grants a `LingeringBase` rather than a mechanic.
    ///
    /// THE RADIUS IS A FRACTION OF THE BLAST, not a number of its own — DE's
    /// card says "across 90% of the explosion area" — so it is resolved against
    /// the weapon's radial AFTER blast-radius mods, and Firestorm growing the
    /// fire falls out rather than being arranged.
    GrantsLingering(&'static LingeringBase),
    /// …and the SHARE OF THE BLAST AREA that field covers, which is its own
    /// effect because it is its own column on the card: "across X% of the
    /// explosion area", 15% at rank 0 and 90% at rank 5.
    LingeringAreaFraction(f64),
    /// ACID SHELLS: the corpse of anything this weapon kills explodes.
    ///
    /// "causes enemies killed by the Sobek to explode, dealing a flat amount of
    /// Corrosive damage, plus a percentage of the enemy's maximum Health as
    /// Blast damage, to all enemies within 15m of the target", with "linear
    /// damage falloff from 100% to 0% from the central enemy".
    ///
    /// It CHAINS by construction rather than by arrangement: the explosion is
    /// queued on the body that died, and a body it kills queues its own.
    AcidShells(AcidShellsPart),
    /// HARKONAR SCOPE: seconds added to the SNIPER COMBO's decay window.
    ///
    /// It adds to a number the WEAPON states rather than setting one, which is
    /// what makes the wiki's two answers one rule: "+12s" reaches 14 seconds on
    /// every sniper and 18 on the Lanka, whose 6 is its own
    /// ([`crate::model::SniperCombo::seconds`]).
    ComboDuration(f64),
    /// Hemorrhage: each `from` status APPLIED rolls `chance` to also apply
    /// one `to` status (at most one roll per damage instance, and never
    /// alongside another `to` proc in the same instance). The chance is
    /// ×`low_rate_multiplier` while the weapon's LIVE fire rate is strictly below
    /// `low_rate_threshold` (exactly at the threshold gets no bonus).
    ProcConversion {
        from: DamageType,
        to: DamageType,
        chance: f64,
        low_rate_threshold: f64,
        low_rate_multiplier: f64,
    },
}

/// ACID SHELLS' corpse explosion — see [`ModEffect::AcidShells`].
///
/// Three numbers off one rank ladder, kept together because none of them means
/// anything alone: 450 Corrosive, 45% of the victim's maximum health as Blast,
/// and a 15 m reach, all at max rank.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AcidShells {
    pub flat_damage: f64,
    pub health_fraction: f64,
    pub radius_m: f64,
}

/// ONE COLUMN OF THE CARD. DE's card names three numbers in one sentence —
/// "dealing 450 Corrosive Damage (+45% Enemy Max Health) in a 15m radius" — and
/// each is its own rank ladder, so each is its own effect and `resolve`
/// assembles them. Splitting a mechanic across three lines is the price of a
/// card whose every number fills per rank; the alternative is a sentence with
/// two X's nothing can answer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AcidShellsPart {
    FlatDamage(f64),
    HealthFraction(f64),
    RadiusM(f64),
}

/// Indirect stat targets (each its own additive bucket) — outside the
/// theoretical-DPS formula, inside practical combat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndirectStat {
    Recoil,
    Noise,
    AmmoMax,
    ProjectileSpeed,
    HolsteredReload,
    DodgeSpeed,
    AcrobaticSpeed,
    Accuracy,
    /// Punch-through depth in METERS. It joins `punch_through_m` on the panel
    /// and `rules::space::struck_along` spends it per body crossed, so it pays only
    /// against a formation — a lone target has nothing behind it.
    PunchThrough,
    /// Aim zoom (FOV) — pistol zoom carries no damage bonus (unlike snipers).
    Zoom,

    // ---- 2D groundwork ------------------------------------
    // Everything below was `kind: unmodeled` — LOADED AS NOTHING, so the mod
    // equipped and the number vanished. They are real stats with no
    // SINGLE-TARGET damage payload, which is what this bucket is for: the
    // value now survives into the panel, the API and a build's saved state,
    // and the 2D world reads them instead of re-deriving them from card text.
    /// Weapon RANGE, as a fraction (Ballista Measure +20%). Not beam length.
    Range,
    /// Beam LENGTH in metres, flat (Sinister Reach +12 m, Ruinous Extension
    /// +8 m). A separate stat from `Range`: different unit, different weapons.
    BeamRange,
    /// Beam LENGTH as a fraction (Galvanized Acceleration +30%). Its own stat
    /// rather than a second number in `BeamRange`, because the two are
    /// different units AND they land in different places: the flat one is
    /// added AFTER the percentage (see `range_m` in `resolve`), so a single
    /// bucket could not express the order even if the units agreed.
    BeamRangePercent,
    /// Movement speed while AIMING (Agile Aim).
    MovementSpeed,
    /// Sprint speed — a WARFRAME stat the weapon carries (Amalgam Serration),
    /// which is exactly why an Amalgam mod cannot go on a companion weapon.
    SprintSpeed,
    /// Ammo PICKUP conversion: what another slot's ammo drop is worth to this
    /// weapon (Ammo Mutation, Vigilante Supplies). Needs a pickup economy.
    AmmoConversion,
    /// Chance to resist staggers/knockdowns while aiming (Resolute Focus).
    StaggerResist,
    /// Chance to reduce the stagger a SELF-inflicted radial attack causes
    /// (Cautious Shot) — the self-damage side of an AoE weapon.
    SelfStagger,
    /// Extra double jumps refreshed on kill while airborne (Aerial Ace) — a
    /// COUNT, not a fraction.
    DoubleJump,
    /// Flat damage a killed enemy explodes for (Combustion Beam). Real damage,
    /// but it needs a second enemy to land on.
    KillExplosion,
    /// Chance for a status to spread to enemies within 6 m (Shivering
    /// Contagion). Also multi-target only.
    StatusSpread,
    /// A SYNDICATE RADIAL's scale — "+1 Truth" on Gilded Truth. The EFFECT is
    /// `ModEffect::SyndicateRadial`, which names the syndicate; this bucket
    /// only exists so the card's number has somewhere to print.
    SyndicateRadial,

    // ---- TOME MODS ----------------------------------------
    // A Tome equips Pistol mods AND eight of its own (wiki `Tome`), and not one
    // of the eight buys anything this arena has. They are here rather than
    // absent because a card a player owns and cannot find in the builder is
    // indistinguishable from one the app got wrong — the value survives into
    // the panel and the card says what it is worth here.
    //
    // THE THREE ABILITY STATS ARE NOT ONE BUCKET. A Warframe ability rides this
    // fight with a STRENGTH and a DURATION the reader types (data/abilities/),
    // and Strength reaches the damage while Efficiency — which is energy spent
    // — cannot, because there is no energy economy to spend it in. Collapsing
    // them would hide that difference behind one label.
    /// Ability Strength, as a fraction (Vome Invocation, per alt-fire hit).
    /// Feeds a number that DOES reach the damage; what is missing is the loop
    /// from the weapon back into it.
    AbilityStrength,
    /// Ability Duration, as a fraction (Ris Invocation). Same loop, same gap.
    AbilityDuration,
    /// Ability Efficiency, as a fraction (Netra Invocation) — energy SPENT,
    /// which this sim never does.
    AbilityEfficiency,
    /// Energy regenerated per second (Xata Invocation). Needs the same economy.
    EnergyRegen,
    /// A buff granted to ALLIES, as a fraction (Lohk Canticle's fire rate, Fass
    /// Canticle's shield recharge). One bucket for both because the reason they
    /// pay nothing is the same and is not about the stat: this arena is one
    /// Tenno, so there is nobody to grant it to.
    AllyBuff,
    /// Armour and shields stripped from OTHER enemies on a kill (Jahu
    /// Canticle), as a fraction. Real in a crowd, and the arena has a crowd —
    /// what is missing is a death that debuffs its neighbours.
    StripOnKill,
    /// Chance for a killed enemy to drop a Universal Orb (Khra Canticle). What
    /// falls on the floor decides how long a MISSION lasts, which an engagement
    /// of fixed length cannot measure.
    OrbDrop,
}

impl IndirectStat {
    pub fn label(&self) -> &'static str {
        match self {
            IndirectStat::AbilityStrength => "Ability Strength",
            IndirectStat::AbilityDuration => "Ability Duration",
            IndirectStat::AbilityEfficiency => "Ability Efficiency",
            IndirectStat::EnergyRegen => "Energy Regen/s",
            IndirectStat::AllyBuff => "Ally Buff",
            IndirectStat::StripOnKill => "Armour/Shield Strip on Kill",
            IndirectStat::OrbDrop => "Universal Orb Chance",
            IndirectStat::Recoil => "Recoil",
            IndirectStat::Noise => "Noise Reduction",
            IndirectStat::AmmoMax => "Ammo Reserve",
            IndirectStat::ProjectileSpeed => "Projectile Speed",
            IndirectStat::HolsteredReload => "Holstered Reload/s",
            IndirectStat::DodgeSpeed => "Dodge Speed",
            IndirectStat::AcrobaticSpeed => "Acrobatic Speed",
            IndirectStat::Accuracy => "Accuracy",
            IndirectStat::PunchThrough => "Punch Through",
            IndirectStat::Zoom => "Zoom",
            IndirectStat::Range => "Range",
            IndirectStat::BeamRange | IndirectStat::BeamRangePercent => "Beam Range",
            IndirectStat::MovementSpeed => "Movement Speed (aiming)",
            IndirectStat::SprintSpeed => "Sprint Speed",
            IndirectStat::AmmoConversion => "Ammo Pickup Conversion",
            IndirectStat::StaggerResist => "Stagger Resist (aiming)",
            IndirectStat::SelfStagger => "Self-Stagger Reduction",
            IndirectStat::SyndicateRadial => "Syndicate Radial",
            IndirectStat::DoubleJump => "Double Jumps",
            IndirectStat::KillExplosion => "Explosion on Kill",
            IndirectStat::StatusSpread => "Status Spread Chance",
        }
    }

    /// How the stored number READS. Most of these are fractions and print as
    /// a percentage, but three are not, and printing "+1200.0%" for a 12 m
    /// beam extension is worse than not stating it at all.
    pub fn unit(&self) -> &'static str {
        match self {
            IndirectStat::PunchThrough | IndirectStat::BeamRange => "m",
            // …and the percentage half of the same stat reads as one, which is
            // why they are two variants: one label, two units.
            IndirectStat::BeamRangePercent => "%",
            IndirectStat::DoubleJump => "x",
            IndirectStat::KillExplosion => "",
            _ => "%",
        }
    }

    /// The stat's value as it READS. One implementation, because there are two
    /// callers — this enum's own effect line and the API's stat table — and
    /// they disagreeing is how "+800% Beam Range (m)" reached the picker for a
    /// mod that grants 8 metres.
    pub fn format(&self, v: f64) -> String {
        // INFINITE IS A WORD, NOT A BIG NUMBER: the engine holds a finite
        // sentinel (see it for why) and no reader is shown it. `>=`, because
        // mods add to it — a Seeker takes the total to 1001.4.
        if *self == IndirectStat::PunchThrough
            && v >= crate::rules::space::INFINITE_BODY_PUNCH_THROUGH_M
        {
            return "infinite".to_string();
        }
        let unit = self.unit();
        if unit == "%" {
            return pct(v);
        }
        let a = v.abs();
        let s = if (a - a.round()).abs() < 1e-6 {
            format!("{}", a.round() as i64)
        } else {
            format!("{a:.2}").trim_end_matches('0').trim_end_matches('.').to_string()
        };
        format!("{}{s}{unit}", if v >= 0.0 { "+" } else { "−" })
    }
}

/// WHICH OF THE FRAME'S STATS an Invocation raises.
///
/// Four cards, four stats, and only two of them reach a damage number today —
/// which is why this is an enum rather than two fields: the pair that pays
/// nothing is transcribed with the pair that does, and each says which it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbilityStat {
    /// Vome Invocation. Multiplies what Roar, Eclipse and Nourish are worth —
    /// `data::abilities::at_strength` is linear, so a strength bonus is a
    /// straight multiplier on the ability's own number.
    Strength,
    /// Ris Invocation. Extends how long they last.
    Duration,
    /// Netra Invocation. NOTHING READS IT: this arena configures an ability
    /// buff rather than casting one, so there is no energy cost for efficiency
    /// to reduce. Carried so the card can say that in one place.
    Efficiency,
    /// Xata Invocation. NOTHING READS IT either, and for the same reason —
    /// there is no energy pool to regenerate.
    EnergyRegen,
}

impl AbilityStat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Strength => "Ability Strength",
            Self::Duration => "Ability Duration",
            Self::Efficiency => "Ability Efficiency",
            Self::EnergyRegen => "Energy Regen",
        }
    }

    /// WHY THIS ONE PAYS NOTHING, or `None` where it pays.
    ///
    /// The same shape `ShardEffect::unmodelled_reason` uses, and for the same
    /// reason: an effect is applied or it says why not, never neither and never
    /// both.
    pub fn unmodelled_reason(self) -> Option<&'static str> {
        match self {
            Self::Strength | Self::Duration => None,
            Self::Efficiency => Some(
                "this arena CONFIGURES a Warframe buff rather than casting one, so there is no                  energy cost for efficiency to reduce",
            ),
            Self::EnergyRegen => Some(
                "this arena has no energy pool — a Warframe buff here is a setting on the                  fight, not something you pay for",
            ),
        }
    }
}

/// A mod as the resolver sees it (stats at the equipped rank).
#[derive(Debug, Clone)]
pub struct ModDef {
    pub id: &'static str,
    /// DE's own name for the card, carried through rather than rebuilt from
    /// the id, which is lossy in both directions: "Semi-Shotgun Cannonade"
    /// comes back as "Semi Shotgun Cannonade" and its wiki link 404s, "Hell's
    /// Chamber" loses its apostrophe, "Bane of Grineer" gains a capital O.
    pub name: &'static str,
    /// Drain at the EQUIPPED rank (max, unless the id is `<card>@<rank>`).
    pub base_drain: u32,
    /// Max rank (drain rises 1/rank from rank 0, so rank-0 drain = base_drain − max_rank).
    pub max_rank: u32,
    pub polarity: Polarity,
    /// Card rarity (frame colour). Display-only — no mechanical effect.
    pub rarity: Rarity,
    /// Exilus (utility) mod: may occupy the exilus slot in addition to
    /// regular slots. Exilus mods are handling/QoL effects with no damage
    /// model, so the optimizer skips them.
    pub exilus: bool,
    /// A STANCE, and the combo scripts it supplies.
    ///
    /// THE FIRST MOD IN THIS REPO THAT CHANGES WHAT A WEAPON FIRES rather than
    /// what it fires with. A stance publishes four ground combos, a slide and a
    /// heavy, and installing one replaces the weapon entry's own scripts — so
    /// the same Magistar in the same mode is a different sequence of swings on
    /// Crushing Ruin and on Shattering Storm.
    ///
    /// IT NEEDS NO SLOT OF ITS OWN in the wire, which is the one thing that
    /// makes it cheap: a stance mod is legal in the stance slot and NOWHERE
    /// else, so a flat mod list can say which entry is the stance by looking at
    /// it. That is exactly what the exilus slot could NOT do — an
    /// exilus-eligible mod is legal in a main slot too, which is why that one
    /// travels in a field of its own (AGENTS.md).
    ///
    /// Keyed by [`crate::model::FormKind::id`] — see [`StanceCombos`].
    pub stance: Option<StanceCombos>,
    /// Mods sharing a family are mutually exclusive (wiki Incompatible).
    pub family: Option<&'static str>,
    /// Weapon property required to EQUIP this mod at all — "continuous" for
    /// the beam-only mods. Distinct from `requires`, which is a calc-layer
    /// gate: that one equips and sits inert, this one is never offered.
    pub requires_weapon: Option<&'static str>,
    /// The weapons this mod may be equipped on, and nothing else. Empty means
    /// "any weapon whose pool carries it" — see `mods_data`.
    pub exclusive_to: &'static [&'static str],
    /// DE's INCOMPATIBILITY tags for this mod, lowercased — the mirror of
    /// `requires_weapon`, and the reason Amalgam Serration is not offered on
    /// a sentinel weapon while plain Serration is (wiki: "This mod cannot be
    /// equipped on Sentinel weapons", tags `SENTINEL_WEAPON, POWER_WEAPON`).
    /// An Amalgam mod's second half buffs the WARFRAME, which is why the
    /// weapon a companion carries cannot hold one.
    pub excludes_weapon: Vec<&'static str>,
    /// The MOD SET this mod belongs to (`data/mod_sets/<id>.yaml`). A set
    /// bonus is granted by the group, not by any member, and it scales per
    /// equipped member with no threshold — see [`crate::data::mod_sets`].
    pub set: Option<&'static str>,
    /// Weapon TRAIT this mod's effects require to apply (else the whole mod is
    /// inert — a calc-layer gate, NOT an equip block). Declared only for
    /// general effects that would otherwise be misapplied (Semi-Pistol
    /// Cannonade → `semi_auto`); self-gating effects (beam range) declare none.
    pub requires: Option<&'static str>,
    /// Stats this mod LOCKS from being modified while equipped (Pistol Acuity →
    /// `multishot`, Semi-Pistol Cannonade → `fire_rate`): every mod's bonus to
    /// that bucket is zeroed in `resolve`.
    pub disables: Vec<&'static str>,
    pub effects: Vec<ModEffect>,
    /// Does this mod have an effect the sim knowingly does NOT model?
    ///
    /// False for almost every mod. True means the CARD must say so — an
    /// `unmodeled` effect is DROPPED at load, so a mod carrying one loads as a
    /// mod that does nothing and says nothing, which is exactly how it looks
    /// to a player who equips it and sees no change. The reason lives in the
    /// YAML comment beside the effect, where a maintainer reads it — not in a
    /// field the app renders.
    pub unmodeled: bool,
    /// ...and does it act on something this simulator does not HAVE — Warframe
    /// energy, enemy behaviour, traversal, reviving? Never a todo: building it
    /// would not move a damage figure. Told apart from `unmodeled` because
    /// saying "not modelled" for both makes the model's own edge look like
    /// unfinished work.
    pub out_of_scope: bool,
}

/// Mod card rarity — determines the in-game frame colour (bronze / silver /
/// gold / white). Purely cosmetic; carried for UI display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

impl ModDef {
    /// The primary element this mod adds, if any (position-sensitive).
    pub fn primary_element(&self) -> Option<DamageType> {
        self.effects.iter().find_map(|e| match e {
            ModEffect::Element(t, _) => Some(*t),
            _ => None,
        })
    }
}

/// How stacking/conditional effects are valued (docs/OPTIMIZER.md §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackPolicy {
    /// Full stacks / 100% uptime on every conditional buff.
    AssumedMax,
    /// On-kill stacking buffs start at their configured INITIAL stacks
    /// (full, per correction) and then evolve purely by
    /// mechanics: kills refresh/grant, timeouts decay one stack.
    Emergent,
    /// A COMPANION's weapon. Galvanized (and other conditional
    /// on-kill/on-headshot/on-reload) effects can be EQUIPPED and only their
    /// unconditional BASE part applies.
    ///
    /// The reason is not that a companion is excluded from the buff — it is
    /// not. The TRIGGER belongs to the Tenno: the on-kill roll comes from the
    /// Tenno's own weapons, and the stacks it grants then apply to the Tenno
    /// AND the companion. What this arena cannot do is
    /// simulate the two together — it fires ONE weapon — so when that weapon
    /// is the companion's, nothing on the field can generate the stacks and
    /// only the base is honest.
    ///
    /// So this is an ARENA limit, not a game rule, and it is the wrong answer
    /// the moment a Tenno weapon and a companion weapon are simulated side by
    /// side. (wiki `Galvanized_Mods`;, corrected 2026-07-31)
    BaseOnly,
}

/// "No timeout": the duration a LOCKED buff card runs on.
///
/// Locking is not a flag the engine has to remember to consult — it OVERWRITES
/// the buff's duration. Every clock in the sim is
/// `expiry = now + duration`, so an infinite duration gives a buff that starts
/// where its card says, still climbs on every trigger, and never expires —
/// which is exactly what the label promises, expressed in the one place that
/// can express it.
///
/// A FLAG (`pinned` / `locked`) would have to be honoured at every read site,
/// and missing one lets the stacks decay and the trigger be skipped, so "no
/// timeout" comes to mean "decays to zero and can never come back". A duration
/// cannot be forgotten — there is nothing to thread.
pub const NO_TIMEOUT: f64 = f64::INFINITY;

/// A live on-kill stacking buff spec handed to the sim under
/// [`StackPolicy::Emergent`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StackSpec {
    /// Contribution per stack (multishot: already × base pellets; CO:
    /// per-type rate).
    pub per_stack: f64,
    pub max_stacks: u32,
    /// Per-refresh duration; decay = lose ONE stack and reset (the
    /// Galvanized family's graceful decay). [`NO_TIMEOUT`] when the buff card
    /// is locked — the stacks then climb as usual and never fall off.
    pub duration: f64,
    /// Stacks at t = 0 (user setting: full by default, 0 for a cold
    /// start; afterwards mechanics rule either way).
    pub initial_stacks: u32,
    /// WHICH SWITCH GRANTS THESE STACKS (`data::buff_events::ALL`), `None` when
    /// nothing does — the answer `FightParams::deny_buff_triggers` reads and
    /// the same one the card is greyed by, so the page and the run cannot
    /// disagree. It travels on the spec because the data states it and the
    /// engine has no business classifying it a second time: melee's Condition
    /// Overload and Galvanized Shot share this shape and one is earned on a
    /// kill while the other is earned by nothing.
    pub earned_on: Option<&'static str>,
}

/// WHAT A STANCE PUBLISHES: `(form id, its swings)`.
///
/// NO NAME, and that is a decision rather than an omission. A MODE'S NAME IS FIXED and the stance changes what it is WORTH:
/// swapping this card moves the numbers behind `neutral` and never what
/// `neutral` is called, which is the only way "which stance is best for the
/// neutral combo" can be asked at all. It was briefly a triple carrying the
/// combo's own name, and drawing that name on the mode made the two builds
/// incomparable.
pub type StanceCombos = &'static [(&'static str, &'static [crate::model::ComboHit])];

/// TENNOKAI, resolved: what the equipped cards came to.
///
/// `enabled` is the gate and the rest are sums. Default is OFF, which is every
/// build that carries none of the seven cards — and the mechanic genuinely does
/// not exist without one, so this is the game's own answer rather than a
/// modelling shortcut.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Tennokai {
    pub enabled: bool,
    pub chance: f64,
    pub every_n_hits: u32,
    pub window_seconds: f64,
    pub damage: f64,
    pub crit_damage: f64,
    pub status_chance: f64,
    /// SECONDS A TENNOKAI **KILL** RE-OPENS THE WINDOW FOR, 0 when no card buys
    /// it. *"Kills grant an additional 4s Tennokai opportunity"* — the only way
    /// in this mechanic to swing Tennokai twice without a normal hit between,
    /// since every other card has to build back to its own trigger.
    pub chain_seconds: f64,
    /// Does [`Self::damage`] pay only inside a CHAINED window? *"The damage
    /// bonus is only active following the first kill"* — so the swing that
    /// earns the chain does not carry it and the swing after it does.
    pub damage_needs_chain: bool,
    /// Does a Tennokai swing that FAILS TO KILL empty the combo counter?
    ///
    /// *"Curse activates when failing to kill a target with a Tennokai attack,
    /// and reset your Combo Counter."* It is the whole cost of the card and the
    /// reason it is a gamble rather than a bonus — and status immunity does not
    /// save it: *"your combo will still be reset"*.
    pub curse_resets_combo: bool,
    /// Heat a second the WIELDER takes on that failure, for
    /// [`Self::curse_seconds`]. Nothing here damages the Tenno, so it is
    /// COUNTED and never applied — a number the reader is owed, not a death.
    pub curse_heat_per_second: f64,
    pub curse_seconds: f64,
    /// THE CHARGE BEFORE A TENNOKAI ATTACK, seconds — and it is ZERO.
    ///
    /// Measured: the window's swing goes out with no charge at all. DE says
    /// only that it *"increases its Wind Up Speed"* and publishes no figure, so
    /// this was a +100% stand-in; the measurement replaces it with the thing
    /// the mechanic is for.
    ///
    /// IT IS STILL RESOLVED HERE rather than off the build's bucket, because
    /// *"the Wind-Up Speed of Tennokai attacks is not affected by Wind-Up Speed
    /// bonuses from other sources"* (wiki, Tennokai) — the field stays so that
    /// a figure, if one is ever published, has one place to go.
    pub windup_seconds: f64,
}

/// HATA-SATYA's pile: a rate per hit and the CEILING ON WHAT IT IS WORTH.
///
/// The distinction is the whole of this type. Every other stacking buff in the
/// app is capped by a STACK COUNT, which is what DE publishes for it; this card
/// publishes a number instead — "The critical chance bonus is capped at 500% at
/// all mod ranks" — and ranks only the rate under it. So the pile is not 416
/// stacks of 1.2%: it is however many hits have landed, worth 500% once they
/// pass the ceiling. The two readings differ by 0.8
/// percentage points at max rank and by a factor of six at rank 0, where 500%
/// is 2,500 hits away rather than 417.
///
/// It is also why the replay draws this row as a VALUE rather than as a count:
/// the number that stops climbing is the one DE published, and a stack count
/// beside a ceiling it can never state is a chart about the wrong quantity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CritPerHit {
    /// Relative crit chance per landed hit — the same bucket Point Strike is
    /// in, so it multiplies the UNMODDED base.
    pub per_stack: f64,
    /// The published ceiling on the bonus (5.0 = 500%), at the rank this build
    /// equips — which is max rank, the only one the sim runs.
    ///
    /// FLAT ACROSS RANKS ON THIS CARD, and that is a fact about the card rather
    /// than about the class: the same shape exists ranked the other way round,
    /// where the rate is fixed and the CEILING is what a rank buys (Primary
    /// Bulwark's +250% -> +500%, Primary Overcharge, Secondary Surge). Those
    /// three ladder it through the arcane loader's own `rank0`/`rankMax`, so
    /// nothing here has to; a MOD that ladders it would state the second
    /// endpoint the way `duration_rank0` does.
    pub max_bonus: f64,
}

impl CritPerHit {
    /// What `stacks` hits are worth, which is the rate until the ceiling and
    /// the ceiling afterwards.
    pub fn bonus(&self, stacks: u32) -> f64 {
        (self.per_stack * f64::from(stacks)).min(self.max_bonus)
    }

    /// The first stack that reaches the ceiling — 417 at max rank, where the
    /// 416th is worth 499.2% and the 417th is worth 500%.
    ///
    /// It is NOT where the fight's counter stops: the pile takes every hit and
    /// the CEILING is what clamps. This is a UI number —
    /// the maximum the card's "stacks the run starts with" stepper offers,
    /// since a higher one buys nothing — and the answer to "how long is the
    /// ramp", which is the fact the card never states and the one that moves
    /// with rank: 417 hits at max rank, 2,500 at rank 0.
    pub fn max_stacks(&self) -> u32 {
        if self.per_stack <= 0.0 {
            return 0;
        }
        (self.max_bonus / self.per_stack).ceil() as u32
    }
}

/// A non-stacking timed buff (a single refreshable window) handed to the sim:
/// Galvanized Crosshairs' on-headshot crit, Sharpened Bullets' on-kill crit
/// damage, Pressurized Magazine's on-reload fire rate. One shape rather than a
/// parallel `Option<(f64, f64)>` per card.
/// EXECUTIONER'S FORTUNE: a headshot's chance to FILL THE MAGAZINE, no reload
/// played and no time spent.
///
/// Not a reload-speed bonus and not a percentage refill — the reload happening
/// for free, which is why it lives beside the magazine. It draws from the
/// RESERVE, so a dry one gives nothing.
///
/// **It does nothing in an Incarnon form**, and "Does not affect Incarnon Form"
/// is no special case: what this refills is a MAGAZINE, and an Incarnon form
/// has max CHARGES, outside the ammo economy.
/// LINGERING JUDGEMENT: a headshot STREAK arms extra headshot damage.
///
/// The bonus joins the ADDITIVE headshot bracket, beside Primary Deadhead's
/// ("stacks additively with Primary Deadhead's headshot damage bonus"), so a
/// build already carrying the arcane gets far less out of it than +50% reads.
#[derive(Debug, Clone, Copy)]
pub struct HeadshotStreak {
    /// Headshots needed (2), inside `within` seconds.
    pub hits: u32,
    pub within: f64,
    /// The headshot-damage bonus while the window is open (0.50).
    pub value: f64,
    /// How long the window lasts once armed (8 s).
    pub duration: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct InstantReload {
    /// Per qualifying headshot (0.10 on the Furis pair, 0.20 on the Phenmor).
    pub chance: f64,
    /// Must the headshot also KILL? The Phenmor says so; the Furis pair do not.
    pub needs_kill: bool,
}

/// How the Condition Overload bonus behaves — PER WEAPON: some weapons take it
/// as an independent multiplier, some fold it into base damage, and some do not
/// benefit at all. The wiki
/// CO-mechanic catalog classifies weapons):
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoBehavior {
    /// Joins the base-damage bucket (additive with Hornet Strike):
    /// direct hit × (1 + base_damage + co × types) / (1 + base_damage).
    AdditiveWithBaseDamage,
    /// A free-standing final multiplier on direct hits:
    /// direct hit × (1 + co × types).
    Independent,
    /// The bonus simply does not apply on this weapon.
    Inert,
}

/// A MELEE INCARNON FORM: what opens it, and how long it is on for.
///
/// It is NOT A FORM in this repo's sense and does not become one — no weapon
/// entry is unlocked and no animation changes. It is the same weapon resolved
/// twice, and this is the rule for which of the two you are in
/// (`fight::Arms::HeavyAtCombo`, `fight::Ends::After`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeleeIncarnon {
    /// The combo MULTIPLIER a heavy attack must go down at — 6x on the
    /// Magistar, 5x on the Praedos, 3x with Swift Transmute.
    pub arm_at_combo: f64,
    /// Seconds it is on for. 180 on a Genesis, which is the whole engagement;
    /// 90 on the Praedos, which is half of one.
    pub seconds: f64,
}

impl ModEffect {
    /// WHAT A SET THAT ENHANCES ITS OWN MEMBERS DOES TO ONE OF THEM.
    ///
    /// *"Increases the effects of both mods by 25% when both are equipped
    /// together"* (wiki, Sacrificial Set) — so the card's own numbers grow and
    /// nothing else about it moves.
    ///
    /// `None` IS THE LOUD ANSWER: a kind this cannot scale is a member whose
    /// bonus would silently go unpaid, and `every_self_scaling_member_can_be_
    /// scaled` refuses one rather than letting the set pay it nothing.
    #[must_use]
    pub fn scaled(&self, k: f64) -> Option<ModEffect> {
        use ModEffect::*;
        Some(match self {
            BaseDamage(v) => BaseDamage(*v * k),
            CritChance(v) => CritChance(*v * k),
            CritChanceHeavyDoubled(v) => CritChanceHeavyDoubled(*v * k),
            CritDamage(v) => CritDamage(*v * k),
            StatusChance(v) => StatusChance(*v * k),
            Multishot(v) => Multishot(*v * k),
            FireRate(v) => FireRate(*v * k),
            // …AND THE FACTION HALF, because the set says BOTH mods' effects
            // and a card's second stat is one of its effects.
            FactionDamage(f, v) => FactionDamage(*f, *v * k),
            _ => return None,
        })
    }
}

/// What an emergent arcane stacking buff adds per stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArcGrant {
    /// Joins the Hornet Strike bracket live (scales ModifiedBase too).
    BaseDamage,
    /// Additive multishot (already an absolute pellet-count bonus × base? —
    /// no: a RELATIVE bonus; the sim multiplies by base pellets itself).
    Multishot,
    /// Joins the reload-speed bucket (time = base / (1 + Σ)).
    ReloadSpeed,
    /// Joins the crit-DAMAGE bucket — a RELATIVE bonus on the attack part's
    /// own base crit damage, like the crit-damage mods (Primary
    /// Blight/Frostbite). Multiplied out per stage in the sim, not here.
    CritDamage,
    /// Joins the status-chance bucket (Primary Crux). VERBATIM (wiki):
    /// "Status Chance bonus is additive to mods like Rifle Aptitude", so it is
    /// a RELATIVE bonus on the attack part's base status chance.
    ///
    /// Unlike `CritDamage` this is NOT resolved to an absolute value in
    /// [`ArcaneDef::fx`]: the direct hit and the explosion carry DIFFERENT
    /// base status chances, so only the sim — which knows which attack part it
    /// is resolving — can multiply it out.
    StatusChance,
    /// Additive ammo efficiency, i.e. the refunded fraction of a round
    /// (Primary Crux's second grant). Wiki: "additive with other sources of
    /// Ammo Efficiency", the same bucket Frenzy feeds.
    AmmoEfficiency,
}

impl ArcGrant {
    /// The `disables:` key this grant feeds — the same vocabulary a locking mod
    /// writes. A lock is *"set to its default ignoring other bonuses, even
    /// negative effects"* (MEASUREMENTS M30), and an arcane's buff is a bonus
    /// like a mod's, so the two only have to agree on the stat's NAME.
    pub fn locked_stat(self) -> &'static str {
        match self {
            ArcGrant::BaseDamage => "base_damage",
            ArcGrant::Multishot => "multishot",
            ArcGrant::ReloadSpeed => "reload_speed",
            ArcGrant::CritDamage => "crit_damage",
            ArcGrant::StatusChance => "status_chance",
            ArcGrant::AmmoEfficiency => "ammo_efficiency",
        }
    }
}

/// Which WARFRAME stat an arcane scales off. The Tenno carries them; this
/// names the one an arcane reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TennoStat {
    /// Primary Bulwark: "+1% damage for each unit of armor past 1,000".
    Armor,
    /// Primary Overcharge: "35% of Max Energy as Multishot".
    MaxEnergy,
    /// Dreadful Killshot: "for every 75 Current Warframe Health".
    ///
    /// CURRENT health is what the card says and MAX health is what this reads,
    /// which is the house rule for a Tenno state the arena does not simulate:
    /// nothing in this fight damages the player, so the two are the same number
    /// for the whole engagement unless somebody types otherwise.
    Health,
    /// Melee Retaliation: *"Gain 30% Melee Damage for every 200 current
    /// Shields, up to 420%"*.
    ///
    /// CURRENT shields, and the arena's Tenno carries whatever the fight says —
    /// which for the neutral player is ZERO, so the card pays nothing and the
    /// panel says why. That is the honest answer rather than a broken gate:
    /// Secondary Kinship reads the same way in a solo fight, and the difference
    /// between "this reads a state you have not got" and "this does not work"
    /// is exactly what `TennoScaled`'s own panel line exists to draw.
    ///
    /// THE OVERSHIELD HALF IS NOT MODELLED — *"Bonus halved for Overshields"* —
    /// and the card says so.
    Shields,
}

impl TennoStat {
    /// The stat's own name, for a card that has to say which one it reads.
    pub fn name(self) -> &'static str {
        match self {
            TennoStat::Armor => "armor",
            TennoStat::MaxEnergy => "max energy",
            TennoStat::Health => "health",
            TennoStat::Shields => "shields",
        }
    }
}
