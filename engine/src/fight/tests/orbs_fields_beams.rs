use super::*;

/// The reference case, and the arithmetic the whole field model rests on:
/// ONE grenade at t=0 leaves TEN 40-damage ticks at t=0..9 — ✅ measured
/// (MEASUREMENTS M13): the first lands WITH the impact, then nine more over
/// the remaining nine seconds. A 10 s engagement sees all ten.
#[test]
fn one_grenade_leaves_ten_ticks_starting_with_the_impact() {
    let p = FightParams {
        damage: DamageVector::default(), // inert impact: the field alone
        lingering: Some(cloud(crate::model::FieldStacking::Stack)),
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        magazine_size: 1.0,
        reload_seconds: 999.0, // exactly one shot in the window
        infinite_reserve: false,
        reserve_ammo: 0.0,
        ..no_status()
    };
    let s = monte_carlo(&p, 4, 3);
    assert!((s.mean_shots - 1.0).abs() < 1e-9, "shots {}", s.mean_shots);
    // Ticks at 0..9, all ten inside a 10 s run.
    assert!(
        (s.mean_field_ticks - 10.0).abs() < 1e-9,
        "field ticks {}",
        s.mean_field_ticks
    );
    assert!(
        (s.mean_damage - 10.0 * 40.0).abs() < 1e-9,
        "dmg {} (expected 10 x 40 = the field's full 400)",
        s.mean_damage
    );
}

/// ONE THROW, SIX STRIKES, ONE A SECOND — and they are the orb's, not a
/// collision's.
///
/// The whole difference between this and a lingering field is WHO gets hit, so the count is asserted against the COMBAT RECORD
/// rather than a total: six rows, every one of them `Orb`, at t=0..5. A
/// total cannot tell six strikes from one collision and five field ticks,
/// which is exactly the model this replaced.
#[test]
fn one_orb_strikes_six_times_a_second_apart_and_none_of_them_is_a_collision() {
    let p = orb_thrower();
    let s = monte_carlo(&p, 4, 3);
    assert!((s.mean_shots - 1.0).abs() < 1e-9, "shots {}", s.mean_shots);
    assert!(
        (s.mean_damage - 6.0 * 280.0).abs() < 1e-9,
        "dmg {} (expected six strikes of 280)",
        s.mean_damage
    );
    let rec = record(&p, 7, 0.0, 20.0, 100, 0);
    let rows: Vec<(f64, crate::record::Origin)> = rec
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            crate::record::Kind::Damage(d) => Some((e.t, d.origin)),
            _ => None,
        })
        .collect();
    assert_eq!(rows.len(), 6, "six numbers, got {rows:?}");
    for (k, (at, origin)) in rows.iter().enumerate() {
        assert!(
            (at - k as f64).abs() < 1e-9 && *origin == crate::record::Origin::Orb,
            "the strikes are the ORB's, one a second from the throw: {rows:?}"
        );
    }
}

/// A STRIKE WITH NOBODY IN REACH IS SPENT, which is what makes the count
/// the geometry's rather than a number written down.
///
/// The orb leaves the muzzle at 6 m/s and reaches 6 m of its own, so a
/// target further out than that has to be flown to — and every second of
/// the flight costs a strike. `ceil(6 - flight)` is the owner's rule
/// (M63); this asserts the two ends of it and the shape between them.
#[test]
fn a_strike_with_nobody_in_reach_is_spent() {
    let strikes = |gap_m: f64| {
        let p = FightParams {
            target_at: crate::rules::space::Vec2::new(gap_m, 0.0),
            ..orb_thrower()
        };
        (monte_carlo(&p, 4, 3).mean_damage / 280.0).round() as u32
    };
    // AT CONTACT the target is inside the reach from the muzzle, so the
    // first strike lands at the throw and all six do.
    assert_eq!(strikes(crate::rules::space::CONTACT_RANGE_M), 6, "at contact");
    // …and far enough out that the orb spends its whole fuse getting
    // there, none of them does. 6 m/s x 6 s is 36 m of travel, and the
    // reach adds 6 more.
    assert_eq!(strikes(60.0), 0, "out of reach for the whole fuse");
    // IN BETWEEN IT FALLS OFF ONE AT A TIME rather than all at once, which
    // is the property a two-point test cannot see.
    let ladder: Vec<u32> = [12.0, 18.0, 24.0, 30.0].iter().map(|&g| strikes(g)).collect();
    assert!(
        ladder.windows(2).all(|w| w[0] >= w[1]) && ladder[0] > ladder[3],
        "strikes fall as the throw gets longer: {ladder:?}"
    );
}

/// AN UNAIMED ATTACK DECIDES WHERE ITS OWN INSTANCES LAND.
///
/// The orb picks its body — *"shock 1 enemy within 6 meters of it every 1
/// second"* — so the scenario's `headshot_pct`, which describes the
/// player's aim, is the wrong number for every strike. Pinned at 0 and 1
/// rather than the weapon's own 0.1 so both arms are arithmetic instead of
/// a sample.
#[test]
fn an_unaimed_attack_decides_where_its_own_strikes_land() {
    let run = |chance: Option<f64>| {
        monte_carlo(
            &FightParams {
                orb: Some(crate::build::loadout::ResolvedOrb {
                    unaimed_headshot_chance: chance,
                    speed_after_contact_mps: 0.0,
                    ..orb_spec()
                }),
                // The aim weights say BODY, so an arm that comes back with
                // heads can only have got them from the unaimed chance.
                body_parts: vec![
                    BodyPart {
                        name: "head".into(),
                        aim_weight: 0.0,
                        multiplier: 3.0,
                        is_head: true,
                        is_weak_point: true,
                        crit_bonus: false,
                    },
                    BodyPart {
                        name: "body".into(),
                        aim_weight: 1.0,
                        multiplier: 1.0,
                        is_head: false,
                        is_weak_point: false,
                        crit_bonus: false,
                    },
                ],
                ..orb_thrower()
            },
            4,
            3,
        )
        .mean_damage
    };
    assert!(
        (run(None) - 6.0 * 280.0).abs() < 1e-9,
        "with no unaimed chance the aim weights decide, and they say body: {}",
        run(None)
    );
    assert!(
        (run(Some(1.0)) - 6.0 * 3.0 * 280.0).abs() < 1e-9,
        "all six on a 3x head: {}",
        run(Some(1.0))
    );
}

/// …AND A HEAD STRIKE TAKES THE CRITICAL-LOCATION FOLD-IN, because nothing
/// says it should not.
///
/// `Critical_Hit` §Critical Headshots doubles `cd` inside the tier formula
/// on an eligible >1x location, and a strike that *"can hit weakspots"*
/// (wiki, this weapon) is a hit on one. Inventing an exception for the orb
/// would be a claim with no measurement behind it; taking the rule as it
/// stands is not.
///
/// A 3x head at 100% crit: without the fold-in `1 + 1x(2 - 1) = 2`, with it
/// `1 + 1x(4 - 1) = 4`, so the eligible target is worth exactly twice the
/// ineligible one and `crit_bonus` is the only thing separating the arms.
#[test]
fn an_orb_strike_on_an_eligible_head_folds_the_crit_in() {
    let run = |crit_bonus: bool| {
        let mut part = orb_part(280.0);
        part.crit_chance = 1.0;
        if let Some(inputs) = part.critical_inputs.as_mut() { inputs.set_supplied_chance(part.crit_chance, part.base_crit_chance); }
        part.base_crit_chance = 1.0;
        if let Some(inputs) = part.critical_inputs.as_mut() { inputs.set_supplied_chance(part.crit_chance, part.base_crit_chance); }
        part.crit_damage = 2.0;
        if let Some(inputs) = part.critical_inputs.as_mut() { inputs.set_supplied_damage(part.crit_damage, part.base_crit_damage, None); }
        monte_carlo(
            &FightParams {
                orb: Some(crate::build::loadout::ResolvedOrb {
                    unaimed_headshot_chance: Some(1.0),
                    speed_after_contact_mps: 0.0,
                    ..orb_spec()
                }),
                orb_strike: Some(part),
                body_parts: vec![BodyPart {
                    name: "head".into(),
                    aim_weight: 1.0,
                    multiplier: 3.0,
                    is_head: true,
                    is_weak_point: true,
                    crit_bonus,
                }],
                ..orb_thrower()
            },
            4,
            3,
        )
        .mean_damage
    };
    let plain = run(false);
    let folded = run(true);
    assert!(
        (plain - 6.0 * 3.0 * 280.0 * 2.0).abs() < 1e-6,
        "the ineligible arm is six 3x heads at the plain 2x crit: {plain}"
    );
    assert!(
        (folded - 2.0 * plain).abs() < 1e-6,
        "an eligible head is worth twice an ineligible one: {folded} against {plain}"
    );
}

/// A STRIKE FORCES ITS OWN PROCS, and the detonation forces none.
///
/// *"Every strike from the alternate fire has a forced Electricity status
/// effect"* — and the explosion does not, which is one attack answering the
/// same question both ways. At ZERO status chance nothing can proc at all,
/// so any burn here came from the forced list, and the bare arm is the
/// negative control a presence-only assertion would not have.
#[test]
fn an_orb_strike_forces_its_own_procs_and_one_declaring_none_gets_none() {
    let bare = monte_carlo(&orb_thrower(), 4, 3).mean_damage;
    let forced = {
        let mut part = orb_part(280.0);
        part.forced_procs =
            crate::rules::damage::ForcedProcs::from_types([DamageType::Electricity]);
        monte_carlo(&FightParams { orb_strike: Some(part), ..orb_thrower() }, 4, 3)
            .mean_damage
    };
    assert!(
        (bare - 6.0 * 280.0).abs() < 1e-9,
        "the control is the six bare strikes, got {bare}"
    );
    assert!(
        forced > bare,
        "a forced Electricity proc per strike must burn something: {forced} against {bare}"
    );
}

/// THE FUSE ENDS IN A DETONATION, and it goes off WHERE THE ORB IS.
///
/// Every other explosion in this engine goes off AT a body, so where it
/// happens never had to be said. This one happens at a PLACE the orb
/// reached, and the two arms are the same fight with the orb held and let
/// go: held, its blast lands on the body it stopped at; drifting at 2 m/s
/// for six seconds it is twelve metres away and its six-metre blast
/// reaches nobody.
///
/// A detonation that landed either way would prove the position was
/// decoration, which is the one thing an entity model must not be.
#[test]
fn the_detonation_goes_off_where_the_orb_got_to() {
    let mut blast = orb_part(200.0);
    blast.radius_m = 6.0;
    blast.falloff_start_m = 0.0;
    blast.falloff_reduction = 0.8;
    let held = FightParams { orb_blast: Some(blast), ..orb_thrower() };
    let held_damage = monte_carlo(&held, 4, 3).mean_damage;
    assert!(
        (held_damage - (6.0 * 280.0 + 200.0)).abs() < 1e-9,
        "a stationary orb detonates on the body it stopped at: {held_damage}"
    );
    let drifting = monte_carlo(
        &FightParams { orb: Some(orb_spec()), ..held },
        4,
        3,
    )
    .mean_damage;
    assert!(
        drifting < held_damage - 199.0,
        "an orb twelve metres past the target detonates on nobody: {drifting} against {held_damage}"
    );
}

/// AN ORB THAT DRIFTS LEAVES A LONE TARGET BEHIND, and six strikes on one
/// standing body is GEOMETRICALLY IMPOSSIBLE at the numbers we have — which
/// is arithmetic rather than a simulation result:
///
/// ```text
///   approach   (reach + body) / launch speed   6.25 / 6 = 1.04 s
///   departure  (reach + body) / slowed speed   6.25 / 2 = 3.13 s
///                                              ------------------
///   at contact                    (no approach)          3.13 s   -> 4
///   thrown from beyond the reach                         4.17 s   -> 5
/// ```
///
/// Six strikes a second apart need more than five seconds in reach.
///
/// THE ANSWER IS THE ARENA rather than any of these numbers: the orb
/// BOUNCES off walls and this arena has none, so every value is right and
/// the fight is an open field — +55.6% on the same build with the drift
/// taken out, which is why the weapon's page calls this a floor.
#[test]
fn an_orb_that_drifts_leaves_a_lone_target_behind() {
    let strikes = |gap_m: f64, orb: crate::build::loadout::ResolvedOrb| {
        let p = FightParams {
            target_at: crate::rules::space::Vec2::new(gap_m, 0.0),
            orb: Some(orb),
            ..orb_thrower()
        };
        (monte_carlo(&p, 4, 3).mean_damage / 280.0).round() as u32
    };
    // AT CONTACT the departure window alone decides it: 6.25 / 2 = 3.13 s,
    // which holds four strike instants.
    assert_eq!(strikes(crate::rules::space::CONTACT_RANGE_M, orb_spec()), 4, "at contact");
    // …AND NO THROW DISTANCE BUYS SIX. The approach adds at most another
    // 1.04 s, so five is the ceiling — asserted across the whole range
    // rather than at a distance somebody picked, because "six somewhere"
    // is exactly the claim being tested.
    let ladder: Vec<u32> = (0..=30)
        .map(|g| strikes(f64::from(g).max(crate::rules::space::CONTACT_RANGE_M), orb_spec()))
        .collect();
    assert!(
        ladder.iter().all(|&n| n <= 5),
        "the in-reach window cannot hold six: {ladder:?}"
    );
    assert!(ladder.contains(&5), "and it does reach five: {ladder:?}");
    // WHAT WOULD BUY SIX, which is how the wall was found: neither of
    // these is the real answer, and the second one is what a room does to
    // an orb that bounces. A slower drift does…
    let slower = crate::build::loadout::ResolvedOrb {
        speed_after_contact_mps: 1.2,
        ..orb_spec()
    };
    assert_eq!(strikes(crate::rules::space::CONTACT_RANGE_M, slower), 6, "at 1.2 m/s");
    // …and so does an orb that stops where it touches.
    let held = crate::build::loadout::ResolvedOrb {
        speed_after_contact_mps: 0.0,
        ..orb_spec()
    };
    assert_eq!(strikes(crate::rules::space::CONTACT_RANGE_M, held), 6, "held still");
}

/// JAHU CANTICLE: A KILL STRIPS EVERYONE INSIDE AFFINITY RANGE, and the
/// range is measured from the PLAYER.
///
/// *"Killing enemies reduces the Armor and Shields of other enemies within
/// Affinity Range by X%"*, and Affinity Range is *"a 50-meter radius"*
/// around the squad (wiki `Affinity`) — so the distance that matters is
/// yours to the body, not the corpse's.
///
/// THE RANGE ASSERTION IS THE ONE THAT MATTERS. A strip that ignored the
/// radius entirely would pass any test that only checked it fires, so the
/// second arm puts the crowd beyond it and asserts nothing moves.
#[test]
fn a_kill_strips_the_armour_of_everyone_inside_affinity_range() {
    let mut damage = DamageVector::default();
    damage.set(DamageType::Impact, 400.0);
    let armoured = |at: crate::rules::space::Vec2| {
        let mut params = FightParams::default().foe.clone();
        params.base_health = 400.0;
        params.base_armor = 600.0;
        params.base_shield = 0.0;
        params.level = 1;
        params.base_level = 1;
        params.mode = TargetMode::InstantRespawn;
        crate::formation::FoeSpec {
            id: "e2".into(),
            params,
            body_parts: BodyPart::humanoid(),
            at,
        }
    };
    let fought = |strip: Option<(f64, f64)>, at: crate::rules::space::Vec2| {
        let mut p = FightParams {
            damage,
            // AN EXPLOSION, so the shot reaches the crowd body at all. A
            // plain single-target pellet lands on the aimed one and nothing
            // else, and a strip nobody is standing in the blast of is a
            // strip nothing can measure.
            radial: Some(crate::build::loadout::ResolvedRadial {
                radius_m: 8.0,
                falloff_reduction: 0.0,
                ..radial_of(0.0, 0.0)
            }),
            strip_on_kill_in_range: strip,
            magazine_size: 1e9,
            infinite_reserve: true,
            fire_rate: 4.0,
            duration_seconds: 30.0,
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            others: vec![armoured(at)],
            ..no_status()
        };
        // THE AIMED BODY DIES AND COMES BACK, which is what produces the
        // kills; the CROWD body is what the strip is measured on.
        p.foe.mode = TargetMode::InstantRespawn;
        p.foe.base_health = 1.0;
        p.foe.base_shield = 0.0;
        p.foe.base_armor = 0.0;
        p.foe.level = 1;
        p.foe.base_level = 1;
        let s = monte_carlo(&p, 8, 3);
        s.mean_damage_by_body.0[1]
    };
    let near = crate::rules::space::Vec2::new(3.0, 0.0);
    let far = crate::rules::space::Vec2::new(400.0, 0.0);
    let bare = fought(None, near);
    let stripped = fought(Some((0.05, 50.0)), near);
    assert!(bare > 0.0, "the fixture reaches the crowd body: {bare}");
    assert!(
        stripped > bare * 1.05,
        "a stripped body takes more: {stripped} against {bare}"
    );
    // …AND NOTHING 400 m AWAY IS TOUCHED, which is what makes the radius a
    // radius rather than a decoration.
    assert!(
        (fought(Some((0.05, 50.0)), far) - fought(None, far)).abs() < 1e-9,
        "outside Affinity Range the card does nothing"
    );
}

/// A WEAPON WITH NO MAGAZINE NEVER RELOADS, so nothing keyed to a reload
/// ever fires.
///
/// A Tome has no clip — the module gives the Grimoire a magazine of ZERO —
/// and this entry writes 1 because the sim cannot fire a magazine of zero.
/// The loop therefore emptied that round and RELOADED for zero seconds
/// after every shot, and a reload that costs no time is still an EVENT:
/// every reload-triggered buff fired on every shot and stayed up for the
/// whole engagement. Measured on the real weapon, Pressurized Magazine took
/// it from 1,409 DPS to 2,699 — on a weapon that does not reload.
///
/// THE FIXTURE IS THE BUG. A one-round magazine, a zero-second reload and a
/// fire-rate buff on reload: the arm without the flag reloads
/// after every shot and rides the buff all engagement, and the arm with it
/// fires the same weapon with the buff never once triggered.
#[test]
fn a_weapon_with_no_magazine_never_reloads_so_no_reload_buff_fires() {
    let mut damage = DamageVector::default();
    damage.set(DamageType::Electricity, 10.0);
    let fired = |no_magazine: bool| {
        monte_carlo(
            &FightParams {
                damage,
                no_magazine,
                magazine_size: 1.0,
                reload_seconds: 0.0,
                infinite_reserve: true,
                fire_rate: 1.0,
                duration_seconds: 60.0,
                stacking_buffs: vec![crate::model::StackingBuff {
                    id: "on_reload_fire_rate",
                    trigger: crate::model::BuffTrigger::ReloadComplete,
                    grant: crate::model::BuffGrant::FireRate,
                    per_stack: 1.0,
                    max_stacks: 1,
                    duration: 60.0,
                    chance: 1.0,
                    decay: crate::model::BuffDecay::PerStackExpiry,
                    initial_stacks: 0,
                    stacks_per_trigger: 1,
                    per_shell: false,
                    cleared_by: crate::model::ClearedBy::Nothing,
                    precombat: false,
                }],
                crit_multiplier: 1.0,
                base_crit_chance: 0.0,
                arcane: ArcaneFx::none(),
                ..no_status()
            },
            4,
            3,
        )
        .mean_shots
    };
    let reloading = fired(false);
    let no_clip = fired(true);
    assert!(
        reloading > no_clip * 1.5,
        "a weapon that reloads rides the buff: {reloading} against {no_clip}"
    );
    assert!(
        (no_clip - 60.0).abs() < 1e-9,
        "and one with no magazine fires its bare rate, buff never triggered: {no_clip}"
    );
}

/// A ROUND LEAVES AFTER THE WIND-UP, and the interval is still the rate's.
///
/// Every gun in this roster fires at zero and the Grimoire's primary fires
/// at 0.1 s. It costs a sustained engagement NOTHING —
/// which is why it was nearly written off as latency, and why this asserts
/// the TIMES rather than the total: a mean cannot tell a stream that starts
/// at 0.1 from one that starts at 0, and the combat record's whole claim is
/// that its timestamps can be laid beside a recording.
///
/// Strike `k` lands at `windup + k / rate`, so the two arms are the same
/// numbers 0.1 s apart, and the LAST shot of the fight is the one that can
/// fall off the end.
#[test]
fn a_round_leaves_after_the_windup_and_the_interval_is_still_the_rates() {
    let mut damage = DamageVector::default();
    damage.set(DamageType::Electricity, 10.0);
    let fired = |windup_seconds: f64| {
        let p = FightParams {
            damage,
            windup_seconds,
            fire_rate: 2.0,
            magazine_size: 1e9,
            infinite_reserve: true,
            duration_seconds: 3.0,
            body_parts: vec![BodyPart {
                name: "body".into(),
                aim_weight: 1.0,
                multiplier: 1.0,
                is_head: false,
                is_weak_point: false,
                crit_bonus: false,
            }],
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            ..no_status()
        };
        let rec = record(&p, 7, 0.0, 10.0, 100, 0);
        rec.events()
            .iter()
            .filter_map(|e| match &e.kind {
                crate::record::Kind::Damage(_) => Some(e.t),
                _ => None,
            })
            .collect::<Vec<f64>>()
    };
    let instant = fired(0.0);
    let wound_up = fired(0.1);
    assert_eq!(instant, vec![0.0, 0.5, 1.0, 1.5, 2.0, 2.5], "a gun fires on the trigger");
    assert_eq!(
        wound_up,
        vec![0.1, 0.6, 1.1, 1.6, 2.1, 2.6],
        "…and this one 0.1 s later, at the same interval"
    );
}

#[test]
fn a_meter_opens_full_and_throws_one_orb_every_time_it_refills() {
    // THE CLOCK ALONE. `seconds_per_hit` is zeroed because this fixture's
    // gun deals nothing and would still be crediting hits — a metered form
    // fires its base attack, so a zero-damage pellet is a landing pellet
    // and the first version of this test was measuring two terms at once.
    let mut p = metered();
    p.meter = p.meter.map(|m| crate::build::loadout::ResolvedMeter { seconds_per_hit: 0.0, ..m });
    let s = monte_carlo(&p, 4, 3);
    // Four throws in 180 s: t=0 (it opens full), 45, 90, 135. The one at
    // 180 does not happen — the engagement ends as it fills.
    let strikes = s.mean_field_ticks / 6.0;
    assert!(
        (strikes - 4.0).abs() < 1e-9,
        "four throws of six strikes in 180 s: {} ({} strikes)",
        strikes,
        s.mean_field_ticks
    );
    // …AND THE SAME FORM WITHOUT A METER THROWS ON EVERY TRIGGER PULL,
    // which is the `transformed` mode. The two differ by 45x, which is
    // what the meter is worth getting wrong.
    // …AND THE SAME ORB WITHOUT A METER SHOWS THE REPLACE RULE INSTEAD.
    // Throwing every second, each one wipes the last after a single strike
    // — 180 throws worth 180 strikes, against 4 throws worth 24. Only one
    // orb exists at a time, and this is the arithmetic
    // of that: six strikes an orb when it is left alone, one when it is not.
    //
    // It is not a MODE — a Tome has two and neither of them is "hold the
    // alt fire" — but it is the cheapest way to see both rules at once.
    let ungated = monte_carlo(
        &FightParams {
            meter: None,
            magazine_size: 1e9,
            infinite_reserve: true,
            fire_rate: 1.0,
            ..p
        },
        4,
        3,
    );
    assert!(
        (ungated.mean_field_ticks - 180.0).abs() < 1e-9,
        "throwing every second, each orb gets ONE strike before the next replaces it: {}",
        ungated.mean_field_ticks
    );
}

/// A HIT TAKES A SECOND OFF IT — *"Hitting enemies with the primary fire
/// reduces recharge time by 1 second per hit"* — and the page settles what
/// counts without a field: *"Multishot will count as an additional hit"*,
/// so it is per landing PELLET.
///
/// The fixture fires 1/s and lands one pellet a shot, so the meter gains
/// two seconds a second and fills in 22.5 rather than 45: eight throws in
/// 180 s against four. Doubling the MULTISHOT doubles the hits and fills it
/// in 15, which is the half of the rule a per-shot reading would miss.
#[test]
fn a_landing_pellet_takes_a_second_off_the_meter() {
    let mut direct = DamageVector::default();
    direct.set(DamageType::Electricity, 10.0);
    let shooting = |multishot: f64| {
        monte_carlo(
            &FightParams {
                // A GUN THAT ALSO THROWS: the pellets are the base form's
                // and the orb rides the meter, which is the cycle's shape.
                damage: direct,
                multishot,
                base_multishot: multishot,
                fire_rate: 1.0,
                magazine_size: 1e9,
                infinite_reserve: true,
                ..metered()
            },
            4,
            3,
        )
        .mean_field_ticks
            / 6.0
    };
    let one = shooting(1.0);
    let two = shooting(2.0);
    assert!(
        (one - 8.0).abs() < 1e-9,
        "one pellet a second fills it in 22.5 s, so eight throws: {one}"
    );
    assert!(
        two > one + 1e-9,
        "and two pellets a second fill it faster: {two} against {one}"
    );
}

/// A KILL MAY LEAVE AMMO, and picking it up is worth ten seconds.
///
/// *"Picking up secondary or universal ammo reduces recharge time by 10
/// seconds"*, and this arena's rule is that a kill's drops arrive the
/// instant it dies — no vacuum radius, no walking back for them. Only the SECONDARY half of the roll counts, so the expected
/// credit is `0.45 x 0.5 x 10 = 2.25` seconds a kill.
///
/// THE TWO ARMS ARE THE SAME FIGHT with the pickup WORTH different amounts,
/// which is what isolates the term — and the loop it closes is the point:
/// ammo off a kill fills the meter, a full meter is another orb, and
/// another orb kills again. Measured at 24 kills against 37.6.
#[test]
fn a_kill_that_drops_secondary_ammo_fills_the_meter() {
    let worth = |seconds_per_ammo_pickup: f64| {
        let mut p = metered();
        p.meter = p.meter.map(|m| crate::build::loadout::ResolvedMeter {
            seconds_per_hit: 0.0,
            seconds_per_ammo_pickup,
            ..m
        });
        // A BODY THAT DIES AND COMES BACK, so the run produces the kills
        // whose drops are the thing under test. Its death ends the orb that
        // killed it — which costs both arms the same strikes and leaves the
        // difference to the ammo.
        p.foe.mode = TargetMode::InstantRespawn;
        p.foe.base_health = 1.0;
        p.foe.base_shield = 0.0;
        p.foe.base_armor = 0.0;
        p.foe.level = 1;
        p.foe.base_level = 1;
        // A WEAPON THAT KEEPS FIRING, so the orb walk is settled shot by
        // shot rather than in one block at the end. A kill ends the walk
        // for that call — the rule the cloud tick has always had —
        // so a fixture that pulls the trigger once would abandon every orb
        // after the first body dropped.
        p.magazine_size = 1e9;
        p.infinite_reserve = true;
        p.fire_rate = 1.0;
        let s = monte_carlo(&p, 24, 3);
        (s.mean_kills, s.mean_field_ticks)
    };
    let (kills, none) = worth(0.0);
    let (kills_with_ammo, ten) = worth(10.0);
    assert!(kills > 1.0, "the fixture has to kill something: {kills}");
    assert!(
        ten > none,
        "ammo off a kill fills the meter, so more orbs go out: {ten} against {none}"
    );
    assert!(
        kills_with_ammo > kills,
        "…and more orbs kill more, which is the loop closing: {kills_with_ammo} against {kills}"
    );
}

/// Overlapping fields STACK — ✅ measured (MEASUREMENTS M13). Both branches
/// stay pinned because the branch is weapon data, not a global rule: a
/// future weapon may well refresh instead. Three grenades one second apart:
/// stacking runs three concurrent streams, refresh keeps re-arming one.
#[test]
fn overlapping_fields_stack_or_refresh_per_the_weapon_data() {
    let mk = |stacking| FightParams {
        damage: DamageVector::default(),
        lingering: Some(cloud(stacking)),
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        // No arcane: the Default fixture's Enervate grants FLAT crit chance,
        // which correctly reaches the field's ticks and would blur a
        // tick-count assertion into a damage one.
        arcane: ArcaneFx::none(),
        fire_rate: 1.0,
        magazine_size: 3.0,
        reload_seconds: 999.0, // 3 shots at t=0,1,2 then dry
        infinite_reserve: false,
        reserve_ammo: 0.0,
        duration_seconds: 20.0,
        ..no_status()
    };
    let st = monte_carlo(&mk(crate::model::FieldStacking::Stack), 4, 3);
    assert!((st.mean_shots - 3.0).abs() < 1e-9, "shots {}", st.mean_shots);
    // Three independent 10-tick streams, all finishing before t=20.
    assert!(
        (st.mean_field_ticks - 30.0).abs() < 1e-9,
        "stacking ticks {}",
        st.mean_field_ticks
    );
    // A tenth of the damage was riding on the first-tick question alone.
    assert!(
        (st.mean_damage - 30.0 * 40.0).abs() < 1e-9,
        "dmg {}",
        st.mean_damage
    );
    let rf = monte_carlo(&mk(crate::model::FieldStacking::Refresh), 4, 3);
    // One field, re-armed at t=1 and t=2 — each re-arm ticks immediately, so
    // 3 shot-time ticks plus the surviving field's own 9 = 12.
    assert!(
        (rf.mean_field_ticks - 12.0).abs() < 1e-9,
        "refresh ticks {}",
        rf.mean_field_ticks
    );
}

#[test]
fn refreshing_a_cloud_replaces_only_its_owners_previous_cloud() {
    let params = FightParams {
        damage: DamageVector::default(),
        lingering: Some(cloud(crate::model::FieldStacking::Refresh)),
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        fire_rate: 1.0,
        magazine_size: 3.0,
        reload_seconds: 999.0,
        infinite_reserve: false,
        reserve_ammo: 0.0,
        duration_seconds: 20.0,
        ..no_status()
    };
    let solo = run_once(&params, &mut Rng::new(3));
    assert_eq!(solo.field_ticks, 12, "the owner's previous clouds must be replaced");
    let pair = run_once(&params.clone().and_also(params), &mut Rng::new(3));
    let by = pair.dealt.by_combatant().0;
    assert_eq!(by[0], solo.effective_damage(), "the second owner's refresh removed the first owner's cloud");
    assert_eq!(by[1], solo.effective_damage(), "the first owner's refresh removed the second owner's cloud");
    assert_eq!(pair.field_ticks, 24);
}

/// Renewed Horror — ✅ measured (MEASUREMENTS M13): reloading from EMPTY
/// makes the NEXT shot's pod live twice as long, "1 direct hit + 20 pod
/// ticks" against the normal 10. A one-round magazine makes every shot after
/// the first a post-reload shot, so the counts are exact: shot 1 leaves 10
/// ticks, shot 2 leaves 20.
#[test]
fn renewed_horror_doubles_only_the_post_reload_field() {
    let p = |boost: f64| FightParams {
        damage: DamageVector::default(),
        lingering: Some(cloud(crate::model::FieldStacking::Stack)),
        field_duration_on_empty_reload: boost,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        fire_rate: 1.0,
        magazine_size: 1.0,
        reload_seconds: 1.0,
        // EXACTLY two shots: one from the magazine, one from the single
        // reserve round, then dry. The 60 s window then lets every field
        // finish, so the tick counts are exact rather than truncated.
        infinite_reserve: false,
        reserve_ammo: 1.0,
        duration_seconds: 60.0,
        ..no_status()
    };
    let off = monte_carlo(&p(1.0), 4, 3);
    let on = monte_carlo(&p(2.0), 4, 3);
    let shots = off.mean_shots;
    assert!((shots - 2.0).abs() < 1e-9, "expected two shots, got {shots}");
    assert!(
        (on.mean_shots - shots).abs() < 1e-9,
        "the buff must not change the cadence"
    );
    // Every shot but the first follows an empty reload, so each of those
    // fields doubles: ticks go from 10n to 10 + 20(n-1).
    let n = shots;
    assert!(
        (off.mean_field_ticks - 10.0 * n).abs() < 1e-9,
        "baseline ticks {} for {n} shots",
        off.mean_field_ticks
    );
    assert!(
        (on.mean_field_ticks - (10.0 + 20.0 * (n - 1.0))).abs() < 1e-9,
        "boosted ticks {} for {n} shots",
        on.mean_field_ticks
    );
}

/// The field is a WEAPON damage instance, not a status DoT: it rolls its own
/// crit off its OWN base stats, and its ticks report in their own bucket
/// rather than the DoT one.
#[test]
fn field_ticks_roll_their_own_crit_and_report_as_field_damage() {
    let mk = |cc: f64| {
        let mut f = cloud(crate::model::FieldStacking::Stack);
        f.crit_chance = cc;
        if let Some(inputs) = f.critical_inputs.as_mut() { inputs.set_supplied_chance(f.crit_chance, f.base_crit_chance); }
        FightParams {
            damage: DamageVector::default(),
            lingering: Some(f),
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            magazine_size: 1.0,
            reload_seconds: 999.0,
            infinite_reserve: false,
            reserve_ammo: 0.0,
                ..no_status()
        }
    };
    let flat = monte_carlo(&mk(0.0), 4, 3);
    let crit = monte_carlo(&mk(1.0), 4, 3);
    // 100% crit at 2.0x doubles every tick.
    assert!(
        (crit.mean_damage - 2.0 * flat.mean_damage).abs() < 1e-6,
        "{} vs {}",
        crit.mean_damage,
        flat.mean_damage
    );
    // Counted as FIELD damage, never as a DoT tick.
    assert_eq!(flat.mean_dot_damage, 0.0, "a field tick is not a status DoT");
    assert_eq!(flat.median_run.dot_ticks, 0, "no bleed/DoT ticks at all");
    assert!((flat.source_damage.field - flat.mean_damage).abs() < 1e-9);
}

/// Status is per TICK — "Toxin clouds can proc Hunter Munitions on each tick
/// of damage" — so a 100%-status cloud procs once per tick, and those procs
/// then feed Condition Overload like any other.
#[test]
fn field_ticks_proc_status_once_each() {
    let mut f = cloud(crate::model::FieldStacking::Stack);
    f.status_chance = 1.0;
    let p = FightParams {
        damage: DamageVector::default(),
        lingering: Some(f),
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        magazine_size: 1.0,
        reload_seconds: 999.0,
        infinite_reserve: false,
        reserve_ammo: 0.0,
        ..no_status()
    };
    let s = monte_carlo(&p, 4, 3);
    assert!(
        (s.mean_procs - s.mean_field_ticks).abs() < 1e-9,
        "procs {} vs ticks {}",
        s.mean_procs,
        s.mean_field_ticks
    );
    assert!(s.mean_dot_damage > 0.0, "the cloud's Toxin procs must burn");
}

/// CONTINUOUS weapons MERGE their multishot instead of making several
/// instances. VERBATIM (wiki Multishot §Continuous Weapons): the combined
/// tick has "damage AND Status Chance equal to the SUM of the individual
/// beams, but the Critical Chance is still equal to that of a single beam."
///
/// Multishot is pinned at exactly 2.0 so the roll is deterministic, and
/// crit is off so the damage assertion is arithmetic.
#[test]
fn a_beam_merges_its_multishot_into_one_instance() {
    let mk = |continuous, multishot: f64| FightParams {
        damage: DamageVector::new().with(DamageType::Toxin, 50.0),
        continuous,
        fire_rate: 8.0,
        multishot,
        base_multishot: 1.0,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        duration_seconds: 4.0,
        arcane: ArcaneFx::none(),
        ..no_status()
    };
    // Doubling multishot doubles the damage either way…
    let b1 = monte_carlo(&mk(true, 1.0), 8, 5);
    let b2 = monte_carlo(&mk(true, 2.0), 8, 5);
    assert!(
        (b2.mean_damage - 2.0 * b1.mean_damage).abs() < 1e-6,
        "beam {} vs 2x{}",
        b2.mean_damage,
        b1.mean_damage
    );
    // …but a beam does it in ONE instance per tick, where a gun fires two.
    // That is the whole mechanic, and the reason crit stays a single roll.
    assert!(
        (b2.mean_pellets - b2.mean_shots).abs() < 1e-9,
        "a beam tick is one instance ({} pellets, {} ticks)",
        b2.mean_pellets,
        b2.mean_shots
    );
    let g2 = monte_carlo(&mk(false, 2.0), 8, 5);
    assert!(
        (g2.mean_pellets - 2.0 * g2.mean_shots).abs() < 1e-9,
        "a gun still fires two pellets ({} vs {})",
        g2.mean_pellets,
        g2.mean_shots
    );
    // And the beam pays the ramp a gun does not: the same two beams' worth
    // of damage arrives lower over a short burst.
    assert!(
        b2.mean_damage < g2.mean_damage,
        "the ramp must cost something: beam {} vs gun {}",
        b2.mean_damage,
        g2.mean_damage
    );
}

/// THE EXPONENT. A beam's DoT goes as multishot **squared**: the merge sums
/// BOTH halves, so a beam does not trade proc COUNT for proc SIZE.
///
/// | | procs per tick | payload each | DoT |
/// | --- | --- | --- | --- |
/// | gun | `M x SC` | 1x | `M` |
/// | beam, rolled status | `M x SC` | `Mx` | `M²` |
/// | beam, FORCED proc | 1 | `Mx` | `M` |
///
/// *"The total output of damaging status effects … is affected **twice** by
/// multishot on all continuous weapons"*, with the exception that proves
/// the mechanism — a forced proc is applied after the merge and so is
/// "equivalent to use on standard weapons".
///
/// Deterministic on purpose: status chance is 1.0 at M=1, so no ratio here
/// is a sample mean, and 3 ticks x M=3 is 9 Toxin stacks — one under the
/// cap that would flatten the effect being measured.
#[test]
fn a_beams_dot_scales_with_multishot_squared() {
    let mk = |continuous, multishot: f64, forced: bool| {
        let mut p = FightParams {
            damage: DamageVector::new().with(DamageType::Toxin, 50.0),
            continuous,
            // Isolate multishot-squared scaling from the 20% ramp floor's
            // dilution by the M58 +1 DoT accumulator.
            beam_ramp_floor: 1.0,
            multishot,
            base_multishot: 1.0,
            status_chance: if forced { 0.0 } else { 1.0 },
            base_status_chance: if forced { 0.0 } else { 1.0 },
            forced_procs: if forced { vec![DamageType::Toxin] } else { Vec::new() },
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            fire_rate: 1.0,
            duration_seconds: 2.5,
            ..FightParams::default()
        };
        // NOTHING MAY DIE and nothing may ramp: a target that dies truncates
        // the run, and the fixture's default arcane is Secondary Enervate,
        // whose crit ramps with the number of instances a shot makes — which
        // is the very thing multishot changes.
        p.foe.base_health = 1e12;
        p.foe.base_armor = 0.0;
        p.foe.base_shield = 0.0;
        p.foe.base_overguard = 0.0;
        p.arcane = ArcaneFx::none();
        // ONE body part at 1x. A proc's payload carries the procing hit's
        // part multiplier, so the fixture's 50/50 body/3x-head draw is
        // noise sitting on top of the very ratio being measured.
        p.body_parts = vec![BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            is_weak_point: false,
            crit_bonus: false,
        }];
        p
    };
    let run = |c, multishot, f| monte_carlo(&mk(c, multishot, f), 8, 11);
    let close = |a: f64, b: f64| (a - b).abs() < 0.02 * b;

    for m in [2.0_f64, 3.0] {
        let (one, many) = (run(true, 1.0, false), run(true, m, false));
        // Procs are NOT traded away: the summed status chance still lands M
        // of them a tick, exactly as a gun's M pellets would.
        let procs = many.mean_procs / one.mean_procs;
        assert!(close(procs, m), "beam procs x{procs} at M={m}");
        // …and each is built from the merged instance, so the total squares.
        let dot = many.mean_dot_damage / one.mean_dot_damage;
        assert!(close(dot, m * m), "beam dot x{dot} at M={m}, want x{}", m * m);

        // A GUN is linear in both halves — same proc count, payload untouched.
        let (g1, gm) = (run(false, 1.0, false), run(false, m, false));
        let gdot = gm.mean_dot_damage / g1.mean_dot_damage;
        assert!(close(gdot, m), "gun dot x{gdot} at M={m}");

        // FORCED procs: merged first, so ONE a tick whatever M is — and the
        // linear payload puts the beam back level with the gun.
        let (f1, fm) = (run(true, 1.0, true), run(true, m, true));
        let fprocs = fm.mean_procs / f1.mean_procs;
        assert!(close(fprocs, 1.0), "forced procs x{fprocs} at M={m} — merged, so one a tick");
        let fdot = fm.mean_dot_damage / f1.mean_dot_damage;
        assert!(close(fdot, m), "forced dot x{fdot} at M={m}, want x{m}");
    }
}

/// The damage RAMP: "Initial damage starts at a lower percentage, and ramps
/// up to 100% of its damage over 0.6 seconds of hitting a target … this
/// lower percentage is 20%." At 10 ticks/s that is the first tick at 20% and
/// full damage from the 7th on.
#[test]
fn a_beam_ramps_from_a_fifth_to_full_over_point_six_seconds() {
    let mut ramp = BeamRamp::default();
    let frame_seconds = 0.1; // 10 ticks/s
    let mults: Vec<f64> = (0..8).map(|i| ramp.tick(i as f64 * frame_seconds, frame_seconds, crate::model::BEAM_RAMP_FLOOR, None)).collect();
    // Native pre-advance: the first tick arrives after 1 / fire_rate (0.1 s -> 1/6 of 0.6 s).
    assert!((mults[0] - (0.2 + 0.8 / 6.0)).abs() < 1e-9, "first tick {}", mults[0]);
    assert!((mults[1] - (0.2 + 0.8 * 2.0 / 6.0)).abs() < 1e-9, "second {}", mults[1]);
    assert!((mults[5] - 1.0).abs() < 1e-9, "6th tick should be full: {}", mults[5]);
    assert!((mults[6] - 1.0).abs() < 1e-9);

    // Stopping decays it, then the reacquired tick advances by its own period.
    let mut r2 = BeamRamp::default();
    for i in 0..8 {
        r2.tick(i as f64 * frame_seconds, frame_seconds, crate::model::BEAM_RAMP_FLOOR, None);
    }
    let after = r2.tick(0.7 + 1.9, frame_seconds, crate::model::BEAM_RAMP_FLOOR, None);
    assert!((after - (0.2 + 0.8 * (0.5 + 1.0 / 6.0))).abs() < 1e-9, "after a gap {after}");
    let mut r3 = BeamRamp::default();
    for i in 0..8 {
        r3.tick(i as f64 * frame_seconds, frame_seconds, crate::model::BEAM_RAMP_FLOOR, None);
    }
    assert!((r3.tick(0.7 + 3.0, frame_seconds, crate::model::BEAM_RAMP_FLOOR, None) - (0.2 + 0.8 / 6.0)).abs() < 1e-9);
}

#[test]
fn finite_reserve_stops_the_gun() {
    // Reserve off: 12 in the mag + 12 in reserve = 24 shots, then dry.
    let p = FightParams {
        duration_seconds: 60.0,
        infinite_reserve: false,
        reserve_ammo: 12.0,
        ..no_status()
    };
    let s = monte_carlo(&p, 20, 4);
    assert!((s.mean_shots - 24.0).abs() < 1e-9, "shots {}", s.mean_shots);
}

/// M133: multi-tendril CentralBeam weapons (Phantasma, Phantasma Prime, Phage)
/// keep their central beam unscaled by multishot mods and live multishot buffs.
#[test]
fn m133_central_beam_is_excluded_from_multishot_scaling() {
    let pool = crate::data::mods::class_pool("shotgun");
    let find_mod = |id: &str| {
        pool.iter()
            .find(|m| m.id == id)
            .unwrap_or_else(|| panic!("missing mod {id}"))
    };
    let hells = find_mod("hells_chamber");
    let galv = find_mod("galvanized_hell");

    // Phantasma Prime: 5 outer beams + 1 fixed central beam.
    let pp = crate::model::WeaponBase::from_data("phantasma_prime", false, &[]);
    assert!(pp.has_central_beam);
    let stock = crate::build::loadout::resolve(&pp, &[], crate::model::StackPolicy::Emergent);
    assert!((stock.multishot - 6.0).abs() < 1e-9, "stock {}", stock.multishot);

    let with_hells = crate::build::loadout::resolve(&pp, &[hells], crate::model::StackPolicy::Emergent);
    // 1 + 5 * (1 + 1.2) = 12.0 (not 6 * 2.2 = 13.2).
    assert!((with_hells.multishot - 12.0).abs() < 1e-9, "hells {}", with_hells.multishot);

    let galv_max = crate::build::loadout::resolve(&pp, &[galv], crate::model::StackPolicy::AssumedMax);
    let want_galv = 1.0 + 5.0 * (1.0 + 1.1 + 4.0 * 0.29997);
    assert!((galv_max.multishot - want_galv).abs() < 1e-4, "galv max {} vs {want_galv}", galv_max.multishot);

    let galv_emergent = crate::build::loadout::resolve(&pp, &[galv], crate::model::StackPolicy::Emergent);
    let on_kill = galv_emergent.multishot_stack.expect("Galvanized Hell on-kill");
    assert!((on_kill.per_stack - 5.0 * 0.29997).abs() < 1e-4, "per_stack {}", on_kill.per_stack);

    // Phage: 6 outer beams + 1 fixed central beam -> 1 + 6 * 2.2 = 14.2.
    let phage = crate::model::WeaponBase::from_data("phage", false, &[]);
    assert!(phage.has_central_beam);
    let phage_hells = crate::build::loadout::resolve(&phage, &[hells], crate::model::StackPolicy::Emergent);
    assert!((phage_hells.multishot - 14.2).abs() < 1e-4, "phage hells {}", phage_hells.multishot);
}

/// M133: continuous beam ramp scales status DoT ModifiedBase on the first tick
/// (15% floor -> 13.775 Heat tick on Phantasma Prime + Incendiary Coat) and
/// resets per target generation even without an attack.beam splash block.
#[test]
fn m133_beam_ramp_scales_dot_seeds_and_resets_per_target() {
    let pool = crate::data::mods::class_pool("shotgun");
    let inc = pool
        .iter()
        .find(|m| m.id == "incendiary_coat")
        .expect("incendiary_coat");
    let pp = crate::model::WeaponBase::from_data("phantasma_prime", false, &[]);
    let panel = crate::build::loadout::resolve(&pp, &[inc], crate::model::StackPolicy::Emergent);
    let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(1.5), &ArcaneFx::none());
    p.base_crit_chance = 0.0;
    p.crit_multiplier = 1.0;
    // Force exactly one Heat stack on the first tick (beam status_chance scales
    // by multishot, so 1.0 would land six stacks at once).
    p.status_chance = 0.0;
    p.base_status_chance = 0.0;
    p.forced_procs = vec![DamageType::Heat];
    p.damage = DamageVector::new().with(DamageType::Heat, p.damage.total());
    p.foe.base_health = 1e12;
    p.foe.base_armor = 0.0;
    p.foe.base_shield = 0.0;
    p.foe.base_overguard = 0.0;
    p.body_parts = vec![BodyPart {
        name: "body".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: false,
        is_weak_point: false,
        crit_bonus: false,
    }];
    // Single first tick at Phantasma Prime's real 12/s fire rate and 0.02 client floor:
    // mult_1 = 0.02 + 0.98 * ((1/12) / 0.6) = 0.156111... -> 90 * mult_1 = 14.05 (M133: 14).
    p.fire_rate = 12.0;
    p.magazine_size = 0.5;
    p.ammo_cost = 0.5;
    p.reload_seconds = 5.0;
    p.duration_seconds = 1.5;
    let rec = record(&p, 1, 0.0, 2.0, 200, 0);
    let first_dot = rec
        .events()
        .iter()
        .find_map(|e| match &e.kind {
            crate::record::Kind::Damage(d)
                if d.origin == crate::record::Origin::Status && d.dtype == DamageType::Heat =>
            {
                Some(d.raw)
            }
            _ => None,
        })
        .expect("first-tick Heat DoT");
    let first_mult = 0.02 + 0.98 * ((1.0 / 12.0) / 0.6);
    let expected = (90.0 * first_mult + 1.0) * 0.5 * 1.9;
    assert!((first_dot - expected).abs() < 1e-4, "first-tick Heat DoT {first_dot} != {expected}");

    // Per-target ramp reset for continuous weapons without an attack.beam block:
    // kill the first target after full ramp (>0.6 s), then verify the respawn's
    // first direct hit drops back to the initial 12/s pre-advanced tick (~15.6%).
    let mut respawn = p.clone();
    respawn.fire_rate = 12.0;
    respawn.magazine_size = 11.0;
    respawn.reload_seconds = 0.5;
    respawn.duration_seconds = 1.2;
    respawn.forced_procs.clear();
    // Direct hit at full ramp is 90 * 1.9 = 171; ~8 ticks (~0.67 s) exceed 1000 HP.
    respawn.foe.base_health = 1000.0;
    respawn.foe.mode = TargetMode::InstantRespawn;
    let r = record(&respawn, 1, 0.0, 2.0, 400, 0);
    let mut first_hit = None;
    let mut saw_kill = false;
    let mut after_respawn_hit = None;
    for e in r.events() {
        if let crate::record::Kind::Damage(d) = &e.kind {
            if first_hit.is_none() && d.origin == crate::record::Origin::Own {
                first_hit = Some(d.raw);
            }
            if saw_kill && d.origin == crate::record::Origin::Own {
                after_respawn_hit = Some(d.raw);
                break;
            }
            if d.killed {
                saw_kill = true;
            }
        }
    }
    let initial = first_hit.expect("initial direct hit");
    let hit = after_respawn_hit.expect("direct hit on respawned target");
    let full_hit = respawn.damage.total() * respawn.multishot;
    assert!(
        (hit - initial).abs() < 1e-6 && hit < full_hit * 0.2,
        "respawned target hit {hit} did not match initial 15% ramp hit {initial} (full={full_hit})"
    );
}

/// Planar chaining beams track each target's ramp independently: killing and
/// respawning body 0 resets body 0 to 20% while leaving body 1 at full 100%.
#[test]
fn planar_beam_ramps_track_chain_targets_independently() {
    let mut p = FightParams {
        damage: DamageVector::new().with(DamageType::Heat, 100.0),
        continuous: true,
        beam_ramp_floor: 0.2,
        multishot: 1.0,
        base_multishot: 1.0,
        fire_rate: 10.0,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        status_chance: 0.0,
        base_status_chance: 0.0,
        duration_seconds: 1.2,
        target_at: crate::rules::space::Vec2::new(0.0, 2.0),
        beam: Some(crate::model::BeamGeometry {
            range_m: 20.0,
            damage_radius_m: 0.0,
            radius_takes_multishot: false,
            chain_hops: 1,
            chain_range_m: 10.0,
            chain_damage_per_hop: 0.5,
            chain_compounds: true,
            chain_fanout: false,
            chain_takes_multishot: false,
            chain_nodes_have_radius: false,
            beams_count: 1,
            beams_acquire_deg: 0.0,
            beams_range_m: 0.0,
            ramp: None,
        }),
        ..FightParams::default()
    };
    p.arcane = ArcaneFx::none();
    p.foe.base_health = 650.0;
    p.foe.base_armor = 0.0;
    p.foe.base_shield = 0.0;
    p.foe.base_overguard = 0.0;
    p.foe.mode = TargetMode::InstantRespawn;
    p.body_parts = vec![BodyPart {
        name: "body".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: false,
        is_weak_point: false,
        crit_bonus: false,
    }];
    let mut tank = p.foe.clone();
    tank.base_health = 1e9;
    p.others = vec![crate::formation::FoeSpec {
        id: "chain".into(),
        params: tank,
        body_parts: p.body_parts.clone(),
        at: crate::rules::space::Vec2::new(2.0, 2.0),
    }];

    let rec = record(&p, 1, 0.0, 2.0, 400, 0);
    let kill_t = rec
        .events()
        .iter()
        .find_map(|e| match &e.kind {
            crate::record::Kind::Damage(d) if e.subject == Some(0) && d.killed => Some(e.t),
            _ => None,
        })
        .expect("body 0 killed");
    let (respawn_t, b0) = rec
        .events()
        .iter()
        .find_map(|e| match &e.kind {
            crate::record::Kind::Damage(d) if e.subject == Some(0) && e.t > kill_t + 1e-9 => {
                Some((e.t, d.raw))
            }
            _ => None,
        })
        .expect("body 0 hit after respawn");
    let b1 = rec
        .events()
        .iter()
        .find_map(|e| match &e.kind {
            crate::record::Kind::Damage(d)
                if e.subject == Some(1) && (e.t - respawn_t).abs() < 1e-9 =>
            {
                Some(d.raw)
            }
            _ => None,
        })
        .expect("body 1 chain hit on same tick as body 0 respawn");
    let expected_b0 = (0.2 + 0.8 / 6.0) * 100.0;
    assert!(
        (b0 - expected_b0).abs() < 1e-4,
        "respawned body 0 should reset to first-tick ramp ({expected_b0}), got {b0}"
    );
    assert!((b1 - 50.0).abs() < 1.0, "body 1 should stay at full 100% ramp (50), got {b1}");
}

/// Continuous weapons merge multishot into `beam_merge`; planar punch-through
/// must never re-roll `round_multishot` on top of the merged tick when
/// `1 < multishot < 2` and a tick rolls `1`.
#[test]
fn continuous_punch_through_never_re_rolls_multishot() {
    let mut p = FightParams {
        damage: DamageVector::new().with(DamageType::Heat, 100.0),
        continuous: true,
        beam_ramp_floor: 1.0,
        multishot: 1.5,
        base_multishot: 1.0,
        punch_through_m: 2.0,
        fire_rate: 10.0,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        status_chance: 0.0,
        base_status_chance: 0.0,
        duration_seconds: 2.0,
        target_at: crate::rules::space::Vec2::new(0.0, 1.0),
        ..FightParams::default()
    };
    p.arcane = ArcaneFx::none();
    p.foe.base_health = 1e9;
    p.foe.base_armor = 0.0;
    p.foe.base_shield = 0.0;
    p.foe.base_overguard = 0.0;
    p.body_parts = vec![BodyPart {
        name: "body".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: false,
        is_weak_point: false,
        crit_bonus: false,
    }];
    p.others = vec![crate::formation::FoeSpec {
        id: "behind".into(),
        params: p.foe.clone(),
        body_parts: p.body_parts.clone(),
        at: crate::rules::space::Vec2::new(0.0, 2.0),
    }];
    let rec = record(&p, 42, 0.0, 3.0, 2000, 0);
    let mut sum = [0.0_f64; 2];
    for e in rec.events() {
        if let crate::record::Kind::Damage(d) = &e.kind {
            if let Some(sub) = e.subject {
                if (sub as usize) < 2 {
                    sum[sub as usize] += d.raw;
                }
            }
        }
    }
    assert!(sum[0] > 0.0, "aimed body took damage");
    assert!(
        (sum[1] - sum[0]).abs() < 1e-6,
        "punched body {} != aimed body {} (double-rolled multishot)",
        sum[1], sum[0]
    );
}

#[test]
fn basmu_beam_declares_its_chain_and_ramp() {
    let b = crate::model::WeaponBase::from_data("basmu_beam", false, &[]);
    let geom = b.beam.expect("basmu_beam chain geometry");
    assert_eq!(geom.chain_hops, 5);
    assert!((geom.chain_range_m - 7.0).abs() < 1e-9);
    assert!((geom.chain_damage_per_hop - 0.5).abs() < 1e-9);
    assert!(geom.chain_compounds);
    let ramp = geom.ramp.expect("basmu_beam ramp");
    assert!((ramp.rise_seconds - 0.5).abs() < 1e-9);
    assert!((b.beam_ramp_floor - 0.2).abs() < 1e-9);
}

#[test]
fn beam_radius_zeroes_puncture_depth_while_preserving_avatar_pierce() {
    use crate::build::loadout::resolve;
    use crate::model::StackPolicy;
    let rifles = crate::data::mods::class_pool("rifle");
    let pistols = crate::data::mods::class_pool("pistol");
    let shred = rifles.iter().find(|m| m.id == "primed_shred").expect("primed_shred");
    let seeker = pistols.iter().find(|m| m.id == "seeker").expect("seeker");

    for (id, mod_def) in [
        ("torid_incarnon", shred),
        ("glaxion_vandal", shred),
        ("gaze_primary", shred),
        ("ignis", shred),
        ("ignis_wraith", shred),
        ("catabolyst", seeker),
        ("coda_catabolyst", seeker),
        ("embolist", seeker),
    ] {
        let base = crate::model::WeaponBase::from_data(id, false, &[]);
        let bare = resolve(&base, &[], StackPolicy::Emergent);
        let modded = resolve(&base, &[mod_def], StackPolicy::Emergent);
        assert_eq!(bare.punch_through_m, 0.0, "{id} bare punch_through_m");
        assert_eq!(modded.punch_through_m, 0.0, "{id} modded punch_through_m");
    }

    for id in ["ignis", "ignis_wraith", "phantasma", "phantasma_prime", "cortege", "onos_incarnon", "athodai_alt", "athodai_prime_alt"] {
        let base = crate::model::WeaponBase::from_data(id, false, &[]);
        let bare = resolve(&base, &[], StackPolicy::Emergent);
        assert!(bare.infinite_body_punch_through, "{id} should pierce avatars freely");
    }

    for id in ["furis_incarnon", "atomos"] {
        let base = crate::model::WeaponBase::from_data(id, false, &[]);
        let bare = resolve(&base, &[], StackPolicy::Emergent);
        let modded = resolve(&base, &[seeker], StackPolicy::Emergent);
        assert!(!bare.infinite_body_punch_through, "{id} has no innate avatar pierce");
        assert_eq!(bare.punch_through_m, 0.0, "{id} bare punch_through_m");
        assert!(modded.punch_through_m > 2.0, "{id} accepts punch-through mods: {}", modded.punch_through_m);
    }
}

#[test]
fn ocucor_tendrils_and_boar_extra_beams_scale_with_multishot() {
    use crate::build::loadout::resolve;
    use crate::model::StackPolicy;
    let pistols = crate::data::mods::class_pool("pistol");
    let shotguns = crate::data::mods::class_pool("shotgun");

    // Ocucor tendril on body 1 scales ~2.2x with Barrel Diffusion (+120% MS).
    let ocucor = crate::model::WeaponBase::from_data("ocucor", false, &[]);
    let barrel = pistols.iter().find(|m| m.id == "barrel_diffusion").expect("barrel_diffusion");
    let run_ocucor = |mods: &[&crate::model::ModDef]| -> f64 {
        let panel = resolve(&ocucor, mods, StackPolicy::Emergent);
        let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(2.0), &ArcaneFx::none());
        p.foe = Foe::training_dummy();
        p.body_parts = mono_body(1.0);
        p.crit_multiplier = 1.0;
        p.base_crit_chance = 0.0;
        p.status_chance = 0.0;
        p.base_status_chance = 0.0;
        p.tendrils_initial = 1;
        p.tendrils_held = true;
        p.target_at = crate::rules::space::Vec2::new(0.0, 5.0);
        p.others = vec![crate::formation::FoeSpec {
            id: "side".into(),
            params: Foe::training_dummy(),
            body_parts: mono_body(1.0),
            at: crate::rules::space::Vec2::new(1.0, 5.0),
        }];
        let rec = record(&p, 7, 0.0, 3.0, 2000, 0);
        rec.events().iter().filter_map(|e| match (&e.kind, e.subject) {
            (crate::record::Kind::Damage(d), Some(1)) => Some(d.raw),
            _ => None,
        }).sum()
    };
    let bare_tendril = run_ocucor(&[]);
    let ms_tendril = run_ocucor(&[barrel]);
    assert!(bare_tendril > 0.0, "tendril dealt damage");
    let ratio = ms_tendril / bare_tendril;
    assert!((ratio - 2.2).abs() < 0.25, "tendril multishot ratio expected ~2.2, got {ratio} ({bare_tendril} -> {ms_tendril})");

    // Boar Prime Incarnon extra beams + chains on bodies 1..2 scale ~2.2x with Hell's Chamber (+120% MS).
    let boar = crate::model::WeaponBase::from_data("boar_prime_incarnon", false, &[]);
    let hells = shotguns.iter().find(|m| m.id == "hells_chamber").expect("hells_chamber");
    let run_boar = |mods: &[&crate::model::ModDef]| -> f64 {
        let panel = resolve(&boar, mods, StackPolicy::Emergent);
        let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(2.0), &ArcaneFx::none());
        p.foe = Foe::training_dummy();
        p.body_parts = mono_body(1.0);
        p.crit_multiplier = 1.0;
        p.base_crit_chance = 0.0;
        p.status_chance = 0.0;
        p.base_status_chance = 0.0;
        p.target_at = crate::rules::space::Vec2::new(0.0, 10.0);
        p.others = vec![
            crate::formation::FoeSpec {
                id: "left".into(),
                params: Foe::training_dummy(),
                body_parts: mono_body(1.0),
                at: crate::rules::space::Vec2::new(-0.8, 10.0),
            },
            crate::formation::FoeSpec {
                id: "right".into(),
                params: Foe::training_dummy(),
                body_parts: mono_body(1.0),
                at: crate::rules::space::Vec2::new(0.8, 10.0),
            },
        ];
        let rec = record(&p, 7, 0.0, 3.0, 2000, 0);
        rec.events().iter().filter_map(|e| match (&e.kind, e.subject) {
            (crate::record::Kind::Damage(d), Some(sub)) if sub >= 1 => Some(d.raw),
            _ => None,
        }).sum()
    };
    let bare_boar = run_boar(&[]);
    let ms_boar = run_boar(&[hells]);
    assert!(bare_boar > 0.0, "extra beams dealt damage");
    let boar_ratio = ms_boar / bare_boar;
    assert!((boar_ratio - 2.2).abs() < 0.25, "boar extra-beam multishot ratio expected ~2.2, got {boar_ratio} ({bare_boar} -> {ms_boar})");
}

#[test]
fn athodai_alt_fires_as_continuous_beam_with_merged_multishot_and_ramp() {
    use crate::build::loadout::resolve;
    use crate::model::StackPolicy;
    for (id, expected_range, expected_width) in [("athodai_alt", 12.0, 1.6), ("athodai_prime_alt", 16.0, 4.0)] {
        let base = crate::model::WeaponBase::from_data(id, false, &[]);
        let panel = resolve(&base, &[], StackPolicy::Emergent);
        assert!(base.continuous, "{id} continuous");
        assert!((base.beam_ramp_floor - 0.2).abs() < 1e-9, "{id} floor");
        assert!(panel.infinite_body_punch_through, "{id} avatar pierce");
        assert!((panel.punch_through_m - 2.0).abs() < 1e-9, "{id} terrain PT");
        assert!((panel.projectile_width_m - expected_width).abs() < 1e-9, "{id} sweep width");
        assert!((panel.range_m - expected_range).abs() < 1e-9, "{id} range");
        let p = FightParams::from_panel(&panel, &crate::arena::Arena::training(2.0), &ArcaneFx::none());
        assert!(p.continuous, "{id} continuous");
        let ramp = p.beam_ramp.expect("athodai beam_ramp");
        assert!((ramp.rise_seconds - 0.6).abs() < 1e-9);
        assert!(ramp.per_target);
    }
}

#[test]
fn athodai_alt_dumps_modded_or_partial_clip_with_per_tick_ammo_and_ramp() {
    use crate::build::loadout::resolve;
    use crate::model::StackPolicy;
    let base = crate::model::WeaponBase::from_data("athodai_alt", false, &[]);
    let panel = resolve(&base, &[], StackPolicy::Emergent);
    let arena = crate::arena::Arena::training(2.0);
    let mut p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
    p.duration_seconds = 1.95;
    p.infinite_reserve = false;
    p.reserve_ammo = 0.0;
    let full = monte_carlo(&p, 1, 1);
    assert_eq!(full.mean_shots, 24.0, "full 24-round clip drains 1/tick at 12/s");

    let mut partial = p.clone();
    partial.magazine_size = 5.0;
    let short = monte_carlo(&partial, 1, 1);
    assert_eq!(short.mean_shots, 5.0, "partial 5-round clip caps at 5 ticks");

    let mut boosted = p;
    boosted.magazine_size = 36.0;
    boosted.duration_seconds = 2.95;
    let extended = monte_carlo(&boosted, 1, 1);
    assert_eq!(extended.mean_shots, 36.0, "modded >24 clip fires the entire 36-round clip");
}

#[test]
fn onos_incarnon_ramps_over_eight_ticks_fires_power_shot_and_clamps_under_fifty_charges() {
    use crate::build::loadout::resolve;
    use crate::model::StackPolicy;
    let base = crate::model::WeaponBase::from_data("onos_incarnon", true, &["onos_evo1_incarnon_form"]);
    let panel = resolve(&base, &[], StackPolicy::Emergent);
    let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(2.0), &ArcaneFx::none());
    p.body_parts = mono_body(1.0);
    p.base_crit_chance = 0.0;
    p.critical_inputs = None;
    p.status_chance = 0.0;
    let ps = p.melee_profiles.get_mut("power_shot").unwrap();
    ps.body_parts = mono_body(1.0);
    ps.base_crit_chance = 0.0;
    ps.critical_inputs = None;
    ps.radial.as_mut().unwrap().crit_chance = 0.0;
    ps.radial.as_mut().unwrap().critical_inputs = None;
    p.duration_seconds = 4.6;

    let mut rec = crate::record::Record::window(0.0, 10.0, 1000, 0);
    let res = run_once_traced(&p, &mut Rng::new(7), None, &mut rec);
    assert_eq!(res.shots, 9, "8 beam ticks + 1 auto-switched power_shot");
    let direct_hits: Vec<f64> = rec.events().iter().filter_map(|e| match &e.kind {
        crate::record::Kind::Damage(d)
            if d.origin == crate::record::Origin::Own && d.dtype == DamageType::Radiation => Some(d.raw),
        _ => None,
    }).collect();
    assert_eq!(direct_hits.len(), 8);
    assert!((direct_hits[0] - 30.0 * 1.1875).abs() < 1e-6);
    assert!((direct_hits[7] - 30.0 * 2.5).abs() < 1e-6);
    let blast_hits: Vec<f64> = rec.events().iter().filter_map(|e| match &e.kind {
        crate::record::Kind::Damage(d)
            if matches!(d.origin, crate::record::Origin::Own | crate::record::Origin::Splash)
                && d.dtype == DamageType::Heat => Some(d.raw),
        _ => None,
    }).collect();
    assert_eq!(blast_hits, vec![2200.0, 1100.0]);

    // With < 50 charges left after the 8th tick, focus stays clamped at 2.5x without switching.
    let mut low = p.clone();
    low.magazine_size = 20.0;
    low.infinite_reserve = false;
    low.reserve_ammo = 0.0;
    low.duration_seconds = 6.0;
    let mut low_rec = crate::record::Record::window(0.0, 10.0, 1000, 0);
    let _ = run_once_traced(&low, &mut Rng::new(7), None, &mut low_rec);
    let low_rad: Vec<f64> = low_rec.events().iter().filter_map(|e| match &e.kind {
        crate::record::Kind::Damage(d) if d.dtype == DamageType::Radiation => Some(d.raw),
        _ => None,
    }).collect();
    assert_eq!(low_rad.len(), 12);
    assert!((low_rad[7] - 75.0).abs() < 1e-6);
    assert!((low_rad[11] - 75.0).abs() < 1e-6, "clamped at 2.5x while <50 charges remain");
}

#[test]
fn onos_devastation_cascade_stacks_crit_on_power_shot_and_resets_after_blast() {
    use crate::build::loadout::resolve;
    use crate::model::StackPolicy;
    let plain_evos: &[&str] = &["onos_evo1_incarnon_form"];
    let cascade_evos: &[&str] = &["onos_evo1_incarnon_form", "onos_devastation_cascade"];
    let run_for = |evos: &[&str]| {
        let base = crate::model::WeaponBase::from_data("onos_incarnon", true, evos);
        let panel = resolve(&base, &[], StackPolicy::Emergent);
        let mut p = FightParams::from_panel(&panel, &crate::arena::Arena::training(2.0), &ArcaneFx::none());
        p.duration_seconds = 9.6;
        monte_carlo(&p, 64, 11).mean_damage
    };
    let without = run_for(plain_evos);
    let with = run_for(cascade_evos);
    assert!(with > without * 1.35, "Devastation Cascade boosts PowerShot CC/CD: {without} -> {with}");
}

#[test]
fn inactive_behavior_freezes_beam_ramp_across_mode_switch_while_reload_decays() {
    let spec = Some(crate::model::BeamRampSpec {
        rise_seconds: 0.6,
        ceiling: 1.0,
        rise_ticks: 0,
        cooldown_delay_seconds: 0.8,
        cooldown_divisor: 2.0,
        per_target: true,
        reset_on_release: false,
    });
    let mut beam = BeamRamp::default();
    let mut targets = vec![TargetRamp::default()];
    for step in 0..6 {
        let now = f64::from(step) * 0.1;
        let _ = beam.tick(now, 0.1, 0.2, spec);
        let _ = targets[0].hit(1, now, 0.1, 0.2, spec);
    }
    assert!((targets[0].hit(1, 0.5, 0.1, 0.2, spec) - 1.0).abs() < 1e-12);

    // Suspend at t = 0.6 into another mode for 10 seconds (well past 0.8 + 1.2 s decay).
    let mut inactive = FrozenBeam::default();
    inactive.swap(&mut beam, &mut targets, 0.6, 0.6);
    assert!(targets.is_empty(), "other mode starts with its own empty target list");
    inactive.swap(&mut beam, &mut targets, 10.6, 10.6);
    assert!((targets[0].hit(1, 10.6, 0.1, 0.2, spec) - 1.0).abs() < 1e-12, "frozen while inactive");

    // Staying on the active behavior through a 2.5 s reload decays back to cold.
    let after_reload = targets[0].hit(1, 13.2, 0.1, 0.2, spec);
    assert!((after_reload - (0.2 + 0.8 / 6.0)).abs() < 1e-12, "reload decays active ramp: {after_reload}");
}
