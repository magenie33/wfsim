//! THE ONLY DOOR DAMAGE COMES THROUGH.
//!
//! A module rather than a convention. `Meter`'s fields are private to it, so
//! the ONLY thing in this crate that can move a run's damage totals is
//! [`ledger::settle`] — which books the number and writes the row that
//! explains it in the same call. Adding a tenth damage site that moves every
//! curve on the page and appears in no ledger is not something a person has to
//! remember not to do any more; it does not compile.
//!
//! It is the shape the owner asked for after the honest answer to
//! "is this architecture elegant" was: the write path is one function, and
//! nine call sites are trusted to call it.
use super::*;

/// A run's damage, raw and after mitigation.
#[derive(Debug, Clone, Copy, Default)]
pub struct Meter {
    raw: f64,
    effective: f64,
    dot: f64,
    max_hit: f64,
}

/// THE DPS-OVER-TIME CURVE, and the same rule: a damage site that moved
/// this curve and wrote no row is precisely the drift this module exists to
/// make impossible, and that was the shape of the old `timeline.add`
/// sitting beside a `log_damage` call.
///
/// WHOSE DAMAGE IT WAS, index for index with the arena's own numbering —
/// 0 is the aimed body, `i + 1` is `formation[i]`.
///
/// Booked here for the same reason the totals are, and UNCONDITIONALLY,
/// which it was not: the pellet site asked `if !others.is_empty()` while
/// the status site did not, so a single-target fight credited its DoT ticks
/// to body 0 and its direct hits to nobody. Two answers to one question,
/// decided by which site happened to be running.
#[derive(Debug, Clone, Copy, Default)]
pub struct Spread(BodyDamage);

impl Spread {
    /// PRIVATE ON PURPOSE — `settle` is the only caller there can be.
    fn credit(&mut self, body: usize, effective: f64) {
        if let Some(slot) = self.0 .0.get_mut(body) {
            *slot += effective;
        }
    }

    pub fn by_body(&self) -> &BodyDamage {
        &self.0
    }

    /// HOW MANY BODIES THIS RUN'S WEAPON ACTUALLY REACHED — the count a
    /// formation exists to answer. The Ocucor's is five: its beam and four
    /// tendrils.
    pub fn touched(&self) -> usize {
        self.0 .0.iter().filter(|d| **d > 0.0).count()
    }
}

/// WHO DEALT THE RUN'S DAMAGE, index for index with the fight's attacker
/// roster — 0 is the wielder, the build this panel is about.
///
/// The mirror of [`Spread`], through the same door and for the same reason:
/// a damage site that moved a total without naming who dealt it is exactly
/// the drift this module exists to make impossible, and with a squad on the
/// field "whose number is this" stops being answerable by assumption.
#[derive(Debug, Clone, Copy, Default)]
pub struct Dealt(AttackerDamage);

impl Dealt {
    /// PRIVATE ON PURPOSE — `settle` is the only caller there can be.
    fn credit(&mut self, who: Attacker, effective: f64) {
        if let Some(slot) = self.0 .0.get_mut(who.0) {
            *slot += effective;
        }
    }

    pub fn by_attacker(&self) -> &AttackerDamage {
        &self.0
    }
}

/// WHICH CLOCK AN INSTANCE WAS ON. The only thing `settle` cannot work out
/// for itself: a Heat tick and a Heat hit are the same type on the same
/// body, and only the caller knows which it just resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Clock {
    /// The trigger's: a hit, an explosion, an extra hit, an arcane instance.
    Hit,
    /// Its own: a status DoT tick, a lingering field's tick.
    Dot,
}

/// A TYPE OF ITS OWN rather than a third field on `Meter`, for a measured
/// reason: it is a 600-slot array and `RunResult` is `Copy`, so grouping
/// 4.8 KB with the two hot scalars cost 2.4% on `one_fight` — the totals
/// want to sit near the run's other counters and the curve does not.
#[derive(Debug, Clone, Copy, Default)]
pub struct Curve(Timeline);

impl Curve {
    /// The buckets, for the one reader there is (`webapi`'s DPS chart).
    pub fn buckets(&self) -> &[f64] {
        &self.0 .0
    }
}

impl Meter {
    /// Raw damage dealt (pre-mitigation), direct hits + DoT ticks.
    #[inline]
    pub fn raw(&self) -> f64 {
        self.raw
    }

    /// Damage after target mitigation (overguard neutrality / armour DR).
    #[inline]
    pub fn effective(&self) -> f64 {
        self.effective
    }

    /// The share of it that came from status DoT ticks and lingering
    /// fields — the clock's damage rather than the trigger's.
    pub fn dot(&self) -> f64 {
        self.dot
    }

    /// THE BIGGEST SINGLE NUMBER the build produced. Every instance, not
    /// just direct pellet hits: booked at the pellet site alone, a Slash
    /// tick or a Blast detonation larger than any hit could not be it —
    /// which is not what the field says it is, and the field is what a
    /// reader is shown.
    pub fn max_hit(&self) -> f64 {
        self.max_hit
    }

    /// PRIVATE ON PURPOSE — see the module's own doc. `settle` is the only
    /// caller there can be.
    #[inline]
    fn book(&mut self, raw: f64, effective: f64, clock: Clock) {
        self.raw += raw;
        self.effective += effective;
        if clock == Clock::Dot {
            self.dot += effective;
        }
        if effective > self.max_hit {
            self.max_hit = effective;
        }
    }
}

/// THE ONE PLACE A DAMAGE INSTANCE IS WRITTEN DOWN — the floating numbers and
/// the combat record, from the same breakdown, at every site where damage
/// lands.
///
/// Both outputs come from `TargetState::apply`'s own portions, so they cannot
/// disagree about how many numbers there were or how big each one was; and the
/// record is filled by the same call that moved the pools rather than beside
/// it, which is what makes "the sum of the record is the damage total" true by
/// construction (see [`crate::record`]).
// INLINED ON PURPOSE: the generic half is a handful of additions and a
// branch, and it sits on the hottest path this engine has.
#[inline]
#[allow(clippy::too_many_arguments)]
pub(in crate::fight) fn settle(
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    t: f64,
    // WHO DEALT IT AND WHO TOOK IT, in that order and in two different types.
    // Both were `usize` in the design that had only one attacker, and two bare
    // indices side by side transpose without a word from the compiler.
    who: Attacker,
    body: usize,
    dtype: DamageType,
    kind: PopKind,
    breakdown: &Breakdown,
    settled: Settled,
    debuffs: Option<&DebuffState>,
    clock: Clock,
    inst: impl FnOnce() -> Instance,
) {
    // THE BOOKING FIRST, and it is why this function exists. A damage site used
    // to add to the totals on one line and write its row on another, so the two
    // were kept in step by whoever remembered — and a tenth site added later
    // could move every curve on the page and appear in no ledger at all. The
    // only guard was a test comparing the sum against the meter, which is a
    // guard rather than a guarantee. There is one door now.
    //
    // AND THE ARGUMENTS ARE BUILT ONLY IF ANYONE IS READING. An `Instance` is a
    // dozen locals, three Vecs and a String, and a `TargetAt` re-runs the
    // armour scaling curve; paying for all of it on the 999 runs nobody reads
    // measures +4.0% on `one_fight`. As an `if recording(rec)` at each
    // call site the gate is a second thing a new site has to remember; as a
    // closure, forgetting it is not something the language allows.
        r.meter.book(settled.raw, settled.effective, clock);
    // THE WASTE, through the same door as the damage. Booking it at the
    // call sites instead would be the exact split this function exists to
    // close: a tenth site could move the rate and appear in no ledger.
    r.overkill += settled.overkill;
    r.spilled += settled.spilled;
    // …AND WHAT THE TARGET WAS CARRYING WHILE IT TOOK IT, through the same
    // door for the same reason. A tenth damage site would otherwise land
    // health damage that no average knows about, and the average would
    // still look right.
    r.health_damage += settled.health;
    r.virus_stack_health += settled.virus_stack_health;
    r.armor_left_health += settled.armor_left_health;
    r.spread.credit(body, settled.effective);
    r.dealt.credit(who, settled.effective);
    let i = t.max(0.0) as usize;
    r.curve.0 .0[i.min(TIMELINE_BUCKETS - 1)] += settled.effective;
    if rec.wants(t) {
        write_row(rec, t, who, body, dtype, kind, breakdown, settled, debuffs, inst());
    }
}
