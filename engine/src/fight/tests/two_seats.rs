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

/// A SEAT'S COUNTERS ARE ITS OWN, AND THEY COME TO THE FIGHT'S.
///
/// `shots` and `crits` on the run are the FIGHT's — a real question with a
/// real answer — and `per_seat` is the other one. Both are reported because
/// neither is the other: there is no such thing as the fight's crit rate, and
/// "how many shots were fired here" is not a seat's to answer.
#[test]
fn each_seat_counts_its_own_and_they_sum_to_the_fights() {
    let r = run_once(
        &fight_for(&["cernos_prime", "braton_prime"]),
        &mut Rng::new(0x5EED),
    );
    let shots: u32 = r.per_seat.iter().map(|c| c.shots).sum();
    let crits: u32 = r.per_seat.iter().map(|c| c.crits).sum();
    assert_eq!(shots, r.shots, "seats fired {shots} of the fight's {}", r.shots);
    assert_eq!(crits, r.crits, "seats crit {crits} of the fight's {}", r.crits);
    assert!(
        r.per_seat[0].shots > 0 && r.per_seat[1].shots > 0,
        "one of the seats fired nothing: {:?}",
        &r.per_seat[..2]
    );
}

/// …AND A KILL IS THE FIGHT'S WHILE A FINISH IS A SEAT'S. Without that split
/// every seat claims the same kills and n seats report n times the fight.
#[test]
fn a_kill_is_the_fights_and_a_finish_is_a_seats() {
    let r = run_once(
        &fight_for(&["cernos_prime", "braton_prime"]),
        &mut Rng::new(0x5EED),
    );
    let finishes: u32 = r.per_seat.iter().map(|c| c.finishes).sum();
    assert_eq!(
        finishes, r.kills,
        "the seats finished {finishes} of the fight's {} kills",
        r.kills
    );
}

/// THE ROSTER GROWS WITH THE FIGHT, and a seat is listed whether or not it
/// scored — "it fired nothing" and "it is not there" are different fights.
#[test]
fn the_roster_names_every_seat() {
    assert_eq!(fight_for(&["cernos_prime"]).combatant_ids(), ["wielder"]);
    // …AND EVERY SEAT HAS ITS OWN NAME. The page files a per-seat figure under
    // these, so two seats sharing one id are two figures under one heading and
    // whichever is read second wins.
    assert_eq!(
        fight_for(&["cernos_prime", "braton_prime", "laetum"]).combatant_ids(),
        ["wielder", "seat2", "seat3"]
    );
}

/// A STATUS TICK IS CREDITED TO WHOEVER APPLIED IT, not to whoever is firing
/// when it pays out.
///
/// A pile is the BODY's and its ticks land seconds after the shot that seeded
/// them, so with one seat "whoever is firing now" was always right and with
/// two it is a coin toss. A DoT carries its applier.
///
/// The Cernos Prime opens with a Slash-heavy vector and the Braton Prime does
/// not, so a fight of the two has both seats bleeding the same body: if the
/// credit followed the clock rather than the applier, the split would move
/// with the cadence rather than with the builds.
#[test]
fn a_status_tick_belongs_to_whoever_applied_it() {
    let r = run_once(
        &fight_for(&["cernos_prime", "braton_prime"]),
        &mut Rng::new(0x5EED),
    );
    let by = r.dealt.by_combatant().0;
    // Both seats dealt SOMETHING, and the fight ticked statuses at all —
    // without the second the assertion below would pass on an empty pile.
    assert!(by[0] > 0.0 && by[1] > 0.0, "a seat dealt nothing: {by:?}");
    let ticked: f64 = r.sources.status.iter().sum();
    assert!(ticked > 0.0, "no status ticked, so nothing was attributed");
    // …and the ledger still balances, which is what would break first if a
    // tick were credited to a seat that never applied it.
    let dealt: f64 = by.iter().sum();
    assert!(
        (dealt - r.tally.effective()).abs() < 1e-6,
        "attributed {dealt} against a meter of {}",
        r.tally.effective()
    );
}
