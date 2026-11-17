// SPDX-License-Identifier: AGPL-3.0-or-later
//! Continuous-rate consumer bindings, separate from projectile emission.

#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(untagged)]
pub enum SustainedFireRate {
    Native(NativeContinuousRate),
    Assumed { start: f64, end: f64, over_shots: f64 },
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeContinuousRate {
    pub native_consumer: ContinuousConsumer,
    pub ContinuousFireRateAtten: [f32; 2],
    pub ContinuousMaxShots: u32,
}

/// Explicit engineering route tag; it is not a recovered client member.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuousConsumer { AutomaticZeroDurationRefire }

#[wfsim_gas_macro::metered]
impl NativeContinuousRate {
    /// Slot +0x200, then rate consumer RVA 0x83beb0 (continuous-rate fixture).
    pub fn rate(self, modified_rps: f32, counter: u32) -> f32 {
        let progress = counter.min(self.ContinuousMaxShots) as f32 / self.ContinuousMaxShots.max(1) as f32;
        let [start, end] = self.ContinuousFireRateAtten;
        let span = end - start;
        let ramp = span * progress;
        modified_rps * (start + ramp)
    }

    pub fn entry_counter(self, prior_cycles: f64) -> u32 {
        match self.native_consumer {
            ContinuousConsumer::AutomaticZeroDurationRefire => (prior_cycles as u32).saturating_mul(2).min(self.ContinuousMaxShots),
        }
    }

    /// The two FIRE updates precede REFIRE_WAIT's rate query.
    pub fn refire_counter(self, prior_cycles: f64) -> u32 { self.entry_counter(prior_cycles).saturating_add(2).min(self.ContinuousMaxShots) }
}

#[wfsim_gas_macro::metered]
impl SustainedFireRate {
    pub fn entry_factor(self, prior_cycles: f64) -> f64 {
        match self {
            Self::Native(s) => s.rate(1.0, s.entry_counter(prior_cycles)) as f64,
            Self::Assumed { start, end, over_shots } if over_shots > 0.0 => start + (end - start) * (prior_cycles / over_shots).min(1.0),
            _ => 1.0,
        }
    }

    pub fn refire_rate(self, modified_rps: f64, prior_cycles: f64) -> f64 {
        match self {
            Self::Native(s) => s.rate(modified_rps as f32, s.refire_counter(prior_cycles)) as f64,
            _ => modified_rps * self.entry_factor(prior_cycles),
        }
    }

    pub fn display_span(self) -> (f64, f64, f64) {
        match self {
            Self::Native(s) => (s.ContinuousFireRateAtten[0] as f64, s.ContinuousFireRateAtten[1] as f64, s.ContinuousMaxShots as f64 / 2.0),
            Self::Assumed { start, end, over_shots } => (start, end, over_shots),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[wfsim_gas_macro::metered]
    fn native_rate_and_refire_sampling_match_original_instruction_replays() {
        let data: serde_json::Value = serde_json::from_str(include_str!("../../../../tests/fixtures/client/continuous-rate-2026-10-28.json")).unwrap();
        let native = NativeContinuousRate { native_consumer: ContinuousConsumer::AutomaticZeroDurationRefire,
            ContinuousFireRateAtten: [0.3, 1.0], ContinuousMaxShots: 12 };
        assert_eq!(data["rate_vectors"].as_array().unwrap().len(), 84);
        for case in data["rate_vectors"].as_array().unwrap() {
            let rps = case["supplied_modified_rpm"].as_f64().unwrap() as f32 * (1.0_f32 / 60.0_f32);
            let rate = native.rate(rps, case["count"].as_u64().unwrap() as u32);
            assert_eq!(rate.to_bits(), (case["rate_per_second_float32"].as_f64().unwrap() as f32).to_bits(), "{case}");
            assert_eq!((1.0_f32 / rate).to_bits(), (case["interval_seconds_float32"].as_f64().unwrap() as f32).to_bits(), "{case}");
        }
        for case in data["successive_fire_entries"].as_array().unwrap() {
            let cycles = case["fire_entry_ordinal"].as_u64().unwrap() as f64 - 1.0;
            assert_eq!(native.entry_counter(cycles), case["entry"]["counter"].as_u64().unwrap() as u32);
            let counter = native.refire_counter(cycles);
            assert_eq!(counter, case["second_update"]["counter"].as_u64().unwrap() as u32);
            let rate = native.rate(320.0_f32 * (1.0_f32 / 60.0_f32), counter);
            assert_eq!((1.0_f32 / rate).to_bits(), (case["second_update"]["state_timer"].as_f64().unwrap() as f32).to_bits());
        }
    }
}
