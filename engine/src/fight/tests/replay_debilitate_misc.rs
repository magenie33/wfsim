use super::*;

/// Past 100% crit chance the RATE stops saying anything — every pellet
/// crits, so it reads 1.0 whether the build is at 110% or 410%. The mean
/// TIER is the number that keeps going, and it is the one that
/// multiplies the damage.
///
/// Red is NOT the ceiling: tier 4 and above are real, the game shows
/// them, and `crit_multiplier = 1 + tier x (cd - 1)` has no cap either — so
/// neither may the report.
#[test]
fn the_crit_tier_keeps_climbing_where_the_rate_saturates() {
    let at = |cc: f64| {
        let p = FightParams {
            base_crit_chance: cc,
            // Measure the ROLL, not a promotion or a stacking arcane.
            crit_tier_upgrade_chance: 0.0,
            arcane: crate::data::arcanes::ArcaneFx::none(),
            ..Default::default()
        };
        monte_carlo(&p, 400, 11)
    };
    // Below 100% the two are the SAME number — the tier is not a second
    // opinion, it is the rate without the >= 1 truncation.
    let half = at(0.5);
    assert!(
        (half.mean_crit_tier - half.mean_crit_rate).abs() < 1e-9,
        "below 100% they must agree: tier {} vs rate {}",
        half.mean_crit_tier,
        half.mean_crit_rate
    );

    // At 100% and beyond the rate is pinned at 1.0 and only the tier moves.
    let one = at(1.0);
    let past_red = at(4.2);
    assert!((one.mean_crit_rate - 1.0).abs() < 1e-9);
    assert!((past_red.mean_crit_rate - 1.0).abs() < 1e-9, "the rate saturates");
    assert!(
        past_red.mean_crit_tier > 4.0,
        "above RED and still counting: {}",
        past_red.mean_crit_tier
    );
    assert!(past_red.mean_crit_tier > one.mean_crit_tier + 3.0);
}

/// EVERY on-status trigger the data can express must actually be fired by
/// the sim. Toxin, Electricity and Heat were wired and COLD was not, so
/// Primary Frostbite spent one duration at its seeded stack count and then
/// sat at zero for the rest of every run — it looked implemented, listed
/// in the picker, and quietly did nothing after twelve seconds.
///
/// The check is mechanical rather than by name: it asserts each variant
/// appears in a `bump_trigger` call in the fight's source (every file that
/// calls it is listed). A new on-status arcane cannot be added without wiring it.
#[test]
fn every_on_status_trigger_is_fired_somewhere() {
    let src = [include_str!("../arcane.rs"), include_str!("../procs.rs"), include_str!("../run.rs")].concat();
    for name in ["ToxinStatus", "ElectricityStatus", "HeatStatus", "ColdStatus"] {
        let needle = format!("ArcTrigger::{name}");
        let fired = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .any(|l| l.contains("bump_trigger") && l.contains(&needle));
        assert!(fired, "ArcTrigger::{name} is never bumped — an arcane that \
            waits on it can never earn a stack");
    }
}

/// PUNCTURE'S WEAKENED DOES NOT REACH AN EXPLOSION.
///
/// Wiki (Damage/Puncture_Damage): "Weapon damage that the victim receives
/// has 5% increased Critical Chance per proc up to 25% at max stacks …
/// This is a flat critical chance buff (like Arcane Avenger), but does not
/// apply to Area of Effect damage or Warframe abilities."
///
/// The radial had its own copy of the crit line and that copy added the
/// buff, so a build that stacked Puncture crit its explosion up to 25% of
/// the time for free. The fixture makes that visible rather than statistical:
/// the explosion has ZERO crit chance of its own, so any crit at all can
/// only have come from Weakened.
#[test]
fn weakened_never_crits_an_explosion() {
    let mut p = radial_only(radial_of(0.0, 0.0));
    // The direct hit forces Puncture every shot, so Weakened saturates.
    p.forced_procs = vec![DamageType::Puncture];
    p.status_chance = 1.0;
    p.base_status_chance = 1.0;
    let mut damage = DamageVector::default();
    damage.set(DamageType::Puncture, 100.0);
    p.damage = damage;
    p.magazine_size = 200.0;
    p.reload_seconds = 0.1;
    let s = monte_carlo(&p, 8, 11);
    // The DIRECT hit is allowed to crit off Weakened — it is weapon damage
    // the victim receives, which is exactly what the buff is for. Only the
    // explosion is excluded, so the explosion's own bucket is the
    // assertion: with zero crit chance it must be shots x its flat base,
    // and any crit at all would show as more than that.
    let expected = s.mean_shots * 300.0;
    assert!(
        (s.source_damage.radial - expected).abs() < 1e-6,
        "explosion dealt {} where a never-critting one deals {} — Weakened              reached the AoE",
        s.source_damage.radial,
        expected
    );
    assert!(s.median_run.crits > 0, "the DIRECT hit should still crit off Weakened");
}

/// LOCKED MEANS "NO TIMEOUT", NOT "FROZEN".
///
/// The old reading froze a locked buff at whatever count it was configured
/// with, so locking one at a partial count also stopped it EARNING — and
/// locking one at zero meant it could never turn on at all, which is
/// exactly what a visitor assumed the control did. It now only removes the
/// expiry: the count starts where it was set and still climbs.
///
/// A killable target and an on-kill stacking arcane make it arithmetic:
/// seeded at ONE of three stacks and locked, the buff must end the run
/// worth more than one stack.
#[test]
fn a_locked_buff_still_earns_stacks() {
    use crate::data::arcanes::ArcBuffSpec;
use crate::model::ArcTrigger;
use crate::model::ArcGrant;
    let mk = |initial: u32| {
        let mut damage = DamageVector::default();
        damage.set(DamageType::Impact, 100.0);
        FightParams {
            damage,
            // Dies to every shot and comes straight back, so the on-kill
            // trigger fires on a schedule instead of by luck.
            target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
            arcane: ArcaneFx {
                buffs: vec![ArcBuffSpec {
                    owner: "test".into(),
                    grant: ArcGrant::BaseDamage,
                    trigger: ArcTrigger::Kill,
                    per_stack: 1.0,          // +100% of base per stack
                    max_stacks: 3,
                    // LOCKED — which, since 2026-08-04, IS a duration.
                    duration: crate::model::NO_TIMEOUT,
                    all_drop: false,
                    one_per_instance: false,
                    initial_stacks: initial,
                }],
                ..ArcaneFx::none()
            },
            magazine_size: 60.0,
            reload_seconds: 0.1,
            base_crit_chance: 0.0,
            crit_multiplier: 1.0,
            ..no_status()
        }
    };
    let one = monte_carlo(&mk(1), 6, 5);
    let full = monte_carlo(&mk(3), 6, 5);
    // Frozen at one stack it would never approach the seeded-full run.
    assert!(
        one.mean_damage > 0.9 * full.mean_damage,
        "locked at 1 stack stayed at 1: {} vs {}",
        one.mean_damage,
        full.mean_damage
    );
}

/// ONE ARCANE IS ONE CARD, so one setting must reach every spec it owns.
///
/// Frostbite grants crit damage AND multishot off the same Cold proc and
/// they are the same count by construction — there is no state of the game
/// where one is at 1 and the other at 10. The card was per GRANT, which
/// offered a setting that cannot exist and keyed each half separately;
/// configuring the arcane now has to move both halves or the merge is a
/// display trick over a split model.
#[test]
fn one_config_reaches_every_grant_of_its_arcane() {
    use crate::data::arcanes::ArcBuffSpec;
use crate::model::ArcTrigger;
use crate::model::ArcGrant;
    let spec = |grant: ArcGrant| ArcBuffSpec {
        owner: "primary_frostbite".into(),
        grant,
        trigger: ArcTrigger::ColdStatus,
        per_stack: 0.03,
        max_stacks: 40,
        duration: 12.0,
        all_drop: true,
        one_per_instance: false,
        initial_stacks: 40,
    };
    let mut p = FightParams {
        arcane: ArcaneFx {
            buffs: vec![spec(ArcGrant::CritDamage), spec(ArcGrant::Multishot)],
            ..ArcaneFx::none()
        },
        ..FightParams::default()
    };
    let mut cfg = BuffConfig::new();
    cfg.insert("arcane:primary_frostbite".into(), (7, true));
    p.apply_buff_config(&cfg);
    for b in &p.arcane.buffs {
        assert_eq!(b.initial_stacks, 7, "{:?} kept its own count", b.grant);
        assert_eq!(
            b.duration,
            crate::model::NO_TIMEOUT,
            "{:?} took the config's lock",
            b.grant
        );
    }
}

/// A COLD PROC ON A FROZEN TARGET IS INERT, so it stacks nothing.
///
/// `data/debuffs/frozen.yaml` is explicit — `refreshable: false`, "cannot
/// be extended: Cold procs are inert" — and `apply_cold_proc` has always
/// honoured it for the debuff. Primary Frostbite did not: the trigger was
/// bumped BEFORE the proc and unconditionally, so an arcane whose card
/// reads "On Cold Status Effect" earned a stack from a proc that applied
/// no status.
///
/// It hid well. Frozen lasts 3 s against the arcane's 12 s all-drop timer,
/// so it never looked like the buff going dark — it looked like the buff
/// never quite decaying, in exactly the fight (heavy Cold, one target)
/// where you would credit the arcane for it.
#[test]
fn a_cold_proc_on_a_frozen_target_stacks_nothing() {
    // Ten procs, one per shot, on a target with no overguard and no
    // per-unit caps: the 10th freezes it, and everything after is inert
    // until Frozen expires.
    let mut d = DebuffState::default();
    let mut applied = 0;
    for i in 0..10 {
        if d.apply_cold_proc(i as f64 * 0.1, 1.0, false, None, false) {
            applied += 1;
        }
    }
    assert_eq!(applied, 10, "nine stacks, then the tenth converts to Frozen");
    assert!(d.frozen_until.is_some_and(|f| f > 1.0), "it is Frozen");
    // While Frozen: every further proc reports FALSE. That is the whole
    // fix — the caller stacks the arcane off this answer, not off the
    // attempt.
    for i in 0..5 {
        assert!(
            !d.apply_cold_proc(1.0 + i as f64 * 0.1, 1.0, false, None, false),
            "a proc during Frozen applied a status"
        );
    }
    // ...and once it thaws, procs land again.
    assert!(d.apply_cold_proc(1.0 + FROZEN_DURATION + 0.01, 1.0, false, None, false));

    // A CAPPED list is not the same case: pushing past a cap replaces the
    // oldest, which IS an application, so the arcane keeps stacking there.
    let mut og = DebuffState::default();
    for i in 0..8 {
        assert!(
            og.apply_cold_proc(i as f64 * 0.1, 1.0, true, None, false),
            "under overguard the list caps at 4 but every proc still lands"
        );
    }
    assert_eq!(og.freeze.len(), FREEZE_CAP_UNDER_OVERGUARD);
    assert!(og.frozen_until.is_none(), "an overguard holder never freezes");
}

/// A REPLAY IS THE SAME FIGHT, not a re-roll of it: `Rng` is SplitMix64
/// with one `u64` of state, a run records what it started from, and
/// replaying from that number reproduces it exactly. If that stops being
/// true the panel silently shows a DIFFERENT engagement than the one whose
/// number is on screen, which is worse than having no replay.
/// **THE REPLAY COVERS THE WHOLE FIGHT, EVEN AFTER THE AMMO RUNS OUT.**
///
/// The firing loop `break`s the instant a finite reserve is dry, and the
/// sampler must NOT stop with it: a 180-second engagement that ran out at
/// 58.5 would be drawn as a 58.5-second one, and every rate the replay
/// derives divides by its own last frame.
///
/// Not only the divisor — the two `process_*` calls after the loop settle
/// the burning clouds and remaining DoTs to the end, so damage and KILLS
/// land after the last shot. Asserted on the two things a reader reads: the
/// clock reaches the end, and the last frame's totals are the run's own.
#[test]
fn a_replay_runs_to_the_end_even_when_the_ammo_does_not() {
    // A tiny reserve against a target that cannot be finished: the gun is
    // dry long before the engagement is.
    let p = FightParams {
        duration_seconds: 60.0,
        infinite_reserve: false,
        reserve_ammo: 20.0,
        magazine_size: 10.0,
        ..flat_base()
    };
    let s = monte_carlo(&p, 8, 7);
    let rep = replay(&p, s.median_run.rng_state, 60);

    // THE GUN REALLY DOES RUN DRY — without this the test passes on a
    // fight that never had the problem, which is how a guard goes quiet.
    assert!(
        s.median_run.pellets as f64 <= 40.0,
        "the fixture must run out of ammo, fired {} pellets",
        s.median_run.pellets
    );
    assert_eq!(rep.frames.len(), 60, "every slot filled, to the end of the fight");
    let last = rep.frames.last().unwrap();
    assert!(
        last.t >= p.duration_seconds - rep.frame_seconds - 1e-9,
        "the replay stops at {} of a {} s fight",
        last.t,
        p.duration_seconds
    );
    // THE TOTALS ARE THE RUN'S. A rate read off the last frame has to agree
    // with the summary beside it, which is the bug this exists for.
    assert_eq!(last.kills, s.median_run.kills, "kills after the last shot are lost");
    assert!(
        (last.damage - s.median_run.effective_damage()).abs() <= 1e-6,
        "{} vs {}",
        last.damage,
        s.median_run.effective_damage()
    );
    // …and the clock never goes backwards over the join.
    for w in rep.frames.windows(2) {
        assert!(w[1].t > w[0].t, "frames out of order at {}", w[0].t);
    }
}

/// FOLLOWING A BODY ON THE SECOND ASKING GIVES THE SERIES IT WOULD HAVE HAD
/// ON THE FIRST — the whole of on-demand tracking, and the only thing that
/// makes a cap cost a reader nothing but a wait.
///
/// A REPLAY FOLLOWS EIGHT because a series is 18 KB and a 19x19 ruler would
/// be 6.5 MB of them. That is a wire budget and not an answer, so the ninth
/// body is one engagement away — re-run from the state the fight started
/// at, which is the same property the scout run already leans on, read the
/// other way round.
#[test]
fn a_body_followed_on_asking_gets_the_series_it_would_have_had() {
    // CRIT AND STATUS ON PURPOSE. A fixture with neither is a DETERMINISTIC
    // fight, and a deterministic fight agrees with itself under any seed —
    // which would make "the same run" an assertion about nothing.
    let mut p = FightParams {
        base_crit_chance: 0.5,
        crit_multiplier: 2.0,
        status_chance: 0.6,
        base_status_chance: 0.6,
        body_parts: mono_body(1.0),
        arcane: ArcaneFx::none(),
        ..FightParams::default()
    };
    p.duration_seconds = 12.0;
    // A LINE THE SHOT PUNCHES THROUGH, which is the cheapest way to make a
    // crowd take damage: every body on it is struck, so the replay has more
    // than the aimed one to rank.
    p.punch_through_m = 20.0;
    p.player_at = crate::rules::space::Vec2::ORIGIN;
    p.target_at = crate::rules::space::Vec2::new(0.0, 2.0);
    let (x, y) = (p.target_at.x, p.target_at.y);
    p.others = (1..=12)
        .map(|i| crate::formation::FoeSpec {
            id: format!("e{}", i + 1),
            params: TargetParams::training_dummy(),
            body_parts: BodyPart::humanoid(),
            at: crate::rules::space::Vec2::new(x, y + f64::from(i) * 0.6),
        })
        .collect();
    let state = Rng::new(0x5EED).state();

    let all = replay(&p, state, 40);
    assert!(all.tracked.len() > 1, "the fixture has to reach a crowd: {:?}", all.tracked);
    // ASK FOR ONE THE DEFAULT ALREADY FOLLOWED, which is the only pairing
    // that can be compared at all — and the claim is that asking changes
    // nothing about what comes back.
    let want = all.follow[1];
    let one = replay_following(&p, state, 40, &[want]);
    assert_eq!(one.tracked, vec![all.tracked[0].clone(), all.tracked[1].clone()]);
    assert_eq!(one.frames.len(), all.frames.len());
    for (a, b) in one.frames.iter().zip(all.frames.iter()) {
        assert_eq!(a.debuffs[1], b.debuffs[1], "the asked body's series moved");
        assert_eq!(a.damage.to_bits(), b.damage.to_bits(), "a different fight");
    }
}

/// THE AIMED BODY IS ALWAYS FIRST, whatever was asked for, and asking twice
/// for one body does not follow it twice.
#[test]
fn asking_never_drops_the_aimed_body_or_repeats_one() {
    let mut p = flat_base();
    p.duration_seconds = 6.0;
    p.others = (1..=3)
        .map(|i| crate::formation::FoeSpec {
            id: format!("e{}", i + 1),
            params: TargetParams::training_dummy(),
            body_parts: BodyPart::humanoid(),
            at: crate::rules::space::Vec2::new(f64::from(i) * 0.6, p.target_at.y),
        })
        .collect();
    let rep = replay_following(&p, 7, 8, &[2, 2, 0, 99]);
    assert_eq!(rep.follow, vec![0, 2], "aimed first, asked once, nonsense dropped");
}

#[test]
fn a_replay_reproduces_the_run_it_came_from() {
    let p = FightParams {
        arcane: arc_stacked("secondary_merciless"),
        duration_seconds: 30.0,
        ..flat_base()
    };
    let s = monte_carlo(&p, 12, 99);
    let rep = replay(&p, s.median_run.rng_state, 60);
    assert_eq!(rep.frames.len(), 60, "one frame per slot, gaps filled");
    assert!((rep.frame_seconds - 0.5).abs() < 1e-9, "30 s over 60 frames");

    // Re-running from the same state gives the identical RunResult.
    let again = run_once(&p, &mut Rng::new(s.median_run.rng_state));
    assert_eq!(again.total_damage().to_bits(), s.median_run.total_damage().to_bits());
    assert_eq!(again.pellets, s.median_run.pellets);
    assert_eq!(again.crits, s.median_run.crits);

    // The last frame's cumulative damage is the run's own effective total
    // minus whatever landed after the final sample — never more.
    let last = rep.frames.last().unwrap();
    assert!(last.damage <= s.median_run.effective_damage() + 1e-6);
    assert!(last.damage > 0.0, "something happened");
    // Frames advance in time and never go backwards.
    for w in rep.frames.windows(2) {
        assert!(w[1].t > w[0].t);
        assert!(w[1].damage >= w[0].damage, "cumulative damage cannot fall");
    }
}

/// The roster names every buff the sampler can answer for, and the
/// sampler answers for every buff the roster names. A rostered buff the
/// sampler does not know would draw a flat zero and read as a finding.
#[test]
fn every_rostered_buff_is_sampled() {
    let p = FightParams {
        arcane: arc_stacked("secondary_merciless"),
        duration_seconds: 10.0,
        ..FightParams::default()
    };
    let roster = p.buff_roster();
    assert!(!roster.is_empty(), "this fixture carries buffs");
    let rep = replay(&p, 12345, 20);
    assert_eq!(rep.buffs, roster);
    for f in &rep.frames {
        assert_eq!(f.stacks.len(), roster.len(), "one sample per rostered buff");
    }
    // Merciless was seeded full, so its series starts at its cap rather
    // than at the zero an unknown id would produce.
    let at = roster.iter().position(|b| b.id.starts_with("arcane:")).expect("an arcane");
    assert_eq!(u32::from(rep.frames[0].stacks[at]), roster[at].max_stacks);
}

/// THE FACTION LADDER, which is the whole reason Primary Debilitate is
/// worth more than its card reads. Each derivation step re-applies the
/// bonus, and 3 is never written down — it falls out of the depth.
#[test]
fn faction_compounds_once_per_derivation_step() {
    let f = 1.55; // +55% faction, the wiki's worked example
    assert!((faction_at(f, DEPTH_HIT) - 1.55).abs() < 1e-9);
    assert!((faction_at(f, DEPTH_PROC) - 2.4025).abs() < 1e-9);
    assert!((faction_at(f, DEPTH_DERIVED_PROC) - 3.723875).abs() < 1e-6);

    // The wiki's own numbers for a 100-base melee with +90% Electricity:
    // hit 294, its Electricity proc 228, a SPREAD Electricity proc 353.
    let hit = (100.0 + 90.0) * faction_at(f, DEPTH_HIT);
    let proc = 0.5 * (100.0 + 90.0) * faction_at(f, DEPTH_PROC);
    let spread = 0.5 * (100.0 + 90.0) * faction_at(f, DEPTH_DERIVED_PROC);
    assert!((hit - 294.5).abs() < 0.5, "{hit}");
    assert!((proc - 228.2).abs() < 0.5, "{proc}");
    assert!((spread - 353.8).abs() < 0.5, "{spread}");
}

/// PRIMARY DEBILITATE'S DECISION, pinned without a fight.
///
/// The argument is the count the target is AT, this application included —
/// so `DEBILITATE_STACKS` here is the ninth stack plus the tenth being
/// applied, and that is the shot that splits.
#[test]
fn debilitate_splits_only_a_saturated_combination() {
    let mut rng = Rng::new(0xD0D0);
    // Below the threshold: never, whatever the roll would have been.
    for stacks in 0..DEBILITATE_STACKS {
        assert_eq!(
            debilitate_split(DamageType::Corrosive, stacks, 1.0, &mut rng),
            None,
            "{stacks} stacks is under the bar"
        );
    }
    // At it, with certainty, it always splits — and only into a COMPONENT.
    // ALL SIX combinations, not just the one the reports come in about:
    // Blast is Cold and Heat, and a table that
    // answered for five of six would be wrong in exactly the way nobody
    // checks.
    for combined in [
        DamageType::Corrosive,
        DamageType::Blast,
        DamageType::Viral,
        DamageType::Magnetic,
        DamageType::Radiation,
        DamageType::Gas,
    ] {
        let (a, b) = crate::rules::elements::components_of(combined).expect("a combination");
        for _ in 0..64 {
            let got = debilitate_split(combined, DEBILITATE_STACKS, 1.0, &mut rng)
                .expect("certain at rank 5");
            assert!(
                got == a || got == b,
                "{combined:?} splits into {a:?}/{b:?}, got {got:?}"
            );
        }
    }
    // A PRIMARY has nothing to split into, saturated or not.
    assert_eq!(debilitate_split(DamageType::Heat, 99, 1.0, &mut rng), None);
    assert_eq!(debilitate_split(DamageType::Slash, 99, 1.0, &mut rng), None);
    // No arcane, no split.
    assert_eq!(debilitate_split(DamageType::Viral, 99, 0.0, &mut rng), None);
}

/// THE TENTH APPLICATION SPLITS, and the ninth does not — end to end, on an
/// ordinary combination where the count is observable.
///
/// This is the rule the Blast case turned out to be an instance of. Asserted by CAPPING the fight at nine applications and at
/// ten: nine must pay nothing at all, ten must pay. A test that only
/// asserted "ten splits" would pass just as well under the old
/// already-holds-ten reading, which fires on the eleventh.
#[test]
fn the_tenth_application_is_the_one_that_splits() {
    // Pure Viral, forced, one proc per shot, and exactly `shots` of them:
    // the stack count after shot n is n, capped at ten.
    //
    // THE AMMO is what bounds the shot count, not the clock. Bounding it by
    // duration was the first attempt and it silently asserted nothing: the
    // tenth shot lands at the last instant of the fight and its DoT ticks
    // start a second AFTER the end, so both arms read zero and "nine splits
    // nothing" passed for the wrong reason.
    let run = |shots: f64, chance: f64| {
        let p = FightParams {
            damage: DamageVector::new().with(DamageType::Viral, 100.0),
            dot_modified_base: Some(100.0),
            status_chance: 0.0,
            base_status_chance: 0.0,
            forced_procs: vec![DamageType::Viral],
            fire_rate: 10.0,
            // Long enough for the last shot's DoT to tick out in full.
            duration_seconds: 20.0,
            magazine_size: shots,
            infinite_reserve: false,
            reserve_ammo: 0.0,
            base_crit_chance: 0.0,
            unmodded_crit_chance: 0.0,
            target: TargetParams { base_health: 1e15, ..FightParams::default().target },
            // Viral splits into Cold and Toxin; only Toxin ticks, so a
            // split that lands shows up as DoT damage and nothing else can
            // put damage in that bucket.
            elem_dot_bonus: vec![(DamageType::Toxin, 1.0)],
            arcane: crate::data::arcanes::ArcaneFx {
                debilitate_chance: chance,
                ..crate::data::arcanes::ArcaneFx::none()
            },
            ..FightParams::default()
        };
        monte_carlo(&p, 200, 0x51CE).mean_dot_damage
    };
    // NINE applications: the target never reaches ten, so nothing splits —
    // and Viral's own proc is a multiplier, not a DoT, so the bucket is
    // empty for a reason that cannot be anything else.
    assert_eq!(run(9.0, 1.0), 0.0, "nine applications must split nothing");
    // TEN: the tenth is the one, and at rank 5 it is certain.
    assert!(run(10.0, 1.0) > 0.0, "the tenth application must split");
    // …and it is the ARCANE doing it: same fight, no arcane, no DoT.
    assert_eq!(run(10.0, 0.0), 0.0);
}

/// BLAST REACHES THE THRESHOLD, END TO END — the table above says it may
/// split, and this says the sim ever gets it there.
///
/// It is the case where the threshold rule is the whole mechanic rather
/// than one shot: Blast DETONATES at ten and drains every stack, so the
/// count a later application reads is 0..=9 forever. Reading the
/// pre-application count made the arcane dead on Blast, silently, with a
/// passing unit test on the split function itself. Only a run finds that.
#[test]
fn a_blast_build_actually_reaches_the_debilitate_threshold() {
    let build = |chance: f64| FightParams {
        damage: DamageVector::new().with(DamageType::Blast, 100.0),
        status_chance: 1.0,
        base_status_chance: 1.0,
        fire_rate: 10.0,
        magazine_size: 1e9,
        infinite_reserve: true,
        // Heat is the half of Blast that ticks, so a split that lands is
        // visible as damage; Cold's half is a slow and adds none.
        elem_dot_bonus: vec![(DamageType::Heat, 2.0), (DamageType::Cold, 2.0)],
        arcane: crate::data::arcanes::ArcaneFx {
            debilitate_chance: chance,
            ..crate::data::arcanes::ArcaneFx::none()
        },
        ..FightParams::default()
    };
    const RUNS: u32 = 200;
    const SEED: u64 = 0xB1A5;
    let added = monte_carlo(&build(1.0), RUNS, SEED).mean_damage
        - monte_carlo(&build(0.0), RUNS, SEED).mean_damage;
    assert!(
        added > 0.0,
        "a saturating Blast build must split at least once; added {added}"
    );
}

/// ...and the two components come up about evenly (50/50, per the owner).
#[test]
fn debilitate_picks_either_component_evenly() {
    let mut rng = Rng::new(0x5EED);
    let mut tox = 0;
    const N: usize = 4000;
    for _ in 0..N {
        match debilitate_split(DamageType::Corrosive, DEBILITATE_STACKS, 1.0, &mut rng) {
            Some(DamageType::Toxin) => tox += 1,
            Some(DamageType::Electricity) => {}
            other => panic!("unexpected {other:?}"),
        }
    }
    let share = tox as f64 / N as f64;
    assert!((share - 0.5).abs() < 0.05, "toxin share {share}, want ~0.5");
}

/// A SPLIT PROC TAKES ITS OWN ELEMENT'S BRACKET, which is the whole reason
/// the split is implemented as an ordinary proc one depth down rather than
/// as a bespoke damage formula. Corrosive is Electricity + Toxin: a build
/// carrying a Toxin mod and no Electricity mod scales a split TOXIN tick by
/// the Toxin bonus and a split ELECTRICITY tick by 1.0 — "otherwise you
/// only get the base portion".
#[test]
fn a_split_proc_scales_by_its_own_elements_mods() {
    // +90% Toxin and no Electricity mod at all.
    let p = FightParams {
        elem_dot_bonus: vec![(DamageType::Toxin, 1.9)],
        ..Default::default()
    };
    assert!((p.elem_bracket(DamageType::Toxin) - 1.9).abs() < 1e-9);
    assert!(
        (p.elem_bracket(DamageType::Electricity) - 1.0).abs() < 1e-9,
        "an element with no mod contributes nothing to the bracket"
    );
    // ...and the combined element's own bracket is neither of them, so
    // reusing it for the split would be wrong in both directions.
    assert!((p.elem_bracket(DamageType::Corrosive) - 1.0).abs() < 1e-9);
}

/// PRIMARY DEBILITATE, END TO END — and the test MEASURES the faction
/// exponent rather than asserting the constant the code was written from.
///
/// The split procs are the only difference between the arcane on and off,
/// so the damage they ADD is isolated by subtraction. If those procs sit at
/// depth 3, that added damage scales by f³ when the faction bonus changes —
/// while everything else in the fight scales by f or f². Dividing the added
/// damage at two faction values therefore reads the exponent straight out
/// of the simulation:
///
///     added(f) / added(1) == f³
///
/// This is the assertion the wiki's "three separate times" earns. It also
/// proves the WIRING, not just the arithmetic — `faction_at` was already
/// tested on its own, and a recursion that never fired would pass that and
/// fail this.
#[test]
fn a_debilitate_split_lands_at_the_third_faction_layer() {
    // A pure-Corrosive weapon that procs constantly: every proc is the same
    // combined type, so the target saturates and stays saturated.
    let base = |faction: f64, chance: f64| FightParams {
        damage: DamageVector::new().with(DamageType::Corrosive, 100.0),
        status_chance: 1.0,
        base_status_chance: 1.0,
        fire_rate: 10.0,
        magazine_size: 1e9,
        infinite_reserve: true,
        faction_multiplier: faction,
        // The component bracket is what a split tick scales by; give Toxin
        // and Electricity real mod bonuses so the split has something to
        // read and the two branches are not both 1.0.
        elem_dot_bonus: vec![(DamageType::Toxin, 1.5), (DamageType::Electricity, 1.5)],
        arcane: crate::data::arcanes::ArcaneFx {
            debilitate_chance: chance,
            ..crate::data::arcanes::ArcaneFx::none()
        },
        ..FightParams::default()
    };

    const RUNS: u32 = 400;
    const SEED: u64 = 0xDEB1;
    let dmg = |faction: f64, chance: f64| {
        monte_carlo(&base(faction, chance), RUNS, SEED).mean_damage
    };

    let f = 1.55;
    let added_plain = dmg(1.0, 1.0) - dmg(1.0, 0.0);
    let added_faction = dmg(f, 1.0) - dmg(f, 0.0);

    assert!(
        added_plain > 0.0,
        "the arcane must add damage at all — the split never fired"
    );
    let exponent_ratio = added_faction / added_plain;
    let want = f * f * f;
    assert!(
        (exponent_ratio / want - 1.0).abs() < 0.02,
        "split damage scaled by {exponent_ratio} across a {f} faction bonus; \
         f³ is {want}, f² would be {}",
        f * f
    );
}

/// ...AND IT BURNS OFF `ModifiedBase`, WHICH MAY BE WRONG (M33).
///
/// It pins TODAY'S reading so the open question cannot be resolved by
/// accident: a DoT's nominal base excludes the elemental portions of the
/// hit, DE's rule for a status a WEAPON applied, and the split follows it.
/// The rival reading — the split reads the whole modded INSTANCE, the way
/// an ability-applied status does — puts this ratio at 2.0, and flipping
/// the assertion below is the whole of the change. M33.
///
/// THE FIXTURE HOLDS `ModifiedBase` AT 100 and varies the ELEMENT on top of
/// it, 100 of Corrosive against 200, which is what an elemental mod does.
/// Varying `dot_modified_base` instead proves nothing: `mb_live` is derived
/// FROM it, so halving it halves `mb_live` and doubles the ratio, and the
/// product is invariant — which is the property being claimed, so such a
/// test passes at 1.000 under either reading.
#[test]
fn a_debilitate_split_burns_off_modified_base_not_the_hit() {
    // Corrosive only: it has no DoT of its own, so every point of damage
    // the arcane adds is the split's, and nothing else moves with the base.
    let build = |hit: f64, chance: f64| FightParams {
        damage: DamageVector::new().with(DamageType::Corrosive, hit),
        dot_modified_base: Some(100.0),
        status_chance: 1.0,
        base_status_chance: 1.0,
        fire_rate: 10.0,
        magazine_size: 1e9,
        infinite_reserve: true,
        elem_dot_bonus: vec![(DamageType::Toxin, 1.5), (DamageType::Electricity, 1.5)],
        arcane: crate::data::arcanes::ArcaneFx {
            debilitate_chance: chance,
            ..crate::data::arcanes::ArcaneFx::none()
        },
        ..FightParams::default()
    };
    const RUNS: u32 = 400;
    const SEED: u64 = 0xDEB2;
    let added = |hit: f64| {
        monte_carlo(&build(hit, 1.0), RUNS, SEED).mean_damage
            - monte_carlo(&build(hit, 0.0), RUNS, SEED).mean_damage
    };
    let plain = added(100.0); // hit == ModifiedBase: no element on top
    let doubled = added(200.0); // twice the hit, same ModifiedBase
    assert!(plain > 0.0, "the arcane must add damage at all");
    let ratio = doubled / plain;
    assert!(
        (ratio - 1.0).abs() < 0.02,
        "the split reads ModifiedBase today, so doubling the hit's ELEMENT must              not move it; got {ratio} (the rival reading is 2.0 — see M33)"
    );
}

/// GOTVA PRIME'S PASSIVE — a status-triggered crit-chance SET, and the
/// first crit LOCK in the engine.
///
/// Measured out of the sim rather than asserted: with the passive on, the
/// share of pellets that crit rises toward the armed rate, and it does so
/// ONLY when statuses are landing. A weapon that procs nothing can never
/// arm it, which is the cleanest proof that the trigger is the status and
/// not the shot.
/// PRIMARY COMPRESSION, and the column that makes it two mechanics.
///
/// The arcane pays per metre of blast radius given up, and the weapon's own
/// row says WHERE the payment lands: the Shedu `multiplies` (a free-standing
/// ×6.28 at 6.6 m), the Braton Incarnon `adds` (+240% into the base-damage
/// bucket). The two are the same number and NOT the same build, and the
/// test is the difference rather than either one: a bonus that adds is
/// DILUTED by Serration and one that multiplies is not, so equipping
/// Serration must shrink the first one's worth and leave the second's
/// exactly where it was.
#[test]
fn compression_pays_into_the_bracket_its_row_names() {
    let fx = crate::data::arcanes::for_slot("primary", "primary_compression")
        .unwrap()
        .fx(5, crate::model::StackPolicy::Emergent, &[], crate::data::tenno::default_tenno());
    let arena = crate::arena::Arena::training(30.0);
    let gain = |weapon: &str, mods: &[&crate::model::ModDef]| {
        let base = crate::model::WeaponBase::from_data(weapon, true, &[]);
        let panel = crate::build::loadout::resolve(&base, mods, crate::model::StackPolicy::Emergent);
        let with = monte_carlo(
            &FightParams::from_panel(&panel, &arena, &fx), 8, 0xC0FFEE,
        ).mean_damage;
        let without = monte_carlo(
            &FightParams::from_panel(&panel, &arena, &ArcaneFx::none()), 8, 0xC0FFEE,
        ).mean_damage;
        with / without
    };
    let pool = crate::data::mods::class_pool("rifle");
    let serration = pool.iter().find(|m| m.id == "serration").expect("serration");
    let mods: Vec<&crate::model::ModDef> = vec![serration];

    // The bracket each row names, before any fight runs.
    let shedu = crate::model::WeaponBase::from_data("shedu", true, &[]);
    let p = FightParams::from_panel(
        &crate::build::loadout::resolve(&shedu, &[], crate::model::StackPolicy::Emergent), &arena, &fx,
    );
    // 1 + 6.6 x 0.8 — spelled out, because clippy reads the literal 6.28
    // as an approximation of TAU and it is nothing of the sort.
    assert!((p.compression_multiplier - (1.0 + 6.6 * 0.8)).abs() < 1e-9, "6.6 m -> +528%");
    assert_eq!(p.compression_base_damage, 0.0);
    let braton = crate::model::WeaponBase::from_data("braton_incarnon", true, &[]);
    let p = FightParams::from_panel(
        &crate::build::loadout::resolve(&braton, &[], crate::model::StackPolicy::Emergent), &arena, &fx,
    );
    assert!((p.compression_base_damage - 2.4).abs() < 1e-9, "3.0 m x 0.8 = +240%");
    assert_eq!(p.compression_multiplier, 1.0);

    // …and the fight tells them apart. Serration is +165%, so an ADDING
    // bonus keeps 1/2.65 of its relative worth and a MULTIPLYING one keeps
    // all of it.
    let (adds_bare, adds_serrated) = (gain("braton_incarnon", &[]), gain("braton_incarnon", &mods));
    assert!(
        adds_serrated < adds_bare - 0.5,
        "an `adds` row is diluted by Serration: x{adds_bare:.2} bare, x{adds_serrated:.2} serrated"
    );
    let (mul_bare, mul_serrated) = (gain("shedu", &[]), gain("shedu", &mods));
    assert!(
        (mul_serrated - mul_bare).abs() < 0.05,
        "a `multiplies` row is not: x{mul_bare:.2} bare, x{mul_serrated:.2} serrated"
    );
    assert!(mul_bare > 2.0, "and it is worth something at all: x{mul_bare:.2}");
}

/// DEATH KNELL IS ADDED TO THE FINISHED MULTIPLIER, so a crit-damage mod
/// does not multiply it: `2 x (1 + Crit Damage Mods) + 0.5 x Stacks`.
///
/// Read off the DAMAGE, which at 100% crit chance IS the multiplier, and
/// dealt as IMPACT because the pile's other half raises status chance and
/// an Impact proc deals no damage.
/// A KILL LEAVES ONE STANDING, AND NOTHING READS IT BACK. Three rules the
/// data could lose silently: the weapon must DECLARE it, the body must be
/// in range, and the PEAK is what a 7 s life is visible in.
#[test]
fn a_kill_leaves_a_ghost_standing_where_the_weapon_says_so() {
    let spec = crate::model::SpawnOnKillSpec { seconds: 7.0, range_m: 50.0 };
    let build = |declared: bool, metres_away: f64| FightParams {
        player_at: crate::rules::space::Vec2::new(0.0, 0.0),
        target_at: crate::rules::space::Vec2::new(0.0, metres_away),
        damage: DamageVector::new().with(DamageType::Impact, 5000.0),
        fire_rate: 5.0,
        magazine_size: 1e9,
        infinite_reserve: true,
        duration_seconds: 30.0,
        arcane: crate::data::arcanes::ArcaneFx::none(),
        target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
        body_parts: mono_body(1.0),
        spawn_on_kill: declared.then_some(spec),
        ..FightParams::default()
    };
    let run = |p: &FightParams| monte_carlo(p, 20, 0x6057);

    // DECLARED: every kill leaves one, and they pile up while they last.
    let on = run(&build(true, 10.0));
    assert!(on.mean_kills > 10.0, "the fixture stopped killing: {}", on.mean_kills);
    assert!((on.mean_ghosts - on.mean_kills).abs() < 1e-9,
        "{} kills left {} standing", on.mean_kills, on.mean_ghosts);
    assert!(on.ghosts_peak > 1, "a 7 s life should stack up: {}", on.ghosts_peak);

    // NOT DECLARED: the same kills leave nothing.
    let off = run(&build(false, 10.0));
    assert!((off.mean_kills - on.mean_kills).abs() < 1e-9, "the control moved");
    assert!((off.mean_ghosts).abs() < 1e-9, "undeclared, got {}", off.mean_ghosts);

    // OUT OF RANGE: the card says 50 m, so a body standing at 60 leaves
    // nothing — and dies at exactly the same rate, this arena having no
    // falloff on the fixture.
    let far = run(&build(true, 60.0));
    assert!((far.mean_kills - on.mean_kills).abs() < 1e-9, "the control moved");
    assert!((far.mean_ghosts).abs() < 1e-9, "out of range, got {}", far.mean_ghosts);
}

/// PYRANA PRIME'S SECOND GUN IS BOUGHT WITH A STREAK. Every shot kills here,
/// so at 5 rounds a second three kills fit inside the 2 s window and the gun
/// is up most of the fight; at 0.4 a second no two kills do, and the fight
/// is the undeclared one to the kill.
#[test]
fn a_kill_streak_summons_a_second_gun_and_only_a_streak_does() {
    let spec = |fire_rate_multiplier: f64| crate::model::KillStreakSummonSpec {
        kills: 3,
        kill_window_seconds: 2.0,
        duration_seconds: 6.0,
        magazine_multiplier: 2.0,
        fire_rate_multiplier,
    };
    let build = |summon, fire_rate: f64, magazine_size: f64| FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 5000.0),
        fire_rate,
        magazine_size,
        reload_seconds: 5.0,
        infinite_reserve: true,
        duration_seconds: 30.0,
        arcane: crate::data::arcanes::ArcaneFx::none(),
        target: frail_target(TargetMode::InstantRespawn, 0.0, 0.0),
        body_parts: mono_body(1.0),
        kill_streak_summon: summon,
        ..FightParams::default()
    };
    let kills = |p: &FightParams| monte_carlo(p, 4, 0x9a4a).mean_kills;

    // THE FIRE RATE: a bottomless magazine, so x1.4 is all that can move.
    let (on, off) = (kills(&build(Some(spec(1.4)), 5.0, 1e9)), kills(&build(None, 5.0, 1e9)));
    assert!(off > 100.0, "the fixture stopped killing: {off}");
    assert!(on > off * 1.25, "a streak every few seconds: {on} against {off}");

    // THE MAGAZINE: no fire-rate half, twelve rounds and a long reload, so
    // the extra rounds are all that can move.
    let (on, off) = (kills(&build(Some(spec(1.0)), 2.0, 12.0)), kills(&build(None, 2.0, 12.0)));
    assert!(on > off + 5.0, "the second gun's rounds: {on} against {off}");

    // NO STREAK: 2.5 s between kills never chains.
    let (on, off) = (kills(&build(Some(spec(1.4)), 0.4, 1e9)), kills(&build(None, 0.4, 1e9)));
    assert!((on - off).abs() < 1e-9, "no streak, yet {on} against {off}");
}

#[test]
fn death_knell_adds_its_stacks_to_the_finished_crit_multiplier() {
    let spec = crate::model::WeakpointStacksSpec {
        max_stacks: 3,
        duration_seconds: 2.0,
        crit_multiplier: 0.5,
        status_chance: 0.20,
        ammo_efficiency: 1.0,
    };
    let build = |cd: f64, passive: bool, head: bool| FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 100.0),
        base_crit_chance: 1.0,
        unmodded_crit_chance: 1.0,
        crit_multiplier: 2.0 * cd,
        unmodded_crit_damage: 2.0,
        crit_tier_upgrade_chance: 0.0,
        fire_rate: 10.0,
        magazine_size: 1e9,
        infinite_reserve: true,
        weakpoint_stacks: passive.then_some(spec),
        arcane: crate::data::arcanes::ArcaneFx::none(),
        weakpoint_crit_chance_relative: 0.0,
        body_parts: vec![BodyPart {
            name: if head { "head".into() } else { "body".into() },
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: head,
            crit_bonus: false,
        }],
        ..FightParams::default()
    };
    // Damage per shot, which is the multiplier up to the weapon's base.
    let mult = |p: &FightParams| {
        let s = monte_carlo(p, 200, 0x60D5);
        s.mean_damage / s.mean_shots.max(1.0) / 100.0
    };

    // Bare: the weapon's own 2.0x, and the pile is worth 3 x 0.5 on top.
    let off = mult(&build(1.0, false, true));
    let on = mult(&build(1.0, true, true));
    assert!((off - 2.0).abs() < 0.02, "control moved: {off}");
    assert!((on - 3.5).abs() < 0.05, "2.0 + 3 x 0.5 = 3.5, got {on}");

    // A BODY HIT EARNS NOTHING, which is what makes the number above the
    // pile's rather than something else's.
    let body = mult(&build(1.0, true, false));
    assert!((body - 2.0).abs() < 0.02, "a body shot armed it: {body}");

    // …AND A CRIT-DAMAGE MOD DOES NOT MULTIPLY THEM: 4.0 + a flat 1.5.
    let modded = mult(&build(2.0, true, true));
    assert!((modded - 5.5).abs() < 0.05, "4.0 + 1.5 = 5.5, got {modded}");
}

#[test]
fn gotva_super_crit_arms_on_status_and_only_on_status() {
    let sc = crate::model::SuperCritSpec { chance: 0.15, crit_chance: 3.0 };
    let build = |status: f64, passive: bool| FightParams {
        // TOXIN, not Gotva Prime's own Puncture: a Puncture proc applies
        // Weakened, which grants crit chance of its own — the control would
        // then crit for a reason that is not the passive. (In play the two
        // do stack, and that is part of why this weapon likes statuses.)
        damage: DamageVector::new().with(DamageType::Toxin, 100.0),
        // `base_crit_chance` is the RESOLVED one despite the name, so zero
        // here means every crit observed came from the passive.
        base_crit_chance: 0.0,
        unmodded_crit_chance: 0.0,
        status_chance: status,
        base_status_chance: status,
        fire_rate: 10.0,
        magazine_size: 1e9,
        infinite_reserve: true,
        super_crit_on_status: passive.then_some(sc),
        // A CLEAN baseline. `FightParams::default()` is the Dual Toxocyst
        // fixture WITH Secondary Enervate, whose arcane contributes crit —
        // so without these the "0% crit weapon never crits" control fails
        // for a reason that has nothing to do with the passive.
        arcane: crate::data::arcanes::ArcaneFx::none(),
        crit_tier_upgrade_chance: 0.0,
        weakpoint_crit_chance_relative: 0.0,
        body_parts: vec![BodyPart {
            name: "body".into(),
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            crit_bonus: false,
        }],
        ..FightParams::default()
    };
    let crit_share = |p: &FightParams| monte_carlo(p, 300, 0x607A).mean_crit_rate;

    // No passive: a 0% crit weapon never crits, whatever it procs.
    let ctl = crit_share(&build(1.0, false));
    assert!(ctl < 1e-9, "0% crit and no passive, got {ctl}");
    // Passive, but NOTHING to trigger it: still never.
    assert!(
        crit_share(&build(0.0, true)) < 1e-9,
        "a weapon that applies no status can never arm it"
    );
    // Passive AND statuses landing: it crits, and at roughly the armed rate.
    // Every pellet procs, so ~15% of them arm the NEXT one, and an armed
    // pellet crits with certainty (300% is three guaranteed tiers).
    let on = crit_share(&build(1.0, true));
    assert!(
        (on - 0.15).abs() < 0.03,
        "armed share {on}, want ~0.15 (15% of pellets arm the next)"
    );

    // ...AND IT DOES NOT CARE WHERE THE PELLET LANDS. The card says "the
    // next hit", not "the next weak-point hit", so an armed body shot is
    // 300% exactly as an armed headshot is — and a weak-point crit bonus
    // (Pistol/Primary Acuity) contributes NOTHING to an armed pellet,
    // because the value is SET and not added to.
    //
    // Built with a weak-point crit bonus present and a 0% base, so the only
    // way a head pellet could out-crit a body pellet is if the bonus
    // survived the set. It does not.
    let aimed = |head: bool| FightParams {
        weakpoint_crit_chance_relative: 3.5, // Acuity rank 10
        body_parts: vec![BodyPart {
            name: if head { "head".into() } else { "body".into() },
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: head,
            crit_bonus: false,
        }],
        ..build(1.0, true)
    };
    let (h, b) = (crit_share(&aimed(true)), crit_share(&aimed(false)));
    assert!(
        (h - b).abs() < 0.02,
        "armed crit share differs by body part: head {h}, body {b} — the SET is              supposed to replace the weak-point bonus, not stack with it"
    );
}
