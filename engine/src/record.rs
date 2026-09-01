//! THE COMBAT RECORD — one ordered stream of everything that happened in a
//! fight, and the authority on what it was.
//!
//! Every other output here is a CURVE or a TOTAL, and both hide an error
//! inside an average: a factor applied twice moves a mean by a few per cent
//! and reads as a build being good. This is a stream of discrete events, each
//! carrying every number behind it.
//!
//! **A DAMAGE EVENT IS ONE NUMBER THE GAME POPS**, not one hit: a pellet
//! landing on a shielded body pops two, because Toxin bypasses the shield and
//! its siblings do not. That 1:1 with the screen is what makes this the only
//! output of this app that can be laid beside a recording and checked.
//!
//! What the stream contains, what it is authoritative about and what it is
//! NOT: AGENTS.md §"A FIGHT POPS NUMBERS" and §"THE STREAM IS THE FOUR THINGS
//! A FIGHT DOES".

use crate::damage::DamageType;

/// WHICH FORM THE WEAPON WAS IN, and how much was left in it.
///
/// Stamped on every event rather than inferred from the reload events around
/// it, so a row is readable on its own: "why did the base damage change" is
/// answered by the row itself rather than by scrolling up to find a transmute.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct WeaponAt {
    /// True while the weapon is in a transmuted (Incarnon) form.
    pub transmuted: bool,
    /// The magazine the ACTIVE form spends — what the next shot comes out of.
    pub magazine: u32,
    pub magazine_max: u32,
    /// …AND THE ONE THAT IS NOT FIRING: the base form's while transmuted, the
    /// charge magazine while not. `None` on a weapon with no cycle.
    ///
    /// Both are drawn ALWAYS, because entering the form REFILLS the base
    /// magazine behind the scenes — "swapping either way fully reloads the base
    /// form's magazine" — and a column that showed only the firing one made
    /// that free reload invisible. A reader auditing a cycle wants to see it
    /// happen.
    pub idle_magazine: Option<(u32, u32)>,
    /// THE INCARNON GAUGE, and how much of it fills the form — `None` on a
    /// weapon that has no cycle.
    ///
    /// WHAT IS LEFT IN RESERVE, or `None` where the fight grants infinite ammo
    /// (which every ruler does). Beside the magazine because they are one
    /// question — "can this weapon keep firing" — and a reader asking it should
    /// not have to hold two columns in their head.
    pub reserve: Option<f64>,
    /// A fight STARTS WITH AN EMPTY ONE, which is a real property of the model
    /// and was invisible: the record showed a base form firing and then, with
    /// no warning, a transform. What charges it is the weapon's own rule —
    /// weak-point hits, direct hits, or kills — so a reader watching this
    /// number climb is watching the thing that decides when the earned form
    /// arrives.
    pub gauge: Option<(u32, u32)>,
}

/// WHY THIS ROW EXISTS — what brought this damage instance into being.
///
/// The question a reader asks first of a stream with several numbers at one
/// instant: which of these is the shot I fired, and which are the ones it
/// spawned. Multishot is the case that made it necessary — two pellets from one
/// trigger pull are two numbers, and telling them apart is the difference
/// between "my multishot works" and "something is double-counting".
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default,
    serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// The pellet a trigger pull would have fired with no multishot at all —
    /// and the default, because it is what every site that does not say
    /// otherwise means.
    #[default]
    Own,
    /// …and the ones multishot added.
    Multishot,
    /// The same round, still flying, arriving at a body behind the first.
    PunchThrough,
    /// The same SWING, reaching past the first body — melee's Follow Through.
    ///
    /// Its own origin rather than `PunchThrough`'s, because a reader laying the
    /// panel beside the game is looking at two different mechanics: punch
    /// through spends a budget and decays with the distance flown, a swing
    /// spends nothing and decays geometrically by the order it got to bodies
    /// in. Sharing a row would make the one column that says WHY this body was
    /// hit answer the wrong question.
    FollowThrough,
    /// A chaining beam's hop.
    Chain,
    /// A projectile that bounced.
    Ricochet,
    /// An arcane's echo — a second firing of the same shot.
    Echo,
    /// An area a detonation threw at everything around the body that carried it.
    Splash,
    /// A status effect settling: a bleed, a burn, a tick.
    Status,
    /// A DEPLOYED ORB'S strike, or its detonation — an entity acting from
    /// wherever it has drifted to, on a clock of its own.
    ///
    /// Told apart from `Field` because it is a different mechanic and a reader
    /// laying the panel beside the game needs to know which: a field beats
    /// everyone standing in its area, an orb picks ONE body inside its reach.
    Orb,
    /// A lingering field's own clock (the Torid's cloud).
    Field,
    /// An EXTRA HIT — a second damage instance beside a hit (docs/EXTRA_HIT.md).
    ExtraHit,
    /// MELEE INFLUENCE — a status this swing left on one body, arriving on
    /// everything standing around it.
    ///
    /// Its own row rather than `ExtraHit`'s, though it is one: a reader laying
    /// the panel beside the game is asking WHY this body was hit, and "a swing
    /// three rooms of enemies away procced Electricity" is not an answer any
    /// other origin gives.
    Influence,
    /// An arcane's or a syndicate's own instance.
    Arcane,
}

impl Origin {
    pub fn name(self) -> &'static str {
        match self {
            Origin::Own => "own",
            Origin::Multishot => "multishot",
            Origin::PunchThrough => "punch_through",
            Origin::FollowThrough => "follow_through",
            Origin::Chain => "chain",
            Origin::Ricochet => "ricochet",
            Origin::Echo => "echo",
            Origin::Splash => "splash",
            Origin::Status => "status",
            Origin::Orb => "orb",
            Origin::Field => "field",
            Origin::ExtraHit => "extra_hit",
            Origin::Influence => "influence",
            Origin::Arcane => "arcane",
        }
    }
}

/// ONE FACTOR, with the name it is known by on a card or a wiki page.
///
/// A `&'static str` because every label in this engine is a literal: a factor
/// whose name had to be built at runtime would be a factor nobody could join
/// EVERY FACTOR A NUMBER CAN BE BUILT FROM — a TYPE, not a string.
///
/// It was `&'static str` written at the call site, which made the record only
/// half authoritative about WHY: nothing tied the word "critical" to the 4.4
/// beside it, a typo was a new factor nobody would notice, and two different
/// things were both called "shield gate" — the 0.1 s window and the 5% leak
/// past a broken shield.
///
/// IT ALSO PAYS FOR ITSELF ON THE WIRE. A row carries the factors that did
/// nothing by NAME, thirteen of them on an ordinary rifle hit, and the same
/// thirteen strings on every row of the fight: measured at **859 bytes an
/// event** and **17.2 MB** for a 20,000-row window, of which the repeated
/// names were the largest single share. An index into this table costs one or
/// two characters, and the table is sent once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Factor {
    /// `base damage bracket`
    BaseDamageBracket,
    /// `base damage mods`
    BaseDamageMods,
    /// `below half health`
    HalfHealth,
    /// `headshot damage`
    HeadshotDamage,
    /// `body part`
    BodyPart,
    /// `critical`
    Critical,
    /// `Condition Overload bracket`
    ConditionOverload,
    /// `faction`
    Faction,
    /// `arcane (final)`
    ArcaneFinal,
    /// `attrition`
    Attrition,
    /// `Warframe ability`
    WarframeAbility,
    /// `Warframe ability element`
    WarframeAbilityElement,
    /// `beam ramp`
    BeamRamp,
    /// `Double Tap`
    DoubleTap,
    /// `Synth Charge`
    SynthCharge,
    /// `Chamber (first round)`
    ChamberFirstRound,
    /// `sniper combo`
    SniperCombo,
    /// `multishot-as-damage`
    MultishotAsDamage,
    /// `multishot-generated`
    MultishotGenerated,
    /// `damage falloff`
    DamageFalloff,
    /// `radial falloff`
    RadialFalloff,
    /// `hop falloff`
    HopFalloff,
    /// `merged beams`
    MergedBeams,
    /// `element bracket`
    ElementBracket,
    /// `element bracket + quantization`
    ElementBracketQuantized,
    /// `extra hit share`
    ExtraHitShare,
    /// `field damage`
    FieldDamage,
    /// `Secondary Fortifier`
    SecondaryFortifier,
    /// `shield gate window`
    ShieldGateWindow,
    /// `pool share`
    PoolShare,
    /// `damage type column`
    DamageTypeColumn,
    /// `Disrupt amp`
    DisruptAmp,
    /// `past the shield`
    PastTheShield,
    /// `shield gate`
    ShieldGate,
    /// `Viral amp`
    ViralAmp,
    /// `armour`
    Armour,
    /// `attenuation`
    Attenuation,
    /// `the pool ran out`
    PoolRanOut,
}

impl Factor {
    /// Every factor, in the order their indices are on the wire. APPEND-ONLY
    /// for as long as a client older than the server can exist — which for a
    /// page served from the same deploy is never, so this is a convention
    /// rather than a ratchet.
    pub const ALL: [Factor; 38] = [
        Factor::BaseDamageBracket,
        Factor::BaseDamageMods,
        Factor::HalfHealth,
        Factor::HeadshotDamage,
        Factor::BodyPart,
        Factor::Critical,
        Factor::ConditionOverload,
        Factor::Faction,
        Factor::ArcaneFinal,
        Factor::Attrition,
        Factor::WarframeAbility,
        Factor::WarframeAbilityElement,
        Factor::BeamRamp,
        Factor::DoubleTap,
        Factor::SynthCharge,
        Factor::ChamberFirstRound,
        Factor::SniperCombo,
        Factor::MultishotAsDamage,
        Factor::MultishotGenerated,
        Factor::DamageFalloff,
        Factor::RadialFalloff,
        Factor::HopFalloff,
        Factor::MergedBeams,
        Factor::ElementBracket,
        Factor::ElementBracketQuantized,
        Factor::ExtraHitShare,
        Factor::FieldDamage,
        Factor::SecondaryFortifier,
        Factor::ShieldGateWindow,
        Factor::PoolShare,
        Factor::DamageTypeColumn,
        Factor::DisruptAmp,
        Factor::PastTheShield,
        Factor::ShieldGate,
        Factor::ViralAmp,
        Factor::Armour,
        Factor::Attenuation,
        Factor::PoolRanOut,
    ];

    /// What a reader is shown, and the key the i18n overlay is written against.
    pub fn name(self) -> &'static str {
        match self {
            Factor::BaseDamageBracket => "base damage bracket",
            Factor::BaseDamageMods => "base damage mods",
            Factor::HalfHealth => "below half health",
            Factor::HeadshotDamage => "headshot damage",
            Factor::BodyPart => "body part",
            Factor::Critical => "critical",
            Factor::ConditionOverload => "Condition Overload bracket",
            Factor::Faction => "faction",
            Factor::ArcaneFinal => "arcane (final)",
            Factor::Attrition => "attrition",
            Factor::WarframeAbility => "Warframe ability",
            Factor::WarframeAbilityElement => "Warframe ability element",
            Factor::BeamRamp => "beam ramp",
            Factor::DoubleTap => "Double Tap",
            Factor::SynthCharge => "Synth Charge",
            Factor::ChamberFirstRound => "Chamber (first round)",
            Factor::SniperCombo => "sniper combo",
            Factor::MultishotAsDamage => "multishot-as-damage",
            Factor::MultishotGenerated => "multishot-generated",
            Factor::DamageFalloff => "damage falloff",
            Factor::RadialFalloff => "radial falloff",
            Factor::HopFalloff => "hop falloff",
            Factor::MergedBeams => "merged beams",
            Factor::ElementBracket => "element bracket",
            Factor::ElementBracketQuantized => "element bracket + quantization",
            Factor::ExtraHitShare => "extra hit share",
            Factor::FieldDamage => "field damage",
            Factor::SecondaryFortifier => "Secondary Fortifier",
            Factor::ShieldGateWindow => "shield gate window",
            Factor::PoolShare => "pool share",
            Factor::DamageTypeColumn => "damage type column",
            Factor::DisruptAmp => "Disrupt amp",
            Factor::PastTheShield => "past the shield",
            Factor::ShieldGate => "shield gate",
            Factor::ViralAmp => "Viral amp",
            Factor::Armour => "armour",
            Factor::Attenuation => "attenuation",
            Factor::PoolRanOut => "the pool ran out",
        }
    }

    /// Its place in [`Factor::ALL`] — what travels instead of the name.
    pub fn index(self) -> usize {
        Factor::ALL.iter().position(|f| *f == self).expect("every factor is in ALL")
    }
}

/// against a translation, and the page needs to translate all of them.
pub type Step = (Factor, f64);

/// WHAT THE TARGET LOOKED LIKE THE INSTANT BEFORE THIS NUMBER LANDED.
///
/// BEFORE, not after, and that is the whole design: it makes the stream a chain
/// a reader can walk. The `n`-th event's pools must equal the `n−1`-th event's
/// pools minus what the `n−1`-th took out of them, so a damage instance that
/// was counted twice, or one that moved a pool and was never recorded, breaks
/// the chain at the row where it happened rather than showing up as a total
/// that is a bit too big.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TargetAt {
    pub overguard: f64,
    pub shield: f64,
    pub health: f64,
    /// The LIVE armour value, after every strip — the number the mitigation
    /// term actually read, not the unit's printed one.
    pub armor: f64,
    /// WHEN THE TARGET'S SHIELD-GATE WINDOW RUNS OUT, in seconds, or `None`
    /// when no shield has broken.
    ///
    /// Carried and drawn even though NOTHING THIS ENGINE FIRES TAKES IT: the
    /// window is the target's state, it is set correctly, and the only attack
    /// measured to read it is a melee GROUND SLAM, which is not modelled. Showing it is what makes the day
    /// melee lands a matter of one attack reading a field that is already
    /// right, and it is the only way to check that claim before then.
    pub shield_gate_until: Option<f64>,
}

/// ONE TERM INSIDE AN ADDITIVE BRACKET — `+0.80 Galvanized Shot`.
#[derive(Debug, Clone, PartialEq)]
pub struct Term {
    /// WHAT IT IS, or nothing.
    ///
    /// `None` where the engine holds only a SUM and cannot say which cards are
    /// in it — the base-damage bucket is resolved to one number long before a
    /// fight starts. A category label there ("mods", "arcane") invites a reader
    /// to look for a card that matches it and there is none, which is worse
    /// than a bare number: the point of this panel is that everything on it can
    /// be checked, and a name that cannot be is the one thing that breaks that.
    /// So it is the number alone until the resolver can name the card.
    pub factor: Option<Factor>,
    /// The term's own value, signed. `-0.15` for Anemic Agility.
    pub value: f64,
    /// What it is made of, when it is itself a product — Condition Overload is
    /// `rate x distinct status types`, and a reader checking a card wants the
    /// two numbers rather than their product.
    pub of: Option<(f64, f64)>,
}

/// ONE COMPONENT'S SNAP TO THE QUANTIZATION GRID.
#[derive(Debug, Clone, PartialEq)]
pub struct Snap {
    pub dtype: DamageType,
    pub before: f64,
    /// How many grid units that was, before rounding — the number that makes
    /// the mechanic legible: 89.81 units becoming 90 is a sentence, 2,178.00
    /// becoming 2,182.50 is not.
    pub units: f64,
    pub after: f64,
}

/// ONE LAYER OF THE OFFENSIVE LEDGER, and the shape SAYS which mechanic it is.
///
/// A flat list of multipliers makes the panel print things the game does not
/// have: Condition Overload is an ADDITIVE term in the base-damage bracket on
/// most weapons, so fitting it into a chain of multipliers means dividing the
/// bracket by itself and printing the quotient with a `x` in front of it, and
/// quantization is a per-component snap fitted in as a ratio of totals.
///
/// So a layer is one of three shapes and the shape is the information: a
/// BRACKET lists its terms and adds them, QUANTIZE shows the grid, and only a
/// MUL gets a multiplication sign. Anything the engine divided out cannot be
/// drawn at all, because there is no variant for it.
///
/// THE ENGINE MAY STILL EVALUATE IN ANY ORDER IT LIKES. A non-elemental base
/// bonus multiplies the quantization numerator AND its scale, so it commutes
/// with the snap — which is exactly why the engine can apply Condition Overload
/// afterwards and still be right. The ledger presents the GAME's order; the two
/// are not required to match, and the check that the row multiplies out is what
/// holds them together.
#[derive(Debug, Clone, PartialEq)]
pub enum Layer {
    /// `x (1 + Σ terms)`. An empty bracket is never emitted — that is where the
    /// pile of `x1.00` went.
    Bracket {
        factor: Factor,
        terms: Vec<Term>,
        /// `1 + Σ terms`.
        sum: f64,
        /// What the running total is after it.
        out: f64,
    },
    /// Each component snaps to a multiple of `scale` — see
    /// [`crate::damage::DamageVector::quantized_against`]. Not a multiplier and
    /// never drawn as one.
    Quantize {
        /// `ModifiedBase / 32`.
        scale: f64,
        components: Vec<Snap>,
        out: f64,
    },
    /// A real multiplicative bracket, and the only shape that earns a `x`.
    Mul {
        factor: Factor,
        value: f64,
        /// Its own expansion where it has one: a body part is
        /// `3.00 x (1 + 0.50 headshot damage)`, and the pair is what a reader
        /// checks against the enemy card.
        of: Vec<Term>,
        /// The head of that expansion — the part multiplier itself.
        head: f64,
        out: f64,
    },
}

/// ONE NUMBER THE GAME POPPED, and everything behind it.
#[derive(Debug, Clone, PartialEq)]
pub struct Damage {
    pub origin: Origin,
    /// WHICH PELLET OF THE TRIGGER PULL, counting from 1 — `None` for anything
    /// a pellet did not fire (a status tick, a field's clock).
    ///
    /// It is separate from [`Origin`] because a pellet that has an explosion
    /// produces TWO rows and they are the SAME pellet: the Laetum's Incarnon
    /// form fires three, so a shot is six numbers, and reading them as
    /// "pellet 1 direct, pellet 1 radial, pellet 2 direct, …" is the only
    /// arrangement in which the six add up to something a reader recognises. The engine already settles them in that order — the
    /// stage loop is inside the pellet loop — so what was missing was the
    /// label, and a radial row could not say which pellet threw it.
    pub pellet: Option<u32>,
    /// The EXPLOSION half of an attack that has one, rather than its collision.
    /// Not an [`Origin`]: it is not why the row exists, it is which part of the
    /// attack this is, and the two are asked of the same pellet.
    pub radial: bool,
    /// Which pool it came out of, and what colour it read as.
    pub pool: crate::dummy::Pool,
    pub dtype: DamageType,
    /// WHAT KIND OF NUMBER THE GAME DRAWS THIS AS — a crit, a headcrit, a
    /// status tick, a blast's radial. It is what decides the number's colour
    /// and size on screen, so it travels on the row that IS that number.
    ///
    /// Not derivable from the fields around it, which is why it is stored: a
    /// blast on the body that carried the stack and the radial it throws at
    /// everything else are the same [`Origin`], the same pool and the same
    /// type, and the game draws them differently.
    pub kind: crate::dummy::PopKind,
    /// The body part it landed on, where the instance had one. A status tick
    /// and an explosion do not, and saying so is not the same as leaving it
    /// blank — see MEASUREMENTS M54 for the rule about which DoTs inherit a
    /// weak point.
    pub part: Option<String>,
    pub head: bool,
    /// 0 = no crit, 1 = crit, 2+ = red and beyond.
    pub crit_tier: u32,
    /// THE OFFENSIVE LEDGER, layer by layer — see [`Layer`].
    ///
    /// `base` is where it starts: this weapon's own base damage, before any
    /// bracket. Every layer states what the running total is after it, so the
    /// last one's `out` is `raw` and a reader can check any step alone.
    pub base: f64,
    /// THE CRIT DAMAGE the crit factor was built from: the factor itself is
    /// `1 + crit_tier x (crit_damage - 1)`, and a reader checking a card wants
    /// the formula rather than the product.
    pub crit_damage: f64,
    pub layers: Vec<Layer>,
    pub raw: f64,
    /// THE DEFENSIVE LEDGER: `raw × Π mitigation = effective`. Written from
    /// `TargetState::apply`'s own breakdown rather than reconstructed, which is
    /// what stops the two from disagreeing.
    pub mitigation: Vec<Step>,
    pub effective: f64,
    pub before: TargetAt,
    /// Live stacks on the target, positionally matching
    /// [`crate::dummy::DEBUFF_ROSTER`] — the target's own half of "why is this
    /// number this size".
    pub debuffs: Vec<(u16, f64)>,
    /// LIVE BUFF STACKS on the shooter, positionally matching the roster in
    /// [`crate::record::Record::buffs`] — the other side of `debuffs`.
    ///
    /// A row's factors say what was multiplied in; this says what the build had
    /// UP at that instant, which is the question a reader asks when a factor is
    /// smaller than they expected.
    pub buffs: Vec<(u16, f64)>,
    /// What this instance APPLIED, which is a different question from what it
    /// was: a Corrosive hit can proc nothing.
    pub procs: Vec<DamageType>,
    /// …AND WHAT IT SET OFF ON THE SHOOTER, positional against the buff roster.
    ///
    /// The other half of `procs`, and the reason both exist: a row states what
    /// was up BEFORE it and what it set off, so the next row's state is the
    /// previous row's state plus this. That is a property a reader can check
    /// with their eyes, and it is what the two state columns were missing —
    /// they said what was true and never why it changed.
    pub triggered: Vec<u16>,
    /// Did the target die to this one.
    pub killed: bool,
}

/// A THING THAT HAPPENED. Damage is one kind of it and not the only one.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    /// BOXED, because it is an order of magnitude larger than every other
    /// variant and an enum is as big as its widest arm — a stream of 400,000
    /// events should not pay a damage event's size for a reload.
    Damage(Box<Damage>),
    /// A trigger pull: how many pellets it put out and what it cost.
    Shot {
        pellets: u32,
    },
    /// A pellet that went nowhere. It pops no number and it is why the pellet
    /// count and the damage-row count disagree.
    Miss {
        reason: &'static str,
    },
    ReloadStart {
        seconds: f64,
    },
    ReloadEnd,
    /// Into a transmuted form and out of it — the two ends of the same window,
    /// which is all a reader needs.
    TransformStart {
        seconds: f64,
        into_transmuted: bool,
    },
    TransformEnd {
        transmuted: bool,
    },
    /// A status ran out. It explains a tick that got smaller with nothing else
    /// on screen having changed.
    ///
    /// DECLARED, NOT YET EMITTED — expiry is a `ticks_left` running
    /// to zero inside `process_ticks` rather than an event anything announces.
    StatusExpired {
        dtype: DamageType,
        remaining: u16,
    },
    /// The body died. Under `InstantRespawn` it is standing again immediately,
    /// which is the scenario and not a bug — the pools jumping back up in the
    /// next row is what that looks like.
    ///
    /// DECLARED, NOT YET EMITTED as a row of its own: the damage
    /// event that finished the body already carries `killed`, so the fact is in
    /// the stream and does not yet have a line to itself.
    Killed,
}

/// A THING THAT HAPPENED, at a time, to somebody.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub id: u32,
    pub t: f64,
    /// WHOSE. `0` is the aimed body, `1..` index the formation — the same
    /// numbering `RunResult::damage_by_body` uses. `None` is the PLAYER's own:
    /// a reload belongs to no enemy, which is why a per-enemy view of this
    /// stream is a filter rather than a copy.
    pub subject: Option<u16>,
    /// WHICH SHOT BROUGHT THIS ABOUT, by event id. A bleed that lands four
    /// seconds after the round that seeded it points back at it, which is the
    /// only way to answer "what did that shot end up being worth" — the number
    /// nobody could compute before, because by the time a DoT settles the
    /// engine no longer knows whose it was.
    pub cause: Option<u32>,
    pub weapon: WeaponAt,
    pub kind: Kind,
}

/// THE STREAM, and the window it is being taken over.
///
/// A window rather than the whole fight because the whole fight is usually
/// small and occasionally enormous — 5,016 events on a Braton Prime and 408,817
/// on a Phantasma Prime over a 19x19 formation. Bounding
/// the reader's request rather than the engine's output is what lets the common
/// case be answered entire and the extreme case be answered at all.
#[derive(Debug, Clone, Default)]
pub struct Record {
    on: bool,
    from: f64,
    to: f64,
    limit: usize,
    /// What the instance being resolved has set off on the shooter so far.
    ///
    /// PENDING rather than written straight onto a row, because the bumps come
    /// FIRST: a pellet's on-hit and on-status triggers all fire before its row
    /// is pushed. Collected here and drained by the row, which is also what
    /// keeps the state column honest — the column is what was up BEFORE the
    /// instance, and this is what it changed.
    pending_triggers: Vec<u16>,
    /// How many of this window's events to pass over before keeping any — see
    /// [`Record::window`].
    skip: usize,
    events: Vec<Event>,
    /// EVENTS INSIDE THE WINDOW THAT DID NOT FIT. Carried because a cap nobody
    /// is told about reads as "that is everyone", which is this repo's rule
    /// about every other cap it has.
    dropped: u64,
    next_id: u32,
    /// The weapon as it stands, stamped onto each event. Held here so a row is
    /// self-contained without every call site having to pass it.
    weapon: WeaponAt,
    /// The shot currently being resolved, so what it spawns can point back at
    /// it. Set by [`Self::begin_shot`].
    shot: Option<u32>,
    /// WHAT THE BUFF STACKS ON EACH ROW ARE CALLED, in the order they are held.
    /// The same vocabulary the buff cards use, because they come from one place
    /// (`DummyParams::buff_roster`).
    buffs: Vec<String>,
    /// WHAT THE SHOOTER HAS UP, as `(stacks, expires at)`.
    ///
    /// An ABSOLUTE expiry rather than a countdown: it only moves when the buff
    /// is actually refreshed, so a row carrying it is identical to the row
    /// before it most of the time and the wire drops the repeat — a countdown
    /// would change on every row of the fight. `INFINITY` is a buff with no
    /// clock; `NAN` is one whose end this loop does not track, drawn as no time
    /// rather than as a guess.
    stacks: Vec<(u16, f64)>,
}

impl Record {
    /// Not recording. Allocates nothing; every `push` is one branch.
    pub fn off() -> Self {
        Self::default()
    }

    /// Record everything in `[from, to)`, up to `limit` events, after skipping
    /// the first `skip` of them.
    ///
    /// SKIP IS WHAT MAKES THE WHOLE FIGHT READABLE. A window bounds one read;
    /// paging by OFFSET is what lets several reads cover a stream no single one
    /// can hold, and it is offset rather than time because a page boundary can
    /// fall in the middle of an instant — several numbers share a timestamp, so
    /// "continue from t" either loses them or repeats them.
    ///
    /// IT ALSO MAKES AN ID MEAN SOMETHING. An event's id is its place in the
    /// FIGHT, counted before the skip, so pages concatenate into one stream and
    /// the floating numbers can still name their row across a boundary.
    pub fn window(from: f64, to: f64, limit: usize, skip: usize) -> Self {
        Self {
            on: true,
            from,
            to,
            limit,
            skip,
            ..Self::default()
        }
    }

    pub fn is_on(&self) -> bool {
        self.on
    }

    /// Name the buff roster the rows' stack lists index into.
    pub fn set_buffs(&mut self, ids: Vec<String>) {
        self.buffs = ids;
    }

    pub fn buffs(&self) -> &[String] {
        &self.buffs
    }

    /// THE SHOOTER'S LIVE STACKS from here on, positionally against
    /// [`Self::buffs`]. Held rather than sampled per row for the same reason
    /// the weapon is: the sampler needs a dozen of the run loop's own locals.
    ///
    /// A row between two shots therefore carries the count as of the shot that
    /// preceded it — which is exact for everything a shot changes, and up to
    /// one shot stale for a buff that expires on its own clock.
    pub fn set_stacks(&mut self, stacks: Vec<(u16, f64)>) {
        self.stacks = stacks;
    }

    pub fn stacks(&self) -> &[(u16, f64)] {
        &self.stacks
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// The weapon's state from here on. Called where it CHANGES rather than at
    /// every event, so the two cannot disagree about when a magazine emptied.
    pub fn set_weapon(&mut self, w: WeaponAt) {
        self.weapon = w;
    }

    pub fn weapon(&self) -> WeaponAt {
        self.weapon
    }

    /// Open a shot: everything pushed until the next call points back at this
    /// event as its cause. Returns the id so a caller can hold it across a
    /// boundary the recorder does not see.
    pub fn begin_shot(&mut self, t: f64, pellets: u32) -> Option<u32> {
        // CLEARED FIRST. A shot IS a cause and has none of its own — leaving
        // the previous one in place made every trigger pull point at the one
        // before it, which reads as a chain of shots causing shots (found by
        // looking at a real record).
        self.shot = None;
        let id = self.push(t, None, Kind::Shot { pellets });
        self.shot = id;
        id
    }

    /// What the current shot's id is, for damage that resolves later — a DoT
    /// seeded now and paid four seconds from here.
    pub fn shot(&self) -> Option<u32> {
        self.shot
    }

    /// Attribute what follows to a shot other than the live one — a DoT tick
    /// belongs to the round that seeded it, not to whatever is being fired now.
    pub fn attribute_to(&mut self, cause: Option<u32>) -> Option<u32> {
        std::mem::replace(&mut self.shot, cause)
    }

    /// IS ANYTHING AT `t` GOING TO BE KEPT? Asked BEFORE a row's arguments are
    /// built, which is the difference between a windowed read costing what it
    /// keeps and costing the whole fight.
    ///
    /// A window is the ordinary case on a dense build — the board's leading
    /// Laetum deals ~230,000 damage instances over 180 s and the cap is a
    /// fraction of that — so a reader scrubbed to the last ten seconds was
    /// paying for a `TargetAt` snapshot, three Vecs and a String on every one
    /// of the instances before it, all of them thrown away by `push`.
    /// IT COUNTS WHAT IT REFUSES, or the panel lies about its own cap:
    /// `dropped` is `push`'s counter, so short-circuiting here leaves a damage
    /// row past the limit never reaching it — a 180 s fight cut off at 146 s
    /// reported **542** left out when the truth was tens of thousands.
    ///
    /// ONE PER INSTANCE, not per number: a hit on a shielded body is two rows
    /// and this is asked before the portions are known. It undercounts a
    /// shielded fight slightly and is exact everywhere else — which is the
    /// honest trade against not building the row at all.
    pub fn wants(&mut self, t: f64) -> bool {
        if !self.on || t < self.from || t >= self.to {
            return false;
        }
        if self.events.len() >= self.limit {
            self.dropped += 1;
            return false;
        }
        true
    }

    /// A BUFF THIS INSTANCE SET OFF, held until the row is written.
    pub fn triggered(&mut self, buff: usize) {
        if !self.on {
            return;
        }
        let b = buff.min(u16::MAX as usize) as u16;
        if !self.pending_triggers.contains(&b) {
            self.pending_triggers.push(b);
        }
    }

    /// What this instance has set off so far — drained onto its row.
    pub fn take_triggers(&mut self) -> Vec<u16> {
        self.pending_triggers.clone()
    }

    /// Open a fresh instance: nothing it has not caused belongs to it.
    pub fn begin_instance(&mut self) {
        self.pending_triggers.clear();
    }

    /// Is this page finished — i.e. would another read find more?
    pub fn skipped(&self) -> usize {
        self.skip
    }

    /// Append. Returns the event's id, or `None` when nothing was recorded —
    /// which is the ordinary answer on 999 runs out of a thousand.
    pub fn push(&mut self, t: f64, subject: Option<u16>, kind: Kind) -> Option<u32> {
        if !self.on || t < self.from || t >= self.to {
            return None;
        }
        // THE ID IS THE PLACE IN THE FIGHT, assigned BEFORE the skip so pages
        // concatenate into one stream rather than three streams each starting
        // at zero.
        let id = self.next_id;
        self.next_id += 1;
        if (id as usize) < self.skip {
            return None;
        }
        if self.events.len() >= self.limit {
            self.dropped += 1;
            return None;
        }
        self.events.push(Event {
            id,
            t,
            subject,
            cause: self.shot,
            weapon: self.weapon,
            kind,
        });
        Some(id)
    }
}
