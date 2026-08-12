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
//! - **The table is COMPLETE at fifteen columns, and everything else is
//!   neutral** (user, 2026-08-03). The wiki's `Damage/Overview_Table` publishes
//!   exactly those fifteen; the enemy modules key eighteen faction values
//!   against them, and the ones with no column (Stalker, Unknown, Duviri,
//!   Neutral, Objects, Predator, Prey) are units the game gives no
//!   vulnerability or resistance to. So an unlisted key is not an error to
//!   report — it is the answer, and [`columns_for`] returns it.

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

    /// A column nobody published — one a PLAYER wrote for an enemy of their
    /// own. Same shape and the same rule: a type left out takes damage as
    /// written, so the map says only what is unusual about this target.
    ///
    /// It is also where an IMMUNITY lives, because an immunity is not a
    /// separate mechanic: it is this column reading 0. The game has no third
    /// state between "×1.5" and "nothing gets through", and giving one to the
    /// data would mean two ways to say the same thing.
    pub fn from_multipliers(entries: &[(DamageType, f64)]) -> Column {
        let mut c = Column::NEUTRAL;
        for &(t, v) in entries {
            c.0[t as usize] = v;
        }
        c
    }
}

impl Column {

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
/// as the data files spell it). A key the table does not name takes every
/// damage type as written — see the module note; the fifteen are all there is.
pub fn column(key: &str) -> Column {
    table().factions.get(key).copied().unwrap_or(Column::NEUTRAL)
}

/// Every faction key the table names, sorted. What a player building an enemy
/// of their own is choosing between — the column is the whole of what a faction
/// means to incoming damage, so the list has to come from the table rather than
/// from a copy of it in the UI.
pub fn keys() -> Vec<&'static str> {
    let mut k: Vec<&'static str> = table().factions.keys().map(|s| s.as_str()).collect();
    k.sort_unstable();
    k
}

/// Whether the table names this key at all. Only for saying so — a faction
/// with no column is neutral, not wrong.
pub fn is_listed(key: &str) -> bool {
    table().factions.contains_key(key)
}

/// The Overguard pool's own column. Damage landing on Overguard reads THIS,
/// whatever the enemy is.
pub fn overguard_column() -> Column {
    table().overguard
}

/// What one target resolves to. `key` is `FactionDamageOverride ?? Faction`;
/// a key with no column resolves to the neutral one, which is what the game
/// does with every faction the damage table leaves out.
pub fn columns_for(key: &str) -> Columns {
    Columns {
        faction: column(key),
        overguard: overguard_column(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_columns_load() {
        // The two the wiki states most plainly, and the shape of the rest.
        let g = column("grineer");
        assert_eq!(g.get(DamageType::Impact), 1.5);
        assert_eq!(g.get(DamageType::Corrosive), 1.5);
        assert_eq!(g.get(DamageType::Slash), 1.0, "unlisted = normal damage");

        let o = column("orokin");
        assert_eq!(o.get(DamageType::Puncture), 1.5);
        assert_eq!(o.get(DamageType::Viral), 1.5);
        assert_eq!(o.get(DamageType::Radiation), 0.5, "a RESISTANCE, not a hole");

        // Zariman is an OVERRIDE-only column (no unit has it as its Faction).
        assert_eq!(column("zariman").get(DamageType::Void), 1.5);
    }

    /// The wiki's Damage/Overview_Table publishes fifteen columns and that is
    /// the whole system, so this list is not a sample — it is the set. Locked
    /// here because the rule "everything else is neutral" is only safe while
    /// the fifteen are actually present.
    #[test]
    fn the_table_is_exactly_the_wikis_fifteen_columns() {
        let mut have: Vec<&str> = table().factions.keys().map(|k| k.as_str()).collect();
        have.sort_unstable();
        let mut want = [
            "tenno", "grineer", "kuva_grineer", "corpus", "corpus_amalgam", "infested",
            "infested_deimos", "orokin", "sentient", "narmer", "the_murmur", "zariman",
            "scaldra", "techrot", "anarchs",
        ];
        want.sort_unstable();
        assert_eq!(have, want);
    }

    /// A faction the table leaves out is not a gap and not a typo to catch —
    /// it is a unit the game gives no vulnerability or resistance to (user,
    /// 2026-08-03). The Acolytes ("Stalker") and the Thrax ("Unknown") are the
    /// two we ship, and `Tenno` is the one such column the table does print.
    #[test]
    fn a_faction_with_no_column_takes_every_type_as_written() {
        for key in ["stalker", "unknown", "duviri", "predator", "no_such_faction", "tenno"] {
            let c = column(key);
            assert_eq!(c, Column::NEUTRAL, "{key}");
            assert!(c.listed().is_empty(), "{key}");
        }
        assert!(is_listed("tenno"), "the table does print an empty Tenno column");
        assert!(!is_listed("stalker"));
    }

    #[test]
    fn overguard_is_neutral_except_void() {
        let og = overguard_column();
        assert_eq!(og.get(DamageType::Void), 1.5);
        assert_eq!(og.listed(), vec![(DamageType::Void, 1.5)]);
    }

    /// What each shipped enemy actually resolves to — the roster's own column
    /// assignment, asserted rather than assumed. A unit that gained or lost a
    /// vulnerability is a real finding about the unit, and it should not be
    /// discoverable only by staring at the target card.
    #[test]
    fn the_roster_resolves_to_the_columns_its_data_says() {
        for e in crate::enemy_data::all() {
            let key = e.damage_column_key();
            let listed = column(key).listed();
            match e.id.as_str() {
                "thrax_centurion" => assert_eq!(listed, vec![(DamageType::Void, 1.5)], "{key}"),
                "corrupted_heavy_gunner" => assert_eq!(listed.len(), 3, "{key}"),
                // THE ROSTER'S FIRST GRINEER UNIT, so the first to carry that
                // faction's column at all: Impact and Corrosive x1.5.
                "demolisher_devourer" => assert_eq!(
                    listed,
                    vec![(DamageType::Impact, 1.5), (DamageType::Corrosive, 1.5)],
                    "{key}"
                ),
                // The six Acolytes: faction "Stalker", which the table skips.
                _ => assert!(listed.is_empty(), "{}: unexpected column {key}", e.id),
            }
        }
    }
}
