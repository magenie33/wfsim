// SPDX-License-Identifier: AGPL-3.0-or-later
//! Native16f990 angular state and its rotation helpers; provenance: GEOMETRY.md.

#[wfsim_gas_macro::metered]
pub fn wrap_degrees(value: f32) -> f32 {
    if !value.is_finite() {
        return value;
    }
    if (value - -180.).abs() <= value.abs().max(180.).max(1.) * f32::from_bits(0x358637bd) {
        return 180.;
    }
    if value <= -180. {
        let wrapped = libm::fmodf(value, 360.);
        if wrapped > -180. {
            wrapped
        } else {
            wrapped + 360.
        }
    } else if value > 180. {
        let wrapped = libm::fmodf(value, 360.);
        if wrapped > 180. {
            wrapped + -360.
        } else {
            wrapped
        }
    } else {
        value
    }
}

#[wfsim_gas_macro::metered]
pub fn canonical_euler(mut angles: [f32; 3]) -> [f32; 3] {
    angles[1] = wrap_degrees(angles[1]);
    if angles[1] > 90. {
        angles[0] -= 180.;
        angles[2] -= 180.;
        angles[1] = 180. - angles[1];
    } else if angles[1] < -90. {
        angles[0] += 180.;
        angles[1] = -180. - angles[1];
        angles[2] += 180.;
    }
    angles[0] = wrap_degrees(angles[0]);
    angles[2] = wrap_degrees(angles[2]);
    angles
}

/// Desired Euler is supplied by the native look-at stage. None denotes its
/// direction-squared <= float32(1e-12) branch, independently of target retention.
#[wfsim_gas_macro::metered]
pub fn steer_euler(
    mut current: [f32; 3],
    desired: Option<[f32; 3]>,
    velocity: &mut [f32; 3],
    turning: f32,
    retained: bool,
    seconds: f32,
) -> [f32; 3] {
    let Some(mut desired) = desired else {
        *velocity = [0.; 3];
        return canonical_euler(current);
    };
    if desired[0] - current[0] > 180. {
        desired[0] -= 360.;
    }
    if desired[0] - current[0] < -180. {
        desired[0] += 360.;
    }
    let duration = if retained { turning } else { turning * 5. };
    for i in 0..2 {
        (current[i], velocity[i]) =
            damp_value(current[i], desired[i], velocity[i], duration, seconds);
    }
    canonical_euler(current)
}

#[wfsim_gas_macro::metered]
pub fn damp_value(
    current: f32,
    target: f32,
    velocity: f32,
    duration: f32,
    seconds: f32,
) -> (f32, f32) {
    let omega = 2. / duration.max(f32::from_bits(0x358637bd));
    let x = omega * seconds;
    let decay =
        1. / (((x * f32::from_bits(0x3e70a3d7) + f32::from_bits(0x3ef5c28f)) * x + 1.) * x + 1.);
    let change = current - target;
    let temp = (change * omega + velocity) * seconds;
    (
        (temp + change) * decay + target,
        (velocity - temp * omega) * decay,
    )
}

/// Scalar lane of native3a5fe0; its polynomial and range reduction use binary32.
#[wfsim_gas_macro::metered]
pub fn sin_cos(angle: f32) -> (f32, f32) {
    let turns = (angle * f32::from_bits(0x3e22f983)).round_ties_even();
    let reduced = angle - turns * f32::from_bits(0x40c90fdb);
    let reflected = f32::from_bits(0x40490fdb | (reduced.to_bits() & 0x80000000)) - reduced;
    let (x, sign) = if reduced.abs() <= f32::from_bits(0x3fc90fdb) {
        (reduced, 0)
    } else {
        (reflected, 0x80000000)
    };
    let square = x * x;
    let cosine = (((((f32::from_bits(0xb48bdd11) * square + f32::from_bits(0x37cfb4c2))
        * square
        + f32::from_bits(0xbab609aa))
        * square
        + f32::from_bits(0x3d2aaaa3))
        * square
        - 0.5)
        * square)
        + 1.;
    let sine = ((((((f32::from_bits(0xb2cd365b) * square + f32::from_bits(0x3638b88e))
        * square
        + f32::from_bits(0xb9500bf1))
        * square
        + f32::from_bits(0x3c088886))
        * square
        + f32::from_bits(0xbe2aaaab))
        * square)
        + 1.)
        * x;
    (sine, f32::from_bits(cosine.to_bits() ^ sign))
}

#[wfsim_gas_macro::metered]
pub fn euler_to_quaternion([yaw, pitch, roll]: [f32; 3]) -> [f32; 4] {
    let factor = f32::from_bits(0x3c0efa35);
    let (sp, cp) = sin_cos(pitch * factor);
    let (sy, cy) = sin_cos(yaw * factor);
    let (sr, cr) = sin_cos(roll * factor);
    [
        (cp * sy) * sr + (cy * sp) * cr,
        (-sp * cy) * sr + (sy * cp) * cr,
        (-sp * sy) * cr + (cy * cp) * sr,
        (sp * sy) * sr + (cy * cp) * cr,
    ]
}

#[wfsim_gas_macro::metered]
pub fn euler_direction([yaw, pitch, _]: [f32; 3]) -> [f32; 3] {
    let factor = f32::from_bits(0x3c8efa35);
    let (sy, cy) = sin_cos(yaw * factor);
    let (sp, cp) = sin_cos(pitch * factor);
    [cp * sy, 0. - sp, cp * cy]
}

#[wfsim_gas_macro::metered]
pub fn euler_to_basis([yaw, pitch, roll]: [f32; 3]) -> [f32; 16] {
    let factor = f32::from_bits(0x3c8efa35);
    let (sy, cy) = sin_cos(yaw * factor);
    let (sp, cp) = sin_cos(pitch * factor);
    let (sr, cr) = sin_cos(roll * factor);
    [
        cr * cy + (sr * sp) * sy,
        cp * sr,
        (sr * sp) * cy - cr * sy,
        0.,
        (cr * sp) * sy - cy * sr,
        cr * cp,
        (cr * sp) * cy + sr * sy,
        0.,
        cp * sy,
        0. - sp,
        cp * cy,
        0.,
        0.,
        0.,
        0.,
        1.,
    ]
}

#[wfsim_gas_macro::metered]
pub fn quaternion_direction([x, y, z, w]: [f32; 4]) -> [f32; 4] {
    [
        (y * w + y * w) + (x * z + x * z),
        (y * z + y * z) - (x * w + x * w),
        (z * z - (y * y + x * x)) + w * w,
        0.,
    ]
}

/// Native1c26f10 finite, unit-domain polynomial, including its split-root branch.
#[wfsim_gas_macro::metered]
pub fn asin(value: f32) -> f32 {
    let absolute = value.abs();
    if absolute.to_bits() >> 23 < 0x71 {
        return value;
    }
    if absolute == 1. {
        return f32::from_bits(0x3fc90fdb | (value.to_bits() & 0x80000000));
    }
    let large = absolute >= 0.5;
    let square = if large {
        (1. - absolute) * 0.5
    } else {
        absolute * absolute
    };
    let root = if large { libm::sqrtf(square) } else { absolute };
    let mut polynomial = f32::from_bits(0xbc5b3fe1) - square * f32::from_bits(0x3b81ce6b);
    let denominator = f32::from_bits(0x3f8d6fa5) - square * f32::from_bits(0x3f561f0d);
    polynomial = polynomial * square - f32::from_bits(0x3d678bdd);
    polynomial = polynomial * square + f32::from_bits(0x3e3c94dc);
    polynomial = polynomial * square / denominator;
    let result = if large {
        let high = f32::from_bits(root.to_bits() & 0xffff0000);
        let correction = ((square - high * high) / (high + root)) * 2.;
        let low = f32::from_bits(0x33a22168) - correction;
        let remainder =
            ((root + root) * polynomial - low) - (f32::from_bits(0x3f490fda) - (high + high));
        f32::from_bits(0x3f490fda) - remainder
    } else {
        root * polynomial + root
    };
    f32::from_bits(result.to_bits() ^ (value.to_bits() & 0x80000000))
}

/// Native1c27130 finite-input table reduction and binary64 polynomial.
#[wfsim_gas_macro::metered]
pub fn atan2(y: f32, x: f32) -> f32 {
    let sign = y.to_bits() & 0x80000000;
    if x.is_nan() {
        return x + x;
    }
    if y.is_nan() {
        return y + y;
    }
    if x.is_infinite() && y.is_infinite() {
        let angle = if x.is_sign_negative() {
            0x4016cbe4
        } else {
            0x3f490fdb
        };
        return f32::from_bits(angle | sign);
    }
    if y == 0. {
        return if x.is_sign_negative() {
            f32::from_bits(0x40490fdb | sign)
        } else {
            y
        };
    }
    let (ay, ax) = (f64::from(y.abs()), f64::from(x.abs()));
    let exponent = ((ay.to_bits() >> 52) as i32 & 0x7ff) - ((ax.to_bits() >> 52) as i32 & 0x7ff);
    if x == 0. || exponent > 26 {
        return f32::from_bits(0x3fc90fdb | sign);
    }
    if exponent < -13 && !x.is_sign_negative() {
        return (f64::from(y) / f64::from(x)) as f32;
    }
    if exponent < -26 && x.is_sign_negative() {
        return f32::from_bits(0x40490fdb | sign);
    }
    let swapped = ay > ax;
    let (low, high) = if swapped { (ax, ay) } else { (ay, ax) };
    let ratio = low / high;
    let mut result = if ratio > 0.0625 {
        let index = (ratio * 256. + 0.5) as usize;
        let numerator = low * 256. - index as f64 * high;
        let denominator = index as f64 * low + high * 256.;
        let reduced = numerator / denominator;
        let angle = reduced + f64::from_bits(super::rotation_tables::ATAN[index - 16]);
        let correction = ((reduced * reduced) * reduced) * f64::from_bits(0x3fd5555555550877);
        angle - correction
    } else if ratio >= f64::from_bits(0x3f1a36e2eb1c432d) {
        let square = ratio * ratio;
        let polynomial = f64::from_bits(0x3fd5555555555538)
            - (f64::from_bits(0x3fc99999999643a3) - square * f64::from_bits(0x3fc2492482bd6be1))
                * square;
        ratio - polynomial * (square * ratio)
    } else {
        ratio
    };
    if swapped {
        result = f64::from_bits(0x3ff921fb54442d18) - result;
    }
    if x.is_sign_negative() {
        result = f64::from_bits(0x400921fb54442d18) - result;
    }
    f32::from_bits((result as f32).to_bits() ^ sign)
}

#[wfsim_gas_macro::metered]
pub fn quaternion_to_euler([x, y, z, w]: [f32; 4]) -> [f32; 3] {
    let degrees = f32::from_bits(0x42652ee0);
    let roll_denominator = ((y * y - x * x) - z * z) + w * w;
    let yaw_denominator = z * z - (y * y + x * x) + w * w;
    let threshold = f32::from_bits(0x358637bd);
    let roll_numerator = w * z + x * y;
    let yaw_numerator = y * w + x * z;
    canonical_euler([
        atan2(
            yaw_numerator + yaw_numerator,
            if yaw_denominator.abs() > threshold {
                yaw_denominator
            } else {
                0.
            },
        ) * degrees,
        asin(((x * w - y * z) * 2.).clamp(-1., 1.)) * degrees,
        atan2(
            roll_numerator + roll_numerator,
            if roll_denominator.abs() > threshold {
                roll_denominator
            } else {
                0.
            },
        ) * degrees,
    ])
}

#[wfsim_gas_macro::metered]
pub fn basis_to_euler(basis: [f32; 16]) -> [f32; 3] {
    let degrees = f32::from_bits(0x42652ee0);
    let threshold = f32::from_bits(0x358637bd);
    canonical_euler([
        atan2(
            basis[8],
            if basis[10].abs() > threshold {
                basis[10]
            } else {
                0.
            },
        ) * degrees,
        asin((-basis[9]).clamp(-1., 1.)) * degrees,
        atan2(
            basis[1],
            if basis[5].abs() > threshold {
                basis[5]
            } else {
                0.
            },
        ) * degrees,
    ])
}

/// The retained Unicorn execution's RSQRTPS seed. Physical-CPU approximation
/// differences require their own calibration; this is not a hardware guarantee.
#[wfsim_gas_macro::metered]
pub fn reciprocal_sqrt(value: f32) -> f32 {
    1. / libm::sqrtf(value)
}

#[wfsim_gas_macro::metered]
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
}

#[wfsim_gas_macro::metered]
pub fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[wfsim_gas_macro::metered]
fn basis_unit(vector: [f32; 3]) -> [f32; 3] {
    let square = dot(vector, vector);
    let reciprocal = reciprocal_sqrt(square);
    let correction = 3. - (square * reciprocal) * reciprocal;
    vector.map(|v| correction * ((v * reciprocal) * 0.5))
}

#[wfsim_gas_macro::metered]
pub fn look_at(eye: [f32; 4], target: [f32; 4], up: [f32; 4]) -> [f32; 16] {
    let forward = basis_unit(std::array::from_fn(|i| target[i] - eye[i]));
    let mut up = [up[0], up[1], up[2]];
    let threshold = f32::from_bits(0x3f666666);
    if dot(forward, up).abs() > threshold {
        up = [1., 0., 0.];
        if dot(forward, up).abs() > threshold {
            up = [0., 0., 1.];
            if dot(forward, up).abs() > threshold {
                up = [0., 1., 0.];
            }
        }
    }
    let right = basis_unit(cross(up, forward));
    let up = basis_unit(cross(forward, right));
    [
        right[0], right[1], right[2], 0., up[0], up[1], up[2], 0., forward[0], forward[1],
        forward[2], 0., eye[0], eye[1], eye[2], 1.,
    ]
}
