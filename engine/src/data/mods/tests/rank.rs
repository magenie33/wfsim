use super::*;

/// Hunter Track reads +15% at rank 0 and +90% at rank 5 (its yaml), and a
/// card's drain rises one per rank from its rank-0 cost.
#[test]
fn a_ranked_id_is_the_card_at_that_rank() {
    let max = class_pool("primary").into_iter().find(|m| m.id == "hunter_track").unwrap();
    let r0 = at_rank("hunter_track@0").expect("rank 0 exists");
    let dur = |m: &ModDef| {
        m.effects.iter().find_map(|e| match e {
            ModEffect::StatusDuration(x) => Some(*x),
            _ => None,
        })
    };
    assert_eq!(dur(&max), Some(0.90));
    assert!((dur(&r0).unwrap() - 0.15).abs() < 1e-12);
    assert!((dur(&at_rank("hunter_track@3").unwrap()).unwrap() - 0.60).abs() < 1e-12);
    assert_eq!(r0.id, "hunter_track@0");
    assert_eq!(r0.base_drain + 5, max.base_drain);
    assert_eq!(r0.family, Some("hunter_track"));
    assert_eq!(split_rank("hunter_track@0"), ("hunter_track", Some(0)));
    assert_eq!(ranked_id("hunter_track", 5, 5), "hunter_track");
}

/// Max rank is the bare id, and a card whose ladder is not linear refuses.
#[test]
fn a_rank_that_is_not_a_variant_is_refused() {
    assert!(at_rank("hunter_track@5").is_none(), "max rank is the bare id");
    assert!(at_rank("hunter_track").is_none());
    assert!(at_rank("no_such_card@0").is_none());
    assert!(at_rank("double_tap@0").is_none(), "its stack cap is not linear");
}

/// Every card the default list names exists and has a lower rank to try.
#[test]
fn every_rank_names_real_cards_with_lower_ranks() {
    let list = every_rank();
    assert!(list.mods.iter().any(|m| m == "hunter_track"));
    for id in &list.mods {
        assert!(at_rank(&format!("{id}@0")).is_some(), "{id} has no rank 0 to search");
    }
    for id in &list.arcanes {
        let d = crate::data::arcanes::slot_of(id)
            .and_then(|s| crate::data::arcanes::for_slot(s, id))
            .unwrap_or_else(|| panic!("{id} is not an arcane"));
        assert!(d.max_rank > 0, "{id} has no lower rank");
    }
}

/// A variant joins a pool only beside its card, and the two then exclude
/// each other through the family.
#[test]
fn with_ranks_adds_the_variant_beside_its_card() {
    let mut pool = class_pool("primary");
    let n = pool.len();
    with_ranks(&mut pool, ["hunter_track@1", "hunter_track@1", "serration@0", "hunter_track"]);
    assert_eq!(pool.len(), n + 1, "one variant; a card not in the pool adds nothing");
    let base = pool.iter().find(|m| m.id == "hunter_track").unwrap();
    assert_eq!(base.family, Some("hunter_track"));
}
