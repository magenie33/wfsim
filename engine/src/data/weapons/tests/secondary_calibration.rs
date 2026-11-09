use super::*;

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../../tests/fixtures/client/secondary-calibration-2026-10-28.json")).unwrap()
}

fn close(actual: &serde_json::Value, expected: &serde_json::Value, id: &str, key: &str) {
    if let Some(want) = expected.as_f64() {
        let got = actual.as_f64().unwrap_or_else(|| panic!("{id}: missing {key}"));
        assert!((got - want).abs() < 1e-10, "{id}: {key}: {got} != {want}");
    } else if let Some(object) = expected.as_object() {
        assert_eq!(actual.as_object().map(|v| v.len()), Some(object.len()), "{id}: {key}");
        for (part, value) in object {
            close(&actual[part], value, id, &format!("{key}.{part}"));
        }
    } else {
        assert_eq!(actual, expected, "{id}: {key}");
    }
}

#[test]
fn every_secondary_form_matches_its_adopted_client_fields() {
    let raw: std::collections::BTreeMap<String, serde_json::Value> = crate::data::files_under("weapons/secondary/")
        .filter(|(path, _)| path.ends_with(".yaml"))
        .map(|(_, text)| {
            let value: serde_json::Value = serde_norway::from_str(text).unwrap();
            (value["id"].as_str().unwrap().to_owned(), value)
        }).collect();
    let f = fixture();
    let rows = f["forms"].as_array().unwrap();
    assert_eq!(rows.len(), raw.len(), "every secondary form needs a reviewed binding");
    assert_eq!(rows.len(), 211);
    let mut checked = std::collections::BTreeSet::new();
    for row in rows {
        let id = row["id"].as_str().unwrap();
        assert!(checked.insert(id), "duplicate binding: {id}");
        let original = &raw[id];
        let s = spec(id).unwrap();
        let lookup = |key: &str| {
            let actual = key.split('.').fold(original, |v, part| &v[part]);
            if actual.is_null() && ["reload_seconds", "magazine", "ammo_max", "ammo_pickup"].contains(&key) {
                &raw[original["inherits"].as_str().unwrap()][key]
            } else { actual }
        };
        for (key, expected) in row["applied"].as_object().unwrap() {
            close(lookup(key), expected, id, key);
        }
        for (key, retained) in row["retained"].as_object().unwrap() {
            close(lookup(key), &retained["declared"], id, key);
        }
        if row["instant_release"].as_bool() == Some(true) {
            assert!(s.attack.charge_seconds.is_none(), "{id}: zero partial draw is ordinary fire");
        }
        for reason in row["limits"].as_array().unwrap() {
            assert!(s.unmodeled_parts.iter().any(|p| p.reason.as_deref() == reason.as_str()),
                "{id}: missing {reason}");
        }
        if row.get("secondary_override").is_some() {
            assert!(row.get("primary_override").is_none(), "{id}: slot-specific overrides");
        }
    }
}

#[test]
fn secondary_components_and_resource_boundaries_reach_the_model() {
    for (id, rate, reload) in [("riot_848", 8.0, 1.24), ("efv_8_mars_alt", 1.5, 2.1),
        ("tenet_detron", 3.3333333333333335, 1.0)] {
        let p = base_panel(id, false);
        assert!((p.base_fire_rate - rate).abs() < 1e-8, "{id}: parsed RPM");
        assert!((p.base_reload - reload).abs() < 1e-8, "{id}: parsed state reload");
    }
    let p = base_panel("scyotid_snare", false);
    assert_eq!(p.base_reload, 1.6, "magazine reload is not the toggle state's 0.01 s");
    assert_eq!(spec("scyotid_snare").unwrap().attack.ammo_cost, 4.0);
    let p = base_panel("tenet_plinx_charged", false);
    assert!(p.battery.is_none(), "full-clip composite does not borrow its parent's battery");
    assert_eq!(p.base_reload, 2.5);
    let orb = spec("grimoire_active").unwrap();
    assert_eq!(orb.magazine, Some(1.0));
    assert_eq!(orb.attack.damage["electricity"], 280.0);
    assert!(orb.attack.meter.is_some());
    let stug = spec("stug_incarnon").unwrap();
    assert_eq!(stug.attack.fire_rate, 2.5);
    assert_eq!(stug.attack.damage["corrosive"], 50.0, "attached damage, not zero launch damage");
    let coda = spec("coda_catabolyst").unwrap();
    let g = coda.attack.reload_grenade.as_ref().unwrap();
    assert_eq!((g.count, g.fan_deg), (3, 30.0));
    assert_eq!(base_panel("epitaph", false).charge_seconds, Some(0.36));
    assert_eq!(base_panel("efv_8_mars_alt", false).charge_seconds, Some(1.5));
}
