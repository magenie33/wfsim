use super::*;

use crate::model::IndirectStat;

fn source() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../../tests/fixtures/client/precision-inputs-2026-10-28.json")).unwrap()
}

fn expected_bits(entry: &serde_json::Value) -> u32 {
    let text = entry["float32_bits_le"].as_str().unwrap();
    u32::from_le_bytes(std::array::from_fn(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).unwrap()))
}

#[test]
fn parsed_client_rank_limits_reach_the_mod_catalog() {
    let source = source();
    for entry in source["mod_limits"].as_array().unwrap() {
        let id = entry["id"].as_str().unwrap();
        let item = crate::data::warframes::mod_by_id(id).unwrap();
        assert_eq!(item.internal_name.as_deref(), entry["internal_name"].as_str(), "{id}");
        let maximum = source["quality_max_ranks"][entry["fusion_limit"].as_str().unwrap()].as_u64().unwrap() as u32;
        assert_eq!(item.max_rank, maximum, "{id}");
        assert_eq!(item.base_drain + maximum, entry["max_drain"].as_u64().unwrap() as u32, "{id}");
    }
}

#[test]
fn parsed_client_evolution_inputs_reach_the_applied_weapon() {
    let source = source();
    for entry in source["evolution_inputs"].as_array().unwrap() {
        let id = entry["id"].as_str().unwrap();
        let item = get(id).unwrap();
        assert_eq!(item.weapon, entry["weapon"].as_str().unwrap(), "{id}");
        assert_eq!(item.tier, entry["tier"].as_u64().unwrap() as u32, "{id}");
        assert_eq!(item.position_in_tier, entry["position_in_tier"].as_u64().unwrap() as u32, "{id}");
        let text = crate::data::file(entry["weapon_source_file"].as_str().unwrap()).unwrap();
        let weapon: serde_norway::Value = serde_norway::from_str(text).unwrap();
        assert_eq!(weapon["internal_name"].as_str(), entry["weapon_internal_name"].as_str(), "{id}");

        let index = entry["effect_index"].as_u64().unwrap() as usize;
        let value = match (&item.effects[index], entry["attribute"].as_str().unwrap(), entry["operation"].as_str().unwrap()) {
            (EvoEffect::FlatBaseCritMultiplier(value), "WEAPON_CRIT_DAMAGE", "ADD_BASE") => *value,
            (EvoEffect::FireRateBonus { value, min_sprint, needs_melee_equipped }, "WEAPON_FIRE_RATE", "STACKING_MULTIPLY") => {
                assert_eq!(*min_sprint, 0.0, "{id}");
                assert!(!needs_melee_equipped, "{id}");
                *value
            }
            (EvoEffect::Indirect(IndirectStat::ProjectileSpeed, value), "WEAPON_PROJECTILE_SPEED", "STACKING_MULTIPLY") => *value,
            other => panic!("{id}: mismatched quantity {other:?}"),
        };
        let raw = entry["raw_token"].as_str().unwrap().parse::<f32>().unwrap();
        assert_eq!(raw.to_bits(), expected_bits(entry), "{id}: source token");
        assert_eq!((value as f32).to_bits(), expected_bits(entry), "{id}: loaded input");

        let before = WeaponBase::from_data(&item.weapon, true, &[]);
        let after = WeaponBase::from_data(&item.weapon, true, &[id]);
        let delta = match entry["attribute"].as_str().unwrap() {
            "WEAPON_CRIT_DAMAGE" => after.base_crit_damage - before.base_crit_damage,
            "WEAPON_FIRE_RATE" => after.evo_fire_rate_bonus - before.evo_fire_rate_bonus,
            "WEAPON_PROJECTILE_SPEED" => {
                let speed = |base: &WeaponBase| base.indirect.iter()
                    .find(|(stat, _)| *stat == IndirectStat::ProjectileSpeed).map_or(0.0, |(_, value)| *value);
                speed(&after) - speed(&before)
            }
            other => panic!("{id}: unknown quantity {other}"),
        };
        assert!((delta - value).abs() < 1e-12, "{id}: applied {delta} vs {value}");
    }
}
