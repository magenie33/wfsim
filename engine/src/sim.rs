//! Timeline primitives: a fixed-rate clock and the event stream that drives
//! stateful [`crate::buffs`].
//!
//! The whole simulation is deterministic given a seed and a fixed tick rate, so
//! Monte Carlo runs and golden tests are reproducible (see `docs/CORE.md` §2,
//! and the determinism rule in `docs/BUFFS.md`).

/// Simulation configuration.
///
/// `fps` is the tick rate: how many discrete steps the timeline advances per
/// in-game second. 240 by default; adjustable. Time-based mechanics (e.g. a
/// "30 times per second" rate cap) are expressed in seconds, so they behave
/// identically regardless of `fps` — raising `fps` only increases temporal
/// resolution, it never changes the mechanic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimConfig {
    pub fps: u32,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self { fps: 240 }
    }
}

/// A monotonic tick counter that converts ticks to seconds using [`SimConfig`].
#[derive(Debug, Clone, Copy)]
pub struct SimClock {
    fps: u32,
    tick: u64,
}

impl SimClock {
    pub fn new(config: SimConfig) -> Self {
        assert!(config.fps > 0, "fps must be positive");
        Self {
            fps: config.fps,
            tick: 0,
        }
    }

    /// Advance one tick.
    pub fn step(&mut self) {
        self.tick += 1;
    }

    /// The current tick index (starts at 0).
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Current time in seconds since the start of the run.
    pub fn secs(&self) -> f64 {
        self.tick as f64 / self.fps as f64
    }
}

/// A hit landed on a target — the context perks react to.
///
/// Fields grow as the pipeline lands; construct in tests via `Hit::default()`
/// with `..` overrides. `target_alive` defaults to `true` (the common case).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    /// The hit was a "big" critical hit (crit tier >= 2 — exact threshold to
    /// confirm by golden test), which some perks treat specially.
    pub big_crit: bool,
    /// The hit landed on the **head** hitbox specifically (not any weakspot).
    pub headshot: bool,
    /// The target was alive at the moment of the hit (some perks ignore hits on
    /// corpses, e.g. Frenzy).
    pub target_alive: bool,
}

impl Default for Hit {
    fn default() -> Self {
        Self {
            big_crit: false,
            headshot: false,
            target_alive: true,
        }
    }
}

/// Events emitted by the timeline that perks react to.
///
/// This set will grow as the pipeline lands (Kill, StatusProc, Reload, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Hit(Hit),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_fps_is_240() {
        assert_eq!(SimConfig::default().fps, 240);
    }

    #[test]
    fn clock_converts_ticks_to_seconds() {
        let mut clock = SimClock::new(SimConfig::default());
        assert_eq!(clock.secs(), 0.0);
        for _ in 0..240 {
            clock.step();
        }
        assert!((clock.secs() - 1.0).abs() < 1e-12);
        assert_eq!(clock.tick(), 240);
    }

    #[test]
    fn seconds_are_independent_of_fps() {
        // Half a second is half a second at any tick rate.
        let mut fast = SimClock::new(SimConfig { fps: 240 });
        let mut slow = SimClock::new(SimConfig { fps: 60 });
        for _ in 0..120 {
            fast.step();
        }
        for _ in 0..30 {
            slow.step();
        }
        assert!((fast.secs() - 0.5).abs() < 1e-12);
        assert!((slow.secs() - 0.5).abs() < 1e-12);
    }
}
