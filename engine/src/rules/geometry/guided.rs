// SPDX-License-Identifier: AGPL-3.0-or-later
//! Steering reference for native16f990/11e65e0. State and float32 boundaries persist.

#[derive(Debug, Clone, Copy)]
pub struct Guided {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub angles: [f32; 2],
    pub angular_velocity: [f32; 2],
    pub flight: f32,
}

#[wfsim_gas_macro::metered]
fn length(v: [f32; 3]) -> f32 {
    libm::sqrtf((v[0] * v[0] + v[1] * v[1]) + v[2] * v[2])
}

#[wfsim_gas_macro::metered]
fn angles(v: [f32; 3]) -> [f32; 2] {
    let n = length(v);
    [
        libm::atan2f(v[0], v[2]).to_degrees(),
        -libm::asinf((v[1] / n).clamp(-1., 1.)).to_degrees(),
    ]
}

#[wfsim_gas_macro::metered]
impl Guided {
    pub fn new(position: [f32; 3], direction: [f32; 3], speed: f32) -> Self {
        Self {
            position,
            velocity: direction.map(|x| x * speed),
            angles: angles(direction),
            angular_velocity: [0.; 2],
            flight: 0.,
        }
    }

    /// Polynomial form recovered from16f990; coefficients constrained by the
    /// exported native angle/velocity transitions, not gameplay damage fitting.
    pub fn step(
        &mut self,
        target: Option<[f32; 3]>,
        frame_seconds: f32,
        turning: f32,
        acceleration: f32,
        maximum: f32,
    ) {
        let desired = target
            .map(|t| angles(std::array::from_fn(|i| t[i] - self.position[i])))
            .unwrap_or(self.angles);
        let omega = 2.0 / turning.max(0.0001);
        let x = omega * frame_seconds;
        let decay = 1.0 / (((x * 0.235 + 0.48) * x + 1.0) * x + 1.0);
        let mut yaw_target = desired[0];
        if yaw_target - self.angles[0] > 180.0 {
            yaw_target -= 360.0;
        }
        if yaw_target - self.angles[0] < -180.0 {
            yaw_target += 360.0;
        }
        for (i, target) in [yaw_target, desired[1]].into_iter().enumerate() {
            let change = self.angles[i] - target;
            let temp = (change * omega + self.angular_velocity[i]) * frame_seconds;
            self.angular_velocity[i] = (self.angular_velocity[i] - temp * omega) * decay;
            self.angles[i] = (change + temp) * decay + target;
        }
        let (yaw, pitch) = (self.angles[0].to_radians(), self.angles[1].to_radians());
        let (sy, cy) = libm::sincosf(yaw);
        let (sp, cp) = libm::sincosf(pitch);
        let direction = [sy * cp, -sp, cy * cp];
        let speed = (length(self.velocity) + frame_seconds * acceleration).min(maximum);
        self.velocity = direction.map(|v| v * speed);
        let previous = self.position;
        self.position = std::array::from_fn(|i| previous[i] + self.velocity[i] * frame_seconds);
        self.flight += length(std::array::from_fn(|i| self.position[i] - previous[i]));
    }
}
