// SPDX-License-Identifier: AGPL-3.0-or-later
//! RULES — primitives and the game's formulas: pure functions over numbers,
//! naming no catalog, no build and no fight.
//!
//! The damage pipeline's stateless layers live here (`damage`, `elements`,
//! `status`, `scaling`); stateful modifiers are `perks` granting buffs onto a
//! `buffs` bar, read by the fight as one summed snapshot (docs/BUFFS.md).

pub mod ammo;
pub mod buffs;
pub mod capacity;
pub mod chain;
pub mod damage;
pub mod elements;
pub mod mercy;
pub mod metrics;
pub mod perks;
pub mod rng;
pub mod scaling;
pub mod sim;
pub mod space;
pub mod status;
