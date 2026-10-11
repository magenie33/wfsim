//! THE OKINA'S SPECTRAL DAGGERS — `notes: okina_spectral_dagger`.
//!
//! An ENTITY, like an orb: made by a kill, it orbits the wielder, seeks a body
//! and strikes it on a clock of its own, so the swing loop never waits for it.
//! What it deals is settled through [`field_tick_seeded`], the one function
//! every timed damage instance in this engine shares.
use super::*;

/// ONE DAGGER between the kill that made it and the body it strikes.
#[derive(Debug, Clone, Copy)]
pub(super) struct DaggerState {
    /// Whose kill made it — the rule every entity here follows.
    pub(super) owner: Seat,
    /// What it deals, as of the panel that made it.
    pub(super) part: SpectralDaggerParams,
    /// The body it seeks. `None` while nobody stands inside its reach, and then
    /// it orbits on, holding one of the six places.
    pub(super) body: Option<usize>,
    /// When it strikes — the orbit, then the flight at the infobox's speed.
    pub(super) strikes_at: f64,
}

/// MAKE A DAGGER FOR EVERY KILL SINCE THE LAST LOOK, while the form making them
/// is up, each AT ITS KILL'S OWN TIME (`RunResult::kill_clock`).
///
/// *"Kills from the daggers will not generate new daggers, but kills from
/// status procs created by daggers will"* — a dagger's own hit pauses the clock
/// and advances `kill_mark` past what it killed ([`process_daggers`]), so what
/// is left here is every other kill: a swing's, a status tick's, a slam's.
#[wfsim_gas_macro::metered]
pub(super) fn make_daggers(
    owner: Seat,
    params: &FightParams,
    active: &FightParams,
    t: f64,
    r: &mut RunResult,
    kill_mark: &mut u32,
    live: &mut Vec<DaggerState>,
) {
    let fresh = r.kills.saturating_sub(*kill_mark);
    *kill_mark = r.kills;
    let clock = r.kill_clock;
    let timed = r.kill_clock_len;
    r.kill_clock_len = 0;
    let Some(g) = active.spectral_dagger else { return };
    // *"Up to 6 daggers can be active at once"* — a kill with no room makes none.
    let alive = live.iter().filter(|d| d.owner == owner).count() as u32;
    let n = fresh.min(g.rules.max_alive.saturating_sub(alive));
    if n == 0 {
        return;
    }
    // THE NEAREST BODY INSIDE ITS REACH, measured from the wielder it orbits.
    // The page says it "seeks an enemy within 15 meters" and names no
    // preference, so the nearest is the one that invents none.
    let target = params
        .body_positions()
        .iter()
        .enumerate()
        .map(|(b, &at)| (b, crate::rules::space::gap(params.player_at, at)))
        .filter(|&(_, d)| d <= g.rules.seek_range_m)
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    // A KILL PAST THE CLOCK'S LENGTH is dated to this look.
    let births = clock[..timed].iter().map(|&at| at.min(t)).chain(std::iter::repeat(t));
    for born in births.take(n as usize) {
        live.push(DaggerState {
            owner,
            part: g,
            body: target.map(|(b, _)| b),
            strikes_at: target
                .map_or(f64::INFINITY, |(_, d)| born + g.rules.orbit_seconds + d / g.speed_mps.max(1e-9)),
        });
    }
}

/// SETTLE EVERY DAGGER DUE STRICTLY BEFORE `until`, oldest first — the shape
/// [`process_orbs`] has, for the same reason: a strike changes what the next
/// one sees.
///
/// A dagger STRIKES its body — a direct hit, which rolls its own crit and
/// status, takes Condition Overload and forces ten Cold stacks — and then
/// EXPLODES there, reaching every body inside its sphere with a forced Cold of
/// its own and no Condition Overload.
///
/// …AND MELEE INFLUENCE (W`Okina_Incarnon_Genesis`): *"Dagger direct hits and
/// AoEs can activate Melee Influence's buff, but only statuses procced from the
/// dagger AoE will be spread by the effect"*, and *"Forced cold procs will not
/// be spread"*. The window is the wielder's own (`influence_until`), the one
/// the swings read and open.
#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub(super) fn process_daggers(
    // See `process_ticks`.
    w: &CardWindows,
    daggers: &mut Vec<DaggerState>,
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
    kill_mark: &mut u32,
    influence_until: &mut f64,
) {
    if daggers.is_empty() {
        return;
    }
    let body_at = params.body_positions();
    let influence = params.arcane.influence_chance > 0.0;
    while let Some(i) = daggers
        .iter()
        .enumerate()
        .filter(|(_, g)| g.strikes_at < until)
        .min_by(|a, b| a.1.strikes_at.total_cmp(&b.1.strikes_at))
        .map(|(i, _)| i)
    {
        let dagger = daggers.remove(i);
        let at = dagger.strikes_at;
        let Some(b) = dagger.body else { continue };
        process_ticks(
            w,
            &mut bodies[0], gal, arc, at + 1e-9, params, active, r, rec, &mut d.status,
            &params.foe, 0,
        );
        let kills_before = r.kills;
        r.kill_clock_paused = true;
        // WHAT THE HIT LEFT FOR INFLUENCE, and whether it killed — a body the
        // hit killed seeds nothing (*"hits that one-hit-kill enemies cannot
        // trigger nor benefit"*).
        let mut hit = |k: usize,
                       part: &crate::build::loadout::ResolvedLingering,
                       falloff: f64,
                       is_blast: bool,
                       bodies: &mut [Body],
                       r: &mut RunResult,
                       rec: &mut crate::record::Record,
                       d: &mut crate::rules::rng::Draws|
         -> Option<InfluenceSeed> {
            let (Some(spec), Some(here)) = (params.body(k), bodies.get_mut(k)) else { return None };
            let mut seed = InfluenceSeed::default();
            let killed = field_tick_seeded(
                w,
                dagger.owner,
                part, falloff, at, ctx, here, k, gal, arc, params, active, r, rec, d,
                spec.params, crate::record::Origin::SpectralDagger, None, is_blast,
                influence.then_some(&mut seed),
            );
            // THE BODY THAT STANDS BACK UP IS A NEW INDIVIDUAL, so the pile
            // the last one wore goes with it — before the explosion lands, so
            // the Cold the explosion forces lands on the one standing there.
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
        let mut electricity = hit(b, &dagger.part.strike, 1.0, false, bodies, r, rec, d)
            .is_some_and(|s| s.electricity);
        let centre = body_at[b];
        let mut blast_seeds: Vec<(usize, InfluenceSeed)> = Vec::new();
        for (k, &pos) in body_at.iter().enumerate() {
            let dist = pos.distance(centre);
            if !crate::rules::space::caught_by_blast(dist, dagger.part.blast.radius_m) {
                continue;
            }
            let falloff = dagger.part.blast.falloff_at(crate::rules::space::blast_reach(dist));
            if let Some(seed) = hit(k, &dagger.part.blast, falloff, true, bodies, r, rec, d) {
                electricity |= seed.electricity;
                blast_seeds.push((k, seed));
            }
        }
        if influence {
            // THE SPREAD READS THE WINDOW THIS DAGGER FOUND OPEN, and the roll
            // that opens it comes after — the order a swing keeps.
            let open = at < *influence_until;
            if open {
                for (k, seed) in &blast_seeds {
                    let Some(scale) = seed.scale else { continue };
                    spread_from_influence(
                        dagger.owner, &body_at, bodies, params, active, *k, &seed.carried, scale,
                        params.arcane.influence_radius_m, gal, arc, r, rec, d, at,
                    );
                }
            } else if electricity && d.extra.chance(params.arcane.influence_chance) {
                *influence_until = at + params.arcane.influence_seconds;
            }
        }
        r.kill_clock_paused = false;
        // …AND WHAT IT KILLED MAKES NO DAGGER of its own.
        *kill_mark += r.kills - kills_before;
    }
}
