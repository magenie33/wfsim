use super::*;

/// Contact is assumed after approach, as for the existing claw sphere. Target
/// intent is deterministic here; native score weights and pathfinding are not
/// replayed. A respawn is a new individual, identified by its generation.
#[wfsim_gas_macro::metered]
pub(super) fn contact_targets(
    params: &FightParams,
    bodies: &[Body],
    previous: &mut Option<(usize, u32)>,
    extra_reach_m: f64,
) -> Vec<usize> {
    use crate::model::TargetPreference as P;
    let Some(preference) = params.target_preference else { return Vec::new() };
    if preference == P::Avoid { return Vec::new() }
    let available: Vec<usize> = bodies.iter().enumerate()
        .filter(|(_, b)| !b.fallen && b.state.phase == Phase::Physical)
        .map(|(i, _)| i).collect();
    let origin = previous.and_then(|(i, _)| bodies.get(i)).map_or(params.target_at, |b| b.state.at);
    let same = |i: usize| *previous == Some((i, bodies[i].state.generation));
    let rank = |i: usize| match preference {
        P::Different => u8::from(same(i)),
        P::Eximus => u8::from(!params.body(i).is_some_and(|b| b.params.eximus)),
        P::NearOwner(range) => u8::from(params.player_at.distance(bodies[i].state.at) > range),
        _ => u8::from(!same(i)),
    };
    let Some(&chosen) = available.iter().min_by(|&&a, &&b| {
        rank(a).cmp(&rank(b)).then_with(|| origin.distance(bodies[a].state.at)
            .total_cmp(&origin.distance(bodies[b].state.at))).then(a.cmp(&b))
    }) else { return Vec::new() };
    *previous = Some((chosen, bodies[chosen].state.generation));
    let Some(area) = params.melee_radius else { return vec![chosen] };
    let mut neighbours: Vec<(usize, f64)> = available.into_iter().filter(|i| *i != chosen)
        .filter_map(|i| {
            let distance = bodies[chosen].state.at.distance(bodies[i].state.at);
            crate::rules::space::caught_by_blast(distance, (area.radius_m + extra_reach_m).max(0.0)).then_some((i, distance))
        }).collect();
    neighbours.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    std::iter::once(chosen).chain(neighbours.into_iter().map(|(i, _)| i)).collect()
}
