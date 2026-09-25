use super::*;

fn spec(class: &str, ids: &[&str], malus: Option<&str>) -> RivenSpec {
    RivenSpec {
        class: class.into(),
        bonuses: ids.iter().map(|id| RolledStat { id: (*id).into(), roll: 1.0 }).collect(),
        malus: malus.map(|id| RolledStat { id: id.into(), roll: 1.0 }),
        rank: MAX_RANK,
        polarity: Polarity::Madurai,
    }
}

fn stat(class: &str, id: &str) -> &'static RivenStat {
    pool(class).iter().find(|x| x.id == id).unwrap_or_else(|| panic!("{class}/{id}"))
}

/// THE WIKI'S "Spliced Values" TABLE, which is DE's base x 90 like the rolled
/// stats' column. The pistol's two weak-point rows are the odd ones out
/// (90% against 225% and 247.5%) and are here for that reason.
#[test]
fn the_wikis_spliced_column_agrees() {
    for (class, id, wiki) in [
        ("rifle", "weak_point_damage", 225.0),
        ("rifle", "weak_point_critical_chance", 247.5),
        ("pistol", "weak_point_damage", 90.0),
        ("pistol", "weak_point_critical_chance", 90.0),
        ("rifle", "ammo_efficiency", 9.0),
        ("shotgun", "reload_while_holstered", 90.0),
        ("archgun", "viral", 90.0),
        ("rifle", "status_damage", 90.0),
        ("melee", "melee_damage_on_heavy_attack", 119.7),
        ("melee", "heavy_attack_wind_up_speed", 119.7),
        ("melee", "slam_attack_damage", 119.7),
    ] {
        let got = stat(class, id).base * 90.0 * 100.0;
        assert!((got - wiki).abs() < 0.01, "{class}/{id}: {got} vs wiki {wiki}");
    }
    // Two that are not percentages: metres of reach-like angle, and x0.45.
    assert!((stat("melee", "parry_angle").base * 90.0 - 8.1).abs() < 1e-6);
    assert!((stat("rifle", "damage_to_scaldra").base * 90.0 - 0.45).abs() < 1e-6);
}

/// EIGHTEEN, split by class the way the export ships them: no Reload While
/// Holstered on an Arch-Gun, and melee's four are its own.
#[test]
fn each_class_carries_its_spliced_stats() {
    let n = |c: &str| pool(c).iter().filter(|x| x.spliced).count();
    assert_eq!(n("rifle"), 14);
    assert_eq!(n("pistol"), 14);
    assert_eq!(n("shotgun"), 14);
    assert_eq!(n("archgun"), 13);
    assert_eq!(n("melee"), 4);
    for c in crate::data::mods::classes() {
        for s in pool(c).iter().filter(|x| x.spliced) {
            assert!(!s.malus, "{c}/{}: a spliced stat is always a bonus", s.id);
        }
    }
}

/// A SPLICER MAKES ONE, and a malus is never one.
#[test]
fn a_card_carries_one_spliced_stat_and_only_as_a_bonus() {
    assert!(spec("rifle", &["viral", "critical_chance", "multishot"], Some("zoom")).illegal().is_empty());
    let two = spec("rifle", &["viral", "weak_point_damage", "multishot"], None).illegal();
    assert!(two.iter().any(|e| e.contains("one spliced stat")), "{two:?}");
    let malus = spec("rifle", &["damage", "multishot"], Some("viral")).illegal();
    assert!(malus.iter().any(|e| e.contains("never be the malus")), "{malus:?}");
}

/// EACH LANDS IN THE BUCKET ITS MOD LANDS IN — and a combined element joins
/// its type's total instead of pairing, so it is not one of the card's
/// ordered `elements()`.
#[test]
fn a_spliced_stat_reaches_the_bucket_its_mod_does() {
    let m = spec("rifle", &["viral", "ammo_efficiency", "damage_to_techrot"], None).to_mod_def("riven:s", 1.0);
    assert!(m.effects.iter().any(|e| matches!(e, ModEffect::CombinedElement(DamageType::Viral, v) if *v > 0.0)));
    assert!(m.effects.iter().any(|e| matches!(e, ModEffect::AmmoEfficiency(v) if *v > 0.0)));
    assert!(m.effects.iter().any(|e| matches!(e, ModEffect::FactionDamage(Faction::Techrot, _))));
    let shape = RivenShape { bonuses: vec!["viral".into(), "heat".into()], malus: None };
    assert_eq!(shape.elements("rifle"), vec![DamageType::Heat]);

    let heavy = spec("melee", &["melee_damage_on_heavy_attack", "melee_damage"], None).to_mod_def("riven:h", 1.0);
    assert!(heavy.effects.iter().any(|e| matches!(e, ModEffect::HeavyAttackDamage(v) if *v > 0.0)));
    // Nothing is holstered or parried here, so these two carry no effect.
    for (c, id) in [("rifle", "reload_while_holstered"), ("melee", "parry_angle")] {
        assert_eq!(stat(c, id).kind, "unmodelled", "{c}/{id}");
    }
}

/// THE AMMO EFFICIENCY REACHES THE PANEL, which is what the fight adds to the
/// arcane's bucket (`fight::setup`).
#[test]
fn spliced_ammo_efficiency_reaches_the_panel() {
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::StackPolicy;
    let base = WeaponBase::from_data("braton_prime", true, &[]);
    let riven = spec("rifle", &["ammo_efficiency", "damage"], None).to_mod_def("riven:a", 1.0);
    let bare = resolve(&base, &[], StackPolicy::AssumedMax).ammo_efficiency;
    let with = resolve(&base, &[&riven], StackPolicy::AssumedMax).ammo_efficiency;
    assert_eq!(bare, 0.0);
    // 0.001 x 90 x 0.99 (a plain 2-stat card) at disposition 1.0.
    assert!((with - 0.0891).abs() < 1e-6, "{with}");
}
