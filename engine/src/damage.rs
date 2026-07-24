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
    /// Every damage type, in declaration order (indexes match `as usize`).
    pub const ALL: [DamageType; 16] = [
        DamageType::Impact,
        DamageType::Puncture,
        DamageType::Slash,
        DamageType::Cold,
        DamageType::Electricity,
        DamageType::Heat,
        DamageType::Toxin,
        DamageType::Blast,
        DamageType::Corrosive,
        DamageType::Gas,
        DamageType::Magnetic,
        DamageType::Radiation,
        DamageType::Viral,
        DamageType::True,
        DamageType::Void,
        DamageType::Tau,
    ];

    /// A primary element (Cold / Electricity / Heat / Toxin) — the ones that
    /// combine into secondary elements.
    pub fn is_primary_element(self) -> bool {
        matches!(
            self,
            DamageType::Cold | DamageType::Electricity | DamageType::Heat | DamageType::Toxin
        )
    }
}

/// A hit's damage as a vector over damage types (docs/MECHANICS.md §1).
///
/// Panel damage is the plain **sum** of the components; a crit multiplies the
/// **whole vector** at once (one roll per hit/pellet, never per component);
/// the defense side then resolves each component independently
/// (docs/MECHANICS.md §5, §8).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DamageVector {
    amounts: [f64; DamageType::ALL.len()],
}

impl DamageVector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builder-style: set one component.
    pub fn with(mut self, t: DamageType, amount: f64) -> Self {
        self.amounts[t as usize] = amount;
        self
    }

    pub fn get(&self, t: DamageType) -> f64 {
        self.amounts[t as usize]
    }

    pub fn set(&mut self, t: DamageType, amount: f64) {
        self.amounts[t as usize] = amount;
    }

    pub fn add(&mut self, t: DamageType, amount: f64) {
        self.amounts[t as usize] += amount;
    }

    /// Panel damage: the sum of all components.
    pub fn total(&self) -> f64 {
        self.amounts.iter().sum()
    }

    /// Scale every component (e.g. by the crit tier multiplier).
    pub fn scale(&self, k: f64) -> Self {
        let mut out = *self;
        for a in &mut out.amounts {
            *a *= k;
        }
        out
    }

    /// This type's share of the total — the proc-type weight
    /// (`P(type) = damage / total`, docs/MECHANICS.md §6).
    pub fn share(&self, t: DamageType) -> f64 {
        let total = self.total();
        if total <= 0.0 {
            0.0
        } else {
            self.get(t) / total
        }
    }

    /// Non-zero components in declaration order.
    pub fn iter_nonzero(&self) -> impl Iterator<Item = (DamageType, f64)> + '_ {
        DamageType::ALL
            .iter()
            .map(|&t| (t, self.get(t)))
            .filter(|&(_, a)| a > 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_damage_is_the_sum_of_components() {
        // Five components of 10 each -> panel 50 (user example, MECHANICS §1).
        let v = DamageVector::new()
            .with(DamageType::Impact, 10.0)
            .with(DamageType::Slash, 10.0)
            .with(DamageType::Heat, 10.0)
            .with(DamageType::Toxin, 10.0)
            .with(DamageType::Corrosive, 10.0);
        assert_eq!(v.total(), 50.0);
        assert_eq!(v.share(DamageType::Heat), 0.2);
    }

    #[test]
    fn crit_scales_the_whole_vector_together() {
        let v = DamageVector::new()
            .with(DamageType::Impact, 10.0)
            .with(DamageType::Toxin, 40.0);
        let crit = v.scale(2.0);
        assert_eq!(crit.get(DamageType::Impact), 20.0);
        assert_eq!(crit.get(DamageType::Toxin), 80.0);
        assert_eq!(crit.total(), 100.0);
    }

    #[test]
    fn empty_vector_has_zero_shares() {
        let v = DamageVector::new();
        assert_eq!(v.total(), 0.0);
        assert_eq!(v.share(DamageType::Impact), 0.0);
        assert_eq!(v.iter_nonzero().count(), 0);
    }
}
