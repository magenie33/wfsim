// SPDX-License-Identifier: AGPL-3.0-or-later
//! Player camera branch of native 0x149c340, with supplied camera and target providers.
use super::rotation;

pub struct EyeAim {
    pub target: [f32; 3],
    pub basis: [f32; 16],
}

#[wfsim_gas_macro::metered]
pub fn eye_aim(
    camera: [f32; 3],
    camera_forward: [f32; 3],
    mut target: [f32; 3],
    anchor: [f32; 3],
    avatar_forward: [f32; 3],
    avatar_up: [f32; 4],
) -> EyeAim {
    let d: [f32; 3] = std::array::from_fn(|i| target[i] - camera[i]);
    let square = (d[0] * d[0] + d[1] * d[1]) + d[2] * d[2];
    let mut origin = camera;
    if square >= f32::from_bits(0x2b8cbccc) {
        let p: [f32; 3] = std::array::from_fn(|i| (anchor[i] - camera[i]) * d[i]);
        let fraction = ((p[0] + p[1]) + p[2]) / square;
        if fraction >= 1. {
            origin = target;
            if fraction > 1. {
                target = std::array::from_fn(|i| target[i] + camera_forward[i]);
            }
        } else if fraction > 0. {
            origin = std::array::from_fn(|i| d[i] * fraction + camera[i]);
        }
    }
    if (0..3).all(|i| {
        (origin[i] - target[i]).abs()
            <= origin[i].abs().max(target[i].abs()).max(1.) * f32::from_bits(0x358637bd)
    }) {
        target = std::array::from_fn(|i| origin[i] + avatar_forward[i]);
    }
    let basis = rotation::look_at(
        [origin[0], origin[1], origin[2], 1.],
        [target[0], target[1], target[2], 1.],
        avatar_up,
    );
    EyeAim { target, basis }
}
