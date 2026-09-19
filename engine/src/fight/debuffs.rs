use super::*;

/// Target-side debuff state — the payloads of all proc types a combined
/// vector can produce (data/debuffs/*.yaml; simplifications noted inline).
#[derive(Default)]
pub(super) struct DebuffState {
    /// THE TICK SCHEDULER'S SCRATCH, reused across calls.
    ///
    /// `process_ticks` runs once per SHOT per BODY — a 19x19 ruler is ~300,000
    /// calls a run — so building a fresh heap each time cost more than the scan
    /// it replaced (measured: 444 multishot a run -> 772). It lives here rather than in
    /// a local because the capacity is what is worth keeping, and a body's
    /// schedule is the same size from one shot to the next.
    ///
    /// SCRATCH, not state: it holds nothing between calls and is refilled from
    /// `dots`/`heat`/`blast` every time. Taken out and put back around the loop,
    /// which is what lets the loop hold `&mut self` for everything else.
    pub(super) tick_q: std::collections::BinaryHeap<std::cmp::Reverse<TickKey>>,
    /// The clock reading this state was last pruned at — see [`Self::prune`].
    ///
    /// An OPTION rather than a sentinel float: the derived default of an `f64`
    /// is 0.0, which is a real instant here — the first shot of every
    /// engagement is fired at it — so a bare number would have skipped the one
    /// prune it must never skip. `None` cannot collide with a clock reading.
    pub(super) pruned_at: Option<f64>,
    /// GAS CLOUDS AND TESLA ARCS THIS BODY IS THE ORIGIN OF, waiting to be
    /// handed to everybody standing near it.
    ///
    /// Gas and Electricity are the only two elements whose proc is an AREA:
    /// gas leaves "a gas cloud that deals a tick of damage each second to all
    /// enemies within a 3-meter radius", and an Electricity proc "chains
    /// between nearby enemies" where "only the original target will be
    /// stunned". SO ONLY THE DoT TRAVELS — the stun, the arcane triggers and
    /// the stack counts stay on the body that was hit.
    ///
    /// AN OUTBOX RATHER THAN A THREADED QUEUE: `settle_procs` holds ONE body's
    /// state and spreading needs every body's, while the DRAIN knows which body
    /// it is reading. Nothing moves here, so draining a moment later is exact.
    /// …AND HOW MANY OF THAT EXACT CLOUD. Within one shot every gas proc
    /// produces the SAME cloud — same value, radius and tick time, from the
    /// same instant of the same shot — so they are one entry and a COUNT.
    ///
    /// It is what makes a crowd affordable: the drain hands each entry to every
    /// body in radius, a dozen on a 1.5 m grid, and the receiver keeps at most
    /// its stack cap.
    pub(super) area_out: Vec<(Dot, f64, u16)>,
    /// INSTANT area damage this body owes its neighbours — today only a
    /// simultaneous BLAST detonation. `(damage, radius_m)`.
    ///
    /// Separate from `area_out` because a detonation is a HIT and not a DoT: it
    /// lands once, and it carries no `dtype` for the neighbour to count as a
    /// status ("inherits no additional status effects"). A one-tick DoT would
    /// have been both of those things wrongly.
    pub(super) area_hit: Vec<AreaHit>,
    /// MICROWAVE — the Nukor family's own status effect, and the only one in
    /// this file that carries no payload at all.
    ///
    /// VERBATIM (wiki `Microwave`): *"a unique Status Effect that enlarges the
    /// body parts of the enemies that was shot at … not listed in the game UI,
    /// however is counted towards the damage calculation bonus with
    /// Condition-Overload type equipment"* — Condition Overload, Galvanized
    /// Aptitude, Galvanized Savvy, Galvanized Shot, Secondary Shiver, the Cedo.
    ///
    /// A BOOL, not a stack list, because the Kuva Nukor's page says its procs
    /// have INFINITE duration. So it is on or it is off, and the only thing
    /// that turns it off is the target dying — which resets this whole struct.
    ///
    /// It is worth modelling precisely because it is invisible: on a CO build
    /// this weapon carries one more status TYPE than its damage vector can
    /// explain, and a sim that counted the vector alone was one type short for
    /// the whole engagement.
    pub(super) microwave: bool,
    /// LIFTED expiry — a crowd-control state, and the second status in this
    /// struct whose whole payload is that it COUNTS.
    ///
    /// The wiki files it under `Status_Effect` §"Independent from Damage": it
    /// comes from a specific weapon rather than from the type draw, so it never
    /// competes for the roll and never renormalises anyone else's. Its
    /// combat effect is to suspend the target, which this arena has no concept
    /// of (docs/UNMODELLED.md §"no movement") — what it does have is Condition
    /// Overload, which counts Lifted (MECHANICS §"Condition Overload").
    ///
    /// ONE EXPIRY, not a stack list: it is a STATE, so a second application
    /// refreshes it rather than adding to it. 1 s.
    pub(super) lifted: Option<f64>,
    /// KNOCKED DOWN — the third invisible status, and the one melee cannot
    /// avoid: *"Universal: Players and enemies fall to the ground. Counts as
    /// an individual status for Condition Overload, Galvanized Aptitude,
    /// Galvanized Savvy, and Galvanized Shot"* (wiki, Status Effect
    /// §"Independent from Damage", verbatim).
    ///
    /// EVERY SLAM FORCES IT, so on a slam build it is a permanent Condition
    /// Overload stack and worth +80% — which is why it could not be left out
    /// and why its duration matters. Its CC half is not modelled and cannot be:
    /// this arena has no movement (docs/UNMODELLED.md).
    ///
    /// ONE EXPIRY, not a stack list, for the same reason `lifted` is: it is a
    /// STATE, so a second application refreshes it.
    pub(super) knockdown: Option<f64>,
    /// Stagger stack expiries (6 s, FIFO). No combat payload on a dummy.
    pub(super) stagger: Vec<f64>,
    /// Weakened stacks (10 s): +5% flat crit chance received per stack.
    pub(super) weakened: Vec<f64>,
    /// FLENSING SPIKES' count: BULLETS that have landed a Puncture proc on this
    /// body, and it never goes down (MEASUREMENTS M102). Not a stack count —
    /// one bullet proccing Puncture twice still counts once — and not a
    /// window: the armour does not come back when the Puncture lapses.
    pub(super) flensed: u32,
    /// The bullet (`RunResult::pellets` at its landing) that last counted
    /// above, so a second proc off the same bullet — its explosion, a
    /// multi-proc — is the same bullet and not another.
    pub(super) flensed_by: Option<u32>,
    /// Freeze (Cold) stacks: +0.10/+0.05 flat crit DAMAGE received; cap 4
    /// while overguard holds (never Frozen). The 10th proc consumes all
    /// stacks and enters the Frozen state (`frozen_until`).
    pub(super) freeze: Vec<f64>,
    /// The Frozen state (data/debuffs/frozen.yaml): mutually exclusive
    /// with Freeze — the 10th Cold proc consumes the 9 stacks; while
    /// active, Cold procs are inert and crit damage received is +1.00
    /// flat; on expiry Freeze is SET to exactly 3 fresh 6 s stacks.
    pub(super) frozen_until: Option<f64>,
    /// Disrupt (Magnetic) stacks: shield/overguard damage taken
    /// × (2 + 0.25·(stacks−1)), live.
    pub(super) disrupt: Vec<f64>,
    /// Virus (Viral) stacks: HEALTH damage taken × (2 + 0.25·(stacks−1)),
    /// live per tick (the official live-evaluation example).
    pub(super) virus: Vec<f64>,
    /// Corrosion stacks (8 s): armor × (1 − (0.20 + 0.06·stacks)).
    pub(super) corrosion: Vec<f64>,
    /// JAHU CANTICLE'S accumulated armour strip on THIS body — see
    /// [`DebuffState::canticle_strip`]. A fraction, never a count, and it never
    /// expires: the card states no duration.
    pub(super) canticle_armor_strip: f64,
    /// Confusion (Radiation): no single-target combat payload; tracked for
    /// CO type counting.
    pub(super) confusion: Vec<f64>,
    /// The Bullet Attractor (Void), same treatment as Confusion and for the
    /// same reason: no single-target combat payload — it redirects fire in a
    /// 2.5 m field and nobody shoots back here — but Void IS on Condition
    /// Overload's list of counting procs, so an extra hit that lands one is
    /// worth a CO stack and that is the whole of what it is worth.
    ///
    /// ONE ENTRY. The wiki describes a field on the target, not a stack count,
    /// and a re-proc moves it to where the new hit landed rather than adding a
    /// second one.
    pub(super) attractor: Vec<f64>,
    pub(super) blast: Vec<BlastStack>,
    pub(super) dots: Vec<Dot>,
    pub(super) heat: Option<HeatEntity>,
    /// Heat armor-strip ramp-DOWN (ignite.yaml): after the entity dies at
    /// `.0` with strip `.1`, armor returns 50→40→30→15→0% in 1.5 s steps.
    pub(super) heat_decay: Option<(f64, f64)>,
}

pub(super) const STAGGER_DURATION: f64 = 6.0;
pub(super) const STAGGER_CAP: usize = 5;
pub(super) const WEAKENED_DURATION: f64 = 10.0;
pub(super) const WEAKENED_CAP: usize = 5;
pub(super) const WEAKENED_FLAT_CC_PER_STACK: f64 = 0.05;
pub(super) const BLEED_COEFFICIENT: f64 = 0.35;
pub(super) const BLEED_DELAY: f64 = 1.0;
pub(super) const BLEED_TICKS: u32 = 6;
pub(super) const DOT_COEFFICIENT: f64 = 0.5; // Toxin/Electricity/Heat/Gas ticks

/// Does this status's DoT inherit the WEAK-POINT multiplier of the hit that
/// applied it?
///
/// EVERY ONE OF THEM DOES, and all five are now measured (M91): one weapon,
/// one base, four rows apiece, and Heat, Toxin, Electricity and Gas produce the
/// same four numbers. Toxin stood alone here on a reading of M54's that the
/// cleaner fixture does not reproduce — so the exception is gone rather than
/// moved, and the wiki's sentence holds for the whole family.
///
/// THE SEAM STAYS. It is what makes the next status that turns out not to a
/// one-line change, and it is the only place the question is asked; a bare
/// `part_factor` at the call site would put it back into the arithmetic where
/// nobody can see it. Heat and Blast do not come through here — Heat is a
/// singleton accumulator with its own arm (M90) and a blast is not a DoT.
pub(super) fn dot_takes_weakpoint(_t: DamageType) -> bool {
    true
}

/// Does this status's TICK land on a body part of its own, beside the part the
/// hit struck? Electricity and Gas, and no other (M100): wiki `Enemy_Body_Parts`
/// lists "Electricity and Gas status procs" as "always defaulting to the 1x
/// multiplier, unaffected by acuity-like bonuses but affected by deadhead-like
/// bonuses", and Heat, Toxin and Blast measure at the hit's part alone.
pub(super) fn lands_on_a_part(t: DamageType) -> bool {
    matches!(t, DamageType::Electricity | DamageType::Gas)
}

/// HOW OFTEN A TESLA ARC LANDS ON A NEIGHBOUR'S HEAD: 10 of 189 neighbours,
/// 95% interval about 3% to 9.5% (M100). In game the answer is fixed by where
/// each body stands; this plane has no height to decide it, so it is a rate.
pub(super) const TESLA_HEAD_LANDING_CHANCE: f64 = 10.0 / 189.0;
pub(super) const STATUS_DURATION: f64 = 6.0; // the standard proc duration
/// LIFTED's duration. Not the standard 6 s: an independent
/// proc is its own effect with its own timer, and DE publishes none for this
/// one. See [`DebuffState::lifted`].
pub(super) const LIFTED_SECONDS: f64 = 1.0;

/// HOW LONG A KNOCKDOWN LASTS — **A STAND-IN, NOT A MEASUREMENT**.
///
/// DE publishes none and the wiki's own row says only that the target "fall[s]
/// to the ground"; the whole page is flagged by the wiki as under-researched.
/// It takes `LIFTED_SECONDS`' value for the one reason available — the sibling
/// status in the same table already stands at 1 s — and that is a symmetry
/// argument, which is the weakest kind this repo accepts.
///
/// IT REACHES A NUMBER, so it must be measured. Every slam forces a knockdown,
/// so on a slam build this decides whether Condition Overload reads one more
/// type between slams: at 1 s a Magistar heavy-slam loop (about 1.5 s a swing)
/// drops the stack before the next slam lands and at 2 s it never does, which
/// is 80% of a base-damage bucket either way. Flagged for the owner.
pub(super) const KNOCKDOWN_SECONDS: f64 = 1.0;
pub(super) const CORROSION_DURATION: f64 = 8.0;
/// Bullet Attractor (Void): "a small 2.5 metre radius field ... for 3 seconds"
/// (wiki Damage/Void_Damage). Shorter than the standard 6 s, which is why it is
/// its own constant rather than `STATUS_DURATION`.
pub(super) const ATTRACTOR_DURATION: f64 = 3.0;
pub(super) const TEN_STACK_CAP: usize = 10;
pub(super) const BLAST_COEFFICIENT: f64 = 0.3;
pub(super) const BLAST_FUSE: f64 = 1.5;
/// THE OTHER HALF OF A BLAST STACK, and it is the bigger half.
///
/// Verbatim: "all stacks are dealt simultaneously as enemies within **5**
/// meters are dealt **300%** of base damage per proc", against the 30% the host
/// takes — so a stack is worth TEN TIMES as much to a neighbour as to the body
/// carrying it. "The initial target of the blast procs is not dealt this AoE
/// damage, only the single target damage."
///
/// SIMULTANEOUS IS THE TRIGGER, not the fuse: a stack that simply burns down
/// its own 1.5 s deals the single-target hit and nothing else. The two
/// simultaneous triggers are "reaching 10 blast stacks" and "the target dying".
pub(super) const BLAST_AOE_COEFFICIENT: f64 = 3.0;
pub(super) const BLAST_AOE_RADIUS_M: f64 = 5.0;
pub(super) const FREEZE_CAP_UNDER_OVERGUARD: usize = 4;
pub(super) const FROZEN_DURATION: f64 = 3.0;
pub(super) const FROZEN_CRIT_DAMAGE_RECEIVED: f64 = 1.00;
pub(super) const FROZEN_RESET_STACKS: usize = 3;
pub(super) const HEAT_STRIP_DECAY: [f64; 5] = [0.50, 0.40, 0.30, 0.15, 0.0];
pub(super) const HEAT_STRIP_DECAY_INTERVAL: f64 = 1.5;

/// Whole Heat strip steps taken after `elapsed` seconds. AN INSTANT ON A STEP
/// HAS TAKEN IT: the +1 s tick lands on a boundary, and a bare floor put
/// `born + 1.0 - born` at 1.9999… for ~1% of ignitions, a step short.
pub(super) fn heat_strip_steps(elapsed: f64, interval: f64) -> f64 {
    (elapsed / interval + 1e-9).floor()
}

/// THE ARC A SWING SWEEPS in front of the wielder, degrees, centred on the aim.
///
/// A STAND-IN, and the only invented number in [`FightParams::melee_struck`]:
/// the game publishes an arc for no stance, and the real answer is per ATTACK
/// INPUT — each animation covers its own wedge. Ninety degrees stands in for
/// all of them until those are measured. A `360deg` swing is published and
/// bypasses this; it is not an approximation of anything.
pub(super) const MELEE_ARC_DEG: f64 = 90.0;

/// Live target-side damage-taken modifiers at one instant (the tick-time
/// mitigation pipeline — defender-side, evaluated per hit/tick).
pub(super) struct Mitigation {
    pub(super) disrupt_amp: f64,
    pub(super) virus_amp: f64,
    /// THE LADDER BEHIND `virus_amp`, carried because an AVERAGED amp cannot
    /// be read back into an average stack count: `ten_stack_amp` steps 1.0 →
    /// 2.0 at its foot and by 0.25 everywhere above, so the inverse of a mean
    /// is not the mean of the inverse.
    pub(super) virus_stacks: u32,
    /// (1 − heat strip) × (1 − corrosive strip), applied to armor VALUE.
    pub(super) armor_multiplier: f64,
}

/// WHAT REACHES HEALTH THROUGH A BROKEN ENEMY SHIELD — five per cent of the
/// overflow.
///
/// wiki `Shield`: *"Enemies have a shield gate that lasts 0.1 seconds, during
/// which only 5% of the damage dealt will damage their health"*. Both halves of
/// that sentence are real and they are different: the 0.1 s WINDOW applies to
/// instances that arrive AFTER the break, and this applies to the excess of the
/// instance that broke it. This engine had only the window until 2026-08-27,
/// and threw the excess away entirely.
///
/// Confirmed exactly against four measured body shots (MEASUREMENTS M61).
pub(super) const ENEMY_SHIELD_GATE_LEAK: f64 = 0.05;

/// The +100%/+25% ten-stack amp curve shared by Disrupt and Virus.
pub(super) fn ten_stack_amp(stacks: usize) -> f64 {
    if stacks == 0 {
        1.0
    } else {
        2.0 + 0.25 * (stacks as f64 - 1.0)
    }
}

/// THE DEBUFF ROSTER — what the target can be carrying, and how much of each.
///
/// The mirror of [`FightParams::buff_roster`], and deliberately the same shape:
/// `(id, cap)` pairs whose order the frames index into. The two tables on the
/// page are the same component fed from opposite sides of the fight.
///
/// It is a CONSTANT rather than a function of the build, because a debuff is
/// the TARGET's: the roster is every status this engine models, and a run that
/// never applies one draws a flat line at zero — which is the honest answer to
/// "was Corrosive ever up" and the same answer the buff table gives for a buff
/// nothing triggered.
///
/// A DEATH IS NOT A NEW ROW. The arena replaces the body it kills, and every
/// stack goes with it — so a respawn shows as the series dropping to zero and
/// climbing again, and `uptime` counts that gap against you. That is the point.
/// `None` is UNCAPPED and the chart draws it as `∞`, scaling to whatever the
/// run reached. Four rows changed to it on 2026-08-21: a stated cap the fight
/// routinely exceeds is worse than no cap, because the chart pins at the
/// stated one and the reader is told a pile of 22 is a pile of 10.
pub const DEBUFF_ROSTER: [(&str, Option<u32>); 18] = [
    // The 10-stack families, and the three that are not.
    ("virus", Some(TEN_STACK_CAP as u32)),
    ("corrosion", Some(TEN_STACK_CAP as u32)),
    ("disrupt", Some(TEN_STACK_CAP as u32)),
    ("confusion", Some(TEN_STACK_CAP as u32)),
    ("blast", Some(TEN_STACK_CAP as u32)),
    // Cold is ONE STATUS in two rows: `freeze` is the stack ladder and counts
    // for Condition Overload, `frozen` is the STATE its tenth stack trips and
    // counts for nothing. They are LIVE AT THE SAME TIME — the chill stays,
    // pinned at ten, which is what the game shows — so the two series rising
    // together is the mechanic.
    ("freeze", Some(TEN_STACK_CAP as u32)),
    ("frozen", Some(1)),
    ("stagger", Some(STAGGER_CAP as u32)),
    ("weakened", Some(WEAKENED_CAP as u32)),
    // The Bullet Attractor is a FIELD, not a pile: a re-proc moves it rather
    // than adding a second one.
    ("attractor", Some(1)),
    // The DoT families, counted as live instances of that type — and all four
    // of them are UNCAPPED, which is the wiki's own answer for three of them
    // and the reason `dot_family_cap` exists. Heat is the odd one: it is a
    // singleton accumulator rather than a list, so its count is the number of
    // procs folded into the one entity (`HeatEntity::stacks`). Its row read 0
    // or 1 until 2026-08-21 and a ten-stack burn was drawn as one stack.
    ("bleed", None),
    ("poison", None),
    ("ignite", None),
    // …AND THE OTHER TWO DoT FAMILIES. FOUR damage types reach `push_dot` —
    // Slash, Toxin, Electricity, Gas — and a chart listing only the first two
    // lets a build running either of the others see the damage in the meter and
    // never see WHEN it was up. Both are independent per-instance piles like
    // bleed and poison, and both are AREA: a tesla arc and a gas cloud hand
    // their tick to the neighbours too, which is the other reason a reader
    // wants them drawn.

    // On or off, and off only because the target died. It is drawn like every
    // other row so the replay can show WHEN this weapon's invisible status
    // landed — which is the only place a reader can see it at all.
    ("microwave", Some(1)),
    // ...and the other invisible one, which unlike Microwave ENDS. A row of
    // its own for the same reason: it is a status the target is carrying that
    // no damage type explains, so a reader comparing a Condition Overload
    // number against the vector has to be able to see it.
    ("lifted", Some(1)),
    // …AND THE OTHER TWO DoT FAMILIES. FOUR damage types reach `push_dot` —
    // Slash, Toxin, Electricity, Gas — and a chart listing only the first two
    // lets a build running either of the others see the damage in the meter and
    // never see WHEN it was up.
    //
    // APPENDED, NOT INSERTED beside the other DoTs: a series is read by INDEX,
    // so a row added in the middle relabels every chart drawn from a replay
    // stored before it.
    ("tesla", None),
    // …AND THE ONE DoT FAMILY THAT REALLY IS CAPPED. The Gas page states a
    // STACK cap where the other three state a DISPLAY one — see
    // `dot_family_cap` for all four sentences.
    ("gas", Some(TEN_STACK_CAP as u32)),
    // …AND THE THIRD INVISIBLE ONE, appended for the reason the two DoT
    // families above were: a series is read by INDEX. Melee is what brought it
    // — every slam forces a knockdown — and like Lifted it is a status the
    // target carries that no damage type explains, so a reader checking a
    // Condition Overload bracket against the vector has to be able to see it.
    ("knockdown", Some(1)),
];

impl DebuffState {
    /// The roster's live counts at `now`, positionally matching
    /// [`DEBUFF_ROSTER`]. Expired entries are excluded rather than pruned —
    /// sampling must not change the fight it is sampling.
    /// WHAT IS ON THE TARGET, as `(stacks, expires at)` — the same shape the
    /// shooter's own side answers in, because they are one question asked from
    /// two ends and a reader should not have to learn two vocabularies for it.
    ///
    /// The expiry is ABSOLUTE for the reason the buff side's is: it moves only
    /// when something is actually applied or refreshed, so the wire drops the
    /// repeat. `NAN` is one this loop does not track.
    pub(super) fn sample(&self, now: f64) -> Vec<(u16, f64)> {
        let unknown = f64::NAN;
        // THE SOONEST ONE TO FALL DUE, which is what a reader is watching: the
        // count drops when THAT one goes, not when the last one does.
        let soonest = |v: &Vec<f64>| {
            v.iter().copied().filter(|&e| e > now)
                .fold(unknown, |a: f64, e| if a.is_nan() { e } else { a.min(e) })
        };
        let live = |v: &Vec<f64>| (v.iter().filter(|&&e| e > now).count() as u16, soonest(v));
        let dots_of = |t: DamageType| {
            let live: Vec<&Dot> = self
                .dots
                .iter()
                .filter(|d| d.dtype == t && d.ticks_left > 0)
                .collect();
            let next = live.iter().map(|d| d.next_tick).fold(unknown, |a: f64, e| {
                if a.is_nan() { e } else { a.min(e) }
            });
            (live.len() as u16, next)
        };
        let one = |on: bool, e: f64| (u16::from(on), if on { e } else { unknown });
        vec![
            live(&self.virus),
            live(&self.corrosion),
            live(&self.disrupt),
            live(&self.confusion),
            // A Blast stack is a FUSE rather than an expiry: it is waiting to go
            // off, not waiting to wear off.
            (
                self.blast.iter().filter(|b| b.fuse > now).count() as u16,
                self.blast.iter().map(|b| b.fuse).filter(|&f| f > now)
                    .fold(unknown, |a: f64, e| if a.is_nan() { e } else { a.min(e) }),
            ),
            live(&self.freeze),
            one(self.frozen_until.is_some_and(|e| e > now), self.frozen_until.unwrap_or(unknown)),
            live(&self.stagger),
            live(&self.weakened),
            live(&self.attractor),
            dots_of(DamageType::Slash),
            dots_of(DamageType::Toxin),
            // THE COUNT, not "is it burning". See `HeatEntity::stacks`.
            (
                self.heat.as_ref().map_or(0, |h| h.stacks.min(u32::from(u16::MAX)) as u16),
                self.heat.as_ref().map_or(unknown, |h| h.expiry),
            ),

            (u16::from(self.microwave), f64::INFINITY),
            one(self.lifted.is_some_and(|e| e > now), self.lifted.unwrap_or(f64::NAN)),
            // POSITIONALLY MATCHING `DEBUFF_ROSTER`, which is why these two sit
            // at the END rather than beside the other DoTs: the series are
            // read by index, so inserting a row in the middle would silently
            // relabel every chart drawn from a stored replay.
            dots_of(DamageType::Electricity),
            dots_of(DamageType::Gas),
            one(self.knockdown.is_some_and(|e| e > now), self.knockdown.unwrap_or(f64::NAN)),
        ]
    }
}

impl DebuffState {
    /// FIFO push with the universal replace-oldest rule (application-time
    /// order; uniform durations make the front the oldest).
    pub(super) fn push_capped(list: &mut Vec<f64>, expiry: f64, cap: usize, now: f64) {
        list.retain(|&e| e > now); // lazy prune of expired stacks
        // A STACK THAT IS ALREADY DEAD IS NOT APPLIED — a fight's status
        // duration of -100% or less gives `STATUS_DURATION * status_damage <= 0`,
        // and an expired entry is one `prune` has been told it may skip.
        if expiry <= now {
            return;
        }
        if list.len() >= cap {
            list.remove(0); // replace the oldest APPLIED stack
        }
        list.push(expiry);
    }

    /// Push an independent DoT (Slash/Toxin/Electricity/Gas). These have no
    /// natural cap; under a per-unit cap the count of THIS TYPE is limited,
    /// FIFO replace-oldest (the oldest same-type instance drops).
    /// THE BODY DIED and a fresh individual takes its place with none of its
    /// statuses. THREE THINGS SURVIVE, which is what makes this a method: the
    /// OUTBOXES, because a cloud this body produced belongs to its NEIGHBOURS
    /// and is not delivered yet; a GAS CLOUD, because a cloud is a PLACE ("If
    /// the host target dies, Gas will continue to tick damage on all enemies
    /// caught in the host's radius") and the page says that of gas alone; and
    /// the BLAST STACKS, which "detonate simultaneously when … the target
    /// dying" — the detonation that reaches 5 m.
    /// POST A CLOUD OR AN ARC for the neighbours, keeping at most `cap`. NOT
    /// ONLY AN OPTIMISATION: a cloud evicted before its next tick never ticked
    /// anybody, so keeping every one of twenty hands the neighbours DoTs that
    /// do not exist. It is also most of the COST, since the drain reaches every
    /// body in radius.
    /// IS THERE ANYTHING TO TICK? Asked of 361 bodies once per shot on a crowd
    /// ruler, while the shot reaches thirteen.
    pub(super) fn idle(&self) -> bool {
        self.dots.is_empty() && self.heat.is_none() && self.blast.is_empty()
    }

    pub(super) fn post_area(&mut self, dot: Dot, radius_m: f64, cap: Option<usize>) {
        let lim = cap.unwrap_or(TEN_STACK_CAP).max(1) as u16;
        // COALESCED: the same cloud again is the same cloud, and a receiving
        // body cannot hold more than its cap of them anyway.
        if let Some((d, r, n)) = self.area_out.last_mut() {
            if *r == radius_m
                && d.dtype == dot.dtype
                && d.ticks_left == dot.ticks_left
                && (d.frozen - dot.frozen).abs() < 1e-9
                && (d.next_tick - dot.next_tick).abs() < 1e-9
            {
                *n = (*n + 1).min(lim);
                return;
            }
        }
        if let Some(c) = cap {
            while self.area_out.len() >= c.max(1) {
                self.area_out.remove(0);
            }
        }
        self.area_out.push((dot, radius_m, 1));
    }

    /// THE ONE PLACE A BODY DIES, and everything a death sets off is set off
    /// here — the DoT clouds it leaves, the Blast stacks it was carrying, and
    /// now the corpse explosion an augment turns it into.
    ///
    /// The victim is handed IN rather than remembered: this state resets itself
    /// to default on the next line, so anything the build knows would have to
    /// be restored afterwards, and a field that must be restored is a field
    /// somebody eventually forgets. It also forces a new death site to say
    /// whose death it is, which is the only reason this stays a funnel.
    pub(super) fn on_death(&mut self, acid: Option<crate::loadout::AcidShells>, victim: &TargetParams) {
        let out = std::mem::take(&mut self.area_out);
        let mut hits = std::mem::take(&mut self.area_hit);
        // ACID SHELLS: "enemies killed by the Sobek explode, dealing a flat
        // amount of Corrosive damage, plus a percentage of the enemy's maximum
        // Health as Blast damage, to all enemies within 15m".
        //
        // THE HEALTH IT READS IS THE BASE ONE. "The Blast damage bonus does not
        // account for the bonus Health given to enemies in The Steel Path,
        // Archon Hunt, Deep Archimedea, and similar modes" — so it is the
        // unit's own maximum before the mode's multiplier, which is exactly
        // what `TargetParams::base_max_health` holds.
        if let Some(a) = acid {
            let mut v = DamageVector::new();
            v.add(DamageType::Corrosive, a.flat_damage);
            v.add(DamageType::Blast, a.health_fraction * victim.max_health_before_steel_path());
            if v.total() > 0.0 {
                hits.push(AreaHit {
                    damage: v.total(),
                    radius_m: a.radius_m,
                    shares: TypeShares::of(&v),
                    linear_falloff: true,
                });
            }
        }
        let clouds: Vec<Dot> = self
            .dots
            .iter()
            .filter(|d| d.dtype == DamageType::Gas)
            .copied()
            .collect();
        if !self.blast.is_empty() {
            let single: f64 = self.blast.iter().map(|b| b.value).sum();
            hits.push(AreaHit {
                damage: single / BLAST_COEFFICIENT * BLAST_AOE_COEFFICIENT,
                radius_m: BLAST_AOE_RADIUS_M,
                shares: TypeShares::single(DamageType::Blast),
                linear_falloff: false,
            });
        }
        *self = DebuffState::default();
        self.area_out = out;
        self.area_hit = hits;
        self.dots = clouds;
    }

    pub(super) fn push_dot_capped(&mut self, mut dot: Dot, cap: Option<usize>) {
        // A CONSOLIDATED FAMILY TICKS ON ONE CLOCK PER BODY, anchored to the
        // instance that started it — a THIRD model beside the per-instance one
        // (Slash, Toxin) and Heat's singleton. Wiki `Damage/Electricity
        // Damage`, Update 33.6, verbatim:
        //
        //   "multiple procs on an enemy no longer deal their respective damage
        //    separately, like current Slash statuses, but once per second,
        //    similar to Heat status. However, they still maintain each own
        //    timer and will not refresh, unlike Heat."
        //
        // So the CLOCK is shared and the TIMER is not, which is why this moves
        // `next_tick` and leaves `ticks_left` alone. Gas is the same shape,
        // confirmed in game. IT IS TICK-COUNT NEUTRAL by arithmetic: `k` ticks
        // joining a clock `phi < 1` ahead fires `ceil(k - phi) = k` times, so
        // what moves is WHEN the damage lands. HERE rather than in `push_dot`,
        // because a Gas cloud and a Tesla arc reach NEIGHBOURS through
        // `post_area` and those instances join the neighbour's clock too.
        if matches!(dot.dtype, DamageType::Electricity | DamageType::Gas) {
            if let Some(shared) = self
                .dots
                .iter()
                .find(|d| d.dtype == dot.dtype && d.ticks_left > 0)
                .map(|d| d.next_tick)
            {
                if shared > dot.next_tick {
                    dot.next_tick = shared;
                }
            }
        }
        if let Some(cap) = cap {
            while self.dots.iter().filter(|d| d.dtype == dot.dtype).count() >= cap {
                match self.dots.iter().position(|d| d.dtype == dot.dtype) {
                    Some(i) => {
                        self.dots.remove(i);
                    }
                    None => break,
                }
            }
        }
        self.dots.push(dot);
    }

    /// Apply a Heat proc to the singleton accumulator: add its contribution to
    /// the consolidated tick value and refresh the shared expiry (ticks stay
    /// anchored to the first proc). Under a per-unit cap, hold at most `cap`
    /// contributions FIFO — a new proc drops the OLDEST contribution instead
    /// of being ignored (the universal replace-oldest rule;).
    pub(super) fn apply_heat(
        &mut self,
        t: f64,
        contrib: f64,
        expiry: f64,
        cap: Option<usize>,
        origin: HeatOrigin,
    ) {
        let HeatOrigin { bracket, depth, unit } = origin;
        // A BURN BORN DEAD IS NOT APPLIED — status duration at or below -100%
        // nullifies Ignite (wiki, Status Effect). Letting it in handed the
        // ramp-down a negative interval, which never steps: a permanent 50%.
        if expiry <= t {
            return;
        }
        match &mut self.heat {
            Some(h) => {
                match cap {
                    Some(c) => {
                        if h.recent.len() >= c {
                            h.value -= h.recent.remove(0);
                        }
                        h.recent.push(contrib);
                        h.value += contrib;
                        // The list IS the count under a cap, so it cannot drift
                        // from `value` however many procs are dropped.
                        h.stacks = h.recent.len() as u32;
                    }
                    None => {
                        h.value += contrib;
                        h.stacks += 1;
                    }
                }
                h.expiry = expiry; // refresh regardless
            }
            None => {
                let mut recent = Vec::new();
                if cap.is_some() {
                    recent.push(contrib);
                }
                self.heat = Some(HeatEntity {
                    // THE FIRST PROC SETS THE LINK. One entity, one clock, one
                    // bracket — and every later contribution on this target is
                    // the same weapon in the same fight.
                    bracket,
                    depth,
                    unit,
                    born: t,
                    expiry,
                    next_tick: t + 1.0,
                    value: contrib,
                    recent,
                    stacks: 1,
                });
            }
        }
    }

    pub(super) fn weakened_active(&mut self, now: f64) -> usize {
        self.weakened.retain(|&e| e > now);
        self.weakened.len()
    }

    pub(super) fn prune(&mut self, now: f64, status_damage: f64) {
        // ONCE PER INSTANT. `mitigation` prunes, and mitigation is asked once
        // per DAMAGE INSTANCE — so a shot that lands nine procs pruned ten
        // times at the same clock reading. Measured on the build that started
        // this (Phantasma Prime, eight status mods, Primary Debilitate): 62,091
        // prunes a run of which 45,107 were repeats of one already done.
        //
        // IT IS EXACT, not an approximation, and the invariant is what makes it
        // so: after a prune at `now` every list holds only expiries past `now`,
        // and the ONE way a list grows is `push_capped`, which prunes as it goes
        // and refuses a stack that is already dead. So a second prune at the
        // same reading has nothing to find. The clock only ever moves forward,
        // which is what makes an equality test the whole of the check.
        if self.pruned_at == Some(now) {
            return;
        }
        self.pruned_at = Some(now);
        self.lifted = self.lifted.filter(|&e| e > now);
        self.knockdown = self.knockdown.filter(|&e| e > now);
        self.stagger.retain(|&e| e > now);
        self.weakened.retain(|&e| e > now);
        self.freeze.retain(|&e| e > now);
        self.disrupt.retain(|&e| e > now);
        self.virus.retain(|&e| e > now);
        self.corrosion.retain(|&e| e > now);
        self.confusion.retain(|&e| e > now);
        self.attractor.retain(|&e| e > now);
        if let Some(f) = self.frozen_until {
            if f <= now {
                // THAW: chill is SET to exactly 3 stacks with FRESH 6 s timers
                // issued from the trigger's context (M6/M7, and the wiki says
                // the same: "Upon thaw: 3 Cold stacks will remain").
                //
                // MEASURED, NOT DERIVED, and it stays that way under this
                // model: the ten pinned stacks are REPLACED by three, which no
                // amount of reasoning about a stack list produces on its own.
                // The model it replaced did not derive it either — it had the
                // pile consumed at the tenth proc and three issued on the way
                // out — so this is a rule either way and is written down as
                // one rather than made to look like a consequence.
                self.frozen_until = None;
                self.freeze = vec![f + STATUS_DURATION * status_damage; FROZEN_RESET_STACKS];
                self.freeze.retain(|&e| e > now); // long-idle prune
            }
        }
        if let Some(h) = &self.heat {
            // Strict: a tick scheduled at EXACTLY the expiry still lands
            // (the +6 s tick of an unrefreshed proc).
            if h.expiry < now {
                // Begin the armor-strip ramp-down from the strip level the
                // entity had reached when it died.
                self.heat_decay = Some((h.expiry, self.heat_ramp_up(h.expiry, status_damage)));
                self.heat = None;
            }
        }
        if let Some((t0, s0)) = self.heat_decay {
            let steps = heat_strip_steps(now - t0, HEAT_STRIP_DECAY_INTERVAL * status_damage) as usize;
            let start = HEAT_STRIP_DECAY
                .iter()
                .position(|&s| s <= s0 + 1e-9)
                .unwrap_or(HEAT_STRIP_DECAY.len() - 1);
            if start + steps >= HEAT_STRIP_DECAY.len() - 1 {
                self.heat_decay = None; // fully returned
            }
        }
    }

    /// ACTIVE Cold statuses on the target for per-Cold-stack bonuses
    /// (Secondary Shiver): the live Freeze stack count; the Frozen state
    /// counts as the full 10 (it consumed 9 stacks + the trigger proc).
    pub(super) fn cold_status_count(&mut self, now: f64) -> u32 {
        if self.frozen_until.is_some_and(|f| f > now) {
            return 10;
        }
        self.freeze.retain(|&e| e > now);
        self.freeze.len() as u32
    }

    /// The CHILL LADDER's own crit-damage-received bonus. Wiki
    /// (`Damage/Cold_Damage`), verbatim: *"+0.1x increased Critical Damage
    /// multiplier with 1 stack, and +0.05x per subsequent stack, adding up to
    /// +0.50x at 9 stacks"*.
    ///
    /// THE LADDER HAS TEN RUNGS, measured (MEASUREMENTS M46) against a wiki
    /// sentence that says otherwise about the SLOW.
    ///
    /// `0.10 + 0.05 x (n - 1)`, so +0.50x at nine and **+0.55x at ten**. The
    /// tenth is one past the published table because the page stops at nine: on
    /// everything it describes the tenth stack IS Frozen, whose own +1.0x
    /// replaces the ladder. Only a target that reaches ten WITHOUT freezing can
    /// show the tenth rung, which is why it took a Demolisher.
    ///
    /// The `Demolisher` line — "will not freeze at 10 procs, instead their
    /// movement will be Slowed by 90%" — is the SLOW table's ninth rung and
    /// says nothing about this one. A measurement beats a reading of a
    /// neighbouring table.
    pub(super) fn chill_cd_bonus(&self) -> f64 {
        match self.freeze.len() {
            0 => 0.0,
            n => 0.10 + 0.05 * (n as f64 - 1.0),
        }
    }

    /// What the target's COLD STATE adds to crit damage received, into
    /// `cd_total` BEFORE the tier formula.
    ///
    /// FROZEN REPLACES THE LADDER, it does not add to it — wiki, of the +1.0x:
    /// *"doubled from the per-stack bonus"*, i.e. it stands in for the +0.50x
    /// rather than stacking on top. Written as a REPLACEMENT here rather than
    /// as an early return, because the two are live at the same time now: the chill is still there, pinned at ten, and this
    /// is the one place that has to say which of the two the target feels.
    pub(super) fn cold_cd_bonus(&self, now: f64) -> f64 {
        if self.frozen_until.is_some_and(|f| f > now) {
            FROZEN_CRIT_DAMAGE_RECEIVED
        } else {
            self.chill_cd_bonus()
        }
    }

    /// Apply one Cold proc (data/debuffs/freeze.yaml + frozen.yaml):
    /// inert while Frozen; the 10th stack CONSUMES all Freeze stacks and
    /// enters Frozen; overguard holders cap at 4 (never Frozen).
    ///
    /// Returns whether a Cold status was actually APPLIED. `false` only while
    /// Frozen, where `frozen.yaml` is explicit — `refreshable: false`, "cannot
    /// be extended: Cold procs are inert". Anything that keys on "this weapon
    /// applied a Cold status" must read this rather than the attempt: no
    /// status landed, so Primary Frostbite has nothing to stack off. A CAPPED stack list still returns true — pushing past a
    /// cap replaces the oldest, which is an application.
    pub(super) fn apply_cold_proc(
        &mut self,
        t: f64,
        status_damage: f64,
        under_overguard: bool,
        caps: Option<StackCaps>,
        no_frozen: bool,
    ) -> bool {
        if self.frozen_until.is_some_and(|f| f > t) {
            return false; // inert
        }
        self.freeze.retain(|&e| e > t);
        // ONE PUSH PATH, ALWAYS FIFO. Chill is an ordinary
        // capped stack list on every target there is; what differs between
        // targets is only whether reaching the top ALSO trips a state.
        //
        // It replaces a model in which the tenth proc CONSUMED the nine stacks
        // and there was no chill entity while Frozen — which needed the game's
        // own "10 stacks" display to be called Frozen's cosmetic, and needed
        // three separate push paths (an overguard cap, a per-unit cap, and a
        // target that cannot freeze at all). The evidence against it is a
        // target that cannot be frozen: its chill really does reach ten and
        // really does cycle FIFO there, so ten chill stacks are a state the
        // game has, not a label on a different one.
        let cap = match (under_overguard, caps) {
            (true, Some(c)) => FREEZE_CAP_UNDER_OVERGUARD.min(c.general),
            (true, None) => FREEZE_CAP_UNDER_OVERGUARD,
            (false, Some(c)) => c.general,
            (false, None) => TEN_STACK_CAP,
        };
        DebuffState::push_capped(&mut self.freeze, t + STATUS_DURATION * status_damage, cap, t);
        // FROZEN IS A STATE, NOT A STACK. It is tripped by the tenth chill and
        // it suppresses further chill for its 3 s (the early return above);
        // the chill itself stays, pinned at ten, which is what the game shows.
        //
        // EVERY TARGET THAT CANNOT FREEZE NOW FALLS OUT OF ONE CONDITION
        // rather than having a branch: a cap below ten — an Overguard holder's
        // four, an Acolyte's four — makes `len() >= TEN_STACK_CAP` unreachable
        // by arithmetic, and a unit that is simply immune says so once.
        if !no_frozen && self.freeze.len() >= TEN_STACK_CAP {
            self.frozen_until = Some(t + FROZEN_DURATION * status_damage);
        }
        true
    }

    /// Heat armor-strip ramp-UP: 15/30/40/50% at 0.5 s steps after the
    /// FIRST proc (steps scaled by status duration).
    pub(super) fn heat_ramp_up(&self, now: f64, status_damage: f64) -> f64 {
        let Some(h) = &self.heat else { return 0.0 };
        let steps = heat_strip_steps(now - h.born, 0.5 * status_damage);
        match steps as i64 {
            i64::MIN..=0 => 0.0,
            1 => 0.15,
            2 => 0.30,
            3 => 0.40,
            _ => 0.50,
        }
    }

    /// Total Heat armor strip: the live entity's ramp-up, or the dead
    /// entity's ramp-down tail (whichever strips more if both exist).
    pub(super) fn heat_strip(&self, now: f64, status_damage: f64) -> f64 {
        let up = self.heat_ramp_up(now, status_damage);
        let down = match self.heat_decay {
            Some((t0, s0)) if now >= t0 => {
                let steps = heat_strip_steps(now - t0, HEAT_STRIP_DECAY_INTERVAL * status_damage) as usize;
                let start = HEAT_STRIP_DECAY
                    .iter()
                    .position(|&s| s <= s0 + 1e-9)
                    .unwrap_or(HEAT_STRIP_DECAY.len() - 1);
                HEAT_STRIP_DECAY[(start + steps).min(HEAT_STRIP_DECAY.len() - 1)]
            }
            _ => 0.0,
        };
        up.max(down)
    }

    pub(super) fn corrosive_strip(&self) -> f64 {
        match self.corrosion.len() {
            0 => 0.0,
            n => (0.20 + 0.06 * n as f64).min(1.0),
        }
    }

    /// JAHU CANTICLE: armour this body has lost to KILLS rather than to a
    /// status — *"Killing enemies reduces the Armor and Shields of other
    /// enemies within Affinity Range by X%"*.
    ///
    /// Stored as the accumulated FRACTION rather than as a count, because that
    /// is what it is: each kill removes a share of what is LEFT, which is the
    /// rule every other strip in this engine composes by and the only reading
    /// under which repeated kills cannot take armour below zero.
    pub(super) fn canticle_strip(&self) -> f64 {
        self.canticle_armor_strip
    }

    /// FLENSING SPIKES: a WEAPON removing armour off a status that strips
    /// none by itself. `per` is the perk's rate per BULLET that has procced
    /// Puncture on this body — not per live stack, and never restored
    /// (MEASUREMENTS M102): five such bullets take the whole of the armour,
    /// while five stacks off fewer bullets leave some of it standing.
    pub(super) fn puncture_strip(&self, per: f64) -> f64 {
        if per <= 0.0 {
            return 0.0;
        }
        (per * f64::from(self.flensed)).min(1.0)
    }

    /// Prune and compute the live mitigation snapshot for `now`.
    /// `aura_armor` is Corrosive Projection's term — a multiplier on the
    /// target's armour that the SQUAD brings, negative and additive across up
    /// to four teammates. It multiplies with the strips rather than adding to
    /// them, which is the formula this engine has documented since it was
    /// written: `x (1 - 0.18 x corrosive_projections)` in
    /// data/debuffs/ignite.yaml, named and never fed a value until 2026-08-21.
    pub(super) fn mitigation(
        &mut self,
        now: f64,
        status_damage: f64,
        puncture_strip_per: f64,
        aura_armor: f64,
    ) -> Mitigation {
        self.prune(now, status_damage);
        self.amps(now, status_damage, puncture_strip_per, aura_armor)
    }

    /// THE AMPS ALONE, of a target already pruned at this instant.
    ///
    /// Split out because the pellet loop reads them ONCE PER INSTANCE and
    /// prunes once per pellet: an explosion settles against the state its own
    /// collision left, but the two are at the same instant `t`, so pruning
    /// again could only be a no-op — an O(stacks) walk per instance, measured
    /// at +4.6% on `one_fight` when `mitigation` was called instead.
    pub(super) fn amps(
        &self,
        now: f64,
        status_damage: f64,
        puncture_strip_per: f64,
        aura_armor: f64,
    ) -> Mitigation {
        Mitigation {
            disrupt_amp: ten_stack_amp(self.disrupt.len()),
            virus_amp: ten_stack_amp(self.virus.len()),
            virus_stacks: self.virus.len() as u32,
            // THREE SOURCES, MULTIPLIED — the two the game strips with and the
            // one a perk grants. They compose the way the first two already
            // did, rather than sharing a bucket: each removes a share of what
            // is LEFT, which is what "remove 20% of enemy Armor" means when
            // something else already removed some.
            armor_multiplier: (1.0 - self.heat_strip(now, status_damage))
                * (1.0 - self.corrosive_strip())
                * (1.0 - self.puncture_strip(puncture_strip_per))
                // …AND WHAT THE KILLS TOOK. A fourth source, composed the same
                // way the three above are: each removes a share of what is
                // left. It owes nothing to a proc and never expires — the card
                // states no duration, and a strip with no clock is permanent.
                * (1.0 - self.canticle_strip())
                // …AND THE SQUAD'S, which is not a strip: an aura is up from
                // the first shot and owes nothing to a proc.
                * (1.0 + aura_armor).max(0.0),
        }
    }

    /// Distinct status TYPES currently on the target (Condition Overload's
    /// multiplier input). Assumes `prune` ran at this instant.
    pub(super) fn distinct_statuses(&self) -> usize {
        let mut n = 0;
        // MICROWAVE counts, and counting is the whole of what it does.
        n += usize::from(self.microwave);
        // ...and so does LIFTED, for the same reason and with an end.
        n += usize::from(self.lifted.is_some());
        // ...and KNOCKDOWN, which the same wiki row names in the same sentence.
        n += usize::from(self.knockdown.is_some());
        n += usize::from(!self.stagger.is_empty());
        n += usize::from(!self.weakened.is_empty());
        n += usize::from(!self.freeze.is_empty());
        n += usize::from(!self.disrupt.is_empty());
        n += usize::from(!self.virus.is_empty());
        n += usize::from(!self.corrosion.is_empty());
        n += usize::from(!self.confusion.is_empty());
        // Void counts. The mod's own list says so in full — "Impact, Puncture,
        // Slash, Cold, Electricity, Heat, Toxin, Blast, Corrosive, Gas,
        // Magnetic, Radiation, Viral, Void and Tau procs all count for the
        // damage bonus" — and it is the only damage a Bullet Attractor is worth
        // in this arena.
        n += usize::from(!self.attractor.is_empty());
        n += usize::from(!self.blast.is_empty());
        n += usize::from(self.heat.is_some());
        // NO `frozen` LINE. COLD IS ONE STATUS and its stacks are `freeze`,
        // counted above; Frozen is a STATE the tenth of them trips, with its
        // own crowd control and its own crit-damage bonus, and it is not a
        // second type on the target. Counting it here as well would count Cold
        // twice on a frozen target — the chill STAYS while Frozen — and inflate
        // every Condition Overload bracket. `has_status` answers the other
        // reading of the same question, "is this target cold-statused" for a
        // perk rather than "how many TYPES are on it".
        // THE LIVE DoT TYPES, as a bitmask. A `Vec<DamageType>` with a linear
        // `contains` per entry is a heap allocation and an O(n²) scan on a
        // function Condition Overload asks per damage INSTANCE, which on a
        // launcher is a few thousand times a run. Seventeen damage types fit in
        // a `u32`, so the same set is one word and `count_ones` is the answer —
        // identical by construction, a set of at most 17 things, counted.
        let mut seen: u32 = 0;
        for d in &self.dots {
            if d.ticks_left > 0 {
                seen |= 1 << d.dtype as u32;
            }
        }
        n + seen.count_ones() as usize
    }
}
