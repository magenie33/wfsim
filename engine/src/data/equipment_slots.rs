// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE EQUIPMENT SLOTS — `data/equipment_slots.yaml`: where a thing is
//! equipped, in the order the home page lists them.
//!
//! Not the MOD slots a weapon has (`data::weapons::slots`): this is which
//! arsenal slot the item itself goes in, and it groups the roster.

use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct EquipmentSlot {
    pub id: String,
    /// The game's own English; the overlay transcribes it.
    pub name: String,
    /// Which roster fills the section: `weapon`, `warframe`, `companion`,
    /// `operator`, or `placeholder` for a category the builder does not hold yet.
    pub holds: String,
    /// DE's Public Export `productCategory` for what the slot holds.
    #[serde(default)]
    pub de_category: Vec<String>,
}

#[derive(Deserialize)]
struct File {
    slots: Vec<EquipmentSlot>,
}

/// Every equipment slot, in home-page order.
pub fn all() -> &'static [EquipmentSlot] {
    static SLOTS: OnceLock<Vec<EquipmentSlot>> = OnceLock::new();
    SLOTS.get_or_init(|| {
        let text = crate::data::file("equipment_slots.yaml").expect("data/equipment_slots.yaml");
        serde_norway::from_str::<File>(text).expect("parse equipment_slots.yaml").slots
    })
}

/// The slots that hold weapons, and therefore the only values a weapon's
/// `slot` may take.
pub fn weapon_slots() -> impl Iterator<Item = &'static EquipmentSlot> {
    all().iter().filter(|s| s.holds == "weapon")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_weapon_sits_in_a_declared_weapon_slot() {
        let ids: Vec<&str> = weapon_slots().map(|s| s.id.as_str()).collect();
        for w in crate::data::weapons::all() {
            assert!(ids.contains(&w.slot.as_str()), "{}: slot {} is not declared in data/equipment_slots.yaml", w.id, w.slot);
        }
    }

    #[test]
    fn a_slot_is_declared_once_and_holds_a_known_roster() {
        let mut seen = std::collections::HashSet::new();
        for s in all() {
            assert!(seen.insert(s.id.as_str()), "{} declared twice", s.id);
            assert!(["weapon", "warframe", "companion", "operator", "placeholder"].contains(&s.holds.as_str()), "{}: holds {}", s.id, s.holds);
        }
    }
}
