// SPDX-License-Identifier: AGPL-3.0-or-later
//! Native11e65e0 default numerical route with supplied targets and empty movement.
use super::rotation;

#[derive(Debug, Clone, Copy)]
pub struct Guided {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub orientation: [f32; 4],
    pub angular_velocity: [f32; 3],
    pub bank: f32,
    pub flight: f32,
}

#[wfsim_gas_macro::metered]
fn length(v: [f32; 3]) -> f32 {
    libm::sqrtf((v[0] * v[0] + v[1] * v[1]) + v[2] * v[2])
}

#[wfsim_gas_macro::metered]
fn angles(direction: [f32; 3]) -> [f32; 3] {
    let point = [direction[0], direction[1], direction[2], 0.];
    rotation::basis_to_euler(rotation::look_at([0.; 4], point, [0., 1., 0., 0.]))
}

#[wfsim_gas_macro::metered]
pub(super) fn unit(vector: [f32; 3]) -> Option<[f32; 3]> {
    let threshold = f32::from_bits(0x358637bd);
    let norm = length(vector);
    if norm > threshold {
        let reciprocal = 1. / norm;
        return Some(vector.map(|v| reciprocal * v));
    }
    let maximum = vector[0].abs().max(vector[1].abs()).max(vector[2].abs());
    if maximum <= threshold {
        return None;
    }
    let reciprocal = 1. / maximum;
    let scaled = vector.map(|v| reciprocal * v);
    let norm = length(scaled);
    (norm > threshold).then(|| scaled.map(|v| (1. / norm) * v))
}

#[wfsim_gas_macro::metered]
impl Guided {
    /// Supplied-direction initializer; the game's actual emission writer is separate.
    pub fn new(position: [f32; 3], direction: [f32; 3], speed: f32) -> Self {
        Self {
            position,
            velocity: direction.map(|x| x * speed),
            orientation: rotation::euler_to_quaternion(angles(direction)),
            angular_velocity: [0.; 3],
            bank: 0.,
            flight: 0.,
        }
    }

    pub fn step(
        &mut self,
        target: Option<[f32; 3]>,
        frame_seconds: f32,
        turning: f32,
        acceleration: f32,
        maximum: f32,
    ) -> [f32; 3] {
        self.step_with_world(
            target,
            frame_seconds,
            turning,
            acceleration,
            maximum,
            self.orientation,
        )
    }

    /// Default clock/state, no gravity and an empty native +1a10 reference. Movement is the
    /// retained empty-world branch; real collision and its callbacks remain separate.
    pub fn step_with_world(
        &mut self,
        target: Option<[f32; 3]>,
        seconds: f32,
        turning: f32,
        acceleration: f32,
        maximum: f32,
        world_orientation: [f32; 4],
    ) -> [f32; 3] {
        let direction = target.map_or_else(
            || {
                let forward = rotation::quaternion_direction(self.orientation);
                [forward[0], forward[1], forward[2]]
            },
            |point| std::array::from_fn(|i| point[i] - self.position[i]),
        );
        let square = (direction[0] * direction[0] + direction[1] * direction[1])
            + direction[2] * direction[2];
        let desired = (square > f32::from_bits(0x2b8cbccc)).then(|| angles(direction));
        let current = rotation::quaternion_to_euler(self.orientation);
        let euler = rotation::steer_euler(
            current,
            desired,
            &mut self.angular_velocity,
            turning,
            target.is_some(),
            seconds,
        );
        let forward = rotation::euler_direction(euler);
        let speed = (seconds * acceleration + length(self.velocity)).min(maximum);
        self.velocity = forward.map(|v| speed * v);
        let distance = speed * seconds;
        let displacement = self.velocity.map(|v| seconds * v);
        self.flight += distance;
        self.position = std::array::from_fn(|i| displacement[i] + self.position[i]);
        let Some(direction) = unit(displacement) else {
            return displacement;
        };
        let inverse = [
            -world_orientation[0],
            -world_orientation[1],
            -world_orientation[2],
        ];
        let turn = rotation::cross(inverse, direction).map(|v| v + v);
        let cross = rotation::cross(inverse, turn);
        let local =
            std::array::from_fn(|i| cross[i] + (world_orientation[3] * turn[i] + direction[i]));
        let Some(local) = unit(local) else {
            return displacement;
        };
        let mut euler = angles(direction);
        let target_bank = (local[0] * -1000.).clamp(-70., 70.);
        (self.bank, self.angular_velocity[2]) = rotation::damp_value(
            self.bank,
            target_bank,
            self.angular_velocity[2],
            turning,
            seconds,
        );
        euler[2] = self.bank;
        self.orientation = rotation::euler_to_quaternion(euler);
        displacement
    }
}
