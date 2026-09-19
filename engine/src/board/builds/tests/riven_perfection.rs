//! THE CARD'S SIGN IS NOT THE BUILD'S DIRECTION — the case the whole design
//! rests on, measured through the simulator rather than argued.
//!
//! Three weapons pay *"50% chance to deal +2000% damage on non-critical hits"* in
//! their Incarnon form. On those, critical chance is a LIABILITY — a Laetum
//! Incarnon crit is worth x2.2 where a non-crit is worth `0.5 x 21 + 0.5 x 1 =
//! 11` — so a riven whose MALUS is critical chance wants it as DEEP as it goes,
//! and the same shape on an ordinary weapon wants it as shallow. The perk says
//! the stat is one to ask about; the fight says which end. Neither is ever told
//! which way is up.
//!
//! A MALUS'S ROLL SCALES ITS MAGNITUDE, NOT ITS VALUE, so `ROLL_MAX` on a malus
//! is the DEEPEST one and "the top of the band" and "the better stat" are
//! opposites there. The band ends are named by their roll below for that
//! reason: `deep` and `shallow` are a reading of the number and belong in
//! prose, not in a variable somebody has to get right twice.
use crate::build::rivens::{
    ambiguous_stats, best_roll, god_roll, RivenShape, RivenSpec, ROLL_MAX, ROLL_MIN,
};

/// THE WHOLE LADDER, because Devouring Attrition is TIER 5 and a tier is
/// only open when the ones below it are filled — a set with a gap is
/// trimmed to its longest legal prefix, so naming tier 1 and tier 5 alone
/// applies neither.
const ATTRITION: &[&str] = &[
    "laetum_evo1_incarnon_form",
    "laetum_rapid_wrath",
    "laetum_awakened_readiness",
    "laetum_incarnon_efficiency",
    "laetum_devouring_attrition",
];

/// A riven whose MALUS is critical chance, and nothing else that could
/// confuse the reading.
fn negative_crit() -> RivenShape {
    RivenShape {
        bonuses: vec!["damage".into(), "multishot".into()],
        malus: Some("critical_chance".into()),
    }
}

/// That shape with every bonus at its ceiling and the malus at `malus_roll`.
fn at(shape: &RivenShape, malus_roll: f64) -> RivenSpec {
    let mut sp = god_roll(shape, "pistol");
    for b in sp.bonuses.iter_mut() {
        b.roll = ROLL_MAX;
    }
    if let Some(m) = sp.malus.as_mut() {
        m.roll = malus_roll;
    }
    sp
}

fn fight(weapon: &str, evos: &[&str], spec: &RivenSpec) -> f64 {
    let disposition =
        crate::data::weapons::spec(weapon).and_then(|s| s.disposition).unwrap_or(1.0);
    let riven = spec.to_mod_def(
        Box::leak(format!("riven:{weapon}").into_boxed_str()), disposition);
    let base = crate::model::WeaponBase::from_data(weapon, true, evos);
    let tenno = crate::data::tenno::default_tenno();
    let panel = crate::build::loadout::resolve_for(
        &base, &[&riven], crate::model::StackPolicy::Emergent, tenno);
    let arena = crate::arena::Arena::training(12.0);
    let dp = crate::fight::FightParams::from_panel(
        &panel, &arena, &crate::data::arcanes::ArcaneFx::none());
    crate::fight::monte_carlo(&dp, 120, 7).mean_damage
}

#[test]
fn a_negative_crit_riven_goes_the_other_way_on_a_devouring_attrition_weapon() {
    let shape = negative_crit();

    // THE LAETUM, Incarnon, with Devouring Attrition taken: the DEEPEST
    // crit malus pays MOST, because every crit is a roll that did not pay
    // 21x.
    let deep = fight("laetum_incarnon", ATTRITION, &at(&shape, ROLL_MAX));
    let shallow = fight("laetum_incarnon", ATTRITION, &at(&shape, ROLL_MIN));
    assert!(
        deep > shallow,
        "Devouring Attrition: the deepest crit malus should pay MOST ({deep} vs {shallow})"
    );

    // …AND AN ORDINARY WEAPON GOES THE OTHER WAY, which is what makes the
    // first half evidence rather than a coincidence: same shape, same stat,
    // same sign on the card, opposite end of the band.
    let deep_p = fight("laetum", &[], &at(&shape, ROLL_MAX));
    let shallow_p = fight("laetum", &[], &at(&shape, ROLL_MIN));
    assert!(
        shallow_p > deep_p,
        "without Devouring Attrition the crit malus should be SHALLOW \
             ({shallow_p} vs {deep_p})"
    );

    // AND THE PIPELINE FINDS BOTH WITHOUT BEING TOLD. The weapon's row
    // says critical chance is a stat to ask about; the fight says which
    // end. No per-stat table and no sign convention — and on a weapon with
    // no row the stat is not ambiguous at all, which is the CHEAP half of
    // the same answer.
    let asked = ambiguous_stats(&shape, "laetum");
    assert_eq!(
        asked.iter().map(String::as_str).collect::<Vec<_>>(),
        vec!["critical_chance"],
        "the weapon's own row names the stat it takes the sign off"
    );
    let with = best_roll(&shape, "pistol", &asked, |sp| {
        Some((fight("laetum_incarnon", ATTRITION, sp), 0.0))
    });
    assert_eq!(
        with.malus.as_ref().unwrap().roll,
        ROLL_MAX,
        "on Devouring Attrition the malus belongs at its deepest"
    );
    // …AND ON A WEAPON WITH NO ROW, NOTHING IS ASKED. The god roll is the
    // answer and it is the right one: nothing there pays for not critting,
    // so a deeper crit malus is just a worse card.
    let none = ambiguous_stats(&shape, "lex_prime");
    assert!(none.is_empty(), "{none:?}");
    let without = best_roll(&shape, "pistol", &none, |_| unreachable!("no fight is run"));
    assert_eq!(
        without.malus.as_ref().unwrap().roll,
        ROLL_MIN,
        "without it, at its shallowest"
    );
    // Both agree about the BONUSES, which is the uninteresting half and the
    // one a per-stat table would have got right.
    assert!(with.bonuses.iter().all(|b| b.roll == ROLL_MAX));
    assert!(without.bonuses.iter().all(|b| b.roll == ROLL_MAX));
}
