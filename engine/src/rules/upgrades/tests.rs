use super::*;

#[test]
#[wfsim_gas_macro::metered]
fn all_original_instruction_operation_vectors_match_bits() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/client/upgrade-operations-2026-10-28.json")).unwrap();
    let vectors = fixture["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 1115);
    for (index, case) in vectors.iter().enumerate() {
        let mut accumulator = Accumulator::default();
        for pair in case["contributions"].as_array().unwrap() {
            accumulator.push(serde_json::from_value(pair[0].clone()).unwrap(), pair[1].as_f64().unwrap() as f32);
        }
        let value = accumulator.apply(case["base"].as_f64().unwrap() as f32, case["rounded"].as_bool().unwrap());
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
