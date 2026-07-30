//! Declarative MOD SET loader: `data/mod_sets/<id>.yaml` -> the set bonuses.
//!
//! A set bonus is not a property of any one mod — it is what a GROUP of them
//! grants together — so it is defined once here and every member just names
//! its set (`set: vigilante`), the same define-once/reference-anywhere rule
//! `data/README.md` states for perks.
//!
//! Bonuses scale PER EQUIPPED MEMBER, with no threshold: one member already
//! grants its share (user, 2026-07-31). The wiki states the Vigilante set that
//! way — 5% per mod up to 30% at six — and it is why a set can be worth
//! carrying before it is complete.

use std::sync::OnceLock;

use serde::Deserialize;

/// What a set grants per equipped member. One variant so far; the enum is the
/// seam, so a second set does not have to invent its own plumbing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetBonusKind {
    /// Chance to raise a critical hit's TIER by one (Vigilante). Only a hit
    /// that already crit can be promoted.
    CritTierUpgrade,
    /// Parsed but not modeled — the mod set still loads.
    Unmodeled,
}

#[derive(Debug, Clone)]
pub struct ModSetDef {
    pub id: &'static str,
    pub name: &'static str,
    /// Members the set has in the game, INCLUDING ones no weapon can equip
    /// (Vigilante counts two Warframe mods). Recorded so the ceiling a weapon
    /// build can reach is not mistaken for the set's full bonus.
    pub members: u32,
    pub kind: SetBonusKind,
    /// The bonus one equipped member contributes.
    pub per_mod: f64,
}

#[derive(Debug, Deserialize)]
struct BonusFile {
    kind: String,
    per_mod: f64,
}

#[derive(Debug, Deserialize)]
struct SetFile {
    id: String,
    name: String,
    #[serde(default)]
    members: u32,
    bonus: BonusFile,
}

fn all() -> &'static [ModSetDef] {
    static S: OnceLock<Vec<ModSetDef>> = OnceLock::new();
    S.get_or_init(|| {
        let mut out: Vec<ModSetDef> = crate::data::files_under("mod_sets/")
            .filter_map(|(_, text)| serde_norway::from_str::<SetFile>(text).ok())
            .map(|f| ModSetDef {
                id: Box::leak(f.id.into_boxed_str()),
                name: Box::leak(f.name.into_boxed_str()),
                members: f.members,
                kind: match f.bonus.kind.as_str() {
                    "crit_tier_upgrade_chance" => SetBonusKind::CritTierUpgrade,
                    _ => SetBonusKind::Unmodeled,
                },
                per_mod: f.bonus.per_mod,
            })
            .collect();
        out.sort_by_key(|s| s.id);
        out
    })
}

/// Every set in the data, by id.
pub fn sets() -> &'static [ModSetDef] {
    all()
}

/// One set by id. `None` = a mod names a set with no definition file, which
/// the pool test refuses.
pub fn set_def(id: &str) -> Option<&'static ModSetDef> {
    all().iter().find(|s| s.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_vigilante_set_loads_its_bonus() {
        let v = set_def("vigilante").expect("data/mod_sets/vigilante.yaml");
        assert_eq!(v.kind, SetBonusKind::CritTierUpgrade);
        assert!((v.per_mod - 0.05).abs() < 1e-12);
        assert_eq!(v.members, 6);
        // Four of the six are primary mods, so a WEAPON build tops out at 20%.
        assert!((v.per_mod * 4.0 - 0.20).abs() < 1e-12);
    }

    /// A member naming a set with no definition would silently contribute
    /// nothing, which is the failure mode this whole file exists to avoid.
    #[test]
    fn every_mod_set_named_by_a_mod_is_defined() {
        for class in crate::mods_data::classes() {
            for m in crate::mods_data::class_pool(class) {
                if let Some(s) = m.set {
                    assert!(set_def(s).is_some(), "{} names undefined set {s}", m.id);
                }
            }
        }
    }
}
