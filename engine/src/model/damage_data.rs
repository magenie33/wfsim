// SPDX-License-Identifier: AGPL-3.0-or-later
//! Native damage submission identity and counted ordinary status payloads.

/// Native proc ids have their own namespace; they are not Rust enum ordinals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProcType(u8);

#[wfsim_gas_macro::metered]
impl ProcType {
    pub fn new(value: u8) -> Option<Self> { (value < 30).then_some(Self(value)) }
    pub fn index(self) -> usize { self.0 as usize }
    pub fn id(self) -> u8 { self.0 }

    pub fn damage_type(self) -> Option<crate::rules::damage::DamageType> {
        use crate::rules::damage::DamageType::*;
        [Impact, Puncture, Slash, Heat, Cold, Electricity, Toxin, Blast,
            Radiation, Gas, Magnetic, Viral, Corrosive].get(self.index()).copied()
    }

    pub fn from_damage_type(kind: crate::rules::damage::DamageType) -> Option<Self> {
        (0..13).map(Self).find(|native| native.damage_type() == Some(kind))
    }
}

/// Native DamageData's byte counters saturate; script forced setters coalesce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcCounters([u8; 30]);

#[wfsim_gas_macro::metered]
impl ProcCounters {
    pub const ORDINARY_MASK: u32 = 0x1fff;

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
        self.0.iter().enumerate().fold(0, |mask, (i, &count)| mask | (u32::from(count > 0) << i))
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

/// Offset-bound part tags retain ANY_PART and unspecified independently of HEAD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitPart(pub u8);

#[wfsim_gas_macro::metered]
impl HitPart {
    pub const BODY: Self = Self(0);
    pub const HEAD: Self = Self(1);
    pub const ANY_PART: Self = Self(6);
    pub const UNSPECIFIED: Self = Self(7);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceId(pub u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeaponId(pub u32);

/// Semantic members with unrecovered native spellings retain Rust role names.
#[derive(Debug, Clone, Copy)]
pub struct DamageData<Scalar: Copy> {
    pub scalar: Scalar,
    pub source: Option<SourceId>,
    pub source_weapon: Option<WeaponId>,
    pub part: HitPart,
    pub position: [f32; 4],
    pub selected_proxy: Option<u32>,
    pub procs: ProcCounters,
}

#[wfsim_gas_macro::metered]
impl<Scalar: Copy> DamageData<Scalar> {
    pub fn new(scalar: Scalar) -> Self {
        Self { scalar, source: None, source_weapon: None,
            part: HitPart::UNSPECIFIED, position: [0.0; 4], selected_proxy: None,
            procs: ProcCounters::default() }
    }

    /// Primary's scalar copy and explicit source setters leave fresh geometry.
    pub fn derived_packet(&self) -> Self {
        let mut packet = Self::new(self.scalar);
        packet.source = self.source;
        packet.source_weapon = self.source_weapon;
        packet
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_setters_coalesce_while_counter_increments_saturate() {
        let kind = ProcType::new(5).unwrap();
        let mut counters = ProcCounters::default();
        counters.set_forced(kind);
        counters.set_forced(kind);
        assert_eq!(counters.count(kind), 1);
        counters.add(kind, 255);
        assert_eq!(counters.count(kind), 255);
        assert_eq!(counters.mask(), 1 << 5);
        assert!(ProcType::new(30).is_none());
    }

    #[test]
    fn accepted_callback_counts_compound_presence_and_coalesces_components() {
        use crate::rules::rng::NativeRng;
        let magnetic = ProcType::new(10).unwrap();
        for seed in 0..128 {
            let mut single = ProcCounters::default();
            single.add(magnetic, 1);
            let mut many = ProcCounters::default();
            many.add(magnetic, 20);
            let a = single.split_combined(1 << 10, &mut NativeRng::new(seed));
            let b = many.split_combined(1 << 10, &mut NativeRng::new(seed));
            assert_eq!(a, b);
            assert_eq!(a.types().count(), 1);
            assert!(a.present().all(|kind| [4, 5].contains(&kind.id())));
            assert_eq!(many.split_combined(0, &mut NativeRng::new(seed)), ProcCounters::default());
        }
        let mut incoming = ProcCounters::default();
        for kind in 7..13 { incoming.add(ProcType::new(kind).unwrap(), 20); }
        for seed in 0..128 {
            let result = incoming.split_combined(0x3f << 7, &mut NativeRng::new(seed));
            assert!((1..=4).contains(&result.types().count()));
            assert!(result.present().all(|kind| result.count(kind) == 1 && (3..7).contains(&kind.id())));
        }
    }

    #[test]
    fn each_ordinary_proc_maps_by_native_id_and_retains_its_count() {
        for id in 0..13 {
            let native = ProcType::new(id).unwrap();
            assert_eq!(ProcType::from_damage_type(native.damage_type().unwrap()), Some(native));
            let mut counters = ProcCounters::default();
            counters.add(native, 20);
            assert_eq!(ProcCounters::from_types(counters.types()), counters);
        }
    }

    #[test]
    fn scalar_copy_preserves_each_fresh_constructor_boundary() {
        for part in [HitPart::BODY, HitPart::HEAD, HitPart::ANY_PART] {
            for scalar in [0.0_f32, 123.25, 4096.0] {
                let mut parent = DamageData::new(scalar);
                parent.source = Some(SourceId(2));
                parent.source_weapon = Some(WeaponId(3));
                parent.part = part;
                parent.position = [10.0, 20.0, 30.0, 1.0];
                parent.selected_proxy = Some(4);
                parent.procs.add(ProcType::new(10).unwrap(), 20);
                let child = parent.derived_packet();
                assert_eq!(child.scalar.to_bits(), scalar.to_bits());
                assert_eq!((child.source, child.source_weapon),
                    (Some(SourceId(2)), Some(WeaponId(3))));
                assert_eq!(child.part, HitPart::UNSPECIFIED);
                assert_eq!(child.position, [0.0; 4]);
                assert_eq!(child.selected_proxy, None);
                assert_eq!(child.procs.mask(), 0);
            }
        }
    }
}
