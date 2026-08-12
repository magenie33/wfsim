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
/// HOW MANY OF A SET'S MEMBERS THIS ENGINE CAN EQUIP.
///
/// A set is up to six mods spread across the Warframe AND the weapons — "Only
/// the number of equipped mods within the set dictates the Set Bonus strength"
/// (wiki, Set_Mods) — and this engine has no Warframe loadout. So a set whose
/// members are not all weapon mods has a LOWER CEILING here than in game: the
/// Vigilante set is six at 5% each, two of them (Vigor, Pursuit) go on the
/// frame, and a weapon build tops out at 20% against 30%.
///
/// Counted from the data rather than written down, so a set that later becomes
/// complete stops being reported as short on its own.
pub fn members_carried(set_id: &str) -> u32 {
    static COUNTS: OnceLock<Vec<(String, u32)>> = OnceLock::new();
    let counts = COUNTS.get_or_init(|| {
        let mut out: Vec<(String, u32)> = Vec::new();
        for (_path, text) in crate::data::files_under("mods/") {
            let Some(set) = text.lines().find_map(|l| l.strip_prefix("set:")) else { continue };
            let set = set.split('#').next().unwrap_or("").trim().to_string();
            if set.is_empty() {
                continue;
            }
            match out.iter_mut().find(|(s, _)| *s == set) {
                Some(e) => e.1 += 1,
                None => out.push((set, 1)),
            }
        }
        out
    });
    counts.iter().find(|(s, _)| s == set_id).map_or(0, |(_, n)| *n)
}

pub fn set_def(id: &str) -> Option<&'static ModSetDef> {
    all().iter().find(|s| s.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A MOD THAT BELONGS TO A SET SAYS SO, AND THE SET EXISTS.
    ///
    /// Both halves are derived from the mod's own `internal_name`, which is
    /// DE's path and carries `/Sets/<Family>/` for every set member — so this
    /// cannot be satisfied by remembering to add a mod to a list. It is the
    /// list.
    ///
    /// It exists because the pool had SEVEN set members with no `set:` line and
    /// SIX set families with no file at all (2026-08-10). Carnis Stinger,
    /// Jugulus Spines and Saxum Spittle each grant a real set bonus in game and
    /// the app said nothing about any of it — while three other sets were
    /// declared `unmodeled` precisely so it could. `augur_seeker` was the sharp
    /// case: its set IS modelled and its sibling `augur_pact` declares it, so
    /// the two members of one set disagreed about whether they were in it.
    ///
    /// A set whose bonus does nothing for a weapon is still declared, with
    /// `kind: unmodeled` — that is the whole convention, and the reason this
    /// check can be an equality rather than a courtesy.
    #[test]
    fn every_set_member_names_its_set_and_every_named_set_exists() {
        let mut members = 0;
        for (path, text) in crate::data::files_under("mods/") {
            let field = |k: &str| {
                text.lines()
                    .find_map(|l| l.strip_prefix(k))
                    .map(|v| v.split('#').next().unwrap_or("").trim().to_string())
            };
            let Some(internal) = field("internal_name:") else { continue };
            if !internal.contains("/Sets/") {
                continue;
            }
            members += 1;
            let set = field("set:").unwrap_or_else(|| {
                panic!("{path} is a set member ({internal}) and declares no `set:`")
            });
            assert!(
                set_def(&set).is_some(),
                "{path} names the set `{set}`, and data/mod_sets/{set}.yaml does not exist"
            );
        }
        assert!(members >= 15, "only {members} set members found — data empty?");
    }

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
