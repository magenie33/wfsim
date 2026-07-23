//! Event-driven stateful effects (arcanes, conditional mods, combo).
//!
//! Each effect is a small, isolated state machine. It reacts to timeline
//! [`crate::sim::Event`]s, keeps its own local state (stacks, timers,
//! cooldowns), and reports its current [`Contributions`] to the modifier
//! buckets. The pure damage pipeline reads a snapshot of the summed
//! contributions; it never mutates effect state. See `docs/EFFECTS.md`.
//!
//! Most effects reduce to a small set of primitives and will eventually be
//! data-driven through a single interpreter. Complex outliers get a hand-written
//! `impl Effect` behind the same trait — the pipeline can't tell the difference.

pub mod secondary_enervate;

/// What an effect currently adds to the modifier buckets.
///
/// One field per bucket the effects layer can touch. Extended as more effects
/// land (damage, status chance, fire rate, faction damage, ...). Values are
/// additive within their bucket; the mod-resolution layer combines buckets.
/// Terminology follows `docs/GLOSSARY.md`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Contributions {
    /// Flat crit chance: an absolute percentage-point bonus added to the final
    /// critical chance (0.10 == +10 points), *not* scaled by base. Distinct
    /// from a crit chance multiplier. See `docs/GLOSSARY.md`.
    pub flat_critical_chance: f64,
}

impl std::ops::Add for Contributions {
    type Output = Contributions;

    /// Sum two contribution sets bucket-by-bucket.
    fn add(self, other: Contributions) -> Contributions {
        Contributions {
            flat_critical_chance: self.flat_critical_chance + other.flat_critical_chance,
        }
    }
}

impl std::iter::Sum for Contributions {
    /// Sum every active effect's contribution into one snapshot.
    fn sum<I: Iterator<Item = Contributions>>(iter: I) -> Contributions {
        iter.fold(Contributions::default(), |acc, c| acc + c)
    }
}

/// A stateful modifier that lives on the timeline.
pub trait Effect {
    /// Stable identifier, e.g. `"secondary_enervate"`.
    fn id(&self) -> &str;

    /// React to a timeline event at time `t_secs` (seconds since run start).
    fn on_event(&mut self, event: &crate::sim::Event, t_secs: f64);

    /// The effect's current contribution to the modifier buckets.
    fn contributions(&self) -> Contributions;
}
