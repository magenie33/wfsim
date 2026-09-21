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
    let base = crate::model::WeaponBase::from_data(
        "furis_incarnon",
        true,
        &["furis_evo1_incarnon_form", evo],
    );
    let p = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
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
    for e in crate::data::evolutions::pool() {
        for card in e.buff_cards() {
            let base = crate::model::WeaponBase::from_data(
                &e.weapon,
                true,
                &[e.id.as_str()],
            );
            let p = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
            let arena = crate::arena::Arena::training(30.0);
            // `for_panel`, NOT `from_panel` — the same decision every real
            // surface makes. A melee Incarnon's card is backed by a fight
            // built from two panels, and asking the one-panel constructor
            // here would fail a card that works.
            let params = FightParams::for_panel(&p, &arena, &ArcaneFx::none(), || {
                let b = crate::model::WeaponBase::from_data(&e.weapon, true, &[]);
                crate::build::loadout::resolve(&b, &[], crate::model::StackPolicy::Emergent)
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

/// **LEADED GAS PAYS BOTH HALVES, AND ONLY WHILE THE WINDOW IS OPEN.**
///
/// One buff, two stats, one clock — so the assertions are paired: a fight that
/// never lands a weak point gets neither, and the same fight aiming at a head
/// gets both. What each half REACHES is different — the element joins the
/// damage vector, the status chance its relative bracket — so a test on damage
/// alone would pass on a card that granted only one of them.
#[test]
fn leaded_gas_turns_gas_and_status_on_at_a_weak_point() {
    let card =
        crate::model::WeakpointBuff { element: DamageType::Gas, bonus: 3.0, duration: 6.0 };
    let fight = |head: bool, card: Option<crate::model::WeakpointBuff>| {
        let p = FightParams {
            damage: DamageVector::new().with(DamageType::Impact, 100.0),
            dot_modified_base: Some(100.0),
            status_chance: 0.1,
            base_status_chance: 0.1,
            crit_multiplier: 1.0,
            fire_rate: 2.0,
            magazine_size: 1e9,
            duration_seconds: 20.0,
            body_parts: if head { all_head() } else { mono_body(1.0) },
            on_weakpoint: card,
            arcane: ArcaneFx::none(),
            ..FightParams::default()
        };
        monte_carlo(&p, 200, 0x1EAD)
    };
    // A CARD THAT NEVER TRIGGERS MOVES NOTHING, which is the control that makes
    // the rest mean anything.
    let (body_bare, body_card) = (fight(false, None), fight(false, Some(card)));
    assert!(
        (body_bare.mean_damage - body_card.mean_damage).abs() < 1e-9,
        "{} vs {}",
        body_bare.mean_damage,
        body_card.mean_damage
    );
    let (head_bare, head_card) = (fight(true, None), fight(true, Some(card)));
    // +300% OF THE MODIFIED BASE AS GAS is most of the instance.
    assert!(
        head_card.mean_damage > head_bare.mean_damage * 2.0,
        "the element reaches the hit: {:.0} -> {:.0}",
        head_bare.mean_damage,
        head_card.mean_damage
    );
    // …AND 0.1 STATUS CHANCE BECOMES 0.4, which the proc count shows and the
    // damage could not have.
    assert!(
        head_card.mean_procs > head_bare.mean_procs * 3.0,
        "{} -> {} procs",
        head_bare.mean_procs,
        head_card.mean_procs
    );
}

/// …AND THE ELEMENT REACHES THE GAS CLOUD, which is what the card is for.
///
/// VERBATIM: *"The Gas damage bonus will increase the damage of gas status
/// effects from the Vesper 77 while the buff is active"* — and the reason that
/// is worth saying at all is the rule beside it: a Heat or Toxin mod adds
/// nothing to a Gas tick, so a bonus for GAS is one of the only things that
/// ever reaches one.
#[test]
fn leaded_gas_reaches_the_cloud() {
    let dot_of = |card: Option<crate::model::WeakpointBuff>| {
        let p = FightParams {
            damage: DamageVector::new().with(DamageType::Gas, 100.0),
            dot_modified_base: Some(100.0),
            // EVERY SHOT PROCS, so the count is fixed and the only thing left
            // that can move is what a tick is worth.
            status_chance: 1.0,
            base_status_chance: 1.0,
            forced_procs: vec![DamageType::Gas],
            crit_multiplier: 1.0,
            fire_rate: 1.0,
            magazine_size: 1e9,
            duration_seconds: 20.0,
            body_parts: all_head(),
            on_weakpoint: card,
            arcane: ArcaneFx::none(),
            ..FightParams::default()
        };
        let s = monte_carlo(&p, 200, 0x1EAD);
        (s.mean_dot_damage, s.mean_dot_ticks)
    };
    let (bare, bare_ticks) = dot_of(None);
    let (carded, carded_ticks) = dot_of(Some(crate::model::WeakpointBuff {
        element: DamageType::Gas,
        bonus: 3.0,
        duration: 6.0,
    }));
    assert!(
        (bare_ticks - carded_ticks).abs() < 1.0,
        "the same clouds either way: {bare_ticks} vs {carded_ticks}"
    );
    assert!(
        carded > bare * 1.5,
        "the bonus is in the Gas tick's bracket: {bare:.0} -> {carded:.0}"
    );
}
