//! Frenzy — Dual Toxocyst's weapon passive, modeled as a [`Perk`].
//!
//! Data / source of truth: the inline `passives:` block in
//! `data/weapons/dual_toxocyst.yaml` (trigger + granted buff in one block)
//! (<https://wiki.warframe.com/w/Dual_Toxocyst>). Terminology per
//! `docs/GLOSSARY.md`.
//!
//! Mechanic (status: **unverified** — pending in-game golden test):
//! - Trigger: landing a **true headshot** on a **living** target grants the
//!   Frenzy buff. Other weakspots do not count, and headshots on corpses do not
//!   grant it.
//! - The buff lasts 3 s and is **refreshed** by subsequent headshots.
//! - While active it grants: fire rate **x2.5** (an independent multiplier,
//!   applied after fire-rate mods), **+100% ammo efficiency** (=> infinite
//!   ammo), and **+100% Toxin** injected as an elemental mod at the end of the
//!   mod order. (Recoil -75% is not a damage bucket and is omitted here.)
//!
//! This perk is stateless — the buff's duration/refresh lives on the [`Buff`]
//! (its `expiry_secs`), which the [`BuffBar`] expires as time advances.

use crate::buffs::{Buff, BuffBar, BuffScope, Contributions, InjectedElement, InjectionPosition};
use crate::damage::DamageType;
use crate::perks::Perk;
use crate::sim::Event;

/// The Frenzy buff's bar id.
pub const BUFF_ID: &str = "frenzy";

/// Buff duration in seconds (refreshed on each qualifying headshot).
const DURATION_SECS: f64 = 3.0;

/// Independent fire-rate multiplier (wiki "+150%" == x2.5).
const FIRE_RATE_MULTIPLIER: f64 = 2.5;

/// +100% ammo efficiency => infinite ammo.
const AMMO_EFFICIENCY_BONUS: f64 = 1.0;

/// +100% Toxin, injected as an elemental mod at the end of the mod order.
const TOXIN_PERCENT_OF_BASE: f64 = 1.0;

impl Frenzy {
    /// The Frenzy buff with NO expiry — for buff-lock simulation settings
    /// (force a buff permanently active, e.g. "assume Frenzy uptime 100%").
    pub fn permanent_buff() -> Buff {
        Buff {
            id: BUFF_ID.into(),
            scope: BuffScope::Weapon,
            stacks: 1,
            expiry_secs: None,
            contributions: Self::buff_contributions(),
        }
    }
}

/// Frenzy — a stateless perk (the buff carries all the timed state).
#[derive(Debug, Clone, Default)]
pub struct Frenzy;

impl Frenzy {
    pub fn new() -> Self {
        Frenzy
    }

    /// The contributions the Frenzy buff grants while active.
    fn buff_contributions() -> Contributions {
        Contributions {
            fire_rate_multiplier: FIRE_RATE_MULTIPLIER,
            ammo_efficiency: AMMO_EFFICIENCY_BONUS,
            injected_elements: vec![InjectedElement {
                damage_type: DamageType::Toxin,
                amount_percent_of_base: TOXIN_PERCENT_OF_BASE,
                position: InjectionPosition::EndOfModOrder,
            }],
            ..Default::default()
        }
    }
}

impl Perk for Frenzy {
    fn id(&self) -> &str {
        BUFF_ID
    }

    fn on_event(&mut self, event: &Event, t_secs: f64, bar: &mut BuffBar) {
        let Event::Hit(hit) = event;

        // Only a true headshot on a living target grants/refreshes Frenzy.
        if !(hit.headshot && hit.target_alive) {
            return;
        }

        // Grant or refresh: a fresh 3 s window from now. Non-stacking, so a
        // single upsert replaces any existing Frenzy buff.
        bar.upsert(Buff {
            id: BUFF_ID.into(),
            scope: BuffScope::Weapon,
            stacks: 1,
            expiry_secs: Some(t_secs + DURATION_SECS),
            contributions: Self::buff_contributions(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::Hit;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn headshot() -> Event {
        Event::Hit(Hit {
            headshot: true,
            target_alive: true,
            ..Hit::default()
        })
    }

    #[test]
    fn headshot_grants_frenzy_with_correct_contributions() {
        let mut bar = BuffBar::new();
        let mut f = Frenzy::new();
        f.on_event(&headshot(), 0.0, &mut bar);

        let buff = bar.get(BUFF_ID).expect("frenzy granted");
        assert_eq!(buff.stacks, 1);
        assert_eq!(buff.expiry_secs, Some(3.0));

        let c = bar.total_contributions();
        assert!(approx(c.fire_rate_multiplier, 2.5));
        assert!(approx(c.ammo_efficiency, 1.0));
        assert_eq!(c.injected_elements.len(), 1);
        let inj = c.injected_elements[0];
        assert_eq!(inj.damage_type, DamageType::Toxin);
        assert!(approx(inj.amount_percent_of_base, 1.0));
        assert_eq!(inj.position, InjectionPosition::EndOfModOrder);
    }

    #[test]
    fn body_shot_does_not_grant() {
        let mut bar = BuffBar::new();
        let mut f = Frenzy::new();
        f.on_event(&Event::Hit(Hit::default()), 0.0, &mut bar); // not a headshot
        assert!(bar.get(BUFF_ID).is_none());
    }

    #[test]
    fn headshot_on_corpse_does_not_grant() {
        let mut bar = BuffBar::new();
        let mut f = Frenzy::new();
        let corpse_headshot = Event::Hit(Hit {
            headshot: true,
            target_alive: false,
            ..Hit::default()
        });
        f.on_event(&corpse_headshot, 0.0, &mut bar);
        assert!(bar.get(BUFF_ID).is_none());
    }

    #[test]
    fn buff_expires_after_three_seconds() {
        let mut bar = BuffBar::new();
        let mut f = Frenzy::new();
        f.on_event(&headshot(), 0.0, &mut bar);
        bar.expire(2.9);
        assert!(bar.get(BUFF_ID).is_some());
        bar.expire(3.0);
        assert!(bar.get(BUFF_ID).is_none());
    }

    #[test]
    fn subsequent_headshot_refreshes_the_window() {
        let mut bar = BuffBar::new();
        let mut f = Frenzy::new();
        f.on_event(&headshot(), 0.0, &mut bar); // expires at 3.0
        f.on_event(&headshot(), 2.0, &mut bar); // refreshed: expires at 5.0
        bar.expire(3.0);
        assert!(bar.get(BUFF_ID).is_some());
        assert_eq!(bar.get(BUFF_ID).unwrap().expiry_secs, Some(5.0));
    }
}
