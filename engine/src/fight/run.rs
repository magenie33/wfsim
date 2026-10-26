use super::*;

/// THE SHOT COMBO COUNTER as it stands at `t`, given the count as of the last
/// landing hit. Decay is computed rather than ticked because this loop has no
/// clock of its own: it advances shot to shot, and a counter that lost one per
/// elapsed period is exactly what *"reduced by 1 after a short period of time
/// that no successful hits have been made"* describes.
///
/// A MISS never happens here. The wiki's other decay trigger — *"or if the
/// player misses a shot"* — needs an accuracy model this arena does not have
/// (docs/UNMODELLED.md: no distance), so every shot lands and the counter only
/// ever decays through time. It is recorded on the weapon's card as the one
/// way this runs generous.
#[wfsim_gas_macro::metered]
pub(super) fn combo_at(
    spec: Option<crate::model::SniperCombo>,
    held: bool,
    count: u32,
    last_hit: f64,
    t: f64,
) -> u32 {
    let Some(c) = spec else { return 0 };
    if held || c.seconds <= 0.0 {
        return count;
    }
    let lost = ((t - last_hit) / c.seconds).floor();
    if lost <= 0.0 {
        count
    } else if lost >= f64::from(u32::MAX) {
        0
    } else {
        count.saturating_sub(lost as u32)
    }
}

/// A stacking spec's decay period, or 0 when the spec is absent.
/// PYRANA PRIME'S STREAK, as the bar holds it: `stacks` kills on one clock.
#[wfsim_gas_macro::metered]
pub(super) fn kill_streak(s: crate::model::KillStreakSummonSpec, stacks: u32, t: f64) -> crate::rules::buffs::Buff {
    crate::rules::buffs::Buff {
        id: crate::model::KillStreakSummonSpec::STREAK_BUFF_ID.into(),
        scope: crate::rules::buffs::BuffScope::Weapon,
        stacks,
        expiry_seconds: Some(t + s.kill_window_seconds),
        contributions: crate::rules::buffs::Contributions::default(),
    }
}

/// PYRANA PRIME'S SECOND GUN, as the bar holds it: the fire-rate half is a
/// bar multiplier like Frenzy's, and the magazine half reads whether it is up.
#[wfsim_gas_macro::metered]
pub(super) fn summoned_gun(s: crate::model::KillStreakSummonSpec, t: f64) -> crate::rules::buffs::Buff {
    crate::rules::buffs::Buff {
        id: crate::model::KillStreakSummonSpec::BUFF_ID.into(),
        scope: crate::rules::buffs::BuffScope::Weapon,
        stacks: 1,
        expiry_seconds: Some(t + s.duration_seconds),
        contributions: crate::rules::buffs::Contributions {
            fire_rate_multiplier: s.fire_rate_multiplier,
            ..Default::default()
        },
    }
}

#[wfsim_gas_macro::metered]
pub(super) fn dur(spec: &Option<crate::model::StackSpec>) -> f64 {
    spec.as_ref().map_or(0.0, |s| s.duration)
}

#[wfsim_gas_macro::metered]
pub fn run_once(params: &FightParams, rng: &mut Rng) -> RunResult {
    run_once_traced(params, rng, None, &mut crate::record::Record::off())
}

/// WHAT THE STACKING BUFFS OF ONE GRANT ARE WORTH at `t`, summed over the
/// cards that grant it, with the live stacks the run holds.
///
/// IT TAKES THE PANEL EXPLICITLY, and that is not a style choice: in a CYCLE
/// the two forms resolve the same buff against different base rates (the
/// Furis Incarnon's 12 ticks/s against the base form's 10), so a FireRate
/// buff's absolute `per_stack` differs per form. Reading the outer `params`
/// instead of the ACTIVE form handed the base form the Incarnon form's rate.
/// The STACKS stay shared (one buff, one count, across the whole engagement);
/// only the conversion is per form.
#[inline]
#[wfsim_gas_macro::metered]
pub(super) fn buff_total(from: &FightParams, grant: crate::model::BuffGrant, stacks: &mut [LiveStacks], t: f64) -> f64 {
    from.stacking_buffs
        .iter()
        .enumerate()
        .filter(|(_, b)| b.grant == grant)
        .map(|(i, b)| b.per_stack * stacks[i].current(t, b.duration) as f64)
        .sum::<f64>()
}





/// …AND A BUMP THAT COUNTS SHELLS. `bump_buffs!` grants a whole magazine
/// per trigger, which is what an ordinary reload loads; the Incarnon route
/// loads a KNOWN number of shells and splits them across two moments, so it
/// needs to say how many. Reload-counting buffs are left out — they are
/// waiting for the reload to finish, and it has not.
#[inline]
#[wfsim_gas_macro::metered]
pub(super) fn bump_shells(params: &FightParams, stacks: &mut [LiveStacks], n: u32, t: f64, rng: &mut Rng) {
    for (i, b) in params.stacking_buffs.iter().enumerate() {
        if b.per_shell
            && b.trigger == crate::model::BuffTrigger::ReloadComplete
            && (b.chance >= 1.0 || rng.chance(b.chance))
        {
            for _ in 0..n {
                stacks[i].bump(t, b.duration, b.max_stacks);
            }
        }
    }
}

/// THE MAGAZINE IS FULL AGAIN — a reload that COMPLETED, or either Incarnon
/// transform completing, since swapping either way fully reloads the base
/// form's magazine (wiki).
///
/// One function rather than the same three lines at four sites: everything that
/// a refill ends, ends here. Ready Retaliation is armed or dropped, Reaver's
/// Rapture is reset, and the burst count restarts. THE MOMENT IS THE COMPLETION — a
/// reload that has begun has refilled nothing.
///
/// `also_a_reload` is false where the event refills without being a reload:
/// swapping OUT of the Incarnon form refills the base magazine. Strun's Blazing
/// Barrel survives it; Stug's clears on either transform (M126).
///
/// `from_empty` is `Some` where the refill ENDS a reload (a transform into the
/// Incarnon form is one) and says whether it began empty: Ready Retaliation is
/// up after one that did — see `ResolvedPanel::rs_on_reload`. `None` leaves it.
#[inline]
#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn magazine_refilled(
    params: &FightParams,
    ammo: &mut Ammo,
    r: &mut RunResult,
    stacks: &mut [LiveStacks],
    rs_armed: &mut bool,
    opening_closed: &mut bool,
    also_a_reload: bool,
    from_empty: Option<bool>,
) {
    if let Some(empty) = from_empty {
        *rs_armed = empty;
    }
    ammo.rounds_this_mag = 0;
    // HOW MANY TIMES THE MAGAZINE HAS BEEN FULL AGAIN. Counted here
    // rather than derived from `r.reloads` and `r.transforms`, because
    // those two miss the fourth site: the Incarnon EXIT refills the
    // base magazine and increments neither.
    ammo.refills += 1;
    if !*opening_closed {
        *opening_closed = true;
        r.first_magazine_damage = r.effective_damage();
    }
    for (i, b) in params.stacking_buffs.iter().enumerate() {
        let cleared = match b.cleared_by {
            crate::model::ClearedBy::MagazineRefilled => true,
            crate::model::ClearedBy::Reload => also_a_reload,
            _ => false,
        };
        if cleared {
            stacks[i] = LiveStacks::seed(0, b.max_stacks, b.duration);
        }
    }
}

/// THE WEAPON AS IT STANDS, stamped onto the combat record.
///
/// CALLED WHERE STATE CHANGES, and those are six
/// places that each own a different set of locals: the shot that spends a
/// round, the two ends of a reload, and the two ends of each transform. It
/// Written at every EVENT and not at the shot only: otherwise every event
/// between two shots carries the previous shot's weapon, and a
/// `transform_end` that has just put 216 charges in an Incarnon magazine
/// reports `base 0/12` — the one row a reader opens the record to see.
#[inline]
#[wfsim_gas_macro::metered]
pub(super) fn record_weapon(
    params: &FightParams,
    rec: &mut crate::record::Record,
    ammo: &Ammo,
    incarnon: &IncarnonState,
) {
    if rec.is_on() {
        rec.set_weapon(crate::record::WeaponAt {
            transmuted: !incarnon.in_base_form,
            magazine: (if incarnon.in_base_form { incarnon.base_magazine } else { ammo.loaded }).max(0.0) as u32,
            magazine_max: params
                .cycle
                .as_ref()
                .filter(|_| incarnon.in_base_form)
                .map_or(ammo.cap, |cy| cy.base_form.magazine_size)
                as u32,
            // THE FORM THAT IS NOT FIRING, so the free reload a
            // transmute performs on the base magazine is visible rather
            // than inferred.
            idle_magazine: params.cycle.as_ref().map(|cy| {
                if incarnon.in_base_form {
                    // THE CHARGES ARE THE GAUGE IN ANOTHER UNIT. A Laetum's 216 charges over a
                    // 12-unit gauge is 18 a unit exactly, and the row
                    // must not print `216 / 216` from the moment the
                    // fight opens — a full Incarnon magazine on a
                    // weapon that has not earned a single unit of it is
                    // the one column a reader opens this panel to
                    // check. The gauge decides it; `charges` can run
                    // past the fill mark, so the share is clamped.
                    let to_fill = match cy.arms {
                        Arms::Gauge { charges_to_fill, .. } => charges_to_fill,
                        // A melee Incarnon has no gauge, and this
                        // column is never drawn for one: melee has no
                        // magazine either.
                        Arms::HeavyAtCombo(_) => 0,
                    };
                    let share = if to_fill > 0 {
                        (f64::from(incarnon.charges) / f64::from(to_fill)).min(1.0)
                    } else {
                        0.0
                    };
                    ((share * ammo.cap).round().max(0.0) as u32, ammo.cap as u32)
                } else {
                    (incarnon.base_magazine.max(0.0) as u32, cy.base_form.magazine_size as u32)
                }
            }),
            // THE GAUGE, and it starts EMPTY — which is the model's own
            // rule and was nowhere on screen.
            // INFINITE IS `None` rather than a very large number: a
            // ruler grants it, and a column reading "1e9" is a column a
            // reader has to decode.
            reserve: (!params.infinite_reserve).then_some(ammo.reserve),
            // …AND ONLY A GAUGE IS DRAWN AS ONE. A melee Incarnon's
            // way in is a swing, so there is no bar to fill and the
            // column is absent rather than pinned at zero.
            gauge: params.cycle.as_ref().and_then(|cy| match cy.arms {
                Arms::Gauge { charges_to_fill, .. } => Some((incarnon.charges, charges_to_fill)),
                Arms::HeavyAtCombo(_) => None,
            }),
        });
    }
}

/// SAMPLING HAPPENS TWICE — once before each turn and once over the tail —
/// and it takes the whole cast because a frame is the whole fight at an
/// instant: every seat's buffs and every followed body's debuffs, not the
/// acting seat's alone.
///
/// It takes the run's state by reference rather than closing over it: the body
/// mutably borrows the seats, `bodies`, `r` and `trace` at once, which a
/// closure cannot hold together.
#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn sample_frames_up_to(
    until: f64,
    params: &FightParams,
    trace: &mut Option<&mut Replay>,
    next_frame: &mut f64,
    frame_seconds: f64,
    seats: &mut [Combatant],
    r: &RunResult,
    bodies: &[Body],
) {
    if let Some(rep) = trace.as_deref_mut() {
        while *next_frame <= until && *next_frame < params.duration_seconds {
            // ONE SERIES SET PER SEAT, each read off ITS OWN build and its own
            // pile. Sampling only whoever was about to act spliced every
            // seat's state into one curve that belonged to none of them.
            let stacks: Vec<Vec<u16>> = seats
                .iter_mut()
                .enumerate()
                .map(|(si, me)| {
                    let roster = rep.buffs.get(si).map_or(&[][..], Vec::as_slice);
                    sample_stacks(
                        me.params, roster, *next_frame, &mut me.arc, &mut me.gal,
                        &mut me.buff_stacks,
                        &me.windows.crit_on_headshot_stacks, me.windows.crit_on_headshot,
                        me.windows.weakpoint_buff, me.windows.fire_rate_after_reload,
                        me.windows.base_damage_after_reload, me.windows.base_damage_eximus,
                        me.windows.streak, me.tendril.count, me.crit_per_hit.stacks, &me.bar,
                        combo_at(me.fixed.combo_spec, me.params.combo_held,
                            me.sniper_combo.count, me.sniper_combo.last_hit, *next_frame),
                        me.incarnon.incarnon_until,
                        me.influence_until,
                    )
                    .iter()
                    .map(|(n, _)| *n)
                    .collect()
                })
                .collect();
            rep.frames.push(Frame {
                t: *next_frame,
                // ONE SET PER FOLLOWED BODY, in the same order and from the
                // same list as the debuffs below.
                pools: rep
                    .follow
                    .iter()
                    .map(|&bi| Pools {
                        overguard: bodies[bi].state.overguard,
                        shield: bodies[bi].state.shield,
                        health: bodies[bi].state.health,
                    })
                    .collect(),
                damage: r.effective_damage(),
                kills: r.kills,
                // AS THEY STAND, SEAT BY SEAT. `RunResult::per_seat` is a
                // running total credited turn by turn, so a frame is a read of
                // it rather than a second place counters are kept.
                per_seat: r.per_seat[..seats.len()].to_vec(),
                field_ticks: r.field_ticks,
                sources: r.sources,
                dealt: *r.dealt.by_combatant(),
                stacks,
                // ONE SERIES PER FOLLOWED BODY, in `Replay::tracked`'s
                // order — the aimed one first.
                debuffs: rep
                    .follow
                    .iter()
                    .map(|&bi| {
                        // THE CURVES WANT COUNTS. The expiry beside
                        // each one is the RECORD's — a chart of a
                        // stack count has no use for it.
                        bodies[bi]
                            .debuffs
                            .sample(*next_frame)
                            .into_iter()
                            .map(|(n, _)| n)
                            .collect::<Vec<u16>>()
                    })
                    .collect(),
            });
            *next_frame += frame_seconds;
        }
    }
}

/// ONE TRIGGER'S BUFFS, bumped. The per-shell family is the other arm
/// (`bump_shells`); this one is every buff a single event grants.
#[wfsim_gas_macro::metered]
pub(super) fn bump_on_trigger(
    params: &FightParams,
    stacks: &mut [LiveStacks],
    want: crate::model::BuffTrigger,
    t: f64,
    rng: &mut Rng,
) {
    for (i, b) in params.stacking_buffs.iter().enumerate() {
        if !b.per_shell && b.trigger == want && (b.chance >= 1.0 || rng.chance(b.chance)) {
            for _ in 0..b.stacks_per_trigger.max(1) {
                stacks[i].bump(t, b.duration, b.max_stacks);
            }
        }
    }
}

/// A RELOAD FROM EMPTY, and what rides on it: the trigger's own buffs, and
/// Resonant Restore, which is not a `StackingGrant` because what it grants is
/// the capacity every other line of the loop reads.
#[wfsim_gas_macro::metered]
pub(super) fn bump_reload_from_empty(
    params: &FightParams,
    stacks: &mut [LiveStacks],
    ammo: &mut Ammo,
    t: f64,
    rng: &mut Rng,
) {
    bump_on_trigger(params, stacks, crate::model::BuffTrigger::ReloadFromEmpty, t, rng);
    // RESONANT RESTORE rides the same event, because it is the same
    // event: "On Reload From Empty: Increase Base Magazine Capacity by
    // +15. Stacks up to 3x". It is not a `StackingGrant` because what
    // it grants is not a term in a bracket — it is the capacity every
    // other line of this loop reads, so it moves `mag_cap` itself.
    //
    // MONOTONIC AND CAPPED: no card in this family carries a clock, and
    // the stack count is the only thing that stops it. The magazine
    // GROWS but does not fill — a reload draws from the reserve as it
    // always did, and the extra room is what the next draw can use.
    if let Some((per, max)) = params.magazine_growth_on_empty_reload {
        if ammo.growth_stacks < max {
            ammo.growth_stacks += 1;
            ammo.cap += per * ammo.summon_multiplier;
        }
    }
}

/// One engagement, optionally SAMPLED into a [`Replay`].
///
/// `trace` is `Some` for exactly one run per `monte_carlo` — the median one,
/// replayed afterwards from its recorded RNG state. Threading an `Option`
/// through rather than duplicating the loop is the point: a traced run and a
/// scored run must be the same code, or the replay shows a fight that did not
/// happen.
#[wfsim_gas_macro::metered]
pub fn run_once_traced(
    params: &FightParams,
    rng: &mut Rng,
    mut trace: Option<&mut Replay>,
    // THE COMBAT RECORD — see [`crate::record`]. `Record::off()` for every run
    // nobody is reading, which is 999 of a thousand: it allocates nothing and
    // costs one branch per event.
    rec: &mut crate::record::Record,
) -> RunResult {
    // A FRAME THAT CASTS WRITES ITS WINDOWS AS THE RUN GOES, so the run takes a
    // copy of the params with windows of its own: runs never share them, and
    // the params a caller holds are never written.
    let owned;
    let params = match &params.frame {
        Some(spec) => {
            let mut p = params.clone();
            p.abilities_live = Some(std::sync::Arc::new(std::sync::Mutex::new(spec.opening())));
            owned = p;
            &owned
        }
        None => params,
    };
    // THE ENGAGEMENT AS IT OPENS — see [`open`]. Destructured by value, so
    // every name below is the one the setup gave it.
    let (fight, mut me) = open(params, Seat::WIELDER, rng, rec, &trace);
    // …AND THE OTHER MODE, opened the same way on a stream of its own that is
    // thrown away, so holding it moves no draw of the fight's. Only what it
    // READS is kept; what it would carry is the one weapon's and stays here.
    if let Some(tg) = &params.toggle {
        let (_, o) = open(&tg.other, Seat::WIELDER, &mut Rng::new(rng.state()), rec, &None);
        me.other = Some(super::open::OtherMode {
            params: o.params,
            fixed: o.fixed,
            apl: o.apl,
            seconds: tg.switch_seconds,
            bucket: tg.reload_bucket,
        });
    }
    // FLATTENED BACK INTO LOCALS, deliberately. The loop names these thirty-eight
    // pieces directly on the hottest path this engine has, and `open`'s own note
    // measures 2.7% a shot for handing them over any other way. What the split
    // buys is not how the loop reads it — it is that "what does a second
    // combatant share with you" now has an answer a type gives.
    let Fight {
        mut next_frame,
        mut bodies,
        mut r,
        mut entities,
        mut ghost_pile,
        body_at,
        area_near,
        bounce_bodies,
        frame_seconds,
    } = fight;
    // NOT FLATTENED. The loop named this seat's thirty-eight pieces as locals,
    // which is the one shape that cannot hold a SECOND seat: you cannot flatten
    // two magazines into one `ammo`. They are read off the combatant now, and
    // what that costs is measured rather than assumed — `one_fight`.
    // THE SEATS. One today, and the loop below no longer knows that: it takes
    // whichever is due next, which is the whole difference between a fight
    // with a combatant in it and a fight built around one.
    let mut seats: Vec<Combatant> = vec![me];
    // …AND ONE PER BUILD BESIDE IT. Each opens exactly the way the first did,
    // so a second seat is not a second code path — it is the same setup run
    // again for another build against the same arena. The world it opens is
    // thrown away: there is one fight and the first seat's copy is it.
    //
    // ITS OWN `Draws` COMES WITH IT. `open` derives the streams from the seed
    // it advances, so two seats do not share a stream and neither re-rolls the
    // other's crits — which is the property that keeps every measured number
    // where it was when a seat is added.
    for (i, also) in params.also_acting.iter().enumerate() {
        if seats.len() >= crate::fight::MAX_COMBATANTS {
            break;
        }
        let (_, other) = open(also, Seat(i + 1), rng, rec, &None);
        seats.push(other);
    }

    /// WHO ACTS NEXT — the earliest `next_t`, and a TIE GOES TO THE LOWER
    /// SEAT. Ties are not rare: two weapons on the same cadence share every
    /// instant, and without a stated order the replay would settle them in
    /// whatever order the scheduler happened to visit, which is a different
    /// fight each run.
    ///
    /// A seat that is finished parks at infinity and is never picked again.
    fn next_seat(seats: &[Combatant]) -> Option<usize> {
        seats
            .iter()
            .enumerate()
            .filter(|(_, c)| c.next_t.is_finite())
            .min_by(|(ia, a), (ib, b)| {
                a.next_t
                    .total_cmp(&b.next_t)
                    .then_with(|| ia.cmp(ib))
            })
            .map(|(i, _)| i)
    }

    // …AND ITS CONSTANTS STAY ON IT TOO. Binding them as locals would hold a
    // shared borrow of the whole combatant for the length of the loop, which is
    // the one thing that stops the mutable halves being reached at all.

    // **THE LIST THIS RUN EXECUTES**, composed once rather than per decision:
    // it is the same list for the whole engagement, and building it inside the
    // loop would allocate on every shot.
    let has_blobs = seats.iter().any(|s| s.params.blob.is_some()
        || s.params.cycle.as_ref().is_some_and(|c| c.base_form.blob.is_some()));
    let mut t;
    // WHOSE TURN, AND WHEN. The fight's clock is whatever the next seat is due
    // at; the body then reads `t` as "now" exactly as it did when there was
    // only ever one seat to be due. It ends when every seat has parked.
    loop {
        let due = next_seat(&seats);
        // A SHOT ARRIVING NO LATER THAN THE NEXT TURN LANDS FIRST, at its own
        // time (`fight::flight`). One that would arrive after the engagement
        // never does. A weapon whose shot does not fly never has one.
        let by = due.map(|s| seats[s].next_t);
        let landing = next_landing(&seats, by, params.duration_seconds);
        let next_action = landing.map(|(s, k)| seats[s].in_flight[k].t)
            .or(by).unwrap_or(f64::INFINITY);
        // A blob's clock can fill its owner's gauge between trigger pulls.
        let blob_due = has_blobs.then(|| entities.iter().filter_map(|e| match e {
            Entity::Pile(p) => e.due().map(|at| (p.owner.0, at)),
            _ => None,
        }).filter(|(_, at)| *at <= next_action && *at < params.duration_seconds)
            .min_by(|a, b| a.1.total_cmp(&b.1))).flatten();
        if let Some((si, at)) = blob_due {
            if trace.is_some() {
                sample_frames_up_to(at, params, &mut trace, &mut next_frame, frame_seconds,
                    &mut seats, &r, &bodies);
            }
            let me = &mut seats[si];
            let counters_before = SeatCounters::of(&r);
            let active = match &me.params.cycle {
                Some(cy) if me.incarnon.in_base_form => &cy.base_form,
                _ => me.params,
            };
            process_entities(&mut entities, at + 1e-9, &mut turn!(me, active, &mut me.d),
                &mut r, rec, &mut bodies);
            if let Some(cy) = me.params.cycle.as_ref().filter(|_| me.incarnon.in_base_form) {
                if let Arms::Gauge { charge_on, charges_to_fill } = cy.arms {
                    if me.incarnon.charges >= charges_to_fill {
                        let mut now = at;
                        charge_the_gauge(me.params, &me.apl, false, rec, rng, &mut me.d, cy,
                            charge_on, r.pellets, r.weakpoint_hits + r.headshots_on_others,
                            &mut now, &mut r, &mut me.ammo, &mut me.incarnon, &mut me.double_tap,
                            &mut me.buff_stacks, &mut me.rs_armed, &mut me.opening_closed);
                        if !me.incarnon.in_base_form { me.next_t = now; }
                    }
                }
            }
            r.per_seat[si].add(SeatCounters::of(&r).since(counters_before));
            continue;
        }
        if let Some((si, k)) = landing {
            let mut l = seats[si].in_flight.remove(k);
            if trace.is_some() {
                sample_frames_up_to(
                    l.t, params, &mut trace, &mut next_frame, frame_seconds,
                    &mut seats, &r, &bodies,
                );
            }
            let me = &mut seats[si];
            let counters_before = SeatCounters::of(&r);
            let active = l.active(me.params);
            // What is due before the hit settles first, as it does before a pull.
            process_ticks(
                &me.windows, &mut bodies[0], &mut me.gal, &mut me.arc, l.t + 1e-9,
                me.params, active, &mut r, rec, &mut me.d.status, &me.params.foe, 0,
            );
            // …AND EVERY ENTITY OF THIS SEAT DUE BEFORE IT: a pile whose fuse ran
            // out goes off before the round that would have fed it lands.
            process_entities(&mut entities, l.t, &mut turn!(me, active, &mut me.d), &mut r, rec, &mut bodies);
            l.pellets_before = r.pellets;
            l.headshots_before = r.weakpoint_hits + r.headshots_on_others;
            // WHAT IT DEALS POINTS BACK AT THE PULL THAT FIRED IT, not at
            // whichever pull is the latest.
            let latest = rec.attribute_to(l.shot);
            let after = land_the_shot(
                &l,
                me,
                &mut Ground {
                    r: &mut r,
                    rec,
                    rng,
                    bodies: &mut bodies,
                    entities: &mut entities,
                    body_at: &body_at,
                    bounce_bodies: &bounce_bodies,
                    area_near: &area_near,
                },
            );
            rec.attribute_to(latest);
            // A TRANSFORM THE HIT'S GAUGE STARTED holds the next pull.
            me.next_t = me.next_t.max(after);
            me.windows.sync_frenzy(&me.bar);
            r.per_seat[si].add(SeatCounters::of(&r).since(counters_before));
            continue;
        }
        let Some(seat_index) = due else { break };
        t = seats[seat_index].next_t;
        // SAMPLE first, so a frame shows the fight as it stood BEFORE the shot
        // at `t` — the same convention the timeline buckets use. Sampling here
        // rather than on a fixed clock is deliberate: this loop is the only
        // place that advances time, and a buff can only change on an event it
        // drives. A gap between shots emits repeated frames, which is exactly
        // what a fight with nothing happening in it looks like.
        //
        // AND BEFORE THE ACTING SEAT IS TAKEN, because a frame carries every
        // seat's buffs and the borrow below would leave it holding one.
        if trace.is_some() {
            sample_frames_up_to(
                t, params, &mut trace, &mut next_frame, frame_seconds,
                &mut seats, &r, &bodies,
            );
        }
        let me = &mut seats[seat_index];
        // WHOSE TURN THIS IS, for the counters. Read before and credited after
        // — see `SeatCounters`: the bumps are scattered and this is the one
        // place that knows whose they are.
        let counters_before = SeatCounters::of(&r);
        // The streams are threaded on as `&mut` from here: every function this
        // turn calls rolls off the ACTING SEAT'S own `Draws`, which is what
        // keeps a second combatant from re-rolling the first one's crits.
        let d = &mut me.d;
        // THE FLASH, READ ONCE FOR THIS SCAN. Melee state, and the one fact in
        // `Now` no other part of the fight can answer.
        let flash = me.params.tennokai.enabled && t < me.melee.tennokai_until;
        match before_the_shot(
            me.params,
            &me.apl,
            flash,
            rec,
            rng,
            d,
            &mut t,
            &mut me.arc,
            &mut me.gal,
            &mut me.buff_stacks,
            &mut me.bar,
            &mut me.windows,
            &mut me.tendril,
            &mut me.crit_per_hit,
            &mut me.incarnon,
            &mut r,
            &mut bodies,
            &mut me.ammo,
            me.last_shot_t,
            &mut me.weakpoint_pile,
            &mut me.kill_buff_mark,
            &mut me.syndicate,
            &mut ghost_pile,
            &mut me.double_tap,
            &mut me.rs_armed,
            &mut me.opening_closed,
            &mut me.field_duration_boost,
            me.other.as_ref().map(|o| super::cycle::SwitchTo { form: o.params.form.id(), seconds: o.seconds, bucket: o.bucket }),
        ) {
            // A SEAT THAT CANNOT ACT AGAIN IS DONE, and the FIGHT is not:
            // your magazine running out at second 40 does not stop a companion
            // that fires until 180. It parks, and the loop ends when every
            // seat has.
            Flow::Break => {
                me.next_t = f64::INFINITY;
                r.per_seat[seat_index].add(SeatCounters::of(&r).since(counters_before));
                continue;
            }
            // …AND ONE THAT IS WAITING GIVES THE TURN BACK. `before_the_shot`
            // has already moved `t` to whenever it can act next, so writing it
            // down and re-picking is what lets another seat fire during a
            // reload rather than after it.
            Flow::Continue => {
                me.next_t = t;
                r.per_seat[seat_index].add(SeatCounters::of(&r).since(counters_before));
                continue;
            }
            // THE OTHER MODE, TAKEN UP: its params, what it reads and its list,
            // and the one magazine held to what this mode holds.
            Flow::Switch => {
                if let Some(o) = me.other.as_mut() {
                    std::mem::swap(&mut me.params, &mut o.params);
                    std::mem::swap(&mut me.fixed, &mut o.fixed);
                    std::mem::swap(&mut me.apl, &mut o.apl);
                }
                me.ammo.cap = me.params.magazine_size;
                me.ammo.loaded = me.ammo.loaded.min(me.ammo.cap);
                me.next_t = t;
                r.per_seat[seat_index].add(SeatCounters::of(&r).since(counters_before));
                continue;
            }
            Flow::Go => {}
        }

        // Active-phase view: the base form's panel during the rebuild
        // phase, the outer me.params otherwise. Target/aim/locks are shared
        // from the outer me.params.
        let active: &FightParams = match &me.params.cycle {
            Some(cy) if me.incarnon.in_base_form => &cy.base_form,
            _ => me.params,
        };
        // The instance total and its SHAPE (Toxin's shield bypass, the
        // vulnerability column) are derived PER STAGE now — each attack part
        // has its own vector — so only the vector and ModifiedBase survive at
        // pellet scope.
        let (qvec, modded_base) = if me.incarnon.in_base_form {
            let p = me.fixed.base_pre.as_ref().expect("cycle state needs base pre");
            (&p.0, p.2)
        } else {
            (&me.fixed.main_pre.0, me.fixed.main_pre.2)
        };
        // FRENZY'S TOXIN IS A BUFF'S, so it is on this shot only while the
        // buff is: read before the bar lapses it, which is what lets the
        // ticks settled below see the exact moment it ran out.
        me.windows.sync_frenzy(&me.bar);
        let resting =
            if me.incarnon.in_base_form { &me.fixed.base_resting } else { &me.fixed.main_resting };
        let qvec = match resting {
            Some(down) if t >= me.windows.frenzy => down,
            _ => qvec,
        };
        // …AND A COMPANION'S PILE, settled on the hits landed so far and read
        // for this shot: the vector its count resolved to. No robotic weapon
        // has Frenzy, so the two never meet.
        let piled_q;
        let qvec = match &active.per_hit_element {
            Some(p) => {
                me.windows.element_pile.settle(&p.spec, r.pellets, me.last_shot_t);
                match me.windows.element_pile.stacks_at(&p.spec, t) {
                    0 => qvec,
                    k => {
                        piled_q = p.damage[k as usize - 1].quantized_against(modded_base);
                        &piled_q
                    }
                }
            }
            None => qvec,
        };
        // ---- THE MELEE SWING THIS SHOT IS — see [`swing_this_shot`] ------
        let Swung {
            swing,
            initial_now,
            combo_now,
            combo_multiplier,
            tennokai,
            tennokai_heavy,
            tennokai_kill_mark,
            swing_mult,
            melee_struck,
            swing_forced_types,
            swing_forced_independent,
            qvec,
            modded_base,
            direct_pre_snap,
            co_base,
            detonates,
        } = swing_this_shot(
            me.params,
            &me.apl,
            active,
            t,
            qvec,
            modded_base,
            &mut me.buff_stacks,
            &mut me.melee,
            &r,
        );

        // Status events scheduled before this shot land first.
        process_ticks(
            &me.windows,
            &mut bodies[0],
            &mut me.gal,
            &mut me.arc,
            t + 1e-9,
            me.params,
            active,
            &mut r,
            rec,
            &mut d.status,
            &me.params.foe,
            0,
        );

        let Resolved {
            flat_crit,
            crit_chance_relative,
            effective_cc,
            undamaged,
            weakpoint_cd,
            weakpoint_sc,
            sc_arc_shot,
            live_rate,
            bd_reload_add,
            bd_eximus_add,
            rolled,
            sc_mult,
            cc_mult,
            dt_mult,
            ms_damage,
            n_pellets,
            mut beam_merge,
            blob_stacks,
            mut rounds,
        } = resolve_the_shot(
            me.params,
            active,
            rec,
            d,
            t,
            combo_multiplier,
            &mut me.bar,
            &mut me.arc,
            &mut me.gal,
            &mut me.buff_stacks,
            &me.fixed.rec_roster,
            &me.fixed.rec_buff_index,
            &mut me.windows,
            &me.tendril,
            &me.crit_per_hit,
            &me.sniper_combo,
            me.fixed.combo_spec,
            me.influence_until,
            &mut me.incarnon,
            &mut me.ammo,
            &mut bodies[0],
            &mut me.weakpoint_pile,
            &mut me.double_tap,
            super::melee::animosity_crit(me.params, &me.melee, active.spends_combo || tennokai_heavy),
        );
        // AN ORB ATTACK FIRES NO PELLETS — when the TRIGGER is what deploys it.
        // The shot settles no collision and no explosion, because everything it
        // deals is delivered later by the orb from wherever the orb is.
        //
        // A METER MOVES THE THROW OFF THE TRIGGER, and then the trigger goes
        // back to doing what it does: in a Tome's CYCLE the weapon is firing
        // its primary the whole time and the orb goes out on the clock, so the
        // pellets are the primary's and this must not touch them. Zeroing them
        // regardless was worth the whole base form — a cycle reporting 169 DPS
        // against the 1,419 its gun alone deals.
        //
        // The COUNT is zeroed rather than the loop jumped past, and that is not
        // a style choice — a `continue` here skips the rest of the shot's own
        // body, which is where the clock, the magazine and the reload live. It
        // hung the engine on the first run.
        let mut n_pellets = if active.orb.is_some() && active.meter.is_none() { 0 } else { n_pellets };
        // A SWING THAT LANDS TWICE IS TWO INSTANCES. `Hits = { 1, 2 }` in the
        // wiki's own module — Crushing Ruin's forward combo lands its second
        // 100% twice and Shattered Village lands two 50% spins per attack — and
        // each is its own crit roll, its own status roll and its own combo
        // point, which is what makes it a COUNT here rather than a multiplier
        // on the damage.
        //
        // It rides the pellet loop because that loop already means "this many
        // instances of this attack", which is the same thing multishot means.
        // Melee has no multishot mod in its pool, so the two can never compete.
        // …AND A TENNOKAI SWING LANDS ONCE — see `swing_instances`.
        if let Some(h) = &swing {
            let n = swing_instances(h.hits, tennokai_heavy);
            if n > 1 {
                n_pellets = n_pellets.saturating_mul(n);
            }
        }
        if active.multishot_ammo_bonus > 0.0 && rolled > 1 {
            // `ammo_efficiency_applies == false` IS the charge-backed marker —
            // such a magazine is "outside the ammo economy entirely", so it has
            // no Capacity behind it and the surcharge comes out of the charge
            // pool itself. That is what shortens the Incarnon window.
            let charge_backed = !active.ammo_efficiency_applies;
            let mut afforded = 0u32;
            for _ in 0..rolled - 1 {
                let pool = if charge_backed {
                    if me.incarnon.in_base_form { &mut me.incarnon.base_magazine } else { &mut me.ammo.loaded }
                } else {
                    // From CAPACITY. With infinite reserves — which the Incarnon
                    // cycle's base phase always assumes — nothing can starve,
                    // which is correct rather than missing.
                    if me.params.infinite_reserve {
                        afforded += 1;
                        continue;
                    }
                    &mut me.ammo.reserve
                };
                if *pool < 1.0 - 1e-9 {
                    break;
                }
                *pool -= 1.0;
                afforded += 1;
            }
            // A beam merges its multishot into ONE instance, so starvation
            // shows up as a smaller merge multiplier, not as fewer instances.
            if active.continuous {
                let base_ms = active.base_multishot.max(1.0);
                let live = (1 + afforded) as f64;
                beam_merge = base_ms + (live - base_ms) * (1.0 + active.multishot_ammo_bonus);
            } else {
                n_pellets = 1 + afforded;
                rounds.truncate(n_pellets);
            }
        }
        // The damage ramp, evaluated once per tick. A FINAL multiplier on the
        // instance and NOT on ModifiedBase: it is a transient scaling of the
        // beam's output, not a weapon-stat change, so the status payloads are
        // left out of it. Nothing sources that either way — flagged in
        // MECHANICS — but it is a sub-2% question on sustained fire, unlike the
        // merge above.
        let beam_ramp = if active.continuous {
            me.beam.tick(t, 1.0 / live_rate.max(1e-9), active.beam_ramp_floor)
        } else {
            1.0
        };
        // THE GAUGE'S OWN MARK, and it counts what the gauge counts —
        // `RunResult::weakpoint_hits`, not the reported headshot rate.
        let headshots_before = r.weakpoint_hits + r.headshots_on_others;
        let pellets_before = r.pellets;
        // THE HEADSHOT-DAMAGE BRACKETS as of this shot: the field's head ladder
        // below, and what a Tesla arc is worth on a neighbour's head.
        let (shot_hb, shot_hi) = {
            let streak = match me.params.headshot_streak {
                Some(s) if t < me.windows.streak => s.value,
                _ => 0.0,
            } + buff_total(active, crate::model::BuffGrant::HeadshotDamage, &mut me.buff_stacks, t);
            if active.headshot_bonus_multiplicative {
                (me.params.arcane.headshot_multiplier_bonus + streak, active.headshot_damage_bonus)
            } else {
                (
                    me.params.arcane.headshot_multiplier_bonus + streak + active.headshot_damage_bonus,
                    0.0,
                )
            }
        };
        let shot_head_landing = (1.0 + shot_hb) * (1.0 + shot_hi);
        // Field ticks due before this shot, with the buff state as of now.
        me.field_ctx = FieldCtx {
            direct_hit: false,
            weakpoint_direct: false,
            critical_head_direct: false,
            flat_crit,
            crit_chance_relative_mods: crit_chance_relative - me.params.arcane.crit_chance_relative,
            crit_damage_relative_mods: buff_total(active, crate::model::BuffGrant::CritDamage, &mut me.buff_stacks, t),
            base_damage_add_mods: bd_reload_add
                + bd_eximus_add
                + active.compression_base_damage
                + buff_total(active, crate::model::BuffGrant::BaseDamage, &mut me.buff_stacks, t)
                + buff_total(active, crate::model::BuffGrant::FlatBaseDamage, &mut me.buff_stacks, t),
            // The same ladder the pellet loop builds below, on the aimed body's
            // own head — see the field's note on why it is computed twice
            // rather than shared.
            head_factor: {
                let m = me.params
                    .body_parts
                    .iter()
                    .find(|p| p.is_head)
                    .map_or(1.0, |p| p.multiplier);
                active.head_value(m, 1.5) * (1.0 + shot_hb) * (1.0 + shot_hi)
            },
            head_landing: shot_head_landing,
        };
        settle_what_is_in_the_air(
            &mut turn!(me, active, &mut *d),
            rec,
            &mut t,
            &mut entities,
            &mut me.meter,
            &mut me.ammo,
            &mut bodies,
            &mut r,
            &mut me.strip_kills_seen,
        );
        // …AND THE DAGGERS THE KILLS SINCE THE LAST SWING MADE, each at its own
        // kill's time — they fly on a clock of their own and never hold one up.
        make_daggers(me.seat, me.params, active, t, &mut r, &mut me.dagger_kill_mark, &mut entities);
        process_entities(&mut entities, t, &mut turn!(me, active, &mut *d), &mut r, rec, &mut bodies);
        r.shots += 1;
        r.body_shots += bodies.len() as u32;

        // A SHOT THAT DEPLOYS RATHER THAN ARRIVES. An orb attack settles no
        // collision and no explosion here: everything it deals is delivered
        // later, by the orb, from wherever the orb is at the time. So the
        // pellet loop is skipped whole.
        //
        // ONE ORB, whatever multishot says. On this weapon multishot buys CHAIN
        // TARGETS instead (*"Number of chains is affected by Multishot"*), and
        // that is already inside `ResolvedOrb::chain_bodies` — so the count is
        // spent where the game spends it rather than on a second projectile.
        // A METER MOVES THE THROW OFF THE TRIGGER. With one, the shot loop fires
        // the weapon's ordinary attack and the orb goes out when the clock says
        // so — which is the whole weapon: you shoot, and every so often you
        // throw. Without one, the trigger deploys, which is what the form's own
        // `transformed` mode shows.
        if let Some(o) = active.orb.filter(|_| active.meter.is_none()) {
            throw_orb(o, me.seat, me.params, t, &mut entities);
        }

        // WHAT THE PULL DECIDED, owned by the hit it becomes (`fight::flight`).
        let landing = Landing {
            t,
            base_form: me.incarnon.in_base_form,
            shot: rec.shot(),
            flash,
            qvec,
            direct_pre_snap,
            modded_base,
            co_base,
            combo_now,
            combo_multiplier,
            swing,
            swing_mult,
            swing_forced_types,
            swing_forced_independent,
            tennokai,
            tennokai_heavy,
            tennokai_kill_mark,
            detonates,
            melee_struck,
            pierce_live_m: buff_total(active, crate::model::BuffGrant::PunchThrough, &mut me.buff_stacks, t),
            effective_cc,
            crit_chance_relative,
            flat_crit,
            cc_mult,
            sc_mult,
            sc_arc_shot,
            weakpoint_cd,
            weakpoint_sc,
            bd_reload_add,
            bd_eximus_add,
            dt_mult,
            ms_damage,
            live_rate,
            beam_ramp,
            next_shot_bonus: if me.weakpoint_next_shot_armed {
                me.params.arcane.weakpoint_next_shot_damage
            } else {
                0.0
            },
            undamaged,
            n_pellets,
            beam_merge,
            blob_stacks,
            rounds,
            shot_head_landing,
            pellets_before,
            headshots_before,
            burst_completed: None,
            burst_starts: (me.ammo.rounds_this_mag.max(1) - 1)
                .is_multiple_of(active.burst.map_or(1, |b| b.count.max(1))),
        };
        // A SHOT THAT FLIES IS DUE THE GAP OVER ITS SPEED FROM NOW, and the
        // trigger does not wait for it: the next pull is timed from this one.
        let flight = active.flight_seconds(me.params.gap());
        if flight > 0.0 {
            let swing = landing.swing.clone();
            let burst_len = active.burst.map_or(1, |b| b.count.max(1));
            me.in_flight.push(Landing {
                t: t + flight,
                burst_completed: Some(me.ammo.rounds_this_mag.is_multiple_of(burst_len)),
                ..landing
            });
            next_shot_due(
                me.params, active, &mut t, swing, initial_now, tennokai, tennokai_heavy, &r,
                &mut me.bar, &mut me.buff_stacks, &me.ammo, &me.incarnon, &mut me.melee,
                &mut me.spool, &me.windows, &mut me.last_shot_t,
            );
        } else {
            t = land_the_shot(
                &landing,
                me,
                &mut Ground {
                    r: &mut r,
                    rec,
                    rng,
                    bodies: &mut bodies,
                    entities: &mut entities,
                    body_at: &body_at,
                    bounce_bodies: &bounce_bodies,
                    area_near: &area_near,
                },
            );
            next_shot_due(
                me.params, active, &mut t, landing.swing, initial_now, tennokai, tennokai_heavy, &r,
                &mut me.bar, &mut me.buff_stacks, &me.ammo, &me.incarnon, &mut me.melee,
                &mut me.spool, &me.windows, &mut me.last_shot_t,
            );
        }
        // AN ABILITY CAST THE FRAME MADE ON THIS TURN earns what a cast earns,
        // paid when the turn ends: the piles it feeds have no clock to run.
        if let Some(n) = me.spool.frame.as_mut().map(crate::data::casting::FrameRuntime::take_casts) {
            for _ in 0..n {
                bump_on_trigger(me.params, &mut me.buff_stacks, crate::model::BuffTrigger::AbilityCast, t, &mut me.d.spine);
            }
        }
        // WHERE THIS SEAT IS DUE NEXT. `next_shot_due` advanced `t` by this
        // weapon's cadence, which is this seat's clock and nobody else's.
        me.next_t = t;
        // …and again once the shot has refreshed it, for the ticks the
        // fight settles after its last shot.
        me.windows.sync_frenzy(&me.bar);
        r.per_seat[seat_index].add(SeatCounters::of(&r).since(counters_before));
    }

    // THE METER'S LAST FILLS, after the trigger stops. A weapon that is out of
    // ammo still recharges, and an orb earned at t = 179 is an orb the fight
    // gets — so the clock is run out to the end and every throw it buys is
    // thrown, before the orbs are drained below.
    // EVERY SEAT'S METER, not one seat's. A combatant that carries a tome
    // earns its last orbs whoever else is in the fight.
    for me in seats.iter_mut() {
    let d = &mut me.d;
    let _ = &d;
    if let (Some(m), Some(o)) = (me.fixed.field_active.meter, me.fixed.field_active.orb) {
        me.meter.seconds += params.duration_seconds - me.meter.clocked;
        while me.meter.seconds >= m.seconds_to_fill {
            me.meter.seconds -= m.seconds_to_fill;
            // AT THE INSTANT IT FILLED, not at the end: the orb has a six
            // second fuse and the difference is whether its strikes land inside
            // the engagement at all.
            let at = params.duration_seconds - me.meter.seconds;
            // WHAT IS ALREADY IN THE AIR GETS TO LIVE UNTIL IT IS REPLACED.
            // Only one orb exists at a time, so throwing them all and walking
            // the list afterwards would leave the LAST one and silently drop
            // every other — which is what happened the day the replace rule
            // landed. Settling up to the throw is the same order
            // the shot loop settles them in.
            let active = me.fixed.field_active;
            process_entities(&mut entities, at, &mut turn!(me, active, &mut *d), &mut r, rec, &mut bodies);
            throw_orb(o, me.seat, params, at, &mut entities);
        }
    }
    }

    // EACH OWNER SETTLES ITS OWN, up to the end of the engagement: an entity
    // carries the seat that left it and reads that SHOOTER's live windows, so
    // draining the whole list under one seat's cards would pay a second seat's
    // leavings out of the wielder's build. What would act after the end never
    // does — a pile whose fuse outlasts it, a volley thrown too late — and the
    // clouds still burning tick with the buff snapshot of the last shot.
    let end = params.duration_seconds;
    for me in seats.iter_mut() {
        let active = match &me.params.cycle {
            Some(cy) if me.incarnon.in_base_form => &cy.base_form,
            _ => me.params,
        };
        // A MAGAZINE THROWN BY A RELOAD NO SHOT FOLLOWED — the engagement ended
        // inside it.
        if let Some((at, from_empty)) = me.ammo.grenade_thrown_at.take() {
            entities.push(Entity::Volley(Volley { owner: me.seat, at, from_empty }));
        }
        // THE KILLS SINCE THE LAST SWING still make their daggers, and one made
        // in time lands inside the engagement.
        make_daggers(me.seat, me.params, active, end, &mut r, &mut me.dagger_kill_mark, &mut entities);
        process_entities(&mut entities, end, &mut turn!(me, active, &mut me.d), &mut r, rec, &mut bodies);
    }
    // …then drain what is left up to the end of the engagement — EVERY BODY,
    // not just the aimed one. A neighbour a chain hop or a splash set burning
    // was drained once per shot and then abandoned at the last one, so whatever
    // was still on it when firing stopped was recorded and never paid.
    //
    // WHOSE WINDOWS A DoT IS DRAINED UNDER IS STILL THE WIELDER'S. A tick reads
    // live element buffs (`FightParams::element_at`) and takes one set for the
    // whole body, while the damage it books already carries its own `dot_owner`
    // — so with a second seat the credit is right and the size is the
    // wielder's. It closes inside `process_ticks`, by the tick reading the
    // state of the seat that seeded it.
    let me = &mut seats[Seat::WIELDER.0];
    process_ticks(
        &me.windows,
        &mut bodies[0],
        &mut me.gal,
        &mut me.arc,
        end,
        params,
        me.fixed.field_active,
        &mut r,
        rec,
        &mut me.d.status,
        &params.foe,
        0,
    );
    settle_crowd_ticks(
        &me.windows, &mut bodies, &mut me.gal, &mut me.arc, end,
        params, me.fixed.field_active, &mut r, rec, &mut me.d.status,
    );

    // THE REPLAY COVERS THE WHOLE FIGHT, INCLUDING THE PART WITH NO SHOOTING
    // IN IT. The firing loop `break`s the moment a finite reserve runs dry,
    // and the sampler must NOT go with it: a 180-second engagement that runs
    // out of ammo at 58.5 would be drawn as a 58.5-second one, and every rate
    // the replay derives would divide by that shorter clock — 378 KPM beside
    // its own `852.83 kill score in 180s`, which is 284.
    //
    // Not only the divisor: the two `process_*` calls above settle the
    // burning clouds and the remaining DoTs to the end, so KILLS land after the
    // last shot too.
    //
    // THE TAIL IS FLAT, a real limitation rather than a rounding one: the drain
    // runs to the end in one step, so every frame after it carries the settled
    // state and the DoT decline is drawn as a single step where the firing
    // stopped. Sampling INSIDE the drain would mean stepping both `process_*`
    // calls a frame at a time, which reorders how status settles — a
    // golden-value change rather than a rendering one.
    sample_frames_up_to(
        params.duration_seconds, params, &mut trace, &mut next_frame, frame_seconds,
        &mut seats, &r, &bodies,
    );

    // Partial credit: the fraction of the current individual's TOTAL bar
    // already depleted — overguard + shield + health: the
    // whole bar counts, so shield damage earns progress and shield REGEN
    // gives it back). If health has hit 0 the unit is DEAD, so the whole bar
    // is gone regardless of any overguard/shield left (e.g. a Toxin-bypass
    // kill that never broke the shield) — full credit.
    // InfiniteHealth pools never deplete -> 0.
    //
    // A THRAX'S BAR IS BOTH OF ITS FORMS. With the spectral switch on, emptying
    // the physical form is real progress and is not the kill, so the spectre's
    // health joins the denominator: a gun that cannot touch it stalls at the
    // physical form's share of the individual instead of scoring a kill.
    let spectral_health =
        params.foe.spectral.map_or(0.0, |sp| params.foe.max_health() * sp.health_share);
    let pool = params.foe.overguard()
        + params.foe.max_shield()
        + params.foe.max_health()
        + spectral_health;
    let aimed = &bodies[0].state;
    let remaining = if aimed.health <= 0.0 {
        0.0
    } else {
        aimed.overguard + aimed.shield + aimed.health
    };
    let partial = if pool > 0.0 {
        (1.0 - remaining / pool).clamp(0.0, 1.0)
    } else {
        0.0
    };
    r.kill_progress = r.kills as f64 + partial;

    r
}
