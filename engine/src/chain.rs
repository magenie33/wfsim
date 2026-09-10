//! BEAM CHAINING — where ONE shot lands when there is more than one body.
//!
//! The mechanic, its wiki quotes and its measurements are docs/MECHANICS.md
//! §12. This module is the geometric half: which bodies a shot reaches, at what
//! share, in what order.
//!
//! THE TWO ANSWERS THAT ARE THIS MODULE'S RATHER THAN THE GAME'S:
//!
//! - **A TIE GOES TO THE LOWEST BODY INDEX.** The real tie-break is not a
//!   function of the formation at all — a non-humanoid model changes the path
//!   while every relative position stays identical, so the order is the game's
//!   spatial query returning bodies in world-space broadphase order. What is
//!   guaranteed instead is the property the wiki states, *"if the enemies never
//!   move, the chain path is always fixed"*: arbitrary, stable, and free,
//!   because the TOTAL is invariant to it
//!   (`the_total_is_invariant_to_tie_breaks`).
//! - **A HOP IS A DISTANCE TEST AND NOTHING ELSE.** No line of sight, so a body
//!   behind another is as reachable as one beside it.

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
    /// Does `falloff` COMPOUND along the path, or does every hop deal the same
    /// share of the main beam?
    ///
    /// Compounding is the common shape — the Atomos is *"0.75^n times the main
    /// beam's damage, where n is the chain number"*. The Kuva Nukor is not:
    /// *"each doing 50% of the main beam's damage"*, both hops at 50% rather
    /// than 50% and 25%. One word's difference on the page and a factor of two
    /// on the second hop.
    pub compounds: bool,
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
    /// Did it land on a HEAD? True for the directly struck target, for a body
    /// the same shot punched through to, and for a RICOCHET that rolled one.
    /// False for everything else — a splash, a chain hop, an echo, a tendril.
    ///
    /// It is the shield-gate question (`head_direct`), and nothing else: what a
    /// head is WORTH is `part_factor`.
    pub headshot: bool,
    /// THE BODY PART'S OWN FACTOR — 1.0 on a body, and the head's multiplier
    /// with every headshot bonus folded in on a head.
    ///
    /// Separate from `share`, and it has to be: `share` is "a beam with a
    /// smaller base damage", so it scales the hit AND the status base that hit
    /// computes its DoTs from. A head multiplier scales the HIT and leaves the
    /// modded base alone, which is why a headshot's Slash bleed is the same
    /// size as a bodyshot's. Folding one into the other would inflate every DoT
    /// a ricochet headshot leaves.
    pub part_factor: f64,
}

/// Where the beam landed and how wide its damage radius is, AFTER mods.
#[derive(Debug, Clone, Copy)]
pub struct Splash {
    pub at: Vec2,
    pub radius_m: f64,
}

/// AN ATTACK THAT PICKS ITS OWN TARGETS, beside the one it was aimed at.
///
/// NOT MULTISHOT, and the difference is the whole of it: multishot puts more
/// instances on ONE body, this puts one instance on MORE BODIES. The Boar
/// Incarnon *"can fire up to 3 beams that automatically target enemies within
/// 10° of the reticle"* — written as multishot 3 that trebles a single-target
/// number the game does not treble; written here it is inert against one body
/// and worth three times as much against a crowd, which is what the page says.
///
/// The aimed body is ONE OF THE `count`, not extra to it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Acquire {
    /// Beams the attack fires in total, the aimed one included. 1 is an
    /// ordinary weapon and the whole of this is skipped.
    pub count: u32,
    /// Half-angle off the aim line inside which a beam will take a body.
    pub cone_deg: f64,
    /// How far a beam reaches from the muzzle.
    pub range_m: f64,
}

/// Every damage instance one shot produces against `bodies`.
///
/// `aimed` is the index the beam struck directly — the one instance that may
/// headshot, and the seed whose path carries multishot.
///
/// TIES go to the LOWEST BODY INDEX — a square grid is full of them, every
/// orthogonal neighbour being the same distance away — so one formation always
/// produces one path, a stated property in place of a rule that is not
/// reproducible (M52). [`resolve_with`] takes the tie-break as an argument for
/// the invariance test; nothing in production should.
/// THE PART OF A CHAIN THAT NEVER CHANGES, computed once per engagement.
///
/// Nothing in this arena moves, so both O(N) scans inside [`resolve`] ask a
/// constant question thousands of times a run. That is the whole cost of a big
/// formation: `seeds x hops x N` is ~11,000 distance computations a pellet on a
/// 19x19 grid, to reach the same THIRTEEN bodies a 7x7 reaches.
///
/// THE ANSWER IS IDENTICAL rather than approximate, which is what makes it an
/// optimisation: `near` is sorted by (distance, index), exactly the order the
/// scan's "nearest, ties to the lowest index" rule produces, truncated at the
/// chain's range because a hop beyond it was never a candidate.
#[derive(Debug, Clone)]
pub struct Layout {
    /// Bodies the splash catches — the seed set, minus whatever was struck
    /// directly (which the caller supplies per shot).
    caught: Vec<u32>,
    /// Per body, every body within the chain's range, NEAREST FIRST and ties by
    /// index. A hop reads this and takes the first one it has not visited.
    near: Vec<Vec<u32>>,
    /// Bodies a SELF-AIMING attack will take, in the order it takes them, and
    /// empty for every weapon that does not aim itself. Another per-engagement
    /// constant: the cone is measured off an aim line that does not move,
    /// against bodies that do not move.
    acquired: Vec<u32>,
    /// [`Acquire::count`], or 1 when nothing here aims itself.
    beams: u32,
}

impl Layout {
    /// Build it for one arrangement. O(N^2) once, against O(N) per pellet.
    pub fn build(bodies: &[Vec2], splash: Splash, spec: Spec) -> Self {
        let caught = (0..bodies.len())
            .filter(|&i| crate::space::caught_by_blast(bodies[i].distance(splash.at), splash.radius_m))
            .map(|i| i as u32)
            .collect();
        let near = bodies
            .iter()
            .map(|b| {
                let mut v: Vec<(f64, u32)> = bodies
                    .iter()
                    .enumerate()
                    .filter_map(|(j, o)| {
                        let d = b.distance(*o);
                        (d <= spec.range_m + 1e-9).then_some((d, j as u32))
                    })
                    .collect();
                v.sort_by(|x, y| {
                    x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal).then(x.1.cmp(&y.1))
                });
                v.into_iter().map(|(_, j)| j).collect()
            })
            .collect();
        Self { caught, near, acquired: Vec::new(), beams: 1 }
    }

    /// Record which bodies a self-aiming attack will take — see [`Acquire`].
    pub fn acquiring(mut self, bodies: &[Vec2], player_at: Vec2, aim_at: Vec2, spec: Acquire) -> Self {
        self.beams = spec.count.max(1);
        if spec.count > 1 {
            self.acquired = acquired(bodies, player_at, aim_at, spec.cone_deg, spec.range_m);
        }
        self
    }
}

/// WHICH BODIES A SELF-AIMING ATTACK TAKES, and the ONE rule for it.
///
/// Two weapons ask this question and both pages answer it the same way — the
/// Boar Incarnon's beams *"automatically target enemies within 10° of the
/// reticle"*, the Ocucor's tendrils *"home-in on enemies close to the targeting
/// reticle"*. CLOSE TO THE RETICLE IS AN ANGLE, so the order is nearest by
/// angle and not by distance: a body three metres away at forty degrees is
/// further from the reticle than one twenty metres away straight ahead.
///
/// TIES GO TO THE LOWEST INDEX, the same stand-in this module uses for the
/// chain — a square grid puts a whole column at zero degrees, and the real
/// order is the game's spatial query returning bodies in broadphase order.
/// What is guaranteed is that one formation always produces one answer.
///
/// THE RANGE IS FROM THE PLAYER and the ANGLE is from the MUZZLE, which is what
/// each of them means: how far a beam reaches, and where the reticle points.
pub fn acquired(
    bodies: &[Vec2],
    player_at: Vec2,
    aim_at: Vec2,
    cone_deg: f64,
    range_m: f64,
) -> Vec<u32> {
    let muzzle = crate::space::muzzle(player_at, aim_at);
    let mut v: Vec<(f64, u32)> = bodies
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            if !crate::space::within(b.distance(player_at), range_m) {
                return None;
            }
            let off = crate::space::off_axis_deg(muzzle, aim_at, *b);
            crate::space::within(off, cone_deg).then_some((off, i as u32))
        })
        .collect();
    v.sort_by(|x, y| {
        x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal).then(x.1.cmp(&y.1))
    });
    v.into_iter().map(|(_, i)| i).collect()
}

/// THE PATH A DEFLECTED PROJECTILE TAKES — the same walk a chain does, with no
/// damage rule attached to it.
///
/// A RICOCHET IS NOT A CHAIN and is here because it is the same GEOMETRY: a
/// chain hop is one instance at a share of the beam, a bounce is the whole
/// projectile arriving again. So this returns the ORDER OF BODIES and the
/// caller decides what arriving means.
///
/// Verbatim (Latron Incarnon Genesis): *"a traveling projectile that can
/// ricochet off enemies and terrain, exploding up to 6 times with a 4 meter
/// radius, dealing damage once for any collision on enemies, and again for the
/// explosion"*, and *"seem to require multiple enemies to ricochet
/// repeatedly"*. NOBODY TWICE, and for the stronger reason: that second note
/// says a projectile pinging between two bodies is what the game does not do.
///
/// TERRAIN IS NOT HERE. The page says enemies *and terrain*, and this arena has
/// no walls — so a bounce that would have come off a surface finds the next
/// body instead, and a formation of one bounces nowhere. Declared as a gap on
/// the weapons that have it rather than guessed at.
pub fn bounce_path(layout: &Layout, n: usize, from: usize, bounces: u32) -> Vec<usize> {
    let mut out = Vec::new();
    if n == 0 || from >= n {
        return out;
    }
    let mut seen = vec![false; n];
    seen[from] = true;
    let mut cur = from;
    for _ in 0..bounces {
        // NEAREST FIRST — `near` is sorted by (distance, index), which is the
        // same rule and the same tie-break the chain walks under.
        let Some(&next) = layout.near[cur].iter().find(|&&j| !seen[j as usize]) else {
            break;
        };
        let next = next as usize;
        out.push(next);
        seen[next] = true;
        cur = next;
    }
    out
}

/// [`resolve`], from a prebuilt [`Layout`] — the production path.
pub fn resolve_in(layout: &Layout, n: usize, struck: &[usize], spec: Spec) -> Vec<Instance> {
    let mut out = Vec::new();
    if n == 0 {
        return out;
    }
    let struck: Vec<usize> = struck.iter().copied().filter(|&i| i < n).collect();
    let mut seeds: Vec<usize> = struck.clone();
    seeds.extend(layout.caught.iter().map(|&i| i as usize).filter(|i| !struck.contains(i)));
    // THE BEAMS THAT AIMED THEMSELVES, and the aimed body is one of the count —
    // so a weapon firing three takes two more than it was pointed at, and one
    // that struck nobody takes three. Each is a seed like any other: its own
    // chain, its own path, `seen` of its own.
    if layout.beams > 1 {
        let mut extra = (layout.beams as usize).saturating_sub(struck.len());
        for &i in &layout.acquired {
            if extra == 0 {
                break;
            }
            let i = i as usize;
            if i < n && !seeds.contains(&i) {
                seeds.push(i);
                extra -= 1;
            }
        }
    }

    let mut seen = vec![false; n];
    // EVERY BODY THIS SHOT HAS ALREADY REACHED, across all of its paths. A hop
    // prefers a body nobody has touched yet and settles for a repeat only when
    // there is nothing fresh in range — measured (M86), and it is a PREFERENCE
    // rather than a ban: the wiki's *"The chain from the target hit after the
    // Punch Through can deal damage to the first target, and vice versa"* is
    // what a crowded corner still produces.
    //
    // EVERY SEED IS TAKEN BEFORE ANY PATH WALKS. Marking one as its own turn
    // came round let the FIRST beam hop onto a body the third was about to
    // stand on — a body two beams reach and one that nothing reaches, from a
    // rule whose whole point is to spread. It is not a tie-break: the seeds are
    // known before a single hop is chosen, so nothing has to be guessed.
    let mut taken = vec![false; n];
    for &s in &seeds {
        taken[s] = true;
    }
    for &s in &seeds {
        let direct = struck.contains(&s);
        out.push(Instance {
            target: s, share: 1.0, multishot: direct, headshot: direct,
            part_factor: 1.0,
        });

        // ONE `seen` PER SEED — a path never revisits its OWN bodies, and one
        // path running out does not stop another. Cleared rather than
        // reallocated: this runs once per landing pellet.
        seen.iter_mut().for_each(|x| *x = false);
        seen[s] = true;
        let (mut cur, mut share) = (s, 1.0);
        for _ in 0..spec.hops {
            // NEAREST FIRST, so the first entry that qualifies IS the answer the
            // scan computes. `near` excludes `cur` itself only by `seen`.
            let fresh = layout.near[cur]
                .iter()
                .find(|&&j| !seen[j as usize] && !taken[j as usize]);
            let Some(&next) = fresh.or_else(|| layout.near[cur].iter().find(|&&j| !seen[j as usize]))
            else {
                break;
            };
            let next = next as usize;
            taken[next] = true;
            share = if spec.compounds { share * spec.falloff } else { spec.falloff };
            out.push(Instance {
                target: next, share, multishot: direct, headshot: false,
                part_factor: 1.0,
            });
            seen[next] = true;
            cur = next;
        }
    }
    out
}

pub fn resolve(bodies: &[Vec2], struck: &[usize], splash: Splash, spec: Spec) -> Vec<Instance> {
    resolve_with(bodies, struck, splash, spec, &mut |_| 0)
}

/// [`resolve`] with the tie-break handed in — for asserting that it does not
/// change the total, and for nothing else.
pub fn resolve_with(
    bodies: &[Vec2],
    struck: &[usize],
    splash: Splash,
    spec: Spec,
    tie: &mut impl FnMut(usize) -> usize,
) -> Vec<Instance> {
    let mut out = Vec::new();
    if bodies.is_empty() {
        return out;
    }
    // EVERY BODY THIS SHOT HAS REACHED — see `resolve_in`, which this has to
    // answer instance for instance, and which pre-marks the seeds for the same
    // reason: a path may not step onto a body another beam is about to stand on.
    let mut taken = vec![false; bodies.len()];
    // A SHOT THAT STRUCK NOBODY STILL SPLASHES: aim is a direction and the
    // place it lands may be bare floor — *"a 2.3 meter damage radius from the
    // point of impact against a SURFACE"*. Every body the sphere catches is an
    // ordinary seed, so none may headshot and none carries multishot.
    // EVERY BODY THE SHOT PHYSICALLY PASSED THROUGH, in the order the ray met
    // them (`space::struck_along`). It was ONE body until 2026-08-17, which was
    // right while a bullet stopped at the first thing it hit; with punch
    // through the wiki is explicit that each of them is its own start:
    //
    //   "Each enemy hit by the main beam from Punch Through can generate a new
    //    set of 3 chains." / "Punch Through will cause the main beam to chain
    //    INDEPENDENTLY from each additional target hit, potentially doubling or
    //    tripling the total damage output when fired into a crowd."
    //
    // …and the paths really are independent rather than one longer path:
    // "The chain from the target hit after the Punch Through can deal damage to
    // the first target, and vice versa." Which is the same rule the owner gave
    // for two chains meeting: a body takes a second instance only
    // when a SECOND independent link reaches it. `seen` is per seed, so that
    // falls out rather than being arranged.
    let struck: Vec<usize> = struck.iter().copied().filter(|&i| i < bodies.len()).collect();
    // THE SEEDS: everything the damage radius caught. The aimed body is always
    // one of them — the impact is on it.
    //
    // THE AIMED BODY FIRST, so instance 0 is always the direct hit — the one
    // that may headshot and the one multishot follows. The rest are a set with
    // no order the game gives them, and taking them in index order is this
    // module's choice rather than a fact.
    let mut seeds: Vec<usize> = struck.clone();
    seeds.extend((0..bodies.len()).filter(|&i| {
        // ANY PART OF A BODY TOUCHING THE SPHERE IS ENOUGH — the blast rule,
        // in one place (`space::caught_by_blast`,). So the
        // radius a splash really seeds over is its own plus a body radius.
        !struck.contains(&i)
            && crate::space::caught_by_blast(bodies[i].distance(splash.at), splash.radius_m)
    }));

    for &s in &seeds {
        taken[s] = true;
    }
    for &s in &seeds {
        // …AND THE SPLASH IS NOT A SECOND INSTANCE. "A target that is directly
        // struck by the beam is still only hit once", so a seed takes ONE
        // full-share instance whether the beam or the radius reached it.
        // A STRUCK BODY IS A DIRECT HIT: every pellet that punches through
        // reaches it, so it carries multishot, and it may HEADSHOT — punch
        // through does not stop a shot being aimed. A body
        // the sphere merely caught does neither.
        let direct = struck.contains(&s);
        out.push(Instance {
            target: s, share: 1.0, multishot: direct, headshot: direct,
            part_factor: 1.0,
        });

        // …and then runs its own path.
        let (mut cur, mut share) = (s, 1.0);
        let mut seen = vec![false; bodies.len()];
        seen[s] = true;
        for _ in 0..spec.hops {
            // TWO SCANS, AND THE FIRST ONE WINS WHEN IT FINDS ANYTHING: a body
            // nobody has reached yet is preferred over a nearer body that has
            // been (M86). The second is the fallback, which is what a corner
            // with nothing fresh left in range still produces.
            let mut next = None;
            for fresh in [true, false] {
                let mut best = f64::INFINITY;
                let mut tied: Vec<usize> = Vec::new();
                for j in 0..bodies.len() {
                    if seen[j] || (fresh && taken[j]) {
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
                if !tied.is_empty() {
                    next = Some(tied[tie(tied.len()).min(tied.len() - 1)]);
                    break;
                }
            }
            let Some(next) = next else {
                break;
            };
            taken[next] = true;
            share = if spec.compounds { share * spec.falloff } else { spec.falloff };
            out.push(Instance {
                target: next, share, multishot: direct, headshot: false,
                part_factor: 1.0,
            });
            seen[next] = true;
            cur = next;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE PRECOMPUTED PATH ANSWERS EXACTLY WHAT THE SCAN DOES.
    ///
    /// `Layout` exists to make a big formation affordable, and an optimisation
    /// that changes a number is a bug (the same rule `one_fight` enforces for
    /// the engine at large). So this asserts INSTANCE FOR INSTANCE — target,
    /// share, multishot and headshot — over every seed of a grid, at three
    /// spacings and for both chain shapes, compounding and flat.
    ///
    /// The equivalence is not a coincidence to be checked once: `near` is
    /// sorted by (distance, index), which is precisely "nearest, ties to the
    /// lowest index", and truncated at the chain's range, where a hop was never
    /// a candidate anyway.
    #[test]
    fn a_layout_answers_exactly_what_the_scan_does() {
        let nukor = Spec { hops: 2, range_m: 9.0, falloff: 0.5, compounds: false };
        for spacing in [1.5_f64, 2.0, 3.0] {
            let bodies = grid(spacing);
            for radius in [0.0_f64, 2.3, 5.0] {
                for spec in [TORID, nukor] {
                    for struck in [vec![], vec![FRONT_MIDDLE], vec![0usize, 4], vec![0, 1, 2]] {
                        let at = bodies[FRONT_MIDDLE];
                        let splash = Splash { at, radius_m: radius };
                        let slow = resolve(&bodies, &struck, splash, spec);
                        let layout = Layout::build(&bodies, splash, spec);
                        let fast = resolve_in(&layout, bodies.len(), &struck, spec);
                        assert_eq!(slow.len(), fast.len(),
                            "spacing {spacing}, radius {radius}, struck {struck:?}");
                        for (a, b) in slow.iter().zip(fast.iter()) {
                            assert_eq!(a.target, b.target, "spacing {spacing} radius {radius}");
                            assert!((a.share - b.share).abs() < 1e-12);
                            assert_eq!(a.multishot, b.multishot);
                            assert_eq!(a.headshot, b.headshot);
                        }
                    }
                }
            }
        }
    }

    /// The Torid Incarnon's own constants, which are the ones every number in
    /// docs/MECHANICS.md §12 was computed against.
    const TORID: Spec = Spec { hops: 5, range_m: 7.0, falloff: 0.75, compounds: true };

    /// A 3 x 3 formation at 3 m, the fixture the owner chose: the
    /// smallest arrangement dense enough that a five-hop path never runs out of
    /// targets, and sparse enough that the damage radius is the thing deciding
    /// how many chains start.
    fn grid(spacing: f64) -> Vec<Vec2> {
        (0..3)
            .flat_map(|r| (0..3).map(move |c| Vec2::new(c as f64 * spacing, r as f64 * spacing)))
            .collect()
    }
    /// The front row's middle body — the one a player can actually put a beam
    /// on. The centre of the formation is BEHIND it and cannot be aimed at.
    const FRONT_MIDDLE: usize = 1;

    fn total(v: &[Instance]) -> f64 {
        v.iter().map(|i| i.share).sum()
    }

    /// The Boar Incarnon's: three beams, 10 degrees off the reticle, 20 m.
    const BOAR: Acquire = Acquire { count: 3, cone_deg: 10.0, range_m: 20.0 };
    const BOAR_CHAIN: Spec = Spec { hops: 2, range_m: 10.0, falloff: 0.80, compounds: true };

    /// A LINE THE PLAYER IS LOOKING DOWN, and one body off to the side.
    ///
    /// The shooter stands at the origin facing +x. Bodies 0..3 are on the line
    /// at 5 m intervals; body 4 sits 5 m off it at the same depth, which is
    /// 45 degrees away and outside every cone this module is asked about.
    fn firing_line() -> (Vec2, Vec2, Vec<Vec2>) {
        let player = Vec2::new(0.0, 0.0);
        let aim_at = Vec2::new(5.0, 0.0);
        let bodies = vec![
            Vec2::new(5.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(15.0, 0.0),
            Vec2::new(5.0, 5.0),
        ];
        (player, aim_at, bodies)
    }

    fn boar_layout(bodies: &[Vec2], player: Vec2, aim_at: Vec2, spec: Acquire) -> Layout {
        Layout::build(bodies, Splash { at: aim_at, radius_m: 0.0 }, BOAR_CHAIN)
            .acquiring(bodies, player, aim_at, spec)
    }

    /// THREE BEAMS ARE THREE SEEDS, and the aimed body is one of them — so a
    /// shot pointed at one body starts chains from THREE.
    #[test]
    fn a_self_aiming_weapon_seeds_one_chain_per_beam() {
        let (player, aim_at, bodies) = firing_line();
        let layout = boar_layout(&bodies, player, aim_at, BOAR);
        let v = resolve_in(&layout, bodies.len(), &[0], BOAR_CHAIN);
        let seeds: Vec<usize> = v.iter().filter(|i| i.share == 1.0).map(|i| i.target).collect();
        assert_eq!(seeds, vec![0, 1, 2], "seeds: {v:?}");
    }

    /// ONE BEAM IS THE WEAPON EVERY OTHER ENTRY IN THE ROSTER IS — the negative
    /// control, and the thing that must not have moved.
    #[test]
    fn one_beam_takes_only_what_it_was_pointed_at() {
        let (player, aim_at, bodies) = firing_line();
        let one = Acquire { count: 1, ..BOAR };
        let layout = boar_layout(&bodies, player, aim_at, one);
        let v = resolve_in(&layout, bodies.len(), &[0], BOAR_CHAIN);
        let seeds: Vec<usize> = v.iter().filter(|i| i.share == 1.0).map(|i| i.target).collect();
        assert_eq!(seeds, vec![0], "seeds: {v:?}");
        // …and it is exactly what a layout that was never told about beams does.
        let plain = Layout::build(&bodies, Splash { at: aim_at, radius_m: 0.0 }, BOAR_CHAIN);
        assert_eq!(v, resolve_in(&plain, bodies.len(), &[0], BOAR_CHAIN));
    }

    /// ONLY THE BODY THE PLAYER POINTED AT MAY HEADSHOT. The beams the weapon
    /// took for itself hit bodies, and they do not carry merged multishot
    /// either — they are separate beams, not more instances of one.
    #[test]
    fn a_beam_that_aimed_itself_never_headshots() {
        let (player, aim_at, bodies) = firing_line();
        let layout = boar_layout(&bodies, player, aim_at, BOAR);
        let v = resolve_in(&layout, bodies.len(), &[0], BOAR_CHAIN);
        // ONE HEADSHOT, and it is the body the player pointed at.
        let heads: Vec<usize> = v.iter().filter(|i| i.headshot).map(|i| i.target).collect();
        assert_eq!(heads, vec![0], "{v:?}");
        // MERGED MULTISHOT REACHES ONE BEAM AND ITS OWN CHAIN — the aimed seed
        // plus its two hops, and neither of the beams that aimed themselves.
        assert_eq!(v.iter().filter(|i| i.multishot).count(), 3, "{v:?}");
        // …and the two beams that aimed themselves are the other six instances.
        assert_eq!(v.len(), 9, "three seeds, two hops each: {v:?}");
    }

    /// OUTSIDE THE CONE IS OUTSIDE. Body 3 sits 45 degrees off the aim line, so
    /// no widening of the roster's numbers reaches it — and the beam that would
    /// have taken it takes nothing rather than taking the next thing along.
    #[test]
    fn a_body_off_the_reticle_is_not_taken() {
        let (player, aim_at, bodies) = firing_line();
        let layout = boar_layout(&bodies, player, aim_at, BOAR);
        let v = resolve_in(&layout, bodies.len(), &[0], BOAR_CHAIN);
        let seeds: Vec<usize> = v.iter().filter(|i| i.share == 1.0).map(|i| i.target).collect();
        assert!(!seeds.contains(&3), "body 3 is 45 deg off the line: {seeds:?}");
        // Widen the cone past it and it becomes a candidate, which is what makes
        // the exclusion above a property of the ANGLE and not of the ordering.
        let wide = boar_layout(&bodies, player, aim_at, Acquire { cone_deg: 50.0, ..BOAR });
        let v = resolve_in(&wide, bodies.len(), &[0], BOAR_CHAIN);
        let seeds: Vec<usize> = v.iter().filter(|i| i.share == 1.0).map(|i| i.target).collect();
        // …AND IT IS STILL LAST. Bodies 1 and 2 are further away — 10 m and
        // 15 m against 7.07 m — and they are taken first because they are ON
        // the reticle. Close to the reticle is an ANGLE, which is the rule the
        // Ocucor's tendrils were already picked by.
        assert_eq!(seeds, vec![0, 1, 2], "{seeds:?}");
        let widest = boar_layout(&bodies, player, aim_at,
                                 Acquire { count: 4, cone_deg: 50.0, ..BOAR });
        let v = resolve_in(&widest, bodies.len(), &[0], BOAR_CHAIN);
        let seeds: Vec<usize> = v.iter().filter(|i| i.share == 1.0).map(|i| i.target).collect();
        assert_eq!(seeds, vec![0, 1, 2, 3], "a fourth beam reaches it: {seeds:?}");
    }

    /// THE BOARD'S OWN GEOMETRY, counted rather than reasoned about.
    ///
    /// The group-clear ruler stands the shooter at CONTACT with the middle body
    /// of the front rank — 0.4 m — and a 10 degree cone from there covers the
    /// column ahead and nothing beside it. Three beams down one line is still
    /// three beams: the paths are independent, `seen` is per seed, and a body
    /// two of them reach takes two instances. That is the chain's rule
    /// everywhere, not a special case here.
    #[test]
    fn the_boards_geometry_still_pays_for_three_beams() {
        let spacing = 3.0;
        let mut bodies = vec![Vec2::new(0.0, 0.4)];
        for k in -9..=9 {
            for j in 0..19 {
                if k == 0 && j == 0 {
                    continue;
                }
                bodies.push(Vec2::new(k as f64 * spacing, 0.4 + j as f64 * spacing));
            }
        }
        let player = Vec2::new(0.0, 0.0);
        let aim_at = bodies[0];
        let build = |count: u32| {
            Layout::build(&bodies, Splash { at: aim_at, radius_m: 0.0 }, BOAR_CHAIN).acquiring(
                &bodies,
                player,
                aim_at,
                Acquire { count, ..BOAR },
            )
        };
        let one = resolve_in(&build(1), bodies.len(), &[0], BOAR_CHAIN);
        let three = resolve_in(&build(3), bodies.len(), &[0], BOAR_CHAIN);
        assert_eq!(one.len(), 3, "one beam is a seed and two hops: {one:?}");
        assert_eq!(three.len(), 9, "three beams are three of those: {three:?}");
        assert!(
            (total(&three) - 3.0 * total(&one)).abs() < 1e-9,
            "{} against {}",
            total(&three),
            total(&one)
        );
    }

    /// NINE BODIES, NOT SEVEN. A hop prefers one nobody has reached, so three
    /// beams down one column spread into the ranks beside it instead of
    /// doubling back onto each other's seeds (M86).
    #[test]
    fn three_beams_reach_nine_bodies_rather_than_seven() {
        let spacing = 3.0;
        let mut bodies = vec![Vec2::new(0.0, 0.4)];
        for k in -9..=9 {
            for j in 0..19 {
                if k == 0 && j == 0 {
                    continue;
                }
                bodies.push(Vec2::new(k as f64 * spacing, 0.4 + j as f64 * spacing));
            }
        }
        let layout =
            Layout::build(&bodies, Splash { at: bodies[0], radius_m: 0.0 }, BOAR_CHAIN).acquiring(
                &bodies,
                Vec2::new(0.0, 0.0),
                bodies[0],
                BOAR,
            );
        let v = resolve_in(&layout, bodies.len(), &[0], BOAR_CHAIN);
        let mut hit: Vec<usize> = v.iter().map(|i| i.target).collect();
        hit.sort_unstable();
        hit.dedup();
        assert_eq!(v.len(), 9, "still nine instances: {v:?}");
        assert_eq!(hit.len(), 9, "and now nine bodies: {hit:?}");
    }

    /// A PATH MAY NOT STEP ONTO A BODY ANOTHER BEAM IS ABOUT TO STAND ON.
    ///
    /// Every body here is on the shot line, three metres apart, so the nearest
    /// thing to the first beam is the SECOND beam's own seed. Marking a seed as
    /// its turn came round let the first beam hop straight onto it: two beams
    /// on one body, another body reached by nothing, out of a rule whose whole
    /// point is to spread. The seeds are known before a single hop is chosen,
    /// so this is not a tie-break — it is an ordering bug with a fixed answer.
    #[test]
    fn a_beam_never_hops_onto_another_beams_seed() {
        let bodies: Vec<Vec2> = (0..6).map(|j| Vec2::new(0.0, 0.4 + 3.0 * f64::from(j))).collect();
        let layout = Layout::build(&bodies, Splash { at: bodies[0], radius_m: 0.0 }, BOAR_CHAIN)
            .acquiring(&bodies, Vec2::ORIGIN, bodies[0], BOAR);
        let v = resolve_in(&layout, bodies.len(), &[0], BOAR_CHAIN);
        let seeds: Vec<usize> = v.iter().filter(|i| i.share == 1.0).map(|i| i.target).collect();
        assert_eq!(seeds, vec![0, 1, 2], "the three nearest on the line: {seeds:?}");
        assert_eq!(v.len(), 9, "{v:?}");
        // THE FIRST BEAM'S OWN TWO HOPS, and neither may be a seed. Coverage
        // cannot say it — six bodies on a line are all reached either way — so
        // the assertion is about WHICH bodies, which is where the bug lived.
        let first_path: Vec<usize> = v[1..3].iter().map(|i| i.target).collect();
        assert!(
            first_path.iter().all(|t| !seeds.contains(t)),
            "the first beam hopped onto another beam's seed: {first_path:?} against {seeds:?}"
        );
    }

    /// …AND IT IS A PREFERENCE, NOT A BAN. With nowhere fresh left in range a
    /// hop takes a body that has already been hit, which is the wiki's own
    /// *"can deal damage to the first target, and vice versa"*.
    #[test]
    fn a_hop_repeats_only_once_nothing_fresh_is_in_range() {
        let bodies = vec![Vec2::ORIGIN, Vec2::new(0.0, 2.0)];
        let spec = Spec { hops: 1, range_m: 5.0, falloff: 0.5, compounds: true };
        let layout = Layout::build(&bodies, Splash { at: bodies[0], radius_m: 5.0 }, spec);
        let v = resolve_in(&layout, bodies.len(), &[0], spec);
        // Two seeds, one hop each, and only two bodies to share: the second
        // path has nothing fresh and doubles back.
        assert_eq!(v.len(), 4, "{v:?}");
        assert_eq!(v.iter().filter(|i| i.target == 0).count(), 2, "{v:?}");
        assert_eq!(v.iter().filter(|i| i.target == 1).count(), 2, "{v:?}");
    }

    /// THE RANGE IS THE BEAM'S OWN. A body inside the cone but past the reach
    /// is not a beam's target, and the beam is spent rather than moved on.
    #[test]
    fn a_body_past_the_beams_reach_is_not_taken() {
        let (player, aim_at, bodies) = firing_line();
        let short = boar_layout(&bodies, player, aim_at, Acquire { range_m: 12.0, ..BOAR });
        let v = resolve_in(&short, bodies.len(), &[0], BOAR_CHAIN);
        let seeds: Vec<usize> = v.iter().filter(|i| i.share == 1.0).map(|i| i.target).collect();
        assert_eq!(seeds, vec![0, 1], "15 m is past a 12 m beam: {seeds:?}");
    }

    /// THREE BEAMS ARE THREE TIMES THE SHOT, which is the number the whole
    /// change is about: one path is `1 + f + f^2` and three of them is that
    /// three times over, once every seed can find a next body.
    #[test]
    fn three_beams_are_three_paths_worth() {
        let bodies = grid(3.0);
        let player = Vec2::new(-20.0, 3.0);
        let aim_at = bodies[3];
        let one = Layout::build(&bodies, Splash { at: aim_at, radius_m: 0.0 }, BOAR_CHAIN)
            .acquiring(&bodies, player, aim_at, Acquire { count: 1, ..BOAR });
        let three = Layout::build(&bodies, Splash { at: aim_at, radius_m: 0.0 }, BOAR_CHAIN)
            .acquiring(&bodies, player, aim_at, Acquire { range_m: 40.0, ..BOAR });
        let a = total(&resolve_in(&one, bodies.len(), &[3], BOAR_CHAIN));
        let b = total(&resolve_in(&three, bodies.len(), &[3], BOAR_CHAIN));
        assert!((a - 2.44).abs() < 1e-9, "one path is 1 + 0.8 + 0.64: {a}");
        assert!((b - 3.0 * a).abs() < 1e-9, "three beams: {b} against {a}");
    }

    /// A PATH'S WHOLE OUTPUT IS A CONSTANT once it can always find a next body:
    /// `1 + f + f² + … + f^hops`, which is 3.2881 for the Torid. The total is
    /// then seeds x that, and nothing about aim or tie-breaks moves it.
    #[test]
    fn one_seed_yields_its_own_instance_plus_a_full_path() {
        let v = resolve(
            &grid(3.0),
            &[FRONT_MIDDLE],
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
        let bare = resolve(&bodies, &[FRONT_MIDDLE], Splash { at, radius_m: 2.3 }, TORID);
        let primed = resolve(
            &bodies,
            &[FRONT_MIDDLE],
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
            &[FRONT_MIDDLE],
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
            &[FRONT_MIDDLE],
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
                &[aimed],
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
            let v = resolve_with(&bodies, &[FRONT_MIDDLE], splash, TORID, &mut |n| {
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

    /// A FLAT CHAIN PAYS EVERY HOP THE SAME, and a compounding one does not.
    ///
    /// The Kuva Nukor is the roster's only flat one: *"chain up to 2 nearby
    /// enemies within 9 meters from the initial target, each doing 50% of the
    /// MAIN BEAM's damage"* — where every other page reads "of the PREVIOUS
    /// chain's". One word, and a factor of two on the second hop.
    #[test]
    fn a_flat_chain_pays_every_hop_the_same() {
        let bodies: Vec<Vec2> = (0..3).map(|i| Vec2::new(i as f64 * 2.0, 0.0)).collect();
        let splash = Splash { at: bodies[0], radius_m: 0.0 };
        let nukor = Spec { hops: 2, range_m: 9.0, falloff: 0.5, compounds: false };
        let hops: Vec<f64> = resolve(&bodies, &[0], splash, nukor)
            .iter()
            .filter(|i| i.share < 1.0)
            .map(|i| i.share)
            .collect();
        assert_eq!(hops, vec![0.5, 0.5], "both hops at half the main beam");

        // …and the same weapon read the other way would have halved twice.
        let compounding = Spec { compounds: true, ..nukor };
        let hops: Vec<f64> = resolve(&bodies, &[0], splash, compounding)
            .iter()
            .filter(|i| i.share < 1.0)
            .map(|i| i.share)
            .collect();
        assert_eq!(hops, vec![0.5, 0.25]);
    }

    /// A STILL FORMATION HAS ONE PATH. The property the owner asked for in
    /// place of reproducing the game's own tie-break, which is not a function
    /// of the formation at all (MEASUREMENTS M52): shoot the same arrangement
    /// a hundred times and the same bodies take the same shares.
    #[test]
    fn a_formation_that_does_not_move_always_chains_the_same_way() {
        let bodies = grid(3.0);
        let splash = Splash { at: bodies[FRONT_MIDDLE], radius_m: 2.3 * 1.44 };
        let first = resolve(&bodies, &[FRONT_MIDDLE], splash, TORID);
        for _ in 0..100 {
            assert_eq!(resolve(&bodies, &[FRONT_MIDDLE], splash, TORID), first);
        }
        // …and MOVING one body is what changes it. A model that answered the
        // same for every arrangement would pass the loop above too.
        //
        // It has to be a body the shot REACHES: the far corner is out of every
        // path's way, and walking it off the map changes nothing, which is
        // itself the geometry working.
        let mut moved = bodies.clone();
        moved[0] = Vec2::new(30.0, 30.0);
        assert_ne!(resolve(&moved, &[FRONT_MIDDLE], splash, TORID), first);
        // A BODY OFF THE MAP, and it has to be off the map from the START: a
        // hop prefers a body nobody has reached, so the paths push OUTWARD and
        // there is no longer a corner of a 3x3 that nothing touches. Eighteen
        // instances over nine bodies leaves none of them spare.
        let mut far = bodies.clone();
        far.push(Vec2::new(30.0, 30.0));
        let with_far = resolve(&far, &[FRONT_MIDDLE], splash, TORID);
        far[9] = Vec2::new(60.0, 60.0);
        assert_eq!(
            resolve(&far, &[FRONT_MIDDLE], splash, TORID),
            with_far,
            "a body no path reaches may be moved without changing one"
        );
        assert!(
            !with_far.iter().any(|i| i.target == 9),
            "…and it is unreached, which is what makes that a control"
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
            &[0],
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
            &[0],
            Splash { at: Vec2::ORIGIN, radius_m: 2.3 },
            TORID,
        );
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].target, 0);
        assert_eq!(v[0].share, 1.0);
    }
}
