// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE FIGHT — the simulator every number on the page comes from.
//!
//! An [`crate::arena::Arena`] and a resolved build become a flat [`FightParams`]
//! (`setup`), which [`run_once`] plays forward shot by shot against one or more
//! bodies (`run`); [`monte_carlo`] and [`shard`] repeat it, [`replay`] and
//! [`record`] re-run one engagement with its trace. Each other file is one piece
//! of what a shot does once it leaves the barrel. docs/MECHANICS.md is the
//! reference; body parts, crit tiers and the headcrit fold-in are its §5/§7.

use crate::arcanes_data::{ArcBuffSpec, ArcGrant, ArcTrigger, ArcaneFx};
use crate::buffs::BuffBar;
use crate::damage::{DamageType, DamageVector};
use crate::perks::frenzy::Frenzy;
use crate::perks::secondary_enervate::SecondaryEnervate;
use crate::perks::Perk;
use crate::rng::Rng;
use crate::scaling;
use crate::sim::{Event, Hit};
use crate::status;

mod locks;
mod arcane;
mod params;
mod pools;
mod target;
mod target_state;
mod dot;
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
mod ticks;
mod reload;
mod sampling;
mod run;
mod replay;
mod monte_carlo;
pub mod ledger;
#[cfg(test)]
mod tests;

pub use locks::*;
use arcane::*;
pub use params::*;
pub use pools::*;
pub use target::*;
use target_state::*;
use dot::*;
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
pub(crate) use beam::*;
use fields::*;
use orbs::*;
use ticks::*;
use reload::*;
use sampling::*;
pub use run::*;
pub use replay::*;
pub use monte_carlo::*;
