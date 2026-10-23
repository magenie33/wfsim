use super::*;

/// **A BUFF OPENS ABOVE ZERO ONLY BECAUSE THE PRECOMBAT LIST EARNED IT.**
///
/// Every pile opens at zero and the fight earns it, unless the player builds it
/// alone before the fight (`data::apl::Precombat`). So a card that opens full
/// is a card whose perk has a precombat press, and the reverse — a card can
/// no more open full by resemblance than it can be earned without a press.
#[test]
fn a_card_opens_full_exactly_when_the_precombat_list_earns_it() {
    let mut wrong: Vec<String> = Vec::new();
    for def in pool() {
        let earned = def.effects.iter().any(|e| match e {
            EvoEffect::StackingGrant { precombat, .. } => *precombat,
            // Static piles whose press the fight never makes: an ability cast
            // (Fevered Frenzy) and an empty reload kept forever (Reified Bane).
            EvoEffect::AssumedMaxMultishot { .. } | EvoEffect::FlatBaseDamageOnEmptyReload(_) => true,
            _ => false,
        });
        for card in def.buff_cards() {
            if (card.opens_at == CardOpens::Full) != earned {
                wrong.push(format!("{} ({}) opens {:?}, earned before the fight: {earned}", def.id, card.id, card.opens_at));
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}

/// **ONLY WHAT A PLAYER DOES ALONE IS EARNED BEFORE THE FIGHT**, and only for
/// certain: a pile on a hit or a kill is earned against an enemy, and a rolled
/// trigger would make the opening a draw. `earn_before_the_fight` skips both
/// silently, so this is where a file stating one fails.
#[test]
fn precombat_only_on_a_trigger_a_player_performs_alone() {
    let mut wrong: Vec<String> = Vec::new();
    for def in pool() {
        for e in &def.effects {
            if let EvoEffect::StackingGrant { trigger, chance, precombat: true, .. } = e {
                if !trigger.is_precombat() || *chance < 1.0 {
                    wrong.push(format!("{}: {} at {chance}", def.id, trigger.id()));
                }
            }
        }
    }
    for class in crate::data::mods::classes() {
        for m in crate::data::mods::load_class(class) {
            for e in &m.effects {
                if let crate::model::ModEffect::GrantsStackingBuff(b) = e {
                    if b.precombat && (!b.trigger.is_precombat() || b.chance < 1.0) {
                        wrong.push(format!("{}: {} at {}", m.id, b.trigger.id(), b.chance));
                    }
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
}
