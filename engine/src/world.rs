//! 2D top-down world: the spatial layer under the future UI.
//!
//! Design decisions (2026-07-24, see devlog):
//! - The world is a **2D top-down plane**; the Z axis is dropped for now.
//! - Every actor (Warframe, enemy) is abstracted as a **circle**, default
//!   radius 0.25 m. This makes range, line-of-sight, and AoE (circle overlap)
//!   computable against a *real* spatial layout instead of a scalar distance.
//! - "Feel" factors (aim wobble, reaction time, headshot ratio) are modeled
//!   as probabilities, not simulated motor control.
//!
//! This module holds only geometry; combat stays in [`crate::dummy`]. The
//! first spatial scenario ([`Engagement`]) is one shooter vs one target
//! circle: hitscan, static positions, hard range cutoff. Falloff, projectile
//! flight, movement, and AoE circles come next and all reduce to these
//! primitives.

use crate::dummy::{monte_carlo, DummyParams, Summary};

/// Default actor radius in meters (assumption — refine when hitboxes matter).
pub const ACTOR_RADIUS_M: f64 = 0.25;

/// A point / vector on the top-down plane, in meters.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance(self, other: Vec2) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

/// A circular actor footprint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Circle {
    pub center: Vec2,
    pub radius: f64,
}

impl Circle {
    pub const fn new(center: Vec2, radius: f64) -> Self {
        Self { center, radius }
    }

    /// An actor-sized circle (radius 0.25 m) at `center`.
    pub const fn actor(center: Vec2) -> Self {
        Self::new(center, ACTOR_RADIUS_M)
    }

    pub fn contains(&self, p: Vec2) -> bool {
        self.center.distance(p) <= self.radius
    }

    /// Circle overlap — the AoE primitive (blast radius vs actor footprint).
    pub fn intersects(&self, other: &Circle) -> bool {
        self.center.distance(other.center) <= self.radius + other.radius
    }

    /// Distance from a point to this circle's edge (0 inside).
    pub fn edge_distance(&self, p: Vec2) -> f64 {
        (self.center.distance(p) - self.radius).max(0.0)
    }
}

/// One shooter vs one target circle on the plane.
///
/// Current model: hitscan with a hard range cutoff (Dual Toxocyst base form:
/// 300 m) measured to the target's **edge**; if the target is out of range
/// every shot misses (zero damage). Aim quality and headshot ratio remain
/// probabilities inside [`DummyParams::body_parts`].
#[derive(Debug, Clone)]
pub struct Engagement {
    pub shooter: Vec2,
    pub target: Circle,
    /// Weapon hard range in meters (hitscan cutoff).
    pub weapon_range_m: f64,
    pub combat: DummyParams,
}

impl Engagement {
    pub fn target_edge_distance(&self) -> f64 {
        self.target.edge_distance(self.shooter)
    }

    pub fn target_in_range(&self) -> bool {
        self.target_edge_distance() <= self.weapon_range_m
    }

    /// Run the engagement. Positions are static for now, so range is decided
    /// once; the temporal combat loop is delegated to [`monte_carlo`].
    pub fn monte_carlo(&self, runs: u32, seed: u64) -> Summary {
        if self.target_in_range() {
            monte_carlo(&self.combat, runs, seed)
        } else {
            // Out of range: every shot misses. Keep the run/shot accounting.
            let shots_per_run = (self.combat.duration_secs * self.combat.fire_rate).ceil();
            Summary {
                runs,
                duration_secs: self.combat.duration_secs,
                mean_damage: 0.0,
                dps: 0.0,
                std_damage: 0.0,
                min_damage: 0.0,
                max_damage: 0.0,
                mean_effective_damage: 0.0,
                effective_dps: 0.0,
                mean_kills: 0.0,
                std_kills: 0.0,
                min_kills: 0,
                max_kills: 0,
                mean_shots: shots_per_run,
                mean_crit_rate: 0.0,
                mean_big_crit_rate: 0.0,
                mean_headshot_rate: 0.0,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engagement_at(distance: f64, range: f64) -> Engagement {
        Engagement {
            shooter: Vec2::default(),
            target: Circle::actor(Vec2::new(distance, 0.0)),
            weapon_range_m: range,
            combat: DummyParams::default(),
        }
    }

    #[test]
    fn distance_and_edge_distance() {
        let c = Circle::actor(Vec2::new(3.0, 4.0));
        assert!((Vec2::default().distance(c.center) - 5.0).abs() < 1e-12);
        assert!((c.edge_distance(Vec2::default()) - 4.75).abs() < 1e-12);
        assert_eq!(c.edge_distance(c.center), 0.0);
    }

    #[test]
    fn circle_intersection_is_symmetric_and_touch_counts() {
        let a = Circle::actor(Vec2::default());
        let b = Circle::actor(Vec2::new(0.5, 0.0)); // exactly touching
        let c = Circle::actor(Vec2::new(0.51, 0.0));
        assert!(a.intersects(&b) && b.intersects(&a));
        assert!(!a.intersects(&c));
    }

    #[test]
    fn in_range_engagement_matches_plain_dummy_sim() {
        let e = engagement_at(20.0, 300.0);
        let spatial = e.monte_carlo(300, 7);
        let plain = monte_carlo(&e.combat, 300, 7);
        assert_eq!(spatial.mean_damage, plain.mean_damage);
        assert_eq!(spatial.mean_kills, plain.mean_kills);
    }

    #[test]
    fn out_of_range_deals_nothing_but_still_fires() {
        let e = engagement_at(301.0, 300.0);
        assert!(!e.target_in_range());
        let s = e.monte_carlo(100, 7);
        assert_eq!(s.mean_damage, 0.0);
        assert_eq!(s.mean_kills, 0.0);
        assert!((s.mean_shots - 10.0).abs() < 1e-9);
    }

    #[test]
    fn range_is_measured_to_the_target_edge() {
        // Center at 300.2 but radius 0.25 -> edge at 299.95: in range.
        let e = engagement_at(300.2, 300.0);
        assert!(e.target_in_range());
    }
}
