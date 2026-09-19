// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE VOCABULARY — the words every layer above uses to say what a card,
//! a buff or a weapon's shape IS, with nothing about where it came from or
//! what the fight does with it.
//!
//! It names nothing above it: a catalog (`*_data`) reads yaml INTO these types,
//! the build resolves them and the fight plays them. A type that needs a
//! catalog, a Tenno or a fight to answer a question keeps that method in the
//! layer that has one. `scripts/check_engine_layers.mjs` holds the line.

mod buff;
mod effect;
mod enemy;
mod shape;
mod text;
mod weapon;

pub use buff::*;
pub use effect::*;
pub use enemy::*;
pub use shape::*;
pub use text::*;
pub use weapon::*;

// serde defaults shared by the yaml shapes below.
fn one() -> f64 {
    1.0
}
