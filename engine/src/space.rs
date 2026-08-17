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

    /// This point moved `metres` of the way toward `toward`. The direction is
    /// what a body FACES, so a degenerate pair (nowhere to face) stays put.
    pub fn toward(self, toward: Self, metres: f64) -> Self {
        let (dx, dy) = (toward.x - self.x, toward.y - self.y);
        let len = dx.hypot(dy);
        if len <= 0.0 {
            return self;
        }
        Self::new(self.x + dx / len * metres, self.y + dy / len * metres)
    }
}

/// WHERE A SHOT LEAVES — a point on the shooter's own circumference, facing
/// what they are aiming at (owner, 2026-08-16).
///
/// A body with a size fires from its FRONT, not from its centre, which is the
/// only place a muzzle can be once the shooter is a circle rather than a dot.
/// The facing is derived rather than stored: you are looking at what you are
/// shooting at, so aiming at a second target turns you, and there is no
/// third state to keep in sync.
pub fn muzzle(shooter: Vec2, aim_at: Vec2) -> Vec2 {
    shooter.toward(aim_at, BODY_RADIUS_M)
}

/// MUZZLE TO THE TARGET'S CENTRE — the ray-versus-circle test's own parameter,
/// and NOT how far a shot flies.
///
/// The two were one function called `shot_travel` for a few hours, and the name
/// was the error (owner, 2026-08-16): a bullet vanishes when it reaches the
/// target's SURFACE, so what it flies is the [`gap`], and this is one radius
/// longer than that. It appears in [`miss_distance`] because the perpendicular
/// distance from a circle's CENTRE to a ray is what decides whether the ray
/// crosses the circle — a geometric parameter that happens to have units of
/// length, not a distance anything travels.
pub fn range_to_centre(shooter: Vec2, target: Vec2) -> f64 {
    (shooter.distance(target) - BODY_RADIUS_M).max(0.0)
}

/// THE GAP between two bodies — surface to surface, ZERO AT CONTACT, and the
/// distance a shot actually FLIES.
///
/// This is what "how far apart are we" means once bodies have a size, so it is
/// the number the arena SHOWS and the quick sets set (owner, 2026-08-16).
/// Standing on someone reads 0 m, which is what point blank has always meant
/// to a player; the 0.4 m between the two centres is a fact about the model
/// and not something a reader should have to subtract.
///
/// IT IS ALSO THE FLIGHT, which is why damage falloff reads it and why the two
/// need no reconciling: a bullet vanishes when it reaches the target's SURFACE
/// rather than travelling on to its centre, so muzzle-to-surface is both what
/// a player would call the distance and what the projectile covers. Exactly,
/// for a shot down the middle; a grazing one lands further around the circle
/// and covers up to one radius more, which is under the resolution of any
/// window DE publishes.
///
/// AND IT IS WHY CONTACT CANNOT MISS, in one step: a flight of zero leaves a
/// cone no distance to widen over. [`miss_distance`] reaches the same answer
/// from the other side.
pub fn gap(a: Vec2, b: Vec2) -> f64 {
    (a.distance(b) - CONTACT_RANGE_M).max(0.0)
}

/// HOW FAR A DEVIATED SHOT PASSES from the target's centre — the ray's closest
/// approach, which is what decides whether it reaches the body at all.
///
/// RAY-VERSUS-CIRCLE, and nothing more.
///
/// A pellet that leaves `deviation_deg` off the aim line is a RAY from the
/// muzzle, so the question "does it hit" is ray-versus-circle and the answer is
/// `travel · sin(θ) ≤ r`. It was `centre_distance · tan(θ)` until 2026-08-16,
/// which was wrong twice — from the centre rather than the muzzle, and `tan`
/// rather than `sin`, so a wide cone's deviation blew up toward infinity
/// instead of being bounded by the leg it is measured against.
///
/// `range_m` is [`range_to_centre`] and NOT a flight — the perpendicular is
/// dropped from the circle's CENTRE, so that is the leg the formula needs.
/// What the shot flies is the [`gap`], one radius shorter. The parameter was
/// called `travel_m` for a few hours and the name was the whole confusion.
///
/// The fix is what makes CONTACT unmissable at any cone width: the muzzle is
/// then one radius from the target's centre, so the closest approach is
/// `r · sin(θ) ≤ r` for every θ, and a shotgun pressed against an enemy cannot
/// spray past it. The [`gap`] says the same thing more directly — at contact
/// there is no distance to deviate over at all. Under the old formula a 60
/// degree cone missed more than half its pellets at point blank, which nothing
/// in the game does.
///
/// Beyond 90 degrees the shot is going AWAY, and a ray's closest approach is
/// then its own origin — no weapon in the roster has a cone that wide, but the
/// formula is the ray's rather than the infinite line's so it cannot report a
/// hit for a shot fired backwards.
pub fn miss_distance(range_m: f64, deviation_deg: f64) -> f64 {
    let rad = deviation_deg.to_radians();
    if rad >= std::f64::consts::FRAC_PI_2 {
        range_m
    } else {
        range_m * rad.sin()
    }
}

/// WHERE AN EXPLOSION GOES OFF ON A BODY — the point on its circumference
/// FACING THE SHOOTER, not its centre (owner, 2026-08-17).
///
/// A round detonates where it touches, and once a body is a circle rather than
/// a dot that is a real place one body-radius nearer the muzzle. Using the
/// centre put every blast half a metre deeper into the formation than it goes.
///
/// Punch-through is not modelled, so a round never detonates on the FAR side.
pub fn detonation_point(body: Vec2, shooter: Vec2) -> Vec2 {
    body.toward(shooter, BODY_RADIUS_M)
}

/// IS THIS BODY IN THE BLAST? **ANY PART OF IT TOUCHING IS ENOUGH** (owner,
/// 2026-08-17) — a corner clipped by the edge takes damage.
///
/// So the radius a blast really covers is its own plus a body radius, and the
/// test is on the distance between CENTRES, which is the only distance the
/// arena stores.
pub fn caught_by_blast(centre_distance_m: f64, blast_radius_m: f64) -> bool {
    centre_distance_m <= blast_radius_m + BODY_RADIUS_M + 1e-9
}

/// HOW FAR INTO A BODY THE BLAST HAS TO REACH — the distance to its NEAREST
/// point, which is what falloff reads.
///
/// **THE BEST POINT ON THE BODY WINS** (owner, 2026-08-17). A body standing
/// across a falloff gradient has a different number at every point of it, and
/// the model takes the largest — which is the point nearest the epicentre while
/// no weapon in the game falls off the other way. It is a rule rather than an
/// average because an average would need a shape integral to defend, and this
/// needs only "the round found the best part of it", which is what a blast
/// does.
///
/// Zero once the epicentre is inside the body: it cannot reach less far than
/// nothing.
pub fn blast_reach(centre_distance_m: f64) -> f64 {
    (centre_distance_m - BODY_RADIUS_M).max(0.0)
}

/// WHICH BODY A SHOT CROSSES FIRST, along a ray from `muzzle` in `dir`.
///
/// The generalisation of the hit test to a formation, and it is the SAME test:
/// a body is hit when the ray passes within a radius of its centre
/// ([`miss_distance`]), and of the ones it does, the nearest along the ray is
/// the one that stops it.
///
/// AIM IS A DIRECTION, NOT A TARGET (owner, 2026-08-17). A player points the
/// weapon at a place — which may be a body, or the floor beside one — and
/// whatever the line runs through is what gets hit. Aiming a little short of an
/// enemy still hits it, because the enemy's circle is still on the line; that
/// is not a special case, it is what a radius means.
///
/// `dir` need not be normalised and behind the muzzle is not hit: a body is a
/// candidate only at a non-negative distance along the ray.
///
/// Returns the body's index and how far along the ray its SURFACE is — the
/// point of impact, which is where a damage radius is centred and where the
/// distance a shot flew is measured to.
pub fn first_hit(muzzle: Vec2, dir: Vec2, bodies: &[Vec2]) -> Option<(usize, f64)> {
    let len = dir.x.hypot(dir.y);
    if len <= 0.0 {
        return None;
    }
    let (ux, uy) = (dir.x / len, dir.y / len);
    let mut best: Option<(usize, f64)> = None;
    for (i, b) in bodies.iter().enumerate() {
        let (px, py) = (b.x - muzzle.x, b.y - muzzle.y);
        // How far along the ray the body's centre projects, and how far off it
        // sits. A negative projection is behind the shooter.
        let along = px * ux + py * uy;
        if along < 0.0 {
            continue;
        }
        let perp = (px * uy - py * ux).abs();
        if perp > BODY_RADIUS_M {
            continue;
        }
        // THE SURFACE, not the centre: a bullet stops where it enters.
        let half = (BODY_RADIUS_M * BODY_RADIUS_M - perp * perp).max(0.0).sqrt();
        let entry = (along - half).max(0.0);
        if best.is_none_or(|(_, d)| entry < d) {
            best = Some((i, entry));
        }
    }
    best
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
/// and now trivially so: the muzzle sits one radius forward, so the closest
/// approach at contact is `r · sin(θ) ≤ r` for every r and every cone — both
/// boards and every golden value are where they were. What changes is every
/// distance BEYOND contact, where a smaller body is a harder target: the same
/// 2 degree cone that missed a 0.25 m body past about 7 m now misses a 0.2 m
/// one past about 5.7 m.
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
/// So the two CENTRES never come closer than 0.4 m: a zero would put the two of
/// them in the same place, which is the one arrangement the plane cannot hold.
/// It is the floor a DRAGGED scene needs — you cannot push the two dots through
/// each other — and the same number will be the minimum spacing between two
/// ENEMIES when there is more than one of them.
///
/// "Point blank" is still ZERO, because what a reader is shown is the [`gap`]
/// between the two surfaces and that is what this arrangement leaves (owner,
/// 2026-08-16). The 0.4 m lives between the centres, where the model needs it
/// and nobody has to subtract it.
pub const CONTACT_RANGE_M: f64 = 2.0 * BODY_RADIUS_M;


#[cfg(test)]
mod tests {
    use super::*;

    /// THE THREE BLAST RULES, which are one idea: a body is a CIRCLE and a
    /// blast meets it at its nearest surface (owner, 2026-08-17).
    #[test]
    fn a_blast_meets_a_body_at_its_nearest_surface() {
        // IT GOES OFF WHERE IT TOUCHES — one radius nearer the shooter than
        // the body's centre.
        let foe = Vec2::new(0.0, 10.0);
        let d = detonation_point(foe, Vec2::ORIGIN);
        assert!((d.y - (10.0 - BODY_RADIUS_M)).abs() < 1e-12, "{d:?}");
        assert!((foe.distance(d) - BODY_RADIUS_M).abs() < 1e-12);

        // ANY PART TOUCHING IS ENOUGH, so a 3 m blast reaches a body whose
        // CENTRE is 3.2 m away and not one at 3.21.
        assert!(caught_by_blast(3.0, 3.0));
        assert!(caught_by_blast(3.0 + BODY_RADIUS_M, 3.0));
        assert!(!caught_by_blast(3.0 + BODY_RADIUS_M + 0.01, 3.0));

        // …AND FALLOFF READS THE BEST POINT, which is the nearest one.
        assert!((blast_reach(3.0) - 2.8).abs() < 1e-12);
        // A body standing ON the epicentre reaches zero rather than negative.
        assert_eq!(blast_reach(0.0), 0.0);
        assert_eq!(blast_reach(BODY_RADIUS_M * 0.5), 0.0);
    }

    /// AIM IS A DIRECTION, so a shot pointed at the FLOOR beside a body still
    /// hits it — the body's circle is on the line, which is the whole of what a
    /// radius means (owner, 2026-08-17).
    #[test]
    fn a_shot_aimed_short_of_a_body_still_crosses_it() {
        let foe = Vec2::new(0.0, 10.0);
        let muzzle = Vec2::new(0.0, 0.2);
        // Dead on.
        assert_eq!(first_hit(muzzle, Vec2::new(0.0, 1.0), &[foe]).map(|(i, _)| i), Some(0));
        // Aimed at bare floor two metres SHORT of it: same line, same hit.
        let short = Vec2::new(0.0, 8.0);
        let dir = Vec2::new(short.x - muzzle.x, short.y - muzzle.y);
        assert_eq!(first_hit(muzzle, dir, &[foe]).map(|(i, _)| i), Some(0));
        // …and a hand's width to the SIDE of it, still inside the radius.
        let beside = Vec2::new(BODY_RADIUS_M * 0.5, 10.0);
        let dir = Vec2::new(beside.x - muzzle.x, beside.y - muzzle.y);
        assert_eq!(first_hit(muzzle, dir, &[foe]).map(|(i, _)| i), Some(0));
        // …but past the radius it is floor, and nothing is hit.
        let wide = Vec2::new(BODY_RADIUS_M * 3.0, 10.0);
        let dir = Vec2::new(wide.x - muzzle.x, wide.y - muzzle.y);
        assert_eq!(first_hit(muzzle, dir, &[foe]), None);
    }

    /// THE NEAREST BODY ON THE LINE IS THE ONE THAT STOPS IT, and the distance
    /// reported is to its SURFACE — a bullet stops where it enters.
    #[test]
    fn the_first_body_on_the_line_takes_the_shot() {
        let bodies = [Vec2::new(0.0, 20.0), Vec2::new(0.0, 5.0), Vec2::new(0.0, 12.0)];
        let (i, d) = first_hit(Vec2::ORIGIN, Vec2::new(0.0, 1.0), &bodies).unwrap();
        assert_eq!(i, 1, "the body at 5 m is in front of the ones at 12 and 20");
        assert!((d - (5.0 - BODY_RADIUS_M)).abs() < 1e-9, "{d}");
        // …and BEHIND the shooter is not hit at all.
        assert_eq!(first_hit(Vec2::ORIGIN, Vec2::new(0.0, -1.0), &bodies), None);
        // A direction of nothing hits nothing rather than dividing by zero.
        assert_eq!(first_hit(Vec2::ORIGIN, Vec2::ORIGIN, &bodies), None);
    }

    /// CONTACT READS ZERO. The centres are 0.4 m apart and the surfaces are
    /// touching, and the second one is what the fight is described by.
    #[test]
    fn the_gap_is_zero_when_two_bodies_touch() {
        let you = Vec2::ORIGIN;
        let foe = Vec2::new(0.0, CONTACT_RANGE_M);
        assert_eq!(you.distance(foe), CONTACT_RANGE_M);
        assert_eq!(gap(you, foe), 0.0);
        // …and a gap is what you set: 20 m apart is 20.4 m between centres.
        assert!((gap(you, Vec2::new(0.0, 20.4)) - 20.0).abs() < 1e-12);
    }

    /// THE SHOT LEAVES THE FRONT, and the front is wherever you are aiming.
    #[test]
    fn the_muzzle_sits_on_the_shooters_own_circumference() {
        let you = Vec2::ORIGIN;
        // 3-4-5, so the muzzle is one radius along it and nowhere near an axis.
        let foe = Vec2::new(6.0, 8.0);
        let m = muzzle(you, foe);
        assert!((you.distance(m) - BODY_RADIUS_M).abs() < 1e-12);
        assert!((m.x - 0.12).abs() < 1e-12 && (m.y - 0.16).abs() < 1e-12);
        // …and the test's leg is to the CENTRE, one radius in from the ten
        // metres between them — while the FLIGHT is the gap, two radii in.
        assert!((range_to_centre(you, foe) - (10.0 - BODY_RADIUS_M)).abs() < 1e-12);
        assert!((gap(you, foe) - (10.0 - CONTACT_RANGE_M)).abs() < 1e-12);
        // Nowhere to face is not a crash — a body on top of you stays put.
        assert_eq!(muzzle(you, you), you);
    }

    /// AT CONTACT NOTHING MISSES, however wide the cone. The muzzle is then one
    /// radius from the target's centre, so the closest approach is `r·sin(θ)`,
    /// which cannot exceed `r`. It is the property the old `tan` formula broke:
    /// a 60 degree cone missed more than half its pellets pressed against an
    /// enemy, which nothing in the game does.
    #[test]
    fn a_shot_fired_at_contact_cannot_miss_at_any_cone_width() {
        let leg = range_to_centre(Vec2::ORIGIN, Vec2::new(0.0, CONTACT_RANGE_M));
        assert_eq!(leg, BODY_RADIUS_M);
        // …and the FLIGHT is zero, which says the same thing in one step: a
        // cone has no distance to widen over.
        assert_eq!(gap(Vec2::ORIGIN, Vec2::new(0.0, CONTACT_RANGE_M)), 0.0);
        for deg in [0.0, 2.0, 20.0, 40.0, 60.0, 89.0] {
            assert!(
                miss_distance(leg, deg) <= BODY_RADIUS_M + 1e-12,
                "{deg} degrees missed at contact"
            );
        }
        // …but a shot fired BACKWARDS is going away and is not a hit anywhere
        // beyond touching.
        assert_eq!(miss_distance(10.0, 120.0), 10.0);
    }

    /// …AND OUT THERE THE DEVIATION GROWS WITH THE DISTANCE. A 2 degree Braton
    /// deviation against a target 20 m from the muzzle passes 0.70 m from its
    /// centre — well past a 0.2 m body, which is why a distant target is
    /// missed at all.
    #[test]
    fn the_deviation_grows_with_the_distance_flown() {
        let d = miss_distance(20.0, 2.0);
        assert!((d - 0.6980).abs() < 1e-3, "{d}");
        assert!(d > BODY_RADIUS_M);
        // Perfectly on the reticle always lands, at any range.
        assert_eq!(miss_distance(500.0, 0.0), 0.0);
    }

    /// …AND IT HOLDS FOR THE WHOLE ROSTER, not just for the angles this file
    /// happened to pick. Every transcribed cone, fired at contact, lands: the
    /// widest is the Mandonel's uncharged 60 degrees, which the old
    /// centre-to-centre `tan` test dropped more than half of, pressed against
    /// an enemy. A weapon added tomorrow with a wider one is covered too.
    #[test]
    fn no_weapon_in_the_roster_can_miss_at_contact() {
        let leg = range_to_centre(Vec2::ORIGIN, Vec2::new(0.0, CONTACT_RANGE_M));
        let mut widest: f64 = 0.0;
        for w in crate::weapons_data::all() {
            let Some(s) = w.attack.spread else { continue };
            // `Spread::draw` is uniform over [0, 2 x min], so the worst a
            // weapon can roll is twice its own cone.
            let worst = s.min_deg.max(s.max_deg) * 2.0;
            widest = widest.max(worst);
            assert!(
                miss_distance(leg, worst) <= BODY_RADIUS_M + 1e-12,
                "{} missed at contact with a {worst} degree deviation",
                w.id
            );
        }
        // …and the roster really does contain a cone wide enough to have
        // failed the old test, so this is not vacuous.
        assert!(widest > 26.6, "widest deviation in the roster is {widest}");
    }

    #[test]
    fn distance_is_symmetric_and_zero_at_a_point() {
        let a = Vec2::new(3.0, 4.0);
        assert_eq!(a.distance(Vec2::ORIGIN), 5.0);
        assert_eq!(Vec2::ORIGIN.distance(a), 5.0);
        assert_eq!(a.distance(a), 0.0);
    }
}
