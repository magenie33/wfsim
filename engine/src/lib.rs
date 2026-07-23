//! wfsim damage-pipeline engine.
//!
//! See `docs/CORE.md` section 3. Damage is not a single multiplication but a
//! **strictly ordered** pipeline; every layer is an independent, testable pure
//! function. This is an early build — most layers are not implemented yet.
//!
//! Pipeline layers (placeholders, each calibrated against in-game measurements
//! as it is implemented):
//!   [1] mod resolution   [2] elemental combination   [3] per-hit damage vector
//!   [4] critical tiers   [5] status / proc           [6] hit resolution
//!   [7] target mitigation                            [8] temporal integration
//!
//! Stateful modifiers (arcanes, conditional mods, combo) do not live inside the
//! pure pipeline. They live in the timeline (layer [8]) as event-driven
//! [`effects`], each an isolated state machine that reacts to [`sim`] events and
//! reports its current contribution. See `docs/EFFECTS.md`.

pub mod effects;
pub mod sim;

// Damage-pipeline layers will be split into their own modules, e.g.:
// pub mod mod_resolution;
// pub mod elements;
// pub mod crit;
// pub mod status;
// pub mod hit;
// pub mod mitigation;
// pub mod simulate;
