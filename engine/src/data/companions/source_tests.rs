use super::*;

fn build(host: &str) -> CompanionBuild { CompanionBuild { companion: host.into(), ..Default::default() } }
fn close(got: f64, expected: f64) { assert!((got - expected).abs() < 1e-9, "{got} != {expected}"); }

fn source() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../tests/fixtures/client/companions-2026-10-28.json")).unwrap()
}

#[test]
fn concrete_bodies_and_parts_match_client_inputs() {
    let source = source();
    for c in companions().iter().filter(|c| c.prototype_scope.is_none()) {
        let fields = &source["records"][c.internal_name.as_ref().unwrap()]["fields"];
        if let Some(stats) = c.stats.filter(|_| c.assembly.as_deref() != Some("moa")) {
            for (stat, key) in [("health", "MaxHealthOverride"), ("shield", "MaxShieldOverride"), ("armor", "ArmourRatingOverride")] {
                if let Some(raw) = fields[key].as_str() {
                    let x: f32 = raw.split_once('=').unwrap().1.parse().unwrap();
                    assert_eq!((stats.get(stat) as f32).to_bits(), x.to_bits(), "{} {stat}", c.id);
                }
            }
        }
        if fields["ArtifactSlots"].is_null() {
            assert_eq!(serde_json::json!(c.polarities), source["wiki"]["polarity_fallback"][&c.id]);
        }
    }
    for p in companion_parts() {
        let fields = &source["records"][&p.internal_name]["fields"];
        assert!(fields.is_object(), "{}: exact source path", p.id);
        if let Some(base) = p.base {
            for (stat, key) in BODY_STATS.into_iter().zip(["BaseHealth", "BaseShields", "BaseArmor"]) {
                let x: f32 = fields[key].as_str().unwrap().split_once('=').unwrap().1.parse().unwrap();
                assert_eq!((base.get(stat) as f32).to_bits(), x.to_bits(), "{} {stat}", p.id);
            }
        }
        assert_eq!(p.modify_base, fields["ModifyBase"].as_str() == Some("ModifyBase=1"));
        for (stats, prefix, suffixes) in [
            (p.bonus, "Max", ["HealthModifier", "ShieldModifier", "ArmourRatingModifier"]),
            (p.ungilded_bonus, "UngildedMax", ["HealthModifier", "ShieldModifier", "ArmourModifier"]),
        ] {
            for (stat, suffix) in BODY_STATS.into_iter().zip(suffixes) {
                let x = fields[format!("{prefix}{suffix}")].as_str().map_or(0.0, |raw| raw.split_once('=').unwrap().1.parse::<f32>().unwrap());
                assert_eq!((stats.get(stat) as f32).to_bits(), x.to_bits(), "{} {stat}", p.id);
            }
        }
    }
    for input in source["mod_inputs"].as_array().unwrap() {
        let m = companion_mod(input["id"].as_str().unwrap()).unwrap();
        assert_eq!(m.stat_bonuses.len(), 1);
        let rank: f32 = input["rank_value"].as_str().unwrap().parse().unwrap();
        assert_eq!((m.stat_bonuses[0].1 as f32).to_bits(), (rank * (m.max_rank + 1) as f32).to_bits());
    }
    assert_eq!(companion("adarza_kavat").unwrap().stats.unwrap().shield, 270.0, "client overrides wiki 230");
    assert_eq!(companion("wyrm").unwrap().stats.unwrap().armor, 80.0, "wiki Prime path alias must not replace base armor");
    assert_eq!(companion("wyrm_prime").unwrap().stats.unwrap().armor, 150.0);
    assert_eq!(resolve(&build("prototype_companion")).unwrap().polarities, ["penjaga"; 4]);
}

#[test]
fn gilding_grants_one_selected_ordinary_polarity_only_to_concrete_bodies() {
    for id in ["moa", "hound", "panzer_vulpaphyla", "vizier_predasite"] {
        let bare = resolve(&CompanionBuild { gilded: false, ..build(id) }).unwrap();
        let gilded = resolve(&CompanionBuild { gild_polarity: Some("naramon".into()), ..build(id) }).unwrap();
        assert_eq!(gilded.polarities.len(), bare.polarities.len() + 1);
        assert_eq!(gilded.polarities.last().unwrap(), "naramon");
        assert!(resolve(&CompanionBuild { gild_polarity: Some("umbra".into()), ..build(id) }).is_err());
    }
    assert!(resolve(&CompanionBuild { gild_polarity: Some("naramon".into()), ..build("prototype_moa") }).is_err());
}

#[test]
fn weapon_carrier_scopes_preserve_exclusivity_and_fixed_claws() {
    let source = source();
    let helios = weapon_scope("deconstructor_prime").unwrap();
    assert_eq!(helios.companions, ["helios", "helios_prime"]);
    assert_eq!(helios.prototype, "prototype_helios");
    assert!(!can_carry(helios, companion("shade").unwrap()));
    assert!(!can_carry(helios, companion("sentinel").unwrap()));
    assert!(!can_carry(helios, companion("moa").unwrap()));
    let ranged = weapon_scope("verglas").unwrap();
    assert_eq!(ranged.weapons.len(), 19);
    assert!(can_carry(ranged, companion("sentinel").unwrap()));
    assert!(can_carry(ranged, companion("prototype_moa").unwrap()));
    assert!(!can_carry(ranged, companion("hound").unwrap()));
    for id in ["akaten", "batoten", "lacerten"] {
        assert_eq!(weapon_scope(id).unwrap().companions, ["hound"]);
    }
    let mut count = 0;
    for w in crate::data::weapons::all().iter().filter(|w| w.class == "beast_claws") {
        let scope = weapon_scope(&w.id).unwrap();
        assert_eq!(scope.companions.len(), 1);
        let c = companion(&scope.companions[0]).unwrap();
        let binding = source["records"][c.internal_name.as_ref().unwrap()]["fields"]["SpecialSlotItem"].as_str().unwrap();
        let path = format!("weapons/beast_claws/{}.yaml", w.id);
        let raw: serde_norway::Value = serde_norway::from_str(crate::data::file(&path).unwrap()).unwrap();
        assert!(binding.contains(raw["internal_name"].as_str().unwrap()), "{}", w.id);
        assert!(w.wielder_modelled);
        count += 1;
    }
    assert_eq!(count, 17);
    assert!(!can_carry(weapon_scope("venari_prime_claws").unwrap(), companion("venari").unwrap()));
    assert!(!can_carry(weapon_scope("smeeta_claws").unwrap(), companion("adarza_kavat").unwrap()));
}

#[test]
fn every_registered_companion_weapon_has_one_scope() {
    for w in crate::data::weapons::all().iter().filter(|w| w.slot == "sentinel") {
        assert_eq!(carrier_scopes().iter().filter(|s| s.weapons.contains(&w.id)).count(), 1, "{}", w.id);
        let scope = weapon_scope(&w.id).unwrap();
        assert!(can_carry(scope, companion(&scope.prototype).unwrap()));
    }
}

#[test]
fn modular_bodies_enumerate_complete_legal_parts_and_reject_foreign_parts() {
    for (id, count) in [("moa", 640), ("hound", 81), ("panzer_vulpaphyla", 16), ("vizier_predasite", 16)] {
        let host = companion(id).unwrap();
        assert_eq!(assemblies(host).len(), count);
        for assembly in assemblies(host) {
            for gilded in [false, true] {
                let r = resolve(&CompanionBuild { assembly: assembly.clone(), gilded, ..build(id) }).unwrap();
                assert_eq!(r.assembly, assembly);
                assert!(r.stats.health > 0.0 && r.stats.armor >= 0.0);
            }
        }
    }
    let mut bad = build("moa");
    bad.assembly.insert("core".into(), "adlet_core".into());
    assert!(resolve(&bad).is_err());
    assert!(resolve(&CompanionBuild { assembly: default_parts(companion("moa").unwrap()), ..build("prototype_moa") }).is_err());
}

#[test]
fn moa_parts_modify_base_but_hound_brackets_add_with_mods() {
    let mod_pick = |id: &str| crate::data::warframes::SlotPick { id: id.into(), ..Default::default() };
    let mut moa = build("moa");
    moa.assembly = [("core".into(), "drex_core".into()), ("gyro".into(), "trux_gyro".into())].into();
    moa.mods.push(mod_pick("enhanced_vitality"));
    close(resolve(&moa).unwrap().stats.health, 350.0 * 1.15 * 3.5003);
    moa.gilded = false;
    close(resolve(&moa).unwrap().stats.health, 350.0 * 1.075 * 3.5003);
    let mut hound = build("hound");
    hound.assembly = [("core".into(), "adlet_core".into()), ("bracket".into(), "zubb_bracket".into())].into();
    hound.mods.push(mod_pick("enhanced_vitality"));
    close(resolve(&hound).unwrap().stats.health, 350.0 * (1.0 - 0.1 + 2.5003));
}

#[test]
fn prototypes_derive_attribute_minima_with_legal_witnesses() {
    for (id, expected) in [("prototype_moa", [367.5, 315.0, 350.0]), ("prototype_hound", [315.0, 315.0, 315.0]),
                           ("prototype_helios", [560.0, 250.0, 80.0]), ("sentinel", [450.0, 130.0, 80.0]),
                           ("prototype_companion", [367.5, 130.0, 80.0])] {
        let b = build(id);
        let r = resolve(&b).unwrap();
        let scope = carrier_scope(companion(id).unwrap().prototype_scope.as_deref().unwrap()).unwrap();
        for (stat, expected) in BODY_STATS.into_iter().zip(expected) {
            close(r.stats.get(stat), expected);
            let witness = r.floor_sources.iter().find(|s| s.stat == stat).unwrap();
            assert!(scope.companions.contains(&witness.companion));
            let actual = resolve(&CompanionBuild { assembly: witness.assembly.clone(), ..build(&witness.companion) }).unwrap();
            close(actual.stats.get(stat), expected);
        }
    }
}

#[test]
fn body_mod_compatibility_and_rank_follow_the_selected_scope() {
    let has = |host, id| seatable_by(host).any(|m| m.id == id);
    assert!(has("helios_prime", "investigator"));
    assert!(!has("shade", "investigator"));
    assert!(!has("sentinel", "investigator"));
    assert!(has("prototype_helios", "investigator"));
    assert!(has("prototype_moa", "hard_engage"));
    assert!(!has("moa", "fired_up"));
    assert!(!has("smeeta_kavat", "fired_up"));
    assert!(has("smeeta_kavat", "charm"));
    assert!(!has("adarza_kavat", "charm"));
    assert!(!has("smeeta_kavat", "bite"));
    let mut b = build("helios");
    b.mods.push(crate::data::warframes::SlotPick { id: "enhanced_vitality".into(), rank: Some(0), stacks: None });
    close(resolve(&b).unwrap().stats.health, 560.0 * (1.0 + 2.5003 / 11.0));
    let old: serde_json::Value = serde_json::to_value(build("sentinel")).unwrap();
    assert_eq!(old, serde_json::json!({"companion":"sentinel", "mods":[]}));
}

#[test]
fn precept_limits_and_incompatible_mod_families_are_refused() {
    let picks = |ids: &[&str]| ids.iter().map(|id| crate::data::warframes::SlotPick { id: (*id).into(), ..Default::default() }).collect();
    for (host, ids) in [
        ("hound", vec!["null_audit", "repo_audit"]),
        ("moa", vec!["hard_engage", "security_override", "stasis_field"]),
        ("helios", vec!["regen", "primed_regen"]),
        ("smeeta_kavat", vec!["enhanced_vitality", "link_vitality"]),
    ] {
        assert!(!resolve(&CompanionBuild { mods: picks(&ids), ..build(host) }).unwrap().refused.is_empty(), "{host}");
    }
    assert!(resolve(&CompanionBuild { mods: picks(&["null_audit", "evasive_denial", "focused_prospectus"]), ..build("hound") }).unwrap().refused.is_empty());
}
