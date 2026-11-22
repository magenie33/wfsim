// SPDX-License-Identifier: AGPL-3.0-or-later
//! Lazy scene CDF reuse across independent hits with one quantized vector.

use super::{
    append_hit_procs, append_unsaturated, cannot_saturate, ordinary::Cdf, scene_weights,
    ProcCounters, SceneProcHandlers,
};
use crate::rules::{
    damage::{DamageType, DamageVector},
    rng::Rng,
};

const SCENE_TYPES: usize = ProcCounters::ORDINARY_MASK.count_ones() as usize + 2;

/// A cache for one immutable composition, scoped to a group of hits. A change
/// to the target's handler selection replaces the cached CDF on the next draw.
pub(crate) struct SceneProcDraw<'a> {
    vector: &'a DamageVector,
    key: Option<u32>,
    cdf: Cdf<SCENE_TYPES>,
    output: Vec<DamageType>,
}

#[wfsim_gas_macro::metered]
impl<'a> SceneProcDraw<'a> {
    pub(crate) fn new(vector: &'a DamageVector) -> Self {
        Self {
            vector,
            key: None,
            cdf: Cdf::empty(),
            output: Vec::new(),
        }
    }

    pub(crate) fn procs_for_hit(
        &mut self,
        forced: &[DamageType],
        status_chance: f64,
        handlers: SceneProcHandlers,
        rng: &mut Rng,
    ) -> &[DamageType] {
        let mut procs = std::mem::take(&mut self.output);
        procs.clear();
        procs.extend_from_slice(forced);
        let chance = status_chance as f32;
        if cannot_saturate(forced, chance) {
            self.sample(
                chance,
                handlers,
                || rng.unit(),
                |index| append_unsaturated(&mut procs, index),
            );
        } else {
            self.sample(
                chance,
                handlers,
                || rng.unit(),
                append_hit_procs(&mut procs, forced),
            );
        }
        self.output = procs;
        &self.output
    }

    fn sample(
        &mut self,
        chance: f32,
        handlers: SceneProcHandlers,
        random: impl FnMut() -> f32,
        selected: impl FnMut(Option<usize>),
    ) {
        let Self { vector, key, cdf, .. } = self;
        cdf.sample(
            chance,
            |cdf| {
                let mask = (handlers.ordinary_mask & ProcCounters::ORDINARY_MASK)
                    | (u32::from(handlers.void) << 30)
                    | (u32::from(handlers.tau) << 31);
                if *key != Some(mask) {
                    cdf.build(&scene_weights(vector, handlers));
                    *key = Some(mask);
                }
            },
            random,
            selected,
        );
    }
}

#[cfg(test)]
#[path = "tests/proc_draw.rs"]
mod tests;
