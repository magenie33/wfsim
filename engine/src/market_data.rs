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
    riven_families: BTreeMap<String, String>,
    #[serde(default)]
    riven_stats: BTreeMap<String, String>,
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

/// The riven-auction slug for a weapon id. Absent for a weapon with no riven.
///
/// THE WALK IS OURS, THE SLUG IS THEIRS. A form carries no family of its own,
/// so it takes the one belonging to the entry it transforms from; a weapon
/// that declares no family is its own. Doing this here rather than spelling a
/// row per entry into the generated table means a new form gets its link
/// without anyone re-running the script.
pub fn riven_weapon_slug(id: &str) -> Option<&'static str> {
    let by_id = |want: &str| crate::weapons_data::all().iter().find(|w| w.id == want);
    let w = by_id(id)?;
    let base = w.transform_group.as_deref().and_then(by_id).unwrap_or(w);
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

    /// A VARIANT AND A FORM REACH THE SAME AUCTIONS, because one riven fits
    /// them all. Braton Prime is not listed under its own name and its
    /// Incarnon form is not listed at all; both are the Braton's auctions.
    #[test]
    fn a_variant_and_a_form_take_the_familys_auctions() {
        let base = riven_weapon_slug("braton").expect("the Braton is listed");
        assert_eq!(riven_weapon_slug("braton_prime"), Some(base));
        assert_eq!(riven_weapon_slug("braton_prime_incarnon"), Some(base));
        assert_eq!(riven_weapon_slug("mk1_braton"), Some(base));
        // …and a weapon nobody lists still gets nothing rather than a guess.
        assert_eq!(riven_weapon_slug("catchmoon_primary"), None);
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
