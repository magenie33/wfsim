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

/// The radius of a body, in metres — every actor is a circle, and that is the
/// WHOLE model of how big a body is.
///
/// **THE PLANE IS THE MODEL, NOT AN APPROXIMATION OF A SOLID** (owner,
/// 2026-08-15). This number was briefly split in two: a footprint for spacing
/// and a bigger "effective" radius for the hit test, on the reasoning that a
/// real spread cone spends half its deviation vertically where a humanoid is
/// three times its own width, and a plane has nowhere to put that. The split
/// was wrong, and the reason is a division of labour that already exists:
///
///   · the GEOMETRY answers "did this pellet reach the target at all";
///   · `headshot_pct` answers "and given that it did, where did it land" — a
///     pinned per-pellet aim weight, which is exactly the vertical question.
///
/// So folding a body's height into the hit radius asks the second question
/// twice and the first one wrong. One circle, one number, and it is the same
/// number for the hit test and for how much floor a body occupies when there
/// is more than one of them.
///
/// **AND IT IS ONE NUMBER TO CHANGE.** Nothing multiplies it, nothing derives
/// from it, and no data file restates it — a measurement replaces this line and
/// the whole model moves with it.
///
/// **STILL A GUESS, AND SAID SO.** DE publishes no enemy hitbox
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
/// It is NOT in the blast test, which stays centre-to-centre against the
/// weapon's published radius — that radius is what DE calibrated against
/// whatever the game does, so adding a body's own to it would count the same
/// thing twice.
///
/// **IN THE HIT TEST IT IS LOAD-BEARING, DELIBERATELY.** Every miss in this engine is decided
/// against this radius, so it sets how harshly range costs a weapon: a 2 degree
/// aimed cone (the Braton's, from the wiki's own weapon module) puts a pellet
/// inside 0.2 m out to about 6 m and outside it past that. Whether that is the
/// game is exactly what the measurement below answers, and until it is
/// answered a fight at a range says so on the page.
///
/// **THE MEASUREMENT THAT SETTLES IT** is one afternoon in the Simulacrum, and
/// it settles the whole model because this is its only free parameter: stand a
/// known distance from one stationary enemy, fire a counted number of pellets
/// from a weapon of known spread, and count what lands. Two ranges and two
/// spreads over-determine it (docs/MEASUREMENTS.md).
pub const BODY_RADIUS_M: f64 = 0.2;


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
