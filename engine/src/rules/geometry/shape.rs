// SPDX-License-Identifier: AGPL-3.0-or-later
use super::Vec3;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Shape {
    Sphere {
        radius: f64,
    },
    /// Inner segment along local Y; height excludes the two spherical caps.
    Capsule {
        radius: f64,
        height: f64,
    },
    /// Centered local box with half-extents `[dx/2, dy/2, dz/2]`.
    Box {
        half_extents: [f64; 3],
    },
}

#[wfsim_gas_macro::metered]
fn roots(a: f64, b: f64, c: f64) -> [Option<f64>; 2] {
    let discriminant = b * b - 4.0 * a * c;
    if a == 0.0 || discriminant < 0.0 {
        return [None, None];
    }
    let root = libm::sqrt(discriminant);
    [Some((-b - root) / (2.0 * a)), Some((-b + root) / (2.0 * a))]
}

#[wfsim_gas_macro::metered]
impl Shape {
    pub fn radius_and_half_height(self) -> (f64, f64) {
        match self {
            Self::Sphere { radius } => (radius, 0.0),
            Self::Capsule { radius, height } => (radius, height * 0.5),
            Self::Box { half_extents: [hx, hy, hz] } => (hx.max(hz), hy),
        }
    }

    fn box_interval(half_extents: [f64; 3], origin: Vec3, direction: Vec3) -> Option<(f64, f64)> {
        if !half_extents.iter().all(|&h| h >= 0.0 && h.is_finite())
            || !origin.0.iter().chain(&direction.0).all(|v| v.is_finite())
            || direction.dot(direction) == 0.0
        {
            return None;
        }
        let (mut t_min, mut t_max) = (f64::NEG_INFINITY, f64::INFINITY);
        for (i, &h) in half_extents.iter().enumerate() {
            let p = origin.0[i];
            let d = direction.0[i];
            if d.abs() < 1e-15 {
                if p.abs() > h {
                    return None;
                }
            } else {
                let a = (-h - p) / d;
                let b = (h - p) / d;
                t_min = t_min.max(a.min(b));
                t_max = t_max.min(a.max(b));
                if t_min > t_max {
                    return None;
                }
            }
        }
        if t_max < 0.0 {
            return None;
        }
        let enter = t_min.max(0.0);
        Some((enter, t_max.max(enter)))
    }

    /// Entry and exit of one convex primitive, in the original ray parameter.
    pub fn interval(self, origin: Vec3, direction: Vec3) -> Option<(f64, f64)> {
        if let Self::Box { half_extents } = self {
            return Self::box_interval(half_extents, origin, direction);
        }
        let enter = self.intersect(origin, direction)?;
        let (radius, half) = self.radius_and_half_height();
        let [x, y, z] = origin.0;
        let [dx, dy, dz] = direction.0;
        let mut exit = enter;
        if half > 0.0 {
            for t in roots(dx * dx + dz * dz, 2.0 * (x * dx + z * dz), x * x + z * z - radius * radius).into_iter().flatten() {
                if t >= enter && (y + dy * t).abs() <= half { exit = exit.max(t); }
            }
        }
        for cap in [-half, half] {
            let p = Vec3([x, y - cap, z]);
            for t in roots(direction.dot(direction), 2.0 * p.dot(direction), p.dot(p) - radius * radius).into_iter().flatten() {
                let py = y + dy * t;
                if t >= enter && (half == 0.0 || (cap < 0.0 && py <= -half) || (cap >= 0.0 && py >= half)) { exit = exit.max(t); }
            }
        }
        Some((enter, exit))
    }

    /// Analytic reference for the native sphere/capsule/box query (EGEO-01 / E3D-ROSTER-01).
    /// An origin already inside returns zero. Direction retains its ray parameter.
    pub fn intersect(self, origin: Vec3, direction: Vec3) -> Option<f64> {
        if let Self::Box { half_extents } = self {
            return Self::box_interval(half_extents, origin, direction).map(|(enter, _)| enter);
        }
        let (radius, half) = self.radius_and_half_height();
        if radius < 0.0
            || half < 0.0
            || !radius.is_finite()
            || !half.is_finite()
            || !origin.0.iter().chain(&direction.0).all(|v| v.is_finite())
            || direction.dot(direction) == 0.0
        {
            return None;
        }
        let [x, y, z] = origin.0;
        let [dx, dy, dz] = direction.0;
        let closest = Vec3([x, y - y.clamp(-half, half), z]);
        if closest.dot(closest) <= radius * radius {
            return Some(0.0);
        }
        let mut best = f64::INFINITY;
        if half > 0.0 {
            for t in roots(
                dx * dx + dz * dz,
                2.0 * (x * dx + z * dz),
                x * x + z * z - radius * radius,
            )
            .into_iter()
            .flatten()
            {
                if t >= 0.0 && (y + dy * t).abs() <= half {
                    best = best.min(t);
                }
            }
        }
        for cap in [-half, half] {
            let shifted = Vec3([x, y - cap, z]);
            for t in roots(
                direction.dot(direction),
                2.0 * shifted.dot(direction),
                shifted.dot(shifted) - radius * radius,
            )
            .into_iter()
            .flatten()
            {
                let py = y + dy * t;
                if t >= 0.0
                    && (half == 0.0 || (cap < 0.0 && py <= -half) || (cap >= 0.0 && py >= half))
                {
                    best = best.min(t);
                }
            }
        }
        best.is_finite().then_some(best)
    }
}
