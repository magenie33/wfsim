
/// EVERY ENTRY'S RANGE PAGE HAS BEEN OPENED, and this is what says so.
///
/// An entry either STATES a range or is RECORDED in
/// `data/surveys/weapon_range.yaml` as having been read — which a count of
/// silent entries cannot do, since it cannot tell "the page states no
/// range" from "nobody has looked". ABSENCE STILL MEANS UNLIMITED at
/// runtime: 101 pages really do state no reach.
/// EVERY `internal_name` IS ONE THE EXPORT ACTUALLY HOLDS.
///
/// The only join between this data and its cross-check source —
/// `internal_name` == DE's `uniqueName`, never the display name, since
/// the export carries duplicates sharing one. A key resolving to
/// NOTHING still produces "cross-checked — 0 disagreements" out of a
/// comparison that never ran, which is how the Hema was skipped.
///
/// `scripts/survey_internal_names.py` REFUSES to write a key that joins to
/// nothing, so the test is the other half: every entry that states a key
/// must be IN the survey. An entry with no key at all is a FORM, which
/// inherits its weapon's.
#[test]
fn every_internal_name_resolves_in_the_export() {
    let raw = crate::data::file("surveys/internal_names.yaml")
        .expect("data/surveys/internal_names.yaml — run scripts/survey_internal_names.py");
    let doc: serde_norway::Value = serde_norway::from_str(raw).expect("the survey parses");
    let resolved = doc
        .get("resolved")
        .and_then(|m| m.as_mapping())
        .expect("the survey has a `resolved:` mapping");
    assert!(resolved.len() >= 200, "the survey looks empty: {} rows", resolved.len());

    // THE RAW YAML, not the spec: `internal_name` is metadata a FORM
    // inherits through the raw merge and no `WeaponSpec` field holds it, so
    // reading the spec would find nothing to check.
    let mut stated = 0usize;
    let mut unsurveyed: Vec<String> = Vec::new();
    let mut disagreed: Vec<String> = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    for (path, text) in crate::data::files_under("weapons/") {
        let doc: serde_norway::Value =
            serde_norway::from_str(text).unwrap_or_else(|e| panic!("{path}: {e}"));
        let Some(key) = doc.get("internal_name").and_then(|v| v.as_str()) else {
            continue;
        };
        let id = doc
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("{path} has no id"))
            .to_string();
        stated += 1;
        match resolved
            .get(serde_norway::Value::String(id.clone()))
            .and_then(|v| v.as_str())
        {
            None => unsurveyed.push(format!("{id} -> {key}")),
            Some(surveyed) if surveyed != key => {
                disagreed.push(format!("{id}: yaml {key} / survey {surveyed}"))
            }
            Some(_) => {}
        }
        seen.insert(id);
    }
    assert!(
        unsurveyed.is_empty(),
        "{} entry/entries state an `internal_name` the survey does not hold — either \
             the key is a typo (the survey refuses to record one that joins to nothing) or \
             the survey is stale: run scripts/survey_internal_names.py\n  {}",
        unsurveyed.len(),
        unsurveyed.join("\n  ")
    );
    assert!(
        disagreed.is_empty(),
        "{} entry/entries disagree with the survey — the key moved and the survey did \
             not, or the other way round:\n  {}",
        disagreed.len(),
        disagreed.join("\n  ")
    );
    // …and the survey may not keep rows for entries that no longer state a
    // key, which is the direction that would otherwise let a deleted row
    // vouch for a weapon nobody has looked at.
    let orphans: Vec<&str> = resolved
        .iter()
        .filter_map(|(k, _)| k.as_str())
        .filter(|k| !seen.contains(*k))
        .collect();
    assert!(orphans.is_empty(), "the survey names entries the roster lost: {orphans:?}");
    assert_eq!(stated, resolved.len(), "survey row count vs entries that state a key");
}

/// A WEAPON THAT NAMES ITS WIELDERS NAMES FRAMES THE WARFRAME MODULE HAS,
/// renames itself only for one of them, and hands both to every form.
#[test]
fn every_named_wielder_is_a_modelled_warframe() {
    for s in all() {
        for f in &s.wielders {
            assert!(crate::data::warframes::warframe(f).is_some(), "{}: wielder {f} is not in data/warframes/", s.id);
        }
        for f in s.wielder_names.keys() {
            assert!(s.wielders.contains(f), "{}: a name for {f}, who cannot hold it", s.id);
        }
    }
    let t = spec("valkyr_talons_slide").expect("a Talons form");
    assert_eq!(t.wielders, ["valkyr_prime", "valkyr"], "a form inherits its weapon's wielders");
    assert_eq!(t.wielder_names.get("valkyr_prime").map(String::as_str), Some("Valkyr Prime Talons"));
}

/// THE STANCE SLOT'S POLARITY IS READ, and it decides a capacity GRANT
/// rather than a discount — see `rules::capacity::stance_capacity`.
///
/// The Magistar's slot is Vazarin, so Shattering Storm doubles its grant
/// there and Crushing Ruin, which is Madurai, does not. A field nobody
/// reads is the anti-pattern this repo names by name.
#[test]
fn the_stance_slot_polarity_decides_what_a_stance_grants() {
    // EVERY FORM CARRIES IT, because the slot belongs to the WEAPON and not
    // to a way of swinging it — the same inheritance the combo counter's
    // own clock needs, and the same silence when it is missing.
    for form in [
        "magistar", "magistar_forward", "magistar_block", "magistar_block_forward",
        "magistar_heavy", "magistar_slide", "magistar_heavy_slam",
    ] {
        assert_eq!(
            stance_polarity(form),
            Some(crate::rules::capacity::Polarity::Vazarin),
            "{form} lost the stance slot's colour",
        );
    }
    let slot = stance_polarity("magistar");
    let of = |id: &str| {
        crate::data::mods::pool_for_weapon("magistar")
            .iter()
            .find(|m| m.id == id)
            .map(|m| crate::rules::capacity::stance_capacity(m.polarity, slot))
            .expect("in the hammer pool")
    };
    assert_eq!(of("shattering_storm"), 10, "Vazarin on a Vazarin slot");
    assert_eq!(of("crushing_ruin"), 4, "Madurai on a Vazarin slot is 80%, rounded down");
}

#[test]
fn every_entry_has_had_its_range_page_opened() {
    let raw = crate::data::file("surveys/weapon_range.yaml")
        .expect("data/surveys/weapon_range.yaml");
    let doc: serde_norway::Value = serde_norway::from_str(raw).expect("the worksheet parses");
    let checked = doc
        .get("checked")
        .and_then(serde_norway::Value::as_mapping)
        .expect("a `checked:` mapping");
    let silent: Vec<&str> = super::all()
        .iter()
        .filter(|s| {
            let stated = s.attack.range_m.is_some()
                || s.attack.beam.as_ref().is_some_and(|b| b.range_m.is_finite());
            let read = checked.contains_key(serde_norway::Value::String(s.id.clone()));
            !stated && !read
        })
        .map(|s| s.id.as_str())
        .collect();
    assert!(
        silent.is_empty(),
        "{} entries have neither a range nor a line in              data/surveys/weapon_range.yaml. Open the wiki page, write the number              into the weapon, or record that the page states none:
  {}",
        silent.len(),
        silent.join("
  "),
    );
}

/// M66 — TWO NUMBERS NOBODY PUBLISHES, SOLVED OUT OF FOUR READINGS. Each
/// row is `(B + 0.4·mods·types·C) × 33/32`: two solve the pair, the other
/// two check it, and the yaml has to return both.
#[test]
fn the_ballistica_primes_charge_multiplier_solves_out_of_its_gunco_rows() {
    // (mods x types, the damage popped) — MEASUREMENTS M66.
    let rows = [(1.2_f64, 610.0_f64), (2.4, 709.0), (1.6, 643.0), (0.8, 577.0)];
    let raw = |hit: f64| hit * 32.0 / 33.0;
    // A difference cancels B, leaving C alone.
    let co_base = (raw(rows[1].1) - raw(rows[2].1)) / (rows[1].0 - rows[2].0);
    let modded = raw(rows[2].1) - rows[2].0 * co_base;
    assert!((co_base - 80.0).abs() < 0.5, "solved a CO base of {co_base}, against 80");
    assert!((modded - 496.0).abs() < 1.0, "solved a modded base of {modded}, against 496");
    // …and the other two reproduce, which a wrong pair does not.
    for (k, hit) in rows {
        let got = (modded + k * co_base) * 33.0 / 32.0;
        assert!((got - hit).abs() < 1.0, "{k}x GunCO: measured {hit}, solved {got:.1}");
    }

    // Neither is published: 152 + 3, and the uncharged 40 doubled.
    let b = crate::model::WeaponBase::from_data(
        "ballistica_prime", false, &["ballistica_prime_headcracker"],
    );
    assert!((b.base_vector.total() - modded / 3.2).abs() < 0.5,
        "the charged base is {}, solved {}", b.base_vector.total(), modded / 3.2);
    assert!((b.co_base - co_base).abs() < 0.5,
        "the CO base is {}, solved {co_base}", b.co_base);
    // The ramp's OTHER end is not doubled — the x2 is the charge's.
    let n = crate::model::WeaponBase::from_data(
        "ballistica_prime_uncharged", false, &["ballistica_prime_headcracker"],
    );
    assert!((n.base_vector.total() - 43.0).abs() < 1e-9);
    assert!((n.co_base - 40.0).abs() < 1e-9, "the Adding class reads the unevolved base");
}

/// EVERY CO ANOMALY IN THE ROSTER IS ON THIS LIST, and the list is the
/// catalog plus what the catalog's rows carry to their families
/// (docs/CATALOGS.md §1). Nothing else may be anything but ordinary.
///
/// Ordinary has a definition — direct hits only, 100% of the base, added to
/// the base-damage bucket.
///
/// A LIST rather than a count, because the failure this exists to stop is
/// not "someone added an anomaly", it is "someone gave one to the variant
/// next door" — so adding a weapon fails here until its row, or its
/// family's, has been read for it.
#[test]
fn the_only_condition_overload_anomalies_are_the_ones_the_catalog_names() {
    // (entry, behaviour, co_base_fraction) — see docs/CATALOGS.md for the
    // verbatim row behind each.
    const NAMED: &[(&str, &str, f64)] = &[
        ("angstrum_incarnon", "independent", 1.0),
        ("prisma_angstrum_incarnon", "independent", 1.0),
        // Rocket Impact; the base Akarius has no row and carries this one.
        ("akarius_prime", "independent", 1.0),
        ("ballistica", "additive_with_base_damage", 0.25),
        // 0.5263 = 40/76, MEASURED (M66); the catalog's 50% is it rounded.
        ("ballistica_prime", "additive_with_base_damage", 0.526_316),
        ("ballistica_prime_incarnon", "independent", 1.0),
        ("rakta_ballistica", "additive_with_base_damage", 0.25),
        ("cernos_prime", "additive_with_base_damage", 0.5),
        ("dread", "additive_with_base_damage", 0.5),
        ("dread_incarnon", "independent", 1.0),
        ("felarx", "independent", 1.0),
        ("felarx_incarnon", "independent", 1.0),
        ("kunai_incarnon", "independent", 1.0),
        ("mk1_kunai_incarnon", "independent", 1.0),
        ("larkspur_prime_charged", "independent", 1.0),
        ("latron_incarnon", "independent", 1.0),
        ("latron_prime_incarnon", "independent", 1.0),
        ("miter", "additive_with_base_damage", 0.40),
        ("miter_incarnon", "independent", 1.0),
        ("mk1_paris", "additive_with_base_damage", 0.5),
        ("paris", "additive_with_base_damage", 0.5),
        ("paris_incarnon", "independent", 1.0),
        ("paris_prime", "additive_with_base_damage", 0.5),
        ("paris_prime_incarnon", "independent", 1.0),
        // THE KUVA BATCH. Four of the sixteen have a row of their own; the
        // rest are ordinary unless a sibling's row reaches them.
        //   Kuva Seer | Projectile Impact | Projectile | 131 | 131 | 100% | Multiplying
        ("kuva_seer", "independent", 1.0),
        //   Kuva Drakgoon | Charged Attack | Projectile | 460 | 230 | 50% | Adding
        //   "CO-bonus only applies to base (uncharged) damage; uses bows
        //    mechanics; bows have innate 2x damage multiplier when fully
        //    charged" — which is why the fraction is exactly a half and the
        //    catalog's own two damage columns are 460 and 230. The TAPPED
        //    shot has no row and stays ordinary.
        ("kuva_drakgoon", "additive_with_base_damage", 0.5),
        // THE TENET AND CODA BATCH. Twenty weapons, and the
        // two catalog tables name SEVEN of their attacks between them —
        // five that are anomalies and reach this list, and two that the
        // catalog checked and called ordinary (both Tenet Detron rows,
        // `Adding` at 100%), which is a row worth transcribing into the
        // weapon and worth nothing here.
        //   Coda Bassocyst | Normal Attack | Projectile | 808 | 808 | 100% | Multiplying
        // …and its ALT-FIRE has the opposite row — `303 | 0 | 0% | N/A,
        // "Does not apply"` — which would be `inert` and has no entry,
        // because that alt fire is a Mercy-finisher tool rather than a way
        // to fight and is recorded as a gap instead.
        // THE NINETEEN BASE WEAPONS behind the adversary families. Four of
        // them have rows: on the Arca Plasmor and the Hema both variants are
        // named, on the Bubonico only the BASE is and on the Ferrox only the
        // TENET — and those two rows now reach their siblings.
        //   Arca Plasmor | Normal Attack | Projectile | 600 | 600 | 100% | Multiplying
        ("arca_plasmor", "independent", 1.0),
        //   Hema | Normal Attack | Projectile | 47 | 47 | 100% | Multiplying
        ("hema", "independent", 1.0),
        //   Bubonico | Main-fire | Projectile | 287 | 287 | 100% | Multiplying
        //   Bubonico | Alt-fire  | Projectile |   9 |   9 | 100% | Multiplying
        // …and the CODA Bubonico is named on neither, so it carries both.
        ("bubonico", "independent", 1.0),
        ("bubonico_burst", "independent", 1.0),
        // AND TWO THE CATALOG NAMES THAT NO ENTRY CAN TAKE:
        //   Tysis | Normal Attack | Projectile | 49 | 49 | 100% | Adding
        //     — Adding at 100% IS ordinary, so it is not an anomaly.
        //   Pox   | DoT Cloud     | AoE        | 20 | 50 | 250% | Adding
        //     — about the CLOUD, whose term reads 50 against its own 20.
        //     There is no per-part CO fraction, so the cloud takes the
        //     ordinary 100% and the weapon's `unmodeled:` says so.
        //   Catabolyst | Partial Reload Impact    | 11 | 11 | 100% | Multiplying
        //   Catabolyst | Reload From Empty Impact | 11 | 11 | 100% | Multiplying
        //     — both name the thrown grenade's contact hit, which the
        //     engine cannot fire because it happens on a RELOAD.
        ("coda_bassocyst", "independent", 1.0),
        //   Coda Hema | Normal Attack | Projectile | 52 | 52 | 100% | Multiplying
        ("coda_hema", "independent", 1.0),
        //   Tenet Arca Plasmor | Normal Attack | Projectile | 760 | 760 | 100% | Multiplying
        // The ORDINARY Arca Plasmor has a row of its own, three lines up.
        ("tenet_arca_plasmor", "independent", 1.0),
        //   Tenet Plinx | Alt Fire Impact | Projectile | 1000 | 1000 | 100%
        //     | Multiplying | "Scales properly with magazine size"
        // The Base Damage cell is 1000 rather than the infobox's 100, which
        // is the catalog independently confirming that this attack's damage
        // is multiplied by the magazine. The PRIMARY fire has no row.
        ("tenet_plinx_charged", "independent", 1.0),
        //   Tenet Spirex | Slug Impact | Projectile | 120 | 120 | 100% | Multiplying
        // The cell is the slug's own 120, which is what tells the row apart
        // from the 80 explosion beside it.
        ("tenet_spirex", "independent", 1.0),
        // AND ONE THE CATALOG NAMES THAT THIS ROSTER CANNOT TAKE:
        //   Tenet Ferrox | Hitscan AoE Direct | AoE | 60 | 200 | 333%
        //     | Adding | "Radial hit receives CO bonus on direct hit only"
        // It is about the RADIAL, whose term reads the DIRECT hit's base of
        // 200 against its own 60. `co_base_fraction` is one number per
        // ENTRY and that entry's direct hit is ordinary, so the radial takes
        // no Condition Overload at all — the conservative reading, stated in
        // the weapon's own `unmodeled:`.
        ("shedu", "independent", 1.0),
        // The row is `Blob Impact | 0% | Does not apply`, and its unmodded 4
        // names the BASE form (the Incarnon deals 50).
        ("stug", "inert", 1.0),
        ("torid", "independent", 1.0),
        // THE SPEARGUNS' THROW, and the row's Attack Name cell is what
        // scopes it: `Throw` on both, so the PRIMARY FIRE entries have no
        // row and stay ordinary. The Unmodded Damage cells are each throw's
        // own total — 150 and 200 — which is what tells the row apart from
        // the explosion beside it.
        ("scourge_thrown", "independent", 1.0),
        ("scourge_prime_thrown", "independent", 1.0),
        // THE ARCH-GUNS. Four rows in the catalog reach the roster, and
        // three of the four are about telling near-identical entries apart:
        //   Grattler       | Normal attack | Projectile | 100% | Multiplying
        //   Larkspur Prime | Alt-fire      | Projectile | 100% | Multiplying
        // The Grattler's row names the ORDINARY weapon and the Kuva
        // Grattler carries it. The Larkspur Prime's row names its ALT-FIRE,
        // which the ordinary Larkspur's alt-fire carries — and NEITHER
        // weapon's normal fire does, because a row scopes to a form.
        ("grattler", "independent", 1.0),
        // Arbucep | Direct Hit | Projectile | 100% | Multiplying, with the
        // note "Consistent on all 6 projectiles … Does not apply to the
        // 228 damage AoE" — the second half is the engine's standing rule.
        ("arbucep", "independent", 1.0),
        ("larkspur_prime_charged", "independent", 1.0),
        // THE CHARGE ARCH-GUNS. The catalog carries a row per FORM here,
        // which is what makes the Mandonel the sharpest entry in it:
        //   Velocitus    | Uncharged attack | Projectile | 100% | Multiplying
        //   Velocitus    | Charged attack   | Projectile | 100% | Multiplying
        //   Corvas Prime | Uncharged Attack | Projectile | 100% | Multiplying
        //   Corvas Prime | Charged Attack   | Projectile | 100% | Multiplying
        //   Mandonel     | Uncharged Attack | Projectile | 100% | Multiplying
        //   Mandonel     | Charged attack   | Hitscan    | 100% | ADDING
        // One weapon, two rows, two answers — so the Mandonel's charged
        // form is ordinary and does not appear here, while its uncharged
        // one does. The ORDINARY Corvas is named on neither row and carries
        // the Prime's on both forms.
        ("velocitus", "independent", 1.0),
        ("velocitus_uncharged", "independent", 1.0),
        ("corvas_prime", "independent", 1.0),
        ("corvas_prime_uncharged", "independent", 1.0),
        ("mandonel_uncharged", "independent", 1.0),
        // THE KITGUNS. A row names a chamber IN ONE SLOT, and the grip in
        // brackets is the assembly it was measured on, not a scope:
        //   Catchmoon (Primary) (Tremor)  | Normal Attack | 216 | 216 | 100% | Multiplying
        //   Catchmoon (Secondary)         | Normal Attack | 256 | 256 | 100% | Multiplying
        //   Sporelacer (Primary) (Tremor) | Normal Attack | 127 | 127 | 100% | Multiplying
        //   Tombfinger (Primary) (Brash)  | Normal Attack |  38 |  38 | 100% | Multiplying
        //   Tombfinger (Secondary)        | Normal Attack |  18 |  18 | 100% | Multiplying
        // The Sporelacer SECONDARY's row is `Adding` against a base of 57
        // "for any configuration", which no fraction can say — it stays
        // ordinary and its `unmodeled:` quotes the row.
        ("catchmoon_primary", "independent", 1.0),
        ("catchmoon_secondary", "independent", 1.0),
        ("sporelacer_primary", "independent", 1.0),
        ("tombfinger_primary", "independent", 1.0),
        ("tombfinger_secondary", "independent", 1.0),

        // THE 2026-08-20 SWEEP, and the reason it found so many at once:
        // every one of these was filed ORDINARY because the check for a row
        // had been run against docs/CATALOGS.md — our own transcription,
        // which by construction only ever carried the rows the roster already
        // had. Reading the WIKI PAGE instead turned up thirty-five entries the
        // Attack Catalog names and the roster contradicted, a third of them
        // weapons that had been here for months (the Lanka at 38%, both Laser
        // Rifles, the Cernos family at 50%).
        //
        // A "Multiplying" row is `independent`; the relative column is
        // `co_base_fraction`, and 100% leaves the field off.
        ("acceltra", "additive_with_base_damage", 0.743),
        ("aeolak", "independent", 1.0),
        ("aeolak_alt", "independent", 1.0),
        ("alternox", "independent", 1.0),
        ("alternox_prime", "independent", 1.0),
        ("basmu", "independent", 1.0),
        ("battacor", "independent", 1.0),
        ("buzlok", "independent", 1.0),
        ("buzlok_beacon", "independent", 1.0),
        ("cernos", "additive_with_base_damage", 0.5),
        ("cinta", "independent", 1.0),
        ("cinta_charged", "independent", 1.0),
        ("daikyu_prime", "additive_with_base_damage", 0.5),
        ("drakgoon", "additive_with_base_damage", 0.57),
        ("epitaph", "independent", 1.0),
        ("evensong", "additive_with_base_damage", 0.65),
        ("exergis", "independent", 1.0),
        ("fulmin_semi", "independent", 1.0),
        ("harpak_harpoon", "independent", 1.0),
        ("javlok", "independent", 1.0),
        ("lanka", "additive_with_base_damage", 0.38),
        ("laser_rifle", "independent", 1.0),
        ("mutalist_cernos", "additive_with_base_damage", 0.5),
        ("mutalist_cernos_uncharged", "independent", 1.0),
        ("nataruk_perfect", "independent", 1.0),
        ("paracyst_harpoon", "independent", 1.0),
        ("prime_laser_rifle", "independent", 1.0),
        ("quellor_alt", "independent", 1.0),
        ("rakta_cernos", "additive_with_base_damage", 0.5),
        ("seer", "independent", 1.0),
        ("stahlta", "independent", 1.0),
        ("stahlta_charged", "independent", 1.0),
        ("steflos", "independent", 1.0),
        ("tenet_envoy", "independent", 1.0),
        ("trumna_grenade", "independent", 1.0),
        //
        // AND A SECOND PASS THE SAME DAY. The first reconciliation matched a
        // row to a form through a short list of attack NAMES, and the
        // catalog names an attack the way that WEAPON's page does — so
        // "Projectile Impact", "Direct Hit", "Lock-On Mode", "Slug Impact"
        // and "Reload From Empty Impact" all missed silently. Nine more:
        ("aegrit", "independent", 1.0),
        ("cyanex", "independent", 1.0),
        ("cyanex_burst", "independent", 1.0),
        ("epitaph_uncharged", "independent", 1.0),
        ("sepulcrum", "independent", 1.0),
        ("sepulcrum_lockon", "independent", 1.0),
        ("tenet_diplos_lock_on", "independent", 1.0),
        // …and one that takes NO Condition Overload at all: "Sonicor |
        // Projectile Impact | 150 | 0 | 0% | N/A | Does not apply". The Stug
        // has carried the same row since it was written.
        ("sonicor", "inert", 1.0),
        // …and the CASTANAS FAMILY, whose every attack carries the same
        // 0% row: "Castanas | Normal Attack | AoE | 160 | 0 | 0% | N/A |
        // Does not apply", and the Sancti's two detonations likewise. On a
        // mine whose damage IS the blast that was the whole weapon taking a
        // term the game does not give it. The Talons has NO row, and
        // absence means ordinary.
        ("castanas", "inert", 1.0),
        ("sancti_castanas", "inert", 1.0),
        // THE ONE ENTRY HERE THAT NO CATALOG ROW NAMES, and it is not an
        // exception to the rule above — it is the AoE rule below, reaching
        // a part this engine has no other slot for.
        //
        // The Grimoire's orb pulses are RANGE DIRECT HITS: each lands on
        // everything within six metres, which makes them an area attack
        // wearing a direct hit's other properties.
        // "AN AoE PART TAKES NO CO unless its own row says so" is therefore
        // the whole answer, and the wiki's catalog was re-read on the PAGE
        // the same day with no Grimoire row of any kind — absence meaning
        // ORDINARY, and ordinary for an area attack is nothing.
        //
        // It has to be said HERE because the contact pulse is filed as the
        // attack's own `damage:`, the only slot an engine that fires a
        // field off an impact has for it; the five that follow are the
        // `lingering:` field and the final blast is the `radial:`, and both
        // of those take no CO by default. So this line is what makes the
        // three halves of one attack agree.
        ("grimoire_active", "inert", 1.0),

        // CARRIED FROM THE FAMILY (docs/CATALOGS.md §1) — entries the catalog
        // does not name, reading the class of a sibling of the same family
        // and the same FORM. The row behind each is quoted on the entry the
        // catalog does name; the sibling is in brackets.
        ("akarius", "independent", 1.0),                  // [akarius_prime]
        ("ballistica_incarnon", "independent", 1.0),      // [ballistica_prime_incarnon]
        ("rakta_ballistica_incarnon", "independent", 1.0),// [ballistica_prime_incarnon]
        ("coda_bubonico", "independent", 1.0),            // [bubonico]
        ("coda_bubonico_burst", "independent", 1.0),      // [bubonico_burst]
        ("corvas", "independent", 1.0),                   // [corvas_prime]
        ("corvas_uncharged", "independent", 1.0),         // [corvas_prime_uncharged]
        ("epitaph_prime", "independent", 1.0),            // [epitaph]
        ("epitaph_prime_uncharged", "independent", 1.0),  // [epitaph_uncharged]
        ("fulmin_prime_semi", "independent", 1.0),        // [fulmin_semi]
        ("kuva_grattler", "independent", 1.0),            // [grattler]
        ("larkspur_charged", "independent", 1.0),         // [larkspur_prime_charged]
        ("latron_wraith_incarnon", "independent", 1.0),   // [latron_incarnon]
        ("mk1_paris_incarnon", "independent", 1.0),       // [paris_incarnon]
        ("trumna_prime_grenade", "independent", 1.0),     // [trumna_grenade]
        // THE ONE FRACTION THAT TRAVELS: every bow the catalog names reads a
        // half, and the row says why — the CO bonus is the uncharged damage
        // and a full charge doubles it. [daikyu_prime]
        ("daikyu", "additive_with_base_damage", 0.5),
    ];

    let mut unexpected = Vec::new();
    let mut wrong = Vec::new();
    for s in all() {
        let beh = s.co_behavior.as_deref().unwrap_or("additive_with_base_damage");
        let fraction = s.co_base_fraction.unwrap_or(1.0);
        let ordinary = beh == "additive_with_base_damage" && (fraction - 1.0).abs() < 1e-9;
        match NAMED.iter().find(|(id, ..)| *id == s.id) {
            None if !ordinary => unexpected.push(format!("{} = {beh} x{fraction}", s.id)),
            Some((_, b, f)) if beh != *b || (fraction - f).abs() > 1e-9 => {
                wrong.push(format!("{}: {beh} x{fraction}, catalog says {b} x{f}", s.id));
            }
            _ => {}
        }
    }
    assert!(
        unexpected.is_empty(),
        "CO anomaly on an entry the catalog does not name — check the row for THIS              weapon's own name, or make it ordinary: {unexpected:?}"
    );
    assert!(wrong.is_empty(), "CO anomaly disagrees with the catalog: {wrong:?}");

    // A ROW THAT NAMES A THROWN GRENADE'S CONTACT is that part's class, not the
    // weapon's: "Catabolyst | Partial Reload Impact" and "| Reload From Empty
    // Impact", both Multiplying — carried to the Coda, the same family and form.
    for id in ["catabolyst", "coda_catabolyst"] {
        let g = crate::data::weapons::spec(id).and_then(|s| s.attack.reload_grenade.as_ref());
        assert_eq!(
            g.and_then(|g| g.contact_co_behavior.as_deref()),
            Some("independent"),
            "{id}: the grenade's contact is the catalog's Multiplying row"
        );
    }

    // …and every listed entry still EXISTS, so a rename cannot quietly
    // empty this list.
    for (id, ..) in NAMED {
        assert!(all().iter().any(|s| s.id == *id), "no weapon entry {id}");
    }

    // AN AoE PART TAKES NO CO unless its own row says so — direct only.
    // Named, not counted, for the same reason the list above is.
    const RADIAL_CO: &[&str] = &[
        // Braton / Mk1 / Prime / Vandal — Incarnon Form Radial Attack
        "braton_incarnon", "mk1_braton_incarnon", "braton_prime_incarnon",
        "braton_vandal_incarnon",
        // Burston / Burston Prime — Incarnon Form Radial Attack
        "burston_incarnon", "burston_prime_incarnon",
        // Zylok / Zylok Prime — Incarnon Form Radial Attack
        "zylok_incarnon", "zylok_prime_incarnon",
        // THE 2026-08-20 SWEEP. Five more radials the catalog gives their
        // own row and this roster had at `false` — which is not the
        // fraction being off, it is the WHOLE CO term missing from an AoE
        // that is most of the weapon. Each carries its relative column in
        // its own comment and admits the fraction it cannot hold:
        //   Ambassador  75%  | Ferrox      350% | Tenet Ferrox 333%
        //   Opticor    250%  | Opt. Vandal 200% | Trumna       164%
        "ambassador_charged", "ferrox", "tenet_ferrox",
        "opticor", "opticor_vandal", "trumna",
    ];
    let mut radial_co: Vec<&str> = all()
        .iter()
        .filter(|s| s.attack.radial.as_ref().is_some_and(|r| r.takes_condition_overload))
        .map(|s| s.id.as_str())
        .collect();
    radial_co.sort_unstable();
    let mut want = RADIAL_CO.to_vec();
    want.sort_unstable();
    assert_eq!(radial_co, want, "a radial takes CO only where the catalog names it");

    // …and the CLOUDS, which are FIELDS rather than explosions — each with
    // its own catalog row and its own flag. The distinction matters here
    // because it is why this roster has more AoE parts taking CO than it
    // has radials.
    //
    //   Torid           | Toxin AoE Cloud         | 40 |  40 |  100% | Multiplying
    //   Pox             | DoT Cloud               | 20 |  50 |  250% | Adding
    //   Mutalist Cernos | Charged AoE Toxin Cloud |  5 | 205 | 4100% | Adding
    //
    // THE POX'S 250% IS NOT EXPRESSIBLE and the weapon says so: its term
    // reads 50 against a cloud whose own base is 20, and `co_base_fraction`
    // is one number per ENTRY whose THROW is ordinary. It takes the
    // ordinary 100% here, which understates a status-stacking build.
    let field_co: Vec<&str> = all()
        .iter()
        .filter(|s| s.attack.lingering.as_ref().is_some_and(|f| f.takes_condition_overload))
        .map(|s| s.id.as_str())
        .collect();
    // THE MUTALIST CERNOS JOINED THEM ON 2026-08-20, and its 4100% is the
    // most extreme relative column in the catalog: a cloud whose own base
    // is 5 and whose CO term reads 205. Same shape as the Pox's 250% and
    // the same admission — the field takes the term at 100% of its own
    // base, which understates a status-stacking build enormously.
    let mut field_co: Vec<&str> = field_co;
    field_co.sort_unstable();
    assert_eq!(
        field_co, ["mutalist_cernos", "pox", "torid"],
        "a lingering field likewise"
    );
}

/// A CO CLASS BELONGS TO THE FAMILY — docs/CATALOGS.md §1 "THE RULE".
///
/// The list above is the transcription and this is the shape it has to have:
/// variants of one family in one FORM read one class, so a row reaching only
/// the entry the wiki happened to type cannot leave its sibling behind. The
/// form is half the key because the catalog rows a weapon's charged shot
/// apart from its uncharged one and may answer differently — the Mandonel
/// does.
///
/// THE CLASS, NOT THE FRACTION: a `co_base_fraction` is measured against one
/// weapon's own damage, so it travels only where the row states a mechanism
/// (the bows' half). The Acceltra pair is where that bites.
///
/// AND THE SLOT, because a Kitgun chamber in two slots is two weapons and
/// the catalog rows them apart — the Sporelacer is Multiplying as a primary
/// and Adding against a fixed 57 as a secondary.
#[test]
fn a_variant_carries_its_familys_condition_overload_class() {
    fn family(w: &crate::data::weapons::WeaponSpec) -> Option<String> {
        w.riven_family.clone().or_else(|| {
            let group = w.transform_group.as_deref()?;
            all()
                .iter()
                .find(|b| b.transform_group.as_deref() == Some(group) && b.riven_family.is_some())?
                .riven_family
                .clone()
        })
    }
    let mut by_family: std::collections::BTreeMap<
        (String, String, String),
        Vec<&crate::data::weapons::WeaponSpec>,
    > = Default::default();
    for w in all() {
        if let Some(f) = family(w) {
            by_family.entry((f, w.form.clone(), w.slot.clone())).or_default().push(w);
        }
    }
    for ((f, form, _slot), ws) in by_family {
        let mut classes: Vec<(&str, &str)> = ws
            .iter()
            .map(|w| (w.id.as_str(), w.co_behavior.as_deref().unwrap_or("additive_with_base_damage")))
            .collect();
        classes.sort_unstable();
        let first = classes[0].1;
        assert!(
            classes.iter().all(|(_, c)| *c == first),
            "{f} ({form}): one family and one form, {classes:?}"
        );
    }
}
/// The Larkspur Prime is the first weapon that can RUN OUT, and this is
/// the whole data path end to end: YAML -> spec -> base -> panel -> sim.
///
/// "On Reload From Empty" opens when the RELOAD COMPLETES.
///
/// Not when the magazine runs out — the difference is
/// the reload itself, 2.5 s of a 17 s window on this weapon. The test that
/// can see it is the FIRST magazine: nothing has reloaded yet, so Deadly
/// Efficiency must be worth exactly nothing.
///
/// Before 2026-08-01 it was worth nothing for the whole run: `on_reload`
/// granting damage fell through to a `CondBuff`, which contributes only
/// under AssumedMax — so the panel showed +220% and the sim showed none.
#[test]
fn a_reload_from_empty_buff_is_worth_nothing_until_the_first_reload() {
    use crate::fight::{monte_carlo, FightParams};
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::StackPolicy;

    let base = WeaponBase::from_data("larkspur_prime", true, &[]);
    let mods = crate::data::mods::pool_for_weapon("larkspur_prime");
    let de = mods.iter().find(|m| m.id == "primed_deadly_efficiency").expect("archgun pool");

    // 200 beam ticks at 12/s is 16.7 s of firing before the magazine is
    // empty (100 rounds at 0.5 each), so 10 s cannot have reloaded.
    let run = |with: bool, secs: f64| {
        let refs: Vec<&crate::model::ModDef> = if with { vec![de] } else { Vec::new() };
        let panel = resolve(&base, &refs, StackPolicy::Emergent);
        let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(secs), &crate::data::arcanes::ArcaneFx::none());
        p.arcane = crate::data::arcanes::ArcaneFx::none();
        p.infinite_reserve = true;
        let s = monte_carlo(&p, 1, 7);
        (s.mean_damage, s.median_run.reloads)
    };
    let (bare, r0) = run(false, 10.0);
    let (armed, r1) = run(true, 10.0);
    assert_eq!((r0, r1), (0, 0), "10 s cannot reach a reload");
    assert!(
        (armed - bare).abs() < 1e-6,
        "no reload has completed, so the buff cannot be up: {bare} vs {armed}"
    );

    // Over a run that DOES reload, it is worth a great deal — the check
    // that the window opens at all, so the assertion above is not passing
    // because the mod does nothing anywhere.
    let (long_bare, _) = run(false, 120.0);
    let (long_armed, rl) = run(true, 120.0);
    assert!(rl >= 1, "120 s reloads");
    assert!(
        long_armed > long_bare * 2.0,
        "+220% base damage at high uptime: {long_bare} vs {long_armed}"
    );
}

/// AMMO EFFICIENCY REACHES A WEAPON BUILT FROM ITS PANEL.
///
/// `FightParams::from_panel` hardcoded `ammo_efficiency_applies: false`,
/// so every weapon the API simulates had ammo efficiency switched off
/// entirely — Primary Crux's +60% did nothing at all. Nothing caught it
/// because every test of the mechanic builds `FightParams` by hand, where
/// the field defaults to `true`; this one goes through the panel, which is
/// the path a request takes.
///
/// The flag is a real distinction, not a nuisance: a CHARGE-BACKED form is
/// "not affected by Ammo Efficiency" (wiki, Torid Incarnon). So the test
/// has two halves — it must reach the Larkspur and it must NOT reach the
/// Torid's Incarnon form.
#[test]
fn ammo_efficiency_survives_the_trip_through_the_panel() {
    use crate::fight::FightParams;
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::StackPolicy;

    let of = |id: &str| {
        let base = WeaponBase::from_data(id, true, &[]);
        let panel = resolve(&base, &[], StackPolicy::Emergent);
        FightParams::from_panel(&panel, &crate::arena::Arena::training(60.0), &crate::data::arcanes::ArcaneFx::none())
        .ammo_efficiency_applies
    };
    assert!(of("larkspur_prime"), "an ordinary weapon spends real ammo");
    assert!(of("verglas_prime"), "so does a sentinel weapon");
    assert!(of("cernos_prime"), "and a bow");
    assert!(
        !of("torid_incarnon"),
        "a charge-backed form is outside the ammo economy (wiki)"
    );
}

/// EVERYTHING THE WEAPON CAN FIRE IS `magazine + reserve`, and the two
/// mods move different halves of it.
///
/// A magazine mod raises the TOTAL, not just how long between reloads:
/// the loaded magazine is ammo you have, and nothing draws it out of the
/// reserve. An ammo-maximum mod raises the reserve alone. Stated because
/// the opposite is a natural thing to assume — that the magazine is the
/// first slice of the reserve — and it would make a magazine mod free.
///
/// The Larkspur Prime is the only weapon that can show it, being the only
/// finite reserve in the roster. Counted in ROUNDS: its primary is a beam
/// and spends 0.5 per tick, so the tick count is double.
#[test]
fn total_ammo_is_the_magazine_plus_the_reserve() {
    use crate::fight::{monte_carlo, FightParams};
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::StackPolicy;

    let base = WeaponBase::from_data("larkspur_prime", true, &[]);
    let pool = crate::data::mods::pool_for_weapon("larkspur_prime");
    let by = |id: &str| pool.iter().find(|m| m.id == id).expect("archgun pool");

    // An hour is far more than any of these can sustain, so what stops the
    // run is always the ammo.
    let rounds = |ids: &[&str]| {
        let refs: Vec<&crate::model::ModDef> = ids.iter().map(|i| by(i)).collect();
        let panel = resolve(&base, &refs, StackPolicy::Emergent);
        let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(3600.0), &crate::data::arcanes::ArcaneFx::none());
        p.arcane = crate::data::arcanes::ArcaneFx::none();
        (panel.magazine_size, panel.ammo_reserve, monte_carlo(&p, 1, 11).mean_shots * 0.5)
    };

    let (m, r, fired) = rounds(&[]);
    assert_eq!((m, r), (100.0, 400.0), "the ground column");
    assert!((fired - 500.0).abs() < 1e-9, "magazine + reserve: {fired}");

    // +60% magazine: the RESERVE is untouched and the total still grew.
    let (m, r, fired) = rounds(&["magazine_extension"]);
    assert_eq!((m, r), (160.0, 400.0), "only the magazine moved");
    assert!((fired - 560.0).abs() < 1e-9, "{fired}");

    // +165% ammo maximum: the MAGAZINE is untouched.
    let (m, r, fired) = rounds(&["primed_ammo_chain"]);
    assert_eq!((m, r), (100.0, 1060.0), "only the reserve moved");
    assert!((fired - 1160.0).abs() < 1e-9, "{fired}");

    // Together they simply add: 160 + 1060.
    let (m, r, fired) = rounds(&["magazine_extension", "primed_ammo_chain"]);
    assert_eq!((m, r), (160.0, 1060.0));
    assert!((fired - 1220.0).abs() < 1e-9, "{fired}");
}

/// AN ARCH-GUN'S FIRE RATE DOES NOT SHORTEN ITS DRAW.
///
/// The two are separate stats on this weapon class,
/// and the mod cards show the split: Shell Rush is "+50% Charge Rate"
/// where Automatic Trigger is "+60% Fire Rate", and Archgun Ace grants
/// "Fire/Charge Rate" — two names one card would not carry if they were
/// one stat. The wiki's general charge formula does divide the draw by
/// fire rate; the Arch-Gun is the exception, which is why the fact rides
/// on the weapon next to `charge_cadence` instead of being assumed.
///
/// The cycle is draw, shot, then an interval of 1/rate — the wiki's own
/// "Effective Fire Rate = 1 / (Modded Charge Time + 1/Modded Fire Rate)".
#[test]
fn an_archgun_charge_answers_to_charge_rate_and_its_interval_to_fire_rate() {
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::{ModEffect, StackPolicy};
    let base = WeaponBase::from_data("larkspur_prime_charged", true, &[]);
    assert!(!base.fire_rate_shortens_draw, "an arch-gun keeps them apart");

    let with = |e: Vec<ModEffect>| {
        let m = crate::model::ModDef {
        stance: None,
            exclusive_to: &[],
            unmodeled: false,
        out_of_scope: false,
            id: "t",
            name: "t",
            base_drain: 0,
            max_rank: 0,
            polarity: crate::rules::capacity::Polarity::Madurai,
            rarity: crate::model::Rarity::Common,
            exilus: false,
            family: None,
            requires_weapon: None,
            excludes_weapon: Vec::new(),
            set: None,
            requires: None,
            disables: Vec::new(),
            effects: e,
        };
        let p = resolve(&base, &[&m], StackPolicy::AssumedMax);
        (p.charge_seconds.expect("a charged form draws"), p.fire_rate)
    };
    let (d0, r0) = with(Vec::new());
    assert!((d0 - 0.5).abs() < 1e-9 && (r0 - 2.0).abs() < 1e-9, "{d0} {r0}");

    // Fire rate moves the INTERVAL only.
    let (d1, r1) = with(vec![ModEffect::FireRate(0.60)]);
    assert!((d1 - 0.5).abs() < 1e-9, "the draw is untouched: {d1}");
    assert!((r1 - 3.2).abs() < 1e-9, "2.0 x 1.6: {r1}");

    // Charge rate moves the DRAW only.
    let (d2, r2) = with(vec![ModEffect::ChargeRate(0.50)]);
    assert!((d2 - 0.5 / 1.5).abs() < 1e-9, "0.5 / 1.5: {d2}");
    assert!((r2 - 2.0).abs() < 1e-9, "the rate is untouched: {r2}");

    // Together: 0.3333 draw + 0.3125 interval = 0.6458 s per shot.
    let (d3, r3) = with(vec![ModEffect::FireRate(0.60), ModEffect::ChargeRate(0.50)]);
    assert!((d3 - 0.5 / 1.5).abs() < 1e-9, "{d3}");
    assert!((d3 + 1.0 / r3 - 0.645833333).abs() < 1e-6, "cycle: {}", d3 + 1.0 / r3);
}

/// Ground Arch-Gun: 100 in the magazine, 400 behind it, and no way to
/// resupply (wiki Arch-Gun). 500 ROUNDS and then the weapon is gone —
/// inside a 120 s engagement, so the clock is not what stops it.
///
/// Rounds, not ticks: the primary is a BEAM and a beam tick costs 0.5
/// ("Beam Weapons consume 0.5 ammo per trace", and the Larkspur Prime page
/// repeats it for this weapon), so 500 rounds is 1000 ticks. This read 500
/// until 2026-08-01, when `ammo_cost` was read at last — the weapon had
/// been running dry in half the time the wiki gives it.
#[test]
fn the_larkspur_runs_out_where_a_primary_would_not() {
    use crate::fight::{monte_carlo, FightParams};
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::StackPolicy;

    let base = WeaponBase::from_data("larkspur_prime", true, &[]);
    assert!((base.ammo_reserve - 400.0).abs() < 1e-9, "the Atmosphere column");
    assert!(base.has_reserve, "400 rounds is a reserve");
    assert!(base.no_resupply, "a ground Arch-Gun cannot be resupplied");

    let panel = resolve(&base, &[], StackPolicy::Emergent);
    let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(120.0), &crate::data::arcanes::ArcaneFx::none());
    p.arcane = crate::data::arcanes::ArcaneFx::none();
    let s = monte_carlo(&p, 1, 3);
    assert!(
        (s.mean_shots - 1000.0).abs() < 1e-9,
        "500 rounds at 0.5 per beam tick, exactly: {}",
        s.mean_shots
    );

    // The clock did not stop it: 120 s at 12 rounds/second is far more.
    assert!(120.0 * panel.fire_rate > 900.0);

    // The alt-fire form draws from the SAME pool — one weapon, one supply.
    let alt = WeaponBase::from_data("larkspur_prime_charged", true, &[]);
    assert!((alt.ammo_reserve - 400.0).abs() < 1e-9);
    assert!(alt.has_reserve && alt.no_resupply);

    // And a Primary with the same shape does NOT run out: the Torid
    // states a 60-round reserve and keeps firing, because ammo pickups
    // exist and the sim does not model them.
    let torid = WeaponBase::from_data("torid", true, &[]);
    let tp = resolve(&torid, &[], StackPolicy::Emergent);
    let mut q = FightParams::from_panel(&tp, &crate::arena::Arena::training(120.0), &crate::data::arcanes::ArcaneFx::none());
    q.arcane = crate::data::arcanes::ArcaneFx::none();
    assert!(monte_carlo(&q, 1, 3).mean_shots > 60.0, "a Primary is resupplied");
}

/// HAVING A RESERVE AND BEING ABLE TO REFILL IT ARE TWO FACTS, and they
/// were one field until 2026-08-04 — which is why the Infinite-ammo control
/// was disabled on every weapon but the Arch-Gun. `has_reserve`
/// is derived from `ammo_max` and is what "truly infinite" means;
/// `no_resupply` is the Arch-Gun's own problem.
#[test]
fn a_reserve_and_a_resupply_are_two_different_facts() {
    use crate::model::WeaponBase;
    // A Primary HAS a reserve — 60 rounds, the wiki's Ammo Max — and can
    // also refill it. So the setting is the player's to make.
    let torid = WeaponBase::from_data("torid", true, &[]);
    assert!((torid.ammo_reserve - 60.0).abs() < 1e-9, "wiki Ammo Max 60");
    assert!(torid.has_reserve, "60 rounds is a reserve");
    assert!(!torid.no_resupply, "a Primary is resupplied from pickups");

    let laetum = WeaponBase::from_data("laetum", true, &[]);
    assert!((laetum.ammo_reserve - 210.0).abs() < 1e-9);
    assert!(laetum.has_reserve && !laetum.no_resupply);

    // A sentinel weapon states no reserve AT ALL — the one case where
    // infinite is not a stand-in for anything. This is the only shape
    // that leaves the control with nothing to decide.
    let verglas = WeaponBase::from_data("verglas_prime", true, &[]);
    assert!((verglas.ammo_reserve - 0.0).abs() < 1e-9);
    assert!(!verglas.has_reserve);
}

/// The scenario's setting stands in for PICKUPS, so it cannot give ammo to
/// a weapon that can receive none.
///
/// IT ASSERTS THE WHOLE CHAIN, not the last method in it. The
/// resupply rule moved out of `reserve_is_infinite` and into
/// `build::scenario::Capability::CanResupply`, where a scenario can also argue
/// with it — so a test that called the method with a raw box value would
/// now be testing half a rule and would go green on an Arch-Gun that had
/// silently become bottomless. It resolves first, exactly as `parse_fight`
/// does.
#[test]
fn the_infinite_ammo_setting_cannot_resupply_an_arch_gun() {
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::StackPolicy;
    use crate::build::scenario::{self, AxisValue};
    let ammo = scenario::axis("infinite_ammo").unwrap();
    let run = |id: &str, ticked: bool| {
        let panel =
            resolve(&WeaponBase::from_data(id, true, &[]), &[], StackPolicy::Emergent);
        let v = scenario::resolve(ammo, id, None)
            .value(AxisValue::Flag(ticked))
            .as_flag()
            .unwrap();
        panel.reserve_is_infinite(v)
    };

    // Sentinel: infinite either way, nothing to decide.
    assert!(run("verglas_prime", true));
    assert!(run("verglas_prime", false));

    // Primary: the setting decides, which is the point.
    assert!(run("torid", true));
    assert!(!run("torid", false));

    // Ground Arch-Gun: finite either way — 400 rounds is the engagement.
    assert!(!run("larkspur_prime", true));
    assert!(!run("larkspur_prime", false));

    // …UNLESS THE FIGHT ITSELF SAYS OTHERWISE, which is the one thing a
    // scenario is allowed to argue with. Same weapon, same ticked box, a
    // class rule that says Arch-Guns are resupplied in here.
    let panel = resolve(
        &WeaponBase::from_data("larkspur_prime", true, &[]),
        &[],
        StackPolicy::Emergent,
    );
    let ruled = scenario::resolve(ammo, "larkspur_prime", Some(AxisValue::Flag(true)))
        .value(AxisValue::Flag(true))
        .as_flag()
        .unwrap();
    assert!(panel.reserve_is_infinite(ruled));
}

/// THE CYCLE DRAWS FROM THE SAME RESERVE. Both forms are one weapon with
/// one supply, but every draw inside the cycle was free until 2026-08-04 —
/// so a finite reserve was ignored on every Incarnon weapon, which is most
/// of the roster (the Infinite-ammo setting has to be adjustable).
#[test]
fn an_incarnon_cycle_runs_dry_like_anything_else() {
    use crate::fight::{monte_carlo, FightParams, LockMode};
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::StackPolicy;
    // 600 s, not 300: the fixture has to actually EXHAUST the reserve to
    // say anything, and after the transform stopped skipping the completing
    // shot's interval a 300 s cycle no longer burned the Boar
    // Prime's supply — both runs fired 1951 shots and the assertion below
    // compared a number to itself.
    let arena = crate::arena::Arena::training(600.0);
    let panel = |id| resolve(&WeaponBase::from_data(id, true, &[]), &[], StackPolicy::Emergent);
    let inc = panel("boar_prime_incarnon");
    let base = panel("boar_prime");
    let mk = |infinite| {
        let mut p = FightParams::incarnon_cycle_from_panels(
            &inc, &base, false, LockMode::Initial(0), &arena, &crate::data::arcanes::ArcaneFx::none());
        p.arcane = crate::data::arcanes::ArcaneFx::none();
        p.infinite_reserve = infinite;
        p
    };
    let free = monte_carlo(&mk(true), 1, 3).mean_shots;
    let dry = monte_carlo(&mk(false), 1, 3).mean_shots;
    assert!(dry < free, "a finite reserve must stop the cycle: {dry} vs {free}");
}

use super::*;

#[test]
fn loads_the_weapon_roster() {
    assert!(spec("dual_toxocyst").is_some());
    assert!(spec("dual_toxocyst_incarnon").is_some());
    // The roster lists base entries only.
    assert!(roster().all(|s| s.transforms_from.is_none()));
}

/// The Torid is the first PRIMARY weapon and the first weapon with a
/// lingering FIELD, so this pins what the loader must produce for it — every
/// number from the wiki data module.
#[test]
fn torid_loads_both_forms_with_its_field_and_direct_hit_gauge() {
    use crate::model::{ChargeOn, FieldStacking};
    let b = base_panel("torid", false);
    assert!((b.base_vector.get(DamageType::Toxin) - 100.0).abs() < 1e-9);
    assert!((b.base_crit_chance - 0.15).abs() < 1e-9);
    assert!((b.base_crit_damage - 2.0).abs() < 1e-9);
    assert!((b.base_status_chance - 0.23).abs() < 1e-9);
    assert!((b.base_fire_rate - 1.5).abs() < 1e-9);
    assert!((b.magazine_size - 5.0).abs() < 1e-9);
    assert!((b.base_reload - 1.7).abs() < 1e-9);
    // "Multiplying" in the CO catalog = an INDEPENDENT multiplier, the
    // opposite of the Laetum's "Adding". Both BASE-form rows say so
    // (Main-fire and Toxin AoE Cloud) — but the INCARNON form does not,
    // which is checked below.
    assert_eq!(b.co_behavior, CoBehavior::Independent);
    // The FIELD, with its own stats: note status 25% where the impact is 23%.
    let f = b.lingering.as_ref().expect("torid leaves a cloud");
    assert!((f.base_vector.get(DamageType::Toxin) - 40.0).abs() < 1e-9);
    assert!((f.tick_rate - 1.0).abs() < 1e-9);
    assert!((f.duration_seconds - 10.0).abs() < 1e-9);
    assert!((f.base_status_chance - 0.25).abs() < 1e-9);
    assert!((f.radius_m - 3.0).abs() < 1e-9);
    // To ZERO at the rim, unlike the Laetum radial's 0.2.
    assert!((f.falloff_reduction - 1.0).abs() < 1e-9);
    assert_eq!(f.stacking, FieldStacking::Stack, "measured (M13)");
    assert!(b.radial.is_none(), "the cloud is a field, not an explosion");

    // The Incarnon form: a continuous beam, ONE attack part (its 2.3 m
    // radius is explicitly not a separate instance), charged by DIRECT hits.
    let i = base_panel("torid_incarnon", false);
    assert!((i.base_vector.get(DamageType::Toxin) - 51.0).abs() < 1e-9);
    assert!((i.base_crit_chance - 0.29).abs() < 1e-9);
    assert!((i.base_crit_damage - 3.1).abs() < 1e-9);
    assert!((i.base_status_chance - 0.39).abs() < 1e-9);
    assert!((i.base_fire_rate - 8.0).abs() < 1e-9, "ticks per second");
    assert!(i.radial.is_none(), "the damage radius is not its own instance");
    assert!(i.lingering.is_none(), "no cloud in Incarnon form");
    let g = i.gauge_form.as_ref().expect("torid_incarnon has a gauge");
    assert_eq!(g.charge_on, ChargeOn::DirectHits);
    assert!((g.charges_to_fill - 5.0).abs() < 1e-9);
    assert!((g.max_charges - 170.0).abs() < 1e-9);
    assert!((g.transmute_in - 1.7).abs() < 1e-9, "= the base reload");
    // Charge-backed magazine, so the pseudo-reload supplies the sim's.
    assert!((i.magazine_size - 170.0).abs() < 1e-9);
    assert!((i.base_reload - 2.7).abs() < 1e-9);
    // CO class is per FORM, not per weapon — ✅ measured. The base form's two catalog rows are "Multiplying"
    // (asserted above); the Incarnon form is ordinary ADDITIVE. This used
    // to be inferred from those rows and the inference was wrong.
    assert_eq!(i.co_behavior, CoBehavior::AdditiveWithBaseDamage);

    // BEAM GEOMETRY — shape, not a damage part. Pinned because it is data
    // now rather than prose, and because one of these values is a decision
    // rather than a citation.
    let bm = i.beam.expect("the incarnon form is a beam");
    assert!((bm.range_m - 37.0).abs() < 1e-9);
    assert!((bm.damage_radius_m - 2.3).abs() < 1e-9);
    assert!(!bm.radius_takes_multishot, "the sphere never takes multishot");
    assert_eq!(bm.chain_hops, 5);
    assert!((bm.chain_range_m - 7.0).abs() < 1e-9);
    assert!((bm.chain_damage_per_hop - 0.75).abs() < 1e-9);
    assert!(!bm.chain_takes_multishot);
    // STILL UNVERIFIED (MEASUREMENTS M15), but no longer a coin flip:
    // an explosion is a damage instance WITH FALLOFF, and the wiki denies
    // this sphere both ("not a separate damage instance from the beam").
    // A node sphere would need a falloff nothing documents. Flipped from
    // the 2026-07-30 `true` on that argument; the Y=1
    // protocol in M15 is what would actually close it.
    assert!(!bm.chain_nodes_have_radius, "one sphere, at the beam's contact point");
    // The base form is not a beam.
    assert!(b.beam.is_none());
}

/// Every weapon REGISTERS its form, and the registration has to agree with
/// the entry's own mechanics — a form is a claim about how the weapon is
/// operated, so a `charge` trigger filed as `base` is a data error, not a
/// stylistic one. This is the check that keeps the vocabulary honest as
/// weapons are added.
#[test]
fn every_weapon_registers_a_form_that_matches_its_mechanics() {
    for s in all() {
        let kind = s.form_kind(); // panics on a name outside the vocabulary
        let charge_trigger = s.attack.trigger == "charge";
        // AN ADAPTER FORM IS EXEMPT, and only that. It carries its own
        // kind — the form vocabulary answers "which form of this weapon",
        // and `incarnon` already answers it — so a form that happens to
        // draw is not thereby the charged form. The Dread's Incarnon form
        // draws for 0.6 s and is not what the arsenal means by "charged
        // Dread". Everything else still has to agree: a `charge` trigger
        // filed as `base` is a data error. NOT the gauge: the Mausolon's
        // alt-fire has one and IS the charged form, because a charge is
        // exactly how it is fired.
        assert_eq!(
            charge_trigger && !kind.is_adapter_form(),
            kind == FormKind::Charged,
            "{}: a charge trigger IS the charged form, and nothing else is",
            s.id
        );
        // AN IMPLICATION, NOT AN EQUIVALENCE. An adapter form is always
        // bought with a gauge, so one without the economy is a half-written
        // entry — but the converse stopped holding when the Mausolon landed
        // a gauge on a `charged` form, and asserting it
        // both ways is what made a real weapon look like a data error.
        assert!(
            !kind.is_adapter_form() || s.gauge_form.is_some(),
            "{}: an Incarnon form is entered by filling a gauge, so it has to declare one",
            s.id
        );
        // The entry reached BY a transform is never the default form.
        // THE GAUGE AGAIN, not the adapter: a form you transform INTO is
        // exactly a form you must pay a meter to reach, and naming where
        // it comes from is how the cycle finds its other end.
        assert_eq!(
            s.transforms_from.is_some(),
            s.has_gauge(),
            "{}: only a transformed-into form has a form to come from",
            s.id
        );
    }
    // A group never registers one kind twice — otherwise a form id could
    // not name a form.
    for s in roster() {
        let forms = forms_of(&s.id);
        let mut kinds: Vec<u8> = forms.iter().map(|f| f.kind as u8).collect();
        kinds.sort_unstable();
        let n = kinds.len();
        kinds.dedup();
        assert_eq!(kinds.len(), n, "{}: duplicate form kind in one group", s.id);
        assert_eq!(
            forms.iter().filter(|f| f.is_default).count(),
            1,
            "{}: exactly one default form",
            s.id
        );
    }
}

/// What the registry reads back: one weapon, its forms, default first.
#[test]
fn a_weapons_forms_are_its_transform_group() {
    // Two forms, and only the Incarnon one is transformed INTO — which is
    // what decides whether there is a cycle to simulate.
    let torid = forms_of("torid");
    assert_eq!(torid.len(), 2);
    assert_eq!(torid[0].kind, FormKind::Base);
    assert!(torid[0].is_default && torid[0].weapon_id == "torid");
    assert_eq!(torid[1].kind, FormKind::Incarnon);
    assert!(!torid[1].is_default && torid[1].weapon_id == "torid_incarnon");
    assert!(has_gauge_switched_form("torid"));
    // Asking from the non-default entry gives the SAME group.
    assert_eq!(forms_of("torid_incarnon").len(), 2);

    // One form is a registration, not an absence — and a beam weapon has
    // nothing to transform into.
    let verglas = forms_of("verglas_prime");
    assert_eq!(verglas.len(), 1);
    assert_eq!(verglas[0].kind, FormKind::Base);
    assert!(verglas[0].is_default);
    assert!(!has_gauge_switched_form("verglas_prime"));

    // TWO forms with NO transformation between them: a bow is drawn or
    // tapped, and switching costs nothing but a shorter press. So it has
    // no cycle to simulate even though it has more than one form — which
    // is the whole reason "does it have two forms" and "does it transform"
    // are separate questions.
    let bow = forms_of("cernos_prime");
    assert_eq!(bow.len(), 2);
    assert_eq!(bow[0].kind, FormKind::Charged, "the arsenal's form comes first");
    assert!(bow[0].is_default && bow[0].weapon_id == "cernos_prime");
    assert_eq!(bow[1].kind, FormKind::Base, "the tapped shot is the uncharged form");
    assert!(!bow[1].is_default && bow[1].weapon_id == "cernos_prime_uncharged");
    assert!(!has_gauge_switched_form("cernos_prime"));
    // Asking from either entry gives the same weapon's forms.
    assert_eq!(forms_of("cernos_prime_uncharged").len(), 2);

    // Wire ids are stable: they are what a saved preset stores.
    assert_eq!(FormKind::Base.id(), "base");
    assert_eq!(FormKind::Incarnon.id(), "incarnon");
    assert_eq!(FormKind::Charged.id(), "charged");
}

/// A WEAPON'S FORCED PROCS REACH THE SIM.
///
/// `FightParams::forced_procs` has existed since the Astilla was written up
/// in MECHANICS §6, and the panel filled it with an empty vector — so the
/// field was real, the sim read it, and no weapon could ever put anything
/// in it. Phantasma Prime's charged form is the first that needs to:
/// "Plasma bomb and seeking projectiles have a guaranteed Impact proc."
///
/// Asserted at BOTH ends, because either alone passes on a broken path: the
/// weapon file says Impact, and the RESOLVED panel still says Impact after
/// the mod layer has been through it.
#[test]
fn a_weapons_forced_procs_survive_resolution() {
    let base = WeaponBase::from_data("phantasma_prime_charged", false, &[]);
    assert_eq!(base.forced_procs, vec![crate::rules::damage::DamageType::Impact]);

    let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::AssumedMax);
    assert_eq!(
        panel.forced_procs,
        vec![crate::rules::damage::DamageType::Impact],
        "a forced proc is the weapon's, so no mod bucket may drop it"
    );

    // ...and the BEAM form forces nothing, so this is not a weapon-wide
    // flag wearing an attack's name.
    assert!(
        WeaponBase::from_data("phantasma_prime", false, &[]).forced_procs.is_empty(),
        "the beam has no guaranteed proc; only the charged bomb does"
    );
}

/// AN EXPLOSION'S FORCED PROC IS ITS OWN, and the Scourge pair is why the
/// field stopped being the attack's alone.
///
/// The two are different questions and the roster holds both answers. The
/// Astilla's DIRECT hit forces Impact and its radial does not; the Scourge
/// pair's page says "Guaranteed Impact proc" of the SPEAR EXPLOSION and
/// nothing of the throw. One shared list can only be right for one of them,
/// and putting the Scourge's on the attack would have forced a proc on a
/// hit the game does not force one on.
///
/// Asserted in BOTH directions on the same weapon, because either alone
/// passes on a flag that is simply always set.
#[test]
fn an_explosions_forced_proc_is_its_own_and_not_the_direct_hits() {
    use crate::rules::damage::DamageType;
    let mut buf = [DamageType::Impact; DamageType::ALL.len()];

    for id in ["scourge_thrown", "scourge_prime_thrown"] {
        let base = WeaponBase::from_data(id, false, &[]);
        assert!(
            base.forced_procs.is_empty(),
            "{id}: the THROW forces nothing — the page says it of the explosion"
        );
        let r = base.radial.as_ref().expect("the spear explodes");
        let n = r.forced_procs.fill(&mut buf);
        assert_eq!(&buf[..n], &[DamageType::Impact], "{id}: the EXPLOSION forces Impact");

        // …and it survives the mod layer, which is where the attack's own
        // list was silently dropped before anything filled it.
        let panel = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::AssumedMax);
        let rr = panel.radial.as_ref().expect("resolved");
        let n = rr.forced_procs.fill(&mut buf);
        assert_eq!(&buf[..n], &[DamageType::Impact], "{id}: still Impact after resolution");
    }

    // THE OTHER DIRECTION, on the primary fire of the same weapons: an
    // explosion that forces nothing must still force nothing.
    for id in ["scourge", "scourge_prime"] {
        let base = WeaponBase::from_data(id, false, &[]);
        let r = base.radial.as_ref().expect("the plasma shot explodes");
        assert!(
            r.forced_procs.is_empty(),
            "{id}: no guaranteed proc is claimed of the primary fire's explosion"
        );
    }
}

/// The Cernos Prime is the first CHARGE-trigger weapon, so this pins the
/// three things a bow brings that no other roster entry has: a draw that
/// replaces the fire-rate cadence, an innate headshot bonus, and a CO term
/// computed off the UNCHARGED base. Every number is the wiki data module's,
/// except `co_base_fraction`, which is the
/// CO catalog's own column.
#[test]
fn cernos_prime_loads_as_a_charged_bow() {
    let b = base_panel("cernos_prime", false);
    // PER ARROW — 3 x 184 = the 552 the page quotes for the whole shot.
    assert!((b.base_vector.get(DamageType::Impact) - 165.6).abs() < 1e-9);
    assert!((b.base_vector.get(DamageType::Puncture) - 9.2).abs() < 1e-9);
    assert!((b.base_vector.get(DamageType::Slash) - 9.2).abs() < 1e-9);
    assert!((b.base_vector.total() - 184.0).abs() < 1e-9);
    assert!((b.base_multishot - 3.0).abs() < 1e-9, "innate 3 arrows");
    assert!((b.base_crit_chance - 0.35).abs() < 1e-9);
    assert!((b.base_crit_damage - 2.0).abs() < 1e-9);
    assert!((b.base_status_chance - 0.30).abs() < 1e-9);
    assert!((b.magazine_size - 1.0).abs() < 1e-9, "one nocked arrow");
    assert!((b.base_reload - 0.65).abs() < 1e-9);
    // The listed stat stays the listed stat; the DRAW is what paces it.
    assert!((b.base_fire_rate - 1.0).abs() < 1e-9);
    assert_eq!(b.charge_seconds, Some(0.5));
    // "(x2 for Bows)" — the clause on every fire-rate mod card.
    assert!((b.fire_rate_mod_multiplier - 2.0).abs() < 1e-9);
    // ExtraHeadshotDmg 0.5 on both attacks ("Deals 50% bonus damage on
    // headshots"), into the additive headshot bracket.
    assert!((b.headshot_damage_bonus - 0.5).abs() < 1e-9);
    // CO catalog: 552 | 276 | 50% | Adding. The 50% is the charge
    // multiplier sitting OUTSIDE the additive bracket, not an evolution
    // exclusion — this weapon has no evolutions.
    assert_eq!(b.co_behavior, CoBehavior::AdditiveWithBaseDamage);
    assert!((b.co_base_fraction() - 0.5).abs() < 1e-9);
    // A bow is neither a beam nor an AoE weapon.
    assert!(!b.continuous);
    assert!(b.radial.is_none() && b.lingering.is_none() && b.beam.is_none());
    // Charge is its own trigger family: a mod gated on `semi_auto`
    // (Semi-Rifle Cannonade) is inert on it, which is the in-game rule. The
    // CLASS is still there, and it is what Longbow Sharpshot needs.
    assert_eq!(b.traits, &["bow"]);
}

/// The TAPPED shot: same weapon, half the damage per arrow, and a cadence
/// of pure nock. Wiki Fire Rate gives bows their own effective-fire-rate
/// formula — `1 / (charge + reload)`, no fire-rate term — so a shot with
/// no draw to pay is paced by the 0.65 s nock alone.
#[test]
fn the_tapped_bow_shot_is_the_same_weapon_at_half_damage() {
    let charged = base_panel("cernos_prime", false);
    let tapped = base_panel("cernos_prime_uncharged", false);
    // "bows have innate 2x damage multiplier when fully charged" — the CO
    // catalog's words, and the two entries hold both sides of it.
    assert!((tapped.base_vector.total() * 2.0 - charged.base_vector.total()).abs() < 1e-9);
    assert!((tapped.base_vector.total() - 92.0).abs() < 1e-9);
    // The draw buys damage, speed and punch through — not crit or status.
    assert_eq!(tapped.base_crit_chance, charged.base_crit_chance);
    assert_eq!(tapped.base_status_chance, charged.base_status_chance);
    assert_eq!(tapped.base_multishot, charged.base_multishot);
    // The innate headshot bonus is the WEAPON's: both attacks carry
    // ExtraHeadshotDmg 0.5 in the module.
    assert!((tapped.headshot_damage_bonus - 0.5).abs() < 1e-9);
    // No charge multiplier to leave out, so CO computes on the full base
    // here — the catalog lists no row for this attack, and absence there
    // is a positive statement.
    assert!((tapped.co_base_fraction() - 1.0).abs() < 1e-9);
    assert!((charged.co_base_fraction() - 0.5).abs() < 1e-9);
    // ZERO draw is a cadence statement, not a missing value: 1 / 0.65 =
    // 1.54 shots/s against the charged form's 1 / 1.15 = 0.87.
    assert_eq!(tapped.charge_seconds, Some(0.0));
    assert!(tapped.fire_rate_mod_multiplier > 1.9, "still a bow");
}

/// The draw, not the fire-rate stat, is what a fire-rate mod shortens —
/// and on a bow it is shortened by DOUBLE the printed bonus.
#[test]
fn fire_rate_mods_halve_a_bow_charge_at_double_value() {
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::{ModEffect, StackPolicy};
    let base = WeaponBase::from_data("cernos_prime", false, &[]);
    let bare = resolve(&base, &[], StackPolicy::AssumedMax);
    assert_eq!(bare.charge_seconds, Some(0.5));
    assert!((bare.fire_rate - 1.0).abs() < 1e-9);

    // Shred: +30% on the card, +60% here.
    let shred = crate::data::mods::class_pool("rifle")
        .into_iter()
        .find(|m| m.id == "shred")
        .expect("shred is in the rifle pool");
    assert!(shred.effects.iter().any(|e| matches!(e, ModEffect::FireRate(v) if (v - 0.30).abs() < 1e-9)));
    let p = resolve(&base, &[&shred], StackPolicy::AssumedMax);
    assert!((p.fire_rate - 1.6).abs() < 1e-9, "1.0 x (1 + 2 x 0.30)");
    // 0.5 / 1.6 = 0.3125 s of draw — the reciprocal of the same bucket.
    assert!((p.charge_seconds.expect("still a bow") - 0.3125).abs() < 1e-9);

    // A non-bow spends the bucket the ordinary way: one x, on the rate.
    let torid = WeaponBase::from_data("torid", false, &[]);
    let t = resolve(&torid, &[&shred], StackPolicy::AssumedMax);
    assert!((t.fire_rate - 1.5 * 1.30).abs() < 1e-9);
    assert!(t.charge_seconds.is_none());
}

/// Firestorm reaches the beam's sphere — "The 2.3 meter damage radius from
/// the point of impact CAN benefit from Firestorm (Primed)." It buys no
/// single-target damage (a struck target is hit once), which is why the
/// panel states the radius rather than a DPS delta.
#[test]
fn blast_range_mods_enlarge_the_beam_sphere() {
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::StackPolicy;
    let base = WeaponBase::from_data("torid_incarnon", false, &[]);
    let bare = resolve(&base, &[], StackPolicy::AssumedMax);
    assert!((bare.beam.expect("beam").damage_radius_m - 2.3).abs() < 1e-9);

    let pool = crate::data::mods::class_pool("rifle");
    let pf: Vec<&crate::model::ModDef> =
        pool.iter().filter(|m| m.id == "primed_firestorm").collect();
    let modded = resolve(&base, &pf, StackPolicy::AssumedMax);
    // +44% Blast Range at max rank.
    assert!(
        (modded.beam.expect("beam").damage_radius_m - 2.3 * 1.44).abs() < 1e-9,
        "expected 3.312 m, got {}",
        modded.beam.unwrap().damage_radius_m
    );
}

/// The CO term uses the FULL base including a perk's flat damage. The CO
/// catalog "lists only discrepant attacks", so the ONE exclusion in the
/// roster is the one it names: Dual Toxocyst's Evolution II **Perk 1**
/// (Carnage Reign). Perk 2 raises base damage too and is absent from the
/// table, so it feeds CO in full — which is why the flag lives on the perk
/// rather than on the weapon or on the Adding behaviour class.
#[test]
fn an_evolutions_flat_damage_stays_out_of_the_co_term_by_default() {
    use crate::model::WeaponBase;
    // THE TORID IS WHERE THIS TEST TURNED AROUND. It asserted 1.0 on both
    // tier-2 perks, on the reading that the weapon's catalog rows say
    // "100 | 100%" — until the owner measured the Incarnon form and every
    // reading off BOTH perks solved to a CO base of ~51, the unevolved
    // value, off panels of 102 and 82 (MEASUREMENTS M50). The rows say
    // 100% of an UNEVOLVED 100, which is true by construction.
    let bare = WeaponBase::from_data("torid", false, &[]);
    let evolved = WeaponBase::from_data("torid", false, &["torid_final_fusillade"]);
    assert!(
        (evolved.base_vector.total() - (bare.base_vector.total() + 51.0)).abs() < 1e-9,
        "the evolution still scales the base"
    );
    // …ON THE FORM THAT WAS MEASURED, which is the Incarnon one. The base
    // form is `Multiplying` and stays at 1.0 until somebody measures a
    // Multiplying entry — see `EvolutionDef::excludes_co_base`.
    assert!(
        (evolved.co_base_fraction() - 1.0).abs() < 1e-9,
        "the Multiplying base form is unmeasured, got {}",
        evolved.co_base_fraction()
    );
    for (perk, panel) in [("torid_final_fusillade", 102.0), ("torid_plentiful_mayhem", 82.0)] {
        let inc = WeaponBase::from_data("torid_incarnon", false, &[perk]);
        assert!((inc.base_vector.total() - panel).abs() < 1e-9);
        // ONE CO BASE, TWO PANELS — the shape the measurement turned on.
        assert!(
            (inc.co_base_fraction() * panel - 51.0).abs() < 1e-9,
            "{perk}: solves to a CO base of {}",
            inc.co_base_fraction() * panel
        );
    }

    // Dual Toxocyst + Carnage Reign (Perk 1, +60 on a 75 base) = the
    // catalog's "100% or 56%" row: a +100% CO adds 75, never 135.
    for form in ["dual_toxocyst", "dual_toxocyst_incarnon"] {
        let frame_seconds = WeaponBase::from_data(form, false, &["dual_toxocyst_carnage_reign"]);
        assert!(
            (frame_seconds.co_base_fraction() - 75.0 / 135.0).abs() < 1e-9,
            "{form}: expected 75/135 = 0.5556, got {}",
            frame_seconds.co_base_fraction()
        );
    }
    // …AND SO DOES PERK 2, WHICH THE CATALOG DOES NOT LIST. Fevered Frenzy
    // also raises base damage (+50), and this assertion read 1.0 until it
    // was measured: at the 125 panel, Galvanized Strike at 3 stacks against 2
    // status types gives 305, and a CO term on the full 125 would give 425
    // (MEASUREMENTS M49,).
    //
    // WHAT IT COST TO LEARN, and why the flag is still per PERK. The
    // catalog's ABSENCE-MEANS-ORDINARY rule produced the old number, and
    // the rule is not repealed — it holds for every other row and the
    // negative controls below still assert it. What is now known is that
    // this weapon's exclusion is the WEAPON's rather than one perk's, so
    // both tier-2 options carry the flag. The Despair is why the
    // granularity stays per perk regardless: one of its two is excluded
    // and the other measurably is not.
    let perk2 =
        WeaponBase::from_data("dual_toxocyst", false, &["dual_toxocyst_fevered_frenzy"]);
    assert!(
        (perk2.base_vector.total() - 125.0).abs() < 1e-9,
        "the +50 still reaches the base, got {}",
        perk2.base_vector.total()
    );
    assert!(
        (perk2.co_base_fraction() - 75.0 / 125.0).abs() < 1e-9,
        "Perk 2 was measured to exclude its own +50 too; expected 75/125, got {}",
        perk2.co_base_fraction()
    );
}

/// The roster is data-driven: dropping in `data/weapons/primary/` publishes
/// a primary weapon with no code change, and its mod pools and arcane slot
/// follow from `mod_pools` and `slot`.
/// A weapon's pool is the union of the pools it draws. The Torid sees the
/// primary-wide mods AND the rifle class pool, and so does Verglas Prime: a
/// sentinel weapon of the primary kind (`weapon_category`).
/// A compat tag is not the whole restriction. Sinister Reach and
/// Combustion Beam are tagged PRIMARY and still cannot go on the Torid
/// — they need a CONTINUOUS weapon, and the Torid is a
/// semi-auto grenade launcher. Its INCARNON form is a beam and that
/// changes nothing: modding is decided on the base form.
#[test]
fn a_beam_only_mod_needs_a_continuous_weapon_to_be_offered_at_all() {
    use crate::data::mods::{pool_for_weapon, pool_union};
    let beam_only = ["sinister_reach", "combustion_beam"];
    let torid = pool_for_weapon("torid");
    for id in beam_only {
        assert!(
            pool_union(&["primary".to_string()]).iter().any(|m| m.id == id),
            "{id} is in the primary pool"
        );
        assert!(
            !torid.iter().any(|m| m.id == id),
            "{id} must not be offered on the Torid"
        );
    }
    // The rest of the primary pool still reaches it.
    assert!(torid.iter().any(|m| m.id == "hunter_munitions"));
    assert!(torid.iter().any(|m| m.id == "vigilante_armaments"));
    // Verglas Prime IS continuous (wiki: Continuous Weapons category) and a
    // primary-kind weapon, so the gate that refuses the Torid lets Combustion
    // Beam in. Sinister Reach stays out on its own `exclusive_to` list.
    assert_eq!(
        spec("verglas_prime").unwrap().attack.trigger,
        "held",
        "continuous, per the wiki category"
    );
    let verglas = pool_for_weapon("verglas_prime");
    assert!(verglas.iter().any(|m| m.id == "combustion_beam"), "Combustion Beam goes on the Verglas Prime");
    assert!(!verglas.iter().any(|m| m.id == "sinister_reach"), "not on Sinister Reach's list");
}

#[test]
fn a_weapons_pool_is_the_union_of_the_pools_it_draws() {
    use crate::data::mods::{class_pool, pool_union};
    let torid = pool_union(&spec("torid").unwrap().mod_pools);
    let verglas = pool_union(&spec("verglas_prime").unwrap().mod_pools);
    let rifle = class_pool("rifle").len();
    let primary = class_pool("primary").len();
    assert!(primary > 0, "data/mods/primary/ exists");
    assert_eq!(torid.len(), rifle + primary, "union of both, no overlap");
    assert_eq!(verglas.len(), rifle + primary, "a primary-kind sentinel weapon: both");
    assert!(torid.iter().any(|m| m.id == "vigilante_armaments"));
    assert!(verglas.iter().any(|m| m.id == "vigilante_armaments"));
    // A mod in two pools would still appear once.
    let mut ids: Vec<&str> = torid.iter().map(|m| m.id).collect();
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), n, "the union deduplicates by id");
}

#[test]
fn the_primary_slot_needed_no_code() {
    let t = spec("torid").expect("torid");
    assert_eq!(t.slot, "primary");
    // A UNION, widest first: primary-wide mods AND the rifle class pool.
    assert_eq!(t.mod_pools, ["primary", "rifle"]);
    assert!(roster().any(|s| s.id == "torid"), "selectable");
    assert!(
        !roster().any(|s| s.id == "torid_incarnon"),
        "a form is not its own roster row"
    );
    assert!(
        !crate::data::mods::class_pool("rifle").is_empty(),
        "the rifle pool has to exist for it to be equippable"
    );
}

#[test]
fn base_panels_match_the_wiki_values() {
    let b = base_panel("dual_toxocyst", true);
    assert!((b.base_vector.get(DamageType::Puncture) - 60.0).abs() < 1e-9);
    assert!((b.base_crit_chance - 0.05).abs() < 1e-9);
    assert!((b.magazine_size - 12.0).abs() < 1e-9);
    assert_eq!(b.injected_elements, vec![(DamageType::Toxin, 1.0)]);
    // TRIGGER *AND* CLASS. The class half is what makes a
    // `requires: dual_pistols` gate satisfiable at all — without it Akimbo
    // Slip Strike equipped and did nothing, on every dual pistol.
    assert_eq!(b.traits, &["semi_auto", "dual_pistols"]);
    assert!(b.gauge_form.is_none());

    let i = base_panel("dual_toxocyst_incarnon", false);
    assert!((i.base_crit_damage - 3.0).abs() < 1e-9);
    assert!((i.magazine_size - 270.0).abs() < 1e-9);
    assert!((i.base_reload - 3.35).abs() < 1e-9);
    let inc = i.gauge_form.expect("incarnon block");
    assert!((inc.max_charges - 270.0).abs() < 1e-9);
    assert!((inc.transmute_in - 2.35).abs() < 1e-9);
    assert!((inc.transmute_out - 1.0).abs() < 1e-9);
    assert!(i.injected_elements.is_empty());
    // The TRIGGER comes from the transform group's BASE entry; the CLASS
    // is the form's own, and both halves of a pair share it anyway.
    assert_eq!(i.traits, &["semi_auto", "dual_pistols"]);
}

/// data/README.md's promotion rule, enforced: perk ids are GLOBALLY
/// unique. A table perk may be referenced by many items; an inline perk
/// may exist in exactly ONE item and must not shadow a table id — two
/// carriers means it should have been promoted to data/perks/.
#[test]
fn perk_ids_are_globally_unique_across_table_and_inlines() {
    use std::collections::HashMap;
    let mut home: HashMap<&str, String> = HashMap::new();
    for p in perks() {
        let prev = home.insert(&p.id, format!("data/perks/{}.yaml", p.id));
        assert!(prev.is_none(), "duplicate table perk id: {}", p.id);
    }
    for w in all() {
        for pr in &w.perks {
            if let PerkRef::Inline(p) = pr {
                if let Some(other) = home.get(p.id.as_str()) {
                    panic!(
                        "inline perk '{}' in weapon '{}' collides with {} — \
                             promote it to data/perks/ and reference the id",
                        p.id, w.id, other
                    );
                }
                home.insert(&p.id, format!("inline in weapon '{}'", w.id));
            }
        }
    }
    // Every bare-id reference must resolve (no dangling perk refs —
    // caught here at test time instead of a runtime panic mid-sim).
    for w in all() {
        for pr in &w.perks {
            if let PerkRef::Id(id) = pr {
                assert!(
                    perk(id).is_some(),
                    "weapon '{}' references unknown perk '{}'",
                    w.id, id
                );
            }
        }
    }
}

#[test]
fn perks_accept_both_reference_and_inline_forms() {
    let yaml = r#"
id: test_gun
name: Test Gun
slot: secondary
class: pistols
form: base
magazine: 10
reload_seconds: 1.0
attack: { trigger: auto, fire_rate: 5.0, crit_chance: 0.1, crit_multiplier: 2.0, status_chance: 0.1, damage: { toxin: 10.0 } }
perks:
  - frenzy
  - id: one_off
    grants: { injected_element: { type: heat, amount: 0.5 } }
"#;
    let s: WeaponSpec = serde_norway::from_str(yaml).unwrap();
    assert_eq!(s.perks[0].id(), "frenzy");
    assert!(s.perks[0].resolve().grants.is_some()); // table lookup works
    assert_eq!(s.perks[1].id(), "one_off");
    let g = s.perks[1].resolve().grants.as_ref().unwrap();
    let inj = g.injected_element.as_ref().unwrap();
    assert_eq!(inj.element, "heat");
    // An inline definition registers in the perk namespace: a bare-id
    // reference from ANOTHER entry finds it.
    let found = inline_perk_in("one_off", std::iter::once(&s)).expect("inline registered");
    assert!(found.grants.is_some());
    assert!(inline_perk_in("nope", std::iter::once(&s)).is_none());
}

#[test]
fn innate_slots_come_from_the_yaml_polarities() {
    let s = innate_slots("dual_toxocyst");
    assert_eq!(s[0], Some(Polarity::Madurai));
    assert_eq!(s[1], Some(Polarity::Naramon));
    assert_eq!(s[2], None);
}

/// THE 1x HEADS ARE EXACTLY THE ONES THE WIKI NAMES — `Enemy_Body_Parts`'s list
/// of weapons "always defaulting to the 1x multiplier", plus the module's
/// `ExtraHeadshotDmg = -2` rows, as far as the roster holds them. Both ways: a
/// listed form left at the enemy's multiplier was worth 3x per headshot on the
/// board, and a stray 1x silently takes a weapon's heads away.
///
/// Read and NOT listed: Cyanex (its page narrows the 1x to the explosion) and
/// the Ignis pair (their pages say they "can deal headshots"). Nataruk's QUICK
/// shot keeps its head — "Quick shots will still benefit from headshots".
#[test]
fn the_one_x_heads_are_the_ones_the_wiki_names() {
    let mut have: Vec<&str> = all()
        .iter()
        .filter(|w| w.headshot_multiplier.is_some())
        .map(|w| w.id.as_str())
        .collect();
    have.sort_unstable();
    let mut want = vec![
        "alternox", "alternox_prime", "arca_plasmor", "tenet_arca_plasmor",
        "athodai_alt", "athodai_prime_alt", "catchmoon_primary", "catchmoon_secondary",
        "dread_incarnon", "fulmin_semi", "fulmin_prime_semi", "lex_incarnon",
        "lex_prime_incarnon", "nataruk_perfect", "paris_incarnon", "mk1_paris_incarnon",
        "paris_prime_incarnon", "steflos",
    ];
    want.sort_unstable();
    assert_eq!(have, want);
}

/// A SENTINEL WEAPON CARRIES TWO TAGS — where it sits (`slot: sentinel`) and
/// what kind of weapon it is (`weapon_category`) — and its pool follows the
/// second: a primary-kind one draws the Primary pool, and no other kind does.
/// Only a sentinel weapon states the category; everyone else's is its slot.
#[test]
fn a_sentinel_weapon_is_also_a_primary_secondary_or_melee_weapon() {
    for w in all().iter().filter(|w| w.inherits.is_none()) {
        let sentinel = w.class.contains("sentinel");
        assert_eq!(w.weapon_category.is_some(), sentinel, "{}: category stated iff sentinel", w.id);
        let cat = w.category();
        assert!(["primary", "secondary", "melee", "archgun"].contains(&cat), "{}: {cat}", w.id);
        if sentinel {
            let primary_pool = w.mod_pools.iter().any(|p| p == "primary");
            assert_eq!(primary_pool, cat == "primary", "{}: {cat} kind, pools {:?}", w.id, w.mod_pools);
        }
    }
}
