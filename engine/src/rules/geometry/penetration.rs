// SPDX-License-Identifier: AGPL-3.0-or-later
//! Trace-gun and kinematic projectile traversal from supplied reverse-query records.

#[derive(Debug, Clone, Copy)]
pub struct ReverseHit {
    /// Supplied ResourceRef identity; zero represents the native null-reference case.
    pub reference: u64,
    pub point: [f32; 3],
}

#[derive(Debug)]
pub struct ProjectileExit {
    pub point: [f32; 3],
    pub resume: [f32; 3],
    pub material: f32,
    pub remaining: f32,
}

/// Native A2AA00 uses the remaining budget to construct the reverse ray.
#[wfsim_gas_macro::metered]
pub fn reverse_start(anchor: [f32; 3], direction: [f32; 3], remaining: f32) -> [f32; 3] {
    let reach = remaining * 1.1;
    std::array::from_fn(|i| anchor[i] + direction[i] * reach)
}

#[wfsim_gas_macro::metered]
pub fn projectile_exit(
    anchor: [f32; 3],
    direction: [f32; 3],
    reference: u64,
    reverse: &[ReverseHit],
    remaining: f32,
) -> Option<ProjectileExit> {
    if remaining <= 0. {
        return None;
    }
    let point = reverse
        .iter()
        .rfind(|hit| hit.reference == reference)?
        .point;
    let delta = std::array::from_fn(|i| anchor[i] - point[i]);
    let material = length(delta);
    if material > remaining {
        return None;
    }
    let resume = if material == 0. {
        std::array::from_fn(|i| point[i] + direction[i] * 0.01)
    } else {
        point
    };
    Some(ProjectileExit {
        point,
        resume,
        material,
        remaining: remaining - material,
    })
}

#[wfsim_gas_macro::metered]
fn length([x, y, z]: [f32; 3]) -> f32 {
    libm::sqrtf((x * x + y * y) + z * z)
}

#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub maximum: f32,
    pub accumulated: f32,
    pub infinite: bool,
}

#[derive(Debug)]
pub struct Traversal {
    pub matching_index: Option<usize>,
    pub continues: bool,
    pub accumulated: f32,
}

#[wfsim_gas_macro::metered]
pub fn traverse(
    entry: [f32; 3],
    reference: u64,
    reverse: &[ReverseHit],
    budget: Budget,
) -> Traversal {
    let index = reverse.iter().rposition(|hit| hit.reference == reference);
    let Some(i) = index else {
        return Traversal {
            matching_index: None,
            continues: false,
            accumulated: budget.accumulated,
        };
    };
    let delta: [f32; 3] = std::array::from_fn(|j| entry[j] - reverse[i].point[j]);
    let accumulated = length(delta) + budget.accumulated;
    let continues = budget.infinite || accumulated <= budget.maximum;
    Traversal {
        matching_index: Some(i),
        continues,
        accumulated: if continues {
            accumulated
        } else {
            budget.maximum
        },
    }
}
