use super::*;

/// FROZEN REPLACES THE LADDER'S BONUS, it does not add to it. Wiki, of the
/// +1.0x: "doubled from the per-stack bonus" — it stands in for the +0.50x.
/// Now that both are live at once this is a real choice rather than a
/// consequence of the ladder being empty.
#[test]
fn frozen_replaces_the_chill_bonus_rather_than_adding_to_it() {
    let nine = chill(9, None, false, false);
    assert!((nine.cold_cd_bonus(1.0) - 0.50).abs() < 1e-9, "the published +0.50x at nine");
    let ten = chill(10, None, false, false);
    assert!((ten.cold_cd_bonus(1.0) - FROZEN_CRIT_DAMAGE_RECEIVED).abs() < 1e-9);
    // The ladder is still there and still says its own number — this is the
    // one place that decides which of the two the target feels.
    assert!((ten.chill_cd_bonus() - 0.55).abs() < 1e-9);
}

/// Cernos Prime's innate headshot bonus MULTIPLIES the additive bracket.
///
/// "Cernos Prime's headshot bonus is unique and stacks multiplicatively
/// with Primary Deadhead's headshot bonus" (wiki, Primary Deadhead). The
/// word that matters is UNIQUE: the same note lists innate bonuses on
/// weapons like Kuva Chakkhurr among the ADDITIVE sources, so this is a
/// per-weapon anomaly and the flag rides on the weapon.
///
/// On a 3x head with the arcane's +30% and an innate +50%:
///   additive       3 x (1 + 0.3 + 0.5) = 5.40
///   multiplicative 3 x 1.3 x 1.5       = 5.85   (+8.33%)
#[test]
fn cernos_primes_innate_headshot_bonus_multiplies_instead_of_adding() {
    let build = |mult: bool| FightParams {
        duration_seconds: 30.0,
        headshot_damage_bonus: 0.5,
        headshot_bonus_multiplicative: mult,
        arcane: crate::arcanes_data::ArcaneFx {
            headshot_multiplier_bonus: 0.3,
            ..crate::arcanes_data::ArcaneFx::none()
        },
        body_parts: vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: false,
        }],
        ..no_status()
    };
    let add = monte_carlo(&build(false), 1, 5).mean_damage;
    let mul = monte_carlo(&build(true), 1, 5).mean_damage;
    assert!(
        (mul / add - 5.85 / 5.40).abs() < 1e-6,
        "5.85 / 5.40 = 1.0833…, got {}",
        mul / add
    );

    // With NO arcane bonus the two readings agree — the anomaly is about
    // how the innate share COMBINES, not about its size.
    let bare = |mult: bool| FightParams {
        arcane: crate::arcanes_data::ArcaneFx::none(),
        ..build(mult)
    };
    let (a, m) = (
        monte_carlo(&bare(false), 1, 5).mean_damage,
        monte_carlo(&bare(true), 1, 5).mean_damage,
    );
    assert!((a - m).abs() < 1e-9, "3 x 1.5 either way: {a} vs {m}");
}

#[test]
fn reified_banes_empty_reload_damage_is_a_buff_that_starts_on() {
    // "On Reload From Empty: +14 Base Damage" is a BUFF, not a silent stat: it belongs on the buff bar, it opens at one stack
    // because the modelled fight always reloads from empty, and it never
    // times out. What makes it a buff rather than a decoration is that
    // turning it off has to MOVE the damage.
    use crate::loadout::{resolve, StackPolicy, WeaponBase};
    let with = WeaponBase::from_data(
        "boar_prime", true, &["boar_prime_evo1_incarnon_form", "boar_prime_reified_bane"],
    );
    let panel = resolve(&with, &[], StackPolicy::Emergent);
    let base_damage = panel.evo_base_damage.expect("reified bane grants the evo base_damage buff");
    assert_eq!(base_damage.max_stacks, 1);
    assert_eq!(base_damage.stacks, 1, "it opens ON");
    assert!((base_damage.full - 14.0).abs() < 1e-9, "the empty-reload half is +14, got {}", base_damage.full);

    // It is ANNOUNCED, or no card is drawn for it.
    let params = FightParams::from_panel(&panel, &crate::arena::Arena::training(10.0), &ArcaneFx::none());
    assert!(
        params.buff_roster().iter().any(|b| b.id == "evo_reload_damage" && b.max_stacks == 1),
        "the buff bar never hears about it: {:?}",
        params.buff_roster()
    );

    // And turning it off takes the damage back off — down to what the
    // build would be with only the unconditional +10 half.
    let off = {
        let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(10.0), &ArcaneFx::none());
        let mut cfg = BuffConfig::new();
        cfg.insert("evo_reload_damage".into(), (0, false));
        p.apply_buff_config(&cfg);
        p
    };
    assert!(off.damage.total() < params.damage.total(), "0 stacks changed nothing");
    let ratio = off.damage.total() / params.damage.total();
    let want = base_damage.without / (base_damage.without + base_damage.full);
    assert!((ratio - want).abs() < 1e-9, "scaled by {ratio}, expected {want}");
    assert_eq!(off.evo_base_damage.expect("still declared").stacks, 0);
}

#[test]
fn evo_multishot_config_rescales_the_permanent_stacks() {
    // Fevered Frenzy: base 1 pellet × (+100% at 20 stacks) is baked into
    // the resolved multishot; the per-buff config rescales it statically
    // (no in-sim trigger, no decay). Lock is meaningless and ignored.
    use crate::loadout::{resolve, StackPolicy, WeaponBase};
    let base = WeaponBase::from_data(
        "dual_toxocyst_incarnon",
        true,
        &["dual_toxocyst_evo1_incarnon_form", "dual_toxocyst_fevered_frenzy"],
    );
    let panel = resolve(&base, &[], StackPolicy::Emergent);
    let multishot = panel.evo_multishot.expect("fevered frenzy grants the evo multishot buff");
    assert_eq!(multishot.max_stacks, 20);
    assert!((multishot.full - 1.0).abs() < 1e-12, "1 pellet × +100% = 1.0");

    let mk = |stacks: u32, locked: bool| {
        let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(10.0), &ArcaneFx::none());
        let mut cfg = BuffConfig::new();
        cfg.insert("evo_multishot".into(), (stacks, locked));
        p.apply_buff_config(&cfg);
        p.multishot
    };
    let full = panel.multishot;
    assert!(
        (mk(20, true) - full).abs() < 1e-12,
        "full stacks = untouched"
    );
    assert!(
        (mk(0, false) - (full - 1.0)).abs() < 1e-12,
        "0 stacks removes the whole bonus"
    );
    assert!(
        (mk(10, false) - (full - 0.5)).abs() < 1e-12,
        "half stacks remove half"
    );
    assert!(
        (mk(10, true) - mk(10, false)).abs() < 1e-12,
        "lock is ignored (permanent)"
    );
}

/// Every charge weapon that is NOT a bow pays the draw AND the listed
/// rate's interval — the wiki's general formula, "Effective Fire Rate =
/// 1 / (Modded Charge Time + 1 / Modded Fire Rate)". The listed rate is
/// what happens AFTER the charge, which is why it is added and not
/// replaced.
///
/// Larkspur Prime's alt-fire is the case that brought this in: 0.5 s
/// charge at a listed 2.0/s = 1.0 s a shot, where a bow would fire twice
/// as often off the same two numbers.
#[test]
fn a_general_charge_weapon_pays_the_draw_and_the_rate() {
    let base = FightParams {
        fire_rate: 2.0,
        charge_seconds: Some(0.5),
        magazine_size: 1000.0, // no reload inside the window
        duration_seconds: 10.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let general = FightParams {
        charge_cadence: crate::weapons_data::ChargeCadence::DrawThenRate,
        ..base.clone()
    };
    // 0.5 + 1/2.0 = 1.0 s: shots at 0, 1, 2 … 9 — ten inside 10 s.
    assert_eq!(run_once(&general, &mut Rng::new(1)).shots, 10);

    // The bow reading of the SAME two numbers is the draw alone, 0.5 s —
    // twice as many shots. One weapon's formula is not the other's.
    let bow = FightParams {
        charge_cadence: crate::weapons_data::ChargeCadence::DrawOnly,
        ..base
    };
    assert_eq!(run_once(&bow, &mut Rng::new(1)).shots, 20);
}

/// A BURST weapon's listed fire rate is BURSTS per second, so its real
/// cadence is the wiki's formula and not `1 / fire_rate`:
///
///   Effective Fire Rate = Burst Count / [1/Fire Rate + (Burst Count−1)⋅
///   Burst Delay]
///
/// Burston Prime's numbers — 3 rounds, 5 bursts/s, 0.04 s apart — give
/// 3 / (0.2 + 0.08) = 10.714 rounds/s, better than DOUBLE what the listed
/// 5 would suggest. Reading the stat as a plain rate is not a rounding
/// error on a burst weapon; it is wrong by the burst count.
#[test]
fn a_burst_weapon_fires_its_whole_burst_inside_the_listed_interval() {
    let burston = FightParams {
        fire_rate: 5.0,
        burst: Some(crate::weapons_data::BurstSpec { count: 3, delay_seconds: 0.04 }),
        magazine_size: 100_000.0, // no reload inside the window
        duration_seconds: 10.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    // 0.28 s a burst / 3 = 0.0933… s a round: 107 rounds in 10 s, +1 for
    // the shot at t=0.
    let r = run_once(&burston, &mut Rng::new(1));
    assert_eq!(r.shots, 108, "3 / (1/5 + 2 x 0.04) = 10.714 rounds/s");

    // The SAME two numbers read as an ordinary auto weapon: 5 rounds/s,
    // 51 shots. That is the mistake this field exists to prevent.
    let as_rate = FightParams { burst: None, ..burston.clone() };
    assert_eq!(run_once(&as_rate, &mut Rng::new(1)).shots, 51);

    // A one-round "burst" IS an ordinary weapon — no delay is ever paid,
    // so the two readings must agree exactly.
    let single = FightParams {
        burst: Some(crate::weapons_data::BurstSpec { count: 1, delay_seconds: 0.04 }),
        ..burston.clone()
    };
    assert_eq!(run_once(&single, &mut Rng::new(1)).shots, 51);
}

/// A HELD TRIGGER SPOOLS DOWN, and the loss is most of the magazine rather
/// than a rounding error.
///
/// The Phenmor's Incarnon numbers — 13.33 rounds/s falling to 60% over 51
/// held shots (wiki, verbatim in `SustainedFireRate`). In ten seconds that
/// is 93 rounds instead of 134: **31% fewer shots**, which is the whole of
/// the difference between the rate the arsenal prints and the rate the
/// weapon fires at once its 408-round pool is more than four seconds old.
///
/// The spool is why the two forms compare the way they do at all. Reading
/// 13.33 flat overstates the Incarnon form's sustained damage by half.
#[test]
fn a_held_trigger_spools_down_and_costs_most_of_the_magazine() {
    let phenmor = FightParams {
        fire_rate: 13.33,
        magazine_size: 100_000.0, // no reload inside the window
        duration_seconds: 10.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    assert_eq!(run_once(&phenmor, &mut Rng::new(1)).shots, 134, "the listed rate, flat");

    let spooled = FightParams {
        sustained_fire_rate: Some(crate::weapons_data::SustainedFireRate {
            start: 1.00,
            end: 0.60,
            over_shots: 51.0,
        }),
        ..phenmor.clone()
    };
    assert_eq!(run_once(&spooled, &mut Rng::new(1)).shots, 93);

    // …AND IT RESETS WHEN FIRING STOPS. A magazine of one puts a pause
    // before every shot, so the spool never advances past its first step
    // and the count matches the unspooled weapon EXACTLY — the reset is
    // derived from the gap, not from anyone remembering to clear it in the
    // reload branch.
    let tapped = FightParams {
        magazine_size: 1.0,
        reload_seconds: 0.05,
        ..spooled.clone()
    };
    let flat_tapped = FightParams { sustained_fire_rate: None, ..tapped.clone() };
    assert_eq!(
        run_once(&tapped, &mut Rng::new(1)).shots,
        run_once(&flat_tapped, &mut Rng::new(1)).shots,
        "a pause between every shot is a trigger released between every shot"
    );
}

/// EXECUTIONER'S FORTUNE fills the magazine, and its CONDITION gates it.
///
/// Two weapons, two readings of the same kind: the Furis pair pay on any
/// headshot, the Phenmor only on one that kills. Against a target this sim
/// cannot kill, the second must be worth exactly nothing while the first is
/// worth a great deal — which is the assertion, because a version that
/// ignored `needs_kill` would look perfectly healthy on the Furis.
#[test]
fn executioners_fortune_needs_the_kill_when_the_card_says_so() {
    let head = |chance: f64, needs_kill: bool| FightParams {
        fire_rate: 10.0,
        magazine_size: 10.0,
        reload_seconds: 5.0,
        duration_seconds: 30.0,
        // EVERY shot into a HEAD, which is what the official ruler does
        // and what makes the perk's own rate the only variable here.
        body_parts: all_head(),
        instant_reload: (chance > 0.0)
            .then_some(crate::loadout::InstantReload { chance, needs_kill }),
        ..no_status()
    };
    // A target that cannot die — `InfiniteHealth` says so outright, which
    // is stronger than a large number and is what the default fixture is.
    let unkillable = |p: FightParams| FightParams {
        target: frail_target(TargetMode::InfiniteHealth, 0.0, 0.0),
        ..p
    };

    let plain = run_once(&unkillable(head(0.0, false)), &mut Rng::new(7)).shots;
    // ANY headshot pays (the Furis): a 10% chance on every shot saves most
    // of the reloads, so far more rounds fit in the same 30 s.
    let furis = run_once(&unkillable(head(0.10, false)), &mut Rng::new(7)).shots;
    assert!(furis > plain, "{furis} shots with the perk, {plain} without");

    // ONLY A KILLING headshot pays (the Phenmor): nothing here ever dies,
    // so it must be worth precisely nothing — not "nearly nothing".
    let phenmor = run_once(&unkillable(head(0.20, true)), &mut Rng::new(7)).shots;
    assert_eq!(phenmor, plain, "a kill-gated perk paid without a kill");
}

/// LINGERING JUDGEMENT joins the ADDITIVE headshot bracket, beside Primary
/// Deadhead's — it does not multiply it.
///
/// VERBATIM (wiki, supplied measured 2026-08-10): "Headshot damage
/// bonus stacks additively with Primary Deadhead's headshot damage bonus."
/// That is the whole finding, because the two readings are far apart on a
/// build that carries the arcane and identical on one that does not — so a
/// test using the perk alone would pass either way.
///
/// On a 2x head with the arcane's +30% and the perk's +50%, against the
/// same build without the perk (2 x 1.3 = 2.60):
///   additive        2 x (1 + 0.3 + 0.5) = 3.60   ->  x1.3846
///   multiplicative  2 x 1.3 x 1.5       = 3.90   ->  x1.5000
#[test]
fn lingering_judgement_adds_to_deadheads_bracket_instead_of_multiplying_it() {
    let streak = crate::loadout::HeadshotStreak {
        hits: 2,
        within: 2.0,
        value: 0.50,
        duration: 8.0,
    };
    // Every shot into a 2x head, no crit, no status: the only thing moving
    // the number is the headshot bracket.
    let build = |deadhead: f64, perk: bool| FightParams {
        fire_rate: 10.0,
        magazine_size: 1e9,
        duration_seconds: 10.0,
        base_crit_chance: 0.0,
        unmodded_crit_chance: 0.0,
        body_parts: vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 2.0,
            is_head: true,
            crit_bonus: false,
        }],
        headshot_streak: perk.then_some(streak),
        arcane: crate::arcanes_data::ArcaneFx {
            headshot_multiplier_bonus: deadhead,
            ..crate::arcanes_data::ArcaneFx::none()
        },
        target: TargetParams { base_health: 1e15, ..FightParams::default().target },
        ..no_status()
    };
    let dmg = |p: &FightParams| {
        let r = run_once(p, &mut Rng::new(5));
        r.total_damage() / f64::from(r.shots)
    };
    // WITH DEADHEAD the two readings are 8% apart, and additive is the one
    // the card describes. The measured ratio sits a hair UNDER 1.3846
    // because the shot that arms the streak is not itself buffed — 1 of the
    // 100 shots in the window — which is the behaviour, not slack.
    let base = dmg(&build(0.30, false));
    let with = dmg(&build(0.30, true));
    let ratio = with / base;
    assert!(
        (ratio - 3.60 / 2.60).abs() < 0.02,
        "additive gives {:.4}, multiplicative would give {:.4}, got {ratio:.4}",
        3.60 / 2.60,
        3.90 / 2.60
    );

    // …and WITHOUT the arcane both readings agree at 3.0/2.0, which is why
    // the case above is the one that carries the claim.
    let solo = dmg(&build(0.0, true)) / dmg(&build(0.0, false));
    assert!((solo - 1.5).abs() < 0.02, "{solo}");
}

/// …and the streak has to be EARNED: two headshots inside two seconds.
#[test]
fn lingering_judgement_needs_two_headshots_inside_the_window() {
    let streak = crate::loadout::HeadshotStreak {
        hits: 2,
        within: 2.0,
        value: 0.50,
        duration: 8.0,
    };
    let at_rate = |fire_rate: f64| FightParams {
        fire_rate,
        magazine_size: 1e9,
        duration_seconds: 60.0,
        base_crit_chance: 0.0,
        unmodded_crit_chance: 0.0,
        body_parts: all_head(),
        headshot_streak: Some(streak),
        target: TargetParams { base_health: 1e15, ..FightParams::default().target },
        ..no_status()
    };
    let per_shot = |p: &FightParams| {
        let r = run_once(p, &mut Rng::new(5));
        r.total_damage() / f64::from(r.shots)
    };
    // TEN SHOTS A SECOND: the second one arms it and it never lapses.
    let fast = per_shot(&at_rate(10.0));
    // ONE SHOT EVERY THREE SECONDS: no two headshots ever fall inside two,
    // so the perk is worth nothing at all — the "within" clause binding.
    let slow = per_shot(&at_rate(1.0 / 3.0));
    assert!(fast > slow * 1.4, "fast {fast}, slow {slow}");
    // The slow build must match a weapon without the perk EXACTLY.
    let bare = FightParams { headshot_streak: None, ..at_rate(1.0 / 3.0) };
    assert!((slow - per_shot(&bare)).abs() < 1e-9, "a streak armed without a streak");
}

/// SPITEFUL DEFILEMENT counts status TYPES, and dies on the third.
///
/// VERBATIM (wiki,): "Multiple instances of the same
/// status effect are not counted separately, e.g. having 5 corrosive and 5
/// radiation status effects on a target will not disable this buff." That
/// example is the test: ten procs of two types must leave it running, and
/// one proc of a third type must kill it.
///
/// It also lands AFTER MODS as a FLAT value — "+100% Critical Damage" is
/// `+1.0` on the finished multiplier, not a doubling of it.
#[test]
fn spiteful_defilement_counts_types_not_stacks() {
    let build = |procs: Vec<DamageType>, perk: bool| FightParams {
        fire_rate: 10.0,
        magazine_size: 1e9,
        duration_seconds: 10.0,
        // ALWAYS crit, so crit damage is the only variable.
        base_crit_chance: 1.0,
        unmodded_crit_chance: 1.0,
        crit_multiplier: 2.0,
        unmodded_crit_damage: 2.0,
        forced_procs: procs,
        status_chance: 0.0,
        base_status_chance: 0.0,
        body_parts: mono_body(1.0),
        crit_damage_below_status_count: perk.then_some((3, 1.0)),
        // `no_status()` inherits the default fixture's ARCANE, which has
        // crit terms of its own — they would land inside the ratio and make
        // "+1.0 flat" unmeasurable. Neutralised so the only crit-damage
        // sources are the weapon's 2.0 and the perk's flat add.
        arcane: crate::arcanes_data::ArcaneFx::none(),
        target: TargetParams { base_health: 1e15, ..FightParams::default().target },
        ..no_status()
    };
    let dmg = |p: &FightParams| {
        let r = run_once(p, &mut Rng::new(5));
        r.total_damage() / f64::from(r.pellets.max(1))
    };
    // TWO TYPES, ten stacks each — the card's own example. The perk runs,
    // and a flat +1.0 on a 2.0 multiplier is exactly 1.5x the damage.
    let two = vec![DamageType::Corrosive, DamageType::Radiation];
    let on = dmg(&build(two.clone(), true)) / dmg(&build(two.clone(), false));
    assert!((on - 1.5).abs() < 0.02, "two types must keep it: {on}");

    // A THIRD TYPE turns it off, and nothing else changed.
    let three = vec![DamageType::Corrosive, DamageType::Radiation, DamageType::Viral];
    let off = dmg(&build(three.clone(), true)) / dmg(&build(three, false));
    assert!((off - 1.0).abs() < 0.02, "a third type must kill it: {off}");
}

/// AN INCARNON FORM GETS NOTHING FROM IT — the pool is the wrong one.
///
/// VERBATIM (wiki, Executioner's Fortune): "Does not affect Incarnon Form".
/// The reason is what makes it testable rather than a special case: the
/// perk refills a MAGAZINE, and an Incarnon form has max CHARGES instead. A charge pool is converted from weakpoint hits and
/// sits outside the ammo economy — there is no reload there to make
/// instant.
///
/// A charge-backed form is marked by `ammo_efficiency_applies == false`,
/// the same marker the ammo rules read, so this cannot disagree with them
/// about which pool is which.
#[test]
fn executioners_fortune_does_not_touch_an_incarnon_charge_pool() {
    // A charge-backed form: its "magazine" is the gauge's round pool, and
    // `ammo_efficiency_applies` is the flag that says so.
    let charge_form = |chance: f64| FightParams {
        fire_rate: 10.0,
        magazine_size: 10.0,
        reload_seconds: 5.0,
        duration_seconds: 30.0,
        body_parts: all_head(),
        ammo_efficiency_applies: false, // charge-backed
        target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
        instant_reload: (chance > 0.0)
            .then_some(crate::loadout::InstantReload { chance, needs_kill: false }),
        ..no_status()
    };
    assert_eq!(
        run_once(&charge_form(1.0), &mut Rng::new(3)).shots,
        run_once(&charge_form(0.0), &mut Rng::new(3)).shots,
        "a charge pool has no magazine to fill"
    );

    // …AND THE SAME WEAPON WITH A REAL MAGAZINE DOES take it, so the
    // assertion above is about the pool and not about a perk that never
    // fires. Identical in every other respect, including the seed.
    let with_mag = |chance: f64| FightParams {
        ammo_efficiency_applies: true,
        ..charge_form(chance)
    };
    assert!(
        run_once(&with_mag(1.0), &mut Rng::new(3)).shots
            > run_once(&with_mag(0.0), &mut Rng::new(3)).shots
    );
}

/// …and it DOES pay once the target dies.
///
/// The pair above proves the gate closes; this proves it opens, so the two
/// together cannot be satisfied by a perk that simply never fires.
#[test]
fn a_killing_headshot_fills_the_magazine() {
    let p = FightParams {
        fire_rate: 10.0,
        magazine_size: 10.0,
        reload_seconds: 5.0,
        duration_seconds: 30.0,
        body_parts: all_head(),
        // `InstantRespawn` is the whole point and it is not a detail of
        // frailty: the DEFAULT fixture target is `InfiniteHealth`, so a
        // 1 HP version of it still never dies and a kill-gated perk reads
        // as broken. That is what the first draft of this test did.
        target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
        ..no_status()
    };
    let without = run_once(&p, &mut Rng::new(11)).shots;
    let with = FightParams {
        instant_reload: Some(crate::loadout::InstantReload { chance: 1.0, needs_kill: true }),
        ..p.clone()
    };
    let armed = run_once(&with, &mut Rng::new(11)).shots;
    assert!(armed > without, "{armed} shots with the perk, {without} without");
}

/// READY RETALIATION: the magazine that ran out is what pays for the reload.
///
/// THE EMPTY MAGAZINE ARMS IT and the next reload spends it, so that reload
/// is already faster — the first one of the fight included. The arming moment is the shot that empties the magazine
/// rather than the reload that follows, which only matters when something
/// else happens in between; see the transform test beside this one.
///
/// The FIRST reload is the sharp case and it gets its own window here: the
/// run is cut short so that exactly one reload is in it, and the perk is
/// the difference between the second magazine having started and not. An
/// end-to-end count over many reloads cannot tell rule 1 from "only later
/// reloads count" — the first version of this test asserted the opposite
/// rule and passed, because at those numbers both readings happened to fit
/// the same whole number of magazines.
#[test]
fn ready_retaliation_speeds_up_the_reload_that_arms_it() {

    // 10 rounds at 10/s = 1 s of firing, then a 2 s reload — 1 s with the
    // perk. Stopping the clock at 2.5 s puts the second magazine's first
    // shots on one side of the line and nothing on the other.
    let p = FightParams {
        fire_rate: 10.0,
        magazine_size: 10.0,
        reload_seconds: 2.0,
        duration_seconds: 2.5,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let with = FightParams { rs_on_reload: 1.0, ..p.clone() };
    let without = run_once(&p, &mut Rng::new(1)).shots;
    let armed = run_once(&with, &mut Rng::new(1)).shots;
    assert_eq!(without, 10, "the magazine, and the reload still running at 2.5 s");
    assert!(
        armed > 10,
        "the FIRST reload takes the buff it armed: {armed} shots, wanted more than 10"
    );

    // …and over a long run it compounds into whole extra magazines.
    let long = FightParams { duration_seconds: 60.0, ..p.clone() };
    let long_armed = FightParams { rs_on_reload: 1.0, ..long.clone() };
    let a = run_once(&long, &mut Rng::new(1)).reloads;
    let b = run_once(&long_armed, &mut Rng::new(1)).reloads;
    assert!(b > a, "{b} reloads with the perk, {a} without");
}

/// REAVER'S RAPTURE, and every one of its moments.
///
/// "On Full Burst Hit: +20% Damage, resets on Reload", capped at 5x. Four
/// separate claims, and each is asserted on its own because each can be
/// wrong by itself:
///
/// 1. ONE STACK PER BURST, not per round and not per pellet — the card's
///    "not affected by multishot" — so a 3-round burst weapon earns a stack
///    every three rounds;
/// 2. THE MOMENT IS THE LAST ROUND of the burst, so the burst that earns
///    the stack does not carry it;
/// 3. it CAPS at five;
/// 4. it is RESET BY THE REFILL, at the instant the reload completes.
///
/// Measured on stacks rather than on damage, because a damage figure folds
/// all four together and could be right for the wrong reason.
#[test]
fn reavers_rapture_counts_bursts_and_resets_on_the_refill() {
    let buff = crate::loadout::StackingBuff {
        id: "full_burst_damage",
        trigger: crate::loadout::BuffTrigger::FullBurst,
        grant: crate::loadout::BuffGrant::BaseDamage,
        decay: crate::loadout::BuffDecay::LoseOneAndReset,
        per_stack: 0.20,
        max_stacks: 5,
        duration: crate::loadout::NO_TIMEOUT,
        chance: 1.0,
        initial_stacks: 0,
        stacks_per_trigger: 1,
        per_shell: false,
        cleared_by: crate::loadout::ClearedBy::MagazineRefilled,
        card_opens_full: false,
    };
    // 21 rounds = seven whole bursts; the cap is five, so a magazine that
    // long reaches it and sits there. The reload is long enough that the
    // reset is unambiguous in the trace.
    let p = FightParams {
        fire_rate: 10.0,
        magazine_size: 21.0,
        reload_seconds: 2.0,
        burst: Some(crate::weapons_data::BurstSpec { count: 3, delay_seconds: 0.0 }),
        stacking_buffs: vec![buff],
        duration_seconds: 10.0,
        ..no_status()
    };
    let mut rng = Rng::new(7);
    let r = run_once(&p, &mut rng);
    assert!(r.reloads >= 1, "the fixture has to reload at least once");

    // THE TRACE IS WHAT SAYS WHEN. `replay` seeds the roster from the
    // params — a hand-built `Replay` would have an empty one and the frames
    // would carry no stacks to read.
    let trace = replay(&p, Rng::new(7).state(), 600);
    let i = trace
        .buffs
        .iter()
        .position(|x| x.id == "full_burst_damage")
        .expect("the buff is on the roster");
    let series: Vec<u16> = trace.frames.iter().map(|f| f.stacks[i]).collect();
    assert!(series.contains(&5), "it reaches the cap: {series:?}");
    assert!(series.iter().all(|&v| v <= 5), "and never passes it: {series:?}");
    // RESET: the pile comes back DOWN to zero, which only the refill can do
    // — there is no clock on this buff.
    assert!(
        series.windows(2).any(|w| w[0] > 0 && w[1] == 0),
        "the refill takes the whole pile: {series:?}"
    );
    // ONE STACK PER BURST, and the arithmetic is the assertion. A burst
    // weapon's `fire_rate` is BURSTS per second (wiki), so 10 is ten bursts
    // — thirty rounds — a second. The rounds of a pull land together (this
    // fixture's delay is zero) and pulls start 0.1 s apart from t = 0, so
    // the fifth burst completes at 0.4 s: frame 24-25 at 1/60 s. Per ROUND
    // instead of per burst would have reached it in a third of that.
    let first_cap = series.iter().position(|&v| v == 5).expect("reaches 5");
    let at = first_cap as f64 * trace.frame_seconds;
    assert!(
        at > 0.38 && at < 0.45,
        "the fifth burst at ten bursts a second is 0.4 s: capped at {at:.3} s (frame {first_cap})"
    );
}

/// ON RELOAD FROM EMPTY, on the two cards that grant different stats — and
/// the one moment that tells this trigger apart from a plain reload.
///
/// The Soma's Fresh Havoc is "+6 Base Damage, stacks up to 2x", and the
/// Zylok's Mauler's Magazine "+1x Base Critical Damage Multiplier, stacks
/// up to 2x". Both are held for the mission — "Buff lasts permanently
/// throughout the mission but is lost on death" — so nothing here takes the
/// pile, which is asserted rather than assumed: a `cleared_by` that fired
/// would cap the run at one stack and still look like it worked.
///
/// THE CONVERSIONS ARE THE OTHER HALF. "+6" is a flat base add and "+1x" a
/// BASE crit multiplier, so both change units at `resolve` — the flat one
/// into the share of the base-damage bucket worth the same, the crit one
/// into the post-mod multiplier. Asserted against the card's own arithmetic:
/// the Soma's "+96 in Incarnon Form" is 6 x 2 stacks x 8 pellets.
#[test]
fn on_reload_from_empty_pays_both_cards_and_only_from_empty() {
    let buffs = |weapon: &str, evo: &str| {
        let base = crate::loadout::WeaponBase::from_data(weapon, false, &[evo]);
        crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent)
            .stacking_buffs
    };

    // ---- the Soma: a FLAT base add, on the reload-from-empty trigger ----
    let sb = buffs("soma", "soma_fresh_havoc");
    assert_eq!(sb.len(), 1, "one buff on the card: {sb:?}");
    assert_eq!(sb[0].trigger, crate::loadout::BuffTrigger::ReloadFromEmpty);
    assert_eq!(sb[0].grant, crate::loadout::BuffGrant::FlatBaseDamage);
    assert_eq!(sb[0].max_stacks, 2);
    assert_eq!(sb[0].cleared_by, crate::loadout::ClearedBy::Nothing,
        "the card says it lasts the mission");
    assert_eq!(sb[0].duration, crate::loadout::NO_TIMEOUT, "and has no clock");
    // +6 on a weapon whose unmodded base is `total`, expressed as the share
    // of the base-damage bucket worth the same — unmodded, that is 6/total.
    let soma_base = crate::loadout::WeaponBase::from_data("soma", false, &[]);
    let want = 6.0 / soma_base.base_vector.total();
    assert!((sb[0].per_stack - want).abs() < 1e-9,
        "a flat +6 is {want} of an unmodded {} base, got {}",
        soma_base.base_vector.total(), sb[0].per_stack);

    // …AND THE CARD OPENS FULL. One of the short, closed list the owner
    // exempts from the earned-at-zero rule because keeping it up is not
    // something a player has to think about — a DECISION,
    // named per buff in docs/BUFFS.md, and not something the card text or
    // the buff's shape could decide.
    assert!(sb[0].card_opens_full, "the allowance is granted and the data has to carry it");

    // NOTHING ELSE IS ON THE LIST BY RESEMBLANCE. Twenty buffs reach the
    // same arm with no clock and nothing that clears them — the default for
    // a card that states neither — so this asserts the difference between
    // being named and merely sharing the shape. Reaver's Rapture is one of
    // the eighteen.
    let quiet = buffs("burston", "burston_reavers_rapture");
    assert!(!quiet.is_empty(), "the fixture has to carry a buff");
    assert!(
        quiet.iter().all(|b| !b.card_opens_full),
        "a buff nobody put on the list must not open full: {quiet:?}"
    );

    // ---- the Zylok: a BASE crit-damage add, and the mods multiply it ----
    let zb = buffs("zylok", "zylok_maulers_magazine");
    let z = zb.iter().find(|b| b.grant == crate::loadout::BuffGrant::BaseCritDamage)
        .expect("the crit-damage half of the card");
    assert_eq!(z.trigger, crate::loadout::BuffTrigger::ReloadFromEmpty);
    assert_eq!(z.max_stacks, 2);
    assert!((z.per_stack - 1.0).abs() < 1e-9, "unmodded, +1x stays +1x: {}", z.per_stack);
    // …and WITH a crit-damage mod it is worth more, which is what "Base"
    // buys. Unmodded and modded are the same number for any other reading.
    let vs = crate::mods_data::class_pool("pistol").into_iter()
        .find(|m| m.id == "primed_target_cracker").expect("primed_target_cracker");
    let base = crate::loadout::WeaponBase::from_data("zylok", false, &["zylok_maulers_magazine"]);
    let modded = crate::loadout::resolve(&base, &[&vs], crate::loadout::StackPolicy::Emergent);
    let zm = modded.stacking_buffs.iter()
        .find(|b| b.grant == crate::loadout::BuffGrant::BaseCritDamage).expect("still there");
    let cd_mod = modded.crit_damage / base.base_crit_damage;
    assert!((zm.per_stack - cd_mod).abs() < 1e-6,
        "+1x BASE through a x{cd_mod} crit-damage bucket is worth that much: {}", zm.per_stack);

    // ---- …AND THE INCARNON FORM RUNNING DRY PAYS NOTHING ----
    //
    // The trigger is a reload from EMPTY, and an Incarnon form does not
    // reload: it fires a CHARGE pool, so emptying it is not a magazine
    // event and cannot earn a stack. What CAN pay is
    // the transform IN, and only from empty — "Switching to Incarnon Form
    // from empty will also trigger the buff" — which is a different moment
    // with a different question about the base magazine.
    //
    // A run that opens primed and never earns the form back therefore pays
    // exactly once for its whole Incarnon magazine: at the exit, nothing,
    // and afterwards only what the BASE form's own empty reloads earn.
    {
        let base_form = FightParams {
            fire_rate: 10.0,
            magazine_size: 1_000.0, // deep enough never to reload
            reload_seconds: 0.5,
            ..no_status()
        };
        let p = FightParams {
            fire_rate: 10.0,
            magazine_size: 20.0, // the CHARGE pool, spent and not reloaded
            ammo_efficiency_applies: false,
            infinite_reserve: true,
            stacking_buffs: vec![crate::loadout::StackingBuff {
                id: "on_empty_reload_damage", ..sb[0]
            }],
            duration_seconds: 8.0,
            cycle: Some(IncarnonCycle {
                starts_primed: true,
                base_form: Box::new(base_form),
                arms: Arms::Gauge { charge_on: crate::loadout::ChargeOn::WeakpointHits, charges_to_fill: 1_000_000 },
                ends: Ends::ChargeMagazine,
                transmute_out_seconds: 0.5,
                transmute_seconds: 1.0,
                reload_bucket: 0.0,
            }),
            ..no_status()
        };
        let trace = replay(&p, Rng::new(7).state(), 600);
        let i = trace.buffs.iter().position(|x| x.id == "on_empty_reload_damage")
            .expect("on the roster");
        let series: Vec<u16> = trace.frames.iter().map(|f| f.stacks[i]).collect();
        let r = run_once(&p, &mut Rng::new(7));
        assert_eq!(r.reloads, 0, "neither form reloads in this fixture");
        assert!(
            series.iter().all(|&v| v == 0),
            "an Incarnon form spends CHARGES, not a magazine — running it dry                  must earn nothing: {series:?}"
        );
    }

    // ---- it climbs to its cap over reloads, and NOTHING takes it back ----
    let p = FightParams {
        fire_rate: 10.0,
        magazine_size: 5.0,
        reload_seconds: 0.5,
        stacking_buffs: vec![crate::loadout::StackingBuff {
            id: "on_empty_reload_damage", ..sb[0]
        }],
        duration_seconds: 10.0,
        ..no_status()
    };
    let trace = replay(&p, Rng::new(7).state(), 600);
    let i = trace.buffs.iter().position(|x| x.id == "on_empty_reload_damage")
        .expect("on the roster");
    let series: Vec<u16> = trace.frames.iter().map(|f| f.stacks[i]).collect();
    assert_eq!(series[0], 0, "it opens empty — the fight earns it");
    assert!(series.contains(&2), "two reloads reach the cap: {series:?}");
    assert!(series.iter().all(|&v| v <= 2), "and never pass it");
    assert!(!series.windows(2).any(|w| w[0] > w[1]),
        "nothing takes the pile back — it lasts the mission: {series:?}");

    // ---- and the crit half REACHES THE DAMAGE, not just the panel ----
    // A crit-damage grant is invisible unless the weapon crits, so the
    // fixture crits every shot and the buff is the only difference.
    let crit_p = |b: Vec<crate::loadout::StackingBuff>| FightParams {
        base_crit_chance: 1.0,
        unmodded_crit_chance: 1.0,
        crit_multiplier: 2.0,
        unmodded_crit_damage: 2.0,
        fire_rate: 10.0,
        magazine_size: 5.0,
        reload_seconds: 0.5,
        stacking_buffs: b,
        duration_seconds: 10.0,
        ..no_status()
    };
    let z_buff = crate::loadout::StackingBuff { id: "on_empty_reload_crit_damage", ..*z };
    let with = monte_carlo(&crit_p(vec![z_buff]), 1, 3).mean_damage;
    let without = monte_carlo(&crit_p(vec![]), 1, 3).mean_damage;
    assert!(with > without * 1.10,
        "+1x/+2x base crit damage on a 2x weapon is worth a lot: {with} vs {without}");
}

/// THE ONE MOMENT `ReloadFromEmpty` IS NOT `ReloadComplete`.
///
/// Entering the Incarnon form fully reloads the base magazine whether or not
/// it had run out, and the Soma's card is explicit that only the empty case
/// pays: "Switching to Incarnon Form from empty will ALSO trigger the buff".
/// So a transform on a magazine with rounds left must earn nothing, where a
/// plain reload trigger would earn a stack every cycle.
///
/// The fixture is built so the two ANSWER DIFFERENTLY: the gauge fills in
/// two hits and the base magazine holds ten, so every transform happens with
/// eight rounds still in it. Both buffs are run in the same fight, so the
/// difference cannot be a fixture accident.
#[test]
fn a_transform_on_a_full_magazine_is_not_a_reload_from_empty() {
    let head = vec![BodyPart {
        name: "head".into(), aim_weight: 1.0, multiplier: 1.0,
        is_head: true, crit_bonus: false,
    }];
    let buff = |id: &'static str, trigger| crate::loadout::StackingBuff {
        id,
        trigger,
        grant: crate::loadout::BuffGrant::BaseDamage,
        decay: crate::loadout::BuffDecay::LoseOneAndReset,
        per_stack: 0.10,
        max_stacks: 9,
        duration: crate::loadout::NO_TIMEOUT,
        chance: 1.0,
        initial_stacks: 0,
        stacks_per_trigger: 1,
        per_shell: false,
        cleared_by: crate::loadout::ClearedBy::Nothing,
        card_opens_full: false,
    };
    let base_form = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 50.0),
        magazine_size: 10.0,
        body_parts: head.clone(),
        ..no_status()
    };
    let p = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 100.0),
        magazine_size: 2.0,
        body_parts: head,
        fire_rate: 10.0,
        reload_seconds: 0.5,
        stacking_buffs: vec![
            buff("on_empty_reload_damage", crate::loadout::BuffTrigger::ReloadFromEmpty),
            buff("on_reload_damage", crate::loadout::BuffTrigger::ReloadComplete),
        ],
        cycle: Some(IncarnonCycle {
            starts_primed: false,
            base_form: Box::new(base_form),
            arms: Arms::Gauge { charge_on: crate::loadout::ChargeOn::WeakpointHits, charges_to_fill: 2 },
            ends: Ends::ChargeMagazine,
            transmute_out_seconds: 0.5,
            transmute_seconds: 1.0,
            reload_bucket: 0.0,
        }),
        duration_seconds: 12.0,
        ..no_status()
    };
    let trace = replay(&p, Rng::new(9).state(), 900);
    let peak = |id: &str| {
        let i = trace.buffs.iter().position(|x| x.id == id).expect(id);
        trace.frames.iter().map(|f| f.stacks[i]).max().unwrap_or(0)
    };
    let (from_empty, on_reload) = (peak("on_empty_reload_damage"), peak("on_reload_damage"));
    assert!(on_reload > 0, "the fixture has to transform at all: {on_reload}");
    assert_eq!(from_empty, 0,
        "every transform here happens on eight rounds — none is a reload from \
         empty, yet it earned {from_empty} (the plain reload trigger earned {on_reload})");
}

/// KING'S GAMBIT: a body shot cannot crit, and a weak point crits more.
///
/// VERBATIM (Sicarus_Incarnon_Genesis), the bullet and the two notes that
/// name its brackets:
///   *'''x0''' [[Critical Chance]] on Bodyshots, '''+150%''' Critical Chance on
///    Weakpoint Hits.
///   * Bodyshot modifier is multiplicative with all sources of Critical
///     Chance, effectively making non-headshot critical hits impossible.
///   * Weakpoint modifier is additive with mods such as Pistol Gambit
///
/// "Effectively making non-headshot critical hits impossible" is the sharp
/// claim and it is asserted against a build carrying a crit-chance MOD: a
/// bucket term would be cancelled by enough crit chance, a multiplier
/// cannot, and only the second reading survives a Primed Pistol Gambit.
#[test]
fn kings_gambit_kills_body_crits_and_pays_the_weak_point_additively() {
    let perk = ["sicarus_prime_evo1_incarnon_form", "sicarus_prime_kings_gambit"];
    let panel = |evo: &[&str], mods: &[&crate::loadout::ModDef]| {
        let base = crate::loadout::WeaponBase::from_data("sicarus_prime", false, evo);
        crate::loadout::resolve(&base, mods, crate::loadout::StackPolicy::AssumedMax)
    };
    let pool = crate::mods_data::class_pool("pistol");
    let pg = pool.iter().find(|m| m.id == "primed_pistol_gambit").expect("primed_pistol_gambit");

    // THE PANEL IS UNTOUCHED. Both halves are decided by where a pellet
    // landed, so neither is a panel number — which is also what makes
    // Wiseman's Regard ignore this perk, as the same page says.
    let bare = panel(&[], &[]);
    let with = panel(&perk, &[]);
    assert!((bare.crit_chance - with.crit_chance).abs() < 1e-9,
        "the panel's crit chance does not move: {} vs {}", bare.crit_chance, with.crit_chance);
    assert!((with.weakpoint_crit_chance_relative - 1.50).abs() < 1e-9,
        "the weak-point half seeds the bucket the crit mods write to: {}",
        with.weakpoint_crit_chance_relative);
    assert!((with.bodyshot_crit_chance_multiplier - 0.0).abs() < 1e-9, "x0: {}", with.bodyshot_crit_chance_multiplier);
    assert!((panel(&[], &[]).bodyshot_crit_chance_multiplier - 1.0).abs() < 1e-9, "without the perk, ordinary");

    // ADDITIVE WITH THE MODS: Primed Pistol Gambit is +187%, so the weak
    // point sees base x (1 + 1.87 + 1.50) and the body sees base x (1+1.87)
    // — before the x0 takes it. The two brackets are read off the panel
    // because the sim reads them from exactly there.
    let modded = panel(&perk, &[pg]);
    let b = modded.base_crit_chance;
    let want_wp = modded.crit_chance + b * 1.50;
    assert!(want_wp > modded.crit_chance, "the weak point is worth more");

    // …AND IN THE FIGHT. Two targets, one all head and one all body, so the
    // crit rate is a direct reading of the two branches rather than a blend.
    let run = |evo: &[&str], head: bool| {
        let base = crate::loadout::WeaponBase::from_data("sicarus_prime", false, evo);
        let panel = crate::loadout::resolve(&base, &[pg], crate::loadout::StackPolicy::AssumedMax);
        let mut p = FightParams::from_panel(
            &panel, &crate::arena::Arena::training(20.0), &ArcaneFx::none());
        p.body_parts = vec![BodyPart {
            name: (if head { "head" } else { "body" }).into(),
            aim_weight: 1.0, multiplier: 1.0, is_head: head, crit_bonus: false,
        }];
        p.duration_seconds = 20.0;
        let s = monte_carlo(&p, 8, 5);
        s.mean_crit_rate
    };
    let body_off = run(&[], false);
    assert!(body_off > 0.10, "sanity: without the perk a body shot crits ({body_off})");
    let body_on = run(&perk, false);
    assert_eq!(body_on, 0.0,
        "x0 is multiplicative, so a body crit is impossible even under Primed              Pistol Gambit — got a crit rate of {body_on}");
    let head_on = run(&perk, true);
    assert!(head_on > run(&[], true),
        "and a weak point crits MORE with the perk: {head_on} vs {}", run(&[], true));
}

/// GALVANIC RELOAD: the magazine lasts longer, ONCE PER SHOT, and only
/// while the target is carrying the status.
///
/// VERBATIM (Strun_Incarnon_Genesis) and its three notes:
///   *On hitting a target affected by an {{D|Electricity}} status, '''40%'''
///    chance to restore 1 round in the magazine from ammo pool.
///   *The status effect may originate from any source.
///   *The bonus can only apply once per enemy hit.
///   *The bonus does not affect the Incarnon form.
///
/// The second note is the one worth a test of its own: this is a SHOTGUN
/// family, so per-pellet instead of per-shot would be roughly ten rolls a
/// trigger pull and a magazine that never empties. It is checked by
/// counting RELOADS — the observable a player would notice — with the
/// pellet count as the only thing that changes between two runs.
#[test]
fn galvanic_reload_restores_once_per_shot_not_once_per_pellet() {
    // A fixture that applies Electricity on every pellet, so the target is
    // always carrying one and the roll is the only variable.
    let fixture = |pellets: f64, restore: bool| {
        let mut p = FightParams {
            damage: DamageVector::new().with(DamageType::Electricity, 100.0),
            status_chance: 1.0,
            multishot: pellets,
            magazine_size: 10.0,
            fire_rate: 5.0,
            reload_seconds: 2.0,
            duration_seconds: 60.0,
            ..no_status()
        };
        if restore {
            p.round_restore_on_status = Some((DamageType::Electricity, 0.40, 1.0));
        }
        let s = monte_carlo(&p, 12, 3);
        // SHOTS PER MAGAZINE, which is the thing the perk actually changes.
        // Reload COUNT is the wrong observable here and measuring it says
        // why: a gun that reloads less also spends less time reloading, so
        // it fires more shots in the same 60 s and the count comes back up.
        s.mean_shots / s.mean_reloads.max(1.0)
    };

    // WITHOUT the perk, a 10-round magazine is 10 shots.
    let plain1 = fixture(1.0, false);
    assert!((plain1 - 10.0).abs() < 0.6, "ten rounds, ten shots: {plain1}");

    // WITH it, 40% of shots put a round back, so the magazine is worth
    // 10/(1-0.4) = 16.67 shots. The arithmetic is the assertion.
    let with1 = fixture(1.0, true);
    assert!((with1 - 16.67).abs() < 1.5,
        "a 40% refund makes a 10-round magazine 16.67 shots: {with1}");

    // AND TEN PELLETS CHANGE NOTHING, which is the note. Per pellet, ten
    // rolls a shot would refund on 99.4% of them and the magazine would
    // never empty; per shot, the pellet count is irrelevant.
    let with10 = fixture(10.0, true);
    assert!((with10 - with1).abs() < 1.5,
        "once per ENEMY HIT, so multishot does not multiply it: {with10} shots a              magazine at ten pellets against {with1} at one");

    // …AND NOTHING WITHOUT THE STATUS. Same perk, a target that never
    // catches one, so the condition is the only difference.
    let mut cold = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 100.0),
        status_chance: 0.0,
        magazine_size: 10.0,
        fire_rate: 5.0,
        reload_seconds: 2.0,
        duration_seconds: 60.0,
        ..no_status()
    };
    let dry = monte_carlo(&cold, 12, 3).mean_reloads;
    cold.round_restore_on_status = Some((DamageType::Electricity, 0.40, 1.0));
    assert!((monte_carlo(&cold, 12, 3).mean_reloads - dry).abs() < 1e-9,
        "no Electricity on the target, no refund");
}

/// CRIMSON OVERTURE, and the claim that makes `BuffTrigger::Kill` worth a
/// variant: A KILL COUNTS WHEREVER IT CAME FROM.
///
/// VERBATIM (Boltor_Incarnon_Genesis, EVO2):
///   *Increase Base Damage by '''+X'''.
///   *On Kill: Increase Base Damage by '''+2''' and '''+20%''' [[Ammo Efficiency]]
///    for '''5''' seconds. Stacks up to '''Y'''x
///   | X = 12<br>Y = 4 | X = 0<br>Y = 3 | X = 0<br>Y = 3
///
/// The +2 is in the BULLET and only X and the cap are per-variant — which
/// is where the transcription went wrong before this: the Boltor's card had
/// X in the per-stack slot and no unconditional half at all.
///
/// The trigger is read off the kill COUNTER rather than bumped at each of
/// the six sites a kill can happen, so the second half of this test kills
/// the target with a DoT and nothing else: no direct hit lands the killing
/// blow, and the stacks must still climb.
#[test]
fn on_kill_stacks_climb_from_a_kill_the_gun_did_not_land() {
    let buff = crate::loadout::StackingBuff {
        id: "on_kill_damage",
        trigger: crate::loadout::BuffTrigger::Kill,
        grant: crate::loadout::BuffGrant::BaseDamage,
        decay: crate::loadout::BuffDecay::LoseOneAndReset,
        per_stack: 0.10,
        max_stacks: 4,
        duration: 5.0,
        chance: 1.0,
        initial_stacks: 0,
        stacks_per_trigger: 1,
        per_shell: false,
        cleared_by: crate::loadout::ClearedBy::Nothing,
        card_opens_full: false,
    };
    // A target that dies to every shot and comes straight back, so kills
    // are frequent and nothing else in the fixture is doing anything.
    let p = FightParams {
        magazine_size: 100.0,
        fire_rate: 10.0,
        stacking_buffs: vec![buff],
        duration_seconds: 10.0,
        body_parts: mono_body(1.0),
        target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
        ..flat_base()
    };
    let trace = replay(&p, Rng::new(4).state(), 600);
    let i = trace.buffs.iter().position(|x| x.id == "on_kill_damage")
        .expect("on the roster");
    let series: Vec<u16> = trace.frames.iter().map(|f| f.stacks[i]).collect();
    assert_eq!(series[0], 0, "it opens empty — the fight earns it");
    assert!(series.contains(&4), "four kills reach the cap: {series:?}");
    assert!(series.iter().all(|&v| v <= 4), "and never pass it");

    // …AND A KILL THE GUN DID NOT LAND still counts. The shot does almost
    // nothing and a Slash DoT finishes the target, so every kill happens in
    // the DoT path — a trigger wired to the direct-hit site would score zero
    // here and look fine everywhere else.
    let dot = FightParams {
        damage: DamageVector::new().with(DamageType::Slash, 1.0),
        status_chance: 1.0,
        base_status_chance: 1.0,
        magazine_size: 100.0,
        fire_rate: 1.0,
        stacking_buffs: p.stacking_buffs.clone(),
        duration_seconds: 30.0,
        body_parts: mono_body(1.0),
        target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
        ..flat_base()
    };
    // The claim rests on the SHOT being harmless, so it is asserted rather
    // than reasoned about: 1 damage against 50 health, so no direct hit can
    // ever be the killing blow and every kill in this run is a DoT's.
    assert!(dot.damage.total() < dot.target.base_health,
        "the shot must not be able to kill: {} damage against {} health",
        dot.damage.total(), dot.target.base_health);
    let s = monte_carlo(&dot, 4, 11);
    assert!(s.mean_kills > 0.0, "the fixture has to kill something: {}", s.mean_kills);
    let trace = replay(&dot, Rng::new(11).state(), 1200);
    let i = trace.buffs.iter().position(|x| x.id == "on_kill_damage").expect("roster");
    let peak = trace.frames.iter().map(|f| f.stacks[i]).max().unwrap_or(0);
    assert!(peak > 0,
        "a kill counts wherever it came from — {} kills and the pile never moved",
        s.mean_kills);
}

/// EXACT PENANCE, and the note that separates it from the effect it looks
/// like: A STATUS KILL COUNTS.
///
/// VERBATIM (Lato_Incarnon_Genesis):
///   *On Kill: '''50%''' chance for Instant Reload.
///   *Kills from status effects can also trigger the effect.
///   *The bonus does not affect the Incarnon form.
///
/// `instant_reload_on_headshot` asks for a weak-point DIRECT hit and is
/// wired to the direct-hit site, so a Slash DoT kill would score nothing
/// there. This one is read off the kill counter, and the second fixture is
/// the proof: the shot deals 1 damage against 50 health, so no direct hit
/// can ever be the killing blow.
#[test]
fn exact_penance_reloads_on_a_kill_the_gun_did_not_land() {
    let build = |chance: Option<f64>, slash: bool| FightParams {
        damage: if slash {
            DamageVector::new().with(DamageType::Slash, 1.0)
        } else {
            DamageVector::new().with(DamageType::Impact, 200.0)
        },
        status_chance: if slash { 1.0 } else { 0.0 },
        base_status_chance: if slash { 1.0 } else { 0.0 },
        magazine_size: 5.0,
        ammo_cost: 1.0,
        fire_rate: 2.0,
        reload_seconds: 3.0,
        duration_seconds: 60.0,
        instant_reload_on_kill: chance,
        body_parts: mono_body(1.0),
        target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
        ..flat_base()
    };

    // GUNFIRE KILLS: an instant reload should buy shots, because the three
    // seconds a reload costs are three seconds not spent shooting.
    let bare = monte_carlo(&build(None, false), 6, 5);
    let with = monte_carlo(&build(Some(0.5), false), 6, 5);
    assert!(bare.mean_reloads > 3.0, "the fixture has to reload: {}", bare.mean_reloads);
    assert!(with.mean_shots > bare.mean_shots * 1.3,
        "an instant reload on half the kills buys shots: {} against {}",
        with.mean_shots, bare.mean_shots);

    // …AND A KILL THE GUN DID NOT LAND COUNTS. 1 damage against 50 health,
    // asserted rather than assumed, so every kill here is a Slash DoT's.
    let dot = build(None, true);
    assert!(dot.damage.total() < dot.target.base_health,
        "the shot must not be able to kill: {} against {}",
        dot.damage.total(), dot.target.base_health);
    let dot_bare = monte_carlo(&dot, 6, 7);
    assert!(dot_bare.mean_kills > 0.0, "the DoT has to kill: {}", dot_bare.mean_kills);
    let dot_with = monte_carlo(&build(Some(1.0), true), 6, 7);
    assert!(dot_with.mean_shots > dot_bare.mean_shots,
        "a DoT kill triggers it too — {} shots against {}, on {} kills",
        dot_with.mean_shots, dot_bare.mean_shots, dot_bare.mean_kills);
}

/// A GAS CLOUD'S KILL IS THE WEAPON'S — MEASUREMENTS M70.
///
/// The cloud is its own entity: it has a radius of its own, and it keeps
/// ticking on the bodies around a host that has already died. Reading that
/// as "the cloud killed it, not the gun" is the carve-out this test
/// refuses — Galvanized's stacks and the Merciless family are bumped at
/// each site a kill can happen rather than read off the counter, so a
/// gas-only kill is exactly the one a missing call site loses.
///
/// ONE SHOT IS FIRED AND IT CANNOT KILL, both asserted: 4 damage into 10
/// health, then a reload that outlasts the fight. Every kill here is a
/// cloud tick's.
#[test]
fn a_gas_cloud_kill_earns_the_on_kill_stacks() {
    let spec = crate::loadout::StackSpec {
        per_stack: 0.1,
        max_stacks: 5,
        duration: 4.0,
        initial_stacks: 0,
        earned_on: Some("kill"),
    };
    let p = FightParams {
        damage: DamageVector::new().with(DamageType::Gas, 4.0),
        status_chance: 1.0,
        base_status_chance: 1.0,
        magazine_size: 1.0,
        reload_seconds: 100.0,
        fire_rate: 1.0,
        duration_seconds: 30.0,
        multishot_stack: Some(spec),
        target: TargetParams {
            base_health: 10.0,
            ..frail_target(TargetMode::InstantRespawn, 0.0, 0.0)
        },
        ..flat_base()
    };
    let s = monte_carlo(&p, 4, 11);
    assert!((s.mean_shots - 1.0).abs() < 1e-9,
        "the magazine holds one and the reload outlasts the fight: {} shots",
        s.mean_shots);
    assert!(
        p.damage.total() * (1.0 + spec.per_stack * f64::from(spec.max_stacks))
            < p.target.base_health,
        "and that one shot cannot kill: {} against {}",
        p.damage.total(), p.target.base_health);
    assert!(s.mean_kills > 0.0, "the cloud has to kill something: {}", s.mean_kills);
    let trace = replay(&p, Rng::new(11).state(), 1200);
    let i = trace.buffs.iter().position(|x| x.id == "on_kill_multishot").expect("roster");
    let peak = trace.frames.iter().map(|f| f.stacks[i]).max().unwrap_or(0);
    assert!(peak > 0,
        "a gas kill is the weapon's — {} kills and the pile never moved",
        s.mean_kills);
}

/// A THRAX DIES TWICE, AND NO GUN FINISHES THE SECOND HALF.
///
/// With the fight's `spectral_form` switch on, the physical form falling is
/// not a kill: nothing on-kill fires, nothing drops, the body does not
/// respawn, and every instance after it — a bullet, an explosion, a DoT
/// tick — is refused. What is left is the progress term, which stalls at
/// the physical form's share of the individual.
#[test]
fn a_spectral_thrax_cannot_be_finished_by_a_weapon() {
    let build = |spectral: bool| FightParams {
        magazine_size: 100.0,
        fire_rate: 10.0,
        duration_seconds: 30.0,
        body_parts: mono_body(1.0),
        target: TargetParams {
            base_health: 10.0,
            spectral: spectral.then_some(crate::enemy_data::SpectralForm {
                health_share: 0.40,
                delay_seconds: 2.0,
            }),
            ..frail_target(TargetMode::InstantRespawn, 0.0, 0.0)
        },
        ..flat_base()
    };
    let plain = run_once(&build(false), &mut Rng::new(9));
    let ghost = run_once(&build(true), &mut Rng::new(9));
    assert!(plain.kills > 5, "the fixture has to kill: {}", plain.kills);
    // NOT ONE KILL, however long it fires — the kill waits for the spectre.
    assert_eq!(ghost.kills, 0, "a gun cannot finish a Thrax");
    assert_eq!(ghost.kills_in_reach, 0);
    assert_eq!(ghost.picked_up_ammo, 0.0, "and nothing drops until it does");
    // THE PROGRESS STALLS at what the physical form was worth: 10 health of
    // an individual carrying 10 + 4.
    let settled = monte_carlo(&build(true), 1, 9);
    assert!(
        (settled.mean_kill_progress - 10.0 / 14.0).abs() < 1e-9,
        "{}",
        settled.mean_kill_progress
    );
    // …AND THE DAMAGE STOPS THERE: everything after the physical form is
    // refused, so the run's damage is the first body's bar and no more.
    assert!(ghost.effective_damage() <= plain.effective_damage() / 5.0,
        "{} vs {}", ghost.effective_damage(), plain.effective_damage());
}

/// THE BODIES RESUPPLY THE RESERVE, and the rules that decide by how much
/// (`engine::ammo`).
///
/// A weapon that runs dry mid-fight is the only place any of this is
/// visible: with the reserve infinite — which is what every ruler is scored
/// under — a pack is worth nothing, and that is asserted here too.
#[test]
fn a_kill_resupplies_the_reserve_and_a_mutation_mod_pays_for_the_other_half() {
    // A fixture the supply binds rather than the clock: two rounds a pack,
    // a 20-round reserve, and a body that dies to every shot.
    let build = |drops: bool, conversion: f64, reach: f64| FightParams {
        magazine_size: 10.0,
        ammo_cost: 1.0,
        fire_rate: 4.0,
        reload_seconds: 0.5,
        duration_seconds: 60.0,
        infinite_reserve: false,
        reserve_ammo: 20.0,
        ammo_drops: drops,
        ammo_pickup: 2.0,
        ammo_conversion: conversion,
        ammo_class: Some(crate::ammo::Pickup::Primary),
        pickup_range_m: reach,
        squad_size: 1,
        body_parts: mono_body(1.0),
        target: TargetParams {
            base_health: 1.0,
            ..frail_target(TargetMode::InstantRespawn, 0.0, 0.0)
        },
        ..flat_base()
    };
    let run = |p: &FightParams| run_once(p, &mut Rng::new(4242));
    let starved = run(&build(false, 0.0, f64::INFINITY));
    let dropping = run(&build(true, 0.0, f64::INFINITY));
    let converting = run(&build(true, 0.92, f64::INFINITY));
    // WITHOUT DROPS the reserve is the whole engagement: 10 in the magazine
    // and 20 behind it, and not one round more.
    assert_eq!(starved.shots, 30, "the fixture has to starve");
    assert_eq!(starved.picked_up_ammo, 0.0);
    // WITH THEM it fires for longer — solo is a 45% chance a body drops,
    // and half of what falls is this weapon's own class.
    assert!(dropping.picked_up_ammo > 0.0, "a pack has to be worth something");
    assert!(dropping.shots > starved.shots, "{} vs {}", dropping.shots, starved.shots);
    // …AND A MUTATION MOD PAYS FOR THE OTHER HALF, which is the whole of
    // what that card does here: the secondary packs stop being litter.
    assert!(
        converting.picked_up_ammo > dropping.picked_up_ammo * 1.5,
        "conversion has to pay for the other half: {} vs {}",
        converting.picked_up_ammo, dropping.picked_up_ammo
    );

    // OUT OF REACH IS NOT PICKED UP. The Tenno does not walk, so a body
    // five metres off leaves its pack there for a 1 m radius.
    let far = run(&FightParams {
        target_at: crate::space::Vec2::new(0.0, 5.0),
        ..build(true, 0.92, 1.0)
    });
    assert_eq!(far.picked_up_ammo, 0.0, "a pack out of reach pays nothing");
    assert_eq!(far.shots, starved.shots);

    // AN INFINITE RESERVE IS ALREADY EVERYTHING, so a pack is worth nothing
    // and the fight is identical with drops on and off.
    let infinite = |drops: bool| FightParams {
        infinite_reserve: true,
        ..build(drops, 0.92, f64::INFINITY)
    };
    assert_eq!(run(&infinite(true)).shots, run(&infinite(false)).shots);
    assert_eq!(run(&infinite(true)).picked_up_ammo, 0.0);
}

/// RESONANT RESTORE: the magazine GROWS, up to the cap, and it does not
/// FILL.
///
/// VERBATIM (Gorgon_Incarnon_Genesis): "On Reload From Empty: Increase Base
/// Magazine Capacity by '''+15'''. Stacks up to '''3'''x." Three families carry the
/// card — the Atomos at +5/7x, the three Gorgons at +15/3x, the Stug at
/// +10/3x — and none of them puts a clock on it.
///
/// Measured as SHOTS PER MAGAZINE across the run, which is the observable
/// and the one that catches the cap: a 10-round magazine growing by 15
/// three times is 55 rounds and never 70.
#[test]
fn resonant_restore_grows_the_magazine_to_its_cap_and_stops() {
    let build = |growth: Option<(f64, u32)>| FightParams {
        magazine_size: 10.0,
        ammo_cost: 1.0,
        fire_rate: 20.0,
        reload_seconds: 1.0,
        duration_seconds: 60.0,
        magazine_growth_on_empty_reload: growth,
        body_parts: mono_body(1.0),
        ..flat_base()
    };
    let bare = run_once(&build(None), &mut Rng::new(3));
    assert!(bare.reloads > 5, "the fixture has to reload: {}", bare.reloads);

    // FOUR RELOADS IN AND THE MAGAZINE IS 55, not 70: the first reload pays
    // the first stack, and the fourth pays nothing.
    let grown = run_once(&build(Some((15.0, 3))), &mut Rng::new(3));
    assert!(grown.reloads < bare.reloads,
        "a bigger magazine reloads less: {} against {}", grown.reloads, bare.reloads);
    // Shots per magazine averages over the climb, so it lands between the
    // starting 10 and the capped 55 — and ABOVE the uncapped average would
    // be if nothing stopped it. The cap is asserted separately below.
    let per_mag = grown.shots as f64 / grown.reloads.max(1) as f64;
    assert!(per_mag > 10.0, "it grows: {per_mag} shots a magazine");

    // THE CAP IS REAL. Run long enough that an uncapped version would be
    // far past 55, and compare against one whose cap is 3 either way: the
    // only difference is the number of stacks allowed.
    let long = |max: u32| {
        let r = run_once(&FightParams { duration_seconds: 300.0, ..build(Some((15.0, max))) },
            &mut Rng::new(3));
        r.shots as f64 / r.reloads.max(1) as f64
    };
    let capped = long(3);
    let higher = long(9);
    assert!(higher > capped * 1.5,
        "a higher cap must be worth more, or the cap is not being read: {higher} vs {capped}");
    assert!(capped < 55.0,
        "10 + 3 x 15 = 55 is the ceiling, and the average is under it: {capped}");

    // …AND IT DOES NOT FILL. Growing the capacity mid-fight must not hand
    // the weapon free rounds: the reload still draws from the reserve, so a
    // FINITE one runs out at the same total either way.
    let finite = |growth| {
        let mut p = build(growth);
        p.infinite_reserve = false;
        p.reserve_ammo = 60.0;
        p.duration_seconds = 600.0;
        run_once(&p, &mut Rng::new(3)).shots
    };
    assert_eq!(finite(None), finite(Some((15.0, 3))),
        "a bigger magazine is not more ammo — 60 rounds is 60 shots either way");
}

/// SEQUENTIAL SKULLBUSTER: a streak the next shot can undo, in the ADDITIVE
/// headshot bracket.
///
/// VERBATIM (Onos, EVO5): "On Consecutive Weakpoint Hits: '''+30%''' Headshot
/// Damage. Stacks up to '''4x'''", with the page's Notes adding "Evolution V,
/// Sequential Skullbuster, does not affect Incarnon mode."
///
/// Two things are asserted because each can be wrong alone: the pile must
/// CLIMB while every shot lands on the head and must be taken WHOLE by one
/// body shot — not decay, not lose one — and the grant must land in the
/// additive bracket rather than multiplying it, which only shows on a build
/// that already has a headshot bonus.
#[test]
fn sequential_skullbuster_is_a_streak_and_lands_in_the_additive_bracket() {
    let buff = crate::loadout::StackingBuff {
        id: "on_weakpoint_streak_headshot_damage",
        trigger: crate::loadout::BuffTrigger::ConsecutiveHeadshot,
        grant: crate::loadout::BuffGrant::HeadshotDamage,
        decay: crate::loadout::BuffDecay::LoseOneAndReset,
        per_stack: 0.30,
        max_stacks: 4,
        duration: crate::loadout::NO_TIMEOUT,
        chance: 1.0,
        initial_stacks: 0,
        stacks_per_trigger: 1,
        per_shell: false,
        cleared_by: crate::loadout::ClearedBy::Nothing,
        card_opens_full: false,
    };
    // Every shot on the head: the streak is never broken, so it climbs and
    // sits at the cap.
    let all_head = FightParams {
        fire_rate: 10.0,
        magazine_size: 100.0,
        stacking_buffs: vec![buff],
        duration_seconds: 5.0,
        body_parts: vec![BodyPart {
            name: "head".into(), aim_weight: 1.0, multiplier: 2.0,
            is_head: true, crit_bonus: false,
        }],
        ..flat_base()
    };
    let trace = replay(&all_head, Rng::new(5).state(), 300);
    let i = trace.buffs.iter()
        .position(|x| x.id == "on_weakpoint_streak_headshot_damage")
        .expect("on the roster");
    let series: Vec<u16> = trace.frames.iter().map(|f| f.stacks[i]).collect();
    assert_eq!(series[0], 0, "it opens empty");
    assert!(series.contains(&4), "four weak-point hits reach the cap: {series:?}");
    assert!(series.iter().all(|&v| v <= 4), "and never pass it");

    // A BODY SHOT TAKES THE WHOLE PILE, which is what makes this a streak
    // rather than a timed buff: half the shots land on the body, so the
    // count can never get far and never sits at the cap.
    let mixed = FightParams {
        body_parts: vec![
            BodyPart { name: "head".into(), aim_weight: 1.0, multiplier: 2.0,
                       is_head: true, crit_bonus: false },
            BodyPart { name: "body".into(), aim_weight: 1.0, multiplier: 1.0,
                       is_head: false, crit_bonus: false },
        ],
        ..all_head.clone()
    };
    let mtrace = replay(&mixed, Rng::new(5).state(), 300);
    let j = mtrace.buffs.iter()
        .position(|x| x.id == "on_weakpoint_streak_headshot_damage").expect("roster");
    let mseries: Vec<u16> = mtrace.frames.iter().map(|f| f.stacks[j]).collect();
    assert!(mseries.iter().any(|&v| v > 0), "it still climbs sometimes: {mseries:?}");
    assert!(mseries.windows(2).any(|w| w[0] > 1 && w[1] == 0),
        "a body shot takes the WHOLE pile, not one stack: {mseries:?}");

    // …AND THE BRACKET. The grant is additive with an innate headshot
    // bonus, so a weapon carrying +100% of its own sees 2x(1 + 1.0 + 1.2)
    // and not 2x2.0x2.2. The difference is the whole reading.
    let dmg = |innate: f64, perk: bool| {
        let mut p = FightParams {
            headshot_damage_bonus: innate,
            stacking_buffs: if perk { all_head.stacking_buffs.clone() } else { vec![] },
            ..all_head.clone()
        };
        p.duration_seconds = 5.0;
        monte_carlo(&p, 1, 5).mean_damage
    };
    let (plain, with) = (dmg(1.0, false), dmg(1.0, true));
    assert!(with > plain, "the perk is worth something: {with} vs {plain}");
    // Additive: the ceiling is 1 + 1.0 + 1.2 = 3.2 against 1 + 1.0 = 2.0,
    // a factor of 1.6. Multiplicative would be 2.0 x 2.2 / 2.0 = 2.2. The
    // run averages over the climb, so it must land UNDER the additive
    // ceiling and nowhere near the multiplicative one.
    let ratio = with / plain;
    assert!(ratio < 1.61,
        "additive puts the ceiling at 1.6x; multiplicative would be 2.2x — got {ratio}");
}

/// "CURRENT CRITICAL CHANCE" IS CURRENT AT THE SHOT, not at the arsenal.
///
/// Wiseman's Regard reads "30% of current Critical Chance", and its row
/// names four LIVE sources among the things that feed it — both parts of
/// Galvanized Crosshairs while aiming, Secondary Outburst, Cascadia
/// Overcharge, Secondary Enervate — none of which is on the panel. The rule: anything landing on the WEAPON's own crit chance
/// counts, and it counts LIVE.
///
/// So the panel keeps showing its static answer — that is what a panel can
/// say — and the sim takes that back and pays what the shot actually earns.
///
/// Sicarus Prime, base crit 0.25, base status 0.20, no mods:
///   panel                       crit 0.25   status 0.275 = 0.20 + 0.30 x 0.25
///   + 100% arcane crit          crit 0.50   status 0.350 = 0.275 - 0.075 + 0.150
///   + 300% (Cascadia r5)        crit 1.00   status 0.500 = the 40% cap
#[test]
fn a_derived_stat_reads_the_crit_chance_the_shot_has() {
    let evos = ["sicarus_prime_evo1_incarnon_form", "sicarus_prime_wisemans_regard"];
    let base = crate::loadout::WeaponBase::from_data("sicarus_prime", false, &evos);
    let panel = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::AssumedMax);
    // THE PANEL IS UNCHANGED by making the sim live — a static view still
    // answers with the crit chance it can see.
    assert!((panel.status_chance - 0.275).abs() < 1e-9, "{}", panel.status_chance);
    assert!((panel.crit_chance - 0.25).abs() < 1e-9, "{}", panel.crit_chance);

    let arena = crate::arena::Arena::training(60.0);
    let rate = |crit_chance_relative: f64| {
        let mut p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
        p.arcane.crit_chance_relative = crit_chance_relative;
        p.duration_seconds = 60.0;
        let s = monte_carlo(&p, 12, 4);
        s.mean_procs / s.mean_pellets.max(1.0)
    };
    let (none, one, three) = (rate(0.0), rate(1.0), rate(3.0));
    assert!(one > none && three > one,
        "arcane crit must raise the status rate: {none} -> {one} -> {three}");

    // THE ARITHMETIC, as a RATIO so the multi-proc bookkeeping cancels.
    // +100% arcane crit: effective 0.50, derived 0.150, status 0.350.
    let r1 = one / none;
    assert!((r1 - 0.350 / 0.275).abs() < 0.05,
        "0.275 -> 0.350 is x{:.3}; the rate moved x{r1:.3}", 0.350 / 0.275);

    // THE CAP IS THE SHARP ONE, and the wiki states where it lands: "+434%
    // (Prime) modded Critical Chance". 0.25 x 5.34 = 1.335, and 0.30 of
    // that is 0.4005 — just over the 40% ceiling. So +434% and +900% are
    // two very different crit chances that must produce the SAME status,
    // which no un-capped implementation can do.
    let (at_cap, far_past) = (rate(4.34), rate(9.0));
    assert!((far_past - at_cap).abs() < 0.02,
        "the wiki puts the cap at +434% on the Prime, so +900% buys nothing              more: {at_cap} vs {far_past}");
    // …and just UNDER it still climbs, or the cap is being applied too early.
    let under = rate(3.0);
    assert!(at_cap > under + 0.01,
        "+300% is below the cap (0.30 of 1.00 = 0.30 < 0.40), so +434% must              still be worth something: {under} -> {at_cap}");
}

/// DOUBLE TAP, against the card's own worked example.
///
/// VERBATIM (Double_Tap, Notes): "The bonus is applied on hit to all pellets
/// as damage * 20% * (hits - 1), meaning without multishot, the bonus isn't
/// applied until the second hit. With multishot, not only does the bonus
/// ramp up faster, but also some of the post hit bonus is applied to the
/// hit. For example, with a modded multishot of 3, the first trigger pull
/// would do +40% bonus damage, the second +100%, the third +160%."
///
/// Three numbers, and they pin every part of the reading at once: the count
/// includes THIS pull's hits (3 pellets on the first pull is 20% x (3-1) =
/// 40%), every pellet of a pull takes the SAME bonus rather than ramping
/// inside it, and a pull's hits are its pellets.
#[test]
fn double_tap_pays_the_cards_own_worked_example() {
    let frame_seconds = crate::mods_data::class_pool("rifle")
        .into_iter().find(|m| m.id == "double_tap").expect("double_tap in the rifle pool");
    assert_eq!(frame_seconds.effects.len(), 1, "one effect on the card: {:?}", frame_seconds.effects);
    let (per, cap, dur) = match frame_seconds.effects[0] {
        crate::loadout::ModEffect::ConsecutiveHitDamage { per_stack, max_stacks, duration } =>
            (per_stack, max_stacks, duration),
        ref other => panic!("wrong kind: {other:?}"),
    };
    assert!((per - 0.20).abs() < 1e-9 && cap == 20 && (dur - 2.0).abs() < 1e-9,
        "rank 3 is +20% a stack, 20 stacks, 2s: {per}/{cap}/{dur}");
    // 20 x 20% = +400%, which the card states as the maximum.
    assert!((per * f64::from(cap) - 4.0).abs() < 1e-9);

    // A fixture where nothing but Double Tap moves the number: no crit, no
    // status, one body part, and a target that cannot die.
    let build = |multishot: f64, on: bool, shots: f64| FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 100.0),
        multishot,
        base_multishot: multishot,
        fire_rate: 1.0,
        magazine_size: 100.0,
        consecutive_hit_damage: if on { Some((per, cap, dur)) } else { None },
        // The window is 2s and the cadence 1/s, so the pile never lapses.
        duration_seconds: shots - 0.5,
        body_parts: mono_body(1.0),
        ..flat_base()
    };
    let dmg = |multishot: f64, on: bool, shots: f64|
        monte_carlo(&build(multishot, on, shots), 1, 3).mean_damage;

    // WITHOUT MULTISHOT the first shot pays nothing — 20% x (1-1) = 0.
    assert!((dmg(1.0, true, 1.0) - dmg(1.0, false, 1.0)).abs() < 1e-6,
        "the first hit of an unmodded weapon earns the stack and does not use it");

    // WITH MULTISHOT 3, the card's three numbers. Cumulative, because that
    // is what a run reports: 1.4, then 1.4+2.0 = 3.4, then +2.6 = 6.0,
    // against a flat 1, 2, 3 without the mod.
    for (shots, want) in [(1.0, 1.4 / 1.0), (2.0, 3.4 / 2.0), (3.0, 6.0 / 3.0)] {
        let got = dmg(3.0, true, shots) / dmg(3.0, false, shots);
        assert!((got - want).abs() < 0.01,
            "{shots} pulls at multishot 3: the card says x{want:.2} cumulative, got x{got:.3}");
    }

    // AND IT CAPS. 20 stacks is +400%, so a long run cannot exceed 5x, and
    // a 30-shot run at multishot 3 is well past the ceiling.
    let long = dmg(3.0, true, 30.0) / dmg(3.0, false, 30.0);
    assert!(long < 5.0, "the ceiling is 5x (+400%), got x{long:.3}");
    assert!(long > 4.0, "and a long run should be near it, got x{long:.3}");
}

/// VICIOUS PROMISE: the first arrow only, and OVERGUARD does not count.
///
/// VERBATIM (wiki, Paris Incarnon Genesis): "Enemies are undamaged as long
/// as their health and shield have not been damaged. Damaging Overguard is
/// not taken into account." That exclusion is the assertion worth writing:
/// reading all three pools would switch the perk off on the first shot of
/// every Eximus fight, and the difference is invisible against a target
/// with no overguard at all.
#[test]
fn vicious_promise_reads_health_and_shield_and_ignores_overguard() {
    let arena = crate::arena::Arena::training(60.0);
    let panel = |evo: &[&str]| {
        let base = crate::loadout::WeaponBase::from_data("paris_prime", true, evo);
        crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent)
    };
    let perk = ["paris_prime_vicious_promise"];

    // THE CONVERSION: "+40% BASE crit chance" with no mods is 0.40, and the
    // grant is the post-mod number so an unmodded panel carries it whole.
    let p = panel(&perk);
    assert!((p.crit_chance_on_undamaged - 0.40).abs() < 1e-9, "{}", p.crit_chance_on_undamaged);
    assert!((p.crit_damage_on_undamaged - 2.0).abs() < 1e-9, "{}", p.crit_damage_on_undamaged);
    assert_eq!(panel(&[]).crit_chance_on_undamaged, 0.0, "no perk, no grant");

    // …AND IT REACHES THE FIGHT.
    let crit = |evo: &[&str]| {
        let p = FightParams::from_panel(&panel(evo), &arena, &ArcaneFx::none());
        monte_carlo(&p, 24, 0x71C).mean_crit_rate
    };
    assert!(crit(&perk) > crit(&[]) + 0.05, "an untouched target crits more");

    // THE OVERGUARD EXCLUSION, asserted on the predicate itself. Three
    // states of one target: whole, chewed through the overguard, and hit
    // for real. Only the last one ends the perk.
    let tp = TargetParams { base_overguard: 5_000.0, ..TargetParams::training_dummy() };
    let whole = TargetState::spawn(&tp, crate::space::Vec2::ORIGIN);
    assert!(target_undamaged(&whole, &tp), "a fresh target is undamaged");

    let mut chewed = whole.clone();
    chewed.overguard = 1.0;
    assert!(
        target_undamaged(&chewed, &tp),
        "overguard is not taken into account — a target down to its last point of it is still undamaged"
    );

    let mut hurt = whole.clone();
    hurt.health -= 1.0;
    assert!(!target_undamaged(&hurt, &tp), "one point of health ends it");
    let mut stripped = whole.clone();
    stripped.shield -= 1.0;
    assert!(!target_undamaged(&stripped, &tp), "…and so does one point of shield");
}

/// THE BELOW-HALF-HEALTH BONUS GOES WHEREVER CO GOES, which is one rule
/// rather than the two the cards read like.
///
/// VERBATIM (wiki, Kunai Incarnon Genesis, Swift Conclusion): *"Damage
/// bonus if enemy has less than half health is additive with Hornet Strike
/// in basic Kunai form, and multiplicative in Incarnon form. It is also
/// additive with Galvanized Shot in both forms."*
///
/// Galvanized Shot IS the CO bonus, and the Kunai's two forms are exactly
/// the two CO classes — Adding on the base, Multiplying on the Incarnon.
/// So "additive with Hornet Strike here, multiplicative there, additive
/// with CO always" is the same sentence as "it lands in the CO bracket".
///
/// Asserted as ARITHMETIC on one weapon whose two forms differ, because
/// that is the only place the two readings give different numbers.
#[test]
fn the_half_health_bonus_lands_in_the_weapons_own_co_bracket() {
    // 200% below half health on both Kunai forms; base form Adding,
    // Incarnon form Multiplying (CATALOGS.md).
    let base = crate::loadout::WeaponBase::from_data("kunai", true, &["kunai_swift_conclusion"]);
    let inc = crate::loadout::WeaponBase::from_data(
        "kunai_incarnon", true, &["kunai_swift_conclusion"]);
    assert_eq!(base.co_behavior, crate::loadout::CoBehavior::AdditiveWithBaseDamage);
    assert_eq!(inc.co_behavior, crate::loadout::CoBehavior::Independent);
    assert!(base.base_damage_below_half_health > 1.0 && inc.base_damage_below_half_health > 1.0);

    // A target that is ALWAYS below half health, so the term is always on,
    // and a mod bucket big enough to tell the two brackets apart.
    let arena = crate::arena::Arena::training(60.0);
    let dmg = |weapon: &str, evo: &[&str], mods: &[&str]| {
        let b = crate::loadout::WeaponBase::from_data(weapon, true, evo);
        // THE POOL IS THE BASE WEAPON'S. An Incarnon FORM entry has none
        // of its own — a mod is equipped on the weapon, not on the form.
        let pool = crate::mods_data::pool_for_weapon("kunai");
        let multishot: Vec<&crate::loadout::ModDef> = mods
            .iter()
            .map(|m| pool.iter().find(|d| d.id == *m).unwrap_or_else(|| panic!("no mod {m}")))
            .collect();
        let panel = crate::loadout::resolve(&b, &multishot, crate::loadout::StackPolicy::Emergent);
        let mut p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
        // A POOL THE RUN CHEWS THROUGH SLOWLY. The condition is a live one
        // — health below half — so the fixture has to actually get there:
        // spawning at full and dying instantly would leave it never true,
        // and an unkillable target would leave it never true either.
        p.target.mode = crate::fight::TargetMode::InstantRespawn;
        p.target.base_health = 40_000.0;
        p.target.base_armor = 0.0;
        p.target.base_shield = 0.0;
        monte_carlo(&p, 8, 0x4A1F).mean_damage
    };
    let perk = ["kunai_swift_conclusion"];

    // THE MEASUREMENT IS A DIFFERENCE OF DIFFERENCES, and it has to be.
    // Adding Hornet Strike changes how fast the target dies, so it changes
    // what FRACTION of the run is spent below half health — a contamination
    // both forms carry equally. What only the ADDING form carries is the
    // dilution itself, so the claim is that it loses measurably more.
    let drop = |w: &str| {
        let bare = dmg(w, &perk, &[]) / dmg(w, &[], &[]);
        let modded = dmg(w, &perk, &["hornet_strike"]) / dmg(w, &[], &["hornet_strike"]);
        modded / bare - 1.0
    };
    let base_drop = drop("kunai");
    let inc_drop = drop("kunai_incarnon");
    assert!(
        base_drop < inc_drop - 0.04,
        "the ADDING form is diluted by Hornet Strike and the MULTIPLYING one is not:              base {:.1}% against Incarnon {:.1}% — they should not be the same number",
        base_drop * 100.0,
        inc_drop * 100.0
    );
    assert!(
        inc_drop > -0.10,
        "…and the Incarnon form's small loss is the shared uptime effect, not dilution: {:.1}%",
        inc_drop * 100.0
    );
}

/// FEIGNED RETREAT: half the fight at a time, and the perk's own flat base
/// damage is excluded from what its bonus multiplies.
///
/// The exclusion is the part nobody can see. VERBATIM (wiki, Sicarus
/// Incarnon Genesis): *"Bonus damage is additive with mods such as Hornet
/// Strike but does not take into account the Base Damage increase from this
/// perk."* The card grants BOTH — "+50 Base Damage" and "+40% Damage below
/// half health" — so the naive reading multiplies the 40% by a base this
/// perk itself raised, and is wrong by `0.40 x 50` on every low-health hit.
///
/// Asserted through the loaded rate rather than through a sim, because the
/// correction is arithmetic and a Monte Carlo would bury it in the
/// half-the-fight condition.
#[test]
fn a_below_half_health_bonus_excludes_the_flat_damage_its_own_card_grants() {
    let bare = crate::loadout::WeaponBase::from_data("sicarus", true, &[]);
    let base_total = bare.base_vector.total();
    let with = crate::loadout::WeaponBase::from_data("sicarus", true, &["sicarus_feigned_retreat"]);

    // The card's own flat half, read from the same file the rate came from.
    let own_flat = crate::evolutions_data::get("sicarus_feigned_retreat")
        .expect("the perk")
        .flat_base_damage();
    assert!(own_flat > 0.0, "this card grants flat base damage too, or the test proves nothing");

    // What the rate must be: 0.40, minus the share of it that would have
    // landed on the perk's own contribution to the base.
    let evolved = base_total + own_flat;
    let expected = 0.40 - 0.40 * own_flat / evolved;
    assert!(
        (with.base_damage_below_half_health - expected).abs() < 1e-9,
        "corrected rate {:.6}, expected {expected:.6} (card 0.40, own flat {own_flat}, evolved base {evolved})",
        with.base_damage_below_half_health
    );
    assert!(
        with.base_damage_below_half_health < 0.40,
        "…and it is strictly less than the card's number, which is the whole correction"
    );

    // A LOCK TAKES IT, like every other base-damage bonus.
    assert_eq!(bare.base_damage_below_half_health, 0.0, "no perk, no bonus");
}

/// EVERY "WITH <player stat>" PERK GOES THROUGH ONE GATE, and each still
/// lands in its own bracket.
///
/// Three conditions and three grants, none of which the neutral Tenno
/// opens: it sprints at 0.9 (the slowest a frame has), carries no armor and
/// has no energy pool. That default is the honest one — a build that has
/// not said which frame is holding the gun should not be paid for a
/// threshold it may not reach.
///
/// Asserted per BRACKET, because one list feeding five brackets is exactly
/// the shape where a refactor quietly routes two of them to the same place.
#[test]
fn a_gated_perk_pays_only_the_frames_that_open_it() {
    let slow = crate::tenno_data::default_tenno().clone();
    assert!(slow.sprint < 1.2 && slow.armor <= 450.0 && slow.energy <= 700.0);

    let panel = |weapon: &str, evo: &[&str], tenno: &crate::tenno_data::Tenno| {
        let base = crate::loadout::WeaponBase::from_data(weapon, true, evo);
        crate::loadout::resolve_for(&base, &[], crate::loadout::StackPolicy::Emergent, tenno)
    };

    // MULTISHOT, on armor.
    let mut armoured = slow.clone();
    armoured.armor = 500.0;
    let off = panel("cestra", &["cestra_fortress_salvo"], &slow);
    let on = panel("cestra", &["cestra_fortress_salvo"], &armoured);
    let bare = panel("cestra", &[], &armoured);
    assert!((off.multishot - bare.multishot).abs() < 1e-9, "no armor, no multishot");
    assert!(
        (on.multishot - bare.multishot * 1.8).abs() < 1e-6,
        "+80% of base multishot with armor over 450: {} against {}",
        on.multishot,
        bare.multishot * 1.8
    );

    // BASE CRIT DAMAGE, on max energy — and it is the BASE, so it would be
    // multiplied by a crit-damage mod.
    let mut energetic = slow.clone();
    energetic.energy = 1000.0;
    let off = panel("atomos", &["atomos_paladin_virtue"], &slow);
    let on = panel("atomos", &["atomos_paladin_virtue"], &energetic);
    assert!((on.crit_damage - off.crit_damage - 1.0).abs() < 1e-9,
        "+1x with max energy over 700: {} against {}", on.crit_damage, off.crit_damage);

    // PROJECTILE SPEED, on sprint — a different bucket again.
    let mut fast = slow.clone();
    fast.sprint = 1.25;
    let ps = |t: &crate::tenno_data::Tenno| {
        panel("bronco", &["bronco_speeding_bullet"], t)
            .indirect
            .iter()
            .find(|(s, _)| *s == crate::loadout::IndirectStat::ProjectileSpeed)
            .map_or(0.0, |(_, v)| *v)
    };
    assert!((ps(&slow) - 0.0).abs() < 1e-9, "at 0.9 sprint it is worth nothing");
    assert!((ps(&fast) - 0.60).abs() < 1e-9, "at 1.25 it is the whole +60%: {}", ps(&fast));

    // …and the gates do not leak into each other: an armoured player gets
    // the Cestra's multishot and NOT the Atomos's crit damage.
    let a = panel("atomos", &["atomos_paladin_virtue"], &armoured);
    assert!((a.crit_damage - off.crit_damage).abs() < 1e-9, "armor is not energy");
}

/// DEADLY PACE ASKS WHO IS CARRYING THE BOW. "With Sprint Speed 1.2 or
/// Higher: +80% Fire Rate" — the second perk in the roster to read a PLAYER
/// stat, and it reads it through the same `condition:` spelling the first
/// one uses.
///
/// The neutral Tenno sprints at 0.9, the slowest a frame has, so the
/// default build pays nothing. Asserted on BOTH sides, because a gate that
/// is simply never open passes the first half on its own.
#[test]
fn a_sprint_gated_fire_rate_pays_only_the_frames_that_reach_it() {
    let slow = crate::tenno_data::default_tenno().clone();
    assert!(slow.sprint < 1.2, "the neutral player is the slowest one: {}", slow.sprint);
    let mut fast = slow.clone();
    fast.sprint = 1.25; // Loki Prime

    let rate = |evo: &[&str], tenno: &crate::tenno_data::Tenno| {
        let base = crate::loadout::WeaponBase::from_data("paris_prime", true, evo);
        crate::loadout::resolve_for(&base, &[], crate::loadout::StackPolicy::Emergent, tenno)
            .fire_rate
    };
    let perk = ["paris_prime_deadly_pace"];

    let bare = rate(&[], &slow);
    assert!(
        (rate(&perk, &slow) - bare).abs() < 1e-9,
        "at 0.9 sprint the perk is worth nothing: {} against {bare}",
        rate(&perk, &slow)
    );
    assert!(
        (rate(&perk, &fast) - bare * 1.8).abs() < 1e-6,
        "at 1.25 sprint it is the whole +80%: {} against {}",
        rate(&perk, &fast),
        bare * 1.8
    );
    // …and the same frame gets nothing extra without the perk, so what
    // moved is the perk and not the Tenno.
    assert!((rate(&[], &fast) - bare).abs() < 1e-9, "sprint speed alone changes no fire rate");
}

/// WELL REHEARSED: a body shot takes the pile, which is the one thing that
/// makes this trigger different from "on headshot".
///
/// Modelled as a stack that only CONSECUTIVE weak-point hits build, so the
/// perk is worth its cap to a player who never misses the head, something
/// less to one who mostly does, and exactly nothing to one who never hits
/// it. All three are asserted, because a trigger that simply never fires
/// would pass the third on its own.
#[test]
fn a_consecutive_weakpoint_buff_is_undone_by_a_body_shot() {
    let arena = crate::arena::Arena::training(120.0);
    let dmg = |evo: &[&str], head_share: f64| {
        let base = crate::loadout::WeaponBase::from_data("sybaris_prime", true, evo);
        let panel = crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent);
        let mut p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
        // ONE head and one body, both at 1x, so the only thing the aim
        // changes is which trigger fires — not how hard the hit lands.
        p.body_parts = vec![
            BodyPart { name: "head".into(), aim_weight: head_share, multiplier: 1.0,
                       is_head: true, crit_bonus: false },
            BodyPart { name: "body".into(), aim_weight: 1.0 - head_share, multiplier: 1.0,
                       is_head: false, crit_bonus: false },
        ];
        let s = monte_carlo(&p, 16, 0x5B15);
        s.mean_damage / s.mean_pellets.max(1e-9)
    };
    let perk = ["sybaris_prime_well_rehearsed"];

    let always = dmg(&perk, 1.0) / dmg(&[], 1.0);
    let half = dmg(&perk, 0.5) / dmg(&[], 0.5);
    let never = dmg(&perk, 0.0) / dmg(&[], 0.0);

    // NEVER A HEADSHOT, NEVER A STACK — and "nothing" here is not 1.0,
    // because the same card also grants a static +15 base damage. So the
    // floor is exactly that clause and not one point more, which is the
    // assertion that a trigger firing on the wrong event fails.
    let unmodded = crate::loadout::WeaponBase::from_data("sybaris_prime", true, &[])
        .base_vector
        .total();
    let static_only = (unmodded + 15.0) / unmodded;
    assert!(
        (never - static_only).abs() / static_only < 0.001,
        "body shots only: worth its static clause and nothing else — x{never:.4} against x{static_only:.4}"
    );
    assert!(
        always > never * 1.02,
        "every shot a headshot: the pile stands on top of the static clause, x{always:.4} against x{never:.4}"
    );
    // …AND HALF THE SHOTS TO THE BODY IS WORTH FAR LESS THAN HALF THE PERK,
    // which is the assertion that separates THREE IN A ROW from three in
    // total. At a 50% head rate a streak buff averages 0.5 + 0.25 + 0.125 =
    // 0.875 of its 3 stacks — under a third of the cap — while one that
    // merely accumulates sits near it. So the midpoint is the line.
    //
    // Measured: 1.2173 with the body reset, 1.3094 without it, against a
    // midpoint of 1.2474. Deleting the reset moves it across.
    let midpoint = never + 0.5 * (always - never);
    assert!(
        half < midpoint && half > never * 1.001,
        "three IN A ROW at a 50% head rate: x{half:.4}, which must sit below the              midpoint x{midpoint:.4} of (x{never:.4}, x{always:.4}) — above it means the              stacks are accumulating rather than streaking"
    );
}

/// COLD ON A TARGET THAT CANNOT BE FROZEN: the ladder climbs to the cap and
/// STAYS, which makes Cold worth MORE here rather than less.
///
/// The ordinary ladder spends itself. Nine stacks stand; the tenth proc
/// consumes all of them for a 3-second Frozen window worth +1.00 crit
/// damage, and drops back to three. So the bonus sawtooths. On a Demolisher
/// nothing converts, so ten stacks stand permanently at +0.55 and the 3
/// seconds of +1.00 never come.
///
/// Asserted on the LADDER rather than through a sim, because what changed
/// is a rule about stacks and a Monte Carlo would show it as a few per cent
/// on a damage number.
#[test]
fn cold_never_converts_on_a_target_that_cannot_be_frozen() {
    let mut ordinary = DebuffState::default();
    let mut never = DebuffState::default();
    // Twelve procs, a tenth of a second apart — past the tenth either way.
    for k in 0..12 {
        let t = k as f64 * 0.1;
        ordinary.apply_cold_proc(t, 1.0, false, None, false);
        never.apply_cold_proc(t, 1.0, false, None, true);
    }
    let at = 1.2;

    // THE ORDINARY ONE CONVERTED: it is Frozen, and its stacks were spent.
    assert!(ordinary.frozen_until.is_some_and(|f| f > at), "the tenth proc converts");
    assert!(
        (ordinary.cold_cd_bonus(at) - 1.00).abs() < 1e-9,
        "…and Frozen is +1.00 while it lasts, {}",
        ordinary.cold_cd_bonus(at)
    );

    // THE DEMOLISHER DID NOT. Ten stacks, no Frozen, and the bonus is the
    // ladder's top RUNG rather than the window's.
    assert!(never.frozen_until.is_none(), "nothing converts");
    assert_eq!(never.freeze.len(), 10, "it climbs to the ten-stack cap and stops");
    // +0.55x — the tenth rung, MEASURED on this target (M46):
    // (529 - 423) / 192 = 0.552 at a fixed non-crit of 192. A nine-rung
    // ladder would have read 518 there, i.e. 0.495.
    assert!(
        (never.cold_cd_bonus(at) - 0.55).abs() < 1e-9,
        "0.10 + 0.05 x 9 = 0.55, held all fight: {}",
        never.cold_cd_bonus(at)
    );

    // …and it KEEPS climbing back to the cap rather than being spent: two
    // more procs later it is still ten, where the ordinary one is rebuilding
    // from the three Frozen left it.
    never.apply_cold_proc(1.3, 1.0, false, None, true);
    never.apply_cold_proc(1.4, 1.0, false, None, true);
    assert_eq!(never.freeze.len(), 10, "the cap holds, and nothing consumes it");
}
