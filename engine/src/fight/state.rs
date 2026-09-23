// SPDX-License-Identifier: AGPL-3.0-or-later
//! WHAT ONE RUN CARRIES FROM SHOT TO SHOT, one struct per mechanic, so the
//! loop in `run` names a piece of state by what it belongs to.

use super::*;


/// DOUBLE TAP'S PILE — consecutive hits, its window, and the other form's pile
/// frozen at the last swap.
pub(super) struct DoubleTap {
    /// DOUBLE TAP: consecutive hits, and when they lapse. Reset by the clock
    /// here and never by a miss — this arena has one target that every pellet
    /// reaches, which is the card's other reset ("if the next shot does not hit
    /// an enemy") and it cannot fire.
    pub(super) hits: u32,
    pub(super) expiry: f64,
    /// …AND THE OTHER FORM'S PILE, FROZEN. Each form of a transmuting weapon
    /// keeps its own Double Tap, snapshotted at the instant a transform
    /// COMPLETES and handed back, clock and all, when that form next completes
    /// its way in (MEASUREMENTS M102): a pile at +300% with 0.5 s left comes
    /// back at +300% with 0.5 s left, and a form never yet fired starts from
    /// nothing. `(hits, seconds left)`.
    pub(super) other: (u32, f64),
}

/// THE RECHARGE METER an orb weapon throws from.
pub(super) struct Meter {
    /// THE RECHARGE METER, in seconds toward `seconds_to_fill`. It opens FULL,
    /// and that is the one place a Tome parts company with the rule next door.
    ///
    /// An INCARNON gauge opens empty because a full one is a consumable the
    /// fight has not earned (docs/BUFFS.md) — and it matters most where the
    /// gauge cannot be refilled, so a free opening magazine was pure gift. A
    /// METER is a CLOCK: it fills at one second a second whether or not anyone
    /// is shooting, so it was filling while you ran to the room, and a player
    /// walks into an engagement with it full. Opening it empty would not be
    /// conservative, it would be wrong — and it would cost the first 45 seconds
    /// of every 180-second benchmark to model a state a player is rarely in.
    pub(super) seconds: f64,
    /// …and how far the clock has already been credited, so the seconds are
    /// counted once however many times the loop looks at it.
    pub(super) clocked: f64,
}

/// THE GHOSTS a kill leaves standing, and the kills already paid in them.
pub(super) struct Ghosts {
    /// WHAT THE KILLS LEFT STANDING, one expiry apiece. It exists to be
    /// COUNTED: nothing reads it back into the fight.
    pub(super) standing: Vec<f64>,
    pub(super) kill_mark: u32,
}

/// A SYNDICATE RADIAL'S GAUGE — affinity earned, its cooldown, the kills paid.
pub(super) struct Syndicate {
    /// A SYNDICATE RADIAL's gauge, in affinity, and the same derived-from-kills
    /// trick the tendrils use: `r.kills` is maintained at six sites already.
    pub(super) kill_mark: u32,
    pub(super) points: f64,
    /// When the weapon may convert affinity again. During the cooldown it
    /// converts NOTHING — "the weapon will not convert any affinity into
    /// points, and all collected points are reset to zero" — so this gates the
    /// accumulation, not just the firing.
    pub(super) ready_at: f64,
}

/// THE TENDRILS a kill grows and a reload takes back.
pub(super) struct Tendrils {
    /// THE OCUCOR'S TENDRILS, tracked as two watermarks rather than as a
    /// counter incremented at every kill.
    ///
    /// DERIVED, and deliberately: `r.kills` is already maintained by SIX
    /// different sites (beam kills, status-proc kills, field-tick kills, the
    /// cycle's own…), and a seventh will exist one day. Hooking each of them is
    /// how one gets missed; reading the total they all feed cannot miss any.
    /// Same for the clear, which keys off `r.reloads` — every reload path in
    /// this loop increments it, including the cycle's.
    ///
    /// It also happens to be exactly right about WHICH kills count. The wiki
    /// excludes one case — "Direct kills with tendrils will not generate an
    /// additional tendril" — and a tendril deals no damage in a single-target
    /// arena (its damage on the beam's own target is cosmetic), so a tendril
    /// kills nothing here and `r.kills` is precisely the qualifying set.
    pub(super) kill_mark: u32,
    pub(super) reload_mark: u32,
    /// The card's opening count, which the fight then treats exactly like an
    /// earned one: it is spent by the magazine event that clears the rest.
    pub(super) seed: u32,
    pub(super) count: u32,
}

/// A CRIT-CHANCE-PER-HIT PILE (Hata-Satya's kind): its stacks, and the hits and
/// refills already counted.
pub(super) struct CritPerHit {
    /// HATA-SATYA: the pellet count at the last clear, plus the card's opening
    /// pile. TWO marks — the hits, and the REFILL COUNTER that ends them.
    pub(super) hit_mark: u32,
    pub(super) refill_mark: u32,
    pub(super) seed: u32,
    pub(super) stacks: u32,
}

/// A SNIPER'S SHOT COMBO: the count and the last hit that fed it.
pub(super) struct SniperComboCount {
    /// THE SHOT COMBO COUNTER: the count as of the last landing hit, and when
    /// that was. `combo_at` turns the pair into the count at any later moment.
    /// The seed is in hand at t = 0, so the clock starts there rather than at
    /// minus infinity — otherwise the card's count would decay away before the
    /// first shot.
    pub(super) count: u32,
    pub(super) last_hit: f64,
}

/// A HELD TRIGGER'S SPOOL — shots since it was released, and when the next was due.
pub(super) struct Spool {
    /// **HOW MANY PLANNED CASTS HAVE ALREADY TAKEN THE TRIGGER FINGER.**
    ///
    /// The cast plan is made before the run and its interrupts are in time
    /// order (`data::abilities::plan_casts`), so the run only has to remember
    /// how far down that list it is. Here because this is the struct the shot
    /// clock already carries: a cast that roots the frame is a pause in exactly
    /// the sense a spool-up is.
    pub(super) casts_paid: usize,
    /// HELD-TRIGGER SPOOL — shots since the trigger was last released, and the
    /// moment the next one was due. See `data::weapons::SustainedFireRate`.
    pub(super) shots: f64,
    pub(super) due: f64,
}

/// THE MAGAZINE AND THE RESERVE — what is loaded, what is carried, what the
/// magazine holds now, and the bookkeeping the ammo cards read.
pub(super) struct Ammo {
    /// KILLS ALREADY PAID FOR, so the pickup roll happens once per body.
    ///
    /// Read as a DELTA off the run's own counter rather than hooked onto each
    /// place a body dies — there are nine of those, they are the same nine
    /// `ledger::settle` guards, and a tenth would silently stop dropping ammo.
    /// The counter cannot be missed because every kill already goes through it.
    /// Kills already rolled for drops — the shared tally every reader works off.
    pub(super) drop_kill_mark: u32,
    /// Which kills the magazine refill has already paid out — see the spend
    /// below for why this cannot be the same watermark.
    pub(super) refill_kill_mark: u32,
    /// ROUNDS FIRED SINCE THE MAGAZINE WAS FILLED, which is what says when a
    /// BURST completes: Reaver's Rapture wants a full burst, and a burst is
    /// `burst.count` consecutive rounds out of one magazine. It restarts with
    /// the magazine, so a magazine that does not divide by the count leaves a
    /// partial burst at the end and that burst earns nothing.
    ///
    /// The intra-burst SPACING is averaged here — the cadence code spreads a
    /// burst's rounds evenly, which is the wiki's own effective-rate formula —
    /// so this counts which round completes a burst rather than pinning the
    /// instant it happened. That is the precise part and the part that decides
    /// which shots carry which stack count.
    pub(super) rounds_this_mag: u32,
    /// How many times the magazine has been full again — see the macro below.
    pub(super) refills: u32,
    /// …and the other half of that split: the reload FINISHED, for the buffs
    /// that were counting reloads rather than shells.
    ///
    /// TWO TRIGGERS, ONE SITE. Every reload this loop performs is a reload from
    /// empty — it only reloads when it cannot fire — so both fire here and the
    /// difference between them lives at exactly one other place: the Incarnon
    /// transform, which refills the base magazine whether or not it was empty
    /// and therefore bumps `ReloadFromEmpty` alone, and only when it was.
    /// THE MAGAZINE'S CAPACITY, LIVE. Resonant Restore grows it — "On Reload
    /// From Empty: Increase Base Magazine Capacity by +15. Stacks up to 3x" —
    /// so the capacity is a variable rather than `params.magazine_size`, and
    /// EVERY read of it below goes through this name. It only ever rises, and
    /// only at the one site that pays the stack, which is what lets it be a
    /// plain number instead of a buff lookup at eight call sites.
    pub(super) cap: f64,
    pub(super) growth_stacks: u32,
    /// PYRANA PRIME'S SECOND GUN multiplies that capacity while it is up, so a
    /// growth stack landing meanwhile is paid at the same multiple and comes
    /// back out whole when the gun leaves.
    pub(super) summon_multiplier: f64,
    /// SHELLS OWED TO THE PLAYER FOR THE RELOAD THEY ARE HALFWAY THROUGH.
    ///
    /// Entering the Incarnon form IS a reload — the transmute animation is the
    /// weapon's reload time, which is how you can tell — and
    /// the whole reload runs across the cycle: one shell as you go in, the rest
    /// as you come out. So this holds the rest. Zero while nothing is owed,
    /// which is also what entering on a full magazine leaves it.
    pub(super) owed_shells: u32,
    pub(super) loaded: f64,
    pub(super) reserve: f64,
    /// Deadly Efficiency's window. Opens at reload COMPLETION — `t` is already
    /// past the reload when this is set, the same as `fire_rate_reload_expiry_seconds` — and
    /// seeded from its card exactly like its three siblings.
    /// READY RETALIATION's open window, or -inf while it is shut. Unlike the two
    /// beside it this is not only read at a shot: it changes how long the NEXT
    /// reload takes, so it is passed into every reload and every transmute.
    /// Set by a pellet that rolled Executioner's Fortune, spent once by the shot.
    pub(super) instant_reload_now: bool,
}

/// WHERE A TRANSMUTING WEAPON IS IN ITS CYCLE — which form is out, when the
/// Incarnon window closes, the charges toward the next, and the base form's
/// magazine held while it is away.
pub(super) struct IncarnonState {
    /// ...and the FORM gauge fed by kills rather than by hits (ChargeOn::Kills),
    /// which needs its own because it advances in both forms while the others
    /// only pay out in one.
    pub(super) kill_mark: u32,
    /// Incarnon cycle state. The engagement opens in the BASE form and earns
    /// its way in — see `IncarnonCycle::starts_primed` for why, and for the
    /// reading that opens transformed.
    pub(super) in_base_form: bool,
    /// When a CLOCK-ended Incarnon falls out of its window (`Ends::After`).
    /// Unread by a gauge cycle, whose way out is a magazine.
    ///
    /// A RUN THAT OPENS WITH IT UP OPENS ITS CLOCK TOO — the card's `stacks`
    /// knob is "you walked in with it", not "it is up and already expired".
    pub(super) incarnon_until: f64,
    /// READY RETALIATION IS ARMED BY THE EMPTY MAGAZINE, not by the reload.
    ///
    /// The owner's evidence is the transmute: empty the magazine
    /// and transform immediately, and the TRANSFORM is faster too — which it
    /// could only be if the buff was already on the weapon before any reload
    /// started. It is then spent by the next reload, and coming out of Incarnon
    /// form counts as one — leaving Incarnon is a reload as far as this buff is
    /// concerned, and it is spent.
    ///
    /// So it is a flag rather than a clock. This card states a bonus and no
    /// duration, and that is not an omission — there is nothing to time.
    pub(super) charges: u32,
    pub(super) base_magazine: f64,
}

impl IncarnonState {
    /// **WHAT THE ACTION LIST SEES RIGHT NOW** — `data::apl::Now`, built from
    /// the one state that answers its questions.
    ///
    /// The gauge is ONE number for both halves of the cycle, because a cycle
    /// only ever asks one question: in the form you can hold it is how much of
    /// the next one you have EARNED, and in the earned form it is how much of
    /// it you have LEFT — a charge magazine or a clock, both draining to zero.
    /// A weapon with no cycle has no gauge and reads 0 in the base form, which
    /// is the reading under which no transmute rule can fire at all.
    pub(super) fn now<'a>(
        &self,
        params: &FightParams,
        ammo: &Ammo,
        next_cost: f64,
        t: f64,
        // IS THE FLASH UP — passed in because it is MELEE state and this
        // struct is the cycle's, and a `Now` that guessed it would be a rule
        // answered from a fact nobody supplied.
        tennokai: bool,
        remaining: &'a dyn Fn(&str) -> f64,
    ) -> crate::data::apl::Now<'a> {
        let Some(cy) = params.cycle.as_ref() else {
            return crate::data::apl::Now {
                can_fire: can_fire(ammo.loaded, next_cost),
                gauge_pct: 0.0,
                tennokai,
                in_base_form: true,
                remaining,
            };
        };
        let gauge_pct = match (self.in_base_form, cy.arms, cy.ends) {
            // FILLING: whole charges against the whole it takes, and it
            // OVERSHOOTS past 1 because the shot that crosses the line pays in
            // whole pellets (`charge_the_gauge`).
            (true, Arms::Gauge { charges_to_fill, .. }, _) => {
                f64::from(self.charges) / f64::from(charges_to_fill.max(1))
            }
            // …and a melee Incarnon is armed by the COMBO, which is not a
            // gauge and not yet a condition this list can say. It reads 0, so
            // nothing in the list takes the swing that arms it away.
            (true, Arms::HeavyAtCombo(_), _) => 0.0,
            // SPENDING A CHARGE MAGAZINE, or SPENDING A CLOCK: the same
            // question, and the rule that ends the form cannot tell them apart.
            //
            // EMPTY IS THE ENGINE'S OWN EPSILON and not `> 0`: a magazine sits
            // on 1e-10 after an overdraw, and a gauge calling that "not spent"
            // would hold the form open on rounds nothing can fire.
            (false, _, Ends::ChargeMagazine) if ammo.loaded < 1e-9 => 0.0,
            (false, _, Ends::ChargeMagazine) => ammo.loaded / ammo.cap.max(1e-9),
            // A LOCKED WINDOW NEVER RUNS OUT, so the form is never spent. Said
            // before the division and not after: `inf / inf` is NaN, and NaN
            // through a `.max(0.0)` comes back as ZERO — which read as a locked
            // Incarnon reverting on the first shot of every run.
            (false, _, Ends::After(window)) if !window.is_finite() => 1.0,
            (false, _, Ends::After(window)) => {
                ((self.incarnon_until - t) / window.max(1e-9)).clamp(0.0, 1.0)
            }
        };
        crate::data::apl::Now {
            // THE ACTIVE FORM'S MAGAZINE: a CHARGE-MAGAZINE cycle holds the
            // base form's rounds aside while it is transformed, so in that half
            // asking `ammo.loaded` is asking about a magazine nobody holds. A
            // clock-ended cycle keeps no second magazine — a melee weapon has
            // none at all — and fires the one magazine throughout.
            can_fire: can_fire(
                if self.in_base_form && cy.ends == Ends::ChargeMagazine {
                    self.base_magazine
                } else {
                    ammo.loaded
                },
                next_cost,
            ),
            gauge_pct,
            tennokai,
            in_base_form: self.in_base_form,
            remaining,
        }
    }
}

/// THE WINDOWS A CARD OPENS AND A CLOCK CLOSES — reload and headshot buffs,
/// their piles and their expiries.
pub(super) struct CardWindows {
    /// Pressurized Magazine's on-reload fire-rate buff clock (seeded active
    /// only if configured so; defaults inactive).
    pub(super) fire_rate_after_reload: f64,
    /// Crosshairs (per-stack expiry FIFO + one refreshable buff); the on-head
    /// buff seeds active per its `initial_active` (default on).
    pub(super) crit_on_headshot: f64,
    /// Crosshairs keeps a per-stack expiry rather than one clock — and takes
    /// an infinite duration exactly like the rest.
    pub(super) crit_on_headshot_stacks: Vec<f64>,
    /// LEADED GAS' window: when the element and the status bonus run out. One
    /// clock for both, because the card prints one.
    pub(super) weakpoint_buff: f64,
    /// LINGERING JUDGEMENT: the recent headshots' timestamps, and the window
    /// they have opened. The ring is at most `hits` long — older ones can never
    /// matter, because a streak is the LAST `hits` inside `within`.
    pub(super) headshot_times: Vec<f64>,
    pub(super) streak: f64,
    pub(super) base_damage_after_reload: f64,
    /// EXIMUS ADVANTAGE's window, the same clock as the one above with a
    /// different key. It never opens at all unless the target is an Eximus.
    pub(super) base_damage_eximus: f64,
}

impl Ammo {
    pub(super) fn resize_for_summon(&mut self, params: &FightParams, bar: &BuffBar) {
        let ammo = self;
        // …AND THE MAGAZINE FOLLOWS THE BAR, at one site for both edges. It
        // arrives with a modded magazine's worth of rounds, and "when the
        // ethereal Pyrana disappears, the magazine is reduced to the modded
        // magazine size" (wiki).
        if let Some(s) = params.kill_streak_summon {
            let want = if bar.get(crate::model::KillStreakSummonSpec::BUFF_ID).is_some() {
                s.magazine_multiplier
            } else {
                1.0
            };
            if (want - ammo.summon_multiplier).abs() > 1e-12 {
                if want > ammo.summon_multiplier {
                    ammo.loaded += ammo.cap / ammo.summon_multiplier;
                }
                ammo.cap = ammo.cap / ammo.summon_multiplier * want;
                ammo.loaded = ammo.loaded.min(ammo.cap);
                ammo.summon_multiplier = want;
            }
        }
    }
}

impl Ghosts {
    /// WHAT THE KILLS LEFT STANDING, as a duration. The kills were counted
    /// where they happened; this is where they become ghosts.
    pub(super) fn settle(&mut self, g: crate::model::SpawnOnKillSpec, t: f64, r: &mut RunResult) {
        let ghost_pile = self;
        ghost_pile.standing.retain(|e| *e > t);
        for _ in 0..(r.ghost_kills - ghost_pile.kill_mark) {
            ghost_pile.standing.push(t + g.seconds);
        }
        ghost_pile.kill_mark = r.ghost_kills;
        r.ghosts_peak = r.ghosts_peak.max(ghost_pile.standing.len() as u32);
    }
}

impl CritPerHit {
    /// HATA-SATYA'S PILE — mark-and-diff on the hits, read at the START of the
    /// shot so the hit that earns a stack does not carry it, the rule every
    /// other trigger in the loop follows.
    pub(super) fn settle(&mut self, params: &FightParams, r: &RunResult, ammo: &Ammo) {
        let crit_per_hit = self;
        // WHAT TAKES THE PILE: "Resets upon reloading or holstering", plus
        // "Swapping to Incarnon Form counts as reloading the Soma Prime and
        // will therefore end the bonus". Holstering is a weapon swap, which
        // this arena never does.
        //
        // KEYED ON THE REFILL, NOT ON THE RELOAD COUNTER (owner measured
        // 2026-08-22: coming OUT of the Incarnon form clears it too). The
        // wiki names only the way in, and reading its two events literally
        // left the way out counting as neither a reload nor a transform —
        // so a pile at its ceiling rode the revert into the base form and
        // spent a whole magazine there at +500% that the game would have
        // taken away. This is the engine's own rule about the cycle, which
        // was already written one screen down: swapping EITHER WAY fully
        // reloads the base form's magazine, so the buff is spent by
        // whatever refills it. One mark instead of two, and the event that
        // was missing is the one it now cannot miss — every refill in this
        // loop goes through `magazine_refilled!`.
        //
        // The seed dies with the earned stacks: it is the same buff.
        if ammo.refills != crit_per_hit.refill_mark && !params.crit_chance_per_hit_held {
            crit_per_hit.refill_mark = ammo.refills;
            crit_per_hit.hit_mark = r.pellets;
            crit_per_hit.seed = 0;
        }
        // THE COUNTER IS NOT CAPPED — the BONUS is.
        // The pile takes every hit that lands, and 500% is what it is worth
        // once it passes the ceiling: a card of this class publishes a
        // NUMBER and lets the count run, so 417 stacks and 4,000 stacks are
        // both ordinary states of the same fight. Clamping the count would
        // be modelling a mechanic DE did not write, and the row is drawn as
        // the value anyway, so its ceiling is never on screen.
        crit_per_hit.stacks = crit_per_hit.seed + (r.pellets - crit_per_hit.hit_mark);
    }
}

impl Ammo {
    /// SENTIENT SURGE'S REFILL, spent before the reload check so that a kill
    /// can genuinely save a reload — which is the whole point of the mod.
    pub(super) fn refill_on_kill(&mut self, params: &FightParams, r: &RunResult) {
        let ammo = self;
        // SENTIENT SURGE's refill, spent before the reload check below so
        // that a kill can genuinely save a reload — which is the whole
        // point of the mod. "Reloaded ammo is taken from the Ocucor's ammo
        // reserves. This mod does not generate ammo", so it draws like any
        // other reload and a dry reserve gives nothing.
        //
        // ITS OWN WATERMARK, and NOT the tendril one. The two answer
        // different questions: a tendril asks "how many kills since the
        // last reload" (so its mark moves when the magazine event clears
        // them), while a refill asks "which kills have I already been paid
        // for" (so its mark moves when it is SPENT). Sharing the tendril
        // mark made every loop iteration re-earn the same kills, which
        // topped the magazine up on every shot and handed the weapon an
        // effectively infinite one.
        //
        // And a REFILL IS NOT A RELOAD: it never touches `r.reloads`, so
        // the tendrils live through it. That is the whole reason this mod
        // pairs with this passive — the wiki says it from the other side,
        // "Magazine refill effects such as ... kills with Sentient Surge
        // ... will PREVENT the tendrils from disappearing."
        if params.magazine_refill_on_kill > 0.0 && r.kills > ammo.refill_kill_mark {
            let earned = f64::from(r.kills - ammo.refill_kill_mark)
                * params.magazine_refill_on_kill
                * ammo.cap;
            ammo.refill_kill_mark = r.kills;
            // Capped at the magazine: a refill tops up, it does not bank.
            // Overflow is simply lost, which is what "Refill X% of the
            // Magazine" means on a magazine already near full.
            let room = (ammo.cap - ammo.loaded).max(0.0);
            let want = earned.min(room);
            if want > 0.0 {
                ammo.loaded += draw_from(&mut ammo.reserve, params.infinite_reserve, want);
            }
        }
    }
}

impl Tendrils {
    /// THE TENDRILS a reload takes away and kills grow back, re-derived before
    /// anything decides to reload. A weapon with none has `tendril_max` 0 and
    /// keeps a count of 0 through all of it.
    pub(super) fn settle(&mut self, params: &FightParams, r: &RunResult) {
        let tendril = self;
        // A reload — or an empty magazine, which in this sim always leads
        // to one — clears every tendril. "Tendrils disappear upon
        // reloading or emptying the magazine."
        //
        // ...unless the card says no event takes them (`tendrils_held`),
        // which is what "no timeout" means for a buff whose end is an
        // event rather than a clock. The seed dies with the earned ones:
        // it is the same buff.
        if r.reloads != tendril.reload_mark && !params.tendrils_held {
            tendril.reload_mark = r.reloads;
            // The mark tracks the SAME quantity the count reads.
            tendril.kill_mark = r.kills - r.kills_by_tendril;
            tendril.seed = 0;
        }
        // A TENDRIL'S OWN KILL SPAWNS NOTHING, so the count is fed by
        // every OTHER kill — the beam's, and any status kill including one
        // a tendril's own proc caused.
        let spawning = r.kills - r.kills_by_tendril;
        tendril.count = (tendril.seed + (spawning - tendril.kill_mark)).min(params.tendril_max);
    }
}

impl DoubleTap {

    pub(super) fn swap(&mut self, now: f64) {
        let held = (self.hits, (self.expiry - now).max(0.0));
        self.hits = self.other.0;
        self.expiry = if self.other.1 > 0.0 { now + self.other.1 } else { f64::NEG_INFINITY };
        self.other = held;
    }
}

impl Ammo {
    /// WHAT THE BODIES DROPPED, credited to the reserve — one pickup at a
    /// time, because a pack is only worth what the reserve has room for.
    pub(super) fn credit_pickups(
        &mut self,
        params: &FightParams,
        r: &mut RunResult,
        dropped_primary: u32,
        dropped_secondary: u32,
    ) {
        let ammo = self;
        if !params.ammo_drops || params.infinite_reserve {
            return;
        }
        if let Some(takes) = params.ammo_class {
            for (kind, n) in [
                (crate::rules::ammo::Pickup::Primary, dropped_primary),
                (crate::rules::ammo::Pickup::Secondary, dropped_secondary),
            ] {
                for _ in 0..n {
                    let got = crate::rules::ammo::credit(
                        kind,
                        takes,
                        ammo.reserve,
                        params.reserve_ammo,
                        params.ammo_pickup,
                        params.ammo_conversion,
                    );
                    if got > 0.0 {
                        ammo.reserve += got;
                        r.picked_up_ammo += got;
                    }
                }
            }
        }
    }
}

impl Meter {
    /// THE RECHARGE METER, credited with the seconds since it was last looked
    /// at. A shot boundary is where every other clock in this loop is read,
    /// and the meter is coarse enough not to care: it is 45 seconds long and
    /// the fastest thing that fills it is worth one.
    pub(super) fn tick(
        &mut self,
        m: crate::build::loadout::ResolvedMeter,
        active: &FightParams,
        params: &FightParams,
        dropped_secondary: u32,
        t: &mut f64,
        orbs: &mut Vec<OrbState>,
    ) {
        let meter = self;
        meter.seconds += *t - meter.clocked;
        meter.clocked = *t;
        // …AND WHAT THE BODIES DROPPED. *"Picking up secondary or universal
        // ammo reduces recharge time by 10 seconds"*.
        //
        // ONLY SECONDARY COUNTS. A primary pickup does nothing for a tome's
        // meter, and universal packs are placed in a Simulacrum rather than
        // dropped by anything, so a kill can only ever contribute through
        // the secondary half of its roll.
        //
        // INFINITE AMMO DOES NOT REMOVE THE PICKUP. The house rule is about
        // the reserve, and a real fight is under its cap almost all of the
        // time — the pack is still on the floor either way.
        meter.seconds += f64::from(dropped_secondary) * m.seconds_per_ammo_pickup;
        // A FULL METER IS ONE THROW. It is not a magazine — the page says
        // "requires a fully filled meter in order to fire", so what is
        // spent is the whole thing and what is bought is a single orb.
        if meter.seconds >= m.seconds_to_fill {
            meter.seconds -= m.seconds_to_fill;
            if let Some(o) = active.orb {
                throw_orb(o, params, *t, orbs);
                // …AND THE PRIMARY FIRE STOPS FOR THE ANIMATION. A throw is
                // a wind-up and a recovery, and the weapon can do nothing
                // else until both are over — which is the cycle's whole
                // price beyond the meter.
                //
                // THEN IT WINDS UP AGAIN. Coming back to the primary is
                // pressing its trigger, and that costs what pressing it
                // always costs; the interval only "corresponds exactly to
                // the fire rate" while you are holding it down.
                *t += o.throw_seconds + o.recovery_seconds + active.windup_seconds;
            }
        }
    }
}

impl Ammo {
    /// AN INSTANT RELOAD — the magazine the weapon is actually firing, which
    /// in a cycle is the base form's while the Incarnon one is out.
    pub(super) fn instant_reload(&mut self, params: &FightParams, incarnon: &mut IncarnonState) {
        let ammo = self;
        ammo.instant_reload_now = false;
        match &params.cycle {
            Some(cy) => {
                incarnon.base_magazine += draw_from(&mut ammo.reserve, params.infinite_reserve,
                    reload_draw(cy.base_form.magazine_size, incarnon.base_magazine));
            }
            None => {
                ammo.loaded += draw_from(&mut ammo.reserve, params.infinite_reserve,
                    reload_draw(ammo.cap, ammo.loaded));
            }
        }
    }
}

/// EXACT PENANCE'S ROLL on a weak-point hit: a magazine the weapon actually
/// has, the head it names, the kill if the card asks for one.
pub(super) fn roll_instant_reload(
    active: &FightParams,
    params: &FightParams,
    d: &mut crate::rules::rng::Draws,
    head_direct: bool,
    killed: bool,
    incarnon: &IncarnonState,
    ammo: &mut Ammo,
) {
    if let Some(ef) = params.instant_reload {
        let has_magazine = match &params.cycle {
            Some(_) => incarnon.in_base_form,
            None => active.ammo_efficiency_applies,
        };
        if has_magazine
            && head_direct
            && (!ef.needs_kill || killed)
            && d.extra.chance(ef.chance)
        {
            ammo.instant_reload_now = true;
        }
    }
}
