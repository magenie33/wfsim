// SPDX-License-Identifier: AGPL-3.0-or-later
use super::ValidBuild;
use crate::data::{mods, weapons};

impl ValidBuild {
    /// Sustainable inputs with a complete script for this build's stance slot.
    pub fn board_modes(&self) -> Vec<weapons::WeaponPlayMode> {
        let pool = mods::pool_naming(&self.weapon, &self.mods);
        let stance = self.mods.iter().filter_map(|id| pool.get(id)).find_map(|m| m.stance);
        weapons::play_modes(&self.weapon)
            .into_iter()
            .filter(|m| m.sustainable)
            .filter(|m| weapons::spec(m.other_id.unwrap_or(m.weapon_id)).is_some_and(|s| {
                weapons::combo_error(&self.weapon, s.form_kind(), stance).is_none()
            }))
            .collect()
    }
}
