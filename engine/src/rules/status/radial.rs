// SPDX-License-Identifier: AGPL-3.0-or-later
//! Electric/Gas retained-record reduction; packet-context fixture binds the offsets.

/// Ordered opaque identities; their producer/ownership semantics remain caller-bound.
#[allow(non_snake_case)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct GroupKey {
    pub refCellAt28: u64,
    pub refCellAt30: u64,
    pub byteAt18: u8,
    pub opaqueIdentityAt38: u64,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecordContext {
    pub payload40: u32,
    pub payload70: u32,
    pub payload60: u8,
    pub part74: u8,
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Copy)]
pub struct RadialGroup {
    pub accumulator: f32,
    pub count: u32,
    pub payload4c: u32,
    pub payload50: u32,
    pub payload54: u8,
    pub part55: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct PreparedRadial {
    pub scaled: f32,
    pub clamped: f32,
    pub element: f32,
    pub damage: f32,
}

#[wfsim_gas_macro::metered]
impl Default for RadialGroup {
    fn default() -> Self {
        Self { accumulator: 1.0, count: 0, payload4c: 0, payload50: 0, payload54: 0, part55: 7 }
    }
}

#[wfsim_gas_macro::metered]
impl RadialGroup {
    /// Native visitation order is significant; the last visited record writes the context.
    pub fn visit(&mut self, retained_seed: f32, context: RecordContext) {
        self.accumulator += retained_seed;
        self.count = self.count.wrapping_add(1);
        self.payload4c = context.payload40;
        self.payload50 = context.payload70;
        self.payload54 = context.payload60;
        self.part55 = context.part74;
    }

    /// The local minimum precedes the own-element lookup, not final recipient mitigation.
    pub fn prepare(self, coefficient: f32, own_element_percentage: f32) -> f32 {
        self.prepare_steps(coefficient, own_element_percentage).damage
    }

    pub fn prepare_steps(self, coefficient: f32, own_element_percentage: f32) -> PreparedRadial {
        let scaled = coefficient * self.accumulator;
        let clamped = scaled.max(1.0);
        let element = 1.0_f32 + own_element_percentage;
        PreparedRadial { scaled, clamped, element, damage: clamped * element }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    #[wfsim_gas_macro::metered]
    fn every_original_record_and_radial_transfer_matches_binary32() {
        let data: serde_json::Value = serde_json::from_str(include_str!("../../../../tests/fixtures/client/packet-context-2026-10-28.json")).unwrap();
        let cases = data["record_group_cases"].as_array().unwrap();
        assert_eq!(cases.len(), 31);
        let mut groups = BTreeMap::new();
        let mut sequence = "";
        for (index, case) in cases.iter().enumerate() {
            let next = case["sequence"].as_str().unwrap();
            if sequence != next { groups.clear(); sequence = next; }
            let key = &case["supplied_key"];
            let key = GroupKey { refCellAt28: key[0].as_u64().unwrap(), refCellAt30: key[1].as_u64().unwrap(),
                byteAt18: key[2].as_u64().unwrap() as u8, opaqueIdentityAt38: key[3].as_u64().unwrap() };
            let group = groups.entry(key).or_insert_with(RadialGroup::default);
            assert_eq!(group.accumulator.to_bits(), (case["accumulator_before"].as_f64().unwrap() as f32).to_bits(), "{case}");
            group.visit(case["input_seed"].as_f64().unwrap() as f32,
                RecordContext { part74: case["input_part"].as_u64().unwrap() as u8, ..Default::default() });
            assert_eq!(group.accumulator.to_bits(), (case["accumulator_after"].as_f64().unwrap() as f32).to_bits(), "{case}");
            assert_eq!(u64::from(group.part55), case["last_visited_part"].as_u64().unwrap());
            let transfer = &data["radial_transfer_cases"][index];
            assert_eq!(group.prepare(0.5, 0.0).to_bits(), (transfer["prepared_before_other_modifiers"].as_f64().unwrap() as f32).to_bits(), "{transfer}");
            assert_eq!(groups.len() as u64, case["group_count"].as_u64().unwrap());
        }
    }

    #[test]
    #[wfsim_gas_macro::metered]
    fn payload_bits_and_own_element_follow_the_last_visited_record() {
        let mut group = RadialGroup::default();
        group.visit(6908.0, RecordContext { payload40: 3, payload70: 4, payload60: 5, part74: 1 });
        group.visit(0.0, RecordContext { payload40: 7, payload70: 8, payload60: 9, part74: 0 });
        assert_eq!((group.payload4c, group.payload50, group.payload54, group.part55), (7, 8, 9, 0));
        assert_eq!(group.prepare(0.5, 1.65).to_bits(), 9154.426_f32.to_bits());
    }
}
