use super::*;

/// …AND BOTH HALVES OF THE CYCLE TAKE IT, including the way OUT.
///
/// The wiki names one direction — "Swapping to Incarnon Form counts as
/// reloading the Soma Prime and will therefore end the bonus" — and the
/// revert is MEASURED rather than inferred: it clears the pile too, which
/// the weapon's GENESIS page states outright ("Switching to and from
/// Incarnon Form resets Hata-Satya's bonus").
///
/// The sharp case for this mod, because the pile is at its ceiling exactly
/// when the Incarnon ammo runs out: without it the base form opens on +500%
/// and keeps it for a whole magazine.
///
/// THE PILE IS READ IN THE BASE FORM ALONE, which is what makes the count
/// mean anything: the mod is the WEAPON's, so both forms build it and both
/// forms spend it, and a total over the whole run cannot tell a pile that
/// survived the revert from one that was rebuilt after it. The Incarnon
/// panel carries no crit chance for the pile to multiply, so every crit
/// here was landed after the revert.
#[test]
fn leaving_the_incarnon_form_clears_hata_satyas_pile() {
    let head = vec![BodyPart {
        name: "head".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: true,
        crit_bonus: false,
    }];
    let base_form = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 50.0),
        base_crit_chance: 0.0,
        unmodded_crit_chance: 1.0, // the base a relative bonus multiplies
        crit_multiplier: 2.0,
        arcane: ArcaneFx::none(),
        magazine_size: 200.0,
        fire_rate: 10.0,
        body_parts: head.clone(),
        ..no_status()
    };
    // 2% a hit, so 40 Incarnon rounds are worth +80% carried over and the
    // twenty shots after the revert are worth +38% rebuilt from zero —
    // two numbers no run can confuse.
    let build = |held: bool| FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 50.0),
        base_crit_chance: 0.0,
        unmodded_crit_chance: 0.0, // …and NONE in the Incarnon form
        crit_multiplier: 2.0,
        crit_chance_per_hit: Some(crate::model::CritPerHit {
            per_stack: 0.02,
            max_bonus: 5.0,
        }),
        crit_chance_per_hit_held: held,
        arcane: ArcaneFx::none(),
        magazine_size: 40.0,
        ammo_efficiency_applies: false,
        infinite_reserve: true,
        fire_rate: 10.0,
        duration_seconds: 6.5,
        body_parts: head.clone(),
        foe: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
        cycle: Some(IncarnonCycle {
            starts_primed: true,        // open IN the form…
            base_form: Box::new(base_form.clone()),
            // …and never earn it back
            arms: Arms::Gauge {
                charge_on: crate::model::ChargeOn::WeakpointHits,
                charges_to_fill: 1_000_000,
            },
            ends: Ends::ChargeMagazine,
            transmute_out_seconds: 0.5,
            transmute_seconds: 1.0,
            reload_bucket: 0.0,
        }),
        ..no_status()
    };
    let cleared = run_once(&build(false), &mut Rng::new(11));
    assert_eq!(cleared.transforms, 0, "this fixture opens primed and never transforms back in");
    let after = cleared.pellets - 40;
    assert!(after >= 15, "the base form has to fire after the revert: {after} shots");

    // THE NEGATIVE CONTROL, and it is the card's own setting: "no timeout"
    // means no event takes the pile, so the same fixture must carry it
    // through the revert and crit nearly every shot. Without this the
    // assertion above passes just as well on a build that lost the pile
    // for some other reason — or never had one.
    let held = run_once(&build(true), &mut Rng::new(11));
    assert!(
        held.crits >= after - 1,
        "held: the pile must survive the revert at its 80% and crit through, got {} of {after}",
        held.crits
    );
    assert!(
        cleared.crits * 3 < held.crits,
        "the revert must clear the pile: {} crits over {after} base-form shots against {} held",
        cleared.crits, held.crits
    );
}

/// …AND THE CAP IS ON THE BONUS, not on the stack count.
///
/// 500% is published flat at every rank, and DE does not stop counting hits
/// to protect it: the pile keeps taking hits and what it is WORTH stops at
/// the ceiling. The two readings are told apart by the
/// LAST STACK — 416 x 1.2% is 499.2%, and the 417th hit is the one that
/// makes it 500% rather than the one that overshoots and is refused.
#[test]
fn hata_satyas_pile_stops_at_its_published_ceiling() {
    // THE CARD'S OWN NUMBERS, because this is arithmetic about them: 1.2%
    // a hit under a 500% ceiling.
    let card = crate::model::CritPerHit { per_stack: 0.012, max_bonus: 5.0 };
    assert_eq!(card.max_stacks(), 417, "the ceiling is reached, not fitted under");
    assert!((card.bonus(416) - 4.992).abs() < 1e-12, "the 416th is still under it");
    assert!((card.bonus(417) - 5.0).abs() < 1e-12, "and the 417th reads the ceiling");
    // Nothing past it moves a number, which is why the counter may stop.
    assert!((card.bonus(10_000) - 5.0).abs() < 1e-12);
    // AT RANK 0 THE SAME CEILING IS 2,500 HITS AWAY, which is the half a
    // stack count written into the yaml could never have expressed.
    let rank0 = crate::model::CritPerHit { per_stack: 0.002, max_bonus: 5.0 };
    assert_eq!(rank0.max_stacks(), 2_500);

    // …and it BINDS in a fight. Same fixture, two ceilings.
    let build = |max_bonus: f64| FightParams {
        base_crit_chance: 0.0,
        unmodded_crit_chance: 1.0,
        crit_multiplier: 2.0,
        // 1% a hit, so the ceiling is reached in `max_bonus x 100` hits —
        // and a crit rate is clamped at 1.0, so a ceiling has to bite BELOW
        // that to be visible at all.
        crit_chance_per_hit: Some(crate::model::CritPerHit {
            per_stack: 0.01,
            max_bonus,
        }),
        magazine_size: 100_000.0,
        fire_rate: 10.0,
        duration_seconds: 60.0,
        body_parts: mono_body(1.0),
        foe: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
        ..flat_base()
    };
    let rate = |max_bonus: f64| {
        let r = run_once(&build(max_bonus), &mut Rng::new(9));
        r.crits as f64 / r.pellets.max(1) as f64
    };
    // 600 shots at 10/s over 60 s, so both piles fill; only the ceiling
    // separates them.
    let low = rate(0.20);
    let high = rate(0.80);
    assert!(
        low < 0.35 && high > 0.6,
        "the ceiling must bind: +20% gave {low}, +80% gave {high}"
    );
}

/// EXIMUS ADVANTAGE: the window is opened by a weak-point hit ON AN EXIMUS,
/// and by nothing else.
///
/// The second half is the one worth pinning. Written as a plain on-headshot
/// buff the mod would pay +600% base damage in every fight, and the card
/// would look like one of the strongest in the pool against a target it
/// does nothing for.
#[test]
fn eximus_advantage_needs_an_eximus_and_a_weak_point() {
    let build = |eximus: bool, head_weight: f64| {
        let mut p = FightParams {
            base_damage_on_eximus_weakpoint: Some(crate::model::TimedBuff {
                value: 6.0,
                duration: 10.0,
                initial_active: false,
            }),
            magazine_size: 100_000.0,
            fire_rate: 10.0,
            duration_seconds: 30.0,
            // A head and a body, so "aim at the head" is a choice the
            // fixture can make rather than the only thing it can do.
            body_parts: vec![
                BodyPart {
                    name: "head".into(),
                    aim_weight: head_weight,
                    multiplier: 1.0, // no head MULTIPLIER: isolate the buff
                    is_head: true,
                    crit_bonus: false,
                },
                BodyPart {
                    name: "body".into(),
                    aim_weight: 1.0 - head_weight,
                    multiplier: 1.0,
                    is_head: false,
                    crit_bonus: false,
                },
            ],
            foe: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
            ..flat_base()
        };
        p.foe.can_be_eximus = eximus;
        p.foe.eximus = eximus;
        p
    };
    let dmg = |p: &FightParams| run_once(p, &mut Rng::new(21)).effective_damage();

    // The two halves of the trigger, each denied in turn.
    let armed = dmg(&build(true, 1.0));
    let not_eximus = dmg(&build(false, 1.0));
    let not_weakpoint = dmg(&build(true, 0.0));
    assert!(
        armed > not_eximus * 1.5,
        "an Eximus weak-point hit must pay: {armed} vs {not_eximus}"
    );
    assert!(
        (not_weakpoint - not_eximus).abs() / not_eximus.max(1.0) < 0.05,
        "body hits on an Eximus must pay nothing: {not_weakpoint} vs {not_eximus}"
    );
}

/// THE TENDRIL CARD: the one way to measure this mod in the fight it is
/// actually played in.
///
/// A tendril costs a KILL, so against a target that does not die the
/// Ocucor's only augment is worth exactly nothing and the weapon reads as
/// if it were unmodded. The count is a buff by every test that matters, so
/// it takes a buff card's two knobs, and this pins both: the seed is worth
/// its stacks, and the LOCK is what carries them past a reload.
#[test]
fn the_tendril_card_seeds_the_count_and_the_lock_holds_it() {
    // No kills at all: the target never dies, so the sim can never grant a
    // tendril and every crit below came from the card.
    let build = |stacks: u32, held: bool, mag: f64| FightParams {
        base_crit_chance: 0.0,
        unmodded_crit_chance: 1.0,
        crit_multiplier: 2.0,
        tendril_max: 4,
        crit_chance_per_tendril: 0.25,
        tendrils_initial: stacks,
        tendrils_held: held,
        magazine_size: mag,
        reload_seconds: 0.5,
        fire_rate: 10.0,
        duration_seconds: 60.0,
        body_parts: mono_body(1.0),
        foe: frail_target(TargetMode::InfiniteHealth, 0.0, 1e12),
        ..flat_base()
    };
    let crit_rate = |p: &FightParams| {
        let r = run_once(p, &mut Rng::new(7));
        assert_eq!(r.kills, 0, "this fixture must never kill: the card is the only source");
        r.crits as f64 / r.pellets.max(1) as f64
    };

    // Unset, the mod is unmeasurable — the state the report describes.
    assert!(crit_rate(&build(0, false, 100_000.0)) < 1e-9);
    // Four tendrils at +25% each, off a base of 1.0: every shot crits.
    assert!(
        crit_rate(&build(4, false, 100_000.0)) > 0.99,
        "the card must reach the fight"
    );
    // ...and it is the COUNT, not a switch: two tendrils buy half of it.
    let two = crit_rate(&build(2, false, 100_000.0));
    assert!((0.45..0.55).contains(&two), "two tendrils should be worth ~50%, got {two}");

    // A ONE-ROUND magazine reloads after every shot, and a reload clears
    // tendrils — the seed included, because the seed is the same buff.
    let unheld = crit_rate(&build(4, false, 1.0));
    assert!(unheld < 0.2, "a reload must spend the seed too, got {unheld}");
    // LOCKED ("no timeout"): for a buff whose end is an event rather than
    // a clock, that event is what stops happening.
    let held = crit_rate(&build(4, true, 1.0));
    assert!(held > 0.99, "a locked card must survive every reload, got {held}");
}

/// SENTIENT SURGE'S REFILL PAYS EACH KILL ONCE — and is not a reload.
///
/// "On Kill: Refill X% of the Magazine", drawn from the reserve, so a kill
/// is worth a FIXED number of rounds and then it is spent. On the Ocucor
/// that is 20% of 60 = 12 rounds against 6 ammo a second, i.e. one kill
/// every two seconds and the weapon never reloads again — which is the
/// mod's whole reputation, and is a property of the KILL RATE rather than
/// of the refill being generous.
///
/// The bug this pins re-earned every kill on every loop iteration, which
/// topped the magazine up on every shot and quietly handed the weapon an
/// infinite one. It showed up nowhere except the reload count, and the
/// tendril test never looked at reloads.
#[test]
fn the_magazine_refill_pays_each_kill_once() {
    // 10 rounds, 1 ammo a shot, 1 shot a second, and a target that dies to
    // every shot: 10 kills a magazine, each worth 20% of 10 = 2 rounds.
    let build = |refill: f64| FightParams {
        magazine_size: 10.0,
        ammo_cost: 1.0,
        fire_rate: 1.0,
        reload_seconds: 1.0,
        duration_seconds: 120.0,
        magazine_refill_on_kill: refill,
        body_parts: mono_body(1.0),
        foe: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
        ..flat_base()
    };
    let run = |refill: f64| {
        let r = run_once(&build(refill), &mut Rng::new(3));
        (r.shots, r.reloads)
    };

    let (bare_shots, bare_reloads) = run(0.0);
    assert!(bare_reloads > 5, "the fixture must reload a lot, got {bare_reloads}");

    // Every shot kills, so every shot returns 2 rounds while spending 1:
    // the magazine fills faster than it drains and the reloads stop.
    let (surge_shots, surge_reloads) = run(0.20);
    assert!(surge_reloads < bare_reloads, "the refill must save reloads");
    assert!(surge_shots > bare_shots, "and buy shots with the time saved");

    // THE ARITHMETIC, on a refill too small to outrun the drain. 5% of 10
    // is 0.5 rounds a kill against 1 spent, so the magazine still empties —
    // just half as often. NEVER near zero, which is where the bug put it.
    let (_, slow_reloads) = run(0.05);
    assert!(
        slow_reloads > bare_reloads / 3 && slow_reloads < bare_reloads,
        "a 5% refill halves the drain, it does not remove it: bare {bare_reloads}, \
             with refill {slow_reloads}"
    );

    // AND A REFILL IS NOT A RELOAD, which is the entire point of pairing it
    // with a passive that a reload destroys. Measured as tendrils SURVIVING:
    // a per-tendril crit bonus off a zero base, so every crit counted here
    // belongs to a tendril that lived through a magazine being topped up.
    let surging = FightParams {
        base_crit_chance: 0.0,
        unmodded_crit_chance: 1.0,
        tendril_max: 4,
        crit_chance_per_tendril: 0.25,
        ..build(0.20)
    };
    let r = run_once(&surging, &mut Rng::new(3));
    assert!(
        r.crits as f64 / r.pellets.max(1) as f64 > 0.5,
        "tendrils must survive a refill — if the refill cleared them like a \
             reload does, the crit rate would sit near zero"
    );
}

/// A SYNDICATE RADIAL IS ARMED BY AFFINITY AND CAPPED BY ITS COOLDOWN.
///
/// "All affinity earned by the weapon during a mission fills a gauge",
/// 1000 points at a maxed augment, and a weapon kill gives the weapon HALF
/// the enemy's affinity — "Kill with weapons: Half Affinity goes to the
/// Warframe and half to the killing weapon".
///
/// Two things worth pinning separately, because a model can get either one
/// right alone: that kills ARM it at the rate the wiki gives, and that the
/// 30 s cooldown BOUNDS it however fast you kill.
#[test]
fn a_syndicate_radial_arms_on_affinity_and_is_capped_by_its_cooldown() {
    let truth = *crate::data::syndicates::get("truth").expect("truth");
    // A target worth 200 affinity at level 1: the multiplier is
    // 1 + 0.1425 = 1.1425, so 228 floored, and the weapon's half is 114.
    // Nine kills fill the 1000-point gauge.
    let mut t = frail_target(TargetMode::InstantRespawn, 0.0, 0.0);
    t.base_affinity = 200.0;
    let build = |secs: f64, radial: Option<crate::data::syndicates::SyndicateDef>| FightParams {
        syndicate_radial: radial,
        magazine_size: 100_000.0,
        fire_rate: 10.0,
        duration_seconds: secs,
        body_parts: mono_body(1.0),
        foe: t.clone(),
        ..flat_base()
    };
    let blasts = |p: &FightParams| {
        let r = run_once(p, &mut Rng::new(11));
        (r.sources.syndicate / truth.damage).round() as u32
    };

    // OFF: no augment, no explosions, however much dies.
    assert_eq!(blasts(&build(300.0, None)), 0, "no augment, no radial");

    // ON, and every shot kills, so the gauge refills far faster than the
    // cooldown allows: 300 s / 30 s = 10 detonations and not one more.
    assert_eq!(
        blasts(&build(300.0, Some(truth))),
        10,
        "the cooldown bounds it however fast the gauge fills"
    );
    // ...and the bound is the COOLDOWN, not the clock: half the time, half
    // the blasts.
    assert_eq!(blasts(&build(150.0, Some(truth))), 5);

    // THE ARMING IS REAL, not a timer wearing a gauge's name. A target
    // worth a tenth as much affinity takes ten times the kills to fill it,
    // and at this fire rate that is slow enough to miss cooldowns.
    let mut poor = t.clone();
    poor.base_affinity = 2.0;
    let starved = blasts(&FightParams { foe: poor, ..build(300.0, Some(truth)) });
    assert!(
        starved < 10,
        "a low-affinity target must fill the gauge more slowly, got {starved} blasts"
    );
}

#[test]
fn the_explosion_rolls_its_own_status_apart_from_the_direct_hit() {
    // Wiki (Laetum): "Initial hit and explosion apply status
    // separately." The direct hit here can never proc (0% SC), so any
    // proc at all is the explosion's own draw.
    let quiet = run_once(&radial_only(radial_of(0.0, 0.0)), &mut Rng::new(7));
    assert_eq!(quiet.procs, 0, "0% radial SC = no procs anywhere");

    let loud = run_once(&radial_only(radial_of(1.0, 0.0)), &mut Rng::new(7));
    assert_eq!(
        loud.procs, loud.shots,
        "100% radial SC = exactly one proc per landed explosion"
    );
    assert!(
        loud.tally.dot() > 0.0,
        "the explosion's Heat procs must burn on their own"
    );
    assert_eq!(quiet.shots, loud.shots, "status does not change the cadence");
}

/// AN EXPLOSION FORCES ITS OWN PROCS, at 0% status chance everywhere.
///
/// The Scourge pair is the roster's first: "Guaranteed Impact proc" is said
/// of the SPEAR EXPLOSION and of nothing else on that weapon, where the
/// Astilla says it of the direct hit and not of its radial. The engine read
/// the ATTACK's list and applied it to direct hits only, so this half was
/// unreachable — the fixture below has 0% status on both parts, so any proc
/// at all can only be the forced one.
#[test]
fn an_explosion_forces_its_own_procs_with_no_status_chance_anywhere() {
    let mut r = radial_of(0.0, 0.0);
    let quiet = run_once(&radial_only(r), &mut Rng::new(11));
    assert_eq!(quiet.procs, 0, "nothing forced and 0% SC = no procs at all");

    r.forced_procs = crate::rules::damage::ForcedProcs::from_types([DamageType::Impact]);
    let forced = run_once(&radial_only(r), &mut Rng::new(11));
    assert_eq!(
        forced.procs, forced.shots,
        "one forced Impact per landed explosion, however low the status chance"
    );
    assert_eq!(quiet.shots, forced.shots, "a forced proc does not move the cadence");
}

/// A VOLLEY SETTLES PELLET BY PELLET, AND EVERY INSTANCE RE-READS THE
/// TARGET. The four popped numbers and the arithmetic that forces this
/// ordering out of them are MEASUREMENTS M62; the assertion pins the
/// numbers rather than the rule, so any of the three regressing reddens it.
///
/// * **PELLET-MAJOR** — a pellet resolves its own explosion before the next
///   pellet's collision.
/// * **AN INSTANCE DOES NOT AMPLIFY ITSELF** — the first collision reads
///   x1.00, because its own forced proc lands after it is settled.
/// * **EVERY INSTANCE RE-READS THE TARGET**, not every shot and not even
///   every pellet. A mitigation snapshot hoisted out of the instance loop
///   for speed is a few per cent in the "this build is good" direction, and
///   invisible in every mean this engine reports.
#[test]
fn a_volley_settles_pellet_by_pellet_and_each_instance_re_reads_the_target() {
    let mut blast = DamageVector::default();
    blast.set(DamageType::Impact, 600.0);
    let p = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 200.0),
        // FORCED ON BOTH HALVES, which is what makes the fourth stack the
        // fourth instance's and the ladder one step per row.
        forced_procs: vec![DamageType::Viral],
        radial: Some(crate::build::loadout::ResolvedRadial {
            forced_procs: crate::rules::damage::ForcedProcs::from_types([DamageType::Viral]),
            damage: blast,
            modified_base: 600.0,
            // AT THE EPICENTRE, so the explosion's number is its own and
            // not a falloff curve's.
            falloff_reduction: 0.0,
            ..radial_of(0.0, 0.0)
        }),
        // EXACTLY TWO PELLETS, so the volley is four instances every time.
        multishot: 2.0,
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        status_chance: 0.0,
        base_status_chance: 0.0,
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        // NOTHING IN THE WAY — no shield, no armour, no overguard, no
        // vulnerability column — so each number is its base times Viral and
        // the arithmetic above is the whole of it.
        foe: frail_target(TargetMode::InfiniteHealth, 0.0, 0.0),
        // ONE TRIGGER PULL inside the window: a second volley would start
        // from the stacks the first one left.
        fire_rate: 1.0,
        duration_seconds: 0.5,
        magazine_size: 100.0,
        ..no_status()
    };

    let rec = record(&p, 0, 0.0, f64::INFINITY, 1_000, 0);
    let rows: Vec<(Option<u32>, bool, f64)> = rec
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            crate::record::Kind::Damage(d) => Some((d.pellet, d.radial, d.effective)),
            _ => None,
        })
        .collect();
    assert_eq!(rows.len(), 4, "one volley is four numbers: {rows:?}");

    let want = [
        (Some(1), false, 200.0),
        (Some(1), true, 1200.0),
        (Some(2), false, 450.0),
        (Some(2), true, 1500.0),
    ];
    for (i, (pellet, radial, damage)) in want.iter().enumerate() {
        let (gp, gr, gd) = rows[i];
        assert_eq!(
            (gp, gr),
            (*pellet, *radial),
            "row {} is pellet {pellet:?} {} — a volley settles pellet by \
                 pellet, got {rows:?}",
            i + 1,
            if *radial { "radial" } else { "direct" }
        );
        assert!(
            (gd - damage).abs() < 0.5,
            "row {} (pellet {pellet:?} {}) is {damage}, got {gd} — the \
                 whole volley: {rows:?}",
            i + 1,
            if *radial { "radial" } else { "direct" }
        );
    }
}

/// …AND IT IS NOT THE DIRECT HIT'S. The same weapon says one and not the
/// other, so a shared list would be wrong for whichever it was not written
/// for.
#[test]
fn a_radials_forced_proc_does_not_reach_the_direct_hit() {
    let mut p = radial_only(radial_of(0.0, 0.0));
    p.radial.as_mut().unwrap().forced_procs =
        crate::rules::damage::ForcedProcs::from_types([DamageType::Impact]);
    // The direct part has 0% status and forces nothing, so a proc from IT
    // would show as more procs than there are explosions.
    let r = run_once(&p, &mut Rng::new(11));
    assert_eq!(
        r.procs, r.shots,
        "exactly one per explosion — the direct hit contributed none"
    );
}

#[test]
fn the_explosion_rolls_its_own_crit_and_never_counts_as_a_pellet_crit() {
    // Separate instance = separate crit roll. `crits`/`headshots` stay
    // direct-pellet counters (an explosion never headshots), but the
    // damage must show the radial's own multiplier.
    let flat = run_once(&radial_only(radial_of(0.0, 0.0)), &mut Rng::new(3));
    let crit = run_once(&radial_only(radial_of(0.0, 1.0)), &mut Rng::new(3));
    assert_eq!(
        crit.crits, flat.crits,
        "the radial's crit chance must not move the pellet crit counter"
    );
    assert_eq!(
        crit.headshots, flat.headshots,
        "an explosion never headshots, so it adds nothing to that counter"
    );
    assert!(
        (crit.sources.radial - 2.0 * flat.sources.radial).abs() < 1e-6,
        "100% radial crit at 2x = double the explosion damage ({} vs {})",
        crit.sources.radial,
        flat.sources.radial
    );
    assert_eq!(flat.sources.direct, 0.0, "the fixture's direct hit is inert");
}

/// A RELATIVE crit buff joins the crit BUCKET, so it reaches the explosion
/// — and scales the EXPLOSION's own base, not the direct hit's. Both halves
/// matter: under `AssumedMax` these bonuses arrive inside `r.crit_damage`
/// through the mod bucket, so an Emergent path that skipped them made one
/// mod behave differently under two policies.
///
/// Pinned stacks make it arithmetic, and the direct part is given a very
/// different base (10x) on purpose: had the buff been resolved against the
/// direct base — the bug — the explosion would have landed at cd 12, not 4.
/// Laetum Incarnon uses the same 22%/2.2x for both parts, so only a fixture
/// with deliberately different bases can catch that half.
#[test]
fn a_relative_crit_buff_reaches_the_explosion_against_its_own_base() {
    use crate::data::arcanes::ArcBuffSpec;
    use crate::model::ArcTrigger;
    use crate::model::ArcGrant;
    let radial = |crit_damage: f64| {
        let mut damage = DamageVector::default();
        damage.set(DamageType::Heat, 300.0);
        crate::build::loadout::ResolvedRadial {
            blast_kind: crate::model::BlastKind::Contact,
            damage,
            modified_base: 300.0,
            crit_chance: 1.0, // always crits: no crit-roll noise
            crit_damage,
            base_crit_chance: 1.0,
            base_crit_damage: 2.0,
            status_chance: 0.0,
            base_status_chance: 0.0,
            radius_m: 2.0,
            falloff_start_m: 0.0,
            falloff_reduction: 0.0,
            forced_procs: Default::default(),
            takes_condition_overload: false,
            takes_multishot: true,
            co_base: crate::model::CoBase::whole_for(crate::model::CoStage::Radial), // the default: an explosion gets no CO
        }
    };
    // +50% x 2 pinned stacks = +100% of the part's base crit damage.
    let buff = ArcaneFx {
        buffs: vec![ArcBuffSpec {
            owner: "test".into(),
            grant: ArcGrant::CritDamage,
            trigger: ArcTrigger::ToxinStatus,
            per_stack: 0.5,
            max_stacks: 2,
            duration: crate::model::NO_TIMEOUT,
            all_drop: true,
            one_per_instance: false,
            initial_stacks: 2,
        }],
        ..ArcaneFx::none()
    };
    let live = run_once(
        &FightParams {
            unmodded_crit_damage: 10.0, // NOT the base the radial must use
            arcane: buff,
            ..radial_only(radial(2.0))
        },
        &mut Rng::new(4),
    );
    // What the same bonus looks like folded into the radial's resolved crit
    // damage — i.e. what AssumedMax produces through the bucket.
    let folded = run_once(&radial_only(radial(4.0)), &mut Rng::new(4));
    assert!(
        (live.sources.radial - folded.sources.radial).abs() < 1e-6,
        "explosion {} vs bucket-equivalent {}",
        live.sources.radial,
        folded.sources.radial
    );
    // And it really is a change: cd 2 -> 4 doubles a guaranteed crit.
    let bare = run_once(&radial_only(radial(2.0)), &mut Rng::new(4));
    assert!(
        (live.sources.radial - 2.0 * bare.sources.radial).abs() < 1e-6,
        "explosion {} vs unbuffed {}",
        live.sources.radial,
        bare.sources.radial
    );
}

/// M11 (in-game): on a LONE enemy one Laetum shot grants
/// TWO stacks of Overwhelming Attrition — the direct hit and the
/// explosion each arm it. The clean way to assert that here is a
/// ZERO-damage radial: it still runs as a stage, so any extra damage
/// can only come from the buff having been armed a second time.
#[test]
fn the_explosion_arms_an_on_hit_buff_of_its_own() {
    let mk = |with_radial: bool| {
        let radial = with_radial.then(|| crate::build::loadout::ResolvedRadial {
            blast_kind: crate::model::BlastKind::Contact,
            damage: DamageVector::default(), // 0 damage: a pure extra INSTANCE
            modified_base: 0.0,
            crit_chance: 0.0,
            crit_damage: 2.0,
            base_crit_chance: 0.0,
            base_crit_damage: 2.0,
            status_chance: 0.0,
            base_status_chance: 0.0,
            radius_m: 2.0,
            falloff_start_m: 0.0,
            falloff_reduction: 0.0,
            forced_procs: Default::default(),
            takes_condition_overload: false,
            takes_multishot: true,
            co_base: crate::model::CoBase::whole_for(crate::model::CoStage::Radial), // the default: an explosion gets no CO
        });
        let p = FightParams {
            radial,
            stacking_buffs: vec![crate::model::StackingBuff {
            id: "on_plain_hit_damage",
            trigger: crate::model::BuffTrigger::PlainHit,
            grant: crate::model::BuffGrant::BaseDamage,
            chance: 1.0,
            decay: crate::model::BuffDecay::LoseOneAndReset,
                per_stack: 4.0,
                max_stacks: 3,
                duration: 10.0,
                // Earn them in the run — that is what is under test.
                initial_stacks: 0,
                stacks_per_trigger: 1,
                per_shell: false,
                cleared_by: crate::model::ClearedBy::Nothing,
                card_opens_full: false,
            }],
            // Never crits, never procs: every instance is "plain", so
            // the only variable is HOW MANY instances a shot produces.
            base_crit_chance: 0.0,
            status_chance: 0.0,
            forced_procs: Vec::new(),
            // ONE body part at 1x, so no aim variance rides on top of the
            // effect being measured.
            body_parts: mono_body(1.0),
            ..FightParams::default()
        };
        // AVERAGED, not one engagement. Adding the radial adds a real
        // extra instance that makes its own crit decision, so the two
        // builds do not share a sample path and one run of each is a coin
        // flip rather than evidence. Over 200 engagements the mechanism is
        // plain: 11867 -> 12842.
        let s = monte_carlo(&p, 200, 4);
        (s.source_damage.direct, s.source_damage.radial)
    };
    let (solo, no_blast) = mk(false);
    let (paired, blast) = mk(true);
    assert_eq!(no_blast, 0.0, "control has no radial at all");
    assert_eq!(blast, 0.0, "the radial deals zero damage by construction");
    assert!(
        paired > solo,
        "the zero-damage explosion still arms the buff, so the DIRECT              damage must climb faster: {solo:.0} -> {paired:.0}"
    );
}

/// EVERY buff the roster offers must be READ by `apply_buff_config`.
///
/// A card whose setting reaches nothing is the failure mode this whole
/// area keeps producing: `buff_roster` (what exists), `enumerate_buffs`
/// (what is drawn) and `apply_buff_config` (what is obeyed) are three
/// lists, and Deadly Efficiency was in the first two and missing from the
/// third — so its card was drawn, set, and dropped, for as long as it has
/// existed. Nothing about the UI could reveal that: a knob that does
/// nothing looks exactly like a knob whose buff is not up.
///
/// The check is generic on purpose. It does not name the fields a buff
/// writes into — it sets one id at a time and asserts the params CHANGED,
/// so a buff added later is covered without anyone remembering to come
/// back here.
#[test]
fn every_buff_the_roster_offers_is_actually_read() {
    let params = every_buff_params();

    // Applied OUTSIDE this function, deliberately — the weapon passive is
    // a `locked_buffs` entry built by the api (`frenzy_apply`), not a
    // field of these params. It is exempt from the check, not from
    // being read.
    const ELSEWHERE: [&str; 1] = ["frenzy"];

    for b in params.buff_roster() {
        let id = b.id;
        if ELSEWHERE.contains(&id.as_str()) {
            continue;
        }
        let mut configured = params.clone();
        let mut cfg = BuffConfig::new();
        cfg.insert(id.clone(), (1, true));
        configured.apply_buff_config(&cfg);
        assert_ne!(
            format!("{configured:?}"),
            format!("{params:?}"),
            "the card for '{id}' is drawn but nothing reads it"
        );
    }
}

/// …AND EVERY BUFF THE ROSTER OFFERS MUST BE DENIABLE — the mirror of the
/// check above, naming no field. `deny_buff_triggers` is a WALK over the
/// shapes, so deny EVERY event and only the buffs that ask for nothing may
/// come back; anything else is a shape nobody remembered.
#[test]
fn every_buff_the_roster_offers_can_be_denied() {
    use crate::data::buff_events::{of_builtin, ALL};
    let params = every_buff_params();
    let before: Vec<String> = params.buff_roster().into_iter().map(|b| b.id).collect();
    assert!(before.len() > 10, "the fixture stopped covering the roster: {before:?}");

    // WHAT MAY SURVIVE, from the tables the denial reads — a list here
    // would be a second opinion about which buffs are passive.
    let passive = |id: &str| -> bool {
        if let Some(t) = of_builtin(id) {
            return t.is_none();
        }
        // A CARD BUILT FROM A `StackSpec` names its trigger on the spec.
        if let Some(s) = match id {
            "condition_overload" => params.co_stack,
            "on_kill_multishot" => params.multishot_stack,
            "on_headshot_kill_cc" => params.crit_chance_stack,
            _ => None,
        } {
            return s.earned_on.is_none();
        }
        params
            .stacking_buffs
            .iter()
            .find(|b| b.id == id)
            .map(|_| false)
            .or_else(|| {
                params.arcane.buffs.iter().find(|b| format!("arcane:{}", b.owner) == id)
                    .map(|b| b.trigger.id().is_none())
            })
            .unwrap_or_else(|| panic!("`{id}` is rostered and names no trigger"))
    };
    let want: Vec<String> = before.iter().filter(|id| passive(id)).cloned().collect();

    let mut denied = params.clone();
    let every: Vec<String> = ALL.iter().map(|(id, _)| (*id).to_string()).collect();
    denied.deny_buff_triggers(&every);
    // The sweep above is over the LIST; a trigger the enums spell and the
    // list forgot is `the_switch_list_and_the_triggers_agree`, not here.
    let after: Vec<String> = denied.buff_roster().into_iter().map(|b| b.id).collect();
    assert_eq!(
        after, want,
        "a fight where nothing triggers still grants these — deny_buff_triggers has no arm for them"
    );
}

/// THE GALVANIZED FAMILY IS EARNED ON A KILL, Condition Overload payload
/// included — the half of that card a fight denying kills has to take away.
///
/// Its id is shared with MELEE's Condition Overload, which is unconditional
/// and which no switch may reach, so the two are here together: one id, two
/// mechanics, and the answer comes off the spec rather than off the id.
#[test]
fn a_kill_denied_takes_the_galvanized_co_and_leaves_melees() {
    let spec = |earned_on| crate::model::StackSpec {
        per_stack: 0.4,
        max_stacks: 3,
        duration: 14.0,
        initial_stacks: 0,
        earned_on,
    };
    let mut galvanized = FightParams { co_stack: Some(spec(Some("kill"))), ..Default::default() };
    galvanized.deny_buff_triggers(&["kill".to_string()]);
    assert_eq!(galvanized.co_stack, None, "Galvanized Strike's payload needs a kill");

    let mut melee = FightParams { co_stack: Some(spec(None)), ..Default::default() };
    melee.deny_buff_triggers(&["kill".to_string(), "hit_enemy_with_status".to_string()]);
    assert!(melee.co_stack.is_some(), "melee's Condition Overload is earned by nothing");
}

/// A KILL BUYS MORE THAN BUFFS, and none of these has a card to be greyed
/// — Sentient Surge's magazine, Exact Penance's free reload and Jahu
/// Canticle's armour strip reached the fight for free under a ruler that
/// hands out no kills.
#[test]
fn a_kill_denied_takes_the_refill_the_reload_and_the_strip() {
    let mut p = FightParams {
        magazine_refill_on_kill: 0.2,
        instant_reload_on_kill: Some(0.5),
        strip_on_kill_in_range: Some((0.5, 20.0)),
        ..Default::default()
    };
    p.deny_buff_triggers(&["kill".to_string()]);
    assert_eq!(p.magazine_refill_on_kill, 0.0);
    assert_eq!(p.instant_reload_on_kill, None);
    assert_eq!(p.strip_on_kill_in_range, None);
}

#[test]
fn overwhelming_attrition_takes_the_buff_cards_two_knobs() {
    // LOCKED = NO TIMEOUT, not frozen. A locked buff whose stacks decay
    // from the seed and whose trigger is skipped reads "no timeout" as
    // "decays to zero and can never come back" — the exact opposite of the
    // label, and indistinguishable from the buff not working at all.
    let mk = |initial: u32, locked: bool, fire_rate: f64, secs: f64| {
        let mut p = FightParams {
            stacking_buffs: vec![crate::model::StackingBuff {
            id: "on_plain_hit_damage",
            trigger: crate::model::BuffTrigger::PlainHit,
            grant: crate::model::BuffGrant::BaseDamage,
            chance: 1.0,
            decay: crate::model::BuffDecay::LoseOneAndReset,
                per_stack: 4.0,
                max_stacks: 3,
                // Locking IS this: the card's duration, overwritten.
                duration: if locked { crate::model::NO_TIMEOUT } else { 10.0 },
                initial_stacks: initial,
                stacks_per_trigger: 1,
                per_shell: false,
                cleared_by: crate::model::ClearedBy::Nothing,
                card_opens_full: false,
            }],
            fire_rate,
            duration_seconds: secs,
            ..FightParams::default()
        };
        // No crits and no procs, so EVERY hit is a plain hit and the buff
        // arms on all of them.
        p.base_crit_chance = 0.0;
        p.status_chance = 0.0;
        p.forced_procs = Vec::new();
        run_once(&p, &mut Rng::new(11)).total_damage()
    };
    let without = |fire_rate: f64, secs: f64| {
        run_once(
            &FightParams {
                base_crit_chance: 0.0,
                status_chance: 0.0,
                forced_procs: Vec::new(),
                fire_rate,
                duration_seconds: secs,
                ..FightParams::default()
            },
            &mut Rng::new(11),
        )
        .total_damage()
    };

    // ---- 1 shot/s, 10 s buff: nothing ever expires ---------------------
    // The clock is what locking removes, so where no stack would have
    // expired anyway, locking must change NOTHING.
    let (earned, locked_none) = (mk(0, false, 1.0, 10.0), mk(0, true, 1.0, 10.0));
    assert!(
        (locked_none - earned).abs() < 1e-9,
        "locking a buff nothing was expiring changed it: {locked_none} vs {earned}"
    );
    // …and above all, it is not a way to switch the buff off.
    assert!(
        locked_none > without(1.0, 10.0),
        "a buff locked at 0 stacks still EARNS: {locked_none} vs {}",
        without(1.0, 10.0)
    );
    // Seeding it full only helps.
    assert!(mk(3, true, 1.0, 10.0) >= locked_none);

    // ---- one shot every 20 s, 10 s buff: the timeout bites -------------
    // Unlocked, every stack has expired before the next shot lands (and
    // the hit that grants a stack does not benefit from it), so the buff
    // is worth nothing. Locked, it climbs and HOLDS — which is the whole
    // of what the setting promises.
    let (slow_open, slow_locked) = (mk(0, false, 0.05, 100.0), mk(0, true, 0.05, 100.0));
    assert!(
        (slow_open - without(0.05, 100.0)).abs() < 1e-9,
        "a 10 s buff cannot survive 20 s between shots: {slow_open}"
    );
    assert!(
        slow_locked > slow_open,
        "NO TIMEOUT must hold the stacks across the gap: {slow_locked} vs {slow_open}"
    );
}

#[test]
#[should_panic(expected = "cannot be an Eximus")]
fn impossible_eximus_combination_panics_at_spawn() {
    let mut t = frail_target(TargetMode::InstantRespawn, 0.0, 0.0);
    t.eximus = true; // can_be_eximus is false -> impossible in-game
    let p = FightParams {
        foe: t,
        ..FightParams::default()
    };
    let _ = run_once(&p, &mut Rng::new(1));
}

#[test]
fn eximus_boosts_health_and_grants_overguard() {
    let mut t = frail_target(TargetMode::InfiniteHealth, 0.0, 0.0);
    t.can_be_eximus = true;
    t.eximus = true;
    t.level = 200;
    // Unarmored/unshielded: base health max(50*1.1, 0.375*(50+900)*6).
    let base = (50.0f64 * 1.1).max(0.375 * 950.0 * 6.0);
    let expect = base * t.health_curve.multiplier(199.0);
    assert!((t.max_health() - expect).abs() < 1e-6);
    // Eximus overguard: base 12, scaled.
    assert!(t.overguard() > 0.0);
}

#[test]
fn instant_respawn_kills_every_shot_on_a_frail_target() {
    let p = FightParams {
        foe: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
        ..FightParams::default()
    };
    let s = monte_carlo(&p, 200, 5);
    // 50 HP, no armor, no overguard: every shot (>= 75 raw) kills, and the
    // target respawns in place — 10 kills per 10-shot run, no variance.
    // Killing hits' procs are discarded, so no bleeds ever tick.
    assert!((s.mean_kills - 10.0).abs() < 1e-9, "kills {}", s.mean_kills);
    assert_eq!(s.std_kills, 0.0);
    assert_eq!((s.min_kills, s.max_kills), (10, 10));
    // The final target respawned untouched: no partial credit.
    assert!((s.mean_kill_progress - 10.0).abs() < 1e-9);
    assert_eq!(s.mean_dot_damage, 0.0);
    assert_eq!(s.mean_procs, 0.0);
}

#[test]
fn infinite_health_never_dies_and_applies_armor_dr() {
    // 300 armor (>= the 200 spawn minimum, stays 300) -> post-U36 DR
    // = 0.9 * sqrt(300/2700) = 30%: effective is exactly 70% of raw,
    // and no kills. Status off so no armor-ignoring Cinematic ticks mix in.
    let p = FightParams {
        foe: frail_target(TargetMode::InfiniteHealth, 300.0, 0.0),
        ..no_status()
    };
    let s = monte_carlo(&p, 200, 5);
    assert_eq!(s.mean_kills, 0.0);
    assert!(
        (s.mean_effective_damage - s.mean_damage * 0.7).abs() < 1e-9,
        "effective {} vs raw {}",
        s.mean_effective_damage,
        s.mean_damage
    );
}

#[test]
fn bleed_ticks_ignore_armor_entirely() {
    // Forced Slash, capped armor (90% DR): direct hits take 0.1x but
    // ticks land at full value. crit off, mono 1x, 10 s:
    // direct effective = 750 × 0.1 = 75; dot effective = 1037.4 (full).
    let p = FightParams {
        crit_multiplier: 1.0,
        forced_procs: vec![DamageType::Slash],
        body_parts: mono_body(1.0),
        foe: frail_target(TargetMode::InfiniteHealth, 2700.0, 0.0),
        ..no_status()
    };
    let s = monte_carlo(&p, 50, 5);
    assert!(
        (s.mean_dot_damage - 1037.4).abs() < 1e-6,
        "dot {}",
        s.mean_dot_damage
    );
    assert!(
        (s.mean_effective_damage - (75.0 + 1037.4)).abs() < 1e-6,
        "effective {}",
        s.mean_effective_damage
    );
}

#[test]
fn armor_reduced_damage_floors_at_one_per_type() {
    // Tiny hits vs capped armor: 5 raw x (1 - 0.9) = 0.5 -> floored to
    // 1 (scalar hit = one damage type). crit_multiplier 1.0 keeps every
    // shot's raw at exactly base x part multiplier; pure-Impact vector so
    // the only possible proc is a harmless Stagger.
    let p = FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 5.0),
        crit_multiplier: 1.0,
        body_parts: mono_body(1.0),
        foe: frail_target(TargetMode::InfiniteHealth, 2700.0, 0.0),
        ..FightParams::default()
    };
    let s = monte_carlo(&p, 100, 3);
    // 10 shots per run, each floored to exactly 1 effective damage.
    assert!(
        (s.mean_effective_damage - 10.0).abs() < 1e-9,
        "effective {}",
        s.mean_effective_damage
    );
}

/// ENEMY OVERGUARD IS A BAR IN FRONT OF THE HEALTH BAR, not a wall in
/// front of it. See [`TargetState::apply`] for the wiki's own wording; the
/// consequence a player states is that a unit holding Overguard can be
/// killed by ONE shot, and this is that sentence as arithmetic.
#[test]
fn a_hit_that_breaks_overguard_carries_the_rest_into_health() {
    // 100 of Overguard in front of 50 of health, and no armour: a 1000
    // hit spends 100 and the other 900 has to land.
    let target = frail_target(TargetMode::InstantRespawn, 0.0, 100.0);
    let mut st = TargetState::spawn(&target, crate::rules::space::Vec2::ORIGIN);
    let og = st.overguard;
    let hp = st.health;
    let settled = st.apply(
        1000.0,
        TypeShares::single(DamageType::Impact),
        false,
        0.0,
        &target,
        true,
        &Mitigation { disrupt_amp: 1.0, virus_amp: 1.0, virus_stacks: 0, armor_multiplier: 1.0 },
        1.0,
        None,
    );
    assert!(settled.killed, "an Overguard holder can be killed by one shot");
    assert_eq!(settled.spilled, 0.0, "nothing is thrown away on the way past");
    assert!(
        (settled.effective - 1000.0).abs() < 1e-9,
        "the whole instance landed: {}",
        settled.effective
    );
    // …AND IT ARRIVED WHERE IT WAS AIMED. The excess is health damage, so
    // the overkill is everything past the two bars — a check the totals
    // alone would pass with the 900 charged back to Overguard.
    assert!(
        (settled.overkill - (1000.0 - og - hp)).abs() < 1e-9,
        "overkill is what was left after both bars: {} against {}",
        settled.overkill,
        1000.0 - og - hp
    );
}

#[test]
fn overguard_ignores_armor_while_it_holds() {
    // Huge armor but active overguard: hits are neutral (effective == raw)
    // until overguard breaks; the pool is large enough to absorb all runs.
    // (Bleed ticks land on overguard at full value too.)
    let mut t = frail_target(TargetMode::InfiniteHealth, 2700.0, 1e12);
    t.base_health = 1.0;
    let p = FightParams {
        foe: t,
        ..FightParams::default()
    };
    let s = monte_carlo(&p, 100, 9);
    assert_eq!(s.mean_kills, 0.0);
    assert!(
        (s.mean_effective_damage - s.mean_damage).abs() < 1e-9,
        "overguard hits must be unmitigated"
    );
}

#[test]
fn default_training_dummy_passes_damage_through() {
    let s = monte_carlo(&FightParams::default(), 200, 21);
    assert_eq!(s.mean_kills, 0.0);
    assert!((s.mean_effective_damage - s.mean_damage).abs() < 1e-9);
}

#[test]
fn incarnon_procs_at_the_listed_rate_per_pellet() {
    // Incarnon profile vs the plain dummy: 43% SC per PELLET (multishot
    // 2.0 doubles opportunities, not the per-pellet chance).
    let s = monte_carlo(&FightParams::dual_toxocyst_incarnon(), 500, 11);
    let per_pellet = s.mean_procs / s.mean_pellets;
    assert!(
        (per_pellet - 0.43).abs() < 0.02,
        "procs/pellet {per_pellet}"
    );
    // Bleeds are flowing and feeding damage.
    assert!(s.mean_dot_damage > 0.0);
}
