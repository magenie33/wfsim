// SPDX-License-Identifier: AGPL-3.0-or-later
//! Explicit-pose body queries. Native bindings and adoption scope: docs/GEOMETRY.md.
pub mod penetration;
mod query;
mod shape;
mod vector;

pub use query::*;
pub use shape::*;
pub use vector::*;

#[cfg(test)]
mod tests;
