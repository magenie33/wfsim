// SPDX-License-Identifier: AGPL-3.0-or-later
//! IS THIS A BUILD SOMEONE COULD ACTUALLY EQUIP? The simulator does not ask,
//! deliberately: it is a calculator and slot legality is the UI's job. A
//! SUBMISSION is the other case — fed over a network where the UI is not on the
//! path — so this module checks the arsenal's rules and never runs inside
//! `simulate`.
//!
//! NORMALISE, THEN REJECT. [`normalize`] runs first because the evolution
//! ladder is applied by TRUNCATION rather than by an error, so hashing before
//! it would let a board row name one build and hold another's number.
//!
//! Two builds are the SAME FIGHT when they produce the same number, which the
//! wire payload mostly says already. What it does not say is the PAIRING, and
//! that IS a fight: Heat/Cold/Toxin/Electric is Blast + Corrosive against
//! Heat/Toxin/Cold/Electric's Gas + Magnetic, 12,424 DPS to 46,583 on the
//! Torid. So the identity is the weapon, the mod sequence CANONICALISED to one
//! representative per pairing, the evolution set and the arcanes — rivens
//! absent on purpose, since a board counting personal random items ranks luck.
//!
//! CANONICALISE ON THE POOLED SEQUENCE, never on mod slots: two mods of one
//! element are ONE entry to the engine.

use std::collections::BTreeSet;

use crate::rules::capacity::PlannedMod;

mod axes;
mod canonical;
mod element_order;
mod validate;
mod identity;
#[cfg(test)]
mod tests;

pub use axes::*;
pub use canonical::*;
pub use element_order::*;
pub use validate::*;
pub use identity::*;
