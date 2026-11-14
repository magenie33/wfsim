// SPDX-License-Identifier: AGPL-3.0-or-later
//! Native conditional activation and strict-negative timer cleanup.

#[allow(non_snake_case)]
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub struct ConditionalUpgrade {
    pub CanReproc: bool,
    pub ReprocTime: f32,
    pub UpgradeDuration: f32,
}

#[wfsim_gas_macro::metered]
impl Default for ConditionalUpgrade {
    fn default() -> Self { Self { CanReproc: true, ReprocTime: 0.0, UpgradeDuration: 0.0 } }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ConditionalState {
    pub active: bool,
    remaining_seconds: f32,
    reproc_seconds: f32,
}

#[wfsim_gas_macro::metered]
impl ConditionalState {
    pub fn activate(&mut self, spec: ConditionalUpgrade) -> bool {
        if self.active && (!spec.CanReproc || self.reproc_seconds > 0.0) { return false; }
        self.active = true;
        self.remaining_seconds = spec.UpgradeDuration;
        self.reproc_seconds = spec.ReprocTime;
        true
    }
    pub fn update(&mut self, frame_seconds: f32) -> bool {
        if !self.active { return false; }
        self.remaining_seconds -= frame_seconds;
        self.reproc_seconds -= frame_seconds;
        if self.remaining_seconds < 0.0 {
            self.active = false;
            true
        } else { false }
    }
}
