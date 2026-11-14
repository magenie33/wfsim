// SPDX-License-Identifier: AGPL-3.0-or-later
//! Source-spelled rank inputs compiled outside the per-hit working set.

use crate::rules::upgrades::{rank_value, query, UpgradeEntry, UpgradeType};
pub use crate::rules::upgrades::FusionPowerCurve;

#[allow(non_snake_case)]
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct RankedUpgrade {
    #[serde(flatten)]
    pub entry: UpgradeEntry,
    pub FusionPowerStep: f32,
    pub FusionPowerCurve: FusionPowerCurve,
}

#[wfsim_gas_macro::metered]
impl RankedUpgrade {
    pub fn value(self, rank: u32, max_rank: u32) -> f32 {
        rank_value(self.entry.UpgradeType, self.entry.OperationType,
            self.entry.Value, self.FusionPowerStep, rank, max_rank, self.FusionPowerCurve)
    }

    pub fn unlocked_value(self, rank: u32, max_rank: u32, unlock: u32) -> Option<f32> {
        if rank < unlock || max_rank < unlock { return None; }
        Some(self.value(rank - unlock, max_rank - unlock))
    }

    pub fn query(self, kind: UpgradeType, base: f32, rank: u32, max_rank: u32) -> f32 {
        query(kind, base, [UpgradeEntry { Value: self.value(rank, max_rank), ..self.entry }], false)
    }
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct ConditionalChance {
    pub UpgradeChance: f32,
    pub UpgradeChanceScalePerRank: f32,
    pub ChanceScalesWithRank: bool,
}

#[wfsim_gas_macro::metered]
impl ConditionalChance {
    pub fn at(self, rank: u32) -> f32 {
        let factor = rank.wrapping_add(1) as f32;
        let chance = if !self.ChanceScalesWithRank { self.UpgradeChance }
        else if self.UpgradeChanceScalePerRank > 0.0 {
            let scaled = (factor - 1.0_f32) * self.UpgradeChanceScalePerRank;
            self.UpgradeChance + scaled
        } else { self.UpgradeChance * factor };
        chance.clamp(0.0, 1.0)
    }
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct ConditionalParameters {
    #[serde(flatten)]
    pub chance: ConditionalChance,
    pub UpgradeDuration: f32,
    pub DurationScalesWithRank: bool,
    pub MaxConditionalStacks: u8,
    pub MaxConditionalStacksFusionStep: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct ConditionalInputs {
    pub chance: f32,
    pub duration_seconds: f32,
    pub max_stacks: u8,
}

#[wfsim_gas_macro::metered]
impl ConditionalParameters {
    pub fn at(self, rank: u32) -> ConditionalInputs {
        ConditionalInputs {
            chance: self.chance.at(rank),
            duration_seconds: if self.DurationScalesWithRank {
                self.UpgradeDuration * rank.wrapping_add(1) as f32
            } else { self.UpgradeDuration },
            max_stacks: self.MaxConditionalStacks.wrapping_add((rank as u8).wrapping_mul(self.MaxConditionalStacksFusionStep)),
        }
    }
}
