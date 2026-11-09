use super::*;

#[test]
fn incarnon_transitions_match_the_client_profiles() {
    let source: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"), "/../tests/fixtures/incarnon/switching-2026-10-28.json"
    ))).unwrap();
    let profiles = source["weapons"].as_array().unwrap();
    let names: std::collections::BTreeMap<String, String> = crate::data::files_under("weapons/")
        .filter_map(|(_, text)| {
            let doc: serde_norway::Value = serde_norway::from_str(text).unwrap();
            Some((doc.get("id")?.as_str()?.to_owned(), doc.get("internal_name")?.as_str()?.to_owned()))
        }).collect();
    let arena = crate::arena::Arena::training(60.0);
    let arcane = crate::data::arcanes::ArcaneFx::none();
    let mut checked = 0;
    for form in all().iter().filter(|s| s.form == "incarnon" && s.gauge_form.is_some()) {
        let base_spec = spec(form.transforms_from.as_deref().unwrap()).unwrap();
        let internal_name = names.get(&base_spec.id).unwrap().as_str();
        let profile = profiles.iter().find(|p| p["internal_name"] == internal_name)
            .unwrap_or_else(|| panic!("{}: no package timing", form.id));
        let expected = |direction: &str| profile[direction]["mode_switch_time"]
            .as_str().unwrap().parse::<f32>().unwrap();
        let raw = WeaponBase::from_data(&form.id, true, &[]);
        let gauge = raw.gauge_form.unwrap();
        assert_eq!((gauge.transmute_in as f32).to_bits(), expected("entry").to_bits(), "{} entry", form.id);
        assert_eq!((gauge.transmute_out as f32).to_bits(), expected("exit").to_bits(), "{} exit", form.id);
        assert_eq!(raw.base_reload, gauge.transmute_in + gauge.transmute_out, "{} full cycle", form.id);
        let base = WeaponBase::from_data(&base_spec.id, true, &[]);
        let normal = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
        let mut transformed = crate::build::loadout::resolve(&raw, &[], crate::model::StackPolicy::Emergent);
        for speed_bonus in [0.0, 0.5] {
            transformed.reload_bonus = speed_bonus;
            let params = crate::fight::FightParams::incarnon_cycle_from_panels(
                &transformed, &normal, false, crate::fight::LockMode::Initial(0), &arena, &arcane,
            );
            let cycle = params.cycle.unwrap();
            assert_eq!(cycle.transmute_seconds, gauge.transmute_in / (1.0 + speed_bonus));
            assert_eq!(cycle.transmute_out_seconds, gauge.transmute_out / (1.0 + speed_bonus));
        }
        checked += 1;
    }
    assert!(checked > 0, "the package fixture must cover playable Incarnon forms");
}
