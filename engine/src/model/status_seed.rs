// SPDX-License-Identifier: AGPL-3.0-or-later
//! Retained status inputs; applying a coefficient is a consumer decision.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatusSeed {
    pub base: f64,
    pub coefficient: f64,
    pub status_damage: f64,
    pub critical: f64,
    pub body_part: f64,
    pub attrition: f64,
    pub ability_final: f64,
    /// A Blast stack's faction, frozen at the proc; 1.0 for a tick, which
    /// re-reads faction live.
    pub faction: f64,
}


#[wfsim_gas_macro::metered]
impl StatusSeed {
    /// The seed itself: the product of every field.
    pub fn value(&self) -> f64 {
        self.base * self.coefficient * self.status_damage * self.critical
            * self.body_part * self.attrition * self.ability_final * self.faction
    }

    pub fn retained_value(&self) -> f64 {
        self.base * self.status_damage * self.critical * self.body_part
            * self.attrition * self.ability_final * self.faction
    }

    pub fn supplied(value: f64) -> Self {
        Self { base: value, coefficient: 1.0, status_damage: 1.0, critical: 1.0,
            body_part: 1.0, attrition: 1.0, ability_final: 1.0, faction: 1.0 }
    }
}

/// Native radial records store a binary32 seed before their coefficient is read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StoredStatusSeed {
    Radial { retained_seed: f32, coefficient: f32 },
    Supplied(f64),
}

#[wfsim_gas_macro::metered]
impl StoredStatusSeed {
    pub fn radial(input: StatusSeed) -> Self {
        Self::Radial { retained_seed: input.retained_value() as f32, coefficient: input.coefficient as f32 }
    }

    pub fn from_unbound_source(input: StatusSeed) -> Self {
        Self::Supplied(input.retained_value() * input.coefficient)
    }

    pub fn supplied(value: f64) -> Self { Self::Supplied(value) }

    pub fn snapshot_damage(self) -> f64 {
        match self {
            Self::Radial { retained_seed, coefficient } => (coefficient * retained_seed) as f64,
            Self::Supplied(damage) => damage,
        }
    }

    pub fn radial_values(self) -> Option<(f32, f32)> {
        match self { Self::Radial { retained_seed, coefficient } => Some((retained_seed, coefficient)), _ => None }
    }
}
