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
use wfsim_optimizer::truth::{judge, Truth};
use wfsim_optimizer::{
    enumerate_candidates_observed, run_funnel, schedule_to, Candidate, Constraints, Job, Scenario,
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
            tenno: wfsim_engine::tenno_data::default_tenno().clone(),
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
fn exhaust(scenario: &Scenario) -> (Vec<Candidate>, Vec<Job>) {
    let pool: Vec<ModDef> = wfsim_engine::mods_data::pool_for_weapon("verglas_prime")
        .into_iter()
        .filter(|m| SCOPE.contains(&m.id))
        .collect();
    assert_eq!(pool.len(), SCOPE.len(), "the fixture scope must all be equippable");
    let base = WeaponBase::from_data("verglas_prime", true, &[]);
    let innate = wfsim_engine::weapons_data::innate_slots("verglas_prime");
    let (cands, _stats, complete) = enumerate_candidates_observed(
        &pool,
        &base,
        None,
        0,
        8, // exactly 8: the fixture is about ranking, not about build size
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
    let (cands, jobs) = exhaust(&s);
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

/// The production funnel, graded. It may return any build the reference cannot
/// separate from the best; anything else is a build it LOST.
#[test]
fn the_funnel_lands_inside_the_reference_answer_set() {
    const RUNS: u32 = 60;
    let s = scenario(30.0, 150);
    let (cands, jobs) = exhaust(&s);
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
