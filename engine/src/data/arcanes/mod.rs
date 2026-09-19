// SPDX-License-Identifier: AGPL-3.0-or-later
//! Declarative arcane loader: `data/arcanes/<slot>/*.yaml` -> the arcane pool.
//!
//! Arcanes are DATA, not code (same pattern as [`crate::data::mods`]). Each
//! YAML records the wiki-verified schema (X-templated description, effects
//! with `rank0`/`rankMax`, triggered buffs as `kind: buff`, an explicit
//! `ranks:` list only where per-rank values are non-linear). This module
//! parses them into [`ArcaneDef`] and resolves a def AT A RANK into the flat
//! [`ArcaneFx`] the simulator consumes.
//!
//! Policy handling mirrors mods (docs/OPTIMIZER.md §3): triggers the timeline
//! actually fires (kills, headshot kills, own Heat/Electricity procs, target
//! state) run EMERGENTLY; triggers outside the sim's world (rolls, ability
//! casts, weapon swaps, overshields) contribute their assumed-max value ONLY
//! under [`StackPolicy::AssumedMax`] — under `Emergent` they are honest
//! no-ops until the configured-buff policy lands.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;
use serde_norway::Value;

use crate::model::{Rarity, StackPolicy};
use crate::model::{count_x, fill_x, pct};
use crate::model::{ArcGrant, TennoStat};
use crate::model::ArcTrigger;
use crate::model::{ArcCondition, ArcEffect};
use crate::model::Scale;

mod parse;
mod fx;
mod def;
mod pools;
#[cfg(test)]
mod tests;

pub use parse::*;
pub use fx::*;
pub use def::*;
pub use pools::*;
