// SPDX-License-Identifier: AGPL-3.0-or-later
use super::super::guided::Guided;
use super::super::rotation::*;
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../fixtures/native-steering.json")).unwrap()
}

fn floats<const N: usize>(value: &Value) -> [f32; N] {
    std::array::from_fn(|i| f32::from_bits(value[i].as_u64().unwrap() as u32))
}

fn scalar(value: &Value) -> f32 {
    f32::from_bits(value.as_u64().unwrap() as u32)
}

fn bits<const N: usize>(actual: [f32; N], expected: &Value, case: usize) {
    assert_eq!(
        actual.map(f32::to_bits),
        floats::<N>(expected).map(f32::to_bits),
        "case {case}"
    );
}

#[test]
fn native_angular_smoothing_and_wrapping_are_bit_exact() {
    let data = fixture();
    let cases = data["steering"].as_array().unwrap();
    assert_eq!(data["original_controls"].as_u64().unwrap(), 429);
    assert_eq!(
        cases.len(),
        data["original_controls"].as_u64().unwrap() as usize + 216 + 120
    );
    for (index, case) in cases.iter().enumerate() {
        let mut velocity = floats(&case["velocity"]);
        let angles = steer_euler(
            floats(&case["current"]),
            (!case["desired"].is_null()).then(|| floats(&case["desired"])),
            &mut velocity,
            scalar(&case["turning"]),
            case["has_target"].as_bool().unwrap(),
            scalar(&case["dt"]),
        );
        bits(angles, &case["wrapped"], index);
        bits(velocity, &case["next_velocity"], index);
    }
}

fn flight_state(value: &Value) -> Guided {
    Guided {
        position: floats(&value["position"]),
        velocity: floats(&value["velocity"]),
        orientation: floats(&value["orientation"]),
        angular_velocity: floats(&value["angular_velocity"]),
        bank: scalar(&value["bank"]),
        flight: scalar(&value["flight"]),
    }
}

fn step_flight(state: &mut Guided, case: &Value) {
    let before = &case["before"];
    let target = before["retained"]
        .as_bool()
        .unwrap()
        .then(|| floats(&before["target"]));
    state.step_with_world(
        target,
        scalar(&case["seconds"]),
        scalar(&before["turning"]),
        scalar(&before["acceleration"]),
        scalar(&before["maximum"]),
        floats(&before["world_orientation"]),
    );
}

fn check_flight(state: &Guided, expected: &Value, index: usize) {
    bits(state.position, &expected["position"], index);
    bits(state.velocity, &expected["velocity"], index);
    bits(state.orientation, &expected["orientation"], index);
    bits(state.angular_velocity, &expected["angular_velocity"], index);
    bits([state.bank], &serde_json::json!([expected["bank"]]), index);
    bits(
        [state.flight],
        &serde_json::json!([expected["flight"]]),
        index,
    );
}

#[test]
fn complete_native_flight_steps_and_sequences_are_bit_exact() {
    let input: Value =
        serde_json::from_str(include_str!("../fixtures/native-flight.json")).unwrap();
    let cases = input["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 429);
    let mut sequence = None;
    let mut continuous = flight_state(&cases[0]["before"]);
    for (index, case) in cases.iter().enumerate() {
        let mut independent = flight_state(&case["before"]);
        step_flight(&mut independent, case);
        check_flight(&independent, &case["after"], index);
        if sequence.as_ref() != Some(&case["sequence"]) {
            continuous = flight_state(&case["before"]);
            sequence = Some(case["sequence"].clone());
        }
        step_flight(&mut continuous, case);
        check_flight(&continuous, &case["after"], index);
    }
    let controls = fixture();
    let cases = controls["physical_controls"].as_array().unwrap();
    assert_eq!(cases.len(), 120);
    for (index, case) in cases.iter().enumerate() {
        let mut state = flight_state(&case["before"]);
        step_flight(&mut state, case);
        check_flight(&state, &case["after"], index + 429);
    }
}

#[test]
fn native_rotation_polynomials_and_transforms_are_bit_exact() {
    let data = fixture();
    let mut checked = 0;
    for (index, case) in data["rotation"].as_array().unwrap().iter().enumerate() {
        let input = &case["inputs"];
        let output = &case["outputs"];
        match case["kind"].as_str().unwrap() {
            "wrap" => bits(canonical_euler(floats(&input[0])), &output[0], index),
            "sin_cos" => {
                let pairs = floats::<4>(&input[0]).map(sin_cos);
                bits(pairs.map(|p| p.0), &output[0], index);
                bits(pairs.map(|p| p.1), &output[1], index);
            }
            "euler_to_quaternion" => {
                bits(euler_to_quaternion(floats(&input[0])), &output[0], index)
            }
            "quaternion_direction" => {
                bits(quaternion_direction(floats(&input[0])), &output[0], index)
            }
            "euler_to_basis" => bits(euler_to_basis(floats(&input[0])), &output[0], index),
            "quaternion_to_euler" => {
                bits(quaternion_to_euler(floats(&input[0])), &output[0], index)
            }
            "basis_to_euler" => bits(basis_to_euler(floats(&input[0])), &output[0], index),
            "asin" => bits([asin(floats::<1>(&input[0])[0])], &output[0], index),
            "atan2" => {
                let [y, x] = floats(&input[0]);
                bits([atan2(y, x)], &output[0], index);
            }
            "look_at" => bits(
                look_at(floats(&input[0]), floats(&input[1]), floats(&input[2])),
                &output[0],
                index,
            ),
            "reciprocal_sqrt" => bits(
                [reciprocal_sqrt(floats::<1>(&input[0])[0])],
                &output[0],
                index,
            ),
            other => panic!("unbound native rotation stage {other}"),
        }
        checked += 1;
    }
    assert_eq!(checked, 4931);
}

#[test]
fn nonfinite_angles_do_not_index_the_finite_atan_table() {
    assert!(atan2(f32::NAN, 1.).is_nan());
    assert!(atan2(1., f32::NAN).is_nan());
    assert_eq!(atan2(f32::INFINITY, f32::INFINITY).to_bits(), 0x3f490fdb);
    assert_eq!(
        atan2(f32::NEG_INFINITY, f32::NEG_INFINITY).to_bits(),
        0xc016cbe4
    );
}
