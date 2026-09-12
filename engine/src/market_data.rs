//! WHAT WARFRAME.MARKET CALLS THINGS WE MODEL — `data/market.yaml`.
//!
//! Slugs, and nothing else. The site is the authority on its own identifiers
//! and on nothing here: no price travels with a slug, because a price needs a
//! refresh rule and a staleness rule, and this would then be a second source
//! for a number the app already computes.
//!
//! AN ABSENT ENTRY IS NOT LISTED THERE, and the page then offers no link. That
//! is why no card in this repo carries a `tradeable` field: membership of this
//! table is the whole rule, and it cannot drift from what the site will
//! actually show a visitor.
//!
//! A RIVEN'S SLUG IS ITS FAMILY'S. One card fits Braton, Braton Prime and the
//! Incarnon form alike, and the auctions are listed under the family — so
//! `riven_weapons` is keyed by our entry and valued by the family's slug.
//!
//! `scripts/gen_market.py` writes it, joining by `internal_name` and by the
//! riven stat `tag` — never by display name.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
struct Market {
    #[serde(default)]
    mods: BTreeMap<String, String>,
    #[serde(default)]
    weapons: BTreeMap<String, String>,
    #[serde(default)]
    arcanes: BTreeMap<String, String>,
    #[serde(default)]
    lich_weapons: BTreeMap<String, String>,
    #[serde(default)]
    sister_weapons: BTreeMap<String, String>,
    #[serde(default)]
    riven_families: BTreeMap<String, String>,
    #[serde(default)]
    riven_stats: BTreeMap<String, String>,
}

/// The entry a FORM stands in for. A form carries no `internal_name` and no
/// family of its own, and nothing about a trade changes when one is selected —
/// the Incarnon Braton Prime is the same card on the same market stall.
fn base_entry(id: &str) -> Option<&'static crate::weapons_data::WeaponSpec> {
    let by_id = |want: &str| crate::weapons_data::all().iter().find(|w| w.id == want);
    let w = by_id(id)?;
    Some(w.transform_group.as_deref().and_then(by_id).unwrap_or(w))
}

fn market() -> &'static Market {
    static M: OnceLock<Market> = OnceLock::new();
    M.get_or_init(|| {
        crate::data::file("market.yaml")
            .map(|t| serde_norway::from_str(t).expect("data/market.yaml"))
            .unwrap_or_default()
    })
}

/// The item slug for a mod id, or `None` where the mod does not trade.
pub fn mod_slug(id: &str) -> Option<&'static str> {
    market().mods.get(id).map(String::as_str)
}

/// The item slug for a weapon id — a Prime's is its SET, which is the row
/// that carries the weapon's own uniqueName. Absent for the 250 weapons that
/// only ever come off a blueprint, and for the adversary weapons below, which
/// are auctioned instead.
pub fn weapon_slug(id: &str) -> Option<&'static str> {
    market().weapons.get(&base_entry(id)?.id).map(String::as_str)
}

/// The item slug for an arcane id.
pub fn arcane_slug(id: &str) -> Option<&'static str> {
    market().arcanes.get(id).map(String::as_str)
}

/// An adversary weapon's AUCTION — its type and its slug.
///
/// Kuva and Tenet weapons are auctioned rather than sold, because the valence
/// bonus a copy came out of its Lich with is part of what is being traded. The
/// two are separate auction types, so the table an entry is in IS the `type=`
/// its link needs; the three weapon tables never overlap (tested).
pub fn adversary_auction(id: &str) -> Option<(&'static str, &'static str)> {
    let base = &base_entry(id)?.id;
    market().lich_weapons.get(base).map(|s| ("lich", s.as_str()))
        .or_else(|| market().sister_weapons.get(base).map(|s| ("sister", s.as_str())))
}

/// The riven-auction slug for a weapon id. Absent for a weapon with no riven.
///
/// THE WALK IS OURS, THE SLUG IS THEIRS. A form takes the family of the entry
/// it transforms from; a weapon that declares no family is its own. Doing this
/// here rather than spelling a row per entry into the generated table means a
/// new form gets its link without anyone re-running the script.
pub fn riven_weapon_slug(id: &str) -> Option<&'static str> {
    let base = base_entry(id)?;
    let family = base.riven_family.as_deref().unwrap_or(&base.id);
    market().riven_families.get(family).map(String::as_str)
}

/// The auction-filter slug for a riven stat id (`data/rivens/`).
pub fn riven_stat_slug(id: &str) -> Option<&'static str> {
    market().riven_stats.get(id).map(String::as_str)
}

/// Every riven stat's filter slug, for the page to build a search from. The
/// WHOLE table travels: a riven card names the stats it rolled and the page
/// has to translate all of them at once or the search it builds is a
/// different one.
pub fn riven_stats() -> impl Iterator<Item = (&'static str, &'static str)> {
    market().riven_stats.iter().map(|(k, v)| (k.as_str(), v.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_loaded_and_populated() {
        assert!(mod_slug("serration").is_some());
        assert!(riven_weapon_slug("kuva_bramma").is_some());
        assert_eq!(riven_stat_slug("critical_chance"), Some("critical_chance"));
    }

    /// A GENERATED TABLE GOES STALE IN SILENCE. Renaming a mod's file leaves
    /// its old id here, where it resolves to nothing and the card quietly
    /// stops offering a link — so every key must still be an id we ship.
    #[test]
    fn every_key_is_an_id_this_app_still_has() {
        let mods: Vec<String> = crate::mods_data::classes()
            .iter()
            .flat_map(|c| crate::mods_data::class_pool(c))
            .map(|m| m.id.to_string())
            .collect();
        for id in market().mods.keys() {
            assert!(mods.contains(id), "market.yaml names a mod we do not have: {id}");
        }
        let weapons: Vec<&str> = crate::weapons_data::all().iter().map(|w| w.id.as_str()).collect();
        for table in [&market().weapons, &market().lich_weapons, &market().sister_weapons] {
            for id in table.keys() {
                assert!(weapons.contains(&id.as_str()), "market.yaml names a weapon we do not have: {id}");
            }
        }
        let arcanes: Vec<&str> = crate::arcanes_data::slots()
            .iter()
            .flat_map(|s| crate::arcanes_data::slot_pool(s))
            .map(|a| a.id.as_str())
            .collect();
        for id in market().arcanes.keys() {
            assert!(arcanes.contains(&id.as_str()), "market.yaml names an arcane we do not have: {id}");
        }
        let families: Vec<&str> = crate::weapons_data::all()
            .iter()
            .map(|w| w.riven_family.as_deref().unwrap_or(&w.id))
            .collect();
        for fam in market().riven_families.keys() {
            assert!(
                families.contains(&fam.as_str()),
                "market.yaml names a riven family no weapon claims: {fam}"
            );
        }
        let stats: Vec<String> = riven_classes()
            .iter()
            .flat_map(|c| crate::rivens_data::pool(c))
            .map(|s| s.id.clone())
            .collect();
        for id in market().riven_stats.keys() {
            assert!(stats.contains(id), "market.yaml names a riven stat we do not have: {id}");
        }
    }

    /// ONE MARK PER WEAPON. A weapon is SOLD or AUCTIONED, never both, and the
    /// page draws one link from whichever table holds it — so a weapon in two
    /// of them would make the mark's destination depend on lookup order.
    #[test]
    fn a_weapon_is_sold_or_auctioned_but_never_both() {
        let m = market();
        for id in m.weapons.keys() {
            assert!(!m.lich_weapons.contains_key(id) && !m.sister_weapons.contains_key(id),
                "{id} is both sold and auctioned");
        }
        for id in m.lich_weapons.keys() {
            assert!(!m.sister_weapons.contains_key(id), "{id} is both a lich and a sister auction");
        }
    }

    #[test]
    fn a_weapon_and_an_arcane_reach_their_pages() {
        // A Prime sells as its SET, never as a bare name.
        assert_eq!(weapon_slug("braton_prime"), Some("braton_prime_set"));
        // …and a form is the same stall.
        assert_eq!(weapon_slug("braton_prime_incarnon"), Some("braton_prime_set"));
        // An adversary weapon is auctioned instead, and carries no item page.
        assert_eq!(weapon_slug("kuva_bramma"), None);
        assert_eq!(adversary_auction("kuva_bramma"), Some(("lich", "kuva_bramma")));
        assert_eq!(adversary_auction("tenet_tetra"), Some(("sister", "tenet_tetra")));
        // A blueprint-only weapon is listed nowhere, and says so by absence.
        assert_eq!(weapon_slug("braton"), None);
        assert_eq!(adversary_auction("braton"), None);
        assert!(arcane_slug("cascadia_empowered").is_some());
    }

    /// A VARIANT AND A FORM REACH THE SAME AUCTIONS, because one riven fits
    /// them all. Braton Prime is not listed under its own name and its
    /// Incarnon form is not listed at all; both are the Braton's auctions.
    #[test]
    fn a_variant_and_a_form_take_the_familys_auctions() {
        let base = riven_weapon_slug("braton").expect("the Braton is listed");
        assert_eq!(riven_weapon_slug("braton_prime"), Some(base));
        assert_eq!(riven_weapon_slug("braton_prime_incarnon"), Some(base));
        assert_eq!(riven_weapon_slug("mk1_braton"), Some(base));
        // A KITGUN'S TWO SLOT ENTRIES ARE ONE CHAMBER, and one riven fits it
        // either way — they carry the chamber part's own `internal_name`, so
        // both reach the same auctions.
        let chamber = riven_weapon_slug("catchmoon_primary").expect("the Catchmoon is listed");
        assert_eq!(riven_weapon_slug("catchmoon_secondary"), Some(chamber));
        // A SYNDICATE VARIANT IS ITS BASE WEAPON'S FAMILY, which is what lets
        // one Magistar riven be priced against both.
        assert_eq!(riven_weapon_slug("sancti_magistar"), riven_weapon_slug("magistar"));
        assert!(riven_weapon_slug("magistar").is_some());
    }

    /// The classes with a stat pool — `data/rivens/` also holds the survey and
    /// the exception tables, which have no `stats:` of their own.
    fn riven_classes() -> Vec<String> {
        crate::data::files_under("rivens/")
            .filter(|(_, text)| text.contains("\nstats:"))
            .filter_map(|(p, _)| {
                p.strip_prefix("rivens/")
                    .and_then(|f| f.strip_suffix(".yaml"))
                    .map(str::to_string)
            })
            .collect()
    }

    /// EVERY ROLLABLE STAT OR THE LINK IS A DIFFERENT SEARCH. A missing slug
    /// drops one of the roll's stats from the auction filter, which returns
    /// rivens the visitor is not holding — worse than offering no link.
    #[test]
    fn every_riven_stat_can_be_filtered_on() {
        for class in riven_classes() {
            for stat in crate::rivens_data::pool(&class) {
                assert!(
                    riven_stat_slug(&stat.id).is_some(),
                    "no auction filter for riven stat {} ({class})",
                    stat.id
                );
            }
        }
    }
}
