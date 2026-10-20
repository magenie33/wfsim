// SPDX-License-Identifier: AGPL-3.0-or-later
//! WHICH OF TWO BUILDS THAT SCORE AS ONE IS THE ANSWER (docs/OPTIMIZER.md,
//! "A tie keeps the preferred build"). The search's step, its list, the
//! finder's and the board's representative of a tie and the bots' short list
//! all ask this one order; the page's twin is `preferredBuild`
//! (`87-headless.js`).
//!
//! More mod drain first: a card that costs more is the stronger card where
//! the fight reads neither, and a tie that keeps the cheaper keeps a weaker
//! build for no reason the fight can see. Then the cards' English names, the
//! arcanes' English names, and each evolution tier's own order — the
//! wiki's, its default first.

use std::cmp::Ordering;

/// What the order reads of a build. `cards` and `arcanes` are English names
/// in any order; `evolutions` is each tier's `position_in_tier`, in tier order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Preference<'a> {
    /// The mods' drain at the ranks they are equipped at, the exilus and a
    /// riven included, before any polarity.
    pub drain: u32,
    pub cards: Vec<&'a str>,
    pub arcanes: Vec<&'a str>,
    pub evolutions: Vec<u32>,
}

/// `Less` = `a` is the one kept.
pub fn preferred(a: &Preference, b: &Preference) -> Ordering {
    let sorted = |x: &[&str]| {
        let mut v = x.to_vec();
        v.sort_unstable();
        v.into_iter().map(str::to_string).collect::<Vec<_>>()
    };
    b.drain
        .cmp(&a.drain)
        .then_with(|| sorted(&a.cards).cmp(&sorted(&b.cards)))
        .then_with(|| sorted(&a.arcanes).cmp(&sorted(&b.arcanes)))
        .then_with(|| a.evolutions.cmp(&b.evolutions))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p<'a>(drain: u32, cards: &[&'a str], arcanes: &[&'a str], evolutions: &[u32]) -> Preference<'a> {
        Preference { drain, cards: cards.to_vec(), arcanes: arcanes.to_vec(), evolutions: evolutions.to_vec() }
    }

    #[test]
    fn more_drain_then_names_then_the_tier_order() {
        let heavy = p(16, &["Vigilante Armaments"], &[], &[]);
        let light = p(10, &["Ammo Drum"], &[], &[]);
        assert_eq!(preferred(&heavy, &light), Ordering::Less, "the costlier card is kept");
        let a = p(10, &["Ammo Drum"], &[], &[]);
        let b = p(10, &["Hush"], &[], &[]);
        assert_eq!(preferred(&a, &b), Ordering::Less, "at one drain, the first name");
        let x = p(10, &["Hush"], &["Primary Deadhead"], &[1, 2]);
        let y = p(10, &["Hush"], &["Primary Merciless"], &[1, 1]);
        assert_eq!(preferred(&x, &y), Ordering::Less, "then the arcane's name, before any evolution");
        let e1 = p(10, &["Hush"], &[], &[1, 1]);
        let e2 = p(10, &["Hush"], &[], &[1, 3]);
        assert_eq!(preferred(&e1, &e2), Ordering::Less, "then each tier's own order");
        assert_eq!(preferred(&e1, &e1), Ordering::Equal);
    }

    #[test]
    fn the_cards_are_a_set() {
        let a = p(10, &["Hush", "Ammo Drum"], &[], &[]);
        let b = p(10, &["Ammo Drum", "Hush"], &[], &[]);
        assert_eq!(preferred(&a, &b), Ordering::Equal);
    }
}
