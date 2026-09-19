// SPDX-License-Identifier: AGPL-3.0-or-later
//! Declarative mod loader: `data/mods/<class>/*.yaml` -> the mod pool.
//!
//! Mods are DATA, not code. Each `data/mods/<class>/<id>.yaml` describes a mod
//! (drain,
//! polarity, per-rank effects); this module parses them into [`ModDef`] so the
//! pool is a single auditable source of truth that non-programmers can extend
//! via PR (same pattern as [`crate::data::enemies`] for enemies).
//!
//! The YAML records the TRUE mechanical effect (tooltip lies are corrected in
//! place — see docs/DATA_SOURCES.md). Effect `kind`s map to [`ModEffect`]; the
//! pool holds each card at MAX rank and [`at_rank`] builds a lower one. Effect
//! kinds with no damage impact (dodge/acrobatic speed, weapon_scoped markers)
//! are loaded as no-ops. Unknown kinds are ignored with the mod still loaded,
//! so a not-yet-modeled special effect never silently drops the whole mod.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;
use serde_norway::Value;

use crate::rules::damage::DamageType;
use crate::model::{CondBucket, Faction, IndirectStat, ModDef, ModEffect, Rarity};
use crate::rules::capacity::Polarity;

mod parse;
mod pools;
mod describe;
#[cfg(test)]
mod tests;

use parse::*;
pub use pools::*;
pub use describe::*;
