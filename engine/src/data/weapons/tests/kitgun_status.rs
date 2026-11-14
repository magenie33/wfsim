use crate::build::loadout::resolve;
use crate::data::weapons::kitguns::{self, Assembly};
use crate::model::{StackPolicy, WeaponBase};

#[test]
fn golden_m132_vermisplicer_status_is_shared_by_five_tendrils() {
    let reading: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"), "/../tests/golden/M132-vermisplicer-secondary-status.json"
    ))).unwrap();
    let id = reading["weapon"].as_str().unwrap();
    let per_tendril = reading["per_tendril_status_chance"].as_f64().unwrap();
    let combined = reading["combined_status_chance"].as_f64().unwrap();
    let count = reading["tendrils"].as_f64().unwrap();
    for loader in ["flutterfire", "ramflare"] {
        for grip in kitguns::grips().iter().filter(|g| g.slot == "secondary") {
            let assembly = Assembly { chamber: "vermisplicer".into(),
                grip: grip.id.clone(), loader: loader.into() };
            let base = WeaponBase::from_data_assembled(id, false, &[], Some(&assembly.into()));
            let panel = resolve(&base, &[], StackPolicy::Emergent);
            assert_eq!(panel.multishot, count);
            assert!((panel.status_chance - per_tendril).abs() < 1e-12,
                "{loader}/{}: {} != {per_tendril}", grip.id, panel.status_chance);
            assert!((panel.status_chance * panel.multishot - combined).abs() < 1e-12);
        }
    }
}

#[test]
fn kitgun_status_mods_keep_the_native_denominator() {
    let assembly = Assembly { chamber: "vermisplicer".into(),
        grip: "haymaker".into(), loader: "ramflare".into() };
    let base = WeaponBase::from_data_assembled("vermisplicer_secondary", false,
        &[], Some(&assembly.into()));
    let pool = crate::data::mods::pool_for_build("vermisplicer_secondary", &[]);
    let cards: Vec<_> = ["sure_shot", "barrel_diffusion"].iter()
        .map(|id| pool.iter().find(|m| m.id == *id).unwrap()).collect();
    let panel = resolve(&base, &cards, StackPolicy::Emergent);
    assert!((panel.multishot - 11.0).abs() < 1e-12);
    assert!((panel.status_chance - 0.078 * 1.9).abs() < 1e-12);
}

#[test]
fn every_beam_assembly_matches_its_native_loader_status_budget() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/client/kitgun-status-2026-10-28.json"
    )).unwrap();
    let barrels = fixture["barrels"].as_array().unwrap();
    let loaders = fixture["loaders"].as_array().unwrap();
    assert_eq!(barrels.len(), kitguns::chambers().len());
    assert_eq!(loaders.len(), kitguns::loaders().len());
    let mut checked = 0;
    for barrel in barrels {
        let id = barrel["id"].as_str().unwrap();
        let chamber = kitguns::chamber(id).unwrap();
        let count: f64 = barrel["fire_iterations"].as_str().unwrap().parse().unwrap();
        assert_eq!(chamber.multishot, count, "{id}: native emission count");
        if barrel["gun_type"] != "GT_BEAM" { continue; }
        for loader in loaders {
            let loader_id = loader["id"].as_str().unwrap();
            let native: f64 = loader["beam"]["ProcChance"].as_str().unwrap().parse().unwrap();
            for grip in kitguns::grips().iter().filter(|g| g.slot == chamber.slot) {
                let assembly = Assembly { chamber: chamber.chamber.clone(),
                    grip: grip.id.clone(), loader: loader_id.into() };
                let base = WeaponBase::from_data_assembled(id, false, &[], Some(&assembly.into()));
                assert!((base.base_status_chance * count - native).abs() < 3e-8,
                    "{id}/{loader_id}/{}: {} times {count} != {native}", grip.id, base.base_status_chance);
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 400, "four beam chambers, twenty loaders, five grips");
}
