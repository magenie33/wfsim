// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ordinary critical consumer; source RVAs and native vectors are pinned in the critical fixture.

use super::rng::NativeRng;
use super::upgrades::{NativeScalar, OperationType, UpgradeType, UpgradeEntry, ScalarMutation};

/// Selected attribute operations stay pending until the hit's live query.
/// These are engineering interfaces; their stat ids retain the client's names.
#[derive(Debug, Clone, Copy)]
pub struct CriticalInputs {
    chance: CriticalAttribute,
    damage: CriticalAttribute,
    damage_without_gate: Option<(CriticalAttribute, f32)>,
}

#[derive(Debug, Clone, Copy)]
enum CriticalAttribute {
    Selected(NativeScalar),
    Supplied { resolved: f64, relative_base: f64 },
}

#[wfsim_gas_macro::metered]
impl CriticalAttribute {
    fn query(self, kind: UpgradeType, relative: f64, flat: f64) -> f64 {
        match self {
            Self::Selected(scalar) => live_scalar(scalar, kind, relative, flat) as f64,
            Self::Supplied { resolved, relative_base } => resolved + relative_base * relative + flat,
        }
    }
}

#[wfsim_gas_macro::metered]
impl CriticalInputs {
    pub fn selected(chance: f64, damage: f64, relative_chance: f64, relative_damage: f64,
        flat_chance: f64, flat_damage: f64) -> Self
    {
        Self {
            chance: CriticalAttribute::Selected(selected_scalar(UpgradeType::WEAPON_CRIT_CHANCE, chance, relative_chance, flat_chance)),
            damage: CriticalAttribute::Selected(selected_scalar(UpgradeType::WEAPON_CRIT_DAMAGE, damage, relative_damage, flat_damage)),
            damage_without_gate: None,
        }
    }

    pub fn with_damage_gate(mut self, without: f64, below: f64, relative: f64, flat: f64) -> Self {
        self.damage_without_gate = Some((CriticalAttribute::Selected(selected_scalar(UpgradeType::WEAPON_CRIT_DAMAGE, without, relative, flat)), below as f32));
        self
    }

    pub fn chance(self, relative: f64, flat: f64) -> f64 {
        self.chance.query(UpgradeType::WEAPON_CRIT_CHANCE, relative, flat)
    }

    pub fn damage(self, effective_chance: f64, relative: f64, flat: f64) -> f64 {
        let base = match self.damage_without_gate {
            Some((without, below)) if effective_chance as f32 >= below => without,
            _ => self.damage,
        };
        base.query(UpgradeType::WEAPON_CRIT_DAMAGE, relative, flat)
    }

    /// A caller's supplied chance does not replace the independently bound damage query.
    pub fn set_supplied_chance(&mut self, resolved: f64, relative_base: f64) {
        self.chance = CriticalAttribute::Supplied { resolved, relative_base };
    }

    pub fn set_supplied_damage(&mut self, resolved: f64, relative_base: f64, gate: Option<(f64, f64)>) {
        self.damage = CriticalAttribute::Supplied { resolved, relative_base };
        self.damage_without_gate = gate.map(|(granted, below)|
            (CriticalAttribute::Supplied { resolved: resolved - granted, relative_base }, below as f32));
    }
}

#[wfsim_gas_macro::metered]
fn selected_scalar(kind: UpgradeType, base: f64, relative: f64, flat: f64) -> NativeScalar {
    let mut scalar = NativeScalar::new(base as f32);
    scalar.install_selected(kind, entries(kind, relative, flat), ScalarMutation::RetainOperations);
    scalar
}

#[wfsim_gas_macro::metered]
fn entries(kind: UpgradeType, relative: f64, flat: f64) -> [UpgradeEntry; 2] {
    [UpgradeEntry { UpgradeType: kind, OperationType: OperationType::STACKING_MULTIPLY, Value: relative as f32 },
     UpgradeEntry { UpgradeType: kind, OperationType: OperationType::ADD, Value: flat as f32 }]
}

#[wfsim_gas_macro::metered]
fn live_scalar(mut scalar: NativeScalar, kind: UpgradeType, relative: f64, flat: f64) -> f32 {
    scalar.install_selected(kind, entries(kind, relative, flat), ScalarMutation::RetainOperations);
    scalar.value(false)
}

/// A supplied positive tier bypasses both the chance query and its random draw.
#[wfsim_gas_macro::metered]
pub fn roll(chance: f32, preset_tier: u32, rng: &mut NativeRng) -> u32 {
    if preset_tier > 0 { return preset_tier; }
    if chance <= 0.0 || chance.is_nan() { return 0; }
    let integral = chance.floor();
    let integer = super::upgrades::truncate_integer(integral);
    let tier = (integer as u8) as u32;
    let fraction = chance - integral;
    tier + u32::from(fraction > 0.0 && rng.unit() <= fraction)
}

#[wfsim_gas_macro::metered]
pub fn factor(tier: u32, multiplier: f32) -> f32 {
    if tier == 0 { return 1.0; }
    let step = multiplier - 1.0_f32;
    let extra = step * tier as f32;
    (1.0_f32 + extra).max(0.0)
}

/// The receiving critical block multiplies the source in binary32 after its zero guard.
#[wfsim_gas_macro::metered]
pub fn apply_factor(source: f32, factor: f32) -> f32 { source * factor.max(0.0) }

/// Canonical base plus ADD_BASE, before pending relative operations transfer.
#[wfsim_gas_macro::metered]
pub fn quantize(multiplier: f32) -> f32 {
    let scaled = multiplier.clamp(0.0, 32.0) * 127.96875_f32;
    let index = (scaled + 0.5_f32) as i32;
    index as f32 * (32.0_f32 / 4095.0_f32)
}

#[cfg(test)]
mod tests;
