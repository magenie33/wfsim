// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE FIGHT — the simulator every number on the page comes from.
//!
//! An [`crate::arena::Arena`] and a resolved build become a flat [`FightParams`]
//! (`setup`), which [`run_once`] plays forward shot by shot against one or more
//! bodies (`run`); [`monte_carlo`] and [`shard`] repeat it, [`replay`] and
//! [`record`] re-run one engagement with its trace. Each other file is one piece
//! of what a shot does once it leaves the barrel. docs/MECHANICS.md is the
//! reference; body parts, crit tiers and the headcrit fold-in are its §5/§7.

use crate::data::arcanes::{ArcBuffSpec, ArcaneFx};
use crate::model::ArcTrigger;
use crate::model::ArcGrant;
use crate::rules::buffs::BuffBar;
use crate::rules::damage::{DamageType, DamageVector};
use crate::rules::perks::frenzy::Frenzy;
use crate::rules::perks::secondary_enervate::SecondaryEnervate;
use crate::rules::perks::Perk;
use crate::rules::rng::Rng;
use crate::rules::scaling;
use crate::rules::sim::{Event, Hit};
use crate::rules::status;
use crate::target::*;
use crate::rules::metrics::RunStat;
use crate::record::PopKind;

mod locks;
mod arcane;
mod params;
mod pools;
mod target_state;
mod dot;
mod bumps;
mod cycle;
mod debuffs;
mod melee;
mod trace;
mod setup;
mod result;
mod layers;
mod rolls;
mod stacks;
mod procs;
mod scale;
mod spread;
mod beam;
mod fields;
mod orbs;
mod pellet;
mod ticks;
mod reload;
mod sampling;
mod run;
mod state;
mod replay;
mod monte_carlo;
pub mod ledger;
#[cfg(test)]
mod tests;

pub use locks::*;
use arcane::*;
pub use params::*;
pub use pools::*;
use target_state::*;
use dot::*;
use bumps::bump_buffs;
use cycle::*;
pub use debuffs::*;
use melee::*;
pub use trace::*;
pub use result::*;
use layers::*;
use rolls::*;
use stacks::*;
pub use procs::*;
use scale::*;
use spread::*;
use beam::*;
use fields::*;
use orbs::*;
use pellet::*;
use ticks::*;
use reload::*;
use sampling::*;
pub use run::*;
use state::*;
pub use replay::*;
pub use monte_carlo::*;
