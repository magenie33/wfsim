use super::*;

#[test]
fn native_weakness_inputs_reach_every_bound_form_and_shared_fight() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/client/weapon-weakness-2026-10-28.json"
    )).unwrap();
    for form in fixture["forms"].as_array().unwrap() {
        let id = form["id"].as_str().unwrap();
        let row = &fixture["owners"][form["owner"].as_str().unwrap()];
        let value = row["value"].as_f64().unwrap();
        let hex = row["weakness_bonus_f32_le_hex"].as_str().unwrap();
        let bytes: [u8; 4] = std::array::from_fn(|i|
            u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap());
        let base = WeaponBase::from_data(id, true, &[]);
        assert_eq!(base.weakness_bonus, Some(value), "{id}");
        assert_eq!((base.weakness_bonus.unwrap() as f32).to_le_bytes(), bytes, "{id}");
        let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
        let params = crate::fight::FightParams::from_panel(&panel,
            &crate::arena::Arena::training(1.0), &crate::data::arcanes::ArcaneFx::none());
        assert_eq!(panel.weakness_bonus, Some(value), "{id}");
        assert_eq!(params.weakness_bonus, Some(value), "{id}");
    }
}

#[test]
fn unbound_component_owners_do_not_receive_a_guessed_weapon_default() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/client/weapon-weakness-2026-10-28.json"
    )).unwrap();
    for row in fixture["unbound_forms"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        assert_eq!(WeaponBase::from_data(id, true, &[]).weakness_bonus, None, "{id}");
    }
    assert_eq!(WeaponBase::from_data("ocucor", true, &[]).weakness_bonus, Some(0.2));
    assert_eq!(WeaponBase::from_data("kuva_nukor", true, &[]).weakness_bonus, Some(0.0));
    assert_eq!(WeaponBase::from_data("skana", true, &[]).weakness_bonus, Some(0.0));
}
