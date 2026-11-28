// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE INCARNON CYCLE'S TRANSITIONS — the two ways in and the two ways out,
//! and what each of them costs the fight: an animation, a magazine, the piles
//! a refill takes, and the buffs a reload arms.

use super::*;
use crate::data::apl::Action;

/// HOW LONG EACH ABILITY'S WINDOW HAS LEFT — the one fact a `buff.X.remains`
/// rule reads. Zero for an ability nobody picked and for one already lapsed.
#[wfsim_gas_macro::metered]
pub(super) fn ability_remains(params: &FightParams, t: f64) -> impl Fn(&str) -> f64 + '_ {
    move |id: &str| {
        params
            .abilities_now()
            .iter()
            .filter(|a| a.id == id && a.live_at(t))
            .map(|a| a.ends_at_seconds - t)
            .fold(0.0, f64::max)
    }
}

/// WHAT THE SHOT LOOP DOES NEXT, when a transition has decided it.
pub(super) enum Flow {
    /// The shot never happens: the loop starts again at the new `t`.
    Continue,
    /// The engagement is over — a dry reserve, or the clock.
    Break,
    /// Nothing here stopped the shot; carry on with it.
    Go,
    /// The switch to the other mode has run (`t` is past it): the seat takes
    /// up the other mode's params, and the loop starts again.
    Switch,
}

/// WHERE A SEAT HOLDING TWO MODES CAN SWITCH TO, and what it costs: the form
/// kind's id, the switch already divided by `1 + bucket`, and the bucket.
#[derive(Debug, Clone, Copy)]
pub(super) struct SwitchTo {
    pub(super) form: &'static str,
    pub(super) seconds: f64,
    pub(super) bucket: f64,
}

/// THE CHARGE-MAGAZINE CYCLE: a form that ends when its magazine runs out.
///
/// Both ways across the boundary are here — the revert when the charge
/// magazine empties, and the base form's own reload — because the two share
/// the animation, the refill and the piles either of them takes.
#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn charge_magazine_cycle(
    params: &FightParams,
    apl: &crate::data::apl::Apl,
    tennokai: bool,
    rec: &mut crate::record::Record,
    rng: &mut Rng,
    next_cost: f64,
    t: &mut f64,
    r: &mut RunResult,
    ammo: &mut Ammo,
    incarnon: &mut IncarnonState,
    double_tap: &mut DoubleTap,
    windows: &mut CardWindows,
    arc: &mut ArcRuntime,
    buff_stacks: &mut [LiveStacks],
    rs_armed: &mut bool,
    opening_closed: &mut bool,
    field_duration_boost: &mut bool,
    beam: &mut BeamRamp,
    target_ramps: &mut Vec<TargetRamp>,
    // When the last shot left — what a battery's delay counts from.
    last_shot_t: f64,
    bespoke: &mut BespokeState,
    in_flight_count: usize,
    switch_to: Option<SwitchTo>,
) -> Flow {
        // WHAT THE LIST CALLS FOR AT THIS INSTANT. Asked once and matched
        // against, so the branches below are what the fight DOES and the list
        // is what decides — a rule inserted above `reload` stops the reload.
        let remains = ability_remains(params, *t);
        let mut now_state = incarnon.now(params, ammo, next_cost, *t, tennokai, &remains, switch_to.map(|s| s.form));
        now_state.beacon_needed = params.bespoke.as_ref().is_some_and(|b| b.beacon_crit_chance_bonus > 0.0)
            && *t >= bespoke.beacon_until
            && in_flight_count == 0
            && ammo.loaded >= 2.0 - 1e-9;
        now_state.beacon_sent = params.bespoke.as_ref().is_some_and(|b| b.beacon_duration_seconds.is_some())
            && (*t < bespoke.beacon_until || in_flight_count > 0 || !can_fire(ammo.loaded, next_cost));
        let want = apl.pick(&now_state);
        let active = params.cycle.as_ref().filter(|_| incarnon.in_base_form)
            .map_or(params, |cy| &cy.base_form);
        if active.uses_reserve_arrows() && want == Action::Reload {
            return if can_fire(ammo.available(params, incarnon), next_cost) { Flow::Go } else { Flow::Break };
        }
        if let Some(cy) = params.cycle.as_ref().filter(|c| c.ends == Ends::ChargeMagazine || (c.ends == Ends::SingleShot && incarnon.in_base_form)) {
            if want == Action::TransformOut {
                let suspend_at = *t;
                if params.beam_ramp.is_some_and(|s| s.reset_on_release) {
                    *beam = BeamRamp::default();
                }
                incarnon.power_shot_armed = false;
                // Charge magazine spent: revert to the base form. The swap
                // fully reloads the base magazine (wiki side effect). The
                // revert does NOT count as a transform — `transforms` counts
                // TRANSMUTES INTO the Incarnon form only (user:
                // both-directions counting read as doubled).
                // COMING OUT OF INCARNON FORM is scaled by reload speed, so it
                // takes Ready Retaliation when the buff is up — and, not being
                // a reload, neither arms nor drops it.
                let spent = rescale_reload(cy.transmute_out_seconds, cy.reload_bucket,
                    live_reload_speed(params, &cy.base_form, *rs_armed, buff_stacks, *t));
                rec.push(*t, None, crate::record::Kind::TransformStart {
                    seconds: spent,
                    into_transmuted: false,
                });
                r.downtime_seconds += spent;
                *t += spent;
                // …but it is NOT a reload, and one perk can tell the difference:
                // see `ClearedBy::Reload`.
                magazine_refilled(params, ammo, r, buff_stacks, rs_armed, opening_closed, false, None);
                incarnon.in_base_form = true;
                incarnon.frozen_beam.swap(beam, target_ramps, suspend_at, *t);
                double_tap.swap(*t);
                record_weapon(params, rec, ammo, incarnon);
                rec.push(*t, None, crate::record::Kind::TransformEnd { transmuted: false });
                incarnon.charges = 0;
                // The swap's auto-reload is the SAME mechanism as a normal one, so it draws whole rounds rather than
                // filling to capacity: a base magazine sitting on 4.25 comes
                // back on 4.25, not 5.
                //
                // ...and it draws from the SAME RESERVE, because one weapon has
                // one supply. Until 2026-08-04 every draw inside the cycle was
                // free, so a finite reserve was silently ignored on every
                // Incarnon weapon — the Infinite-ammo setting did nothing on
                // five of the seven weapons in the roster.
                incarnon.base_magazine += draw_from(&mut ammo.reserve, params.infinite_reserve,
                    reload_draw(cy.base_form.magazine_size, incarnon.base_magazine));
                // THE REST OF THE SHELLS LAND HERE. The draw above is normally
                // zero — the magazine came back full on the way IN — so this is
                // the second half of that one reload, not a second reload.
                if ammo.owed_shells > 0 {
                    bump_shells(params, buff_stacks, ammo.owed_shells, *t, rng);
                    ammo.owed_shells = 0;
                    // …and only now has a reload finished, for whatever was
                    // counting reloads instead of shells.
                    bump_on_trigger(params, buff_stacks, crate::model::BuffTrigger::ReloadComplete, *t, rng);
                }
                return Flow::Continue;
            }
            if want == Action::Reload {
                // Base-form reload. A dry finite reserve stops the gun here
                // exactly as it does outside the cycle — the weapon is out of
                // ammo, not out of one of its two forms.
                if !params.infinite_reserve && ammo.reserve < 1e-9 {
                    return Flow::Break;
                }
                // FROM EMPTY OR NOT, read before anything refills it — see the
                // plain path below, which asks the same question the same way.
                let from_empty = !can_fire(incarnon.base_magazine, 1.0);
                // THE SAME CLEAR as the plain path below: an empty magazine
                // takes the pile whichever branch notices it, and a CYCLE
                // reloads the base form here.
                let clears = if from_empty {
                    crate::model::ClearedBy::EmptyMagazine
                } else {
                    crate::model::ClearedBy::PartialReload
                };
                for (i, b) in params.stacking_buffs.iter().enumerate() {
                    if b.cleared_by == clears {
                        buff_stacks[i] = LiveStacks::seed(0, b.max_stacks, b.duration);
                    }
                }
                let rs = live_reload_speed(params, &cy.base_form, *rs_armed, buff_stacks, *t);
                let spent = live_reload_time_for_capacity(&cy.base_form, params, arc, rs, *t, from_empty, cy.base_form.magazine_size);
                schedule_reload_effects(&cy.base_form, ammo, incarnon.base_magazine, cy.base_form.magazine_size, *t, spent, from_empty);
                bespoke.perfect_reload_heat_active = false;
                if from_empty && cy.base_form.empty_reload_radial.is_some() {
                    bespoke.empty_reload_burst_at = Some(*t);
                }
                // THE OPENING WINDOW closes when the first reload STARTS, which
                // is here — everything dealt up to this instant is what the
                // magazine you walked in with was worth.
                rec.push(*t, None, crate::record::Kind::ReloadStart { seconds: spent });
                r.downtime_seconds += spent;
                *t += spent;
                magazine_refilled(params, ammo, r, buff_stacks, rs_armed, opening_closed, true, Some(from_empty));
                r.reloads += 1;
                if let Some(b) = cy.base_form.fire_rate_on_reload {
                    windows.fire_rate_after_reload = *t + b.duration;
                }
                if let Some(b) = cy.base_form.base_damage_on_reload {
                    windows.base_damage_after_reload = *t + b.duration;
                }
                // Same whole-rounds rule as the plain reload below (M14), and
                // the same shared reserve: a short draw is a short magazine.
                let loaded = draw_from(&mut ammo.reserve, params.infinite_reserve,
                    reload_draw(cy.base_form.magazine_size, incarnon.base_magazine));
                incarnon.base_magazine += loaded;
                record_weapon(params, rec, ammo, incarnon);
                rec.push(*t, None, crate::record::Kind::ReloadEnd);
                // ONE STACK PER SHELL THIS RELOAD LOADED, counted here and not
                // from a number resolved once at the panel.
                //
                // The static `stacks_per_trigger` is the OUTER form's magazine,
                // and in a cycle the outer form is the INCARNON one — so a
                // base-form reload of 6 shells was granting 60 stacks, straight
                // to +600% fire rate (measured 61 on the Felarx).
                // Counting the draw is the same rule the Incarnon route already
                // used and it needs no second number to stay true: a dry
                // reserve loads fewer shells and pays fewer stacks, with
                // nothing written down to say so.
                bump_shells(params, buff_stacks, loaded.round().max(0.0) as u32, *t, rng);
                bump_on_trigger(params, buff_stacks, crate::model::BuffTrigger::ReloadComplete, *t, rng);
                if from_empty {
                    bump_reload_from_empty(params, buff_stacks, ammo, *t, rng);
                    // Renewed Horror: "On Reload from Empty".
                    *field_duration_boost = true;
                }
                return Flow::Continue;
            }
        } else if let (Action::SwitchMode { .. }, Some(sw)) = (&want, switch_to) {
            // THE SWITCH IS ON THE RELOAD CLOCK ("Mode swapping speed is
            // affected by Reload Speed", wiki Haalvu) and is not a reload:
            // nothing keyed to one fires, and a battery refills through it as
            // through any other pause.
            let spent = rescale_reload(sw.seconds, sw.bucket, live_reload_speed(params, params, *rs_armed, buff_stacks, *t));
            r.downtime_seconds += spent;
            *t += spent.max(1e-6);
            if *t >= params.duration_seconds {
                return Flow::Break;
            }
            return Flow::Switch;
        } else if let (Action::Recharge, Some(b)) = (&want, params.battery) {
            let from = if last_shot_t.is_finite() { last_shot_t } else { *t };
            let add = params.arcane.reload_bonus
                + arc.total(&params.arcane.buffs, ArcGrant::ReloadSpeed, *t)
                + live_reload_speed(params, params, *rs_armed, buff_stacks, *t);
            let delay = b.delay_partial_seconds / crate::rules::sim::reload_divisor(1.0 + params.reload_bonus + add);
            let full = from + delay + (ammo.cap - ammo.loaded) / b.regen_per_second.max(1e-9);
            let wait = (full - *t).max(0.0);
            r.downtime_seconds += wait;
            *t += wait;
            if *t >= params.duration_seconds {
                return Flow::Break;
            }
            return Flow::Continue;
        } else if matches!(want, Action::Reload | Action::PerfectReload) {
            let perfect = want == Action::PerfectReload;
            let from_empty = !can_fire(ammo.loaded, 1.0);
            let clears = if from_empty {
                crate::model::ClearedBy::EmptyMagazine
            } else {
                crate::model::ClearedBy::PartialReload
            };
            for (i, b) in params.stacking_buffs.iter().enumerate() {
                if b.cleared_by == clears {
                    buff_stacks[i] = LiveStacks::seed(0, b.max_stacks, b.duration);
                }
            }
            if !params.infinite_reserve && ammo.reserve < 1e-9 {
                return Flow::Break;
            }
            let rs = live_reload_speed(params, params, *rs_armed, buff_stacks, *t);
            let full_spent = live_reload_time_for_capacity(params, params, arc, rs, *t, from_empty, ammo.cap);
            let spent = match params.bespoke.as_ref().and_then(|b| b.perfect_reload).filter(|_| perfect) {
                Some(pr) => full_spent * pr.window_pct,
                None => full_spent,
            };
            schedule_reload_effects(params, ammo, ammo.loaded, ammo.cap, *t, spent, from_empty);
            if from_empty && params.empty_reload_radial.is_some() {
                bespoke.empty_reload_burst_at = Some(*t);
            }
            rec.push(*t, None, crate::record::Kind::ReloadStart { seconds: spent });
            r.downtime_seconds += spent;
            *t += spent;
            bespoke.perfect_reload_heat_active = perfect && params.bespoke.as_ref().is_some_and(|b| b.perfect_reload.is_some());
            let is_magazine_reload = params.battery.is_none();
            magazine_refilled(params, ammo, r, buff_stacks, rs_armed, opening_closed, is_magazine_reload, is_magazine_reload.then_some(from_empty));
            r.reloads += 1;
            if is_magazine_reload {
                if let Some(b) = params.fire_rate_on_reload {
                    windows.fire_rate_after_reload = *t + b.duration;
                }
                if let Some(b) = params.base_damage_on_reload {
                    windows.base_damage_after_reload = *t + b.duration;
                }
            }
            let want = reload_draw(ammo.cap, ammo.loaded);
            let loaded = draw_from(&mut ammo.reserve, params.infinite_reserve, want);
            ammo.loaded += loaded;
            record_weapon(params, rec, ammo, incarnon);
            rec.push(*t, None, crate::record::Kind::ReloadEnd);
            if is_magazine_reload {
                bump_shells(params, buff_stacks, loaded.round().max(0.0) as u32, *t, rng);
                bump_on_trigger(params, buff_stacks, crate::model::BuffTrigger::ReloadComplete, *t, rng);
                if from_empty {
                    bump_reload_from_empty(params, buff_stacks, ammo, *t, rng);
                    *field_duration_boost = true;
                }
            }
            if *t >= params.duration_seconds {
                return Flow::Break;
            }
        }
    Flow::Go
}

/// THE GAUGE, AND THE WAY IN. Kills and pellets charge it; a full one
/// transmutes, which is an animation, a refill and a pile swap — the same
/// three the way out pays (`charge_magazine_cycle`).
#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn charge_the_gauge(
    params: &FightParams,
    apl: &crate::data::apl::Apl,
    tennokai: bool,
    rec: &mut crate::record::Record,
    rng: &mut Rng,
    d: &mut crate::rules::rng::Draws,
    cy: &IncarnonCycle,
    charge_on: crate::model::ChargeOn,
    base_hit: bool,
    pellets_before: u32,
    headshots_before: u32,
    t: &mut f64,
    r: &mut RunResult,
    ammo: &mut Ammo,
    incarnon: &mut IncarnonState,
    double_tap: &mut DoubleTap,
    buff_stacks: &mut [LiveStacks],
    rs_armed: &mut bool,
    opening_closed: &mut bool,
    beam: &mut BeamRamp,
    target_ramps: &mut Vec<TargetRamp>,
    bespoke: &mut BespokeState,
) {
        let fresh_kills = r.kills - incarnon.kill_mark;
        incarnon.kill_mark = r.kills;
        let overkill_pellets = std::mem::take(&mut bespoke.overkill_respawn_pellets);
        let overkill_headshots = std::mem::take(&mut bespoke.overkill_respawn_headshots);
        let contacts = (r.pellets + r.punched_contacts).saturating_sub(overkill_pellets);
        let weakpoints = (r.weakpoint_hits + r.headshots_on_others).saturating_sub(overkill_headshots);
        let weighted = contacts.saturating_sub(incarnon.contact_mark)
            + weakpoints.saturating_sub(incarnon.weakpoint_mark)
            + r.blast_gauge_hits.saturating_sub(incarnon.blast_mark);
        incarnon.contact_mark = r.pellets + r.punched_contacts;
        incarnon.weakpoint_mark = r.weakpoint_hits + r.headshots_on_others;
        incarnon.blast_mark = r.blast_gauge_hits;
        // Hits retain their firing form when they land after a return.
        if incarnon.in_base_form && base_hit {
            incarnon.charges += match charge_on {
                crate::model::ChargeOn::WeakpointHits => {
                    ((r.weakpoint_hits + r.headshots_on_others) - headshots_before)
                        .saturating_sub(overkill_headshots)
                }
                crate::model::ChargeOn::DirectHits => {
                    (r.pellets - pellets_before).saturating_sub(overkill_pellets)
                }
                crate::model::ChargeOn::Kills => fresh_kills,
                crate::model::ChargeOn::HitsAndWeakpoints => weighted,
            };
            // A FULL GAUGE ARMS THE TRANSFORM; the cadence below still
            // runs. A `continue` here would skip the completing shot's OWN
            // interval and let the next shot fire at the same instant,
            // making the transform a free extra shot. The moment is the end
            // of the shot that filled the gauge, which is the start of the
            // next one.
            //
            // The gauge also OVERSHOOTS and that is not a rounding: a
            // 7-pellet shot into a 30-charge gauge arrives at 35 on the
            // fifth shot, never at 30, so the comparison is `>=` and the
            // shot that crosses it is fired in the BASE form.
            let remains = ability_remains(params, *t);
            // The completing shot has paid its ammunition. Select the next
            // input here; the top of the loop prices the selected form and
            // decides any reload before it fires.
            let action = apl.pick(&incarnon.now(params, ammo, 0.0, *t, tennokai, &remains, None));
            if action == Action::TransformIn || (cy.ends == Ends::SingleShot && action == Action::AltFire) {
                if cy.ends == Ends::SingleShot {
                    incarnon.frozen_beam.swap(beam, target_ramps, *t, *t);
                    incarnon.in_base_form = false;
                    incarnon.charges = 0;
                    ammo.loaded = incarnon.base_magazine;
                    ammo.cap = cy.base_form.magazine_size;
                    r.transforms += 1;
                    record_weapon(params, rec, ammo, incarnon);
                    return;
                }
                let suspend_at = *t;
                if cy.base_form.beam_ramp.is_some_and(|s| s.reset_on_release) {
                    *beam = BeamRamp::default();
                }
                // BOTH DIRECTIONS TAKE IT. The wiki says Ready
                // Retaliation "can affect transition INTO Incarnon form
                // with a well-timed manual reload" and not the way back;
                // the second half is wrong, and transforming with an EMPTY
                // magazine is the proof — the animation is faster, so the
                // buff was there before any reload began.
                //
                // AND IT IS SPENT WHEN THE TRANSFORM COMPLETES, which
                // collapses the rule to one line: swapping either way fully
                // reloads the base form's magazine (wiki), so both
                // transforms are reloads and the buff is spent by whatever
                // refills the magazine.
                // WAS THE BASE MAGAZINE ACTUALLY EMPTY? Read BEFORE the
                // refill below, because that is the question the card asks:
                // "Switching to Incarnon Form from empty will also trigger
                // the buff" (wiki, Soma's Fresh Havoc). Transforming with
                // rounds still in the magazine reloads it and earns nothing,
                // which is the one place `ReloadFromEmpty` and
                // `ReloadComplete` are different events.
                let transformed_from_empty = !can_fire(incarnon.base_magazine, 1.0);
                let spent = rescale_reload(cy.transmute_seconds, cy.reload_bucket,
                    live_reload_speed(params, &cy.base_form, *rs_armed, buff_stacks, *t));
                rec.push(*t, None, crate::record::Kind::TransformStart {
                    seconds: spent,
                    into_transmuted: true,
                });
                r.downtime_seconds += spent;
                *t += spent;
                magazine_refilled(params, ammo, r, buff_stacks, rs_armed, opening_closed, true, Some(transformed_from_empty));
                if transformed_from_empty {
                    bump_on_trigger(params, buff_stacks, crate::model::BuffTrigger::ReloadFromEmpty, *t, &mut d.spine);
                }
                r.transforms += 1;
                incarnon.in_base_form = false;
                incarnon.frozen_beam.swap(beam, target_ramps, suspend_at, *t);
                incarnon.power_shot_armed = false;
                incarnon.power_shot_hit_mark = r.hits;
                double_tap.swap(*t);
                // The CHARGE magazine is filled by the gauge, not reloaded
                // from reserve — it is outside the ammo economy, takes no
                // efficiency, and so is always whole anyway.
                ammo.loaded = ammo.cap;
                // THE ROW LANDS HERE, not beside the push above: an event
                // is stamped with the weapon AS IT NOW IS, and until this
                // line the form and the magazine are still the old ones.
                // The base magazine's refill IS a reload: whole rounds off whatever is already in it,
                // and out of the same reserve as every other reload. This
                // was the site that kept the base magazine topped up for
                // free — with all three draws inside the cycle unbilled, a
                // finite reserve never moved off its starting value.
                let loaded = draw_from(&mut ammo.reserve, params.infinite_reserve,
                    reload_draw(cy.base_form.magazine_size, incarnon.base_magazine));
                incarnon.base_magazine += loaded;
                // THE ROW LANDS HERE, once BOTH magazines are what the
                // transmute made them: the charge magazine it filled and the
                // base one it silently reloaded. Announced any earlier and
                // the free reload — the whole reason both magazines are on
                // every row — is missing from the row that performed it.
                record_weapon(params, rec, ammo, incarnon);
                rec.push(*t, None, crate::record::Kind::TransformEnd { transmuted: true });
                // …AND THAT RELOAD PAYS ITS SHELLS. One as you go in, the
                // rest owed until you come out.
                //
                // Counting the shells the draw ACTUALLY loaded is what
                // makes a dry reserve behave: no shells, no stacks, and no
                // separate rule needed to say so.
                let shells = loaded.round().max(0.0) as u32;
                if shells > 0 {
                    bump_shells(params, buff_stacks, 1, *t, rng);
                    ammo.owed_shells = shells - 1;
                }
                                                       // Frenzy persists across the transform.
            }
        }
}
