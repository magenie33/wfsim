// SPDX-License-Identifier: AGPL-3.0-or-later
//! Posed player/weapon/enemy geometry consumed by the shared fight.
use crate::model::{GuidedUpdate, Homing, Placement, Scene3d};
use crate::rules::geometry::{eye::{EyeAim, eye_aim}, glaive, guided::Guided, rotation, Affine, Proxy, Shape, Vec3};

mod attachment;
mod projectiles;
pub(crate) use attachment::FireState;
pub use attachment::FireOrigin;

#[derive(Debug, Clone)]
pub struct Scene {
    input: Scene3d,
    muzzle: Vec3,
    camera: Vec3,
    avatar_up: [f32; 4],
    reticles: [Reticle; 2],
    bodies: Vec<Body>,
    bounds: Vec<BoundsNode>,
    equipped_fire: Option<attachment::PreparedFire>,
}

#[derive(Debug, Clone)]
struct Reticle {
    anchor: Vec3,
    direction: Vec3,
    hits: Vec<(usize, EyeAim)>,
    fallback: EyeAim,
}

#[derive(Debug, Clone)]
struct Body {
    proxies: Vec<Prepared>,
    radial_proxy: Option<Collider>,
    low: Vec3,
    high: Vec3,
    root: Vec3,
    anchor: Vec3,
    center: [f32; 3],
}

#[derive(Debug, Clone)]
struct Prepared {
    proxy: Proxy,
    collider: Collider,
    center: Vec3,
}

#[derive(Debug, Clone, Copy)]
struct Ray {
    origin: Vec3,
    direction: Vec3,
    maximum: f64,
    width: f64,
}

#[derive(Debug, Clone)]
struct BoundsNode {
    low: Vec3,
    high: Vec3,
    body: Option<usize>,
    escape: usize,
}

#[wfsim_gas_macro::metered]
impl BoundsNode {
    fn build(bodies: &[Body]) -> Vec<Self> {
        // Keep the direct scan for scenes too small to amortize the tree.
        if bodies.len() < 8 || bodies.iter().any(|body| {
            !(0..3).all(|i| body.low.0[i].is_finite() && body.high.0[i].is_finite()
                && body.low.0[i] <= body.high.0[i])
        }) {
            return Vec::new();
        }
        let mut indices = (0..bodies.len()).collect::<Vec<_>>();
        let mut nodes = Vec::with_capacity(bodies.len() * 2 - 1);
        Self::append(&mut nodes, &mut indices, bodies);
        nodes
    }

    // Depth-first layout skips a rejected subtree with a single index.
    fn append(nodes: &mut Vec<Self>, indices: &mut [usize], bodies: &[Body]) {
        let (mut low, mut high) = (bodies[indices[0]].low, bodies[indices[0]].high);
        for &index in &indices[1..] {
            for axis in 0..3 {
                low.0[axis] = low.0[axis].min(bodies[index].low.0[axis]);
                high.0[axis] = high.0[axis].max(bodies[index].high.0[axis]);
            }
        }
        let node = nodes.len();
        let body = (indices.len() == 1).then_some(indices[0]);
        nodes.push(Self { low, high, body, escape: node + 1 });
        if body.is_none() {
            let span = high.minus(low);
            let axis = (0..3).max_by(|&a, &b| span.0[a].total_cmp(&span.0[b])).unwrap();
            indices.sort_unstable_by(|&a, &b| {
                let center = |i: usize| bodies[i].low.0[axis] * 0.5 + bodies[i].high.0[axis] * 0.5;
                center(a).total_cmp(&center(b)).then(a.cmp(&b))
            });
            let (left, right) = indices.split_at_mut(indices.len() / 2);
            Self::append(nodes, left, bodies);
            Self::append(nodes, right, bodies);
            nodes[node].escape = nodes.len();
        }
    }
}

#[wfsim_gas_macro::metered]
impl Ray {
    fn reaches(self, low: Vec3, high: Vec3) -> bool {
        let (mut enter, mut exit) = (0.0_f64, self.maximum);
        for i in 0..3 {
            if self.direction.0[i].abs() < 1e-15 {
                if self.origin.0[i] < low.0[i] - self.width || self.origin.0[i] > high.0[i] + self.width {
                    return false;
                }
            } else {
                let a = (low.0[i] - self.width - self.origin.0[i]) / self.direction.0[i];
                let b = (high.0[i] + self.width - self.origin.0[i]) / self.direction.0[i];
                enter = enter.max(a.min(b));
                exit = exit.min(a.max(b));
                if enter > exit {
                    return false;
                }
            }
        }
        true
    }
}

#[derive(Debug, Clone)]
struct Collider {
    shape: Shape,
    inverse: Affine,
    scale: f64,
    low: Vec3,
    high: Vec3,
}

#[derive(Debug, Clone, Copy)]
struct BodyHit {
    body: usize,
    proxy: usize,
    part: u8,
    enter: f64,
    exit: f64,
}

#[wfsim_gas_macro::metered]
impl BodyHit {
    fn cmp_entry(&self, other: &Self) -> std::cmp::Ordering {
        self.enter.total_cmp(&other.enter).then(self.body.cmp(&other.body))
    }
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
    pub basis_origin: Vec3,
    pub head_anchor: Vec3,
    pub aim_target: Vec3,
    pub direction: Vec3,
    pub steering_target: Option<usize>,
    pub impacts: Vec<Impact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blast_center: Option<Vec3>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub radial_impacts: Vec<Impact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beam_endpoint: Option<Vec3>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub beam_contacts: Vec<BeamContact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub redirects: Vec<Impact>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bends: Vec<SurfaceBend>,
    #[serde(skip)]
    pub(crate) stopped_at: Option<Vec3>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fire_origin: Option<FireOrigin>,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct SurfaceBend {
    pub point: Vec3,
    pub normal: Vec3,
    pub direction: Vec3,
    pub travel: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BeamContactKind {
    Radius,
    Acquired,
    Chain,
    Tendril,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct BeamContact {
    pub kind: BeamContactKind,
    pub from: Vec3,
    pub impact: Impact,
    pub share: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FlightTrace {
    pub time: f64,
    pub shot: Option<u32>,
    pub round: u32,
    pub flight: Flight,
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
impl Collider {
    fn new(shape: Shape, world: Affine) -> Result<Self, String> {
        let scale = uniform_scale(world)?;
        let inverse = world.inverse().ok_or("singular model transform")?;
        let (radius, half) = shape.radius_and_half_height();
        if !radius.is_finite() || !half.is_finite() || radius <= 0. || half < 0. {
            return Err("invalid proxy dimensions".into());
        }
        let [a, b] = [-half, half].map(|y| world.point(Vec3([0., y, 0.])));
        let low = Vec3(std::array::from_fn(|i| a.0[i].min(b.0[i]) - radius * scale));
        let high = Vec3(std::array::from_fn(|i| a.0[i].max(b.0[i]) + radius * scale));
        Ok(Self { shape, inverse, scale, low, high })
    }

    fn distance(&self, point: Vec3) -> f64 {
        let (radius, half) = self.shape.radius_and_half_height();
        let Vec3([x, y, z]) = self.inverse.point(point);
        let offset = Vec3([x, y - y.clamp(-half, half), z]);
        (offset.length() - radius).max(0.) * self.scale
    }
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
        if input.guided_updates.len() > 10_000 {
            return Err("scene_3d supports up to 10,000 supplied projectile updates".into());
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
        let mount = |m: &crate::model::Mount| -> Result<Affine, String> {
            let bone = input
                .player
                .bones
                .get(m.bone)
                .ok_or("player mount bone is missing")?;
            Ok(input.player.world.compose(*bone).compose(m.local))
        };
        let muzzle = mount(&input.player.muzzle)?.point(Vec3([0.; 3]));
        let camera = mount(&input.camera)?;
        let equipped_fire = input.player.equipped_fire.as_ref()
            .map(|equipment| attachment::PreparedFire::new(equipment, mount))
            .transpose()?;
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
                let collider = Collider::new(proxy.shape, proxy.world)?;
                for i in 0..3 {
                    low.0[i] = low.0[i].min(collider.low.0[i]);
                    high.0[i] = high.0[i].max(collider.high.0[i]);
                }
                let center = proxy.world.point(Vec3([0.; 3]));
                proxies.push(Prepared { proxy, collider, center });
            }
            let radial_proxy = body.radial_proxy.map(|p| {
                Collider::new(p.shape, body.world.compose(p.local))
            }).transpose()?;
            bodies.push(Body {
                proxies,
                radial_proxy,
                low,
                high,
                root: body.world.point(Vec3([0.; 3])),
                anchor: body.world.point(body.candidate_anchor),
                center: body.world.point(body.steering_center).0.map(|v| v as f32),
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
        let bounds = BoundsNode::build(&bodies);
        let (reticles, avatar_up) = Self::prepare_reticles(&input, camera, &bodies, &bounds);
        Ok(Self {
            input,
            muzzle,
            camera: camera.point(Vec3([0.; 3])),
            avatar_up,
            reticles,
            bodies,
            bounds,
            equipped_fire,
        })
    }

    pub fn root(&self, index: usize) -> Vec3 {
        self.bodies[index].root
    }

    fn intersect_body(body: &Body, index: usize, ray: Ray) -> Option<BodyHit> {
        if !ray.reaches(body.low, body.high) {
            return None;
        }
        let (mut enter, mut exit, mut proxy, mut part) = (f64::INFINITY, 0.0_f64, 0, 0);
        for (slot, p) in body.proxies.iter().enumerate() {
            if !p.proxy.ray_enabled {
                continue;
            }
            let expansion = ray.width / p.collider.scale;
            let shape = match p.proxy.shape {
                Shape::Sphere { radius } => Shape::Sphere { radius: radius + expansion },
                Shape::Capsule { radius, height } => Shape::Capsule { radius: radius + expansion, height },
            };
            if let Some((a, b)) =
                shape.interval(p.collider.inverse.point(ray.origin), p.collider.inverse.direction(ray.direction))
            {
                if a < enter {
                    enter = a;
                    proxy = slot;
                    part = p.proxy.part;
                }
                exit = exit.max(b);
            }
        }
        (enter < ray.maximum).then_some(BodyHit { body: index, proxy, part, enter, exit })
    }

    fn visit_hits(
        bodies: &[Body],
        bounds: &[BoundsNode],
        ray: Ray,
        accepts: impl Fn(usize) -> bool,
        mut hit: impl FnMut(BodyHit),
    ) {
        let mut body = |index| {
            if !accepts(index) {
                return;
            }
            if let Some(contact) = Self::intersect_body(&bodies[index], index, ray) {
                hit(contact);
            }
        };
        if bounds.is_empty() || ray.width < 0. {
            for index in 0..bodies.len() {
                body(index);
            }
            return;
        }
        let mut index = 0;
        while let Some(node) = bounds.get(index) {
            if let Some(actor) = node.body {
                body(actor);
                index += 1;
            } else if ray.reaches(node.low, node.high) {
                index += 1;
            } else {
                index = node.escape;
            }
        }
    }

    fn hits(bodies: &[Body], bounds: &[BoundsNode], ray: Ray) -> Vec<BodyHit> {
        let mut out = Vec::new();
        Self::visit_hits(bodies, bounds, ray, |_| true, |hit| out.push(hit));
        out.sort_by(BodyHit::cmp_entry);
        out
    }

    fn first_hit(
        &self,
        origin: Vec3,
        direction: Vec3,
        maximum: f64,
        width: f64,
        accepts: impl Fn(usize) -> bool,
    ) -> Option<BodyHit> {
        let ray = Ray { origin, direction, maximum, width };
        let mut best = None;
        Self::visit_hits(&self.bodies, &self.bounds, ray, accepts, |hit| {
            if best.as_ref().is_none_or(|current| hit.cmp_entry(current).is_lt()) {
                best = Some(hit);
            }
        });
        best
    }

    // Poses and ordered camera intersections are fixed. Only actor eligibility
    // and the head/body choice change when a round is fired.
    fn prepare_reticles(
        input: &Scene3d,
        camera: Affine,
        bodies: &[Body],
        bounds: &[BoundsNode],
    ) -> ([Reticle; 2], [f32; 4]) {
        let axis = |transform: Affine, direction| {
            transform
                .direction(Vec3(direction))
                .normalized()
                .unwrap_or(Vec3(direction))
                .0
                .map(|v| v as f32)
        };
        let camera_forward = axis(camera, [0., 0., 1.]);
        let avatar_forward = axis(input.player.world, [0., 0., 1.]);
        let [x, y, z] = axis(input.player.world, [0., 1., 0.]);
        let avatar_up = [x, y, z, 0.];
        let camera = camera.point(Vec3([0.; 3]));
        let solve = |target: Vec3| {
            eye_aim(
                camera.0.map(|v| v as f32),
                camera_forward,
                target.0.map(|v| v as f32),
                input.reference_point.0.map(|v| v as f32),
                avatar_forward,
                avatar_up,
            )
        };
        let reticles = [input.body_proxy, input.head_proxy].map(|selected| {
            let anchor = bodies[0]
                .proxies
                .iter()
                .find(|p| p.proxy.cache_index == selected)
                .unwrap()
                .center;
            let offset = anchor.minus(camera);
            let direction = offset.normalized().unwrap_or(Vec3([0., 0., 1.]));
            let ray = Ray { origin: camera, direction, maximum: offset.length() + 1e-6, width: 0. };
            let hits = Self::hits(bodies, bounds, ray)
                .into_iter()
                .map(|h| (h.body, solve(camera.along(direction, h.enter))))
                .collect();
            Reticle { anchor, direction, hits, fallback: solve(anchor) }
        });
        (reticles, avatar_up)
    }

    pub fn distance_to_body(&self, point: Vec3, index: usize) -> f64 {
        self.bodies[index]
            .proxies
            .iter()
            .filter(|p| p.proxy.ray_enabled)
            .map(|p| p.collider.distance(point))
            .fold(f64::INFINITY, f64::min)
    }

    pub(crate) fn distance_to_radial_body(&self, point: Vec3, index: usize) -> f64 {
        self.bodies[index].radial_proxy.as_ref().map_or_else(
            || self.distance_to_body(point, index), |proxy| proxy.distance(point),
        )
    }

    pub(crate) fn contact_distance(
        &self, start: Vec3, direction: Vec3, reach: f64, eligible: &[bool],
    ) -> Option<f64> {
        self.first_hit(start, direction, reach, 0., |i| eligible.get(i) == Some(&true))
            .map(|hit| hit.enter)
    }

    pub(crate) fn contact_impact(
        &self, start: Vec3, direction: Vec3, reach: f64, width: f64, eligible: &[bool],
    ) -> Option<Impact> {
        let hit = self.first_hit(start, direction, reach, width, |i| eligible.get(i) == Some(&true))?;
        let point = start.along(direction, hit.enter);
        let exit = start.along(direction, hit.exit);
        Some(Impact {
            body: hit.body, part: hit.part, proxy: self.bodies[hit.body].proxies[hit.proxy].proxy.cache_index,
            point, exit, travel: hit.enter, material: exit.minus(point).length(),
            seconds: 0., remaining: 0., radial: false, radial_distance: 0.,
        })
    }

    pub(crate) fn target_anchor(&self, actor: usize, head: bool) -> Vec3 {
        let center = self.beam_center(actor);
        self.bodies[actor].proxies.iter()
            .find(|p| p.proxy.ray_enabled && p.proxy.part == u8::from(head))
            .map_or(center, |p| p.center)
    }

    pub(crate) fn beam_center(&self, index: usize) -> Vec3 {
        Vec3(self.bodies[index].center.map(f64::from))
    }

    /// Open-arena acquisition uses root reach and the angle to the posed center.
    pub(crate) fn beam_acquire(
        &self, origin: Vec3, line: Vec3, cone_deg: f64, range: f64, eligible: &[bool],
    ) -> Vec<Impact> {
        let dot_limit = libm::cos(cone_deg.to_radians());
        let player = self.input.player.world.point(Vec3([0.; 3]));
        let mut candidates: Vec<_> = (0..self.bodies.len())
            .filter(|&i| eligible.get(i) == Some(&true) && self.root(i).minus(player).length() <= range)
            .filter_map(|i| {
                let toward = self.beam_center(i).minus(origin).normalized()?;
                let score = toward.dot(line);
                (score >= dot_limit).then_some((score, i))
            })
            .collect();
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
        candidates.into_iter().map(|(_, i)| self.beam_recipient(i, origin)).collect()
    }

    /// Indirect beams use a body attachment. Only the primary ray supplies a
    /// weak-point hit. Use the posed center when no body attachment is tagged.
    pub(crate) fn beam_recipient(&self, body: usize, from: Vec3) -> Impact {
        let center = self.beam_center(body);
        let proxy = self.bodies[body].proxies.iter()
            .filter(|p| p.proxy.ray_enabled && p.proxy.part != 1)
            .min_by(|a, b| a.center.minus(center).length().total_cmp(&b.center.minus(center).length()));
        let point = proxy.map_or(center, |p| p.center);
        Impact {
            body, proxy: proxy.map_or(0, |p| p.proxy.cache_index), part: 0, point, exit: point,
            seconds: 0., travel: point.minus(from).length(), material: 0., remaining: 0.,
            radial: false, radial_distance: 0.,
        }
    }

    /// The reload fan uses the modeled throw clock and reticle reach. Live
    /// installed release pose and grenade gravity are separate inputs.
    pub(crate) fn reload_throw(
        &self, head: bool, yaw_deg: f64, eligible: &[bool],
    ) -> (Flight, Vec3) {
        let reticle = &self.reticles[usize::from(head)];
        let aim = reticle.hits.iter()
            .find(|(body, _)| eligible.get(*body) == Some(&true))
            .map_or(&reticle.fallback, |(_, aim)| aim);
        let target = Vec3(aim.target.map(f64::from));
        let reach = target.minus(self.muzzle).length() + 1e-6;
        let phase = if yaw_deg.is_sign_negative() { 0.5 } else { 0. };
        let flight = self.trace(head, yaw_deg.abs(), phase, None, None, 0., false, false, 0., reach, eligible, None);
        let line = flight.direction.normalized().unwrap_or(reticle.direction);
        let endpoint = flight.stopped_at.unwrap_or_else(|| flight.muzzle.along(line, reach));
        (flight, endpoint)
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
        projectile_reverse_query: bool,
        width: f64,
        range: f64,
        eligible: &[bool],
        fire_origin: Option<FireOrigin>,
    ) -> Flight {
        let reticle = &self.reticles[usize::from(head)];
        let aim = reticle.hits.iter()
            .find(|(body, _)| eligible.get(*body) == Some(&true))
            .map_or(&reticle.fallback, |(_, aim)| aim);
        let muzzle = fire_origin.map_or(self.muzzle, |fire| fire.position);
        let aim_target = Vec3(aim.target.map(f64::from));
        let eye_forward = Vec3(std::array::from_fn(|i| f64::from(aim.basis[8 + i])));
        let [x, y, z] = muzzle.0.map(|v| v as f32);
        let to_target = aim_target.minus(Vec3([x, y, z].map(f64::from)));
        let launch_basis = if to_target.dot(eye_forward) > 0.
            && to_target.dot(to_target) > f64::from(f32::MIN_POSITIVE)
        {
            rotation::look_at(
                [x, y, z, 1.],
                [aim.target[0], aim.target[1], aim.target[2], 1.],
                self.avatar_up,
            )
        } else {
            aim.basis
        };
        let (s, c) = libm::sincos(deviation.to_radians());
        let (sp, cp) = libm::sincos(phase * std::f64::consts::TAU);
        let direction = Vec3(std::array::from_fn(|i| {
            f64::from(launch_basis[8 + i]) * c
                + f64::from(launch_basis[i]) * s * cp
                + f64::from(launch_basis[4 + i]) * s * sp
        }));
        let mut flight = Flight {
            muzzle,
            camera: self.camera,
            basis_origin: Vec3([aim.basis[12], aim.basis[13], aim.basis[14]].map(f64::from)),
            head_anchor: reticle.anchor,
            aim_target,
            direction,
            steering_target: self.input.preselected_target,
            impacts: Vec::new(),
            blast_center: None,
            radial_impacts: Vec::new(),
            beam_endpoint: None,
            beam_contacts: Vec::new(),
            entity: None,
            redirects: Vec::new(),
            bends: Vec::new(),
            stopped_at: None,
            fire_origin,
        };
        if let Some(homing) = homing {
            self.trace_guided(&mut flight, homing, range, eligible);
            return flight;
        }

        let maximum = pierce.max(0.) as f32;
        let mut accumulated = 0.0_f32;
        let max_distance = range.min(speed.unwrap_or(100_000.) * self.input.maximum_flight_seconds);
        let line = direction.normalized().unwrap_or(reticle.direction);
        if projectile_reverse_query && !every_body && maximum.is_finite() {
            self.trace_kinematic(&mut flight, line, max_distance, width * 0.5, speed, maximum, eligible);
            return flight;
        }
        let ray = Ray { origin: muzzle, direction: line, maximum: max_distance, width: width * 0.5 };
        for BodyHit { body, proxy, part, enter, exit } in Self::hits(&self.bodies, &self.bounds, ray) {
            if eligible.get(body) != Some(&true) {
                continue;
            }
            let point = muzzle.along(line, enter);
            let end = muzzle.along(line, exit);
            let material = end.minus(point).length();
            let seconds = speed.map_or(0., |v| enter / v);
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
                proxy: self.bodies[body].proxies[proxy].proxy.cache_index,
                part,
                point,
                exit: end,
                seconds,
                travel: enter,
                material,
                remaining,
                radial: false,
                radial_distance: 0.,
            });
            if !crossing.continues {
                flight.stopped_at = Some(point);
                break;
            }
        }
        flight
    }

    fn trace_guided(&self, flight: &mut Flight, homing: Homing, range: f64, eligible: &[bool]) {
        let direction = flight.direction.normalized().unwrap_or(Vec3([0., 0., 1.]));
        let mut state = Guided::new(
            flight.muzzle.0.map(|v| v as f32),
            direction.0.map(|v| v as f32),
            homing.initial_speed,
        );
        let dt = self.input.frame_seconds;
        let limit = (self.input.maximum_flight_seconds / dt).ceil() as usize;
        let cone_dot = libm::cosf(homing.seek_cone_deg * f32::from_bits(0x3c8efa35));
        let mut published = state.position;
        let mut step = 0;
        let default_update = GuidedUpdate { substeps: 1, publish_transform: true };
        let updates = self.input.guided_updates.iter().copied().chain(std::iter::repeat(default_update));
        for update in updates {
            if step >= limit {
                break;
            }
            // Native 820B36 refreshes once before its loop, including catch-up updates.
            flight.steering_target = self.refresh_guided_target(
                flight.steering_target, &state, homing, cone_dot, eligible,
            );
            let target = flight
                .steering_target
                .map(|i| self.bodies[i].center);
            let world_orientation = state.orientation;
            for _ in 0..usize::from(update.substeps).min(limit - step) {
                step += 1;
                let start = state.position;
                let orientation = state.orientation;
                let traveled = f64::from(state.flight);
                let displacement = state.step_with_world(
                    target,
                    dt as f32,
                    homing.turning_seconds,
                    homing.acceleration,
                    homing.maximum_speed,
                    world_orientation,
                );
                if let Some(mut impact) = self.guided_contact(
                    start,
                    displacement,
                    orientation,
                    homing.collision_radius,
                    (range - traveled).max(0.),
                    eligible,
                ) {
                    impact.seconds = step as f64 * dt;
                    impact.travel += traveled;
                    flight.impacts.push(impact);
                    // The original accumulator includes the attempted impact substep.
                    flight.blast_center = (state.flight > 0.25).then(|| Vec3(published.map(f64::from)));
                    return;
                }
                if f64::from(state.flight) >= range {
                    return;
                }
            }
            if update.publish_transform {
                published = state.position;
            }
        }
    }

    fn refresh_guided_target(
        &self,
        retained: Option<usize>,
        state: &Guided,
        homing: Homing,
        cone_dot: f32,
        eligible: &[bool],
    ) -> Option<usize> {
        let retained = retained.filter(|&i| eligible.get(i) == Some(&true));
        if retained.is_some() || state.flight <= homing.minimum_flight_before_seek {
            return retained;
        }
        let origin = Vec3(state.position.map(f64::from));
        let [x, y, z, _] = rotation::quaternion_direction(state.orientation);
        let scores = self.bodies.iter().enumerate().filter_map(|(i, body)| {
            if eligible.get(i) != Some(&true) {
                return None;
            }
            glaive::candidate_score(
                state.position,
                [x, y, z],
                self.root(i).0.map(|v| v as f32),
                body.anchor.0.map(|v| v as f32),
                homing.seek_distance,
                cone_dot,
            )
            .map(|score| (i, score))
        });
        glaive::select_candidate(scores, |i| {
            if self.input.clear_homing_visibility {
                return true;
            }
            let offset = self.bodies[i].anchor.minus(origin);
            offset.normalized().is_none_or(|ray| {
                self.first_hit(origin, ray, offset.length() + 1e-6, 0., |body| {
                    eligible.get(body) == Some(&true)
                }).is_none_or(|hit| hit.body == i)
            })
        })
    }

    fn guided_contact(
        &self,
        start: [f32; 3],
        displacement: [f32; 3],
        orientation: [f32; 4],
        radius: f32,
        remaining_range: f64,
        eligible: &[bool],
    ) -> Option<Impact> {
        let origin = Vec3(start.map(f64::from));
        for endpoint in glaive::query_ends(start, displacement, orientation, radius, false) {
            let delta = Vec3(endpoint.map(f64::from)).minus(origin);
            let Some(line) = delta.normalized() else {
                continue;
            };
            let line = Vec3(line.0.map(|v| f64::from(v as f32)));
            let limit = f64::from(delta.length() as f32).min(remaining_range);
            // The posed-body adapter supplies distance order and an opposing normal.
            let hit = self.first_hit(origin, line, limit, 0., |body| {
                eligible.get(body) == Some(&true)
            });
            let normal = line.0.map(|v| -v as f32);
            if let Some(BodyHit { body, proxy, part, enter, exit }) =
                glaive::impact_record(displacement, hit.is_some(), hit.map(|hit| (hit, true, normal)))
            {
                return Some(Impact {
                    body,
                    proxy: self.bodies[body].proxies[proxy].proxy.cache_index,
                    part,
                    point: origin.along(line, enter),
                    exit: origin.along(line, exit),
                    seconds: 0.,
                    travel: enter,
                    material: exit - enter,
                    remaining: 0.,
                    radial: false,
                    radial_distance: 0.,
                });
            }
        }
        None
    }
}

#[cfg(test)]
mod tests;
