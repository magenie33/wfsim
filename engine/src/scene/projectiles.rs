// SPDX-License-Identifier: AGPL-3.0-or-later
use super::{BodyHit, DetonationTrace, Flight, Impact, Scene, SurfaceBend, Vec3};
use crate::rules::geometry::penetration::{ReverseHit, projectile_exit, reverse_start};

#[wfsim_gas_macro::metered]
impl Scene {
    /// An explosive projectile follows each reflected leg until a blocking
    /// contact spends its final rebound or the flight timer expires.
    pub(crate) fn trace_rebound_blast(
        &self, path: &mut Flight, model: crate::model::ReboundBlast,
        mut speed: f64, width: f64, eligible: &[bool],
    ) -> usize {
        let floor = self.input.player.world.0[7];
        let Some(mut direction) = path.direction.normalized() else { return 0 };
        let mut from = path.muzzle;
        let mut seconds = 0.;
        let mut travel = 0.;
        let mut direct_before_reflection = None;
        for remaining in (0..=model.reflections).rev() {
            let reach = speed * (model.lifetime_seconds - seconds).max(0.);
            let ground = floor_distance(from, direction, floor);
            let hit = self.contact_impact(from, direction, reach.min(ground), width, eligible);
            let floor_contact = hit.is_none() && ground <= reach;
            let leg = hit.as_ref().map_or(reach.min(ground), |h| h.travel);
            let point = from.along(direction, leg);
            seconds = if speed > 0. { seconds + leg / speed } else { model.lifetime_seconds };
            travel += leg;
            if let Some(hit) = hit {
                path.impacts.push(Impact { seconds, travel, ..hit });
            }
            let expired = hit.is_none() && !floor_contact;
            path.detonations.push(DetonationTrace {
                point, seconds, travel, body: hit.map(|h| h.body), expired,
            });
            path.stopped_at = Some(point);
            if expired || remaining == 0 { break; }
            let normal = match hit {
                Some(h) => self.surface_normal(h),
                None => Some(Vec3([0., 1., 0.])),
            };
            let Some(normal) = normal else { break };
            direct_before_reflection.get_or_insert(path.impacts.len());
            direction = direction.along(normal, -2. * direction.dot(normal));
            path.bends.push(SurfaceBend { point, normal, direction, travel });
            speed *= model.restitution;
            from = point.along(direction, 1e-6);
        }
        direct_before_reflection.unwrap_or(path.impacts.len())
    }

    /// Reflect a kinematic shot off the open-arena floor or a blocking
    /// proxy. Equal normal/tangent restitution preserves mirror direction.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn reflect_path(
        &self, path: &mut Flight, bounces: u32, speed: Option<f64>, width: f64,
        range: f64, pierce: f64, eligible: &[bool],
    ) -> usize {
        let end = range.min(speed.unwrap_or(100_000.) * self.input.maximum_flight_seconds);
        let floor = self.input.player.world.0[7];
        let Some(mut direction) = path.direction.normalized() else { return path.impacts.len() };
        let first_floor = floor_distance(path.muzzle, direction, floor);
        let stop = path.stopped_at.and_then(|at| path.impacts.last().copied().filter(|h| h.point == at));
        let stopping = stop.filter(|h| h.travel <= first_floor);
        path.impacts.retain(|h| h.travel <= first_floor);
        path.stopped_at = stopping.map(|h| h.point);
        let first = path.impacts.len();
        let (mut from, mut travel, mut normal) = if let Some(hit) = stopping {
            let Some(normal) = self.surface_normal(hit) else { return first };
            (hit.point, hit.travel, normal)
        } else if first_floor < end {
            (path.muzzle.along(direction, first_floor), first_floor, Vec3([0., 1., 0.]))
        } else {
            return first;
        };
        let mut eligible = eligible.to_vec();
        let mut budget = path.impacts.last().map_or(pierce as f32, |h| h.remaining);
        for hit in &path.impacts {
            eligible[hit.body] = false;
        }
        for _ in 0..bounces {
            direction = direction.along(normal, -2. * direction.dot(normal));
            path.bends.push(SurfaceBend { point: from, normal, direction, travel });
            // Step past the contact to avoid querying that surface at zero.
            from = from.along(direction, 1e-6);
            travel += 1e-6;
            let reach = end - travel;
            if reach <= 0. { break; }
            let step_to_floor = floor_distance(from, direction, floor);
            let limit = reach.min(step_to_floor);
            let mut first_blocking = None;
            for BodyHit { body, proxy, part, enter, exit } in Self::hits(
                &self.bodies, &self.bounds, super::Ray { origin: from, direction, maximum: limit, width },
            ) {
                if !eligible[body] { continue; }
                let point = from.along(direction, enter);
                let leaving = from.along(direction, exit);
                let thickness = point.minus(leaving).length();
                let material = thickness as f32;
                let passes = budget > 0. && material <= budget;
                let hit = Impact {
                    body, proxy: self.bodies[body].proxies[proxy].proxy.cache_index, part,
                    point, exit: leaving, travel: travel + enter,
                    seconds: speed.map_or(0., |v| (travel + enter) / v),
                    material: thickness, remaining: (budget - material).max(0.),
                    radial: false, radial_distance: 0.,
                };
                path.impacts.push(hit);
                eligible[body] = false;
                budget = hit.remaining;
                if !passes {
                    first_blocking = Some(hit);
                    break;
                }
            }
            if let Some(hit) = first_blocking {
                let Some(next_normal) = self.surface_normal(hit) else { break };
                (from, travel, normal) = (hit.point, hit.travel, next_normal);
            } else if step_to_floor < reach {
                from = from.along(direction, step_to_floor);
                travel += step_to_floor;
                normal = Vec3([0., 1., 0.]);
            } else {
                break;
            }
        }
        first
    }

    fn surface_normal(&self, hit: Impact) -> Option<Vec3> {
        let shape = self.bodies[hit.body].proxies.iter().find(|p| p.proxy.cache_index == hit.proxy)?;
        let local = shape.collider.inverse.point(hit.point);
        let (_, half) = shape.collider.shape.radius_and_half_height();
        let local_normal = Vec3([local.0[0], local.0[1] - local.0[1].clamp(-half, half), local.0[2]]);
        shape.collider.inverse.inverse()?.direction(local_normal).normalized()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn trace_kinematic(
        &self,
        flight: &mut Flight,
        line: Vec3,
        range: f64,
        width: f64,
        speed: Option<f64>,
        mut remaining: f32,
        eligible: &[bool],
    ) {
        let mut origin = flight.muzzle;
        let mut travel = 0.;
        let mut seen = vec![false; self.bodies.len()];
        let direction = line.0.map(|v| v as f32);
        while let Some(BodyHit { body, proxy: slot, part, enter, .. }) =
            self.first_hit(origin, line, range - travel, width, |body| {
                eligible.get(body) == Some(&true) && !seen[body]
            })
        {
            let point = origin.along(line, enter);
            let anchor = point.0.map(|v| v as f32);
            let proxy = &self.bodies[body].proxies[slot];
            let crossing = if remaining > 0. {
                let start = Vec3(reverse_start(anchor, direction, remaining).map(f64::from));
                let delta = Vec3(anchor.map(f64::from)).minus(start);
                let collider = &proxy.collider;
                // One installed collider per supplied handle. Other handles cannot match it.
                let record = collider
                    .shape
                    .intersect(
                        collider.inverse.point(start),
                        collider.inverse.direction(delta),
                    )
                    .filter(|&t| t <= 1.)
                    .map(|t| ReverseHit {
                        reference: 1,
                        point: start.along(delta, t).0.map(|v| v as f32),
                    });
                projectile_exit(anchor, direction, 1, record.as_slice(), remaining)
            } else {
                None
            };
            let (exit, material) = crossing.as_ref().map_or((point, 0.), |hit| {
                (Vec3(hit.point.map(f64::from)), f64::from(hit.material))
            });
            if let Some(hit) = &crossing {
                remaining = hit.remaining;
            }
            let entered = travel + enter;
            flight.impacts.push(Impact {
                body,
                proxy: proxy.proxy.cache_index,
                part,
                point,
                exit,
                seconds: speed.map_or(0., |v| entered / v),
                travel: entered,
                material,
                remaining,
                radial: false,
                radial_distance: 0.,
            });
            let Some(hit) = crossing else { break };
            seen[body] = true;
            let resume = Vec3(hit.resume.map(f64::from));
            travel = entered + resume.minus(point).dot(line).max(0.);
            origin = resume;
        }
    }

    /// Open-scene radial routing after the projectile submits its published position.
    pub fn blast_hits(
        &self,
        direct: Impact,
        center: Vec3,
        radius: f64,
        eligible: &[bool],
    ) -> Vec<Impact> {
        let mut candidates = Vec::new();
        for (i, body) in self.bodies.iter().enumerate() {
            if eligible.get(i) != Some(&true) {
                continue;
            }
            let (overlap, low, high) = if let Some(p) = &body.radial_proxy {
                (p.distance(center) <= radius, p.low, p.high)
            } else {
                (
                    self.distance_to_body(center, i) <= radius,
                    body.low,
                    body.high,
                )
            };
            if !overlap {
                continue;
            }
            let aabb_distance = (0..3)
                .map(|axis| {
                    let offset = (low.0[axis] - center.0[axis])
                        .max(center.0[axis] - high.0[axis])
                        .max(0.);
                    offset * offset
                })
                .sum::<f64>();
            candidates.push((i, aabb_distance));
        }
        candidates.sort_by(|a, b| a.1.total_cmp(&b.1));
        candidates
            .into_iter()
            .map(|(body, _)| {
                // Native 4EB3F0 selects the nearest attachment reference in cache order.
                let (proxy, point, distance_sq) = self.bodies[body]
                    .proxies
                    .iter()
                    .map(|p| {
                        let point = p.center;
                        let delta = point.minus(center);
                        (&p.proxy, point, delta.dot(delta))
                    })
                    .min_by(|a, b| a.2.total_cmp(&b.2))
                    .unwrap();
                Impact {
                    body,
                    proxy: proxy.cache_index,
                    part: proxy.part,
                    point,
                    exit: point,
                    material: 0.,
                    remaining: 0.,
                    radial: true,
                    radial_distance: libm::sqrt(distance_sq),
                    ..direct
                }
            })
            .collect()
    }
}

#[wfsim_gas_macro::metered]
fn floor_distance(from: Vec3, direction: Vec3, floor: f64) -> f64 {
    if direction.0[1] >= 0. || from.0[1] < floor {
        f64::INFINITY
    } else {
        (floor - from.0[1]) / direction.0[1]
    }
}
