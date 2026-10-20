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
/// and advances `kill_mark` past what it killed (`fight::entities`), so what
/// is left here is every other kill: a swing's, a status tick's, a slam's.
#[wfsim_gas_macro::metered]
pub(super) fn make_daggers(
    owner: Seat,
    params: &FightParams,
    active: &FightParams,
    t: f64,
    r: &mut RunResult,
    kill_mark: &mut u32,
    live: &mut Vec<Entity>,
) {
    let fresh = r.kills.saturating_sub(*kill_mark);
    *kill_mark = r.kills;
    let clock = r.kill_clock;
    let timed = r.kill_clock_len;
    r.kill_clock_len = 0;
    let Some(g) = active.spectral_dagger else { return };
    // *"Up to 6 daggers can be active at once"* — a kill with no room makes none.
    let alive = live.iter().filter(|e| matches!(e, Entity::Dagger(g) if g.owner == owner)).count() as u32;
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
        live.push(Entity::Dagger(DaggerState {
            owner,
            part: g,
            body: target.map(|(b, _)| b),
            strikes_at: target
                .map_or(f64::INFINITY, |(_, d)| born + g.rules.orbit_seconds + d / g.speed_mps.max(1e-9)),
        }));
    }
}

