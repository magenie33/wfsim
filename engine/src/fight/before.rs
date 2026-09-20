// SPDX-License-Identifier: AGPL-3.0-or-later
//! WHAT HAPPENS BEFORE A SHOT — everything the fight settles between one shot
//! resolving and the next one being fired: the replay's frames, the clock's
//! end, a battery's regen, what the kills since the last shot earned, and the
//! reload or transform the magazine now calls for.
//!
//! IT DECIDES WHETHER THERE IS A SHOT AT ALL, which is why it returns a
//! [`Flow`]: the clock can end the engagement here, and a reload or a
//! transform takes the whole turn.

use super::*;

/// Settle the fight up to this shot's trigger pull.
#[allow(clippy::too_many_arguments)]
pub(super) fn before_the_shot(
    params: &FightParams,
    rec: &mut crate::record::Record,
    rng: &mut Rng,
    trace: &mut Option<&mut Replay>,
    d: &mut crate::rules::rng::Draws,
    t_at: &mut f64,
    next_frame: &mut f64,
    frame_seconds: f64,
    arc: &mut ArcRuntime,
    gal: &mut GalStacks,
    buff_stacks: &mut [LiveStacks],
    bar: &mut BuffBar,
    windows: &mut CardWindows,
    tendril: &mut Tendrils,
    crit_per_hit: &mut CritPerHit,
    sniper_combo: &SniperComboCount,
    combo_spec: Option<crate::model::SniperCombo>,
    incarnon: &mut IncarnonState,
    influence_until: f64,
    target: &mut TargetState,
    r: &mut RunResult,
    debuffs: &mut DebuffState,
    others: &[SpreadFoe],
    ammo: &mut Ammo,
    last_shot_t: f64,
    weakpoint_pile: &mut LiveStacks,
    kill_buff_mark: &mut u32,
    syndicate: &mut Syndicate,
    ghost_pile: &mut Ghosts,
    double_tap: &mut DoubleTap,
    rs_armed: &mut bool,
    opening_closed: &mut bool,
    field_duration_boost: &mut bool,) -> Flow {
    let t = *t_at;
        if trace.is_some() {
            sample_frames_up_to(
                t, params, trace, next_frame, frame_seconds, arc, gal, buff_stacks,
                bar, windows, tendril, crit_per_hit, sniper_combo, combo_spec, incarnon,
                influence_until, target, r, debuffs, others,
            );
        }
        if t >= params.duration_seconds {
            return Flow::Break;
        }

        // The buff bar has to be settled BEFORE the reload decision, not after:
        // whether an empty magazine reloads depends on what the NEXT shot would
        // cost, and that is a live buff read. Expiry is monotone, so the second
        // `bar.expire` below (at the post-reload t) is still correct.
        // Whether the NEXT shot costs zero ammo — it decides whether an empty
        // magazine reloads, so it has to be known before that branch.
        // A BATTERY REFILLS WHILE NOBODY IS SHOOTING, counted BEFORE anything
        // asks whether this shot can be fired — otherwise an empty magazine
        // goes straight to the reload branch and the mechanic never gets a
        // turn. The gap is the one a spool reads: `t - spool_due` is what the
        // weapon spent not firing (`data::weapons::Battery`).
        //
        // THE EMPTY CASE IS NOT HERE — that is the ordinary reload, whose
        // `reload_seconds` already IS `delay_empty + magazine/rate` (1.25 s on
        // the Shedu). What this adds is the battery filling BETWEEN shots,
        // which on a weapon slowed below one shot per `delay_partial` means it
        // never empties at all.
        if let Some(b) = params.battery {
            let idle = t - last_shot_t;
            let delay = if ammo.loaded < 1e-9 { b.delay_empty_seconds } else { b.delay_partial_seconds };
            if last_shot_t.is_finite() && b.regen_per_second > 0.0 && idle > delay {
                let gained = (idle - delay) * b.regen_per_second;
                ammo.loaded = (ammo.loaded + gained).min(ammo.cap);
            }
        }

        let next_cost = {
            let ap: &FightParams = match &params.cycle {
                Some(cy) if incarnon.in_base_form => &cy.base_form,
                _ => params,
            };
            bar.expire(t);
            for lock in &params.locked_buffs {
                if lock.mode == LockMode::Permanent {
                    match lock.buff {
                        LockedBuff::Frenzy => {
                            if ap.frenzy {
                                bar.upsert(Frenzy::permanent_buff());
                            }
                        }
                    }
                }
            }
            // Does the next shot cost anything? Feeds `can_fire` below. Capped
            // at 100%, so this is "exactly free", never "more than free".
            let eff = ammo_efficiency(
                ap.ammo_efficiency_applies,
                bar.total_contributions().ammo_efficiency
                    + weakpoint_ammo(params.weakpoint_stacks, weakpoint_pile, t),
                params.arcane.ammo_efficiency,
                arc.total(&params.arcane.buffs, ArcGrant::AmmoEfficiency, t),
                crate::data::abilities::ammo_efficiency_at(&params.abilities, t),
            );
            // `ap` already picks the form whose magazine is about to be
            // checked, so this is THAT form's cost.
            if eff >= 1.0 - 1e-9 {
                0.0
            } else {
                ap.ammo_cost * (1.0 - eff)
            }
        };

        // A KILL IS A KILL WHEREVER IT CAME FROM, so on-kill stacks are read
        // off the counter rather than bumped at each of the six places one can
        // happen — a direct hit, a DoT tick, a field tick and three more. The
        // same mark-and-diff Sentient Surge's refill and the tendrils use, two
        // blocks down, and for the same reason: a list of call sites is a list
        // to forget one from.
        if r.kills != *kill_buff_mark {
            let fresh = r.kills - *kill_buff_mark;
            *kill_buff_mark = r.kills;
            for _ in 0..fresh {
                bump_on_trigger(params, buff_stacks, crate::model::BuffTrigger::Kill, t, &mut d.extra);
                // EXACT PENANCE, on the same counter and for the same reason:
                // "Kills from status effects can also trigger the effect", and
                // a DoT kill happens nowhere near the direct-hit site that
                // `instant_reload_on_headshot` is wired to.
                if let Some(chance) = params.instant_reload_on_kill {
                    if d.extra.chance(chance) {
                        ammo.instant_reload_now = true;
                    }
                }
                // PYRANA PRIME'S STREAK, off the same counter and held on the bar:
                // each kill restarts the clock `bar.expire` already runs, so a
                // lapse drops the whole streak. A kill while the second gun is
                // up starts nothing — "will not refresh the duration".
                if let Some(s) = params.kill_streak_summon {
                    use crate::model::KillStreakSummonSpec as K;
                    if bar.get(K::BUFF_ID).is_none() {
                        let streak = bar.get(K::STREAK_BUFF_ID).map_or(0, |b| b.stacks) + 1;
                        if streak >= s.kills {
                            bar.remove(K::STREAK_BUFF_ID);
                            bar.upsert(summoned_gun(s, t));
                        } else {
                            bar.upsert(kill_streak(s, streak, t));
                        }
                    }
                }
            }
        }
        ammo.resize_for_summon(params, bar);

        // TENDRILS and the magazine they keep alive, both re-derived from the
        // counters above before anything decides to reload.
        ammo.refill_on_kill(params, r);
        tendril.settle(params, r);

        // …AND THE SAME MARK-AND-DIFF FOR WHAT IS STANDING. The kills were
        // counted where they happened; this is where they become a duration.
        // The DURATION is the weapon's, so it is read off whichever form
        // carries it — the kills were already filtered by the form that fired.
        let spawner = params
            .spawn_on_kill
            .or_else(|| params.cycle.as_ref().and_then(|c| c.base_form.spawn_on_kill));
        if let Some(g) = spawner {
            ghost_pile.settle(g, t, r);
        }

        // HATA-SATYA's pile, the same mark-and-diff one block up with the
        // counter swapped: hits instead of kills. `r.pellets` is the count of
        // DIRECT pellet hits, which is exactly the qualifying set — "Additional
        // hits from Multishot and Punch Through also count towards the bonus",
        // and both land here.
        //
        // Read at the START of the shot, so the hit that earns a stack does not
        // carry it. That is the rule every other trigger in this loop follows.
        if params.crit_chance_per_hit.is_some() {
            crit_per_hit.settle(params, r, ammo);
        }

        // THE SYNDICATE GAUGE. Affinity the WEAPON earned, which is half of
        // each kill's — "Kill with weapons: Half Affinity goes to the Warframe
        // and half to the killing weapon" (wiki Affinity).
        if let Some(sy) = params.syndicate_radial {
            let fresh = r.kills - syndicate.kill_mark;
            syndicate.kill_mark = r.kills;
            if fresh > 0 && t >= syndicate.ready_at {
                // Per kill: base affinity x the level multiplier, FLOORED to a
                // whole number ("the base affinity multiplied by the Affinity
                // Multiplier value is also rounded down"), then halved.
                let per_kill = (params.target.base_affinity
                    * scaling::affinity_multiplier(params.target.level, params.target.eximus))
                .floor()
                    * WEAPON_AFFINITY_SHARE;
                syndicate.points += f64::from(fresh) * per_kill;
            }
            if syndicate.points >= sy.affinity_to_fill && t >= syndicate.ready_at {
                // Fires, then BOTH rules: points to zero and no conversion at
                // all until the cooldown is out.
                syndicate.points = 0.0;
                syndicate.ready_at = t + sy.cooldown_seconds;
                fire_syndicate_radial(
                    &sy,
                    r,
                    rec,
                    target,
                    debuffs,
                    gal,
                    arc,
                    params,
                    &mut d.status,
                    t,
                );
            }
        }

        // Phase transitions and reloads.
        // A WEAPON WITH NO MAGAZINE NEVER EMPTIES, so it never reaches the
        // reload below. Topping it up here rather than making `can_fire` lie is
        // what keeps the whole reload path — the event, the animation, the
        // buffs, the windows, the ammo draw — from happening at all, which is
        // the only way to be sure none of it fires: there is no list of
        // reload-keyed effects to keep in step, because the reload does not
        // occur.
        //  rather than the active form: having a clip is a property of
        // the WEAPON, not of which of its forms is being fired, and this is
        // read before the form is decided anyway.
        if params.no_magazine {
            ammo.loaded = ammo.cap;
        }
        // …AND A CLOCK ENDS THE OTHER KIND, before the magazine block below,
        // which is entirely about a magazine a melee weapon does not have.
        // *"activate Incarnon Form for 90 seconds"* — the window runs from the
        // swing that armed it, is not refreshed by anything, and re-arms the
        // same way it armed the first time.
        if let Some(cy) = &params.cycle {
            if let Ends::After(_) = cy.ends {
                if !incarnon.in_base_form && t >= incarnon.incarnon_until {
                    incarnon.in_base_form = true;
                    record_weapon(params, rec, ammo, incarnon);
                }
            }
        }
        charge_magazine_cycle(
            params, rec, rng, next_cost, t_at, r, ammo, incarnon, double_tap,
            windows, arc, buff_stacks, rs_armed, opening_closed,
            field_duration_boost,
        )
}
