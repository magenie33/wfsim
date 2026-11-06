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

/// The strategy's elemental carriers are one element a row, 60/60: every card
/// is real, carries one primary element and status chance, a row is one element,
/// and the four rows are four elements. Every preset has an id, a name and run
/// terms the run bar can hold.
#[test]
fn the_built_in_searches_start_from_each_elements_dual_cards() {
    use crate::model::ModEffect;
    let p = search_presets();
    assert_eq!(p.element_starts.len(), 4, "one start per primary element");
    let mut seen = Vec::new();
    for row in &p.element_starts {
        let mut row_element = None;
        for id in row {
            let m = at_rank(&format!("{id}@0")).unwrap_or_else(|| panic!("{id} is not a card with ranks"));
            let elements: Vec<_> = m.effects.iter()
                .filter_map(|e| match e { ModEffect::Element(t, _) => Some(*t), _ => None }).collect();
            assert_eq!(elements.len(), 1, "{id} carries one element");
            assert!(m.effects.iter().any(|e| matches!(e, ModEffect::StatusChance(_))), "{id} carries status chance");
            assert!(row_element.is_none_or(|x| x == elements[0]), "{id} is not its row's element");
            row_element = Some(elements[0]);
        }
        assert!(!seen.contains(&row_element), "two rows are one element");
        seen.push(row_element);
    }
    for x in &p.presets {
        assert!(!x.id.is_empty() && !x.name.is_empty(), "{x:?}");
        assert!((1..=100).contains(&x.finalists) && [1, 10].contains(&x.candidate_runs), "{x:?}");
    }
}
