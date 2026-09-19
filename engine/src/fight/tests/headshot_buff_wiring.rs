//! A BUFF IS THREE PLACES, and a card that appears without them lies.
//!
//! `EvolutionDef::buff_cards` decides what the UI OFFERS. The sim decides what
//! it DOES, and that takes three separate arms in this file: the roster (what
//! the replay draws), the sampler (what its curve reads), and the config (what
//! a locked/seeded stack count means). Headcracker shipped with the card and
//! none of the three for one commit, which is the worst shape available — the
//! panel offered a control, the control did nothing, and nothing said so.
//!
//! Asserted for BOTH on-headshot buffs, because the failure is silent: a buff
//! missing from the roster simply never appears, and no test that looks only at
//! damage would notice.
use super::*;

fn roster_of(evo: &str) -> Vec<String> {
    let base = crate::loadout::WeaponBase::from_data(
        "furis_incarnon",
        true,
        &["furis_evo1_incarnon_form", evo],
    );
    let p = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent);
    let params = FightParams::from_panel(&p, &crate::arena::Arena::training(30.0), &ArcaneFx::none());
    params.buff_roster().into_iter().map(|b| b.id).collect()
}

#[test]
fn headcracker_is_on_the_replay_roster() {
    let r = roster_of("furis_headcracker");
    assert!(
        r.iter().any(|id| id == "on_headshot_fire_rate"),
        "the card is offered, so the curve must exist too: {r:?}"
    );
}

/// ...and it is NOT there when the perk is not taken — a roster that always
/// lists it would draw a flat zero line and read as a finding.
#[test]
fn and_only_when_the_perk_is_taken() {
    let r = roster_of("furis_elemental_balance");
    assert!(!r.iter().any(|id| id == "on_headshot_fire_rate"), "{r:?}");
}

/// EVERY card the evolution loader offers must have a sim arm behind it.
/// This is the general form of the bug rather than the instance: it walks
/// the whole evolution pool, so the next perk to gain a card fails here
/// until the three arms exist.
#[test]
fn every_evolution_buff_card_is_backed_by_the_sim() {
    for e in crate::evolutions_data::pool() {
        for card in e.buff_cards() {
            let base = crate::loadout::WeaponBase::from_data(
                &e.weapon,
                true,
                &[e.id.as_str()],
            );
            let p = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent);
            let arena = crate::arena::Arena::training(30.0);
            // `for_panel`, NOT `from_panel` — the same decision every real
            // surface makes. A melee Incarnon's card is backed by a fight
            // built from two panels, and asking the one-panel constructor
            // here would fail a card that works.
            let params = FightParams::for_panel(&p, &arena, &ArcaneFx::none(), || {
                let b = crate::loadout::WeaponBase::from_data(&e.weapon, true, &[]);
                crate::loadout::resolve(&b, &[], crate::loadout::StackPolicy::Emergent)
            });
            let listed = params.buff_roster().into_iter().any(|b| b.id == card.id);
            assert!(
                listed,
                "{} offers a buff card `{}` the sim never rosters — the panel would \
                 show a control that does nothing",
                e.id, card.id
            );
        }
    }
}
