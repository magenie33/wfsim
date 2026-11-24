// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shooting channel for a stationary player holding a fixed reticle.
use crate::model::{Spread, SpreadScaling, SpreadTiming};

#[derive(Default)]
pub(super) struct SpreadState {
    active: Option<*const super::FightParams>,
    frame: u64,
    factor: f32,
    raw: f32,
    smooth: f32,
    velocity: f32,
    delay: f32,
}

#[wfsim_gas_macro::metered]
impl SpreadState {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit(
        &mut self,
        active: *const super::FightParams,
        scaling: SpreadScaling,
        mut spread: Spread,
        timing: SpreadTiming,
        live: f64,
        time: f64,
        frame_seconds: f64,
    ) -> Spread {
        let frame = (time / frame_seconds + 1e-9).floor() as u64;
        let dt = frame_seconds as f32;
        let factor = scaling.factor(live) as f32;
        let min = scaling.base.min_deg as f32 * factor;
        let max = scaling.base.max_deg as f32 * factor;
        let inverse = scaling.runtime.inverse;
        let rest = if inverse { max } else { min };
        if self.active != Some(active) || (self.factor == 0.0 && factor > 0.0) {
            *self = Self {
                active: Some(active),
                frame,
                factor,
                raw: rest,
                smooth: rest,
                ..Self::default()
            };
        } else if self.factor != factor {
            let ratio = factor / self.factor;
            self.raw *= ratio;
            self.smooth *= ratio;
            self.velocity *= ratio;
            self.factor = factor;
        }
        // Client accuracy update 0x137570 feeds the smoothed-spread getter.
        let x = 20.0 * dt;
        let smoothing = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
        while self.frame < frame {
            if (self.raw - rest).abs() <= f32::EPSILON
                && (self.smooth - rest).abs() <= 1e-5
                && self.velocity.abs() <= 1e-5
            {
                self.raw = rest;
                self.smooth = rest;
                self.velocity = 0.0;
                self.delay = (self.delay - (frame - self.frame) as f32 * dt).max(0.0);
                break;
            }
            if self.delay <= 0.0 {
                let recovery = timing.recovery_rate * dt;
                self.raw += (rest - self.raw).clamp(-recovery, recovery);
            }
            self.delay = (self.delay - dt).max(0.0);
            let change = self.smooth - self.raw;
            let temp = (self.velocity + 20.0 * change) * dt;
            self.smooth = self.raw + (change + temp) * smoothing;
            self.velocity = (self.velocity - 20.0 * temp) * smoothing;
            self.frame += 1;
        }
        self.frame = frame;

        // Growth updates the shooting channel, while launch reads the previously
        // smoothed value. All rounds sharing this release use that value.
        spread.min_deg = f64::from(self.smooth.max(0.0)) + scaling.added_spread;
        let rate = timing.rate * factor;
        let delta = if timing.exponential {
            rate * libm::expf(rate * self.raw)
        } else {
            rate
        };
        self.raw = if inverse {
            (self.raw - delta).max(min)
        } else {
            (self.raw + delta).min(max)
        };
        self.delay = timing.recovery_delay;
        spread
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coda_bubonico_disk_matches_the_native_first_sample() {
        let spread = Spread {
            min_deg: 6.999999523162842,
            max_deg: 7.0,
            sampling: Some(crate::model::SpreadSampling {
                horizontal_ratio: 0.5,
                uniform: false,
                hip_min_deg: None,
                hip_max_deg: None,
            }),
        };
        let (angle, phase) = spread.sample_fixed_cone(7499. / 32767., 28739. / 32767., 0, 7);
        let (sin_angle, forward) = angle.to_radians().sin_cos();
        let (vertical, horizontal) = (phase * std::f64::consts::TAU).sin_cos();
        let direction = [sin_angle * horizontal, sin_angle * vertical, forward];
        let native = [
            0.00379172177053988,
            0.028376083821058273,
            0.9995900988578796,
        ];
        for (actual, expected) in direction.into_iter().zip(native) {
            assert!((actual - expected).abs() < 1e-7);
        }
    }

    #[test]
    fn inverse_spread_matches_client_release_sequences_at_two_frame_rates() {
        let timing = SpreadTiming {
            rate: 12.0,
            recovery_rate: 70.0,
            recovery_delay: 0.05,
            exponential: true,
        };
        let base = Spread {
            min_deg: 1.0,
            max_deg: 7.0,
            sampling: None,
        };
        let scaling = SpreadScaling {
            base,
            accuracy_bonus: 0.0,
            added_spread: 0.0,
            runtime: crate::model::SpreadRuntime {
                inverse: true,
                ..Default::default()
            },
        };
        // Native coupled-sequences.json, settled Coda Bubonico primary, default upgrades.
        let at_sixty = [
            (0, 7.0),
            (28, 6.978040),
            (50, 6.878265),
            (67, 6.533396),
            (82, 6.224984),
            (94, 5.448275),
            (106, 5.407576),
            (117, 5.043998),
            (128, 5.013304),
            (139, 5.011970),
            (151, 5.381927),
        ];
        let at_one_twenty = [
            (0, 7.0),
            (56, 6.971181),
            (98, 6.791906),
            (132, 6.401564),
            (162, 6.015096),
            (187, 5.256767),
            (210, 4.820981),
        ];
        for (hz, releases) in [(60., at_sixty.as_slice()), (120., at_one_twenty.as_slice())] {
            let mut state = SpreadState::default();
            for &(frame, expected) in releases {
                let sample = state.emit(
                    std::ptr::null(),
                    scaling,
                    base,
                    timing,
                    0.,
                    f64::from(frame) / hz,
                    1. / hz,
                );
                assert!(
                    (sample.min_deg - expected).abs() < 2e-5,
                    "{hz} Hz frame {frame}: {} != {expected}",
                    sample.min_deg
                );
            }
        }
    }
}
