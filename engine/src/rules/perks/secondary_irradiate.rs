// SPDX-License-Identifier: AGPL-3.0-or-later
//! RadialDamageOnMaxRadStackHit.lua's owner counter and explicit resumption.
//! Source and scheduler boundaries: docs/SECONDARY_IRRADIATE.md.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitClass { Ordinary, Radial, Script, Dot }

#[derive(Debug, Clone, Copy)]
pub struct Source {
    pub weapon_type: u64,
    pub source_proc_type: u8,
    pub arcane_intermediate: bool,
}

#[wfsim_gas_macro::metered]
impl Source {
    fn accepted(self) -> bool { self.source_proc_type == 30 && !self.arcane_intermediate }
}

/// The keyed OnDamaged callable belongs to the victim, shared by source owners.
#[derive(Debug, Default, Clone)]
pub struct Listener { installed: bool }

#[wfsim_gas_macro::metered]
impl Listener {
    pub fn installed(&self) -> bool { self.installed }
    pub fn invalidate(&mut self) { self.installed = false; }
}

/// A suspended activation retains its original query context, never the last hit's.
#[derive(Debug)]
pub struct Job<C> { victim: usize, context: C }

#[derive(Debug)]
pub struct Emission<C, Q> {
    pub context: C,
    pub queried: Q,
    pub num_hits: f64,
    pub coefficient: f64,
}

/// These are the recovered script keys, not inferred native C++ member names.
#[allow(non_snake_case)]
#[derive(Debug, Default)]
pub struct SecondaryIrradiate {
    pub RadialDamageOnMaxRadStackHit_WeaponType: Option<u64>,
    pub RadialDamageOnMaxRadStackHit_IsFullyCharged: Option<bool>,
    RadialDamageOnMaxRadStackHit_FrameHitCounter: Vec<f64>,
}

#[wfsim_gas_macro::metered]
impl SecondaryIrradiate {
    pub fn count(&self, victim: usize) -> f64 {
        self.RadialDamageOnMaxRadStackHit_FrameHitCounter.get(victim).copied().unwrap_or(0.0)
    }

    fn counter(&mut self, victim: usize) -> &mut f64 {
        self.RadialDamageOnMaxRadStackHit_FrameHitCounter.resize(
            self.RadialDamageOnMaxRadStackHit_FrameHitCounter.len().max(victim + 1), 0.0);
        &mut self.RadialDamageOnMaxRadStackHit_FrameHitCounter[victim]
    }

    pub fn activate<C>(&mut self, listener: &mut Listener, victim: usize,
        radiation: usize, class: HitClass, source: Option<Source>, context: C) -> Option<Job<C>>
    {
        let source = source.filter(|s| s.accepted())?;
        if radiation < 10 || class != HitClass::Ordinary { return None; }
        self.RadialDamageOnMaxRadStackHit_WeaponType = Some(source.weapon_type);
        self.counter(victim);
        self.RadialDamageOnMaxRadStackHit_IsFullyCharged = None;
        listener.installed = true;
        Some(Job { victim, context })
    }

    /// Collection intentionally does not repeat activation's hit-class exclusions.
    /// The caller supplies IsA(stored_type); logical weapon ids are not native types.
    #[allow(clippy::too_many_arguments)]
    pub fn collect(&mut self, listener: &Listener, victim: usize, radiation: usize,
        source: Option<Source>, is_a: impl FnOnce(u64) -> bool,
        charged: bool, num_hits: u32) -> bool
    {
        if !listener.installed { return false; }
        if !source.is_some_and(|s| s.accepted()) { return false; }
        let Some(stored) = self.RadialDamageOnMaxRadStackHit_WeaponType else { return false; };
        if !is_a(stored) { return false; }
        if radiation < 10 {
            *self.counter(victim) = 0.0;
            return true;
        }
        self.RadialDamageOnMaxRadStackHit_IsFullyCharged = Some(charged);
        *self.counter(victim) += f64::from(num_hits);
        true
    }

    /// Caller supplies the Sleep(0) boundary and the original source/victim guards.
    /// Query precedes read/clear even for zero count or a nonpositive coefficient.
    pub fn resume<C, Q>(&mut self, job: Job<C>, context_valid: bool, configured: f64,
        query: impl FnOnce(&C, Option<bool>) -> Q) -> Option<Emission<C, Q>>
    {
        if !context_valid { return None; }
        let queried = query(&job.context, self.RadialDamageOnMaxRadStackHit_IsFullyCharged);
        let num_hits = self.count(job.victim);
        *self.counter(job.victim) = 0.0;
        let coefficient = configured * num_hits;
        if coefficient <= 0.0 { return None; }
        Some(Emission { context: job.context, queried, num_hits, coefficient })
    }
}

#[cfg(test)]
#[path = "tests/secondary_irradiate.rs"]
mod tests;
