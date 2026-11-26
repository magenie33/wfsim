// SPDX-License-Identifier: AGPL-3.0-or-later
//! Open-arena geometry for continuous weapons. Damage settles through the
//! ordinary beam and chain paths after the complete set of seeds is known.
use super::*;
use crate::model::BeamGeometry;
use crate::scene::{BeamContact, BeamContactKind, Flight, Impact, Scene};

#[derive(Debug, Clone)]
pub(in crate::fight) struct Seed {
    pub kind: Option<BeamContactKind>,
    pub multishot: bool,
    pub links: Vec<BeamContact>,
}

#[derive(Default)]
pub(super) struct Seeds {
    pub direct: Vec<Option<Seed>>,
    pub indirect: Vec<(Impact, Seed)>,
}

#[wfsim_gas_macro::metered]
impl Seed {
    fn new(kind: Option<BeamContactKind>, multishot: bool) -> Self {
        Self { kind, multishot, links: Vec::new() }
    }
}

#[wfsim_gas_macro::metered]
pub(super) fn seeds(
    scene: &Scene, path: &mut Flight, active: &FightParams, eligible: &[bool], tendrils: u32,
) -> Seeds {
    let beam = active.beam.filter(|b| b.chain_hops > 0 || b.damage_radius_m > 0. || b.beams_count > 1);
    if beam.is_none() && tendrils == 0 {
        return Seeds::default();
    }
    let mut seeds = Seeds {
        direct: path.impacts.iter().map(|_| Some(Seed::new(None, true))).collect(),
        indirect: Vec::new(),
    };
    let mut taken = vec![false; eligible.len()];
    for hit in &path.impacts {
        taken[hit.body] = true;
    }
    let reticle = path.aim_target.minus(path.muzzle).normalized().unwrap_or(path.direction);
    let sight_line = path.aim_target.minus(path.camera).normalized().unwrap_or(reticle);
    if let Some(beam) = beam {
        if beam.damage_radius_m > 0. {
            let line = path.direction.normalized().unwrap_or(reticle);
            let endpoint = path
                .stopped_at
                .or_else(|| path.impacts.last().map(|hit| hit.point))
                .unwrap_or_else(|| path.muzzle.along(line, active.range_m));
            path.beam_endpoint = Some(endpoint);
            for (body, &is_present) in eligible.iter().enumerate() {
                if is_present && !taken[body] && scene.distance_to_body(endpoint, body) <= beam.damage_radius_m {
                    let impact = scene.beam_recipient(body, endpoint);
                    add_seed(&mut seeds, path, &mut taken, impact, endpoint,
                             BeamContactKind::Radius, beam.radius_takes_multishot);
                }
            }
        }
        if beam.beams_count > 1 {
            let count = (beam.beams_count as usize).saturating_sub(path.impacts.len());
            let muzzle = path.muzzle;
            let range = beam.beams_range_m + (active.range_m - beam.range_m);
            let candidates = scene.beam_acquire(muzzle, path.camera, sight_line, beam.beams_acquire_deg, range, eligible);
            let mut remaining = count;
            for impact in candidates {
                if remaining == 0 { break; }
                if taken[impact.body] { continue; }
                add_seed(&mut seeds, path, &mut taken, impact, muzzle, BeamContactKind::Acquired, false);
                remaining -= 1;
            }
        }
        if beam.chain_hops > 0 {
            for (index, (hit, seed)) in path.impacts.iter().zip(&mut seeds.direct).enumerate() {
                if beam.chain_fanout && index + 1 != path.impacts.len() {
                    continue;
                }
                seed.as_mut().unwrap().links = links(scene, *hit, beam, eligible, &mut taken);
            }
            for (hit, seed) in &mut seeds.indirect {
                seed.links = links(scene, *hit, beam, eligible, &mut taken);
            }
        }
    }
    if tendrils > 0 && active.tendril_max > 0 {
        let muzzle = path.muzzle;
        let candidates = scene.beam_acquire(muzzle, path.camera, sight_line, active.tendril_acquire_deg, active.tendril_range_m, eligible);
        let mut remaining = tendrils;
        for impact in candidates {
            if remaining == 0 {
                break;
            }
            if taken[impact.body] {
                continue;
            }
            add_seed(&mut seeds, path, &mut taken, impact, muzzle, BeamContactKind::Tendril, false);
            remaining -= 1;
        }
    }
    for seed in seeds.direct.iter().filter_map(Option::as_ref).chain(seeds.indirect.iter().map(|(_, seed)| seed)) {
        path.beam_contacts.extend_from_slice(&seed.links);
    }
    seeds
}

#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
fn add_seed(seeds: &mut Seeds, path: &mut Flight, taken: &mut [bool], impact: Impact,
            from: crate::rules::geometry::Vec3, kind: BeamContactKind, multishot: bool) {
    taken[impact.body] = true;
    path.beam_contacts.push(BeamContact { kind, from, impact, share: 1. });
    seeds.indirect.push((impact, Seed::new(Some(kind), multishot)));
}

#[wfsim_gas_macro::metered]
fn links(scene: &Scene, seed: Impact, beam: BeamGeometry, eligible: &[bool], taken: &mut [bool]) -> Vec<BeamContact> {
    let mut seen = vec![false; eligible.len()];
    seen[seed.body] = true;
    let mut current = seed.body;
    let mut share = 1.;
    let mut path = Vec::new();
    let reach_sq = beam.chain_range_m * beam.chain_range_m;
    for _ in 0..beam.chain_hops {
        let root = scene.root(current);
        let next = eligible.iter().enumerate().filter_map(|(i, &present)| {
            if !present || seen[i] || (beam.chain_fanout && taken[i]) {
                return None;
            }
            let gap = scene.root(i).minus(root);
            let squared = gap.dot(gap);
            (squared <= reach_sq).then_some((taken[i], squared, i))
        }).min_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
        let Some((_, _, body)) = next else { break };
        taken[body] = true;
        seen[body] = true;
        share = if beam.chain_compounds { share * beam.chain_damage_per_hop } else { beam.chain_damage_per_hop };
        let from = scene.beam_center(current);
        let impact = scene.beam_recipient(body, from);
        path.push(BeamContact { kind: BeamContactKind::Chain, from, impact, share });
        if !beam.chain_fanout {
            current = body;
        }
    }
    path
}
