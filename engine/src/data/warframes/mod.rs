// SPDX-License-Identifier: AGPL-3.0-or-later
//! WARFRAMES — the builder's frame, and everything a build seats on it.
//!
//! A Warframe build is eight mods, an exilus, an aura, two arcanes, five archon
//! shards and at most one Helminth infusion. [`resolve`] turns one into the
//! numbers the arsenal shows and the four abilities at those numbers. Nothing
//! here reaches a fight; the rules and their sources are `docs/WARFRAMES.md`.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::data::auras::AuraEffect;
use crate::data::shards::ShardPick;

mod stats;
mod focus;
mod artifacts;
mod cards;
mod abilities;
mod frames;
mod resolve;
#[cfg(test)]
mod tests;

pub use stats::*;
pub use focus::*;
pub use artifacts::*;
pub use cards::*;
pub use abilities::*;
pub use frames::*;
pub use resolve::*;
