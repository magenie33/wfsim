use super::*;

/// Timed STATUS events due strictly before `until`, in chronological order:
/// DoT ticks (Bleed/Toxin/Electricity/Gas + break-proc Tesla), the Heat
/// singleton's anchored ticks, and Blast fuse expiries. Mitigation is evaluated
/// LIVE at each event (the snapshot boundary rule); status damage never procs
/// status.
///
/// Lingering-FIELD ticks are NOT here — they are weapon damage that rolls its
/// own crit and its own status, so they get their own pass
/// ([`process_field_ticks`]).
/// `rng` is the STATUS stream, and it is here for exactly one payload: a Blast
/// detonation triggers an EXTRA HIT, which rolls a status of its own. Every
/// other event this function settles is a payload already decided.
/// WHERE THE QUEUE STARTS PAYING, in live DoTs on one body.
///
/// Not tuned to a curve: it is a floor well above what a formation body carries
/// (a handful) and well below what a status build puts on the aimed one (291
/// measured). Anywhere between those two the choice does not matter, which is
/// what makes a single constant honest here rather than a fitted one.
pub(super) const TICK_QUEUE_MIN: usize = 32;

/// ONE SCHEDULED STATUS EVENT, ordered the way the scan that preceded it was.
///
/// The scan took the strictly earliest event, considering DoTs in index order,
/// then Heat, then Blast — so a tie went to the first thing considered. `(t,
/// class, index)` ascending is that rule as a key, which is what lets a queue
/// replace the scan without moving a single number: an optimisation that
/// changes an answer is a bug, and the ORDER of these events is the order the
/// RNG is consumed in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TickKey {
    pub(super) t: f64,
    /// 0 = DoT, 1 = Heat, 2 = Blast — the order the scan considered them in.
    pub(super) class: u8,
    pub(super) index: u32,
}

impl Eq for TickKey {}
impl Ord for TickKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // `total_cmp` because these are clock readings: never NaN, and an
        // ordering that is total is what `BinaryHeap` requires.
        self.t
            .total_cmp(&other.t)
            .then(self.class.cmp(&other.class))
            .then(self.index.cmp(&other.index))
    }
}
impl PartialOrd for TickKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl DebuffState {
    /// THE SHAPE OF THE SCHEDULE, as three numbers.
    ///
    /// A queued event names its slot by INDEX, so anything that reshapes the
    /// collections invalidates the queue. Rather than remember to say so at
    /// every mutation site — the failure mode this repo has been bitten by
    /// before — the loop DERIVES it: take the fingerprint, process one event,
    /// take it again, and rebuild if it moved.
    pub(super) fn tick_shape(&self) -> (usize, usize, bool) {
        (self.dots.len(), self.blast.len(), self.heat.is_some())
    }

    /// THE NEXT EVENT, BY SCANNING. The original rule, kept as the small-state
    /// path — and kept as the DEFINITION of the order, which the queue's key
    /// reproduces rather than replaces.
    pub(super) fn scan_next(&self, until: f64) -> Option<TickKey> {
        let mut best: Option<TickKey> = None;
        let mut consider = |t: f64, class: u8, index: usize| {
            if t < until && best.as_ref().is_none_or(|b| t < b.t) {
                best = Some(TickKey { t, class, index: index as u32 });
            }
        };
        for (i, d) in self.dots.iter().enumerate() {
            if d.ticks_left > 0 {
                consider(d.next_tick, 0, i);
            }
        }
        if let Some(h) = &self.heat {
            if h.next_tick <= h.expiry {
                consider(h.next_tick, 1, 0);
            }
        }
        for (i, b) in self.blast.iter().enumerate() {
            consider(b.fuse, 2, i);
        }
        best
    }

    /// Every event this state has pending before `until`, into a queue the
    /// caller owns — see [`Self::tick_q`] for why it is not returned.
    pub(super) fn refill_tick_queue(
        &self,
        q: &mut std::collections::BinaryHeap<std::cmp::Reverse<TickKey>>,
        until: f64,
    ) {
        q.clear();
        let mut push = |t: f64, class: u8, index: usize| {
            if t < until {
                q.push(std::cmp::Reverse(TickKey { t, class, index: index as u32 }));
            }
        };
        for (i, d) in self.dots.iter().enumerate() {
            if d.ticks_left > 0 {
                push(d.next_tick, 0, i);
            }
        }
        if let Some(h) = &self.heat {
            if h.next_tick <= h.expiry {
                push(h.next_tick, 1, 0);
            }
        }
        for (i, b) in self.blast.iter().enumerate() {
            push(b.fuse, 2, i);
        }
    }
}

/// EVERY BODY BUT THE AIMED ONE, drained to `until`.
///
/// The aimed body's clock is the run loop's own and is settled with the rest of
/// it; these are the neighbours a chain hop, a splash, a tendril or an echo
/// reached. The shot path and the END of the engagement share this because they
/// share the DECISION — a status on a neighbour is paid out, or it was never
/// applied at all — and the tail drained only the aimed body, so whatever was
/// still burning on a neighbour when firing stopped was recorded and never paid.
#[allow(clippy::too_many_arguments)]
pub(super) fn settle_crowd_ticks(
    w: &CardWindows,
    bodies: &mut [Body],
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    until: f64,
    params: &FightParams,
    active: &FightParams,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    rng: &mut Rng,
) {
    for (b, foe) in bodies.iter_mut().enumerate().skip(1) {
        // NOTHING TO BURN, NOTHING TO DO. A formation is up to 400 bodies
        // and a shot reaches a handful; walking the rest once per shot is
        // the whole difference between a crowd being affordable and not.
        if foe.debuffs.idle() {
            continue;
        }
        let Some(spec) = params.body(b) else { continue };
        process_ticks(w, foe, gal, arc, until, params, active, r, rec, rng, spec.params, b);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn process_ticks(
    // THE SHOOTER'S LIVE WINDOWS, because a burning cloud reads them: an
    // element buff strengthens what is already burning and stops the moment it
    // lapses (`FightParams::element_at`). A tick loop that could not see them
    // would have to freeze the answer at the proc, which is a different number.
    w: &CardWindows,
    // THE BODY THIS LANDS ON. Its pools and the statuses on them are
    // one thing, so they arrive as one.
    body: &mut Body,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    until: f64,
    params: &FightParams,
    active: &FightParams,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    rng: &mut Rng,
    // WHICH BODY'S ticks these are — see `settle_procs`'s parameter of the
    // same name.
    foe: &Foe,
    // WHICH BODY IS TICKING, in the fight's numbering — `FightParams::body`.
    //
    // A FORMATION BODY NEVER TICKED AT ALL until 2026-08-17: this was called
    // for the aimed body and for nothing else, so every status a chain hop, a
    // splash, a tendril or an echo applied to a neighbour was recorded and
    // never paid out. Gas and Electricity are the two elements whose PROC is an
    // area, and they cannot work at all without it.
    body_index: usize,
) {
    let Body { state: target, debuffs } = body;
    enum Ev {
        Dot(usize),
        Heat,
        Blast(usize),
    }
    let p = foe;
    let status_damage = params.status_duration_multiplier;
    // A QUEUE, NOT A SCAN, above the threshold. Asking every pending status on
    // a body which of them is next, once per EVENT, measures at 291 live DoTs,
    // 58,918 tick events and 18.2 MILLION scan iterations a run on a dense
    // status build — 80% of the engagement in this function. The 291 is not a
    // leak: an ordinary enemy has no stack cap, so `dot_cap` is `None` on
    // purpose and the list is that long because the game's is.
    //
    // …AND ONLY WHERE IT PAYS. A queue costs one pass plus `log n` an event; a
    // scan costs `n` an event and nothing to set up, so on a body with three
    // DoTs the scan wins outright — and a formation is mostly those, ~300,000
    // per-body calls a run on a 19x19. Building a heap for each measured 444
    // multishot a run against 831.
    //
    // The shape of the state picks the path, and the two are one loop:
    // `scan_next` is the ORDER's definition and `TickKey` reproduces it, which
    // keeps the answer identical on either side of the threshold. An empty
    // `BinaryHeap` allocates nothing.
    // WHICH SHOT THE EVENT BEING SETTLED BELONGS TO — set by each arm below.
    let mut seeded_by;
    // WHOSE STACK IS PAYING OUT. A pile is the BODY's and its ticks land long
    // after the shot, so without this the damage goes to whoever happens to be
    // firing when it ticks. Heat and Blast consolidate every stack into one
    // event and cannot name one applier, so they answer the wielder and say so
    // where they set it.
    let mut dot_owner;
    // …AND THE TWO HALVES OF A DoT TICK, for the ledger alone: the seeds it
    // holds and what the accumulator's own 1 is worth. `None` on every event
    // that is not a DoT tick. Kept here rather than derived at the ledger
    // because only this loop can still see them apart — see `Dot::live` and
    // `Dot::accumulator_unit`, which take DIFFERENT faction layers.
    let mut dot_parts: Option<Vec<crate::record::Part>>;
    let use_queue = debuffs.dots.len() > TICK_QUEUE_MIN;
    let mut q = std::mem::take(&mut debuffs.tick_q);
    if use_queue {
        debuffs.refill_tick_queue(&mut q, until);
    }
    let mut shape = debuffs.tick_shape();
    loop {
        let next = if use_queue {
            q.pop().map(|std::cmp::Reverse(k)| k)
        } else {
            debuffs.scan_next(until)
        };
        let Some(k) = next else { break };
        // A STALE KEY. A consolidated family advances EVERY one of its live
        // instances when the first of them fires, so the queue still holds
        // entries for ticks that have already been paid. The scan path never
        // produces one — it reads `next_tick` live — which is why this guard is
        // here rather than inside `refill_tick_queue`: both paths must answer
        // identically, and only one of them can go stale.
        if k.class == 0 {
            match debuffs.dots.get(k.index as usize) {
                Some(d) if (d.next_tick - k.t).abs() < 1e-9 => {}
                _ => continue,
            }
        }
        let (now, ev) = match k.class {
            0 => (k.t, Ev::Dot(k.index as usize)),
            1 => (k.t, Ev::Heat),
            _ => (k.t, Ev::Blast(k.index as usize)),
        };
        dot_parts = None;

        let mit = debuffs.mitigation(now, status_damage, params.armor_strip_per_puncture, params.squad.enemy_armor_multiplier);
        // A tick is one damage type — which is also the type the
        // vulnerability column reads. Bleed is the exception: it is stored
        // under Slash (that is the proc that made it) but the damage is
        // CINEMATIC, which takes no faction modifier anywhere, and
        // `ignores_armor` is this file's marker for it.
        // `xh` is the EXTRA HIT bracket, and `Some` is what says this payload
        // triggers one at all. A DoT tick and a Heat tick are `None` — no extra
        // hit fires off a status — and the Blast detonation is the documented
        // exception (wiki `Extra_Hit`, Bugs: "Only Xata's Whisper will be
        // triggered by blast Detonations, no other extra hit will").
        let (value, ignores_armor, is_dot_tick, hit_type, src, xh) = match &ev {
            Ev::Dot(i) => {
                let dtype = debuffs.dots[*i].dtype;
                let ignores_armor = debuffs.dots[*i].ignores_armor;
                // THE ROW POINTS BACK AT THE ROUND THAT SEEDED IT, not at
                // whatever the weapon is doing four seconds later. Swapped for
                // the duration of this settlement and put back below, so the
                // recorder's "current shot" stays the loop's business.
                seeded_by = debuffs.dots[*i].cause;
                dot_owner = debuffs.dots[*i].owner;
                // A CONSOLIDATED FAMILY PAYS ONCE — one damage instance for
                // every live stack, which is the number the game pops.
                //
                // The clock was already shared (`push_dot_capped`); this is the
                // other half of the same rule, and without it ten Electricity
                // stacks landed as ten instances at one instant where the game
                // shows one. It is NOT bookkeeping: an instance is the unit
                // that attenuation clamps, that a shield gate multiplies, and
                // that overkill is measured against, so ten small ones and one
                // large one are different fights on any target that has those.
                //
                // wiki `Damage/Electricity Damage`, U33.6: "no longer deal
                // their respective damage separately ... but once per second".
                let value = if matches!(dtype, DamageType::Electricity | DamageType::Gas) {
                    let mut sum = 0.0;
                    // ONE ACCUMULATOR FOR THE WHOLE GROUP, taken from whichever
                    // seed joined it first — "they are added to the same
                    // accumulator, so its initial value of 1 is included only
                    // once" (`Dot::accumulator_unit`). Adding it per stack is
                    // the exact mistake the page calls out.
                    let mut unit = 0.0;
                    for d in debuffs.dots.iter_mut() {
                        if d.dtype == dtype
                            && d.ticks_left > 0
                            && (d.next_tick - now).abs() < 1e-9
                        {
                            d.next_tick += 1.0;
                            d.ticks_left -= 1;
                            // AT `now`, NOT AT THE MOMENT IT WAS APPLIED — see
                            // `Dot::live`. A buff the shooter gained while this
                            // was burning is already in it.
                            // THE LANDING SCALES THE ACCUMULATOR TOO: 234, not
                            // 233, off a 24 body tick (M100). A group takes the
                            // landing of the seed that brought its `1`.
                            sum += d.live(params, now, w) * d.landing;
                            if unit == 0.0 {
                                unit = d.accumulator_unit(params, now, w) * d.landing;
                            }
                        }
                    }
                    // NO EXPANSION HERE. A consolidated group is several
                    // stacks paying into one tick and they need not share a
                    // depth, so a single product drawn over all of them would
                    // be a claim about stacks this arm cannot inspect.
                    dot_parts = Some(vec![
                        crate::record::Part {
                            factor: crate::record::Factor::StatusSeeds,
                            amount: sum,
                            head: 0.0,
                            of: Vec::new(),
                        },
                        crate::record::Part {
                            factor: crate::record::Factor::StatusAccumulator,
                            amount: unit,
                            head: 0.0,
                            of: Vec::new(),
                        },
                    ]);
                    sum + unit
                } else {
                    let d = &mut debuffs.dots[*i];
                    d.next_tick += 1.0;
                    d.ticks_left -= 1;
                    // Slash and Toxin tick independently, so each stack is its
                    // own tick group and carries its own accumulator.
                    let (seeds, acc) = (d.live(params, now, w), d.accumulator_unit(params, now, w));
                    let (over_seed, over_acc) = d.explain(params, now, w);
                    dot_parts = Some(vec![
                        crate::record::Part {
                            factor: crate::record::Factor::StatusSeeds,
                            amount: seeds,
                            head: d.frozen,
                            of: over_seed,
                        },
                        crate::record::Part {
                            factor: crate::record::Factor::StatusAccumulator,
                            amount: acc,
                            head: d.unit,
                            of: over_acc,
                        },
                    ]);
                    seeds + acc
                };
                let hit_type = if ignores_armor {
                    DamageType::Cinematic
                } else {
                    dtype
                };
                (value, ignores_armor, true, hit_type, dtype, None)
            }
            Ev::Heat => {
                // A SINGLETON HAS NO ONE SEEDER: every proc refreshes the same
                // entity, so the tick belongs to all of them and to none. Left
                // unattributed rather than credited to whichever proc came
                // first, which would read as a fact and be an arbitrary pick.
                seeded_by = u32::MAX;
                // ONE EVENT FOR EVERY STACK, so there is no one applier to
                // name — the same reason `seeded_by` is unset here. Credited
                // to the wielder rather than to whoever fired last, which
                // would read as a fact and be an arbitrary pick.
                dot_owner = Seat::WIELDER;
                let h = debuffs.heat.as_mut().expect("heat event needs entity");
                h.next_tick += 1.0;
                // AT `now`. See `Dot::live` — the same rule, on the one status
                // whose stacks all share a clock.
                let bracket = h.bracket + params.element_at(DamageType::Heat, now, w);
                let f = params.faction_at_time(now);
                // ONE ACCUMULATOR for the whole consolidated tick, whatever
                // `stacks` says — `Dot::accumulator_unit`, and Heat is the case
                // the page spells out.
                let paid = h.value * bracket * faction_at(f, h.depth) + h.unit * bracket * f;
                (paid, false, true, DamageType::Heat, DamageType::Heat, None)
            }
            Ev::Blast(i) => {
                seeded_by = u32::MAX;
                // Consolidated like Heat, and unattributable for the same
                // reason.
                dot_owner = Seat::WIELDER;
                // ONE MOMENT, HOWEVER MANY STACKS SHARE IT. Two applied by the
                // same shot carry the same fuse and go off together; an arcane
                // counting hits sees one, the same way it sees one for a
                // shotgun's pellets.
                if arc.blast_pops.last() != Some(&now) {
                    arc.blast_pops.push(now);
                    r.blast_pops += 1;
                }
                let b = debuffs.blast.remove(*i);
                (
                    b.value,
                    false,
                    false,
                    DamageType::Blast,
                    DamageType::Blast,
                    Some(b.xh_bracket),
                )
            }
        };

        // SECONDARY FORTIFIER REACHES A TICK TOO, and only while the Overguard
        // it is about is still there.
        //
        // The bonus is "DYNAMICALLY APPLIED, so the effect is lost entirely
        // after depleting the Overguard from an enemy" — a live check at the
        // moment damage lands, which is what a tick is. The card says "Deals x8
        // Extra Damage to Overguard" with no qualifier about hits, and the same
        // page says DoTs trigger the STEAL half. "Not inheritable" is not
        // evidence against it: that names `Heat_Inherit`, the attribution
        // mechanic, and says the bonus does not travel down THAT path.
        //
        // ONCE, NOT SQUARED. Faction damage is re-applied per derivation step
        // because DE re-applies it (`faction_at(f, depth)`); nothing says that
        // about this, and nothing here is a derivation — the tick is the same
        // instance's payload landing later. The multiplier's own VALUE
        // is a separate open question: our data reads DE's "x8" as the total
        // and it may be a total of x9 (MEASUREMENTS M38).
        let fortifier = if target.overguard > 0.0 {
            params.arcane.overguard_multiplier
        } else {
            1.0
        };
        let unfortified = value;
        let value = value * fortifier;
        // …and `apply` is told what that was, so the share carrying past a
        // depleted Overguard can shed it.
        let mut breakdown = Breakdown::default();
        let settled = target.apply(
            value,
            TypeShares::single(hit_type),
            false,
            now,
            p,
            ignores_armor,
            &mit,
            fortifier,
            watching(rec, &mut breakdown),
        );
        let (effective, killed, broke) = (settled.effective, settled.killed, settled.broken);
        // …AND WHOSE BODY IT WAS. Only worth recording once there is more than
        // one to tell apart, which is the same rule the direct hit follows.
        r.sources.add_status(src, effective);
        let was = rec.attribute_to((seeded_by != u32::MAX).then_some(seeded_by));
        ledger::settle(
            r, rec, now, dot_owner, body_index, src,
            if src == DamageType::Blast { PopKind::Blast } else { PopKind::Status },
            &breakdown, settled, Some(debuffs),
            ledger::Clock::Dot,
            || Instance {
                origin: crate::record::Origin::Status,
                // THE SEED, AND ONE LIVE FACTOR. A tick's own half —
                // coefficient, ModifiedBase, status damage, and the crit and
                // body part of the hit that applied it — is frozen into one
                // number at the moment the status landed (`Dot::frozen`), so
                // decomposing it FURTHER here would mean reporting facts about
                // a hit this function can no longer see. It is the ROW THE TICK
                // POINTS AT that carries them: `Event::cause` names the shot
                // that seeded this, and that row has the full ledger.
                //
                // WHAT IS SPLIT OUT IS THE ACCUMULATOR, because it is the one
                // part of a tick that belongs to no hit at all: the tick group's
                // own 1 (MEASUREMENTS M58). It is small and it is the thing a
                // reader checking our arithmetic against a closed form will be
                // off by, so a row that swallowed it would read as a rounding
                // error in our favour.
                base: unfortified,
                layers: dot_parts
                    .take()
                    .map(|parts| {
                        let out: f64 = parts.iter().map(|x| x.amount).sum();
                        let mut v = vec![crate::record::Layer::Sum {
                            factor: crate::record::Factor::StatusSeeds,
                            parts,
                            out,
                        }];
                        v.extend(mul_layers(
                            out,
                            &[(crate::record::Factor::SecondaryFortifier, fortifier)],
                        ));
                        v
                    })
                    .unwrap_or_else(|| mul_layers(
                        unfortified,
                        &[(crate::record::Factor::SecondaryFortifier, fortifier)],
                    )),
                ..Instance::default()
            },
        );
        rec.attribute_to(was);
        r.dot_ticks += is_dot_tick as u32;
        r.note_kills(killed as u32, now, params.drop_is_in_reach(target.at));
        if let Some(pool) = broke {
            push_break_proc(debuffs, dot_owner, params, now, pool);
        }
        if killed {
            // Status-proc kills grant Galvanized stacks too (GS rules),
            // and count for on-kill arcanes (Merciless — NOT Deadhead,
            // whose precision boundary excludes status-proc kills).
            // GAS TOO, and it is the one that had to be asked: the cloud is an
            // entity with a radius that outlives its host, so "the cloud's kill
            // rather than the weapon's" is a reading the shape invites and the
            // game refuses (MEASUREMENTS M70).
            gal.bump_on_kill(params, now);
            arc.on_kill(params, now);
            // Fresh individual: clean DebuffBar.
            debuffs.on_death(dot_owner, params.acid_shells, &params.foe);
            break;
        }
        // …and the detonation's EXTRA HIT, off the value that actually landed —
        // Fortifier's multiplier included, since it multiplied the instance this
        // is a percentage of. No body part: a detonation struck none.
        if let Some(bracket) = xh {
            if fire_extra_hits(
                dot_owner,
                value,
                bracket,
                1.0,
                false,
                active.status_chance,
                now,
                debuffs,
                gal,
                arc,
                target,
                params,
                active,
                &mit,
                r,
                rec,
                rng,
            ) {
                break;
            }
        }

        // KEEP THE QUEUE HONEST. A queued event names its slot by INDEX, so
        // anything that RESHAPES the collections invalidates every entry — a
        // Blast detonation removes itself, and a detonation's own procs can
        // push a DoT and evict another. The shape is DERIVED rather than
        // announced by each mutation site (`tick_shape`), so a site added later
        // cannot forget to say so: the loop simply looks.
        //
        // Otherwise the event that just fired goes back in at its new time,
        // which is what makes this O(log n) instead of a fresh scan.
        if !use_queue {
            continue;   // the scan re-reads the truth every time; nothing to keep
        }
        let now_shape = debuffs.tick_shape();
        if now_shape != shape {
            debuffs.refill_tick_queue(&mut q, until);
            shape = now_shape;
        } else {
            match &ev {
                Ev::Dot(i) => {
                    let d = &debuffs.dots[*i];
                    if d.ticks_left > 0 && d.next_tick < until {
                        q.push(std::cmp::Reverse(TickKey {
                            t: d.next_tick, class: 0, index: *i as u32,
                        }));
                    }
                }
                Ev::Heat => {
                    if let Some(h) = &debuffs.heat {
                        if h.next_tick <= h.expiry && h.next_tick < until {
                            q.push(std::cmp::Reverse(TickKey {
                                t: h.next_tick, class: 1, index: 0,
                            }));
                        }
                    }
                }
                // A detonation removes itself, so the shape ALWAYS moved and
                // the rebuild above ran. Nothing to re-queue.
                Ev::Blast(_) => {}
            }
        }
    }
    // BACK WHERE IT LIVES, with its capacity — see `tick_q`.
    q.clear();
    debuffs.tick_q = q;
    debuffs.dots.retain(|d| d.ticks_left > 0);
}

/// WHAT IS STILL IN THE AIR, settled up to this shot — the clouds and the
/// orbs already out there, the armour the kills since the last shot stripped,
/// and the packs they dropped.
///
/// A SHOT BOUNDARY IS THE CLOCK every one of these is read on. None of them
/// is fired by the trigger, and each carries the same buff snapshot the shot
/// about to go off does.
#[allow(clippy::too_many_arguments)]
pub(super) fn settle_what_is_in_the_air(
    // See `process_ticks`.
    w: &CardWindows,
    params: &FightParams,
    active: &FightParams,
    field_active: &FightParams,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    t: &mut f64,
    field_ctx: &FieldCtx,
    fields: &mut Vec<FieldState>,
    orbs: &mut Vec<OrbState>,
    meter: &mut Meter,
    // Whose seat all of this belongs to — the orb its meter throws, and the
    // ticks of a cloud it already left.
    owner: Seat,
    ammo: &mut Ammo,
    bodies: &mut [Body],
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    r: &mut RunResult,
    strip_kills_seen: &mut u32,) {
        process_field_ticks(
            w,
            fields,
            gal,
            arc,
            *t,
            params,
            field_active,
            field_ctx,
            r,
            rec,
            d,
            bodies,
        );
        // JAHU CANTICLE. Every kill takes a share off the armour of every enemy
        // inside Affinity Range — which is measured from the PLAYER, not from
        // the corpse (wiki `Affinity`: the squad shares within a 50 m radius),
        // so which body died does not matter and a count is enough.
        //
        // THE SHARES COMPOSE rather than adding: each kill removes a share of
        // what is LEFT, which is the rule every other strip in this engine
        // follows and the only one under which repeated kills cannot take
        // armour past zero. Two kills at 5% leave 0.9025 of it, not 0.90.
        //
        // NO CLOCK. The card states no duration, so what it takes it keeps.
        if let Some((share, radius)) = active.strip_on_kill_in_range {
            let fresh = r.kills.saturating_sub(*strip_kills_seen);
            *strip_kills_seen = r.kills;
            if fresh > 0 && share > 0.0 {
                let keep = (1.0 - share).powi(fresh as i32);
                for (b, foe) in bodies.iter_mut().enumerate() {
                    let Some(spec) = params.body(b) else { continue };
                    if crate::rules::space::gap(params.player_at, spec.at) > radius {
                        continue;
                    }
                    let fd = &mut foe.debuffs;
                    fd.canticle_armor_strip = 1.0 - (1.0 - fd.canticle_armor_strip) * keep;
                }
            }
        }
        // WHAT THE BODIES DROPPED since this was last looked at — ONE roll per
        // kill IN REACH, read by everything that cares (docs/MECHANICS.md
        // §"THE AMMO ECONOMY"). Rolled whether or not anything reads it, which
        // is what keeps two builds of one weapon on the same dice.
        let (mut dropped_primary, mut dropped_secondary) = (0u32, 0u32);
        for _ in 0..r.kills_in_reach.saturating_sub(ammo.drop_kill_mark) {
            let (p, s) = crate::rules::ammo::on_kill(
                params.squad_size,
                params.landscape,
                params.foe.eximus,
                &mut d.drops,
            );
            dropped_primary += p;
            dropped_secondary += s;
        }
        ammo.drop_kill_mark = r.kills_in_reach;
        // …AND WHAT THIS WEAPON DOES WITH THEM (`rules::ammo::credit`). Nothing at all
        // while the reserve is infinite: the house rule already hands the
        // weapon everything a pack could.
        ammo.credit_pickups(params, r, dropped_primary, dropped_secondary);
        // THE RECHARGE METER, credited with the seconds since it was last
        // looked at. A shot boundary is where every other clock in this loop is
        // read, and the meter is coarse enough not to care: it is 45 seconds
        // long and the fastest thing that fills it is worth one.
        if let Some(m) = active.meter {
            meter.tick(m, owner, active, params, dropped_secondary, t, orbs);
        }
        // …AND EVERY ORB EVENT DUE BEFORE THIS SHOT. Same boundary and the same
        // buff snapshot the field walk takes; an orb's clock is its own and no
        // fire-rate bucket reaches it, which is the wiki's *"Tick rate is not
        // affected by Fire Rate"* holding by construction.
        process_orbs(
            w,
            orbs,
            gal,
            arc,
            *t,
            params,
            field_active,
            field_ctx,
            r,
            rec,
            d,
            bodies,
        );
}
