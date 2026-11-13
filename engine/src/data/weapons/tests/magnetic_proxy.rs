use super::*;

#[test]
fn magnetic_proxy_inputs_match_the_compiled_native_catalog() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/client/magnetic-proxy-inputs.json"
    )).unwrap();
    for row in fixture["weapons"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let base = WeaponBase::from_data(id, true, &[]);
        assert_eq!(base.weakness_bonus, row["value"].as_f64(), "{id}");
    }
    let enemies = crate::data::enemies::all();
    for row in fixture["enemies"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let enemy = enemies.iter().find(|enemy| enemy.id == id).unwrap();
        let expected = row["proxies"].as_array().unwrap();
        assert_eq!(enemy.hit_proxies.len(), expected.len(), "{id}");
        for (actual, expected) in enemy.hit_proxies.iter().zip(expected) {
            assert_eq!(actual.weakness, expected["weakness"].as_f64().unwrap(), "{id}");
            assert_eq!(actual.damage_multiplier, expected["damage_multiplier"].as_f64(), "{id}");
        }
    }
}

#[test]
fn magnetic_weakness_follows_each_form_and_the_shared_fight_constructor() {
    use crate::build::loadout::resolve;
    use crate::fight::FightParams;
    for id in ["laetum", "laetum_incarnon", "phantasma_prime", "phantasma_prime_charged"] {
        let base = WeaponBase::from_data(id, true, &[]);
        let expected = if id.starts_with("laetum") { 0.5 } else { 0.2 };
        let panel = resolve(&base, &[], crate::model::StackPolicy::AssumedMax);
        let params = FightParams::from_panel(&panel, &crate::arena::Arena::training(1.0),
            &crate::data::arcanes::ArcaneFx::none());
        assert_eq!(params.weakness_bonus, Some(expected), "{id}");
    }
}
