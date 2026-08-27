//! THE COMBAT RECORD — one ordered stream of everything that happened in a
//! fight, and the authority on what it was.
//!
//! # A row is a thing that happened
//!
//! Every other output this engine produces is a CURVE or a TOTAL, and both hide
//! an error inside an average: a factor applied twice moves a mean by a few per
//! cent and reads as a build being good. The record is the opposite — a stream
//! of discrete events, each carrying every number behind it, so any one of them
//! can be checked by hand and the whole run can be diffed against another.
//!
//! **A DAMAGE EVENT IS ONE NUMBER THE GAME POPS.** Not "one hit": a pellet that
//! lands on a shielded body pops two numbers, because Toxin bypasses the shield
//! and its siblings do not, and the game shows them side by side. That 1:1 with
//! the screen is what makes this the only output of this app that can be laid
//! beside a recording and checked — which for a product whose promise is
//! "matches in-game measurements" is the final arbiter (owner, 2026-08-27).
//!
//! **EVERYTHING IS AN EVENT, not just damage** (owner, 2026-08-27). A reload, a
//! transmute into an Incarnon form, a pellet that missed, a status running out
//! — none of them pop a number and all of them explain the stream around them.
//! A Warframe casting an ability and a body moving are the same shape and are
//! not modelled yet; they arrive as new [`Kind`] variants and nothing else has
//! to change. The consequence worth stating: a weapon event belongs to NOBODY,
//! so a per-enemy view of this stream is a FILTER (`subject == that body, or
//! none`) rather than the same event copied into every enemy's table.
//!
//! # It is the write path, not a report
//!
//! A log written *beside* the simulation is a report, and a report can drift
//! from what it reports; the checks over it can then only prove it is
//! self-consistent. This one is filled by the same call that mutates the
//! target's pools, from the same numbers, which is what makes "the sum of the
//! record is the damage total" true by construction rather than by assertion.
//!
//! What it is authoritative about is bounded, and the bounds are worth saying:
//!
//! * **WHAT happened** — amounts, order, pools, whose body — by construction.
//! * **WHY** — only half. The factor NAMES are hand-written strings, and no
//!   mechanism ties `"critical"` to the 4.4 beside it; a check can prove the
//!   product equals the damage and cannot prove the label is right.
//! * **One engagement**, not a score. A board number is the mean over a
//!   thousand runs; this is the median one. It can show that this fight's every
//!   number is right and says nothing about whether the mean is.
//!
//! # Cost
//!
//! Measured 2026-08-27 over one 180 s engagement: an ordinary fight deals
//! **2,000–5,000** damage instances (Braton Prime single target 5,016; Torid on
//! the 19x19 group ruler 3,690), so its whole record fits in a megabyte and can
//! be handed over entire. The worst measured build — Phantasma Prime, eight
//! status mods, 361 bodies — deals **408,817**, which is why [`Record`] takes a
//! WINDOW and a cap rather than assuming the small case.
//!
//! Recording is off for every run nobody is reading, which is 999 of a
//! thousand: [`Record::off`] allocates nothing and every `push` is one branch.

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
    /// happen (owner, 2026-08-27).
    pub idle_magazine: Option<(u32, u32)>,
    /// THE INCARNON GAUGE, and how much of it fills the form — `None` on a
    /// weapon that has no cycle.
    ///
    /// WHAT IS LEFT IN RESERVE, or `None` where the fight grants infinite ammo
    /// (which every ruler does). Beside the magazine because they are one
    /// question — "can this weapon keep firing" — and a reader asking it should
    /// not have to hold two columns in their head (owner, 2026-08-27).
    pub reserve: Option<f64>,
    /// A fight STARTS WITH AN EMPTY ONE, which is a real property of the model
    /// and was invisible: the record showed a base form firing and then, with
    /// no warning, a transform. What charges it is the weapon's own rule —
    /// weak-point hits, direct hits, or kills — so a reader watching this
    /// number climb is watching the thing that decides when the earned form
    /// arrives (owner, 2026-08-27).
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
    /// A lingering field's own clock (the Torid's cloud).
    Field,
    /// An EXTRA HIT — a second damage instance beside a hit (docs/EXTRA_HIT.md).
    ExtraHit,
    /// An arcane's or a syndicate's own instance.
    Arcane,
}

impl Origin {
    pub fn name(self) -> &'static str {
        match self {
            Origin::Own => "own",
            Origin::Multishot => "multishot",
            Origin::PunchThrough => "punch_through",
            Origin::Chain => "chain",
            Origin::Ricochet => "ricochet",
            Origin::Echo => "echo",
            Origin::Splash => "splash",
            Origin::Status => "status",
            Origin::Field => "field",
            Origin::ExtraHit => "extra_hit",
            Origin::Arcane => "arcane",
        }
    }
}

/// ONE FACTOR, with the name it is known by on a card or a wiki page.
///
/// A `&'static str` because every label in this engine is a literal: a factor
/// whose name had to be built at runtime would be a factor nobody could join
/// against a translation, and the page needs to translate all of them.
pub type Step = (&'static str, f64);

/// WHAT THE TARGET LOOKED LIKE THE INSTANT BEFORE THIS NUMBER LANDED.
///
/// BEFORE, not after, and that is the whole design: it makes the stream a chain
/// a reader can walk. The `n`-th event's pools must equal the `n−1`-th event's
/// pools minus what the `n−1`-th took out of them, so a damage instance that
/// was counted twice, or one that moved a pool and was never recorded, breaks
/// the chain at the row where it happened rather than showing up as a total
/// that is a bit too big (owner, 2026-08-27).
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
    /// measured to read it is a melee GROUND SLAM, which is not modelled
    /// (owner, 2026-08-27 — MEASUREMENTS M61). Showing it is what makes the day
    /// melee lands a matter of one attack reading a field that is already
    /// right, and it is the only way to check that claim before then.
    pub shield_gate_until: Option<f64>,
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
    /// arrangement in which the six add up to something a reader recognises
    /// (owner, 2026-08-27). The engine already settles them in that order — the
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
    /// THE OFFENSIVE LEDGER: `base × Π steps = raw`.
    ///
    /// `base` is this instance's own modded damage before anything below it —
    /// one pellet's share on a multishot weapon.
    pub base: f64,
    /// WHERE `base` STARTED, when the engine can say — this attack part's
    /// ModifiedBase. `base_from x Pi base_steps = base`, and the two together
    /// turn one opaque number into a chain a reader can check against the
    /// build panel (owner, 2026-08-27).
    ///
    /// 0.0 where the site has nothing more to say: a status tick's base IS the
    /// seed it was frozen with, and decomposing it means naming facts about a
    /// hit that is over — the row `Event::cause` points at carries those.
    pub base_from: f64,
    pub base_steps: Vec<Step>,
    /// THE CRIT DAMAGE the crit factor was built from: the factor itself is
    /// `1 + crit_tier x (crit_damage - 1)`, and a reader checking a card wants
    /// the formula rather than the product.
    pub crit_damage: f64,
    pub steps: Vec<Step>,
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
    pub debuffs: Vec<u16>,
    /// LIVE BUFF STACKS on the shooter, positionally matching the roster in
    /// [`crate::record::Record::buffs`] — the other side of `debuffs`.
    ///
    /// A row's factors say what was multiplied in; this says what the build had
    /// UP at that instant, which is the question a reader asks when a factor is
    /// smaller than they expected (owner, 2026-08-27).
    pub buffs: Vec<u16>,
    /// What this instance APPLIED, which is a different question from what it
    /// was: a Corrosive hit can proc nothing.
    pub procs: Vec<DamageType>,
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
    /// A PELLET ARRIVED. One trigger pull is `Shot`; each pellet of it that
    /// reaches a body is one of these, and the numbers it produces hang off it.
    ///
    /// IT IS NOT A DAMAGE ROW AND IT POPS NOTHING. What it carries is the
    /// flight — where the round went and what it had left when it got there —
    /// which belongs to no damage number and had nowhere else to live: a hit on
    /// a shielded body produces TWO numbers and they are ONE arrival, and a
    /// round with punch through strikes several bodies on one trigger pull.
    /// Both facts were previously inferred from two rows sharing a timestamp
    /// (owner, 2026-08-27).
    Hit {
        /// The part it landed on, and whether that part is a weak point — the
        /// same pick every damage row of this pellet reports.
        part: Option<String>,
        head: bool,
        /// How far the round actually flew: muzzle to the body's surface, the
        /// number the arena prints (MECHANICS §11).
        flew_m: f64,
        /// What is left of the weapon's reach past this body, where it declares
        /// one. `None` is every weapon that does not.
        range_left_m: Option<f64>,
        /// What is left of the punch-through budget after crossing this body —
        /// `BODY_MATERIAL_M` scaled by the chord actually crossed, so a body
        /// clipped at the rim costs almost nothing. `None` with no budget.
        punch_through_left_m: Option<f64>,
    },
    Miss {
        reason: &'static str,
    },
    ReloadStart {
        seconds: f64,
    },
    ReloadEnd,
    /// Into a transmuted form and out of it — the two ends of the same window,
    /// which is all a reader needs (owner, 2026-08-27).
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
    /// DECLARED, NOT YET EMITTED (2026-08-27) — expiry is a `ticks_left` running
    /// to zero inside `process_ticks` rather than an event anything announces.
    StatusExpired {
        dtype: DamageType,
        remaining: u16,
    },
    /// The body died. Under `InstantRespawn` it is standing again immediately,
    /// which is the scenario and not a bug — the pools jumping back up in the
    /// next row is what that looks like.
    ///
    /// DECLARED, NOT YET EMITTED as a row of its own (2026-08-27): the damage
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
/// on a Phantasma Prime over a 19x19 formation (measured 2026-08-27). Bounding
/// the reader's request rather than the engine's output is what lets the common
/// case be answered entire and the extreme case be answered at all.
#[derive(Debug, Clone, Default)]
pub struct Record {
    on: bool,
    from: f64,
    to: f64,
    limit: usize,
    /// The pellet arrival being resolved, if any — see [`Record::begin_hit`].
    hit: Option<u32>,
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
    stacks: Vec<u16>,
}

impl Record {
    /// Not recording. Allocates nothing; every `push` is one branch.
    pub fn off() -> Self {
        Self::default()
    }

    /// Record everything in `[from, to)`, up to `limit` events.
    pub fn window(from: f64, to: f64, limit: usize) -> Self {
        Self {
            on: true,
            from,
            to,
            limit,
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
    pub fn set_stacks(&mut self, stacks: Vec<u16>) {
        self.stacks = stacks;
    }

    pub fn stacks(&self) -> &[u16] {
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
        // looking at a real record, 2026-08-27).
        self.shot = None;
        self.hit = None;
        let id = self.push(t, None, Kind::Shot { pellets });
        self.shot = id;
        id
    }

    /// Open a HIT: this pellet arrived, and what it goes on to do points back
    /// at the arrival rather than straight at the trigger pull.
    ///
    /// The hit's OWN cause is the shot, because `self.hit` is cleared before it
    /// is pushed — so the chain reads shot → hit → number, one level per thing
    /// that actually happened.
    pub fn begin_hit(&mut self, t: f64, subject: Option<u16>, kind: Kind) -> Option<u32> {
        self.hit = None;
        let id = self.push(t, subject, kind);
        self.hit = id;
        id
    }

    /// Close it. A pellet that MISSED causes nothing, and an explosion thrown
    /// by a round that missed belongs to the shot rather than to an arrival
    /// that never happened.
    pub fn end_hit(&mut self) {
        self.hit = None;
    }

    /// What the current shot's id is, for damage that resolves later — a DoT
    /// seeded now and paid four seconds from here.
    pub fn shot(&self) -> Option<u32> {
        self.shot
    }

    /// Attribute what follows to a shot other than the live one — a DoT tick
    /// belongs to the round that seeded it, not to whatever is being fired now.
    pub fn attribute_to(&mut self, cause: Option<u32>) -> Option<u32> {
        // …AND NEVER TO A HIT THAT IS NO LONGER HAPPENING. A DoT tick names the
        // round that seeded it; leaving a live hit in place would have it name
        // whichever pellet the loop was on when the tick fell due.
        self.hit = None;
        std::mem::replace(&mut self.shot, cause)
    }

    /// Append. Returns the event's id, or `None` when nothing was recorded —
    /// which is the ordinary answer on 999 runs out of a thousand.
    pub fn push(&mut self, t: f64, subject: Option<u16>, kind: Kind) -> Option<u32> {
        if !self.on || t < self.from || t >= self.to {
            return None;
        }
        if self.events.len() >= self.limit {
            self.dropped += 1;
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.events.push(Event {
            id,
            t,
            subject,
            // THE NEAREST THING THAT CAUSED IT: the arrival if a pellet is
            // being resolved, the trigger pull otherwise.
            cause: self.hit.or(self.shot),
            weapon: self.weapon,
            kind,
        });
        Some(id)
    }
}
