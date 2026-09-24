use super::*;

#[test]
fn thrax_9999_takes_everything_on_overguard_neutrally() {
    // THE ULTIMATE STRESS TEST benchmark: Thrax @9999 STEEL PATH
    // (9.67M health behind 15.5M overguard) - every instance (direct
    // pellets AND Cinematic bleed ticks) lands on the neutral Overguard
    // pool, so effective == raw exactly, and nothing ever dies.
    let spec = crate::data::enemies::EnemySpec::load(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/enemies/thrax_centurion.yaml"
    )))
    .unwrap();
    let p = FightParams {
        foe: spec
            .target_params(9999, true, false, TargetMode::InstantRespawn)
            .unwrap(),
        duration_seconds: 60.0,
        ..FightParams::dual_toxocyst_incarnon()
    };
    let s = monte_carlo(&p, 50, 12);
    assert_eq!(s.mean_kills, 0.0);
    assert!(
        (s.mean_effective_damage - s.mean_damage).abs() < 1e-6,
        "overguard must be neutral: eff {} raw {}",
        s.mean_effective_damage,
        s.mean_damage
    );
    assert!(
        s.mean_procs > 0.0,
        "procs must still apply behind overguard"
    );
}

#[test]
fn viral_amps_health_damage_live() {
    // Forced Viral on the infinite dummy at 1 shot/s: stacks expire
    // after 6 s, so the count before shot k is 0,1,2,3,4,5 then a
    // steady 5. Amps 1,2,2.25,2.5,2.75,3,3,3,3,3 -> Σ = 25.5.
    let s = monte_carlo(&bare(DamageType::Viral), 20, 3);
    assert!(
        (s.mean_effective_damage - 75.0 * 25.5).abs() < 1e-9,
        "eff {}",
        s.mean_effective_damage
    );
    // Raw is pre-mitigation: the amp is defender-side.
    assert!((s.mean_damage - 750.0).abs() < 1e-9);
}

#[test]
fn magnetic_amps_overguard_damage_live() {
    // Same amp curve, but on the overguard pool.
    let p = FightParams {
        foe: frail_target(TargetMode::InfiniteHealth, 0.0, 1e12),
        ..bare(DamageType::Magnetic)
    };
    let s = monte_carlo(&p, 20, 3);
    assert!(
        (s.mean_effective_damage - 75.0 * 25.5).abs() < 1e-9,
        "eff {}",
        s.mean_effective_damage
    );
}

#[test]
fn the_vulnerability_column_scales_each_component_on_its_own() {
    let impact = DamageVector::new().with(DamageType::Impact, 100.0);
    let neutral = eff_vs("unknown", impact);
    assert!(neutral > 0.0);

    // Grineer: Impact ×1.5 (and Corrosive, which this hit has none of).
    assert!(
        (eff_vs("grineer", impact) - neutral * 1.5).abs() < 1e-6,
        "grineer impact {}",
        eff_vs("grineer", impact)
    );
    // A RESISTANCE is the same mechanism pointing down.
    let rad = DamageVector::new().with(DamageType::Radiation, 100.0);
    assert!(
        (eff_vs("orokin", rad) - eff_vs("unknown", rad) * 0.5).abs() < 1e-6,
        "orokin radiation"
    );
    // Per COMPONENT, not per hit: half a vector at ×1.5 is ×1.25 overall.
    // This is the "dilution" the wiki means — compositional, not a bucket.
    let mixed = DamageVector::new()
        .with(DamageType::Impact, 50.0)
        .with(DamageType::Slash, 50.0);
    assert!(
        (eff_vs("grineer", mixed) - eff_vs("unknown", mixed) * 1.25).abs() < 1e-6,
        "mixed {} vs {}",
        eff_vs("grineer", mixed),
        eff_vs("unknown", mixed)
    );
    // A faction with nothing to say about this type changes nothing.
    assert!((eff_vs("stalker", impact) - neutral).abs() < 1e-9);
}

/// The damage-by-type breakdown is what a reader consults to decide what
/// to add next, so it splits by what each component actually DID.
#[test]
fn the_by_type_breakdown_follows_the_column_too() {
    let v = DamageVector::new()
        .with(DamageType::Impact, 50.0)
        .with(DamageType::Slash, 50.0);
    let grineer = crate::data::factions::column("grineer");
    let mut dst = [0.0f64; DamageType::ALL.len()];
    // 125 effective is what 50 Impact ×1.5 + 50 Slash ×1.0 comes to.
    add_by_type(&mut dst, &v, 125.0, &grineer);
    assert!((dst[DamageType::Impact as usize] - 75.0).abs() < 1e-9, "{dst:?}");
    assert!((dst[DamageType::Slash as usize] - 50.0).abs() < 1e-9, "{dst:?}");
    // Neutral: the plain proportional split it always was.
    let mut flat = [0.0f64; DamageType::ALL.len()];
    add_by_type(&mut flat, &v, 100.0, &crate::data::factions::Column::NEUTRAL);
    assert!((flat[DamageType::Impact as usize] - 50.0).abs() < 1e-9);
}

/// Overguard is a LAYER over the unit, with its own table — the unit's
/// column must not reach it, and Void must.
#[test]
fn overguard_reads_its_own_column_not_the_units() {
    let shot = |key: &str, t: DamageType| {
        let p = FightParams {
            damage: DamageVector::new().with(t, 100.0),
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            foe: column_dummy(key, 1e12),
            ..no_status()
        };
        monte_carlo(&p, 20, 3).mean_effective_damage
    };
    let base = shot("unknown", DamageType::Impact);
    // Grineer's Impact ×1.5 stops at the overguard layer.
    assert!((shot("grineer", DamageType::Impact) - base).abs() < 1e-9);
    // Void ×1.5 is the Overguard column's own entry, on any unit.
    assert!((shot("grineer", DamageType::Void) - base * 1.5).abs() < 1e-6);
    assert!((shot("unknown", DamageType::Void) - base * 1.5).abs() < 1e-6);
}

/// Bleed is stored under Slash — the proc that made it — but the damage is
/// CINEMATIC, which takes no faction modifier anywhere. An Infested target
/// (Slash ×1.5) must boost the HIT and not the bleed it leaves.
#[test]
fn bleed_takes_no_faction_modifier_though_it_is_filed_under_slash() {
    let run = |key: &str| {
        let p = FightParams {
            damage: DamageVector::new().with(DamageType::Slash, 100.0),
            foe: column_dummy(key, 0.0),
            ..bare(DamageType::Slash)
        };
        let s = monte_carlo(&p, 20, 3);
        (s.mean_effective_damage - s.mean_dot_damage, s.mean_dot_damage)
    };
    let (direct_n, dot_n) = run("unknown");
    let (direct_i, dot_i) = run("infested");
    assert!(dot_n > 0.0 && direct_n > 0.0, "the fixture must do both");
    // The direct Slash hit takes the column…
    assert!(
        (direct_i - direct_n * 1.5).abs() < 1e-6,
        "direct {direct_i} vs {direct_n}"
    );
    // …and the bleed it leaves does not.
    assert!((dot_i - dot_n).abs() < 1e-9, "dot {dot_i} vs {dot_n}");
}

/// The column and Toxin's shield bypass are two readings of ONE shape:
/// the Toxin part is scaled by Toxin's factor on its way past the shield,
/// the rest by theirs on the way into it.
#[test]
fn the_column_follows_each_component_into_the_pool_it_lands_in() {
    // Narmer: Toxin ×1.5, Magnetic ×0.5. Seven shots of 16 Toxin + 16
    // Magnetic into a shield that never breaks and 160 health:
    //   neutral  7 × 16       = 112  -> alive
    //   narmer   7 × 16 × 1.5 = 168  -> dead
    // The Magnetic half never reaches health under either column.
    let kills = |key: &str| {
        let p = FightParams {
            damage: DamageVector::new()
                .with(DamageType::Toxin, 16.0)
                .with(DamageType::Magnetic, 16.0),
            crit_multiplier: 1.0,
            base_crit_chance: 0.0,
            arcane: ArcaneFx::none(),
            body_parts: mono_body(1.0),
            fire_rate: 10.0,
            duration_seconds: 0.65,
            magazine_size: 100.0,
            foe: Foe {
                type_mods: crate::data::factions::columns_for(key),
                base_shield: 10_000.0,
                base_health: 160.0,
                ..frail_target(TargetMode::InstantRespawn, 0.0, 0.0)
            },
            ..no_status()
        };
        monte_carlo(&p, 20, 5).mean_kills
    };
    assert_eq!(kills("unknown"), 0.0, "112 damage must not kill 160 health");
    assert_eq!(kills("narmer"), 1.0, "Toxin x1.5 past the shield kills it");
}

/// **THE BRATON PRIME CAPTURE, ALL NINE TICKS**.
///
/// A base-35 rifle, three element brackets, and every reading came back a
/// flat 36/35 above what this engine computed — while every DIRECT hit on
/// the same builds landed on the digit. The accumulator is the whole of the
/// difference (`Dot::accumulator_unit`):
///
/// | bracket | `35 × 0.5 × b` | `(35 + 1) × 0.5 × b` | measured |
/// | --- | --- | --- | --- |
/// | 1.0 | 17.5 | **18** | 18 |
/// | 1.9 | 33.25 | **34.2** | 34 |
/// | 3.0 | 52.5 | **54** | 54 |
///
/// A BASE-35 RIFLE IS WHY IT WAS FINDABLE. The `1` is worth 0.5 damage
/// before multipliers, so it is 2.9% here and 0.25% on a base of 400 —
/// under the noise of every fixture this engine had.
///
/// The measured column is the game's pop-up, `floor(x + 0.5)`, which is why
/// 34.2 reads as 34; the assertion is on the unrounded tick.
#[test]
fn a_base_35_rifles_toxin_ticks_reproduce_the_capture() {
    let tick = |bracket: f64| {
        let p = FightParams {
            damage: DamageVector::new().with(DamageType::Toxin, 35.0),
            dot_modified_base: Some(35.0),
            elem_dot_bonus: vec![(DamageType::Toxin, bracket)],
            crit_multiplier: 1.0,
            forced_procs: vec![DamageType::Toxin],
            body_parts: mono_body(1.0),
            // ONE shot, and long enough for all six of its ticks — 0.2
            // would put a second shot at t=5 and a seventh tick at t=6.
            fire_rate: 0.1,
            duration_seconds: 6.5,
            ..no_status()
        };
        monte_carlo(&p, 20, 5).mean_dot_damage / 6.0
    };
    for (bracket, want) in [(1.0, 18.0), (1.9, 34.2), (3.0, 54.0)] {
        let got = tick(bracket);
        assert!((got - want).abs() < 1e-6, "bracket {bracket}: {got} vs {want}");
    }
}

#[test]
fn toxin_dot_matches_the_bleed_shape_at_half_base() {
    // Toxin mirrors the forced-bleed test at coefficient 0.5 vs 0.35, and
    // like Slash it ticks INDEPENDENTLY, so each stack is its own tick
    // group: 39 ticks × (75 + 1) × 0.5 = 39 × 38 = 1482 (no armor: full
    // value).
    let s = monte_carlo(&bare(DamageType::Toxin), 20, 5);
    assert!(
        (s.mean_dot_damage - 1482.0).abs() < 1e-6,
        "dot {}",
        s.mean_dot_damage
    );
}

/// M54: A BLAST AoE CARRIES THE WEAK POINT AND A TOXIN DoT DOES NOT.
///
/// Two rules that sound like one and go opposite ways, both measured in
/// game on the same afternoon. The blast half is the
/// sharp one because the numbers came back exact:
///
/// | 10 stacks applied by | a neighbour takes |
/// | --- | --- |
/// | body hits, no crits | 1050 |
/// | head hits, no crits | 3150 |
///
/// `3150 / 1050 = 3.000`, which is the head multiplier and nothing else.
/// This asserts the RATIO rather than either number, so it holds whatever
/// the build is.
#[test]
fn a_blast_aoe_carries_the_weak_point_and_a_toxin_dot_does_not() {
    // ---- the blast half: the AoE a body detonation posts, head vs body.
    //
    // Driven through `settle_procs`' own scale rather than a fixture, so it
    // is the production path that is being measured.
    let aoe = |part_factor: f64| {
        let mut d = DebuffState::default();
        for _ in 0..TEN_STACK_CAP {
            d.blast.push(BlastStack {
                fuse: 99.0,
                value: BLAST_COEFFICIENT * 1000.0 * part_factor,
                xh_bracket: 1.0,
            });
        }
        // `on_death` is the OTHER trigger the wiki names -- "10 stacks OR
        // the target dying" -- and it posts the same radial the tenth stack
        // would, which is the cheap way to read it here.
        d.on_death(Seat::WIELDER, None, &frail_target(TargetMode::InstantRespawn, 0.0, 0.0));
        d.area_hit.first().map(|h| h.damage).unwrap_or(0.0)
    };
    let body = aoe(1.0);
    let head = aoe(3.0);
    assert!(body > 0.0, "a full pile detonates");
    assert!(
        (head / body - 3.0).abs() < 1e-9,
        "M54: a head-applied pile reaches a neighbour at exactly x3 — \
             measured 3150 / 1050 = 3.000, got {head} / {body}"
    );
    // …AND THE 10x BETWEEN THE TWO HALVES. Ten stacks are 300% each to the
    // neighbours against 30% each to the host: `1050 / 10.5 = 100` over ten
    // stacks, i.e. ten times the host's own total.
    let host: f64 = TEN_STACK_CAP as f64 * BLAST_COEFFICIENT * 1000.0;
    assert!(
        (body / host - 10.0).abs() < 1e-9,
        "M54: the radial is 10x the single-target total — got {}",
        body / host
    );

    // ---- …AND THE DoTs, WHICH NOW ALL GO THE SAME WAY (M91). Toxin was
    // the one exception here, on a reading of M54's that a cleaner fixture
    // does not reproduce — Heat, Toxin, Electricity and Gas pop the same
    // four numbers on one weapon. Asserted as a family so that carving an
    // exception back out is a deliberate edit rather than a drift.
    for t in [
        DamageType::Slash,
        DamageType::Toxin,
        DamageType::Electricity,
        DamageType::Gas,
        DamageType::Heat,
    ] {
        assert!(dot_takes_weakpoint(t), "{t:?}: M91 measured all four alike");
    }
}

/// A CONSOLIDATED FAMILY PAYS ONCE, and the HEAP path pays the same as the
/// scan.
///
/// `process_ticks` asks for the next event by a linear SCAN under
/// `TICK_QUEUE_MIN` live DoTs and a heap over it. The merge advances EVERY
/// live instance of a family at once, leaving heap keys for ticks already
/// paid; without the guard that skips them a stale key BURNS a tick and the
/// damage comes out LOW. The DENSITY is what makes it bite — at one proc a
/// shot the fixture never leaves the scan. 48,000 with it, 47,400 without.
/// THE RECORD IS THE WHOLE FIGHT: the effective damage of every event adds
/// up to the run's own total, which is what makes it a LEDGER rather than a
/// report. A site that moves a pool and records nothing fails it low, one
/// that records twice fails it high, and neither shows up in any aggregate.
/// THE DAMAGE METER'S PARTS ADD UP TO THE WHOLE — every instance credited
/// to exactly one bucket. `SourceDamage` is the one aggregate NOT behind
/// `ledger`'s door, because its buckets are EIGHT differently shaped
/// answers where the totals are one, so a descriptor carrying that to
/// `settle` would move the shapes rather than remove them.
#[test]
fn the_damage_meters_parts_add_up_to_the_whole() {
    let mut p = FightParams {
        forced_procs: vec![DamageType::Slash, DamageType::Heat],
        radial: Some(radial_of(1.0, 0.0)),
        ..bare(DamageType::Slash)
    };
    p.base_crit_chance = 0.5;
    p.crit_multiplier = 2.0;
    p.multishot = 2.4;

    let r = run_once(&p, &mut Rng::new(0x5EED));
    let s = &r.sources;
    let parts = s.direct
        + s.radial
        + s.field
        + s.arcane_on_status
        + s.extra_hit
        + s.syndicate
        + s.status.iter().sum::<f64>();
    let whole = r.effective_damage();
    assert!(whole > 0.0, "the fixture deals damage");
    // IT GRADES ITS OWN COVERAGE. A conservation test passes perfectly on a
    // bucket the fixture never fills, which is how the first version of it
    // survived having `sources.radial` deleted.
    assert!(s.direct > 0.0, "the fixture lands direct hits");
    assert!(s.radial > 0.0, "…and explosions: {}", s.radial);
    assert!(s.status.iter().sum::<f64>() > 0.0, "…and burns");
    assert!(
        (parts - whole).abs() <= 1e-6 * whole,
        "the meter's parts sum to {parts} and the run dealt {whole} — a              damage site booked its number and not its bucket"
    );
}

#[test]
fn the_record_adds_up_to_the_damage_total() {
    let mut p = FightParams {
        forced_procs: vec![DamageType::Slash, DamageType::Heat],
        ..bare(DamageType::Slash)
    };
    p.base_crit_chance = 0.5;
    p.crit_multiplier = 2.0;
    p.multishot = 2.4;
    // SOMETHING IN THE BASE-DAMAGE BRACKET. An unmodded weapon's ledger is
    // legitimately EMPTY — every factor is 1.0 and the base IS the raw —
    // so a fixture with no mods cannot assert anything about the shape of
    // a bracket.
    p.base_damage_bonus = 1.65;
    p.co_per_type = 0.8;
    p.co_behavior = crate::model::CoBehavior::AdditiveWithBaseDamage;

    let s = monte_carlo(&p, 8, 5);
    let state = s.median_run.rng_state;
    let rec = record(&p, state, 0.0, f64::INFINITY, 1_000_000, 0);
    assert_eq!(rec.dropped(), 0, "the cap is not what this is measuring");

    let damage: Vec<&crate::record::Damage> = rec
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            crate::record::Kind::Damage(d) => Some(&**d),
            _ => None,
        })
        .collect();
    assert!(damage.len() > 50, "the fixture is busy: {} rows", damage.len());

    // …AND IT IS THE SAME FIGHT. `record` re-runs from the median run's own
    // RNG state, so this is not "a fight with a similar total".
    let same = run_once(&p, &mut Rng::new(state));
    let sum: f64 = damage.iter().map(|d| d.effective).sum();
    assert!(
        (sum - same.effective_damage()).abs() / same.effective_damage() < 1e-9,
        "record {sum} vs run {}",
        same.effective_damage()
    );

    // EVERY ROW MULTIPLIES OUT — and now layer by layer, which is a
    // stronger claim than the flat product was: each layer STATES the
    // running total after it, so a bracket that adds its terms wrongly
    // fails here even when the end of the chain happens to land right.
    let mut checked = 0;
    for d in &damage {
        let mut at = d.base;
        for l in &d.layers {
            at = match l {
                crate::record::Layer::Bracket { terms, sum, out, .. } => {
                    let s: f64 = 1.0 + terms.iter().map(|t| t.value).sum::<f64>();
                    assert!(
                        (s - sum).abs() <= 1e-9,
                        "{:?}: 1 + Σ terms = {s}, layer says {sum}", d.origin
                    );
                    assert!(
                        (at * s - out).abs() <= 1e-6 * out.abs().max(1.0),
                        "{:?}: bracket out {} vs {}", d.origin, at * s, out
                    );
                    *out
                }
                crate::record::Layer::Quantize { out, .. } => *out,
                // A SUM RESTATES THE RUNNING TOTAL RATHER THAN MOVING IT:
                // it says what the number already there is MADE OF, so both
                // halves are asserted — the parts add up, and they add up
                // to what the chain was already carrying.
                crate::record::Layer::Sum { parts, out, .. } => {
                    let s: f64 = parts.iter().map(|x| x.amount).sum();
                    assert!(
                        (s - out).abs() <= 1e-9 * out.abs().max(1.0),
                        "{:?}: Σ parts = {s}, layer says {out}", d.origin
                    );
                    assert!(
                        (at - out).abs() <= 1e-6 * out.abs().max(1.0),
                        "{:?}: a sum moved the total: {out} against {at}", d.origin
                    );
                    *out
                }
                crate::record::Layer::Mul { value, out, .. } => {
                    assert!(
                        (at * value - out).abs() <= 1e-6 * out.abs().max(1.0),
                        "{:?}: mul out {} vs {}", d.origin, at * value, out
                    );
                    *out
                }
            };
        }
        let raw: f64 = at;
        assert!(
            (raw - d.raw).abs() <= 1e-6 * d.raw.abs().max(1.0),
            "{:?} at {:?}: base through the layers = {raw}, row says {}",
            d.origin, d.pool, d.raw
        );
        let eff: f64 = d.raw * d.mitigation.iter().map(|(_, v)| v).product::<f64>();
        assert!(
            (eff - d.effective).abs() <= 1e-6 * d.effective.abs().max(1.0),
            "{:?} at {:?}: raw x mitigation = {eff}, row says {}",
            d.origin, d.pool, d.effective
        );
        checked += 1;
    }
    // EVERY ROW, not most of them. A row with no layers is not exempt —
    // an empty ledger is the claim that the base IS the raw, and it is
    // ordinary now that a factor of exactly 1 is dropped at construction.
    assert_eq!(checked, damage.len(), "every row reconciles");
    // …AND EVERY DIRECT HIT CARRIES ONE. A status tick whose every factor
    // is 1.0 has an EMPTY ledger, which is correct and is most of a burn
    // build's stream — so "most rows" was never the property. What has to
    // hold is that a pellet, which always has a base-damage bracket, never
    // comes back with nothing to show.
    let hits: Vec<_> = damage
        .iter()
        .filter(|d| matches!(d.origin, crate::record::Origin::Own
            | crate::record::Origin::Multishot))
        .collect();
    assert!(hits.len() > 10, "the fixture lands hits: {}", hits.len());
    assert!(
        hits.iter().all(|d| !d.layers.is_empty()),
        "every direct hit carries a ledger"
    );
    // …AND IT OPENS WITH THE BASE-DAMAGE BRACKET, which is where Condition
    // Overload lives now. A row that opens with a `Mul` is one where the
    // bracket was collapsed back into a quotient.
    assert!(
        hits.iter().all(|d| matches!(d.layers.first(),
            Some(crate::record::Layer::Bracket { .. }))),
        "a pellet's ledger opens with its base-damage bracket"
    );

    // A ROW SAYS WHICH PELLET IT WAS. With multishot above 1 the stream has
    // to contain both, or the column is a constant.
    let origins: std::collections::BTreeSet<_> =
        damage.iter().map(|d| d.origin).collect();
    assert!(
        origins.contains(&crate::record::Origin::Own)
            && origins.contains(&crate::record::Origin::Multishot),
        "multishot is told apart from the pellet that was fired anyway: {origins:?}"
    );
    assert!(
        origins.contains(&crate::record::Origin::Status),
        "a burn is not a hit: {origins:?}"
    );
}

/// THE ENEMY SHIELD GATE, REPRODUCED FROM THE POP-UPS (MEASUREMENTS M61).
///
/// Four pop-ups off an unmodded base-form Laetum into a level 1 Crewman
/// with 120 shield, each two numbers — shield then health:
///
/// | hit | shield | health |
/// |---|---|---|
/// | 160 | 158 | 2 |
/// | 353 | 341 | 12 |
/// | 776 | 743 | 33 |
/// | 1710 | 1630 | 80 |
///
/// Every health side is `0.05 × (hit − 120)`, so the wiki's *"5% of the
/// damage dealt when hitting the shield gate will target enemy Health"* is
/// about the OVERFLOW: five per cent of 160 is 8 and the pop-up read 2.
///
/// AN ENGINE THAT DISCARDS THE EXCESS ANSWERS ZERO, and nothing in the
/// roster catches it — all three entries in `data/enemies/` carry
/// `shield: 0`.
#[test]
fn a_hit_that_breaks_a_shield_leaks_five_per_cent_to_health() {
    // The measurement's target, as the pop-ups describe it: 120 shield,
    // enough health to survive so the leak can be read off the pool.
    let target = Foe {
        base_shield: 120.0,
        base_armor: 0.0,
        base_health: 1e9,
        ..frail_target(TargetMode::InstantRespawn, 0.0, 0.0)
    };
    // ONE HIT, then read what each pool lost. `Cinematic` is the one type
    // that neither bypasses a shield nor reads a column, so the fixture
    // measures the GATE and nothing else.
    let leak_for = |hit: f64, head: bool| -> (f64, f64) {
        let mut st = TargetState::spawn(&target, crate::rules::space::Vec2::ORIGIN);
        let before_shield = st.shield;
        let before_health = st.health;
        st.apply(
            hit,
            TypeShares::single(DamageType::Impact),
            head,
            0.0,
            &target,
            true,
            &Mitigation { disrupt_amp: 1.0, virus_amp: 1.0, virus_stacks: 0, armor_multiplier: 1.0 },
            1.0,
            None,
        );
        (before_shield - st.shield, before_health - st.health)
    };

    // The four pop-ups, to the digit the game rounds to.
    for (hit, health) in [(160.0, 2.0), (353.0, 12.0), (776.0, 33.0), (1710.0, 80.0)] {
        let (shield_lost, health_lost) = leak_for(hit, false);
        assert!(
            (shield_lost - 120.0).abs() < 1e-9,
            "the shield gives up its whole pool: {shield_lost}"
        );
        assert!(
            (health_lost.round() - health).abs() < 1e-9,
            "hit {hit}: health took {health_lost}, the game popped {health}"
        );
    }

    // …AND A WEAKPOINT HIT IGNORES THE GATE ENTIRELY — *"Any Headshots or
    // shots to Weakspots completely bypass Corpus enemy Shield Gating"*. The
    // whole overflow lands, which is twenty times the body shot's.
    let (_, head_leak) = leak_for(160.0, true);
    assert!(
        (head_leak - 40.0).abs() < 1e-9,
        "a weakpoint hit pays the whole overflow: {head_leak}"
    );

    // A HIT THAT DOES NOT BREAK THE SHIELD REACHES NOTHING. Without this the
    // rule above would pass just as well on an engine that leaked 5% of
    // every hit, which is the reading of the wiki sentence this measurement
    // ruled out.
    let (shield_lost, health_lost) = leak_for(100.0, false);
    assert!(
        (shield_lost - 100.0).abs() < 1e-9 && health_lost.abs() < 1e-9,
        "an absorbed hit stays absorbed: shield {shield_lost}, health {health_lost}"
    );
}

/// A HIT ON A SHIELDED BODY POPS TWO NUMBERS, and the game shows two.
///
/// *"the Toxin portion bypasses straight to health"* is what `apply` has
/// always done — and it reported the SUM, so the one output of this app
/// that could be laid beside a recording and checked number for number was
/// the one output that could not. The split is not new
/// damage: the total is unchanged and only its presentation was wrong.
///
/// The fixture is a half-Toxin vector on a shielded target with armour, so
/// the two halves take DIFFERENT mitigation — the shield half none, the
/// Toxin half the armour term — which is what makes them two numbers rather
/// than one number written twice.
#[test]
fn a_hit_on_a_shielded_body_pops_the_toxin_half_separately() {
    let target = Foe {
        base_shield: 400.0,
        base_armor: 300.0,
        base_health: 1e12,
        mode: TargetMode::InfiniteHealth,
        ..frail_target(TargetMode::InfiniteHealth, 300.0, 0.0)
    };
    let p = FightParams {
        damage: DamageVector::new()
            .with(DamageType::Impact, 50.0)
            .with(DamageType::Toxin, 50.0),
        crit_multiplier: 1.0,
        base_crit_chance: 0.0,
        arcane: ArcaneFx::none(),
        body_parts: mono_body(1.0),
        foe: target,
        ..no_status()
    };
    let s = monte_carlo(&p, 4, 11);
    let state = s.median_run.rng_state;
    let rec = record(&p, state, 0.0, f64::INFINITY, 1_000_000, 0);
    let rows: Vec<&crate::record::Damage> = rec
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            crate::record::Kind::Damage(d) => Some(&**d),
            _ => None,
        })
        .collect();
    assert!(!rows.is_empty(), "the fixture lands hits");

    let shield: Vec<&&crate::record::Damage> =
        rows.iter().filter(|d| d.pool == Pool::Shield).collect();
    let health: Vec<&&crate::record::Damage> =
        rows.iter().filter(|d| d.pool == Pool::Health).collect();
    assert!(
        !shield.is_empty() && !health.is_empty(),
        "one pellet, two rows: {} on the shield, {} through it",
        shield.len(),
        health.len()
    );
    // …AND THEY ARE TOLD APART BY WHAT THEY ARE. The half a shield stops is
    // the physical one; the half that goes through is Toxin.
    assert!(
        shield.iter().all(|d| d.dtype == DamageType::Impact),
        "the shield half reads Impact"
    );
    assert!(
        health.iter().all(|d| d.dtype == DamageType::Toxin),
        "the half that got through reads Toxin"
    );
    // …AND THEY ARE DIFFERENT SIZES, because only one of them met armour.
    // Equal numbers would mean the split was cosmetic.
    let (a, b) = (shield[0].effective, health[0].effective);
    assert!(
        (a - b).abs() > 1e-3,
        "the two halves take different mitigation: {a} vs {b}"
    );

    // NO DAMAGE WAS INVENTED. The two rows of one instance sum to what the
    // damage meter counted — which is the property that makes this a
    // presentation fix and not a rebalance.
    let same = run_once(&p, &mut Rng::new(state));
    let sum: f64 = rows.iter().map(|d| d.effective).sum();
    assert!(
        (sum - same.effective_damage()).abs() / same.effective_damage() < 1e-9,
        "rows {sum} vs meter {}",
        same.effective_damage()
    );
}

#[test]
fn every_frame_that_took_damage_wrote_a_row() {
    // A build that exercises several of the nine at once: a direct hit, a
    // crit, a weak point, a Slash bleed and a Heat tick.
    let mut p = FightParams {
        forced_procs: vec![DamageType::Slash, DamageType::Heat],
        ..bare(DamageType::Slash)
    };
    p.base_crit_chance = 0.5;
    p.crit_multiplier = 2.0;
    let rep = replay(&p, 0, 60);
    let rec = record(&p, 0, 0.0, f64::INFINITY, 1_000_000, 0);
    assert_eq!(rec.dropped(), 0, "the cap is not what this is measuring");
    let rows: Vec<(f64, &crate::record::Damage)> = rec
        .events()
        .iter()
        .filter_map(|e| match &e.kind {
            crate::record::Kind::Damage(d) => Some((e.t, &**d)),
            _ => None,
        })
        .collect();

    // ONE FRAME IS ONE HALF-OPEN SLICE OF THE CLOCK, and a frame's damage
    // is what the curve gained over it — so a frame that gained damage and
    // holds no row is a damage site that moved a curve and wrote nothing.
    let mut frames_with_damage = 0;
    let mut frames_with_rows = 0;
    let mut prev = 0.0;
    let mut prev_t = 0.0;
    for f in &rep.frames {
        let gained = f.damage - prev;
        prev = f.damage;
        let (from, to) = (prev_t, f.t);
        prev_t = f.t;
        if gained <= 1e-9 {
            continue;
        }
        frames_with_damage += 1;
        if rows.iter().any(|(t, _)| *t > from - 1e-9 && *t <= to + 1e-9) {
            frames_with_rows += 1;
        }
    }
    assert!(frames_with_damage > 5, "the fixture deals damage: {frames_with_damage}");
    assert_eq!(
        frames_with_damage, frames_with_rows,
        "{} frames gained damage and {} carry a row — a damage site              without a `log_damage` beside its `timeline.add`",
        frames_with_damage, frames_with_rows
    );

    // …AND THE KINDS ARE TOLD APART. A run with crits, weak points and a
    // DoT must produce more than one kind, or the classifier is a constant.
    let kinds: std::collections::BTreeSet<PopKind> =
        rows.iter().map(|(_, d)| d.kind).collect();
    assert!(kinds.len() >= 2, "a fight has more than one kind of number: {kinds:?}");
    assert!(
        kinds.contains(&PopKind::Status),
        "a Slash bleed and a Heat tick are statuses: {kinds:?}"
    );

    // …AND NOTHING IS WRITTEN DOWN WHEN NOBODY IS READING. The recorder is
    // off for the 999 runs that are summed and thrown away, which is what
    // keeps it free — asserted because "it is only on while recording" is
    // exactly the kind of claim that quietly stops being true.
    let mut off = crate::record::Record::off();
    assert!(!off.is_on(), "an untraced run collects nothing");
    run_once_traced(&p, &mut Rng::new(0), None, &mut off);
    assert!(off.events().is_empty(), "and it collected nothing");
}

#[test]
fn a_dense_electricity_fight_pays_each_stack_once() {
    let dense = FightParams {
        fire_rate: 10.0,
        forced_procs: vec![DamageType::Electricity; 8],
        ..bare(DamageType::Electricity)
    };
    let s = monte_carlo(&dense, 20, 5);
    assert!(
        // …plus one accumulator per tick group, sixteen of them.
        (s.mean_dot_damage - (48_000.0 + 16.0 * 0.5)).abs() < 1e-6,
        "dot {} — 47,400 is the answer with the stale-key guard removed",
        s.mean_dot_damage
    );
}

/// A GUARDIAN EXIMUS'S AURA TAKES NINE TENTHS, AND NOT OFF OVERGUARD.
///
/// The first effect this engine models that a body's own side gave it. wiki
/// `Eximus` §Guardian: the shields *"provide '''90%''' damage reduction to all
/// attacks to their allies in range"*, and *"Damage reduction does not apply to
/// {{D|Overguard}} of nearby units"* — two claims, and the second is the one a
/// single multiplier on the instance would get wrong.
///
/// So this measures BOTH: a body with no overguard takes a tenth, and a body
/// with one loses that pool at full speed. A target that is pure overguard is
/// the sharpest form of the second claim — its whole bar is the pool the aura
/// cannot touch, so its damage must not move at all.
#[test]
fn a_guardian_aura_spares_overguard_and_takes_nine_tenths_of_the_rest() {
    let run = |overguard: f64, aura: bool| {
        let mut p = FightParams {
            duration_seconds: 20.0,
            ..FightParams::dual_toxocyst_incarnon()
        };
        p.foe.base_overguard = overguard;
        p.foe.base_health = 1.0e9;
        p.foe.base_armor = 0.0;
        p.foe.base_shield = 0.0;
        p.foe.guardian_aura = aura;
        let r = run_once(&p, &mut Rng::new(0x5EED));
        r.taken.by_body().0[0]
    };

    // NO POOL IN FRONT OF IT: the aura is the only thing between the shot and
    // health, so a tenth lands.
    let (bare, shielded) = (run(0.0, false), run(0.0, true));
    assert!(bare > 0.0, "the fixture landed nothing");
    let share = shielded / bare;
    assert!(
        (share - 0.10).abs() < 0.005,
        "90% off what reaches health: {shielded} of {bare} is {share}",
    );

    // A BAR THAT IS ENTIRELY THE POOL THE AURA CANNOT REACH. Big enough that
    // the engagement never empties it, so every instance in the fight is
    // settled against overguard and nothing else.
    let (og_bare, og_aura) = (run(1.0e9, false), run(1.0e9, true));
    assert!(og_bare > 0.0, "the overguard fixture landed nothing");
    assert!(
        (og_aura - og_bare).abs() < 1e-6,
        "the aura does not apply to Overguard: {og_aura} against {og_bare}",
    );
}

/// AN ANCIENT PROTECTOR PUTS A BAR IN FRONT OF THE BODY, and not in front of
/// one that brought its own.
///
/// The other shape in the class the Guardian aura opened: that one takes damage
/// off, this one adds a pool. wiki `Ancient Protector`: four pulses of *"'''200%'''
/// of their maximum health as Overguard"*, *"capped at '''800%''' of a unit's
/// maximum health"*, and it *"Cannot grant Overguard to ... any enemy with innate
/// Overguard"* — which is the half a bare addition would get wrong.
#[test]
fn an_ancient_protectors_aura_is_eight_times_health_and_skips_a_body_with_its_own() {
    let foe = |protector: bool, eximus: bool| {
        let spec = crate::data::enemies::EnemySpec::load(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../data/enemies/crewman.yaml"
        )))
        .unwrap();
        let mut f = spec
            .target_params(150, false, eximus, TargetMode::InstantRespawn)
            .unwrap();
        f.ancient_protector_aura = protector;
        f
    };

    // A UNIT WITH NO OVERGUARD OF ITS OWN takes the whole grant, and it is a
    // multiple of its OWN health — so it scales exactly as that does.
    let bare = foe(false, false);
    assert_eq!(bare.overguard(), 0.0, "the fixture has none of its own");
    let held = foe(true, false);
    let want = held.max_health() * crate::target::ANCIENT_PROTECTOR_OVERGUARD;
    assert!(
        (held.overguard() - want).abs() < 1e-6,
        "800% of {} is {want}, got {}",
        held.max_health(),
        held.overguard(),
    );

    // …AND A BODY THAT BROUGHT ITS OWN IS EXCLUDED BY NAME, so an Eximus is
    // unchanged rather than stacked.
    let elite = foe(false, true);
    let elite_in_aura = foe(true, true);
    assert!(elite.overguard() > 0.0, "an Eximus has its own");
    assert!(
        (elite_in_aura.overguard() - elite.overguard()).abs() < 1e-6,
        "the aura is refused: {} against {}",
        elite_in_aura.overguard(),
        elite.overguard(),
    );
}
