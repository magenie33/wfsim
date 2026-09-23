// SPDX-License-Identifier: AGPL-3.0-or-later
//! WHAT HAPPENS AFTER A SHOT — everything the trigger pull owes once its last
//! pellet has settled: the counters a whole pull moves, the one `Hit` event
//! the shot is, what the swing did to the combo counter, and when the next
//! shot comes due.
//!
//! ONCE PER PULL, NOT PER PELLET, which is what each of these mechanics says
//! of itself — a card that applies "once per enemy hit" is tenfold on a
//! shotgun if it is rolled a pellet at a time.

use super::*;

/// Settle the shot that has just landed, and set the clock to the next one.
#[allow(clippy::too_many_arguments)]
pub(super) fn after_the_shot(
    params: &FightParams,
    apl: &crate::data::apl::Apl,
    flash: bool,
    active: &FightParams,
    rec: &mut crate::record::Record,
    rng: &mut Rng,
    d: &mut crate::rules::rng::Draws,
    t: &mut f64,
    landed_this_shot: bool,
    any_big: bool,
    any_head: bool,
    pellets_before: u32,
    headshots_before: u32,
    swing: Option<crate::model::ComboHit>,
    combo_now: f64,
    combo_multiplier: f64,
    initial_now: f64,
    tennokai: bool,
    tennokai_heavy: bool,
    tennokai_kill_mark: u32,
    r: &mut RunResult,
    bar: &mut BuffBar,
    arc: &mut ArcRuntime,
    enervate: &mut Option<SecondaryEnervate>,
    frenzy: &mut Frenzy,
    buff_stacks: &mut [LiveStacks],
    rec_buff_index: &[Option<usize>],
    target: &mut TargetState,
    debuffs: &mut DebuffState,
    others: &mut [SpreadFoe],
    ammo: &mut Ammo,
    incarnon: &mut IncarnonState,
    double_tap: &mut DoubleTap,
    melee: &mut MeleeState,
    sniper_combo: &mut SniperComboCount,
    spool: &mut Spool,
    windows: &CardWindows,
    rs_armed: &mut bool,
    opening_closed: &mut bool,
    field_duration_boost: &mut bool,
    last_shot_t: &mut f64,) {
        // A SHOT THAT HIT NOTHING DROPS THE SHOT COMBO COUNTER.
        //
        // A counter that decays only through its own timer runs slightly
        // generous, and is the right model only in an arena where nothing can
        // miss. This one can, so the other half of the mechanic is here.
        //
        // PER SHOT, not per pellet: the counter counts trigger pulls that
        // connected, and a multishot pull that lands one pellet connected.
        // Nothing happens on a weapon with no combo, and nothing happens at
        // point blank — `landed_this_shot` cannot be false there.
        if active.sniper_combo.is_some() && !landed_this_shot {
            sniper_combo.count = 0;
        }

        // THE SPEAR PLANTS ITS FIELD, and the shot that planted it does not
        // benefit from it — hence here, after every pellet of this throw has
        // landed. The next one is 1.6 s away against a 4.7 s field, so from
        // the second throw on it is simply up; only the opening throw is
        // affected by the ordering, and this is the ordering that claims the
        // least (the wiki has the field pulse ON impact, not before it).
        //
        // ONE FIELD, replaced rather than stacked, which is already what
        // `push_capped(.., 1, ..)` does — and it is the right shape for a
        // second reason the owner measured: a new throw destroys
        // the OLD FIELD at the moment it starts, but what that field already
        // applied to an enemy runs its own clock. A field the sim never
        // re-pulses and a debuff that survives its field are the same thing
        // from the target's side, which is the only side this arena has.
        if let Some(secs) = active.attractor_seconds {
            DebuffState::push_capped(&mut debuffs.attractor, *t + secs, 1, *t);
        }

        // REAVER'S RAPTURE: THE ROUND THAT COMPLETED A BURST, counted here —
        // after every pellet of it has landed, so the burst that earns the
        // stack does not carry it. The next burst does.
        //
        // "Not affected by multishot or punch through" is why this is outside
        // the pellet loop; "counts object hits" and "activates even if the
        // first hit of a burst kills the target" are both already true of this
        // arena, where one target respawns and every round reaches it. So a
        // completed burst IS a full burst hit, with nothing left to condition
        // on.
        //
        // A weapon with no burst has a count of one, and then every round
        // completes its own burst — which is what the trigger means there.
        let burst_len = active.burst.map_or(1, |b| b.count.max(1));
        if ammo.rounds_this_mag.is_multiple_of(burst_len) {
            bump_buffs!(params, buff_stacks, rec_buff_index, rec, crate::model::BuffTrigger::FullBurst, *t, rng);
        }

        // EXECUTIONER'S FORTUNE, SPENT. The roll is per pellet, the effect is
        // not: a magazine fills once however many pellets rolled it, so this is
        // a flag the pellet loop sets and the shot consumes.
        //
        // It is an INSTANT reload, so no time passes — which is the whole perk,
        // and why it is not in the reload bucket. It draws from the reserve
        // like every other refill here (a dry reserve gives nothing), and it
        // fills whole rounds to capacity the way `reload_draw` defines a
        // reload, so an overdrawn counter comes back where a real reload would
        // leave it.
        //
        // A REFILL IS NOT A RELOAD, the rule Sentient Surge established above:
        // `r.reloads` is untouched, so nothing keyed on reloads — Mounting
        // Momentum's shells, Deadly Efficiency's window, Ready Retaliation's —
        // is triggered by it. That is a reading, not a measurement: DE's own
        // text calls it a reload, and if it turns out to arm those buffs this
        // is the one line to change.
        //
        // The form was already checked at the roll — an Incarnon form never
        // sets this flag — so this only has to fill the right counter.
        if ammo.instant_reload_now {
            ammo.instant_reload(params, incarnon);
        }

        // Renewed Horror is spent by the shot that follows the reload, however
        // many grenades that shot put out.
        *field_duration_boost = false;

        // GALVANIC RELOAD: "On hitting a target affected by an Electricity
        // status, 40% chance to restore 1 round in the magazine from ammo pool."
        //
        // ONCE PER SHOT, which is the card's own qualifier — "The bonus can only
        // apply once per enemy hit" — and on a shotgun family the difference
        // between that and once per pellet is tenfold. So it is rolled HERE,
        // outside the pellet loop, beside the other per-pull events.
        //
        // "FROM AMMO POOL", so a dry reserve restores nothing: the round is
        // drawn like any other. And a restore is NOT a reload — nothing that
        // watches reloads sees it, the same rule `magazine_refill_on_kill` follows.
        if let Some((st, chance, rounds)) = active.round_restore_on_status {
            if has_status(debuffs, st) && d.extra.chance(chance) {
                let room = (ammo.cap - ammo.loaded).max(0.0);
                let want = rounds.min(room);
                if want > 0.0 {
                    ammo.loaded += draw_from(&mut ammo.reserve, params.infinite_reserve, want);
                }
            }
        }

        // ONE Hit event per trigger pull (hitscan pellets are not separate
        // Hits - GLOSSARY): headshot/big-crit flags aggregate any pellet.
        let hit = Event::Hit(Hit {
            big_crit: any_big,
            headshot: any_head,
            target_alive: true,
        });
        // A BLAST THAT WENT OFF SINCE THE LAST SHOT IS A HIT TOO — one per
        // MOMENT, however many stacks shared it, which is what the pile records.
        //
        // A fuse paying out is a damage number at a moment, and that is what an
        // arcane counting hits sees: nine stacks expiring one at a time are
        // nine, ten going off together are one. AND BLAST NEVER CRITS, so a pop
        // only ever BUILDS the ramp — it can never fill the big-crit counter
        // that resets it, which is why `big_crit` is false here and not read
        // from anything.
        //
        // Fed at the pop's own time and before this shot's, because both are
        // true: the arcane is rate-limited, and the ramp this shot fires under
        // is the one the pops left behind.
        for pop in arc.blast_pops.drain(..) {
            if let Some(en) = enervate.as_mut() {
                en.on_event(
                    &Event::Hit(Hit { big_crit: false, headshot: false, target_alive: true }),
                    pop,
                    bar,
                );
            }
        }
        if let Some(en) = enervate.as_mut() {
            en.on_event(&hit, *t, bar);
        }
        if active.frenzy {
            frenzy.on_event(&hit, *t, bar);
        }

        // Gauge charging (base phase): every weakpoint PELLET builds one
        // charge (charge_rules); a full gauge transmutes back immediately.
        //
        // A GAUGE IS ONE OF TWO WAYS IN, so the whole block is the GUN's. A
        // melee Incarnon arms on the swing itself (`Arms::HeavyAtCombo`, below
        // beside the combo counter it reads) and skips every line of this: it
        // has no gauge to fill, no charge magazine to fill, and no transmute
        // animation to spend.
        // HOW MANY CHARGES IT TAKES IS THE GAUGE'S OWN BUSINESS NOW: the list
        // asks `gauge.pct`, and `IncarnonState::now` reads the count off the
        // form entry. What this still decides is WHETHER there is a gauge —
        // a melee Incarnon arms on the swing and skips every line of it.
        if let Some((cy, charge_on)) =
            params.cycle.as_ref().and_then(|cy| match cy.arms {
                Arms::Gauge { charge_on, .. } => Some((cy, charge_on)),
                Arms::HeavyAtCombo(_) => None,
            })
        {
            charge_the_gauge(
                params, apl, flash, rec, rng, d, cy, charge_on, pellets_before, headshots_before,
                t, r, ammo, incarnon, double_tap, buff_stacks,
                rs_armed, opening_closed,
            );
        }

        // ---- WHAT THIS SWING DID TO THE COMBO COUNTER -------------------
        //
        // GAIN FIRST, THEN SPEND, and the order is the game's: a heavy attack
        // pays the multiplier that was standing when it went down (read above,
        // into `combo_multiplier`), lands, and then empties the counter.
        //
        // POINTS ARE THE STANCE MULTIPLIER. *"Stance attacks add combo points,
        // scaling with the attack's stance damage multiplier (100% stance
        // damage multiplier = 1 point)"* — one number doing two jobs, and it is
        // the same number in game. PER BODY LANDED, which is the wiki's own
        // reading of the Rauta: *"generates 2 combo points per pellet landing
        // on enemy (max 28 points across 14 pellets)"*.
        //
        // ONLY A LANDED SWING COUNTS: *"Only successful strikes against enemies
        // award points"*, so a miss neither adds nor refreshes.
        if let Some(h) = &swing {
            after_swing(
                h, active, params, rec, d, combo_now, combo_multiplier, tennokai, tennokai_kill_mark,
                tennokai_heavy, pellets_before, *t,
                r, melee, incarnon, ammo, arc, target, debuffs,
                others,
            );
        }

        // Next shot: cadence reflects the bar as of now (Frenzy just
        // granted/refreshed counts immediately), plus Pressurized
        // Magazine's live on-reload fire-rate buff.
        bar.expire(*t);
        let mut fr_add = match active.fire_rate_on_reload {
            Some(b) if *t < windows.fire_rate_after_reload => b.value,
            _ => 0.0,
        };
        // THE SAME BUCKET fire-rate mods and a static `fire_rate_bonus`
        // evolution live in — `base * (1 + fr + evo + 0.05n)`. `per_stack` is
        // already the absolute rate that fraction is worth, so adding it here,
        // inside the bracket rather than outside it, is what keeps it additive
        // with mods instead of multiplicative with them.
        fr_add += buff_total(active, crate::model::BuffGrant::FireRate, buff_stacks, *t);
        // …AND A WARFRAME ABILITY'S SHARE, into the same sum: Warcry's attack
        // speed is "additive to mods (e.g., Fury)" and its own strength knob has
        // already been spent on it (`data::abilities::resolve`).
        //
        // AN AUGMENT GROWS THE WINDOW FIRST — the melee kills since this was last
        // paid, at the ability's own seconds each, capped by its own ceiling. The
        // ONE place in this fight where an ability's window is not what it was at
        // the start, which is why `resolve` refuses a growing window on any
        // effect the other readers would have to know about.
        // A CAST THAT ROOTS THE FRAME IS A PAUSE IN THE SHOOTING, and it is the
        // honest half of what an ability costs: the energy buys the window and
        // this buys nothing at all. Planned before the run and in time order, so
        // the run only remembers how far down the list it is.
        while spool
            .casts_paid
            .lt(&params.cast_interrupts.len())
            .then(|| params.cast_interrupts[spool.casts_paid])
            .is_some_and(|(at, _)| at <= *t)
        {
            *t += params.cast_interrupts[spool.casts_paid].1;
            spool.casts_paid += 1;
        }
        let grows = params
            .abilities
            .iter()
            .find(|a| a.extend_per_melee_kill_seconds > 0.0);
        if let Some(a) = grows {
            let fresh = r.kills - melee.ability_kill_mark;
            melee.ability_kill_mark = r.kills;
            let cap = (a.extend_cap_seconds - a.ends_at_seconds).max(0.0);
            melee.ability_extra_seconds =
                (melee.ability_extra_seconds + f64::from(fresh) * a.extend_per_melee_kill_seconds)
                    .min(cap);
        }
        fr_add += crate::data::abilities::fire_rate_at(
            &params.abilities, *t, melee.ability_extra_seconds);
        let rate = if params.locks("fire_rate") {
            active.fire_rate
        } else {
            (active.fire_rate + fr_add) * bar.total_contributions().fire_rate_multiplier
        };
        // THE TRIGGER CAME OFF, DERIVED rather than listed. Every pause in
        // this loop — a reload, a transform, a dry magazine, a stall on a dry
        // reserve — leaves this shot LATER than the moment the last one made it
        // due, and that is precisely what releasing the trigger is. Asking the
        // clock here, rather than clearing the count in each branch that
        // pauses, is what stops the next pause anyone adds from silently
        // keeping the spool alive — and the reload branch already proves the
        // point: it does not `continue`, it falls through and fires in the same
        // iteration, so a check at the top of the loop would never have seen
        // it (the test caught this: 66 shots against the 80 a released trigger
        // owes).
        if *t > spool.due + 1e-9 {
            spool.shots = 0.0;
        }
        *last_shot_t = *t;
        // …and then the SPOOL, which is a fraction of whatever that rate came
        // to: a fire-rate mod raises the ceiling and the floor together, so the
        // Phenmor's Incarnon form still spends most of its 408-round magazine
        // at 60% of whatever it was built to.
        let rate = rate * spool_factor(active.sustained_fire_rate, spool.shots);
        spool.shots += 1.0;
        // On a CHARGE weapon the pull costs a draw, not a rate: divide the
        // modded charge time by whatever the live buffs did to the rate
        // (`rate / active.fire_rate` is exactly that factor, and it is 1.0 when no
        // buff is up). Same bucket, reciprocal application — see `charge_seconds`.
        *t += seconds_to_next_shot(
            active, rate, initial_now, swing, tennokai, tennokai_heavy, ammo, incarnon, melee,
        );
        spool.due = *t;
}
