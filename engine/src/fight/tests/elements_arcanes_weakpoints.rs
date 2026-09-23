use super::*;

#[test]
fn electricity_joins_one_clock_per_body() {
    // THE GOLDEN FOLLOWS THE MECHANIC, and the citation is dated: wiki
    // `Damage/Electricity Damage`, Update 33.6 — "multiple procs on an
    // enemy no longer deal their respective damage separately … but once
    // per second, similar to Heat status. However, they still maintain each
    // own timer and will not refresh, unlike Heat." The PRE-33.6 model
    // gives 45 ticks × 37.5 = 1687.5 and is not what this asserts.
    //
    // Under one clock the first proc ticks at 0 and every later one waits
    // for the shared tick, one second ahead: 39 ticks × 37.5 = 1462.5
    // before 10 s. IT IS NOT A 13% NERF — the six ticks are past the
    // fixture's window rather than lost, and every instance still pays its
    // full six.
    //
    // THE ACCUMULATOR IS PER TICK GROUP, and Electricity is the family that
    // has groups: the 39 seed-ticks land on TEN distinct tick times (0, and
    // 1..9), so the `1` is counted ten times and not thirty-nine
    // (`Dot::accumulator_unit`).
    let s = monte_carlo(&bare(DamageType::Electricity), 20, 5);
    assert!(
        (s.mean_dot_damage - (39.0 * 37.5 + 10.0 * 0.5)).abs() < 1e-6,
        "dot {}",
        s.mean_dot_damage
    );
    // …AND THE INSTANCES ARE STILL THEIR OWN. Heat, whose procs REFRESH
    // each other, answers 1687.5 on this same cadence — so a test that only
    // pinned the total could not tell the two models apart. The difference
    // that matters is that Electricity does not refresh, which is why its
    // number is lower here and Heat's is not.
}

#[test]
fn heat_is_a_single_refreshing_accumulator() {
    // Ten forced Heat procs, one per second: ONE entity born at t=0,
    // each proc adds 37.5 to the tick and refreshes the expiry, ticks
    // anchored at 1,2,...,9 (< 10 s): tick k has k contributions ->
    // Σ k=1..9 of 37.5k = 37.5 × 45 = 1687.5, plus ONE accumulator per
    // tick — nine of them, not forty-five, because Heat is the archetype
    // of a consolidated tick group (`Dot::accumulator_unit`): + 9 × 0.5.
    let s = monte_carlo(&bare(DamageType::Heat), 20, 5);
    assert!(
        (s.mean_dot_damage - (1687.5 + 9.0 * 0.5)).abs() < 1e-6,
        "dot {}",
        s.mean_dot_damage
    );
}

#[test]
fn condition_overload_multiplies_by_distinct_status_types() {
    // Forced Impact (Stagger) with co_per_type = 1.0, no base-damage
    // mods: shot 1 sees 0 types, shots 2..10 see 1 ->
    // 75 × (1 + 9 × 2) = 1425.
    let p = FightParams {
        co_per_type: 1.0,
        ..bare(DamageType::Impact)
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_damage - 1425.0).abs() < 1e-9,
        "dmg {}",
        s.mean_damage
    );
}

#[test]
fn condition_overload_is_diluted_by_base_damage_mods() {
    // Additive class: with +100% base damage, one status type gives
    // (1 + 1 + 1)/(1 + 1) = 1.5× instead of 2× ->
    // 75 × (1 + 9 × 1.5) = 1087.5.
    let p = FightParams {
        base_damage_bonus: 1.0,
        co_per_type: 1.0,
        ..bare(DamageType::Impact)
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_damage - 1087.5).abs() < 1e-9,
        "dmg {}",
        s.mean_damage
    );
}

#[test]
fn condition_overload_behavior_classes_differ_per_weapon() {
    use crate::model::CoBehavior;
    // Same +100% base damage, one active type. Independent ignores
    // the dilution: 75 × (1 + 9 × 2) = 1425. Inert: 75 × 10 = 750.
    let p = |b| FightParams {
        base_damage_bonus: 1.0,
        co_per_type: 1.0,
        co_behavior: b,
        ..bare(DamageType::Impact)
    };
    let ind = monte_carlo(&p(CoBehavior::Independent), 20, 5);
    assert!((ind.mean_damage - 1425.0).abs() < 1e-9);
    let inert = monte_carlo(&p(CoBehavior::Inert), 20, 5);
    assert!((inert.mean_damage - 750.0).abs() < 1e-9);
}

#[test]
fn cold_raises_crit_damage_received() {
    // Forced Cold, cc 100%, cd 2.0: stack counts 0,1,...,5 then a
    // steady 5 (6 s expiry) -> b = 0,.10,.15,.20,.25,.30 then .30
    // (Σ = 2.20); total = 75 × Σ(2 + b) = 75 × 22.2.
    let p = FightParams {
        base_crit_chance: 1.0,
        crit_multiplier: 2.0,
        forced_procs: vec![DamageType::Cold],
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_damage - 75.0 * 22.2).abs() < 1e-9,
        "dmg {}",
        s.mean_damage
    );
}

#[test]
fn blast_stacks_fire_singly_on_fuse_expiry() {
    // Forced Blast at 1 shot/s: each stack's 1.5 s fuse fires a
    // 0.3 × 75 = 22.5 hit; fuses at 1.5..9.5 land before 10 s = 9 of
    // 10; never 10 simultaneous stacks -> no early detonation.
    let s = monte_carlo(&bare(DamageType::Blast), 20, 5);
    assert!(
        (s.mean_dot_damage - 9.0 * 22.5).abs() < 1e-9,
        "burst {}",
        s.mean_dot_damage
    );
}

/// **ELEMENTAL DAMAGE DOES NOT REACH A BLAST DETONATION**, which is the one
/// rule that makes Blast unlike every other damaging status: *"Unlike other
/// damaging statuses, adding more elemental damage (Heat and Cold) will not
/// increase the Blast proc damage"* (wiki, `Damage/Blast_Damage`).
///
/// MEASURED AS A CONTROLLED PAIR on a Braton Prime, base 35. 90% Cold + 90% Heat hits for 98 = 35 x 2.8 and detonates
/// for 11; adding a further +200% Blast takes the HIT to 168 = 35 x 4.8 and
/// leaves the detonation at 11 — `0.3 x 35 = 10.5` both times, the element
/// bracket nowhere in it.
///
/// The engine is right BY CONSTRUCTION — a stack reads `modified_base`,
/// which is the Serration bucket alone, while elements are a bracket
/// applied at the hit — and by-construction is exactly what a later change
/// breaks silently, since every other status would WANT the bracket there.
///
/// BOTH HALVES ARE ASSERTED. A test that only checked "unchanged" would
/// pass just as well on a build where the ability did nothing at all, so
/// the direct damage has to move in the same run that the detonation does
/// not.
#[test]
fn elemental_damage_moves_the_hit_and_never_the_blast_detonation() {
    let plain = bare(DamageType::Blast);
    let imbued = FightParams {
        // Valence Formation is the only source that can add Blast as its
        // own element rather than by combining two — see
        // `data/abilities/valence_formation.yaml`.
        abilities: crate::data::abilities::resolve(
            &[crate::data::abilities::AbilityPick {
                id: "valence_formation",
                duration_seconds: None,
                element: Some("blast"),
            }],
            &crate::data::abilities::Caster::default(),
            "",
            "melee",
        ),
        ..bare(DamageType::Blast)
    };
    let (a, b) = (monte_carlo(&plain, 20, 5), monte_carlo(&imbued, 20, 5));
    // The hit moves — +200% of modified base on a weapon that had 100% of
    // it, so x3. Taken as `damage - dot` because `mean_damage` is the
    // WHOLE run and the detonations are in it; leaving them in dilutes the
    // very ratio this half exists to prove (x2.5748 with them).
    let direct = |s: &Summary| s.mean_damage - s.mean_dot_damage;
    let ratio = direct(&b) / direct(&a);
    assert!((ratio - 3.0).abs() < 1e-6, "the imbue has to land: x{ratio:.4}");
    // …and the detonation does not, to the last bit.
    assert!(
        (b.mean_dot_damage - a.mean_dot_damage).abs() < 1e-9,
        "blast took an element bracket: {} vs {}",
        b.mean_dot_damage,
        a.mean_dot_damage
    );
}

#[test]
fn overguard_break_with_disrupt_fires_the_tesla_payload() {
    // 100 overguard, forced Magnetic (InstantRespawn so pools deplete;
    // health big enough that nothing dies): shot 1 (amp 1, no stacks
    // yet) leaves 25 and lands a stack; shot 2 (amp 2.0) breaks ->
    // break proc = 3% × 1 stack × 100 = 3 total over 6 ticks. Health
    // then takes shots 3..10 at amp 1: dot damage == 3 exactly.
    let mut t = frail_target(TargetMode::InstantRespawn, 0.0, 100.0);
    t.base_health = 10_000.0;
    let p = FightParams {
        foe: t,
        ..bare(DamageType::Magnetic)
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_dot_damage - 3.0).abs() < 1e-9,
        "break proc {}",
        s.mean_dot_damage
    );
}

/// THE CYCLE, EARNED — and the same fixture opening primed, for contrast.
///
/// The engagement starts in the BASE form and pays for its first transmute
/// like everything else consumable in this sim. Opening transformed with a
/// full charge magazine is a gift a fight should not make by default;
/// `starts_primed` keeps that reading available and this test pins both, so
/// the difference between them is a number rather than a memory.
#[test]
fn incarnon_cycle_alternates_forms_deterministically() {
    // Incarnon: 100 dmg, mag 2, 1/s. Base: 50 dmg, aim 100% head
    // (each pellet charges), 2 charges to fill, revert 0.5 s,
    // transmute 1.0 s.
    //
    // EARNED, over 12 s:
    //   base @0,1 -> transmute -> inc @3,4 | revert 5->5.5
    //   base @5.5,6.5 -> transmute -> inc @8.5,9.5 | revert 10.5->11
    //   base @11. Totals: 4x100 + 5x50 = 650; 9 shots; 2 transforms
    //   (transmutes INTO the form only — the reverts do not count).
    //
    // PRIMED, same window: inc @0,1 | revert 2->2.5 | base @2.5,3.5 ->
    //   transmute -> inc @5.5,6.5 | revert 7.5->8 | base @8,9 -> transmute
    //   -> inc @11. 5x100 + 4x50 = 700, one Incarnon shot traded for one
    //   base shot.
    //
    // THE SHOT THAT FILLS THE GAUGE PAYS ITS OWN INTERVAL, which puts
    // the Incarnon rounds at 3 rather than 2. The window is 12 s because at
    // 10 s the two readings TIE at 600 — an artefact of where the clock
    // falls rather than a fact about the gift.
    let head = vec![BodyPart {
        name: "head".into(),
        aim_weight: 1.0,
        multiplier: 1.0, // 1x so no crit-location bonus, pure counts
        is_head: true,
        crit_bonus: false,
    }];
    let base_form = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 50.0),
        crit_multiplier: 1.0,
        body_parts: head.clone(),
        ..no_status()
    };
    let p = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 100.0),
        crit_multiplier: 1.0,
        magazine_size: 2.0,
        ammo_efficiency_applies: false,
        arcane: ArcaneFx::none(),
        body_parts: head,
        cycle: Some(IncarnonCycle {
            // These fixtures test the EARNED cycle, which is the standard one.
            starts_primed: false,
            base_form: Box::new(base_form),
            arms: Arms::Gauge { charge_on: crate::model::ChargeOn::WeakpointHits, charges_to_fill: 2 },
            ends: Ends::ChargeMagazine,
            transmute_out_seconds: 0.5,
            transmute_seconds: 1.0,
            reload_bucket: 0.0,
        }),
        ..no_status()
    };
    let p = FightParams { duration_seconds: 12.0, ..p };
    let s = monte_carlo(&p, 5, 9);
    assert!((s.mean_damage - 650.0).abs() < 1e-9, "earned dmg {}", s.mean_damage);
    assert!((s.mean_shots - 9.0).abs() < 1e-9, "shots {}", s.mean_shots);
    assert!((s.mean_transforms - 2.0).abs() < 1e-9);
    assert_eq!(s.mean_reloads, 0.0);

    // ...and the reading that walks in already charged.
    let mut primed = p.clone();
    if let Some(c) = primed.cycle.as_mut() {
        c.starts_primed = true;
    }
    let q = monte_carlo(&primed, 5, 9);
    assert!((q.mean_damage - 700.0).abs() < 1e-9, "primed dmg {}", q.mean_damage);
    // The FREE MAGAZINE, priced: one Incarnon shot traded for one base
    // shot, over an engagement this short. On a real weapon at a headshot
    // rate that can never refill the gauge it is the whole of its Incarnon
    // damage.
    assert!(q.mean_damage > s.mean_damage, "the gift is worth something");

    // **THE FIGHT IS EXECUTING THE LIST, NOT ITS OWN MIND.** One rule inserted
    // above the mode's own — go in whenever there is anything to go in WITH —
    // and the same weapon on the same dice transmutes on a part-filled gauge
    // instead of waiting for a full one. It is the whole claim `data::apl`
    // makes, and no other test here would notice a loop that ignored the list.
    let eager = FightParams {
        apl_inserted: crate::data::apl::Apl(vec![crate::data::apl::Rule {
            action: crate::data::apl::Action::TransformIn,
            when: crate::data::apl::When::GaugeAtLeast { pct: 0.0 },
        }]),
        ..p.clone()
    };
    let e = monte_carlo(&eager, 5, 9);
    assert!(
        e.mean_transforms > s.mean_transforms,
        "an eager rule did not reach the loop: {} transforms, was {}",
        e.mean_transforms,
        s.mean_transforms,
    );
    // …AND THE MODE'S OWN LIST IS WHAT AN EMPTY INSERT RUNS, to the line, so
    // every published row is the policy it was measured under.
    assert_eq!(p.apl(), crate::data::apl::preset("cycle").unwrap());
    let plain = FightParams { cycle: None, ..p.clone() };
    assert_eq!(plain.apl(), crate::data::apl::preset("base").unwrap());
}

/// `charge_on` is WEAPON DATA, not a constant. Documented in the yaml and
/// ignored by the loader, every weapon charges off weak-point hits — wrong
/// for the Torid, which the wiki charges through plain direct hits
/// ("Angstrum Incarnon Genesis and Torid Incarnon Genesis are instead
/// charged through direct hits").
///
/// Body-only aim is the discriminator: there are no weak-point hits at all,
/// so a `WeakpointHits` weapon never transforms again and a `DirectHits`
/// one keeps cycling.
#[test]
fn the_gauge_charges_off_whatever_the_weapon_data_says() {
    let base_form = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 50.0),
        crit_multiplier: 1.0,
        body_parts: mono_body(1.0), // NO heads: no weak-point hits ever
        ..no_status()
    };
    let mk = |charge_on| FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 100.0),
        crit_multiplier: 1.0,
        magazine_size: 2.0,
        ammo_efficiency_applies: false,
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        cycle: Some(IncarnonCycle {
            // These fixtures test the EARNED cycle, which is the standard one.
            starts_primed: false,
            base_form: Box::new(base_form.clone()),
            arms: Arms::Gauge { charge_on, charges_to_fill: 2 },
            ends: Ends::ChargeMagazine,
            transmute_out_seconds: 0.5,
            transmute_seconds: 1.0,
            reload_bucket: 0.0,
        }),
        ..no_status()
    };
    let wp = monte_carlo(&mk(crate::model::ChargeOn::WeakpointHits), 5, 9);
    assert_eq!(
        wp.mean_transforms, 0.0,
        "no weak-point hits = the gauge never fills again"
    );
    let direct = monte_carlo(&mk(crate::model::ChargeOn::DirectHits), 5, 9);
    assert!(
        direct.mean_transforms > 0.0,
        "plain direct hits must fill a direct-hit gauge (transforms {})",
        direct.mean_transforms
    );
}

/// THE EXPLOSION BELONGS TO THE FORM THAT HAS ONE. A cycle fires two
/// different weapons in turn, and the radial stage was read off the OUTER
/// params — which are the Incarnon form's — for every shot, base phase
/// included. So a weapon whose Incarnon detonates threw that explosion on
/// every base-form shot as well, for free and forever.
///
/// Where it showed: a board pinned to a 0% headshot rate, where eight of
/// the nine Incarnon forms never charge at all. Measured on the real
/// Burston Prime (Serration only, 4 s, 400 runs, zero headshots, ZERO
/// transforms on both sides): `incarnon_cycle` 2470 DPS against a pinned
/// base form's 1738, +42%, the whole gap in a `radial` source dealing Heat
/// — an element the base form has nowhere in its vector. See MEASUREMENTS
/// M32.
///
/// Body-only aim is the discriminator again, for the same reason as the
/// test above: no weak-point hits, so the gauge never fills and the
/// engagement is base form from end to end. What it must equal is the SAME
/// base form fired on its own.
#[test]
fn a_cycle_that_never_transforms_is_its_base_form() {
    let base_form = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 50.0),
        crit_multiplier: 1.0,
        body_parts: mono_body(1.0), // NO heads: the gauge can never fill
        ..no_status()
    };
    let cycling = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 100.0),
        crit_multiplier: 1.0,
        magazine_size: 2.0,
        ammo_efficiency_applies: false,
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        // The Incarnon form's, and ONLY the Incarnon form's — `base_form`
        // above declares none.
        radial: Some(radial_of(0.0, 0.0)),
        cycle: Some(IncarnonCycle {
            starts_primed: false,
            base_form: Box::new(base_form.clone()),
            arms: Arms::Gauge { charge_on: crate::model::ChargeOn::WeakpointHits, charges_to_fill: 2 },
            ends: Ends::ChargeMagazine,
            transmute_out_seconds: 0.5,
            transmute_seconds: 1.0,
            reload_bucket: 0.0,
        }),
        ..no_status()
    };
    let cyc = monte_carlo(&cycling, 5, 9);
    let alone = monte_carlo(&base_form, 5, 9);
    assert_eq!(cyc.mean_transforms, 0.0, "the fixture must never transform");
    assert_eq!(
        cyc.mean_damage, alone.mean_damage,
        "a cycle stuck in its base form must deal exactly what that form deals              ({} vs {}) — the difference is the other form's explosion",
        cyc.mean_damage, alone.mean_damage
    );
}

#[test]
fn initial_lock_grants_frenzy_once_then_mechanics_rule() {
    // Body-only aim (no natural headshots), Frenzy at Initial: the
    // t=0 grant runs out at 3 s. Shots at 0,0.4,...,2.8 (8) then
    // 3.2,4.2,...,9.2 (7) = 15 — vs 25 Permanent, 10 unlocked.
    let p = FightParams {
        frenzy: true,
        locked_buffs: vec![BuffLock::initial(LockedBuff::Frenzy, 1)],
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let s = monte_carlo(&p, 20, 4);
    assert!((s.mean_shots - 15.0).abs() < 1e-9, "shots {}", s.mean_shots);
}

/// SPLIT FLIGHTS' decay: "Stacks expire all at once after 2 seconds
/// without a hit." The contrast with the Galvanized family below is the
/// whole reason `BuffDecay::AllAtOnce` exists — same three stacks, same
/// clock, and one sheds them over three windows while this drops the pile
/// in one.
#[test]
fn an_all_at_once_pile_goes_whole_and_a_hit_refreshes_all_of_it() {
    let mut s = LiveStacks::seed_all_at_once(3, 4, 2.0);
    assert_eq!(s.current(1.9, 2.0), 3);
    assert_eq!(s.current(2.1, 2.0), 0, "the pile, not one stack");

    // A hit inside the window carries every stack forward — the same
    // shared clock the Galvanized family uses, which is why "subsequent
    // hits refresh all stacks' duration" needs no code of its own.
    let mut s = LiveStacks::seed_all_at_once(3, 4, 2.0);
    s.bump(1.5, 2.0, 4);
    assert_eq!(s.current(3.4, 2.0), 4, "climbed to 4 and still alive at 3.4");
    assert_eq!(s.current(3.6, 2.0), 0);
}

#[test]
fn galvanized_decay_loses_one_stack_and_resets_duration() {
    let mut s = LiveStacks {
        stacks: 3,
        expiry: 5.0,
        each: Vec::new(),
        per_stack: false,
        all_at_once: false,
    };
    assert_eq!(s.current(4.9, 10.0), 3);
    assert_eq!(s.current(5.1, 10.0), 2); // lost one, next decay at 15
    assert_eq!(s.current(14.9, 10.0), 2);
    assert_eq!(s.current(15.1, 10.0), 1);
    assert_eq!(s.current(26.0, 10.0), 0);
}

#[test]
fn emergent_multishot_stacks_are_earned_by_kills_from_zero() {
    // Cold-start config (initial 0): frail 50 HP target dies to every
    // pellet; +1.0 pellet per stack, cap 2, long duration. Strike k
    // fires (1 + stacks) pellets and the FIRST pellet's kill bumps
    // the stack: pellets per shot 1, 2, 3, 3, ... = 1 + 2 + 8×3 = 27.
    let spec = crate::model::StackSpec {
        per_stack: 1.0,
        max_stacks: 2,
        duration: 100.0,
        initial_stacks: 0,
        earned_on: Some("kill"),
    };
    let p = FightParams {
        multishot_stack: Some(spec),
        foe: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
        arcane: ArcaneFx::none(),
        crit_multiplier: 1.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_pellets - 27.0).abs() < 1e-9,
        "pellets {}",
        s.mean_pellets
    );

    // Initial-full (the user's default): every shot fires 3 pellets
    // from t = 0 (kills keep the stacks refreshed) -> 30 pellets.
    let full = FightParams {
        multishot_stack: Some(crate::model::StackSpec {
            initial_stacks: 2,
            ..spec
        }),
        ..p
    };
    let s2 = monte_carlo(&full, 20, 5);
    assert!(
        (s2.mean_pellets - 30.0).abs() < 1e-9,
        "pellets {}",
        s2.mean_pellets
    );
}

#[test]
fn co_base_fraction_scales_the_co_bonus() {
    // Additive class, base_damage 0, fraction 0.6, one active type:
    // 75 × (1 + 9 × (1 + 0.6)) = 75 × 15.4 = 1155.
    let p = FightParams {
        co_per_type: 1.0,
        co_base: crate::model::CoBase::new(0.6, 1.0, crate::model::CoStage::Direct),
        ..bare(DamageType::Impact)
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_damage - 1155.0).abs() < 1e-9,
        "dmg {}",
        s.mean_damage
    );
}

#[test]
fn deadhead_adds_base_damage_stacks_and_headshot_bonus() {
    // Deadhead full stacks (initial): arc base_damage = 3 × 1.2 = 3.6 -> ratio
    // 4.6 (base_damage 0). Headshot bonuses multiply the base multiplier via an
    // additive bracket (Enemy_Body_Parts verbatim: 3 × (1 + 30% + …)):
    // this 3x head becomes 3.9x.
    // 10 shots × 75 × 3.9 × 4.6 = 13,455. No kills: no decay in 10 s.
    let p = FightParams {
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: arc_stacked("secondary_deadhead"),
        body_parts: vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: false,
        }],
        ..no_status()
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_damage - 13_455.0).abs() < 1e-9,
        "dmg {}",
        s.mean_damage
    );
}

#[test]
fn cascadia_flare_hard_resets_without_fresh_heat() {
    // Initial 40 stacks (+480% -> ×5.8), 10 s shared timer. 15 s at
    // 1/s with the 12-round magazine: shots at 0..11, reload 2.35 s,
    // one more at 14.35 (13 shots). Without Heat procs (forced
    // Impact): only t < 10 boosted: 10×435 + 3×75 = 4575.
    let starved = FightParams {
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: arc_stacked("cascadia_flare"),
        forced_procs: vec![DamageType::Impact],
        body_parts: mono_body(1.0),
        duration_seconds: 15.0,
        ..no_status()
    };
    let s = monte_carlo(&starved, 20, 5);
    assert!(
        (s.mean_damage - 4575.0).abs() < 1e-9,
        "dmg {}",
        s.mean_damage
    );
    // Forced Heat procs refresh the shared timer every shot (the
    // 2.35 s reload gap is well under 10 s): all 13 direct shots
    // boosted = 13 × 435 = 5655. The Heat singleton itself also
    // benefits (mb_live): each proc adds 0.5 × 435 = 217.5 to the
    // tick; ticks at 1..14 carry Σ min(k,12) = 102 contributions →
    // 22,185 — plus ONE accumulator per tick, fourteen of them and not a
    // hundred and two, because Heat consolidates
    // (`Dot::accumulator_unit`): + 14 × 0.5. DoT = 22,192; total 27,847.
    let sustained = FightParams {
        forced_procs: vec![DamageType::Heat],
        ..starved
    };
    let s2 = monte_carlo(&sustained, 20, 5);
    assert!(
        (s2.mean_dot_damage - (22_185.0 + 14.0 * 0.5)).abs() < 1e-6,
        "dot {}",
        s2.mean_dot_damage
    );
    assert!(
        (s2.mean_damage - (27_840.0 + 14.0 * 0.5)).abs() < 1e-6,
        "dmg {}",
        s2.mean_damage
    );
}

#[test]
fn merciless_stacks_join_the_base_damage_bucket_and_decay_one_by_one() {
    // Full 12 stacks × 30% = +360% -> ratio 4.6 (base_damage 0). Within the
    // first 4 s no decay: 4 shots × 75 × 4.6 = 1380.
    let p = FightParams {
        arcane: arc_stacked("secondary_merciless"),
        duration_seconds: 3.9,
        ..flat_base()
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_damage - 1380.0).abs() < 1e-9,
        "dmg {}",
        s.mean_damage
    );
    // Kill family without kills: lose ONE stack per 4 s timeout.
    // Shots t0-3 @12, t4-7 @11, t8-9 @10 stacks:
    // 75 × (4×4.6 + 4×4.3 + 2×4.0) = 3270.
    let p10 = FightParams {
        duration_seconds: 10.0,
        ..p
    };
    let s10 = monte_carlo(&p10, 20, 5);
    assert!(
        (s10.mean_damage - 3270.0).abs() < 1e-9,
        "dmg {}",
        s10.mean_damage
    );
}

#[test]
fn conjunction_voltage_adds_multishot_and_reload_speed() {
    // 40 stacks × 3% multishot = +120% -> 2.2 expected pellets/shot;
    // forced Electricity keeps the shared 12 s timer refreshed.
    let p = FightParams {
        arcane: arc_stacked("conjunction_voltage"),
        forced_procs: vec![DamageType::Electricity],
        ..flat_base()
    };
    let s = monte_carlo(&p, 2000, 7);
    let per_shot = s.mean_pellets / s.mean_shots;
    assert!((per_shot - 2.2).abs() < 0.05, "pellets/shot {per_shot}");
    // Reload speed: 40 × 1.5% = +60% -> 2.35 s / 1.6. A 2-round
    // magazine over 20 s fits in more shots than without the arcane.
    let slow = FightParams {
        magazine_size: 2.0,
        duration_seconds: 20.0,
        forced_procs: vec![DamageType::Electricity],
        ..flat_base()
    };
    let fast = FightParams {
        arcane: arc_stacked("conjunction_voltage"),
        ..slow.clone()
    };
    let a = monte_carlo(&slow, 20, 5);
    let b = monte_carlo(&fast, 20, 5);
    assert!(
        b.mean_shots > a.mean_shots,
        "shots {} vs {}",
        b.mean_shots,
        a.mean_shots
    );
}

#[test]
fn shiver_adds_damage_per_cold_status_on_the_target() {
    // Forced Cold procs land AFTER each shot and last 6 s each: shot k
    // sees min(k, 5) live stacks (older procs lapse), Σ = 35. GunCO
    // bracket on the additive-with-base_damage weapon:
    // 75 × Σ(1 + 0.45 × stacks) = 75 × 25.75 = 1931.25.
    let p = FightParams {
        arcane: arc("secondary_shiver"),
        forced_procs: vec![DamageType::Cold],
        ..flat_base()
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_damage - 1931.25).abs() < 1e-9,
        "dmg {}",
        s.mean_damage
    );
}

#[test]
fn shiver_is_scaled_by_the_gunco_base_fraction() {
    // GunCO sources compute on the ORIGINAL base, excluding evolution
    // flat damage (wiki CO catalog) — Shiver is one of them, so its
    // per-stack bonus scales by co_base_fraction like Galvanized Strike's.
    // Same setup as above with fraction 0.5: 75 × Σ(1 + 0.45×0.5×min(k,5))
    // = 75 × (10 + 0.225 × 35) = 1340.625.
    let p = FightParams {
        arcane: arc("secondary_shiver"),
        forced_procs: vec![DamageType::Cold],
        co_base: crate::model::CoBase::new(0.5, 1.0, crate::model::CoStage::Direct),
        ..flat_base()
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_damage - 1340.625).abs() < 1e-9,
        "dmg {}",
        s.mean_damage
    );
}

#[test]
fn fortifier_multiplies_damage_while_overguard_holds() {
    // ×9 on every direct hit while the (infinite) overguard is up:
    // 10 × 75 × 9 = 6750. NINE, not eight — the card's "x8" is the EXTRA
    // (MEASUREMENTS M38,).
    let mut t = Foe::training_dummy();
    t.base_overguard = 1e9;
    let p = FightParams {
        arcane: arc("secondary_fortifier"),
        foe: t,
        ..flat_base()
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_damage - 6750.0).abs() < 1e-9,
        "dmg {}",
        s.mean_damage
    );
}

#[test]
fn empowered_adds_a_flat_instance_per_applied_status() {
    // Each forced Impact proc adds +750 flat (unscaled by mods/crit):
    // 10 × (75 + 750) = 8250.
    let p = FightParams {
        arcane: arc("cascadia_empowered"),
        forced_procs: vec![DamageType::Impact],
        ..flat_base()
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_damage - 8250.0).abs() < 1e-9,
        "dmg {}",
        s.mean_damage
    );
}

#[test]
fn encumber_rolls_one_extra_random_status_per_pull() {
    // 24% chance per proc-carrying pull, at most one: procs/run ≈
    // 10 × 1.24.
    let p = FightParams {
        arcane: arc("secondary_encumber"),
        forced_procs: vec![DamageType::Impact],
        ..flat_base()
    };
    let s = monte_carlo(&p, 4000, 11);
    assert!((s.mean_procs - 12.4).abs() < 0.3, "procs {}", s.mean_procs);
}

/// The wiki states Encumber's per-shot rate as a CLOSED FORM, and it is
/// about multishot — the one thing the single-pellet test above cannot
/// see:
///
///   1 − (1 − chance × min(statusChance, 1)) ^ pelletCount
///
/// Every pellet that applied a status rolls `chance`, first success wins.
/// With forced procs each of 3 pellets always applies one, so the rate is
/// 1 − 0.76³ = 0.561024, and a 10-shot run lands
/// 10 × (3 forced + 0.561024) = 35.61 procs. The value of pinning this is
/// that the naive readings both give a different number: one roll per
/// PULL would give 10 × 3.24 = 32.4, and one roll per PELLET with no
/// per-instant limit would give 10 × 3.72 = 37.2.
#[test]
fn encumbers_per_shot_rate_matches_the_wikis_closed_form_under_multishot() {
    let p = FightParams {
        arcane: arc("secondary_encumber"),
        forced_procs: vec![DamageType::Impact],
        multishot: 3.0,
        ..flat_base()
    };
    let s = monte_carlo(&p, 4000, 17);
    let want = 10.0 * (3.0 + (1.0 - 0.76_f64.powi(3)));
    assert!(
        (s.mean_procs - want).abs() < 0.3,
        "procs {} vs closed form {want}",
        s.mean_procs
    );
    // And it must NOT be either naive reading.
    assert!((s.mean_procs - 32.4).abs() > 0.5, "one roll per PULL");
    assert!((s.mean_procs - 37.2).abs() > 0.5, "one roll per PELLET, uncapped");
}

#[test]
fn cryogenic_cold_bursts_raise_crit_damage_received() {
    // Rank 5: every Puncture status also applies 3 Cold stacks; Cold
    // raises crit damage RECEIVED, so a guaranteed-crit run does more
    // damage with the arcane than without.
    let base = FightParams {
        base_crit_chance: 1.0,
        crit_multiplier: 2.0,
        forced_procs: vec![DamageType::Puncture],
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let with = FightParams {
        arcane: arc("secondary_cryogenic"),
        ..base.clone()
    };
    let a = monte_carlo(&base, 300, 5);
    let b = monte_carlo(&with, 300, 5);
    assert!(
        b.mean_damage > a.mean_damage * 1.05,
        "with {} vs without {}",
        b.mean_damage,
        a.mean_damage
    );
}

#[test]
fn surge_assumed_max_is_a_final_multiplier() {
    // AssumedMax: the ×8 cap on every shot — 10 × 75 × 8 = 6000.
    let fx = crate::data::arcanes::secondary("secondary_surge")
        .unwrap()
        .fx(5, crate::model::StackPolicy::AssumedMax, &[], crate::data::tenno::default_tenno());
    let p = FightParams {
        arcane: fx,
        ..flat_base()
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_damage - 6000.0).abs() < 1e-9,
        "dmg {}",
        s.mean_damage
    );
}

#[test]
fn hemorrhage_converts_impact_procs_to_bleeds() {
    // Fire rate 1 < 2.5: chance 0.35 × 2 = 0.7 per damage instance —
    // forced Impact procs seed Slash bleeds (35% ticks of ModifiedBase).
    let with = FightParams {
        proc_conversion: Some(crate::build::loadout::ProcConv {
            from: DamageType::Impact,
            to: DamageType::Slash,
            chance: 0.35,
            low_rate_threshold: 2.5,
            low_rate_multiplier: 2.0,
        }),
        forced_procs: vec![DamageType::Impact],
        ..flat_base()
    };
    let without = FightParams {
        proc_conversion: None,
        ..with.clone()
    };
    let a = monte_carlo(&without, 200, 5);
    let b = monte_carlo(&with, 200, 5);
    assert!(a.mean_dot_damage == 0.0, "impact alone must not bleed");
    assert!(b.mean_dot_damage > 0.0, "hemorrhage must bleed");
    // ~0.7 of 10 shots convert; ticks land at t+1..t+6 but only while
    // t < 10, so shot k contributes min(6, 9−k) ticks — 39 total.
    let expect = 0.7 * 39.0 * 0.35 * 75.0;
    assert!(
        (b.mean_dot_damage / expect - 1.0).abs() < 0.10,
        "dot {} vs expect {expect}",
        b.mean_dot_damage
    );
}

/// PROC CONVERSION'S THREE UNWRITTEN RULES — the ones the card states in
/// its Notes and the test above never touched.
///
/// All three are Magnetic Welt's wording and all three
/// are Hemorrhage's too, since they are one effect:
///
/// 1. **Exactly 2.5 does NOT get the 2x.** "If the weapon's fire rate is
///    exactly 2.5, it will not receive the 2x bonus" — a STRICT `<`. This
///    is not a corner: `strun`, `strun_wraith` and `strun_prime_incarnon`
///    are all listed at exactly 2.50, so the boundary decides the mod's
///    value on three roster weapons.
/// 2. **One roll per damage instance, however many Impact procs land.**
///    "Proccing Impact more than once in a single instance of damage will
///    not allow this mod to proc more than once, nor will it increase the
///    chance of the mod proccing."
/// 3. **Nothing if the instance already carries the target status.**
///    "Cannot produce multiple procs in a single instance of damage
///    alongside any other Magnetic sources such as a weapon's innate
///    Magnetic."
#[test]
fn proc_conversion_obeys_its_three_notes() {
    let welt = |rate: f64, forced: Vec<DamageType>| FightParams {
        fire_rate: rate,
        proc_conversion: Some(crate::build::loadout::ProcConv {
            from: DamageType::Impact,
            to: DamageType::Slash,
            chance: 0.35,
            low_rate_threshold: 2.5,
            low_rate_multiplier: 2.0,
        }),
        forced_procs: forced,
        ..flat_base()
    };
    // Per-shot bleed, so the fire rate does not smuggle itself into the
    // comparison by changing how many shots land.
    let per_shot = |p: &FightParams| {
        let s = monte_carlo(p, 400, 5);
        s.mean_dot_damage / f64::from(monte_carlo(p, 400, 5).mean_shots.max(1.0) as u32)
    };
    let imp = vec![DamageType::Impact];

    // 1. THE BOUNDARY. 2.49 doubles, 2.50 does not — and the gap is the
    //    factor 2 itself, so nothing subtler could be mistaken for it.
    let under = per_shot(&welt(2.49, imp.clone()));
    let exactly = per_shot(&welt(2.50, imp.clone()));
    assert!(
        (under / exactly - 2.0).abs() < 0.15,
        "2.49 must double and 2.50 must not: {under} vs {exactly}"
    );

    // 2. A SECOND IMPACT PROC IN THE SAME INSTANCE CHANGES NOTHING. Two
    //    forced Impacts are still one membership test and one roll.
    let twice = per_shot(&welt(2.49, vec![DamageType::Impact, DamageType::Impact]));
    assert!(
        (twice / under - 1.0).abs() < 0.12,
        "a second Impact proc must not add a roll: {twice} vs {under}"
    );

    // 3. AN INSTANCE THAT ALREADY CARRIES THE TARGET STATUS gets nothing —
    //    the innate-Slash case, worth exactly the same as no mod at all.
    let already = welt(2.49, vec![DamageType::Impact, DamageType::Slash]);
    let bare = FightParams { proc_conversion: None, ..already.clone() };
    let a = monte_carlo(&already, 400, 5).mean_dot_damage;
    let b = monte_carlo(&bare, 400, 5).mean_dot_damage;
    assert!(
        (a / b - 1.0).abs() < 1e-9,
        "an innate source must shut the mod out entirely: {a} vs {b}"
    );
}

#[test]
fn weakpoint_damage_adds_into_the_part_multiplier_at_1_5x() {
    // Acuity r10 on a 3x head, 100% weak-point aim: 3 + 3.5 × 1.5 =
    // 8.25x -> 10 × 75 × 8.25 = 6187.5 (wiki Pistol_Acuity example).
    let p = FightParams {
        weakpoint_damage: 3.5,
        crit_tier_upgrade_chance: 0.0,
        slash_on_crit: 0.0,
        body_parts: vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: false,
        }],
        ..flat_base()
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_damage - 6187.5).abs() < 1e-9,
        "dmg {}",
        s.mean_damage
    );
}

/// A WEAPON MAY OVERRULE WHAT A HEAD IS WORTH, and the Tenet Arca Plasmor
/// is the roster's first: *"1x headshot multiplier (meaning it does no extra
/// damage)"*, on an enemy whose head is worth 3x.
///
/// Three claims, because the field is only right if all three hold. The
/// enemy's multiplier is REPLACED rather than reduced; a headshot MOD still
/// pays on top of the replacement, which is the wiki's own next sentence
/// (*"this can be increased using Primary Deadhead"*); and the critical
/// headshot doubling goes quiet, since the rule it comes from is about a
/// weak point worth more than 1x and this head no longer is.
#[test]
fn a_weapon_may_overrule_what_a_head_is_worth() {
    let head3x = |m: Option<f64>, deadhead: f64| FightParams {
        headshot_multiplier: m,
        arcane: ArcaneFx { headshot_multiplier_bonus: deadhead, ..ArcaneFx::none() },
        crit_tier_upgrade_chance: 0.0,
        slash_on_crit: 0.0,
        body_parts: vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: true,
        }],
        ..flat_base()
    };
    let dmg = |p: &FightParams| monte_carlo(p, 20, 5).mean_damage;

    // 1. THE PART'S 3x IS REPLACED BY THE WEAPON'S 1x — not scaled by it,
    //    and not left standing beside it.
    let ordinary = dmg(&head3x(None, 0.0));
    let flat = dmg(&head3x(Some(1.0), 0.0));
    assert!(
        (ordinary / flat - 3.0).abs() < 1e-9,
        "a 1x weapon on a 3x head must be worth exactly a third: {ordinary} vs {flat}"
    );

    // 2. A HEADSHOT MOD STILL PAYS, on top of the weapon's own multiplier —
    //    Primary Deadhead at +120% is 1 x (1 + 1.2).
    let deadhead = dmg(&head3x(Some(1.0), 1.2));
    assert!(
        (deadhead / flat - 2.2).abs() < 1e-9,
        "Deadhead must still pay on a 1x head: {deadhead} vs {flat}"
    );

    // 3. …AND THE BODY IS UNTOUCHED, which is what says the override is
    //    scoped to the head rather than to the weapon's whole output.
    let body = |m: Option<f64>| {
        dmg(&FightParams {
            body_parts: vec![BodyPart {
                name: "body".into(),
                aim_weight: 1.0,
                multiplier: 1.0,
                is_head: false,
                crit_bonus: false,
            }],
            ..head3x(m, 0.0)
        })
    };
    assert!(
        (body(Some(1.0)) - body(None)).abs() < 1e-9,
        "the override must not reach a body hit: {} vs {}",
        body(Some(1.0)),
        body(None)
    );
}

#[test]
fn weakpoint_crit_chance_applies_on_weakpoint_pellets_only() {
    // +100% absolute cc on head hits with cd 2.0: every head pellet
    // tier-1 crits (×2); body pellets never crit. 100% head aim:
    // 10 × 75 × 2 = 1500.
    let head = FightParams {
        // RELATIVE now: 1.0 x a base of 1.0 = +100% absolute.
        weakpoint_crit_chance_relative: 1.0,
        unmodded_crit_chance: 1.0,
        crit_multiplier: 2.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        body_parts: vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: true,
            crit_bonus: false,
        }],
        ..no_status()
    };
    let s = monte_carlo(&head, 20, 5);
    assert!(
        (s.mean_damage - 1500.0).abs() < 1e-9,
        "dmg {}",
        s.mean_damage
    );
    let body = FightParams {
        body_parts: mono_body(1.0),
        ..head
    };
    let s2 = monte_carlo(&body, 20, 5);
    assert!(
        (s2.mean_damage - 750.0).abs() < 1e-9,
        "dmg {}",
        s2.mean_damage
    );
}

#[test]
fn sharpened_bullets_cd_buff_refreshes_on_kills() {
    // Frail 50 HP respawning targets: kills keep the +100%-absolute cd
    // buff up; guaranteed crits make the buff visible in raw damage.
    let mut frail = Foe::training_dummy();
    frail.base_health = 50.0;
    frail.mode = TargetMode::InstantRespawn;
    let base = FightParams {
        base_crit_chance: 1.0,
        crit_multiplier: 2.0,
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        foe: frail,
        ..no_status()
    };
    let with = FightParams {
        crit_damage_on_kill: Some(crate::model::TimedBuff {
            value: 1.0,
            duration: 9.0,
            initial_active: false,
        }),
        ..base.clone()
    };
    let a = monte_carlo(&base, 50, 5);
    let b = monte_carlo(&with, 50, 5);
    assert!(
        b.mean_damage > a.mean_damage * 1.3,
        "with {} vs without {}",
        b.mean_damage,
        a.mean_damage
    );
}

#[test]
fn pressurized_magazine_fire_rate_buff_follows_reloads() {
    // A 2-round magazine reloads constantly; +100%-absolute fire rate
    // for 9 s after each reload fits in more shots over 20 s.
    let base = FightParams {
        magazine_size: 2.0,
        duration_seconds: 20.0,
        ..flat_base()
    };
    let with = FightParams {
        fire_rate_on_reload: Some(crate::model::TimedBuff {
            value: 1.0,
            duration: 9.0,
            initial_active: false,
        }),
        ..base.clone()
    };
    let a = monte_carlo(&base, 20, 5);
    let b = monte_carlo(&with, 20, 5);
    assert!(
        b.mean_shots > a.mean_shots,
        "shots {} vs {}",
        b.mean_shots,
        a.mean_shots
    );
}

#[test]
fn shield_depletion_counts_toward_kill_progress() {
    // Pure Impact never reaches health, but denting the SHIELD now earns
    // partial credit: the whole overguard+shield+health
    // bar counts, so shield damage — and regen — moves the score). One
    // 75-Impact shot into a 1000+1000 = 2000 bar -> 75/2000 = 0.0375,
    // with health still full (0 kills).
    let p = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 75.0),
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        foe: shielded_target(1000.0, 1000.0),
        duration_seconds: 1.0,
        ..no_status()
    };
    let s = monte_carlo(&p, 10, 7);
    assert_eq!(s.mean_kills, 0.0);
    assert!(
        (s.mean_kill_progress - 0.0375).abs() < 1e-9,
        "score {}",
        s.mean_kill_progress
    );
}

#[test]
fn toxin_share_bypasses_shields_into_health() {
    // Vector 16 Toxin / 16 Impact (quantization-exact): each 32-damage
    // shot sends 16 to the shield and 16 straight to health. Health
    // 160 dies exactly on the 10th shot while 840 shield remains.
    let p = FightParams {
        damage: DamageVector::new()
            .with(DamageType::Toxin, 16.0)
            .with(DamageType::Impact, 16.0),
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        foe: shielded_target(1000.0, 160.0),
        ..no_status()
    };
    let s = monte_carlo(&p, 20, 5);
    assert!((s.mean_kills - 1.0).abs() < 1e-9, "kills {}", s.mean_kills);
    // Control: an all-Impact vector never touches health.
    let q = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 32.0),
        ..p
    };
    let s2 = monte_carlo(&q, 20, 5);
    assert_eq!(s2.mean_kills, 0.0);
}

#[test]
fn a_broken_shield_leaks_and_the_window_costs_gunfire_nothing() {
    // Shield 100, 75-damage shots at 20/s (0.05 s cadence).
    //
    //   t=0.00  75 into a 100 shield  -> 75 absorbed, 25 left
    //   t=0.05  75 into the last 25   -> 25 absorbed, 50 overflow, of which
    //                                    5% (2.50) reaches health
    //   t=0.10  the window is up      -> 75, because no gun takes it
    //   t=0.15  the window is over    -> 75
    //
    // Effective = 75 + 27.50 + 75 + 75 = **252.50**.
    //
    // TWO CORRECTIONS, BOTH MEASURED, AND THEY PULL OPPOSITE WAYS
    // (MEASUREMENTS M61, wiki `Shield`). It read 228.75 when the breaking
    // hit was charged to the shield IN FULL — counted as 75 of damage dealt
    // while the 50 it had left over went nowhere — and 181.25 once the
    // overflow landed but the 0.1 s window still quartered the next shot.
    //
    // The window is real and it is not gunfire's: only a melee GROUND SLAM
    // was measured taking it, and a gun's AoE does not either. This engine fires guns and nothing else, so nothing here
    // takes it — the state is still set, and `record::TargetAt` draws it,
    // so the day melee lands the field it needs is already right.
    let p = FightParams {
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        fire_rate: 20.0,
        duration_seconds: 0.2,
        magazine_size: 100.0,
        body_parts: mono_body(1.0),
        foe: shielded_target(100.0, 1e9),
        ..no_status()
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_effective_damage - 252.5).abs() < 1e-9,
        "eff {}",
        s.mean_effective_damage
    );
    // …AND THE WINDOW IS STILL RECORDED, which is the half that has to
    // survive for melee to be one line of work rather than a rebuild: a row
    // that lands while it is up says so, and nothing reduces it.
    let rec = record(&p, monte_carlo(&p, 4, 5).median_run.rng_state, 0.0, 1.0, 5_000, 0);
    let windows: Vec<f64> = rec
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            crate::record::Kind::Damage(d) => d.before.shield_gate_until,
            _ => None,
        })
        .collect();
    assert!(
        !windows.is_empty(),
        "a broken shield still opens a window, even though nothing reads it"
    );
    // Weakpoint hits bypass the breaking hit's overflow gate, so all-head
    // aim delivers every shot whole: 4 × 75 = 300. It was 300 under both
    // earlier readings too, which is what makes it the control: neither fix
    // may move the case the rule says it does not touch.
    let head = FightParams {
        body_parts: vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: true,
            crit_bonus: false,
        }],
        ..p
    };
    let s2 = monte_carlo(&head, 20, 5);
    assert!(
        (s2.mean_effective_damage - 300.0).abs() < 1e-9,
        "eff {}",
        s2.mean_effective_damage
    );
}

#[test]
fn attenuation_caps_damage_per_instance_and_per_second() {
    // Instance cap 5% × 1000 HP = 50: each 75 shot clamps to 50.
    let mut t = shielded_target(0.0, 1000.0);
    t.base_health = 1000.0;
    t.mode = TargetMode::InfiniteHealth;
    t.attenuation = Some(Attenuation {
        instance_fraction: 0.05,
        dps_fraction: 0.50,
    });
    let p = FightParams {
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        foe: t,
        ..no_status()
    };
    let s = monte_carlo(&p, 20, 5);
    assert!(
        (s.mean_effective_damage - 500.0).abs() < 1e-9,
        "eff {}",
        s.mean_effective_damage
    );
    // DPS cap 50%/s = 500: at 20 shots/s only the first 10 clamped
    // shots fit the 1 s bucket -> 500 total in the 1 s run.
    let q = FightParams {
        fire_rate: 20.0,
        duration_seconds: 1.0,
        magazine_size: 100.0,
        ..p
    };
    let s2 = monte_carlo(&q, 20, 5);
    assert!(
        (s2.mean_effective_damage - 500.0).abs() < 1e-9,
        "eff {}",
        s2.mean_effective_damage
    );
}

#[test]
fn crosshairs_buff_is_refreshed_by_headshot_hits() {
    // Head aim: every hit refreshes the +0.12 buff, so all 18 shots
    // in 20 s (12-mag + reload) fire at cc 0.22 with cd 2:
    // E = 18 × 75 × 1.22 = 1647.
    let p = FightParams {
        base_crit_chance: 0.1,
        // The buff value is RELATIVE now (it joins the crit bucket, so it
        // scales each part's own base). A base of 1.0 makes 0.12 land as
        // +12% absolute, leaving the arithmetic above unchanged.
        unmodded_crit_chance: 1.0,
        crit_chance_on_headshot: Some(crate::model::TimedBuff {
            value: 0.12,
            duration: 12.0,
            initial_active: true,
        }),
        arcane: ArcaneFx::none(),
        body_parts: vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: true,
            crit_bonus: false,
        }],
        duration_seconds: 20.0,
        ..no_status()
    };
    let s = monte_carlo(&p, 4000, 23);
    assert!(
        (s.mean_damage - 1647.0).abs() / 1647.0 < 0.02,
        "dmg {}",
        s.mean_damage
    );
}

#[test]
fn crosshairs_cc_buffs_start_full_and_expire_without_headshots() {
    // Body-only aim: nothing refreshes the initial-full buffs, so both
    // lapse at 12 s. cc: 0.1 + 0.12 + 5×0.04 = 0.42 before, 0.1 after.
    // 20 s at 1/s with the 12-mag: shots 0..11 (all < 12 s), reload
    // 2.35, shots 14.35..19.35 (6 bare). cd 2 -> E = 75 × (1 + cc):
    // E[total] = 75 × (12×1.42 + 6×1.1) = 1773.
    let p = FightParams {
        base_crit_chance: 0.1,
        unmodded_crit_chance: 1.0, // relative buff values — see above
        crit_chance_on_headshot: Some(crate::model::TimedBuff {
            value: 0.12,
            duration: 12.0,
            initial_active: true,
        }),
        crit_chance_stack: Some(crate::model::StackSpec {
            per_stack: 0.04,
            max_stacks: 5,
            duration: 12.0,
            initial_stacks: 5,
            earned_on: Some("headshot_kill"),
        }),
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        duration_seconds: 20.0,
        ..no_status()
    };
    let s = monte_carlo(&p, 4000, 21);
    assert!(
        (s.mean_damage - 1773.0).abs() / 1773.0 < 0.02,
        "dmg {}",
        s.mean_damage
    );
}

#[test]
fn kill_progress_gives_partial_credit_for_depleted_pools() {
    // 1000 HP target, one 75-damage shot in the window: 0 kills but
    // 7.5% of the pool depleted -> score 0.075.
    let mut t = frail_target(TargetMode::InstantRespawn, 0.0, 0.0);
    t.base_health = 1000.0;
    let p = FightParams {
        crit_multiplier: 1.0,
        body_parts: mono_body(1.0),
        foe: t,
        duration_seconds: 1.0,
        ..no_status()
    };
    let s = monte_carlo(&p, 10, 3);
    assert_eq!(s.mean_kills, 0.0);
    assert!(
        (s.mean_kill_progress - 0.075).abs() < 1e-9,
        "score {}",
        s.mean_kill_progress
    );
}

#[test]
fn frozen_state_machine_follows_the_recorded_rules() {
    let mut d = DebuffState::default();
    // Nine Freeze stacks build normally (no overguard).
    for k in 0..9 {
        d.apply_cold_proc(k as f64 * 0.1, 1.0, false, None, false);
    }
    assert_eq!(d.freeze.len(), 9);
    assert!((d.cold_cd_bonus(0.9) - 0.50).abs() < 1e-9); // 0.10+0.05×8
    // THE 10TH PROC IS BOTH the tenth stack and the Frozen trigger — the
    // ladder KEEPS it. This assertion read
    // `d.freeze.is_empty()` under the model where the tenth proc consumed
    // the pile, which made the game's own "10 stacks" display Frozen's
    // cosmetic; a target that cannot freeze reaches ten and cycles there,
    // so the ten are real.
    d.apply_cold_proc(1.0, 1.0, false, None, false);
    assert_eq!(d.freeze.len(), 10);
    assert_eq!(d.frozen_until, Some(4.0));
    assert!((d.cold_cd_bonus(1.5) - 1.00).abs() < 1e-9); // REPLACES the ladder
    // Cold procs are inert while Frozen, which is what pins the ten.
    d.apply_cold_proc(2.0, 1.0, false, None, false);
    assert_eq!(d.freeze.len(), 10);
    // Thaw: hard reset to exactly 3 stacks with FRESH 6 s timers
    // anchored at the thaw instant (expire at 4 + 6 = 10 s).
    d.prune(4.5, 1.0);
    assert_eq!(d.frozen_until, None);
    assert_eq!(d.freeze.len(), 3);
    d.prune(9.9, 1.0);
    assert_eq!(d.freeze.len(), 3);
    d.prune(10.1, 1.0);
    assert!(d.freeze.is_empty());
}

#[test]
fn overguard_caps_freeze_at_four_and_never_freezes() {
    let mut d = DebuffState::default();
    for k in 0..20 {
        d.apply_cold_proc(k as f64 * 0.1, 1.0, true, None, false);
    }
    assert_eq!(d.freeze.len(), 4);
    assert_eq!(d.frozen_until, None);
    assert!((d.cold_cd_bonus(2.0) - 0.25).abs() < 1e-9); // 0.10+0.05×3
}

#[test]
fn heat_strip_ramps_up_and_decays_after_the_entity_dies() {
    let mut d = DebuffState {
        heat: Some(HeatEntity {
            bracket: 1.0,
            depth: 0,
            unit: 0.0,
            born: 0.0,
            expiry: 6.0,
            next_tick: 1.0,
            value: 1.0,
            recent: Vec::new(),
            stacks: 1,
        }),
        ..Default::default()
    };
    // Ramp-up: 0.5 s steps 15/30/40/50%.
    assert_eq!(d.heat_strip(0.4, 1.0), 0.0);
    assert_eq!(d.heat_strip(0.6, 1.0), 0.15);
    assert_eq!(d.heat_strip(1.6, 1.0), 0.40);
    assert_eq!(d.heat_strip(2.5, 1.0), 0.50);
    // Entity dies at 6.0 -> ramp-down every 1.5 s: 50/40/30/15/0.
    d.prune(6.5, 1.0);
    assert!(d.heat.is_none());
    assert_eq!(d.heat_strip(6.5, 1.0), 0.50);
    assert_eq!(d.heat_strip(7.6, 1.0), 0.40);
    assert_eq!(d.heat_strip(9.1, 1.0), 0.30);
    assert_eq!(d.heat_strip(10.6, 1.0), 0.15);
    assert_eq!(d.heat_strip(12.1, 1.0), 0.0);
}
