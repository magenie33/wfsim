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
    pub redirected: bool,
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
    let misses = (scene.miss_chance > 0.).then(|| {
        groups.iter().map(|&copies| copies > 0 && me.d.aim.chance(scene.miss_chance)).collect::<Vec<_>>()
    });
    let origins = scene.fire_origins(&mut me.fire_state, active.form, &groups);
    let eligible: Vec<_> = bodies
        .iter()
        .map(|b| b.state.phase == Phase::Physical)
        .collect();
    let progress = Rc::new(Cell::new(Progress::default()));
    let mut events = Vec::new();
    let accuracy = active.spread_scaling.map_or_else(crate::model::SpreadRuntime::default, |s| s.runtime);
    let enter_forked_beam = active.continuous && l.spread.is_some() && match accuracy.beam_mode {
        Some(crate::model::BeamSpreadMode::ChildQuat { .. } | crate::model::BeamSpreadMode::Fan { .. }) => true,
        Some(crate::model::BeamSpreadMode::Smoothed { .. }) => l.spread.is_some_and(|s| !s.is_pinpoint()),
        None => l.blob_stacks > 1 && l.spread.is_some_and(|s| !s.is_pinpoint()),
    };
    if enter_forked_beam {
        launch_forked_beam(
            l, active, me, &scene, &eligible, &origins, misses.as_deref(),
            head_weight, all, accuracy, burst_completed, progress.clone(), &mut events, rec,
        );
    } else {
        let split_multishot = l.spread.is_some_and(|s| {
            !s.is_pinpoint()
                || accuracy.extra_floor.is_some()
                || accuracy.multishot_cone.is_some()
                || accuracy.fan.is_some()
                || accuracy.extra_spread.is_some()
        });
        let total_emissions = if split_multishot { l.n_pellets as usize } else { groups.len() };
        let ordinary = groups.iter().copied().enumerate().filter(|(_, copies)| *copies > 0)
            .map(|(round, copies)| (round, if split_multishot { 1 } else { copies }, 0u32));
        let additional_groups: &[u32] = if split_multishot { &groups } else { &[] };
        let extra = additional_groups.iter().copied().enumerate()
            .flat_map(|(round, copies)| (1..copies).map(move |copy_idx| (round, 1, copy_idx)));
        for (iteration, (round, copies, copy_offset)) in ordinary.chain(extra).enumerate() {
            let charging = me.incarnon.in_base_form && !me.params.gauge_charging_body_parts.is_empty();
            let weakpoint_aim = if charging { 1. } else { scene.weakpoint_aim.unwrap_or(head_weight / all.max(1e-20)) };
            let head = me.d.aim.next_f64() < weakpoint_aim;
            if misses.as_ref().is_some_and(|misses| misses[round]) {
                let origin = origins.as_ref().and_then(|origins| origins[round]);
                let path = scene.launch_flight(head, 0., 0., &eligible, origin);
                rec.push(l.t, None, crate::record::Kind::Miss { reason: "scenario miss" });
                rec.trace_spatial(l.t, l.shot, round as u32, path);
                continue;
            }
            let spread = l.spread.map(|mut s| {
                if let Some(scale) = accuracy.charge_attenuation {
                    s.min_deg *= f64::from(scale);
                }
                s
            });
            let (dev, phi) = if accuracy.first_pellet_pinpoint && iteration == 0 {
                (0., 0.)
            } else if let Some(cone) = accuracy.multishot_cone.filter(|_| iteration > 0 && total_emissions > 1) {
                cone.sample(iteration, total_emissions, accuracy.charge_attenuation.unwrap_or(1.0))
            } else if let (Some(s), Some(fan)) = (spread, accuracy.fan) {
                let jitter = if s.is_pinpoint() { 0.5 } else { me.d.aim.next_f64() };
                fan.sample(iteration, total_emissions.max(1), s.min_deg, jitter)
            } else if let Some(mut s) = spread {
                if let Some(floor) = accuracy.extra_floor.filter(|_| l.n_pellets > 1) {
                    let base = groups.len().max(1);
                    if iteration >= base {
                        let lower = floor + (iteration - base) as f32 * 0.2 / base as f32;
                        s.min_deg = s.min_deg.max(f64::from(lower));
                    }
                }
                if let Some(extra_sp) = accuracy.extra_spread.filter(|_| l.n_pellets > 1) {
                    let h_ratio = s.sampling.map_or(0.5, |sp| sp.horizontal_ratio);
                    if extra_sp.uniform {
                        extra_sp.sample(s.min_deg, h_ratio, 0., 1., iteration as u32, l.n_pellets)
                    } else if s.is_pinpoint() && extra_sp.rotation_offset_deg == 0.0 {
                        (0., 0.)
                    } else {
                        extra_sp.sample(
                            s.min_deg,
                            h_ratio,
                            me.d.blast_dir.next_f64(),
                            me.d.aim.next_f64(),
                            iteration as u32,
                            l.n_pellets,
                        )
                    }
                } else if !s.is_pinpoint() {
                    s.sample(
                        me.d.aim.next_f64(),
                        me.d.blast_dir.next_f64(),
                        me.d.aim.next_f64(),
                        iteration as u32,
                        total_emissions.max(1) as u32,
                    )
                } else {
                    (0., 0.)
                }
            } else {
                (0., 0.)
            };
        let pierce = if active.continuous
            && active.beam.is_some_and(|b| b.damage_radius_m > 0.)
            && !active.infinite_body_punch_through
        {
            0.
        } else {
            active.punch_through_m + l.pierce_live_m
        };
        let fire_origin = origins.as_ref().and_then(|o| o[round]);
        let rebound = active.rebound_blast.zip(active.projectile_speed_mps);
        let (mut path, reflected_from) = if let Some((model, speed)) = rebound {
            let mut path = scene.launch_flight(head, dev, phi, &eligible, fire_origin);
            let reflected = scene.trace_rebound_blast(
                &mut path, model, speed, active.projectile_width_m * 0.5, &eligible,
            );
            (path, reflected)
        } else {
            let mut path = scene.trace(
                head, dev, phi, active.projectile_speed_mps, active.homing, pierce,
                active.infinite_body_punch_through, active.projectile_reverse_query,
                active.projectile_width_m, active.range_m, &eligible, fire_origin,
            );
            if let Some(lock) = active.lock_on {
                lock_shot(&scene, active, lock, &eligible, head, me, &mut path);
            }
            let reflected = active.ricochet.filter(|r| r.reflects_from_surfaces)
                .map_or(usize::MAX, |r| scene.reflect_path(&mut path, r.bounces,
                    active.projectile_speed_mps, active.projectile_width_m * 0.5,
                    active.range_m, pierce, &eligible));
            (path, reflected)
        };
        let mut seeds = beam::seeds(&scene, &mut path, active, &eligible, me.tendril.count);
        let mut contact_blasts = Vec::new();
        let mut surface_blasts = Vec::new();
        if let Some(radial) = active.radial.filter(|_| rebound.is_some()) {
            for det in &path.detonations {
                let hits: Vec<_> = eligible.iter().enumerate().filter_map(|(body, &present)| {
                    if !present {
                        return None;
                    }
                    let distance = scene.distance_to_body(det.point, body);
                    (radial.falloff_at(distance) > 0.).then_some(crate::scene::Impact {
                        body, proxy: 0, part: 0, point: det.point, exit: det.point,
                        seconds: det.seconds, travel: det.travel, material: 0., remaining: 0.,
                        radial: true, radial_distance: distance,
                    })
                }).collect();
                path.radial_impacts.extend_from_slice(&hits);
                if det.body.is_some() {
                    contact_blasts.push(hits);
                } else {
                    surface_blasts.extend(hits);
                }
            }
        } else if let (Some(hit), Some(radial)) = (path.impacts.first().copied(), active.radial) {
            if let Some(center) = path.blast_center {
                path.radial_impacts = scene.blast_hits(hit, center, f64::from(radial.radius_m as f32), &eligible);
            } else if active.homing.is_none() {
                path.radial_impacts = eligible.iter().enumerate().filter_map(|(body, &present)| {
                    if !present {
                        return None;
                    }
                    let distance = scene.distance_to_body(hit.point, body);
                    (radial.falloff_at(distance) > 0.).then_some(crate::scene::Impact {
                        body, part: 0, radial: true, radial_distance: distance, ..hit
                    })
                }).collect();
            }
        } else if let Some(radial) = active.radial.filter(|_| rebound.is_none() && active.homing.is_none()) {
            if let Some((point, travel, seconds)) = scene.floor_contact(
                path.muzzle, path.direction, active.projectile_speed_mps, active.range_m,
            ) {
                path.detonations.push(crate::scene::DetonationTrace {
                    point, seconds, travel, body: None, expired: false,
                });
                path.stopped_at = Some(point);
                let hits: Vec<_> = eligible.iter().enumerate().filter_map(|(body, &present)| {
                    if !present {
                        return None;
                    }
                    let distance = scene.distance_to_body(point, body);
                    (radial.falloff_at(distance) > 0.).then_some(crate::scene::Impact {
                        body, proxy: 0, part: 0, point, exit: point,
                        seconds, travel, material: 0., remaining: 0.,
                        radial: true, radial_distance: distance,
                    })
                }).collect();
                path.radial_impacts.extend_from_slice(&hits);
                surface_blasts.extend(hits);
            }
        }
        let each = l.rounds.per_round(l.n_pellets);
        let instance_each = if split_multishot { 1.0 } else { each };
        for (contact, hit) in path.impacts.iter().enumerate() {
            let count = if split_multishot || contact == 0 || rebound.is_some() {
                copies
            } else {
                each.floor() as u32 + u32::from(me.d.extra.chance(each.fract()))
            };
            // Direct and immediate radial instances retain per-instance ordering.
            for local in 0..count {
                let instance = copy_offset + local;
                let follows = instance > 0;
                events.push(Landing {
                    spatial: Some(SpatialLanding {
                        impact: *hit,
                        miss: false,
                        first: false,
                        last: false,
                        instance,
                        follows,
                        contact,
                        splash: false,
                        beam: seeds.direct.get_mut(contact).and_then(Option::take),
                        redirected: contact >= reflected_from,
                        progress: progress.clone(),
                    }),
                    t: l.t + hit.seconds,
                    n_pellets: 1,
                    rounds: Rounds::single(1, instance_each),
                    burst_starts: false,
                    burst_completed,
                    ..l.clone()
                });
                if let Some(rc) = active.ricochet.filter(|r| !r.reflects_from_surfaces) {
                    for impact in redirects(&scene, *hit, rc, &eligible, &mut me.d.spine) {
                        path.redirects.push(impact);
                        events.push(Landing {
                            spatial: Some(SpatialLanding {
                                impact,
                                miss: false,
                                first: false,
                                last: false,
                                instance,
                                follows: false,
                                contact: 0,
                                splash: false,
                                beam: None,
                                redirected: true,
                                progress: progress.clone(),
                            }),
                            t: l.t + hit.seconds,
                            n_pellets: 1,
                            rounds: Rounds::single(1, instance_each),
                            burst_starts: false,
                            burst_completed,
                            ..l.clone()
                        });
                    }
                }
                if let Some(rad) = active.radial {
                    let radial_impacts = if rebound.is_some() {
                        contact_blasts[contact].as_slice()
                    } else if contact == 0 {
                        path.radial_impacts.as_slice()
                    } else {
                        &[]
                    };
                    for &impact in radial_impacts {
                        events.push(Landing {
                            spatial: Some(SpatialLanding {
                                impact,
                                miss: false,
                                first: false,
                                last: false,
                                instance,
                                follows,
                                contact,
                                splash: impact.body != hit.body,
                                redirected: contact >= reflected_from,
                                beam: None,
                                progress: progress.clone(),
                            }),
                            t: l.t + hit.seconds + rad.delay_seconds,
                            radial_only: true,
                            n_pellets: 1,
                            rounds: Rounds::single(1, instance_each),
                            burst_starts: false,
                            burst_completed,
                            ..l.clone()
                        });
                    }
                }
            }
        }
        if let Some(rad) = active.radial {
            for &impact in &surface_blasts {
                for local in 0..copies {
                    let instance = copy_offset + local;
                    let follows = instance > 0;
                    events.push(Landing {
                        spatial: Some(SpatialLanding {
                            impact,
                            miss: false,
                            first: false,
                            last: false,
                            instance,
                            follows,
                            contact: 0,
                            splash: true,
                            beam: None,
                            redirected: false,
                            progress: progress.clone(),
                        }),
                        t: l.t + impact.seconds + rad.delay_seconds,
                        radial_only: true,
                        n_pellets: 1,
                        rounds: Rounds::single(1, instance_each),
                        burst_starts: false,
                        burst_completed,
                        ..l.clone()
                    });
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
                    redirected: false,
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
        if path.impacts.is_empty() && !indirect && surface_blasts.is_empty() {
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
                redirected: false,
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

#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
fn launch_forked_beam(
    l: &Landing,
    active: &FightParams,
    me: &mut Combatant,
    scene: &crate::scene::Scene,
    eligible: &[bool],
    origins: &Option<Vec<Option<crate::scene::FireOrigin>>>,
    misses: Option<&[bool]>,
    head_weight: f64,
    all: f64,
    accuracy: crate::model::SpreadRuntime,
    burst_completed: Option<bool>,
    progress: Rc<Cell<Progress>>,
    events: &mut Vec<Landing>,
    rec: &mut crate::record::Record,
) {
    let charging = me.incarnon.in_base_form && !me.params.gauge_charging_body_parts.is_empty();
    let weakpoint_aim = if charging { 1. } else { scene.weakpoint_aim.unwrap_or(head_weight / all.max(1e-20)) };
    let head = me.d.aim.next_f64() < weakpoint_aim;
    let fire_origin = origins.as_ref().and_then(|o| o.first().copied().flatten());
    if misses.is_some_and(|m| m.first() == Some(&true)) {
        let path = scene.launch_flight(head, 0., 0., eligible, fire_origin);
        rec.push(l.t, None, crate::record::Kind::Miss { reason: "scenario miss" });
        rec.trace_spatial(l.t, l.shot, 0, path);
        return;
    }
    let mut spread = l.spread.expect("forked beam has spread");
    if let Some(scale) = accuracy.charge_attenuation {
        spread.min_deg *= f64::from(scale);
    }
    let pierce = if active.beam.is_some_and(|b| b.damage_radius_m > 0.) && !active.infinite_body_punch_through {
        0.
    } else {
        active.punch_through_m + l.pierce_live_m
    };
    let ray_count = l.blob_stacks.max(1);
    let per_ray_merge = l.beam_merge / f64::from(ray_count);
    let dt = (1.0 / l.live_rate.max(1e-4)) as f32;
    let live_accuracy = if active.live_accuracy {
        super::run::buff_total(active, crate::model::BuffGrant::Accuracy, &mut me.buff_stacks, l.t)
    } else {
        0.0
    };
    let mode_cone_deg = accuracy.beam_mode.map(|mode| {
        me.beam_spread_state.prepare_tick(
            active as *const FightParams,
            me.ammo.refills,
            mode,
            active.spread_scaling,
            spread.min_deg,
            live_accuracy,
        )
    });
    let mut paths = Vec::with_capacity(ray_count as usize);
    for ray in 0..ray_count {
        let (dev, phi) = if let (Some(mode), Some(cone_deg)) = (accuracy.beam_mode, mode_cone_deg) {
            me.beam_spread_state.sample_ray(
                mode,
                cone_deg,
                ray,
                dt,
                &mut me.d.aim,
                &mut me.d.blast_dir,
            )
        } else if spread.is_pinpoint() {
            (0., 0.)
        } else {
            spread.sample(
                me.d.aim.next_f64(),
                me.d.blast_dir.next_f64(),
                me.d.aim.next_f64(),
                ray,
                ray_count,
            )
        };
        paths.push(scene.trace(
            head, dev, phi, active.projectile_speed_mps, active.homing, pierce,
            active.infinite_body_punch_through, active.projectile_reverse_query,
            active.projectile_width_m, active.range_m, eligible, fire_origin,
        ));
    }
    if let Some(mode) = accuracy.beam_mode {
        me.beam_spread_state.finish_tick(mode, dt);
    }
    let forked = beam::forked_seeds(scene, &mut paths, active, eligible, me.tendril.count);
    let hit_any = !forked.direct.is_empty() || !forked.indirect.is_empty();
    for (contact, entry) in forked.direct.into_iter().enumerate() {
        events.push(Landing {
            spatial: Some(SpatialLanding {
                impact: entry.impact,
                miss: false,
                first: false,
                last: false,
                instance: 0,
                follows: false,
                contact,
                splash: false,
                beam: entry.seed,
                redirected: false,
                progress: progress.clone(),
            }),
            t: l.t + entry.impact.seconds,
            n_pellets: 1,
            rounds: Rounds::single(1, 1.),
            beam_merge: f64::from(entry.tags) * per_ray_merge,
            burst_starts: false,
            burst_completed,
            ..l.clone()
        });
    }
    for (impact, seed) in forked.indirect {
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
                redirected: false,
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
    if !hit_any {
        rec.push(
            l.t,
            None,
            crate::record::Kind::Miss {
                reason: "3d trajectory reached no enemy",
            },
        );
    }
    for (ray, path) in paths.into_iter().enumerate() {
        rec.trace_spatial(l.t, l.shot, ray as u32, path);
    }
}

#[wfsim_gas_macro::metered]
fn redirects(
    scene: &crate::scene::Scene, seed: crate::scene::Impact, rc: crate::model::Ricochet,
    eligible: &[bool], rng: &mut Rng,
) -> Vec<crate::scene::Impact> {
    let mut eligible = eligible.to_vec();
    let mut current = seed;
    let mut path = Vec::new();
    for _ in 0..rc.bounces {
        eligible[current.body] = false;
        let root = scene.root(current.body);
        let near = eligible.iter().enumerate().filter_map(|(body, &present)| {
            let gap = scene.root(body).minus(root).length();
            (present && gap <= rc.range_m).then_some((body, gap))
        }).min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((target, _)) = near else { break };
        let anchor = scene.target_anchor(target, rng.chance(rc.headshot_chance));
        let Some(direction) = anchor.minus(current.point).normalized() else { break };
        let reach = anchor.minus(current.point).length() + 1e-6;
        let Some(hit) = scene.contact_impact(current.point, direction, reach, 0., &eligible) else { break };
        path.push(hit);
        current = hit;
    }
    path
}

#[wfsim_gas_macro::metered]
fn lock_shot(
    scene: &crate::scene::Scene, active: &FightParams, lock: crate::model::LockOn,
    eligible: &[bool], head: bool, me: &mut Combatant, path: &mut crate::scene::Flight,
) {
    let forward = path.aim_target.minus(path.camera).normalized()
        .or_else(|| path.aim_target.minus(path.muzzle).normalized())
        .unwrap_or(path.direction);
    let candidates = scene.beam_acquire(path.muzzle, path.camera, forward, lock.acquire_deg, lock.range_m, eligible);
    let choices = &candidates[..candidates.len().min(lock.targets as usize)];
    if choices.is_empty() { return; }
    let round = me.ammo.rounds_this_mag.saturating_sub(1) as usize;
    let actor = choices[round % choices.len()].body;
    let gap = scene.distance_to_body(path.muzzle, actor);
    let Some((miss, head_keep)) = lock.at(gap, choices.len() as u32) else { return };
    path.steering_target = Some(actor);
    path.impacts.clear();
    path.stopped_at = None;
    if me.d.aim.chance(miss) { return; }
    let anchor = scene.target_anchor(actor, head && me.d.aim.chance(head_keep));
    let Some(dir) = anchor.minus(path.muzzle).normalized() else { return };
    path.direction = dir;
    if let Some(mut hit) = scene.contact_impact(path.muzzle, dir, active.range_m, active.projectile_width_m * 0.5, eligible) {
        hit.seconds = active.projectile_speed_mps.map_or(0., |speed| hit.travel / speed);
        path.stopped_at = Some(hit.point);
        path.impacts.push(hit);
    }
}
