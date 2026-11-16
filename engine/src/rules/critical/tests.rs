use super::*;

#[wfsim_gas_macro::metered]
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../tests/fixtures/client/critical-2026-10-28.json")).unwrap()
}

#[test]
#[wfsim_gas_macro::metered]
fn original_critical_rolls_preserve_tiers_and_random_state() {
    let data = fixture();
    let cases = data["critical_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 67);
    for case in cases {
        let mut rng = NativeRng::new(case["rng_before"].as_u64().unwrap_or(0));
        let preset = case["preset_tier"].as_u64().unwrap_or(0) as u32;
        let tier = roll(case["chance"].as_f64().unwrap() as f32, preset, &mut rng);
        assert_eq!(u64::from(tier), case["tier"].as_u64().unwrap(), "{case}");
        if let Some(state) = case["rng_after"].as_u64() {
            assert_eq!(rng.state(), state, "{case}");
        }
        let scalar = apply_factor(100.0, factor(tier, case["critical_multiplier"].as_f64().unwrap() as f32));
        assert_eq!(scalar.to_bits(), (case["source_after_critical"].as_f64().unwrap() as f32).to_bits(), "{case}");
    }
}

#[test]
#[wfsim_gas_macro::metered]
fn original_quantizer_and_low_critical_factors_match_every_bit() {
    let data = fixture();
    let cases = data["quantization_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    for case in cases {
        if let Some(input) = case["input_critical_multiplier"].as_f64() {
            assert_eq!(quantize(input as f32).to_bits(), (case["quantized_critical_multiplier"].as_f64().unwrap() as f32).to_bits(), "{case}");
        } else {
            let multiplier = quantize(case["nominal_critical_multiplier_before_quantization"].as_f64().unwrap() as f32);
            assert_eq!(multiplier.to_bits(), (case["critical_multiplier_at_roll"].as_f64().unwrap() as f32).to_bits());
            let scalar = apply_factor(case["source_before_critical"].as_f64().unwrap() as f32,
                factor(case["tier"].as_u64().unwrap() as u32, multiplier));
            assert_eq!(scalar.to_bits(), (case["source_after_critical"].as_f64().unwrap() as f32).to_bits(), "{case}");
        }
    }
}

#[test]
#[wfsim_gas_macro::metered]
fn critical_roll_keeps_byte_conversion_and_inclusive_discrete_boundary() {
    assert_eq!(roll(256.0, 0, &mut NativeRng::new(0)), 0);
    assert_eq!(roll(257.0, 0, &mut NativeRng::new(0)), 1);
    let mut probe = NativeRng::new(0);
    let fraction = probe.unit();
    let mut rng = NativeRng::new(0);
    assert_eq!(roll(fraction, 0, &mut rng), 1);
    assert_eq!(rng.state(), probe.state());
}

#[test]
#[wfsim_gas_macro::metered]
fn selected_and_live_critical_operations_share_the_original_native_fold() {
    let data = fixture();
    let mut checked = 0;
    for case in data["modifier_cases"].as_array().unwrap() {
        if case["stat_id"].as_u64().unwrap() == 253 || case["recipe"] == "explicit-minimum" { continue; }
        let mut relative = Vec::new();
        let mut flat = 0.0;
        for op in case["operations"].as_array().unwrap() {
            match op[0].as_u64().unwrap() {
                3 => relative.push(op[1].as_f64().unwrap()),
                0 => flat = op[1].as_f64().unwrap(),
                _ => panic!("unbound input: {case}"),
            }
        }
        let base = case["base"].as_f64().unwrap();
        let inputs = CriticalInputs::selected(base, base, relative[0], relative[0], 0.0, 0.0);
        let live = relative.get(1).copied().unwrap_or(0.0);
        let actual = if case["stat_id"] == 244 { inputs.chance(live, flat) }
            else { inputs.damage(0.0, live, flat) };
        let expected = case["float_query_result"].as_f64().unwrap();
        assert_eq!(actual, expected, "{case}");
        checked += 1;
    }
    assert_eq!(checked, 14);
}

#[test]
#[wfsim_gas_macro::metered]
fn conditional_base_is_selected_before_static_and_live_damage_operations() {
    let inputs = CriticalInputs::selected(0.32, 5.0, 0.0, 1.0, 0.0, 0.0)
        .with_damage_gate(2.0, 0.4, 1.0, 0.0);
    assert_eq!(inputs.damage(0.32, 1.0, 0.5), 15.5);
    assert_eq!(inputs.damage(0.4, 1.0, 0.5), 6.5);
    assert_eq!(inputs.damage(0.5, 1.0, 0.5), 6.5);
}

#[test]
#[wfsim_gas_macro::metered]
fn supplying_chance_preserves_the_independent_pending_damage_query() {
    let mut inputs = CriticalInputs::selected(0.32, 2.0, 1.5, 1.0, 0.0, 0.0);
    inputs.set_supplied_chance(1.0, 0.0);
    assert_eq!(inputs.chance(100.0, 0.0), 1.0);
    assert_eq!(inputs.damage(1.0, 0.5, 0.25), 5.25);
    inputs.set_supplied_damage(3.0, 2.0, None);
    assert_eq!(inputs.chance(0.0, 0.0), 1.0);
    assert_eq!(inputs.damage(1.0, 0.5, 0.25), 4.25);
}
