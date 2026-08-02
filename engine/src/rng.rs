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
