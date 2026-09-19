use super::{spec, spec_assembled};

/// A KITGUN'S ASSEMBLY REACHES THE PANEL — the whole point of
/// `spec_assembled`, asserted on the numbers a fight actually reads rather
/// than on the spec it was composed from.
///
/// The roster entry carries the DEFAULT assembly, so the test is that
/// naming another MOVES every number it should and moves them to the
/// parts' own values.
#[test]
fn an_assembly_composes_all_the_way_into_a_panel() {
    use crate::data::weapons::kitguns::Assembly;
    // NAMING NO ASSEMBLY IS THE DEFAULT ONE, never the chamber's preview —
    // so no path can produce a preview-based panel by forgetting to pass
    // parts. Asserted against the default composed by hand, because the
    // whole point is that the two agree without the caller knowing.
    let unnamed = crate::model::WeaponBase::from_data("tombfinger_secondary", false, &[]);
    let dflt = crate::data::weapons::kitguns::default_assembly("tombfinger_secondary").unwrap();
    assert_eq!(dflt.grip, "ulnaris", "the grip nearest the `base` preview");
    assert_eq!(dflt.loader, "bellows", "the first loader that changes nothing");
    let named = crate::model::WeaponBase::from_data_assembled(
        "tombfinger_secondary",
        false,
        &[],
        Some(&dflt),
    );
    assert_eq!(unnamed.base_vector, named.base_vector);
    assert_eq!(unnamed.base_fire_rate, named.base_fire_rate);
    assert_eq!(unnamed.magazine_size, named.magazine_size);
    // …and it is NOT the preview: the module's no-grip row totals 84 and
    // Ulnaris totals 100.01. The panel's own vector is the DIRECT hit, so
    // the shot is that plus the explosion — which is the carve holding.
    let whole = unnamed.base_vector.total()
        + unnamed.radial.as_ref().map_or(0.0, |r| r.base_vector.total());
    assert!((whole - 100.01).abs() < 1e-6, "{whole}");
    let haymaker = Assembly {
        chamber: "tombfinger".into(),
        grip: "haymaker".into(),
        loader: "thunderdrum".into(),
    };
    let built = crate::model::WeaponBase::from_data_assembled(
        "tombfinger_secondary",
        false,
        &[],
        Some(&haymaker),
    );

    // THE DIRECT HIT IS WHAT THE EXPLOSION LEAVES. Haymaker is 32 Impact +
    // 25 Puncture + 123 Radiation, and 19.5% of the Radiation stays here.
    let d = &built.base_vector;
    assert!((d.get(crate::rules::damage::DamageType::Impact) - 32.0).abs() < 1e-9, "{d:?}");
    assert!((d.get(crate::rules::damage::DamageType::Puncture) - 25.0).abs() < 1e-9, "{d:?}");
    assert!(
        (d.get(crate::rules::damage::DamageType::Radiation) - 123.0 * 0.195).abs() < 1e-9,
        "{d:?}"
    );
    // …AND THE OTHER 80.5% IS THE EXPLOSION.
    let r = built.radial.as_ref().expect("the secondary explodes");
    assert!(
        (r.base_vector.get(crate::rules::damage::DamageType::Radiation) - 123.0 * 0.805).abs() < 1e-9,
        "{:?}",
        r.base_vector
    );
    assert!((r.radius_m - 1.9).abs() < 1e-9);

    // EVERY OTHER AXIS THE ASSEMBLY OWNS MOVED, and moved to the part's own
    // number: the grip's fire rate, the loader's magazine class and reload,
    // and crit and status as the loader's additive deltas on the chamber.
    assert!((built.base_fire_rate - 2.17).abs() < 1e-9, "{}", built.base_fire_rate);
    assert_ne!(built.base_fire_rate, unnamed.base_fire_rate);
    // Thunderdrum is -4% crit chance, -0.1 crit damage, +7% status, the
    // `highest` magazine class (29 rounds on this chamber) and a 2.1 s
    // reload. TWO OF THE THREE DELTAS ARE NEGATIVE, which is the whole
    // reason they are additive and not a multiplier.
    assert!((built.base_crit_chance - 0.20).abs() < 1e-9, "{}", built.base_crit_chance);
    assert!((built.base_crit_damage - 1.9).abs() < 1e-9, "{}", built.base_crit_damage);
    assert!((built.base_status_chance - 0.31).abs() < 1e-9, "{}", built.base_status_chance);
    assert_eq!(built.magazine_size, 29.0);
    assert!((built.base_reload - 2.1).abs() < 1e-9, "{}", built.base_reload);

    // AND A GRIP FROM THE OTHER SLOT DOES NOT COMPOSE. It is a real weapon
    // and it is the wrong one, which is the mismatch that reads as working.
    let tremor = Assembly {
        chamber: "tombfinger".into(),
        grip: "tremor".into(),
        loader: "thunderdrum".into(),
    };
    assert!(
        spec_assembled(spec("tombfinger_secondary").unwrap(), Some(&tremor)).is_none(),
        "a primary grip composed into the secondary entry"
    );
    // …and the same grip on the PRIMARY entry does.
    assert!(
        spec_assembled(spec("tombfinger_primary").unwrap(), Some(&tremor)).is_some()
    );
}

/// A KITGUN'S ROSTER ENTRY AND ITS PARTS FILE MUST AGREE ABOUT WHAT THE
/// WEAPON IS, and every entry that names a chamber must name one that
/// exists. Both are the kind of mismatch that composes into a plausible
/// weapon rather than into an error.
#[test]
fn every_modular_entry_matches_its_chamber() {
    for s in super::all() {
        let Some(k) = s.kitgun.as_deref() else { continue };
        let c = crate::data::weapons::kitguns::chambers()
            .iter()
            .find(|c| c.id == k)
            .unwrap_or_else(|| panic!("{}: no chamber record {k}", s.id));
        assert_eq!(c.slot, s.slot, "{}: slot", s.id);
        assert_eq!(
            c.blast.is_some(),
            s.attack.radial.is_some(),
            "{}: the chamber explodes {} and the entry {}",
            s.id,
            c.blast.is_some(),
            s.attack.radial.is_some()
        );
        // THE ENTRY'S FORM MUST BE ONE THE CHAMBER PUBLISHES AN EXPLOSION
        // FOR. A form the parts file has never heard of composes to nothing
        // at all, and the panel that would have said so panics.
        if let Some(b) = &c.blast {
            assert!(
                b.forms.contains_key(&s.form),
                "{}: form `{}` has no explosion; the chamber states {:?}",
                s.id,
                s.form,
                b.forms.keys().collect::<Vec<_>>()
            );
        }
        // THE FACTS BOTH FILES STATE, which `spec_assembled` composes none
        // of — so a disagreement here is a weapon that fights with the
        // entry's number while the parts file says another.
        let trigger = match c.trigger.as_str() {
            "Semi-Auto" => "semi_auto",
            "Auto" => "auto",
            "Held" => "held",
            "Charge" => "charge",
            t => panic!("{}: chamber trigger {t:?}", c.id),
        };
        assert_eq!(s.attack.trigger, trigger, "{}: trigger", s.id);
        assert_eq!(s.disposition, Some(c.riven_disposition), "{}: disposition", s.id);
        assert_eq!(s.ammo_max, Some(c.ammo_max), "{}: ammo_max", s.id);
        assert_eq!(s.accuracy, Some(c.accuracy), "{}: accuracy", s.id);
        // A BEAM CHAMBER IS A BEAM ENTRY: the reach it prices per grip has
        // nowhere to land on an entry without a `beam:` block.
        assert_eq!(
            c.tags.iter().any(|t| t == "BEAM"),
            s.attack.beam.is_some(),
            "{}: beam block",
            s.id
        );
        // THE FALLOFF: both state DE's own `Reduction`, the share removed.
        match (&c.falloff, &s.attack.falloff) {
            (None, None) => {}
            (Some(k), Some(e)) => {
                assert_eq!((k.start_m, k.end_m), (e.start_m, e.end_m), "{}: falloff window", s.id);
                assert!((k.reduction - e.reduction).abs() < 1e-9,
                    "{}: the chamber removes {} and the entry {}", s.id, k.reduction, e.reduction);
            }
            (k, e) => panic!("{}: chamber falloff {k:?}, entry falloff {e:?}", s.id),
        }
        if let Some(p) = c.punch_through_m {
            assert_eq!(s.attack.punch_through_m, p, "{}: punch through", s.id);
        }
        for p in &c.forced_procs {
            assert!(s.attack.forced_procs.contains(p), "{}: the module forces {p}", s.id);
        }
    }
}

/// AN ENTRY'S OWN NUMBERS ARE ITS DEFAULT ASSEMBLY'S (notes:
/// `kitgun_entry_is_default_assembly`). A fight never reads them, because
/// `spec_assembled` overwrites every one — but the prerendered weapon page
/// does, so an entry that drifts publishes a stat line nobody can build.
#[test]
fn every_modular_entry_states_its_default_assembly() {
    let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
    let same = |a: &std::collections::BTreeMap<String, f64>,
                b: &std::collections::BTreeMap<String, f64>| {
        a.len() == b.len() && a.iter().all(|(k, v)| b.get(k).is_some_and(|w| near(*v, *w)))
    };
    for s in super::all().iter().filter(|s| s.kitgun.is_some()) {
        let d = spec_assembled(s, None).unwrap_or_else(|| panic!("{}: no default", s.id));
        let (a, b) = (&s.attack, &d.attack);
        assert!(same(&a.damage, &b.damage), "{}: damage {:?}, default {:?}", s.id, a.damage, b.damage);
        if let (Some(x), Some(y)) = (&a.radial, &b.radial) {
            assert!(same(&x.damage, &y.damage), "{}: radial {:?}, default {:?}", s.id, x.damage, y.damage);
        }
        if let (Some(x), Some(y)) = (&a.beam, &b.beam) {
            assert!(near(x.range_m, y.range_m), "{}: reach {}, default {}", s.id, x.range_m, y.range_m);
        }
        assert!(near(a.fire_rate, b.fire_rate), "{}: fire rate {}, default {}", s.id, a.fire_rate, b.fire_rate);
        assert_eq!(a.charge_seconds, b.charge_seconds, "{}: charge", s.id);
        for (what, x, y) in [
            ("crit chance", a.crit_chance, b.crit_chance),
            ("crit multiplier", a.crit_multiplier, b.crit_multiplier),
            ("status chance", a.status_chance, b.status_chance),
            ("multishot", a.multishot, b.multishot),
        ] {
            assert!(near(x, y), "{}: {what} {x}, default {y}", s.id);
        }
        assert_eq!(s.magazine, d.magazine, "{}: magazine", s.id);
        assert_eq!(s.reload_seconds, d.reload_seconds, "{}: reload", s.id);
    }
}

/// EVERY GRIP OF EVERY MODULAR ENTRY BUILDS AND FIRES. `every_entry_builds_
/// and_fires` reaches only the DEFAULT assembly; a grip whose row is the odd
/// one — Sporelacer's Gibber publishes no Impact, so its direct hit is empty —
/// is exactly the panel nobody built until a player picked it.
#[test]
fn every_grip_of_every_modular_entry_builds_and_fires() {
    use crate::data::weapons::kitguns::Assembly;
    let arena = crate::arena::Arena::training(3.0);
    let mut ran = 0;
    for s in super::all().iter().filter(|s| s.kitgun.is_some()) {
        let c = crate::data::weapons::kitguns::chamber(s.kitgun.as_deref().unwrap()).unwrap();
        for g in crate::data::weapons::kitguns::grips().iter().filter(|g| g.slot == c.slot) {
            let a = Assembly { chamber: c.chamber.clone(), grip: g.id.clone(), loader: "bellows".into() };
            let base = crate::model::WeaponBase::from_data_assembled(&s.id, false, &[], Some(&a));
            let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
            let p = crate::fight::FightParams::from_panel(
                &panel, &arena, &crate::data::arcanes::ArcaneFx::none());
            let r = crate::fight::monte_carlo(&p, 2, 3);
            assert!(r.mean_damage > 0.0, "{} on {}: fires nothing", s.id, g.id);
            ran += 1;
        }
    }
    assert_eq!(ran, 60, "twelve entries, five grips each");
}

/// A BEAM'S GRIP REACHES THE PANEL as the beam's range — Gaze, secondary,
/// on Haymaker is 22 m and on Gibber 41 m, whatever the entry's preview says.
#[test]
fn a_beam_kitguns_reach_is_its_grips() {
    use crate::data::weapons::kitguns::Assembly;
    let reach = |grip: &str| {
        let a = Assembly { chamber: "gaze".into(), grip: grip.into(), loader: "bellows".into() };
        let s = spec_assembled(spec("gaze_secondary").unwrap(), Some(&a)).expect("composes");
        s.attack.beam.as_ref().expect("a beam").range_m
    };
    assert_eq!(reach("haymaker"), 22.0);
    assert_eq!(reach("gibber"), 41.0);
    // A FLAT REACH LANDS ON THE ATTACK: Catchmoon's 42 m wall.
    let a = Assembly { chamber: "catchmoon".into(), grip: "brash".into(), loader: "bellows".into() };
    let s = spec_assembled(spec("catchmoon_primary").unwrap(), Some(&a)).expect("composes");
    assert_eq!(s.attack.range_m.as_ref().map(super::RangeSpec::metres), Some(42.0));
    // …AND THE PAGE'S INFINITE PUNCH THROUGH SURVIVES a module that states none.
    assert_eq!(s.attack.punch_through_m, crate::rules::space::INFINITE_BODY_PUNCH_THROUGH_M);
}
/// PAX CHARGE REMOVES THE RELOAD, and this is that end to end: the arcane
/// grants nothing but a reload-speed bonus and a flag, the CHAMBER states
/// the rate, and the sim's own battery — written for the Shedu — does the
/// rest. Asserted on the fight rather than on the panel, because a flag
/// that reaches a card and not the loop is exactly what this is for.
#[test]
fn pax_charge_turns_the_magazine_into_a_battery() {
    use crate::model::StackPolicy;
    let base = crate::model::WeaponBase::from_data("tombfinger_secondary", false, &[]);
    assert_eq!(base.recharge_per_second, Some(50.0), "the chamber states its rate");

    // ITS OWN SEAT, and NOT the weapon's. *"These can be installed
    // simultaneously with Secondary/Primary arcanes"* (wiki, `Kitgun`), so
    // a Kitgun holds one of each and the two never compete.
    let arc = crate::data::arcanes::for_slot("kitgun", "pax_charge")
        .expect("pax charge is offered in the Kitgun seat");
    for seat in ["primary", "secondary"] {
        assert!(
            crate::data::arcanes::for_slot(seat, "pax_charge").is_none(),
            "{seat}: a Kitgun arcane is competing with the ordinary pool"
        );
    }
    // …AND ON NOTHING ELSE: the equip rule is a TRAIT, since no class can
    // say "Kitgun" — a secondary Tombfinger is a `pistol` exactly like a Lex.
    for w in ["lex", "braton_prime"] {
        assert!(
            crate::data::arcanes::pool_for_weapon(w, "kitgun").is_empty(),
            "{w} is offered a Kitgun arcane"
        );
    }
    // All eight, on every modular entry, in the Kitgun seat and nowhere else.
    let modular: Vec<&str> =
        super::all().iter().filter(|s| s.kitgun.is_some()).map(|s| s.id.as_str()).collect();
    assert_eq!(modular.len(), 12, "six chambers, two slots each");
    for w in modular {
        let kit = crate::data::arcanes::pool_for_weapon(w, "kitgun");
        assert_eq!(kit.len(), 8, "{w}: the four Pax and four Residual arcanes");
        let own = crate::data::weapons::arcane_pools(w);
        assert_eq!(own[0], "kitgun", "{w}: the distinctive seat comes first");
        assert_eq!(own.len(), 2, "{w}: a Kitgun holds one of each");
        assert!(
            !crate::data::arcanes::pool_for_weapon(w, own[1])
                .iter()
                .any(|a| a.id.starts_with("pax_") || a.id.starts_with("residual_")),
            "{w}: a Kitgun arcane leaked into the ordinary seat"
        );
    }
    // A NON-MODULAR WEAPON IS UNCHANGED — one seat, its own slot's.
    assert_eq!(crate::data::weapons::arcane_pools("lex"), vec!["secondary"]);
    assert_eq!(crate::data::weapons::arcane_pools("braton_prime"), vec!["primary"]);

    let tenno = crate::data::tenno::default_tenno();
    let fx = arc.fx(arc.max_rank, StackPolicy::Emergent, &["modular"], tenno);
    // MAX RANK IS +50% RECHARGE DELAY REDUCTION, joining the reload bucket.
    assert!((fx.reload_bonus - 0.50).abs() < 1e-9, "{}", fx.reload_bonus);
    assert!(fx.rechargeable_magazine);

    // THE FIGHT. Same weapon, same everything, with and without the arcane.
    let arena = crate::arena::Arena::training(12.0);
    let panel = crate::build::loadout::resolve_for(&base, &[], StackPolicy::Emergent, tenno);
    let plain = crate::fight::FightParams::from_panel(
        &panel, &arena, &crate::data::arcanes::ArcaneFx::none());
    let charged = crate::fight::FightParams::from_panel(&panel, &arena, &fx);
    assert!(plain.battery.is_none(), "an ordinary Kitgun has no battery");
    let b = charged.battery.expect("pax charge installs one");
    assert_eq!(b.regen_per_second, 50.0);
    // THE DELAY IS THE RELOAD, shortened by the arcane's own bonus: the
    // default assembly's Bellows loader reloads in 2.1 s, and 2.1 / 1.5 is
    // 1.4 s — which is the worked example on the arcane's own page.
    assert!((b.delay_empty_seconds - 1.4).abs() < 1e-6, "{}", b.delay_empty_seconds);
    assert_eq!(b.delay_partial_seconds, b.delay_empty_seconds);
}
