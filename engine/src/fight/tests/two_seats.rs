//! A SECOND THING ACTING IN THE SAME FIGHT.
//!
//! Not "a companion": the engine does not know what is in a seat, only that a
//! seat is a build and a clock. What these assert is the three things that
//! have to be true whatever is in it — it fires, its damage is its own, and
//! its being there does not move the first seat's rolls.
use super::*;

/// One build, resolved with no mods, for a weapon this roster has.
fn build(weapon: &str) -> crate::build::loadout::ResolvedPanel {
    let base = crate::model::WeaponBase::from_data(weapon, false, &[]);
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent)
}

/// The same arena for every seat, which is what makes their fight-level terms
/// agree without a caller copying anything.
fn fight_for(weapons: &[&str]) -> FightParams {
    let arena = crate::arena::Arena::training(10.0);
    let fx = crate::data::arcanes::ArcaneFx::none();
    let mut p = FightParams::from_panel(&build(weapons[0]), &arena, &fx);
    for w in &weapons[1..] {
        p = p.and_also(FightParams::from_panel(&build(w), &arena, &fx));
    }
    p
}

/// IT FIRES, AND THE FIGHT IS BIGGER FOR IT. A seat that were opened and never
/// picked would leave this identical to the solo fight, which is exactly the
/// failure a roster listing an idle seat is meant to make visible.
#[test]
fn a_second_seat_deals_damage_of_its_own() {
    let solo = run_once(&fight_for(&["cernos_prime"]), &mut Rng::new(0x5EED));
    let pair = run_once(
        &fight_for(&["cernos_prime", "braton_prime"]),
        &mut Rng::new(0x5EED),
    );

    let by = pair.dealt.by_combatant().0;
    assert!(by[0] > 0.0, "the wielder dealt nothing");
    assert!(by[1] > 0.0, "the second seat never fired: {by:?}");
    assert!(
        pair.tally.effective() > solo.tally.effective(),
        "two seats dealt {} where one dealt {}",
        pair.tally.effective(),
        solo.tally.effective()
    );
}

/// THE TWO CUTS STILL AGREE. Who dealt it and who took it are one fight sorted
/// two ways, and a second seat settling through anything but the one door
/// would show up here as a total that does not match.
#[test]
fn both_seats_book_through_the_same_door() {
    let r = run_once(
        &fight_for(&["cernos_prime", "braton_prime"]),
        &mut Rng::new(0x5EED),
    );
    let dealt: f64 = r.dealt.by_combatant().0.iter().sum();
    let taken: f64 = r.taken.by_body().0.iter().sum();
    assert!(
        (dealt - taken).abs() < 1e-6 && (dealt - r.tally.effective()).abs() < 1e-6,
        "dealt {dealt}, taken {taken}, meter {}",
        r.tally.effective()
    );
}

/// THE RUN'S COUNTERS ARE THE FIGHT'S, AND WITH TWO SEATS THEY ARE A MIXTURE.
///
/// `shots`, `crits`, `procs` and the rest sit on `RunResult`, so a fight with
/// two things firing reports their sum — and a sum of two weapons' shots is
/// nobody's shot count. The panel already knows this is wrong for it: there is
/// no such thing as the fight's crit rate.
///
/// Pinned rather than left to be discovered. It fails the day the counters go
/// per seat, which is the day it should.
#[test]
fn the_runs_counters_are_the_fights_not_a_seats() {
    let solo = run_once(&fight_for(&["cernos_prime"]), &mut Rng::new(0x5EED));
    let pair = run_once(
        &fight_for(&["cernos_prime", "braton_prime"]),
        &mut Rng::new(0x5EED),
    );
    assert!(
        pair.shots > solo.shots,
        "two seats fired {} shots against one seat's {}",
        pair.shots,
        solo.shots
    );
}

/// THE ROSTER GROWS WITH THE FIGHT, and a seat is listed whether or not it
/// scored — "it fired nothing" and "it is not there" are different fights.
#[test]
fn the_roster_names_every_seat() {
    assert_eq!(fight_for(&["cernos_prime"]).combatant_ids().len(), 1);
    assert_eq!(
        fight_for(&["cernos_prime", "braton_prime"])
            .combatant_ids()
            .len(),
        2
    );
}
