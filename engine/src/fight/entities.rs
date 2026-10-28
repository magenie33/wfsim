// SPDX-License-Identifier: AGPL-3.0-or-later
//! ENTITIES — what an attack or an event puts into the fight to act on a clock
//! of its own: a Torid's cloud, a Stug's blob pile, a Grimoire's orb, an
//! Okina's spectral dagger, a Catabolyst's reload volley, a Vectis Incarnon's
//! embedded orb.
//!
//! ONE LIST AND ONE ORDER. Whatever is due first acts first, whichever kind it
//! is, because each event changes what the next one sees — its procs, its kill,
//! the pile it leaves. What every one of them DEALS is settled by
//! [`field_tick`], the one function a timed damage instance goes through.
//!
//! What differs is how each one exists — stuck in a body, flying, orbiting the
//! wielder — and what it does when its time comes, which is its kind's own
//! step below. docs/MECHANICS.md §"Entities".
use super::*;

/// ONE ENTITY. Each carries the seat that made it: it outlives the pull, and
/// what it deals is booked to, and sized by, whoever left it.
#[derive(Debug, Clone, Copy)]
pub(super) enum Entity {
    Cloud(FieldState),
    Pile(BlobPile),
    Orb(OrbState),
    Dagger(DaggerState),
    Volley(Volley),
    ReloadCold(ReloadCold),
    ColdTrail(ColdTrailTick),
    Embed(EmbedState),
}

/// A RELOAD'S THROW, due at the instant it left the hand (`fight::grenades`).
#[derive(Debug, Clone, Copy)]
pub(super) struct Volley {
    pub(super) owner: Seat,
    pub(super) at: f64,
    /// The big throw follows a reload from empty; any other throws the partial.
    pub(super) from_empty: bool,
}

/// AN ORB EMBEDDED IN A BODY — see [`crate::data::weapons::EmbedSpec`].
#[derive(Debug, Clone, Copy)]
pub(super) struct EmbedState {
    pub(super) owner: Seat,
    pub(super) part: crate::build::loadout::ResolvedEmbed,
    /// The body it is stuck in.
    pub(super) body: usize,
    /// WHAT THE SHOT CARRIED, to every hit of the orb: Plentiful Mayhem, the
    /// chamber's first round, the shot combo counter. Measured for the last two
    /// (M117): a first round's orb reads 835 on all three of its hits.
    pub(super) carried: f64,
    /// The additive headshot bracket as of the shot, which the explosion takes
    /// (M117: x1.6 with the scope, x1.9 with Deadhead, x1 from the hip).
    pub(super) head_bracket: f64,
    /// The status types on the target before the round's own procs, which its
    /// first hit counts and its later hits do not (M118).
    pub(super) types_before: u32,
    pub(super) next_hit: f64,
    pub(super) hits_left: u32,
    /// When a head's orb explodes; `None` for one in a body.
    pub(super) explodes_at: Option<f64>,
}

#[wfsim_gas_macro::metered]
impl EmbedState {
    /// LEFT BY A PROJECTILE LANDING at `t`. It hits at once either way; in a
    /// body it hits `body_more_hits` more times, in a head it explodes instead.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        owner: Seat,
        part: crate::build::loadout::ResolvedEmbed,
        body: usize,
        t: f64,
        head: bool,
        carried: f64,
        head_bracket: f64,
        types_before: u32,
    ) -> Self {
        Self {
            owner,
            part,
            body,
            carried,
            head_bracket,
            types_before,
            next_hit: t,
            hits_left: if head { 1 } else { 1 + part.body_more_hits },
            explodes_at: head.then_some(t + part.head_after_seconds),
        }
    }

    fn due(&self) -> Option<f64> {
        let hit = (self.hits_left > 0).then_some(self.next_hit);
        match (hit, self.explodes_at) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }
}

#[wfsim_gas_macro::metered]
impl Entity {
    pub(super) fn owner(&self) -> Seat {
        match self {
            Entity::Cloud(f) => f.owner,
            Entity::Pile(p) => p.owner,
            Entity::Orb(o) => o.owner,
            Entity::Dagger(g) => g.owner,
            Entity::Volley(v) => v.owner,
            Entity::ReloadCold(v) => v.owner,
            Entity::ColdTrail(v) => v.owner,
            Entity::Embed(e) => e.owner,
        }
    }

    /// WHEN IT NEXT ACTS, or `None` once it has nothing left to do. A dagger
    /// with nobody to seek is due at infinity: it holds its place and never
    /// strikes.
    pub(super) fn due(&self) -> Option<f64> {
        match self {
            Entity::Cloud(f) => (f.ticks_left > 0).then_some(f.next_tick),
            Entity::Pile(p) => Some(p.next_tick.min(p.fuse_at)),
            Entity::Orb(o) => {
                let strike_due = o.strikes_left > 0 && o.next_strike <= o.fuse_at;
                Some(if strike_due { o.next_strike } else { o.fuse_at })
            }
            Entity::Dagger(g) => Some(g.strikes_at),
            Entity::Volley(v) => Some(v.at),
            Entity::ReloadCold(v) => Some(v.at),
            Entity::ColdTrail(v) => Some(v.at),
            Entity::Embed(e) => e.due(),
        }
    }
}

/// WHOSE ENTITIES ARE BEING SETTLED, and the state of theirs an entity reads
/// and moves.
pub(super) struct Turn<'a> {
    /// See `process_ticks` — an entity reads its maker's live windows.
    pub(super) w: &'a mut CardWindows,
    pub(super) buff_stacks: &'a mut [LiveStacks],
    pub(super) weakpoint_pile: &'a mut LiveStacks,
    pub(super) rec_buff_index: &'a [Option<usize>],
    pub(super) seat: Seat,
    pub(super) params: &'a FightParams,
    /// The form firing now: a pile and a volley read it.
    pub(super) active: &'a FightParams,
    /// The form whose panel carries the timed parts (`Fixed::field_active`):
    /// a cloud, an orb and an embedded orb outlive a transmute and read it.
    pub(super) field_active: &'a FightParams,
    pub(super) ctx: &'a FieldCtx,
    pub(super) gal: &'a mut GalStacks,
    pub(super) arc: &'a mut ArcRuntime,
    pub(super) d: &'a mut crate::rules::rng::Draws,
    pub(super) kill_mark: &'a mut u32,
    pub(super) influence_until: &'a mut f64,
    pub(super) incarnon: &'a mut IncarnonState,
    pub(super) bar: &'a mut BuffBar,
    pub(super) enervate: &'a mut Option<SecondaryEnervate>,
}

/// A [`Turn`] for combatant `$me`, firing `$active`, rolling off `$d` — its
/// disjoint fields borrowed one by one, so the rest of the combatant stays free.
macro_rules! turn {
    ($me:expr, $active:expr, $d:expr) => {
        Turn {
            w: &mut $me.windows,
            buff_stacks: &mut $me.buff_stacks,
            weakpoint_pile: &mut $me.weakpoint_pile,
            rec_buff_index: &$me.fixed.rec_buff_index,
            seat: $me.seat,
            params: $me.params,
            active: $active,
            field_active: $me.fixed.field_active,
            ctx: &$me.field_ctx,
            gal: &mut $me.gal,
            arc: &mut $me.arc,
            d: $d,
            kill_mark: &mut $me.dagger_kill_mark,
            influence_until: &mut $me.influence_until,
            incarnon: &mut $me.incarnon,
            bar: &mut $me.bar,
            enervate: &mut $me.enervate,
        }
    };
}
pub(super) use turn;

enum Flow {
    Go,
    /// The aimed body died to a cloud or an orb, which ends this walk — the
    /// next one picks up from the body standing back up.
    Stop,
}

/// SETTLE EVERY EVENT OF `turn.seat`'s ENTITIES DUE STRICTLY BEFORE `until`,
/// earliest first, ties in the order they were made.
#[wfsim_gas_macro::metered]
pub(super) fn process_entities(
    entities: &mut Vec<Entity>,
    until: f64,
    turn: &mut Turn,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    bodies: &mut [Body],
) {
    while let Some((i, at)) = entities
        .iter()
        .enumerate()
        .filter(|(_, e)| e.owner() == turn.seat)
        .filter_map(|(i, e)| e.due().filter(|&at| at < until).map(|at| (i, at)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        let flow = match entities[i] {
            Entity::Cloud(_) => cloud_tick(entities, i, at, turn, r, rec, bodies),
            Entity::Pile(_) => {
                let Entity::Pile(pile) = entities[i] else { unreachable!() };
                if pile.fuse_at <= pile.next_tick + 1e-9 {
                    entities.remove(i);
                    detonate_blob(
                        turn.w, pile, at, turn.gal, turn.arc, turn.params, turn.active, turn.ctx, r, rec,
                        turn.d, bodies, turn.incarnon, &mut BlobEvents {
                            stacks: turn.buff_stacks, weakpoint_pile: turn.weakpoint_pile,
                            rec_index: turn.rec_buff_index,
                            bar: turn.bar, enervate: turn.enervate,
                        },
                    );
                } else {
                    if let Entity::Pile(p) = &mut entities[i] { p.next_tick += 1.0; }
                    tick_blob(turn.w, pile, at, turn.gal, turn.arc, turn.params, turn.active,
                        turn.ctx, r, rec, turn.d, bodies, &mut BlobEvents {
                            stacks: turn.buff_stacks, weakpoint_pile: turn.weakpoint_pile,
                            rec_index: turn.rec_buff_index, bar: turn.bar, enervate: turn.enervate,
                        });
                }
                Flow::Go
            }
            Entity::Orb(_) => orb_event(entities, i, at, turn, r, rec, bodies),
            Entity::Dagger(_) => {
                let Entity::Dagger(dagger) = entities.remove(i) else { unreachable!() };
                dagger_strike(dagger, turn, r, rec, bodies);
                Flow::Go
            }
            Entity::Volley(_) => {
                let Entity::Volley(v) = entities.remove(i) else { unreachable!() };
                // THE FORM THAT THROWS IT, which a reload of the base form did
                // even if the fight has since transmuted.
                let active = match &turn.params.cycle {
                    Some(cy) if turn.active.reload_grenade.is_none() => &cy.base_form,
                    _ => turn.active,
                };
                throw_reload_grenades(
                    turn.w, v.owner, v.at, v.from_empty, turn.ctx, turn.gal, turn.arc, turn.params,
                    active, r, rec, turn.d, bodies,
                );
                Flow::Go
            }
            Entity::ColdTrail(_) => {
                let Entity::ColdTrail(mut tick) = entities.remove(i) else { unreachable!() };
                cold_trail_tick(tick, turn, r, rec, bodies);
                tick.left -= 1;
                tick.at += tick.interval_seconds;
                if tick.left > 0 { entities.push(Entity::ColdTrail(tick)); }
                Flow::Go
            }
            Entity::ReloadCold(_) => {
                let Entity::ReloadCold(wave) = entities.remove(i) else { unreachable!() };
                reload_cold_wave(wave, turn, r, rec, bodies);
                Flow::Go
            }
            Entity::Embed(_) => embed_event(entities, i, at, turn, r, rec, bodies),
        };
        if let Flow::Stop = flow {
            return;
        }
    }
    entities.retain(|e| e.due().is_some());
}

/// ONE TICK OF A CLOUD — on the body it is stuck to, and on every other body
/// standing in it. MECHANICS §7 "Lingering damage FIELDS".
#[wfsim_gas_macro::metered]
fn cloud_tick(
    entities: &mut [Entity],
    i: usize,
    at: f64,
    turn: &mut Turn,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    bodies: &mut [Body],
) -> Flow {
    let (params, active) = (turn.params, turn.field_active);
    // Status events strictly before this tick land first.
    process_ticks(
        turn.w, &mut bodies[0], turn.gal, turn.arc, at + 1e-9, params, active, r, rec,
        &mut turn.d.status, &params.foe, 0,
    );
    let Entity::Cloud(f) = &mut entities[i] else { unreachable!() };
    let (part, damage_multiplier, owner) = (f.part, f.damage_multiplier, f.owner);
    f.next_tick += 1.0 / part.tick_rate;
    f.ticks_left -= 1;
    let killed = field_tick(
        turn.w, owner, &part, damage_multiplier, at, turn.ctx, &mut bodies[0], 0, turn.gal, turn.arc,
        params, active, r, rec, turn.d, &params.foe, crate::record::Origin::Field, None, false,
    );
    // …AND EVERY OTHER BODY STANDING IN IT. The cloud is stuck to the body it
    // was left on, so that is its centre, and the three blast rules decide the
    // rest: any body touching it burns, at its own falloff distance measured to
    // its own nearest point (`engine::space`).
    //
    // A KILL HERE DOES NOT CLEAR THE CLOUD: the clouds belong to the body they
    // are stuck to, so only THAT one dying takes them with it.
    for (b, foe) in bodies.iter_mut().enumerate().skip(1) {
        let Some(spec) = params.body(b) else { continue };
        let dist = spec.at.distance(params.target_at);
        if !crate::rules::space::caught_by_blast(dist, part.radius_m) {
            continue;
        }
        field_tick(
            turn.w, owner, &part,
            damage_multiplier * part.falloff_at(crate::rules::space::blast_reach(dist)),
            at, turn.ctx, foe, b, turn.gal, turn.arc, params, active, r, rec, turn.d, spec.params,
            crate::record::Origin::Field, None, false,
        );
    }
    if killed {
        // A CLOUD IS A PLACE, AND IT OUTLIVES WHAT IT STUCK TO. VERBATIM (the
        // weapon's page): "Torid projectiles can also attach to corpses and will
        // remain at their position even if they disintegrate" — and the
        // respawned individual stands where the dead one did, so the cloud is
        // on it.
        bodies[0].debuffs.on_death(owner, params.acid_shells, &params.foe);
        return Flow::Stop;
    }
    Flow::Go
}

/// AN ORB'S NEXT EVENT: a strike on one body inside its reach, or — the last
/// event of its life — the detonation (`fight::orbs`).
///
/// A TICK WITH NOBODY IN REACH STRIKES NOBODY AND IS SPENT, which is what makes
/// the measured count fall out of the geometry rather than being written down
/// (MEASUREMENTS M63).
#[wfsim_gas_macro::metered]
fn orb_event(
    entities: &mut Vec<Entity>,
    i: usize,
    at: f64,
    turn: &mut Turn,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    bodies: &mut [Body],
) -> Flow {
    let (params, active) = (turn.params, turn.field_active);
    let body_at = params.body_positions();
    // Status events strictly before this one land first, exactly as they do
    // before a cloud's tick.
    process_ticks(
        turn.w, &mut bodies[0], turn.gal, turn.arc, at + 1e-9, params, active, r, rec,
        &mut turn.d.status, &params.foe, 0,
    );
    let orb = {
        let Entity::Orb(o) = &mut entities[i] else { unreachable!() };
        o.advance(at, &body_at);
        *o
    };
    let is_strike = orb.strikes_left > 0 && orb.next_strike <= orb.fuse_at;
    if !is_strike {
        entities.remove(i);
        orb_detonation(
            turn.w, &orb, at, turn.ctx, turn.gal, turn.arc, params, active, r, rec, turn.d, bodies,
            &body_at,
        );
        return Flow::Go;
    }
    if let Entity::Orb(o) = &mut entities[i] {
        o.next_strike += orb.part.strike_interval_seconds;
        o.strikes_left -= 1;
    }

    // WHO IS IN REACH. Any part of a body touching is enough — the rule every
    // sphere in this engine uses.
    let in_reach: Vec<usize> = (0..body_at.len())
        .filter(|&b| crate::rules::space::caught_by_blast(body_at[b].distance(orb.at), orb.part.strike_radius_m))
        .collect();
    if in_reach.is_empty() {
        return Flow::Go;
    }
    // ONE OF THEM, AT RANDOM. The page says the orb picks and does not say how;
    // a uniform draw is the only reading that invents no preference.
    let pick = ((turn.d.spine.next_f64() * in_reach.len() as f64) as usize).min(in_reach.len() - 1);
    let seed = in_reach[pick];

    // …AND THE CHAIN OFF IT. `chain_bodies` is the TOTAL a strike reaches, the
    // struck body included, with multishot already inside it — *"Number of
    // chains is affected by Multishot"*. NO DRAW: the count is exact (M63).
    let reached = orb.part.chain_bodies.max(1) as usize;
    let mut path = vec![seed];
    if reached > 1 {
        let mut rest: Vec<usize> = in_reach.into_iter().filter(|&b| b != seed).collect();
        // Nearest first from wherever the path has got to, ties by index — the
        // same walk `rules::chain::Layout` does, and nobody twice.
        while path.len() < reached {
            let from = *path.last().expect("seeded");
            let Some((k, _)) = rest
                .iter()
                .enumerate()
                .map(|(k, &b)| (k, body_at[from].distance(body_at[b])))
                .filter(|(_, dist)| *dist <= orb.part.chain_range_m + 1e-9)
                .min_by(|a, b| a.1.total_cmp(&b.1))
            else {
                break;
            };
            path.push(rest.remove(k));
        }
    }

    let mut aimed_died = false;
    let mut share = 1.0;
    for (hop, &b) in path.iter().enumerate() {
        if hop > 0 {
            share *= orb.part.chain_damage_per_hop;
        }
        let killed = orb_strike(
            turn.w, &orb, share, at, turn.ctx, b, turn.gal, turn.arc, params, active, r, rec, turn.d,
            bodies,
        );
        aimed_died |= killed && b == 0;
    }
    if aimed_died {
        bodies[0].debuffs.on_death(orb.owner, params.acid_shells, &params.foe);
        return Flow::Stop;
    }
    Flow::Go
}

/// A DAGGER STRIKES ITS BODY — a direct hit, which rolls its own crit and
/// status, takes Condition Overload and forces ten Cold stacks — and then
/// EXPLODES there, reaching every body inside its sphere with a forced Cold of
/// its own and no Condition Overload (`fight::daggers`).
///
/// …AND MELEE INFLUENCE (W`Okina_Incarnon_Genesis`): *"Dagger direct hits and
/// AoEs can activate Melee Influence's buff, but only statuses procced from the
/// dagger AoE will be spread by the effect"*, and *"Forced cold procs will not
/// be spread"*. The window is the wielder's own (`influence_until`), the one
/// the swings read and open.
#[wfsim_gas_macro::metered]
fn dagger_strike(
    dagger: DaggerState,
    turn: &mut Turn,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    bodies: &mut [Body],
) {
    let Some(b) = dagger.body else { return };
    let (params, active) = (turn.params, turn.params);
    let at = dagger.strikes_at;
    let body_at = params.body_positions();
    let influence = params.arcane.influence_chance > 0.0;
    process_ticks(
        turn.w, &mut bodies[0], turn.gal, turn.arc, at + 1e-9, params, active, r, rec,
        &mut turn.d.status, &params.foe, 0,
    );
    let kills_before = r.kills;
    r.kill_clock_paused = true;
    let (w, ctx) = (&*turn.w, turn.ctx);
    // WHAT THE HIT LEFT FOR INFLUENCE, and whether it killed — a body the hit
    // killed seeds nothing (*"hits that one-hit-kill enemies cannot trigger nor
    // benefit"*).
    let hit = |k: usize,
                   part: &crate::build::loadout::ResolvedLingering,
                   falloff: f64,
                   is_blast: bool,
                   bodies: &mut [Body],
                   r: &mut RunResult,
                   rec: &mut crate::record::Record,
                   gal: &mut GalStacks,
                   arc: &mut ArcRuntime,
                   d: &mut crate::rules::rng::Draws|
     -> Option<InfluenceSeed> {
        let (Some(spec), Some(here)) = (params.body(k), bodies.get_mut(k)) else { return None };
        let mut seed = InfluenceSeed::default();
        let killed = field_tick_seeded(
            w, dagger.owner, part, falloff, at, ctx, here, k, gal, arc, params, active, r, rec, d,
            spec.params, crate::record::Origin::SpectralDagger, None, is_blast,
            influence.then_some(&mut seed),
        ).killed;
        // THE BODY THAT STANDS BACK UP IS A NEW INDIVIDUAL, so the pile the
        // last one wore goes with it — before the explosion lands, so the Cold
        // the explosion forces lands on the one standing there.
        if killed {
            if k == 0 {
                bodies[0].debuffs.on_death(dagger.owner, params.acid_shells, &params.foe);
            } else {
                bodies[k].debuffs.on_death(dagger.owner, None, spec.params);
            }
            return None;
        }
        Some(seed)
    };
    let mut electricity = hit(b, &dagger.part.strike, 1.0, false, bodies, r, rec, turn.gal, turn.arc, turn.d)
        .is_some_and(|s| s.electricity);
    let centre = body_at[b];
    let mut blast_seeds: Vec<(usize, InfluenceSeed)> = Vec::new();
    for (k, &pos) in body_at.iter().enumerate() {
        let dist = pos.distance(centre);
        if !crate::rules::space::caught_by_blast(dist, dagger.part.blast.radius_m) {
            continue;
        }
        let falloff = dagger.part.blast.falloff_at(crate::rules::space::blast_reach(dist));
        if let Some(seed) = hit(k, &dagger.part.blast, falloff, true, bodies, r, rec, turn.gal, turn.arc, turn.d) {
            electricity |= seed.electricity;
            blast_seeds.push((k, seed));
        }
    }
    if influence {
        // THE SPREAD READS THE WINDOW THIS DAGGER FOUND OPEN, and the roll that
        // opens it comes after — the order a swing keeps.
        if at < *turn.influence_until {
            for (k, seed) in &blast_seeds {
                let Some(scale) = seed.scale else { continue };
                spread_from_influence(
                    dagger.owner, &body_at, bodies, params, active, *k, &seed.carried, scale,
                    params.arcane.influence_radius_m, turn.gal, turn.arc, r, rec, turn.d, at,
                );
            }
        } else if electricity && turn.d.extra.chance(params.arcane.influence_chance) {
            *turn.influence_until = at + params.arcane.influence_seconds;
        }
    }
    r.kill_clock_paused = false;
    // …AND WHAT IT KILLED MAKES NO DAGGER of its own.
    *turn.kill_mark += r.kills - kills_before;
}

/// AN EMBEDDED ORB'S NEXT EVENT: a hit, or — in a head — the explosion.
///
/// Every part is Area of Effect damage: no body part, no crit-headshot rule, and
/// none of Cold's crit damage or Puncture's crit chance (M117). What the orb
/// hits is the body it is stuck in and anyone else inside its radius, each at
/// its own falloff from that body.
#[wfsim_gas_macro::metered]
fn embed_event(
    entities: &mut Vec<Entity>,
    i: usize,
    at: f64,
    turn: &mut Turn,
    r: &mut RunResult,
    rec: &mut crate::record::Record,
    bodies: &mut [Body],
) -> Flow {
    let (params, active) = (turn.params, turn.field_active);
    process_ticks(
        turn.w, &mut bodies[0], turn.gal, turn.arc, at + 1e-9, params, active, r, rec,
        &mut turn.d.status, &params.foe, 0,
    );
    let Entity::Embed(e) = &mut entities[i] else { unreachable!() };
    let is_hit = e.hits_left > 0 && e.explodes_at.is_none_or(|x| e.next_hit <= x);
    // THE FIRST HIT is the one that lands with the round: none spent yet.
    let first = is_hit && e.hits_left == if e.explodes_at.is_some() { 1 } else { 1 + e.part.body_more_hits };
    let orb = *e;
    let (part, hits, multiplier) = if is_hit {
        e.hits_left -= 1;
        e.next_hit += orb.part.body_every_seconds;
        if e.due().is_none() {
            entities.remove(i);
        }
        // ITS HITS TAKE THE WEAPON'S CO TERM ON THE ROUND'S BASE, and the
        // compression beside it; the explosion takes neither (M118): 788 =
        // 596.25 x 1.12 + 150 x 0.8, a 10391 headshot explosion beside a status.
        let mut hit = lingering_of(&orb.part.hit, Some(active.co_behavior));
        hit.co_base = Some(active.co_base.against(orb.part.hit.co_base.of()).borrowed_for(crate::model::CoStage::Field));
        hit.co_status_types = first.then_some(orb.types_before);
        hit.takes_compression = true;
        (hit, 1, orb.carried)
    } else {
        entities.remove(i);
        (lingering_of(&orb.part.explosion, None), orb.part.head_explosion_hits, orb.carried * orb.head_bracket)
    };
    let body_at = params.body_positions();
    let Some(&centre) = body_at.get(orb.body) else { return Flow::Go };
    // TWO HITS OF THE EXPLOSION ARE TWO INSTANCES, each its own crit and its own
    // forced Cold (M117: 5195 beside 1484 off one headshot).
    for _ in 0..hits {
        for (b, &pos) in body_at.iter().enumerate() {
            let dist = pos.distance(centre);
            if !crate::rules::space::caught_by_blast(dist, part.radius_m) {
                continue;
            }
            let (Some(spec), Some(here)) = (params.body(b), bodies.get_mut(b)) else { continue };
            let falloff = part.falloff_at(crate::rules::space::blast_reach(dist));
            let killed = field_tick(
                turn.w, orb.owner, &part, multiplier * falloff, at, turn.ctx, here, b, turn.gal,
                turn.arc, params, active, r, rec, turn.d, spec.params, crate::record::Origin::Embed,
                None, true,
            );
            // THE BODY THAT STANDS BACK UP IS A NEW INDIVIDUAL.
            if killed {
                if b == 0 {
                    bodies[0].debuffs.on_death(orb.owner, params.acid_shells, &params.foe);
                } else {
                    bodies[b].debuffs.on_death(orb.owner, None, spec.params);
                }
            }
        }
    }
    Flow::Go
}

