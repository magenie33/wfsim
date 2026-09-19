//! THE MODE TABLE, DERIVED — and it must stay derived.
//!
//! One row per weapon on a board is not enough: a Torid played through its
//! Incarnon cycle and a Torid that never transmutes are two different weapons
//! to hold, and only one of them was ever measurable. What each weapon offers
//! falls out of the forms it registers plus one question about the second one —
//! does entering it cost a meter you have to earn?
use super::*;

/// Every weapon can be played in its arsenal form, and that always counts.
#[test]
fn every_weapon_has_a_base_mode_and_it_is_always_rankable() {
    for w in roster() {
        let multishot = play_modes(&w.id);
        let base = multishot.iter().find(|m| m.mode == PlayMode::Base);
        let base = base.unwrap_or_else(|| panic!("{}: no base mode", w.id));
        assert!(base.sustainable, "{}: its own arsenal form is not rankable", w.id);
        assert_eq!(
            multishot.iter().filter(|m| m.mode == PlayMode::Base).count(),
            1,
            "{}: more than one base mode", w.id
        );
    }
}

/// A GAUGE IS WHAT DECIDES IT, and the two shapes are exactly these.
///
/// A second form you pay a meter for gives a CYCLE that a board can rank
/// and an always-in-it mode that it cannot — "always Incarnon" is not a
/// playstyle, it is a few seconds at a time. A second form that is only a
/// different trigger pull can be held forever, so it is rankable and there
/// is no cycle to run.
#[test]
fn a_gauge_gives_a_cycle_and_costs_the_alternate_its_rank() {
    for w in roster() {
        let forms = forms_of(&w.id);
        let multishot = play_modes(&w.id);
        let alt = forms.iter().find(|f| !f.is_default);
        let Some(alt) = alt else {
            assert_eq!(multishot.len(), 1, "{}: one form, so one mode", w.id);
            continue;
        };
        let _ = alt;
        // EVERY alternate form gets its own mode, and a weapon may have
        // more than one: a bow with an adapter has a tapped shot and an
        // Incarnon form.
        let alts: Vec<_> = forms.iter().filter(|f| !f.is_default).collect();
        // THE SAME QUESTION `play_modes` ASKS, asked through the same
        // function, never a copy of it: a ratchet drifting from the thing
        // it ratchets is worse than no ratchet.
        let gauged = |f: &FormRef| is_gauge_fed(f.weapon_id);
        let any_gauged = alts.iter().any(|f| gauged(f));
        let has = |m: PlayMode| multishot.iter().any(|x| x.mode == m);
        let rankable = |m: PlayMode| multishot.iter().any(|x| x.mode == m && x.sustainable);

        // A METERED form contributes its CYCLE and no mode of its own —
        // there is no state to be in, so `Transformed` would be a mode
        // describing nothing.
        let metered = |f: &FormRef| {
            spec(f.weapon_id).is_some_and(|s| s.attack.meter.is_some())
        };
        let own_modes = alts.iter().filter(|f| !metered(f)).count();
        // ONE CYCLE PER (GAUGE FORM, FORM YOU CAN HOLD): which form fills
        // the gauge is a different build, so a weapon with a free second
        // form has a second cycle rather than one that picks for you.
        let cycles = alts.iter().filter(|f| gauged(f)).count()
            * (1 + alts.iter().filter(|f| !gauged(f)).count());
        assert_eq!(
            multishot.len(), 1 + own_modes + cycles,
            "{}: {} forms should give base + one mode each + a cycle per feeder: {:?}",
            w.id, forms.len(), multishot.iter().map(|m| m.id).collect::<Vec<_>>()
        );
        assert_eq!(has(PlayMode::Cycle), any_gauged, "{}: cycle iff gauge", w.id);
        assert_eq!(
            has(PlayMode::Transformed),
            alts.iter().any(|f| gauged(f) && !metered(f)),
            "{}: a gauge-fed mode iff a gauge you can be IN", w.id
        );
        assert_eq!(
            has(PlayMode::Alternate), alts.iter().any(|f| !gauged(f)),
            "{}: a free second form is an alternate", w.id
        );
        // …and the ids are DISTINCT, which is the whole reason the gauged
        // one is its own mode: a build names a mode by id.
        let mut ids: Vec<&str> = multishot.iter().map(|m| m.id).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "{}: two modes share an id: {ids:?}", w.id);

        assert!(!rankable(PlayMode::Transformed),
            "{}: a gauge-fed form cannot be held for an engagement", w.id);
        assert_eq!(rankable(PlayMode::Alternate), has(PlayMode::Alternate),
            "{}: a free form can be held for one", w.id);
    }
}

/// A MODE IS TRANSLATED AT ONE BOUNDARY, into the fight parser's own
/// vocabulary — form KINDS, plus the single policy word for the cycle.
#[test]
fn each_mode_names_the_form_a_fight_would_have_to_run() {
    let f = |w: &str, m: PlayMode| {
        play_modes(w).into_iter().find(|x| x.mode == m).map(|x| x.form())
    };
    // The gauge weapon: never transmute, or run the cycle.
    assert_eq!(f("torid", PlayMode::Base), Some("base"));
    assert_eq!(f("torid", PlayMode::Cycle), Some("gauge_cycle"));
    // TRANSFORMED, not Alternate: being in the Incarnon form for a
    // whole engagement is a thing the builder can show and not a
    // thing a ruler ranks.
    assert_eq!(f("torid", PlayMode::Transformed), Some("incarnon"));
    assert_eq!(f("torid", PlayMode::Alternate), None, "the Torid has no free second form");
    // A bow with an adapter has BOTH, which is why they are two modes.
    assert_eq!(f("paris", PlayMode::Base), Some("charged"));
    assert_eq!(f("paris", PlayMode::Alternate), Some("base"), "the tapped shot");
    assert_eq!(f("paris", PlayMode::Transformed), Some("incarnon"));
    assert_eq!(f("paris", PlayMode::Cycle), Some("gauge_cycle"));
    // The free one, where `base` mode is the CHARGED form because that is
    // what the arsenal hands you — the mode is named for its role, not for
    // the form's own name.
    assert_eq!(f("cernos_prime", PlayMode::Base), Some("charged"));
    assert_eq!(f("cernos_prime", PlayMode::Alternate), Some("base"));
    assert_eq!(f("cernos_prime", PlayMode::Cycle), None);
}

/// The two weapons the owner named, spelled out — a derivation is worth
/// nothing if it derives the wrong table.
#[test]
fn torid_and_cernos_prime_each_offer_two() {
    let on = |id: &str| -> Vec<&'static str> {
        play_modes(id).into_iter().filter(|m| m.sustainable).map(|m| m.mode.id()).collect()
    };
    // The gauge one: fill it and spend it, or never transmute at all.
    assert_eq!(on("torid"), vec!["base", "cycle"]);
    // The free one: charge every arrow, or none of them.
    assert_eq!(on("cernos_prime"), vec!["base", "alternate"]);
    // ...and a weapon with one form offers one.
    assert_eq!(on("ocucor"), vec!["base"]);
}

/// TWO FORMS TO FILL THE GAUGE IN ARE TWO CYCLES. Which half fills it is a
/// build: the tapped shot puts four bolts a press into weakpoints where
/// the charged one pays 0.8 s a press for the same four.
#[test]
fn the_ballistica_prime_can_fill_its_gauge_from_either_shot() {
    let modes = play_modes("ballistica_prime");
    let ids: Vec<&str> = modes.iter().map(|m| m.id).collect();
    assert_eq!(ids, ["base", "alternate", "cycle", "alternate_cycle", "transformed"]);
    let cycle = |id: &str| *modes.iter().find(|m| m.id == id).expect(id);
    // The two spend the SAME form and are fed by different ones.
    assert_eq!(cycle("cycle").weapon_id, "ballistica_prime");
    assert_eq!(cycle("alternate_cycle").weapon_id, "ballistica_prime_uncharged");
    for id in ["cycle", "alternate_cycle"] {
        assert_eq!(cycle(id).other_id, Some("ballistica_prime_incarnon"));
        assert!(cycle(id).sustainable, "{id}: a cycle is a playstyle");
        // ONE POLICY WORD for both: which form feeds it is the mode's own
        // `weapon_id`, and not a second vocabulary at the fight boundary.
        assert_eq!(cycle(id).form(), "gauge_cycle");
    }
    // Four ways to play it, and the Incarnon form alone is not one of them.
    let on: Vec<&str> = modes.iter().filter(|m| m.sustainable).map(|m| m.id).collect();
    assert_eq!(on, ["base", "alternate", "cycle", "alternate_cycle"]);
}

/// A GAUGE WITHOUT AN ADAPTER — the Mausolon.
///
/// Its alt-fire is bought with five kills ("Getting 5 kills with the
/// Mausolon's primary fire will unlock an Alternate Fire", wiki), so it is
/// the same POLICY as a Zariman weapon's and none of the same hardware:
/// no Genesis, no tier-1 unlock, and a `charged` form rather than an
/// `incarnon` one — each of which is a tempting way to recognise a gauge
/// and none of which works.
///
/// Three claims, and each fails on a different half of that:
///   * the cycle EXISTS, and is sustainable — kills keep coming;
///   * the alt-fire alone is NOT, because five kills buy one laser;
///   * the form needs no adapter, so it is not hidden behind an evolution.
#[test]
fn the_mausolon_earns_its_alt_fire_without_an_adapter() {
    let all: Vec<&'static str> = play_modes("mausolon").iter().map(|m| m.id).collect();
    assert_eq!(all, vec!["base", "cycle", "transformed"]);
    let on: Vec<&'static str> = play_modes("mausolon")
        .into_iter()
        .filter(|m| m.sustainable)
        .map(|m| m.mode.id())
        .collect();
    assert_eq!(on, vec!["base", "cycle"]);

    let c = play_modes("mausolon")
        .into_iter()
        .find(|m| m.mode == PlayMode::Cycle)
        .expect("five kills is a gauge");
    assert_eq!(c.weapon_id, "mausolon");
    assert_eq!(c.other_id, Some("mausolon_charged"));

    let alt = spec("mausolon_charged").expect("the alt-fire is a form entry");
    assert!(alt.has_gauge());
    // THE GAUGE IS DECLARED AND THE ADAPTER IS NOT INFERRED FROM IT. This
    // is the pair that was one method until this weapon arrived.
    assert!(!alt.form_kind().is_adapter_form());
    assert_eq!(alt.form_kind(), FormKind::Charged);
    assert!(has_gauge_switched_form("mausolon"));

    let g = alt.gauge_form.as_ref().expect("checked above");
    assert_eq!(g.gauge.charge_on, "kills");
    assert!((g.gauge.charges_to_fill - 5.0).abs() < 1e-9);
    // ONE LASER PER FILL, and no transition either way — the charge IS the
    // shot, so a house-standard 1 s transmute would invent downtime.
    assert!((g.gauge.max_rounds - 1.0).abs() < 1e-9);
    assert!((g.transmute_in_seconds).abs() < 1e-9);
    assert!((g.transmute_out_seconds).abs() < 1e-9);
}

/// A cycle names BOTH ends, because it is the only mode that is about two
/// forms rather than one.
#[test]
fn a_cycle_carries_the_form_it_returns_to_and_the_one_it_spends() {
    let c = play_modes("torid")
        .into_iter()
        .find(|m| m.mode == PlayMode::Cycle)
        .expect("the Torid has a cycle");
    assert_eq!(c.weapon_id, "torid");
    assert_eq!(c.other_id, Some("torid_incarnon"));
    // Every other mode is about one form and says so.
    for m in play_modes("cernos_prime") {
        assert_eq!(m.other_id, None, "{} names a second form", m.mode.id());
    }
}

/// EVERY DIRECT-HIT FALLOFF IS WELL FORMED, and none admits a gap the
/// fight now closes.
#[test]
fn a_weapon_with_damage_falloff_says_it_is_not_modelled() {
    let mut with = 0;
    for w in all() {
        let Some(f) = &w.attack.falloff else { continue };
        with += 1;
        assert!(f.end_m > f.start_m, "{}: falloff {f:?} does not span", w.id);
        // `reduction` is the fraction REMOVED, so zero is no falloff at all
        // and should not be carrying the field.
        assert!(f.reduction < 1.0 && f.reduction > 0.0, "{}: removes {}", w.id, f.reduction);
        assert!(
            !w.unmodeled_parts
                .iter()
                .any(|u| u.reason.as_deref() == Some("damage_falloff")),
            "{} still admits a direct-hit falloff the engine now models",
            w.id
        );
    }
    // …AND NO ENTRY ADMITS ANY FALLOFF AS UNMODELLED. Both the direct hit's
    // and an explosion's are applied at the body's own distance, and an
    // admission that outlives its gap tells a player to distrust a number
    // that is right — on the page, where they read it.
    for w in all() {
        for u in &w.unmodeled_parts {
            let t = u.text.to_lowercase();
            assert!(
                !(t.contains("falloff") && t.contains("no distance")),
                "{} admits a falloff the fight applies: {}",
                w.id,
                u.text
            );
        }
    }
    // The Boar is the shape this exists for: hit-scan, and its damage is
    // halved past 25 m.
    let boar = spec("boar").unwrap().attack.falloff.as_ref().unwrap();
    assert_eq!((boar.start_m, boar.end_m, boar.reduction), (15.0, 25.0, 0.5));
    assert!(with >= 10, "only {with} weapons carry a falloff");
}

/// FIRESTORM REACHES EVERY EXPLOSION BUT THE SHEDU'S.
///
/// *"Explosion cannot benefit from Firestorm (Primed) despite being area of
/// effect"* (wiki Shedu, verbatim) — the roster's only exception, and the
/// owner asked for it to be confirmed rather than assumed.
///
/// The same weapon's OTHER AoE goes the other way: its battery discharge
/// *"is affected by base damage, Faction Damage Bonus, and Firestorm /
/// Primed Firestorm"*. So the blast-radius bucket reaches exactly the one
/// radius Primary Compression is forbidden to spend ("cannot use reload
/// pulse radial"), and the arcane is stuck at the shot's unmoddable 6.6 m.
///
/// It changes no damage while the arena has one target, which is precisely
/// why it needs a test: nothing else would notice it going wrong.
#[test]
fn only_the_shedus_explosion_refuses_the_blast_radius_bucket() {
    let firestorm = crate::data::mods::class_pool("rifle")
        .into_iter()
        .find(|m| m.id == "primed_firestorm")
        .expect("primed firestorm");
    let radius = |id: &str, mods: &[&crate::model::ModDef]| {
        let base = crate::model::WeaponBase::from_data(id, true, &[]);
        crate::build::loadout::resolve(&base, mods, crate::model::StackPolicy::Emergent)
            .radial
            .map(|r| r.radius_m)
    };
    // THE SHEDU: 6.6 m with the mod and without it.
    assert_eq!(radius("shedu", &[]), Some(6.6));
    assert_eq!(radius("shedu", &[&firestorm]), Some(6.6), "Firestorm must not reach it");
    // THE TORID, the same pool and the same mod, moves: its cloud is a
    // `lingering` rather than a radial, so the DIRECT comparison is another
    // radial weapon that does take the bucket.
    let laetum = radius("laetum_incarnon", &[]);
    assert!(laetum.is_some(), "the Laetum's Incarnon form explodes");
    // …and the flag is declared exactly once in the whole roster.
    let refusing: Vec<&str> = all()
        .iter()
        .filter(|w| w.attack.radial.as_ref().is_some_and(|r| !r.takes_blast_radius_mods))
        .map(|w| w.id.as_str())
        .collect();
    assert_eq!(refusing, ["shedu"], "a second weapon started refusing the bucket");
}

/// EVERY COMPRESSION ROW IS TRANSCRIBED, AND EVERY ONE IS SAYABLE.
///
/// The table is the arcane's whole per-weapon behaviour (docs/CATALOGS.md
/// §2) and it is copied by hand, so this asserts the shape rather than
/// trusting the copy: a stacking class the engine has no bracket for, or an
/// effectiveness outside the published range, is a transcription error and
/// not a weapon that behaves strangely.
///
/// The published range is not [0, 1]: the Vectis pair are 0.04 and the
/// Trumna's alt-fire is 1.27 ("Merged"), so the bound is generous on
/// purpose — what it catches is a percent written as 100 instead of 1.0.
#[test]
fn the_roster_reproduces_primary_compressions_published_column() {
    // The table's "Max Damage Bonus @ Base Radius" — the wiki's own
    // arithmetic on its own numbers, and a column we never transcribed:
    // it falls out of the radius, the row and the rank ramp. So it is a
    // CROSS-CHECK rather than a restatement — a radius typed wrong, an
    // effectiveness misread, an override invented, and this stops matching.
    let table: &[(&str, f64)] = &[
        ("shedu", 5.28),
        ("torid", 2.40),              // the Toxin cloud
        ("torid_incarnon", 0.0),      // "Doesn't Work" — the beam exclusion
        ("braton_incarnon", 2.40),
        ("braton_prime_incarnon", 2.40),
        ("braton_vandal_incarnon", 2.40),
        ("mk1_braton_incarnon", 2.40),
        ("burston_incarnon", 1.60),
        ("burston_prime_incarnon", 1.60),
        ("gorgon_incarnon", 4.00),
        ("gorgon_wraith_incarnon", 4.00),
        ("prisma_gorgon_incarnon", 4.00),
        ("latron_incarnon", 3.20),
        ("latron_prime_incarnon", 3.20),
        ("miter_incarnon", 2.40),
        ("strun_incarnon", 3.20),
        ("strun_prime_incarnon", 3.20),
        ("strun_wraith_incarnon", 3.20),
        ("phantasma_charged", 3.84),
        ("phantasma_prime_charged", 3.84),
        // THE ONE OVERRIDE: 0.8 x 0.1 m, not 0.8 x 6.7 m x 4%.
        ("vectis_incarnon", 0.08),
        ("vectis_prime_incarnon", 0.08),
        // THE SPEARGUNS, whose two rows are the same weapon read twice —
        // 1.7 m on the primary fire and 7.0 m on the throw, ordinary in
        // every column. The Prime shares both rows: the catalog's Weapon
        // cell is literally "Scourge (Scourge Prime)".
        ("scourge", 1.36),
        ("scourge_prime", 1.36),
        ("scourge_thrown", 5.60),
        ("scourge_prime_thrown", 5.60),
        // THE ARCH-GUNS. Four rows reach the roster and two of them are a
        // tested ZERO — "Doesn't Work" is a stronger statement than an
        // absent row, so it is carried rather than left to be inferred.
        //   Mausolon | Main-fire Radial | 100% | Adds | Stolen   1.8 m
        //   Mausolon | Alt-fire Radial  | 100% | Adds | Snapshot 8.0 m
        //   Cortege  | Primary Fire+AoE |   0% | Doesn't Work
        //   Cortege  | Alt-Fire + AoE   |   0% | Doesn't Work
        //   Kuva Ayanga | Primary+AoE   |   0% | Doesn't Work
        ("mausolon", 1.44),
        ("mausolon_charged", 6.40),
        ("cortege", 0.0),
        ("cortege_alt", 0.0),
        ("kuva_ayanga", 0.0),
        ("arbucep", 0.0),
        // THE TENET AND CODA BATCH. Six rows, and the last is
        // the third tested ZERO in the catalog.
        //   Tenet Envoy   | Primary Fire + AoE | 100% | Multiplies | 8.0 m | +640%
        //   Tenet Tetra   | Alt-Fire + AoE     | 100% | Multiplies | 8.0 m | +640%
        //   Tenet Ferrox  | Primary Fire + AoE | 100% | ADDS       | 4.0 m | +320%
        //   Tenet Quanta  | Alt-Fire + AoE     | 100% / 8% | Multiplies | 0.5 m | +40%
        //   Coda Bubonico | Alt-Fire + AoE     | 100% | Multiplies | 7.0 m | +560%
        //   Tenet Ferrox  | Throw + AoE        |   0% | Doesn't Work
        // The Quanta's two effectiveness figures are its cube's TWO
        // explosions — 100% on the 0.5 m contact blast this roster fires,
        // 8% on the 6 m one a player shoots loose, which is unmodelled. The
        // base-radius column agrees with the first, so the single figure is
        // right for what the entry carries.
        ("tenet_envoy", 6.40),
        ("tenet_tetra_grenade", 6.40),
        ("tenet_ferrox", 3.20),
        ("tenet_quanta_cube", 0.40),
        ("coda_bubonico_burst", 5.60),
        ("tenet_ferrox_thrown", 0.0),
        // THE NINETEEN BASE WEAPONS. The Ferrox row is ONE row
        // covering both variants — its base-radius cell reads
        // "3.6 m (4.0 m)", the parenthetical being the Tenet's — which is
        // the opposite of the CO table's rule and safe only because the
        // cell says so.
        //   Bubonico       | Alt-Fire + AoE     | 100% | Multiplies | 7.0 m | +560%
        //   Ferrox         | Primary Fire + AoE | 100% | ADDS       | 3.6 m | +288%
        //   Ferrox         | Throw + AoE        |   0% | Doesn't Work
        //   Quanta         | Alt-Fire + AoE     | 100% / 8% | Multiplies | 0.5 m | +40%
        //   Quanta Vandal  | Alt-Fire + AoE     | 100% / 8% | Multiplies | 0.5 m | +40%
        //   Glaxion Vandal | Primary Fire + AoE |   0% | Doesn't Work | 2.0 m
        // The Glaxion Vandal's is the fourth tested ZERO in the catalog and
        // the general exclusion applied to a real radius: a beam attack
        // with an AoE component.
        ("bubonico_burst", 5.60),
        ("ferrox", 2.88),
        ("ferrox_thrown", 0.0),
        ("quanta_cube", 0.40),
        ("quanta_vandal_cube", 0.40),
        ("glaxion_vandal", 0.0),

        // THE 2026-08-20 SWEEP. The published table named FIFTY-NINE more
        // roster attacks than the roster had transcribed — most of them from
        // this month's intake, and a dozen that had been here far longer. An
        // attack with no `compression:` pays the arcane NOTHING (
        // `build::loadout::resolve` reads `Some(c)` or nothing at all), so every one
        // of them was silently worth zero to a build carrying it.
        //
        // Each figure below is OUR radius x 0.8, which is what the arcane
        // takes. Where that disagrees with the table's own Max Damage Bonus
        // column the line says so, and there are exactly three:
        //
        //   lenz / prisma_lenz — 7.2 m x 0.8 is 5.76 and the table rounds its
        //     own arithmetic to +575%.
        //   secura_penta — the table gives the three Pentas ONE row at 4.0 m,
        //     and this weapon's own module row is 6.0 m. The weapon wins.
        //   battacor_charged — the table's radius column says 3.4 m and its
        //     bonus column says +208%, which is 2.6 m. The table disagrees
        //     with ITSELF there; ours follows its radius column.
        ("acceltra", 3.20),
        ("acceltra_prime", 4.00),
        ("aeolak_alt", 5.60),
        ("afentis", 2.40),
        ("afentis_prime", 4.40),
        ("alternox_alt", 4.80),
        ("alternox_prime_alt", 4.80),
        ("ambassador_charged", 4.80),
        ("astilla", 1.92),
        ("astilla_prime", 1.92),
        ("basmu", 1.36),
        ("battacor_charged", 2.72),   // table prints +208%
        ("carmine_penta", 3.20),
        ("cedo_alt", 4.80),
        ("cedo_prime_alt", 4.80),
        ("coda_sporothrix", 1.60),
        ("corinth_airburst", 7.52),
        ("corinth_prime_airburst", 7.84),
        ("enkaus_alt", 0.0),
        ("evensong", 3.20),
        ("gaze_primary", 0.0),
        ("grattler", 0.0),
        ("ignis", 0.0),
        ("ignis_wraith", 0.0),
        ("javlok", 1.92),
        ("javlok_throw", 4.80),
        ("komorex", 0.0),
        ("kuva_bramma", 6.64),
        ("kuva_chakkhurr", 2.32),
        ("kuva_grattler", 0.0),
        ("kuva_ogris", 6.32),
        ("kuva_tonkor", 5.60),
        ("kuva_zarr", 5.60),
        ("larkspur_charged", 0.0),
        ("larkspur_prime_charged", 0.0),
        ("lenz", 5.76),   // table prints +575%
        ("morgha", 0.0),
        ("morgha_alt", 0.0),
        ("mutalist_cernos", 0.0),
        ("mutalist_quanta_orb", 3.52),
        ("ogris", 5.68),
        ("opticor", 4.80),
        ("opticor_quick", 4.80),
        ("opticor_vandal", 3.68),
        ("opticor_vandal_quick", 3.68),
        ("panthera_prime", 1.28),
        ("penta", 3.20),
        ("prisma_lenz", 5.76),   // table prints +575%
        ("proboscis_cernos", 5.60),
        ("secura_penta", 4.80),   // table prints +320%
        ("simulor", 4.00),
        ("sporelacer_primary", 1.68),
        ("sporothrix", 1.36),
        ("stahlta_charged", 0.0),
        ("synoid_simulor", 4.00),
        ("tombfinger_primary", 4.96),
        ("tonkor", 5.60),
        ("trumna", 1.28),
        ("trumna_prime", 1.28),
        ("vadarya_prime", 0.0),
        ("zarr", 3.92),
        ("zhuge_prime", 2.08),
    ];
    // At rank 5 a metre is worth +100%, so the bonus IS the metres lost.
    let fx = crate::data::arcanes::for_slot("primary", "primary_compression")
        .expect("the arcane is in the primary pool")
        .fx(5, crate::model::StackPolicy::Emergent, &[], crate::data::tenno::default_tenno());
    assert_eq!(fx.compression_damage_per_m, 1.0, "+100% per metre at max rank");
    for (id, expected) in table {
        let base = crate::model::WeaponBase::from_data(id, true, &[]);
        let p = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
        let bonus = p.compression.map_or(0.0, |c| c.radius_lost_m) * fx.compression_damage_per_m;
        assert!(
            (bonus - expected).abs() < 5e-3,
            "{id}: the table says +{}%, this build pays +{:.1}%",
            expected * 100.0, bonus * 100.0
        );
    }
    // …and every row the roster carries is in the list above, so a new
    // weapon cannot join the catalog without its column being checked.
    // `all()`, not `roster()`: a compression row belongs to an ATTACK, and
    // most of these are Incarnon FORMS, which are entries of their own.
    let carried: Vec<&str> = all()
        .iter()
        .filter(|w| w.attack.compression.is_some())
        .map(|w| w.id.as_str())
        .collect();
    for id in &carried {
        assert!(table.iter().any(|(t, _)| t == id), "{id} has a row and no expected bonus");
    }
    assert_eq!(carried.len(), table.len());
}

/// A THROW PAYS FOR ITS OWN RELOAD, so the wind-up is not the cycle.
///
/// The spearguns' alt-fire is wind-up → release → reload, every throw
/// — the reload is unconditional rather than a magazine
/// running dry. That is a `magazine: 1` weapon, the same shape as a bow's
/// nock, and it is worth pinning because the entry carried the PRIMARY
/// FIRE's 40 rounds for two days: the sim then threw 40 times between
/// reloads and the mode read 59% faster than it is.
///
/// The sharp half is the second assertion. With one magazine per throw the
/// reload is a FLOOR the wind-up cannot cross, so a fire-rate bonus buys
/// only the wind-up's share of the cycle — under a 40-round magazine it
/// bought the whole thing, which is what made a fire-rate build the
/// obvious one on a weapon where it is not.
#[test]
fn a_thrown_speargun_paces_on_wind_up_plus_reload() {
    use crate::fight::{monte_carlo, FightParams};
    const DURATION: f64 = 180.0;
    // Both entries: the Prime is not a different mechanic.
    for id in ["scourge_thrown", "scourge_prime_thrown"] {
        assert_eq!(
            spec(id).unwrap().magazine,
            Some(1.0),
            "{id}: one throw is one magazine — the 40 rounds are the primary fire's"
        );
        let run = |mods: &[&crate::model::ModDef]| {
            let b = crate::model::WeaponBase::from_data(id, true, &[]);
            let p = crate::build::loadout::resolve(&b, mods, crate::model::StackPolicy::Emergent);
            let params = FightParams::from_panel(
                &p,
                &crate::arena::Arena::training(DURATION),
                &crate::data::arcanes::ArcaneFx::none(),
            );
            // The cycle the data describes, against the one the sim ran:
            // the first throw costs no wind-up, so the count is one more
            // than the cycles that fit.
            let cycle = 1.0 / params.fire_rate + params.reload_seconds;
            let shots = monte_carlo(&params, 8, 5).mean_shots;
            let want = (DURATION / cycle).floor() + 1.0;
            assert!(
                (shots - want).abs() < 1e-9,
                "{id}: {shots} throws in {DURATION}s, but a {cycle:.3}s cycle fits {want}"
            );
            (params.fire_rate, shots)
        };
        // Bare: 1.0 s of wind-up + 0.6 s of reload.
        let (rate, shots) = run(&[]);
        assert!((rate - 1.0).abs() < 1e-9, "{id}: the wind-up is 1 / {rate}");

        // …AND A FIRE-RATE MOD CANNOT BUY THE RELOAD. Stated as the
        // inequality rather than as a figure, so it holds whatever the
        // card is worth: throughput rises, and by strictly less than the
        // fire rate did.
        let vile = crate::data::mods::class_pool("rifle")
            .into_iter()
            .find(|m| m.id == "vile_acceleration")
            .expect("vile acceleration is in the rifle pool");
        let (fast_rate, fast_shots) = run(&[&vile]);
        assert!(fast_rate > rate, "{id}: the mod must raise the rate");
        assert!(
            fast_shots > shots && fast_shots / shots < fast_rate / rate - 1e-9,
            "{id}: x{:.3} fire rate bought x{:.3} throws — the reload is a floor",
            fast_rate / rate,
            fast_shots / shots
        );
    }
}

/// THE THROW PLANTS A FIELD, and the field outlives the throw after it.
///
/// The Bullet Attractor is the Void effect, so it is
/// worth one line in the Condition Overload counter and nothing else here.
/// What makes it worth a test is the ARITHMETIC of the two clocks: 4.7 s
/// on the target against a 1.6 s throw cycle, so from the second throw on
/// it is simply up — and a new throw destroying the OLD FIELD does not
/// take back what that field already applied.
///
/// Measured through the CO count rather than through the debuff, because
/// the count is the only thing the field is worth: a build with a CO mod
/// must be worth more on the throw than the same build is without the
/// field, and the gap must be exactly one status type's share.
#[test]
fn a_thrown_speargun_plants_a_bullet_attractor_that_counts() {
    use crate::fight::{monte_carlo, FightParams};
    for id in ["scourge_thrown", "scourge_prime_thrown"] {
        let b = crate::model::WeaponBase::from_data(id, true, &[]);
        assert_eq!(b.attractor_seconds, Some(4.7), "{id}: the wiki's 4.7 s");
        let p = crate::build::loadout::resolve(&b, &[], crate::model::StackPolicy::Emergent);
        let mut params = FightParams::from_panel(
            &p,
            &crate::arena::Arena::training(60.0),
            &crate::data::arcanes::ArcaneFx::none(),
        );
        assert_eq!(params.attractor_seconds, Some(4.7), "{id}: through the panel");
        // The whole claim, stated as damage: a Condition Overload build
        // that counts the field beats the same build that cannot see it.
        params.co_per_type = 0.8;
        params.co_behavior = crate::model::CoBehavior::Independent;
        let with = monte_carlo(&params, 24, 7).mean_effective_damage;
        let without = FightParams { attractor_seconds: None, ..params.clone() };
        let without = monte_carlo(&without, 24, 7).mean_effective_damage;
        assert!(
            with > without * 1.02,
            "{id}: the field must reach the CO count — {without:.0} -> {with:.0}"
        );
    }
    // NEGATIVE CONTROL: nobody else plants one. The debuff has exactly two
    // sources — this attack and Xata's Whisper's Void instance — and a
    // field granted to a weapon that has none would be invisible in every
    // other test here.
    let planters: Vec<&str> = all()
        .iter()
        .filter(|w| w.attack.attractor_seconds.is_some())
        .map(|w| w.id.as_str())
        .collect();
    assert_eq!(planters, vec!["scourge_prime_thrown", "scourge_thrown"]);
}

/// AIMING IS THE WHOLE CONDITION — *"On aim: x0.2 explosion radius"*.
#[test]
fn compression_is_worth_nothing_to_a_player_who_is_not_aiming() {
    let base = crate::model::WeaponBase::from_data("shedu", true, &[]);
    let mut hipfire = crate::data::tenno::default_tenno().clone();
    hipfire.state.aiming = false;
    let aimed = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let hip = crate::build::loadout::resolve_for(
        &base, &[], crate::model::StackPolicy::Emergent, &hipfire,
    );
    assert!(aimed.compression.is_some_and(|c| c.radius_lost_m > 5.27));
    assert!(hip.compression.is_none(), "no aim, no trade, no bonus");
}

#[test]
fn every_compression_row_is_one_the_engine_could_apply() {
    let mut rows = 0;
    let mut adds = 0;
    for w in all() {
        let Some(c) = &w.attack.compression else { continue };
        rows += 1;
        assert!(
            matches!(c.stacking.as_str(), "multiplies" | "adds"),
            "{}: `{}` is not a stacking class",
            w.id, c.stacking
        );
        assert!(
            (0.0..=1.5).contains(&c.effectiveness),
            "{}: effectiveness {} — a percent written as a whole number?",
            w.id, c.effectiveness
        );
        // THE THIRD COLUMN, and the legend's own vocabulary. `Snapshot` is
        // the ordinary value; the other three each mean the arcane reads a
        // radius that is not this attack's own, which is the part a
        // transcription flattens into "it works".
        assert!(
            matches!(
                c.radius_calculation.as_str(),
                "snapshot" | "stolen" | "doesnt_work" | "constant_check"
            ),
            "{}: `{}` is not a Radius Calculation",
            w.id, c.radius_calculation
        );
        // An OVERRIDE is only ever the reason a row's radius is not the
        // attack's, so it must not also be at full effectiveness — that
        // pair would be two answers to one question.
        assert!(
            c.reads_radius_m.is_none() || c.effectiveness != 1.0,
            "{}: reads_radius_m with 100% effectiveness — which one is the radius?",
            w.id
        );
        // …and the two that must agree: an effectiveness of zero IS
        // "Doesn't Work", spelled in the other column.
        assert_eq!(
            c.effectiveness == 0.0,
            c.radius_calculation == "doesnt_work",
            "{}: effectiveness {} against radius calculation `{}`",
            w.id, c.effectiveness, c.radius_calculation
        );
        if c.stacking == "adds" {
            adds += 1;
        }
    }
    assert!(rows >= 80, "only {rows} rows transcribed");
    // THE MINORITY IS REAL, and it is the half of the table most likely to
    // be flattened by a copy. Eighteen rows print "Adds": every Braton and
    // Burston Incarnon (six), BOTH Mausolon radials — its two rows are the
    // only Arch-Gun ones in the table and both print it — both Ferroxes,
    // and the eight the 2026-08-20 sweep brought in (the Ambassador's
    // charge, the Battacor's, both Opticors in both forms, and both
    // Trumnas' primary fire).
    assert_eq!(
        adds, 18,
        "the Adds minority moved — count it against the table before changing this"
    );
    // …and every one of them is named, so a row that quietly changes
    // bracket is a failure rather than a number that still adds up.
    let adders: std::collections::BTreeSet<&str> = all()
        .iter()
        .filter(|w| w.attack.compression.as_ref().is_some_and(|c| c.stacking == "adds"))
        .map(|w| w.id.as_str())
        .collect();
    assert_eq!(
        adders,
        [
            "ambassador_charged", "battacor_charged",
            "braton_incarnon", "braton_prime_incarnon", "braton_vandal_incarnon",
            "burston_incarnon", "burston_prime_incarnon",
            "ferrox", "tenet_ferrox",
            "mausolon", "mausolon_charged",
            "mk1_braton_incarnon",
            "opticor", "opticor_quick", "opticor_vandal", "opticor_vandal_quick",
            "trumna", "trumna_prime",
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>()
    );
    // …and a tested ZERO is not the same as an absent row. The Torid's
    // Incarnon form has one, and its base form is 100% — one arcane, two
    // answers, inside one weapon's cycle.
    assert_eq!(spec("torid_incarnon").unwrap().attack.compression.as_ref().unwrap().effectiveness, 0.0);
    assert_eq!(spec("torid").unwrap().attack.compression.as_ref().unwrap().effectiveness, 1.0);
    // The Vectis pair are the reason the range is not a boolean.
    assert_eq!(spec("vectis_incarnon").unwrap().attack.compression.as_ref().unwrap().effectiveness, 0.04);
}

/// THE TORID'S CO CATALOG ROWS, pinned — both of them, and both forms.
///
/// The wiki's Condition Overload catalog gives this weapon TWO rows:
///
/// > Torid | Main-fire | Projectile | 100 | 100 | 100% | Multiplying
/// > Torid | Toxin AoE Cloud | AoE | 40 | 40 | 100% | Multiplying
///
/// Three facts are load-bearing and none is the default:
///
/// 1. MULTIPLYING, which is `Independent` here — a free-standing
///    `x (1 + co x types)` rather than a share of the base-damage bucket.
/// 2. THE CLOUD TAKES IT TOO, where CO is a direct-hit bonus everywhere
///    else.
/// 3. THE INCARNON FORM DOES NOT: it is ordinary additive, so one weapon's
///    two forms disagree — the shape a refactor flattens unnoticed, since
///    both would still "have CO".
///
/// The base fractions are 100% on both rows, the default, so they are
/// asserted rather than declared.
#[test]
fn the_torid_carries_both_of_its_co_catalog_rows() {
    use crate::model::CoBehavior;
    let base = spec("torid").expect("torid");
    let inc = spec("torid_incarnon").expect("torid_incarnon");

    // 1. MAIN FIRE: 100 Toxin, multiplying, on the whole base.
    assert_eq!(base.attack.damage.get("toxin").copied(), Some(100.0));
    assert_eq!(base.co_behavior.as_deref(), Some("independent"));
    assert_eq!(base.co_base_fraction.unwrap_or(1.0), 1.0);

    // 2. THE CLOUD: 40 Toxin, and it takes CO — the anomaly.
    let cloud = base.attack.lingering.as_ref().expect("the Torid's cloud");
    assert_eq!(cloud.damage.get("toxin").copied(), Some(40.0));
    assert!(
        cloud.takes_condition_overload,
        "the catalog gives the cloud its own Multiplying row"
    );

    // 3. THE INCARNON FORM IS ORDINARY, and has no cloud to argue about.
    assert_eq!(inc.co_behavior.as_deref(), Some("additive_with_base_damage"));
    assert!(inc.attack.lingering.is_none(), "the Incarnon form is a beam");

    // …and the two really do resolve to different brackets, which is the
    // claim rather than the spelling.
    let resolved = |id: &str| {
        crate::build::loadout::resolve(
            &crate::model::WeaponBase::from_data(id, true, &[]),
            &[],
            crate::model::StackPolicy::Emergent,
        )
        .co_behavior
    };
    assert_eq!(resolved("torid"), CoBehavior::Independent);
    assert_eq!(resolved("torid_incarnon"), CoBehavior::AdditiveWithBaseDamage);
}

/// EVERY SPOOL RECONCILES WITH ITS OWN PAGE, and says what it costs.
///
/// The field is small and easy to typo into silence — serde ignores what it
/// does not know — so every weapon's numbers are asserted by value. The
/// risers get a second, stronger check: each page states its spool TWICE,
/// as a percentage per shot and as a count of shots to optimal, and the two
/// must agree. `over_shots` carries the span (the exact half) and this
/// re-derives BOTH published figures from it, so a mistyped span cannot
/// survive — it would have to be wrong in a way that keeps two independent
/// sentences true.
#[test]
fn every_spool_reconciles_with_its_own_page() {
    // (weapon, start, end, over_shots, the page's "% per shot", its
    //  "N shots before optimal" — the last two are what the wiki prints.)
    let published: &[(&str, f64, f64, f64, f64, i64)] = &[
        ("phenmor_incarnon", 1.00, 0.60, 51.0, 0.0, 0),
        ("gorgon", 0.20, 1.00, 7.5, 0.10667, 9),
        ("gorgon_wraith", 0.20, 1.00, 5.0, 0.16, 6),
        ("prisma_gorgon", 0.20, 1.00, 6.0, 0.13333, 7),
        ("soma", 0.25, 1.00, 5.0, 0.15, 6),
        ("soma_prime", 0.25, 1.00, 2.5, 0.30, 4),
        // "Fire rate ramps from 20% baseline, increasing by 20% per shot",
        // so it is full from the FIFTH round: 0.20 + 4 x 0.20 = 1.00. The
        // page's Disadvantages also say "Requires 5-12 shot spool before
        // optimal performance" — a RANGE because two things spool at
        // different speeds on this weapon: the fire rate over 5 shots and
        // the PELLET COUNT over 12. Only the first is modelled, which is
        // what the weapon's own admission says.
        ("kuva_kohm", 0.20, 1.00, 4.0, 0.20, 5),
        // "Requires a spool-up of 7 shots before optimal fire rate is
        // achieved", and "Fire rate starts at 30% of the listed value, and
        // increases by 11.67% per shot" — 0.70 / 6 = 11.67% a shot, full
        // from the 7th, so the two published sentences reconcile exactly.
        ("coda_bubonico", 0.30, 1.00, 6.0, 0.11667, 7),
        // "Requires a spool-up of 5 shots before optimal fire rate is
        // achieved", and "fire rate starts at 10% of the listed value, and
        // increases by 22.5% per shot" — 0.90 / 4 = 22.5% a shot, full from
        // the 5th. The lowest opening rate in the roster.
        ("supra", 0.10, 1.00, 4.0, 0.225, 5),
        // "…a spool-up of 4 shots", "starts at 40% … increases by 20% per
        // shot" — 0.60 / 3 = 20% a shot, full from the 4th.
        ("supra_vandal", 0.40, 1.00, 3.0, 0.20, 4),
        // "Primary fire requires a spool-up of 9 shots before optimal fire
        // rate is achieved", and "fire rate starts at 40% of the listed
        // value, and increases by 7.5% per shot" — 0.60 / 8 = 7.5% a shot,
        // full from the 9th. The Prime's page prints the same two numbers.
        ("tenora", 0.40, 1.00, 8.0, 0.075, 9),
        ("tenora_prime", 0.40, 1.00, 8.0, 0.075, 9),
    ];
    for (id, start, end, over, per_shot, full_at) in published {
        let s = spec(id)
            .unwrap_or_else(|| panic!("{id}"))
            .attack
            .sustained_fire_rate
            .unwrap_or_else(|| panic!("{id} lost its spool"));
        assert_eq!((s.start, s.end, s.over_shots), (*start, *end, *over), "{id}");
        assert!(s.start > 0.0 && s.end > 0.0, "{id}");
        if *full_at == 0 {
            continue; // the faller: its page gives a span, not a count
        }
        // THE TWO PUBLISHED FIGURES, both from `over_shots`.
        assert!(
            ((s.end - s.start) / s.over_shots - per_shot).abs() < 5e-5,
            "{id}: {} a shot, page says {per_shot}",
            (s.end - s.start) / s.over_shots
        );
        assert_eq!(s.over_shots.ceil() as i64 + 1, *full_at, "{id}: full-at shot");
    }
    // …and every one of them owes the reader the play pattern it assumes.
    let mut with = 0;
    for w in all() {
        if w.attack.sustained_fire_rate.is_none() {
            continue;
        }
        with += 1;
        assert!(
            w.unmodeled.iter().any(|u| u.contains("spool")),
            "{} spools and its `unmodeled:` never says what the sim assumes",
            w.id
        );
    }
    assert_eq!(with, published.len(), "a spool with no published numbers above");
    // A FORM SPOOLS ON ITS OWN. The Phenmor's base form is semi-auto and has
    // no held trigger; the Gorgons' Incarnon forms are Auto Charge and their
    // pages say plainly that they do not spool.
    for id in ["phenmor", "gorgon_incarnon", "soma_incarnon", "prisma_gorgon_incarnon"] {
        assert!(spec(id).unwrap().attack.sustained_fire_rate.is_none(), "{id}");
    }
}
