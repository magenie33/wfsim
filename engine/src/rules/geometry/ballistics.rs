// SPDX-License-Identifier: AGPL-3.0-or-later
//! Client kinematic flight, wave sweep growth, restitution reflection and smart bounce seek.

/// Linear capsule sweep growth along flight time (`WaveProjectile::SweepAndResize`, `0xf18c90`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveResize {
    pub initial_radius_m: f32,
    pub final_radius_m: f32,
    pub resize_duration_seconds: f32,
    pub initial_height_m: f32,
    pub final_height_m: f32,
}

/// Forward cone target acquisition on bounce (`Glaive::FindSeekTarget`, `0x136bab0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BounceSeek {
    pub cone_deg: f32,
    pub range_m: f32,
}

/// Per-form kinematic flight, collision, fuse and bounce parameters.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BallisticsRuntime {
    pub infinite_puncture_depth: bool,
    pub max_life_seconds: f32,
    pub initial_speed_mps: f32,
    pub max_speed_mps: f32,
    pub min_speed_mps: f32,
    pub accel_mps2: f32,
    pub gravity_multiplier: f32,
    pub gravity_delay_seconds: f32,
    pub arming_distance_m: f32,
    pub explode_on_impact: bool,
    pub explode_on_expiry: bool,
    pub free_avatar_pierce: bool,
    pub can_bounce: bool,
    pub max_bounces: u16,
    pub restitution_normal: f32,
    pub restitution_tangent: f32,
    pub wave_resize: Option<WaveResize>,
    pub bounce_seek: Option<BounceSeek>,
}

/// `WeaponProjectileFireBehavior::ConfigureSpawnedProjectile` (`0xb94f40`) initial speed with `0.1x` floor.
#[wfsim_gas_macro::metered]
pub fn spawn_speed_mps(base_speed_mps: f32, speed_multiplier: f32) -> f32 {
    (base_speed_mps * speed_multiplier).max(base_speed_mps * 0.1_f32)
}

/// `Projectile::StepKinematicFlight` (`0x1585e40`) one-tick float32 kinematic integration:
/// scalar acceleration + `0.001` threshold pre-gravity 3D `MaxSpeed` sphere clamp,
/// `-9.8` world gravity after `gravity_delay_seconds` floored at `-50.0` m/s, then `p + v * frame_seconds`.
#[allow(clippy::too_many_arguments)]
#[wfsim_gas_macro::metered]
pub fn step_kinematic_flight(
    pos: [f32; 3],
    vel: [f32; 3],
    frame_seconds: f32,
    accel_mps2: f32,
    velocity_acceleration: f32,
    min_speed_mps: f32,
    max_speed_mps: f32,
    gravity_enabled: bool,
    gravity_multiplier: f32,
    gravity_delay_remaining_seconds: &mut f32,
) -> ([f32; 3], [f32; 3]) {
    let (mut vx, mut vy, mut vz) = (vel[0], vel[1], vel[2]);
    let v2 = (vx * vx + vy * vy) + vz * vz;
    let speed = libm::sqrtf(v2);
    let (ux, uy, uz) = if speed > 1.0e-6_f32 {
        let inv = 1.0_f32 / speed;
        (vx * inv, vy * inv, vz * inv)
    } else {
        (1.0_f32, 0.0_f32, 0.0_f32)
    };
    let mut new_speed =
        (speed + ((speed * velocity_acceleration) + accel_mps2) * frame_seconds).max(min_speed_mps);
    if max_speed_mps > 0.0_f32 {
        new_speed = new_speed.min(max_speed_mps);
    }
    if (speed - new_speed).abs() >= 0.001_f32 && new_speed >= 0.0_f32 {
        let delta = new_speed - speed;
        vx += delta * ux;
        vy += delta * uy;
        vz += delta * uz;
    }
    if gravity_enabled {
        if *gravity_delay_remaining_seconds > 0.0_f32 {
            *gravity_delay_remaining_seconds =
                (*gravity_delay_remaining_seconds - frame_seconds).max(0.0_f32);
        }
        if *gravity_delay_remaining_seconds <= 0.0_f32 {
            let dv_y = -9.8_f32 * (frame_seconds * gravity_multiplier);
            vy = (vy + dv_y).max(-50.0_f32);
        }
    }
    let next_pos = [
        pos[0] + vx * frame_seconds,
        pos[1] + vy * frame_seconds,
        pos[2] + vz * frame_seconds,
    ];
    (next_pos, [vx, vy, vz])
}

/// `WaveProjectile::SweepAndResize` (`0xf18c90`) linear radius and height interpolation.
#[wfsim_gas_macro::metered]
pub fn wave_dimensions(
    resize: WaveResize,
    elapsed_seconds: f32,
    size_multiplier: f32,
) -> (f32, f32) {
    let t = if resize.resize_duration_seconds > 0.0_f32 {
        (elapsed_seconds / resize.resize_duration_seconds).clamp(0.0_f32, 1.0_f32)
    } else {
        1.0_f32
    };
    let radius = (resize.initial_radius_m + (resize.final_radius_m - resize.initial_radius_m) * t)
        * size_multiplier;
    let height = (resize.initial_height_m + (resize.final_height_m - resize.initial_height_m) * t)
        * size_multiplier;
    (radius, height)
}

/// `Projectile::OnCollisionContact` (`0x13c58d9`) decomposed normal/tangent restitution reflection.
#[wfsim_gas_macro::metered]
pub fn restitution_reflect(
    vel: [f32; 3],
    normal: [f32; 3],
    restitution_normal: f32,
    restitution_tangent: f32,
    elasticity_multiplier: f32,
) -> [f32; 3] {
    let en = (restitution_normal * elasticity_multiplier).clamp(0.0_f32, 1.0_f32);
    let et = (restitution_tangent * elasticity_multiplier).clamp(0.0_f32, 1.0_f32);
    let vn = vel[0] * normal[0] + vel[1] * normal[1] + vel[2] * normal[2];
    let vt = [
        vel[0] - vn * normal[0],
        vel[1] - vn * normal[1],
        vel[2] - vn * normal[2],
    ];
    [
        vt[0] * et - vn * en * normal[0],
        vt[1] * et - vn * en * normal[1],
        vt[2] * et - vn * en * normal[2],
    ]
}

/// `Glaive::FindSeekTarget` (`0x136bab0`) cone-gated `dist / cos(theta)` score (minimum wins).
#[wfsim_gas_macro::metered]
pub fn bounce_seek_score(
    origin: [f32; 3],
    direction: [f32; 3],
    target: [f32; 3],
    seek: BounceSeek,
) -> Option<f32> {
    let dx = target[0] - origin[0];
    let dy = target[1] - origin[1];
    let dz = target[2] - origin[2];
    let dist_sq = (dx * dx + dy * dy) + dz * dz;
    if dist_sq <= 0.0_f32 || dist_sq > seek.range_m * seek.range_m {
        return None;
    }
    let dist = libm::sqrtf(dist_sq);
    let cos_theta = (dx * direction[0] + dy * direction[1] + dz * direction[2]) / dist;
    let cos_limit = libm::cosf(seek.cone_deg.clamp(0.0_f32, 180.0_f32) * f32::from_bits(0x3c8efa35));
    if cos_theta < cos_limit {
        return None;
    }
    if cos_theta > 1.0e-4_f32 {
        Some(dist / cos_theta)
    } else {
        Some(1.0e30_f32)
    }
}
