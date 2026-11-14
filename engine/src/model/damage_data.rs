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
    /// None is the constructor's native discriminator30; ordinary counter ids stay0..29.
    pub origin_proc: Option<ProcType>,
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
        Self { scalar, origin_proc: None, source: None, source_weapon: None,
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

#[wfsim_gas_macro::metered]
impl DamageData<crate::rules::upgrades::NativeScalar> {
    pub fn apply_proc_upgrades(&mut self, kind: ProcType,
        entries: impl IntoIterator<Item = crate::rules::upgrades::ProcFilteredUpgrade>,
        mutation: crate::rules::upgrades::ScalarMutation)
    {
        let stat = crate::rules::upgrades::UpgradeType::WEAPON_PROC_DAMAGE;
        self.scalar.install_selected(stat, entries.into_iter().filter_map(|entry| entry.for_proc(kind)), mutation);
    }

    pub fn retained_proc_seed(&self, kind: ProcType,
        entries: impl IntoIterator<Item = crate::rules::upgrades::ProcFilteredUpgrade>) -> f32
    {
        self.scalar.query_selected(crate::rules::upgrades::UpgradeType::WEAPON_PROC_DAMAGE,
            entries.into_iter().filter_map(|entry| entry.for_proc(kind)), false)
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

    #[test]
    fn original_parent_and_child_queries_pass_through_the_packet_boundary() {
        use crate::rules::upgrades::{NativeScalar, OperationType, ProcFilteredUpgrade,
            ScalarMutation, UpgradeEntry, UpgradeType};
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/client/packet-context-2026-10-28.json")).unwrap();
        let cases = fixture["two_stage_upgrade_cases"].as_array().unwrap();
        assert_eq!(cases.len(), 54);
        for case in cases {
            let entry = ProcFilteredUpgrade {
                entry: UpgradeEntry { UpgradeType: UpgradeType::WEAPON_PROC_DAMAGE,
                    OperationType: OperationType::STACKING_MULTIPLY,
                    Value: case["supplied_stacking_bonus"].as_f64().unwrap() as f32 },
                proc_mask: case["supplied_valid_proc_mask"].as_u64().unwrap() as u32,
            };
            let parent_proc = ProcType::new(case["parent_proc_type"].as_u64().unwrap() as u8).unwrap();
            let child_proc = ProcType::new(case["child_proc_type"].as_u64().unwrap() as u8).unwrap();
            let mut parent = DamageData::new(NativeScalar::new(case["input_parent_source"].as_f64().unwrap() as f32));
            parent.origin_proc = Some(parent_proc);
            parent.procs.set_forced(child_proc);
            let mutation = if case["supplied_canonicalization_flag"] == 0 {
                ScalarMutation::Materialize
            } else { ScalarMutation::RetainOperations };
            parent.apply_proc_upgrades(parent_proc, [entry], mutation);
            assert_eq!(parent.scalar.value(false).to_bits(), (case["parent_evaluated_source"].as_f64().unwrap() as f32).to_bits(), "{case}");
            assert_eq!(parent.scalar.base().to_bits(), (case["ordinary_proc_retained_input"].as_f64().unwrap() as f32).to_bits(), "{case}");
            assert_eq!(parent.retained_proc_seed(child_proc, [entry]).to_bits(),
                (case["ordinary_proc_stored_seed"].as_f64().unwrap() as f32).to_bits(), "{case}");
            assert_eq!(parent.procs.count(child_proc), 1);
        }
    }
}
