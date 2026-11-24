// SPDX-License-Identifier: AGPL-3.0-or-later
//! Lazy scene CDF reuse across independent hits with one quantized vector.

use super::{
    build_scene_cdf, cannot_saturate, HitProcs, ProcCounters, SceneProcHandlers, SCENE_CDF_CAP,
};
#[cfg(test)]
use super::scene_weights;
use crate::rules::{
    damage::{DamageType, DamageVector},
    rng::Rng,
};

/// A cache for one immutable composition, scoped to a group of hits. A change
/// to the target's handler selection replaces the cached CDF on the next draw.
pub(crate) struct SceneProcDraw<'a> {
    vector: &'a DamageVector,
    key: Option<u32>,
    entries: [(DamageType, f32); SCENE_CDF_CAP],
    len: usize,
    total: f32,
    output: HitProcs,
}

#[wfsim_gas_macro::metered]
impl<'a> SceneProcDraw<'a> {
    #[inline]
    pub(crate) fn new(vector: &'a DamageVector) -> Self {
        Self {
            vector,
            key: None,
            entries: [(DamageType::Impact, 0.0_f32); SCENE_CDF_CAP],
            len: 0,
            total: 0.0_f32,
            output: HitProcs::default(),
        }
    }

    #[inline]
    pub(crate) fn hit_procs(
        &mut self,
        forced: &[DamageType],
        status_chance: f64,
        handlers: SceneProcHandlers,
        rng: &mut Rng,
    ) -> HitProcs {
        let mut procs = HitProcs::from_slice(forced);
        let chance = status_chance as f32;
        if chance <= 0.0 {
            return procs;
        }
        if cannot_saturate(forced, chance) {
            let mut remaining = chance;
            while remaining > 0.0 {
                if rng.unit() > remaining { break; }
                self.ensure_cdf(handlers);
                let threshold = rng.unit() * self.total;
                for &(dtype, cumulative) in &self.entries[..self.len] {
                    if cumulative >= threshold {
                        procs.push(dtype);
                        break;
                    }
                }
                remaining -= 1.0_f32;
            }
        } else {
            self.sample_saturated_into(&mut procs, forced, chance, handlers, rng);
        }
        procs
    }

    pub(crate) fn procs_for_hit(
        &mut self,
        forced: &[DamageType],
        status_chance: f64,
        handlers: SceneProcHandlers,
        rng: &mut Rng,
    ) -> &[DamageType] {
        self.output = self.hit_procs(forced, status_chance, handlers, rng);
        &self.output
    }

    #[inline]
    fn ensure_cdf(&mut self, handlers: SceneProcHandlers) {
        let mask = (handlers.ordinary_mask & ProcCounters::ORDINARY_MASK)
            | (u32::from(handlers.void) << 30)
            | (u32::from(handlers.tau) << 31);
        if self.key != Some(mask) {
            (self.len, self.total) = build_scene_cdf(self.vector, handlers, &mut self.entries);
            self.key = Some(mask);
        }
    }

    #[cold]
    fn sample_saturated_into(
        &mut self,
        procs: &mut HitProcs,
        forced: &[DamageType],
        chance: f32,
        handlers: SceneProcHandlers,
        rng: &mut Rng,
    ) {
        let mut counts = [0u8; DamageType::ALL.len()];
        for &ty in forced {
            let slot = &mut counts[ty as usize];
            *slot = slot.saturating_add(1);
        }
        let mut remaining = chance;
        while remaining > 0.0 {
            if rng.unit() > remaining { break; }
            self.ensure_cdf(handlers);
            let threshold = rng.unit() * self.total;
            for &(dtype, cumulative) in &self.entries[..self.len] {
                if cumulative >= threshold {
                    let slot = &mut counts[dtype as usize];
                    if *slot < 255 {
                        *slot += 1;
                        procs.push(dtype);
                    }
                    break;
                }
            }
            remaining -= 1.0_f32;
        }
    }

    #[cfg(test)]
    fn sample(
        &mut self,
        chance: f32,
        handlers: SceneProcHandlers,
        mut random: impl FnMut() -> f32,
        mut selected: impl FnMut(Option<usize>),
    ) {
        let mut remaining = chance;
        while remaining > 0.0 {
            if random() > remaining {
                break;
            }
            self.ensure_cdf(handlers);
            let threshold = random() * self.total;
            let mut choice = None;
            for &(dtype, cumulative) in &self.entries[..self.len] {
                if cumulative >= threshold {
                    choice = match dtype {
                        DamageType::Void => Some(30),
                        DamageType::Tau => Some(31),
                        other => super::ProcType::from_damage_type(other).map(|p| p.id() as usize),
                    };
                    break;
                }
            }
            selected(choice);
            remaining -= 1.0_f32;
        }
    }
}

#[cfg(test)]
#[path = "tests/proc_draw.rs"]
mod tests;
