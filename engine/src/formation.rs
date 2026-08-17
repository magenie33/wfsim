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
pub const MAX_BODIES: usize = 50;

/// One body in the formation, as the caller declares it — everything that
/// makes it a target, and where it stands.
#[derive(Debug, Clone)]
pub struct FoeSpec {
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
        Self { foes: vec![FoeSpec { params, body_parts, at }], aimed: 0 }
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
                FoeSpec { params: p.clone(), body_parts: b.clone(), at: Vec2::new(0.0, 10.0) },
                FoeSpec { params: p.clone(), body_parts: b.clone(), at: Vec2::new(0.0, 3.0) },
                FoeSpec { params: p, body_parts: b, at: Vec2::new(0.0, 6.0) },
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
        let torid = crate::chain::Spec { hops: 5, range_m: 7.0, falloff: 0.75 };
        let at = f.foes[f.aimed].at;
        let total = |radius_m: f64| -> f64 {
            crate::chain::resolve(
                &f.positions(),
                Some(f.aimed),
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
