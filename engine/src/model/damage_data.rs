// SPDX-License-Identifier: AGPL-3.0-or-later
//! Native damage submission identity and counted ordinary status payloads.

pub use crate::rules::status::{ProcCounters, ProcType};

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
