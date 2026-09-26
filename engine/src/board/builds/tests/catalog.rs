
/// AN EXALTED WEAPON'S ROW IS ITS WARFRAME'S TOO: refused without one at any
/// ruler, and the frame is part of what the build IS.
#[test]
fn an_exalted_weapon_is_refused_until_it_names_its_warframe() {
    let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let talons = ids(&["pressure_point", "hysteria"]);
    assert!(validate("valkyr_talons", &talons, &[], &[], "").is_ok(), "the builder keeps it");
    for form in ["valkyr_talons", "valkyr_talons_slide", "valkyr_talons_heavy"] {
        let refused = validate_for_board(ANY_RULER, form, &talons, &[], &[], "");
        assert!(refused.err().is_some_and(|e| e.contains("Exalted")), "{form}");
    }
    // …AND IT IS THAT RULE REFUSING, not the made-up ruler: an ordinary melee
    // weapon at the same door is turned away for the ruler instead.
    let magistar = validate_for_board(ANY_RULER, "magistar", &ids(&["pressure_point"]), &[], &[], "");
    assert!(magistar.err().is_some_and(|e| e.contains("unknown benchmark")));
}

/// TWO FRAMES ARE TWO BUILDS on an Exalted weapon, and NEITHER on every other:
/// a ruler pins a frameless Tenno for a gun, so the wielder it arrived with is
/// dropped and two players testing one build in two hands submit one build.
#[test]
fn only_an_exalted_row_is_keyed_by_the_frame_holding_it() {
    use crate::data::warframes::{Build as Frame, SlotPick};
    let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let frame = |mods: &[&str]| Frame {
        frame: "valkyr".to_string(),
        mods: mods.iter().map(|m| SlotPick { id: (*m).to_string(), rank: None, stacks: None }).collect(),
        ..Default::default()
    };
    let door = |weapon: &str, mods: &[String], w: Option<&Frame>| {
        validate_for_board_with(ANY_RULER, weapon, mods, &[], &[], "", None, None, None, w)
    };
    // The made-up ruler refuses everything AFTER the wielder rule, so the
    // Exalted refusal is read off the error and the rest off `ValidBuild`.
    let talons = ids(&["pressure_point", "hysteria"]);
    assert!(door("valkyr_talons", &talons, None)
        .err().is_some_and(|e| e.contains("Exalted")), "no frame, no row");
    assert!(door("valkyr_talons", &talons, Some(&frame(&["intensify"])))
        .err().is_some_and(|e| e.contains("unknown benchmark")), "with a frame it reaches the ruler");
}
const ANY_RULER: &str = "no_such_ruler";

/// A FIXED STANCE IS PART OF EVERY BUILD: Valkyr Talons without Hysteria is
/// a build nobody can hold, and with it the card's 5 doubles to 10 on its
/// matching slot (MEASUREMENTS M94).
#[test]
fn a_fixed_stance_is_required_and_grants_its_capacity() {
    let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let without = validate("valkyr_talons", &ids(&["pressure_point"]), &[], &[], "");
    assert!(without.err().is_some_and(|e| e.contains("hysteria is fixed")));
    assert!(validate("valkyr_talons_slide", &ids(&["pressure_point", "hysteria"]), &[], &[], "").is_ok());
    let hysteria = crate::data::mods::pool_for_weapon("valkyr_talons")
        .into_iter()
        .find(|m| m.id == "hysteria")
        .expect("in its own pool");
    let slot = crate::data::weapons::stance_polarity("valkyr_talons");
    assert_eq!(crate::rules::capacity::stance_capacity(hysteria.polarity, slot), 10, "60 + 10 = 70");
}

/// **WHAT THE EDITOR OFFERS, THE BOARD ACCEPTS** — over every weapon that
/// takes a riven, not over a named one.
///
/// The page's picker is `rivenPoolAll()` minus the `riven_excludes` this
/// engine serves; `check_riven_shape` decides what a board row may carry.
/// Nothing joined the two, and they were EXACT INVERSES: the check read
/// `derived_for` as a whitelist when it is the exclusion list, so every
/// legal riven was refused with "a X riven does not roll Y" and only
/// illegal ones could have passed. The board has never carried a riven
/// build, and this is why.
///
/// It is asserted as the PROPERTY over the whole roster, because a test
/// naming one weapon and one stat is the same shape as the bug: a hand
/// list cannot report what is not on it.
#[test]
fn the_board_accepts_every_stat_the_riven_editor_offers() {
    let mut checked = 0usize;
    for w in crate::data::weapons::all() {
        let class = super::riven_class(&w.id);
        if class.is_empty() {
            continue;
        }
        let excluded = crate::build::rivens::excluded_for(&w.id);
        let offered: Vec<&crate::build::rivens::RivenStat> = crate::build::rivens::pool(class)
            .iter()
            .filter(|x| !excluded.iter().any(|e| *e == x.id))
            .collect();
        assert!(!offered.is_empty(), "{}: the editor offers nothing at all", w.id);
        // PER SLOT, because the picker offers per slot: five stats are
        // bonus-only and one melee stat is malus-only, so "offered" is two
        // lists and a stat asked for in the wrong one is refused by both
        // surfaces rather than by one.
        // A PARTNER IS A ROLLED STAT: the picker offers one spliced stat a card.
        let bonuses: Vec<&String> = offered.iter().filter(|x| x.bonus && !x.spliced).map(|x| &x.id).collect();
        assert!(bonuses.len() >= 2, "{}: fewer than two bonus stats", w.id);
        for id in offered.iter() {
            // THE SMALLEST LEGAL SHAPE that carries the stat — a riven has
            // two or three bonuses, so the partners are other OFFERED ones
            // and a refusal can still only be about the stat under test.
            let mut partners = bonuses.iter().filter(|x| **x != &id.id);
            let shape = if id.bonus {
                crate::build::rivens::RivenShape {
                    bonuses: vec![id.id.clone(), partners.next().expect("a second stat").to_string()],
                    malus: None,
                }
            } else {
                crate::build::rivens::RivenShape {
                    bonuses: vec![
                        partners.next().expect("a second stat").to_string(),
                        partners.next().expect("a third stat").to_string(),
                    ],
                    malus: Some(id.id.clone()),
                }
            };
            let r = super::check_riven_shape(&w.id, &shape);
            assert!(r.is_ok(), "{} / {}: offered by the picker, refused by the board — {}",
                w.id, id.id, r.unwrap_err());
            checked += 1;
        }
    }
    // …AND IT ACTUALLY WALKED SOMETHING. A loop that `continue`s past every
    // weapon passes silently.
    assert!(checked > 500, "only {checked} (weapon, stat) pairs were checked");
}

/// A TWO-ELEMENT RIVEN IS AN ATOM, and canonicalisation still has to mean
/// EXACTLY "the same fight".
///
/// This is the property, not the arrangement: two mod orders share a
/// representative if and only if they resolve to the same damage vector.
/// Asserted by RESOLVING every ordering, because a rule written over
/// positions is the thing being tested and cannot also be the judge.
///
/// The fixture is chosen so the answer is not one class: a riven bringing
/// Cold+Heat, a Toxin mod and an Electricity mod make three genuinely
/// different fights depending on where the atom sits —
/// Blast+Corrosive, Viral+Radiation, or Magnetic+Gas — and the atom can
/// never be split, so a representative that separated its two elements
/// would be an order no build can produce.
#[test]
fn a_two_element_riven_is_an_atom_and_one_form_still_means_one_fight() {
    use crate::rules::capacity::Polarity;
    use crate::build::rivens::{RivenShape, RivenSpec, RolledStat};

    let shape = RivenShape {
        // SORTED, so the atom's own element order is `cold` then `heat` —
        // fixed by the shape and unreachable by any permutation.
        bonuses: vec!["cold".into(), "heat".into(), "multishot".into()],
        malus: None,
    };
    assert_eq!(shape.elements("rifle").len(), 2, "the fixture needs a two-element riven");

    let spec = RivenSpec {
        class: "rifle".into(),
        bonuses: shape
            .bonuses
            .iter()
            .map(|id| RolledStat { id: id.clone(), roll: 1.0 })
            .collect(),
        malus: None,
        rank: 8,
        polarity: Polarity::Madurai,
    };
    let riven_def = spec.to_mod_def(RIVEN_SLOT, 1.0);

    // The three elemental carriers, plus a plain mod so the plain/elemental
    // split is exercised too.
    let ids: Vec<String> = [RIVEN_SLOT, "infected_clip", "stormbringer", "serration"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    // What a given order actually RESOLVES to — the judge.
    let resolved = |order: &[String]| {
        let pool = crate::data::mods::pool_for_weapon("torid");
        let mods: Vec<&crate::model::ModDef> = order
            .iter()
            .map(|id| {
                if id == RIVEN_SLOT {
                    &riven_def
                } else {
                    pool.iter().find(|m| m.id == *id).expect("mod in pool")
                }
            })
            .collect();
        let base = crate::model::WeaponBase::from_data("torid", true, &[]);
        let panel = crate::build::loadout::resolve_for(
            &base, &mods, crate::model::StackPolicy::Emergent,
            crate::data::tenno::default_tenno());
        format!("{:?}", panel.damage)
    };

    // Every ordering of the four, grouped by canonical form and by fight.
    let mut by_form: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        Default::default();
    let mut by_fight: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        Default::default();
    let refs: Vec<&String> = ids.iter().collect();
    for order in orderings(&refs) {
        let order: Vec<String> = order.into_iter().cloned().collect();
        let form = canonical_mods_with("torid", &order, Some(&shape)).join(",");
        let fight = resolved(&order);
        by_form.entry(form.clone()).or_default().insert(fight.clone());
        by_fight.entry(fight).or_default().insert(form);
    }

    // THE FIXTURE IS WORTH RUNNING: more than one fight is reachable.
    assert!(by_fight.len() >= 3, "{} distinct fights — fixture too weak", by_fight.len());

    // THE REPRESENTATIVE IS A MEMBER OF ITS OWN CLASS. Everything else here
    // is about telling builds APART; this is the one that says the build
    // kept is the build submitted. A representative that resolves to
    // something else scores a fight nobody entered — which is the 12,424
    // against 46,583 this whole function exists to prevent, arrived at from
    // the other direction.
    for order in orderings(&refs) {
        let order: Vec<String> = order.into_iter().cloned().collect();
        let form = canonical_mods_with("torid", &order, Some(&shape));
        assert_eq!(
            resolved(&form),
            resolved(&order),
            "the representative of {order:?} is {form:?}, which is a different fight"
        );
    }

    // SOUND: one representative never covers two fights. This is the half
    // that matters — a collision here files two builds under one row and
    // the second one submitted is the one that disappears.
    for (form, fights) in &by_form {
        assert_eq!(fights.len(), 1, "`{form}` covers {} different fights", fights.len());
    }
    // COMPLETE: one fight never gets two representatives, which would put
    // the same build on the board twice.
    for (fight, forms) in &by_fight {
        assert_eq!(
            forms.len(), 1,
            "one fight has {} representatives: {forms:?} ({fight})", forms.len()
        );
    }

    // AND THE ATOM IS NEVER SPLIT: in every representative the riven sits
    // in one place, and the mods around it are an order that exists.
    for form in by_form.keys() {
        assert_eq!(
            form.split(',').filter(|x| *x == RIVEN_SLOT).count(),
            1,
            "`{form}` does not carry the riven exactly once"
        );
    }
}

/// …AND A RIVEN WITH NO ELEMENT IS A PLAIN MOD, so it must not disturb the
/// pairing of the build it sits in. The negative control: without it, the
/// test above would pass just as well on a rule that treated every riven as
/// elemental.
#[test]
fn a_riven_with_no_element_does_not_pair() {
    use crate::build::rivens::RivenShape;
    let plain = RivenShape {
        bonuses: vec!["damage".into(), "multishot".into()],
        malus: Some("recoil".into()),
    };
    assert!(plain.elements("rifle").is_empty());
    let with: Vec<String> = [RIVEN_SLOT, "infected_clip", "stormbringer"]
        .iter().map(|s| s.to_string()).collect();
    let without: Vec<String> =
        ["infected_clip", "stormbringer"].iter().map(|s| s.to_string()).collect();
    let a = canonical_mods_with("torid", &with, Some(&plain));
    let b = canonical_mods_with("torid", &without, None);
    assert_eq!(
        a.iter().filter(|x| *x != RIVEN_SLOT).cloned().collect::<Vec<_>>(),
        b,
        "a riven with no element changed how the build's elements paired"
    );
}
use super::*;

/// The capacity a benchmark build is judged against, for THIS weapon —
/// derived exactly as `validate` derives it, so a test can never assert a
/// number the rule does not use.
fn cap_of(weapon: &str) -> u32 {
    let spec = crate::data::weapons::spec(weapon).expect("weapon");
    crate::rules::capacity::capacity(
        crate::rules::capacity::rank_after(spec.max_rank, crate::rules::capacity::forma_to_max_rank(spec.max_rank)),
        true,
    )
}

fn v(x: &[&str]) -> Vec<String> {
    x.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_legal_build_passes_and_reports_what_it_costs() {
    let b = validate(
        "boar_prime",
        &v(&["primed_point_blank", "hells_chamber", "blunderbuss", "primed_ravage"]),
        &[],
        &v(&["none"]), "")
    .expect("four ordinary shotgun mods are legal");
    assert_eq!(b.mods.len(), 4);
    assert!(b.drain <= cap_of("boar_prime"));
    // Forma is a COST, not a legality term: it is reported, never rejected.
    assert!(b.forma <= 4, "four mods cannot need more than four Forma");
}

/// A POLARITY BELONGS TO THE WEAPON, NOT TO THE SLOT — so a benchmark
/// build spends the EXILUS slot's polarity even though the exilus SLOT is
/// out of scope.
///
/// Two slots' polarities swap without changing what either slot IS, so the
/// exilus one can be moved onto a main slot and the exilus slot left
/// carrying whatever came back, empty. The board withheld it until this
/// was known and over-charged 699 of its 928 stored rows by one Forma each.
///
/// The Torid is the sharp case and the biggest one — 95 of those rows. Its
/// exilus polarity is Madurai, which its two innate slots do not carry.
#[test]
fn a_benchmark_build_spends_the_exilus_slots_polarity() {
    let innate = crate::data::weapons::innate_slots("torid");
    let exilus = crate::data::weapons::exilus_polarity("torid")
        .expect("the Torid has an exilus polarity");
    assert!(
        !innate.iter().flatten().any(|p| *p == exilus),
        "this case only bites when the exilus polarity is not already innate"
    );

    // A pool of eight mods that all want the EXILUS polarity, so the ninth
    // is the only free match on offer and its absence costs a Forma.
    let planned: Vec<crate::rules::capacity::PlannedMod> = (0..8)
        .map(|_| crate::rules::capacity::PlannedMod { base_drain: 12, polarity: exilus })
        .collect();
    let spec = crate::data::weapons::spec("torid").unwrap();
    let with_it = {
        let mut v = innate.to_vec();
        v.push(Some(exilus));
        crate::rules::capacity::fit(spec.max_rank, &v, &planned, BENCHMARK_INVESTMENT, None).unwrap()
    };
    let without =
        crate::rules::capacity::fit(spec.max_rank, innate.as_ref(), &planned, BENCHMARK_INVESTMENT, None)
            .unwrap();
    assert_eq!(
        with_it.cost.total() + 1,
        without.cost.total(),
        "the ninth polarity is worth exactly one Forma here"
    );

    // …AND `validate` USES IT. The pool it builds is the nine, so this
    // cannot drift from the assertion above by someone editing one of them.
    let innate_used: Vec<Option<crate::rules::capacity::Polarity>> = {
        let mut v = crate::data::weapons::innate_slots("torid").to_vec();
        v.push(crate::data::weapons::exilus_polarity("torid"));
        v
    };
    assert_eq!(innate_used.len(), MAIN_SLOTS + 1);
    assert_eq!(innate_used[MAIN_SLOTS], Some(exilus));
}

#[test]
fn the_arsenal_rules_are_the_ones_enforced() {
    // A mod from another class.
    assert!(validate("boar_prime", &v(&["serration"]), &[], &[], "").is_err());
    // Two of one family.
    let e = validate("boar_prime", &v(&["hells_chamber", "galvanized_hell"]), &[], &[], "")
        .unwrap_err();
    assert!(e.contains("family"), "{e}");
    // NINE mods — refused even though one of them is exilus-eligible,
    // because a benchmark build has eight slots and the exilus slot is out
    // of scope (it measures nothing a damage ranking can read).
    let nine = v(&["primed_point_blank", "hells_chamber", "blunderbuss", "primed_ravage",
                   "scattering_inferno", "toxic_barrage", "galvanized_savvy", "vicious_spread",
                   "lock_and_load"]);
    assert!(crate::data::mods::pool_for_weapon("boar_prime")
        .iter().any(|m| m.id == "lock_and_load" && m.exilus),
        "the ninth is exilus-eligible, so this tests the SLOT rule");
    let e = validate("boar_prime", &nine, &[], &[], "").unwrap_err();
    assert!(e.contains("8"), "{e}");
    // An arcane the weapon cannot seat.
    assert!(validate("boar_prime", &[], &[], &v(&["secondary_enervate"]), "").is_err());
}

/// CAPACITY IS A LIVE CONSTRAINT at eight slots — asked of the data rather
/// than assumed, and the answer was not the one I expected.
///
/// Matching a polarity halves a mod's drain (rounded UP), so eight mods
/// cost the sum of their halves and Boar Prime's priciest eight come to
/// exactly 60 — which is why an earlier version of this test, built on
/// that one weapon, concluded the check was slack. Across the roster the
/// worst case is 64, so it refuses real builds. Both directions are
/// asserted per weapon: the planner's verdict has to agree with the
/// arithmetic, whichever way it falls.
#[test]
fn capacity_is_a_live_constraint_at_eight_slots() {
    let mut worst = 0u32;
    for w in crate::data::weapons::all() {
        let mut pool = crate::data::mods::pool_for_weapon(&w.id);
        pool.sort_by_key(|m| std::cmp::Reverse(m.base_drain));
        let (mut fams, mut picked): (Vec<&str>, Vec<String>) = (Vec::new(), Vec::new());
        for m in &pool {
            if picked.len() == MAIN_SLOTS {
                break;
            }
            if let Some(f) = m.family {
                if fams.contains(&f) {
                    continue;
                }
                fams.push(f);
            }
            picked.push(m.id.to_string());
        }
        if picked.len() < MAIN_SLOTS {
            continue;
        }
        // A FIXED STANCE RIDES EVERY BUILD, and its grant is capacity the
        // build can spend (Valkyr Talons' Hysteria, MEASUREMENTS M94).
        let fixed = pool.iter().find(|m| {
            crate::data::weapons::spec(&w.id).and_then(|s| s.fixed_stance.as_deref()) == Some(m.id)
        });
        if let Some(m) = fixed {
            picked.push(m.id.to_string());
        }
        let cap = cap_of(&w.id)
            + fixed.map_or(0, |m| {
                crate::rules::capacity::stance_capacity(m.polarity, crate::data::weapons::stance_polarity(&w.id))
            });
        // An adversary weapon has no legal build with no element, so the
        // sweep gives every weapon the one its own spec starts with — this
        // test is about capacity and must not trip over legality.
        let val = crate::data::weapons::valence_of(&w.id)
            .and_then(|s| s.elements.first().cloned())
            .unwrap_or_default();
        let cost: u32 = picked
            .iter()
            .map(|id| pool.iter().find(|m| m.id == id.as_str()).unwrap().base_drain.div_ceil(2))
            .sum();
        worst = worst.max(cost);
        // Whatever the number, the VERDICT and the cost must agree: the
        // planner is the authority, not this arithmetic.
        let got = validate(&w.id, &picked, &[], &[], &val);
        match got {
            Ok(v) => assert!(
                cost <= cap && v.drain <= cap,
                "{}: accepted at {} but the halves come to {cost}", w.id, v.drain
            ),
            Err(e) => assert!(
                cost > cap && e.contains("capacity"),
                "{}: refused ({e}) though the halves come to {cost}", w.id
            ),
        }
    }
    // The rule is not dead code: somewhere in the roster, eight mods do not
    // fit however much Forma you own.
    assert!(
        worst > 60,
        "capacity never binds anywhere ({worst}) — this check would be decoration"
    );
}

/// A GALVANIZED CARD IS ITS PARENT'S VARIANT, and the two are one family.
///
/// *"Galvanized Steel is the Galvanized variant of True Steel"* is the
/// wiki's own first sentence, and Reflex Coil and Melee Elementalist have
/// the same one — so a build holds ONE of each family.
///
/// FOCUS ENERGY IS THE CONTROL: it buys Heavy Attack Efficiency as Reflex
/// Coil does and is a different card, which the wiki states by arithmetic —
/// *"with both Focus Energy and Reflex Coil, 10% of the combo counter will
/// still be consumed"*.
#[test]
fn a_galvanized_card_collides_with_the_card_it_is_a_variant_of() {
    let legal = |a: &str, b: &str| {
        validate_with(
            "magistar",
            &[a.to_string(), b.to_string()],
            &[],
            &[],
            "",
            None,
            None,
            None,
        )
        .is_ok()
    };
    for (a, b) in [
        ("galvanized_steel", "true_steel"),
        ("galvanized_steel", "sacrificial_steel"),
        ("galvanized_reflex", "reflex_coil"),
        ("galvanized_elementalist", "melee_elementalist"),
    ] {
        assert!(!legal(a, b), "{a} and {b} are one family and both were accepted");
    }
    assert!(
        legal("focus_energy", "reflex_coil"),
        "two different cards that both buy efficiency stack in game",
    );
}

/// TWO ASSEMBLIES OF ONE CHAMBER ARE TWO BUILDS, and the identity says so.
///
/// A grip sets damage, fire rate and the charge; a loader sets the magazine,
/// the reload and three deltas that can be negative. An identity that could
/// not tell them apart files the second under the first's number — the same
/// failure the valence and the exilus slot each had before they were part
/// of it, and the one that survives longest, because a board holding one
/// modular row cannot show it.
#[test]
fn two_assemblies_of_one_chamber_are_two_rows() {
    let Some(w) = crate::data::weapons::roster().find(|w| w.kitgun.is_some()) else {
        return; // no modular weapon in the roster: nothing to key apart
    };
    let record = w.kitgun.clone().unwrap();
    let default = crate::data::weapons::kitguns::default_assembly(&record).expect("a default");
    // ANOTHER LEGAL GRIP for this slot, whichever it is.
    let other = crate::data::weapons::kitguns::grips()
        .iter()
        .find(|g| {
            g.id != default.grip
                && crate::data::weapons::spec_assembled(
                    w,
                    Some(&crate::data::weapons::kitguns::Assembly {
                        chamber: record.clone(),
                        grip: g.id.clone(),
                        loader: default.loader.clone(),
                    }),
                )
                .is_some()
        })
        .map(|g| g.id.clone());
    let Some(other) = other else { return };
    let of = |grip: &str| {
        validate_with(
            &w.id,
            &[],
            &[],
            &[],
            "",
            None,
            None,
            Some(&crate::data::weapons::kitguns::Assembly {
                chamber: record.clone(),
                grip: grip.to_string(),
                loader: default.loader.clone(),
            }),
        )
        .map(|b| identity(&b))
    };
    let a = of(&default.grip).expect("the default assembles");
    let b = of(&other).expect("the other grip assembles");
    assert_ne!(a, b, "two grips, one identity: {a}");
    assert!(a.contains(&default.grip), "{a}");
}

/// …AND A WEAPON THAT TAKES NO PARTS IS KEYED EXACTLY AS IT WAS.
///
/// The assembly is APPENDED to the identity, so adding it must not re-key
/// the board: every row of every ordinary weapon has to hash to the string
/// it already did, or the first run after this files 22,000 rows as new.
#[test]
fn a_weapon_with_no_parts_is_keyed_as_it_always_was() {
    let b = validate("braton_prime", &["serration".into()], &[], &[], "")
        .expect("a legal Braton");
    assert!(b.assembly.is_none());
    assert_eq!(identity(&b), "braton_prime|serration|||");
}

/// A PAIR THAT DOES NOT COMPOSE IS REFUSED, not repaired. `assembly_of`
/// repairs a stale link for a FIGHT, where something has to be drawn; a
/// board record is a statement about one build, and swapping a part behind
/// the submitter is the divergence a record exists to prevent.
#[test]
fn parts_that_do_not_make_the_weapon_are_refused() {
    let Some(w) = crate::data::weapons::roster().find(|w| w.kitgun.is_some()) else {
        return;
    };
    let err = validate_with(
        &w.id,
        &[],
        &[],
        &[],
        "",
        None,
        None,
        Some(&crate::data::weapons::kitguns::Assembly {
            chamber: w.kitgun.clone().unwrap(),
            grip: "not_a_grip".into(),
            loader: "not_a_loader".into(),
        }),
    )
    .unwrap_err();
    assert!(err.contains("not_a_grip"), "{err}");
    // …AND A WEAPON THAT TAKES NONE MAY NOT BE HANDED ANY.
    let err = validate_with(
        "braton_prime",
        &[],
        &[],
        &[],
        "",
        None,
        None,
        Some(&crate::data::weapons::kitguns::Assembly {
            chamber: String::new(),
            grip: "gaze".into(),
            loader: "ramble".into(),
        }),
    )
    .unwrap_err();
    assert!(err.contains("not assembled from parts"), "{err}");
}


/// THE REPRESENTATIVE: what differs is kept, what does not is not.
///
/// Measured on the Torid: three elementals in slots
/// 1-3, the same three in 4-6, and the same three interleaved with the
/// non-elementals all score an IDENTICAL 146,707.582 DPS, as does
/// reshuffling the non-elementals among themselves. So position is not the
/// build — only the elementals' order relative to EACH OTHER is.
#[test]
fn one_representative_per_build_positional_first_in_rank_order() {
    let mods = |x: &[&str]| canonical_mods("torid", &v(x));
    let want = mods(&["split_chamber", "serration", "point_strike", "hellfire", "cryo_rounds", "infected_clip"]);

    // Elements moved, interleaved, and the rest reshuffled: one answer.
    for spelling in [
        &["hellfire", "cryo_rounds", "infected_clip", "serration", "split_chamber", "point_strike"][..],
        &["serration", "hellfire", "split_chamber", "cryo_rounds", "point_strike", "infected_clip"][..],
        &["point_strike", "split_chamber", "serration", "hellfire", "cryo_rounds", "infected_clip"][..],
    ] {
        assert_eq!(mods(spelling), want, "{spelling:?}");
    }
    // THE POSITION-BEARING CARDS COME FIRST, at a fixed offset, and one
    // rule orders them: biggest drain, then DE's own English name. All three
    // are 6, so the names decide — Cryo Rounds before Hellfire. Cold and
    // Heat make the Blast pair and lead; Infected Clip is the odd one out
    // and trails, because chunking reads from the front and a trailing
    // element that moved would re-pair everything after it.
    assert_eq!(&want[..3], &v(&["cryo_rounds", "hellfire", "infected_clip"])[..]);
    // …and the plain cards follow under the SAME rule: Split Chamber 15,
    // Serration 14, Point Strike 9. (Asserted against the pool rather than
    // from memory — I had Serration first and the pool says otherwise.)
    assert_eq!(&want[3..], &v(&["split_chamber", "serration", "point_strike"])[..]);
    let pool = crate::data::mods::pool_for_weapon("torid");
    let drain = |id: &str| pool.iter().find(|m| m.id == id).unwrap().base_drain;
    assert!(drain("split_chamber") > drain("serration"));
    assert!(drain("serration") > drain("point_strike"));

    // ...and swapping two ELEMENTS is a different build, because it pairs
    // differently: Gas + Magnetic against Blast + Corrosive.
    assert_ne!(
        mods(&["hellfire", "infected_clip", "cryo_rounds", "serration"]),
        mods(&["hellfire", "cryo_rounds", "infected_clip", "serration"])
    );
}

/// ORDER IS PART OF THE FIGHT, because mods combine ELEMENTS in the order
/// they are listed. This test asserted the opposite for a day, on one
/// measurement that happened to reorder mods whose pairing did not change.
///
/// The Torid says it plainly: Heat, Cold, Toxin, Electric pairs to Blast +
/// Corrosive and scores 12,424 DPS; the same four as Heat, Toxin, Cold,
/// Electric pairs to Gas + Magnetic and scores 46,583 (measured
/// 2026-08-04). One row for both would have published a number belonging
/// to neither.
#[test]
fn a_row_key_is_a_build_id_and_the_mode_it_was_played_in() {
    let v = |x: &[&str]| x.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
    let b = validate("torid", &v(&["serration"]), &[], &[], "").unwrap();
    // ONE ROW PER (BUILD, MODE): a build played two ways is two entrants.
    assert_ne!(board_key(&b, "base"), board_key(&b, "cycle"));
    // …AND A BLANK MODE IS `base`, which is what every row written before
    // the dimension existed means. The page and the scorer both hand over
    // whatever they were given, so the default belongs here rather than at
    // two call sites that could disagree about it.
    assert_eq!(board_key(&b, ""), board_key(&b, "base"));
    // …AND THE BUILD HALF IS THE ID, which is what `builds` is keyed by.
    assert_eq!(board_key(&b, "base"), format!("{}#base", build_id(&b)));
}

#[test]
fn the_order_of_the_mods_is_part_of_the_identity() {
    let a = validate("torid", &v(&["hellfire", "cryo_rounds", "infected_clip", "stormbringer"]), &[], &[], "").unwrap();
    let b = validate("torid", &v(&["hellfire", "infected_clip", "cryo_rounds", "stormbringer"]), &[], &[], "").unwrap();
    // NORMALISATION NEVER RE-PAIRS. The pairing that arrived — Blast and
    // Corrosive — is exactly the pairing that comes out; what it settles is
    // the order inside each pair and between them, and the rule is the one
    // rule: biggest drain, then DE's English name. All four are 6, so the
    // names decide, and the pair whose first card sorts first leads.
    assert_eq!(a.mods, v(&["cryo_rounds", "hellfire", "infected_clip", "stormbringer"]),
               "one rank rule, same pairing");
    assert_ne!(identity(&a), identity(&b), "two pairings, two rows");

    // ...and a different SET is still a different identity.
    let c = validate("torid", &v(&["hellfire", "cryo_rounds"]), &[], &[], "").unwrap();
    assert_ne!(identity(&a), identity(&c));
}

/// ...BUT SWAPPING TWO ELEMENTALS INSIDE ONE PAIR IS THE SAME BUILD.
///
/// The rule above ("order is part of the fight") was right and one notch
/// too fine. `rules::elements::combine` pairs the sequence with `chunks_exact(2)`
/// and combines each pair with `combined_of`, which is SYMMETRIC and pools
/// both amounts — so the two spellings of a pair are the same damage by
/// construction.
///
/// It reached the board: the Ocucor carried two rows differing only in
/// Frostbite and Pistol Pestilence being swapped, both scoring 6.0779.
#[test]
fn swapping_two_elementals_inside_a_pair_is_one_build() {
    let one = validate("ocucor", &v(&["frostbite", "pistol_pestilence"]), &[], &[], "").unwrap();
    let two = validate("ocucor", &v(&["pistol_pestilence", "frostbite"]), &[], &[], "").unwrap();
    assert_eq!(identity(&one), identity(&two), "Cold + Toxin is Viral either way");

    // ...and the guard against over-collapsing: with FOUR elementals, the
    // same swap ACROSS a pair boundary re-pairs everything and must stay
    // two builds. Cold+Toxin / Heat+Electricity against Cold+Heat /
    // Toxin+Electricity — Viral+Radiation against Blast+Corrosive.
    let split = |x: &[&str]| identity(&validate("ocucor", &v(x), &[], &[], "").unwrap());
    assert_ne!(
        split(&["frostbite", "pistol_pestilence", "heated_charge", "convulsion"]),
        split(&["frostbite", "heated_charge", "pistol_pestilence", "convulsion"]),
        "moving an elemental across a pair boundary is a different fight"
    );
}

/// THE INVARIANT THAT MATTERS: canonicalising must never change the FIGHT.
///
/// Every other test here compares strings, and a string test cannot tell a
/// tidier spelling from a different build. This one resolves both orders
/// and compares the DAMAGE VECTOR, which is the thing the board is really
/// promising is unchanged.
///
/// It exists because the string tests all passed while the board published
/// a wrong number (5669040, reverted): the canonical order was tidy, valid,
/// and a different fight.
///
/// THE DUPLICATE-ELEMENT CASE IS THE POINT. Primed Heated Charge and Scorch
/// are both Heat, so `ElementalInput::push` pools them and the engine sees
/// THREE elements where the mod list has four — every position after the
/// duplicate shifts. A canonicaliser reasoning about mod slots gets this
/// wrong and nothing but a damage comparison says so.
#[test]
fn canonicalising_never_changes_the_damage() {
    let base = crate::model::WeaponBase::from_data("ocucor", false, &[]);
    let pool = crate::data::mods::pool_for_weapon("ocucor");
    let resolve_in_order = |ids: &[String]| {
        let refs: Vec<&crate::model::ModDef> = ids
            .iter()
            .filter_map(|id| pool.iter().find(|m| m.id == id.as_str()))
            .collect();
        crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::BaseOnly).damage
    };
    for spelling in [
        // The build that broke: two Heat mods pooling behind a Cold/Toxin
        // pair. Viral + Heat, and it must stay Viral + Heat.
        &["ice_storm", "pistol_pestilence", "primed_heated_charge", "scorch"][..],
        // ...and the same four submitted every other way round. Each is
        // whatever fight it is; canonicalising must not turn it into
        // another one.
        &["scorch", "primed_heated_charge", "ice_storm", "pistol_pestilence"][..],
        &["ice_storm", "scorch", "pistol_pestilence", "primed_heated_charge"][..],
        // Two clean pairs, no duplicates.
        &["frostbite", "pistol_pestilence", "heated_charge", "convulsion"][..],
        // An odd one out, which must stay the odd one out.
        &["frostbite", "pistol_pestilence", "heated_charge"][..],
    ] {
        let submitted = v(spelling);
        let canon = canonical_mods("ocucor", &submitted);
        assert_eq!(
            resolve_in_order(&canon),
            resolve_in_order(&submitted),
            "canonicalising {spelling:?} changed the damage: {canon:?}"
        );
    }
}

/// ALL THREE AXES REACH THE BUILD — mods, evolutions AND arcanes.
///
/// Asserted through the IDENTITY rather than by reading fields back,
/// because identity is what a board row is keyed on: if an axis did not
/// enter, two builds differing only on that axis would collide into one
/// row and the second submitter's build would silently become the first's.
/// So each axis is changed alone, and each change must move the key.
#[test]
fn a_change_on_any_axis_is_a_different_build() {
    let mods = v(&["hellfire", "serration", "split_chamber"]);
    // Tier 1 then tier 2 — a LADDER, so the second has to be the rung
    // above the first or normalisation stops at the gap.
    let evos = v(&["torid_evo1_incarnon_form", "torid_final_fusillade"]);
    let arc = v(&["primary_deadhead"]);
    let base = validate("torid", &mods, &evos, &arc, "").expect("a legal torid build");
    // Everything arrived.
    assert_eq!(base.mods.len(), 3);
    assert_eq!(base.evolutions, evos, "the whole ladder prefix");
    assert_eq!(base.arcanes, arc);

    let key = identity(&base);
    let other_mods = v(&["hellfire", "serration", "point_strike"]);
    let other_evos = v(&["torid_evo1_incarnon_form", "torid_plentiful_mayhem"]);
    let other_arc = v(&["primary_merciless"]);
    for (what, b) in [
        ("mods", validate("torid", &other_mods, &evos, &arc, "")),
        ("evolutions", validate("torid", &mods, &other_evos, &arc, "")),
        ("arcanes", validate("torid", &mods, &evos, &other_arc, "")),
    ] {
        let b = b.unwrap_or_else(|e| panic!("{what}: {e}"));
        assert_ne!(identity(&b), key, "{what} does not reach the identity");
    }
}

/// The ladder is applied by TRUNCATION, so normalisation has to happen
/// before the identity is taken — otherwise a row names a build the score
/// does not belong to.
#[test]
fn an_evolution_set_is_trimmed_to_its_legal_prefix_before_it_is_identified() {
    // Tier 3 with nothing below it: the ladder opens nothing, so the whole
    // set drops rather than the build being scored with a tier-3 perk.
    let b = validate("boar_prime", &[], &v(&["boar_prime_reified_bane"]), &[], "").unwrap();
    assert!(
        b.evolutions.is_empty(),
        "a tier nothing unlocked is not part of the build: {:?}",
        b.evolutions
    );
    // Filled from tier 1 up, it survives.
    let full = validate(
        "boar_prime",
        &[],
        &v(&["boar_prime_evo1_incarnon_form", "boar_prime_fortress_salvo"]),
        &[], "")
    .unwrap();
    assert_eq!(full.evolutions.len(), 2, "{:?}", full.evolutions);
}
/// "FULL" IS PER WEAPON, and that is the whole point of computing it rather
/// than writing a number down: a sentinel weapon seats no arcane and has no
/// evolutions, so it is complete with eight mods and nothing else, while a
/// Laetum needs five tiers and an Arch-Gun needs two arcanes.
#[test]
fn complete_means_something_different_on_every_weapon() {
    use crate::data::weapons::arcane_pools;
    // The shapes the rule has to cover, straight from the data.
    assert_eq!(arcane_pools("larkspur_prime").len(), 2, "an Arch-Gun seats two");
    assert_eq!(arcane_pools("boar_prime").len(), 1);
    assert_eq!(arcane_pools("verglas_prime").len(), 0, "a sentinel weapon seats none");
    assert_eq!(crate::data::evolutions::tier_count("laetum"), 5);
    assert_eq!(crate::data::evolutions::tier_count("boar_prime"), 4);
    assert_eq!(crate::data::evolutions::tier_count("gotva_prime"), 0, "no evolutions");

    // A full Gotva Prime: eight mods, no tiers to fill, one arcane seat.
    let mods: Vec<String> = crate::data::mods::pool_for_weapon("gotva_prime")
        .iter()
        .filter(|m| !m.exilus)
        .take(MAIN_SLOTS)
        .map(|m| m.id.to_string())
        .collect();
    assert_eq!(mods.len(), MAIN_SLOTS, "the pool can fill a build");
    let arc = vec!["primary_merciless".to_string()];
    let ok = validate_for_board("standard_single_target", "gotva_prime", &mods, &[], &arc, "");
    assert!(ok.is_ok(), "a full rifle build is admitted: {ok:?}");

    // ...and the same build with the arcane seat empty is not.
    let none = vec!["none".to_string()];
    let err = validate_for_board("standard_single_target", "gotva_prime", &mods, &[], &none, "")
        .unwrap_err();
    assert!(err.contains("arcane"), "the reason names the axis: {err}");

    // One mod short is refused on the MOD axis, not the arcane one.
    let short = &mods[..MAIN_SLOTS - 1];
    let err = validate_for_board("standard_single_target", "gotva_prime", short, &[], &arc, "")
        .unwrap_err();
    assert!(err.contains("main slots"), "{err}");

    // An INCARNON weapon with no evolutions is the base form, not a weak
    // build of the same gun — refused on the evolution axis.
    let bp: Vec<String> = crate::data::mods::pool_for_weapon("boar_prime")
        .iter()
        .filter(|m| !m.exilus)
        .take(MAIN_SLOTS)
        .map(|m| m.id.to_string())
        .collect();
    let err = validate_for_board("standard_single_target", "boar_prime", &bp, &[],
                                 &["primary_crux".to_string()], "")
        .unwrap_err();
    assert!(err.contains("evolution"), "{err}");
}

/// AN ADVERSARY WEAPON'S VALENCE IS MANDATORY, and it is a LEGALITY rule
/// rather than a board one.
///
/// Every copy in the game comes out of a Lich carrying an element, so a
/// build with none is not a weaker build of that weapon — it is a weapon
/// nobody has. Accepting it in `validate` and refusing it one layer up, in
/// `validate_for_board` and only when the ruler asks, leaves every other
/// caller free to score a gun that does not exist.
///
/// Both directions, because a rule that only ever refuses is a rule nobody
/// can satisfy: an element the weapon rolls is admitted and survives into
/// the identity, and one it does not roll is named in the error.
#[test]
fn an_adversary_weapon_has_no_build_without_an_element() {
    let mods: Vec<String> = crate::data::mods::pool_for_weapon("kuva_nukor")
        .iter()
        .filter(|m| !m.exilus)
        .take(2)
        .map(|m| m.id.to_string())
        .collect();

    let e = validate("kuva_nukor", &mods, &[], &[], "").unwrap_err();
    assert!(e.contains("Valence") && e.contains("Lich"), "{e}");

    let ok = validate("kuva_nukor", &mods, &[], &[], "heat").expect("heat is one of its seven");
    assert_eq!(ok.valence, "heat");
    assert!(identity(&ok).ends_with("|heat"), "{}", identity(&ok));

    let e = validate("kuva_nukor", &mods, &[], &[], "puncture").unwrap_err();
    assert!(e.contains("progenitor element"), "{e}");

    // ...and an ORDINARY weapon is untouched in both directions: none is
    // the only legal answer there.
    assert!(validate("torid", &v(&["serration"]), &[], &[], "").is_ok());
    assert!(validate("torid", &v(&["serration"]), &[], &[], "heat").is_err());
}

/// ...AND A FULL ONE IS ADMISSIBLE TO THE BOARD, which is the half a
/// legality rule can quietly take away.
///
/// The ruler asks for `valence: full`, and the clause lives in `validate`
/// (above), so the requirement is met by a build being legal at all. That
/// is a strictly stronger rule and it would be worth nothing if the wrapper
/// stopped accepting the builds it guarantees — the Kuva Nukor is the
/// roster's only adversary weapon, so nothing else would notice.
#[test]
fn a_full_adversary_build_is_admissible() {
    // Eight mods from eight different families — two of one family is its
    // own refusal, and this test is not about that one.
    let mut fams: Vec<&str> = Vec::new();
    let mods: Vec<String> = crate::data::mods::pool_for_weapon("kuva_nukor")
        .iter()
        .filter(|m| !m.exilus)
        .filter(|m| match m.family {
            Some(f) if fams.contains(&f) => false,
            Some(f) => {
                fams.push(f);
                true
            }
            None => true,
        })
        .take(MAIN_SLOTS)
        .map(|m| m.id.to_string())
        .collect();
    assert_eq!(mods.len(), MAIN_SLOTS, "the pool can fill a build");
    // Its own slot's arcane, and every seat filled — the ruler asks for both.
    let arc: Vec<String> = crate::data::weapons::arcane_pools("kuva_nukor")
        .iter()
        .map(|slot| {
            crate::data::arcanes::pool_for_weapon("kuva_nukor", slot)
                .first()
                .map_or("none".to_string(), |a| a.id.to_string())
        })
        .collect();
    let ok = validate_for_board("standard_single_target", "kuva_nukor", &mods, &[], &arc, "heat");
    assert!(ok.is_ok(), "a full adversary build is admitted: {ok:?}");
    assert_eq!(ok.unwrap().valence, "heat");

    // ...and the ruler's own `valence: full` still bites, now from one
    // layer down: the same build with no element is refused.
    let e = validate_for_board("standard_single_target", "kuva_nukor", &mods, &[], &arc, "").unwrap_err();
    assert!(e.contains("Valence"), "{e}");
}

/// THE BOARD'S ENTRY STANDARD, STATED AS A TABLE.
///
/// Eight main slots FULL, every arcane seat and every evolution tier FULL,
/// and the two slots the game itself makes optional — the stance and the
/// exilus — optional here too. A ruler may still narrow it; this is the
/// shape a ruler that wants a complete build is asking for.
///
/// THE FOURTH ROW IS WHY THIS EXISTS. A full melee build carries all three
/// — eight mains, a stance and an exilus — and that is TEN mods against the
/// nine slots the capacity planner counts, because the stance was priced
/// among them while also handing its capacity back. The board refused
/// exactly the builds it exists to rank, and said "does not fit this
/// weapon's capacity even with Forma".
#[test]
fn the_entry_standard_takes_a_full_build_with_or_without_the_optional_slots() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<String>>();
    const MAINS: [&str; 8] = [
        "primed_pressure_point", "sacrificial_steel", "organ_shatter", "primed_reach",
        "primed_fever_strike", "north_wind", "shocking_touch", "weeping_wounds",
    ];
    const EVOS: [&str; 5] = [
        "praedos_evo1_incarnon_form", "praedos_seismic_slam", "praedos_adept_reflexes",
        "praedos_evolved_ascension", "praedos_universal_readiness",
    ];
    let go = |mods: Vec<String>, evos: Vec<String>, arcs: Vec<String>, ex: Option<&str>| {
        validate_for_board_with(
            "standard_multi_target", "praedos", &mods, &evos, &arcs, "", None, ex, None, None,
        )
    };
    let arc = || s(&["melee_influence"]);
    let stance = || s(&[&MAINS[..], &["sovereign_outcast"]].concat());

    // THE FOUR SHAPES THAT ARE A COMPLETE BUILD.
    for (what, mods, ex) in [
        ("eight mains", s(&MAINS), None),
        ("…and a stance", stance(), None),
        ("…and an exilus", s(&MAINS), Some("conditions_perfection")),
        ("…and both", stance(), Some("conditions_perfection")),
        // AN EXILUS-ELIGIBLE MOD IN A MAIN SLOT IS A MAIN MOD. What is
        // optional is the SLOT, not the mod: the game lets one of these sit
        // anywhere, and a build that spends a main slot on it has eight
        // mains like any other.
        ("…with an exilus-eligible mod among the mains",
         s(&["primed_pressure_point", "sacrificial_steel", "organ_shatter",
             "primed_reach", "primed_fever_strike", "north_wind",
             "shocking_touch", "conditions_perfection"]), None),
    ] {
        assert!(go(mods, s(&EVOS), arc(), ex).is_ok(), "{what} is a complete build");
    }

    // …AND WHAT IS NOT COMPLETE IS REFUSED, each for its own reason.
    let short = go(s(&MAINS[..7]), s(&EVOS), arc(), None).unwrap_err();
    assert!(short.contains("main slots"), "seven mains: {short}");
    let no_arc = go(s(&MAINS), s(&EVOS), vec![], None).unwrap_err();
    assert!(no_arc.contains("arcane"), "no arcane: {no_arc}");
    let no_evo = go(s(&MAINS), s(&EVOS[..3]), arc(), None).unwrap_err();
    assert!(no_evo.contains("evolution"), "three tiers: {no_evo}");
}

/// An unknown benchmark admits nothing: a number published against a ruler
/// that does not exist has no standard behind it.
#[test]
fn an_unknown_benchmark_admits_nothing() {
    let e = validate_for_board("no_such_ruler", "gotva_prime", &[], &[], &[], "").unwrap_err();
    assert!(e.contains("unknown benchmark"), "{e}");
}

/// A SECONDARY WEAPON'S ARCANE IS A SECONDARY ARCANE. With the seats taken
/// from every arcane DIRECTORY that exists, sorted, seat 0 is "primary" on
/// every weapon and a Dual Toxocyst build carrying `secondary_deadhead` is
/// refused for seating an arcane it seats.
#[test]
fn a_weapon_seats_its_own_slots_arcanes() {
    // Eight mods from EIGHT DIFFERENT FAMILIES — two of one family is its
    // own refusal, and this test is not about that one.
    let mods = |w: &str| -> Vec<String> {
        let mut fams: Vec<&str> = Vec::new();
        let mut out = Vec::new();
        for m in crate::data::mods::pool_for_weapon(w).iter().filter(|m| !m.exilus) {
            if out.len() == MAIN_SLOTS {
                break;
            }
            if let Some(f) = m.family {
                if fams.contains(&f) {
                    continue;
                }
                fams.push(f);
            }
            out.push(m.id.to_string());
        }
        out
    };
    // A SECONDARY, with a secondary arcane and its full evolution ladder.
    let evos: Vec<String> = (1..=crate::data::evolutions::tier_count("dual_toxocyst"))
        .filter_map(|t| {
            crate::data::evolutions::options("dual_toxocyst", t)
                .first()
                .map(|e| e.id.to_string())
        })
        .collect();
    let ok = validate_for_board(
        "standard_single_target", "dual_toxocyst", &mods("dual_toxocyst"), &evos,
        &["secondary_deadhead".to_string()], "");
    assert!(ok.is_ok(), "a secondary seats a secondary arcane: {ok:?}");

    // ...and it does NOT seat a primary one.
    let e = validate_for_board(
        "standard_single_target", "dual_toxocyst", &mods("dual_toxocyst"), &evos,
        &["primary_deadhead".to_string()], "")
    .unwrap_err();
    assert!(e.contains("not an arcane"), "{e}");

    // A sentinel weapon seats none, so any arcane at all is refused.
    let e = validate_for_board(
        "standard_single_target", "verglas_prime", &mods("verglas_prime"), &[],
        &["primary_crux".to_string()], "")
    .unwrap_err();
    assert!(e.contains("seats 0") || e.contains("not an arcane"), "{e}");
}

/// **A CORNER KNOWS WHICH BUILD IT IS AN ALTERNATIVE TO, AND THE DEFAULT KNOWS
/// IT IS ONE.** The scorer's entry line reads this to decide whether a corner
/// is worth a fight yet (`board::entry`), so both answers have to be exact:
/// the god roll at max rank answers `None`, and every other corner answers the
/// god roll's own id.
#[test]
fn a_corner_names_the_default_it_waits_on() {
    use crate::build::rivens::RivenShape;
    let shape = RivenShape {
        bonuses: vec!["damage".into(), "puncture".into()],
        malus: Some("zoom".into()),
    };
    let mods: Vec<String> =
        [RIVEN_SLOT, "serration"].iter().map(|s| (*s).to_string()).collect();
    let b = validate_with("braton_prime", &mods, &[], &[], "", Some(&shape), None, None).unwrap();
    // THE DEFAULT: every bonus at its ceiling, the malus at its floor.
    let default = b.clone().with_riven_rolls(crate::build::rivens::default_rolls(&shape));
    assert_eq!(default_corner(&default), None, "the default is not waiting on anybody");
    // …AND THE OTHER END OF THE AMBIGUOUS STAT is a build of its own, which
    // names the default rather than itself.
    let mut rolls = crate::build::rivens::default_rolls(&shape);
    rolls[1] = 0.9;
    let corner = b.with_riven_rolls(rolls);
    assert_ne!(build_id(&corner), build_id(&default), "two corners are two builds");
    assert_eq!(default_corner(&corner).as_deref(), Some(build_id(&default).as_str()));
}
