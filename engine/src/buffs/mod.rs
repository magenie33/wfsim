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
///
/// We can model this **more finely than the in-game display** — the game often
/// collapses these into one buff icon, but we track exactly which entity a buff
/// affects. Extend this enum as finer targets are needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuffScope {
    /// Applies to the specific weapon that granted it (e.g. Frenzy, Secondary
    /// Enervate). Weapon identity will be added when multi-weapon builds land.
    Weapon,
    /// Applies to the Warframe / player.
    Warframe,
    /// Applies to the companion (pet / sentinel).
    Companion,
    /// Applies to the companion's weapon.
    CompanionWeapon,
    /// Applies squad-wide.
    Squad,
}

/// Where a buff injects an element into the mod order (see `docs/GLOSSARY.md`,
/// "Injected mod", and `docs/MECHANICS.md` §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionPosition {
    /// Appended as the last mod, so it combines elementally after everything
    /// else (e.g. Frenzy's +100% Toxin).
    EndOfModOrder,
}

/// An element a buff adds as if it were an elemental mod at a defined position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InjectedElement {
    pub damage_type: crate::damage::DamageType,
    /// Amount as a fraction of the weapon's base damage (1.0 == +100%).
    pub amount_percent_of_base: f64,
    pub position: InjectionPosition,
}

/// What a buff adds to the modifier buckets.
///
/// Each field is a distinct bucket with its **own** combination rule when buffs
/// are combined (see [`Contributions::combine`]): additive buckets sum,
/// multiplicative buckets multiply, injected elements concatenate. Terminology:
/// `docs/GLOSSARY.md`.
#[derive(Debug, Clone, PartialEq)]
pub struct Contributions {
    /// Additive: flat crit chance, an absolute percentage-point bonus
    /// (0.10 == +10 pts), not scaled by base.
    pub flat_crit_chance: f64,
    /// Multiplicative: an independent fire-rate multiplier (Frenzy's x2.5),
    /// applied on its own bucket after additive fire-rate mods. Identity 1.0.
    pub fire_rate_multiplier: f64,
    /// Additive: ammo-efficiency bonus `e`; shots_per_ammo = 1/(1-e), 1.0 =>
    /// infinite ammo. (Energized Munitions is the multiplicative exception, not
    /// modeled here.)
    pub ammo_efficiency: f64,
    /// Elements injected into the mod order (order preserved).
    pub injected_elements: Vec<InjectedElement>,
}

impl Default for Contributions {
    fn default() -> Self {
        // Identities: 0 for additive buckets, 1.0 for the multiplicative bucket,
        // empty for injected elements.
        Self {
            flat_crit_chance: 0.0,
            fire_rate_multiplier: 1.0,
            ammo_efficiency: 0.0,
            injected_elements: Vec::new(),
        }
    }
}

impl Contributions {
    /// Combine two contribution sets bucket-by-bucket, each by its own rule.
    pub fn combine(mut self, other: Contributions) -> Contributions {
        self.flat_crit_chance += other.flat_crit_chance;
        self.fire_rate_multiplier *= other.fire_rate_multiplier;
        self.ammo_efficiency += other.ammo_efficiency;
        self.injected_elements.extend(other.injected_elements);
        self
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
    pub expiry_seconds: Option<f64>,
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
            .retain(|b| b.expiry_seconds.is_none_or(|e| e > t_secs));
    }

    /// All active buffs, for display (the UI reads this).
    pub fn active(&self) -> &[Buff] {
        &self.buffs
    }

    /// Combined contributions of every active buff.
    ///
    /// Scope-aware application (weapon vs Warframe vs squad) will refine this
    /// once non-weapon buffs exist; today every buff is `Weapon`-scoped.
    pub fn total_contributions(&self) -> Contributions {
        self.buffs.iter().fold(Contributions::default(), |acc, b| {
            acc.combine(b.contributions.clone())
        })
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
            expiry_seconds: Some(3.0),
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
            expiry_seconds: None,
            contributions: Contributions::default(),
        };
        bar.upsert(mk(1));
        bar.upsert(mk(2));
        assert_eq!(bar.active().len(), 1);
        assert_eq!(bar.get("x").unwrap().stacks, 2);
    }
}
