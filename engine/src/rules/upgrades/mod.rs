// SPDX-License-Identifier: AGPL-3.0-or-later
//! Native attribute folding; the source binding is in the upgrade-operations fixture.

mod ids;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct UpgradeType(pub u32);

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OperationType {
    ADD,
    ADD_BASE,
    MULTIPLY,
    STACKING_MULTIPLY,
    SET,
    SET_BASE,
    SET_MIN,
    SET_MAX,
}

/// An already-selected native upgrade entry; selection preserves caller order.
#[allow(non_snake_case)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UpgradeEntry {
    pub UpgradeType: UpgradeType,
    pub OperationType: OperationType,
    pub Value: f32,
}

/// QC_NONE rank transform; every multiply and add is a binary32 boundary.
#[wfsim_gas_macro::metered]
pub fn rank_none(operation: OperationType, base: f32, step: f32, rank: u32, max_rank: u32) -> f32 {
    if max_rank == 0 { return base; }
    let fraction = rank.min(max_rank) as f32 / max_rank as f32;
    let scaled_rank = max_rank as f32 * fraction;
    let inverse = operation == OperationType::MULTIPLY && (0.5..1.0).contains(&base);
    let origin = if inverse { 1.0_f32 - base } else { base };
    let step = if step == 0.0 { origin } else { step };
    let scaled = scaled_rank * step;
    let value = scaled + origin;
    if inverse { (1.0_f32 - value).max(0.0) } else { value }
}

#[derive(Debug, Clone, Copy)]
pub struct Accumulator {
    add: f32,
    add_base: f32,
    multiply: f32,
    stacking_multiply: f32,
    set: Option<f32>,
    set_base: Option<f32>,
    minimum: Option<f32>,
    maximum: Option<f32>,
}

#[wfsim_gas_macro::metered]
impl Default for Accumulator {
    fn default() -> Self {
        Self { add: 0.0, add_base: 0.0, multiply: 1.0, stacking_multiply: 0.0,
            set: None, set_base: None, minimum: None, maximum: None }
    }
}

/// SSE MINSS/MAXSS retain the second operand when unordered or equal.
#[wfsim_gas_macro::metered]
fn minimum(first: f32, second: f32) -> f32 {
    if first < second { first } else { second }
}

#[wfsim_gas_macro::metered]
fn maximum(first: f32, second: f32) -> f32 {
    if first > second { first } else { second }
}

#[wfsim_gas_macro::metered]
impl Accumulator {
    pub fn push(&mut self, operation: OperationType, value: f32) {
        match operation {
            OperationType::ADD => self.add += value,
            OperationType::ADD_BASE => self.add_base += value,
            OperationType::MULTIPLY => self.multiply *= value,
            OperationType::STACKING_MULTIPLY => self.stacking_multiply += value,
            OperationType::SET => self.set = Some(value),
            OperationType::SET_BASE => self.set_base = Some(value),
            OperationType::SET_MIN => self.minimum = Some(maximum(self.minimum.unwrap_or(f32::MIN_POSITIVE), value)),
            OperationType::SET_MAX => self.maximum = Some(minimum(self.maximum.unwrap_or(f32::MAX), value)),
        }
    }

    pub fn apply(self, original_base: f32, rounded: bool) -> f32 {
        let mut value = if let Some(value) = self.set {
            value
        } else {
            let base = self.set_base.unwrap_or(original_base) + self.add_base;
            let stacking = 1.0_f32 + self.stacking_multiply;
            let scaled = stacking * base;
            let added = scaled + self.add;
            let mut value = added * self.multiply;
            if let Some(bound) = self.minimum { value = maximum(bound, value); }
            if let Some(bound) = self.maximum { value = minimum(value, bound); }
            value
        };
        if rounded { value = round_integer(value); }
        value
    }
}

/// Caller-controlled native integer conversion, including the int32 sentinel.
#[wfsim_gas_macro::metered]
pub fn round_integer(value: f32) -> f32 {
    let biased = value + 0.5_f32;
    if !biased.is_finite() || !(-2147483648.0_f32..2147483648.0_f32).contains(&biased) {
        return biased;
    }
    let integral = biased as i32;
    if integral == i32::MIN || integral as f32 == biased { biased }
    else { (integral - i32::from(biased.is_sign_negative())) as f32 }
}

/// Fold the selected entries for one stat without changing their order.
#[wfsim_gas_macro::metered]
pub fn query(kind: UpgradeType, base: f32, entries: impl IntoIterator<Item = UpgradeEntry>, rounded: bool) -> f32 {
    let mut accumulator = Accumulator::default();
    for entry in entries {
        if entry.UpgradeType == kind { accumulator.push(entry.OperationType, entry.Value); }
    }
    accumulator.apply(base, rounded)
}

#[cfg(test)]
mod tests;
