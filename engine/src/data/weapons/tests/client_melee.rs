use super::*;

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../../tests/fixtures/client/melee-calibration-2026-10-28.json")).unwrap()
}

fn close(actual: &serde_json::Value, expected: &serde_json::Value, id: &str, key: &str) {
    if let Some(want) = expected.as_f64() {
        let got = actual.as_f64().unwrap_or_else(|| panic!("{id}: missing {key}"));
        assert!((got - want).abs() < 1e-9, "{id}: {key}: {got} != {want}");
    } else if let Some(object) = expected.as_object() {
        assert_eq!(actual.as_object().map(|v| v.len()), Some(object.len()), "{id}: {key}");
        for (part, value) in object {
            close(&actual[part], value, id, &format!("{key}.{part}"));
        }
    } else if let Some(array) = expected.as_array() {
        assert_eq!(actual.as_array().map(|v| v.len()), Some(array.len()), "{id}: {key}");
        for (i, value) in array.iter().enumerate() {
            close(&actual[i], value, id, &format!("{key}[{i}]"));
        }
    } else {
        assert_eq!(actual, expected, "{id}: {key}");
    }
}

#[test]
fn every_melee_entry_matches_its_adopted_client_fields() {
    let raw: std::collections::BTreeMap<String, serde_json::Value> = crate::data::files_under("weapons/")
        .filter(|(path, _)| path.ends_with(".yaml"))
        .map(|(_, text)| {
            let value: serde_norway::Value = serde_norway::from_str(text).unwrap();
            let value = serde_json::to_value(value).unwrap();
            (value["id"].as_str().unwrap().to_owned(), value)
        }).collect();
    let f = fixture();
    let rows = f["forms"].as_array().unwrap();
    let specs: std::collections::BTreeSet<_> = all().iter()
        .filter(|s| s.weapon_type.as_str() == "melee").map(|s| s.id.as_str()).collect();
    assert_eq!(rows.len(), 1730);
    assert_eq!(specs.len(), rows.len(), "every melee entry needs a reviewed binding");
    let mut checked = std::collections::BTreeSet::new();
    for row in rows {
        let id = row["id"].as_str().unwrap();
        assert!(checked.insert(id) && specs.contains(id), "duplicate or non-melee binding: {id}");
        let s = spec(id).unwrap();
        if row["form"] == "slide" {
            assert_eq!(s.attack.follow_through, Some(1.0), "{id}: slide component keeps full damage");
        }
        let original = &raw[id];
        let lookup = |key: &str| {
            let actual = key.split('.').fold(original, |v, part| &v[part]);
            if actual.is_null() && ["combo_duration_seconds", "mastery_rank", "disposition"].contains(&key)
                && original["inherits"].is_string() {
                &raw[original["inherits"].as_str().unwrap()][key]
            } else { actual }
        };
        for (key, expected) in row["applied"].as_object().unwrap() {
            if expected == ".inf" && key == "combo_duration_seconds" {
                assert!(s.combo_duration_seconds.is_infinite() && s.combo_duration_seconds > 0.0, "{id}: infinite combo");
                continue;
            }
            close(lookup(key), expected, id, key);
            if key == "combo_duration_seconds" {
                assert!((s.combo_duration_seconds - expected.as_f64().unwrap()).abs() < 1e-9, "{id}: inherited clock reaches the model");
            }
        }
        for (key, value) in row["retained"].as_object().unwrap() {
            close(lookup(key), &value["declared"], id, key);
        }
        for reason in row["limits"].as_array().unwrap() {
            assert!(s.unmodeled_parts.iter().any(|part| part.reason.as_deref() == reason.as_str()), "{id}: missing {reason}");
        }
    }
}

#[test]
fn shared_actions_match_current_properties_and_their_pinned_clock_source() {
    let f = fixture();
    let (_, text) = crate::data::files_under("melee/").find(|(p, _)| *p == "melee/actions.yaml").unwrap();
    let catalog: serde_json::Value = serde_norway::from_str(text).unwrap();
    let actions = f["shared_actions"].as_object().unwrap();
    assert_eq!(f["actions"].as_array().unwrap().len(), 1041);
    assert_eq!(actions.len(), 689);
    assert_eq!(catalog["actions"].as_object().unwrap().len(), actions.len());
    for (id, expected) in actions {
        close(&catalog["actions"][id], expected, id, "action");
    }
    let trees = f["shared_trees"].as_object().unwrap();
    assert_eq!(trees.len(), 117);
    close(&catalog["trees"], &f["shared_trees"], "shared catalog", "trees");
    assert!(f["actions"].as_array().unwrap().iter().filter(|r| r.get("current_events").is_some()).count() >= 200);
    let candidates = f["clock_evidence"]["current_clip_candidates"].as_array().unwrap();
    assert_eq!(candidates.len(), 31);
    assert!(f["clock_evidence"]["physical_cycle"]["timing_seconds"]["natural_recovery"].is_null());
}

#[test]
fn calibrated_weapon_clocks_and_packets_reach_melee_inputs() {
    for id in ["garuda_talons_heavy", "garuda_prime_talons_heavy", "valkyr_talons_heavy"] {
        let base = crate::model::WeaponBase::from_data(id, true, &[]);
        let p = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
        assert_eq!(p.combo_script[0].windup_seconds, 0.75, "{id}: native charge before the first input");
    }
    let base = crate::model::WeaponBase::from_data("caustacyst_heavy", true, &[]);
    let p = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    assert_eq!(p.combo_script[0].windup_seconds, 0.5);
    assert_eq!(base_panel("sun_and_moon_slide", false).follow_through, Some(1.0), "slides keep full damage");
    assert_eq!(spec("glaive_prime_charged_throw").unwrap().attack.radial.as_ref().unwrap().falloff_reduction, Some(0.0),
        "recall is not the bounce explosion's falloff consumer");
    let p = base_panel("corufell_prime_heavy", false);
    assert!((p.base_fire_rate - 65.0 / 60.0).abs() < 1e-9);
    assert_eq!(p.base_status_chance, 0.2, "the actual heavy projectile's status");
    assert_eq!(spec("sun_and_moon").unwrap().attack.follow_through, Some(0.55));
    assert_eq!(base_panel("guandao_prime", false).combo_duration_seconds, 6.0);
    assert_eq!(base_panel("pulmonars", false).combo_duration_seconds, 9.0);
    assert_eq!(base_panel("vitrica", false).combo_duration_seconds, 10.0);
    assert!(base_panel("xoris", false).combo_duration_seconds.is_infinite());
}
