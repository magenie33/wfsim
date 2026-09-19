use super::*;

/// A FORMATION TAKES MORE THAN A LONE TARGET, and the whole difference is
/// the chain — the end-to-end assertion that the layer, the mechanic and
/// the run loop are joined up. The Torid Incarnon is the roster's only
/// chaining beam, so it is the weapon this is asked of, over eight more
/// bodies at 3 m with the front row's middle one aimed at (MECHANICS §12).
/// THE DAMAGE METER ACCOUNTS FOR EVERY BODY.
///
/// `effective_damage`, the score and the DPS count the whole formation, so
/// a METER written on the aimed body's path alone reports 5304 of damage by
/// source against 15980 actually dealt — and a reader comparing the
/// headline with the breakdown correctly concludes one of them is invented.
///
/// Asserted as a SHARE rather than a number: the roll call records per-body
/// damage only once there is more than one body, so the two totals are
/// comparable only in a crowd.
#[test]
fn the_damage_meter_accounts_for_the_whole_formation() {
    let base = crate::model::WeaponBase::from_data("akarius", false, &[]);
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let panel = crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let mut arena = crate::arena::Arena::training(3.0);
    arena.target_at = crate::space::Vec2::new(0.0, 5.4);
    let at = |x: f64, y: f64| crate::formation::FoeSpec {
        id: String::new(),
        params: TargetParams::training_dummy(),
        body_parts: BodyPart::humanoid(),
        at: crate::space::Vec2::new(x, y),
    };
    arena.others = vec![at(2.0, 5.2), at(-2.0, 5.2), at(0.0, 7.2)];
    let p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
    let r = run_once(&p, &mut Rng::new(0x5EED));

    let bodies: f64 = r.spread.by_body().0.iter().sum();
    let meter = r.sources.direct + r.sources.radial + r.sources.field
        + r.sources.arcane_on_status + r.sources.extra_hit + r.sources.syndicate
        + r.sources.status.iter().sum::<f64>();
    assert!(bodies > 0.0, "the crowd took nothing at all");
    assert!(
        meter >= bodies * 0.95,
        "the meter must account for what the bodies took: {meter} against {bodies}"
    );
    // …and the EXPLOSION is where the crowd's damage is filed, not some
    // new row: one blast reaching four bodies is the radial part four
    // times over.
    assert!(
        r.sources.radial > r.sources.direct * 3.0,
        "radial {} against direct {}", r.sources.radial, r.sources.direct
    );
}

/// A RANGE IS A WALL, NOT A RAMP.
///
/// The Phantasma's page states the two facts side by side — *"Limited range
/// of 20 meters"* as a disadvantage and *"No Damage Falloff"* — and only
/// the second was modelled, so the beam reached whatever it was aimed at
/// however far away it stood. The number had been sitting in the weapon's
/// own comment since the entry was written, unread, and wrong: BOTH
/// Phantasma files said 25 m, which is the Prime's.
///
/// FULL DAMAGE TO THE END AND ZERO PAST IT, with no taper in between —
/// asserted on both sides of the wall and one metre either way, because a
/// ramp would pass a test that only looked at 5 m and 50 m.
#[test]
fn a_weapon_deals_nothing_past_its_range() {
    let at = |weapon: &str, gap: f64| {
        let base = crate::model::WeaponBase::from_data(weapon, false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel = crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(10.0);
        // `gap` is surface to surface — what the shot flies and what the
        // arena shows, so "20 m" means the number the reader is looking at.
        arena.target_at = crate::space::Vec2::new(
            0.0,
            gap + crate::space::CONTACT_RANGE_M,
        );
        let p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        run_once(&p, &mut Rng::new(0x5EED)).effective_damage()
    };

    // THE BASE IS 20 m AND THE PRIME IS 25 — the wiki's own numbers, and
    // the one edge the Prime has over it.
    assert!(at("phantasma", 19.0) > 0.0, "19 m is inside 20");
    assert!(at("phantasma", 20.0) > 0.0, "the wall is inclusive");
    assert_eq!(at("phantasma", 21.0), 0.0, "21 m is past 20 and must be nothing");
    assert!(at("phantasma_prime", 21.0) > 0.0, "the Prime reaches 25");
    assert_eq!(at("phantasma_prime", 26.0), 0.0, "26 m is past 25");
    // NO TAPER: the beams deal full damage right up to the wall, so the
    // number at 1 m and at 19 m is the same one.
    assert!(
        (at("phantasma", 1.0) - at("phantasma", 19.0)).abs() < 1e-9,
        "no damage falloff means no damage falloff: {} at 1 m against {} at 19 m",
        at("phantasma", 1.0), at("phantasma", 19.0),
    );
    // THE NEGATIVE CONTROL. A weapon that declares no range is unlimited,
    // which is what every weapon in this engine was until today — so this
    // must not have quietly become a cap on the whole roster.
    assert!(at("braton", 60.0) > 0.0, "a weapon with no declared range still reaches");
}

/// A SHOT THAT WENT WIDE DOES NOT BLAST A BYSTANDER.
///
/// Reported by a player, found on the wire: the explosion had TWO
/// epicentres. The aimed body read its falloff from how far the pellet
/// passed it — correctly — while every other body read its distance from
/// the AIMED BODY'S SURFACE, so a shot nine metres wide dropped the aimed
/// body's damage 61% and left the body two metres behind it taking a direct
/// hit's blast (7120 against 7115 on target).
///
/// It is asserted as a MONOTONICITY rather than as a number: the further
/// the weapon points from the crowd, the less the crowd takes. Under the
/// old code the bystander's damage was flat in this parameter and then fell
/// off a cliff to zero, which is what a single number could easily have
/// been picked to miss.
#[test]
fn a_shot_that_went_wide_does_not_blast_the_bystanders() {
    let target_at = crate::space::Vec2::new(0.0, 10.0);
    let bystander = crate::space::Vec2::new(0.0, 12.0);
    // The Akarius is a rocket pistol: a real radial, 7.2 m, with spread.
    let bystander_damage = |aim_x: f64| {
        let base = crate::model::WeaponBase::from_data("akarius", false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel = crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(10.0);
        arena.target_at = target_at;
        arena.others = vec![crate::formation::FoeSpec {
            id: "e2".into(),
            params: TargetParams::training_dummy(),
            body_parts: BodyPart::humanoid(),
            at: bystander,
        }];
        arena.aim_at = Some(crate::space::Vec2::new(aim_x, 10.0));
        let p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        let r = run_once(&p, &mut Rng::new(0x5EED));
        r.spread.by_body().0.get(1).copied().unwrap_or(0.0)
    };

    let on_target = bystander_damage(0.0);
    let a_little = bystander_damage(4.0);
    let wide = bystander_damage(9.0);
    assert!(on_target > 0.0, "the bystander must take the blast at all: {on_target}");
    assert!(
        a_little < on_target && wide < a_little,
        "pointing further from the crowd must cost the crowd damage:              on target {on_target}, 4 m off {a_little}, 9 m off {wide}"
    );
}

#[test]
fn a_formation_takes_the_damage_a_chain_spreads_into_it() {
    let build = |others: Vec<crate::formation::FoeSpec>| {
        let base = crate::model::WeaponBase::from_data(
            "torid_incarnon",
            false,
            &["torid_evo1_incarnon_form"],
        );
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel = crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(10.0);
        arena.others = others;
        FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none())
    };

    let alone = build(Vec::new());
    assert!(alone.beam.is_some(), "the Incarnon form is a beam and must say so");
    let lone = run_once(&alone, &mut Rng::new(0x5EED)).effective_damage();

    // …AND THE SAME SHOT INTO A FORMATION. The grid is built around where
    // the aimed body already stands, so the fight it opens with is the one
    // every golden value uses and only the neighbours are new.
    let grid = crate::formation::Formation::grid(
        TargetParams::training_dummy(),
        BodyPart::humanoid(),
        3,
        3,
        3.0,
        alone.target_at,
    );
    let others: Vec<crate::formation::FoeSpec> = grid
        .foes
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != grid.aimed)
        .map(|(_, f)| f.clone())
        .collect();
    assert_eq!(others.len(), 8);
    let crowd = run_once(&build(others.clone()), &mut Rng::new(0x5EED)).effective_damage();

    assert!(
        crowd > lone * 1.5,
        "a formation must take substantially more than one body: {crowd} against {lone}"
    );

    // …AND THE RADIUS IS WHAT DECIDES HOW MUCH. Widen it by hand the way
    // Primed Firestorm does and the seeds go from one to four, which is the
    // finding `formation_value` prints and the reason the mod is worth a
    // slot at all.
    let mut primed = build(others);
    if let Some(b) = primed.beam.as_mut() {
        b.damage_radius_m *= 1.44;
    }
    let wide = run_once(&primed, &mut Rng::new(0x5EED)).effective_damage();
    assert!(
        wide > crowd * 1.5,
        "a wider radius seeds more chains: {wide} against {crowd}"
    );

    // …AND A LONE TARGET NOTICES NEITHER. The mod that quadruples the shot
    // in a crowd is worth exactly nothing against one body, which is the
    // asymmetry this whole layer exists to show.
    let mut primed_alone = build(Vec::new());
    if let Some(b) = primed_alone.beam.as_mut() {
        b.damage_radius_m *= 1.44;
    }
    assert_eq!(
        run_once(&primed_alone, &mut Rng::new(0x5EED)).effective_damage(),
        lone,
        "a damage radius has nothing to act on against one target"
    );
}

/// THE BASE FORM REACHES A FORMATION TOO, and by a different mechanism —
/// its AoE is a lingering CLOUD, not a chain. Which is
/// what makes a full Incarnon CYCLE simulable against a crowd: the grenade
/// half spreads through an area and the beam half through a chain, and both
/// halves now have somewhere to go.
#[test]
fn a_lingering_cloud_burns_everyone_standing_in_it() {
    let build = |others: Vec<crate::formation::FoeSpec>, radius_mult: f64| {
        let base = crate::model::WeaponBase::from_data("torid", false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let mut panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        // FIRESTORM, applied by hand so the test states the mechanic rather
        // than depending on a mod id: the cloud's radius is what the mod
        // scales, and `resolve` already multiplies it by `1 + br`.
        if let Some(f) = panel.lingering.as_mut() {
            f.radius_m *= radius_mult;
        }
        let mut arena = crate::arena::Arena::training(10.0);
        arena.others = others;
        FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none())
    };
    let alone = build(Vec::new(), 1.0);
    assert!(alone.lingering.is_some(), "the base form leaves a cloud");
    let lone = run_once(&alone, &mut Rng::new(0x5EED)).sources.field;
    assert!(lone > 0.0, "the cloud must burn the body it is stuck to");

    // Four bodies in a line at 1.5 m — the near two inside a 3 m cloud,
    // the third only once a radius mod widens it. Their falloff differs, so
    // this also asserts that each takes its OWN number rather than a share.
    let others: Vec<crate::formation::FoeSpec> = (1..=4)
        .map(|i| crate::formation::FoeSpec {
            id: String::new(),
            params: TargetParams::training_dummy(),
            body_parts: BodyPart::humanoid(),
            at: crate::space::Vec2::new(i as f64 * 1.5, alone.target_at.y),
        })
        .collect();
    let crowd = run_once(&build(others.clone(), 1.0), &mut Rng::new(0x5EED)).sources.field;
    assert!(crowd > lone * 1.5, "a cloud burns everyone in it: {crowd} against {lone}");

    // …AND A WIDER CLOUD CATCHES MORE OF THEM, which is the base form's
    // half of what a radius mod buys. At 3 m the body at 4.5 m is outside;
    // at 3 m x 1.44 it is not.
    let wide = run_once(&build(others, 1.44), &mut Rng::new(0x5EED)).sources.field;
    assert!(wide > crowd, "a wider cloud reaches further: {wide} against {crowd}");

    // …AND A LONE BODY NOTICES NEITHER. The radius is worth exactly nothing
    // against one target, which is the asymmetry the whole layer exists to
    // show — the same assertion the beam half makes.
    assert_eq!(run_once(&build(Vec::new(), 1.44), &mut Rng::new(0x5EED)).sources.field, lone);
}

/// THE OCUCOR'S TENDRILS ARE WORTH NOTHING AGAINST ONE BODY AND FOUR MORE
/// BEAMS AGAINST A CROWD — which is what its own weapon file has said since
/// the day it landed, with nowhere to put it.
///
/// *"Tendrils homing in on the main beam's target are only COSMETIC, and
/// don't deal any additional damage or status effects"* — so with one
/// target every tendril is worth zero, and modelling them would have
/// invented up to five beams the wiki says do not exist. A second body is
/// what makes them real, at *"base damage equal to the primary beam's"*.
#[test]
fn ocucor_tendrils_pay_only_once_there_is_a_second_body() {
    let build = |others: Vec<crate::formation::FoeSpec>, tendrils: u32| {
        let base = crate::model::WeaponBase::from_data("ocucor", false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(10.0);
        arena.others = others;
        let mut p =
            FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        // The tendrils a run OPENS with — a tendril costs a kill, and this
        // fixture's dummy does not die, so seeding is the only way to have
        // any (the same knob Sentient Surge's card sets).
        p.tendrils_initial = tendrils;
        p.tendrils_held = true;
        p
    };
    assert!(build(Vec::new(), 0).tendril_max > 0, "the Ocucor has tendrils");
    assert!(build(Vec::new(), 0).tendril_range_m > 0.0, "…and they have a reach");

    // ONE BODY: four tendrils are worth exactly nothing, asserted as
    // equality, because they all home on the body the beam is already on.
    let lone0 = run_once(&build(Vec::new(), 0), &mut Rng::new(0x5EED)).effective_damage();
    let lone4 = run_once(&build(Vec::new(), 4), &mut Rng::new(0x5EED)).effective_damage();
    assert_eq!(lone4, lone0, "a tendril on the main beam's own target is cosmetic");

    // FOUR MORE BODIES, inside the reticle cone and the reach.
    let others: Vec<crate::formation::FoeSpec> = (1..=4)
        .map(|i| crate::formation::FoeSpec {
            id: String::new(),
            params: TargetParams::training_dummy(),
            body_parts: BodyPart::humanoid(),
            at: crate::space::Vec2::new(i as f64 * 0.6, 4.0),
        })
        .collect();
    let crowd0 = run_once(&build(others.clone(), 0), &mut Rng::new(0x5EED)).effective_damage();
    let crowd4 = run_once(&build(others.clone(), 4), &mut Rng::new(0x5EED)).effective_damage();
    assert!(
        crowd4 > crowd0 * 1.5,
        "four tendrils are four more beams: {crowd4} against {crowd0}"
    );

    // …AND THEY REACH ONLY WHAT IS IN FRONT OF YOU. The same four bodies
    // walked out past the tendrils' 20 m and they are worth nothing again.
    let far: Vec<crate::formation::FoeSpec> = others
        .iter()
        .map(|f| crate::formation::FoeSpec {
            at: crate::space::Vec2::new(f.at.x, 400.0),
            ..f.clone()
        })
        .collect();
    assert_eq!(
        run_once(&build(far.clone(), 4), &mut Rng::new(0x5EED)).effective_damage(),
        run_once(&build(far, 0), &mut Rng::new(0x5EED)).effective_damage(),
        "past their reach a tendril takes nobody"
    );
}

/// SECONDARY IRRADIATE ECHOES INTO A FORMATION, and only off a target
/// that is actually IRRADIATED.
///
/// Two claims, and each one was a bug on its own day.
///
/// It needs somebody to echo to — *"deal X% of the hit damage to enemies
/// within Xm"* — which is why `arcanes_data` filed it with the TEAM buffs,
/// correctly, until there was a formation (group report via the owner,
/// 2026-08-17).
///
/// AND IT NEEDS THE TARGET AT 10 STACKS OF RADIATION, which is the other
/// half of its own card and was not read at all: the loader took
/// `trigger`/`grants` and the per-rank values and never `condition:`, so
/// the echo fired off a target with no Radiation on it (player report via
/// the owner). This test measured the bug for as long as it
/// existed, because a bare Dual Toxocyst makes no Radiation and the echo
/// landed anyway.
#[test]
fn secondary_irradiate_echoes_only_off_an_irradiated_target() {
    // RADIATION IS HEAT + ELECTRICITY, and reaching 10 stacks needs the
    // status chance to get there inside the engagement — so the build that
    // is supposed to trigger it is a real one rather than a flag.
    let build = |others: Vec<crate::formation::FoeSpec>, with: bool, rad: bool| {
        let base = crate::model::WeaponBase::from_data("dual_toxocyst", false, &[]);
        // TEN STACKS IS THE CAP, so the build has to actually get there and
        // stay there: the two elements that make Radiation, the multishot
        // and fire rate that pay for the ROLLS, and the status chance that
        // wins them. Reaching it is the test.
        let want = [
            "primed_heated_charge", "primed_convulsion", "lethal_torrent",
            "barrel_diffusion", "gunslinger", "stunning_speed", "sure_shot",
        ];
        let pool = crate::mods_data::pistol_pool();
        let mods: Vec<crate::model::ModDef> = if rad {
            want
                .iter()
                .filter_map(|m| pool.iter().find(|d| d.id == *m).cloned())
                .collect()
        } else {
            Vec::new()
        };
        if rad {
            assert_eq!(mods.len(), want.len(), "the Radiation build is complete");
        }
        let refs: Vec<&crate::model::ModDef> = mods.iter().collect();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let fx = if with {
            crate::arcanes_data::secondary("secondary_irradiate")
                .expect("the arcane is in the roster")
                .fx(5, crate::model::StackPolicy::AssumedMax, &[], crate::tenno_data::default_tenno())
        } else {
            crate::arcanes_data::ArcaneFx::none()
        };
        // A LONGER ENGAGEMENT, because stacks have to ACCUMULATE: a proc
        // lasts a few seconds and ten of them have to be alive at once.
        let mut arena = crate::arena::Arena::training(60.0);
        arena.others = others;
        FightParams::from_panel(&panel, &arena, &fx)
    };
    let p = build(Vec::new(), true, false);
    assert!(p.arcane.echo_share > 0.0, "the echo has a payload");
    assert_eq!(
        p.arcane.echo_needs_radiation_stacks, 10,
        "…and the card's own price, read from `condition:` rather than dropped"
    );

    // FOUR MORE BODIES inside its radius, which is 4.5 m to 7 m.
    let others: Vec<crate::formation::FoeSpec> = (1..=4)
        .map(|i| crate::formation::FoeSpec {
            id: String::new(),
            params: TargetParams::training_dummy(),
            body_parts: BodyPart::humanoid(),
            at: crate::space::Vec2::new(i as f64 * 0.9, 0.4),
        })
        .collect();

    // ONE BODY: worth exactly nothing, as an equality — there is nobody to
    // echo TO however irradiated the target is.
    let a = run_once(&build(Vec::new(), false, true), &mut Rng::new(0x5EED)).effective_damage();
    let b = run_once(&build(Vec::new(), true, true), &mut Rng::new(0x5EED)).effective_damage();
    assert_eq!(b, a, "an echo with nobody to echo to is worth nothing");

    // A CROWD AND NO RADIATION: also exactly nothing, and this is the one
    // that was wrong. An equality rather than a bound, because a gate that
    // is read either holds or does not.
    let c0 = run_once(&build(others.clone(), false, false), &mut Rng::new(0x5EED)).effective_damage();
    let c1 = run_once(&build(others.clone(), true, false), &mut Rng::new(0x5EED)).effective_damage();
    assert_eq!(
        c1, c0,
        "no Radiation on the target, no echo — the arcane's own condition"
    );

    // A CROWD AND A RADIATION BUILD: it lands.
    let d0 = run_once(&build(others.clone(), false, true), &mut Rng::new(0x5EED)).effective_damage();
    let d1 = run_once(&build(others, true, true), &mut Rng::new(0x5EED)).effective_damage();
    assert!(d1 > d0, "irradiated and in a crowd, the echo lands: {d1} against {d0}");
}

/// A TENDRIL'S OWN KILL SPAWNS NOTHING, which is the one kind of kill in
/// this engine worth less than another.
///
/// Verbatim from the rule list the Ocucor's file transcribes: *"a kill by
/// the primary beam, or by a status effect from any source (including one a
/// tendril applied), spawns a tendril; a DIRECT kill by a tendril does NOT
/// spawn another."* Until a tendril could kill anyone the distinction had
/// nothing to bite on.
#[test]
fn a_tendrils_own_kill_spawns_no_tendril() {
    let base = crate::model::WeaponBase::from_data("ocucor", false, &[]);
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let panel = crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let mut arena = crate::arena::Arena::training(20.0);
    // FRAIL bodies inside the tendrils' reach, so the tendrils really do
    // the killing rather than the beam. A REAL unit at level 1, because
    // the training dummy has infinite health and cannot die at all — which
    // is the right fixture for measuring a weapon and the wrong one for
    // measuring a kill.
    let specs = crate::enemy_data::all();
    let unit = specs
        .iter()
        .find(|e| e.id == "corrupted_heavy_gunner")
        .expect("the roster has one");
    let frail = unit
        .target_params(1, false, false, TargetMode::InstantRespawn)
        .expect("a level 1 ordinary unit is legal");
    arena.target = frail.clone();
    arena.others = (1..=4)
        .map(|i| crate::formation::FoeSpec {
            id: String::new(),
            params: frail.clone(),
            body_parts: BodyPart::humanoid(),
            at: crate::space::Vec2::new(i as f64 * 0.6, 4.0),
        })
        .collect();
    let mut p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
    p.tendrils_initial = 4;
    p.tendrils_held = true;
    let r = run_once(&p, &mut Rng::new(0x5EED));
    assert!(r.kills > 0, "the frail bodies must actually die");
    assert!(
        r.kills_by_tendril > 0,
        "and the tendrils must be the ones taking some of them: {r:?}",
    );
    assert!(r.kills_by_tendril <= r.kills);
}

/// THE BENCHMARK FIGHT IS THE MIDDLE RUN BY THE FIGHT'S OWN METRIC: the
/// exact middle of an odd count, the upper of the two middles of an even one.
#[test]
fn the_benchmark_fight_is_the_upper_middle_run_by_the_metric() {
    const SEED: u64 = 0x5EED;
    let mut p = FightParams::default();
    for runs in [7u32, 8] {
        for stat in [crate::metrics::RunStat::KillProgress, crate::metrics::RunStat::EffectiveDamage] {
            p.sample_by = stat;
            let mut ranked: Vec<(f64, u64)> = (0..runs)
                .map(|i| {
                    let state = SEED ^ u64::from(i).wrapping_mul(GOLDEN_GAP);
                    (stat.of(&run_once(&p, &mut Rng::new(state))), state)
                })
                .collect();
            ranked.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let s = monte_carlo(&p, runs, SEED);
            assert_eq!(s.median_run.rng_state, ranked[runs as usize / 2].1, "{runs} runs by {stat:?}");
        }
    }
}

/// EIGHT SHARDS ARE ONE RUN.
///
/// The assertion the whole fleet rests on: sharding is worth nothing if
/// the answer depends on how many workers were free, and a board row is a
/// public claim anyone can reproduce. It works because every run's dice are
/// a pure function of `(seed, index)` and every field of `Shard` merges by
/// addition, a min, a max or a concatenation — none of which depends on the
/// ORDER the shards arrive in.
///
/// COMPARED ON THE WHOLE SUMMARY, field by field: a merge that lost the
/// standard deviation or the ttk percentile would pass any assertion about
/// the mean. TO A PART IN 10^12 rather than bit for bit, because
/// floating-point addition is not associative — anything the merge actually
/// lost would be off by far more. The two exact things, which run is the
/// benchmark fight and the integer counts, use `assert_eq!`.
#[test]
fn eight_shards_are_one_run() {
    let base = crate::model::WeaponBase::from_data("torid", false, &[]);
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let panel = crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let specs = crate::enemy_data::all();
    let unit = specs.iter().find(|e| e.id == "corrupted_heavy_gunner").unwrap();
    let mut arena = crate::arena::Arena::training(20.0);
    arena.target = unit
        .target_params(30, false, false, TargetMode::InstantRespawn)
        .expect("a level 30 unit is legal");
    // A CROWD, because the per-body means are part of what has to survive
    // a merge and a single target cannot test them.
    arena.others = (1..=4)
        .map(|i| crate::formation::FoeSpec {
            id: format!("e{}", i + 1),
            params: arena.target.clone(),
            body_parts: BodyPart::humanoid(),
            at: crate::space::Vec2::new(f64::from(i) * 0.7, 1.2),
        })
        .collect();
    let p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());

    // 42, not 40: it does NOT divide by eight, so the last shards are
    // short — which is the fleet that actually runs.
    const RUNS: u32 = 42;
    const SEED: u64 = 0xBEEF_CAFE;
    let whole = monte_carlo(&p, RUNS, SEED);

    // EIGHT SLICES, and a ragged one on purpose: 40 does not divide into 8
    // evenly once a shard is allowed to be short, and a fleet whose last
    // worker gets the remainder is the fleet that actually runs.
    let mut merged = Shard::default();
    let mut at = 0u32;
    for k in 0..8u32 {
        let count = RUNS / 8 + u32::from(k < RUNS % 8);
        merged.merge(&shard(&p, at, count, SEED, false, &mut |_| {}));
        at += count;
    }
    assert_eq!(at, RUNS, "the slices must cover the whole range exactly");
    let (part, _) = merged.finish(&p, RUNS);

    // FIELD BY FIELD.
    assert_eq!(part.runs, whole.runs);
    for (name, a, b) in [
        ("mean_damage", part.mean_damage, whole.mean_damage),
        ("std_damage", part.std_damage, whole.std_damage),
        ("min_damage", part.min_damage, whole.min_damage),
        ("max_damage", part.max_damage, whole.max_damage),
        ("mean_effective", part.mean_effective_damage, whole.mean_effective_damage),
        ("std_effective", part.std_effective_damage, whole.std_effective_damage),
        ("mean_kill_progress", part.mean_kill_progress, whole.mean_kill_progress),
        ("std_kill_progress", part.std_kill_progress, whole.std_kill_progress),
        ("mean_kills", part.mean_kills, whole.mean_kills),
        ("std_kills", part.std_kills, whole.std_kills),
        ("mean_procs", part.mean_procs, whole.mean_procs),
        // A RATIO AND ITS DENOMINATOR, the same pairing and for the same
        // reason as the two lines below: the fleet merges shards, and a
        // ratio recomputed from a lost denominator agrees with nothing.
        ("mean_virus_stacks", part.mean_virus_stacks, whole.mean_virus_stacks),
        ("mean_armor_left", part.mean_armor_left, whole.mean_armor_left),
        ("mean_health_damage", part.mean_health_damage, whole.mean_health_damage),
        // …AND WHAT IT IS DIVIDED BY. `procs`/`pellets` reach a
        // caller as a RATE (`check_custom_enemies` asks whether a damage x0
        // column moves the proc draw), and the page runs every simulation on
        // a worker fleet — so a denominator lost in the merge would move
        // that rate with the numerator intact and nothing else disagreeing.
        ("mean_pellets", part.mean_pellets, whole.mean_pellets),
        // COVERAGE TRAVELS TOO. `one_fight` fails when the suite burns
        // nothing, and a fleet that lost this field would report zero
        // burns for a fight full of them — the guard turning on a working
        // engine, which is worse than no guard.
        ("mean_dot_ticks", part.mean_dot_ticks, whole.mean_dot_ticks),
        ("mean_crit_rate", part.mean_crit_rate, whole.mean_crit_rate),
        ("mean_headshot_rate", part.mean_headshot_rate, whole.mean_headshot_rate),
        ("burst_dps", part.burst_dps, whole.burst_dps),
        ("ttk_mean", part.ttk_mean, whole.ttk_mean),
        ("ttk_median", part.ttk_median, whole.ttk_median),
        ("ttk_p90", part.ttk_p90, whole.ttk_p90),
        ("max_hit", part.max_hit, whole.max_hit),
        ("mean_max_hit", part.mean_max_hit, whole.mean_max_hit),
        ("damage_per_pellet", part.damage_per_pellet, whole.damage_per_pellet),
        ("mean_downtime_seconds", part.mean_downtime_seconds, whole.mean_downtime_seconds),
        ("source.direct", part.source_damage.direct, whole.source_damage.direct),
        ("source.field", part.source_damage.field, whole.source_damage.field),
    ] {
        assert!((a - b).abs() <= b.abs() * 1e-12 + 1e-12, "{name}: {a} vs {b}");
    }
    assert_eq!(part.min_kills, whole.min_kills);
    assert_eq!(part.max_kills, whole.max_kills);
    assert_eq!(part.ttk_runs, whole.ttk_runs);
    // …AND THE MEDIAN ENGAGEMENT, which the shard finds by ranking one
    // number per run and REPLAYING the winner.
    assert_eq!(
        part.median_run.rng_state, whole.median_run.rng_state,
        "the same run must be the median either way"
    );
    assert!((part.median_run.effective_damage() - whole.median_run.effective_damage()).abs() < 1e-9);
    // …AND WHO TOOK WHAT, which only a crowd can test.
    assert!(part.mean_damage_by_body.0[1] > 0.0, "the fixture must reach a neighbour");
    for i in 0..6 {
        let (a, b) = (part.mean_damage_by_body.0[i], whole.mean_damage_by_body.0[i]);
        assert!((a - b).abs() <= b.abs() * 1e-12 + 1e-12, "body {i}: {a} vs {b}");
    }
}

/// AN EXPLOSION'S OWN DAMAGE FALLOFF IS READ, from its EPICENTRE.
///
/// Nineteen entries carry a `falloff_reduction` on their radial and their
/// `unmodeled:` lines said it was not modelled — "this arena has no
/// distance, so every shot lands at point blank", written before the arena
/// had one. It does, and this is the proof: a body further
/// from the blast takes strictly less than one standing on it.
///
/// THE AIMED BODY STILL TAKES THE FULL SHARE, which is not the exception it
/// looks like — the bomb detonates ON it, so its epicentre distance is
/// zero. That is why the gap was invisible with one target.
#[test]
fn an_explosions_falloff_is_read_from_its_epicentre() {
    let base = crate::model::WeaponBase::from_data("phantasma_prime_charged", false, &[]);
    let refs: Vec<&crate::model::ModDef> = Vec::new();
    let panel = crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    assert!(
        panel.radial.as_ref().is_some_and(|r| r.falloff_reduction > 0.0),
        "the fixture must be a weapon whose explosion falls off"
    );
    // TWO BODIES BEHIND THE TARGET, one close to the blast and one at its
    // rim — both inside the 4.8 m radius, so the only thing between them is
    // the falloff.
    let mut arena = crate::arena::Arena::training(10.0);
    arena.others = [1.0_f64, 4.5]
        .into_iter()
        .map(|d| crate::formation::FoeSpec {
            id: String::new(),
            params: TargetParams::training_dummy(),
            body_parts: BodyPart::humanoid(),
            at: crate::space::Vec2::new(0.0, arena.target_at.y + d),
        })
        .collect();
    let p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
    let r = run_once(&p, &mut Rng::new(0x5EED));
    let d = &r.spread.by_body().0;
    assert!(d[1] > 0.0 && d[2] > 0.0, "both are inside the radius: {:?}", &d[..3]);
    assert!(
        d[2] < d[1] * 0.9,
        "the body at the rim must take clearly less: {:?}",
        &d[..3]
    );
}

/// A SIMULTANEOUS BLAST DETONATION REACHES 5 m, AND NOT THE HOST.
///
/// The third element whose proc is an area, and the one whose two halves
/// differ most: "all stacks are dealt simultaneously as enemies within 5
/// meters are dealt 300% of base damage per proc" against the 30% the host
/// takes, and "the initial target of the blast procs is not dealt this AoE
/// damage, only the single target damage".
#[test]
fn a_simultaneous_blast_detonation_reaches_five_metres() {
    let build = |forced: Vec<DamageType>| {
        let base = crate::model::WeaponBase::from_data("braton_prime", false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(12.0);
        // ONE INSIDE the 5 m and one OUTSIDE, on the same line, so the
        // radius is what is being tested and not the geometry.
        arena.others = [3.0_f64, 20.0]
            .into_iter()
            .map(|x| crate::formation::FoeSpec {
                id: String::new(),
                params: TargetParams::training_dummy(),
                body_parts: BodyPart::humanoid(),
                at: crate::space::Vec2::new(x, arena.target_at.y),
            })
            .collect();
        let mut p =
            FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        p.status_chance = 1.0;
        p.forced_procs = forced;
        p
    };
    let r = run_once(&build(vec![DamageType::Blast]), &mut Rng::new(0x5EED));
    let d = &r.spread.by_body().0;
    assert!(d[0] > 0.0, "the host takes the single-target half");
    assert!(d[1] > 0.0, "the body 3 m away takes the detonation: {:?}", &d[..3]);
    assert_eq!(d[2], 0.0, "20 m is outside the 5 m radius: {:?}", &d[..3]);

    // THE CONTROL: Impact stacks the same way and reaches nobody, so this
    // is Blast's own mechanic rather than any full stack bar detonating.
    let imp = run_once(&build(vec![DamageType::Impact]), &mut Rng::new(0x5EED));
    assert_eq!(imp.spread.touched(), 1, "{:?}", &imp.spread.by_body().0[..3]);
}

/// A FULL PILE PAYS THE HOST AS TEN NUMBERS, NOT ONE — MEASUREMENTS M91.
///
/// The total is the same either way, so this asserts the COUNT: an instance
/// is the unit attenuation clamps, a shield gate multiplies and overkill is
/// measured against, and ten small ones are a different fight from one
/// large one on any target that has those.
///
/// THE ARCANE STILL SEES ONE, which is the trap this pairs with. What an
/// arcane counting hits reads is `blast_pops`, fed per MOMENT (M76) — so
/// splitting the damage must not split the count, and the assertion below
/// is what says it did not.
#[test]
fn a_full_blast_pile_pays_the_host_as_ten_numbers_and_the_arcane_as_one() {
    let mut p = FightParams {
        forced_procs: vec![DamageType::Blast],
        body_parts: mono_body(1.0),
        crit_multiplier: 1.0,
        // A PILE FILLS AND NOBODY DIES: ten shots inside the fuse, and a
        // target the pile cannot finish, so the loop pays every stack.
        fire_rate: 20.0,
        duration_seconds: 2.0,
        magazine_size: 1e9,
        ..no_status()
    };
    p.target.base_health = 1e15;
    let rec = record(&p, 0, 0.0, f64::INFINITY, 10_000, 0);
    // THE DETONATION IS THE INSTANT THAT HOLDS MORE THAN ONE BLAST NUMBER.
    let mut by_t: std::collections::BTreeMap<u64, Vec<f64>> = Default::default();
    for e in rec.events() {
        if let crate::record::Kind::Damage(d) = &e.kind {
            if d.dtype == DamageType::Blast && d.origin == crate::record::Origin::Status {
                by_t.entry((e.t * 1e6) as u64).or_default().push(d.base);
            }
        }
    }
    let pile = by_t.values().find(|v| v.len() > 1).expect("a pile detonates");
    assert_eq!(pile.len(), TEN_STACK_CAP, "ten stacks are ten numbers: {pile:?}");
    // …AND THEY ARE THE STACKS, not a tenth of a sum: every one is equal,
    // because this fixture applies the same stack ten times.
    let first = pile[0];
    assert!(
        pile.iter().all(|v| (v - first).abs() < 1e-9),
        "each number is one stack's own: {pile:?}"
    );

    let run = run_once(&p, &mut Rng::new(0));
    assert_eq!(
        run.blast_pops, 0,
        "a pile that reached the cap is not a fuse paying out, so it feeds no ramp"
    );
}

/// A BLAST GOING OFF IS ONE HIT PER MOMENT, NOT PER STACK.
///
/// What an arcane counting hits sees is the damage NUMBER at a moment. Two
/// stacks applied by the same shot carry the same fuse and go off together,
/// so they are one — the same rule that makes a shotgun's pellets one hit.
/// Measured in game: nine stacks left to expire on their own build nine,
/// two applied at once build one, and reaching ten builds one.
///
/// AND THE TENTH IS NOT A FUSE AT ALL. A full pile detonates where the
/// stack lands rather than on its clock, so it is not one of these — which
/// is what the wiki says of it, and why the fast case below counts fewer.
#[test]
fn a_blast_is_one_hit_per_moment_however_many_stacks_share_it() {
    let build = |forced: Vec<DamageType>, rate: f64| {
        let base = crate::model::WeaponBase::from_data("braton_prime", false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel = crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let arena = crate::arena::Arena::training(10.0);
        let mut p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        p.status_chance = 1.0;
        p.forced_procs = forced;
        p.fire_rate = rate;
        p
    };
    let pops = |forced: Vec<DamageType>, rate: f64| {
        run_once(&build(forced, rate), &mut Rng::new(0x5EED)).blast_pops
    };
    // SLOW ENOUGH THAT NOTHING REACHES TEN, so every detonation is a fuse.
    let one = pops(vec![DamageType::Blast], 0.2);
    assert!(one > 0, "the fixture has to detonate something");
    for n in 2..=3 {
        let many = pops(vec![DamageType::Blast; n], 0.2);
        assert_eq!(
            many, one,
            "{n} stacks a shot are {n} times the stacks and the SAME moments"
        );
    }
    // …AND A PILE THAT REACHES TEN COUNTS FEWER, because the detonation it
    // fires is not a fuse. Same weapon, same procs, only the rate moved.
    assert!(
        pops(vec![DamageType::Blast, DamageType::Blast], 10.0) < one,
        "a max-stack detonation is not a moment a hit is counted in"
    );
}

/// A GAS CLOUD OUTLIVES ITS HOST; EVERY OTHER DoT DIES WITH IT.
///
/// Verbatim: "If the host target dies, Gas will continue to tick damage on
/// all enemies caught in the host's radius for its remaining duration" —
/// and the respawned individual stands where the dead one did.
///
/// A UNIT TEST OF THE RULE, and asserted in BOTH directions: a version that
/// kept everything would pass an assertion about gas alone.
#[test]
fn a_gas_cloud_survives_the_death_and_nothing_else_does() {
    let dot = |dtype| Dot {
        cause: u32::MAX,
        next_tick: 5.0, ticks_left: 4, frozen: 100.0, landing: 1.0, bracket: 1.0, depth: 0,
        source_scaled: false, unit: 0.0, dtype, ignores_armor: false,
    };
    let mut d = DebuffState::default();
    d.dots.push(dot(DamageType::Gas));
    d.dots.push(dot(DamageType::Toxin));
    d.dots.push(dot(DamageType::Electricity));
    d.blast.push(BlastStack { fuse: 9.0, value: 30.0, xh_bracket: 1.0 });
    d.microwave = true;

    d.on_death(None, &frail_target(TargetMode::InstantRespawn, 0.0, 0.0));

    assert_eq!(d.dots.len(), 1, "only the cloud stays: {:?}", d.dots);
    assert_eq!(d.dots[0].dtype, DamageType::Gas);
    assert_eq!(d.dots[0].ticks_left, 4, "…with its own remaining duration");
    assert!(d.blast.is_empty(), "the stacks detonated on the way out");
    assert!(!d.microwave, "everything else was the dead individual's");
    // …AND THE DETONATION IS OWED TO THE NEIGHBOURS: 300% a stack against
    // the 30% the host carried, so ten times what it held.
    assert_eq!(d.area_hit.len(), 1);
    assert!((d.area_hit[0].damage - 300.0).abs() < 1e-9, "{:?}", d.area_hit);
    assert!((d.area_hit[0].radius_m - BLAST_AOE_RADIUS_M).abs() < 1e-9);
}

/// GAS AND ELECTRICITY REACH PAST THE BODY THEY LANDED ON.
///
/// The two elements whose proc is an AREA, and both were an ordinary
/// single-body DoT until 2026-08-17 — right while the arena held one body,
/// and half the mechanic once it held 361. Verbatim: gas leaves "a gas
/// cloud that deals a tick of damage each second to all enemies within a
/// 3-meter radius"; electricity "chains between nearby enemies", hitting
/// "all enemies in a 3-meter radius".
///
/// A COUNT, not a total: the claim is that the proc reaches bodies the shot
/// never touched, and only a per-body count can say that. The neighbours
/// stand 2 m from the aimed body — inside both radii — and the far pair at
/// 12 m, outside them, which is the control that stops this passing on a
/// weapon that simply hits everything.
#[test]
fn a_gas_or_electric_proc_reaches_the_bodies_standing_around_it() {
    let build = |element: &str| {
        // A FORCED PROC EVERY SHOT, so the test measures the spread and not
        // the status roll — and a weapon with NO AoE of its own, so the
        // only thing that can reach a neighbour is the proc under test.
        // The Torid was the first fixture and its lingering cloud reached
        // them by itself, which the Toxin control caught.
        let base = crate::model::WeaponBase::from_data("braton_prime", false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(12.0);
        arena.others = [
            crate::space::Vec2::new(2.0, arena.target_at.y),
            crate::space::Vec2::new(-2.0, arena.target_at.y),
            crate::space::Vec2::new(12.0, arena.target_at.y),
            crate::space::Vec2::new(-12.0, arena.target_at.y),
        ]
        .into_iter()
        .map(|at| crate::formation::FoeSpec {
            id: String::new(),
            params: TargetParams::training_dummy(),
            body_parts: BodyPart::humanoid(),
            at,
        })
        .collect();
        let mut p =
            FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        p.status_chance = 1.0;
        p.forced_procs = vec![match element {
            "gas" => DamageType::Gas,
            "electricity" => DamageType::Electricity,
            _ => DamageType::Toxin,
        }];
        p
    };
    for element in ["gas", "electricity"] {
        let r = run_once(&build(element), &mut Rng::new(0x5EED));
        let d = &r.spread.by_body().0;
        assert!(d[0] > 0.0, "{element}: the aimed body");
        assert!(
            d[1] > 0.0 && d[2] > 0.0,
            "{element}: the two bodies 2 m away must take the cloud: {:?}",
            &d[..5]
        );
        assert!(
            d[3] == 0.0 && d[4] == 0.0,
            "{element}: 12 m is outside every radius on the page: {:?}",
            &d[..5]
        );
    }
    // …AND TOXIN, WHICH IS NOT AN AREA, reaches nobody. The control that
    // says this is the ELEMENT's mechanic and not the engine spreading
    // every DoT it has.
    let r = run_once(&build("toxin"), &mut Rng::new(0x5EED));
    assert_eq!(r.spread.touched(), 1, "{:?}", &r.spread.by_body().0[..5]);
}

/// WHAT THE WARFRAME BRINGS REACHES THE NUMBER — both halves, and they are
/// different kinds of thing.
///
/// CORROSIVE PROJECTION is a bonus: a multiplier on the target's armour
/// that is up from the first shot and owes nothing to a proc. The engine's
/// armour formula named the term the day it was written and was never fed a
/// value (data/debuffs/ignite.yaml).
///
/// THE EMERALD SHARD IS A CEILING, and that is the sharp one. `+2 (+3) max
/// stacks of Corrosion`, and `corrosive_strip` is `0.20 + 0.06 x stacks`
/// capped at 1.0 — so ten stacks take 80% of armour and fourteen take all
/// of it. It is the only thing in this family that changes what a build CAN
/// DO rather than how much it does.
#[test]
fn auras_and_shards_reach_the_damage() {
    use crate::auras_data::AuraPick;
    use crate::shards_data::ShardPick;
    let build = |tenno: crate::tenno_data::Tenno| {
        let base = crate::model::WeaponBase::from_data("braton_prime", false, &[]);
        // CORROSIVE, AND ENOUGH STATUS TO STACK IT. A ceiling cannot be
        // measured by a build that never reaches the old one: the first
        // version of this test ran an unmodded rifle, which procs no
        // Corrosive at all, and both sides came back 16,296.
        let pool = crate::mods_data::pool_for_weapon("braton_prime");
        let refs: Vec<&crate::model::ModDef> = ["infected_clip", "stormbringer",
            "rifle_aptitude", "high_voltage", "malignant_force"]
            .iter()
            .filter_map(|id| pool.iter().find(|m| m.id == *id))
            .collect();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(20.0);
        // AN ARMOURED TARGET, because every effect here is about armour.
        arena.target.base_armor = 500.0;
        arena.tenno = tenno;
        FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none())
    };
    let neutral = crate::tenno_data::default_tenno().clone();
    let dmg = |t: crate::tenno_data::Tenno| {
        monte_carlo(&build(t), 40, 0x5EED).mean_effective_damage
    };
    let plain = dmg(neutral.clone());

    // ONE Corrosive Projection is -18% armour; FOUR is -72%, which the
    // aura's own page states as the squad maximum.
    let mut solo = neutral.clone();
    solo.auras = vec![AuraPick { id: "corrosive_projection".into(), count: 1 }];
    let mut squad = neutral.clone();
    squad.auras = vec![AuraPick { id: "corrosive_projection".into(), count: 4 }];
    let (one, four) = (dmg(solo), dmg(squad));
    assert!(one > plain, "one projection helps: {plain:.0} -> {one:.0}");
    assert!(four > one, "four help more: {one:.0} -> {four:.0}");

    // …AND THE CEILING. Five Tauforged Emeralds are +15 stacks, which takes
    // the cap from 10 to 25 — past the 14 that strips all of the armour.
    let mut shards = neutral.clone();
    shards.shards = (0..5)
        .map(|_| ShardPick {
            shard: "emerald_archon_shard".into(),
            effect: "corrosion_stack_cap".into(),
            tauforged: true,
        })
        .collect();
    let capped = dmg(shards);
    assert!(capped > plain,
        "a raised corrosion ceiling strips more armour: {plain:.0} -> {capped:.0}");

    // THE NEGATIVE CONTROL, and it is the one that matters: the NEUTRAL
    // player brings none of this, which is the fight the board is scored
    // under. A default that quietly carried an aura would move every stored
    // row and nothing would say so.
    assert!(neutral.auras.is_empty() && neutral.shards.is_empty(),
        "the neutral player brings no Warframe");
}

/// A PUNCHED HEADSHOT IS CARRIED; A BOUNCE'S IS ROLLED.
///
/// The two are different mechanics and this is where they differ most. Punch through is the SAME round still flying in a
/// STRAIGHT LINE, and this plane holds it at one height — so a round that
/// entered a head enters the head of whatever is behind it, and one that
/// entered a body enters bodies. A BOUNCE leaves at a new angle, so where
/// it lands next is independent and gets its own roll.
///
/// It read `headshot: true, part_factor: 1.0`, wrong in both directions at
/// once: every punched body counted as a headshot for the conditions that
/// read one even when the shot was a body hit, and none of them took the
/// head's MULTIPLIER when it was.
#[test]
fn a_punched_body_inherits_the_headshot_and_a_bounce_does_not() {
    let build = |head: bool| {
        let base = crate::model::WeaponBase::from_data("braton_prime", false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(10.0);
        arena.others = (1..=3)
            .map(|i| crate::formation::FoeSpec {
                id: String::new(),
                params: TargetParams::training_dummy(),
                body_parts: BodyPart::humanoid(),
                at: crate::space::Vec2::new(
                    0.0,
                    crate::space::CONTACT_RANGE_M * (1.0 + i as f64),
                ),
            })
            .collect();
        let mut p =
            FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        p.punch_through_m = 2.0;
        // THE AIMED PELLET, pinned: one part, head or body, so the only
        // thing moving between the two runs is what the round entered.
        p.body_parts = vec![BodyPart {
            name: if head { "head".into() } else { "body".into() },
            aim_weight: 1.0,
            multiplier: if head { 3.0 } else { 1.0 },
            is_head: head,
            crit_bonus: false,
        }];
        p
    };
    let dmg = |head: bool| {
        let r = run_once(&build(head), &mut Rng::new(0x5EED));
        // EVERY BODY BUT THE AIMED ONE: the aimed body's own multiplier is
        // the thing being pinned, so counting it would measure the pin.
        (r.spread.by_body().0[1..4].iter().sum::<f64>(), r.spread.touched())
    };
    let (body, nb) = dmg(false);
    let (heads, nh) = dmg(true);
    assert_eq!((nb, nh), (4, 4), "the same four bodies either way");
    // A 3x HEAD, CARRIED. Not exactly 3x — the punched bodies also take
    // their own falloff and their own mitigation — but far past the noise,
    // and it must be a RISE rather than the 1.0 the old code pinned them to.
    assert!(heads > body * 2.0,
        "a punched body inherits the head: {body:.0} -> {heads:.0}");
    // THE OTHER HALF IS NOT COVERED HERE, and saying so is the point. The
    // instance also carries a headshot FLAG, and the only thing it decides
    // is whether the hit passes a gated SHIELD — which needs a shielded
    // target, and varying the aimed part changes `raw` at the same time, so
    // one engagement cannot separate the two. The flag was `true`
    // unconditionally until 2026-08-21; it is `head_direct` now on the same
    // reasoning as the multiplier above.
}

/// PUNCH THROUGH REACHES THE BODY BEHIND, and the budget decides how many.
///
/// A COUNT, not a total, for the reason the Ocucor test gives: two bodies
/// and three bodies can deal the same damage and only one of them is the
/// weapon. The arrangement is a COLUMN — bodies stacked along the aim line,
/// which is the only place punch-through reaches anybody — at contact
/// spacing, so the thresholds are the wiki table's own.
#[test]
fn punch_through_reaches_the_body_behind_and_the_budget_says_how_many() {
    let build = |n: usize, pt: f64| {
        let base = crate::model::WeaponBase::from_data("braton_prime", false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(10.0);
        // A COLUMN BEHIND THE TARGET, each one contact-spaced from the last
        // — `Arena::training` puts the aimed body on the y axis, so these
        // stand directly behind it.
        arena.others = (1..=n)
            .map(|i| crate::formation::FoeSpec {
                id: String::new(),
                params: TargetParams::training_dummy(),
                body_parts: BodyPart::humanoid(),
                at: crate::space::Vec2::new(
                    0.0,
                    crate::space::CONTACT_RANGE_M * (1.0 + i as f64),
                ),
            })
            .collect();
        let mut p =
            FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        p.punch_through_m = pt;
        p
    };
    // THE TABLE, on a real engagement: 0.4 m reaches nobody behind, 0.5
    // reaches one, 1.0 reaches two.
    for (pt, want) in [(0.0, 1), (0.4, 1), (0.5, 2), (1.0, 3), (2.0, 5)] {
        let r = run_once(&build(4, pt), &mut Rng::new(0x5EED));
        assert_eq!(
            r.spread.touched(), want,
            "{pt} m of punch through: {:?}", &r.spread.by_body().0[..5]
        );
    }
    // …AND ONLY ALONG THE LINE. Punch through penetrates what is in FRONT
    // of the shot; it does not widen it.
    let mut side = build(0, 9.0);
    side.others = vec![crate::formation::FoeSpec {
        id: String::new(),
        params: TargetParams::training_dummy(),
        body_parts: BodyPart::humanoid(),
        at: crate::space::Vec2::new(6.0, 4.0),
    }];
    assert_eq!(run_once(&side, &mut Rng::new(0x5EED)).spread.touched(), 1);
}

/// ARDENT TRIGGER PAYS OFF A COLUMN AND NOTHING OFF A LONE TARGET.
///
/// *"On Punch Through Hit: +40% Fire Rate for 6 seconds."* The trigger asks
/// for a body BEHIND the one you hit, so a one-target ruler is where this
/// perk is worth exactly zero — and the Paris Prime carries 3 m of innate
/// punch through, which is what makes the column reachable with no mod in.
///
/// ON A BOW THE RATE IS THE DRAW, so the whole of what the buff buys is
/// shorter charge time. That is why this is a SHOT COUNT and not a damage
/// total: the perk moves how often the weapon fires and nothing about what
/// a shot is worth, and a damage assertion would be reading the wrong
/// number for the right reason.
#[test]
fn ardent_trigger_buys_draw_speed_and_only_against_a_column() {
    let shots = |evo: &[&str], behind: usize| {
        let base = crate::model::WeaponBase::from_data("paris_prime", true, evo);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(10.0);
        arena.others = (1..=behind)
            .map(|i| crate::formation::FoeSpec {
                id: String::new(),
                params: TargetParams::training_dummy(),
                body_parts: BodyPart::humanoid(),
                at: crate::space::Vec2::new(
                    0.0,
                    crate::space::CONTACT_RANGE_M * (1.0 + i as f64),
                ),
            })
            .collect();
        let p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        monte_carlo(&p, 8, 0x5017).mean_shots
    };
    let perk = ["paris_prime_ardent_trigger"];

    // A LONE TARGET EARNS IT NOTHING — the mechanic, not an admission.
    let alone = shots(&perk, 0);
    assert!(
        (alone - shots(&[], 0)).abs() / alone < 1e-6,
        "nothing behind the target, so nothing to punch through: {alone} against {}",
        shots(&[], 0)
    );

    // …AND A COLUMN EARNS IT EVERY SHOT. 40% off a 0.5 s draw is the whole
    // gain, so the count rises by less than 40% — the reload and the
    // between-shot time it does not touch are the rest of the cycle.
    let (with, without) = (shots(&perk, 2), shots(&[], 2));
    assert!(
        with > without * 1.02,
        "a column arms it every shot: {with} against {without}"
    );
    assert!(
        with < without * 1.40,
        "…and it can only buy the DRAW, not the whole cycle: {with} against {without}"
    );
}

/// A RANGE IS A WALL FOR THE BODIES A SHOT PUNCHES THROUGH TOO.
///
/// The gate was computed ONCE, off the aimed body's gap, and every body
/// behind it was punched through at whatever distance it stood — so a 25 m
/// beam reached the whole of the group-clear ruler's column, the last rank
/// 54.4 m away, and a beam-range card bought nothing in a crowd.
///
/// A COUNT, not a total, for the reason the test above it gives: the
/// question is who the beam REACHED.
#[test]
fn a_beams_range_is_a_wall_for_the_bodies_behind_as_well() {
    let touched = |mods: &[&str]| {
        let base = crate::model::WeaponBase::from_data("phantasma_prime", false, &[]);
        let pool = crate::mods_data::pool_for_weapon("phantasma_prime");
        let refs: Vec<&crate::model::ModDef> = mods
            .iter()
            .map(|m| pool.iter().find(|d| d.id == *m).unwrap_or_else(|| panic!("{m}")))
            .collect();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(10.0);
        // THE RULER'S OWN COLUMN — a body every 3 m, out to 36 m, which is
        // half again the weapon's 25 m reach.
        arena.others = (1..=12)
            .map(|i| crate::formation::FoeSpec {
                id: String::new(),
                params: TargetParams::training_dummy(),
                body_parts: BodyPart::humanoid(),
                at: crate::space::Vec2::new(0.0, 3.0 * f64::from(i)),
            })
            .collect();
        let p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        assert!(p.range_m.is_finite(), "the wall is a number: {}", p.range_m);
        run_once(&p, &mut Rng::new(0x5EED)).spread.touched()
    };
    let bare = touched(&[]);
    assert!(
        bare < 13,
        "a 25 m beam cannot reach a body 36 m away: {bare} of 13 bodies"
    );
    // …AND THE CARD THAT MOVES THE WALL IS WORTH A BODY. Sinister Reach is
    // the one mod in the pool that buys beam range, and a crowd is the only
    // place it can be worth anything at all.
    let reaching = touched(&["sinister_reach"]);
    assert!(
        reaching > bare,
        "beam range buys bodies in a column: {reaching} against {bare}"
    );
}

/// ONE ROUND, ONE ANSWER, ALL THE WAY DOWN THE COLUMN.
///
/// Punch through is the same round still flying at one height, so a round
/// that entered a head enters the head of whatever is behind it and one
/// that entered a body stays a body shot the whole way. The DAMAGE has said
/// so since the head factor started travelling inside `raw_per_bucket`; the
/// RECORD did not, because a spread row was built as Head-or-Direct and
/// never Crit — so a body a shot punched through took the crit and popped a
/// white number for it.
///
/// ASSERTED ON THE STREAM, not on a total: the question is whether the rows
/// one tick writes agree, and a sum cannot be asked that.
#[test]
fn one_round_pops_the_same_kind_of_number_on_every_body_it_crosses() {
    let base = crate::model::WeaponBase::from_data("phantasma_prime", false, &[]);
    let panel = crate::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let mut arena = crate::arena::Arena::training(6.0);
    arena.others = (1..=4)
        .map(|i| crate::formation::FoeSpec {
            id: String::new(),
            params: TargetParams::training_dummy(),
            body_parts: BodyPart::humanoid(),
            at: crate::space::Vec2::new(0.0, 3.0 * f64::from(i)),
        })
        .collect();
    let p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
    let rec = record(&p, 0x5EED, 0.0, 6.0, 60_000, 0);
    // WHAT EACH INSTANT POPPED — the round's own rows only, since a status
    // tick is on its own clock and not on this one.
    let mut per_tick: std::collections::BTreeMap<u64, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    let mut crits = 0;
    for e in rec.events() {
        let crate::record::Kind::Damage(d) = &e.kind else { continue };
        if !matches!(
            d.origin,
            crate::record::Origin::Own | crate::record::Origin::PunchThrough
        ) {
            continue;
        }
        if d.origin == crate::record::Origin::PunchThrough && d.crit_tier > 0 {
            crits += 1;
        }
        per_tick
            .entry((e.t * 1000.0).round() as u64)
            .or_default()
            .insert(format!("{:?}", d.kind));
    }
    assert!(per_tick.len() > 20, "the fight ran: {} ticks", per_tick.len());
    let split: Vec<_> = per_tick.iter().filter(|(_, k)| k.len() > 1).collect();
    assert!(
        split.is_empty(),
        "one round, one answer — these instants disagreed: {split:?}"
    );
    // …AND THE CRIT IS ON THE ROW, not merely inside the number. Without
    // this the assertion above passes on a stream that calls every punched
    // hit a plain one.
    assert!(crits > 0, "a punched body's crit is drawn as a crit");
}

/// A PUNCHED BODY'S BURN IS THE SAME SIZE AS THE AIMED BODY'S.
///
/// Heat and Blast read the hit's weak point — measured, MEASUREMENTS M54 —
/// and their payloads are built from the MODDED BASE, which carries no head
/// factor. Punch through reads 1.0 for its hit on purpose (the multiplier
/// is already inside the raw it was handed), and that 1.0 reached the
/// payloads too: a punched body burned for a third of the aimed body's on a
/// ruler whose own rule is that a punched body IS a weak-point hit.
///
/// TOXIN IS THE SECOND READING OF THE SAME CLAIM. It reads the weak point
/// too (M91), so it must land symmetric for the same reason Heat does —
/// where it was once the control that could not move at all.
#[test]
fn a_punched_bodys_burn_is_the_size_the_aimed_bodys_is() {
    let burn = |element: &str| {
        let base = crate::model::WeaponBase::from_data("phantasma_prime", false, &[]);
        let pool = crate::mods_data::pool_for_weapon("phantasma_prime");
        let refs: Vec<&crate::model::ModDef> = [element]
            .iter()
            .map(|m| pool.iter().find(|d| d.id == *m).unwrap_or_else(|| panic!("{m}")))
            .collect();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(10.0);
        arena.others = vec![crate::formation::FoeSpec {
            id: String::new(),
            params: TargetParams::training_dummy(),
            body_parts: BodyPart::humanoid(),
            at: crate::space::Vec2::new(0.0, crate::space::CONTACT_RANGE_M * 2.0),
        }];
        let mut p =
            FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        // EVERY SHOT ON THE HEAD, which is the rulers' own rule and the only
        // aim under which this question has an answer at all.
        p.body_parts = vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: true,
        }];
        let r = run_once(&p, &mut Rng::new(0x5EED));
        let (aimed, behind) = (r.spread.by_body().0[0], r.spread.by_body().0[1]);
        assert!(behind > 0.0, "{element}: the beam reaches the body behind");
        aimed / behind
    };
    // Heat is the one that moved: 1.88 before, and the burn is most of the
    // damage on a build whose only element is one.
    let heat = burn("incendiary_coat");
    assert!(
        (heat - 1.0).abs() < 0.15,
        "the same round, so the same burn: aimed/behind = {heat}"
    );
    let toxin = burn("toxic_barrage");
    assert!(
        (toxin - 1.0).abs() < 0.15,
        "…and the other family member lands the same, for the same reason: {toxin}"
    );
}

/// A MERGED BEAM'S STATUSES ARE THE SAME SIZE ON THE BODY BEHIND.
///
/// Multishot on a continuous weapon merges into ONE bigger tick and its
/// ModifiedBase carries the merge, which is what makes a damaging status
/// *"affected twice by multishot"*. Every spread was handed the UNMERGED
/// base, so a body a beam punched through took the same HIT and a DoT
/// `beam_merge` times smaller — on the group-clear ruler the aimed Thrax
/// read twice the damage of the one standing behind it, out of a Toxin tick
/// of 1778 against 32.6.
///
/// A RATIO OF TOTALS, and a training dummy so the two bodies differ in
/// nothing else: same target, no armour, neither of them dies.
#[test]
fn a_merged_beams_statuses_are_the_same_size_on_the_body_behind() {
    let base = crate::model::WeaponBase::from_data("phantasma_prime", false, &[]);
    let pool = crate::mods_data::pool_for_weapon("phantasma_prime");
    // MULTISHOT AND SOMETHING THAT BURNS: the weapon's own Radiation leaves
    // no DoT, so the merge has nothing to be read off without a Toxin card.
    let refs: Vec<&crate::model::ModDef> = ["hells_chamber", "toxic_barrage"]
        .iter()
        .map(|m| pool.iter().find(|d| d.id == *m).unwrap_or_else(|| panic!("{m}")))
        .collect();
    let panel = crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
    let mut arena = crate::arena::Arena::training(10.0);
    arena.others = vec![crate::formation::FoeSpec {
        id: String::new(),
        params: TargetParams::training_dummy(),
        body_parts: BodyPart::humanoid(),
        at: crate::space::Vec2::new(0.0, crate::space::CONTACT_RANGE_M * 2.0),
    }];
    let p = FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
    assert!(p.punch_through_m > 0.0, "the innate punch through reaches it");
    let r = run_once(&p, &mut Rng::new(0x5EED));
    let (aimed, behind) = (r.spread.by_body().0[0], r.spread.by_body().0[1]);
    assert!(behind > 0.0, "the beam reaches the body behind at all");
    assert!(
        (behind / aimed - 1.0).abs() < 0.15,
        "the same round, so the same damage: {aimed} aimed against {behind} behind"
    );
}

/// AN AoE ATTACK TAKES NO PUNCH THROUGH, from its weapon or from a mod.
///
/// The punch-through page's own catalog rule, both halves: *"weapon
/// projectiles with an area of effect (AoE) component will not Punch
/// Through enemies or level geometry at all"*, and *"Projectile AoE weapons
/// cannot have their Punch Through stat modified"*. So a Shred on a grenade
/// launcher is worth literally nothing — which is a thing to SAY, not a
/// number to quietly add up.
#[test]
fn an_aoe_attack_takes_no_punch_through_from_a_mod() {
    // The MOD POOL is the parent weapon's — an Incarnon form is a form of a
    // gun, not a gun of its own (AGENTS.md: "a form inherits its weapon").
    let panel = |w: &str, pool_of: &str, mods: &[&str]| {
        let base = crate::model::WeaponBase::from_data(w, false, &[]);
        let pool = crate::mods_data::pool_for_weapon(pool_of);
        let refs: Vec<&crate::model::ModDef> = mods
            .iter()
            .map(|m| pool.iter().find(|d| d.id == *m).unwrap_or_else(|| panic!("{m}")))
            .collect();
        crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent)
    };
    // A RIFLE TAKES IT: Primed Shred is +2.2 m at max rank.
    let bare = panel("braton_prime", "braton_prime", &[]);
    let shred = panel("braton_prime", "braton_prime", &["primed_shred"]);
    assert!((bare.punch_through_m - 0.0).abs() < 1e-9);
    assert!(shred.punch_through_m > 2.0, "{}", shred.punch_through_m);

    // A BEAM WITH AN AoE COMPONENT DOES NOT EITHER, and only its own page
    // says so: the Torid's Incarnon form carries no `radial:` and no
    // `lingering:`, so the class rule about AoE PROJECTILES does not reach
    // it — *"Punch Through mods have no effect on the behavior of the
    // beam"* is a declaration in the entry (`punch_through_mods: false`).
    // It is the case that proves the shape cannot be the whole rule.
    let inc_bare = panel("torid_incarnon", "torid", &[]);
    let inc_shred = panel("torid_incarnon", "torid", &["primed_shred"]);
    assert!(inc_bare.radial.is_none() && inc_bare.lingering.is_none(),
        "the fixture must be the case the class rule would get wrong");
    assert!((inc_shred.punch_through_m - 0.0).abs() < 1e-9,
        "the Incarnon beam takes no punch through: {}", inc_shred.punch_through_m);

    // THE TORID'S BASE FORM DOES NOT — it explodes on first contact.
    let torid_bare = panel("torid", "torid", &[]);
    let torid_shred = panel("torid", "torid", &["primed_shred"]);
    assert!(torid_bare.lingering.is_some(), "the fixture must be an AoE weapon");
    assert!((torid_shred.punch_through_m - torid_bare.punch_through_m).abs() < 1e-9,
        "an AoE attack's punch through cannot be modified: {} vs {}",
        torid_shred.punch_through_m, torid_bare.punch_through_m);
}

/// THE OCUCOR REACHES FIVE BODIES AND NO MORE — one beam and four tendrils. The COUNT is the claim a formation exists to make,
/// and a total cannot make it: five bodies and six bodies can deal the same
/// damage and only one of them is the weapon.
#[test]
fn the_ocucor_reaches_exactly_five_bodies() {
    let build = |n: usize, tendrils: u32| {
        let base = crate::model::WeaponBase::from_data("ocucor", false, &[]);
        let refs: Vec<&crate::model::ModDef> = Vec::new();
        let panel =
            crate::loadout::resolve(&base, &refs, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(10.0);
        // A LINE ACROSS THE FRONT, all inside the tendrils' 20 m and their
        // 40 degree cone off the reticle.
        arena.others = (1..=n)
            .map(|i| crate::formation::FoeSpec {
                id: String::new(),
                params: TargetParams::training_dummy(),
                body_parts: BodyPart::humanoid(),
                at: crate::space::Vec2::new(i as f64 * 0.7, 4.0),
            })
            .collect();
        let mut p =
            FightParams::from_panel(&panel, &arena, &crate::arcanes_data::ArcaneFx::none());
        p.tendrils_initial = tendrils;
        p.tendrils_held = true;
        p
    };
    // EIGHT BODIES ON THE FLOOR, four tendrils up: five take damage.
    let r = run_once(&build(8, 4), &mut Rng::new(0x5EED));
    assert_eq!(r.spread.touched(), 5, "{:?}", &r.spread.by_body().0[..9]);
    // …and it is the AIMED one plus the four nearest the reticle, in order.
    assert!(r.spread.by_body().0[0] > 0.0, "the beam's own body");
    assert!(r.spread.by_body().0[1..=4].iter().all(|d| *d > 0.0), "the four tendrils");
    assert!(r.spread.by_body().0[5..=8].iter().all(|d| *d == 0.0), "and nobody else");

    // FEWER TENDRILS REACH FEWER BODIES, one for one — which is what says
    // the count is the tendrils' and not the formation's.
    for up in 0..=4u32 {
        assert_eq!(
            run_once(&build(8, up), &mut Rng::new(0x5EED)).spread.touched(),
            1 + up as usize,
            "{up} tendrils"
        );
    }
    // …AND A SHORT FORMATION IS NOT PADDED: two bodies is two, however many
    // tendrils are up.
    assert_eq!(run_once(&build(1, 4), &mut Rng::new(0x5EED)).spread.touched(), 2);
}

/// A CONE IS SPELLED ONE WAY. Ten entries carried BOTH a parsed
/// `spread: {min_deg, max_deg}` and a flat `spread_min_deg`/`spread_max_deg`
/// that no code read — the second spelling arrived with the intake on
/// 2026-08-15 beside a hand-transcribed one nobody had removed.
///
/// Two of the ten DISAGREED, and the unparsed one was wrong in the way this
/// repo has on the record: `furis_incarnon` and `mk1_furis_incarnon` had
/// their BASE form's 1/8 written on them, where the wiki module gives the
/// Incarnon attack 5/15. That is the Larkspur error again (AGENTS.md §"A
/// FORM INHERITS ITS WEAPON"), and it survived because nothing could see
/// the field — which is the whole argument for one spelling.
#[test]
fn a_cone_is_spelled_one_way() {
    for path in crate::data::files_under("weapons/") {
        let text = path.1;
        assert!(
            !text.contains("spread_min_deg") && !text.contains("spread_max_deg"),
            "{}: carries a second, unparsed spelling of its cone — `spread:` is the one",
            path.0
        );
    }
}

/// THE CHILL LADDER, AGAINST THE MEASURED TABLE.
///
/// Laetum base form (crit multiplier 2.2), Lavos's +200% Cold, every shot
/// on a Demolisher's torso — a target that cannot freeze, so the ladder can
/// be walked one stack at a time all the way to ten. Non-crit held at 192
/// (192.3 before the display rounded it) and the crit read:
///
/// | stacks | crit | crit/192.3 | ladder |
/// |---|---|---|---|
/// | 0  | 423 | 2.20 | 0.00 |
/// | 1  | 442 | 2.30 | 0.10 |
/// | 2  | 452 | 2.35 | 0.15 |
/// | 3  | 462 | 2.40 | 0.20 |
/// | 5  | 481 | 2.50 | 0.30 |
/// | 10 | 529 | 2.75 | 0.55 |
///
/// Every row lands within half a point of `2.2 + 0.10 + 0.05 x (n - 1)`,
/// including the TENTH rung — which is one past the published table and was
/// briefly capped away on a wiki inference. MEASUREMENTS M46.
#[test]
fn the_chill_ladder_matches_the_measured_table() {
    for (stacks, want) in [(0, 0.00), (1, 0.10), (2, 0.15), (3, 0.20), (5, 0.30), (10, 0.55)] {
        let d = chill(stacks, None, true, false); // cannot freeze: walk to ten
        assert!(
            (d.chill_cd_bonus() - want).abs() < 1e-9,
            "{stacks} stacks: {} against a measured {want}",
            d.chill_cd_bonus()
        );
    }
}

/// …AND A HIT IS SCALED BY THE STACKS THAT WERE ALREADY THERE, not by the
/// one it is about to apply.
///
/// It is the rule that reads the measured table straight: the shot fired
/// into a target at 0 stacks — the one that takes it to 1 — came back 423,
/// which is the bare 2.2 multiplier. Its own Cold proc did not pay it.
///
/// It also explains a reading that looked like a fault: the same weapon on
/// the same target alternated between 423 and 529 with an unchanged
/// non-crit, which is not a bonus flickering but the first shot into a
/// fresh target (0 stacks) beside a shot once the ladder was full (10).
///
/// THE ENGINE ALREADY DID THIS and now knows why: `cd_abs` is read at the
/// top of the pellet body, before `settle_procs` applies that pellet's own
/// status. Earlier pellets of the SAME pull do count — they landed first.
#[test]
fn a_hit_reads_the_stacks_that_were_already_on_the_target() {
    let mut d = DebuffState::default();
    // The shot that takes a fresh target from 0 to 1 is scaled by 0.
    let before = d.chill_cd_bonus();
    d.apply_cold_proc(0.0, 1.0, false, None, true);
    let after = d.chill_cd_bonus();
    assert!((before - 0.00).abs() < 1e-9, "the 0 -> 1 shot sees nothing");
    assert!((after - 0.10).abs() < 1e-9, "and the NEXT one sees one stack");
}
