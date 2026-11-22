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

/// Ordered binary32 prefixes with one entry per positive weight.
#[cfg(test)]
pub(super) struct Cdf<const N: usize> {
    entries: [(usize, f32); N],
    len: usize,
    total: f32,
}

#[cfg(test)]
#[wfsim_gas_macro::metered]
impl<const N: usize> Cdf<N> {
    pub(super) fn empty() -> Self {
        Self {
            entries: [(0, 0.0); N],
            len: 0,
            total: 0.0,
        }
    }

    /// Capacity must cover the positive entries. All slots participate in the fold.
    pub(super) fn build(&mut self, weights: &[f32]) {
        self.len = 0;
        self.total = 0.0;
        for (index, &weight) in weights.iter().enumerate() {
            self.total += weight;
            if weight > 0.0 {
                self.entries[self.len] = (index, self.total);
                self.len += 1;
            }
        }
    }

    /// Prepare once, after the first successful chance and before its type draw.
    /// The caller may keep an existing table. Every success still draws a type.
    pub(super) fn sample(&mut self, chance: f32, prepare: impl FnOnce(&mut Self),
        mut random: impl FnMut() -> f32, mut selected: impl FnMut(Option<usize>))
    {
        let mut prepare = Some(prepare);
        let mut remaining = chance;
        while remaining > 0.0 {
            if random() > remaining { break; }
            if let Some(prepare) = prepare.take() { prepare(self); }
            let threshold = random() * self.total;
            let mut choice = None;
            for &(index, cumulative) in &self.entries[..self.len] {
                if cumulative >= threshold {
                    choice = Some(index);
                    break;
                }
            }
            selected(choice);
            remaining -= 1.0_f32;
        }
    }
}

/// A packet owns its table for the duration of its hit.
#[cfg(test)]
#[wfsim_gas_macro::metered]
pub(super) fn sample<const N: usize>(chance: f32, skip_byte_0x241: u8,
    evaluate: impl FnOnce() -> [f32; N], random: impl FnMut() -> f32,
    selected: impl FnMut(Option<usize>))
{
    if skip_byte_0x241 != 0 { return; }
    Cdf::<N>::empty().sample(chance, |cdf| cdf.build(&evaluate()), random, selected);
}

/// 310c60 core: no NumHits scaling or final-immunity filtering occurs here.
#[wfsim_gas_macro::metered]
pub fn generate(chance: f32, skip_byte_0x241: u8, components: &DamageComponents,
    handlers: &[&[ProcEvaluator]; 30], counters: &mut ProcCounters, rng: &mut NativeRng)
{
    if skip_byte_0x241 != 0 || chance <= 0.0 { return; }
    let mut entries = [(0usize, 0.0_f32); 30];
    let mut len = 0usize;
    let mut total = 0.0_f32;
    let mut built = false;
    let mut remaining = chance;
    while remaining > 0.0 {
        if rng.unit() > remaining { break; }
        if !built {
            for (index, evals) in handlers.iter().enumerate() {
                let weight = handler_weight(evals, components);
                total += weight;
                if weight > 0.0 {
                    entries[len] = (index, total);
                    len += 1;
                }
            }
            built = true;
        }
        let threshold = rng.unit() * total;
        for &(index, cumulative) in &entries[..len] {
            if cumulative >= threshold {
                counters.add(ProcType::new(index as u8).expect("proc index"), 1);
                break;
            }
        }
        remaining -= 1.0_f32;
    }
}

#[cfg(test)]
#[path = "tests/ordinary.rs"]
mod tests;
