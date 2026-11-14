// SPDX-License-Identifier: AGPL-3.0-or-later
//! Source-spelled rank inputs compiled outside the per-hit working set.

use crate::rules::upgrades::{rank_none, query, UpgradeEntry, UpgradeType};

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub enum FusionPowerCurve { QC_NONE }

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
        match self.FusionPowerCurve {
            FusionPowerCurve::QC_NONE => rank_none(self.entry.OperationType,
                self.entry.Value, self.FusionPowerStep, rank, max_rank),
        }
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
        let chance = if !self.ChanceScalesWithRank { self.UpgradeChance }
        else if self.UpgradeChanceScalePerRank > 0.0 {
            let scaled = rank as f32 * self.UpgradeChanceScalePerRank;
            self.UpgradeChance + scaled
        } else { self.UpgradeChance * (rank as f32 + 1.0_f32) };
        chance.clamp(0.0, 1.0)
    }
}
