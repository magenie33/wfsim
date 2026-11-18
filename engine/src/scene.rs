// SPDX-License-Identifier: AGPL-3.0-or-later
//! Posed player/weapon/enemy geometry consumed by the shared fight.
use crate::model::{Homing, Placement, Scene3d};
use crate::rules::geometry::{guided::Guided, Affine, Proxy, Vec3};

#[derive(Debug, Clone)]
pub struct Scene {
    input: Scene3d,
    muzzle: Vec3,
    camera: Vec3,
    bodies: Vec<Body>,
}

#[derive(Debug, Clone)]
struct Body {
    proxies: Vec<Prepared>,
    low: Vec3,
    high: Vec3,
    anchor: Vec3,
    center: Vec3,
}

#[derive(Debug, Clone)]
struct Prepared {
    proxy: Proxy,
    inverse: Affine,
    scale: f64,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Impact {
    pub body: usize,
    pub proxy: usize,
    pub part: u8,
    pub point: Vec3,
    pub exit: Vec3,
    pub seconds: f64,
    pub travel: f64,
    pub material: f64,
    pub remaining: f32,
    pub radial: bool,
    pub radial_distance: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Flight {
    pub muzzle: Vec3,
    pub camera: Vec3,
    pub head_anchor: Vec3,
    pub aim_target: Vec3,
    pub direction: Vec3,
    pub steering_target: Option<usize>,
    pub impacts: Vec<Impact>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FlightTrace {
    pub time: f64,
    pub shot: Option<u32>,
    pub round: u32,
    pub flight: Flight,
}

#[wfsim_gas_macro::metered]
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3([
        a.0[1] * b.0[2] - a.0[2] * b.0[1],
        a.0[2] * b.0[0] - a.0[0] * b.0[2],
        a.0[0] * b.0[1] - a.0[1] * b.0[0],
    ])
}

#[wfsim_gas_macro::metered]
fn controller_radius(c: crate::model::Controller) -> f32 {
    c.radius.max(0.0025) * c.scale
}

#[wfsim_gas_macro::metered]
fn uniform_scale(m: Affine) -> Result<f64, String> {
    if !m.0.iter().all(|v| v.is_finite()) {
        return Err("nonfinite scene transform".into());
    }
    let axes = [Vec3([1., 0., 0.]), Vec3([0., 1., 0.]), Vec3([0., 0., 1.])].map(|v| m.direction(v));
    let scale = axes[0].length();
    if scale <= 0.
        || axes
            .iter()
            .any(|v| (v.length() / scale - 1.).abs() > 0.0001)
        || [(0, 1), (0, 2), (1, 2)]
            .iter()
            .any(|&(a, b)| axes[a].dot(axes[b]).abs() > scale * scale * 0.0001)
    {
        return Err("scene_3d requires rigid transforms with uniform positive scale".into());
    }
    Ok(scale)
}

#[wfsim_gas_macro::metered]
fn controller_segment(root: Vec3, c: crate::model::Controller) -> (f64, f64) {
    let radius = controller_radius(c);
    let inner = (c.height - 2. * c.radius.max(0.0025)).max(0.01) * c.scale;
    let low = root.0[1] + f64::from(radius + c.skin);
    (low, low + f64::from(inner))
}

#[wfsim_gas_macro::metered]
impl Scene {
    pub fn input(&self) -> &Scene3d {
        &self.input
    }
    pub fn muzzle(&self) -> Vec3 {
        self.muzzle
    }
    pub fn camera(&self) -> Vec3 {
        self.camera
    }
    pub fn new(mut input: Scene3d, count: usize) -> Result<Self, String> {
        if input.enemies.len() != count || count == 0 || count > 361 {
            return Err("scene_3d needs one model per arena enemy (maximum 361)".into());
        }
        if !(0.001..=0.1).contains(&input.frame_seconds)
            || !(0.01..=10.0).contains(&input.maximum_flight_seconds)
        {
            return Err("scene_3d frame/flight duration is out of bounds".into());
        }
        if input.player.bones.is_empty()
            || input.player.bones.len() > 4096
            || !input.reference_point.0.iter().all(|v| v.is_finite())
        {
            return Err("scene_3d needs finite reference state and 1..4096 player bones".into());
        }
        for m in std::iter::once(&input.player.world)
            .chain(input.player.bones.iter())
            .chain([&input.player.muzzle.local, &input.camera.local])
        {
            uniform_scale(*m)?;
        }
        for (world, c) in std::iter::once((input.player.world, input.player.controller))
            .chain(input.enemies.iter().map(|e| (e.world, e.controller)))
        {
            let scale = uniform_scale(world)?;
            if (scale / f64::from(c.scale) - 1.).abs() > 0.0001 {
                return Err("controller and avatar world scale must agree".into());
            }
        }
        for c in std::iter::once(input.player.controller)
            .chain(input.enemies.iter().map(|b| b.controller))
        {
            if ![c.height, c.radius, c.skin, c.scale]
                .iter()
                .all(|v| v.is_finite())
                || c.height <= 0.
                || c.radius <= 0.
                || c.skin < 0.
                || c.scale <= 0.
            {
                return Err("invalid scene_3d controller dimensions".into());
            }
        }
        let root = input.player.world.point(Vec3([0.; 3]));
        if !matches!(input.placement, Placement::Supplied) {
            let old = input.enemies[0].world.point(Vec3([0.; 3]));
            if (old.0[1] - root.0[1]).abs() > 1e-6 {
                return Err("contact/distance placement currently requires upright controllers on one floor".into());
            }
            for world in [input.player.world, input.enemies[0].world] {
                let up = world
                    .direction(Vec3([0., 1., 0.]))
                    .normalized()
                    .ok_or("invalid controller up")?;
                if up.minus(Vec3([0., 1., 0.])).length() > 0.0001 {
                    return Err("automatic placement requires upright controllers".into());
                }
            }
            let a = controller_segment(root, input.player.controller);
            let b = controller_segment(old, input.enemies[0].controller);
            if a.0.max(b.0) > a.1.min(b.1) {
                return Err(
                    "automatic contact requires overlapping controller inner segments".into(),
                );
            }
            let distance = match input.placement {
                Placement::Contact => f64::from(
                    (controller_radius(input.player.controller)
                        + controller_radius(input.enemies[0].controller))
                        + input.player.controller.skin,
                ),
                Placement::CenterDistance { distance } if distance.is_finite() && distance > 0. => {
                    distance
                }
                _ => return Err("invalid center distance".into()),
            };
            let delta = old.minus(root);
            let horizontal = Vec3([delta.0[0], 0., delta.0[2]])
                .normalized()
                .ok_or("placement needs a facing direction")?;
            let at = root.along(horizontal, distance);
            let shift = at.minus(old);
            for body in &mut input.enemies {
                for i in 0..3 {
                    body.world.0[i * 4 + 3] += shift.0[i];
                }
            }
        }
        let mount = |m: &crate::model::Mount| -> Result<Vec3, String> {
            let bone = input
                .player
                .bones
                .get(m.bone)
                .ok_or("player mount bone is missing")?;
            Ok(input
                .player
                .world
                .compose(*bone)
                .compose(m.local)
                .point(Vec3([0.; 3])))
        };
        let muzzle = mount(&input.player.muzzle)?;
        let camera = mount(&input.camera)?;
        let mut bodies = Vec::new();
        for body in &input.enemies {
            if !body
                .candidate_anchor
                .0
                .iter()
                .chain(&body.steering_center.0)
                .all(|v| v.is_finite())
            {
                return Err("nonfinite scene target state".into());
            }
            if body.proxies.is_empty() || body.proxies.len() > 128 {
                return Err("scene body needs 1..128 posed proxies".into());
            }
            let (mut low, mut high) = (Vec3([f64::INFINITY; 3]), Vec3([f64::NEG_INFINITY; 3]));
            let mut proxies = Vec::new();
            for source in &body.proxies {
                let mut proxy = source.clone();
                proxy.world = body.world.compose(source.world);
                if !proxy.world.0.iter().all(|v| v.is_finite()) {
                    return Err("nonfinite model transform".into());
                }
                let inverse = proxy.world.inverse().ok_or("singular model transform")?;
                let scale = uniform_scale(proxy.world)?;
                let (radius, half) = match proxy.shape {
                    crate::rules::geometry::Shape::Sphere { radius } => (radius, 0.),
                    crate::rules::geometry::Shape::Capsule { radius, height } => {
                        (radius, height * 0.5)
                    }
                };
                if !radius.is_finite() || !half.is_finite() || radius <= 0. || half < 0. {
                    return Err("invalid proxy dimensions".into());
                }
                for y in [-half, half] {
                    let at = proxy.world.point(Vec3([0., y, 0.]));
                    for i in 0..3 {
                        low.0[i] = low.0[i].min(at.0[i] - radius * scale);
                        high.0[i] = high.0[i].max(at.0[i] + radius * scale);
                    }
                }
                proxies.push(Prepared {
                    proxy,
                    inverse,
                    scale,
                });
            }
            bodies.push(Body {
                proxies,
                low,
                high,
                anchor: body.world.point(body.candidate_anchor),
                center: body.world.point(body.steering_center),
            });
        }
        for i in [input.head_proxy, input.body_proxy] {
            if !bodies[0].proxies.iter().any(|p| p.proxy.cache_index == i) {
                return Err("aimed model is missing the selected aim proxy".into());
            }
        }
        if input.preselected_target.is_some_and(|i| i >= count) {
            return Err("launch target is outside scene".into());
        }
        Ok(Self {
            input,
            muzzle,
            camera,
            bodies,
        })
    }

    pub fn root(&self, index: usize) -> Vec3 {
        self.input.enemies[index].world.point(Vec3([0.; 3]))
    }

    fn broad(body: &Body, origin: Vec3, direction: Vec3, maximum: f64, width: f64) -> bool {
        let (mut enter, mut exit) = (0.0_f64, maximum);
        for i in 0..3 {
            if direction.0[i].abs() < 1e-15 {
                if origin.0[i] < body.low.0[i] - width || origin.0[i] > body.high.0[i] + width {
                    return false;
                }
            } else {
                let a = (body.low.0[i] - width - origin.0[i]) / direction.0[i];
                let b = (body.high.0[i] + width - origin.0[i]) / direction.0[i];
                enter = enter.max(a.min(b));
                exit = exit.min(a.max(b));
                if enter > exit {
                    return false;
                }
            }
        }
        true
    }

    fn hits(
        &self,
        origin: Vec3,
        direction: Vec3,
        maximum: f64,
        width: f64,
    ) -> Vec<(usize, usize, u8, f64, f64)> {
        let mut out = Vec::new();
        for (i, body) in self.bodies.iter().enumerate() {
            if !Self::broad(body, origin, direction, maximum, width) {
                continue;
            }
            let (mut enter, mut exit, mut index, mut part) = (f64::INFINITY, 0.0_f64, 0, 0);
            for p in &body.proxies {
                if !p.proxy.ray_enabled {
                    continue;
                }
                let shape = match p.proxy.shape {
                    crate::rules::geometry::Shape::Sphere { radius } => {
                        crate::rules::geometry::Shape::Sphere {
                            radius: radius + width / p.scale,
                        }
                    }
                    crate::rules::geometry::Shape::Capsule { radius, height } => {
                        crate::rules::geometry::Shape::Capsule {
                            radius: radius + width / p.scale,
                            height,
                        }
                    }
                };
                if let Some((a, b)) =
                    shape.interval(p.inverse.point(origin), p.inverse.direction(direction))
                {
                    if a < enter {
                        enter = a;
                        index = p.proxy.cache_index;
                        part = p.proxy.part;
                    }
                    exit = exit.max(b);
                }
            }
            if enter < maximum {
                out.push((i, index, part, enter, exit));
            }
        }
        out.sort_by(|a, b| a.3.total_cmp(&b.3).then(a.0.cmp(&b.0)));
        out
    }

    pub fn distance_to_body(&self, point: Vec3, index: usize) -> f64 {
        self.bodies[index]
            .proxies
            .iter()
            .filter(|p| p.proxy.ray_enabled)
            .map(|p| {
                let local = p.inverse.point(point);
                let (radius, half) = match p.proxy.shape {
                    crate::rules::geometry::Shape::Sphere { radius } => (radius, 0.),
                    crate::rules::geometry::Shape::Capsule { radius, height } => {
                        (radius, height * 0.5)
                    }
                };
                let d = Vec3([
                    local.0[0],
                    local.0[1] - local.0[1].clamp(-half, half),
                    local.0[2],
                ]);
                ((d.length() - radius) * p.scale).max(0.)
            })
            .fold(f64::INFINITY, f64::min)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn trace(
        &self,
        head: bool,
        deviation: f64,
        phase: f64,
        speed: Option<f64>,
        homing: Option<Homing>,
        pierce: f64,
        every_body: bool,
        width: f64,
        range: f64,
        eligible: &[bool],
    ) -> Flight {
        let selected = if head {
            self.input.head_proxy
        } else {
            self.input.body_proxy
        };
        let head_anchor = self.bodies[0]
            .proxies
            .iter()
            .find(|p| p.proxy.cache_index == selected)
            .unwrap()
            .proxy
            .world
            .point(Vec3([0.; 3]));
        let camera_direction = head_anchor
            .minus(self.camera)
            .normalized()
            .unwrap_or(Vec3([0., 0., 1.]));
        let aim_target = self
            .hits(
                self.camera,
                camera_direction,
                head_anchor.minus(self.camera).length() + 1e-6,
                0.,
            )
            .first()
            .map_or(head_anchor, |h| self.camera.along(camera_direction, h.3));
        let d = aim_target.minus(self.camera);
        let denom = d.dot(d);
        let reference = self.input.reference_point;
        let fraction = if denom > 0. {
            reference.minus(self.camera).dot(d) / denom
        } else {
            0.
        };
        let eye = self.camera.along(d, fraction.clamp(0., 1.));
        let forward = aim_target
            .minus(eye)
            .normalized()
            .unwrap_or(camera_direction);
        let right = cross(Vec3([0., 1., 0.]), forward)
            .normalized()
            .unwrap_or(Vec3([1., 0., 0.]));
        let up = cross(forward, right);
        let (s, c) = libm::sincos(deviation.to_radians());
        let (sp, cp) = libm::sincos(phase * std::f64::consts::TAU);
        let direction = Vec3(std::array::from_fn(|i| {
            forward.0[i] * c + right.0[i] * s * cp + up.0[i] * s * sp
        }));
        let mut flight = Flight {
            muzzle: self.muzzle,
            camera: self.camera,
            head_anchor,
            aim_target,
            direction,
            steering_target: self.input.preselected_target,
            impacts: Vec::new(),
        };
        let maximum = pierce.max(0.) as f32;
        let mut accumulated = 0.0_f32;
        let mut visited = vec![false; self.bodies.len()];
        let mut traveled = 0.;
        let frame_seconds = self.input.frame_seconds;
        let initial = homing.map_or(speed.unwrap_or(100_000.) as f32, |h| h.initial_speed);
        let mut state = Guided::new(
            self.muzzle.0.map(|v| v as f32),
            direction.0.map(|v| v as f32),
            initial,
        );
        let limit = if homing.is_some() {
            (self.input.maximum_flight_seconds / frame_seconds).ceil() as usize
        } else {
            1
        };
        for step in 0..limit {
            let origin = Vec3(state.position.map(f64::from));
            let (line, length, elapsed) = if let Some(h) = homing {
                if flight
                    .steering_target
                    .is_some_and(|i| eligible.get(i) != Some(&true))
                {
                    flight.steering_target = None;
                }
                if flight.steering_target.is_none() && state.flight > h.minimum_flight_before_seek {
                    let current = Vec3(state.velocity.map(f64::from))
                        .normalized()
                        .unwrap_or(direction);
                    flight.steering_target = self
                        .bodies
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| eligible.get(*i) == Some(&true))
                        .filter_map(|(i, b)| {
                            let to = b.anchor.minus(origin);
                            let distance = to.length();
                            let angle = libm::acos(current.dot(to.normalized()?).clamp(-1., 1.))
                                .to_degrees();
                            let visible = self
                                .hits(origin, to.normalized()?, distance + 1e-6, 0.)
                                .first()
                                .is_none_or(|hit| hit.0 == i);
                            (visible
                                && distance <= f64::from(h.seek_distance)
                                && angle <= f64::from(h.seek_cone_deg))
                            .then_some((i, distance))
                        })
                        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
                        .map(|v| v.0);
                }
                let target = flight
                    .steering_target
                    .map(|i| self.bodies[i].center.0.map(|v| v as f32));
                state.step(
                    target,
                    frame_seconds as f32,
                    h.turning_seconds,
                    h.acceleration,
                    h.maximum_speed,
                );
                let delta = Vec3(state.position.map(f64::from)).minus(origin);
                let length = delta.length();
                (
                    delta.normalized().unwrap_or(direction),
                    length,
                    step as f64 * frame_seconds,
                )
            } else {
                (
                    direction,
                    range.min(speed.unwrap_or(100_000.) * self.input.maximum_flight_seconds),
                    0.,
                )
            };
            let query_max = length.min((range - traveled).max(0.));
            let radius = homing
                .map_or(0., |h| f64::from(h.collision_radius))
                .max(width * 0.5);
            for (body, proxy, part, enter, exit) in self.hits(origin, line, query_max, radius) {
                if visited[body] || eligible.get(body) != Some(&true) {
                    continue;
                }
                visited[body] = true;
                let point = origin.along(line, enter);
                let end = origin.along(line, exit);
                let material = end.minus(point).length();
                let seconds = if homing.is_some() {
                    elapsed + frame_seconds * enter / length
                } else {
                    speed.map_or(0., |v| enter / v)
                };
                let crossing = crate::rules::geometry::penetration::traverse(
                    point.0.map(|v| v as f32),
                    body as u64 + 1,
                    &[crate::rules::geometry::penetration::ReverseHit {
                        reference: body as u64 + 1,
                        point: end.0.map(|v| v as f32),
                    }],
                    crate::rules::geometry::penetration::Budget {
                        maximum,
                        accumulated,
                        infinite: every_body,
                    },
                );
                accumulated = crossing.accumulated;
                let remaining = (maximum - accumulated).max(0.);
                flight.impacts.push(Impact {
                    body,
                    proxy,
                    part,
                    point,
                    exit: end,
                    seconds,
                    travel: traveled + enter,
                    material,
                    remaining,
                    radial: false,
                    radial_distance: 0.,
                });
                if !crossing.continues {
                    return flight;
                }
            }
            traveled += query_max;
            if traveled >= range || homing.is_none() {
                break;
            }
        }
        flight
    }
}

#[cfg(test)]
mod tests;
