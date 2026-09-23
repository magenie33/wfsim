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

pub(super) fn dur(spec: &Option<crate::model::StackSpec>) -> f64 {
    spec.as_ref().map_or(0.0, |s| s.duration)
}

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
/// a refill ends, ends here. Ready Retaliation is spent, Reaver's Rapture is
/// reset, and the burst count restarts. THE MOMENT IS THE COMPLETION — a
/// reload that has begun has refilled nothing.
///
/// `also_a_reload` is false where the event refills without being a reload:
/// swapping OUT of the Incarnon form refills the base magazine, and Blazing
/// Barrel is stated to survive it.
#[inline]
pub(super) fn magazine_refilled(
    params: &FightParams,
    ammo: &mut Ammo,
    r: &mut RunResult,
    stacks: &mut [LiveStacks],
    rs_armed: &mut bool,
    opening_closed: &mut bool,
    also_a_reload: bool,
) {
    *rs_armed = false;
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

/// SAMPLING HAPPENS TWICE, and the second time is
/// body mutably borrows `arc`, `gal`, `buff_stacks`, `target`, `r`,
/// `debuffs`, `others` and `trace` at once, which a closure cannot hold
/// together.
///
/// It takes the run's state by reference rather than closing over it: the
/// sampler reads nine pieces at once and writes one frame, which is the whole
/// reason it was a macro.
#[allow(clippy::too_many_arguments)]
pub(super) fn sample_frames_up_to(
    until: f64,
    params: &FightParams,
    trace: &mut Option<&mut Replay>,
    next_frame: &mut f64,
    frame_seconds: f64,
    arc: &mut ArcRuntime,
    gal: &mut GalStacks,
    buff_stacks: &mut [LiveStacks],
    bar: &BuffBar,
    windows: &CardWindows,
    tendril: &Tendrils,
    crit_per_hit: &CritPerHit,
    sniper_combo: &SniperComboCount,
    combo_spec: Option<crate::model::SniperCombo>,
    incarnon: &IncarnonState,
    influence_until: f64,
    target: &TargetState,
    r: &RunResult,
    debuffs: &DebuffState,
    others: &[SpreadFoe],
) {
    if let Some(rep) = trace.as_deref_mut() {
        while *next_frame <= until && *next_frame < params.duration_seconds {
            let stacks = sample_stacks(
                params, &rep.buffs, *next_frame, arc, gal, buff_stacks,
                &windows.crit_on_headshot_stacks, windows.crit_on_headshot, windows.weakpoint_buff,
                windows.fire_rate_after_reload, windows.base_damage_after_reload,
                windows.base_damage_eximus, windows.streak, tendril.count, crit_per_hit.stacks, bar,
                combo_at(combo_spec, params.combo_held, sniper_combo.count,
                    sniper_combo.last_hit, *next_frame),
                incarnon.incarnon_until,
        influence_until,
            );
            rep.frames.push(Frame {
                t: *next_frame,
                overguard: target.overguard,
                shield: target.shield,
                health: target.health,
                damage: r.effective_damage(),
                kills: r.kills,
                shots: r.shots,
                pellets: r.pellets,
                crits: r.crits,
                big_crits: r.big_crits,
                crit_tier_sum: r.crit_tier_sum,
                headshots: r.headshots,
                procs: r.procs,
                field_ticks: r.field_ticks,
                reloads: r.reloads,
                transforms: r.transforms,
                sources: r.sources,
                dealt: *r.dealt.by_combatant(),
                stacks: stacks.iter().map(|(n, _)| *n).collect(),
                // ONE SERIES PER FOLLOWED BODY, in `Replay::tracked`'s
                // order — the aimed one first.
                debuffs: rep
                    .follow
                    .iter()
                    .map(|&bi| {
                        // THE CURVES WANT COUNTS. The expiry beside
                        // each one is the RECORD's — a chart of a
                        // stack count has no use for it.
                        let one = if bi == 0 {
                            debuffs.sample(*next_frame)
                        } else {
                            others[bi - 1].debuffs.sample(*next_frame)
                        };
                        one.into_iter().map(|(n, _)| n).collect::<Vec<u16>>()
                    })
                    .collect(),
            });
            *next_frame += frame_seconds;
        }
    }
}

/// ONE TRIGGER'S BUFFS, bumped. The per-shell family is the other arm
/// (`bump_shells`); this one is every buff a single event grants.
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
pub fn run_once_traced(
    params: &FightParams,
    rng: &mut Rng,
    mut trace: Option<&mut Replay>,
    // THE COMBAT RECORD — see [`crate::record`]. `Record::off()` for every run
    // nobody is reading, which is 999 of a thousand: it allocates nothing and
    // costs one branch per event.
    rec: &mut crate::record::Record,
) -> RunResult {
    // THE ENGAGEMENT AS IT OPENS — see [`open`]. Destructured by value, so
    // every name below is the one the setup gave it.
    let (fight, me) = open(params, rng, rec, &trace);
    // FLATTENED BACK INTO LOCALS, deliberately. The loop names these thirty-eight
    // pieces directly on the hottest path this engine has, and `open`'s own note
    // measures 2.7% a shot for handing them over any other way. What the split
    // buys is not how the loop reads it — it is that "what does a second
    // combatant share with you" now has an answer a type gives.
    let Fight {
        mut t,
        mut next_frame,
        mut target,
        mut debuffs,
        mut others,
        mut r,
        mut fields,
        mut orbs,
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
    let mut me = me;
    // The streams are threaded on as `&mut` from here: every function the
    // loop calls rolls off this one `Draws`.
    let d = &mut me.d;
    // …AND ITS CONSTANTS STAY ON IT TOO. Binding them as locals would hold a
    // shared borrow of the whole combatant for the length of the loop, which is
    // the one thing that stops the mutable halves being reached at all.

    // **THE LIST THIS RUN EXECUTES**, composed once rather than per decision:
    // it is the same list for the whole engagement, and building it inside the
    // loop would allocate on every shot.
    let apl = params.apl();
    loop {
        // SAMPLE first, so a frame shows the fight as it stood BEFORE the
        // shot at `t` — the same convention the timeline buckets use.
        // Sampling here rather than on a fixed clock is deliberate: this loop
        // is the only place that advances time, and a buff can only change on
        // an event this loop drives. A gap between shots emits repeated
        // frames, which is exactly what a fight with nothing happening in it
        // looks like.
            // NOTHING TO SAMPLE WITHOUT A TRACE, and the check is HERE rather than
        // only inside: the sampler reads twenty pieces of the run, and a fight
        // nobody is replaying should not pay for passing them.
        // THE FLASH, READ ONCE FOR THIS SCAN. Melee state, and the one fact in
        // `Now` no other part of the fight can answer.
        let flash = params.tennokai.enabled && t < me.melee.tennokai_until;
        match before_the_shot(
            params,
            &apl,
            flash,
            rec,
            rng,
            &mut trace,
            d,
            &mut t,
            &mut next_frame,
            frame_seconds,
            &mut me.arc,
            &mut me.gal,
            &mut me.buff_stacks,
            &mut me.bar,
            &mut me.windows,
            &mut me.tendril,
            &mut me.crit_per_hit,
            &me.sniper_combo,
            me.fixed.combo_spec,
            &mut me.incarnon,
            me.influence_until,
            &mut target,
            &mut r,
            &mut debuffs,
            &others,
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
        ) {
            Flow::Continue => continue,
            Flow::Break => break,
            Flow::Go => {}
        }

        // Active-phase view: the base form's panel during the rebuild
        // phase, the outer params otherwise. Target/aim/locks are shared
        // from the outer params.
        let active: &FightParams = match &params.cycle {
            Some(cy) if me.incarnon.in_base_form => &cy.base_form,
            _ => params,
        };
        // …AND WHO IS ON THE LINE IS THE ACTIVE FORM'S ANSWER TOO, for the same
        // reason: a form's punch through is its own (`open`).
        let struck: &[usize] = match &me.fixed.base_struck {
            Some(b) if me.incarnon.in_base_form => b,
            _ => &me.fixed.struck,
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
        } = swing_this_shot(
            params,
            &apl,
            active,
            t,
            qvec,
            modded_base,
            &mut me.buff_stacks,
            &mut me.melee,
            &r,
        );
        // …and the instances read it where they always did.
        let qvec = &qvec;
        // The per-projectile vectors belong to the FORM that is firing, like
        // everything else at this scope. A cycle whose base form has them and
        // whose Incarnon form does not simply reads an empty slice there.
        let (variants, variant_rad): (&[_], &[_]) = if me.incarnon.in_base_form {
            match params.cycle.as_ref() {
                Some(_) => (&me.fixed.base_variants, &me.fixed.base_variant_rad),
                None => (&me.fixed.main_variants, &me.fixed.main_variant_rad),
            }
        } else {
            (&me.fixed.main_variants, &me.fixed.main_variant_rad)
        };

        // Status events scheduled before this shot land first.
        process_ticks(
            &me.windows,
            &mut debuffs,
            &mut me.gal,
            &mut me.arc,
            t + 1e-9,
            &mut target,
            params,
            active,
            &mut r,
            rec,
            &mut d.status,
            &params.foe,
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
        } = resolve_the_shot(
            params,
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
            &mut debuffs,
            &target,
            &mut me.weakpoint_pile,
            &mut me.double_tap,
            &mut me.rs_armed,
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
                    if params.infinite_reserve {
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
        let (mut any_head, mut any_big) = (false, false);
        let headshots_before = r.headshots + r.headshots_on_others;
        let pellets_before = r.pellets;
        // THE HEADSHOT-DAMAGE BRACKETS as of this shot: the field's head ladder
        // below, and what a Tesla arc is worth on a neighbour's head.
        let (shot_hb, shot_hi) = {
            let streak = match params.headshot_streak {
                Some(s) if t < me.windows.streak => s.value,
                _ => 0.0,
            } + buff_total(active, crate::model::BuffGrant::HeadshotDamage, &mut me.buff_stacks, t);
            if active.headshot_bonus_multiplicative {
                (params.arcane.headshot_multiplier_bonus + streak, active.headshot_damage_bonus)
            } else {
                (
                    params.arcane.headshot_multiplier_bonus + streak + active.headshot_damage_bonus,
                    0.0,
                )
            }
        };
        let shot_head_landing = (1.0 + shot_hb) * (1.0 + shot_hi);
        // Field ticks due before this shot, with the buff state as of now.
        me.field_ctx = FieldCtx {
            flat_crit,
            crit_chance_relative_mods: crit_chance_relative - params.arcane.crit_chance_relative,
            base_damage_add_mods: bd_reload_add
                + bd_eximus_add
                + active.compression_base_damage
                + buff_total(active, crate::model::BuffGrant::BaseDamage, &mut me.buff_stacks, t)
                + buff_total(active, crate::model::BuffGrant::FlatBaseDamage, &mut me.buff_stacks, t),
            // The same ladder the pellet loop builds below, on the aimed body's
            // own head — see the field's note on why it is computed twice
            // rather than shared.
            head_factor: {
                let m = params
                    .body_parts
                    .iter()
                    .find(|p| p.is_head)
                    .map_or(1.0, |p| p.multiplier);
                let m = active.headshot_multiplier.unwrap_or(m);
                (m + 1.5 * active.weakpoint_damage) * (1.0 + shot_hb) * (1.0 + shot_hi)
            },
            head_landing: shot_head_landing,
        };
        settle_what_is_in_the_air(
            &me.windows,
            params,
            active,
            me.fixed.field_active,
            rec,
            d,
            &mut t,
            &me.field_ctx,
            &mut fields,
            &mut orbs,
            &mut me.meter,
            me.seat,
            &mut me.ammo,
            &mut target,
            &mut debuffs,
            &mut others,
            &mut me.gal,
            &mut me.arc,
            &mut r,
            &mut me.strip_kills_seen,
        );
        // Secondary Encumber: at most ONE extra proc per instant — pellets
        // of one pull land simultaneously, so one roll per pull.
        let mut encumber_done = false;
        r.shots += 1;
        // ...and the same boundary for a per-instance arcane cap: the whole
        // pull is ONE damage instance, pellets and radial included.
        me.arc.next_instance();

        // DID THIS SHOT HIT ANYTHING AT ALL — the question the SHOT COMBO
        // COUNTER asks, and it is the shot's rather than the pellet's: a
        // multishot pull that puts one pellet of six on the target is a hit.
        let mut landed_this_shot = false;

        // THE SHOT'S FACTORS, filled by its first landing pellet — see
        // `SpreadStrike`. `None` when nothing landed, and then nothing spreads.
        let mut strike_spread: Option<SpreadStrike> = None;

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
            throw_orb(o, me.seat, params, t, &mut orbs);
        }

        {
            let shot = Strike {
                seat: me.seat,
                active,
                qvec,
                direct_pre_snap,
                variants,
                variant_rad,
                modded_base,
                co_base,
                combo_multiplier,
                combo_spec: me.fixed.combo_spec,
                swing: &swing,
                swing_mult,
                swing_forced_types: &swing_forced_types,
                swing_forced_independent: &swing_forced_independent,
                tennokai,
                tennokai_heavy,
                melee_struck: &melee_struck,
                struck,
                body_at: &body_at,
                bounce_bodies: &bounce_bodies,
                chain_layout: &me.fixed.chain_layout,
                ricochet_layout: &me.fixed.ricochet_layout,
                aim_off_axis: me.fixed.aim_off_axis,
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
                status_damage: me.fixed.status_damage,
                live_rate,
                beam_ramp,
                undamaged,
                t,
                bar: &me.bar,
                rec_roster: &me.fixed.rec_roster,
                rec_buff_index: &me.fixed.rec_buff_index,
                params,
            };
            let mut live = Live {
                r: &mut r,
                rec,
                d,
                target: &mut target,
                others: &mut others,
                debuffs: &mut debuffs,
                gal: &mut me.gal,
                arc: &mut me.arc,
                buff_stacks: &mut me.buff_stacks,
                windows: &mut me.windows,
                ammo: &mut me.ammo,
                incarnon: &mut me.incarnon,
                meter: &mut me.meter,
                tendril: &mut me.tendril,
                crit_per_hit: &mut me.crit_per_hit,
                sniper_combo: &mut me.sniper_combo,
                weakpoint_pile: &mut me.weakpoint_pile,
                strike_spread: &mut strike_spread,
                fields: &mut fields,
                orbs: &mut orbs,
                any_big: &mut any_big,
                any_head: &mut any_head,
                beam_merge: &mut beam_merge,
                encumber_done: &mut encumber_done,
                field_duration_boost: &mut me.field_duration_boost,
                influence_until: &mut me.influence_until,
                landed_this_shot: &mut landed_this_shot,
                super_crit_armed: &mut me.super_crit_armed,
            };
            for pellet_idx in 0..n_pellets {
                settle_pellet(pellet_idx, &shot, &mut live);
            }
        }

        // NO FORMATION, NOTHING TO REACH — see [`spread_beyond_the_target`].
        if !others.is_empty() {
            spread_beyond_the_target(
                &me.windows,
                params,
                active,
                rec,
                d,
                t,
                &strike_spread,
                &mut others,
                &mut me.gal,
                &mut me.arc,
                &mut r,
                me.tendril.count,
                me.fixed.chain_layout.as_ref(),
                struck,
            );
        }

        // …AND THE CLOUDS AND ARCS THIS SHOT LEFT reach the bodies standing in
        // them. Once per SHOT, after every instance of it has settled: a gas
        // cloud posted by the first pellet and one posted by the fourth are the
        // same instant, and the drain is where a body's outbox becomes its
        // neighbours' DoTs (`DebuffState::area_out`).
        drain_area_procs(
            &mut debuffs,
            &mut target,
            &mut others,
            params,
            &area_near,
            &mut r,
            rec,
            t,
            shot_head_landing,
            &mut d.arc_landing,
        );

        after_the_shot(
            params,
            &apl,
            flash,
            active,
            rec,
            rng,
            d,
            &mut t,
            landed_this_shot,
            any_big,
            any_head,
            pellets_before,
            headshots_before,
            swing,
            combo_now,
            combo_multiplier,
            initial_now,
            tennokai,
            tennokai_heavy,
            tennokai_kill_mark,
            &mut r,
            &mut me.bar,
            &mut me.arc,
            &mut me.enervate,
            &mut me.frenzy,
            &mut me.buff_stacks,
            &me.fixed.rec_buff_index,
            &mut target,
            &mut debuffs,
            &mut others,
            &mut me.ammo,
            &mut me.incarnon,
            &mut me.double_tap,
            &mut me.melee,
            &mut me.sniper_combo,
            &mut me.spool,
            &me.windows,
            &mut me.rs_armed,
            &mut me.opening_closed,
            &mut me.field_duration_boost,
            &mut me.last_shot_t,
        );
    }

    // THE METER'S LAST FILLS, after the trigger stops. A weapon that is out of
    // ammo still recharges, and an orb earned at t = 179 is an orb the fight
    // gets — so the clock is run out to the end and every throw it buys is
    // thrown, before the orbs are drained below.
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
            process_orbs(
                &me.windows,
                &mut orbs, &mut debuffs, &mut me.gal, &mut me.arc, at, &mut target,
                params, me.fixed.field_active, &me.field_ctx, &mut r, rec, d, &mut others,
            );
            throw_orb(o, me.seat, params, at, &mut orbs);
        }
    }
    // The orbs still in the air after the last shot — every strike they have
    // left and the detonation that ends them. Before the clouds for the same
    // reason the clouds come before the status drain: each event settles what
    // preceded it and pushes procs of its own.
    process_orbs(
        &me.windows,
        &mut orbs,
        &mut debuffs,
        &mut me.gal,
        &mut me.arc,
        params.duration_seconds,
        &mut target,
        params,
        me.fixed.field_active,
        &me.field_ctx,
        &mut r,
        rec,
        d,
        &mut others,
    );
    // The clouds still burning after the last shot, with the buff snapshot from
    // that shot (nothing refreshes it once firing stops). FIRST, because each
    // tick settles the status events before it and pushes procs of its own…
    process_field_ticks(
        &me.windows,
        &mut fields,
        &mut debuffs,
        &mut me.gal,
        &mut me.arc,
        params.duration_seconds,
        &mut target,
        params,
        me.fixed.field_active,
        &me.field_ctx,
        &mut r,
        rec,
        d,
        &mut others,
    );
    // …then drain what is left up to the end of the engagement.
    process_ticks(
        &me.windows,
        &mut debuffs,
        &mut me.gal,
        &mut me.arc,
        params.duration_seconds,
        &mut target,
        params,
        me.fixed.field_active,
        &mut r,
        rec,
        &mut d.status,
        &params.foe,
        0,
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
        params.duration_seconds, params, &mut trace, &mut next_frame, frame_seconds, &mut me.arc, &mut me.gal, &mut me.buff_stacks,
        &me.bar, &me.windows, &me.tendril, &me.crit_per_hit, &me.sniper_combo, me.fixed.combo_spec, &me.incarnon, me.influence_until,
        &target, &r, &debuffs, &others,
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
    let remaining = if target.health <= 0.0 {
        0.0
    } else {
        target.overguard + target.shield + target.health
    };
    let partial = if pool > 0.0 {
        (1.0 - remaining / pool).clamp(0.0, 1.0)
    } else {
        0.0
    };
    r.kill_progress = r.kills as f64 + partial;

    r
}
