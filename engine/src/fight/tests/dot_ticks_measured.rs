use super::*;

#[test]
fn longer_status_duration_slows_the_heat_strip_ramp() {
    // ignite.yaml: the ramp steps scale WITH status duration —
    // +100% duration means 1.0 s steps, full strip only at 4 s
    // (counter-intuitive: longer duration = SLOWER strip).
    let d = DebuffState {
        heat: Some(HeatEntity {
            bracket: 1.0,
            depth: 0,
            unit: 0.0,
            born: 0.0,
            expiry: 12.0,
            next_tick: 1.0,
            value: 1.0,
            recent: Vec::new(),
            stacks: 1,
        }),
        ..Default::default()
    };
    // status_damage = 2.0: steps at 1.0 s intervals.
    assert_eq!(d.heat_strip(0.9, 2.0), 0.0);
    assert_eq!(d.heat_strip(1.1, 2.0), 0.15);
    assert_eq!(d.heat_strip(3.9, 2.0), 0.40);
    assert_eq!(d.heat_strip(4.1, 2.0), 0.50);
    // status_damage = 0.5: full strip already at 1.0 s.
    assert_eq!(d.heat_strip(1.1, 0.5), 0.50);
}

/// M99: a Braton Prime at +165% base and +200% Heat, status duration
/// -87.5%, on a Steel Path level 210 Corrupted Heavy Gunner. Armour is at
/// the 2700 cap, so the five strip steps read 29/50/73/89/107.
#[test]
fn m99_heat_strip_climbs_without_a_tick_on_capped_armour() {
    let unit = crate::data::enemies::all()
        .into_iter()
        .find(|e| e.id == "corrupted_heavy_gunner")
        .expect("the roster has one");
    let target = unit
        .target_params(210, true, false, TargetMode::InfiniteHealth)
        .expect("a level 210 Steel Path unit is legal");
    let mb = 35.0 * 2.65;
    let p = FightParams {
        foe: target,
        damage: DamageVector::new()
            .with(DamageType::Impact, 1.75 * 2.65)
            .with(DamageType::Puncture, 12.25 * 2.65)
            .with(DamageType::Slash, 21.0 * 2.65)
            .with(DamageType::Heat, 2.0 * mb),
        dot_modified_base: Some(mb),
        status_duration_multiplier: 0.125,
        // Shots at 0, 0.1, 0.2, 0.3 s: the burn lasts 0.75 s and never
        // reaches its +1 s tick, and the steps are 0.0625 s apart.
        fire_rate: 10.0,
        duration_seconds: 0.35,
        ..bare(DamageType::Heat)
    };
    let s = monte_carlo(&p, 1, 1);
    // The quantized hit: Puncture 31.88 x1.5 (Orokin), Impact 5.80,
    // Slash 55.07, Heat 185.5 — 294.22 before armour.
    let raw = 31.882_812_5 * 1.5 + 5.796_875 + 55.070_312_5 + 185.5;
    let at = |strip: f64| raw * (1.0 - 0.9 * (1.0 - strip).sqrt());
    let expected = at(0.0) + at(0.15) + at(0.40) + at(0.50);
    assert!(
        (s.mean_effective_damage - expected).abs() < 1e-6,
        "eff {} vs {expected}",
        s.mean_effective_damage
    );
    assert!((at(0.50) - 107.0).abs() < 0.05, "the measured 107: {}", at(0.50));
    assert!((raw - 185.5) * 0.1 < 11.0 && (raw - 185.5) * 0.1 > 10.5, "the measured 11");
}

/// PAST -100% STATUS DURATION NO DOT LANDS: "all duration/DoT procs are
/// nullified" (MECHANICS §6). Tesla and Gas tick at once, so they are the
/// two a one-tick floor would leak.
#[test]
fn no_dot_ticks_once_status_duration_is_gone() {
    for dtype in [DamageType::Electricity, DamageType::Gas, DamageType::Slash, DamageType::Heat, DamageType::Toxin] {
        let ticks = |sd: f64| {
            let p = FightParams { status_duration_multiplier: sd, ..bare(dtype) };
            monte_carlo(&p, 4, 1).mean_dot_ticks
        };
        assert_eq!(ticks(0.0), 0.0, "{dtype:?} at -100%");
        assert_eq!(ticks(-0.5), 0.0, "{dtype:?} at -150%");
        assert!(ticks(1.0) > 0.0, "{dtype:?} at +0%");
    }
}

/// M99, held fire: the same build keeps its 0.75 s burn alive by refreshing
/// it, so the one entity ticks every second and its tick grows with the
/// procs folded in — the measured 1164 is 23 of them at the full strip.
#[test]
fn m99_a_refreshed_short_burn_ticks_every_second() {
    let unit = crate::data::enemies::all()
        .into_iter()
        .find(|e| e.id == "corrupted_heavy_gunner")
        .expect("the roster has one");
    let target = unit
        .target_params(210, true, false, TargetMode::InfiniteHealth)
        .expect("a level 210 Steel Path unit is legal");
    let mb = 35.0 * 2.65;
    let p = FightParams {
        foe: target,
        damage: DamageVector::new()
            .with(DamageType::Impact, 1.75 * 2.65)
            .with(DamageType::Puncture, 12.25 * 2.65)
            .with(DamageType::Slash, 21.0 * 2.65)
            .with(DamageType::Heat, 2.0 * mb),
        dot_modified_base: Some(mb),
        elem_dot_bonus: vec![(DamageType::Heat, 3.0)],
        status_duration_multiplier: 0.125,
        // Held fire and no reload inside the window: a proc every 0.1 s
        // against a 0.75 s burn.
        fire_rate: 10.0,
        magazine_size: 100.0,
        duration_seconds: 3.05,
        ..bare(DamageType::Heat)
    };
    let ignite = DEBUFF_ROSTER.iter().position(|(id, _)| *id == "ignite").expect("a row");
    let mut rec = crate::record::Record::window(0.0, 10.0, 100_000, 0);
    let _ = run_once_traced(&p, &mut crate::rules::rng::Rng::new(1), None, &mut rec);
    let ticks: Vec<(f64, f64)> = rec
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            crate::record::Kind::Damage(d)
                if d.origin == crate::record::Origin::Status && d.dtype == DamageType::Heat =>
            {
                // `debuffs` is (stacks, expiry) per roster row.
                Some((f64::from(d.debuffs[ignite].0), d.effective))
            }
            _ => None,
        })
        .collect();
    assert_eq!(ticks.len(), 3, "one tick a second: {ticks:?}");
    // 0.5 × 92.75 × (1 + 2) per proc, through the 50% strip on 2700.
    let per_proc = 0.5 * mb * 3.0 * (1.0 - 0.9 * 0.5_f64.sqrt());
    for (n, eff) in &ticks {
        assert!(*n >= 9.0, "the procs fold into one entity: {ticks:?}");
        assert!((eff / n - per_proc).abs() < 0.5, "{eff} over {n} vs {per_proc}");
    }
    assert!((per_proc * 23.0 - 1164.0).abs() < 1.0, "the measured 1164: {}", per_proc * 23.0);
}

/// M100: the Electricity tick takes the hit's whole head ladder (x5.4) in
/// its seed and, where the tick itself lands on a head, the headshot
/// brackets over a 1x base on top (x1.8) — 24 off a body, 234 off a head,
/// and 130 on a neighbour's body off a head. Gas lands the same way.
#[test]
fn m100_an_electricity_tick_lands_on_a_part_of_its_own() {
    let e = DamageType::Electricity;
    let body = m100_first_ticks(&m100_fixture(e, false, 0), 1, e);
    let head = m100_first_ticks(&m100_fixture(e, true, 0), 1, e);
    assert_eq!(body, vec![Some(24.0)], "the measured body tick");
    assert_eq!(head, vec![Some(234.0)], "the measured head tick");

    let crowd = m100_first_ticks(&m100_fixture(e, true, 8), 1, e);
    assert_eq!(crowd[0], Some(234.0));
    for (i, t) in crowd.iter().enumerate().skip(1) {
        assert!(
            matches!(t, Some(v) if *v == 130.0 || *v == 234.0),
            "neighbour {i}: the arc keeps the head seed, 130 or 234 where it lands on a head: {crowd:?}"
        );
    }

    for g in [false, true] {
        let t = m100_first_ticks(&m100_fixture(DamageType::Gas, g, 0), 1, DamageType::Gas);
        let v = t[0].expect("a gas tick");
        assert_eq!(v, if g { 234.0 } else { 24.0 }, "gas, head {g}");
    }
}

/// M100's other half: the SAME fixture on Heat, Toxin and Blast keeps the
/// hit's ladder and nothing more — 5.4x the body tick, not 9.7x.
#[test]
fn m100_heat_toxin_and_blast_take_the_hits_part_alone() {
    for (dtype, body_tick, head_tick) in [
        (DamageType::Toxin, 24.0, 130.0),
        (DamageType::Blast, 5.0, 26.0),
    ] {
        let body = m100_first_ticks(&m100_fixture(dtype, false, 0), 1, dtype)[0];
        let head = m100_first_ticks(&m100_fixture(dtype, true, 0), 1, dtype)[0];
        assert_eq!((body, head), (Some(body_tick), Some(head_tick)), "{dtype:?}");
    }
    assert!(!lands_on_a_part(DamageType::Heat));
}

/// M100: a neighbour's arc lands on its head 10 times in 189.
#[test]
fn m100_an_arc_lands_on_a_neighbours_head_at_the_measured_rate() {
    let e = DamageType::Electricity;
    let p = m100_fixture(e, true, 9);
    let (mut heads, mut all) = (0, 0);
    for seed in 0..120 {
        for t in m100_first_ticks(&p, seed, e).into_iter().skip(1).flatten() {
            all += 1;
            heads += usize::from(t == 234.0);
        }
    }
    let rate = heads as f64 / all as f64;
    assert_eq!(all, 120 * 9);
    assert!((rate - TESLA_HEAD_LANDING_CHANCE).abs() < 0.025, "{heads}/{all}");
}

#[test]
fn a_burn_nullified_by_negative_duration_strips_nothing() {
    // Status duration -110%: the proc's expiry is before its own instant.
    let sd = -0.1;
    let mut d = DebuffState::default();
    d.apply_heat(1.0, 3.0, 1.0 + 6.0 * sd, None, HeatOrigin { bracket: 1.0, depth: 0, unit: 0.0 });
    assert!(d.heat.is_none());
    for now in [1.5, 5.0, 60.0] {
        d.prune(now, sd);
        assert_eq!(d.heat_strip(now, sd), 0.0, "at {now}");
    }
}

#[test]
fn a_tick_on_a_strip_boundary_sees_the_step_taken() {
    // 0.0292 + 1.0 - 0.0292 is 0.9999… in f64, so a bare floor read the
    // +1 s tick one step short.
    let born = 0.0292;
    let d = DebuffState {
        heat: Some(HeatEntity {
            bracket: 1.0,
            depth: 0,
            unit: 0.0,
            born,
            expiry: born + 6.0,
            next_tick: born + 1.0,
            value: 1.0,
            recent: Vec::new(),
            stacks: 1,
        }),
        ..Default::default()
    };
    assert!((born + 1.0 - born) / 0.5 < 2.0, "the fixture must sit under the boundary");
    assert_eq!(d.heat_strip(born, 1.0), 0.0);
    assert_eq!(d.heat_strip(born + 1.0, 1.0), 0.30);
    assert_eq!(d.heat_strip(born + 1.0, 0.5), 0.50);
}

#[test]
fn heat_cap_keeps_the_most_recent_contributions_fifo() {
    // Uncapped: every contribution folds into the consolidated tick.
    let mut d = DebuffState::default();
    for c in [1.0, 2.0, 3.0, 4.0, 5.0] {
        d.apply_heat(0.0, c, 6.0, None, HeatOrigin { bracket: 1.0, depth: 0, unit: 0.0 });
    }
    assert_eq!(d.heat.as_ref().unwrap().value, 15.0);

    // Capped at 3 (a per-unit cap): hold the 3 MOST RECENT contributions,
    // FIFO dropping the oldest — {3,4,5}=12, NOT the first three (the old
    // ignore-new model would have frozen it at 1+2+3=6).
    let mut d = DebuffState::default();
    for c in [1.0, 2.0, 3.0, 4.0, 5.0] {
        d.apply_heat(0.0, c, 6.0, Some(3), HeatOrigin { bracket: 1.0, depth: 0, unit: 0.0 });
    }
    let h = d.heat.as_ref().unwrap();
    assert_eq!(h.recent, vec![3.0, 4.0, 5.0]);
    assert_eq!(h.value, 12.0);
}

/// GAS IS THE ONLY DoT FAMILY WITH A STACK CAP, and the other three say so
/// in as many words — see `dot_family_cap` for all four wiki sentences.
///
/// The engine had it wrong in BOTH directions at once until 2026-08-21: the
/// area path capped every family at ten, the direct path capped none, and a
/// Torid carried 34 live Gas instances where the game allows 10.
#[test]
fn only_gas_has_a_dot_stack_cap() {
    assert_eq!(dot_family_cap(DamageType::Gas), Some(TEN_STACK_CAP));
    for open in [DamageType::Slash, DamageType::Toxin, DamageType::Electricity] {
        assert_eq!(dot_family_cap(open), None, "{open:?} is uncapped on its own page");
    }
    // …and the ROSTER agrees, because the chart's stated cap is what a
    // reader compares a pile against: a row that says /10 and routinely
    // reaches 22 pins at the top and misinforms.
    let row = |id: &str| DEBUFF_ROSTER.iter().find(|(k, _)| *k == id).expect(id).1;
    assert_eq!(row("gas"), Some(TEN_STACK_CAP as u32));
    for open in ["bleed", "poison", "tesla", "ignite"] {
        assert_eq!(row(open), None, "{open} is drawn as uncapped");
    }
}

/// A BURN REPORTS HOW MANY PROCS ARE IN IT.
///
/// Heat is the one status DoT that CONSOLIDATES — every proc folds into a
/// single entity whose tick grows linearly — so there is no list to count
/// and the debuff row read `heat.is_some()`, which is 0 or 1. A player
/// watching a twenty-stack burn was told it was one stack, and the ramp is
/// most of what Heat does (a player report).
#[test]
fn a_heat_pile_reports_its_depth_and_not_just_that_it_is_burning() {
    let ignite = DEBUFF_ROSTER.iter().position(|(k, _)| *k == "ignite").expect("ignite");
    let mut d = DebuffState::default();
    for _ in 0..7 {
        d.apply_heat(0.0, 3.0, 6.0, None, HeatOrigin { bracket: 1.0, depth: 0, unit: 0.0 });
    }
    assert_eq!(d.sample(1.0)[ignite].0, 7, "seven procs, seven stacks");
    // The DAMAGE was always right — this is the number that was thrown away.
    assert!((d.heat.as_ref().expect("burning").value - 21.0).abs() < 1e-9);
    // UNDER A CAP it holds at the cap, because the oldest contribution is
    // dropped as the new one lands.
    let mut c = DebuffState::default();
    for _ in 0..7 {
        c.apply_heat(0.0, 3.0, 6.0, Some(3), HeatOrigin { bracket: 1.0, depth: 0, unit: 0.0 });
    }
    assert_eq!(c.sample(1.0)[ignite].0, 3);
    // …and an expired burn is no stacks at all rather than a stale count.
    c.prune(99.0, 1.0);
    assert_eq!(c.sample(99.0)[ignite].0, 0);
}

#[test]
fn independent_dots_cap_per_type_fifo() {
    let dot = |v: f64, ty| Dot {
        cause: u32::MAX,
        next_tick: 0.0,
        ticks_left: 6,
        frozen: v,
        landing: 1.0,
        bracket: 1.0,
        depth: 0,
        source_scaled: false,
        unit: 0.0,
        dtype: ty,
        ignores_armor: false,
    };
    let mut d = DebuffState::default();
    // Cap 2 per type: four Toxin procs keep only the two newest (3,4).
    for v in [1.0, 2.0, 3.0, 4.0] {
        d.push_dot_capped(dot(v, DamageType::Toxin), Some(2));
    }
    // A different DoT type is capped independently.
    d.push_dot_capped(dot(9.0, DamageType::Slash), Some(2));
    let tox: Vec<f64> = d
        .dots
        .iter()
        .filter(|x| x.dtype == DamageType::Toxin)
        .map(|x| x.frozen)
        .collect();
    let sla: Vec<f64> = d
        .dots
        .iter()
        .filter(|x| x.dtype == DamageType::Slash)
        .map(|x| x.frozen)
        .collect();
    assert_eq!(tox, vec![3.0, 4.0]);
    assert_eq!(sla, vec![9.0]);
}

#[test]
fn corrosion_strips_armor_multiplicatively() {
    // Capped armor (90% DR): forced Corrosive procs stack 20%+6%/stack
    // strip, so later shots take less DR than the first. Exact: shot k
    // has n = k−1 stacks; strip(0)=0, strip(n)=0.20+0.06n.
    let p = FightParams {
        foe: frail_target(TargetMode::InfiniteHealth, 2700.0, 0.0),
        ..bare(DamageType::Corrosive)
    };
    let s = monte_carlo(&p, 20, 5);
    // Stack counts before each shot (8 s expiry, 1 shot/s):
    // 0,1,2,...,7 then a steady 7.
    let expected: f64 = [0, 1, 2, 3, 4, 5, 6, 7, 7, 7]
        .iter()
        .map(|&n| {
            let strip = if n == 0 { 0.0 } else { 0.20 + 0.06 * n as f64 };
            let dr = 0.9 * ((2700.0 * (1.0 - strip)) / 2700.0_f64).sqrt();
            (75.0 * (1.0 - dr)).max(1.0)
        })
        .sum();
    assert!(
        (s.mean_effective_damage - expected).abs() < 1e-9,
        "eff {} vs {expected}",
        s.mean_effective_damage
    );
}

#[test]
fn enervate_raises_crit_rate_above_base() {
    // Base crit is 5%, but Enervate stacks flat crit as the fight goes on, so
    // the observed crit rate should exceed 5%.
    let s = monte_carlo(&FightParams::default(), 2000, 3);
    assert!(
        s.mean_crit_rate > 0.05,
        "crit rate was {}",
        s.mean_crit_rate
    );
}

/// A magazine reloads when the NEXT SHOT cannot be paid for, and the test
/// is `cost <= ceil(current)` — not `current > 0`.
///
/// Both cases are the owner's, and the rule hid for as long
/// as it did because it only bites above 1: for any cost <= 1 the two
/// tests agree on every positive magazine, since `ceil(x) >= 1` there.
/// Ammo efficiency put costs in the 0.x range and exposed nothing. The
/// Larkspur Prime's alt-fire costs TEN, and there they part company.
#[test]
fn a_reload_is_decided_by_what_the_next_shot_costs() {
    // 7 left, the shot costs 10: it does NOT fire. Under `current > 0` it
    // did, and landed on −3 — a debt one whole-round draw cannot clear.
    assert!(!can_fire(7.0, 10.0), "7 cannot pay for 10");
    assert!(can_fire(10.0, 10.0), "10 pays for 10 exactly");
    // 0.2 left, the shot costs 1: it DOES fire, because ceil(0.2) is 1.
    assert!(can_fire(0.2, 1.0), "the ceiling is what lets this through");
    assert!(!can_fire(0.0, 1.0), "empty is empty");
    // A beam tick at 0.5 clears the same ceiling with room to spare.
    assert!(can_fire(0.2, 0.5), "0.5 <= ceil(0.2)");
    // A FREE shot is not a special case, it is `cost == 0`: the ceiling
    // lets it through on an empty magazine, so no `free_shot` flag is
    // needed.
    assert!(can_fire(0.0, 0.0), "0 <= ceil(0)");
    assert!(can_fire(-0.8, 0.0), "and on an overdrawn one");

    // The overdraw the second case creates is bounded to (−1, 0], which is
    // what keeps `reload_draw`'s whole-round rule (M14) correct: a
    // magazine sitting at −0.8 comes back at capacity − 0.8, not capacity.
    let after = -0.8 + reload_draw(100.0, -0.8);
    assert!((after - 99.2).abs() < 1e-9, "capacity - 0.8: {after}");
}

/// The DEFAULT is what this is about. `finite_reserve_stops_the_gun`
/// already proves a finite pool ends the run; what matters for the data
/// path is that switching it on is the ONLY thing that does, and that the
/// size of the pool is then worth modding for.
///
/// Ammo PICKUPS are not modelled, so a weapon that can be resupplied
/// mid-fight must keep the infinite default or it would run dry for a
/// reason the game does not have. A ground Arch-Gun is the case that
/// cannot: "Archguns only have a limited amount of ammo", and when it is
/// gone the weapon is removed for a five-minute cooldown (wiki Arch-Gun).
#[test]
fn only_a_finite_reserve_ends_a_run_early_and_its_size_then_matters() {
    let params = |finite: bool, reserve: f64| FightParams {
        magazine_size: 10.0,
        fire_rate: 20.0,
        reload_seconds: 0.1,
        duration_seconds: 30.0,
        infinite_reserve: !finite,
        reserve_ammo: reserve,
        arcane: crate::data::arcanes::ArcaneFx::none(),
        ..Default::default()
    };
    let shots = |p: &FightParams| monte_carlo(p, 1, 5).mean_shots;

    // The same weapon, the same reserve, one flag apart: 600 rounds of
    // clock against 35 rounds of ammunition.
    let finite = shots(&params(true, 25.0));
    let infinite = shots(&params(false, 25.0));
    assert!(finite < 40.0 && infinite > 300.0, "{finite} vs {infinite}");

    // A bigger pool lasts longer, which is what makes Ammo Chain and a
    // riven's Ammo Maximum worth a slot on such a weapon at all.
    assert!(shots(&params(true, 90.0)) > finite);

    // And the reserve is not the magazine: with none, the magazine is all
    // there is.
    let one = shots(&params(true, 0.0));
    assert!((one - 10.0).abs() < 1e-9, "just the magazine: {one}");
}
