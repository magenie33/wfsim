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
            target_at: crate::space::Vec2::new(gap_m, 0.0),
            ..orb_thrower()
        };
        (monte_carlo(&p, 4, 3).mean_damage / 280.0).round() as u32
    };
    // AT CONTACT the target is inside the reach from the muzzle, so the
    // first strike lands at the throw and all six do.
    assert_eq!(strikes(crate::space::CONTACT_RANGE_M), 6, "at contact");
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
                orb: Some(crate::loadout::ResolvedOrb {
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
                        crit_bonus: false,
                    },
                    BodyPart {
                        name: "body".into(),
                        aim_weight: 1.0,
                        multiplier: 1.0,
                        is_head: false,
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
        part.base_crit_chance = 1.0;
        part.crit_damage = 2.0;
        monte_carlo(
            &FightParams {
                orb: Some(crate::loadout::ResolvedOrb {
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
            crate::damage::ForcedProcs::from_types([DamageType::Electricity]);
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
    let strikes = |gap_m: f64, orb: crate::loadout::ResolvedOrb| {
        let p = FightParams {
            target_at: crate::space::Vec2::new(gap_m, 0.0),
            orb: Some(orb),
            ..orb_thrower()
        };
        (monte_carlo(&p, 4, 3).mean_damage / 280.0).round() as u32
    };
    // AT CONTACT the departure window alone decides it: 6.25 / 2 = 3.13 s,
    // which holds four strike instants.
    assert_eq!(strikes(crate::space::CONTACT_RANGE_M, orb_spec()), 4, "at contact");
    // …AND NO THROW DISTANCE BUYS SIX. The approach adds at most another
    // 1.04 s, so five is the ceiling — asserted across the whole range
    // rather than at a distance somebody picked, because "six somewhere"
    // is exactly the claim being tested.
    let ladder: Vec<u32> = (0..=30)
        .map(|g| strikes(f64::from(g).max(crate::space::CONTACT_RANGE_M), orb_spec()))
        .collect();
    assert!(
        ladder.iter().all(|&n| n <= 5),
        "the in-reach window cannot hold six: {ladder:?}"
    );
    assert!(ladder.contains(&5), "and it does reach five: {ladder:?}");
    // WHAT WOULD BUY SIX, which is how the wall was found: neither of
    // these is the real answer, and the second one is what a room does to
    // an orb that bounces. A slower drift does…
    let slower = crate::loadout::ResolvedOrb {
        speed_after_contact_mps: 1.2,
        ..orb_spec()
    };
    assert_eq!(strikes(crate::space::CONTACT_RANGE_M, slower), 6, "at 1.2 m/s");
    // …and so does an orb that stops where it touches.
    let held = crate::loadout::ResolvedOrb {
        speed_after_contact_mps: 0.0,
        ..orb_spec()
    };
    assert_eq!(strikes(crate::space::CONTACT_RANGE_M, held), 6, "held still");
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
    let armoured = |at: crate::space::Vec2| {
        let mut params = FightParams::default().target.clone();
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
    let fought = |strip: Option<(f64, f64)>, at: crate::space::Vec2| {
        let mut p = FightParams {
            damage,
            // AN EXPLOSION, so the shot reaches the crowd body at all. A
            // plain single-target pellet lands on the aimed one and nothing
            // else, and a strip nobody is standing in the blast of is a
            // strip nothing can measure.
            radial: Some(crate::loadout::ResolvedRadial {
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
        p.target.mode = TargetMode::InstantRespawn;
        p.target.base_health = 1.0;
        p.target.base_shield = 0.0;
        p.target.base_armor = 0.0;
        p.target.level = 1;
        p.target.base_level = 1;
        let s = monte_carlo(&p, 8, 3);
        s.mean_damage_by_body.0[1]
    };
    let near = crate::space::Vec2::new(3.0, 0.0);
    let far = crate::space::Vec2::new(400.0, 0.0);
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
                    card_opens_full: false,
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
/// Shot `k` lands at `windup + k / rate`, so the two arms are the same
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
    p.meter = p.meter.map(|m| crate::loadout::ResolvedMeter { seconds_per_hit: 0.0, ..m });
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
        p.meter = p.meter.map(|m| crate::loadout::ResolvedMeter {
            seconds_per_hit: 0.0,
            seconds_per_ammo_pickup,
            ..m
        });
        // A BODY THAT DIES AND COMES BACK, so the run produces the kills
        // whose drops are the thing under test. Its death ends the orb that
        // killed it — which costs both arms the same strikes and leaves the
        // difference to the ammo.
        p.target.mode = TargetMode::InstantRespawn;
        p.target.base_health = 1.0;
        p.target.base_shield = 0.0;
        p.target.base_armor = 0.0;
        p.target.level = 1;
        p.target.base_level = 1;
        // A WEAPON THAT KEEPS FIRING, so the orb walk is settled shot by
        // shot rather than in one block at the end. A kill ends the walk
        // for that call — the rule `process_field_ticks` has always had —
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
        multishot,
        base_multishot: 1.0,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        duration_seconds: 4.0,
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
        p.target.base_health = 1e12;
        p.target.base_armor = 0.0;
        p.target.base_shield = 0.0;
        p.target.base_overguard = 0.0;
        p.arcane = ArcaneFx::none();
        // ONE body part at 1x. A proc's payload carries the procing hit's
        // part multiplier, so the fixture's 50/50 body/3x-head draw is
        // noise sitting on top of the very ratio being measured.
        p.body_parts = vec![BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
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
    let mults: Vec<f64> = (0..8).map(|i| ramp.tick(i as f64 * frame_seconds, frame_seconds, crate::model::BEAM_RAMP_FLOOR)).collect();
    assert!((mults[0] - 0.20).abs() < 1e-9, "first tick {}", mults[0]);
    // Each held tick adds 0.1/0.6 of the way from 20% to 100%.
    assert!((mults[1] - (0.2 + 0.8 / 6.0)).abs() < 1e-9, "second {}", mults[1]);
    assert!((mults[6] - 1.0).abs() < 1e-9, "7th tick should be full: {}", mults[6]);
    assert!((mults[7] - 1.0).abs() < 1e-9);

    // Stopping decays it: "0.8 seconds after the weapon stops hitting a
    // target, the damage decays back to its initial point over 2 seconds."
    // Idle time is the gap MINUS the tick that would have been due, so a
    // 1.9 s gap is 1.8 s idle, 1.0 s of it past the delay = half the ramp.
    let mut r2 = BeamRamp::default();
    for i in 0..8 {
        r2.tick(i as f64 * frame_seconds, frame_seconds, crate::model::BEAM_RAMP_FLOOR);
    }
    let after = r2.tick(0.7 + 1.9, frame_seconds, crate::model::BEAM_RAMP_FLOOR);
    assert!((after - (0.2 + 0.8 * 0.5)).abs() < 1e-9, "after a gap {after}");
    // And a long enough gap returns it all the way to the floor.
    let mut r3 = BeamRamp::default();
    for i in 0..8 {
        r3.tick(i as f64 * frame_seconds, frame_seconds, crate::model::BEAM_RAMP_FLOOR);
    }
    assert!((r3.tick(0.7 + 3.0, frame_seconds, crate::model::BEAM_RAMP_FLOOR) - 0.20).abs() < 1e-9);
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
