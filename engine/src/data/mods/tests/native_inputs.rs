use super::*;

#[test]
fn original_mod_rank_inputs_reach_the_loaded_effects_without_interpolation() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../../../tests/fixtures/client/upgrade-ranks-2026-10-28.json")).unwrap();
    let entries = fixture["entries"].as_array().unwrap();
    let bindings = fixture["mod_inputs"].as_array().unwrap();
    assert_eq!(bindings.len(), 150);
    let mut checked = 0;
    for binding in bindings {
        let path = binding["path"].as_str().unwrap().strip_prefix("data/").unwrap();
        let text = crate::data::files_under("mods/").find(|(p, _)| *p == path).unwrap().1;
        let raw: ModFile = serde_norway::from_str(text).unwrap();
        let index = binding["effect_index"].as_u64().unwrap() as usize;
        let source = &binding["source"];
        let original = entries.iter().find(|row| row["owner"] == source["owner"]
            && row["array"] == source["array"] && row["index"] == source["index"]).unwrap();
        for rank in 0..=raw.max_rank {
            let compiled = effect_at_rank(&raw.effects[index], rank, raw.max_rank);
            assert!(effect(&raw.id, &compiled).is_some(), "{} must reach its modeled consumer", raw.id);
            let amount = n(&compiled, "rankMax").unwrap();
            assert_eq!(format!("{:08x}", (amount as f32).to_bits().swap_bytes()),
                original["bits"][rank as usize].as_str().unwrap(), "{} rank{rank}", raw.id);
            checked += 1;
        }
    }
    assert!(checked > 800);
}

#[test]
fn serration_pool_and_lower_ranks_use_the_native_transform() {
    let pool = class_pool("rifle");
    let card = pool.iter().find(|m| m.id == "serration").unwrap();
    let amount = |m: &ModDef| match m.effects[0] {
        ModEffect::BaseDamage(v) => v,
        ref other => panic!("Serration consumer {other:?}"),
    };
    assert_eq!(amount(card), 1.649999976158142);
    let source_base: f32 = "0.15000001".parse().unwrap();
    assert_ne!(amount(card) as f32, source_base * 11.0_f32);
    assert_eq!(amount(&at_rank("serration@5").unwrap()), 0.8999999761581421);
}

#[test]
#[should_panic(expected = "authoritative rank inputs")]
fn native_mods_reject_a_second_rank_value_source() {
    let e: Value = serde_norway::from_str("kind: base_damage_bonus\nUpgradeType: 253\nOperationType: STACKING_MULTIPLY\nValue: 0.15000001\nFusionPowerStep: 0\nFusionPowerCurve: QC_LINEAR\nrankMax: 1.65\n").unwrap();
    effect_at_rank(&e, 10, 10);
}
