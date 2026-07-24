//! Mod capacity & polarity math — the gate in front of mod resolution
//! (pipeline layer [1]). Source: wiki `Polarity` (docs/MECHANICS.md §2).
//!
//! - Matching slot polarity: drain **−50%, rounded UP** (11 → 6).
//! - Mismatched polarity: drain **+25%, rounded half-UP** (11 → 13.75 → 14;
//!   MEASURED 2026-07-24, user: 10 → 12.5 → **13**).
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
            // +25%, rounded half-up (user-measured: 10 -> 13). f64::round
            // rounds half away from zero, which matches.
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

/// A mod to be fitted by the forma planner.
#[derive(Debug, Clone, Copy)]
pub struct PlannedMod {
    pub base_drain: u32,
    pub polarity: Polarity,
}

/// Result of forma planning.
#[derive(Debug, Clone)]
pub struct FormaPlan {
    /// Polarity on each slot after planning (index-aligned with mods; the
    /// mod at index i sits in slot i). `None` = blank slot.
    pub slots: Vec<Option<Polarity>>,
    pub forma_used: u32,
    pub total_drain: u32,
}

/// Auto-forma: fit one mod per slot into `cap`, starting from the weapon's
/// innate polarity pool, using as few Forma as possible. Mismatches are
/// never beneficial (blanks are strictly better), so the planner only ever
/// matches or leaves blank; innate polarities can be freely rearranged
/// among slots (Forma allows repositioning), so they form a POOL.
pub fn plan_forma(
    cap: u32,
    innate_slots: &[Option<Polarity>],
    mods: &[PlannedMod],
) -> Result<FormaPlan, String> {
    assert!(mods.len() <= innate_slots.len(), "more mods than slots");
    let mut matched = vec![false; mods.len()];

    // Biggest-drain mods first for every greedy choice.
    let mut order: Vec<usize> = (0..mods.len()).collect();
    order.sort_by(|&a, &b| mods[b].base_drain.cmp(&mods[a].base_drain));

    // 1. Spend the innate polarity pool on the biggest matching mods.
    let mut pool: Vec<Polarity> = innate_slots.iter().flatten().copied().collect();
    for &i in &order {
        if let Some(pos) = pool.iter().position(|&p| p == mods[i].polarity) {
            pool.remove(pos);
            matched[i] = true;
        }
    }

    let drain = |matched: &[bool]| -> u32 {
        mods.iter()
            .zip(matched)
            .map(|(m, &ok)| {
                if ok {
                    m.base_drain.div_ceil(2)
                } else {
                    m.base_drain
                }
            })
            .sum()
    };

    // 2. Forma the biggest unmatched mod until the build fits.
    let mut forma_used = 0u32;
    while drain(&matched) > cap {
        let Some(&next) = order.iter().find(|&&i| !matched[i]) else {
            return Err(format!(
                "build needs {} capacity even fully forma'd (cap {cap})",
                drain(&matched)
            ));
        };
        matched[next] = true;
        forma_used += 1;
    }

    let slots = mods
        .iter()
        .zip(&matched)
        .map(|(m, &ok)| if ok { Some(m.polarity) } else { None })
        .collect();
    Ok(FormaPlan {
        slots,
        forma_used,
        total_drain: drain(&matched),
    })
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
        // MEASURED (2026-07-24, user): the half case rounds UP.
        assert_eq!(
            slot_drain(10, Polarity::Madurai, Some(Polarity::Naramon)),
            13
        ); // 12.5 -> 13
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

    #[test]
    fn auto_forma_fits_the_proposed_dt_build_with_four_forma() {
        // Dual Toxocyst: innate pool [Madurai, Naramon], 8 slots, cap 60.
        // Proposed 8: Hornet 14M, PTC 14M, GalvDiffusion 14M, PPG 12M,
        // GalvShot 12V, Lethal Torrent 11M, Frostbite 7M, Jolt 7M.
        let m = |d, p| PlannedMod {
            base_drain: d,
            polarity: p,
        };
        use Polarity::*;
        let mods = [
            m(14, Madurai),
            m(14, Madurai),
            m(14, Madurai),
            m(12, Madurai),
            m(12, Vazarin),
            m(11, Madurai),
            m(7, Madurai),
            m(7, Madurai),
        ];
        let innate = [
            Some(Madurai),
            Some(Naramon),
            None,
            None,
            None,
            None,
            None,
            None,
        ];
        let plan = plan_forma(60, &innate, &mods).unwrap();
        // Innate Madurai halves one 14 (7); the Naramon polarity finds no
        // taker. Forma greedily: 14->7, 14->7, 12->6, 12->6 = 4 Forma.
        // Total: 7+7+7+6+6+11+7+7 = 58 <= 60.
        assert_eq!(plan.forma_used, 4, "plan: {plan:?}");
        assert_eq!(plan.total_drain, 58);
    }

    #[test]
    fn auto_forma_rejects_impossible_builds() {
        let m = PlannedMod {
            base_drain: 16,
            polarity: Polarity::Madurai,
        };
        // Eight 16-drain mods fully forma'd still need 8 x 8 = 64 > 60.
        let innate = [None; 8];
        assert!(plan_forma(60, &innate, &[m; 8]).is_err());
    }
}
