//! Pipeline layer [2]: elemental combination.
//!
//! Implements the hierarchy algorithm of docs/MECHANICS.md §3 (wiki `Damage`
//! §Modding/§Load Order) for the cases the engine currently needs: a weapon
//! with a **purely physical innate vector** (Dual Toxocyst) plus mod-added
//! primaries, combined-element mods, and buff-injected elements. Innate
//! elemental weapons (rules 2/3/5/7 innate clauses) are recorded but not yet
//! wired — no such weapon is in the database.

use crate::damage::{DamageType, DamageVector};

/// The secondary element formed by two distinct primaries (order-free).
pub fn combined_of(a: DamageType, b: DamageType) -> Option<DamageType> {
    use DamageType::*;
    match (a, b) {
        (Cold, Electricity) | (Electricity, Cold) => Some(Magnetic),
        (Cold, Heat) | (Heat, Cold) => Some(Blast),
        (Cold, Toxin) | (Toxin, Cold) => Some(Viral),
        (Electricity, Heat) | (Heat, Electricity) => Some(Radiation),
        (Electricity, Toxin) | (Toxin, Electricity) => Some(Corrosive),
        (Heat, Toxin) | (Toxin, Heat) => Some(Gas),
        _ => None,
    }
}

/// Elemental contributions entering the hierarchy.
#[derive(Debug, Clone, Default)]
pub struct ElementalInput {
    /// Primary-element amounts in **first-placement order** (rule 4: the
    /// first mod of an element establishes its position; later same-element
    /// mods are pre-merged into that entry by the caller or via `push`).
    pub ordered: Vec<(DamageType, f64)>,
    /// Combined-element mod amounts (Magnetic Might family): added directly,
    /// outside the primary hierarchy (rule 7).
    pub direct_secondary: Vec<(DamageType, f64)>,
    /// Buff-injected primaries (Frenzy's +100% Toxin): appended at the END
    /// of the order (rule 8), additive with same-element mods.
    pub injected: Vec<(DamageType, f64)>,
}

impl ElementalInput {
    /// Add a primary at its first-placement position (rule 4).
    pub fn push(&mut self, t: DamageType, amount: f64) {
        if let Some(e) = self.ordered.iter_mut().find(|(x, _)| *x == t) {
            e.1 += amount;
        } else {
            self.ordered.push((t, amount));
        }
    }
}

/// Combine the physical vector with the elemental hierarchy.
///
/// Adjacent uncombined primaries merge pairwise into secondaries (rule 1);
/// an odd trailing primary stays pure. Injected elements enter last: into
/// their element's existing position if one exists, else appended.
pub fn combine(physical: &DamageVector, input: &ElementalInput) -> DamageVector {
    let mut order = input.ordered.clone();
    for &(t, amount) in &input.injected {
        if let Some(e) = order.iter_mut().find(|(x, _)| *x == t) {
            e.1 += amount;
        } else {
            order.push((t, amount));
        }
    }

    let mut out = *physical;
    let mut pairs = order.chunks_exact(2);
    for pair in &mut pairs {
        let (a, av) = pair[0];
        let (b, bv) = pair[1];
        let sec = combined_of(a, b).expect("two distinct primaries always combine");
        out.add(sec, av + bv);
    }
    if let [(t, v)] = pairs.remainder() {
        out.add(*t, *v);
    }
    for &(t, v) in &input.direct_secondary {
        out.add(t, v);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use DamageType::*;

    #[test]
    fn two_primaries_merge_into_their_secondary() {
        // Cold(1) + Electricity(2) -> Magnetic, both amounts pooled.
        let mut input = ElementalInput::default();
        input.push(Cold, 240.0);
        input.push(Electricity, 240.0);
        let out = combine(&DamageVector::new().with(Impact, 100.0), &input);
        assert_eq!(out.get(Magnetic), 480.0);
        assert_eq!(out.get(Cold), 0.0);
        assert_eq!(out.get(Impact), 100.0);
    }

    #[test]
    fn odd_third_element_stays_pure() {
        // Cold(1) Toxin(2) Heat(3) -> Viral + pure Heat (the Load Order
        // worked example, minus Prova's innate Electricity).
        let mut input = ElementalInput::default();
        input.push(Cold, 10.0);
        input.push(Toxin, 20.0);
        input.push(Heat, 30.0);
        let out = combine(&DamageVector::new(), &input);
        assert_eq!(out.get(Viral), 30.0);
        assert_eq!(out.get(Heat), 30.0);
    }

    #[test]
    fn four_elements_form_two_pairs_in_order() {
        // Heat(1) Toxin(2) Cold(3) Electricity(4) -> Gas + Magnetic.
        let mut input = ElementalInput::default();
        input.push(Heat, 1.0);
        input.push(Toxin, 2.0);
        input.push(Cold, 4.0);
        input.push(Electricity, 8.0);
        let out = combine(&DamageVector::new(), &input);
        assert_eq!(out.get(Gas), 3.0);
        assert_eq!(out.get(Magnetic), 12.0);
    }

    #[test]
    fn same_element_mods_merge_at_first_placement() {
        // Rule 4: Frostbite(1) Toxin(2) DeepFreeze(3): Deep Freeze's Cold
        // joins slot 1's entry -> Viral gets ALL the cold, nothing trails.
        let mut input = ElementalInput::default();
        input.push(Cold, 240.0);
        input.push(Toxin, 240.0);
        input.push(Cold, 360.0);
        let out = combine(&DamageVector::new(), &input);
        assert_eq!(out.get(Viral), 840.0);
        assert_eq!(out.get(Cold), 0.0);
    }

    #[test]
    fn injected_toxin_appends_last_or_joins_existing() {
        // Frenzy +100% Toxin with a lone Heat mod -> Gas (rule 8).
        let mut input = ElementalInput::default();
        input.push(Heat, 100.0);
        input.injected.push((Toxin, 75.0));
        let out = combine(&DamageVector::new(), &input);
        assert_eq!(out.get(Gas), 175.0);

        // With Cold(1)+Toxin(2) already forming Viral, the injection joins
        // Toxin's established position instead of trailing pure.
        let mut input2 = ElementalInput::default();
        input2.push(Cold, 50.0);
        input2.push(Toxin, 50.0);
        input2.injected.push((Toxin, 75.0));
        let out2 = combine(&DamageVector::new(), &input2);
        assert_eq!(out2.get(Viral), 175.0);
        assert_eq!(out2.get(Toxin), 0.0);
    }

    #[test]
    fn combined_element_mods_bypass_the_hierarchy() {
        // Magnetic Might: its Magnetic never pairs with primaries (rule 7).
        let mut input = ElementalInput::default();
        input.push(Heat, 100.0);
        input.direct_secondary.push((Magnetic, 75.0));
        let out = combine(&DamageVector::new(), &input);
        assert_eq!(out.get(Heat), 100.0);
        assert_eq!(out.get(Magnetic), 75.0);
    }
}
