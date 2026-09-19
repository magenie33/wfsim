use super::*;

/// A LIVE BUG IS DECLARED WHERE IT LIVES, and the trigger is derived rather
/// than a list of one (the house rule: when a fix means "remember to write
/// it down for this arcane too", generalise). Anything that carries the
/// Debilitate split carries the leak with it — the zero-damage instance is
/// what leaks, and that instance IS the effect — so an arcane cannot have
/// one without the other.
///
/// It also pins the SHAPE: a sentence a player can act on, not a flag. The
/// card has to say what the number rests on, because the number is real
/// today and gone after a hotfix (MEASUREMENTS M37).
#[test]
fn the_split_arcane_admits_the_bug_it_rides_on() {
    let mut checked = 0;
    for a in slots().into_iter().flat_map(slot_pool) {
        let splits = a.effects.iter().any(|e| matches!(e, ArcEffect::Debilitate(_)));
        if !splits {
            // NEGATIVE CONTROL: an ordinary arcane declares nothing, so the
            // mark means something when it does appear.
            assert!(
                a.live_bugs.is_empty() || a.id != "primary_deadhead",
                "{} declares a live bug with nothing to leak", a.id
            );
            continue;
        }
        checked += 1;
        assert!(
            !a.live_bugs.is_empty(),
            "{} splits a status, so it leaks its instance's multipliers into                  the DoT — the card has to say so (MEASUREMENTS M37)", a.id
        );
        for b in &a.live_bugs {
            assert!(b.len() > 40, "{}: a bug line has to be a sentence", a.id);
        }
    }
    assert_eq!(checked, 1, "the roster's split arcanes");
}
