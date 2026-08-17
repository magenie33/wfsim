//! THE FORMATION — more than one body on the floor, and which one is being shot.
//!
//! The arena has held exactly one target since this engine was written, and
//! every golden value rests on that. This is the layer that makes the count a
//! number instead of a constant, and it is deliberately SMALL: a formation is a
//! list of bodies plus the index of the one being aimed at, and the whole of
//! what it decides is WHO. What each body then is — its pools, its armor, its
//! hitboxes, its debuffs — stays exactly what it was.
//!
//! # The aim policy
//!
//! **ONE TARGET AT A TIME, AND YOU SWITCH ONLY WHEN IT DIES** (owner,
//! 2026-08-15, decided before this module existed so it would not be invented
//! by whoever wrote it). "Who do you shoot at" is a PLAY PATTERN, the same
//! class of decision as reload interruption, and docs/UNMODELLED.md is explicit
//! that this repo refuses to invent one.
//!
//! It buys two things. The fight stays CONTINUOUS with the single-target
//! behaviour every golden value and both boards rest on — with one body the
//! policy never fires. And it makes punch-through, explosions and chaining
//! pure upside rather than something a re-targeting rule is quietly optimising
//! around.
//!
//! WHICH ONE NEXT is the model's own choice and not the owner's: the NEAREST
//! LIVE body to the player. It is the only rule that needs no state, and a
//! player who has just lost their target looks at what is in front of them.
//! `retarget` is the one place it lives.
//!
//! # What a formation does NOT do
//!
//! Move. Bodies stand where they are put, which is what makes a chain's path
//! fixed (MEASUREMENTS M52) and what the whole 2D arena assumes today.

use crate::space::Vec2;

/// HOW MANY BODIES A FORMATION MAY HOLD — fifty (owner, 2026-08-17).
///
/// DECLARED HERE and read by everyone: the api refuses a longer list and
/// `RunResult::damage_by_body` is sized off it. It is the SIM that pays, which
/// is why the number belongs to the engine rather than to the page — every
/// body is a full target with its own pools, procs and DoTs, and a chain
/// resolves against all of them on every shot.
/// FOUR HUNDRED, and the number is MEASURED rather than chosen (2026-08-17).
///
/// It was 50 while the arena was learning to hold more than two bodies, which
/// was the right number for "some, not unbounded" and the wrong one the moment
/// a CROWD RULER was sized: `formation_cost` says the roster's largest blast —
/// the Morgha alt's 12 m — stops growing at a 17x17 grid, which is 289 bodies.
/// A cap under that would have made the ARENA the thing being measured.
///
/// RAISING IT IS FREE, which is the part that had to be checked rather than
/// assumed: `RunResult` is `Copy` and carries a `[f64; MAX_BODIES + 1]`, so the
/// worry was a fixed cost paid by every fight including the single-target ones
/// the board already runs. Measured at 400 against 50, a one-body engagement
/// costs 0.538 ms against 0.533 — inside the noise of the machine. The array is
/// 3.2 KB and a run produces one.
///
/// 400 rather than 289: a 19x19 is 361, and that is the size the measurement
/// settles on — where the roster's LARGEST blast (the Morgha alt's 12 m) stops
/// growing at 110 bodies and stays there through 21x21 and 23x23. Past 19 the
/// only thing more rows change is how deep an infinite-punch-through weapon's
/// column runs, which is the one thing that should not grow.
pub const MAX_BODIES: usize = 400;

/// One body in the formation, as the caller declares it — everything that
/// makes it a target, and where it stands.
#[derive(Debug, Clone)]
pub struct FoeSpec {
    /// WHO THIS ONE IS, and it is a name rather than a position.
    ///
    /// A formation body was identified only by its index in the list until
    /// 2026-08-17, which is enough for the ENGINE — it reads bodies by index
    /// and always will — and not enough for anything that has to talk ABOUT
    /// one. Every debuff, every pool and every DoT is already this body's own
    /// (`dummy::SpreadFoe`); what was missing was a way to say WHOSE, so a
    /// damage figure, a canvas label and a replay could name the same enemy
    /// (owner, 2026-08-17).
    ///
    /// STABLE ACROSS EDITS: it travels in the scenario, so deleting the body in
    /// front does not rename the one behind. A blank id is filled in by
    /// position at parse time, which is what every scenario written before this
    /// existed means and what keeps them all readable.
    pub id: String,
    pub params: crate::dummy::TargetParams,
    /// Where a pellet can land on it and what each spot multiplies.
    pub body_parts: Vec<crate::dummy::BodyPart>,
    pub at: Vec2,
}

/// The formation as the sim holds it: who is where, and who is being shot.
#[derive(Debug, Clone)]
pub struct Formation {
    pub foes: Vec<FoeSpec>,
    /// Index into `foes` — the body the beam is on. Every other body is
    /// reached only by what SPREADS: a damage radius, a chain, an explosion.
    pub aimed: usize,
}

impl Formation {
    /// The single-target arena, which is what this engine has always run.
    /// `Formation::one(..).len() == 1` and the aim policy can never fire.
    pub fn one(params: crate::dummy::TargetParams, body_parts: Vec<crate::dummy::BodyPart>, at: Vec2) -> Self {
        Self { foes: vec![FoeSpec { id: "e1".into(), params, body_parts, at }], aimed: 0 }
    }

    pub fn len(&self) -> usize {
        self.foes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.foes.is_empty()
    }
    /// Where every body stands, in index order — what [`crate::chain::resolve`]
    /// takes.
    pub fn positions(&self) -> Vec<Vec2> {
        self.foes.iter().map(|f| f.at).collect()
    }

    /// WHO TO SHOOT NEXT once the aimed body is down: the nearest LIVE one to
    /// the player, or `None` when the formation is spent.
    ///
    /// `alive` is asked rather than stored because liveness belongs to the run,
    /// not to the layout — the same formation is fought a thousand times in a
    /// Monte Carlo and its bodies stand in the same places every time.
    pub fn retarget(&self, player_at: Vec2, alive: impl Fn(usize) -> bool) -> Option<usize> {
        (0..self.foes.len())
            .filter(|&i| alive(i))
            .min_by(|&a, &b| {
                // Ties by INDEX, for the same reason `chain::resolve` breaks
                // them that way: arbitrary is fine, unstable is not.
                self.foes[a]
                    .at
                    .distance(player_at)
                    .partial_cmp(&self.foes[b].at.distance(player_at))
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.cmp(&b))
            })
    }

    /// A GRID, which is the shape a formation is measured in — `cols x rows` at
    /// `spacing` metres, all of one kind, the front row nearest the player.
    ///
    /// The front row's MIDDLE body is the one aimed at, because that is the one
    /// a player can actually put a beam on: the centre of a formation is behind
    /// it (owner, 2026-08-17). It is the fixture MECHANICS §12's numbers are
    /// computed against.
    /// THE POSITIONS OF A GRID BUILT AROUND THE BODY BEING AIMED AT, index 0
    /// being that body — the shape a SCENARIO describes.
    ///
    /// [`Self::grid`] takes a front-row corner and makes a whole formation;
    /// this takes the aimed body's own place and lays the rest out around it,
    /// because a fight already knows where its target stands. The front row is
    /// centred on it and every other row is one spacing further along
    /// `forward`, which is the direction the shot travels — so "the shooter is
    /// at the middle of an edge" is not a special case, it is what falls out of
    /// the aim line being perpendicular to the front rank.
    ///
    /// It exists so a benchmark can state a crowd in THREE NUMBERS. 361 bodies
    /// written out is 360 lines nobody can check by reading, and a ruler whose
    /// terms cannot be argued with is not a ruler (owner, 2026-08-17).
    pub fn grid_around(front_middle: Vec2, forward: Vec2, cols: usize, rows: usize, spacing: f64)
        -> Vec<Vec2>
    {
        let len = forward.x.hypot(forward.y);
        // A zero direction has no grid to build; one row deep is the same
        // arrangement whatever it points at.
        let (fx, fy) = if len > 0.0 { (forward.x / len, forward.y / len) } else { (0.0, 1.0) };
        // ACROSS is the perpendicular, so the front rank faces the shooter.
        let (ax, ay) = (fy, -fx);
        let mut out = Vec::with_capacity(cols * rows);
        for r in 0..rows {
            for c in 0..cols {
                let across = (c as f64 - (cols as f64 - 1.0) / 2.0) * spacing;
                let along = r as f64 * spacing;
                out.push(Vec2::new(
                    front_middle.x + ax * across + fx * along,
                    front_middle.y + ay * across + fy * along,
                ));
            }
        }
        // THE AIMED BODY FIRST, which is the order a scenario wants: its
        // `target_at` is index 0 and `formation` is the rest.
        let aimed = cols / 2;
        out.swap(0, aimed);
        out
    }

    pub fn grid(
        params: crate::dummy::TargetParams,
        body_parts: Vec<crate::dummy::BodyPart>,
        cols: usize,
        rows: usize,
        spacing: f64,
        front_at: Vec2,
    ) -> Self {
        let mut foes = Vec::with_capacity(cols * rows);
        for r in 0..rows {
            for c in 0..cols {
                foes.push(FoeSpec {
                    id: format!("e{}", r * cols + c + 1),
                    params: params.clone(),
                    body_parts: body_parts.clone(),
                    // x across the front, y away from the player.
                    at: Vec2::new(
                        front_at.x + (c as f64 - (cols as f64 - 1.0) / 2.0) * spacing,
                        front_at.y + r as f64 * spacing,
                    ),
                });
            }
        }
        let aimed = cols / 2;
        Self { foes, aimed }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> (crate::dummy::TargetParams, Vec<crate::dummy::BodyPart>) {
        (
            crate::dummy::TargetParams::training_dummy(),
            crate::dummy::DummyParams::humanoid_parts(),
        )
    }

    /// ONE BODY IS STILL THE FIGHT THIS ENGINE RUNS, and the aim policy cannot
    /// fire in it — which is what keeps every golden value where it is.
    #[test]
    fn a_formation_of_one_is_the_arena_this_engine_has_always_had() {
        let (p, b) = spec();
        let f = Formation::one(p, b, Vec2::new(0.0, 0.4));
        assert_eq!(f.len(), 1);
        assert_eq!(f.aimed, 0);
        assert_eq!(f.positions(), vec![Vec2::new(0.0, 0.4)]);
        // Nobody left once it is down.
        assert_eq!(f.retarget(Vec2::ORIGIN, |_| false), None);
        assert_eq!(f.retarget(Vec2::ORIGIN, |_| true), Some(0));
    }

    /// THE NEAREST LIVE BODY, and it never picks a corpse.
    #[test]
    fn retargeting_takes_the_nearest_body_still_standing() {
        let (p, b) = spec();
        let f = Formation {
            foes: vec![
                FoeSpec { id: String::new(), params: p.clone(), body_parts: b.clone(), at: Vec2::new(0.0, 10.0) },
                FoeSpec { id: String::new(), params: p.clone(), body_parts: b.clone(), at: Vec2::new(0.0, 3.0) },
                FoeSpec { id: String::new(), params: p, body_parts: b, at: Vec2::new(0.0, 6.0) },
            ],
            aimed: 0,
        };
        assert_eq!(f.retarget(Vec2::ORIGIN, |_| true), Some(1));
        // …and with the nearest one down, the next nearest.
        assert_eq!(f.retarget(Vec2::ORIGIN, |i| i != 1), Some(2));
        assert_eq!(f.retarget(Vec2::ORIGIN, |i| i == 0), Some(0));
    }

    /// A GRID PUTS THE AIM ON THE FRONT ROW'S MIDDLE, because the centre of a
    /// formation is behind it and cannot be shot at.
    #[test]
    fn a_grid_is_aimed_at_the_front_rows_middle_body() {
        let (p, b) = spec();
        let f = Formation::grid(p, b, 3, 3, 3.0, Vec2::new(0.0, 5.0));
        assert_eq!(f.len(), 9);
        // Front row is y = 5, and the aimed body is its middle one.
        assert_eq!(f.foes[f.aimed].at, Vec2::new(0.0, 5.0));
        assert_eq!(f.foes[0].at, Vec2::new(-3.0, 5.0));
        assert_eq!(f.foes[2].at, Vec2::new(3.0, 5.0));
        // …and rows go AWAY from the player.
        assert_eq!(f.foes[4].at, Vec2::new(0.0, 8.0));
        assert_eq!(f.foes[8].at, Vec2::new(3.0, 11.0));
    }

    /// …AND THE GRID FEEDS THE CHAIN THE NUMBERS MECHANICS §12 STATES. A 3 x 3
    /// at 3 m under Primed Firestorm is four seeds and 13.15 damage; bare it is
    /// one seed and 3.29. The fixture and the mechanic are asserted together so
    /// neither can drift from the documented figure alone.
    #[test]
    fn the_grid_fixture_reproduces_the_documented_chain_totals() {
        let (p, b) = spec();
        let f = Formation::grid(p, b, 3, 3, 3.0, Vec2::new(0.0, 5.0));
        let torid = crate::chain::Spec { hops: 5, range_m: 7.0, falloff: 0.75, compounds: true };
        let at = f.foes[f.aimed].at;
        let total = |radius_m: f64| -> f64 {
            crate::chain::resolve(
                &f.positions(),
                &[f.aimed],
                crate::chain::Splash { at, radius_m },
                torid,
            )
            .iter()
            .map(|i| i.share)
            .sum()
        };
        assert!((total(2.3) - 3.2881).abs() < 1e-3, "{}", total(2.3));
        assert!((total(2.3 * 1.44) - 13.1524).abs() < 1e-3, "{}", total(2.3 * 1.44));
    }
}
