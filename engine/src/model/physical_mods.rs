use serde::Deserialize;
use crate::rules::damage::DamageType;

/// Stat brackets granted by the dominant modded physical damage type.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
pub struct PhysicalModBonus {
    #[serde(default)]
    pub fire_rate_bonus: f64,
    #[serde(default)]
    pub range_m: f64,
    #[serde(default)]
    pub status_chance_bonus: f64,
}

/// Physical MOD potency and the weapon's three physical-type stat grants.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PhysicalModSpec {
    pub multiplier: f64,
    #[serde(default)]
    pub impact: PhysicalModBonus,
    #[serde(default)]
    pub puncture: PhysicalModBonus,
    #[serde(default)]
    pub slash: PhysicalModBonus,
}

#[derive(Debug, Clone)]
pub struct PhysicalModBase {
    pub spec: PhysicalModSpec,
    /// The weapon's own physical split, before a progenitor bonus or mods.
    pub damage: [f64; 3],
}

#[wfsim_gas_macro::metered]
impl PhysicalModBase {
    /// Wiki Coda_Motovore, Characteristics and Notes: progenitor Impact does
    /// not select a wheel. Compare unquantized modded physical components.
    pub fn bonus(&self, physical: &[(DamageType, f64)]) -> PhysicalModBonus {
        [DamageType::Impact, DamageType::Puncture, DamageType::Slash]
            .into_iter()
            .zip(self.damage)
            .map(|(t, amount)| {
                let bonus = physical.iter().find(|(ty, _)| *ty == t).map_or(0.0, |(_, v)| *v);
                (t, amount * (1.0 + bonus).max(0.0))
            })
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map_or(PhysicalModBonus::default(), |(t, _)| match t {
                DamageType::Impact => self.spec.impact,
                DamageType::Puncture => self.spec.puncture,
                _ => self.spec.slash,
            })
    }
}
