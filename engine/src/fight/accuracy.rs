// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shooting channel and continuous beam spread states for a stationary player.
use crate::model::{BeamSpreadMode, Spread, SpreadScaling, SpreadTiming};
use crate::rules::math::Portable;
use crate::rules::rng::Rng;

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
        let frame_seconds = frame_seconds as f32;
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
        let x = 20.0 * frame_seconds;
        let smoothing = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
        while self.frame < frame {
            if (self.raw - rest).abs() <= f32::EPSILON
                && (self.smooth - rest).abs() <= 1e-5
                && self.velocity.abs() <= 1e-5
            {
                self.raw = rest;
                self.smooth = rest;
                self.velocity = 0.0;
                self.delay = (self.delay - (frame - self.frame) as f32 * frame_seconds).max(0.0);
                break;
            }
            if self.delay <= 0.0 {
                let recovery = timing.recovery_rate * frame_seconds;
                self.raw += (rest - self.raw).clamp(-recovery, recovery);
            }
            self.delay = (self.delay - frame_seconds).max(0.0);
            let change = self.smooth - self.raw;
            let temp = (self.velocity + 20.0 * change) * frame_seconds;
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

#[derive(Clone, Copy)]
struct SmoothRay {
    cur: [f32; 3],
    target: [f32; 3],
    vel: [f32; 3],
}

#[wfsim_gas_macro::metered]
impl SmoothRay {
    fn centered() -> Self {
        Self {
            cur: [0.0, 0.0, 1.0],
            target: [0.0, 0.0, 1.0],
            vel: [0.0, 0.0, 0.0],
        }
    }

    fn dist_sq_to_target(self) -> f32 {
        let dx = self.cur[0] - self.target[0];
        let dy = self.cur[1] - self.target[1];
        let dz = self.cur[2] - self.target[2];
        dx * dx + dy * dy + dz * dz
    }

    /// LotusBeamFireBehavior smooth direction step 0x1727ca0.
    fn step_smooth(&mut self, smooth_time: f32, frame_seconds: f32) {
        let omega = 2.0 / smooth_time.max(1e-4);
        let x = omega * frame_seconds;
        let exp = 1.0 / (1.0 + x * (1.0 + x * (0.48 + x * 0.235)));
        for axis in 0..3 {
            let change = self.cur[axis] - self.target[axis];
            let temp = (self.vel[axis] + omega * change) * frame_seconds;
            self.vel[axis] = (self.vel[axis] - omega * temp) * exp;
            self.cur[axis] = self.target[axis] + (change + temp) * exp;
        }
    }
}

#[wfsim_gas_macro::metered]
fn cone_xyz(spread_deg: f32, horizontal_ratio: f32, phase: f32, radial_scale: f32) -> [f32; 3] {
    let half_rad = spread_deg.clamp(0.0, 179.0).to_radians() * 0.5;
    let phi = f64::from(phase) * std::f64::consts::TAU;
    let disk = libm::tanf(half_rad) * radial_scale;
    let x = horizontal_ratio * disk * (phi.cos_portable() as f32);
    let y = (1.0 - horizontal_ratio) * disk * (phi.sin_portable() as f32);
    let len = libm::sqrtf(x * x + y * y + 1.0);
    if len > 1e-12 {
        [x / len, y / len, 1.0 / len]
    } else {
        [0.0, 0.0, 1.0]
    }
}

#[wfsim_gas_macro::metered]
fn quat_rotate_unit(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let [qx, qy, qz, qw] = q;
    let tx = 2.0 * (qy * v[2] - qz * v[1]);
    let ty = 2.0 * (qz * v[0] - qx * v[2]);
    let tz = 2.0 * (qx * v[1] - qy * v[0]);
    let rx = v[0] + qw * tx + (qy * tz - qz * ty);
    let ry = v[1] + qw * ty + (qz * tx - qx * tz);
    let rz = v[2] + qw * tz + (qx * ty - qy * tx);
    let len = libm::sqrtf(rx * rx + ry * ry + rz * rz);
    if len > 1e-12 {
        [rx / len, ry / len, rz / len]
    } else {
        [0.0, 0.0, 1.0]
    }
}

#[wfsim_gas_macro::metered]
fn xyz_to_dev_phi(dir: [f32; 3]) -> (f64, f64) {
    let x = f64::from(dir[0]);
    let y = f64::from(dir[1]);
    let z = f64::from(dir[2]);
    let deviation = x.hypot_portable(y).atan2_portable(z).to_degrees();
    let phase = y.atan2_portable(x).rem_euclid(std::f64::consts::TAU) / std::f64::consts::TAU;
    (deviation, phase)
}

#[derive(Default)]
pub(super) struct BeamSpreadState {
    active: Option<*const super::FightParams>,
    refills: u32,
    rays: Vec<SmoothRay>,
    fan_state: u8,
    fan_progress: f32,
}

#[wfsim_gas_macro::metered]
impl BeamSpreadState {
    pub(super) fn prepare_tick(
        &mut self,
        active: *const super::FightParams,
        refills: u32,
        mode: BeamSpreadMode,
        scaling: Option<SpreadScaling>,
        fallback_deg: f64,
        live_accuracy: f64,
    ) -> f64 {
        if self.active != Some(active) || self.refills != refills {
            self.active = Some(active);
            self.refills = refills;
            self.rays.clear();
            self.fan_state = 0;
            self.fan_progress = 0.0;
        }
        match mode {
            BeamSpreadMode::Fan {
                base_deg,
                atten_max,
                reverse,
                force_loop,
                ..
            } => {
                let raw = scaling.map_or(f64::from(base_deg), |sc| {
                    (f64::from(base_deg) * sc.factor(live_accuracy) + sc.added_spread).max(0.0)
                }) as f32;
                let atten = if !reverse && !force_loop {
                    1.0 + (atten_max - 1.0) * self.fan_progress
                } else {
                    atten_max + (1.0 - atten_max) * self.fan_progress
                };
                f64::from((raw * atten).clamp(0.0, 179.0))
            }
            BeamSpreadMode::ChildQuat { .. } | BeamSpreadMode::Smoothed { .. } => {
                fallback_deg.max(0.0)
            }
        }
    }

    pub(super) fn sample_ray(
        &mut self,
        mode: BeamSpreadMode,
        spread_deg: f64,
        ray: u32,
        frame_seconds: f32,
        aim_rng: &mut Rng,
        dir_rng: &mut Rng,
    ) -> (f64, f64) {
        match mode {
            BeamSpreadMode::Smoothed {
                smooth_time,
                horizontal_ratio,
            } => {
                if spread_deg <= 0.0 {
                    return (0.0, 0.0);
                }
                let idx = ray as usize;
                if self.rays.len() <= idx {
                    self.rays.resize(idx + 1, SmoothRay::centered());
                }
                let slot = &mut self.rays[idx];
                // Native 0xbf187b picks a new cone target when within 0.01 of target.
                if slot.dist_sq_to_target() < 1e-4 {
                    let u_phi = dir_rng.next_f64() as f32;
                    let u_rad = aim_rng.next_f64() as f32;
                    slot.target = cone_xyz(
                        spread_deg as f32,
                        horizontal_ratio,
                        u_phi,
                        libm::sqrtf(u_rad),
                    );
                }
                let norm_len = libm::sqrtf(
                    slot.cur[0] * slot.cur[0]
                        + slot.cur[1] * slot.cur[1]
                        + slot.cur[2] * slot.cur[2],
                );
                let emitted = if norm_len > 1e-12 {
                    [
                        slot.cur[0] / norm_len,
                        slot.cur[1] / norm_len,
                        slot.cur[2] / norm_len,
                    ]
                } else {
                    [0.0, 0.0, 1.0]
                };
                let angles = xyz_to_dev_phi(emitted);
                slot.step_smooth(smooth_time, frame_seconds);
                angles
            }
            BeamSpreadMode::ChildQuat {
                horizontal_ratio,
                quats,
            } => {
                let jitter = if spread_deg <= 0.0 {
                    [0.0, 0.0, 1.0]
                } else {
                    let u_phi = dir_rng.next_f64() as f32;
                    let u_rad = aim_rng.next_f64() as f32;
                    cone_xyz(
                        spread_deg as f32,
                        horizontal_ratio,
                        u_phi,
                        libm::sqrtf(u_rad),
                    )
                };
                let rotated = quat_rotate_unit(quats[(ray as usize) % 4], jitter);
                xyz_to_dev_phi(rotated)
            }
            BeamSpreadMode::Fan {
                base_rays,
                inner_ring,
                horizontal_ratio,
                ..
            } => {
                let base = f32::from(base_rays.max(1));
                let ratio = ray as f32 / base;
                let cone = if ratio >= 1.0 && inner_ring {
                    (spread_deg as f32) * 0.5
                } else {
                    spread_deg as f32
                };
                xyz_to_dev_phi(cone_xyz(cone, horizontal_ratio, ratio, 1.0))
            }
        }
    }

    /// LotusBeamFireBehavior::ExpandCollapse 0xaffd40 progress update.
    pub(super) fn finish_tick(&mut self, mode: BeamSpreadMode, frame_seconds: f32) {
        let BeamSpreadMode::Fan {
            expansion_time,
            reverse,
            force_loop,
            ..
        } = mode
        else {
            return;
        };
        if !force_loop {
            if expansion_time > 0.0 {
                self.fan_progress = (self.fan_progress + frame_seconds / expansion_time).min(1.0);
            } else {
                self.fan_progress = 1.0;
            }
            return;
        }
        if self.fan_state == 0 {
            self.fan_state = if reverse { 2 } else { 3 };
        }
        let step = if expansion_time > 0.0 {
            frame_seconds / expansion_time
        } else {
            1.0
        };
        if self.fan_state == 2 {
            let next = self.fan_progress + step;
            if next < 1.0 {
                self.fan_progress = next;
            } else {
                self.fan_progress = 1.0;
                self.fan_state = 3;
            }
        } else if self.fan_state == 3 {
            let next = self.fan_progress - step;
            if next > 0.0 {
                self.fan_progress = next;
            } else {
                self.fan_progress = 0.0;
                self.fan_state = 2;
            }
        }
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

    #[test]
    fn beam_child_quats_fan_convergence_and_smooth_wandering_match_client_controls() {
        // Phantasma stationary S=0 child-beam quaternions (Unicorn phantasma[0]).
        let phantasma = BeamSpreadMode::ChildQuat {
            horizontal_ratio: 0.7,
            quats: &[
                [0.008_726, 0.013_089, -0.000_114, 0.999_876],
                [0.008_726, -0.013_089, 0.000_114, 0.999_876],
                [-0.008_726, 0.013_089, 0.000_114, 0.999_876],
                [-0.008_726, -0.013_089, -0.000_114, 0.999_876],
            ],
        };
        let mut state = BeamSpreadState::default();
        let mut aim = Rng::new(1);
        let mut dir = Rng::new(2);
        for ray in 0..6 {
            let (dev, _) = state.sample_ray(phantasma, 0.0, ray, 1.0 / 12.0, &mut aim, &mut dir);
            assert!((dev - 1.802_717).abs() < 1e-4, "ray {ray}: {dev}");
        }

        // Phage unmodded 92 -> 2.3 deg over 0.6s (Unicorn fan[0]).
        let phage = BeamSpreadMode::Fan {
            base_deg: 92.0,
            base_rays: 6,
            atten_max: 0.025,
            expansion_time: 0.6,
            reverse: false,
            force_loop: false,
            inner_ring: true,
            horizontal_ratio: 0.6,
        };
        let mut phage_state = BeamSpreadState::default();
        let frame_seconds = 0.05_f32;
        let mut step0_ray0 = 0.0;
        let mut step12_ray0 = 0.0;
        for step in 0..=12 {
            let cone = phage_state.prepare_tick(std::ptr::null(), 0, phage, None, 92.0, 0.0);
            let (dev, _) =
                phage_state.sample_ray(phage, cone, 0, frame_seconds, &mut aim, &mut dir);
            if step == 0 {
                step0_ray0 = dev;
            }
            if step == 12 {
                step12_ray0 = dev;
            }
            phage_state.finish_tick(phage, frame_seconds);
        }
        assert!((step0_ray0 - 31.853_439).abs() < 1e-3, "step 0: {step0_ray0}");
        assert!((step12_ray0 - 0.690_064).abs() < 1e-3, "step 12: {step12_ray0}");

        // Convectrix alt-fire oscillating sweep (Unicorn fan[5]).
        let conv_alt = BeamSpreadMode::Fan {
            base_deg: 92.0,
            base_rays: 2,
            atten_max: 0.0025,
            expansion_time: 1.5,
            reverse: true,
            force_loop: true,
            inner_ring: true,
            horizontal_ratio: 0.6,
        };
        let mut alt_state = BeamSpreadState::default();
        let mut cones = Vec::new();
        for _ in 0..=25 {
            let c = alt_state.prepare_tick(std::ptr::null(), 0, conv_alt, None, 92.0, 0.0);
            cones.push(c);
            alt_state.finish_tick(conv_alt, 0.1);
        }
        assert!((cones[0] - 0.23).abs() < 1e-3);
        assert!((cones[15] - 92.0).abs() < 1e-3);
        assert!((cones[25] - 30.82).abs() < 1e-2);

        // Glaxion + Heavy Caliber smoothed rays start centered on tick 0 then diverge smoothly.
        let smooth = BeamSpreadMode::Smoothed {
            smooth_time: 0.5,
            horizontal_ratio: 0.5,
        };
        let mut sm_state = BeamSpreadState::default();
        let (d0_t0, _) = sm_state.sample_ray(smooth, 2.97, 0, 1.0 / 12.0, &mut aim, &mut dir);
        let (d1_t0, _) = sm_state.sample_ray(smooth, 2.97, 1, 1.0 / 12.0, &mut aim, &mut dir);
        assert_eq!((d0_t0, d1_t0), (0.0, 0.0));
        let (d0_t1, p0_t1) = sm_state.sample_ray(smooth, 2.97, 0, 1.0 / 12.0, &mut aim, &mut dir);
        let (d1_t1, p1_t1) = sm_state.sample_ray(smooth, 2.97, 1, 1.0 / 12.0, &mut aim, &mut dir);
        assert!(d0_t1 > 0.0 && d1_t1 > 0.0 && (d0_t1, p0_t1) != (d1_t1, p1_t1));

        // MultishotUseConeAngle (Unicorn ms_cone[2]: 3.0 deg across 3 rounds -> -1.5, +1.5 deg).
        let ms_cone = crate::model::MultishotCone {
            cone_deg: 3.0,
            offset_deg: 0.0,
        };
        assert_eq!(ms_cone.sample(1, 3, 1.0), (1.5, 0.5));
        assert_eq!(ms_cone.sample(2, 3, 1.0), (1.5, 0.0));
    }
}
