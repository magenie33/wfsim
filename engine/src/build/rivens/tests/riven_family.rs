//! A RIVEN IS THE WEAPON FAMILY'S, so every member of a family has to offer the
//! same card — **within one riven CLASS**.
//!
//! *"Riven mods can be used on variants of a particular weapon, including MK1,
//! Prime, Vandal, Wraith, Dex, Prisma, Mara, and Syndicate variants"*, so the
//! pool is a property of the FAMILY rather than of the entry: a stat one
//! variant can roll and another cannot is a card the editor offers on both, the
//! board accepts on one, and refuses on the other.
//!
//! THE CLASS IS PART OF THE KEY, and a KITGUN is why: `tombfinger_primary` and
//! `tombfinger_secondary` are one family and two riven types — as a primary the
//! chamber takes a RIFLE riven, as a secondary a PISTOL one — so grouping by
//! family alone reads two families' worth of card as a disagreement inside one.
//!
//! Derived from the roster rather than from a list of families, so a weapon
//! added tomorrow is covered by nobody.
use std::collections::BTreeMap;

/// One weapon's view of a card: its id, and the stats it may NOT roll.
type Member = (&'static str, Vec<&'static str>);

#[test]
fn a_kitguns_two_slots_take_one_card() {
    // Kitgun: "Kitgun Riven mods for a specific chamber can be equipped on
    // both the primary and secondary versions" — one card, one pool.
    let mut by_family: BTreeMap<&str, Vec<(&str, Option<&str>)>> = BTreeMap::new();
    for w in crate::data::weapons::all().iter().filter(|w| w.kitgun.is_some()) {
        let family = w.riven_family.as_deref().expect("a Kitgun names its chamber's family");
        by_family.entry(family).or_default().push((w.id.as_str(), super::class_for_weapon(&w.id)));
    }
    assert!(by_family.values().filter(|m| m.len() == 2).count() >= 6, "{by_family:?}");
    for (family, members) in &by_family {
        assert!(
            members.iter().all(|(_, c)| *c == Some("pistol")),
            "{family}: every slot takes the chamber's pistol riven: {members:?}"
        );
    }
}

#[test]
fn every_member_of_a_riven_family_rolls_the_same_pool() {
    // Keyed by the CARD — a (family, riven class) pair, not a family.
    let mut by_card: BTreeMap<(&str, &str), Vec<Member>> = BTreeMap::new();
    for w in crate::data::weapons::all() {
        let Some(family) = w.riven_family.as_deref() else { continue };
        let class = super::class_for_weapon(&w.id).unwrap_or("");
        let mut excluded = super::excluded_for(&w.id);
        excluded.sort_unstable();
        by_card
            .entry((family, class))
            .or_default()
            .push((w.id.as_str(), excluded));
    }
    let mut bad = Vec::new();
    for ((family, class), members) in &by_card {
        let (first_id, first_excluded) = &members[0];
        for (id, excluded) in &members[1..] {
            if excluded != first_excluded {
                bad.push(format!(
                    "{family} ({class}): {first_id} excludes {first_excluded:?}                          but {id} excludes {excluded:?}"
                ));
            }
        }
    }
    assert!(bad.is_empty(), "riven pools differ inside one card:
{}", bad.join("
"));
    // A FLOOR, so the sweep cannot pass by finding nothing: the roster has
    // plenty of families with several members and this must be looking at
    // them rather than at a roster it failed to read.
    let shared = by_card.values().filter(|m| m.len() > 1).count();
    assert!(shared > 20, "only {shared} cards are shared by more than one weapon");
}
