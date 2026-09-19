// SPDX-License-Identifier: AGPL-3.0-or-later
//! Perks: held capabilities that grant buffs.
//!
//! A **perk** is something you hold or equip — an arcane, a weapon passive, an
//! Incarnon evolution — whose possession lets you *trigger* a buff. On a
//! timeline [`crate::rules::sim::Event`] a perk applies / refreshes / resets its
//! [`crate::rules::buffs::Buff`] in the [`crate::rules::buffs::BuffBar`]. The perk keeps its
//! own private bookkeeping (rate-limit timers, counters) that the HUD does not
//! show; the buff holds the visible stacks and duration.
//!
//! This is the grantor side of the buff system; the runtime overlay lives in
//! [`crate::rules::buffs`]. See `docs/BUFFS.md`.

pub mod frenzy;
pub mod secondary_enervate;

use crate::rules::buffs::BuffBar;
use crate::rules::sim::Event;

/// A held capability that grants a buff on its trigger.
pub trait Perk {
    /// Stable id of the perk (and of the buff it manages), e.g.
    /// `"secondary_enervate"`, `"frenzy"`.
    fn id(&self) -> &str;

    /// React to an event at `t_secs`, applying/refreshing/resetting the buff
    /// this perk grants in `bar`.
    fn on_event(&mut self, event: &Event, t_secs: f64, bar: &mut BuffBar);
}
