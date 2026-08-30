//! Deterministic pseudo-random number generator.
//!
//! One seeded RNG threaded through the whole simulation keeps Monte Carlo runs
//! and golden tests reproducible (the determinism rule in `docs/BUFFS.md`).
//! SplitMix64 — small, fast, no dependencies.

/// A seeded SplitMix64 generator.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Create a generator from a seed.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The generator's ENTIRE state. SplitMix64 keeps it in one `u64`, so
    /// `Rng::new(rng.state())` is an exact clone — which is what lets the
    /// simulator replay one chosen engagement out of thousands without
    /// storing anything per run but this number.
    pub fn state(&self) -> u64 {
        self.state
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)` with 53 bits of precision.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }

    /// Return true with probability `p` (clamped to `[0, 1]`).
    pub fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p.clamp(0.0, 1.0)
    }
}

/// ONE ENGAGEMENT'S RANDOMNESS, SPLIT SO THAT UNRELATED DECISIONS DO NOT MOVE
/// EACH OTHER.
///
/// Every roll off ONE stream makes the answer to "what does this mod change?"
/// noisier than the mod: a status chance high enough to land a second proc
/// draws one more number to pick its element, and every crit roll, body part
/// and fractional multishot after it is a DIFFERENT number. The fight is
/// re-rolled rather than adjusted, which reads on the page as a lie — a
/// Cold-only build's status chance moved the reported DPS +0.73% where the same
/// build re-seeded eight times spread 2.16%.
///
/// So the streams are split by WHAT IS BEING DECIDED, each derived from the
/// run's own seed:
///
/// * `spine` — what the shot does: fractional multishot, crit tier and
///   promotion, body part, the non-crit multiplier. Nothing else may perturb
///   these.
/// * `status` — whether a hit procs, which element, and the proc-derived rolls.
/// * `extra` — everything conditional on the two above, kept off the spine
///   because it fires a different number of times as the build changes.
///
/// That is common random numbers done where it belongs: not re-using one seed
/// between two builds, which the caller already did, but making the seed mean
/// the same thing in both.
#[derive(Debug, Clone)]
pub struct Draws {
    pub spine: Rng,
    pub status: Rng,
    pub extra: Rng,
    /// WHERE THE PELLET WENT — the spread roll, on a stream of its own.
    ///
    /// Same reason the other three are split: a build that changes only its
    /// accuracy must not re-roll this engagement's crits, and a fight moved
    /// from 0 m to 30 m must not either. It is drawn from ONLY when a shot can
    /// actually miss (a transcribed accuracy, a spread above zero, and a range
    /// above zero), so a point-blank fight advances it never and every number
    /// this engine reported before the arena had a distance is byte-identical.
    pub aim: Rng,
    /// WHICH WAY the pellet went, as against how far — the direction around
    /// the spread cone, on a stream of its own.
    ///
    /// It is separate from `aim` for the ordinary reason (a build that changes
    /// only its accuracy must not re-roll anything else) and for one specific
    /// to it: the direction was drawn from `aim` and only when the weapon was
    /// pointed AWAY from the target, because against ONE body only the
    /// magnitude of a miss decides anything. That stopped being true when a
    /// fight could hold a crowd — which side a grenade landed on decides who is
    /// in the blast — and a fight with a crowd must now draw it always. Off
    /// `aim` that would have shifted every subsequent spread roll; off a stream
    /// of its own it shifts nothing, which is what this split is for.
    pub blast_dir: Rng,
}

impl Draws {
    /// Derive the streams from one engagement seed.
    ///
    /// Each stream's starting state is the seed MIXED with the stream index,
    /// not offset by it. Offsetting is the obvious thing and it is wrong here:
    /// SplitMix64 advances by adding the golden-ratio constant, so seeding
    /// stream 2 at `seed + 2γ` makes it stream 1 shifted by a single draw —
    /// the crit roll for one pellet is then literally the status roll for the
    /// one before it. The streams looked separate and were one sequence read
    /// at three offsets, which showed up as a status rate 6% off its own
    /// parameter (`incarnon_procs_at_the_listed_rate_per_pellet`).
    ///
    /// Running the index through the same avalanche the generator uses for its
    /// output puts the three starting states far apart, which is what SplitMix64
    /// is designed for.
    pub fn new(seed: u64) -> Self {
        let at = |k: u64| {
            let mut z = seed.wrapping_add(k.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            Rng::new(z ^ (z >> 31))
        };
        Self { spine: at(1), status: at(2), extra: at(3), aim: at(4), blast_dir: at(5) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_a_seed() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn f64_in_unit_interval() {
        let mut r = Rng::new(1);
        for _ in 0..10_000 {
            let x = r.next_f64();
            assert!((0.0..1.0).contains(&x));
        }
    }

    #[test]
    fn chance_bounds_are_total() {
        let mut r = Rng::new(7);
        assert!(!r.chance(0.0)); // never
        assert!(r.chance(1.0)); // always
    }

    #[test]
    fn chance_frequency_is_roughly_right() {
        let mut r = Rng::new(123);
        let n = 100_000;
        let hits = (0..n).filter(|_| r.chance(0.3)).count();
        let freq = hits as f64 / n as f64;
        assert!((freq - 0.3).abs() < 0.01, "freq was {freq}");
    }
}
