use super::*;
use crate::data::weapons::{assembly, zaws};

#[test]
fn zaw_parts_reach_every_form_of_the_two_independent_weapons() {
    let roster: Vec<_> = roster().filter(|w| w.zaw.is_some()).collect();
    assert_eq!(roster.len(), 22);
    let mut count = 0;
    for w in roster {
        let r = w.zaw.as_ref().unwrap();
        assert_eq!(crate::data::weapons::arcane_pools(&w.id), ["zaw", "melee"]);
        for grip in zaws::grips().iter().filter(|g| g.handedness == r.handedness) {
            for link in zaws::links() {
                let a: assembly::Assembly = zaws::Assembly { record: w.id.clone(), grip: grip.id.clone(), link: link.id.clone() }.into();
                let built = spec_assembled(w, Some(&a)).unwrap();
                let total = built.attack.damage.values().sum::<f64>();
                assert!((built.attack.slam.as_ref().unwrap().damage.values().sum::<f64>() - 2.0 * total).abs() < 1e-9);
                assert!(built.attack.damage.values().all(|v| v.is_finite() && *v > 0.0));
                assert!(built.attack.fire_rate > 0.0 && built.attack.crit_chance > 0.0 && built.attack.status_chance > 0.0);
                for form in all().iter().filter(|f| f.group() == w.group()) {
                    let s = spec_assembled(form, Some(&a)).unwrap();
                    assert_eq!(s.class, w.class);
                    assert_eq!(s.riven_family, w.riven_family);
                    assert_eq!(s.attack.fire_rate, built.attack.fire_rate);
                    if form.form == "heavy_slam" {
                        assert_eq!(s.attack.damage.values().sum::<f64>(), 0.0);
                        assert!(s.attack.radial.as_ref().unwrap().damage.values().sum::<f64>() > 0.0);
                        assert!((s.attack.radial.as_ref().unwrap().damage.values().sum::<f64>() - 3.0 * total).abs() < 1e-9);
                        assert!(s.attack.radial.as_ref().unwrap().crit_chance.is_none());
                    } else {
                        assert_eq!(s.attack.damage, built.attack.damage);
                    }
                }
                count += 1;
            }
        }
    }
    assert_eq!(count, 1760);
}

// Source-derived composition cases, not in-game golden measurements (docs/ZAW.md).
#[test]
fn zaw_composition_keeps_ordinary_shares_and_adds_equal_plague_deltas() {
    let balla = spec("balla_one_handed").unwrap();
    let a = assembly::of(&balla.id, "korb", "", "vargeet_ii_jai").unwrap();
    let s = spec_assembled(balla, Some(&a)).unwrap();
    assert!((s.attack.damage.values().sum::<f64>() - 248.0).abs() < 1e-9);
    assert!((s.attack.damage["puncture"] - 148.8).abs() < 1e-9);
    assert!((s.attack.crit_chance - 0.32).abs() < 1e-9);
    assert!((s.attack.status_chance - 0.10).abs() < 1e-9);
    let plague = spec("plague_kripath_two_handed").unwrap();
    let a = assembly::of(&plague.id, "shtung", "", "vargeet_ii_jai").unwrap();
    let s = spec_assembled(plague, Some(&a)).unwrap();
    assert!((s.attack.damage.values().sum::<f64>() - 255.96).abs() < 1e-9);
    assert!((s.attack.damage["viral"] - 74.74).abs() < 1e-9);
    assert!((s.attack.damage["impact"] - 40.74).abs() < 1e-9);
}

#[test]
fn zaw_board_parts_are_strict_and_stale_shares_repair_only_the_bad_part() {
    use crate::board::builds::{identity, validate_with};
    let w = spec("sepfahn_one_handed").unwrap();
    let key = |a: Option<&assembly::Assembly>| validate_with(&w.id, &[], &[], &[], "", None, None, a).map(|b| identity(&b));
    let d = assembly::default_for(w).unwrap();
    assert_eq!(key(None).unwrap(), key(Some(&d)).unwrap());
    let a = assembly::of(&w.id, "peye", "", "vargeet_ii_jai").unwrap();
    assert_ne!(key(Some(&a)).unwrap(), key(None).unwrap());
    for (grip, loader, link) in [("jayap", "", "jai"), ("peye", "", "bad"), ("peye", "bellows", "jai"), ("peye", "", "")] {
        let bad = assembly::of(&w.id, grip, loader, link).unwrap();
        assert!(key(Some(&bad)).is_err(), "accepted {grip}/{loader}/{link}");
    }
    let fixed = assembly::repaired(w, "jayap", "", "vargeet_ii_jai").unwrap();
    assert_eq!(fixed.grip(), d.grip());
    assert_eq!(fixed.component(), "vargeet_ii_jai");
    let foreign = assembly::of("catchmoon_primary", "", "", "jai").unwrap();
    assert!(spec_assembled(spec("catchmoon_primary").unwrap(), Some(&foreign)).is_none());
    assert_eq!(a.parts(), serde_json::json!({"grip":"peye","link":"vargeet_ii_jai"}));
}

#[test]
fn exodia_has_its_own_seat_and_feeds_the_existing_combo_mechanism() {
    use crate::data::arcanes;
    use crate::model::{StackPolicy, WeaponBase};
    let w = "sepfahn_one_handed";
    let pool = arcanes::pool_for_weapon(w, "zaw");
    assert_eq!(pool.len(), 8);
    assert!(arcanes::pool_for_weapon("nikana_prime", "zaw").is_empty());
    assert!(arcanes::pool_for_weapon(w, "melee").iter().all(|a| !a.id.starts_with("exodia_")));
    let panel = crate::build::loadout::resolve(&WeaponBase::from_data(w, true, &[]), &[], StackPolicy::Emergent);
    let tenno = crate::data::tenno::default_tenno();
    for rank in 0..=3 {
        let triumph = arcanes::secondary("exodia_triumph").unwrap().fx(rank, StackPolicy::Emergent, &["zaw"], tenno);
        let valor = arcanes::secondary("exodia_valor").unwrap().fx(rank, StackPolicy::Emergent, &["zaw"], tenno);
        let arena = crate::arena::Arena::training(3.0);
        let p = crate::fight::FightParams::from_panel(&panel, &arena, &triumph);
        assert_eq!(p.combo_count_chance, 0.125 * (rank + 1) as f64);
        assert_eq!(p.combo_count_chance_on_lifted, 0.0);
        let p = crate::fight::FightParams::from_panel(&panel, &arena, &valor);
        assert_eq!(p.combo_count_chance, 0.0);
        assert_eq!(p.combo_count_chance_on_lifted, 0.5 * (rank + 1) as f64);
    }
    for id in ["exodia_contagion", "exodia_force", "exodia_epidemic", "exodia_hunt", "exodia_brave", "exodia_might"] {
        let a = arcanes::secondary(id).unwrap();
        assert!(a.has_unmodeled() && a.unmodeled_effects().is_empty());
    }
}
