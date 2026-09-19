// SPDX-License-Identifier: AGPL-3.0-or-later
//! WHAT AN ENEMY IS, as a file states it.

/// A THRAX'S SPECTRAL FORM — the body that stands up where the physical one
/// fell. *"Destroying the physical form reverts it into a spectral form with
/// 40% of the physical form's health. Void damage deals 10x damage to the
/// spectral form"*, and Void damage is the OPERATOR's: no weapon in this
/// roster can touch it, which is why a fight with it on has a ceiling of zero
/// kills for every gun.
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct SpectralForm {
    /// Its health, as a share of the physical form's.
    pub health_share: f64,
    /// Seconds between the physical form falling and the spectre standing up.
    /// ASSUMED — nothing publishes it.
    pub delay_seconds: f64,
}
