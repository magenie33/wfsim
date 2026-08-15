//! WHERE THINGS ARE — the fight's 2D layer.
//!
//! The arena had two actors and no geometry between them: every shot landed at
//! point blank, so a weapon that falls off with distance was simulated at its
//! best case and said so. This is the missing half, and it is COORDINATES
//! rather than a scalar "engagement distance" on purpose (owner, 2026-08-15).
//!
//! For one target the two are the same number — a player and an enemy are two
//! points, and the only degree of freedom between them is how far apart they
//! stand. The reason to store the points anyway is what comes after: a second
//! body, an explosion whose epicentre is not on anybody, a beam that sweeps.
//! None of those can be expressed as a distance, and a model that stored one
//! would have to be rewritten rather than extended to get them. The extra cost
//! today is one field instead of one number.
//!
//! Metres, and the plane is the GROUND — `y` is the second horizontal axis, not
//! height. The arena has no vertical dimension and nothing here pretends
//! otherwise; a hitbox's height lives in [`crate::dummy::BodyPart`], which is a
//! question about where on a body a pellet landed rather than where the body is.

/// A point on the arena floor, in metres.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ORIGIN: Self = Self { x: 0.0, y: 0.0 };

    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Metres between two points.
    pub fn distance(self, other: Self) -> f64 {
        let (dx, dy) = (self.x - other.x, self.y - other.y);
        dx.hypot(dy)
    }
}

/// The radius of a body on the floor, in metres — every actor is a circle.
///
/// **A GUESS, AND SAID SO** (owner, 2026-08-15). DE publishes no enemy hitbox
/// size: the wiki's `Area of Effect` gives the zone shapes and the falloff and
/// never says whether the radius is measured to a body's centre or to its
/// surface, `Hit Mechanic` describes only the player's side, and `Line of
/// Sight` is the one page that says what the game thinks an enemy IS — three
/// rays "to the target's head, torso, and feet", which is a vertical segment
/// with no width at all. The one public model of Warframe AoE (malurth's
/// simulator) labels its own `entitySize = 0.5` "a guess" in the control's
/// tooltip and then does not use it: its blast test is centre-to-centre
/// against the published radius, and the radius only ever draws the circle.
///
/// So this number is load-bearing for NOTHING today, deliberately. It is not in
/// the blast test (an explosion is measured to the target's centre against the
/// weapon's published radius, which is what the published radius was calibrated
/// against), and it is not in a hit test, because there is no hit test — every
/// shot lands, exactly as before. What it is here for is to have ONE place to
/// correct when it is measured, instead of a constant appearing in three.
///
/// **Why it must not quietly become load-bearing**: the moment spread decides
/// hit or miss, this radius decides every low-accuracy weapon. The wiki's own
/// formula is `Accuracy = 100 / (average spread in degrees)`, and the roster
/// carries the stat already — `accuracy: 12.5` is 8°, which at 20 m is a 2.8 m
/// lateral offset against a 0.2 m circle, i.e. a weapon that misses almost
/// always and does not in game. Wiring accuracy to this radius therefore needs
/// the measurement first (docs/MEASUREMENTS.md), not a plausible number.
pub const BODY_RADIUS_M: f64 = 0.2;

/// The radius a SPREAD CONE has to land inside to count as a hit, in metres.
///
/// A DIFFERENT QUESTION FROM [`BODY_RADIUS_M`], and keeping them one number was
/// measurably wrong (2026-08-15). The body radius is how much FLOOR a body
/// occupies — what decides whether two enemies can stand there, which is what
/// it is for. This is how big a body looks to a bullet, and in a plane with no
/// vertical axis those are not the same number at all.
///
/// WHY IT IS BIGGER. Real spread is a CONE, so half of every deviation goes
/// vertical — and a humanoid is about 0.6 m wide and 1.8 m tall, so vertical
/// deviation is very largely forgiven and horizontal deviation is not. A flat
/// model has nowhere to put that, so the silhouette has to come back as an
/// effective radius: the circle of the same AREA as a 0.6 x 1.8 m silhouette is
/// `sqrt(0.6 * 1.8 / PI)` = 0.586 m.
///
/// **THIS IS A DERIVATION, NOT A MEASUREMENT**, and the difference matters
/// enough to say twice. What it rests on is two silhouette dimensions nobody
/// published either. It is here rather than 0.2 m because 0.2 m is refutable
/// from the game as it ships — the wiki's own conversion makes a Braton a 3.5
/// degree cone, and against a 0.2 m circle that is a weapon which misses two
/// shots in three at FIVE METRES, which is not the Braton anyone has fired.
/// 0.586 makes the same weapon land 95% of them there. Both numbers are
/// unverified; only one of them is unverified and also absurd.
///
/// THE MEASUREMENT THAT SETTLES IT is one afternoon in the Simulacrum and it
/// settles the whole model, because this is the model's only free parameter:
/// stand a known distance from one stationary enemy, fire a counted number of
/// pellets from a weapon of known accuracy, and count what lands. Two ranges
/// and two accuracies over-determine it. Until then every fight at a range
/// says so on the page (docs/MEASUREMENTS.md).
pub const AIM_TARGET_RADIUS_M: f64 = 0.586;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_is_symmetric_and_zero_at_a_point() {
        let a = Vec2::new(3.0, 4.0);
        assert_eq!(a.distance(Vec2::ORIGIN), 5.0);
        assert_eq!(Vec2::ORIGIN.distance(a), 5.0);
        assert_eq!(a.distance(a), 0.0);
    }
}
