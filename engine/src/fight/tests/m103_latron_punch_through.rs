//! LATRON PRIME against the owner's readings (MEASUREMENTS M103): the base
//! form punches through and the Incarnon form does not, a round through two
//! weak points charges the gauge twice, and no Latron declares a bounce.
//!
//! …AND WHAT A WEAK-POINT KILL IS, which the same reading settles for the whole
//! engine (docs/BUFFS.md): the round's own kill, paid once per body it killed
//! that way. Galvanized Scope's two halves and the locked-buff guards are here
//! because they are what reads it.
use super::*;

/// A LINE OF HEADS — the aimed body and `n` more behind it, one body length
/// apart, each of them nothing but head. The aimed part is pinned to a head
/// too, so every shot is a weak-point hit and the only thing moving between
/// the runs below is how many bodies the round reaches.
fn a_line_of_heads(mods: &[&str], n: usize) -> FightParams {
    let head = || {
        vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: false,
        }]
    };
    // BOTH FORMS, because the gauge is charged by the base form and spent by
    // the other one — `incarnon_cycle_from_panels` is the fight the page runs.
    let panel = |id: &str| {
        let evo = ["latron_prime_evo1_incarnon_form"];
        let base = crate::model::WeaponBase::from_data(id, false, &evo);
        let pool = crate::data::mods::pool_for_weapon("latron_prime");
        let refs: Vec<&crate::model::ModDef> = mods
            .iter()
            .map(|m| pool.iter().find(|d| d.id == *m).unwrap_or_else(|| panic!("{m}")))
            .collect();
        crate::build::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent)
    };
    let mut arena = crate::arena::Arena::training(20.0);
    // THE AIMED BODY IS ALL HEAD, in the arena rather than on the outer
    // params: the base form reads its own copy, and that is the form that
    // charges.
    arena.body_parts = head();
    arena.others = (1..=n)
        .map(|i| crate::formation::FoeSpec {
            id: String::new(),
            params: TargetParams::training_dummy(),
            body_parts: head(),
            at: crate::rules::space::Vec2::new(
                0.0,
                crate::rules::space::CONTACT_RANGE_M * (1.0 + i as f64),
            ),
        })
        .collect();
    FightParams::incarnon_cycle_from_panels(
        &panel("latron_prime_incarnon"),
        &panel("latron_prime"),
        false,
        LockMode::Initial(0),
        &arena,
        &ArcaneFx::none(),
    )
}

/// ONE ROUND THROUGH TWO HEADS IS TWO WEAK-POINT HITS, so a gauge that wants
/// eight of them is full in three shots instead of eight — "8 weakpoints will
/// completely fill the gauge" counts LANDINGS, not trigger pulls (M103).
///
/// READ OFF THE SHOTS THE FIRST TRANSMUTE COST, not off a counter and not off
/// the transform count: a charge that is tallied and never spent looks exactly
/// like one that works, and over a whole engagement this cycle is bound by the
/// 40-round dump rather than by the charging.
#[test]
fn m103_a_round_through_two_heads_charges_the_gauge_twice() {
    let shots_to_fill = |mods: &[&str], n: usize| {
        let rec = crate::fight::replay::record(
            &a_line_of_heads(mods, n), 5, 0.0, f64::INFINITY, 200_000, 0,
        );
        let mut shots = 0u32;
        for e in rec.events() {
            match &e.kind {
                crate::record::Kind::Strike { .. } => shots += 1,
                crate::record::Kind::TransformStart { .. } => return shots,
                _ => {}
            }
        }
        panic!("the fixture never transformed");
    };
    // EIGHT WEAK POINTS, one a shot: the gauge's own number.
    assert_eq!(shots_to_fill(&[], 2), 8, "one head a shot");
    // …AND THREE WITH TWO BODIES BEHIND THE FIRST — nine landings, the gauge
    // overshooting by one, which is the same `>=` every gauge crosses on.
    assert_eq!(shots_to_fill(&["primed_shred"], 2), 3, "three heads a shot");
    // …AND IT IS THE BODIES, NOT THE MOD: with nobody behind the target
    // punch through reaches nothing and pays nothing.
    assert_eq!(shots_to_fill(&["primed_shred"], 0), 8, "nobody behind it");
}

/// …AND EVERY OTHER WEAK-POINT TRIGGER IS PAID THE SAME NUMBER OF TIMES.
/// Death Knell is the readable one — a stack per weak-point hit, per pellet —
/// so a round through two more heads climbs its pile three times a shot.
#[test]
fn m103_a_punched_weak_point_fires_what_a_weak_point_fires() {
    let damage = |mods: &[&str], n: usize| {
        let mut p = a_line_of_heads(mods, n);
        p.weakpoint_stacks = Some(crate::model::WeakpointStacksSpec {
            max_stacks: 6,
            duration_seconds: 6.0,
            crit_multiplier: 0.5,
            status_chance: 0.0,
            ammo_efficiency: 0.0,
        });
        // THE AIMED BODY ALONE, so the punched bodies' own damage cannot be
        // what this reads — the claim is about the pile, not about the crowd.
        monte_carlo(&p, 12, 0x103).mean_damage_by_body.0[0]
    };
    let alone = damage(&[], 2);
    let through = damage(&["primed_shred"], 2);
    assert!(
        through > alone * 1.05,
        "the punched heads fill Death Knell's pile too: {alone:.0} -> {through:.0}"
    );
}

/// THE INCARNON FORM PIERCES NOBODY and the base form does — *"Punch Through
/// does not affect the incarnon projectile"*, ✅ measured (M103) — and no
/// Latron declares a BOUNCE, whose rule the same reading put out of reach.
#[test]
fn m103_the_incarnon_form_refuses_punch_through_and_declares_no_bounce() {
    for id in ["latron", "latron_prime", "latron_wraith"] {
        let inc = format!("{id}_incarnon");
        let panel = |w: &str| {
            let base = crate::model::WeaponBase::from_data(w, false, &[]);
            let pool = crate::data::mods::pool_for_weapon(id);
            let shred: Vec<&crate::model::ModDef> =
                vec![pool.iter().find(|d| d.id == "primed_shred").expect("primed_shred")];
            crate::build::loadout::resolve(&base, &shred, crate::model::StackPolicy::Emergent)
        };
        assert!(panel(id).punch_through_m > 2.0, "{id} takes the mod");
        assert!(
            (panel(&inc).punch_through_m - 0.0).abs() < 1e-9,
            "{inc} takes no punch through: {}",
            panel(&inc).punch_through_m
        );
        assert!(
            crate::model::WeaponBase::from_data(&inc, false, &[]).ricochet.is_none(),
            "{inc} declares no bounce"
        );
    }
}

/// GALVANIZED SCOPE'S KILL HALF KEEPS A CLOCK PER STACK, which no other
/// Galvanized mod does — and the discriminator is that the pile FALLS WHILE
/// THE KILLS KEEP COMING. One clock refreshed by each kill would sit at the
/// cap forever; five independent ones hold only the kills of the last 3.5 s.
///
/// A kill a second against a 5-stack cap: four is the most that can be alive
/// at once, and between two kills one always drops out.
#[test]
fn galvanized_scopes_kill_stacks_each_run_their_own_clock() {
    let p = FightParams {
        fire_rate: 1.0,
        magazine_size: 1e9,          // no reload: downtime is a different test
        body_parts: all_head(),
        target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
        crit_chance_stack: Some(crate::model::StackSpec {
            per_stack: 0.04,
            max_stacks: 5,
            duration: 3.5,
            initial_stacks: 0,
            earned_on: Some("headshot_kill"),
        }),
        duration_seconds: 20.0,
        ..no_status()
    };
    let trace = replay(&p, Rng::new(7).state(), 400);
    let i = trace.buffs.iter().position(|x| x.id == "on_headshot_kill_cc").expect("rostered");
    let series: Vec<u16> = trace.frames.iter().map(|f| f.stacks[i]).collect();
    assert_eq!(series[0], 0, "the pile is earned, not given");
    assert_eq!(
        *series.iter().max().expect("frames"), 4,
        "a 3.5 s clock per stack holds four kills, never the cap: {series:?}"
    );
    assert!(
        series.windows(2).filter(|w| w[0] > w[1]).count() >= 4,
        "stacks drop out one at a time while the kills go on: {series:?}"
    );
}

/// …AND A HEADSHOT KILL IS A KILL BY THE HIT ITSELF. The bleed a weak-point
/// hit started finishes this target, and the mod's kill half earns nothing:
/// "On Headshot Kill" is the round's kill, not the body's death.
#[test]
fn a_bleed_that_finishes_a_headshot_target_is_not_a_headshot_kill() {
    let mut target = frail_target(TargetMode::InstantRespawn, 0.0, 0.0);
    target.base_health = 150.0;   // the 100 hit leaves it standing; the bleed does not
    let p = FightParams {
        fire_rate: 1.0,
        magazine_size: 1.0,
        reload_seconds: 100.0,    // one shot, and the window belongs to the bleed
        damage: DamageVector::new().with(DamageType::Slash, 100.0),
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        forced_procs: vec![DamageType::Slash],
        body_parts: all_head(),
        target,
        crit_chance_stack: Some(crate::model::StackSpec {
            per_stack: 0.04,
            max_stacks: 5,
            duration: 30.0,
            initial_stacks: 0,
            earned_on: Some("headshot_kill"),
        }),
        arcane: ArcaneFx::none(),
        duration_seconds: 20.0,
        ..no_status()
    };
    let r = run_once(&p, &mut Rng::new(7));
    assert_eq!(r.headshots, 1, "the fixture lands one weak-point hit");
    assert!(r.kills >= 1, "…and the bleed finishes the target: {} kills", r.kills);
    let trace = replay(&p, Rng::new(7).state(), 400);
    let i = trace.buffs.iter().position(|x| x.id == "on_headshot_kill_cc").expect("rostered");
    let series: Vec<u16> = trace.frames.iter().map(|f| f.stacks[i]).collect();
    assert!(series.iter().all(|&v| v == 0), "a bleed's kill earns no stack: {series:?}");
}

/// …AND A BODY THE SAME ROUND PUNCHED THROUGH AND KILLED IS ONE. Same rule as
/// the weak-point HITS above: the round entered the same part of each body, so
/// one shot through three heads is three kills the mod pays for.
///
/// ONE SHOT IN THE WHOLE ENGAGEMENT (a magazine of one and a reload nobody
/// waits out), and the BASE form alone — so the pile that stands afterwards is
/// that single round's, with no clock and no second shot in it.
#[test]
fn m103_a_punched_headshot_kill_pays_what_the_aimed_one_pays() {
    let stacks = |mods: &[&str]| {
        let mut p = a_line_of_heads(mods, 2);
        p = FightParams {
            cycle: None,              // the base form, which is the one that pierces
            magazine_size: 1.0,
            reload_seconds: 1e6,
            fire_rate: 1.0,
            ..p
        };
        p.target = frail_target(TargetMode::InstantRespawn, 0.0, 0.0);
        for f in p.others.iter_mut() {
            f.params = frail_target(TargetMode::InstantRespawn, 0.0, 0.0);
        }
        p.punch_through_m = if mods.is_empty() { 0.0 } else { 2.0 };
        p.crit_chance_stack = Some(crate::model::StackSpec {
            per_stack: 0.04,
            max_stacks: 10,
            duration: 30.0,
            initial_stacks: 0,
            earned_on: Some("headshot_kill"),
        });
        let trace = replay(&p, Rng::new(7).state(), 200);
        let i = trace.buffs.iter().position(|x| x.id == "on_headshot_kill_cc").expect("rostered");
        let s: Vec<u16> = trace.frames.iter().map(|f| f.stacks[i]).collect();
        (*s.iter().max().unwrap_or(&0), run_once(&p, &mut Rng::new(7)))
    };
    let (alone, ra) = stacks(&[]);
    assert_eq!(ra.shots, 1, "one shot, so the pile cannot be two rounds'");
    assert_eq!(alone, 1, "the aimed kill alone");
    let (through, rt) = stacks(&["metal_auger"]);
    assert_eq!(rt.shots, 1, "…and the same one shot");
    assert_eq!(through, 3, "and the two bodies behind it");
}

/// A LOCKED BUFF A WEAPON DOES NOT HAVE GRANTS NOTHING, and neither does one
/// locked at zero stacks.
///
/// Found while reading the weak-point kill above: an Incarnon cycle always
/// carries a Frenzy lock, and a configured card passes `Initial(0)` — which
/// armed Frenzy's x2.5 fire rate at t = 0 on every cycle weapon, Frenzy being
/// a perk the Dual Toxocyst has and the Latron does not. The cadence is the
/// assertion because that is what the buff buys: shots per second of fight.
#[test]
fn a_frenzy_lock_pays_nothing_on_a_weapon_without_frenzy() {
    let shots_in = |lock: Option<LockMode>, frenzy: bool| {
        let mut p = FightParams {
            fire_rate: 2.0,
            magazine_size: 1e9,
            body_parts: all_head(),   // every shot is the trigger Frenzy reads
            duration_seconds: 10.0,
            frenzy,
            ..no_status()
        };
        p.locked_buffs = lock
            .map(|mode| vec![crate::fight::BuffLock { buff: crate::fight::LockedBuff::Frenzy, mode }])
            .unwrap_or_default();
        run_once(&p, &mut Rng::new(7)).shots
    };
    let plain = shots_in(None, false);
    assert_eq!(plain, 20, "2 shots a second for ten seconds");
    assert_eq!(
        shots_in(Some(LockMode::Initial(0)), false), plain,
        "a card at zero stacks opens the fight with nothing up"
    );
    assert_eq!(
        shots_in(Some(LockMode::Initial(1)), false), plain,
        "…and a locked Frenzy on a weapon that has none grants nothing"
    );
    // THE CONTROL: the same lock on a weapon that DOES list the perk is worth
    // x2.5 fire rate, and every headshot here refreshes it.
    assert!(
        shots_in(Some(LockMode::Initial(1)), true) > plain * 2,
        "the fixture must be able to show Frenzy at all"
    );
}
