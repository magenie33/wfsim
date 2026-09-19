use super::*;

/// THE TENTH CHILL STACK **IS** THE FROZEN TRIGGER, and both are live.
///
/// The model this replaced had the tenth proc CONSUME the nine stacks, so
/// the ladder read zero for the three seconds the game displays it at ten.
/// Cold is ONE status whose stacks are the ladder; Frozen is a STATE the
/// tenth of them trips, and the wiki describes it the
/// same way — "Maximum stacks: 10 total", of which "the 10th stack" is the
/// Frozen state.
#[test]
fn the_tenth_chill_stack_freezes_and_stays() {
    let nine = chill(9, None, false, false);
    assert_eq!(nine.freeze.len(), 9, "nine procs, nine stacks");
    assert!(nine.frozen_until.is_none(), "nine does not freeze");

    let ten = chill(10, None, false, false);
    assert_eq!(ten.freeze.len(), 10, "the ladder keeps its tenth stack");
    assert!(ten.frozen_until.is_some(), "…and the tenth trips Frozen");
}

/// …AND FURTHER PROCS ARE INERT while it holds — which is what PINS the
/// ladder at ten rather than a rule of its own. Wiki: "Frozen enemies
/// cannot receive additional Cold stacks".
#[test]
fn a_frozen_target_takes_no_more_chill() {
    let mut d = chill(10, None, false, false);
    assert!(!d.apply_cold_proc(1.0, 1.0, false, None, false), "inert while Frozen");
    assert_eq!(d.freeze.len(), 10, "pinned, not climbing");
}

/// COLD COUNTS ONCE FOR CONDITION OVERLOAD, frozen or not.
///
/// THE SENTINEL FOR A TRAP NOTHING ELSE IN THE SUITE WOULD NOTICE.
/// `distinct_statuses` counting `freeze` and `frozen` on separate lines is
/// harmless only while the two are mutually exclusive; under a model where
/// the chill STAYS while Frozen it counts Cold twice and inflates every CO
/// bracket on a frozen target.
#[test]
fn cold_is_one_status_type_whether_or_not_the_target_is_frozen() {
    assert_eq!(chill(1, None, false, false).distinct_statuses(), 1, "chilled");
    assert_eq!(chill(9, None, false, false).distinct_statuses(), 1, "nine stacks");
    assert_eq!(chill(10, None, false, false).distinct_statuses(), 1, "frozen");
}

/// WHAT CANNOT FREEZE NEEDS NO RULE OF ITS OWN — the reason to prefer this
/// model, and the evidence that produced it.
///
/// A target that cannot be frozen stacks to ten and cycles FIFO there,
/// which is what showed that ten chill stacks are a state the game has. A
/// CAP does it by arithmetic and needs no flag at all: an Overguard
/// holder's four and an Acolyte's four make the trigger unreachable, which
/// is how the wiki derives it ("a maximum of 4 Cold stacks", "preventing
/// Frozen status entirely").
#[test]
fn a_target_that_cannot_freeze_just_never_trips_the_trigger() {
    let immune = chill(14, None, true, false);
    assert_eq!(immune.freeze.len(), 10, "the ladder still fills and cycles");
    assert!(immune.frozen_until.is_none(), "and never freezes");
    // …and it keeps the ladder's TOP bonus all fight, rather than spending
    // it on a 3 s window every ten procs. +0.55x: the tenth stack carries a
    // rung of its own, MEASURED on exactly this target (M46) as
    // (529 - 423) / 192 = 0.552 at a fixed non-crit.
    assert!((immune.chill_cd_bonus() - 0.55).abs() < 1e-9);

    let og = chill(14, None, false, true);
    assert_eq!(og.freeze.len(), FREEZE_CAP_UNDER_OVERGUARD, "Overguard caps at four");
    assert!(og.frozen_until.is_none(), "so ten is unreachable — no flag needed");

    let capped = chill(14, Some(StackCaps { general: 4, impact: 4 }), false, false);
    assert_eq!(capped.freeze.len(), 4, "an Acolyte caps at four");
    assert!(capped.frozen_until.is_none(), "…and cannot freeze, by arithmetic");
}

/// THE BURSTON PRIME'S CO READS 13 OF ITS 55, ON THE DIRECT HIT TOO.
///
/// MEASURED — the six readings, the ratio that cancels the target's
/// damage-type column, and the solve that lands on 13/55 = 0.2364 (the 24%
/// the catalog prints) are MEASUREMENTS M48.
///
/// Reading that 24% as the RADIAL's alone computes the direct hit's CO on
/// the full 55 and returns 231 where the game gives 181, a 28%
/// overstatement. The exclusion is the PERK's rather than the attack
/// part's, so `co_base_excludes_this_evolution` on both tier-2 +42 options
/// carries it to wherever the +42 landed.
#[test]
fn the_burston_primes_co_reads_only_its_unevolved_base() {
    let base = crate::loadout::WeaponBase::from_data(
        "burston_prime_incarnon",
        false,
        &["burston_prime_forceful_finality"],
    );
    assert!(
        (base.co_base_fraction() - 13.0 / 55.0).abs() < 1e-9,
        "direct-hit CO fraction {} against a measured 13/55",
        base.co_base_fraction()
    );
    // …and every reading it was solved from. Two AXES are exercised — the
    // arcane's stacks and the status TYPE count — and the radial is the
    // same expression without the crit, which is what identifies it as an
    // uncritical explosion rather than a second guess at the fraction.
    let f = base.co_base_fraction();
    let co = |stacks: f64, types: f64| 1.0 + 0.4 * stacks * types * f;
    for (stacks, types, direct, radial) in
        [(1.0, 1.0, 181.0, None), (2.0, 1.0, 196.0, Some(65.0)), (2.0, 2.0, 227.0, Some(76.0))]
    {
        // (the base form is checked below, on its own numbers)
        let d = 55.0 * 3.0 * co(stacks, types);
        assert!((d - direct).abs() < 1.0, "{stacks}x{types}: direct {d} vs {direct}");
        if let Some(r) = radial {
            let got = 55.0 * co(stacks, types);
            assert!((got - r).abs() < 1.0, "{stacks}x{types}: radial {got} vs {r}");
        }
    }

    // THE BASE FORM, which is the independent confirmation: another
    // attack, another crit multiplier, another fraction — and the same
    // rule. 46 base + the same 42 = 88, so CO reads 46/88, and the
    // expression collapses to `crit x (evolved + rate x original)`, which
    // is the mechanic said plainly: the perk's +42 is added AFTER CO and
    // never multiplied by it.
    let bf = crate::loadout::WeaponBase::from_data(
        "burston_prime",
        false,
        &["burston_prime_forceful_finality"],
    );
    assert!((bf.co_base_fraction() - 46.0 / 88.0).abs() < 1e-9, "{}", bf.co_base_fraction());
    // RATIOS to the bare crit (188), which cancels the damage-type column.
    for (stacks, types, hit) in [(1.0_f64, 3.0_f64, 306.0_f64), (2.0, 3.0, 423.0)] {
        let f = (hit / 188.0 - 1.0) / (0.4 * stacks * types);
        assert!(
            (f - 46.0 / 88.0).abs() < 0.005,
            "base form {stacks}x{types}: solved {f} against 46/88"
        );
    }

    // THE TWIN AT THE SAME TIER carries the same flag: it adds the same
    // +42, so a build taking it computes CO on the same base.
    let twin = crate::loadout::WeaponBase::from_data(
        "burston_prime_incarnon",
        false,
        &["burston_prime_fortress_salvo"],
    );
    assert!((twin.co_base_fraction() - 13.0 / 55.0).abs() < 1e-9);
}

/// THE DUAL TOXOCYST COMPUTES GunCO ON A FLAT 75, WHATEVER ITS PANEL SAYS —
/// and Carnage Reign's own "+33% per Status Type" does not work at all
/// (MEASUREMENTS M49,).
///
/// THE CO BASE IS THE UNEVOLVED 75 UNDER EITHER tier-2 option, against
/// CATALOGS' rule that absence from the catalog means ORDINARY — which
/// would feed Fevered Frenzy's +50 into the term. The measurement is not
/// close: at a 125 panel with 3 stacks against 2 status types, feeding the
/// +50 gives `125 + 125 x 2.4 = 425` and the game gives 305.
///
/// THE +33% IS DEAD. Carnage Reign reads "+33% Direct Damage per Status
/// Type affecting the target" and pays nothing — confirmed by the readings
/// below and by unequipping the GunCO mod entirely and shooting a
/// status-afflicted enemy for exactly the panel. A LIVE BUG rather than a
/// gap: the card is a CO source, DE's own list says so, and a hotfix
/// restores it.
#[test]
fn the_dual_toxocysts_co_reads_a_flat_seventy_five_under_either_evolution() {
    let with = |evo: &str| crate::loadout::WeaponBase::from_data("dual_toxocyst", false, &[evo]);

    // 75 of the 125 panel, and 75 of the 135 one — a FLAT 75 either way,
    // which is the whole finding.
    let frenzy = with("dual_toxocyst_fevered_frenzy");
    let carnage = with("dual_toxocyst_carnage_reign");
    assert!(
        (frenzy.co_base_fraction() - 75.0 / 125.0).abs() < 1e-9,
        "Fevered Frenzy CO fraction {} against a measured 75/125",
        frenzy.co_base_fraction()
    );
    assert!(
        (carnage.co_base_fraction() - 75.0 / 135.0).abs() < 1e-9,
        "Carnage Reign CO fraction {} against a measured 75/135",
        carnage.co_base_fraction()
    );

    // …AND CARNAGE REIGN CONTRIBUTES NO CO OF ITS OWN. If the card's +33%
    // paid out, the 135 panel would read 315 + 2 x 0.33 x 75 = 364 at two
    // status types, and it reads 315.
    assert_eq!(
        carnage.innate_co_per_type, 0.0,
        "Carnage Reign's +33% pays nothing in game (M49)"
    );

    // EVERY READING IT WAS SOLVED FROM. Galvanized Shot is 40% a stack per
    // status type, and the whole expression is
    // `panel + 75 x 0.4 x stacks x types` — the CO term added to the panel
    // rather than multiplying it, on a base the evolution never touched.
    let hit = |panel: f64, f: f64, stacks: f64, types: f64| {
        panel * (1.0 + 0.4 * stacks * types * f)
    };
    for (evo, panel, stacks, types, measured) in [
        ("dual_toxocyst_fevered_frenzy", 125.0, 3.0, 2.0, 305.0),
        ("dual_toxocyst_carnage_reign", 135.0, 3.0, 1.0, 225.0),
        ("dual_toxocyst_carnage_reign", 135.0, 1.0, 1.0, 165.0),
        ("dual_toxocyst_carnage_reign", 135.0, 3.0, 2.0, 315.0),
    ] {
        let b = with(evo);
        // The panel itself is the flat +50 / +60 landing on the base 75.
        assert!(
            (b.base_vector.total() - panel).abs() < 1e-9,
            "{evo}: panel {} against {panel}",
            b.base_vector.total()
        );
        let got = hit(panel, b.co_base_fraction(), stacks, types);
        assert!(
            (got - measured).abs() < 1.0,
            "{evo} {stacks}x{types}: {got} against a measured {measured}"
        );
    }
}

/// BOAR PRIME'S CO READS ITS OWN BASE IN BOTH FORMS, and Reified Bane's
/// +10 and +14 stay out of it (M88): 40 a pellet, 30 a tick. +165% base
/// damage, Galvanized Savvy at 40% a stack per type, both halves of the
/// perk up — a pellet at 2 stacks x 3 types reads 266 and a tick at 2 x 1
/// reads 167, and a CO term that took the +24 reads 323 and 186.
#[test]
fn boar_prime_co_reads_its_own_base_under_reified_bane_in_both_forms() {
    for (id, own_base, stacks, types, measured) in [
        ("boar_prime", 40.0, 2.0, 3.0, 266.0),
        ("boar_prime_incarnon", 30.0, 2.0, 1.0, 167.0),
    ] {
        let b = crate::loadout::WeaponBase::from_data(id, false, &["boar_prime_reified_bane"]);
        let panel = b.base_vector.total();
        // THE +14, not the card's +10 — the gated half as the damage reads it.
        assert!((panel - (own_base + 24.0)).abs() < 1e-9, "{id}: panel {panel}");
        let f = b.co_base_fraction();
        assert!((panel * f - own_base).abs() < 1e-9, "{id}: CO reads {}", panel * f);
        let got = panel * (1.0 + 1.65 + 0.4 * stacks * types * f);
        assert!((got - measured).abs() < 0.5, "{id}: {got} against a measured {measured}");
    }
}

/// THE TORID'S INCARNON FORM COMPUTES CO ON 51 OF ITS 102 (MEASUREMENTS
/// M50,) — the second weapon where the catalog's
/// ABSENCE-MEANS-ORDINARY rule was measured and was wrong, and the
/// most-played Incarnon in the game.
///
/// THE FORM IS IDENTIFIED BY THE CRIT, not by the report. A bare crit of
/// 316 against a panel of 102 is x3.098, which is the Incarnon form's 3.1;
/// the base form's is 2.0 and would have read 302 off its own 151 panel.
/// That is what makes the reading unambiguous without a second run.
///
/// WHAT THE CATALOG SAYS ABOUT THIS FORM: nothing. Its two Torid rows are
/// the base form's main-fire and cloud, both "100 | 100% | Multiplying",
/// and neither carries the "or 151" variant the Dual Toxocyst's row does.
/// The silence was read as "the evolution feeds in full" and means "the
/// table never measured the evolved weapon" — see docs/CATALOGS.md, where
/// the default is now 0 for 2 on the weapons anyone has checked.
#[test]
fn the_torid_incarnons_co_reads_half_its_evolved_base() {
    let inc = crate::loadout::WeaponBase::from_data(
        "torid_incarnon",
        false,
        &["torid_final_fusillade"],
    );
    assert!(
        (inc.base_vector.total() - 102.0).abs() < 1e-9,
        "panel {} against a measured 102",
        inc.base_vector.total()
    );
    assert!(
        (inc.co_base_fraction() - 51.0 / 102.0).abs() < 1e-9,
        "CO fraction {} against a measured 51/102",
        inc.co_base_fraction()
    );

    // THE READING, solved the way M48 taught: a RATIO to the bare crit,
    // which cancels the target's damage-type column. Both numbers are the
    // game's rounded display, so the solve is a BAND rather than a point —
    // and 0.5 sits in it while 1.0 is nowhere near.
    let (bare, hit) = (316.0_f64, 380.0_f64);
    let solved = |b: f64, h: f64| (h / b - 1.0) / 0.4;
    let (lo, hi) = (solved(bare + 0.5, hit - 0.5), solved(bare - 0.5, hit + 0.5));
    assert!(lo < 0.5 && 0.5 < hi, "0.5 outside the display band [{lo}, {hi}]");
    assert!(hi < 1.0, "the band must exclude a CO fed by the +51: [{lo}, {hi}]");
    // …and the engine's own fraction lands in the same band.
    assert!(lo < inc.co_base_fraction() && inc.co_base_fraction() < hi);

    // THE TIER-MATE, measured on its own numbers — flagged by inference
    // from this one first and confirmed within the hour.
    let mate = crate::loadout::WeaponBase::from_data(
        "torid_incarnon",
        false,
        &["torid_plentiful_mayhem"],
    );
    assert!((mate.base_vector.total() - 82.0).abs() < 1e-9);
    assert!((mate.co_base_fraction() - 51.0 / 82.0).abs() < 1e-9);

    // AND THE DECISIVE FORM OF THE FINDING, which is not either fraction:
    // THE CO BASE IS A CONSTANT. Solved as an ABSOLUTE rather than as a
    // ratio, every reading across both perks lands on the unevolved 51 —
    // while the panels they were read off are 102 and 82. A term that fed
    // on the evolution would solve to those two numbers instead and would
    // not agree with itself across the pair. This is what a ratio alone
    // cannot say, and it is why the second perk was worth measuring.
    for (panel, bare, stacks, hit) in [
        (102.0_f64, 316.0_f64, 1.0_f64, 380.0_f64),
        (82.0, 254.0, 1.0, 318.0),
        (82.0, 254.0, 2.0, 381.0),
    ] {
        let co_base = (hit / bare - 1.0) / (0.4 * stacks) * panel;
        assert!(
            (co_base - 51.0).abs() < 1.0,
            "solved a CO base of {co_base} off a {panel} panel, against 51"
        );
    }

    // AND THE BASE FORM IS THE OTHER CLASS, which is the deliberate half.
    // It is `Multiplying` where the form measured above is `Adding`, and it
    // reads its FULL evolved 151 — the cheapest open experiment there was
    // (393 if fed against 311 if not, 26% apart), run measured within
    // the day and answering FED. Its own readings are the test below.
    let base = crate::loadout::WeaponBase::from_data(
        "torid",
        false,
        &["torid_final_fusillade"],
    );
    assert!((base.base_vector.total() - 151.0).abs() < 1e-9);
    assert_eq!(base.co_behavior, crate::loadout::CoBehavior::Independent);
    assert!(
        (base.co_base_fraction() - 1.0).abs() < 1e-9,
        "the Multiplying half reads its full evolved base (M51), got {}",
        base.co_base_fraction()
    );
}

/// M51 — THE TORID'S BASE FORM, AND THE TWO CO CLASSES DISAGREE.
///
/// The same weapon and the same two tier-2 perks as the test above, on the
/// `Multiplying` entry rather than the `Adding` one. The readings are
/// MEASUREMENTS M51; the answer is the OPPOSITE of M50's — the CO
/// multiplier is 1.40 and 1.80 under BOTH perks, so a `Multiplying` entry
/// reads its full evolved base.
///
/// THE DECISIVE SHAPE IS THE TWO COLUMNS. The same flat +51 lands on both
/// attack parts, so the impact's evolved base is 151 and the cloud's is 91.
/// A term reading anything OTHER than the evolved base has a different
/// fraction in each column (0.662 against 0.440) and must print two
/// different multipliers; it printed 1.3997 and 1.4000. And the CLOUD
/// taking CO at all confirms the doubly-discrepant catalog row from the
/// other side.
#[test]
fn the_torid_base_forms_multiplying_co_reads_its_full_evolved_base() {
    for (perk, evolved, impact, cloud) in [
        (
            "torid_final_fusillade",
            151.0_f64,
            // (stacks x types, bare, hit)
            [(1.0_f64, 763.0_f64, 1068.0_f64), (2.0, 763.0, 1373.0)],
            [(1.0_f64, 460.0_f64, 644.0_f64), (2.0, 460.0, 827.0)],
        ),
        (
            "torid_plentiful_mayhem",
            131.0,
            [(1.0, 662.0, 926.0), (2.0, 662.0, 1191.0)],
            [(1.0, 359.0, 502.0), (2.0, 359.0, 646.0)],
        ),
    ] {
        let b = crate::loadout::WeaponBase::from_data("torid", false, &[perk]);
        assert!((b.base_vector.total() - evolved).abs() < 1e-9,
            "{perk}: panel {} against a measured {evolved}", b.base_vector.total());
        assert_eq!(b.co_behavior, crate::loadout::CoBehavior::Independent);
        assert!((b.co_base_fraction() - 1.0).abs() < 1e-9);

        // Solved the way M50 taught: a RATIO to the bare hit, which cancels
        // the target's damage-type column and every mod on the build. Both
        // numbers are the game's rounded display, so it is a BAND.
        for (part, readings) in [("impact", impact), ("cloud", cloud)] {
            for (types, bare, hit) in readings {
                let solved = |b: f64, h: f64| (h / b - 1.0) / (0.4 * types);
                let (lo, hi) =
                    (solved(bare + 0.5, hit - 0.5), solved(bare - 0.5, hit + 0.5));
                assert!(lo < 1.0 && 1.0 < hi,
                    "{perk} {part} {types} types: the full evolved base is outside \
                     the display band [{lo:.4}, {hi:.4}]");
                assert!(lo > 0.85,
                    "{perk} {part} {types} types: the band [{lo:.4}, {hi:.4}] must \
                     exclude a CO fed by the unevolved base");
            }
        }
    }

    // THE TWO COLUMNS, stated as the argument rather than as six bands: one
    // perk, two attack parts, two different evolved bases (151 and 91), and
    // the SAME measured multiplier. Every rival hypothesis splits them.
    let m_impact = 1068.0_f64 / 763.0;
    let m_cloud = 644.0_f64 / 460.0;
    assert!((m_impact - m_cloud).abs() < 0.005,
        "the two columns must agree: {m_impact:.4} against {m_cloud:.4}");
    // …and what "reads the unevolved base" would have required of them.
    let split = (1.0 + 0.4 * 100.0 / 151.0) - (1.0 + 0.4 * 40.0 / 91.0);
    assert!(split > 0.08, "the rival hypothesis must be distinguishable, got {split:.4}");
}

/// A RESPAWNED BODY IS A NEW INDIVIDUAL, EVERYWHERE — not only where the
/// gun is pointed.
///
/// The aimed path has cleared the target's statuses on death since the
/// engine had statuses. The spread path — punch through, chains, blasts,
/// clouds — counted the kill and left the pile standing, so on an
/// instant-respawn ruler every body but the aimed one inherited six
/// seconds of stacks it had never been given: more Condition Overload,
/// more Viral, less armour, on every fight with a formation in it.
///
/// Two bodies on one line, both frail, both punched through by the same
/// shot. What separates them is only WHICH ONE IS AIMED AT, so the damage
/// they take may not diverge.
#[test]
fn a_respawned_body_in_the_formation_starts_with_no_statuses() {
    let base = crate::loadout::WeaponBase::from_data("soma_prime", true, &[]);
    let pool = crate::mods_data::pool_for_weapon("soma_prime");
    let refs: Vec<&crate::loadout::ModDef> = ["malignant_force", "primed_cryo_rounds"]
        .iter()
        .filter_map(|id| pool.iter().find(|m| m.id == *id))
        .collect();
    assert_eq!(refs.len(), 2, "Viral needs both halves");
    let panel = crate::loadout::resolve(&base, &refs, crate::loadout::StackPolicy::Emergent);
    let frail = frail_target(TargetMode::InstantRespawn, 0.0, 0.0);
    let mut arena = crate::arena::Arena::training(30.0);
    arena.target_at = crate::space::Vec2::new(0.0, 1.0);
    arena.others = vec![crate::formation::FoeSpec {
        id: "e2".into(),
        params: frail.clone(),
        body_parts: FightParams::humanoid_parts(),
        at: crate::space::Vec2::new(0.0, 2.0),
    }];
    let mut p = FightParams::from_panel(
        &panel, &arena, &crate::arcanes_data::ArcaneFx::none(),
    );
    p.target = frail;
    // Enough to cross both bodies and keep going.
    p.punch_through_m = 4.0;
    let r = run_once(&p, &mut Rng::new(0x5EED));
    let by = r.spread.by_body().0;
    assert!(by[0] > 0.0 && by[1] > 0.0, "both bodies are on the line: {by:?}");
    assert!(
        by[1] <= by[0] * 1.5,
        "the body behind may not snowball on stacks the aimed one is denied:              aimed {} against behind {} ({:.2}x)",
        by[0], by[1], by[1] / by[0]
    );
}
