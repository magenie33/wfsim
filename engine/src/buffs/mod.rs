//! Buffs: runtime states overlaid on a target, mirroring the in-game buff bar.
//!
//! A [`Buff`] is a live overlay — it has stacks, an optional expiry, a
//! [`BuffScope`] (what it applies to), and the [`Contributions`] it currently
//! grants. The [`BuffBar`] is the single place that holds every active buff,
//! exactly like the player's HUD buff bar (a buff appears there regardless of
//! whether its scope is the weapon, the Warframe, or the squad).
//!
//! A buff is granted by a [`crate::perks::Perk`] — an arcane, a weapon passive,
//! an Incarnon evolution — that you hold. On a trigger event the perk applies /
//! refreshes / resets a buff in the bar.
//!
//! The pure damage pipeline never mutates buffs — it reads a summed
//! [`Contributions`] snapshot from the bar. See `docs/BUFFS.md`.

/// What a buff applies to. The HUD shows all buffs regardless of scope; scope
/// decides where the buff's contributions are actually applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuffScope {
    /// Applies to the specific weapon that granted it (e.g. Frenzy, Secondary
    /// Enervate). Weapon identity will be added when multi-weapon builds land.
    Weapon,
    /// Applies to the Warframe / player.
    Warframe,
    /// Applies squad-wide.
    Squad,
}

/// What a buff adds to the modifier buckets.
///
/// One field per **additive** bucket the buff layer can touch. Values are summed
/// across buffs. Multiplicative buckets (e.g. an independent fire-rate
/// multiplier) are intentionally *not* here yet — they must be combined by the
/// mod-resolution layer, not naively summed. Terminology: `docs/GLOSSARY.md`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Contributions {
    /// Flat crit chance: an absolute percentage-point bonus (0.10 == +10 pts),
    /// not scaled by base. See `docs/GLOSSARY.md`.
    pub flat_critical_chance: f64,
}

impl std::ops::Add for Contributions {
    type Output = Contributions;
    fn add(self, other: Contributions) -> Contributions {
        Contributions {
            flat_critical_chance: self.flat_critical_chance + other.flat_critical_chance,
        }
    }
}

impl std::iter::Sum for Contributions {
    fn sum<I: Iterator<Item = Contributions>>(iter: I) -> Contributions {
        iter.fold(Contributions::default(), |acc, c| acc + c)
    }
}

/// A live buff overlaid on a target — one entry in the [`BuffBar`].
#[derive(Debug, Clone, PartialEq)]
pub struct Buff {
    /// Stable id, e.g. `"secondary_enervate"`, `"frenzy"`.
    pub id: String,
    pub scope: BuffScope,
    /// Number of stacks (>= 1 while the buff is present).
    pub stacks: u32,
    /// Absolute time in seconds when the buff expires, or `None` for a buff with
    /// no time limit (persists until reset/removed, like Secondary Enervate).
    pub expiry_secs: Option<f64>,
    /// The buff's current total contribution to the modifier buckets.
    pub contributions: Contributions,
}

/// The central container of active buffs — the model's buff bar / HUD mirror.
#[derive(Debug, Clone, Default)]
pub struct BuffBar {
    buffs: Vec<Buff>,
}

impl BuffBar {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a buff, replacing any existing buff with the same id.
    pub fn upsert(&mut self, buff: Buff) {
        match self.buffs.iter_mut().find(|b| b.id == buff.id) {
            Some(existing) => *existing = buff,
            None => self.buffs.push(buff),
        }
    }

    pub fn get(&self, id: &str) -> Option<&Buff> {
        self.buffs.iter().find(|b| b.id == id)
    }

    /// Remove a buff by id, if present.
    pub fn remove(&mut self, id: &str) {
        self.buffs.retain(|b| b.id != id);
    }

    /// Drop every buff whose expiry is at or before `t_secs`.
    pub fn expire(&mut self, t_secs: f64) {
        self.buffs
            .retain(|b| b.expiry_secs.is_none_or(|e| e > t_secs));
    }

    /// All active buffs, for display (the UI reads this).
    pub fn active(&self) -> &[Buff] {
        &self.buffs
    }

    /// Summed contributions of every active buff.
    ///
    /// Scope-aware application (weapon vs Warframe vs squad) will refine this
    /// once non-weapon buffs exist; today every buff is `Weapon`-scoped.
    pub fn total_contributions(&self) -> Contributions {
        self.buffs.iter().map(|b| b.contributions).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expiry_drops_lapsed_buffs() {
        // A duration-based buff like Frenzy: present until its expiry passes.
        let mut bar = BuffBar::new();
        bar.upsert(Buff {
            id: "frenzy".into(),
            scope: BuffScope::Weapon,
            stacks: 1,
            expiry_secs: Some(3.0),
            contributions: Contributions::default(),
        });
        bar.expire(2.9);
        assert!(bar.get("frenzy").is_some());
        bar.expire(3.0);
        assert!(bar.get("frenzy").is_none());
    }

    #[test]
    fn upsert_replaces_same_id() {
        let mut bar = BuffBar::new();
        let mk = |stacks| Buff {
            id: "x".into(),
            scope: BuffScope::Weapon,
            stacks,
            expiry_secs: None,
            contributions: Contributions::default(),
        };
        bar.upsert(mk(1));
        bar.upsert(mk(2));
        assert_eq!(bar.active().len(), 1);
        assert_eq!(bar.get("x").unwrap().stacks, 2);
    }
}
