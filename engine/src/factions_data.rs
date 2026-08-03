//! The post-U36 faction VULNERABILITY COLUMN: `data/factions/damage_modifiers.yaml`
//! → an incoming-damage multiplier per damage type.
//!
//! This is **System B** of the two faction systems (docs/MECHANICS.md §8). It
//! is not the Bane/Expel bucket — that one is keyed by the enemy's `Faction`,
//! multiplies the whole instance and double-dips DoT ticks. This one is keyed
//! by `FactionDamageOverride ?? Faction`, is a clean **per-component**
//! multiplier, and stacks multiplicatively with everything else:
//!
//! ```text
//! per-component = damage × bane_mult × column(type) × pool math
//! ```
//!
//! Two consequences the shape of this module encodes:
//!
//! - The column is chosen by the POOL as well as the enemy. Damage landing on
//!   Overguard reads the Overguard column (neutral but ×1.5 Void), never the
//!   enemy's own — which is why [`Columns`] carries both and the pool decides.
//! - A key that is not in the table is a DATA ERROR, not a neutral enemy. The
//!   file writes neutral columns down (`unknown: {}`, `stalker: {}`, `tenno:
//!   {}`) precisely so that a typo cannot quietly mean "takes normal damage".

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::damage::DamageType;

/// One faction's incoming-damage multipliers, indexed by damage type.
/// Anything the file does not list is 1.0 — the file lists only what the wiki
/// publishes, and every published value today happens to be 1.5 or 0.5 (do
/// not bake that coincidence in: the type is `f64`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Column([f64; DamageType::ALL.len()]);

impl Column {
    /// Takes normal damage from everything.
    pub const NEUTRAL: Column = Column([1.0; DamageType::ALL.len()]);

    pub fn get(&self, t: DamageType) -> f64 {
        self.0[t as usize]
    }

    /// The entries that are NOT 1.0, in damage-type order — what a target card
    /// has to say about this enemy, and nothing it does not.
    pub fn listed(&self) -> Vec<(DamageType, f64)> {
        DamageType::ALL
            .iter()
            .copied()
            .filter(|t| self.0[*t as usize] != 1.0)
            .map(|t| (t, self.0[t as usize]))
            .collect()
    }
}

/// The two columns one target resolves to: its faction's, and the Overguard
/// pool's. They are separate because Overguard is not part of the enemy — it
/// is a layer over it with its own table (wiki Overguard).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Columns {
    pub faction: Column,
    pub overguard: Column,
}

impl Columns {
    pub const NEUTRAL: Columns = Columns {
        faction: Column::NEUTRAL,
        overguard: Column::NEUTRAL,
    };
}

impl Default for Columns {
    fn default() -> Self {
        Columns::NEUTRAL
    }
}

/// A section's entries. `object:` in the file carries only a comment, which
/// YAML reads as null — hence the Option, and hence "absent = neutral" rather
/// than a parse failure on a column that genuinely modifies nothing.
type Section = HashMap<String, Option<HashMap<String, f64>>>;

#[derive(Debug, Deserialize)]
struct ModifiersFile {
    factions: Section,
    #[serde(default)]
    special: Section,
}

fn to_column(entries: &Option<HashMap<String, f64>>, key: &str) -> Column {
    let mut col = Column::NEUTRAL;
    for (name, mult) in entries.iter().flatten() {
        let t = DamageType::from_name(name)
            .unwrap_or_else(|| panic!("factions/damage_modifiers.yaml: {key}: unknown damage type {name}"));
        col.0[t as usize] = *mult;
    }
    col
}

struct Table {
    factions: HashMap<String, Column>,
    overguard: Column,
}

fn table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| {
        let text = crate::data::file("factions/damage_modifiers.yaml")
            .expect("data/factions/damage_modifiers.yaml is embedded");
        let f: ModifiersFile =
            serde_norway::from_str(text).expect("parse factions/damage_modifiers.yaml");
        Table {
            factions: f
                .factions
                .iter()
                .map(|(k, v)| (k.clone(), to_column(v, k)))
                .collect(),
            // Absent = neutral: the pool exists either way, and its own row is
            // what makes Void ×1.5 on Overguard true.
            overguard: f
                .special
                .get("overguard")
                .map(|v| to_column(v, "overguard"))
                .unwrap_or(Column::NEUTRAL),
        }
    })
}

/// The column for a faction key (`FactionDamageOverride ?? Faction`, lowercase
/// as the data files spell it). `None` = the key is not in the table, which is
/// an error at the caller — see the module note.
pub fn column(key: &str) -> Option<Column> {
    table().factions.get(key).copied()
}

/// The Overguard pool's own column. Damage landing on Overguard reads THIS,
/// whatever the enemy is.
pub fn overguard_column() -> Column {
    table().overguard
}

/// What one target resolves to. `key` is `FactionDamageOverride ?? Faction`.
pub fn columns_for(key: &str) -> Option<Columns> {
    column(key).map(|faction| Columns {
        faction,
        overguard: overguard_column(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_columns_load() {
        // The two the wiki states most plainly, and the shape of the rest.
        let g = column("grineer").expect("grineer column");
        assert_eq!(g.get(DamageType::Impact), 1.5);
        assert_eq!(g.get(DamageType::Corrosive), 1.5);
        assert_eq!(g.get(DamageType::Slash), 1.0, "unlisted = normal damage");

        let o = column("orokin").expect("orokin column");
        assert_eq!(o.get(DamageType::Puncture), 1.5);
        assert_eq!(o.get(DamageType::Viral), 1.5);
        assert_eq!(o.get(DamageType::Radiation), 0.5, "a RESISTANCE, not a hole");

        // Zariman is an OVERRIDE-only column (no unit has it as its Faction).
        assert_eq!(column("zariman").unwrap().get(DamageType::Void), 1.5);
    }

    /// Neutrality is a written-down column, not the absence of one — the whole
    /// reason `column()` can return None and mean "data error".
    #[test]
    fn neutral_columns_exist_and_are_neutral() {
        for key in ["unknown", "stalker", "tenno"] {
            let c = column(key).unwrap_or_else(|| panic!("{key} must be in the table"));
            assert_eq!(c, Column::NEUTRAL, "{key}");
            assert!(c.listed().is_empty(), "{key}");
        }
        assert!(column("no_such_faction").is_none());
    }

    #[test]
    fn overguard_is_neutral_except_void() {
        let og = overguard_column();
        assert_eq!(og.get(DamageType::Void), 1.5);
        assert_eq!(og.listed(), vec![(DamageType::Void, 1.5)]);
    }

    /// Every faction key any enemy resolves to has to be in the table, or the
    /// enemy silently takes normal damage from everything.
    #[test]
    fn every_enemy_resolves_to_a_column() {
        for e in crate::enemy_data::all() {
            let key = e.damage_column_key();
            assert!(column(key).is_some(), "{}: no column for {key}", e.id);
        }
    }
}
