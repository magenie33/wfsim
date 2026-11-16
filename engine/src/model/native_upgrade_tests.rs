use super::*;
use crate::rules::rng::NativeRng;
use crate::rules::upgrades::{Accumulator, OperationType};

#[wfsim_gas_macro::metered]
fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/client/status-arcanes-2026-10-28.json")).unwrap()
}

#[wfsim_gas_macro::metered]
fn rank_fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/client/upgrade-ranks-2026-10-28.json")).unwrap()
}

#[wfsim_gas_macro::metered]
fn bits(value: f32) -> String { format!("{:08x}", value.to_bits().swap_bytes()) }

#[test]
#[wfsim_gas_macro::metered]
fn all_original_rank_projections_match_their_source_tokens_and_bits() {
    let data = rank_fixture();
    let entries = data["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 4629);
    let mut count = 0;
    for entry in entries {
        let spec: RankedUpgrade = serde_json::from_value(entry["spec"].clone()).unwrap();
        let original: f32 = entry["raw_value"].as_str().unwrap().parse().unwrap();
        assert_eq!(spec.entry.Value.to_bits(), original.to_bits());
        let max_rank = entry["max_rank"].as_u64().unwrap() as u32;
        let offset = entry["offset"].as_u64().unwrap() as u32;
        for (rank, expected) in entry["bits"].as_array().unwrap().iter().enumerate() {
            let value = spec.unlocked_value(rank as u32 + offset, max_rank + offset, offset).unwrap();
            assert_eq!(bits(value), expected.as_str().unwrap(), "{} {} entry{} rank{}", entry["owner"], entry["array"], entry["index"], rank);
            count += 1;
        }
        for locked in 0..offset { assert!(spec.unlocked_value(locked, max_rank + offset, offset).is_none()); }
    }
    assert_eq!(count, 20479);
    assert_eq!(data["unprojected"].as_array().unwrap().len(), 99);
}

#[test]
#[wfsim_gas_macro::metered]
fn all_native_conditional_chance_duration_and_byte_stack_inputs_match() {
    let data = rank_fixture();
    let entries = data["conditional"].as_array().unwrap();
    assert_eq!(entries.len(), 1324);
    let mut count = 0;
    for entry in entries {
        let spec: ConditionalParameters = serde_json::from_value(entry["inputs"].clone()).unwrap();
        for (rank, expected) in entry["ranks"].as_array().unwrap().iter().enumerate() {
            let value = spec.at(rank as u32);
            assert_eq!(bits(value.chance), expected[0].as_str().unwrap(), "{} chance rank{rank}", entry["owner"]);
            assert_eq!(bits(value.duration_seconds), expected[1].as_str().unwrap(), "{} duration rank{rank}", entry["owner"]);
            assert_eq!(u64::from(value.max_stacks), expected[2].as_u64().unwrap(), "{} stacks rank{rank}", entry["owner"]);
            count += 1;
        }
    }
    assert_eq!(count, 5398);
}

#[test]
#[wfsim_gas_macro::metered]
fn all_native_extra_unlock_cases_preserve_unavailable_values() {
    let data = rank_fixture();
    let cases = data["unlocks"].as_array().unwrap();
    assert_eq!(cases.len(), 697);
    let mut maxima = std::collections::BTreeMap::new();
    for case in cases {
        let key = (case["path"].as_str().unwrap(), case["entry"].as_u64().unwrap());
        let rank = case["rank"].as_u64().unwrap() as u32;
        maxima.entry(key).and_modify(|value: &mut u32| *value = (*value).max(rank)).or_insert(rank);
    }
    let mut inactive = 0;
    let spec: RankedUpgrade = serde_json::from_str(r#"{"UpgradeType":253,"OperationType":"STACKING_MULTIPLY","Value":0.15,"FusionPowerStep":0,"FusionPowerCurve":"QC_LINEAR"}"#).unwrap();
    for case in cases {
        let key = (case["path"].as_str().unwrap(), case["entry"].as_u64().unwrap());
        let value = spec.unlocked_value(case["rank"].as_u64().unwrap() as u32, maxima[&key], case["unlock"].as_u64().unwrap() as u32);
        assert_eq!(value.is_some(), case["active"].as_bool().unwrap(), "{case}");
        inactive += usize::from(value.is_none());
    }
    assert_eq!(inactive, 265);
}

#[wfsim_gas_macro::metered]
fn seed(value: &serde_json::Value) -> u64 {
    u64::from_str_radix(value.as_str().unwrap().trim_start_matches("0x"), 16).unwrap()
}

#[wfsim_gas_macro::metered]
fn probability(state: &ConditionalState, chance: f32) -> f32 {
    let mut accumulator = Accumulator::default();
    accumulator.push(OperationType::ADD, chance);
    if state.active { accumulator.push(OperationType::SET, 0.0); }
    accumulator.apply(0.0, false)
}

#[test]
#[wfsim_gas_macro::metered]
fn native_sampler_matches_original_packet_counter_and_random_state() {
    let data = fixture();
    let cases = data["contexts"]["results"]["secondary"]["sampler_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 90);
    for case in cases {
        let mut counters = ProcCounters::default();
        for kind in 0..13 { counters.add(ProcType::new(kind).unwrap(), 1); }
        let mut random = NativeRng::new(seed(&case["seed"]));
        counters.encumber(case["probability"].as_f64().unwrap() as f32, 0x1fff, &mut random);
        assert_eq!(random.state(), seed(&case["final_seed"]), "{case}");
        for kind in 0..13 {
            let added = case["chosen_types"].as_array().unwrap().iter().filter(|t| t.as_u64() == Some(kind as u64)).count();
            assert_eq!(counters.count(ProcType::new(kind).unwrap()), 1 + added as u8, "{case}");
        }
    }
}

#[test]
#[wfsim_gas_macro::metered]
fn native_primary_constructor_and_scalar_copy_match_bounded_stages() {
    let data = fixture();
    let cases = data["contexts"]["results"]["primary_context_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 9);
    for case in cases {
        let mut incoming = DamageData::new(case["source_scalar"].as_f64().unwrap() as f32);
        incoming.part = HitPart(case["incoming_part"].as_u64().unwrap() as u8);
        incoming.position = [1.0, 2.0, 3.0, 1.0];
        incoming.selected_proxy = Some(2);
        let copied = incoming.derived_packet();
        assert_eq!(copied.scalar.to_bits(), (case["copied_scalar"].as_f64().unwrap() as f32).to_bits());
        assert_eq!(copied.part.0 as u64, case["new_part_before_resolution"].as_u64().unwrap());
        for (actual, expected) in copied.position.into_iter().zip(case["new_position"].as_array().unwrap()) {
            assert_eq!(actual.to_bits(), (expected.as_f64().unwrap() as f32).to_bits());
        }
        assert_eq!(copied.selected_proxy.map_or(-1, i64::from), case["selected_proxy_index"].as_i64().unwrap());
        assert_eq!(case["resolved_part_no_external_proxy_writer"], 0);
        assert_eq!(case["application_record_part"], 0);
    }
}

#[test]
#[wfsim_gas_macro::metered]
fn native_registration_query_and_update_match_all_sequence_packets() {
    let data = fixture();
    let sequences = data["lifecycle"]["results"]["sequences"].as_array().unwrap();
    assert_eq!(sequences.len(), 19);
    for sequence in sequences {
        let chance = f32::from_bits(u32::from_le_bytes([0x8e, 0xc2, 0x75, 0x3e]));
        let mut state = ConditionalState::default();
        let mut random = NativeRng::new(seed(&sequence["initial_seed"]));
        let mut successes = 0;
        for packet in sequence["packets"].as_array().unwrap() {
            let queried = probability(&state, chance);
            assert_eq!(queried.to_bits(), (packet["queried_chance"].as_f64().unwrap() as f32).to_bits());
            assert_eq!(random.state(), seed(&packet["before_seed"]));
            let mut counters = ProcCounters::default();
            counters.add(ProcType::new(10).unwrap(), sequence["incoming_Magnetic_count_per_packet"].as_u64().unwrap() as u8);
            if counters.encumber(queried, 0x1fff, &mut random) {
                assert!(state.activate(ConditionalUpgrade { CanReproc: false, ..ConditionalUpgrade::default() }));
                successes += 1;
            }
            assert_eq!(random.state(), seed(&packet["after_seed"]));
            assert_eq!(probability(&state, chance).to_bits(), (packet["chance_before_optional_update"].as_f64().unwrap() as f32).to_bits());
            if let Some(dt) = sequence["update_dt_after_each_packet"].as_f64() { state.update(dt as f32); }
            assert_eq!(probability(&state, chance).to_bits(), (packet["chance_after_optional_update"].as_f64().unwrap() as f32).to_bits());
            for kind in 0..13 {
                assert_eq!(u64::from(counters.count(ProcType::new(kind).unwrap())), packet["proc_counters"][kind as usize].as_u64().unwrap());
            }
        }
        assert_eq!(successes, sequence["successful_selections"].as_u64().unwrap());
    }
}
