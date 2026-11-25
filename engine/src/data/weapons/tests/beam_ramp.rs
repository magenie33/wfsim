use super::*;

#[test]
fn every_held_form_binds_its_client_attenuation_declaration() {
    let rows: serde_json::Value = serde_json::from_str(include_str!("../../../../../tests/fixtures/client/beam-ramp-roster-2026-10-28.json")).unwrap();
    let mut checked = std::collections::BTreeSet::new();
    for row in rows.as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        assert!(checked.insert(id));
        assert!(row["error"].is_null());
        let p = base_panel(id, false);
        assert!(p.continuous, "{id}");
        let f = &row["fields"];
        let lower: f64 = f["ContinuousDamageAttenuation"].as_str().unwrap().trim_matches(['{', '}']).split(',').next().unwrap().parse().unwrap();
        let num = |key: &str| f[key].as_str().unwrap().parse::<f64>().unwrap();
        let close = |a: f64, b: f64| assert!((a-b).abs() < 1e-6, "{id}: {a} != {b}");
        close(p.beam_ramp_floor, lower);
        let rise = num("ContinuousDamageAttenuationTime");
        if rise == 0. {
            assert_eq!(id, "onos_incarnon");
            assert!(p.beam_ramp.is_none(), "Onos uses its distinct charge consumer");
            continue;
        }
        let r = p.beam_ramp.unwrap();
        close(r.rise_seconds, rise);
        close(r.cooldown_delay_seconds, num("ContinuousDamageAttenuationCooldownDelay"));
        close(r.cooldown_divisor, num("ContinuousDamageAttenuationCooldownTime"));
        assert_eq!(r.per_target, f["AttenuateContinuousDamagePerTarget"] == "1", "{id}");
    }
    let held: std::collections::BTreeSet<_> = all().iter().filter(|w| w.attack.trigger == "held").map(|w| w.id.as_str()).collect();
    assert_eq!(held, checked);
}

#[test]
fn beam_bursts_bind_attenuation_without_changing_burst_cadence() {
    let receipt: serde_json::Value = serde_json::from_str(include_str!("../../../../../tests/fixtures/client/beam-ramp-bursts-2026-10-28.json")).unwrap();
    for row in receipt["rows"].as_array().unwrap() {
        let id = row["id"].as_str().unwrap();
        let p = base_panel(id, false);
        assert_eq!(spec(id).unwrap().attack.trigger, "burst");
        assert!(!p.continuous);
        let f = &row["fields"];
        let r = p.beam_ramp.unwrap();
        let close = |actual: f64, key: &str| assert!((actual - f[key].as_str().unwrap().parse::<f64>().unwrap()).abs() < 1e-6, "{id}: {key}");
        assert_eq!(p.beam_ramp_floor, 0.2);
        assert_eq!(f["ContinuousDamageAttenuation"], "{0.2,1}");
        close(r.rise_seconds, "ContinuousDamageAttenuationTime");
        close(r.cooldown_delay_seconds, "ContinuousDamageAttenuationCooldownDelay");
        close(r.cooldown_divisor, "ContinuousDamageAttenuationCooldownTime");
        assert!(r.per_target);
        assert_eq!(f["AttenuateContinuousDamagePerTarget"], "1");
    }
}
