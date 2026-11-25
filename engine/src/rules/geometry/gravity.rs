// SPDX-License-Identifier: AGPL-3.0-or-later
//! Client kinematic projectile gravity, bounded CB-04 replay.
#[wfsim_gas_macro::metered]
pub(crate) fn vertical_velocity(before: f32, dt: f32, multiplier: f32, delay: f32, elapsed: f32) -> f32 {
    if elapsed >= delay && before > -50. {
        before + (dt * multiplier) * -9.8_f32
    } else {
        before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_gravity_vectors_match_bit_for_bit() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/native-gravity.json")).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 72);
        for case in cases {
            let value = |key: &str| case[key].as_f64().unwrap() as f32;
            let result = vertical_velocity(value("velocity_y_before"), value("dt"),
                value("gravity_multiplier"), value("delay"), value("elapsed"));
            let expected = case["velocity_after"][1].as_f64().unwrap() as f32;
            assert_eq!(result.to_bits(), expected.to_bits(), "{case}");
        }
    }
}
