// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pipeline layer [1]: mod resolution — a chosen mod set becomes panel stats.
//!
//! Buckets (docs/MECHANICS.md, docs/GLOSSARY.md): every relative bonus of one
//! kind sums additively into its bucket, then buckets combine by their real
//! rules (crit chance = base × (1 + Σcc); elemental amount = ModifiedBase ×
//! bonus; reload time = base / (1 + Σreload); …). Elemental entries enter the
//! layer-[2] hierarchy in **mod order** ([`crate::rules::elements`]).
//!
//! Conditional/stacking effects resolve under a [`StackPolicy`] — today only
//! `AssumedMax` (docs/OPTIMIZER.md §3: every stacking buff at full stacks).
//! `resolve` is the step, `panel` what comes out of it, `weapon_base` a
//! weapon's raw numbers going in and `describe` an effect's card line.

use crate::model::pct;
use crate::model::{AbilityStat, AcidShells, AcidShellsPart, CoBehavior, CondBucket, CritPerHit, Faction, HeadshotStreak, IndirectStat, InstantReload, MeleeIncarnon, ModDef, ModEffect, StackPolicy, StackSpec, Tennokai};
use crate::model::{BuffGrant, GatedGrant, StackingBuff, TennoScaledTerm, TimedBuff};
use crate::model::{BeamGeometry, CoBase, CoStage, Falloff, FieldStacking, GaugeForm, LingeringBase, RadialBase, Ricochet, Spread};
use crate::model::WeaponBase;
use crate::rules::damage::{DamageType, DamageVector};
use crate::rules::elements::{self, ElementalInput};

mod panel;
mod describe;
mod weapon_base;
mod resolve;
#[cfg(test)]
mod tests;

pub use panel::*;
pub use resolve::*;
