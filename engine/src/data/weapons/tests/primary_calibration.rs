use super::*;

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../../tests/fixtures/client/primary-calibration-2026-10-28.json")).unwrap()
}

#[test]
fn every_primary_form_matches_its_adopted_client_fields() {
    let raw: std::collections::BTreeMap<String, serde_json::Value> = crate::data::files_under("weapons/primary/")
        .filter(|(path, _)| path.ends_with(".yaml"))
        .map(|(_, text)| {
            let value: serde_json::Value = serde_norway::from_str(text).unwrap();
            (value["id"].as_str().unwrap().to_owned(), value)
        }).collect();
    let f = fixture();
    let rows = f["forms"].as_array().unwrap();
    assert_eq!(rows.len(), raw.len(), "every primary form needs a reviewed binding");
    assert_eq!(rows.len(), 318);
    let mut checked = std::collections::BTreeSet::new();
    for row in rows {
        let id = row["id"].as_str().unwrap();
        assert!(checked.insert(id), "duplicate binding: {id}");
        let original = &raw[id];
        let s = spec(id).unwrap();
        for (key, expected) in row["applied"].as_object().unwrap() {
            if key == "attack.sustained_fire_rate" {
                if let Some(crate::model::SustainedFireRate::Native(native)) = s.attack.sustained_fire_rate {
                    let (start, end, span) = crate::model::SustainedFireRate::Native(native).display_span();
                    assert_eq!((start as f32).to_bits(), (expected["start"].as_f64().unwrap() as f32).to_bits());
                    assert_eq!(end, expected["end"].as_f64().unwrap());
                    assert_eq!(span, expected["over_shots"].as_f64().unwrap());
                    continue;
                }
            }
            let mut actual = key.split('.').fold(original, |v, part| &v[part]);
            if actual.is_null() && ["reload_seconds", "magazine", "ammo_max", "ammo_pickup"].contains(&key.as_str()) {
                actual = &raw[original["inherits"].as_str().unwrap()][key];
            }
            assert_json_close(actual, expected, id, key);
        }
        for reason in row["limits"].as_array().unwrap() {
            assert!(s.unmodeled_parts.iter().any(|part| part.reason.as_deref() == reason.as_str()),
                "{id}: missing {}", reason);
        }
    }
}

fn assert_json_close(actual: &serde_json::Value, expected: &serde_json::Value, id: &str, key: &str) {
    if let Some(want) = expected.as_f64() {
        let got = actual.as_f64().unwrap_or_else(|| panic!("{id}: {key} missing"));
        assert!((got - want).abs() < 1e-10, "{id}: {key}: {got} != {want}");
    } else if let Some(object) = expected.as_object() {
        assert_eq!(actual.as_object().map(|v| v.len()), Some(object.len()), "{id}: {key}");
        for (part, value) in object {
            assert_json_close(&actual[part], value, id, &format!("{key}.{part}"));
        }
    } else {
        assert_eq!(actual, expected, "{id}: {key}");
    }
}

#[test]
fn charge_only_refire_and_looped_reload_reach_the_combat_model() {
    use crate::model::ChargeCadence;
    for (id, draw) in [("lanka", 1.0), ("lanka_uncharged", 0.333333)] {
        let p = base_panel(id, false);
        assert_eq!(p.charge_cadence, ChargeCadence::DrawOnly);
        assert!((p.charge_seconds.unwrap() - draw).abs() < 1e-6, "{id}");
    }
    for (id, parts) in [("strun_prime", (0.4, 0.4, 0.2)), ("corinth", (0.8, 0.22, 0.4)),
        ("kuva_zarr", (0.5, 0.75, 0.55))] {
        let p = base_panel(id, false);
        assert_eq!(p.by_round_reload, Some(parts), "{id}: source loop reaches reload scheduling");
    }
    let p = base_panel("flux_rifle", false);
    assert_eq!(p.battery.unwrap().delay_empty_seconds, 0.5);
    assert_eq!(p.base_reload, 1.75);
}
