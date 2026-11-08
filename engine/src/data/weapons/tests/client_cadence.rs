use super::*;
use crate::model::RefireRule;

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../../tests/fixtures/client/cadence-claws-2026-10-28.json")).unwrap()
}

#[test]
fn native_refire_profiles_keep_the_source_class_and_per_form_rule() {
    let f = fixture();
    for row in f["refire_profiles"].as_array().unwrap() {
        let w = spec(row["weapon"].as_str().unwrap()).unwrap();
        let expected: RefireRule = serde_json::from_value(row["rule"].clone()).unwrap();
        assert_eq!(w.attack.refire, expected, "{}", w.id);
        assert!(row["source"]["state_type"].as_str().unwrap().contains("SemiAutomaticStateBehavior"));
        assert_eq!(base_panel(&w.id, false).refire, expected);
    }
    assert_eq!(spec("braton_prime").unwrap().attack.refire, RefireRule::Rate);
    for row in f["rate_inputs"].as_array().unwrap() {
        let w = spec(row["weapon"].as_str().unwrap()).unwrap();
        assert_eq!(w.attack.fire_rate as f32, row["rate"].as_f64().unwrap() as f32);
        assert_eq!(w.attack.refire.interval(w.attack.fire_rate, false), 0.75);
    }
}

#[test]
fn native_cadence_vectors_match_at_audited_float32_nodes() {
    for row in fixture()["cadence_vectors"]["vectors"].as_array().unwrap() {
        let case = row["case"].as_str().unwrap();
        if case == "semi_bypass" {
            continue;
        }
        let rpm = row["weapon_consumer_return_rpm"].as_f64().unwrap() as f32;
        let rate = rpm.max(1.0) * (1.0_f32 / 60.0);
        let value = match case {
            "rate" => rate,
            "base_interval" => RefireRule::Rate.interval(f64::from(rate), false) as f32,
            "semi_interval" => RefireRule::SemiAutomatic { skip_wait_on_empty: false }
                .interval(f64::from(rate), false) as f32,
            _ => panic!("unknown vector"),
        };
        let bytes = value.to_le_bytes().map(|x| format!("{x:02x}")).join("");
        assert_eq!(bytes, row["bits_le"].as_str().unwrap(), "{row}");
    }
    assert_eq!(RefireRule::SemiAutomatic { skip_wait_on_empty: true }.interval(20.0, true), 0.0);
    assert!(RefireRule::SemiAutomatic { skip_wait_on_empty: false }.interval(20.0, true) > 0.0);
}

#[test]
fn claw_inputs_reproduce_the_declared_client_vectors() {
    let f = fixture();
    assert_eq!(f["claws"].as_array().unwrap().len(), 17);
    for row in f["claws"].as_array().unwrap() {
        let id = row["weapon"].as_str().unwrap();
        let w = spec(id).unwrap();
        let raw: serde_norway::Value = serde_norway::from_str(crate::data::file(
            &format!("weapons/beast_claws/{id}.yaml")).unwrap()).unwrap();
        assert_eq!(raw["internal_name"].as_str(), row["source"].as_str());
        assert_eq!(w.attack.refire, serde_json::from_value(row["attack"]["refire"].clone()).unwrap());
        let panel = base_panel(id, false);
        assert_eq!(panel.base_crit_chance as f32, row["attack"]["crit_chance"].as_f64().unwrap() as f32, "{id}: crit chance");
        assert_eq!(panel.base_crit_damage as f32, row["attack"]["crit_multiplier"].as_f64().unwrap() as f32, "{id}: crit damage");
        assert_eq!(panel.base_status_chance as f32, row["attack"]["status_chance"].as_f64().unwrap() as f32, "{id}: status chance");
        for (name, value) in row["attack"]["damage"].as_object().unwrap() {
            let t = crate::rules::damage::DamageType::from_name(name).unwrap();
            assert_eq!(panel.base_vector.get(t) as f32, value.as_f64().unwrap() as f32, "{id}: {name}");
        }
        assert!(!crate::data::weapons::has_exilus_slot(id));
        assert!(crate::data::weapons::arcane_pools(id).is_empty());
        assert!(w.no_riven);
        assert!(crate::build::rivens::class_for_weapon(id).is_none());
        assert_eq!(row["icon"], "Icon=/Lotus/Interface/Icons/GeneticLab/BeastClaws.png");
    }
}

#[test]
fn assumed_claw_intervals_follow_the_reviewed_ordinary_action_proxies() {
    let f = fixture();
    let assumptions = &f["assumed_intervals"];
    assert!(assumptions["runtime_interval_seconds"]["kavat"].is_null());
    assert!(assumptions["runtime_interval_seconds"]["kubrow"].is_null());
    assert_eq!(assumptions["verified_outer_files"], 4);
    assert_eq!(assumptions["verified_archive_members"], 126);
    for row in assumptions["weapon_proxies"].as_array().unwrap() {
        let family = row["proxy_family"].as_str().unwrap();
        let proxy = &assumptions["family_proxies"][family];
        let clip = &proxy["reference"]["base_clip_rows"][0];
        let rate = clip["sequence_rate_float32"].as_f64().unwrap() as f32;
        let delay = proxy["post_action_delay_seconds"].as_f64().unwrap() as f32;
        let estimated = 1.0_f32 / rate + delay;
        let actual = spec(row["weapon"].as_str().unwrap()).unwrap().attack.refire.assumed_interval().unwrap();
        assert_eq!(actual as f32, estimated, "{row}");
        if family == "catbrow" {
            assert_eq!(delay.to_le_bytes(), [0xcd, 0xcc, 0xcc, 0x3d]);
            assert!(actual > 1.13 && actual < 1.14);
        } else {
            assert_eq!(actual, 1.0);
        }
    }
}

#[test]
fn claw_cards_apply_damage_crit_and_conversion_without_a_second_damage_formula() {
    use crate::build::loadout::resolve;
    use crate::model::StackPolicy;
    use crate::rules::damage::DamageType as D;
    let pool = crate::data::mods::pool_for_weapon("adarza_claws");
    let pick = |id: &str| pool.iter().find(|m| m.id == id).unwrap();
    let base = base_panel("adarza_claws", false);
    let p = resolve(&base, &[pick("maul"), pick("bite")], StackPolicy::BaseOnly);
    assert!((p.damage.total() - 90.0 * 4.3).abs() < 1e-8);
    assert!((p.crit_chance - 0.3 * 4.3).abs() < 1e-8);
    assert!((p.crit_damage - crate::rules::damage::quantize_base_crit_damage(2.5) * 3.2).abs() < 1e-8);
    let p = resolve(&base, &[pick("brute_conditioning")], StackPolicy::BaseOnly);
    assert!((p.damage.get(D::Impact) - 90.0 * 4.85).abs() < 1e-8);
    assert_eq!(p.damage.get(D::Slash), 0.0);
    assert_eq!(p.damage.get(D::Puncture), 0.0);
    let base = base_panel("panzer_claws", false);
    let p = resolve(&base, &[pick("frost_jaw"), pick("burning_claws")], StackPolicy::BaseOnly);
    assert_eq!(p.damage.get(D::Viral), 0.0, "both elemental masks convert after coalescing");
    assert!((p.damage.get(D::Heat) - (45.0 + 90.0 * 0.6 + 90.0 * 3.3)).abs() < 1e-8);
    assert_eq!(p.damage.get(D::Slash), 45.0);
    let p = resolve(&base, &[pick("cull_the_weak"), pick("immunity_resistance")], StackPolicy::BaseOnly);
    assert_eq!(p.noncrit_bonus, Some((1.0, 2.4)));
    assert!((p.co_per_type - 0.6).abs() < 1e-9);
    assert_eq!(p.status_damage_multiplier, 1.5);
}
