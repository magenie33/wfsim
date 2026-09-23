use super::*;

#[test]
fn frenzy_ammo_efficiency_prevents_reloads() {
    // All-head with Frenzy: only the first shot consumes ammo (Frenzy's
    // +100% efficiency zeroes the rest), so the 25-shot cadence holds
    // with zero reloads despite the 12-round magazine.
    let p = FightParams {
        frenzy: true,
        body_parts: vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: true,
        }],
        ..no_status()
    };
    let s = monte_carlo(&p, 20, 4);
    assert!((s.mean_shots - 25.0).abs() < 1e-9, "shots {}", s.mean_shots);
    assert_eq!(s.mean_reloads, 0.0);
}

#[test]
fn frenzy_accelerates_fire_rate_on_headshots() {
    // All-head aim: the first headshot grants Frenzy (fire rate x2.5 ->
    // interval 0.4 s), refreshed by every subsequent headshot. Shots at
    // t = 0, 0.4, 0.8, ... -> 1 + floor(9.99../0.4) = 25 shots in 10 s
    // (vs 10 without Frenzy).
    let p = FightParams {
        frenzy: true,
        body_parts: vec![BodyPart {
            name: "head".into(),
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: true,
        }],
        ..no_status()
    };
    let s = monte_carlo(&p, 20, 4);
    assert!((s.mean_shots - 25.0).abs() < 1e-9, "shots {}", s.mean_shots);
    // Body-only aim: Frenzy never triggers -> plain 10 shots.
    let q = FightParams {
        frenzy: true,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let s2 = monte_carlo(&q, 20, 4);
    assert!(
        (s2.mean_shots - 10.0).abs() < 1e-9,
        "shots {}",
        s2.mean_shots
    );
}

#[test]
fn locked_frenzy_is_always_active_without_headshots() {
    // Body-only aim never triggers Frenzy naturally, but the lock keeps
    // it up from t=0: cadence 0.4 s -> 25 shots in 10 s.
    let p = FightParams {
        frenzy: true,
        locked_buffs: vec![BuffLock::permanent(LockedBuff::Frenzy)],
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let s = monte_carlo(&p, 20, 4);
    assert!((s.mean_shots - 25.0).abs() < 1e-9, "shots {}", s.mean_shots);
}

#[test]
fn status_procs_occur_at_the_listed_rate() {
    // 37% SC, no forced procs: mean procs per shot ≈ 0.37 on the
    // never-dying training dummy.
    let s = monte_carlo(&FightParams::default(), 2000, 8);
    let per_shot = s.mean_procs / s.mean_shots;
    assert!((per_shot - 0.37).abs() < 0.02, "procs/shot {per_shot}");
    // Bleeds contribute extra damage on top of the 3375 baseline.
    assert!(s.mean_dot_damage > 0.0);
    assert!(s.mean_damage > 3375.0);
}

#[test]
fn forced_bleed_dot_is_exactly_deterministic() {
    // Forced Slash on every shot, SC 0, mono body 1x, crit_multiplier 1
    // (tier changes nothing): every shot procs one bleed with tick value
    // `(75 + 1) × 0.35 = 26.6`, ticking at +1..+6 s. A Slash stack is its
    // own tick group, so each carries its own accumulator
    // (`Dot::accumulator_unit`). Ticks beyond the 10 s engagement are lost:
    // shots at 0..9 yield 6,6,6,6,5,4,3,2,1,0 ticks = 39 ticks →
    // dot = 39 × 26.6 = 1037.4; direct = 10 × 75 = 750.
    let p = FightParams {
        crit_multiplier: 1.0,
        forced_procs: vec![DamageType::Slash],
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let s = monte_carlo(&p, 50, 5);
    assert!(
        (s.mean_dot_damage - 1037.4).abs() < 1e-6,
        "dot {}",
        s.mean_dot_damage
    );
    assert!(
        (s.mean_damage - 1787.4).abs() < 1e-6,
        "total {}",
        s.mean_damage
    );
    assert!((s.mean_procs - 10.0).abs() < 1e-9);
}

/// A STATUS TICK IS DRAWN AS ITS TWO HALVES, and the halves are not scaled
/// alike — which is the whole reason the ledger has a shape for it.
///
/// `(Σ seeds + 1) x C x M` (MEASUREMENTS M58), and the `1` carries ONE
/// faction layer where the seeds carry the payload's own depth (M56). A
/// target with its own bracket multiplier is the only fixture that can tell
/// the two apart: at x0.8 the seeds take x0.64 and the accumulator x0.8, so
/// a ledger that scaled them together would land on neither number.
#[test]
fn a_status_tick_is_drawn_as_its_two_halves() {
    let mut p = FightParams {
        crit_multiplier: 1.0,
        forced_procs: vec![DamageType::Slash],
        body_parts: mono_body(1.0),
        ..no_status()
    };
    p.target.faction_bracket_multiplier = 0.8;
    p.target.base_health = 1e15;
    let rec = record(&p, 0, 0.0, f64::INFINITY, 10_000, 0);
    let tick = rec
        .events()
        .iter()
        .find_map(|e| match &e.kind {
            crate::record::Kind::Damage(d)
                if d.origin == crate::record::Origin::Status => Some(d.clone()),
            _ => None,
        })
        .expect("a forced bleed ticks");

    let sum = tick.layers.iter().find_map(|l| match l {
        crate::record::Layer::Sum { parts, out, .. } => Some((parts.clone(), *out)),
        _ => None,
    }).expect("a status tick states what it is made of");
    let (parts, out) = sum;
    assert_eq!(parts.len(), 2, "seeds and the accumulator, and nothing else");
    assert_eq!(parts[0].factor, crate::record::Factor::StatusSeeds);
    assert_eq!(parts[1].factor, crate::record::Factor::StatusAccumulator);

    // THE NUMBERS, and they are the fixture's own: base 75, C = 0.35.
    //   seeds 0.35 x 75 x 0.8^2 = 16.80      accumulator 0.35 x 0.8 = 0.28
    assert!((parts[0].amount - 16.8).abs() < 1e-9, "seeds {}", parts[0].amount);
    assert!((parts[1].amount - 0.28).abs() < 1e-9, "accumulator {}", parts[1].amount);
    assert!((out - 17.08).abs() < 1e-9, "tick {out}");
    assert!((tick.base - 17.08).abs() < 1e-9, "the row is its own sum: {}", tick.base);

    // …AND EACH PART SAYS WHERE IT CAME FROM, which is the half a reader
    // checks against a card. The target's multiplier is the term that
    // differs, and it differs by exactly the depth.
    let of = |i: usize, f: crate::record::Factor| -> f64 {
        parts[i].of.iter().find(|g| g.factor == f).map_or(f64::NAN, |g| g.value)
    };
    assert!((parts[0].head - 26.25).abs() < 1e-9, "seed head {}", parts[0].head);
    assert!((parts[1].head - 0.35).abs() < 1e-9, "accumulator head {}", parts[1].head);
    assert!((of(0, crate::record::Factor::TargetMultiplier) - 0.64).abs() < 1e-9);
    assert!((of(1, crate::record::Factor::TargetMultiplier) - 0.8).abs() < 1e-9);
}

/// A HEAT TICK TAKES THE WEAK POINT AND ITS ACCUMULATOR DOES NOT —
/// MEASUREMENTS M90, and the numbers are the reading rather than a
/// direction: a Braton Prime at base 35 with +200% Heat popped 54 on the
/// body and 159 on the head.
///
/// 159 IS THE WHOLE POINT. Scaled whole the head tick would be 162 and
/// with neither half scaled it would be 54, so this fixture separates all
/// three readings — and the three-point difference is the `1` declining a
/// multiplier the seed took.
#[test]
fn a_heat_tick_takes_the_weak_point_and_its_accumulator_does_not() {
    let tick = |part: f64| {
        let p = FightParams {
            // THE FIXTURE'S OWN BASE, and the DoT reads it through
            // `dot_modified_base` the way a real weapon's panel feeds it.
            damage: DamageVector::new().with(DamageType::Heat, 35.0),
            dot_modified_base: Some(35.0),
            // +200% Heat, which is the bracket a tick multiplies by.
            elem_dot_bonus: vec![(DamageType::Heat, 3.0)],
            crit_multiplier: 1.0,
            forced_procs: vec![DamageType::Heat],
            body_parts: mono_body(part),
            // ONE SHOT, so every tick in the window belongs to one proc and
            // the accumulator is counted once rather than folded with a
            // second contribution.
            fire_rate: 0.02,
            duration_seconds: 4.0,
            magazine_size: 100.0,
            ..no_status()
        };
        let rec = record(&p, 0, 0.0, f64::INFINITY, 10_000, 0);
        rec.events()
            .iter()
            .find_map(|e| match &e.kind {
                crate::record::Kind::Damage(d)
                    if d.origin == crate::record::Origin::Status
                        && d.dtype == DamageType::Heat => Some(d.base),
                _ => None,
            })
            .expect("a forced Heat proc ticks")
    };
    let (body, head) = (tick(1.0), tick(3.0));
    assert!((body - 54.0).abs() < 1e-9, "body {body}");
    assert!((head - 159.0).abs() < 1e-9, "head {head}");
    // …AND SAID AS THE RATIO, because that is the form the reading is in
    // and the form a regression would break first.
    assert!((head / body - 2.9444444444444).abs() < 1e-9, "x{}", head / body);
}

/// HUNTER MUNITIONS. A guaranteed crit with a 100% roll must bleed on
/// every shot, on a weapon whose vector holds NO Slash at all — the mod's
/// whole point is that it does not draw from the damage types.
#[test]
fn hunter_munitions_bleeds_off_a_crit_on_a_weapon_with_no_slash() {
    // no_status() gives status chance 0, so a bleed here can only have
    // come from the crit roll. Crit chance 1.0, multiplier 1.0 keeps the
    // arithmetic the same as the forced-Slash case above.
    let p = FightParams {
        damage: DamageVector::new().with(DamageType::Puncture, 75.0),
        base_crit_chance: 1.0,
        crit_multiplier: 1.0,
        slash_on_crit: 1.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let s = monte_carlo(&p, 50, 11);
    // Same schedule as the forced-Slash test: 39 ticks x 0.35 x (75 + 1).
    assert!((s.mean_dot_damage - 1037.4).abs() < 1e-6, "dot {}", s.mean_dot_damage);
    assert!((s.mean_procs - 10.0).abs() < 1e-9, "one proc per shot");

    // Without the mod the same build bleeds not at all.
    let none = FightParams { slash_on_crit: 0.0, ..p.clone() };
    assert_eq!(monte_carlo(&none, 50, 11).mean_dot_damage, 0.0);
}

/// The chance is independent of status chance and rolls PER PELLET, so a
/// 30% mod on a guaranteed crit bleeds about 30% of pellets.
#[test]
fn hunter_munitions_rolls_its_own_chance_per_pellet() {
    let p = FightParams {
        damage: DamageVector::new().with(DamageType::Puncture, 75.0),
        base_crit_chance: 1.0,
        crit_multiplier: 1.0,
        slash_on_crit: 0.30,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let s = monte_carlo(&p, 4000, 12);
    let per_shot = s.mean_procs / 10.0; // 10 shots in the window
    assert!((per_shot - 0.30).abs() < 0.02, "procced {per_shot:.3} per shot, expected ~0.30");
}

/// Hunter Munitions is a CRITICAL HIT'S PRIVILEGE, not a bleed of its own:
/// the trigger changed, nothing else did. Every DoT in
/// this engine hangs off a PARENT hit and inherits that hit's multipliers,
/// and the whole point of pushing `Slash` onto the pellet's proc list is
/// that the mod's bleed gets the same parent as a naturally rolled one.
///
/// Proven rather than argued: against a FORCED Slash proc under identical
/// conditions the bleeds must agree, in every condition a bleed reads.
///
/// They agree to ~0.06% rather than exactly, and that gap is not
/// mechanical: the mod's own roll consumes an RNG draw per pellet, so the
/// two builds walk different random streams. It shrinks with runs
/// (1.6% at 200, 0.06% at 6000), which is what sampling noise does and a
/// real difference does not.
#[test]
fn a_hunter_munitions_bleed_is_indistinguishable_from_any_other_slash() {
    // Guaranteed crit + guaranteed roll, so both builds proc exactly once
    // per pellet and the only difference is WHERE the proc came from.
    let pair = |tweak: fn(&mut FightParams)| {
        let base = || {
            let mut p = FightParams {
                damage: DamageVector::new().with(DamageType::Puncture, 75.0),
                base_crit_chance: 1.0,
                body_parts: mono_body(1.0),
                ..no_status()
            };
            tweak(&mut p);
            p
        };
        let mut forced = base();
        forced.forced_procs = vec![DamageType::Slash];
        let mut hm = base();
        hm.slash_on_crit = 1.0;
        let (f, h) = (monte_carlo(&forced, 6000, 21), monte_carlo(&hm, 6000, 21));
        assert_eq!(f.mean_procs, h.mean_procs, "one proc per pellet either way");
        (f.mean_dot_damage, h.mean_dot_damage)
    };
    let same = |(f, h): (f64, f64), what: &str| {
        assert!(
            (f - h).abs() / f < 0.005,
            "{what}: forced {f:.2} vs hunter munitions {h:.2}"
        );
        h
    };

    // The bleed coefficient and its armour bypass.
    let plain = same(pair(|_| {}), "plain bleed");
    // The PARENT's crit multiplier — the tier that pellet rolled.
    same(pair(|p| p.crit_multiplier = 3.0), "crit multiplier");
    // The PARENT's body part — a 3x head multiplies the bleed too.
    same(pair(|p| p.body_parts = mono_body(3.0)), "part multiplier");
    // Red crits: cc 2.0 is tier 2 guaranteed on both.
    let tier2 = same(
        pair(|p| {
            p.base_crit_chance = 2.0;
            p.crit_multiplier = 3.0;
        }),
        "tier 2",
    );
    // Status DURATION — its own set mate, Hunter Track, does exactly this.
    let longer = same(pair(|p| p.status_duration_multiplier = 1.9), "status duration");
    // The status DAMAGE bucket.
    same(pair(|p| p.status_damage_multiplier = 1.5), "status damage");
    // A Vigilante promotion happens BEFORE this roll reads the tier, so
    // the promoted crit is the parent.
    let promoted = same(
        pair(|p| {
            p.crit_tier_upgrade_chance = 1.0;
            p.crit_multiplier = 3.0;
        }),
        "vigilante-promoted parent",
    );

    // And each of those conditions actually moved the bleed — an
    // agreement between two numbers that never change proves nothing.
    // Only ~14%, not 90%: the engagement window truncates the tail, so a
    // longer bleed only pays for shots with room left to tick.
    assert!(longer > plain * 1.10, "duration added ticks: {plain:.0} -> {longer:.0}");
    let crit_only = same(pair(|p| p.crit_multiplier = 3.0), "crit multiplier");
    assert!(tier2 > crit_only * 1.4, "tier 2 hit harder: {crit_only:.0} -> {tier2:.0}");
    assert!(promoted > crit_only * 1.4, "the promotion reached the bleed");
}

/// INTERNAL BLEEDING's bleed is an ordinary bleed too. It has always fed
/// the same per-pellet proc list, so it has always had the same parent —
/// this makes that checkable rather than something to take on trust, and
/// pins that the two mods differ ONLY in what triggers them.
#[test]
fn an_internal_bleeding_bleed_is_indistinguishable_from_any_other_slash() {
    use crate::build::loadout::ProcConv;
    let base = || FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 75.0),
        base_crit_chance: 0.0,
        crit_multiplier: 3.0,
        status_chance: 1.0,
        base_status_chance: 1.0,
        body_parts: mono_body(3.0), // a 3x part, so the parent matters
        ..FightParams::default()
    };
    // Converted at 100%: every pellet lands Impact and turns it into Slash.
    let mut ib = base();
    ib.proc_conversion = Some(ProcConv {
        from: DamageType::Impact,
        to: DamageType::Slash,
        chance: 1.0,
        low_rate_threshold: 0.0, // never doubled, so the rate is irrelevant
        low_rate_multiplier: 1.0,
    });
    // The same bleed, forced, on a weapon that cannot roll one itself.
    let mut forced = base();
    forced.damage = DamageVector::new().with(DamageType::Puncture, 75.0);
    forced.status_chance = 0.0;
    forced.base_status_chance = 0.0;
    forced.forced_procs = vec![DamageType::Slash];

    let (i, f) = (monte_carlo(&ib, 6000, 41), monte_carlo(&forced, 6000, 41));
    assert!(
        (i.mean_dot_damage - f.mean_dot_damage).abs() / f.mean_dot_damage < 0.005,
        "internal bleeding {:.2} vs forced Slash {:.2}",
        i.mean_dot_damage,
        f.mean_dot_damage
    );
    assert!(i.mean_dot_damage > 0.0);
}

/// The Vigilante promotion reaches EVERY attack part that can crit. The
/// set says "enhance Critical Hits from Primary Weapons" with no qualifier
/// about which part made the hit, so a direct hit, a lingering field tick
/// and an EXPLOSION all take it — the explosion was left out at first,
/// which was an artifact of where the code was edited and nothing else.

#[test]
fn the_vigilante_promotion_reaches_an_explosion_too() {
    let radial = crate::build::loadout::ResolvedRadial {
        blast_kind: crate::model::BlastKind::Contact,
        damage: {
            let mut d = DamageVector::default();
            d.set(DamageType::Radiation, 100.0);
            d
        },
        modified_base: 100.0,
        crit_chance: 1.0, // always a tier-1 crit, so a promotion is visible
        crit_damage: 3.0,
        base_crit_chance: 1.0,
        base_crit_damage: 3.0,
        status_chance: 0.0,
        base_status_chance: 0.0,
        radius_m: 2.0,
        falloff_start_m: 0.0,
        falloff_reduction: 0.0,
        forced_procs: Default::default(),
        takes_condition_overload: false,
        takes_multishot: true,
        co_base: crate::model::CoBase::whole_for(crate::model::CoStage::Radial),
    };
    // A zero-damage direct hit, so everything reported is the explosion's.
    let p = |promote: f64| FightParams {
        damage: DamageVector::default(),
        radial: Some(radial),
        crit_tier_upgrade_chance: promote,
        base_crit_chance: 0.0,
        crit_multiplier: 1.0,
        fire_rate: 1.0,
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let off = monte_carlo(&p(0.0), 200, 51);
    let on = monte_carlo(&p(1.0), 200, 51);
    // The explosion always crits at tier 1; promoting it makes every one a
    // BIG crit, which is the exact statement — the damage ratio is not,
    // because this fixture's total is not all crit-scaled.
    assert!(off.mean_big_crit_rate < 1e-9, "no promotion, no big crits");
    // EVERY crit is a big crit — an identity, not a threshold. A
    // `> 0.44` against a rate of ~0.445 leaves a 0.002 margin whose only
    // evidence is the seed that produced it: the denominator counts all
    // instances and the zero-damage direct hit never crits. Splitting the
    // RNG streams moves this sample to 0.430 and the
    // test failed on a fixture nothing was wrong with (old spread over ten
    // seeds 0.438-0.457, new 0.430-0.453 — the same distribution).
    //
    // The claim was never about the rate. Promotion is certain here, so
    // every crit is promoted, and that is exact at any seed.
    assert!(on.mean_big_crit_rate > 0.0, "nothing crit at all");
    assert!(
        (on.mean_big_crit_rate - on.mean_crit_rate).abs() < 1e-12,
        "every explosion crit promoted: big {:.4} of crit {:.4}",
        on.mean_big_crit_rate, on.mean_crit_rate
    );
    assert!(
        on.mean_damage > off.mean_damage * 1.3,
        "and it is worth damage: {:.0} -> {:.0}",
        off.mean_damage,
        on.mean_damage
    );
}

/// HUNTER MUNITIONS + INTERNAL BLEEDING, against a number the wiki
/// publishes: on a shot that both crits and applies an Impact status the
/// Slash chance is **54.5%** at fire rate >= 2.5, and **79%** below it.
///
/// The two are "drawn independently, and if both proc at the same time,
/// only 1 slash proc is applied" — so the combined chance is a union,
/// 1 - (1-0.30)(1-0.35) = 0.545, and with Internal Bleeding doubled below
/// 2.5, 1 - (1-0.30)(1-0.70) = 0.79. Reproducing both from the two rolls
/// is what shows the exclusion is modeled as an exclusion and not as some
/// second bleed quietly going missing.
#[test]
fn hunter_munitions_and_internal_bleeding_union_to_the_wikis_numbers() {
    use crate::build::loadout::ProcConv;
    // Pure Impact at 100% status: every pellet lands the Impact proc that
    // Internal Bleeding converts from, and every pellet crits.
    let build = |fire_rate: f64| FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 75.0),
        base_crit_chance: 1.0,
        crit_multiplier: 1.0,
        status_chance: 1.0,
        base_status_chance: 1.0,
        fire_rate,
        slash_on_crit: 0.30,
        proc_conversion: Some(ProcConv {
            from: DamageType::Impact,
            to: DamageType::Slash,
            chance: 0.35,
            low_rate_threshold: 2.5,
            low_rate_multiplier: 2.0,
        }),
        body_parts: mono_body(1.0),
        ..FightParams::default()
    };
    // procs per pellet = 1 Impact + P(Slash), so P falls straight out.
    let p_slash = |fr: f64, seed: u64| {
        let p = build(fr);
        let s = monte_carlo(&p, 8000, seed);
        s.mean_procs / s.mean_pellets - 1.0 // reloads make shots != duration x rate
    };
    let fast = p_slash(4.0, 31);
    let slow = p_slash(2.0, 32);
    assert!((fast - 0.545).abs() < 0.015, "fire rate 4.0: {fast:.3}, wiki 0.545");
    assert!((slow - 0.79).abs() < 0.015, "fire rate 2.0: {slow:.3}, wiki 0.79");
}

/// The roll hangs off the CRIT, so at half the crit chance it bleeds half
/// as often — "indirectly affected by its Critical Chance" (wiki).
#[test]
fn hunter_munitions_tracks_the_crit_rate() {
    let at = |cc: f64, seed: u64| {
        let p = FightParams {
            damage: DamageVector::new().with(DamageType::Impact, 75.0),
            base_crit_chance: cc,
            crit_multiplier: 1.0,
            slash_on_crit: 1.0,
            body_parts: mono_body(1.0),
            ..no_status()
        };
        monte_carlo(&p, 4000, seed).mean_procs / 10.0 // 10 shots in the window
    };
    let full = at(1.0, 13);
    let half = at(0.5, 14);
    let none = at(0.0, 15);
    assert!((full - 1.0).abs() < 0.01, "every crit bleeds: {full:.3}");
    assert!(full > half && half > none, "{full:.3} > {half:.3} > {none:.3}");
    assert!(full - none > 0.3, "the crit rate has to move it: {none:.3}");
    // NOT an exact ratio, and not for a reason that belongs to this mod:
    // the raw `FightParams::default()` fixture carries a crit FLOOR of
    // ~0.45 that `base_crit_chance: 0.0` does not clear (it survives
    // zeroing `unmodded_crit_chance` and does not depend on the damage
    // type, so it is neither the relative term nor Puncture's Weakened).
    // Existing tests never saw it because they neutralise crits with
    // `crit_multiplier: 1.0` instead of the chance. Worth chasing on its
    // own; asserting a clean 0.5-of-full here would only be encoding it.
}

#[test]
fn bleed_snapshots_the_proccing_hits_multipliers() {
    // 3x part, forced Slash, crit disabled. The SEED is 75 × 3 = 225 — the
    // body part is a fact about the hit and rides inside it — and the tick
    // is `(225 + 1) × 0.35 = 79.1`. See `Dot::accumulator_unit`.
    let p = FightParams {
        crit_multiplier: 1.0,
        forced_procs: vec![DamageType::Slash],
        body_parts: mono_body(3.0),
        duration_seconds: 2.0, // one shot at t=0 (+ t=1): first bleed ticks once at t=1...
        fire_rate: 0.5,     // single shot at t=0 in a 2 s window
        ..no_status()
    };
    // Strike at t=0 procs a bleed ticking at t=1 (once before 2 s).
    let s = monte_carlo(&p, 10, 6);
    assert!(
        (s.mean_dot_damage - 79.1).abs() < 1e-9,
        "dot {}",
        s.mean_dot_damage
    );
}

#[test]
fn weakened_raises_our_crit_chance() {
    // Forced Puncture, SC 0, mono 1x, cd 2.0, base cc 0:
    // shot k has Enervate 0.10k + Weakened 0.05·min(k,5) flat cc,
    // E[crit_multiplier] = 1 + cc → E[total] = 75 × (10 + Σcc) = 75 × 16.25.
    // Σcc = 0+.15+.30+.45+.60+.75+.85+.95+1.05+1.15 = 6.25.
    let p = FightParams {
        base_crit_chance: 0.0,
        forced_procs: vec![DamageType::Puncture],
        body_parts: mono_body(1.0),
        ..no_status()
    };
    let s = monte_carlo(&p, 4000, 77);
    let expect = 75.0 * 16.25;
    assert!(
        (s.mean_damage - expect).abs() / expect < 0.02,
        "mean {} expect {expect}",
        s.mean_damage
    );
}

/// PRELUDE OF MIGHT IS CHECKED AT THE HIT, NOT ON THE ARSENAL SCREEN.
///
/// Wiki (Furis / Braton Incarnon Genesis), the perk's own row: "With
/// Critical Chance below 40%: Increase Base Critical Damage Multiplier by
/// +3x" — and, on the same row, "Condition is affected by the critical
/// chance increase effect of Puncture status". Weakened is +5% flat crit
/// chance received per stack, so a build that starts under the line walks
/// over it on its own Puncture procs and loses the perk while they hold.
///
/// Arithmetic rather than statistics: one forced Puncture per shot, one
/// shot a second, no other crit source, cd 5.0 granted (2.0 + 3.0).
///   shot 0   0 stacks   cc .32   on    1 + .32 x (5 - 1) = 2.28
///   shot 1   1 stack    cc .37   on                        2.48
///   shot 2   2 stacks   cc .42   off   1 + .42 x (2 - 1) = 1.42
///   shot 3   3 stacks   cc .47   off                       1.47
///   shot 4   4 stacks   cc .52   off                       1.52
/// Sum 9.17, against 5 x 2.28 = 11.40 with the forced proc taken away — the
/// control. Status IMMUNITY would not have been one: a forced proc goes on
/// regardless of it.
#[test]
fn weakened_takes_prelude_of_might_away() {
    let build = || FightParams {
        damage: DamageVector::new().with(DamageType::Puncture, 100.0),
        base_crit_chance: 0.32,
        unmodded_crit_chance: 0.32,
        crit_multiplier: 5.0,
        crit_multiplier_below_crit_chance: Some((3.0, 0.40)),
        unmodded_crit_damage: 2.0,
        forced_procs: vec![DamageType::Puncture],
        body_parts: mono_body(1.0),
        fire_rate: 1.0,
        duration_seconds: 5.0,
        arcane: crate::data::arcanes::ArcaneFx::none(),
        target: TargetParams { base_health: 1e15, ..FightParams::default().target },
        ..no_status()
    };
    let lost = monte_carlo(&build(), 4000, 91).mean_damage;
    assert!(
        (lost - 917.0).abs() / 917.0 < 0.02,
        "with Weakened {lost}, expected 917"
    );

    let mut unproced = build();
    unproced.forced_procs.clear();
    let kept = monte_carlo(&unproced, 4000, 91).mean_damage;
    assert!(
        (kept - 1140.0).abs() / 1140.0 < 0.02,
        "without Weakened {kept}, expected 1140"
    );
}

/// "BELOW" IS STRICT. Same fixture with no procs at all, and a threshold
/// set exactly ON the build's crit chance: the perk is gone, so the run is
/// worth 5 x (1 + .32 x (2 - 1)) = 6.60 -> 660.
#[test]
fn prelude_of_might_is_off_exactly_at_its_threshold() {
    let p = FightParams {
        damage: DamageVector::new().with(DamageType::Puncture, 100.0),
        base_crit_chance: 0.32,
        unmodded_crit_chance: 0.32,
        crit_multiplier: 5.0,
        crit_multiplier_below_crit_chance: Some((3.0, 0.32)),
        unmodded_crit_damage: 2.0,
        body_parts: mono_body(1.0),
        fire_rate: 1.0,
        duration_seconds: 5.0,
        arcane: crate::data::arcanes::ArcaneFx::none(),
        target: TargetParams { base_health: 1e15, ..FightParams::default().target },
        ..no_status()
    };
    let s = monte_carlo(&p, 4000, 91).mean_damage;
    assert!((s - 660.0).abs() / 660.0 < 0.02, "at the threshold {s}, expected 660");
}

/// "+5 **BASE** MULTISHOT" IS NOT "+5 MULTISHOT", and the difference is the
/// whole perk.
///
/// Forceful Finality's card carries the word Base, and the wiki attaches a
/// note to that row: *"Multishot bonus is added before mods, and is thus
/// multiplied by multishot bonuses."* The Torid's Final Fusillade — the
/// other perk that grants a flat multishot on the magazine's last round —
/// says only "+3 Multishot", with no such note, and is flat.
///
/// Both were modelled as flat until 2026-08-11. This measures the pellets a
/// full magazine actually fires, because that is the only place the two
/// readings differ: the panel is identical under either.
#[test]
fn a_base_multishot_grant_is_multiplied_by_multishot_mods() {
    let arena = crate::arena::Arena::training(20.0);
    let pellets = |evo: &[&str], mods: &[&crate::model::ModDef]| {
        let base = crate::model::WeaponBase::from_data("burston_prime", true, evo);
        let panel = crate::build::loadout::resolve(&base, mods, crate::model::StackPolicy::Emergent);
        let p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
        let s = monte_carlo(&p, 200, 0xB0A2);
        (s.mean_pellets / s.mean_shots, panel.multishot, panel.magazine_size)
    };
    let pool = crate::data::mods::class_pool("rifle");
    let split = pool.iter().find(|m| m.id == "split_chamber").expect("split chamber");
    let mods: Vec<&crate::model::ModDef> = vec![split];

    let (bare_off, _, mag) = pellets(&[], &[]);
    let (bare_on, _, _) = pellets(&["burston_prime_forceful_finality"], &[]);
    let (mod_off, multishot, _) = pellets(&[], &mods);
    let (mod_on, _, _) = pellets(&["burston_prime_forceful_finality"], &mods);
    let (bare_gain, mod_gain) = (bare_on - bare_off, mod_on - mod_off);

    // THE CLAIM IS A RATIO, and it is asserted as one. "Worth 0.333 pellets
    // a shot" would be hostage to how many whole magazines fit in the
    // engagement — 150 rounds over a 45-round magazine is three and a
    // third, and the third never reaches its last burst. What the card
    // says is that the same +5 is multiplied by the multishot bonuses, so
    // the two gains stand in exactly that ratio however many bursts landed.
    assert!(
        (mod_gain / bare_gain - multishot).abs() < 0.05,
        "a BASE grant scales with the bucket: x{:.2} of the bare gain, bucket is x{multishot:.2}              ({bare_gain:.3} -> {mod_gain:.3} pellets a shot)",
        mod_gain / bare_gain
    );
    // …and it is worth roughly the whole burst, which is what says the
    // window is three rounds rather than one.
    let per_magazine = bare_gain * mag;
    assert!(
        (11.0..=15.5).contains(&per_magazine),
        "5 pellets on each of a 3-round burst, less the magazine the fight              ends mid-way through: {per_magazine:.1} a magazine"
    );
}

/// The OTHER spelling, unchanged: the Torid's is flat, so a multishot mod
/// does not touch it. Same shape, opposite answer — which is the point.
#[test]
fn a_plain_multishot_grant_is_not_multiplied() {
    let arena = crate::arena::Arena::training(20.0);
    let pellets = |evo: &[&str], mods: &[&crate::model::ModDef]| {
        let base = crate::model::WeaponBase::from_data("torid", true, evo);
        let panel = crate::build::loadout::resolve(&base, mods, crate::model::StackPolicy::Emergent);
        let p = FightParams::from_panel(&panel, &arena, &ArcaneFx::none());
        let s = monte_carlo(&p, 200, 0xB0A2);
        (s.mean_pellets / s.mean_shots, panel.magazine_size)
    };
    let pool = crate::data::mods::class_pool("rifle");
    let split = pool.iter().find(|m| m.id == "split_chamber").expect("split chamber");
    let mods: Vec<&crate::model::ModDef> = vec![split];

    let (bare_off, mag) = pellets(&[], &[]);
    let (bare_on, _) = pellets(&["torid_final_fusillade"], &[]);
    let (mod_off, _) = pellets(&[], &mods);
    let (mod_on, _) = pellets(&["torid_final_fusillade"], &mods);
    let want = 3.0 / mag;
    assert!(
        (bare_on - bare_off - want).abs() < 0.02 && (mod_on - mod_off - want).abs() < 0.03,
        "flat either way ({want:.3}): {bare_off:.3}->{bare_on:.3}, {mod_off:.3}->{mod_on:.3}"
    );
}

#[test]
fn status_immunities_renormalize_toward_other_procs() {
    // Slash-immune target: no bleeds ever, but procs still occur at the
    // full 37% rate (renormalized onto Impact/Puncture).
    let mut p = FightParams::default();
    p.target.status_immunities = vec![DamageType::Slash];
    let s = monte_carlo(&p, 1000, 15);
    assert_eq!(s.mean_dot_damage, 0.0);
    let per_shot = s.mean_procs / s.mean_shots;
    assert!((per_shot - 0.37).abs() < 0.03, "procs/shot {per_shot}");
}

#[test]
fn non_head_weak_spot_never_triggers_headshot() {
    // MOA-fanny-pack-like: 3x location, not a head, no crit bonus.
    // Headshot effects must never fire; damage uses plain cd.
    // Per shot: E = 225*(1+cc) -> total = 2250 + 225*5.0 = 3375.
    let p = single_part(BodyPart {
        name: "fanny pack".into(),
        aim_weight: 1.0,
        multiplier: 3.0,
        is_head: false,
        crit_bonus: false,
    });
    let s = monte_carlo(&p, 2000, 11);
    assert_eq!(s.mean_headshot_rate, 0.0);
    assert!(
        (s.mean_damage - 3375.0).abs() / 3375.0 < 0.02,
        "mean damage was {}",
        s.mean_damage
    );
}

#[test]
fn helmeted_head_triggers_headshot_without_crit_bonus() {
    // Helmeted-Corpus-like: true head (triggers headshot effects) but not
    // eligible for the critical-location bonus -> same expectation as the
    // fanny pack: 3375, yet headshot rate is 100%.
    let p = single_part(BodyPart {
        name: "helmeted head".into(),
        aim_weight: 1.0,
        multiplier: 3.0,
        is_head: true,
        crit_bonus: false,
    });
    let s = monte_carlo(&p, 2000, 13);
    assert_eq!(s.mean_headshot_rate, 1.0);
    assert!(
        (s.mean_damage - 3375.0).abs() / 3375.0 < 0.02,
        "mean damage was {}",
        s.mean_damage
    );
}

#[test]
fn one_x_location_gets_no_crit_bonus_even_if_flagged() {
    // Charger-mouth-like: 1x, not a head. Even with crit_bonus set, a 1x
    // location never receives the critical-location bonus.
    // Per shot: E = 75*(1+cc) -> total = 750 + 75*5.0 = 1125.
    let p = single_part(BodyPart {
        name: "mouth".into(),
        aim_weight: 1.0,
        multiplier: 1.0,
        is_head: false,
        crit_bonus: true,
    });
    let s = monte_carlo(&p, 2000, 17);
    assert!(
        (s.mean_damage - 1125.0).abs() / 1125.0 < 0.02,
        "mean damage was {}",
        s.mean_damage
    );
}

/// THE OCUCOR'S TENDRILS, through Sentient Surge: every kill is worth
/// crit chance and status chance, and a RELOAD takes all of it away.
///
/// The reload half is the one worth pinning. "Tendrils disappear upon
/// reloading or emptying the magazine", and the mod's own page repeats it
/// — "Reloading the weapon will reset the bonuses, as they are directly
/// tied to its tendril effect which resets on reload." A model that let
/// the stacks ride through a reload would look right in every reading that
/// never reloads, which is exactly the reading a short test does.
///
/// The tendrils' own DAMAGE is deliberately absent and is not tested here,
/// because there is nothing to test: on the beam's own target a tendril is
/// cosmetic (wiki), and this arena has no second target.
#[test]
fn tendrils_buy_crit_chance_and_a_reload_takes_it_back() {
    // Base crit 0, so EVERY crit in the result came from a tendril: the
    // measurement cannot be diluted by a base the weapon already had.
    let build = |mag: f64, per_tendril: f64| FightParams {
        base_crit_chance: 0.0,
        unmodded_crit_chance: 1.0, // the base a relative bonus multiplies
        crit_multiplier: 2.0,
        tendril_max: 4,
        crit_chance_per_tendril: per_tendril,
        magazine_size: mag,
        reload_seconds: 0.5,
        fire_rate: 10.0,
        duration_seconds: 60.0,
        body_parts: mono_body(1.0),
        target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
        // flat_base(), not no_status(): the default fixture carries an
        // arcane that crits on its own, which would supply the very thing
        // this test is trying to attribute to tendrils.
        ..flat_base()
    };
    let crit_rate = |p: &FightParams| {
        let r = run_once(p, &mut Rng::new(7));
        assert!(r.kills > 0, "the fixture must actually kill something");
        r.crits as f64 / r.pellets.max(1) as f64
    };

    // A magazine that never runs out: kills accumulate tendrils and the
    // crit chance climbs to the cap.
    let deep = crit_rate(&build(100_000.0, 0.25));
    assert!(deep > 0.5, "tendrils should be carrying the crit rate, got {deep}");

    // OFF, same fixture: nothing but the zero base is left.
    let none = crit_rate(&build(100_000.0, 0.0));
    assert!(none < 1e-9, "no tendril bonus means no crits at all, got {none}");

    // A ONE-ROUND magazine reloads after every shot, so no tendril ever
    // survives to the next pull. Same kills, same bonus, no benefit.
    let shallow = crit_rate(&build(1.0, 0.25));
    assert!(
        shallow < deep / 2.0,
        "a reload must clear the tendrils: deep magazine {deep}, one-round {shallow}"
    );
}

/// …AND THE FIRE BURNS IN A REAL FIGHT, which is the assertion the other
/// two cannot make: a field that resolves correctly and never spawns would
/// pass both of them.
#[test]
fn the_napalm_burns_and_only_the_base_damage_bucket_feeds_it() {
    let pool = crate::data::mods::pool_for_build("ogris", &[]);
    let by = |id: &str| pool.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{id}"));
    let base = crate::model::WeaponBase::from_data("ogris", false, &[]);
    // The FIELD's own share of the run, off `RunResult::sources` — a total
    // would answer with the rocket's damage and hide the fire inside it.
    let field_damage = |mods: &[&_]| {
        let panel = crate::build::loadout::resolve(&base, mods, crate::model::StackPolicy::Emergent);
        let arena = crate::arena::Arena::training(10.0);
        let p = FightParams::from_panel(
            &panel, &arena, &crate::data::arcanes::ArcaneFx::none(),
        );
        let mut total = 0.0;
        for seed in 0..5u64 {
            total += run_once(&p, &mut Rng::new(7 + seed)).sources.field;
        }
        total / 5.0
    };
    let none = field_damage(&[]);
    assert_eq!(none, 0.0, "the Ogris leaves no fire without the augment");
    let with = field_damage(&[by("nightwatch_napalm")]);
    assert!(with > 0.0, "and burns with it: {with}");
    // BASE DAMAGE FEEDS THE FIRE…
    let breakdown = field_damage(&[by("nightwatch_napalm"), by("serration")]);
    assert!(
        breakdown > with * 1.5,
        "Serration reaches it: {breakdown} against {with}"
    );
    // …AND AN ELEMENT DOES NOT, in the fight and not just on the panel.
    let el = field_damage(&[by("nightwatch_napalm"), by("cryo_rounds")]);
    assert!(
        (el - with).abs() < with * 1e-6,
        "a Cold mod moves the rocket and not the fire: {el} against {with}"
    );
}

/// ACID SHELLS: a corpse is an epicentre, and the blast CHAINS.
///
/// Three claims, and the third is the one that cannot be arranged: the
/// explosion is queued on the body that DIED, so a body it kills queues its
/// own on the way out — "a chain reaction that can lead to very large areas
/// being cleared from a single shot". Nothing enumerates the chain.
#[test]
fn acid_shells_explodes_a_corpse_and_the_blast_chains() {
    let pool = crate::data::mods::pool_for_build("sobek", &[]);
    let acid = pool.iter().find(|m| m.id == "acid_shells").expect("the augment");
    let base = crate::model::WeaponBase::from_data("sobek", false, &[]);
    // A LINE OF FRAIL BODIES two metres apart, well inside the 15 m reach:
    // one killed by the gun, the rest reachable only by the chain.
    let fight = |mods: &[&_]| {
        let panel = crate::build::loadout::resolve(&base, mods, crate::model::StackPolicy::Emergent);
        let mut arena = crate::arena::Arena::training(6.0);
        // FRAIL, and deliberately: 50 health is under a single Corrosive
        // 450, so what the chain does is visible as KILLS rather than as a
        // damage total that a bigger direct hit could also explain.
        arena.target = frail_target(TargetMode::InstantRespawn, 0.0, 0.0);
        arena.others = (1..8)
            .map(|i| crate::formation::FoeSpec {
                id: format!("e{}", i + 1),
                params: arena.target.clone(),
                body_parts: arena.body_parts.clone(),
                at: crate::rules::space::Vec2::new(
                    0.0,
                    crate::rules::space::CONTACT_RANGE_M + f64::from(i) * 2.0,
                ),
            })
            .collect();
        let p = FightParams::from_panel(&panel, &arena, &crate::data::arcanes::ArcaneFx::none());
        monte_carlo(&p, 3, 5)
    };
    let without = fight(&[]);
    let with = fight(&[acid]);
    assert!(
        with.mean_kills > without.mean_kills,
        "the corpses have to kill: {} against {}",
        with.mean_kills, without.mean_kills
    );
}

/// HATA-SATYA: the pile is built by HITS and a RELOAD takes it back.
///
/// The same two-magazine measurement the tendrils get one test up, with the
/// counter swapped — and it has to be a measurement rather than a stack
/// count, because the mod's whole shape is that it never reaches its own
/// ceiling in a short magazine. 416 stacks at max rank is a number a Soma
/// Prime with multishot reaches once per magazine and never again.
#[test]
fn hata_satya_builds_on_hits_and_a_reload_takes_it_back() {
    // Base crit 0, so every crit in the result came from the pile.
    let build = |mag: f64, per_hit: f64| FightParams {
        base_crit_chance: 0.0,
        unmodded_crit_chance: 1.0, // the base a relative bonus multiplies
        crit_multiplier: 2.0,
        crit_chance_per_hit: Some(crate::model::CritPerHit {
            per_stack: per_hit,
            max_bonus: 5.0,
        }),
        magazine_size: mag,
        reload_seconds: 0.5,
        fire_rate: 10.0,
        duration_seconds: 60.0,
        body_parts: mono_body(1.0),
        target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
        ..flat_base()
    };
    let crit_rate = |p: &FightParams| {
        let r = run_once(p, &mut Rng::new(7));
        r.crits as f64 / r.pellets.max(1) as f64
    };

    // A magazine that never runs out: the pile climbs all run.
    let deep = crit_rate(&build(100_000.0, 0.01));
    assert!(deep > 0.5, "the pile should be carrying the crit rate, got {deep}");

    // OFF, same fixture: the zero base is all that is left.
    let none = crit_rate(&build(100_000.0, 0.0));
    assert!(none < 1e-9, "no per-hit bonus means no crits at all, got {none}");

    // A ONE-ROUND magazine reloads after every shot, so no stack survives
    // to the next pull — the first hit of each magazine is worth nothing
    // (the pile is read BEFORE the shot) and the second never comes.
    let shallow = crit_rate(&build(1.0, 0.01));
    assert!(
        shallow < deep / 2.0,
        "a reload must clear the pile: deep magazine {deep}, one-round {shallow}"
    );
}
