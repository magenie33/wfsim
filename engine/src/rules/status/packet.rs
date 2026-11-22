// SPDX-License-Identifier: AGPL-3.0-or-later
//! Native status identity and counted submission primitives.

/// Native proc ids have their own namespace; they are not Rust enum ordinals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProcType(u8);

pub(crate) const ORDINARY_DAMAGE_TYPES: [crate::rules::damage::DamageType; 13] = {
    use crate::rules::damage::DamageType::*;
    [
        Impact, Puncture, Slash, Heat, Cold, Electricity, Toxin,
        Blast, Radiation, Gas, Magnetic, Viral, Corrosive,
    ]
};

#[wfsim_gas_macro::metered]
impl ProcType {
    #[inline]
    pub fn new(value: u8) -> Option<Self> {
        if value < 30 { Some(Self(value)) } else { None }
    }
    #[inline]
    pub fn index(self) -> usize { self.0 as usize }
    #[inline]
    pub fn id(self) -> u8 { self.0 }

    #[inline]
    pub fn damage_type(self) -> Option<crate::rules::damage::DamageType> {
        ORDINARY_DAMAGE_TYPES.get(self.index()).copied()
    }

    #[inline]
    pub fn from_damage_type(kind: crate::rules::damage::DamageType) -> Option<Self> {
        use crate::rules::damage::DamageType::*;
        let id = match kind {
            Impact => 0,
            Puncture => 1,
            Slash => 2,
            Heat => 3,
            Cold => 4,
            Electricity => 5,
            Toxin => 6,
            Blast => 7,
            Radiation => 8,
            Gas => 9,
            Magnetic => 10,
            Viral => 11,
            Corrosive => 12,
            _ => return None,
        };
        Some(Self(id))
    }
}

/// Native DamageData's byte counters saturate; script forced setters coalesce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcCounters([u8; 30]);

#[wfsim_gas_macro::metered]
impl ProcCounters {
    pub const ORDINARY_MASK: u32 = 0x1fff;

    pub fn from_bytes(counters: [u8; 30]) -> Self { Self(counters) }
    pub fn bytes(&self) -> &[u8; 30] { &self.0 }

    pub fn from_types(types: impl IntoIterator<Item = crate::rules::damage::DamageType>) -> Self {
        let mut counters = Self::default();
        for kind in types {
            if let Some(kind) = ProcType::from_damage_type(kind) { counters.add(kind, 1); }
        }
        counters
    }

    pub fn types(&self) -> impl Iterator<Item = crate::rules::damage::DamageType> + '_ {
        self.present().flat_map(|kind| std::iter::repeat_n(kind.damage_type(), self.count(kind) as usize))
            .flatten()
    }

    /// Accepted Primary callback: per-type presence, one choice, coalescing setter.
    pub fn split_combined(&self, qualified: u32, random: &mut crate::rules::rng::NativeRng) -> Self {
        let mut result = Self::default();
        let components = [(3, 4), (3, 5), (3, 6), (4, 5), (4, 6), (5, 6)];
        for (index, (a, b)) in components.into_iter().enumerate() {
            let kind = ProcType(index as u8 + 7);
            if self.count(kind) > 0 && qualified & (1 << kind.id()) != 0 {
                result.set_forced(ProcType(if random.index(2) == 0 { a } else { b }));
            }
        }
        result
    }
    pub fn count(&self, kind: ProcType) -> u8 { self.0[kind.index()] }
    pub fn add(&mut self, kind: ProcType, count: u8) {
        self.0[kind.index()] = self.0[kind.index()].saturating_add(count);
    }
    pub fn set_forced(&mut self, kind: ProcType) { self.0[kind.index()] = 1; }
    pub fn mask(&self) -> u32 {
        let mut mask = 0u32;
        for (i, &count) in self.0.iter().enumerate() {
            mask |= u32::from(count > 0) << i;
        }
        mask
    }
    pub fn present(&self) -> impl Iterator<Item = ProcType> + '_ {
        self.0.iter().enumerate().filter_map(|(i, &count)| (count > 0).then_some(ProcType(i as u8)))
    }

    /// Source sampler: inclusive chance, modulo choice and byte increments.
    pub fn encumber(&mut self, chance: f32, valid_double_procs: u32,
        random: &mut crate::rules::rng::NativeRng) -> bool
    {
        if self.mask() & valid_double_procs == 0 || chance <= 0.0 { return false; }
        let mut candidates = [0_u8; 30];
        let mut length = 0;
        for kind in 0..30 {
            if valid_double_procs & (1 << kind) != 0 {
                candidates[length] = kind;
                length += 1;
            }
        }
        let mut remaining = chance;
        let mut selected = false;
        while length > 0 && remaining > 0.0 {
            if random.unit() > remaining { break; }
            let index = random.index(length);
            self.add(ProcType(candidates[index]), 1);
            candidates[index] = candidates[length - 1];
            length -= 1;
            remaining -= 1.0_f32;
            selected = true;
        }
        selected
    }
}
