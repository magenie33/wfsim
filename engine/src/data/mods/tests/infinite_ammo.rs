//! THE CARDS THAT GRANT INFINITE AMMO, at the numbers DE's ammo-efficiency
//! changes give them (see notes: ammo_efficiency_cap): a chance on a headshot,
//! 10% and 0.5 s unranked, 40% and 2 s at max rank.
use super::*;
use crate::model::{BuffGrant, BuffTrigger, ModEffect, StackingBuff};

fn window(m: &crate::model::ModDef) -> StackingBuff {
    m.effects
        .iter()
        .find_map(|e| match e {
            ModEffect::GrantsStackingBuff(b) => Some(*b),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{}: no stacking buff", m.id))
}

#[test]
fn brain_storm_and_skull_shots_are_a_chance_for_infinite_ammo() {
    for (card, weapon) in [("brain_storm", "grakata"), ("skull_shots", "viper")] {
        let max = crate::data::mods::pool_for_weapon(weapon)
            .into_iter()
            .find(|m| m.id == card)
            .unwrap_or_else(|| panic!("{card} on the {weapon}"));
        let b = window(&max);
        assert_eq!(b.grant, BuffGrant::InfiniteAmmo, "{card}");
        assert_eq!(b.trigger, BuffTrigger::Headshot, "{card}");
        assert!((b.chance - 0.40).abs() < 1e-12, "{card}: {}", b.chance);
        assert!((b.duration - 2.0).abs() < 1e-12, "{card}: {}", b.duration);

        let unranked = crate::data::mods::at_rank(&format!("{card}@0")).expect("rank 0 is modelled");
        let b0 = window(&unranked);
        assert!((b0.chance - 0.10).abs() < 1e-12, "{card}: {}", b0.chance);
        assert!((b0.duration - 0.5).abs() < 1e-12, "{card}: {}", b0.duration);

        let info = desc_info(card).expect("the card has a description");
        assert_eq!(info.at(0), "On Headshot:\n10% Chance for Infinite Ammo for 0.5s");
        assert_eq!(info.at(info.max_rank), "On Headshot:\n40% Chance for Infinite Ammo for 2s");
    }
}

/// The two Incarnon perks the same changes move are still ammo EFFICIENCY,
/// and their cards say the new numbers.
#[test]
fn enough_for_everyone_and_crimson_overture_read_the_new_numbers() {
    let text = |id: &str| crate::data::evolutions::get(id).unwrap_or_else(|| panic!("{id}")).description.shown().to_string();
    for id in ["bronco_enough_for_everyone", "bronco_prime_enough_for_everyone"] {
        assert!(text(id).starts_with("+75% Ammo Efficiency"), "{id}: {}", text(id));
    }
    for id in [
        "dera_crimson_overture", "dera_vandal_crimson_overture", "boltor_crimson_overture",
        "telos_boltor_crimson_overture", "boltor_prime_crimson_overture",
    ] {
        let t = text(id);
        assert!(t.contains("+25% Ammo Efficiency for 5 seconds"), "{id}: {t}");
        assert!(t.to_lowercase().contains("ammo efficiency for 5 seconds. stacks up to 3x")
            || t.contains("+25% Ammo Efficiency for 5 seconds, stacks up to 3x"), "{id}: {t}");
    }
}
