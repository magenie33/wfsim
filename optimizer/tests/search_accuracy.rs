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
use wfsim_engine::dummy::{BodyPart, BuffConfig, LockMode, TargetMode};
use wfsim_engine::loadout::{ModDef, StackPolicy, WeaponBase};
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
    let spec = wfsim_engine::enemy_data::all()
        .into_iter()
        .find(|s| s.id == "thrax_centurion")
        .expect("thrax_centurion");
    // A sentinel weapon aims at nothing in particular: spread over the body.
    let bodies: Vec<_> = spec.body_parts.iter().filter(|p| !p.is_head).collect();
    let w = 1.0 / bodies.len().max(1) as f64;
    Scenario {
        arena: Arena {
            abilities: Vec::new(),
            tenno: wfsim_engine::tenno_data::default_tenno().clone(),
            // Point blank: this test grades the SEARCH against an exhaustive
            // reference, so the fight has to be the plainest one there is.
            player_at: wfsim_engine::space::Vec2::ORIGIN,
            target_at: wfsim_engine::space::Vec2::new(0.0, wfsim_engine::space::CONTACT_RANGE_M),
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
                    crit_bonus: b.crit_bonus,
                })
                .collect(),
            duration_secs: duration,
            // ONE BODY — a fixture, not a formation.
            others: Vec::new(),
        },
        incarnon_cycle: false,
        frenzy_lock: LockMode::Initial(0),
        frenzy_locks: Vec::new(),
        frenzy: false,
        buff_cfg: BuffConfig::new(),
        infinite_ammo: true,
        policy: StackPolicy::BaseOnly, // sentinel: nothing on the field triggers its conditionals
    }
}

/// The exhaustive scope: every legal 8-mod build over `SCOPE`, every element
/// order, deduped — the same walk the production search starts from.
fn pool() -> Vec<ModDef> {
    let pool: Vec<ModDef> = wfsim_engine::mods_data::pool_for_weapon("verglas_prime")
        .into_iter()
        .filter(|m| SCOPE.contains(&m.id))
        .collect();
    assert_eq!(pool.len(), SCOPE.len(), "the fixture scope must all be equippable");
    pool
}

fn exhaust(scenario: &Scenario, min: u32) -> (Vec<Candidate>, Vec<Job>) {
    let pool = pool();
    let base = WeaponBase::from_data("verglas_prime", true, &[]);
    let innate = wfsim_engine::weapons_data::innate_slots("verglas_prime");
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
    let arcanes = vec![wfsim_engine::arcanes_data::ArcaneFx::none()];
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
fn run_pipeline(
    s: &Scenario,
    truth: &Truth,
    cands: &[Candidate],
    jobs: &[Job],
    min: usize,
    max_evals: u64,
) -> (Verdict, SearchStats, usize) {
    let pool = pool();
    let base = WeaponBase::from_data("verglas_prime", true, &[]);
    let innate = wfsim_engine::weapons_data::innate_slots("verglas_prime");
    let arcanes = vec![wfsim_engine::arcanes_data::ArcaneFx::none()];
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
    let (screened, stats) = search(&space, &expand, &arcanes, s, &cfg, None, None);
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
    // build a SECOND identity, and 361 of 3,086 results matched nothing
    // (2026-08-03).
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
    (judge(truth, &board, 10, spent), stats, unmatched)
}

/// A scope the budget can finish is still SOLVED, not sampled: the shuffled
/// order is a bijection, so reaching its end visits every subset exactly once.
/// The search must say so, and it must land on the reference's answer.
#[test]
fn a_scope_that_fits_is_searched_exhaustively_and_solved() {
    const RUNS: u32 = 60;
    let s = scenario(30.0, 150);
    let (cands, jobs) = exhaust(&s, 8);
    let arcanes = vec![wfsim_engine::arcanes_data::ArcaneFx::none()];
    let truth = Truth::measure(&cands, &jobs, &arcanes, &s, RUNS, 0xA11CE);
    let (v, stats, unmatched) = run_pipeline(&s, &truth, &cands, &jobs, 8, 0);
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

/// ...and a budget it cannot finish leaves a SAMPLE, which must be honest
/// about being one and must still be worth reading. The old depth-first walk
/// left a lexicographic corner here — builds made of the first few pool
/// entries — which is what let a Heat-less build win on a weapon where Heat is
/// worth 4.5x.
#[test]
fn a_budget_it_cannot_finish_leaves_an_honest_sample() {
    const RUNS: u32 = 40;
    let s = scenario(30.0, 150);
    let (cands, jobs) = exhaust(&s, 1);
    let arcanes = vec![wfsim_engine::arcanes_data::ArcaneFx::none()];
    let truth = Truth::measure(&cands, &jobs, &arcanes, &s, RUNS, 0xA11CE);
    let (v, stats, unmatched) = run_pipeline(&s, &truth, &cands, &jobs, 1, 120);
    println!(
        "[search] {} of {} index positions ({:.1}%), exhaustive {} -> rank {} of {} (regret {:.2}%)",
        stats.sampled, stats.space, stats.coverage() * 100.0, stats.exhaustive,
        v.rank, jobs.len(), v.regret * 100.0
    );
    assert_eq!(unmatched, 0, "{unmatched} results were not in the exhaustive enumeration");
    assert!(!stats.exhaustive, "a search this budget cannot finish must not claim it did");
    assert!(stats.coverage() < 1.0, "coverage {} claims the whole space", stats.coverage());
    assert!(stats.neighbours > 0, "the budget was never spent climbing");
    // Not the optimum — this budget cannot promise one. What it must promise is
    // that a few per cent of the space still buys the top of it, which is the
    // property the depth-first walk did not have at any coverage.
    let top_decile = (jobs.len() / 10).max(5);
    assert!(
        v.rank <= top_decile,
        "rank {} of {} on {:.1}% coverage — a uniform sample plus a climb should not land there",
        v.rank,
        jobs.len(),
        stats.coverage() * 100.0
    );
}

/// The production funnel, graded. It may return any build the reference cannot/// The production funnel, graded. It may return any build the reference cannot
/// separate from the best; anything else is a build it LOST.
#[test]
fn the_funnel_lands_inside_the_reference_answer_set() {
    const RUNS: u32 = 60;
    let s = scenario(30.0, 150);
    let (cands, jobs) = exhaust(&s, 8);
    let arcanes = vec![wfsim_engine::arcanes_data::ArcaneFx::none()];
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
