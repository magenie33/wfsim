use super::*;

#[test]
#[wfsim_gas_macro::metered]
fn all_original_instruction_operation_vectors_match_bits() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/client/upgrade-operations-2026-10-28.json")).unwrap();
    let vectors = fixture["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 1115);
    for (index, case) in vectors.iter().enumerate() {
        let mut scalar = NativeScalar::new(case["base"].as_f64().unwrap() as f32);
        for pair in case["contributions"].as_array().unwrap() {
            scalar.push(serde_json::from_value(pair[0].clone()).unwrap(), pair[1].as_f64().unwrap() as f32);
        }
        let value = scalar.value(case["rounded"].as_bool().unwrap());
        let bits: String = value.to_le_bytes().iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(bits, case["result_bits_le"].as_str().unwrap(), "native vector {index}: {case}");
    }
    assert_eq!(fixture["upgrades"]["140"], "AVATAR_SHIELD_MAX");
    assert_eq!(UpgradeType::AVATAR_SHIELD_MAX.0, 140);
    assert_eq!(fixture["upgrades"]["369"], "WEAPON_PROC_ON_PROC_CHANCE");
    assert_eq!(UpgradeType::WEAPON_PROC_ON_PROC_CHANCE.0, 369);
}

#[test]
#[wfsim_gas_macro::metered]
fn mutating_source_materialization_and_returned_queries_keep_distinct_state() {
    let entry = UpgradeEntry { UpgradeType: UpgradeType::WEAPON_PROC_DAMAGE,
        OperationType: OperationType::STACKING_MULTIPLY, Value: 0.5 };
    let mut parent = NativeScalar::new(100.0);
    parent.install_selected(entry.UpgradeType, [entry], ScalarMutation::Materialize);
    assert_eq!(parent.base(), 150.0);
    assert!(!parent.has_operations());
    let child = parent;
    assert_eq!(child.query_selected(entry.UpgradeType, [entry], false), 225.0);
    assert_eq!(child.base(), 150.0);
    assert!(!child.has_operations());
    let mut pending = NativeScalar::new(100.0);
    pending.install_selected(entry.UpgradeType, [entry], ScalarMutation::RetainOperations);
    assert_eq!(pending.base(), 100.0);
    assert!(pending.has_operations());
    assert_eq!(pending.query_selected(entry.UpgradeType, [entry], false), 150.0);
    assert_eq!(pending.value(false), 150.0);
    pending.push(OperationType::SET, 0.0);
    assert_eq!(pending.value(false), 0.0);
}

#[test]
#[wfsim_gas_macro::metered]
fn neutral_contributions_do_not_install_operation_state_but_set_zero_does() {
    let mut scalar = NativeScalar::new(100.0);
    scalar.push(OperationType::ADD, -0.0);
    scalar.push(OperationType::ADD_BASE, 0.0);
    scalar.push(OperationType::STACKING_MULTIPLY, 0.0);
    scalar.push(OperationType::MULTIPLY, 1.0);
    assert!(!scalar.has_operations());
    scalar.push(OperationType::SET, 0.0);
    assert!(scalar.has_operations());
}

#[test]
#[wfsim_gas_macro::metered]
fn all_original_parent_child_proc_queries_preserve_raw_and_materialized_inputs() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/client/packet-context-2026-10-28.json")).unwrap();
    let cases = fixture["two_stage_upgrade_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 54);
    for case in cases {
        let entry = ProcFilteredUpgrade {
            entry: UpgradeEntry { UpgradeType: UpgradeType::WEAPON_PROC_DAMAGE,
                OperationType: OperationType::STACKING_MULTIPLY,
                Value: case["supplied_stacking_bonus"].as_f64().unwrap() as f32 },
            proc_mask: case["supplied_valid_proc_mask"].as_u64().unwrap() as u32,
        };
        let parent_proc = crate::rules::status::ProcType::new(case["parent_proc_type"].as_u64().unwrap() as u8).unwrap();
        let child_proc = crate::rules::status::ProcType::new(case["child_proc_type"].as_u64().unwrap() as u8).unwrap();
        let mutation = if case["supplied_canonicalization_flag"] == 0 {
            ScalarMutation::Materialize
        } else { ScalarMutation::RetainOperations };
        let mut scalar = NativeScalar::new(case["input_parent_source"].as_f64().unwrap() as f32);
        scalar.install_selected(entry.entry.UpgradeType, entry.for_proc(parent_proc), mutation);
        assert_eq!(scalar.value(false).to_bits(), (case["parent_evaluated_source"].as_f64().unwrap() as f32).to_bits(), "{case}");
        assert_eq!(scalar.base().to_bits(), (case["ordinary_proc_retained_input"].as_f64().unwrap() as f32).to_bits(), "{case}");
        let child = scalar.query_selected(entry.entry.UpgradeType, entry.for_proc(child_proc), false);
        assert_eq!(child.to_bits(), (case["ordinary_proc_stored_seed"].as_f64().unwrap() as f32).to_bits(), "{case}");
    }
}

#[test]
#[wfsim_gas_macro::metered]
fn source_fields_and_stat_selection_keep_native_identity() {
    let entry: UpgradeEntry = serde_json::from_str(r#"{"UpgradeType":140,"OperationType":"MULTIPLY","Value":1.58114}"#).unwrap();
    assert_eq!(entry.UpgradeType, UpgradeType::AVATAR_SHIELD_MAX);
    assert_eq!(query(UpgradeType::AVATAR_SHIELD_MAX, 120.0, [entry], true), 190.0);
    assert_eq!(query(UpgradeType::AVATAR_OVERGUARD_MAX, 120.0, [entry], true), 120.0);
    let encoded = serde_json::to_value(entry).unwrap();
    assert!(encoded.get("OperationType").is_some());
    assert!(encoded.get("operation_type").is_none());
}

#[test]
#[wfsim_gas_macro::metered]
fn integer_conversion_preserves_native_sentinel_and_negative_floor() {
    assert_eq!(round_integer(-1.6), -2.0);
    assert_eq!(round_integer(-1.5), -1.0);
    assert_eq!(round_integer(2147483648.0), 2147483648.0);
    assert_eq!(round_integer(f32::INFINITY), f32::INFINITY);
    assert!(round_integer(f32::NAN).is_nan());
}
