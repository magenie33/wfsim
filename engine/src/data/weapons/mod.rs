// SPDX-License-Identifier: AGPL-3.0-or-later
//! Weapon data loader: `data/weapons/*.yaml` → [`WeaponBase`] + registry
//! metadata (CORE.md §2.3: weapon numbers are DATA; the engine only holds
//! rules). The yamls are the source of record — `loadout`'s per-weapon
//! constructors delegate here, and the web registry derives its weapon list,
//! tags, polarities and form descriptors from the same specs. `catalog` loads
//! the specs (`spec`, `attack`), `panel` turns one into a raw weapon, and the
//! rest answer the registry's questions; `kitguns` assembles a modular one.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use serde::Deserialize;

use crate::rules::damage::{DamageType, DamageVector};
use crate::model::WeaponBase;
use crate::model::{ChargeOn, FieldStacking, GaugeForm, LingeringBase, RadialBase};
use crate::model::CoBehavior;
use crate::rules::capacity::Polarity;
use crate::model::{Battery, BurstSpec, ChargeCadence, CompressionSpec, FalloffSpec, FormKind, HeavyAttack, KillStreakSummonSpec, MeterSpec, OrbSpec, SniperCombo, SpawnOnKillSpec, SpreadSpec, SuperCritSpec, SustainedFireRate, WeakpointStacksSpec};

pub mod kitguns;
use crate::model::{BlastKind, ComboHit};

mod valence;
mod attack;
mod spec;
mod catalog;
mod play_modes;
mod passives;
mod slots;
mod traits;
mod panel;
#[cfg(test)]
mod tests;

pub use valence::*;
pub use attack::*;
pub use spec::*;
pub use catalog::*;
pub use play_modes::*;
pub use passives::*;
pub use slots::*;
pub use traits::*;
pub use panel::*;
