// SPDX-License-Identifier: AGPL-3.0-or-later
//! Declarative Incarnon-evolution loader: `data/evolutions/*.yaml` -> the
//! evolution pool.
//!
//! Evolutions are DATA, not code (same pattern as [`crate::data::mods`] /
//! [`crate::data::arcanes`]): each yaml records the wiki-verified effects;
//! this module parses them into [`EvolutionDef`] and APPLIES a chosen set
//! onto a weapon's raw [`WeaponBase`] — evolutions alter BASE stats before
//! mods (flat base damage scales the vector pro-rata inside ModifiedBase;
//! Commodore's Fortune adds into the BASE crit chance that crit mods then
//! multiply). The `DtEvo2` enum is a selector only — every value lives here.

use std::sync::OnceLock;

use serde::Deserialize;
use serde_norway::Value;

use crate::model::WeaponBase;
use crate::model::EvoEffect;
use crate::model::Scope;

mod parse;
mod catalog;
mod cards;
mod describe;
mod apply;
#[cfg(test)]
mod tests;

use parse::*;
pub use catalog::*;
pub use cards::*;
pub use apply::*;
