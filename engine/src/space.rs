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
/// **MEASURED (owner, 2026-08-16).** Walking into an enemy stops at **0.4 m**
/// centre to centre, and two bodies of the same size touching at 0.4 m makes
/// each of them **0.2 m**. That is the whole derivation and it needs nothing
/// else: the closest approach IS twice the radius, so the one quantity a
/// player can actually read off the game gives it directly.
///
/// It replaces a guess of 0.25 m, which had been reached by taking the circle
/// of the same AREA as a 0.6 x 1.8 m silhouette — an attempt to smuggle a
/// body's HEIGHT back into a flat world, and wrong twice over: the plane is
/// the model, and `headshot_pct` already owns where on a body a landed pellet
/// went. The owner's original 0.2 m was right.
///
/// WHAT IT DOES AND DOES NOT MOVE. Contact behaviour is invariant under it,
/// because `CONTACT_RANGE_M` is 2r and the hit test at contact is
/// `r / 2r = 0.5` whatever r is — so both boards, every golden value and the
/// two entries whose cone is wide enough to miss at contact (the Mandonel's
/// uncharged 60 degrees, the Cryotra's 40) are exactly where they were. What
/// changes is every distance BEYOND contact, where a smaller body is a harder
/// target: the same 2 degree cone that missed a 0.25 m body past about 7 m
/// now misses a 0.2 m one past about 5.7 m.
///
/// STILL OPEN is whether the hit test should read this radius at all, or a
/// larger effective one — DE publishes no hitbox size (the wiki's `Area of
/// Effect` gives the zone shapes and never says whether a radius is measured
/// to a body's centre or its surface, `Hit Mechanic` is the player's side
/// only, and `Line of Sight` describes an enemy as three rays to head, torso
/// and feet, a vertical segment with no width). What is measured is how much
/// FLOOR a body occupies; that the same number governs whether a pellet
/// reaches it is the model's choice, and docs/MEASUREMENTS.md carries the
/// experiment that would confirm it: a counted number of pellets, a known
/// range, a weapon of known spread, count what lands.
pub const BODY_RADIUS_M: f64 = 0.2;

/// THE CLOSEST TWO BODIES CAN STAND — twice a radius, because circles do not
/// overlap (owner, 2026-08-15).
///
/// So the fight's floor is 0.5 m rather than 0, and "point blank" stops being a
/// distance of zero: a zero would put the two of them in the same place, which
/// is the one arrangement the plane cannot hold. It is also the floor a
/// DRAGGED scene needs — you cannot push the two dots through each other — and
/// the same number will be the minimum spacing between two ENEMIES when there
/// is more than one of them.
pub const CONTACT_RANGE_M: f64 = 2.0 * BODY_RADIUS_M;


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
