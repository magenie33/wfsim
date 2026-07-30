//! Pipeline layer [2]: elemental combination.
//!
//! Implements the hierarchy algorithm of docs/MECHANICS.md §3 (wiki `Damage`
//! §Modding/§Load Order) for the cases the engine currently needs: a weapon
//! with a **purely physical innate vector** (Dual Toxocyst) plus mod-added
//! primaries, combined-element mods, buff-injected elements, and weapons whose
//! innate damage is itself elemental (Torid's Toxin).

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
    /// Weapon-innate primaries (Torid's Toxin, Verglas Prime's Cold). Rule 2:
    /// these come **LAST** in the hierarchy, not first — the mods combine
    /// among themselves and the innate takes what is left over. Rule 3 pulls
    /// one forward when a mod shares its element.
    ///
    /// Kept apart from `ordered` rather than pushed into it, because "last"
    /// is not a position the caller can know: it depends on how many mod
    /// elements there turn out to be.
    pub innate: Vec<(DamageType, f64)>,
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

/// Hierarchy rank of the innate elements among themselves — Kuva/Tenet
/// weapons carry two (weapon + progenitor) and rule 2 breaks the tie by
/// **HCET**: whichever comes first here sits second-to-last, the other last.
fn hcet(t: DamageType) -> u8 {
    match t {
        DamageType::Heat => 0,
        DamageType::Cold => 1,
        DamageType::Electricity => 2,
        DamageType::Toxin => 3,
        _ => 4,
    }
}

/// Combine the physical vector with the elemental hierarchy.
///
/// Adjacent uncombined primaries merge pairwise into secondaries (rule 1);
/// an odd trailing primary stays pure. Then the weapon's own innate elements
/// (rule 2: LAST) and finally injected elements (rule 8), each merging into
/// its element's existing position if one exists, else appended.
pub fn combine(physical: &DamageVector, input: &ElementalInput) -> DamageVector {
    let mut order = input.ordered.clone();
    // Rule 2: innate elements go at the END of the mod order. Rule 3: unless a
    // mod already placed that element, in which case the innate is pulled
    // FORWARD onto the mod's position and pools there (Stormbringer in slot 1
    // moves Amprex's innate Electricity to first).
    let mut innate = input.innate.clone();
    innate.sort_by_key(|&(t, _)| hcet(t));
    for (t, amount) in innate {
        match order.iter_mut().find(|(x, _)| *x == t) {
            Some(e) => e.1 += amount,
            None => order.push((t, amount)),
        }
    }
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

    /// MECHANICS §3 rule 2, and the doc's own worked example: Prova/Lecta
    /// (innate Electricity) + Cold(1) Toxin(2) Heat(3) -> Viral + Radiation.
    ///
    /// Placing the innate FIRST instead — which is what the engine used to do
    /// — gives Magnetic + Gas, a different build entirely. One mod element
    /// cannot tell the two apart; two can, which is why this is the test that
    /// matters.
    #[test]
    fn innate_elements_go_last_not_first() {
        let mut input = ElementalInput::default();
        input.push(Cold, 10.0);
        input.push(Toxin, 20.0);
        input.push(Heat, 30.0);
        input.innate.push((Electricity, 40.0));
        let out = combine(&DamageVector::new(), &input);
        assert_eq!(out.get(Viral), 30.0, "Cold(1)+Toxin(2)");
        assert_eq!(out.get(Radiation), 70.0, "Heat(3)+innate Electricity LAST");
        assert_eq!(out.get(Magnetic), 0.0, "innate first would have made Magnetic");
    }

    /// Rule 3: a mod sharing the innate's element pulls it FORWARD to that
    /// mod's position, where the two pool.
    #[test]
    fn a_mod_of_the_same_element_pulls_the_innate_forward() {
        let mut input = ElementalInput::default();
        input.push(Electricity, 10.0); // Stormbringer in slot 1
        input.push(Heat, 20.0);
        input.innate.push((Electricity, 40.0)); // Amprex's own Electricity
        let out = combine(&DamageVector::new(), &input);
        assert_eq!(out.get(Radiation), 70.0, "innate pooled into slot 1, then paired with Heat");
        assert_eq!(out.get(Electricity), 0.0);
    }

    /// Rule 2's exception: a Kuva/Tenet weapon carries two innates (weapon +
    /// progenitor) and HCET (Heat > Cold > Electricity > Toxin) decides which
    /// of them sits second-to-last.
    #[test]
    fn two_innates_order_by_hcet() {
        let mut input = ElementalInput::default();
        input.push(Heat, 10.0);
        // Declared Toxin-first; HCET must still put Cold ahead of it.
        input.innate.push((Toxin, 20.0));
        input.innate.push((Cold, 30.0));
        let out = combine(&DamageVector::new(), &input);
        assert_eq!(out.get(Blast), 40.0, "Heat(mod) + Cold(innate, HCET-first)");
        assert_eq!(out.get(Toxin), 20.0, "Toxin is the odd one out");
        assert_eq!(out.get(Gas), 0.0);
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
