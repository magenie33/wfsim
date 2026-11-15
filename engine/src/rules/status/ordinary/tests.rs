use super::*;
use crate::rules::upgrades::OperationType;
use serde_json::Value;
use std::cell::Cell;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../../../../tests/fixtures/client/ordinary-proc-2026-10-28.json")).unwrap()
}

fn components(value: &Value) -> DamageComponents {
    let mut components = [0.0; 20];
    for (key, value) in value.as_object().unwrap() {
        components[key.parse::<usize>().unwrap()] = value.as_f64().unwrap() as f32;
    }
    components
}

fn graph(value: &Value) -> [Vec<ProcEvaluator>; 30] {
    let mut graph = std::array::from_fn(|_| Vec::new());
    for (key, evaluators) in value.as_object().unwrap() {
        graph[key.parse::<usize>().unwrap()] = evaluators.as_array().unwrap().iter().map(|e| ProcEvaluator {
            DamageTypes: e[0].as_u64().unwrap() as u32, WeightMultiplier: e[1].as_f64().unwrap() as f32,
        }).collect();
    }
    graph
}

fn seed(value: &Value) -> u64 { u64::from_str_radix(value.as_str().unwrap().trim_start_matches("0x"), 16).unwrap() }
fn counters(value: &Value) -> ProcCounters { ProcCounters::from_bytes(serde_json::from_value(value.clone()).unwrap()) }

fn target_entries(value: &Value) -> Vec<UpgradeEntry> {
    value.as_array().into_iter().flatten().map(|e| {
        assert_eq!(e[2], 0, "the supplied entries are unrestricted");
        UpgradeEntry { UpgradeType: UpgradeType::AVATAR_PROC_CHANCE_VULNERABILITY,
            OperationType: [OperationType::ADD, OperationType::ADD_BASE, OperationType::MULTIPLY,
                OperationType::STACKING_MULTIPLY, OperationType::SET][e[0].as_u64().unwrap() as usize],
            Value: e[1].as_f64().unwrap() as f32 }
    }).collect()
}

#[test]
fn original_client_weights_and_selector_predicates() {
    let fixture = fixture();
    let vectors = fixture["weight_vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 9);
    for vector in vectors {
        let components = components(&vector["components"]);
        let graph = graph(&vector["projected_evaluators"]);
        for index in 0..30 {
            assert_eq!(handler_weight(&graph[index], &components).to_bits(),
                (vector["native_weights"][index].as_f64().unwrap() as f32).to_bits(),
                "{} proc {index}", vector["case"]);
        }
    }
    let mut predicates = 0;
    for vector in fixture["selector_vectors"].as_array().unwrap() {
        let evaluator = ProcEvaluator { DamageTypes: 1 << vector["selector_bit"].as_u64().unwrap(), WeightMultiplier: 1.0 };
        let selected: Vec<usize> = serde_json::from_value(vector["native_selected_component_indices"].clone()).unwrap();
        for index in 0..20 {
            assert_eq!(evaluator.selects(index), selected.contains(&index));
            predicates += 1;
        }
    }
    assert_eq!(predicates, 60);
    assert_eq!(fixture["enum_values"]["values"], serde_json::json!((0..26).collect::<Vec<_>>()));
}

#[test]
fn original_client_ordinary_counters_rng_laziness_and_target117() {
    let fixture = fixture();
    let vectors = fixture["sampling_vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 1895);
    let (mut late, mut complete) = (0, 0);
    for (number, vector) in vectors.iter().enumerate() {
        let label = format!("{number}: {}", vector["case"]);
        let components = components(&fixture["components"][vector["components"].as_u64().unwrap() as usize]);
        let graph = graph(&fixture["graphs"][vector["graph"].as_u64().unwrap() as usize]);
        let handlers = std::array::from_fn(|index| graph[index].as_slice());
        let input_chance = vector["input_chance"].as_f64().unwrap() as f32;
        let chance = if vector["entry_rva"] == "0x151e995" {
            late += 1;
            target_proc_chance(input_chance, target_entries(&vector["target117_entries"]))
        } else { input_chance };
        assert_eq!(chance.to_bits(), (vector["effective_chance"].as_f64().unwrap() as f32).to_bits(), "{label}");
        let skip = vector["packet_flag_0x241"].as_u64().unwrap() as u8;
        let initial = counters(&vector["initial_proc_counters"]);
        let expected = counters(&vector["native_proc_counters"]);
        let mut actual = initial;
        let mut rng = NativeRng::new(seed(&vector["initial_rng"]));
        generate(chance, skip, &components, &handlers, &mut actual, &mut rng);
        assert_eq!(actual, expected, "counters {label}");
        assert_eq!(rng.state(), seed(&vector["final_rng"]), "rng {label}");

        let mut traced = initial;
        let mut traced_rng = NativeRng::new(seed(&vector["initial_rng"]));
        let mut draws = Vec::new();
        let mut selected = Vec::new();
        let evaluations = Cell::new(0);
        let total = Cell::new(0.0_f32);
        sample(chance, skip, || {
            evaluations.set(graph.iter().filter(|evaluators| !evaluators.is_empty()).count());
            let weights: [f32; 30] = std::array::from_fn(|index| handler_weight(handlers[index], &components));
            total.set(weights.iter().fold(0.0_f32, |sum, weight| sum + weight));
            weights
        }, || {
            let u = traced_rng.unit();
            draws.push((u, traced_rng.state()));
            u
        }, |index| {
            if let Some(index) = index { traced.add(ProcType::new(index as u8).unwrap(), 1); }
            selected.push(index);
        });
        assert_eq!(traced, expected, "traced {label}");
        assert_eq!(evaluations.get() as u64, vector["native_weight_evaluations"].as_u64().unwrap(), "lazy weights {label}");
        let expected_draws = vector["draws"].as_array().unwrap();
        assert_eq!(draws.len(), expected_draws.len(), "draw count {label}");
        for (index, ((u, state), expected)) in draws.iter().zip(expected_draws).enumerate() {
            assert_eq!(u.to_bits(), (expected["u"].as_f64().unwrap() as f32).to_bits(), "U24 {label}/{index}");
            assert_eq!(*state, seed(&expected["state"]), "state write {label}/{index}");
            assert_eq!(expected["purpose"], if index % 2 == 0 { "chance" } else { "weighted-type" });
            if let Some(threshold) = expected["threshold"].as_f64() {
                assert_eq!((u * total.get()).to_bits(), (threshold as f32).to_bits(), "threshold {label}/{index}");
            }
        }
        assert_eq!(serde_json::json!(selected), vector["selected_types"], "selected {label}");
        if vector["complete_310c60_return"].as_bool().unwrap() { complete += 1; }
    }
    assert_eq!((late, complete), (9, 4));
}

#[test]
fn real_fight_sampler_matches_supplied_default13_native_vectors() {
    use crate::rules::damage::DamageVector;
    let fixture = fixture();
    let mut checked = 0;
    for vector in fixture["sampling_vectors"].as_array().unwrap() {
        let graph = &fixture["graphs"][vector["graph"].as_u64().unwrap() as usize];
        if *graph != fixture["graphs"][0] || vector["packet_flag_0x241"] != 0 { continue; }
        let initial = counters(&vector["initial_proc_counters"]);
        if initial.bytes()[13..].iter().any(|&count| count != 0) { continue; }
        let components = components(&fixture["components"][vector["components"].as_u64().unwrap() as usize]);
        let mut damage = DamageVector::new();
        for (index, amount) in components.iter().copied().enumerate().take(13) {
            damage = damage.with(ProcType::new(index as u8).unwrap().damage_type().unwrap(), amount as f64);
        }
        let mut rng = NativeRng::new(seed(&vector["initial_rng"]));
        let raw = super::super::procs_for_hit(&initial.types().collect::<Vec<_>>(),
            vector["effective_chance"].as_f64().unwrap(), &damage, super::super::SceneProcHandlers::all(), &mut rng);
        assert_eq!(ProcCounters::from_types(raw), counters(&vector["native_proc_counters"]), "{}", vector["case"]);
        assert_eq!(rng.state(), seed(&vector["final_rng"]), "{}", vector["case"]);
        checked += 1;
    }
    assert_eq!(checked, 645);
}
