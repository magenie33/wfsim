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

/// HOW FAR OFF THE AIM LINE A BODY SITS, in degrees, seen from the muzzle.
///
/// ZERO when the weapon is pointed straight at it, which is the fight this
/// engine ran until aim became a place you choose (owner, 2026-08-17). Every
/// deviation the spread cone rolls is measured from the AIM line, and this is
/// how far the body already is from it before a single degree of spread is
/// added.
pub fn off_axis_deg(muzzle: Vec2, aim_at: Vec2, body: Vec2) -> f64 {
    let (ax, ay) = (aim_at.x - muzzle.x, aim_at.y - muzzle.y);
    let (bx, by) = (body.x - muzzle.x, body.y - muzzle.y);
    let (la, lb) = (ax.hypot(ay), bx.hypot(by));
    if la <= 0.0 || lb <= 0.0 {
        return 0.0;
    }
    let cos = ((ax * bx + ay * by) / (la * lb)).clamp(-1.0, 1.0);
    cos.acos().to_degrees()
}

/// WHERE A PELLET PASSES a body, when the weapon is not pointed at it.
///
/// The cone is around the AIM line and the body sits `off_axis` degrees off it,
/// so the two offsets are added as VECTORS in the plane the shot crosses, with
/// the pellet's direction around the cone uniform. `phi` is that direction, in
/// turns.
///
/// Pointed straight at the body (`off_axis == 0`) it collapses to
/// [`miss_distance`] exactly and reads no `phi` — which is what keeps every
/// fight this engine has ever run byte-identical, and why the caller draws
/// `phi` only when it is going to be used.
pub fn miss_distance_off_axis(range_m: f64, off_axis_deg: f64, deviation_deg: f64, phi: f64) -> f64 {
    if off_axis_deg <= 0.0 {
        return miss_distance(range_m, deviation_deg);
    }
    let a = miss_distance(range_m, off_axis_deg);
    let b = miss_distance(range_m, deviation_deg);
    let c = (phi * std::f64::consts::TAU).cos();
    (a * a + b * b - 2.0 * a * b * c).max(0.0).sqrt()
}

/// WHERE THE PELLET ACTUALLY WENT OFF, when it did not hit anything.
///
/// A FLOOR POINT AND A HEIGHT, because the arena is a plane and the spread cone
/// is not. The cross-section of the cone at the target's range is a disc, and
/// only its IN-FLOOR component moves the epicentre across the arena; the rest
/// is how far over or under the shot went, which is a real distance to every
/// body on the floor and belongs in the range rather than being thrown away.
///
/// WHY IT EXISTS (owner, 2026-08-19). [`miss_distance_off_axis`] answers "how
/// far did it pass the aimed body", which was the whole question while a fight
/// held ONE body — its own doc says so: *"the model has never drawn which side
/// — only the magnitude decides anything against one body"*. With a crowd, the
/// side decides WHO IS IN THE BLAST, and until this existed every other body
/// read its distance from the AIMED BODY'S SURFACE however wide the shot went.
/// Measured on the wire: a shot nine metres wide dropped the aimed body's
/// damage 61% and left the body two metres behind it on 7120 against 7115 —
/// the bystander took a direct hit's blast off a shot that went nowhere near.
///
/// IT CANNOT MOVE THE AIMED BODY'S NUMBER, by construction rather than by care:
/// with the body at `O + a·û` and the pellet at `O + b·cos(2πφ)·û + b·sin(2πφ)·v̂`,
/// the distance between them is `√(a² + b² − 2ab·cos 2πφ)`, which is
/// [`miss_distance_off_axis`] exactly. This is that formula's two vectors kept
/// apart instead of collapsed into their difference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detonation {
    /// Where it went off, on the floor.
    pub at: Vec2,
    /// How far above (or below) the floor — always taken as a magnitude, since
    /// a body is as far from a shot that went over it as from one that went
    /// under.
    pub height_m: f64,
}

impl Detonation {
    /// How far this explosion is from a body standing at `body`, in three
    /// dimensions — the floor gap and the height are legs of the same triangle.
    pub fn distance_to(self, body: Vec2) -> f64 {
        self.at.distance(body).hypot(self.height_m)
    }
}

/// Build the detonation for a pellet that left `deviation_deg` off the aim
/// line, `phi` turns around the cone.
///
/// `toward` is the body the offsets are measured against — the û axis points at
/// it, which is what makes `phi` mean the same angle it means in
/// [`miss_distance_off_axis`]. When the weapon points straight at that body the
/// axis is any perpendicular and the answer is symmetric in `phi`, so the
/// choice does not matter; it is taken from the aim line so it is never
/// undefined.
pub fn detonation_of_miss(
    muzzle: Vec2,
    aim_at: Vec2,
    toward: Vec2,
    range_m: f64,
    off_axis_deg: f64,
    deviation_deg: f64,
    phi: f64,
) -> Detonation {
    let (ax, ay) = (aim_at.x - muzzle.x, aim_at.y - muzzle.y);
    let la = ax.hypot(ay);
    if la <= 0.0 {
        return Detonation { at: toward, height_m: 0.0 };
    }
    let dir = Vec2::new(ax / la, ay / la);
    // THE ORIGIN IS THE BODY'S FOOT ON THE AIM RAY, not the ray's point at
    // `range_m`. `a` in the scalar is a PERPENDICULAR distance
    // (`range_m · sin θ`), so the station it is measured at is `range_m · cos θ`
    // along the ray — using `range_m` there instead was wrong by
    // `range_m(1 − cos θ)` and showed up the moment the weapon was pointed off
    // the body at all.
    let (sin_o, cos_o) = off_axis_deg.to_radians().sin_cos();
    let along = range_m * cos_o;
    let o = Vec2::new(muzzle.x + dir.x * along, muzzle.y + dir.y * along);
    // …and the in-floor perpendicular, oriented TOWARD the body so that `phi`
    // is measured from the same direction the scalar formula measures it from.
    let mut u = Vec2::new(-dir.y, dir.x);
    if (toward.x - o.x) * u.x + (toward.y - o.y) * u.y < 0.0 {
        u = Vec2::new(-u.x, -u.y);
    }
    // THE BODY, RECONSTRUCTED rather than read: `o + a·û` is where the scalar
    // believes it stands, and the epicentre has to be placed in the scalar's
    // own frame or the two stop agreeing. They differ only by the plane model's
    // own approximation, which is not this function's to correct.
    let a = range_m * sin_o;
    let b = miss_distance(range_m, deviation_deg);
    let (s, c) = (phi * std::f64::consts::TAU).sin_cos();
    let base = Vec2::new(o.x + u.x * a, o.y + u.y * a);
    Detonation {
        at: Vec2::new(base.x + u.x * (b * c - a), base.y + u.y * (b * c - a)),
        height_m: (b * s).abs(),
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

/// WHAT A BODY COSTS A PUNCH-THROUGH BUDGET, in metres of material — and it is
/// NOT twice [`BODY_RADIUS_M`], because the two are different quantities with
/// different sources.
///
/// The radius above is MEASURED (M46): walking into an enemy stops at 0.4 m
/// centre to centre, so a body occupies 0.2 m of floor. This one is PUBLISHED,
/// and the wiki's Punch Through page gives it twice over:
///
/// - *"The torso hitbox of three butchers combined adds up to over 1.2m of
///   material"* — so over 0.4 m each; and
/// - the "Minimum Mod Ranks for Penetration" table, which is the sharp one.
///
/// THE TABLE PINS IT. Every one of its thirteen humanoid cells is reproduced by
/// a single threshold of 0.5 m, and the table brackets the value from both
/// sides — the largest rank that FAILS is 0.4 and the smallest that WORKS is
/// 0.5, so the true figure is in (0.4, 0.5] and 0.5 is the only round number
/// in it:
///
/// | mod                          | largest ✗ | smallest ✓ |
/// |------------------------------|-----------|------------|
/// | Shred / Seeking Fury         | 0.4       | 0.6        |
/// | Primed Shred                 | 0.4       | 0.6        |
/// | Vigilante Offense            | 0.25      | **0.5**    |
/// | Power Throw                  | 0.3       | 0.7        |
/// | Metal Auger / Seeker         | 0.4       | 0.7        |
///
/// `a_body_costs_what_the_wiki_table_says` asserts the whole table.
///
/// WHY NOT MOVE THE RADIUS INSTEAD. Deriving this from the radius would mean
/// raising it to 0.25 m, which overwrites an in-game measurement the owner took
/// himself with a table whose own note says *"Average data, result will differ
/// due to width variances"* — and would move every distance-dependent number on
/// the board by 0.05 m for the privilege. Two facts about one body, each kept
/// at its own source. The property that motivated the question survives either
/// way: crossing a body costs 0.5, so 0.5 m of punch-through reaches the SECOND
/// of two adjacent enemies, which is exactly what the table says.
///
/// A FLAT COST, not a chord. The table publishes ONE number per enemy type and
/// warns that the real thing varies with width; charging a chord would be a
/// geometry this engine invented, and it would contradict the table for every
/// shot that is not dead centre. QUADRUPEDS are out of scope — the table's own
/// rows for them disagree with each other (Power Throw's 0.7 penetrates where
/// Vigilante Offense's 0.75 does not), which is that caveat showing.
pub const BODY_MATERIAL_M: f64 = 0.5;

/// THE FURTHEST ANY AREA EFFECT REACHES — a gas cloud at full stacks (6 m) plus
/// a body radius, which is more than the Tesla chain's 3 m and the Blast
/// detonation's 5 m.
///
/// It is the truncation for [`Neighbours`]: past it no mechanic in this engine
/// can hand anything to anybody, so the list stops there.
pub const AREA_MAX_M: f64 = 6.0 + BODY_RADIUS_M;

/// WHO IS NEAR WHOM — computed once, because nothing in this arena moves.
///
/// A gas cloud, a Tesla arc and a Blast detonation each hand a payload to every
/// body within a radius of the one that produced it. Asked directly that is
/// `O(bodies)` per proc, and a dense grid produces thousands of procs a second:
/// the Phantasma Prime on a 19x19 ruler went from 88 ms a run to 9,551 —
/// 108x — entirely on that scan (measured 2026-08-18).
///
/// The question is a CONSTANT. This answers it once per run: per body, its
/// neighbours within [`AREA_MAX_M`], NEAREST FIRST, so a lookup at any smaller
/// radius is a prefix of the list and stops at the first body out of range.
///
/// Same shape and same reason as [`crate::chain::Layout`], and deliberately
/// separate: a chain's reach is the WEAPON's and can be 30 m, while these three
/// are the ELEMENT's and are fixed.
#[derive(Debug, Clone, Default)]
pub struct Neighbours {
    near: Vec<Vec<(f64, u32)>>,
}

impl Neighbours {
    /// O(N^2) once — 130k distance computations for a 361-body grid, against
    /// the millions the per-proc scan was doing.
    pub fn build(bodies: &[Vec2]) -> Self {
        Self {
            near: bodies
                .iter()
                .map(|b| {
                    let mut v: Vec<(f64, u32)> = bodies
                        .iter()
                        .enumerate()
                        .filter_map(|(j, o)| {
                            let d = b.distance(*o);
                            (d <= AREA_MAX_M + 1e-9).then_some((d, j as u32))
                        })
                        .collect();
                    v.sort_by(|x, y| {
                        x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal).then(x.1.cmp(&y.1))
                    });
                    v
                })
                .collect(),
        }
    }

    /// Every body within `radius_m` of body `from`, itself included — ANY PART
    /// TOUCHING IS ENOUGH, the same rule every sphere in this engine uses
    /// ([`caught_by_blast`]).
    ///
    /// The list is nearest-first, so this stops at the first body out of range
    /// rather than walking the formation.
    pub fn within(&self, from: usize, radius_m: f64) -> impl Iterator<Item = usize> + '_ {
        let reach = radius_m + BODY_RADIUS_M + 1e-9;
        self.near
            .get(from)
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .take_while(move |(d, _)| *d <= reach)
            .map(|(_, j)| *j as usize)
    }
}

/// EVERY BODY A SHOT PASSES THROUGH, in the order the ray meets them.
///
/// The generalisation of [`first_hit`] to a weapon that does not stop at the
/// first body: *"the total distance of material (object or enemy) that a
/// weapon's projectile, bullet or beam can pass through before dissipating"*.
/// Each body crossed spends [`BODY_MATERIAL_M`] of the budget, so `n` bodies
/// are struck where `n - 1` crossings fit — a budget of zero is the ordinary
/// one-body shot this engine has always fired, and `first_hit` is this function
/// with no budget.
///
/// THE ARENA HAS NO COVER, which is what makes the model complete rather than
/// approximate here: the page's one qualifier on innate punch-through — *"does
/// not apply to surfaces"* — separates bodies from geometry, and this floor has
/// no geometry. Everything the ray meets is a body.
pub fn struck_along(muzzle: Vec2, dir: Vec2, bodies: &[Vec2], punch_through_m: f64) -> Vec<usize> {
    let len = dir.x.hypot(dir.y);
    if len <= 0.0 {
        return Vec::new();
    }
    let (ux, uy) = (dir.x / len, dir.y / len);
    // Everything on the line, nearest first — the same test `first_hit` makes,
    // asked of every body rather than of the best one.
    let mut on_line: Vec<(f64, usize)> = bodies
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            let (px, py) = (b.x - muzzle.x, b.y - muzzle.y);
            let along = px * ux + py * uy;
            if along < 0.0 {
                return None;
            }
            let perp = (px * uy - py * ux).abs();
            if perp > BODY_RADIUS_M {
                return None;
            }
            let half = (BODY_RADIUS_M * BODY_RADIUS_M - perp * perp).max(0.0).sqrt();
            Some(((along - half).max(0.0), i))
        })
        .collect();
    // By entry distance, then by index — the same determinism rule the chain
    // follows, for the same reason: a tie the game breaks in world-space order
    // is not reproducible, and a fixed order is.
    on_line.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.cmp(&b.1)));
    // HOW MANY THE BUDGET REACHES. The first is free — it is the contact the
    // shot was going to make anyway — and each one after it is paid for by
    // crossing the one in front.
    let reach = 1 + (punch_through_m.max(0.0) / BODY_MATERIAL_M + 1e-9).floor() as usize;
    on_line.into_iter().take(reach).map(|(_, i)| i).collect()
}


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

    /// THE WHOLE "Minimum Mod Ranks for Penetration" TABLE, humanoid rows.
    ///
    /// Every published rank of every mod on it, with the wiki's own verdict —
    /// thirteen cells, one threshold, no exceptions. It is what pins
    /// [`BODY_MATERIAL_M`] to 0.5 rather than to any other number: 0.4 fails on
    /// three independent mods and 0.5 works on one, so the value is in
    /// (0.4, 0.5].
    #[test]
    fn a_body_costs_what_the_wiki_table_says() {
        // (punch-through metres, does it reach a SECOND humanoid)
        let table: &[(f64, bool)] = &[
            // Seeking Fury / Shred / Merciless Gunfight
            (0.2, false), (0.4, false), (0.6, true), (0.8, true), (1.0, true), (1.2, true),
            // Vigilante Offense — the row that pins the value from above
            (0.25, false), (0.5, true), (0.75, true), (1.0, true), (1.25, true), (1.5, true),
            // Power Throw
            (0.3, false), (0.7, true), (1.0, true), (1.3, true), (1.7, true), (2.0, true),
            // Metal Auger / Seeker / Seeking Force
            (0.4, false), (0.7, true), (1.1, true), (1.4, true), (1.8, true), (2.1, true),
            // Primed Shred
            (0.2, false), (0.4, false), (0.6, true), (0.8, true), (1.0, true), (1.2, true),
            (1.4, true), (1.6, true), (1.8, true), (2.0, true), (2.2, true),
        ];
        // Two bodies in a line, as close as they go.
        let muzzle = Vec2::new(0.0, BODY_RADIUS_M);
        let dir = Vec2::new(0.0, 1.0);
        let bodies = [Vec2::new(0.0, 3.0), Vec2::new(0.0, 3.0 + CONTACT_RANGE_M)];
        for &(pt, second) in table {
            let hit = struck_along(muzzle, dir, &bodies, pt);
            assert_eq!(
                hit.len(), if second { 2 } else { 1 },
                "{pt} m of punch through: the wiki says the second body is {}",
                if second { "reached" } else { "not reached" }
            );
            assert_eq!(hit[0], 0, "the near body is always the first one struck");
        }
    }

    /// ...AND THE ORDER IS THE RAY'S, not the list's. A formation is stored in
    /// whatever order it was built in, so a shot that crosses three bodies must
    /// sort them by where it MEETS them — the first is the one that may keep a
    /// blast, and the rest are paid for in front-to-back order.
    #[test]
    fn punched_bodies_come_back_in_the_order_the_ray_meets_them() {
        let muzzle = Vec2::new(0.0, BODY_RADIUS_M);
        let dir = Vec2::new(0.0, 1.0);
        // Deliberately stored far, near, middle.
        let bodies = [Vec2::new(0.0, 9.0), Vec2::new(0.0, 3.0), Vec2::new(0.0, 6.0)];
        assert_eq!(struck_along(muzzle, dir, &bodies, 1.0), vec![1, 2, 0]);
        // A budget that pays for one crossing reaches two of them.
        assert_eq!(struck_along(muzzle, dir, &bodies, 0.5), vec![1, 2]);
        // …and none at all is the shot this engine has always fired.
        assert_eq!(struck_along(muzzle, dir, &bodies, 0.0), vec![1]);
    }

    /// A BODY OFF THE LINE IS NOT CROSSED however much punch through is on the
    /// weapon: it penetrates material in FRONT of it, it does not spread.
    #[test]
    fn punch_through_does_not_widen_the_shot() {
        let muzzle = Vec2::new(0.0, BODY_RADIUS_M);
        let dir = Vec2::new(0.0, 1.0);
        let bodies = [Vec2::new(0.0, 3.0), Vec2::new(5.0, 6.0)];
        assert_eq!(struck_along(muzzle, dir, &bodies, 99.0), vec![0]);
    }

    /// WITH NO BUDGET IT IS `first_hit`, which is what keeps every fight this
    /// engine has ever run byte-identical.
    #[test]
    fn no_punch_through_is_the_shot_this_engine_already_fired() {
        let muzzle = Vec2::new(0.0, BODY_RADIUS_M);
        let dir = Vec2::new(1.0, 4.0);
        let bodies = [Vec2::new(2.0, 8.0), Vec2::new(1.0, 4.2), Vec2::new(-9.0, 1.0)];
        let first = first_hit(muzzle, dir, &bodies).map(|(i, _)| i);
        assert_eq!(struck_along(muzzle, dir, &bodies, 0.0).first().copied(), first);
    }

    /// POINTING SOMEWHERE ELSE IS A REAL ANGLE, and pointing AT a body is zero
    /// of it — which is what keeps every fight this engine ran byte-identical.
    #[test]
    fn a_body_off_the_aim_line_has_an_angle_and_one_on_it_has_none() {
        let m = Vec2::ORIGIN;
        let foe = Vec2::new(0.0, 10.0);
        assert_eq!(off_axis_deg(m, foe, foe), 0.0);
        assert_eq!(off_axis_deg(m, Vec2::new(0.0, 40.0), foe), 0.0, "further along the same line");
        assert!((off_axis_deg(m, Vec2::new(10.0, 10.0), foe) - 45.0).abs() < 1e-9);
        // Degenerate inputs answer zero rather than NaN.
        assert_eq!(off_axis_deg(m, m, foe), 0.0);

        // …AND THE OFFSET COLLAPSES TO THE ON-AXIS ONE when it is zero, at
        // every phi, which is the property the old numbers rest on.
        for phi in [0.0, 0.25, 0.5, 0.7] {
            assert_eq!(miss_distance_off_axis(20.0, 0.0, 2.0, phi), miss_distance(20.0, 2.0));
        }
        // Off axis it is a vector sum: pointing 2 degrees away and deviating 2
        // degrees BACK lands on the body, and deviating 2 degrees further out
        // doubles the miss.
        let back = miss_distance_off_axis(20.0, 2.0, 2.0, 0.0);
        let away = miss_distance_off_axis(20.0, 2.0, 2.0, 0.5);
        assert!(back < 1e-9, "{back}");
        assert!((away - 2.0 * miss_distance(20.0, 2.0)).abs() < 1e-9, "{away}");
    }

    /// THE THREE BLAST RULES, which are one idea: a body is a CIRCLE and a
    /// blast meets it at its nearest surface (owner, 2026-08-17).
    /// THE DETONATION AND THE SCALAR ARE ONE MODEL, and this is the property
    /// that says so: however wide the shot and whichever way it went, the
    /// distance from the epicentre to the AIMED body is the miss distance the
    /// engine has always computed. So giving the crowd a real epicentre cannot
    /// move the aimed body's number — the two are the same two vectors, kept
    /// apart instead of collapsed.
    #[test]
    fn a_detonation_is_the_miss_distance_it_came_from() {
        let shooter = Vec2::new(0.0, 0.5);
        for &(bx, by) in &[(0.0, 10.0), (3.0, 10.0), (-4.0, 7.0), (0.2, 25.0)] {
            let body = Vec2::new(bx, by);
            for &aim in &[body, Vec2::new(2.0, 10.0), Vec2::new(-5.0, 9.0)] {
                // The caller's own two steps: the shot leaves the MUZZLE, and
                // the range is measured from the shooter (which steps to the
                // muzzle itself — handing it one subtracts a radius twice).
                let muzzle = muzzle(shooter, aim);
                let range = range_to_centre(shooter, body);
                let off = off_axis_deg(muzzle, aim, body);
                for dev in [0.0, 1.0, 5.0, 20.0] {
                    for phi in [0.0, 0.125, 0.25, 0.5, 0.75, 0.9] {
                        let d = detonation_of_miss(muzzle, aim, body, range, off, dev, phi);
                        let scalar = miss_distance_off_axis(range, off, dev, phi);
                        // EXACT where the weapon points AT the body, which is
                        // every fight this engine ran before aim became a place
                        // you choose — so no existing number can move.
                        //
                        // Pointed ELSEWHERE the two differ by about a
                        // millimetre, and it is the SCALAR that is loose: it
                        // uses `range_m` as the lever for an angle measured at
                        // the muzzle, while the muzzle steps toward the AIM
                        // rather than toward the body. Reconstructing the frame
                        // does not inherit that, and the difference is four
                        // orders of magnitude under the resolution of anything
                        // this engine is calibrated against.
                        // …and RELATIVE past that, because the gap grows with
                        // the angle. THE EXACT ARM IS THE ONE THAT MATTERS —
                        // it is every fight before aim became a place, so this
                        // test's real job is to pin that at 1e-9. Off-axis it
                        // only confirms the two stay the same model: the worst
                        // case in this grid is 1.27%, at 49 degrees off with a
                        // 20 degree cone, which is a shot pointed nowhere near
                        // the body it is being measured against.
                        let tol = if off <= 0.0 { 1e-9 } else { 0.02 * scalar.max(1.0) + 0.02 };
                        assert!(
                            (d.distance_to(body) - scalar).abs() < tol,
                            "body {body:?} aim {aim:?} off {off} dev {dev} phi {phi}:                              detonation says {}, the scalar says {scalar}",
                            d.distance_to(body),
                        );
                    }
                }
            }
        }
    }

    /// …AND A CROWD IS WHY IT HAD TO EXIST. A shot that goes wide moves the
    /// epicentre AWAY from a bystander standing beside the target, which is
    /// the whole of the bug this replaced: the bystander used to read its
    /// distance from the aimed body's surface and take a direct hit's blast
    /// off a shot that went nowhere near it.
    #[test]
    fn a_wide_shot_moves_the_epicentre_off_the_bystander_too() {
        let shooter = Vec2::new(0.0, 0.0);
        let target = Vec2::new(0.0, 10.0);
        let bystander = Vec2::new(0.0, 12.0);
        let muzzle = muzzle(shooter, target);
        let range = range_to_centre(shooter, target);
        // phi = 0 sends the pellet along the floor, away from the aim line.
        let near = detonation_of_miss(muzzle, target, target, range, 0.0, 0.0, 0.0);
        let wide = detonation_of_miss(muzzle, target, target, range, 0.0, 30.0, 0.0);
        assert!(near.distance_to(bystander) < wide.distance_to(bystander),
            "a wide shot must be farther from the bystander: {} vs {}",
            near.distance_to(bystander), wide.distance_to(bystander));
        // …and straight up is still a real distance to everyone on the floor.
        let over = detonation_of_miss(muzzle, target, target, range, 0.0, 30.0, 0.25);
        assert!(over.height_m > 1.0, "height {}", over.height_m);
        assert!(over.distance_to(bystander) > near.distance_to(bystander));
    }

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
