// SPDX-License-Identifier: AGPL-3.0-or-later
use super::{Affine, Shape, Vec3};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proxy {
    pub cache_index: usize,
    pub bone_token: u32,
    pub part: u8,
    pub shape: Shape,
    /// Current world * bone * attachment. Mesh scale is already installed by the caller.
    pub world: Affine,
    pub ray_enabled: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
    pub maximum: f64,
    pub previous_best: f64,
    /// Native mask excludes the named parts, rather than including them.
    pub excluded_parts: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Contact {
    pub owner: u64,
    pub cache_index: usize,
    pub bone_token: u32,
    pub part: u8,
    pub point: Vec3,
    pub distance: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Active,
    Inactive,
    MissingAnimation,
    EmptyBodyList,
}

#[derive(Debug)]
pub enum Refinement {
    Contact(Contact),
    Miss,
    /// Native outer return alone establishes no geometric contact.
    Bypassed {
        accepted: bool,
    },
}

/// Portable reference for native10434e0/1068210, under supplied cache/world.
/// Affine inversion and primitive arithmetic use binary64; see positional tolerance.
#[wfsim_gas_macro::metered]
pub fn refine(owner: u64, proxies: &[Proxy], ray: Ray, gate: Gate) -> Refinement {
    match gate {
        Gate::Inactive => return Refinement::Bypassed { accepted: false },
        Gate::MissingAnimation | Gate::EmptyBodyList => {
            return Refinement::Bypassed { accepted: true }
        }
        Gate::Active => {}
    }
    let mut nearest = ray.maximum.min(ray.previous_best);
    let mut hit = None;
    for proxy in proxies {
        if !proxy.ray_enabled || proxy.part >= 32 || ray.excluded_parts & (1 << proxy.part) != 0 {
            continue;
        }
        let Some(inverse) = proxy.world.inverse() else {
            continue;
        };
        let Some(distance) = proxy
            .shape
            .intersect(inverse.point(ray.origin), inverse.direction(ray.direction))
        else {
            continue;
        };
        if distance < nearest {
            nearest = distance;
            hit = Some(Contact {
                owner,
                cache_index: proxy.cache_index,
                bone_token: proxy.bone_token,
                part: proxy.part,
                point: ray.origin.along(ray.direction, distance),
                distance,
            });
        }
    }
    hit.map_or(Refinement::Miss, Refinement::Contact)
}
