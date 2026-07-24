//! Mod capacity & polarity math — the gate in front of mod resolution
//! (pipeline layer [1]). Source: wiki `Polarity` (docs/MECHANICS.md §2).
//!
//! - Matching slot polarity: drain **−50%, rounded UP** (11 → 6).
//! - Mismatched polarity: drain **+25%, rounded to the nearest integer**
//!   (11 → 13.75 → 14). Exact half-rounding direction unverified — flagged.
//! - Unpolarized slot: full drain.
//! - Capacity = weapon rank (max 30), doubled by an Orokin Catalyst → 60.
//!   (Aura/Stance capacity-bonus polarities are a separate rule, not yet
//!   needed for guns.)

/// Mod/slot polarity (wiki `Polarity`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    Madurai,
    Naramon,
    Vazarin,
    Zenurik,
    Unairu,
    Penjaga,
    Umbra,
}

/// Effective drain of a mod in a slot.
pub fn slot_drain(base_drain: u32, mod_polarity: Polarity, slot_polarity: Option<Polarity>) -> u32 {
    match slot_polarity {
        Some(p) if p == mod_polarity => base_drain.div_ceil(2), // −50%, round up
        Some(_) => {
            // +25%, rounded to nearest (half-up assumed — unverified edge).
            ((base_drain as f64) * 1.25).round() as u32
        }
        None => base_drain,
    }
}

/// Weapon mod capacity: rank, doubled by an Orokin Catalyst/Reactor.
pub fn capacity(rank: u32, catalyst: bool) -> u32 {
    if catalyst {
        rank * 2
    } else {
        rank
    }
}

/// One equipped mod placed in a slot.
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    pub base_drain: u32,
    pub mod_polarity: Polarity,
    pub slot_polarity: Option<Polarity>,
}

/// Validate a loadout against capacity. Returns the total drain, or an
/// error naming the overflow (rigor rule: an over-capacity build is
/// impossible in-game and must be rejected, never silently accepted).
pub fn validate_loadout(cap: u32, placements: &[Placement]) -> Result<u32, String> {
    let used: u32 = placements
        .iter()
        .map(|p| slot_drain(p.base_drain, p.mod_polarity, p.slot_polarity))
        .sum();
    if used > cap {
        Err(format!(
            "loadout uses {used} capacity, exceeding the {cap} cap"
        ))
    } else {
        Ok(used)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matched_polarity_halves_rounding_up() {
        assert_eq!(
            slot_drain(11, Polarity::Madurai, Some(Polarity::Madurai)),
            6
        );
        assert_eq!(
            slot_drain(10, Polarity::Naramon, Some(Polarity::Naramon)),
            5
        );
        assert_eq!(slot_drain(7, Polarity::Vazarin, Some(Polarity::Vazarin)), 4);
    }

    #[test]
    fn mismatched_polarity_adds_a_quarter() {
        assert_eq!(
            slot_drain(11, Polarity::Madurai, Some(Polarity::Naramon)),
            14
        ); // 13.75
        assert_eq!(
            slot_drain(16, Polarity::Madurai, Some(Polarity::Vazarin)),
            20
        );
        assert_eq!(slot_drain(9, Polarity::Umbra, Some(Polarity::Madurai)), 11);
        // 11.25
    }

    #[test]
    fn unpolarized_slot_charges_full_drain() {
        assert_eq!(slot_drain(11, Polarity::Madurai, None), 11);
    }

    #[test]
    fn capacity_doubles_with_a_catalyst() {
        assert_eq!(capacity(30, false), 30);
        assert_eq!(capacity(30, true), 60);
    }

    #[test]
    fn loadout_validation_enforces_the_cap() {
        // Dual Toxocyst fully unlocked: rank 30 + catalyst = 60 capacity;
        // innate Madurai + Naramon slots, Naramon exilus.
        let cap = capacity(30, true);
        let ok = [
            // e.g. a rank-10 Madurai mod (drain 14) in the Madurai slot -> 7
            Placement {
                base_drain: 14,
                mod_polarity: Polarity::Madurai,
                slot_polarity: Some(Polarity::Madurai),
            },
            // rank-10 Naramon mod in the Naramon slot -> 6
            Placement {
                base_drain: 11,
                mod_polarity: Polarity::Naramon,
                slot_polarity: Some(Polarity::Naramon),
            },
            // unpolarized slots at full drain
            Placement {
                base_drain: 11,
                mod_polarity: Polarity::Madurai,
                slot_polarity: None,
            },
            Placement {
                base_drain: 9,
                mod_polarity: Polarity::Vazarin,
                slot_polarity: None,
            },
        ];
        assert_eq!(validate_loadout(cap, &ok), Ok(7 + 6 + 11 + 9));

        // Cramming mismatches until it bursts is an error, not a warning:
        // 16-drain mods in wrong-polarity slots cost 20 each.
        let mismatch = Placement {
            base_drain: 16,
            mod_polarity: Polarity::Madurai,
            slot_polarity: Some(Polarity::Vazarin),
        };
        assert_eq!(validate_loadout(60, &[mismatch; 3]), Ok(60)); // exactly full
        assert!(validate_loadout(60, &[mismatch; 4]).is_err()); // 80 > 60
    }
}
