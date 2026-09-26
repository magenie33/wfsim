// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE RELOAD GRENADES — the Catabolyst family throws its magazine when it
//! reloads, and a reload from empty throws the big one (docs/MECHANICS.md §7.2).
//!
//! Each grenade is a CONTACT HIT on whatever it lands on and an EXPLOSION
//! around where it landed, both settled through `field_tick` — the arithmetic of
//! one damage instance on its own clock, which an orb's detonation already
//! shares. What is the grenade's own is where it lands and Critical Mutation.

use super::*;

/// CRITICAL MUTATION'S PILE, one per seat. The bonus is held as the fraction it
/// grants rather than as a count, because the cap is a fraction ("up to 300%")
/// that holds at every rank.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Mutation {
    pub(super) bonus: f64,
    /// Kills already paid into `bonus` — the run's counter, read as a delta, the
    /// rule every on-kill pile in this loop follows.
    pub(super) kill_mark: u32,
}

impl Mutation {
    /// Pay in every kill since the last throw, capped on the way in, and return
    /// the bonus this throw carries.
    pub(super) fn pay_in(&mut self, kills: u32, per_kill: f64, cap: f64) -> f64 {
        let fresh = kills.saturating_sub(self.kill_mark);
        self.kill_mark = kills;
        self.bonus = (self.bonus + per_kill * f64::from(fresh)).min(cap);
        self.bonus
    }

    /// Charge a throw whose explosions struck `struck` enemies.
    pub(super) fn charge(&mut self, struck: usize, loss: f64) {
        if struck < MUTATION_CROWD {
            self.bonus = (self.bonus - loss).max(0.0);
        }
    }
}

/// Fewer than this many enemies struck by a throw's explosions costs Critical
/// Mutation one step — DE's card: "Reduce by 30% when fewer than 3 enemies are
/// struck by the grenade explosion."
const MUTATION_CROWD: usize = 3;

/// THROW THE MAGAZINE at `at`: every grenade the reload releases, landed and
/// settled, and Critical Mutation's pile paid in and charged for it.
///
/// WHERE THEY LAND. At the reticle's distance, fanned `fan_deg` edge to edge
/// about the aim — the Coda's page: "Reload throws 3 grenades at a 45 degree
/// angle towards the reticle". The middle one lands on the aimed point; the
/// outer two land beside it, and whether their explosions reach the aimed body
/// is the geometry's answer.
#[allow(clippy::too_many_arguments)]
pub(super) fn throw_reload_grenades(
    w: &CardWindows,
    owner: Seat,
    at: f64,
    ctx: &FieldCtx,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    params: &FightParams,
    active: &FightParams,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    bodies: &mut [Body],
) {
    let Some(g) = active.reload_grenade else { return };
    // Status events strictly before the throw land first, exactly as they do
    // before an orb's detonation.
    process_ticks(
        w,
        &mut bodies[0], gal, arc, at + 1e-9, params, active, r, rec, &mut d.status,
        &params.foe, 0,
    );

    // THE PILE, paid in for every kill since the last throw and capped on the
    // way in. It is read only here, so settling it at the throw is exact.
    let bonus = g.crit_per_kill.map_or(0.0, |(per_kill, cap, _)| gal.mutation.pay_in(r.kills, per_kill, cap));
    let contact = lingering_of(&g.contact, bonus);
    let blast = lingering_of(&g.blast, bonus);

    let body_at = params.body_positions();
    let aim = params.aim_point();
    let muzzle = crate::rules::space::muzzle(params.player_at, aim);
    let reach = muzzle.distance(aim);
    let heading = (aim.y - muzzle.y).atan2(aim.x - muzzle.x);
    let mut struck: Vec<usize> = Vec::new();
    for i in 0..g.count {
        let spread = if g.count > 1 {
            g.fan_deg * (f64::from(i) / f64::from(g.count - 1) - 0.5)
        } else {
            0.0
        };
        let a = heading + spread.to_radians();
        let land = crate::rules::space::Vec2::new(muzzle.x + reach * a.cos(), muzzle.y + reach * a.sin());
        // THE CONTACT HIT lands on a body the grenade came down on, and on
        // nothing if it came down on the floor beside one.
        let hit = (0..body_at.len()).find(|&b| body_at[b].distance(land) <= crate::rules::space::BODY_RADIUS_M + 1e-9);
        if let Some(b) = hit {
            if settle(w, owner, &contact, g.last_round_factor, at, ctx, b, gal, arc, params, active, r, rec, d, bodies) && b == 0 {
                bodies[0].debuffs.on_death(owner, params.acid_shells, &params.foe);
                return;
            }
        }
        // THE EXPLOSION, falling off from where it landed rather than from a
        // body — "Explosion has linear Damage Falloff from 100% to 50% from
        // central impact".
        let mut aimed_died = false;
        for (b, &pos) in body_at.iter().enumerate() {
            let dist = pos.distance(land);
            if !crate::rules::space::caught_by_blast(dist, blast.radius_m) {
                continue;
            }
            if !struck.contains(&b) {
                struck.push(b);
            }
            let multiplier = g.last_round_factor * blast.falloff_at(crate::rules::space::blast_reach(dist));
            aimed_died |= settle(w, owner, &blast, multiplier, at, ctx, b, gal, arc, params, active, r, rec, d, bodies) && b == 0;
        }
        if aimed_died {
            bodies[0].debuffs.on_death(owner, params.acid_shells, &params.foe);
            break;
        }
    }
    // …AND CHARGED FOR A THROW THAT MISSED THE CROWD. Per THROW: the card speaks
    // of "the grenade explosion" as one event, and the Coda's three grenades are
    // one reload.
    if let Some((_, _, loss)) = g.crit_per_kill {
        gal.mutation.charge(struck.len(), loss);
    }
}

/// One grenade part on body `b` — the orb's settlement with the body chosen.
#[allow(clippy::too_many_arguments)]
fn settle(
    w: &CardWindows,
    owner: Seat,
    part: &crate::build::loadout::ResolvedLingering,
    multiplier: f64,
    at: f64,
    ctx: &FieldCtx,
    b: usize,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    params: &FightParams,
    active: &FightParams,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    bodies: &mut [Body],
) -> bool {
    let (Some(spec), Some(here)) = (params.body(b), bodies.get_mut(b)) else { return false };
    // AN EXPLOSION'S METER for both halves: the contact is 11 of a throw's
    // 2,000, and a reader looking for what the grenade was worth finds it in one
    // place. The record tells the two apart by their numbers.
    field_tick(
        w, owner, part, multiplier, at, ctx, here, b, gal, arc, params, active, r, rec, d,
        spec.params, crate::record::Origin::ReloadGrenade, None, true,
    )
}

/// A grenade part in the timed-part shape `field_tick` settles, with Critical
/// Mutation's bonus laid into the relative buckets it joins — "additive with
/// mods such as Pistol Gambit" and "such as Target Cracker" (wiki), so it scales
/// the part's own base.
pub(super) fn lingering_of(r: &crate::build::loadout::ResolvedRadial, bonus: f64) -> crate::build::loadout::ResolvedLingering {
    crate::build::loadout::ResolvedLingering {
        damage: r.damage,
        modified_base: r.modified_base,
        crit_chance: r.crit_chance + r.base_crit_chance * bonus,
        crit_damage: r.crit_damage + r.base_crit_damage * bonus,
        status_chance: r.status_chance,
        base_crit_chance: r.base_crit_chance,
        base_crit_damage: r.base_crit_damage,
        base_status_chance: r.base_status_chance,
        tick_rate: 1.0,
        duration_seconds: 0.0,
        first_tick_delay_seconds: 0.0,
        forced_procs: r.forced_procs,
        radius_m: r.radius_m,
        falloff_start_m: r.falloff_start_m,
        falloff_reduction: r.falloff_reduction,
        stacking: crate::model::FieldStacking::Stack,
        takes_condition_overload: false,
    }
}
