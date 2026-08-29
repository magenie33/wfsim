//! WHAT FALLS OFF A BODY, and what a weapon can do with it.
//!
//! The first mechanic in this engine that reads a KILL as a resource rather
//! than as an end: the Grimoire's meter is refilled by picking ammo up, so
//! whether the last enemy dropped any is a term in that weapon's fire rate.
//!
//! IT NEEDS NO PER-ENEMY DROP TABLE, which is the finding that made it
//! tractable at all. Ammo is not on an enemy's own table the way
//! a resource is — the chance is a property of the SQUAD and the place:
//!
//! > *"Chance to drop Primary or Secondary Ammo scales with squad size"* —
//! > solo 45% (60% in Landscapes), 2 players 37.5% (52.5%), 3 players 30%
//! > (45%), 4 players 22.5% (37.5%). *"For most enemies, each roll of their
//! > drop table will only result in a maximum of one Ammo Pickup."*
//! > (wiki `Pickups`)
//!
//! So a Grineer Lancer and a Corpus Crewman drop ammo at the same rate, and the
//! ten enemies this roster carries need nothing added to them. What DOES belong
//! on an enemy is the one thing the page makes an enemy's own:
//!
//! > *"Eximus are guaranteed to drop either a Primary or Secondary Ammo, each
//! > having the same chance of dropping. This does not overwrite the enemies
//! > normal chance of dropping an Ammo pickup."*
//!
//! ADDITIONAL, not instead — so an Eximus rolls the ordinary chance AND drops
//! one for certain, and this engine already knows which bodies are Eximus.
//!
//! WHAT IS NOT HERE, and will not be until it is measured:
//!
//! * **Health and energy orbs.** The same page lists them (50 Health, 25 or 50
//!   Energy) and publishes no drop chance for either. They would also pay
//!   nothing today: this arena has no ability economy for energy to feed and
//!   the player has no health to restore.
//! * **Resources.** A per-enemy table, published per enemy, and it feeds none
//!   of BUILD, SIMULATE or SOLVE — a farming calculator is a different product
//!   (AGENTS.md: anything new either feeds one of the three or reports from
//!   one).
//! * **Heavy ammo**, which is the one ammo kind that IS per enemy (specific
//!   heavy units at 5.01%). No Arch-Gun in this roster reads a pickup yet.

/// Which ammo a pickup turned out to be.
///
/// The split matters because a weapon may care about one kind and not the
/// other: the Grimoire's meter reads *"secondary or universal ammo"* and a
/// primary pickup does nothing for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pickup {
    Primary,
    Secondary,
}

/// Chance one ordinary body drops an ammo pickup of EITHER kind, by squad size.
///
/// Indexed by `squad_size - 1`, so `[0]` is solo. Verbatim from the table
/// quoted above; the landscape column is [`DROP_CHANCE_LANDSCAPE`].
pub const DROP_CHANCE: [f64; 4] = [0.45, 0.375, 0.30, 0.225];

/// …and the same table in a LANDSCAPE, where every rate is higher.
pub const DROP_CHANCE_LANDSCAPE: [f64; 4] = [0.60, 0.525, 0.45, 0.375];

/// The share of ammo pickups that are SECONDARY rather than primary.
///
/// STATED for the Eximus guarantee — *"each having the same chance of
/// dropping"* — and ASSUMED for the ordinary roll, which the page gives as one
/// number for "Primary or Secondary" without splitting it. Half is the reading
/// the Eximus sentence supports and the only one that invents no preference;
/// it is a constant with a name rather than a bare `0.5` so that when somebody
/// measures the real split there is one place to put it.
pub const SECONDARY_SHARE: f64 = 0.5;

/// WHAT ONE BODY DROPS, rolled.
///
/// `rng` is one uniform draw for the ordinary chance and a second for the
/// kind, plus the same pair again for an Eximus's guaranteed drop — which is
/// ADDITIONAL to the ordinary roll rather than instead of it, so an Eximus can
/// leave two pickups.
///
/// Returns how many of them are of each kind, because a caller reading only
/// one kind still has to know the other fell (a future weapon may read both).
pub fn on_kill(
    squad_size: u32,
    landscape: bool,
    eximus: bool,
    rng: &mut crate::rng::Rng,
) -> (u32, u32) {
    let table = if landscape { &DROP_CHANCE_LANDSCAPE } else { &DROP_CHANCE };
    let chance = table[(squad_size.clamp(1, 4) - 1) as usize];
    let (mut primary, mut secondary) = (0, 0);
    let drop = |rng: &mut crate::rng::Rng, primary: &mut u32, secondary: &mut u32| {
        if rng.next_f64() < SECONDARY_SHARE {
            *secondary += 1;
        } else {
            *primary += 1;
        }
    };
    if rng.next_f64() < chance {
        drop(rng, &mut primary, &mut secondary);
    }
    if eximus {
        drop(rng, &mut primary, &mut secondary);
    }
    (primary, secondary)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE PUBLISHED TABLE, transcribed — and the direction that makes it
    /// checkable: every landscape rate is higher than its own squad's, and
    /// every rate falls as the squad grows. A transposed column would pass a
    /// spot check on one cell and fail both of these.
    #[test]
    fn the_drop_table_is_the_published_one() {
        assert!((DROP_CHANCE[0] - 0.45).abs() < 1e-12, "solo is 45%");
        assert!((DROP_CHANCE_LANDSCAPE[0] - 0.60).abs() < 1e-12, "…and 60% outdoors");
        for i in 0..4 {
            assert!(
                DROP_CHANCE_LANDSCAPE[i] > DROP_CHANCE[i],
                "a landscape drops more at squad {}", i + 1
            );
        }
        assert!(DROP_CHANCE.windows(2).all(|w| w[0] > w[1]), "a bigger squad drops less");
        assert!(DROP_CHANCE_LANDSCAPE.windows(2).all(|w| w[0] > w[1]));
        // …and the four-player rate indoors is the solo rate outdoors' opposite
        // number, which is the one coincidence in the table and is worth
        // pinning so a paste of the wrong column is visible.
        assert!((DROP_CHANCE[3] - 0.225).abs() < 1e-12);
    }

    /// AN EXIMUS DROPS MORE, AND IT IS ADDITIONAL. *"This does not overwrite
    /// the enemies normal chance"*, so its expected pickups are the ordinary
    /// chance PLUS one rather than one flat — 1.45 solo against 0.45.
    ///
    /// Asserted as a mean over many rolls rather than on a single kill, because
    /// the ordinary half is a coin and a single roll says nothing.
    #[test]
    fn an_eximus_drop_is_additional_to_the_ordinary_roll() {
        let mean = |eximus: bool| {
            let mut rng = crate::rng::Rng::new(0x5EED);
            let n = 200_000;
            let total: u32 = (0..n)
                .map(|_| {
                    let (p, s) = on_kill(1, false, eximus, &mut rng);
                    p + s
                })
                .sum();
            f64::from(total) / f64::from(n)
        };
        let ordinary = mean(false);
        let eximus = mean(true);
        assert!((ordinary - 0.45).abs() < 0.01, "solo drops 0.45 an ordinary kill: {ordinary}");
        assert!((eximus - 1.45).abs() < 0.01, "and an Eximus 1.45: {eximus}");
    }

    /// HALF OF THEM ARE SECONDARY, which is what a weapon reading one kind
    /// depends on. Stated for the Eximus guarantee and assumed for the roll;
    /// pinned here so the assumption is one number in one place.
    #[test]
    fn half_of_what_falls_is_secondary() {
        let mut rng = crate::rng::Rng::new(7);
        let n = 200_000;
        let (mut p, mut s) = (0u32, 0u32);
        for _ in 0..n {
            let (a, b) = on_kill(1, false, false, &mut rng);
            p += a;
            s += b;
        }
        let share = f64::from(s) / f64::from(p + s);
        assert!((share - 0.5).abs() < 0.01, "half: {share}");
    }
}
