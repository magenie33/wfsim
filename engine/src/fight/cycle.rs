// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE INCARNON CYCLE'S TRANSITIONS — the two ways in and the two ways out,
//! and what each of them costs the fight: an animation, a magazine, the piles
//! a refill takes, and the buffs a reload arms.

use super::*;
use crate::data::apl::Action;

/// HOW LONG EACH ABILITY'S WINDOW HAS LEFT — the one fact a `buff.X.remains`
/// rule reads. Zero for an ability nobody picked and for one already lapsed.
pub(super) fn ability_remains(params: &FightParams, t: f64) -> impl Fn(&str) -> f64 + '_ {
    move |id: &str| {
        params
            .abilities
            .iter()
            .find(|a| a.id == id)
            .map_or(0.0, |a| (a.ends_at_seconds - t).max(0.0))
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
}

/// THE CHARGE-MAGAZINE CYCLE: a form that ends when its magazine runs out.
///
/// Both ways across the boundary are here — the revert when the charge
/// magazine empties, and the base form's own reload — because the two share
/// the animation, the refill and the piles either of them takes.
#[allow(clippy::too_many_arguments)]
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
) -> Flow {
        // WHAT THE LIST CALLS FOR AT THIS INSTANT. Asked once and matched
        // against, so the branches below are what the fight DOES and the list
        // is what decides — a rule inserted above `reload` stops the reload.
        let remains = ability_remains(params, *t);
        let want = apl.pick(&incarnon.now(params, ammo, next_cost, *t, tennokai, &remains));
        if let Some(cy) = params.cycle.as_ref().filter(|c| c.ends == Ends::ChargeMagazine) {
            if want == Action::TransformOut {
                // Charge magazine spent: revert to the base form. The swap
                // fully reloads the base magazine (wiki side effect). The
                // revert does NOT count as a transform — `transforms` counts
                // TRANSMUTES INTO the Incarnon form only (user:
                // both-directions counting read as doubled).
                // COMING OUT OF INCARNON FORM IS A RELOAD too, and for the
                // same stated reason: the swap refills the base magazine. It
                // takes the speed if the buff is up and spends it — which is
                // also why this animation is scaled by reload speed at all.
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
                magazine_refilled(params, ammo, r, buff_stacks, rs_armed, opening_closed, false);
                incarnon.in_base_form = true;
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
                let spent = live_reload_time(&cy.base_form, params, arc, rs, *t, from_empty);
                if cy.base_form.reload_grenade.is_some() {
                    ammo.grenade_thrown_at = Some((*t, from_empty));
                }
                // THE OPENING WINDOW closes when the first reload STARTS, which
                // is here — everything dealt up to this instant is what the
                // magazine you walked in with was worth.
                rec.push(*t, None, crate::record::Kind::ReloadStart { seconds: spent });
                r.downtime_seconds += spent;
                *t += spent;
                magazine_refilled(params, ammo, r, buff_stacks, rs_armed, opening_closed, true);
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
        } else if want == Action::Reload {
            // FROM EMPTY OR NOT — the one question every "reload from empty"
            // card asks, read before the refill. The default list reloads only
            // when it cannot fire, so that list's reloads are all from empty;
            // a rule inserted above it (`reload,if=magazine.pct<=N`) is not,
            // and a card that pays on empty pays nothing for it. Fewer than one
            // whole round is empty, the Incarnon transmute's own reading.
            let from_empty = !can_fire(ammo.loaded, 1.0);
            // AN EMPTY MAGAZINE TAKES THE WHOLE PILE, before the reload that
            // rebuilds it. Mounting Momentum is cleared the instant the count
            // reaches zero — not by the reload, and not by a clock — so firing
            // a magazine dry earns one magazine's worth and never more, and a
            // magazine topped up before it empties keeps its pile.
            // …AND A PARTIAL RELOAD TAKES THE OTHER KIND (Mauler's Magazine).
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
            // Cannot fire: reload (blocking) or, with dry finite reserves,
            // stop firing altogether (DoTs still drain below).
            if !params.infinite_reserve && ammo.reserve < 1e-9 {
                return Flow::Break;
            }
            // THE WINDOW OPENS WHEN THE RELOAD BEGINS — the player's reload
            // ACTION is the trigger, not its completion.
            // So it is armed BEFORE the line below, and the reload that armed
            // it is the first thing it speeds up.
            let rs = live_reload_speed(params, params, *rs_armed, buff_stacks, *t);
            let spent = live_reload_time(params, params, arc, rs, *t, from_empty);
            // THE THROW IS THE RELOAD'S, and it leaves when the reload STARTS:
            // the page says it is thrown mid-reload and states no timing, so the
            // earliest instant is the one that invents no delay.
            if params.reload_grenade.is_some() {
                ammo.grenade_thrown_at = Some((*t, from_empty));
            }
            // TWO ROWS, THE START AND THE END, and nothing in between. What is between them is not a reload event — it is
            // whatever the fight went on doing while the weapon was down, which
            // for a status build is most of its damage.
            rec.push(*t, None, crate::record::Kind::ReloadStart { seconds: spent });
            r.downtime_seconds += spent;
            *t += spent;
            magazine_refilled(params, ammo, r, buff_stacks, rs_armed, opening_closed, true);
            r.reloads += 1;
            if let Some(b) = params.fire_rate_on_reload {
                windows.fire_rate_after_reload = *t + b.duration;
            }
            if let Some(b) = params.base_damage_on_reload {
                windows.base_damage_after_reload = *t + b.duration;
            }
            // Whole rounds only, and `+=` not `=` — both measured (M14). The
            // draw covers the overdraw debt for free: the counter is in (−1, 0]
            // here, so `floor(capacity − current)` is a full magazine, and a
            // −0.75 counter comes back at 4.25 rather than 5.00.
            let want = reload_draw(ammo.cap, ammo.loaded);
            let loaded = draw_from(&mut ammo.reserve, params.infinite_reserve, want);
            ammo.loaded += loaded;
            // THE ROW LANDS HERE, after the rounds are actually in. Announced
            // one line earlier it read `0 / 6` — a reload that had just
            // finished reporting an empty magazine, which is the one thing that
            // row exists to deny (found by reading a Felarx record).
            record_weapon(params, rec, ammo, incarnon);
            rec.push(*t, None, crate::record::Kind::ReloadEnd);
            // …AND THE SHELLS IT LOADED PAY THEIR STACKS. One per shell, from
            // the count the draw actually produced — see the note at the cycle's
            // base-form reload for why this is per-site rather than a single
            // trigger at the top of the loop.
            bump_shells(params, buff_stacks, loaded.round().max(0.0) as u32, *t, rng);
            bump_on_trigger(params, buff_stacks, crate::model::BuffTrigger::ReloadComplete, *t, rng);
            if from_empty {
                bump_reload_from_empty(params, buff_stacks, ammo, *t, rng);
                *field_duration_boost = true; // Renewed Horror
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
pub(super) fn charge_the_gauge(
    params: &FightParams,
    apl: &crate::data::apl::Apl,
    tennokai: bool,
    rec: &mut crate::record::Record,
    rng: &mut Rng,
    d: &mut crate::rules::rng::Draws,
    cy: &IncarnonCycle,
    charge_on: crate::model::ChargeOn,
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
) {
        // KILLS ADVANCE THEIR MARK IN EITHER FORM, and the two halves are
        // separate on purpose. A hit is bounded by the shot that caused it,
        // so `_before` is exact for one; a kill is not — a status tick
        // between two shots kills, and a delta taken at the top of the next
        // shot has already missed it. And the mark advances even while
        // TRANSFORMED, so a kill made with the earned form can never pay
        // for the next one: it is kills with the PRIMARY fire that recharge
        // the Mausolon's laser (wiki), not kills of any kind.
        let fresh_kills = r.kills - incarnon.kill_mark;
        incarnon.kill_mark = r.kills;
        if incarnon.in_base_form {
            // Per PELLET, and per the WEAPON's rule: weak-point hits for
            // the Zariman pistols, any direct hit for the Torid, kills for
            // the Mausolon. A field or radial instance is neither of the
            // first two, so neither can charge those — but it CAN kill,
            // which is the whole difference the third one makes.
            incarnon.charges += match charge_on {
                // EVERY WEAK POINT THE SHOT LANDED, not only the aimed one:
                // a round punching two heads charges twice (M103). The aimed
                // one is counted as a WEAK POINT rather than as a head — see
                // `RunResult::weakpoint_hits`.
                crate::model::ChargeOn::WeakpointHits => {
                    (r.weakpoint_hits + r.headshots_on_others) - headshots_before
                }
                crate::model::ChargeOn::DirectHits => r.pellets - pellets_before,
                crate::model::ChargeOn::Kills => fresh_kills,
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
            // NO `next_cost` TO ASK ABOUT HERE — the shot is over and the next
            // one has not been priced. A full magazine is the honest stand-in:
            // the only rule above `transform_in` a reader can write is a cast,
            // and `reload` sits below it and is asked again at the top of the
            // loop with the real cost.
            if apl.pick(&incarnon.now(params, ammo, 0.0, *t, tennokai, &remains)) == Action::TransformIn {
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
                magazine_refilled(params, ammo, r, buff_stacks, rs_armed, opening_closed, true);
                if transformed_from_empty {
                    bump_on_trigger(params, buff_stacks, crate::model::BuffTrigger::ReloadFromEmpty, *t, &mut d.spine);
                }
                r.transforms += 1;
                incarnon.in_base_form = false;
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
