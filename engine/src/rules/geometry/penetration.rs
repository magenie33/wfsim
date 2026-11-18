// SPDX-License-Identifier: AGPL-3.0-or-later
//! Native1753e2c..1753eb4: supplied reverse-query records, not a world-query implementation.

#[derive(Debug, Clone, Copy)]
pub struct ReverseHit {
    /// Supplied ResourceRef identity; zero represents the native null-reference case.
    pub reference: u64,
    pub point: [f32; 3],
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
    let square: [f32; 3] = delta.map(|v| v * v);
    let distance = libm::sqrtf((square[0] + square[1]) + square[2]);
    let accumulated = distance + budget.accumulated;
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
