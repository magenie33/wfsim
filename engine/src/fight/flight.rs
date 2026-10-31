// SPDX-License-Identifier: AGPL-3.0-or-later
//! A SHOT IN THE AIR — what a trigger pull resolved, held until it arrives.
//!
//! The pull and the hit are two moments. The pull spends the round, rolls the
//! shot's numbers and sets the clock to the next pull; the HIT is everything
//! that keys off contact — damage, procs, on-hit and on-kill buffs, the spread
//! to the bodies around it. Immediate contact lands inside its own turn;
//! projectile flight and timed melee animation contacts become a [`Landing`]
//! the scheduler settles at its own time (docs/MECHANICS.md §"Projectile
//! flight").

use super::*;

/// EVERYTHING THE HIT NEEDS that the pull decided, owned so it can outlive the
/// turn that fired it. The rest — the build, the bodies, the live buffs — is
/// read when it lands, which is when the game reads it.
pub(super) struct Landing {
    /// WHEN IT ARRIVES.
    pub(super) t: f64,
    /// THE FORM THAT FIRED IT. A transform while it flies does not turn a base
    /// form round into an Incarnon one.
    pub(super) base_form: bool,
    /// The record's id for the pull, so what it deals points back at it.
    pub(super) shot: Option<u32>,
    pub(super) flash: bool,
    pub(super) qvec: DamageVector,
    /// Raw composition travels with the projectile alongside its damage snap.
    pub(super) direct_pre_snap: DamageVector,
    pub(super) modded_base: f64,
    pub(super) co_base: crate::model::CoBase,
    pub(super) combo_now: f64,
    pub(super) combo_multiplier: f64,
    pub(super) swing: Option<crate::model::ComboHit>,
    pub(super) swing_mult: f64,
    pub(super) swing_forced_types: Vec<DamageType>,
    pub(super) swing_forced_independent: Vec<&'static str>,
    pub(super) tennokai: bool,
    pub(super) tennokai_heavy: bool,
    pub(super) tennokai_kill_mark: u32,
    /// Is this throw detonated where it arrives — see `MeleeState::detonating`.
    pub(super) detonates: bool,
    pub(super) recalls: bool,
    pub(super) melee_struck: Vec<usize>,
    /// THE PUNCH THROUGH A LIVE GRANT GAVE THE ROUND AS IT LEFT THE MUZZLE
    /// (Ripper Rounds' kill window): a property of the projectile, so a kill
    /// that lands while it flies does not lengthen it.
    pub(super) pierce_live_m: f64,
    pub(super) effective_cc: f64,
    pub(super) crit_chance_relative: f64,
    pub(super) flat_crit: f64,
    pub(super) cc_mult: f64,
    pub(super) sc_mult: f64,
    pub(super) sc_arc_shot: f64,
    pub(super) weakpoint_cd: f64,
    pub(super) weakpoint_sc: f64,
    pub(super) bd_reload_add: f64,
    pub(super) bd_eximus_add: f64,
    pub(super) dt_mult: f64,
    pub(super) ms_damage: f64,
    pub(super) live_rate: f64,
    pub(super) beam_ramp: f64,
    pub(super) next_shot_bonus: f64,
    pub(super) undamaged: bool,
    pub(super) n_pellets: u32,
    pub(super) beam_merge: f64,
    pub(super) blob_stacks: u32,
    /// Which of the `n_pellets` instances ride the round before them.
    pub(super) rounds: super::resolve::Rounds,
    pub(super) shot_head_landing: f64,
    /// The run's counters as the hit begins — the marks the gauge and the
    /// next-shot bonus measure THIS shot's weak points against.
    pub(super) pellets_before: u32,
    pub(super) headshots_before: u32,
    /// See `after_the_landing`.
    pub(super) burst_completed: Option<bool>,
    /// THE FIRST ROUND OF ITS BURST (every round of a weapon that has none).
    /// A burst is one round as far as a kill goes (M119: a Burston's three
    /// take three rounds of ammunition and do not reach the body behind the one
    /// the first killed), so `Body::fallen` lasts the burst.
    pub(super) burst_starts: bool,
}

#[wfsim_gas_macro::metered]
impl Landing {
    /// The panel of the form that fired it — `run_once_traced`'s own rule.
    pub(super) fn active<'p>(&self, params: &'p FightParams) -> &'p FightParams {
        match &params.cycle {
            Some(cy) if self.base_form => &cy.base_form,
            _ => params,
        }
    }
}

/// THE EARLIEST SHOT STILL IN THE AIR that arrives inside the engagement and no
/// later than `by` — a seat's next turn, which a landing at the same instant
/// precedes. Ties go to the lower seat, then to the earlier pull.
#[wfsim_gas_macro::metered]
pub(super) fn next_landing(seats: &[Combatant], by: Option<f64>, end: f64) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize, f64)> = None;
    for (si, c) in seats.iter().enumerate() {
        for (k, l) in c.in_flight.iter().enumerate() {
            if l.t >= end || by.is_some_and(|b| l.t > b) {
                continue;
            }
            if best.is_none_or(|(_, _, t)| l.t < t) {
                best = Some((si, k, l.t));
            }
        }
    }
    best.map(|(si, k, _)| (si, k))
}

/// THE WORLD A HIT LANDS IN — the fight's shared state, which a turn borrows
/// beside the acting combatant.
pub(super) struct Ground<'w> {
    pub(super) r: &'w mut RunResult,
    pub(super) rec: &'w mut crate::record::Record,
    pub(super) rng: &'w mut Rng,
    pub(super) bodies: &'w mut [Body],
    pub(super) entities: &'w mut Vec<Entity>,
    pub(super) body_at: &'w [crate::rules::space::Vec2],
    pub(super) bounce_bodies: &'w [crate::rules::space::Vec2],
    pub(super) area_near: &'w crate::rules::space::Neighbours,
}

/// SETTLE ONE HIT at `l.t`: every pellet, what they spread, and what the hit
/// owes afterwards. Returns the clock, which a transform the gauge fills here
/// moves on.
#[inline]
#[wfsim_gas_macro::metered]
pub(super) fn land_the_shot(l: &Landing, me: &mut Combatant, g: &mut Ground) -> f64 {
    let t = l.t;
    let active = l.active(me.params);
    // …AND WHO IS ON THE LINE IS THE FIRING FORM'S ANSWER TOO: a form's punch
    // through is its own (`open`). A live grant's metres walk the line again
    // for this shot; with none, the engagement's list stands.
    let struck_live: Vec<usize>;
    let struck: &[usize] = match &me.fixed.base_struck {
        _ if l.pierce_live_m > 0.0 => {
            struck_live = active.struck_bodies_with(l.pierce_live_m);
            &struck_live
        }
        Some(b) if l.base_form => b,
        _ => &me.fixed.struck,
    };
    // The per-projectile vectors belong to the form that fired. A cycle whose
    // base form has them and whose Incarnon form does not reads an empty slice.
    let (variants, variant_rad): (&[_], &[_]) = if l.base_form && me.params.cycle.is_some() {
        (&me.fixed.base_variants, &me.fixed.base_variant_rad)
    } else {
        (&me.fixed.main_variants, &me.fixed.main_variant_rad)
    };
    let d = &mut me.d;
    // A SHOT STARTS WITH EVERY BODY STANDING: a kill between shots — a cloud's
    // tick, an orb — is no part of this one (`Body::fallen`). A burst's later
    // rounds are the same shot.
    if l.burst_starts {
        for b in g.bodies.iter_mut() {
            b.fallen = false;
        }
    }
    // ONE INSTANCE for a per-instance arcane cap: the whole pull, pellets and
    // radial included.
    me.arc.next_instance();
    // Secondary Encumber: at most ONE extra proc per instant — pellets of one
    // pull land simultaneously, so one roll per pull.
    let mut encumber_done = false;
    // DID THIS SHOT HIT ANYTHING AT ALL — the question the SHOT COMBO COUNTER
    // asks, and it is the shot's rather than the pellet's.
    let mut landed_this_shot = false;
    let mut blob_contact = None;
    // THE SHOT'S FACTORS, filled by its first landing pellet — see
    // `SpreadStrike`. `None` when nothing landed, and then nothing spreads.
    let mut strike_spread: Option<SpreadStrike> = None;
    let (mut any_head, mut any_big) = (false, false);
    let mut beam_merge = l.beam_merge;
    {
        let shot = Strike {
            round_multishot: l.rounds.per_round(l.n_pellets),
            seat: me.seat,
            active,
            qvec: &l.qvec,
            direct_pre_snap: l.direct_pre_snap,
            variants,
            variant_rad,
            modded_base: l.modded_base,
            co_base: l.co_base,
            combo_multiplier: l.combo_multiplier,
            combo_spec: me.fixed.combo_spec,
            swing: &l.swing,
            swing_mult: l.swing_mult,
            swing_forced_types: &l.swing_forced_types,
            swing_forced_independent: &l.swing_forced_independent,
            tennokai: l.tennokai,
            tennokai_heavy: l.tennokai_heavy,
            detonates: l.detonates,
            recalls: l.recalls,
            melee_struck: &l.melee_struck,
            struck,
            pierce_live_m: l.pierce_live_m,
            body_at: g.body_at,
            bounce_bodies: g.bounce_bodies,
            chain_layout: &me.fixed.chain_layout,
            ricochet_layout: &me.fixed.ricochet_layout,
            aim_off_axis: me.fixed.aim_off_axis,
            effective_cc: l.effective_cc,
            crit_chance_relative: l.crit_chance_relative,
            flat_crit: l.flat_crit,
            cc_mult: l.cc_mult,
            sc_mult: l.sc_mult,
            sc_arc_shot: l.sc_arc_shot,
            weakpoint_cd: l.weakpoint_cd,
            weakpoint_sc: l.weakpoint_sc,
            bd_reload_add: l.bd_reload_add,
            bd_eximus_add: l.bd_eximus_add,
            dt_mult: l.dt_mult,
            ms_damage: l.ms_damage,
            status_damage: me.fixed.status_damage,
            live_rate: l.live_rate,
            beam_ramp: l.beam_ramp,
            next_shot_bonus: l.next_shot_bonus,
            undamaged: l.undamaged,
            t,
            bar: &me.bar,
            rec_roster: &me.fixed.rec_roster,
            rec_buff_index: &me.fixed.rec_buff_index,
            params: me.params,
        };
        let mut round_aim = RoundAim::default();
        let mut live = Live {
            r: g.r,
            rec: g.rec,
            d,
            bodies: g.bodies,
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
            entities: g.entities,
            any_big: &mut any_big,
            any_head: &mut any_head,
            beam_merge: &mut beam_merge,
            encumber_done: &mut encumber_done,
            field_duration_boost: &mut me.field_duration_boost,
            influence_until: &mut me.influence_until,
            landed_this_shot: &mut landed_this_shot,
            blob_contact: &mut blob_contact,
            super_crit_armed: &mut me.super_crit_armed,
            round_aim: &mut round_aim,
        };
        for (pellet_idx, follows) in (0..l.n_pellets).zip(l.rounds.follows(l.n_pellets)) {
            settle_pellet(pellet_idx, follows, &shot, &mut live);
        }
    }

    // THE BLOB EMBEDS in the body the shot struck, and a pile this shot takes
    // to its cap goes off now (`fight::blobs`).
    if let Some((part, (spot, head, weakpoint, critical_head))) = active.blob.zip(blob_contact) {
        let contact = BlobContact {
            body: 0, spot, head, weakpoint, critical_head, base_form: l.base_form,
            generation: g.bodies[0].state.generation,
            head_bracket: l.shot_head_landing,
        };
        if let Some(pile) = embed_blob(g.entities, part, me.seat, contact, l.blob_stacks, t) {
            detonate_blob(
                &mut me.windows, pile, t, &mut me.gal, &mut me.arc,
                me.params, active, &me.field_ctx, g.r, g.rec, d, g.bodies, &mut me.incarnon,
                &mut BlobEvents { stacks: &mut me.buff_stacks, weakpoint_pile: &mut me.weakpoint_pile,
                    rec_index: &me.fixed.rec_buff_index, bar: &mut me.bar, enervate: &mut me.enervate },
            );
        }
        process_entities(g.entities, t + 1e-9, &mut turn!(me, active, d), g.r, g.rec, g.bodies);
    }

    // NO FORMATION, NOTHING TO REACH — see [`spread_beyond_the_target`].
    if g.bodies.len() > 1 {
        spread_beyond_the_target(
            me.seat,
            &me.windows,
            me.params,
            active,
            g.rec,
            d,
            t,
            &strike_spread,
            g.bodies,
            &mut me.gal,
            &mut me.arc,
            g.r,
            me.tendril.count,
            me.fixed.chain_layout.as_ref(),
            struck,
        );
    }
    // …AND ENDS THAT WAY, for whatever settles before the next one — once
    // its burst is done.
    let burst_len = active.burst.map_or(1, |b| b.count.max(1));
    if l.burst_completed.unwrap_or_else(|| me.ammo.rounds_this_mag.is_multiple_of(burst_len)) {
        for b in g.bodies.iter_mut() {
            b.fallen = false;
        }
    }

    // THE NEXT SHOT IS ARMED BY THIS ONE, pellets and spread included, and a
    // shot that found no weak point leaves it unarmed.
    me.weakpoint_next_shot_armed = g.r.weakpoint_hits + g.r.headshots_on_others > l.headshots_before;

    // …AND THE CLOUDS AND ARCS THIS SHOT LEFT reach the bodies standing in
    // them. Once per SHOT, after every instance of it has settled: a gas cloud
    // posted by the first pellet and one posted by the fourth are the same
    // instant, and the drain is where a body's outbox becomes its neighbours'
    // DoTs (`DebuffState::area_out`).
    drain_area_procs(
        g.bodies,
        me.params,
        g.area_near,
        g.r,
        g.rec,
        t,
        l.shot_head_landing,
        &mut d.arc_landing,
    );

    // A KILL THIS SWING OR ANYTHING BEFORE IT MADE is a spectral dagger, if the
    // form that makes them was up when it swung.
    make_daggers(me.seat, me.params, active, t, g.r, &mut me.dagger_kill_mark, g.entities);

    let mut t = t;
    after_the_landing(
        me.params,
        &me.apl,
        l.flash,
        active,
        g.rec,
        g.rng,
        d,
        &mut t,
        landed_this_shot,
        any_big,
        any_head,
        l.pellets_before,
        l.headshots_before,
        l.burst_completed,
        &l.swing,
        l.combo_now,
        l.combo_multiplier,
        l.tennokai,
        l.tennokai_heavy,
        l.tennokai_kill_mark,
        g.r,
        &mut me.bar,
        &mut me.arc,
        &mut me.enervate,
        &mut me.frenzy,
        &mut me.buff_stacks,
        &me.fixed.rec_buff_index,
        g.bodies,
        &mut me.ammo,
        &mut me.incarnon,
        &mut me.double_tap,
        &mut me.melee,
        &mut me.sniper_combo,
        &mut me.rs_armed,
        &mut me.opening_closed,
        &mut me.field_duration_boost,
    );
    t
}
