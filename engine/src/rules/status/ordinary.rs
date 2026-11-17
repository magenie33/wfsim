// SPDX-License-Identifier: AGPL-3.0-or-later
//! N07A: supplied ProcEvaluator graph through ordinary DamageData counters.

use super::{ProcCounters, ProcType};
use crate::rules::rng::NativeRng;

/// Native damage-component indices, independent of the proc-id namespace.
pub type DamageComponents = [f32; 20];

/// Selected evaluator parameters; serialized Chance is not a weight input.
#[allow(non_snake_case)]
#[derive(Debug, Clone, Copy)]
pub struct ProcEvaluator {
    pub DamageTypes: u32,
    pub WeightMultiplier: f32,
}

#[wfsim_gas_macro::metered]
impl ProcEvaluator {
    pub fn selects(&self, component: usize) -> bool {
        component < 20 && self.component_mask() & (1 << component) != 0
    }

    fn component_mask(&self) -> u32 {
        let mut mask = self.DamageTypes & 0xfffff;
        for (selector, group) in [(21, 0x7), (23, 0x78), (24, 0x1f80), (25, 0xfffff)] {
            if self.DamageTypes & (1 << selector) != 0 { mask |= group; }
        }
        mask
    }

    /// 130de40: positive components, ascending indices, separate binary32 sum/mul.
    pub fn weight(&self, components: &DamageComponents) -> f32 {
        let mut sum = 0.0_f32;
        let mut mask = self.component_mask();
        while mask != 0 {
            let index = mask.trailing_zeros() as usize;
            let component = components[index];
            if component > 0.0 { sum += component; }
            mask &= mask - 1;
        }
        sum * self.WeightMultiplier
    }
}

/// 185c310: an absent handler is empty; multiple evaluators take their maximum.
#[wfsim_gas_macro::metered]
pub fn handler_weight(evaluators: &[ProcEvaluator], components: &DamageComponents) -> f32 {
    let mut maximum = 0.0_f32;
    for evaluator in evaluators {
        let weight = evaluator.weight(components);
        if weight > maximum { maximum = weight; }
    }
    maximum
}

/// Shared chance/CDF loop. Weights are lazy; every successful chance draws a type.
/// The selected callback owns the counter namespace and saturation.
#[wfsim_gas_macro::metered]
pub(super) fn sample<const N: usize>(chance: f32, skip_byte_0x241: u8,
    evaluate: impl FnOnce() -> [f32; N], mut random: impl FnMut() -> f32,
    mut selected: impl FnMut(Option<usize>))
{
    if skip_byte_0x241 != 0 { return; }
    let mut evaluate = Some(evaluate);
    let mut cdf = [(0_usize, 0.0_f32); N];
    let mut length = 0;
    let mut total = 0.0_f32;
    let mut remaining = chance;
    while remaining > 0.0 {
        if random() > remaining { break; }
        if let Some(evaluate) = evaluate.take() {
            // Immutable weights give the same ordered binary32 prefixes on every draw.
            for (index, weight) in evaluate().into_iter().enumerate() {
                total += weight;
                if weight > 0.0 {
                    cdf[length] = (index, total);
                    length += 1;
                }
            }
        }
        let threshold = random() * total;
        let mut choice = None;
        for &(index, cumulative) in &cdf[..length] {
            if cumulative >= threshold {
                choice = Some(index);
                break;
            }
        }
        selected(choice);
        remaining -= 1.0_f32;
    }
}

/// 310c60 core: no NumHits scaling or final-immunity filtering occurs here.
#[wfsim_gas_macro::metered]
pub fn generate(chance: f32, skip_byte_0x241: u8, components: &DamageComponents,
    handlers: &[&[ProcEvaluator]; 30], counters: &mut ProcCounters, rng: &mut NativeRng)
{
    sample::<30>(chance, skip_byte_0x241,
        || std::array::from_fn(|index| handler_weight(handlers[index], components)),
        || rng.unit(), |index| {
            if let Some(index) = index { counters.add(ProcType::new(index as u8).expect("proc index"), 1); }
        });
}

#[cfg(test)]
#[path = "tests/ordinary.rs"]
mod tests;
