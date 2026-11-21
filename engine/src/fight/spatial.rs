// SPDX-License-Identifier: AGPL-3.0-or-later
//! Schedule geometry-derived recipients through the ordinary pellet damage path.
use super::*;
use std::{cell::Cell, rc::Rc};

pub(super) mod beam;

/// A spatial landing uses the recipient as the primary target of settlement.
/// The rest of the source configuration is shared by all of its landings.
pub(super) struct Recipient<'p> {
    source: &'p FightParams,
    index: usize,
    params: FightParams,
}

#[wfsim_gas_macro::metered]
impl<'p> Recipient<'p> {
    pub(super) fn params<'a>(
        source: &'p FightParams,
        cached: &'a mut Option<Box<Self>>,
        index: usize,
    ) -> &'a FightParams
    where
        'p: 'a,
    {
        if index == 0 && source.gauge_charging_body_parts.is_empty() {
            return source;
        }
        if cached.as_ref().is_none_or(|view| !std::ptr::eq(view.source, source)) {
            let mut params = source.clone();
            params.gauge_charging_body_parts.clear();
            *cached = Some(Box::new(Self { source, index: 0, params }));
        }
        let view = cached.as_mut().unwrap();
        if view.index != index {
            let spec = source.body(index).expect("prepared spatial recipient");
            view.params.foe.clone_from(spec.params);
            view.params.body_parts.clear();
            view.params.body_parts.extend_from_slice(spec.body_parts);
            view.params.target_at = spec.at;
            view.index = index;
        }
        &view.params
    }
}

#[derive(Debug, Clone)]
pub(super) struct SpatialLanding {
    pub impact: crate::scene::Impact,
    pub miss: bool,
    pub first: bool,
    pub last: bool,
    pub instance: u32,
    pub follows: bool,
    pub contact: usize,
    pub splash: bool,
    pub beam: Option<beam::Seed>,
    progress: Rc<Cell<Progress>>,
}

#[derive(Debug, Clone, Copy, Default)]
struct Progress {
    landed: bool,
    big: bool,
    head: bool,
}

#[wfsim_gas_macro::metered]
impl SpatialLanding {
    pub fn aggregate(&self, landed: &mut bool, big: &mut bool, head: &mut bool) {
        let mut p = self.progress.get();
        p.landed |= *landed;
        p.big |= *big;
        p.head |= *head;
        *landed = p.landed;
        *big = p.big;
        *head = p.head;
        self.progress.set(p);
    }
}

#[wfsim_gas_macro::metered]
pub(super) fn launch(
    l: &Landing,
    active: &FightParams,
    me: &mut Combatant,
    bodies: &[Body],
    rec: &mut crate::record::Record,
) {
    let burst_completed = Some(
        me.ammo
            .rounds_this_mag
            .is_multiple_of(active.burst.map_or(1, |b| b.count.max(1))),
    );
    let Some(scene) = me.params.scene_3d.as_ref().cloned() else {
        return;
    };
    let parts = if me.incarnon.in_base_form && !me.params.gauge_charging_body_parts.is_empty() {
        &me.params.gauge_charging_body_parts
    } else {
        &me.params.body_parts
    };
    let head_weight: f64 = parts
        .iter()
        .filter(|p| p.is_head)
        .map(|p| p.aim_weight)
        .sum();
    let all: f64 = parts.iter().map(|p| p.aim_weight).sum();
    let groups = l.rounds.groups(l.n_pellets);
    let origins = scene.fire_origins(&mut me.fire_state, active.form, &groups);
    let eligible: Vec<_> = bodies
        .iter()
        .map(|b| b.state.phase == Phase::Physical)
        .collect();
    let progress = Rc::new(Cell::new(Progress::default()));
    let mut events = Vec::new();
    for (round, &copies) in groups.iter().enumerate() {
        if copies == 0 {
            continue;
        }
        let head = me.d.aim.next_f64() < head_weight / all.max(1e-20);
        let (dev, phi) = match active.spread.filter(|s| !s.is_pinpoint()) {
            Some(s) => s.sample(
                me.d.aim.next_f64(),
                me.d.blast_dir.next_f64(),
                me.d.aim.next_f64(),
                round as u32,
                groups.len() as u32,
            ),
            None => (0., 0.),
        };
        let mut path = scene.trace(
            head,
            dev,
            phi,
            active.projectile_speed_mps,
            active.homing,
            active.punch_through_m + l.pierce_live_m,
            active.infinite_body_punch_through,
            active.projectile_reverse_query,
            active.projectile_width_m,
            active.range_m,
            &eligible,
            origins.as_ref().and_then(|o| o[round]),
        );
        let mut seeds = beam::seeds(&scene, &mut path, active, &eligible, me.tendril.count);
        if let (Some(hit), Some(radial)) = (path.impacts.first().copied(), active.radial) {
            if let Some(center) = path.blast_center {
                path.radial_impacts = scene.blast_hits(hit, center, f64::from(radial.radius_m as f32), &eligible);
            } else if active.homing.is_none() {
                path.radial_impacts = (0..bodies.len()).filter_map(|body| {
                    let distance = scene.distance_to_body(hit.point, body);
                    (radial.falloff_at(distance) > 0.).then_some(crate::scene::Impact {
                        body, part: 0, radial: true, radial_distance: distance, ..hit
                    })
                }).collect();
            }
        }
        let each = l.rounds.per_round(l.n_pellets);
        for (contact, hit) in path.impacts.iter().enumerate() {
            let count = if contact == 0 {
                copies
            } else {
                each.floor() as u32 + u32::from(me.d.extra.chance(each.fract()))
            };
            // Direct and immediate radial instances retain per-instance ordering.
            for instance in 0..count {
                events.push(Landing {
                    spatial: Some(SpatialLanding {
                        impact: *hit,
                        miss: false,
                        first: false,
                        last: false,
                        instance,
                        follows: instance > 0,
                        contact,
                        splash: false,
                        beam: seeds.direct.get_mut(contact).and_then(Option::take),
                        progress: progress.clone(),
                    }),
                    t: l.t + hit.seconds,
                    n_pellets: 1,
                    rounds: Rounds::single(1, each),
                    burst_starts: false,
                    burst_completed,
                    ..l.clone()
                });
                if let Some(rad) = active.radial.filter(|_| contact == 0) {
                    for &impact in &path.radial_impacts {
                        events.push(Landing {
                            spatial: Some(SpatialLanding {
                                impact,
                                miss: false,
                                first: false,
                                last: false,
                                instance,
                                follows: instance > 0,
                                contact,
                                splash: impact.body != hit.body,
                                beam: None,
                                progress: progress.clone(),
                            }),
                            t: l.t + hit.seconds + rad.delay_seconds,
                            radial_only: true,
                            n_pellets: 1,
                            rounds: Rounds::single(1, each),
                            burst_starts: false,
                            burst_completed,
                            ..l.clone()
                        });
                    }
                }
            }
        }
        let indirect = !seeds.indirect.is_empty();
        for (impact, seed) in seeds.indirect {
            let beam_merge = if seed.multishot { l.beam_merge } else { 1. };
            events.push(Landing {
                spatial: Some(SpatialLanding {
                    impact,
                    miss: false,
                    first: false,
                    last: false,
                    instance: 0,
                    follows: false,
                    contact: 0,
                    splash: false,
                    beam: Some(seed),
                    progress: progress.clone(),
                }),
                n_pellets: 1,
                rounds: Rounds::single(1, 1.),
                beam_merge,
                burst_starts: false,
                burst_completed,
                ..l.clone()
            });
        }
        if path.impacts.is_empty() && !indirect {
            rec.push(
                l.t,
                None,
                crate::record::Kind::Miss {
                    reason: "3d trajectory reached no enemy",
                },
            );
        }
        rec.trace_spatial(l.t, l.shot, round as u32, path);
    }
    if events.is_empty() {
        let impact = crate::scene::Impact {
            body: 0,
            proxy: 0,
            part: 0,
            point: scene.muzzle(),
            exit: scene.muzzle(),
            seconds: 0.,
            travel: 0.,
            material: 0.,
            remaining: 0.,
            radial: false,
            radial_distance: 0.,
        };
        events.push(Landing {
            spatial: Some(SpatialLanding {
                impact,
                miss: true,
                first: false,
                last: false,
                instance: 0,
                follows: false,
                contact: 0,
                splash: false,
                beam: None,
                progress,
            }),
            n_pellets: 0,
            burst_starts: false,
            burst_completed,
            ..l.clone()
        });
    }
    events.sort_by(|a, b| a.t.total_cmp(&b.t));
    if let Some(first) = events.first_mut() {
        first.burst_starts = l.burst_starts;
        first.spatial.as_mut().unwrap().first = true;
    }
    if let Some(last) = events.last_mut() {
        last.spatial.as_mut().unwrap().last = true;
    }
    me.in_flight.extend(events);
}
