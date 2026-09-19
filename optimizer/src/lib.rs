// SPDX-License-Identifier: AGPL-3.0-or-later
//! wfsim optimizer: search for the best mod combination on top of the engine.
//!
//! Principle (docs/CORE.md §5): the optimizer **only calls the engine** and
//! never reimplements a simplified damage formula of its own — otherwise the
//! "optimum" is fake.
//!
//! Search design (docs/OPTIMIZER.md):
//! 1. Enumerate **canonical forms**: an unordered 8-mod subset (families are
//!    mutually exclusive) × the order of its distinct primary elements — the
//!    only position-sensitive dimension. Equivalent permutations are never
//!    generated; element orders that resolve to the same combined vector are
//!    deduplicated after running the (cheap, pure) layer-[2] combination.
//! 2. **Best-effort legalization** as a filter: innate polarity pool →
//!    greedy Forma (`engine::mods::plan_forma`); impossible builds drop out.
//! 3. Conditional buffs evaluate under `StackPolicy::AssumedMax`.
//! 4. Evaluation is staged Monte Carlo (successive halving): cheap short
//!    rounds rank by mean effective damage, finals rank by mean kills.

/// WHICH BUILDS TO LOOK AT — the anytime search over the subset space.
pub mod search;
/// The mod-subset space as an INDEX RANGE rather than a walk — what makes a
/// truncated search a sample instead of a corner.
pub mod space;
/// How a search strategy is GRADED — an exhausted scope, flat-evaluated. The
/// search half of this crate is only as good as what measures it.
pub mod truth;

mod enumerate;
mod evaluate;
mod funnel;
mod pool;
mod threads;

pub use enumerate::{
    enumerate_candidates, enumerate_candidates_each, enumerate_candidates_observed, expand_one,
    rebuild_candidate, Candidate, Constraints, EnumStats,
};
pub(crate) use evaluate::job_seed;
pub use evaluate::{evaluate, evaluate_batch, Job, Scenario};
pub(crate) use funnel::Scored;
pub use funnel::{
    run_funnel, schedule, schedule_to, CheckpointFn, FunnelState, RoundBoardFn, RoundNote,
    ScreenBoardFn, ScreenedJob, BOARD_TOP,
};
pub use pool::{class_pool, dominated_mods, pool};
pub(crate) use threads::batch_width;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use threads::worker_threads;
pub use threads::{deprioritize_current_thread, set_tick_hook, set_worker_threads, tick};
