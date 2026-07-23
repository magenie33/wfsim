//! Damage types (see `docs/MECHANICS.md` §1 and `docs/GLOSSARY.md`).
//!
//! Every hit is a vector over these types, not a scalar. Names follow the wiki.

/// A single damage type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DamageType {
    // Physical (IPS).
    Impact,
    Puncture,
    Slash,

    // Primary elemental.
    Cold,
    Electricity,
    Heat,
    Toxin,

    // Secondary (combined) elemental.
    Blast,     // Heat + Cold
    Corrosive, // Electricity + Toxin
    Gas,       // Heat + Toxin
    Magnetic,  // Cold + Electricity
    Radiation, // Heat + Electricity
    Viral,     // Cold + Toxin

    // Special.
    True,
    Void,
    Tau,
}

impl DamageType {
    /// A primary element (Cold / Electricity / Heat / Toxin) — the ones that
    /// combine into secondary elements.
    pub fn is_primary_element(self) -> bool {
        matches!(
            self,
            DamageType::Cold | DamageType::Electricity | DamageType::Heat | DamageType::Toxin
        )
    }
}
