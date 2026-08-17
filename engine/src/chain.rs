//! BEAM CHAINING — where ONE shot lands when there is more than one body.
//!
//! A chaining beam is the first mechanic in this engine that cannot be asked
//! about a single target: every clause of it is a statement about a FORMATION.
//! This module answers the geometric half and nothing else — given where the
//! bodies are and where the shot landed, which of them takes a damage
//! instance, at what share of the direct hit, and under which rules. What each
//! instance then DOES is the ordinary damage pipeline's business, because a
//! chain hop is not a special kind of hit: it is *"a beam with a smaller base
//! damage"* (owner, 2026-08-17), so its crit, its status, its procs and their
//! DoTs all fall out of the share and need no rule of their own.
//!
//! # The mechanic
//!
//! Sourced from the Torid Incarnon's page, which states it more completely than
//! the general `Continuous_Weapon` page does. The Amprex (3 hops / 10 m / 0.5)
//! and the Kuva Nukor (2 / 9 m / 0.5) are the same shape with other constants,
//! which is why nothing here is named after a weapon.
//!
//! **THE DAMAGE RADIUS SEEDS THE CHAINS.** *"The beam will chain independently
//! to 5 additional enemies starting from EACH target hit by the initial damage
//! radius. Each chain chooses targets independently, and an enemy can be struck
//! by multiple chains."* So a splash that catches four bodies starts four
//! chains, not one — and that, rather than the splash damage itself, is what a
//! radius mod is buying (see the numbers in docs/MECHANICS.md §12).
//!
//! **A PATH VISITS NOBODY TWICE** (owner, 2026-08-16). "Struck by multiple
//! chains" means the repeats come from DISTINCT paths rather than from a path
//! looping back on itself. It is what makes the arithmetic well-defined, and it
//! separates from the alternative at three targets, because the falloff
//! compounds ALONG a path.
//!
//! **THE NEXT HOP IS THE NEAREST VIABLE TARGET** (owner, 2026-08-17), and this
//! is the one clause with a MEASUREMENT behind it: ten hops read off two paths
//! in the Simulacrum went to an orthogonal neighbour every time, never to a
//! diagonal and never past a nearer body (MEASUREMENTS M52).
//!
//! **AND THE TIE-BREAK IS NOT REPRODUCIBLE, SO IT IS NOT REPRODUCED.** Nine of
//! those ten hops were exact ties, and no rule expressible in the formation's
//! own geometry fits them: a fixed compass priority scores 8 of 10 over all 24
//! orderings, a turn preference 8 of 10 over all 96, entity-index order 4 to 7.
//! The same situation resolved two ways — from (3,1) the path went straight
//! where from (4,1) it turned — so nothing that reads only relative positions
//! can be right.
//!
//! The owner's own observation says why: a NON-HUMANOID model changes the path
//! while leaving every relative position identical. What changes with the model
//! is the collider, so the order is the game's spatial query returning bodies
//! in world-space broadphase order — which depends on which cell each body
//! falls into, and is not a function of the formation at all.
//!
//! WHAT IS GUARANTEED HERE INSTEAD (owner, 2026-08-17): *"if the enemies never
//! move, the chain path is always fixed"*. That is the property [`resolve`]
//! has — ties go to the lowest body index, so one formation always produces one
//! path. It is arbitrary and it is stable, which is the honest pair when the
//! real rule is unknowable. And it costs nothing that matters: the TOTAL is
//! invariant to tie-breaking (`the_total_is_invariant_to_tie_breaks`), so the
//! part nobody can know moves damage between bodies without changing how much
//! the formation took.
//!
//! **NO LINE OF SIGHT** (owner, 2026-08-17) — a hop is a distance test and
//! nothing else, so a body behind another is as reachable as one beside it.
//!
//! **ONLY THE DIRECTLY STRUCK TARGET CAN BE HEADSHOT** (owner, 2026-08-17).
//! Everything the splash catches and everything a chain reaches lands on the
//! body. It is the clause with the most consequence for build ranking: in a
//! 3 x 3 formation at 3 m under Primed Firestorm, one of TWENTY-FOUR instances
//! is headshot-eligible and it carries 7.6% of the damage — so a build leaning
//! on a head multiplier keeps almost none of it in a crowd, while a status
//! build collects 24 rolls at full chance.
//!
//! **MULTISHOT FOLLOWS THE SEED, AND CARRIES DOWN ITS WHOLE PATH.** Verbatim:
//! *"the spherical damage radius does not benefit from Multishot; only targets
//! directly hit by the beam benefit"*, and *"due to the damage radius not
//! benefiting from multishot, beams chaining from targets that were in the
//! damage radius but not directly struck by the initial beam itself will also
//! not benefit from multishot"*. So the flag is decided once per seed and
//! inherited by every hop that seed launches.
//!
//! # What this module does NOT decide
//!
//! Whether a radius mod widens the CHAIN range. It does not here (owner,
//! 2026-08-17): [`Splash::radius_m`] is the modded value and [`Spec::range_m`]
//! is not touched by it. Two wiki pages disagree about this and the weapon's
//! own page is the one followed — see docs/MECHANICS.md §12.

use crate::space::Vec2;

/// The chain's three constants — per weapon, and the whole of what differs
/// between the Torid, the Amprex and the Kuva Nukor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spec {
    /// How many hops one path may take. The Torid's 5.
    pub hops: u32,
    /// How far a hop may reach, metres, measured between the two bodies.
    pub range_m: f64,
    /// What each hop deals relative to the hop before it. The Torid's 0.75.
    pub falloff: f64,
}

/// ONE DAMAGE INSTANCE the shot produced, and everything the geometry decides
/// about it. What it deals is `share` times whatever the direct hit deals.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Instance {
    /// Index into the formation handed to [`resolve`].
    pub target: usize,
    /// Fraction of the direct hit's damage — 1.0 for anything the splash
    /// caught, `falloff^k` for the k-th hop of a path.
    pub share: f64,
    /// Does merged-beam multishot reach it? Decided by the SEED and inherited
    /// down its path.
    pub multishot: bool,
    /// May `headshot_pct` be rolled for it? True for the directly struck
    /// target and for nothing else.
    pub headshot: bool,
}

/// Where the beam landed and how wide its damage radius is, AFTER mods.
#[derive(Debug, Clone, Copy)]
pub struct Splash {
    pub at: Vec2,
    pub radius_m: f64,
}

/// Every damage instance one shot produces against `bodies`.
///
/// `aimed` is the index the beam struck directly — the one instance that may
/// headshot, and the seed whose path carries multishot.
///
/// # Ties
///
/// A formation is full of exact ties: in a square grid every orthogonal
/// neighbour is the same distance away. They go to the LOWEST BODY INDEX, which
/// makes one formation always produce one path — the property the owner asked
/// for in place of reproducing a rule that is not reproducible (see the module
/// header, and MEASUREMENTS M52 for the ten hops that refuted every candidate).
///
/// [`resolve_with`] takes the tie-break as an argument, which is what the
/// invariance test uses; nothing in production should.
pub fn resolve(bodies: &[Vec2], aimed: Option<usize>, splash: Splash, spec: Spec) -> Vec<Instance> {
    resolve_with(bodies, aimed, splash, spec, &mut |_| 0)
}

/// [`resolve`] with the tie-break handed in — for asserting that it does not
/// change the total, and for nothing else.
pub fn resolve_with(
    bodies: &[Vec2],
    aimed: Option<usize>,
    splash: Splash,
    spec: Spec,
    tie: &mut impl FnMut(usize) -> usize,
) -> Vec<Instance> {
    let mut out = Vec::new();
    if bodies.is_empty() {
        return out;
    }
    // A SHOT THAT STRUCK NOBODY STILL SPLASHES. Aim is a direction and the
    // place it lands may be bare floor (owner, 2026-08-17) — *"a 2.3 meter
    // damage radius from the point of impact against a SURFACE"*, and a floor
    // is a surface. Every body the sphere catches is then an ordinary seed:
    // none was directly struck, so none may headshot and none carries
    // multishot, which is the same rule the radius-caught seeds already follow.
    let aimed = aimed.filter(|&i| i < bodies.len());
    // THE SEEDS: everything the damage radius caught. The aimed body is always
    // one of them — the impact is on it.
    //
    // THE AIMED BODY FIRST, so instance 0 is always the direct hit — the one
    // that may headshot and the one multishot follows. The rest are a set with
    // no order the game gives them, and taking them in index order is this
    // module's choice rather than a fact.
    let mut seeds: Vec<usize> = aimed.into_iter().collect();
    seeds.extend((0..bodies.len()).filter(|&i| {
        // ANY PART OF A BODY TOUCHING THE SPHERE IS ENOUGH — the blast rule,
        // in one place (`space::caught_by_blast`, owner 2026-08-17). So the
        // radius a splash really seeds over is its own plus a body radius.
        Some(i) != aimed
            && crate::space::caught_by_blast(bodies[i].distance(splash.at), splash.radius_m)
    }));

    for &s in &seeds {
        // …AND THE SPLASH IS NOT A SECOND INSTANCE. "A target that is directly
        // struck by the beam is still only hit once", so a seed takes ONE
        // full-share instance whether the beam or the radius reached it.
        let multishot = Some(s) == aimed;
        out.push(Instance { target: s, share: 1.0, multishot, headshot: Some(s) == aimed });

        // …and then runs its own path.
        let (mut cur, mut share) = (s, 1.0);
        let mut seen = vec![false; bodies.len()];
        seen[s] = true;
        for _ in 0..spec.hops {
            let mut best = f64::INFINITY;
            let mut tied: Vec<usize> = Vec::new();
            for j in 0..bodies.len() {
                if seen[j] {
                    continue;
                }
                let d = bodies[cur].distance(bodies[j]);
                if d > spec.range_m + 1e-9 {
                    continue;
                }
                if d < best - 1e-9 {
                    best = d;
                    tied.clear();
                    tied.push(j);
                } else if (d - best).abs() <= 1e-9 {
                    tied.push(j);
                }
            }
            if tied.is_empty() {
                break;
            }
            let next = tied[tie(tied.len()).min(tied.len() - 1)];
            share *= spec.falloff;
            out.push(Instance { target: next, share, multishot, headshot: false });
            seen[next] = true;
            cur = next;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Torid Incarnon's own constants, which are the ones every number in
    /// docs/MECHANICS.md §12 was computed against.
    const TORID: Spec = Spec { hops: 5, range_m: 7.0, falloff: 0.75 };

    /// A 3 x 3 formation at 3 m, the fixture the owner chose (2026-08-17): the
    /// smallest arrangement dense enough that a five-hop path never runs out of
    /// targets, and sparse enough that the damage radius is the thing deciding
    /// how many chains start.
    fn grid(spacing: f64) -> Vec<Vec2> {
        (0..3)
            .flat_map(|r| (0..3).map(move |c| Vec2::new(c as f64 * spacing, r as f64 * spacing)))
            .collect()
    }
    /// The front row's middle body — the one a player can actually put a beam
    /// on. The centre of the formation is BEHIND it and cannot be aimed at
    /// (owner, 2026-08-17).
    const FRONT_MIDDLE: usize = 1;

    fn total(v: &[Instance]) -> f64 {
        v.iter().map(|i| i.share).sum()
    }

    /// A PATH'S WHOLE OUTPUT IS A CONSTANT once it can always find a next body:
    /// `1 + f + f² + … + f^hops`, which is 3.2881 for the Torid. The total is
    /// then seeds x that, and nothing about aim or tie-breaks moves it.
    #[test]
    fn one_seed_yields_its_own_instance_plus_a_full_path() {
        let v = resolve(
            &grid(3.0),
            Some(FRONT_MIDDLE),
            Splash { at: Vec2::new(3.0, 0.0), radius_m: 2.3 },
            TORID,
        );
        // 2.3 m does not reach a 3 m neighbour, so the aimed body is the only
        // seed and the shot is one instance plus five hops.
        assert_eq!(v.len(), 6);
        assert_eq!(v.iter().filter(|i| i.share == 1.0).count(), 1);
        let want: f64 = (0..6).map(|k| 0.75_f64.powi(k)).sum();
        assert!((total(&v) - want).abs() < 1e-9, "{} against {want}", total(&v));
        assert!((want - 3.2881).abs() < 1e-3);
    }

    /// PRIMED FIRESTORM BUYS SEEDS, and that is the whole of what it buys here.
    /// 2.3 x 1.44 = 3.31 m reaches the three neighbours at 3 m and not the two
    /// diagonals at 4.24 — four seeds, four paths, four times the damage.
    #[test]
    fn a_wider_radius_multiplies_the_shot_by_the_seeds_it_catches() {
        let bodies = grid(3.0);
        let at = Vec2::new(3.0, 0.0);
        let bare = resolve(&bodies, Some(FRONT_MIDDLE), Splash { at, radius_m: 2.3 }, TORID);
        let primed = resolve(
            &bodies,
            Some(FRONT_MIDDLE),
            Splash { at, radius_m: 2.3 * 1.44 },
            TORID,
        );
        assert_eq!(primed.iter().filter(|i| i.share == 1.0).count(), 4, "seeds");
        assert_eq!(primed.len(), 24, "4 seeds x (1 instance + 5 hops)");
        assert!((total(&primed) / total(&bare) - 4.0).abs() < 1e-9);
        assert!((total(&primed) - 13.15).abs() < 0.01, "{}", total(&primed));
    }

    /// ONE INSTANCE IN TWENTY-FOUR MAY HEADSHOT, and it carries 7.6% of the
    /// damage. The clause with the most consequence for ranking builds: a head
    /// multiplier is worth almost nothing in a crowd, where a status build
    /// collects every one of the 24 rolls at full chance.
    #[test]
    fn only_the_directly_struck_body_can_be_headshot() {
        let v = resolve(
            &grid(3.0),
            Some(FRONT_MIDDLE),
            Splash { at: Vec2::new(3.0, 0.0), radius_m: 2.3 * 1.44 },
            TORID,
        );
        let heads: Vec<&Instance> = v.iter().filter(|i| i.headshot).collect();
        assert_eq!(heads.len(), 1);
        assert_eq!(heads[0].target, FRONT_MIDDLE);
        assert_eq!(heads[0].share, 1.0);
        assert!((1.0 / total(&v) - 0.076).abs() < 0.002, "{}", 1.0 / total(&v));
    }

    /// MULTISHOT IS DECIDED BY THE SEED AND INHERITED DOWN ITS PATH — six of
    /// the 24, which is the aimed body's own instance and its five hops.
    #[test]
    fn multishot_follows_the_seed_rather_than_the_hit() {
        let v = resolve(
            &grid(3.0),
            Some(FRONT_MIDDLE),
            Splash { at: Vec2::new(3.0, 0.0), radius_m: 2.3 * 1.44 },
            TORID,
        );
        assert_eq!(v.iter().filter(|i| i.multishot).count(), 6);
        // …and it is one whole path, not six scattered instances: the seed's
        // own instance is the first of them.
        assert!(v[0].multishot && v[0].target == FRONT_MIDDLE && v[0].share == 1.0);
        assert!(v[..6].iter().all(|i| i.multishot));
        assert!(v[6..].iter().all(|i| !i.multishot));
    }

    /// A PATH VISITS NOBODY TWICE, which is the rule that makes three targets
    /// differ from two. Asserted as the property rather than as a number, so it
    /// holds for any formation.
    #[test]
    fn a_path_never_revisits_a_body() {
        let bodies = grid(3.0);
        for aimed in 0..bodies.len() {
            let v = resolve(
                &bodies,
                Some(aimed),
                Splash { at: bodies[aimed], radius_m: 2.3 * 1.44 },
                TORID,
            );
            // Walk the instances back into paths: each full-share entry opens
            // one, and the hops that follow belong to it.
            let mut path: Vec<usize> = Vec::new();
            for i in &v {
                if i.share == 1.0 {
                    let n = path.len();
                    path.sort_unstable();
                    path.dedup();
                    assert_eq!(path.len(), n, "a path visited a body twice");
                    path = vec![i.target];
                } else {
                    path.push(i.target);
                }
            }
        }
    }

    /// TIE-BREAKS MOVE DAMAGE AND NEVER CHANGE THE TOTAL. A grid is nothing but
    /// ties, so a single resolution would be reporting one arbitrary answer as
    /// the answer — this is what says the totals in docs/MECHANICS.md §12 do
    /// not depend on the rule nobody knows.
    #[test]
    fn the_total_is_invariant_to_tie_breaks() {
        let bodies = grid(3.0);
        let splash = Splash { at: bodies[FRONT_MIDDLE], radius_m: 2.3 * 1.44 };
        let mut rng = crate::rng::Rng::new(0x5EED);
        let mut seen_spread = false;
        let mut first: Option<Vec<f64>> = None;
        for _ in 0..500 {
            let v = resolve_with(&bodies, Some(FRONT_MIDDLE), splash, TORID, &mut |n| {
                (rng.next_f64() * n as f64) as usize
            });
            assert!((total(&v) - 13.1524).abs() < 1e-3, "{}", total(&v));
            let mut per = vec![0.0; bodies.len()];
            for i in &v {
                per[i.target] += i.share;
            }
            match &first {
                None => first = Some(per),
                Some(f) => {
                    if f.iter().zip(&per).any(|(a, b)| (a - b).abs() > 1e-9) {
                        seen_spread = true;
                    }
                }
            }
        }
        assert!(seen_spread, "the tie-break must actually move damage around");
    }

    /// A STILL FORMATION HAS ONE PATH. The property the owner asked for in
    /// place of reproducing the game's own tie-break, which is not a function
    /// of the formation at all (MEASUREMENTS M52): shoot the same arrangement
    /// a hundred times and the same bodies take the same shares.
    #[test]
    fn a_formation_that_does_not_move_always_chains_the_same_way() {
        let bodies = grid(3.0);
        let splash = Splash { at: bodies[FRONT_MIDDLE], radius_m: 2.3 * 1.44 };
        let first = resolve(&bodies, Some(FRONT_MIDDLE), splash, TORID);
        for _ in 0..100 {
            assert_eq!(resolve(&bodies, Some(FRONT_MIDDLE), splash, TORID), first);
        }
        // …and MOVING one body is what changes it. A model that answered the
        // same for every arrangement would pass the loop above too.
        //
        // It has to be a body the shot REACHES: the far corner is out of every
        // path's way, and walking it off the map changes nothing, which is
        // itself the geometry working.
        let mut moved = bodies.clone();
        moved[0] = Vec2::new(30.0, 30.0);
        assert_ne!(resolve(&moved, Some(FRONT_MIDDLE), splash, TORID), first);
        let mut untouched = bodies.clone();
        untouched[8] = Vec2::new(30.0, 30.0);
        assert_eq!(
            resolve(&untouched, Some(FRONT_MIDDLE), splash, TORID),
            first,
            "the far corner is reached by nothing, so moving it may not matter"
        );
    }

    /// …AND THE CONSTANT ONLY HOLDS WHILE A PATH CAN FILL ITSELF. Two bodies
    /// give one hop and stop, which is the case the first measurement will use
    /// because it is the one where the unknown hop rule cannot matter.
    #[test]
    fn two_bodies_are_the_clean_measurement() {
        let bodies = vec![Vec2::ORIGIN, Vec2::new(0.0, 2.0)];
        let v = resolve(
            &bodies,
            Some(0),
            Splash { at: Vec2::ORIGIN, radius_m: 2.3 },
            TORID,
        );
        // Both are seeds (2 m is inside the radius), so both take a full
        // instance and each chains once into the other.
        assert_eq!(v.len(), 4);
        let mut per = [0.0; 2];
        for i in &v {
            per[i.target] += i.share;
        }
        assert!((per[0] - 1.75).abs() < 1e-9 && (per[1] - 1.75).abs() < 1e-9, "{per:?}");
        // …and only the aimed one may headshot, even though both were seeds.
        assert_eq!(v.iter().filter(|i| i.headshot).count(), 1);
        assert_eq!(v.iter().filter(|i| i.multishot).count(), 2);
    }

    /// OUT OF RANGE IS OUT: past the chain's reach a formation is a set of
    /// single targets, which is the fight this engine has always modelled.
    #[test]
    fn a_body_beyond_the_chain_range_takes_nothing() {
        let bodies = vec![Vec2::ORIGIN, Vec2::new(0.0, 20.0)];
        let v = resolve(
            &bodies,
            Some(0),
            Splash { at: Vec2::ORIGIN, radius_m: 2.3 },
            TORID,
        );
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].target, 0);
        assert_eq!(v[0].share, 1.0);
    }
}
