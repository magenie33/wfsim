/// THE CO CATALOG'S DAMAGE COLUMN IS A FREE CROSS-CHECK OF THE SHOT.
///
/// "Attack Unmodded Damage" is the whole SHOT while a weapon yaml carries
/// the per-projectile damage and the pellet count separately, so
/// `base_vector.total() x base_multishot` has to reproduce it. A lost
/// pellet count shows up here and nowhere else.
///
/// That is exactly how the Bronco was found: both Incarnon entries carried
/// `multishot: 1.0` where the base forms had 7.
///
/// Rows transcribed from `Condition_Overload_(Mechanic)?action=raw` — only
/// the roster's entries, and only the DIRECT attack of each.
#[test]
fn every_catalog_row_reproduces_our_shot_damage() {
    // (entry, the catalog's Attack Unmodded Damage)
    let rows: &[(&str, f64)] = &[
        ("angstrum_incarnon", 30.0),
        ("atomos_incarnon", 100.0),
        ("ballistica", 100.0),
        // TWICE THE ROW: a full charge takes a x2 nothing publishes (M66).
        ("ballistica_prime", 608.0),
        ("ballistica_prime_incarnon", 830.0),
        ("bronco_prime_incarnon", 238.0),   // 34 x 7 — the row that caught the bug
        ("cernos_prime", 552.0),
        ("cestra", 26.0),
        ("cestra_incarnon", 50.0),
        ("despair_incarnon", 60.0),
        ("dread", 336.0),
        ("dread_incarnon", 400.0),
        ("dual_toxocyst_incarnon", 75.0),
        ("felarx", 760.0),
        ("felarx_incarnon", 600.0),
        ("furis_incarnon", 100.0),
        ("kunai", 46.0),
        // PER PROJECTILE, like its MK1 below — the catalog says 40 and this
        // form fires two. See the note there.
        ("kunai_incarnon", 80.0),
        ("laetum", 160.0),
        ("laetum_incarnon", 100.0),
        ("lato_vandal_incarnon", 152.0),
        ("latron_incarnon", 50.0),
        ("lex_prime_incarnon", 1200.0),
        ("miter", 500.0),
        ("miter_incarnon", 60.0),
        // THE KUNAI FAMILY'S ROWS ARE PER PROJECTILE, and the catalog's own
        // Lato Vandal row proves the others are not: 152 is that form's 76
        // TIMES its 2 multishot where these two carry the damage alone.
        // Three sources give the multishot as 2 — the module's attack row,
        // the Genesis page ("2 base Multishot"), and this catalog calling
        // itself community-sourced. Doubled here: the data is right.
        ("mk1_kunai_incarnon", 48.0),
        ("mk1_paris", 230.0),
        ("paris", 320.0),
        ("paris_incarnon", 460.0),          // the catalog's 520 is the PRIME's
        ("paris_prime", 360.0),
        ("paris_prime_incarnon", 520.0),
        ("rakta_ballistica", 300.0),
        ("shedu", 71.0),
        ("stug", 4.0),
        ("torid", 100.0),
        ("vasto_prime_incarnon", 420.0),
        ("zylok_incarnon", 400.0),
        ("zylok_prime_incarnon", 500.0),
    ];
    for (id, want) in rows {
        let b = crate::model::WeaponBase::from_data(id, false, &[]);
        let got = b.base_vector.total() * b.base_multishot.max(1.0);
        assert!(
            (got - want).abs() < 0.51,
            "{id}: the catalog's shot is {want}, ours is {:.1} x {:.0} pellets = {got:.1}",
            b.base_vector.total(), b.base_multishot
        );
    }
}

/// …AND THE RADIALS the catalog names, a separate column entry whose
/// listed number INCLUDES the flat-damage evolution — so the check is
/// against the unevolved base the third column is computed from.
#[test]
fn every_catalog_radial_row_reproduces_our_explosion() {
    let rows: &[(&str, f64)] = &[
        ("braton_prime_incarnon", 70.0),    // catalog 74 = 70 + Daring Reverie's +4
        ("burston_prime_incarnon", 13.0),   // catalog 55 = 13 + 42
        ("zylok_prime_incarnon", 700.0),    // catalog 776 mixes in the base Zylok's +76
        ("akarius_prime", 509.0),
    ];
    for (id, want) in rows {
        let b = crate::model::WeaponBase::from_data(id, false, &[]);
        let r = b.radial.as_ref().unwrap_or_else(|| panic!("{id} has no radial"));
        assert!((r.base_vector.total() - want).abs() < 0.51,
            "{id}: the catalog's explosion is {want}, ours is {}", r.base_vector.total());
    }
}
/// "CO-BONUS DOES NOT USE BASE DAMAGE INCREASE EVOLUTION" — all eleven rows,
/// checked by the catalog's OWN ARITHMETIC.
///
/// Each row prints two damage figures and a percentage: "100 or 124 (with
/// Evolution II)" against "100% or 81%". The second percentage is
/// unmodded/evolved, which is exactly what `co_base_fraction` becomes when
/// the named perk is applied — so the row checks itself, and the check
/// fails if the flag is MISSING, on the WRONG PERK, or on a perk whose flat
/// damage does not match.
///
/// That third failure is not hypothetical: this list is the group that has
/// gone wrong twice.
///
/// A row naming "Evolution II Perk 1" or "Perk 2" means ONLY that perk is
/// discrepant — its tier-mate feeds the CO term in full even when it adds
/// the same damage (the Vasto Prime's two are both +24).
#[test]
fn the_eleven_evolution_exclusion_rows_reproduce_their_own_percentages() {
    // (entry, perk, catalog unmodded, catalog with-evolution)
    let rows: &[(&str, &str, f64, f64)] = &[
        ("atomos_incarnon", "atomos_hoplite_virtue", 100.0, 124.0),
        ("atomos_incarnon", "atomos_paladin_virtue", 100.0, 124.0),
        ("bronco_prime_incarnon", "bronco_prime_speeding_bullet", 238.0, 448.0),
        ("cestra", "cestra_fortress_salvo", 26.0, 36.0),
        ("cestra", "cestra_steadfast_grit", 26.0, 36.0),
        ("cestra_incarnon", "cestra_fortress_salvo", 50.0, 60.0),
        ("cestra_incarnon", "cestra_steadfast_grit", 50.0, 60.0),
        ("despair_incarnon", "despair_stalkers_vendetta", 60.0, 120.0),
        ("dual_toxocyst_incarnon", "dual_toxocyst_carnage_reign", 75.0, 135.0),
        ("furis_incarnon", "furis_haven_foray", 100.0, 128.0),
        ("furis_incarnon", "furis_stormburst", 100.0, 128.0),
        // THE ONE ROW THE CATALOG CONTRADICTS ITSELF ON, so it carries our
        // number and the row's, and a note rather than a silent choice.
        //
        // The Lato Vandal's Incarnon form is 2 pellets of 76 (wiki infobox:
        // "Total Damage 152 ... Multishot 2 (76.00 damage per projectile)")
        // and Haven Foray adds +22. The row prints "152 or 174", i.e. the
        // +22 landing ONCE on the shot — but every other multi-pellet row in
        // the same table is PER PELLET: the Bronco Prime's 238 -> 448 is
        // 7 x 30 and the Vasto Prime's 420 -> 564 is 6 x 24, both exact.
        //
        // A flat base-damage evolution raises the BASE DAMAGE stat, which a
        // multishot weapon lists per projectile — so per pellet is what the
        // engine does, it agrees with the catalog on both of the other
        // multi-pellet rows, and two cards (the Vasto's Lone Gun, the Soma's
        // Fresh Havoc) say "applied per pellet in Incarnon Form" outright.
        // NEEDS AN IN-GAME MEASUREMENT to settle; until then the row is the
        // outlier, not the engine.
        ("lato_vandal_incarnon", "lato_vandal_haven_foray", 76.0, 98.0),
        ("lex_prime_incarnon", "lex_prime_hoplite_virtue", 1200.0, 1220.0),
        ("lex_prime_incarnon", "lex_prime_trusty_sidearm", 1200.0, 1220.0),
        ("vasto_prime_incarnon", "vasto_prime_deathtrap_trigger", 420.0, 564.0),
        ("zylok_prime_incarnon", "zylok_prime_maulers_magazine", 500.0, 530.0),
        ("zylok_prime_incarnon", "zylok_prime_precisions_payoff", 500.0, 530.0),
    ];
    for (entry, perk, unmodded, evolved) in rows {
        let b = crate::model::WeaponBase::from_data(entry, false, &[perk]);
        let want = unmodded / evolved;
        assert!((b.co_base_fraction() - want).abs() < 1e-6,
            "{entry} + {perk}: the catalog says CO computes on {unmodded} of {evolved}                  ({:.1}%), our co_base_fraction is {:.4}", want * 100.0, b.co_base_fraction());
    }

    // …AND THE TIER-MATES THE CATALOG DOES NOT NAME feed the term in full.
    // Absence from the table is a positive statement, so a perk that raises
    // base damage by the same number as its named sibling is still ordinary.
    //
    // THIS HALF IS THE ONE UNDER PRESSURE: the wiki's Math section
    // lists "Base Damage increases from Incarnon Genesis Evolutions" among
    // the things Adding CO ignores, with no "some" on it, and read as a law
    // it would flip all 107 of these. The page argues that side itself —
    // its "Bow charging" bullet is enumerated by ~15 catalog rows that
    // disagree with each other. See docs/CATALOGS.md.
    // AN ADDING PERK THE CATALOG DOES NOT LIST STILL EXCLUDES ITS OWN
    // BASE, 15 measurements to 0 (M49, M50). The catalog's eleven
    // double-valued rows are the only ones that ever measured an evolved
    // weapon and all eleven exclude; its "100%" rows print an UNEVOLVED
    // base in their own damage column, true by construction.
    //
    // THE FLIP IS ADDING-ONLY: nothing has measured a Multiplying entry's
    // evolved CO base, so the Torid's base form is the open experiment and
    // its own test pins it at 1.0. These five are kept pointing the other
    // way, because they are the set that distinguishes the two defaults.
    for (entry, perk) in [
        ("vasto_prime_incarnon", "vasto_prime_lone_gun"),
        ("bronco_prime_incarnon", "bronco_prime_infused_shots"),
        ("despair_incarnon", "despair_fatal_affliction"),
        ("lato_vandal_incarnon", "lato_vandal_reified_bane"),
        ("vasto_incarnon", "vasto_deathtrap_trigger"),
    ] {
        let b = crate::model::WeaponBase::from_data(entry, false, &[perk]);
        let bare = crate::model::WeaponBase::from_data(entry, false, &[]);
        let f = bare.base_vector.total() / b.base_vector.total();
        assert!(f < 0.999, "{entry} + {perk} raises no base damage");
        assert_eq!(b.co_behavior, crate::model::CoBehavior::AdditiveWithBaseDamage);
        assert!((b.co_base_fraction() - f).abs() < 1e-9,
            "{entry} + {perk}: an Adding entry computes CO on the UNEVOLVED base \
                 by default — expected {f:.4}, got {:.4}", b.co_base_fraction());
    }
}

/// THE OTHER HALF, ROSTER-WIDE: NO EVOLUTION DILUTES A `Multiplying` ENTRY.
///
/// The loop above is `Adding`, where the term reads the UNEVOLVED base. The
/// Torid's base form measured the opposite answer on the other class (M51):
/// the same two tier-2 perks, +51 and +31, and the CO multiplier came back
/// 1.40 and 1.80 under BOTH — so a `Multiplying` term reads the FULL
/// evolved base and the two classes disagree.
///
/// GENERALISED TO ALL 26 ENTRIES ON ONE WEAPON'S READING, ahead of the
/// catalog: a measurement contradicting it edits ONE weapon's yaml rather
/// than this rule. It asserts the PROPERTY rather than the 26 numbers, so
/// it holds for a weapon nobody has entered yet — the fraction with the
/// whole evolution ladder installed equals the fraction with none of it,
/// and a perk declaring `co_base_excludes_this_evolution` without scoping
/// it to its measured form fails HERE.
///
/// The flat 1.0 is asserted SEPARATELY, as a snapshot of today's roster:
/// the day one weapon declares otherwise on evidence, that half moves while
/// the invariant above does not.
#[test]
fn no_evolution_dilutes_a_multiplying_co_base() {
    let mut checked = 0;
    for spec in crate::data::weapons::all() {
        let bare = crate::model::WeaponBase::from_data(&spec.id, false, &[]);
        if bare.co_behavior != crate::model::CoBehavior::Independent {
            continue;
        }
        // The GROUP owns the evolutions, not the form.
        let group = spec.transform_group.as_deref().unwrap_or(&spec.id);
        let ids: Vec<&str> = (1..=crate::data::evolutions::tier_count(group))
            .flat_map(|t| crate::data::evolutions::options(group, t))
            .map(|o| o.id.as_str())
            .collect();
        if ids.is_empty() {
            continue;
        }
        checked += 1;
        let loaded = crate::model::WeaponBase::from_data(&spec.id, false, &ids);
        assert!(
            loaded.base_vector.total() >= bare.base_vector.total(),
            "{}: the ladder lowered the panel", spec.id
        );
        assert!(
            (loaded.co_base_fraction() - bare.co_base_fraction()).abs() < 1e-9,
            "{}: an evolution diluted a Multiplying CO base ({:.4} -> {:.4}); \
                 a Multiplying term reads the FULL evolved base (M51)",
            spec.id, bare.co_base_fraction(), loaded.co_base_fraction()
        );
        // TODAY'S ROSTER — the reserved slot, unexercised everywhere.
        assert!(
            (loaded.co_base_fraction() - 1.0).abs() < 1e-9,
            "{}: no Multiplying entry declares a CO base fraction yet, got {:.4}",
            spec.id, loaded.co_base_fraction()
        );
    }
    assert!(checked >= 8, "only {checked} Multiplying entries with a ladder checked");
}
