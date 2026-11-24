// SPDX-License-Identifier: AGPL-3.0-or-later
//! The supplied-state non-sweep branch of 0x1246b90 and ordinary 0x136bab0 scoring.

#[wfsim_gas_macro::metered]
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
}

/// The original transform builder 0x11d96d0 supplies the first basis vector.
#[wfsim_gas_macro::metered]
pub fn query_ends(
    start: [f32; 3],
    displacement: [f32; 3],
    [x, y, z, w]: [f32; 4],
    radius: f32,
    continuation: bool,
) -> impl Iterator<Item = [f32; 3]> {
    let twice_y = y + y;
    let twice_z = z + z;
    let right = [
        (1. - twice_y * y) - twice_z * z,
        x * twice_y + twice_z * w,
        x * twice_z - twice_y * w,
    ];
    let center: [f32; 3] =
        std::array::from_fn(|i| displacement[i] * f32::from_bits(0x3f99999a) + start[i]);
    let offset = right.map(|v| radius * v);
    let ends = [
        center,
        std::array::from_fn(|i| center[i] + offset[i]),
        std::array::from_fn(|i| center[i] - offset[i]),
    ];
    // 0x1246bec normalizes a scratch copy via 0xa24070 before building rays.
    let count = if super::guided::unit(displacement).is_none() {
        0
    } else if continuation {
        1
    } else {
        3
    };
    ends.into_iter().take(count)
}

/// The supplied backend owns record order, and the target policy owns preacceptance.
/// A positive normal dot drops this segment after choosing its first eligible record.
#[wfsim_gas_macro::metered]
pub fn impact_record<T>(
    displacement: [f32; 3],
    backend_success: bool,
    records: impl IntoIterator<Item = (T, bool, [f32; 3])>,
) -> Option<T> {
    if !backend_success {
        return None;
    }
    let (record, _, normal) = records.into_iter().find(|(_, accepted, _)| *accepted)?;
    (dot(normal, displacement) <= 0.).then_some(record)
}

/// Empty-history score; position, candidate root and sampled aim point are distinct.
#[wfsim_gas_macro::metered]
pub fn candidate_score(
    position: [f32; 3],
    forward: [f32; 3],
    root: [f32; 3],
    anchor: [f32; 3],
    range: f32,
    cone_dot: f32,
) -> Option<f32> {
    let to_root = std::array::from_fn(|i| root[i] - position[i]);
    if libm::sqrtf(dot(to_root, to_root)) > range {
        return None;
    }
    let to_anchor = std::array::from_fn(|i| anchor[i] - position[i]);
    let distance = libm::sqrtf(dot(to_anchor, to_anchor));
    let normalized = if distance > 1e-6 {
        let inverse = 1. / distance;
        to_anchor.map(|v| v * inverse)
    } else {
        // The original uses max-component scaling when the first norm is tiny.
        let max = to_anchor.iter().map(|v| v.abs()).fold(0., f32::max);
        if max == 0. {
            to_anchor
        } else {
            let inv_max = 1. / max;
            let scaled = to_anchor.map(|v| v * inv_max);
            let norm = libm::sqrtf(dot(scaled, scaled));
            if norm > 1e-6 {
                let inv = 1. / norm;
                scaled.map(|v| v * inv)
            } else {
                to_anchor
            }
        }
    };
    let dot = dot(normalized, forward);
    (dot >= cone_dot).then(|| distance / dot)
}

/// A visibility query is issued only for a candidate capable of replacing the winner.
#[wfsim_gas_macro::metered]
pub fn select_candidate(
    scores: impl IntoIterator<Item = (usize, f32)>,
    mut visible: impl FnMut(usize) -> bool,
) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (index, score) in scores {
        if best.is_none_or(|(_, previous)| score < previous) && visible(index) {
            best = Some((index, score));
        }
    }
    best.map(|(index, _)| index)
}
