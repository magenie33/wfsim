//! wfsim damage-pipeline engine.
//!
//! See `docs/CORE.md` section 3. Damage is not a single multiplication but a
//! **strictly ordered** pipeline; every layer is an independent, testable pure
//! function. This is a scaffold — no layer is implemented yet.
//!
//! Pipeline layers (placeholders, each calibrated against in-game measurements
//! as it is implemented):
//!   [1] mod resolution   [2] elemental combination   [3] per-hit damage vector
//!   [4] critical tiers   [5] status / proc           [6] hit resolution
//!   [7] target mitigation                            [8] temporal integration

// Layers will be split into their own modules, e.g.:
// pub mod mod_resolution;
// pub mod elements;
// pub mod crit;
// pub mod status;
// pub mod hit;
// pub mod mitigation;
// pub mod simulate;

#[cfg(test)]
mod tests {
    #[test]
    fn engine_scaffold_compiles() {
        // Golden tests (the north star) will live in tests/golden/, calibrated
        // against Simulacrum measurements.
        assert_eq!(2 + 2, 4);
    }
}
