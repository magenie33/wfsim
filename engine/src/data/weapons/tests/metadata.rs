use super::*;

#[test]
fn repeated_panels_borrow_the_catalogs_metadata() {
    for id in ["braton", "torid", "valkyr_talons", "rattleguts_primary", "rattleguts_secondary", "magistar", "ceramic_dagger_heavy", "plague_kripath_two_handed"] {
        let raw = spec(id).unwrap();
        let first = base_panel(id, false);
        for _ in 0..16 {
            let panel = base_panel(id, false);
            assert!(std::ptr::eq(panel.class, raw.class.as_str()), "{id}: class was copied");
            assert_eq!(panel.slot, raw.rules_slot(), "{id}: slot changed");
            // Weapon-type aliases are static literals; their addresses may differ across codegen units.
            if raw.rules_slot() == raw.slot.as_str() {
                assert!(std::ptr::eq(panel.slot, raw.slot.as_str()), "{id}: slot was copied");
            }
            assert!(std::ptr::eq(panel.mod_pools, first.mod_pools), "{id}: pool view was rebuilt");
            assert_eq!(panel.mod_pools.len(), raw.mod_pools.len());
            for (actual, expected) in panel.mod_pools.iter().zip(&raw.mod_pools) {
                assert!(std::ptr::eq(*actual, expected.as_str()), "{id}: pool name was copied");
            }
            match (panel.summoned_by, raw.summoned_by.as_deref()) {
                (Some(actual), Some(expected)) => assert!(std::ptr::eq(actual, expected)),
                (None, None) => {},
                _ => panic!("{id}: summoning metadata changed"),
            }
        }
    }
}

#[test]
fn a_riven_class_borrows_the_catalog_string() {
    let raw = spec("braton").unwrap();
    assert!(raw.riven_class.is_none(), "fixture must use the mod-pool fallback");
    let expected = raw.mod_pools.iter().rev()
        .find(|pool| !crate::build::rivens::pool(pool).is_empty()).unwrap();
    for _ in 0..32 {
        assert!(std::ptr::eq(crate::build::rivens::class_for_weapon("braton").unwrap(), expected.as_str()));
    }
}
