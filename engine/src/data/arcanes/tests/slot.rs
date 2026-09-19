use super::*;

/// Arcane pools are DISCOVERED from `data/arcanes/<slot>/`, mirroring the
/// mod classes: adding `data/arcanes/primary/` is a data change, not a
/// code change. Ids stay globally unique, so a lookup never needs a slot.
#[test]
fn slots_come_from_the_data_tree() {
    let ss = slots();
    assert!(ss.contains(&"secondary"), "expected the secondary slot, got {ss:?}");
    for s in &ss {
        assert!(!slot_pool(s).is_empty(), "slot {s} has no arcanes");
    }
    assert!(slot_pool("no_such_slot").is_empty());
    assert_eq!(slot_of("secondary_merciless"), Some("secondary"));
    assert_eq!(slot_of("no_such_arcane"), None);
    // Every id is unique across slots — what makes `secondary(id)` safe.
    let mut ids: Vec<&str> = ss.iter().flat_map(|s| slot_pool(s).iter().map(|a| a.id.as_str())).collect();
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), n, "arcane ids collide across slots");
}

/// EVERY EFFECT AN ARCANE DOES NOTHING WITH IS ON ITS CARD.
///
/// `ArcEffect::Inert` is where an effect goes when the loader has no arm
/// for its kind. Nothing printed it — `describe_at` skips it and
/// `has_unmodeled` does not count it — so three arcanes promised a stat
/// they silently did not apply. This pins the list: a NEW inert effect
/// fails here and has to be argued for, and an implemented one has to be
/// deleted from it.
#[test]
fn an_arcane_that_does_nothing_with_an_effect_says_so() {
    let mut found: Vec<String> = Vec::new();
    for s in slots() {
        for a in slot_pool(s) {
            for why in a.unmodeled_effects() {
                found.push(format!("{} :: {why}", a.id));
            }
        }
    }
    found.sort();
    let expected = [
        // Recoil is a stat this arena has no shot placement to spend, and
        // both Deadheads carry a reduction.
        "primary_deadhead :: recoil reduction",
        "secondary_deadhead :: recoil reduction",
        // A MELEE combo counter, on a gun arcane: the bonus is real and it
        // acts on something no weapon in this roster has.
        "primary_dexterity :: combo duration bonus",
        "secondary_dexterity :: combo duration bonus",
        // Overguard is Tenno survivability, not weapon damage.
        "secondary_fortifier :: overguard on damage",
    ];
    let mut expected: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(found, expected, "the inert-arcane list moved");

    // …and the disclosure is what a card would show, not an internal name.
    let dh = secondary("secondary_deadhead").expect("secondary deadhead");
    assert!(dh.unmodeled_effects().iter().all(|w| !w.contains('_')));
}
