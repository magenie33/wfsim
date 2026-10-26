// SPDX-License-Identifier: AGPL-3.0-or-later
//! Stug's attached entities: independent tick and expiry clocks (M127).
use super::*;

#[derive(Debug, Clone, Copy)]
pub(super) struct BlobContact {
    pub(super) body: usize,
    /// The arena distinguishes body parts, not patches within one part.
    pub(super) spot: usize,
    pub(super) head: bool,
    pub(super) weakpoint: bool,
    pub(super) critical_head: bool,
    pub(super) base_form: bool,
    pub(super) generation: u32,
    pub(super) head_bracket: f64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct BlobPile {
    pub(super) owner: Seat,
    pub(super) contact: BlobContact,
    pub(super) part: crate::build::loadout::ResolvedBlob,
    pub(super) stacks: u32,
    pub(super) next_tick: f64,
    pub(super) fuse_at: f64,
}

pub(super) struct BlobEvents<'a> {
    pub(super) stacks: &'a mut [LiveStacks],
    pub(super) weakpoint_pile: &'a mut LiveStacks,
    pub(super) rec_index: &'a [Option<usize>],
    pub(super) bar: &'a mut BuffBar,
    pub(super) enervate: &'a mut Option<SecondaryEnervate>,
}

/// Initialization bypasses the stack callback; only a growing merge detonates
/// early. A blob born full waits for expiry even after further merges (M109).
#[wfsim_gas_macro::metered]
pub(super) fn embed_blob(
    entities: &mut Vec<Entity>,
    part: crate::build::loadout::ResolvedBlob,
    owner: Seat,
    contact: BlobContact,
    stacks: u32,
    t: f64,
) -> Option<BlobPile> {
    let found = entities.iter().position(|e| matches!(e, Entity::Pile(p)
        if p.owner == owner && p.contact.body == contact.body
            && p.contact.spot == contact.spot && p.contact.base_form == contact.base_form
            && p.contact.generation == contact.generation));
    let Some(i) = found else {
        entities.push(Entity::Pile(BlobPile {
            owner, contact, part, stacks: stacks.min(part.cap),
            next_tick: t, fuse_at: t + part.lifespan_seconds,
        }));
        return None;
    };
    let Entity::Pile(pile) = &mut entities[i] else { unreachable!() };
    pile.fuse_at = t + part.lifespan_seconds;
    let next = pile.stacks.saturating_add(stacks).min(part.cap);
    if next <= pile.stacks { return None; }
    pile.stacks = next;
    if next < part.cap { return None; }
    let Entity::Pile(done) = entities.remove(i) else { unreachable!() };
    Some(done)
}

#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn tick_blob(
    w: &mut CardWindows, pile: BlobPile, at: f64, gal: &mut GalStacks,
    arc: &mut ArcRuntime, params: &FightParams, active: &FightParams,
    ctx: &FieldCtx, r: &mut RunResult, rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws, bodies: &mut [Body],
    events: &mut BlobEvents,
) {
    settle_blob(w, pile, false, at, gal, arc, params, active, ctx, r, rec, d, bodies, events);
}

#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn detonate_blob(
    w: &mut CardWindows, pile: BlobPile, at: f64, gal: &mut GalStacks,
    arc: &mut ArcRuntime, params: &FightParams, active: &FightParams,
    ctx: &FieldCtx, r: &mut RunResult, rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws, bodies: &mut [Body], incarnon: &mut IncarnonState,
    events: &mut BlobEvents,
) {
    // A live enemy attachment grants one charge, never one per stack/victim.
    let charges = pile.contact.base_form && incarnon.in_base_form
        && bodies.get(pile.contact.body).is_some_and(|b|
            b.state.generation == pile.contact.generation && !b.fallen);
    settle_blob(w, pile, true, at, gal, arc, params, active, ctx, r, rec, d, bodies, events);
    if charges { incarnon.charges += 1; }
}

/// Attached ticks never take the head bracket. A head's explosion carries its
/// additive bracket to every victim, without their body-part multipliers (M127).
#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
fn settle_blob(
    w: &mut CardWindows, pile: BlobPile, explosion: bool, at: f64, gal: &mut GalStacks,
    arc: &mut ArcRuntime, params: &FightParams, active: &FightParams,
    ctx: &FieldCtx, r: &mut RunResult, rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws, bodies: &mut [Body],
    events: &mut BlobEvents,
) {
    let active = match &params.cycle {
        Some(cy) if pile.contact.base_form => &cy.base_form,
        Some(_) => params,
        None => active,
    };
    process_ticks(w, &mut bodies[0], gal, arc, at + 1e-9, params, active, r, rec,
        &mut d.status, &params.foe, 0);
    let part = if explosion { &pile.part.explosion } else { &pile.part.hit };
    let mut area = lingering_of(part, None);
    let scale = pile.part.stack_scale(pile.stacks, explosion);
    area.damage = area.damage.scale(scale);
    area.modified_base *= scale;
    if explosion { area.radius_m = pile.part.radius_at(pile.stacks); }
    let body_at = params.body_positions();
    let Some(&centre) = body_at.get(pile.contact.body) else { return };
    let head = if explosion && pile.contact.head { pile.contact.head_bracket } else { 1.0 };
    let mut ctx = *ctx;
    // Every entity event reads the live ramp; the firing snapshot predates
    // intervening ticks and explosions, especially during reloads.
    ctx.flat_crit = events.bar.total_contributions().flat_crit_chance
        + crate::data::abilities::flat_crit_at(&params.abilities_now(), at);
    ctx.direct_hit = explosion && pile.part.explosion_direct_events;
    ctx.weakpoint_direct = ctx.direct_hit && pile.contact.weakpoint;
    ctx.critical_head_direct = ctx.direct_hit && pile.contact.critical_head;
    let (mut hit, mut big_crit) = (false, false);
    for (b, &pos) in body_at.iter().enumerate() {
        let dist = pos.distance(centre);
        if !crate::rules::space::caught_by_blast(dist, area.radius_m) { continue; }
        let (Some(spec), Some(here)) = (params.body(b), bodies.get_mut(b)) else { continue };
        let falloff = area.falloff_at(crate::rules::space::blast_reach(dist));
        let outcome = field_tick_seeded(w, pile.owner, &area, falloff * head, at, &ctx, here, b,
            gal, arc, params, active, r, rec, d, spec.params,
            crate::record::Origin::Blob, None, explosion, None);
        let killed = outcome.killed;
        hit = true;
        big_crit |= outcome.crit_tier >= 2;
        if explosion && pile.part.explosion_direct_events {
            let e = &mut *events;
            weakpoint_contact(params, arc, w, e.stacks, e.weakpoint_pile, e.rec_index, rec,
                at, d, pile.contact.weakpoint, spec.params.eximus);
            bump_buffs!(params, e.stacks, e.rec_index, rec, crate::model::BuffTrigger::DirectHit, at, d.extra);
            if pile.contact.weakpoint {
                if b == 0 {
                    r.weakpoint_hits += 1;
                    r.headshots += u32::from(pile.contact.head);
                } else {
                    r.headshots_on_others += 1;
                }
                if killed { weakpoint_kill(params, arc, w, at); }
            }
        }
        if killed {
            bodies[b].debuffs.on_death(pile.owner,
                if b == 0 { params.acid_shells } else { None }, spec.params);
        }
    }
    // AoE gains once. Any big crit qualifies the shared reset gate, so a
    // non-big first victim cannot hide a later victim's big crit.
    if hit {
        if let Some(en) = events.enervate.as_mut() {
            en.on_event(&Event::Hit(Hit { big_crit, ..Hit::default() }), at, events.bar);
        }
    }
}
