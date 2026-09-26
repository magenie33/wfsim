// SPDX-License-Identifier: AGPL-3.0-or-later
//! The optimizer's accuracy, MEASURED — not asserted.
//!
//! A search strategy cannot vouch for itself: "the funnel kept the best build"
//! is a claim about an answer nobody computed. So this exhausts a scope small
//! enough to exhaust, evaluates every job in it flat (`truth::Truth`), and
//! grades the production search against that reference.
//!
//! The scope is small so the test is cheap; the same grading runs at real
//! scale through `wfsim-truth` (see docs/OPTIMIZER.md). What makes the small
//! version worth having is that it FAILS when a change to the search starts
//! losing builds — which is the failure mode that has no other symptom.

use wfsim_engine::arena::Arena;
use wfsim_engine::fight::{BuffConfig, LockMode};
use wfsim_engine::target::{BodyPart, TargetMode};
use wfsim_engine::model::WeaponBase;
use wfsim_engine::model::{ModDef, StackPolicy};
use wfsim_optimizer::descent::{descent, Start};
use wfsim_optimizer::search::{search, SearchConfig, SearchStats};
use wfsim_optimizer::space::SubsetSpace;
use wfsim_optimizer::truth::{judge, Truth, Verdict};
use wfsim_optimizer::{
    enumerate_candidates_observed, expand_one, run_funnel, schedule_to, Candidate, Constraints,
    Job, Scenario,
};

/// Ten rifle mods on Verglas Prime — five of them elemental, so the scope has
/// real element-order structure and a real Viral+Heat answer in it, and the
/// weapon is the SENTINEL case (`BaseOnly`).
const SCOPE: &[&str] = &[
    "serration",
    "split_chamber",
    "point_strike",
    "vital_sense",
    "hammer_shot",
    "cryo_rounds",
    "infected_clip",
    "hellfire",
    "stormbringer",
    "malignant_force",
];

fn scenario(duration: f64, level: u32) -> Scenario {
    let spec = wfsim_engine::data::enemies::all()
        .into_iter()
        .find(|s| s.id == "thrax_centurion")
        .expect("thrax_centurion");
    // A sentinel weapon aims at nothing in particular: spread over the body.
    let bodies: Vec<_> = spec.body_parts.iter().filter(|p| !p.is_head).collect();
    let w = 1.0 / bodies.len().max(1) as f64;
    Scenario {
        also_acting: Vec::new(),
        arena: Arena {
            apl: Default::default(),
            squad_size: 1,
            target_id: "e1".to_string(),
            abilities: Vec::new(),
            ability_picks: Vec::new(),
            ability_strength: 1.0,
            tenno: wfsim_engine::data::tenno::default_tenno().clone(),
            // Point blank: this test grades the SEARCH against an exhaustive
            // reference, so the fight has to be the plainest one there is.
            player_at: wfsim_engine::rules::space::Vec2::ORIGIN,
            target_at: wfsim_engine::rules::space::Vec2::new(0.0, wfsim_engine::rules::space::CONTACT_RANGE_M),
            target: spec
                .target_params(level, true, false, TargetMode::InstantRespawn)
                .expect("target"),
            body_parts: bodies
                .iter()
                .map(|b| BodyPart {
                    name: b.name.clone(),
                    aim_weight: w,
                    multiplier: b.multiplier,
                    is_head: b.is_head,
                    is_weak_point: b.is_weak_point(),
                    crit_bonus: b.crit_bonus,
                })
                .collect(),
            duration_seconds: duration,
            // ONE BODY — a fixture, not a formation.
            others: Vec::new(),
            // …and the weapon points AT it.
            aim_at: None,
        },
        incarnon_cycle: false,
        frenzy_lock: LockMode::Initial(0),
        frenzy_locks: Vec::new(),
        frenzy: false,
        buff_cfg: BuffConfig::new(),
        denied_buff_triggers: Vec::new(),
        infinite_ammo: true,
        policy: StackPolicy::BaseOnly, // sentinel: nothing on the field triggers its conditionals
    }
}

/// The exhaustive scope: every legal 8-mod build over `SCOPE`, every element
/// order, deduped — the same walk the production search starts from.
fn pool() -> Vec<ModDef> {
    let pool: Vec<ModDef> = wfsim_engine::data::mods::pool_for_weapon("verglas_prime")
        .into_iter()
        .filter(|m| SCOPE.contains(&m.id))
        .collect();
    assert_eq!(pool.len(), SCOPE.len(), "the fixture scope must all be equippable");
    pool
}

fn exhaust(scenario: &Scenario, min: u32) -> (Vec<Candidate>, Vec<Job>) {
    let pool = pool();
    let base = WeaponBase::from_data("verglas_prime", true, &[]);
    let innate = wfsim_engine::data::weapons::innate_slots("verglas_prime");
    let (cands, _stats, complete) = enumerate_candidates_observed(
        &pool,
        &base,
        None,
        0,
        min,
        8,
        60,
        &innate,
        &Constraints::default(),
        &[None],
        None,
        0,
        &scenario.arena.tenno,
        scenario.policy,
    );
    assert!(complete, "the fixture scope must be exhaustible");
    let jobs: Vec<Job> = (0..cands.len()).map(|i| (i, 0)).collect();
    (cands, jobs)
}

/// A reference has to reproduce itself before it can grade anything. Two flat
/// measurements of the same scope under different seeds must agree on the top
/// of the ranking; if they do not, the run count is too low and every verdict
/// built on it is noise.
#[test]
fn the_reference_reproduces_itself_under_a_different_seed() {
    const RUNS: u32 = 60;
    let s = scenario(30.0, 150);
    let (cands, jobs) = exhaust(&s, 8);
    let arcanes = vec![wfsim_engine::data::arcanes::ArcaneFx::none()];
    let a = Truth::measure(&cands, &jobs, &arcanes, &s, RUNS, 0xA11CE);
    let b = Truth::measure(&cands, &jobs, &arcanes, &s, RUNS, 0xB0B);

    // The answer SET is what a search is graded against, so that is what has
    // to be stable — not the arbitrary order inside it.
    let ans_a = a.indistinguishable(3.0);
    let ans_b = b.indistinguishable(3.0);
    assert!(
        ans_a.contains(&b.best()) && ans_b.contains(&a.best()),
        "the two references disagree on the best build: #{} vs #{} — raise RUNS",
        a.best(),
        b.best()
    );
    let overlap = a.agrees_with(&b, 10);
    assert!(
        overlap >= 0.7,
        "top-10 overlap {overlap:.2} across seeds — the reference is not settled at {RUNS} runs"
    );
    println!(
        "[reference] {} jobs, {RUNS} runs; answer set {} builds, top-10 overlap {overlap:.2}",
        jobs.len(),
        ans_a.len()
    );
    // A reference whose answer set is most of the scope grades nothing: every
    // strategy passes. The fixture has to be a scope with a real winner in it.
    assert!(
        ans_a.len() * 4 < jobs.len(),
        "the answer set is {} of {} jobs — this scope cannot separate builds, \
         so it cannot grade a search",
        ans_a.len(),
        jobs.len()
    );
}

/// Run the PRODUCTION PIPELINE — search, then funnel — over `space`, and grade
/// its leaderboard against the reference.
///
/// Grading the funnel alone graded the half that was already good: it is handed
/// a job list, and the half that decides what is IN that list is the half that
/// could lose the winner (docs/OPTIMIZER.md).
fn start(mods: &[usize]) -> Start {
    Start { mods: mods.to_vec(), ..Default::default() }
}

fn run_pipeline(
    s: &Scenario,
    truth: &Truth,
    cands: &[Candidate],
    jobs: &[Job],
    min: usize,
    max_evals: u64,
    // `Some(starts)` runs the descent from those starts (empty = one per
    // element) instead of the sampler.
    descent_from: Option<&[Start]>,
) -> (Verdict, SearchStats, usize, Vec<usize>) {
    let pool = pool();
    let base = WeaponBase::from_data("verglas_prime", true, &[]);
    let innate = wfsim_engine::data::weapons::innate_slots("verglas_prime");
    let arcanes = vec![wfsim_engine::data::arcanes::ArcaneFx::none()];
    let families: Vec<Option<&'static str>> = pool.iter().map(|m| m.family).collect();
    let usable: Vec<usize> = (0..pool.len()).collect();
    let space = SubsetSpace::new(&families, &usable, &[], min, 8);
    let expand = |subset: &[usize]| -> Vec<Candidate> {
        let mut out = Vec::new();
        expand_one(
            &pool, &base, None, 0, 60, &innate, &[None], subset,
            &s.arena.tenno, s.policy, &mut out,
        );
        out
    };
    let cfg = SearchConfig { max_evals, keep: 65_536, seed: 0xDEAD_BEEF, ..Default::default() };
    let (screened, stats) = match descent_from {
        Some(starts) => descent(&space, &pool, starts, &expand, &arcanes, s, &cfg, None, None),
        None => search(&space, &expand, &arcanes, s, &cfg, None, None),
    };
    assert!(!screened.is_empty(), "the search returned nothing");

    // Deduplicate into a candidate table exactly as the web path does.
    let mut sc: Vec<Candidate> = Vec::new();
    let mut by_ptr: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut sjobs: Vec<Job> = Vec::new();
    for sj in &screened {
        let key = std::sync::Arc::as_ptr(&sj.cand) as usize;
        let ci = *by_ptr.entry(key).or_insert_with(|| {
            sc.push((*sj.cand).clone());
            sc.len() - 1
        });
        sjobs.push((ci, sj.ai));
    }
    let rounds = schedule_to(sjobs.len(), truth.runs, 10);
    let spent: u64 = {
        let mut field = sjobs.len() as u64;
        let mut n = stats.evals;
        for &(r, keep, _) in &rounds {
            n += field * u64::from(r);
            field = field.min(keep as u64);
        }
        n
    };
    let last = run_funnel(
        &sc, &arcanes, s, sjobs, &rounds, 0xDEAD_BEEF, false, None, None, 0, None, None,
    );
    // Map every result back to its place in the exhaustive list BY IDENTITY.
    // Both sides build candidates through `expand_one` from an ascending
    // subset, so this is exact — and it is also the assertion that caught a
    // real bug: a climbed subset that was not in canonical order gave the same
    // build a SECOND identity, and 361 of 3,086 results matched nothing.
    let ix_of: std::collections::HashMap<(Vec<usize>, u32, u32, usize), usize> = jobs
        .iter()
        .enumerate()
        .map(|(ji, &(ci, ai))| {
            ((cands[ci].ordered.clone(), cands[ci].variant, cands[ci].exilus, ai), ji)
        })
        .collect();
    let mut board = Vec::new();
    let mut unmatched = 0usize;
    for &((ci, ai), _) in last.iter() {
        let k = (sc[ci].ordered.clone(), sc[ci].variant, sc[ci].exilus, ai);
        match ix_of.get(&k) {
            Some(&ji) => board.push(ji),
            None => unmatched += 1,
        }
    }
    assert!(!board.is_empty(), "nothing the search returned was in the exhaustive enumeration");
    (judge(truth, &board, 10, spent), stats, unmatched, board)
}

/// A scope the budget can finish is still SOLVED, not sampled: the shuffled
/// order is a bijection, so reaching its end visits every subset exactly once.
/// The search must say so, and it must land on the reference's answer.
#[test]
fn a_scope_that_fits_is_searched_exhaustively_and_solved() {
    const RUNS: u32 = 60;
    let s = scenario(30.0, 150);
    let (cands, jobs) = exhaust(&s, 8);
    let arcanes = vec![wfsim_engine::data::arcanes::ArcaneFx::none()];
    let truth = Truth::measure(&cands, &jobs, &arcanes, &s, RUNS, 0xA11CE);
    let (v, stats, unmatched, _) = run_pipeline(&s, &truth, &cands, &jobs, 8, 0, None);
    println!(
        "[search] {} of {} index positions, exhaustive {} -> rank {} (regret {:.2}%, recall {:.0}%) in {} sims",
        stats.sampled, stats.space, stats.exhaustive, v.rank, v.regret * 100.0, v.recall * 100.0, v.sims
    );
    assert_eq!(unmatched, 0, "{unmatched} results were not in the exhaustive enumeration —                               the search and the walk disagree about the scope");
    assert!(stats.exhaustive, "an unbudgeted search of a finite space must reach its end");
    assert!((stats.coverage() - 1.0).abs() < 1e-9, "coverage {} is not 1", stats.coverage());
    assert!(
        v.within_noise,
        "rank {} (regret {:.2}%) — outside the answer set of {} builds",
        v.rank,
        v.regret * 100.0,
        truth.indistinguishable(3.0).len()
    );
}

/// ...and a walk the clock cuts leaves a SAMPLE, which must be honest about
/// being one and must still be worth reading. A depth-first walk leaves a
/// lexicographic corner here — builds made of the first few pool entries —
/// which is how a Heat-less build can win on a weapon where Heat is worth 4.5x.
#[test]
fn a_budget_it_cannot_finish_leaves_an_honest_sample() {
    const RUNS: u32 = 40;
    let s = scenario(30.0, 150);
    let (cands, jobs) = exhaust(&s, 1);
    let arcanes = vec![wfsim_engine::data::arcanes::ArcaneFx::none()];
    let truth = Truth::measure(&cands, &jobs, &arcanes, &s, RUNS, 0xA11CE);
    let (v, stats, unmatched, _) = run_pipeline(&s, &truth, &cands, &jobs, 1, 120, None);
    println!(
        "[search] {} of {} index positions ({:.1}%), exhaustive {} -> rank {} of {} (regret {:.2}%)",
        stats.sampled, stats.space, stats.coverage() * 100.0, stats.exhaustive,
        v.rank, jobs.len(), v.regret * 100.0
    );
    assert_eq!(unmatched, 0, "{unmatched} results were not in the exhaustive enumeration");
    assert!(!stats.exhaustive, "a search this budget cannot finish must not claim it did");
    assert!(stats.coverage() < 1.0, "coverage {} claims the whole space", stats.coverage());
    // Not the optimum — this budget cannot promise one. What it must promise is
    // that a few per cent of the space still buys the top of it, which a
    // depth-first corner does not at any coverage.
    let top_decile = (jobs.len() / 10).max(5);
    assert!(
        v.rank <= top_decile,
        "rank {} of {} on {:.1}% coverage — a uniform sample should not land there",
        v.rank,
        jobs.len(),
        stats.coverage() * 100.0
    );
}

/// The DESCENT, graded on the whole size range: from one start per element,
/// and from a single start that carries no element at all — the start a
/// player who knows nothing would type. Both must reach the answer set, and
/// for less than it costs to exhaust the scope.
#[test]
fn the_descent_reaches_the_answer_set_from_any_start() {
    const RUNS: u32 = 40;
    let s = scenario(30.0, 150);
    let (cands, jobs) = exhaust(&s, 1);
    let arcanes = vec![wfsim_engine::data::arcanes::ArcaneFx::none()];
    let truth = Truth::measure(&cands, &jobs, &arcanes, &s, RUNS, 0xA11CE);
    let serration = pool().iter().position(|m| m.id == "serration").expect("in scope");
    for (label, starts) in [("one start per element", vec![]), ("serration alone", vec![start(&[serration])])] {
        let (v, stats, unmatched, _) = run_pipeline(&s, &truth, &cands, &jobs, 1, 0, Some(&starts));
        println!(
            "[descent from {label}] {} subsets, {} evals -> rank {} of {} (regret {:.2}%, recall {:.0}%)",
            stats.subsets, stats.evals, v.rank, jobs.len(), v.regret * 100.0, v.recall * 100.0
        );
        assert_eq!(unmatched, 0, "{unmatched} results were not in the exhaustive enumeration");
        assert!(!stats.exhaustive, "a descent is never an enumeration");
        assert!(
            (stats.evals as usize) < jobs.len() / 2,
            "{} evals against {} jobs — the descent cost more than half an exhaustive walk",
            stats.evals,
            jobs.len()
        );
        assert!(
            v.within_noise,
            "from {label}: rank {} (regret {:.2}%) — outside the answer set of {} builds",
            v.rank,
            v.regret * 100.0,
            truth.indistinguishable(3.0).len()
        );
    }
}

/// A card a start LOCKS is in every build that start scores, even where the
/// unconstrained answer leaves it out — a lock is a promise to the player, not
/// a starting hint. Its other cards stay free: the winner is the best build
/// that carries it.
#[test]
fn a_locked_card_stays_in_every_build_its_start_scores() {
    const RUNS: u32 = 40;
    let s = scenario(30.0, 9999);
    let (cands, jobs) = exhaust(&s, 8);
    let arcanes = vec![wfsim_engine::data::arcanes::ArcaneFx::none()];
    let truth = Truth::measure(&cands, &jobs, &arcanes, &s, RUNS, 0xA11CE);
    let pool = pool();
    let best = &cands[jobs[truth.best()].0].ordered;
    // A card the unconstrained winner does NOT carry, so the lock has to bite.
    let lock = (0..pool.len()).find(|i| !best.contains(i)).expect("a card outside the winner");
    let starts = vec![Start { mods: vec![lock], locked: vec![lock], ..Default::default() }];
    let (_, _, _, board) = run_pipeline(&s, &truth, &cands, &jobs, 8, 0, Some(&starts));
    for &ji in &board {
        assert!(
            cands[jobs[ji].0].ordered.contains(&lock),
            "a board build dropped the locked {}",
            pool[lock].id
        );
    }
    // The best build that carries it, not merely some build that does.
    let with_lock: Vec<usize> =
        truth.order.iter().copied().filter(|&j| cands[jobs[j].0].ordered.contains(&lock)).collect();
    let rank = with_lock.iter().position(|&j| j == board[0]).map(|p| p + 1);
    println!("[locked {}] winner is #{:?} of {} builds carrying it", pool[lock].id, rank, with_lock.len());
    assert!(rank.is_some_and(|r| r <= 3), "winner is {rank:?} among builds carrying the lock");
}

/// A start that LOCKS its arcane is never scored under another — and the
/// same start unlocked does switch, or the lock would be proving nothing.
#[test]
fn a_locked_arcane_is_the_only_one_its_start_scores() {
    let mut s = scenario(30.0, 9999);
    s.policy = StackPolicy::Emergent;
    let pool = pool();
    let base = WeaponBase::from_data("verglas_prime", true, &[]);
    let innate = wfsim_engine::data::weapons::innate_slots("verglas_prime");
    let merciless = wfsim_engine::data::arcanes::secondary("primary_merciless").expect("arcane");
    let arcanes = vec![
        wfsim_engine::data::arcanes::ArcaneFx::none(),
        merciless.fx(merciless.max_rank, s.policy, base.traits, &s.arena.tenno),
    ];
    let families: Vec<Option<&'static str>> = pool.iter().map(|m| m.family).collect();
    let usable: Vec<usize> = (0..pool.len()).collect();
    let space = SubsetSpace::new(&families, &usable, &[], 8, 8);
    let expand = |subset: &[usize]| -> Vec<Candidate> {
        let mut out = Vec::new();
        expand_one(&pool, &base, None, 0, 60, &innate, &[None], subset, &s.arena.tenno, s.policy, &mut out);
        out
    };
    let cfg = SearchConfig { keep: 65_536, ..Default::default() };
    let cryo = pool.iter().position(|m| m.id == "cryo_rounds").expect("in scope");
    let arcanes_scored = |lock: bool| -> Vec<usize> {
        let starts =
            vec![Start { mods: vec![cryo], arcane: Some(0), lock_arcane: lock, ..Default::default() }];
        let (screened, _) = descent(&space, &pool, &starts, &expand, &arcanes, &s, &cfg, None, None);
        screened.iter().map(|j| j.ai).collect()
    };
    assert!(
        arcanes_scored(false).contains(&1),
        "unlocked, the descent never tried Merciless — this fixture cannot test a lock"
    );
    assert!(
        arcanes_scored(true).iter().all(|&ai| ai == 0),
        "a start locked to no arcane was scored under Merciless"
    );
}

/// The production funnel, graded. It may return any build the reference cannot
/// separate from the best; anything else is a build it LOST.
#[test]
fn the_funnel_lands_inside_the_reference_answer_set() {
    const RUNS: u32 = 60;
    let s = scenario(30.0, 150);
    let (cands, jobs) = exhaust(&s, 8);
    let arcanes = vec![wfsim_engine::data::arcanes::ArcaneFx::none()];
    let truth = Truth::measure(&cands, &jobs, &arcanes, &s, RUNS, 0xA11CE);

    let rounds = schedule_to(jobs.len(), RUNS, 10);
    let sims: u64 = {
        let mut field = jobs.len() as u64;
        let mut n = 0;
        for &(r, keep, _) in &rounds {
            n += field * u64::from(r);
            field = field.min(keep as u64);
        }
        n
    };
    let last = run_funnel(
        &cands,
        &arcanes,
        &s,
        jobs.clone(),
        &rounds,
        0xDEAD_BEEF,
        false,
        None,
        None,
        0,
        None,
        None,
    );
    let board: Vec<usize> = last.iter().map(|&((ci, _), _)| ci).collect();
    let v = judge(&truth, &board, 10, sims);
    println!(
        "[funnel] {} jobs -> rank {} (regret {:.2}%, top-10 recall {:.0}%) \
         in {} sims vs the reference's {}",
        jobs.len(),
        v.rank,
        v.regret * 100.0,
        v.recall * 100.0,
        v.sims,
        v.reference_sims
    );
    assert!(
        v.within_noise,
        "the funnel's winner is reference rank {} (regret {:.2}%, top-10 recall {:.0}%) — \
         outside the answer set of {} builds; it lost a build it should have kept",
        v.rank,
        v.regret * 100.0,
        v.recall * 100.0,
        truth.indistinguishable(3.0).len(),
    );
}
