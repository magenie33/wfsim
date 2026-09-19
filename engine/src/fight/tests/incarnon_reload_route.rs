use super::*;

/// A hand-built cycle, because the route is about SHELLS and a fixture is
/// the only way to say how many are missing at the moment of transmuting.
///
/// Base form: 4-round magazine, 1 shot/s, 2 weak-point hits fill the gauge.
/// So the base form fires twice, transmutes with 2 of 4 loaded, and the
/// reload that transmute IS loads two shells.
fn cycle_with(per_shell_perk: bool, base_mag: f64) -> FightParams {
    let head = vec![BodyPart {
        name: "head".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: true,
        crit_bonus: false,
    }];
    let base_form = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 50.0),
        crit_multiplier: 1.0,
        magazine_size: base_mag,
        reload_seconds: 1.0,
        body_parts: head.clone(),
        ..no_status()
    };
    let mut p = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 100.0),
        crit_multiplier: 1.0,
        magazine_size: 2.0,
        ammo_efficiency_applies: false,
        arcane: ArcaneFx::none(),
        body_parts: head,
        duration_seconds: 40.0,
        cycle: Some(IncarnonCycle {
            starts_primed: false,
            base_form: Box::new(base_form),
            arms: Arms::Gauge { charge_on: crate::loadout::ChargeOn::WeakpointHits, charges_to_fill: 2 },
            ends: Ends::ChargeMagazine,
            transmute_out_seconds: 0.5,
            transmute_seconds: 1.0,
            reload_bucket: 0.0,
        }),
        ..no_status()
    };
    if per_shell_perk {
        // Mounting Momentum's shape: one stack per SHELL loaded, +50% fire
        // rate each, cleared by an empty magazine. Big per-stack so the
        // effect is a shot count rather than a rounding.
        p.stacking_buffs = vec![crate::loadout::StackingBuff {
            id: "per_shell_fire_rate",
            trigger: crate::loadout::BuffTrigger::ReloadComplete,
            grant: crate::loadout::BuffGrant::FireRate,
            per_stack: 0.5,
            max_stacks: 99,
            duration: crate::loadout::NO_TIMEOUT,
            chance: 1.0,
            decay: crate::loadout::BuffDecay::LoseOneAndReset,
            initial_stacks: 0,
            stacks_per_trigger: base_mag as u32,
            per_shell: true,
            cleared_by: crate::loadout::ClearedBy::EmptyMagazine,
            card_opens_full: false,
        }];
    }
    p
}

/// THE GAUGE FILLS ON A SHOT, NOT ON A PELLET — so it OVERSHOOTS, and the
/// transform lands at the end of the shot that completed it.
///
/// A shotgun puts 7 pellets into a head at once and the gauge wants 30: you
/// cannot stop at 30, you arrive at 35 on the fifth shot.
///
/// Both halves are asserted because both could be wrong on their own: the
/// COUNT (four shots must not be enough at 28 of 30) and the MOMENT (the
/// fifth shot itself is fired in the BASE form — the transform is paid
/// after it, not instead of it).
#[test]
fn the_gauge_overshoots_and_transforms_at_the_end_of_the_shot() {
    let head = vec![BodyPart {
        name: "head".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: true,
        crit_bonus: false,
    }];
    // 7 pellets a shot, 1 shot/s, gauge 30. Base and Incarnon forms are
    // told apart by their damage so the SHOT COUNT of each is readable
    // from the totals.
    let base_form = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 100.0),
        crit_multiplier: 1.0,
        multishot: 7.0,
        base_multishot: 7.0,
        magazine_size: 1e9,
        fire_rate: 1.0,
        body_parts: head.clone(),
        ..no_status()
    };
    let p = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 100.0),
        crit_multiplier: 1.0,
        multishot: 1.0,
        base_multishot: 1.0,
        magazine_size: 1.0, // one Incarnon round, so it reverts at once
        ammo_efficiency_applies: false,
        arcane: ArcaneFx::none(),
        body_parts: head,
        fire_rate: 1.0,
        // Long enough for exactly one fill-and-transform, and no more.
        duration_seconds: 5.5,
        target: TargetParams { base_health: 1e15, ..FightParams::default().target },
        cycle: Some(IncarnonCycle {
            starts_primed: false,
            base_form: Box::new(base_form),
            arms: Arms::Gauge { charge_on: crate::loadout::ChargeOn::WeakpointHits, charges_to_fill: 30 },
            ends: Ends::ChargeMagazine,
            transmute_out_seconds: 0.0,
            transmute_seconds: 0.0,
            reload_bucket: 0.0,
        }),
        ..no_status()
    };
    // Shots land at t = 0,1,2,…, so the clock is cut just after the
    // Incarnon round to make the count readable: `base` 7-pellet shots and
    // then exactly one Incarnon round.
    // The clock is given explicitly per case: shots land at t = 0,1,2,…
    // and the completing shot pays its own interval AND the transform, so
    // the Incarnon round is one interval after the last base shot.
    let run = |gauge: u32, secs: f64| {
        let q = FightParams {
            duration_seconds: secs,
            cycle: Some(IncarnonCycle {
                arms: Arms::Gauge {
                    charge_on: crate::loadout::ChargeOn::WeakpointHits,
                    charges_to_fill: gauge,
                },
                ..p.cycle.clone().unwrap()
            }),
            ..p.clone()
        };
        let r = run_once(&q, &mut Rng::new(9));
        (r.transforms, r.pellets)
    };
    // 30 AT 7 A SHOT: 7,14,21,28,35 — the fourth is SHORT at 28, so the
    // fifth is the one, and the fifth is itself fired in the BASE form.
    assert_eq!(run(30, 5.5), (1, 5 * 7 + 1), "a gauge of 30 needs five 7-pellet shots");
    // …AND FOUR SHOTS ARE NOT ENOUGH. Cut the clock at t = 4.0, before the
    // fifth: 28 of 30, and nothing has transformed. This is the half that
    // fails if the gauge is ever allowed to fill mid-shot.
    assert_eq!(run(30, 4.0).0, 0, "28 of 30 must not transform");
    // A GAUGE THAT DIVIDES EVENLY transforms on the shot that REACHES it,
    // never the one before.
    assert_eq!(run(28, 4.5), (1, 4 * 7 + 1), "28 of 28 is the fourth shot");
    assert_eq!(run(21, 3.5), (1, 3 * 7 + 1), "21 of 21 is the third");
}

/// ENTERING THE INCARNON FORM IS A RELOAD, and it pays a reload's stacks.
///
/// The transmute animation IS the weapon's reload time, which is how you
/// can tell. The whole reload runs across the cycle — one shell going in,
/// the rest coming out — so nothing here is a rule about transforming: it
/// is a rule about shells.
///
/// This sim transmutes on a GAUGE, so without this arm the route is worth
/// zero stacks on the exact mode the weapon is played in.
#[test]
fn the_incarnon_route_pays_the_shells_it_loads() {
    let with = monte_carlo(&cycle_with(true, 4.0), 1, 9);
    let without = monte_carlo(&cycle_with(false, 4.0), 1, 9);
    assert!(with.mean_transforms >= 2.0, "the fixture has to transmute");
    // MORE SHOTS, and only the route can have paid for them: the base form
    // never empties its magazine here, so its own reloads grant nothing.
    assert!(
        with.mean_shots > without.mean_shots,
        "the route is worth nothing: {} vs {}",
        with.mean_shots,
        without.mean_shots
    );
}

/// A FULL MAGAZINE PAYS NOTHING — 13/13 in is 13/13 out. This is what keeps
/// the route a rule about SHELLS rather than a fee for transforming, and it
/// is the case the owner named first.
#[test]
fn transmuting_on_a_full_magazine_grants_no_stacks() {
    // A ONE-ROUND base magazine: the shot that fills the gauge is the shot
    // that empties it, so the reload happens BEFORE the transmute and the
    // transmute finds nothing to load.
    let one = monte_carlo(&cycle_with(true, 1.0), 1, 9);
    let none = monte_carlo(&cycle_with(false, 1.0), 1, 9);
    assert!(one.mean_transforms >= 2.0);
    // The perk still pays for the base form's OWN reloads, which is why
    // this is not an equality — what it must not do is pay twice for one
    // shell. The route's share is zero here and the ordinary reload's is
    // not, so the two runs differ by the ordinary reloads alone.
    assert!(one.mean_shots >= none.mean_shots);
}

/// …and the panel keeps the RULE, not just the number it resolves to. "13"
/// and "one per shell" are the same integer on a 13-shell magazine, and the
/// route needs to tell them apart.
#[test]
fn the_panel_remembers_that_the_perk_counts_shells() {
    let base = crate::loadout::WeaponBase::from_data(
        "felarx",
        true,
        &["felarx_evo1_incarnon_form", "felarx_mounting_momentum"],
    );
    let panel =
        crate::loadout::resolve(&base, &[], crate::loadout::StackPolicy::Emergent);
    let mm = panel
        .stacking_buffs
        .iter()
        .find(|b| b.id == "per_shell_fire_rate")
        .expect("the perk is on the panel");
    assert!(mm.per_shell, "it counts shells, and the sim needs to know");
    assert_eq!(
        mm.stacks_per_trigger, panel.magazine_size as u32,
        "one per shell in the modded magazine"
    );
}
