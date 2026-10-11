// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE STUG'S BLOBS — docs/MECHANICS.md §7.3.
//!
//! Every shot embeds in the body it hits and adds its stacks to the pile there.
//! A pile detonates when a stack takes it to the cap, or when its lifespan runs
//! out without a new one — every stack refreshes it. The explosion is one
//! stack's, times the stacks, at a radius that grows with them, settled through
//! `field_tick` like every other instance on a clock of its own.

use super::*;

/// ONE PILE: a seat's blobs on one body.
#[derive(Debug, Clone, Copy)]
pub(super) struct BlobPile {
    /// Whose blobs — the rule every entity here follows.
    pub(super) owner: Seat,
    pub(super) body: usize,
    /// What it deals, as of the shot that last fed it.
    pub(super) part: crate::build::loadout::ResolvedBlob,
    pub(super) stacks: u32,
    pub(super) fuse_at: f64,
}

/// A SHOT EMBEDS ITS STACKS in `body`. Answers the pile when this shot
/// detonates it, taken off the list; `None` while it waits.
///
/// THE OVERFLOW (M109): a shot that ALREADY carries more stacks than the cap —
/// multishot over 10, or over 5 in Incarnon form — never steps onto the cap, so
/// it does not detonate at once. The pile holds the cap and waits out its
/// lifespan, and what was over the cap is lost. Every later shot refreshes that
/// lifespan, so a pile fed faster than it expires does not detonate at all.
#[wfsim_gas_macro::metered]
pub(super) fn embed_blob(
    piles: &mut Vec<BlobPile>,
    part: crate::build::loadout::ResolvedBlob,
    owner: Seat,
    body: usize,
    stacks: u32,
    t: f64,
) -> Option<BlobPile> {
    let i = match piles.iter().position(|p| p.owner == owner && p.body == body) {
        Some(i) => i,
        None => {
            piles.push(BlobPile { owner, body, part, stacks: 0, fuse_at: t });
            piles.len() - 1
        }
    };
    let pile = &mut piles[i];
    pile.part = part;
    pile.fuse_at = t + part.lifespan_seconds;
    if stacks > part.cap {
        pile.stacks = part.cap;
        return None;
    }
    pile.stacks += stacks;
    if pile.stacks < part.cap {
        return None;
    }
    // "Blobs which reach 10 stacks now immediately explode" — and a pile does
    // not hold more than the cap, so a shot that crosses it pays the cap.
    let mut done = piles.remove(i);
    done.stacks = part.cap;
    Some(done)
}

/// EVERY PILE WHOSE LIFESPAN RUNS OUT BEFORE `until`, in the order they do.
#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn process_blobs(
    // See `process_ticks`.
    w: &CardWindows,
    piles: &mut Vec<BlobPile>,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    until: f64,
    params: &FightParams,
    active: &FightParams,
    ctx: &FieldCtx,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    bodies: &mut [Body],
) {
    while let Some(i) = piles
        .iter()
        .enumerate()
        .filter(|(_, p)| p.fuse_at < until)
        .min_by(|a, b| a.1.fuse_at.total_cmp(&b.1.fuse_at))
        .map(|(i, _)| i)
    {
        let pile = piles.remove(i);
        let at = pile.fuse_at;
        detonate_blob(w, pile, at, gal, arc, params, active, ctx, r, rec, d, bodies);
    }
}

/// A PILE GOES OFF at `at`: one stack's explosion times the stacks, on every
/// body inside the pile's radius, falling off from the body it was embedded in.
#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn detonate_blob(
    w: &CardWindows,
    pile: BlobPile,
    at: f64,
    gal: &mut GalStacks,
    arc: &mut ArcRuntime,
    params: &FightParams,
    active: &FightParams,
    ctx: &FieldCtx,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    d: &mut crate::rules::rng::Draws,
    bodies: &mut [Body],
) {
    // Status events strictly before the detonation land first, as they do
    // before a grenade's.
    process_ticks(
        w,
        &mut bodies[0], gal, arc, at + 1e-9, params, active, r, rec, &mut d.status,
        &params.foe, 0,
    );
    // A PILE IS ONE INSTANCE OF `n` STACKS' DAMAGE — "75 - 750 damage" — so the
    // stacks scale its base, and the statuses it rolls are seeded from all of it.
    let stacks = f64::from(pile.stacks);
    let mut blast = lingering_of(&pile.part.explosion, None);
    blast.damage = blast.damage.scale(stacks);
    blast.modified_base *= stacks;
    blast.radius_m = pile.part.radius_at(pile.stacks);
    let body_at = params.body_positions();
    let Some(&centre) = body_at.get(pile.body) else { return };
    for (b, &pos) in body_at.iter().enumerate() {
        let dist = pos.distance(centre);
        if !crate::rules::space::caught_by_blast(dist, blast.radius_m) {
            continue;
        }
        let (Some(spec), Some(here)) = (params.body(b), bodies.get_mut(b)) else { continue };
        let falloff = blast.falloff_at(crate::rules::space::blast_reach(dist));
        let killed = field_tick(
            w, pile.owner, &blast, falloff, at, ctx, here, b, gal, arc, params, active,
            r, rec, d, spec.params, crate::record::Origin::Blob, None, true,
        );
        // THE BODY THAT STANDS BACK UP IS A NEW INDIVIDUAL.
        if killed {
            if b == 0 {
                bodies[0].debuffs.on_death(pile.owner, params.acid_shells, &params.foe);
            } else {
                bodies[b].debuffs.on_death(pile.owner, None, spec.params);
            }
        }
    }
}
