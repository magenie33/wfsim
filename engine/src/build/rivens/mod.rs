// SPDX-License-Identifier: AGPL-3.0-or-later
//! Riven mods: the stat pool from `data/rivens/<class>.yaml`, the value
//! formula, the generated name, and the [`ModDef`] a riven resolves to.
//!
//! A riven is not a mod with fixed numbers — it is a mod whose numbers are
//! CONSTRUCTED from a roll, and this is that construction.
//!
//! ```text
//! shown value = base x 10 x (rank + 1) x disposition x config x roll
//! ```
//!
//! `base` is DE's own per-stat number (`upgradeEntries` in the export). The
//! `10 x (rank + 1)` term is 90 at rank 8, and TWO independent sources agree on
//! it: the wiki's own base-value column IS DE's number times 90, to four
//! figures including the ugly ones — 164.9997 / 149.9940 / 60.0300 against
//! Damage 165%, Critical Chance 149.99%, Fire Rate 60.03%.
//!
//! Every stat rolls its 0.9-1.1 INDEPENDENTLY, with no shared per-riven
//! quality, so the corner where every bonus is maximal and the malus minimal is
//! legal and astronomically unlikely. This is a CONSTRUCTOR rather than a
//! roller and must reach that corner: it is the ceiling the optimizer wants.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::rules::damage::DamageType;
use crate::model::{Faction, IndirectStat, ModDef, ModEffect, Rarity};
use crate::rules::capacity::Polarity;

mod stats;
mod pools;
mod spec;
mod rolls;
#[cfg(test)]
mod tests;

pub use stats::*;
pub use pools::*;
pub use spec::*;
pub use rolls::*;
