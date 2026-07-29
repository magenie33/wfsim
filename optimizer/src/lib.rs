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

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;

use wfsim_engine::damage::DamageType;
use wfsim_engine::dummy::{
    monte_carlo, BodyPart, BuffConfig, DummyParams, LockMode, Summary, TargetParams,
};
use wfsim_engine::loadout::{resolve, ModDef, ResolvedPanel, StackPolicy, WeaponBase};
use wfsim_engine::mods::{plan_forma, FormaPlan, PlannedMod, Polarity};

/// The pistol mod pool, mirrored from `data/mods/*.yaml` at MAX RANK
/// (drain = base + max_rank). The YAML files are the source of record; this
/// table is the engine-facing view until the declarative mod loader lands.
pub fn pool() -> Vec<ModDef> {
    // Source of truth: data/mods/*.yaml (mod_type: pistol), loaded by
    // engine::mods_data. Mods are DATA now — add/edit a YAML file, no code.
    // Exilus (utility) mods have no damage model — enumerating them only
    // multiplies the search space, so the optimizer's pool excludes them.
    wfsim_engine::mods_data::pistol_pool()
        .into_iter()
        .filter(|m| !m.exilus)
        .collect()
}

/// Prescribed-mods constraints: forced inclusions/exclusions by mod id.
#[derive(Debug, Clone, Default)]
pub struct Constraints {
    pub require: Vec<String>,
    pub forbid: Vec<String>,
}

/// One canonical, legal, resolved build.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Pool indices of the 8 mods, element mods first in hierarchy order.
    pub ordered: Vec<usize>,
    pub panel: ResolvedPanel,
    /// The transform group's OTHER form resolved against the same mods
    /// (Dual Toxocyst base form) — present when a second form was given.
    pub base_panel: Option<ResolvedPanel>,
    pub plan: FormaPlan,
    /// Weapon-config variant: an index into a caller-owned evolution-set
    /// table (each entry = a chosen evolution-id set + a display label). The
    /// evolution selection is a search dimension; the caller resolves each
    /// set's base/base_form and enumerates candidates tagged with its index.
    pub variant: u32,
    /// Index into the caller's `exilus_opts` slice: which exilus-slot choice
    /// this candidate uses (the option may be `None` = slot left empty). The
    /// exilus slot is a search dimension like the mod subset — the build is
    /// 8 + 1 slots.
    pub exilus: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EnumStats {
    pub subsets: u64,
    pub illegal: u64,
    pub order_variants: u64,
    pub deduped: u64,
}

/// Enumerate all canonical candidates: mod subsets of every size in
/// `min_slots..=max_slots` (family-exclusive, constraint-filtered — slots
/// may be left EMPTY, so a smaller build is a legal candidate; with a
/// harmful mod in the pool it can even win) × distinct-element orders ×
/// exilus-slot options, legalized and deduped by resolved damage vector.
/// Pass `min == max` for the classic exact-size search.
///
/// The build is 8 + 1 slots: `exilus_opts` lists the choices for the exilus
/// slot, each `Some(mod)` or `None` (slot left empty). Every subset × order
/// is expanded per option — the option joins Forma/capacity legalization as
/// a 9th planned mod (extra unpolarized slot, matching the web UI's exilus
/// model) and the resolve() so any modeled effect applies. Today's exilus
/// mods are damage no-ops, so same-mods candidates differing only in exilus
/// tie on score and differ in Forma/drain — still distinct builds. Pass
/// `&[None]` (or `&[]`, treated the same) for a plain 8-slot search.
#[allow(clippy::too_many_arguments)] // search-config surface; a params struct isn't warranted yet
pub fn enumerate_candidates(
    pool: &[ModDef],
    base: &WeaponBase,
    second_form: Option<&WeaponBase>,
    variant: u32,
    min_slots: u32,
    max_slots: u32,
    cap: u32,
    innate: &[Option<Polarity>],
    constraints: &Constraints,
    exilus_opts: &[Option<&ModDef>],
) -> (Vec<Candidate>, EnumStats) {
    let (out, stats, _complete) = enumerate_candidates_observed(
        pool,
        base,
        second_form,
        variant,
        min_slots,
        max_slots,
        cap,
        innate,
        constraints,
        exilus_opts,
        None,
        0,
    );
    (out, stats)
}

/// [`enumerate_candidates`] with an observer: `state` makes the walk
/// CANCELLABLE (`state.cancel`) and publishes a live candidate count
/// (`state.enumerated`); `max_out > 0` hard-caps the number of emitted
/// candidates (a runaway scope would otherwise eat all memory before the
/// funnel even starts). The third return is `true` iff the walk ran to
/// completion — on `false`, inspect `state.cancel` vs the cap to tell
/// which stop it was.
#[allow(clippy::too_many_arguments)]
pub fn enumerate_candidates_observed(
    pool: &[ModDef],
    base: &WeaponBase,
    second_form: Option<&WeaponBase>,
    variant: u32,
    min_slots: u32,
    max_slots: u32,
    cap: u32,
    innate: &[Option<Polarity>],
    constraints: &Constraints,
    exilus_opts: &[Option<&ModDef>],
    state: Option<&FunnelState>,
    max_out: usize,
) -> (Vec<Candidate>, EnumStats, bool) {
    let usable: Vec<usize> = (0..pool.len())
        .filter(|&i| !constraints.forbid.iter().any(|f| f == pool[i].id))
        .collect();
    let required: Vec<usize> = constraints
        .require
        .iter()
        .filter_map(|r| pool.iter().position(|m| m.id == *r))
        .collect();

    let mut stats = EnumStats::default();
    let mut scratch = Vec::new();
    let mut subset = Vec::with_capacity(max_slots as usize);
    let default_opts = [None];
    let exilus_opts = if exilus_opts.is_empty() {
        &default_opts[..]
    } else {
        exilus_opts
    };
    let mut all: Vec<Candidate> = Vec::new();
    let complete = enumerate_rec(
        pool,
        base,
        second_form,
        variant,
        cap,
        innate,
        exilus_opts,
        &usable,
        &required,
        min_slots as usize,
        max_slots as usize,
        0,
        &mut subset,
        &mut stats,
        &mut scratch,
        &mut |scratch: &mut Vec<Candidate>| {
            for c in scratch.drain(..) {
                if max_out > 0 && all.len() >= max_out {
                    return false;
                }
                all.push(c);
            }
            true
        },
        state,
    );
    (all, stats, complete)
}

/// Streaming enumeration: every candidate goes to `emit` as it is built —
/// nothing is materialized, so the scope size stops being a memory bound.
/// `emit` returning `false` aborts the walk (so does `state.cancel`);
/// returns `true` iff the walk ran to completion. `state` also receives the
/// live `enumerated` count.
#[allow(clippy::too_many_arguments)]
pub fn enumerate_candidates_each(
    pool: &[ModDef],
    base: &WeaponBase,
    second_form: Option<&WeaponBase>,
    variant: u32,
    min_slots: u32,
    max_slots: u32,
    cap: u32,
    innate: &[Option<Polarity>],
    constraints: &Constraints,
    exilus_opts: &[Option<&ModDef>],
    state: Option<&FunnelState>,
    emit: &mut dyn FnMut(Candidate) -> bool,
) -> bool {
    let usable: Vec<usize> = (0..pool.len())
        .filter(|&i| !constraints.forbid.iter().any(|f| f == pool[i].id))
        .collect();
    let required: Vec<usize> = constraints
        .require
        .iter()
        .filter_map(|r| pool.iter().position(|m| m.id == *r))
        .collect();
    let mut stats = EnumStats::default();
    let mut scratch = Vec::new();
    let mut subset = Vec::with_capacity(max_slots as usize);
    let default_opts = [None];
    let exilus_opts = if exilus_opts.is_empty() {
        &default_opts[..]
    } else {
        exilus_opts
    };
    enumerate_rec(
        pool,
        base,
        second_form,
        variant,
        cap,
        innate,
        exilus_opts,
        &usable,
        &required,
        min_slots as usize,
        max_slots as usize,
        0,
        &mut subset,
        &mut stats,
        &mut scratch,
        &mut |scratch: &mut Vec<Candidate>| scratch.drain(..).all(&mut *emit),
        state,
    )
}

/// The one enumeration walk behind both the materialized and streaming
/// fronts. `expand_subset` fills `scratch`; after every expansion the
/// `sink` consumes it (drain into a Vec, cap it, or feed a worker
/// pipeline). Returns `false` when the walk was stopped early — a `false`
/// from the sink or a `state.cancel` — and the abort propagates straight
/// up the recursion.
#[allow(clippy::too_many_arguments)]
fn enumerate_rec<S: FnMut(&mut Vec<Candidate>) -> bool>(
    pool: &[ModDef],
    base: &WeaponBase,
    second_form: Option<&WeaponBase>,
    variant: u32,
    cap: u32,
    innate: &[Option<Polarity>],
    exilus_opts: &[Option<&ModDef>],
    usable: &[usize],
    required: &[usize],
    min: usize,
    max: usize,
    from: usize,
    subset: &mut Vec<usize>,
    stats: &mut EnumStats,
    scratch: &mut Vec<Candidate>,
    sink: &mut S,
    state: Option<&FunnelState>,
) -> bool {
    if let Some(st) = state {
        if st.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return false;
        }
    }
    // Every node in the enumeration tree IS a subset — emit it once, here,
    // when it is big enough and carries every required mod (a subset missing
    // a required mod still recurses: descendants may pick it up).
    if subset.len() >= min && required.iter().all(|r| subset.contains(r)) {
        stats.subsets += 1;
        expand_subset(
            pool,
            base,
            second_form,
            variant,
            cap,
            innate,
            exilus_opts,
            subset,
            stats,
            scratch,
        );
        if let Some(st) = state {
            // fetch_add keeps the counter a TOTAL across evo-set calls.
            st.enumerated
                .fetch_add(scratch.len() as u64, std::sync::atomic::Ordering::Relaxed);
        }
        if !sink(scratch) {
            return false;
        }
        scratch.clear(); // a sink may leave leftovers; the walk owns the scratch
        tick(); // wasm heartbeat — no-op on native
    }
    if subset.len() == max {
        return true;
    }
    // Prune only branches that cannot even reach `min` any more.
    if subset.len() + (usable.len() - from) < min {
        return true;
    }
    for k in from..usable.len() {
        let i = usable[k];
        // Family exclusivity (wiki Incompatible).
        if let Some(f) = pool[i].family {
            if subset.iter().any(|&j| pool[j].family == Some(f)) {
                continue;
            }
        }
        subset.push(i);
        let cont = enumerate_rec(
            pool,
            base,
            second_form,
            variant,
            cap,
            innate,
            exilus_opts,
            usable,
            required,
            min,
            max,
            k + 1,
            subset,
            stats,
            scratch,
            sink,
            state,
        );
        subset.pop();
        if !cont {
            return false;
        }
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn expand_subset(
    pool: &[ModDef],
    base: &WeaponBase,
    second_form: Option<&WeaponBase>,
    variant: u32,
    cap: u32,
    innate: &[Option<Polarity>],
    exilus_opts: &[Option<&ModDef>],
    subset: &[usize],
    stats: &mut EnumStats,
    out: &mut Vec<Candidate>,
) {
    // Legalization is order-independent (drain/polarity multiset only), so
    // it happens once per exilus option, outside the order loop.
    let base_planned: Vec<PlannedMod> = subset
        .iter()
        .map(|&i| PlannedMod {
            base_drain: pool[i].base_drain,
            polarity: pool[i].polarity,
        })
        .collect();

    // Distinct primary elements in this subset (position-sensitive).
    let mut elems: Vec<DamageType> = Vec::new();
    for &i in subset {
        if let Some(t) = pool[i].primary_element() {
            if !elems.contains(&t) {
                elems.push(t);
            }
        }
    }
    let mut orders = Vec::new();
    permutations(&elems, &mut Vec::new(), &mut orders);

    for (xi, xopt) in exilus_opts.iter().enumerate() {
        // Equip-once + family exclusivity across the 8+1 slots (future-proof;
        // today's exilus mods share no family with damage mods).
        if let Some(x) = xopt {
            if subset
                .iter()
                .any(|&i| pool[i].id == x.id || (x.family.is_some() && pool[i].family == x.family))
            {
                continue;
            }
        }
        // The exilus option is a 9th planned mod in an extra unpolarized
        // slot (matching the web UI's exilus model); its drain counts
        // against the cap like any other (game rule).
        let mut planned = base_planned.clone();
        let mut slots_vec;
        let slots: &[Option<Polarity>] = match xopt {
            Some(x) => {
                planned.push(PlannedMod {
                    base_drain: x.base_drain,
                    polarity: x.polarity,
                });
                slots_vec = innate.to_vec();
                slots_vec.push(None);
                &slots_vec
            }
            None => innate,
        };
        let Ok(plan) = plan_forma(cap, slots, &planned) else {
            stats.illegal += 1;
            continue;
        };

        // Order dedup is scoped PER exilus option: same-vector orders are the
        // same build, but the same vector under a different exilus option is
        // a different build (drain/Forma differ).
        let mut seen_vectors: Vec<Vec<(DamageType, i64)>> = Vec::new();
        for order in &orders {
            stats.order_variants += 1;
            // Canonical form: element mods first, grouped by the chosen
            // element order; the (order-free) rest after.
            let mut ordered: Vec<usize> = Vec::with_capacity(subset.len());
            for &t in order {
                ordered.extend(
                    subset
                        .iter()
                        .copied()
                        .filter(|&i| pool[i].primary_element() == Some(t)),
                );
            }
            ordered.extend(
                subset
                    .iter()
                    .copied()
                    .filter(|&i| pool[i].primary_element().is_none()),
            );

            let mut refs: Vec<&ModDef> = ordered.iter().map(|&i| &pool[i]).collect();
            // The exilus mod resolves too (honesty: any modeled effect
            // applies; today's exilus mods are damage no-ops). Last =
            // canonical position; exilus mods carry no primary element.
            if let Some(x) = xopt {
                refs.push(x);
            }
            // On-kill stacks start at ZERO and are earned live (user policy).
            let panel = resolve(base, &refs, StackPolicy::Emergent);

            // Second-level dedup: orders resolving to the same combined
            // vector are the same build (docs/OPTIMIZER.md §1). Deduping on
            // the PRIMARY form's vector is safe for the second form too:
            // both are functions of the element partition, which the vector
            // determines (an injected element pairs with the partition's
            // leftover).
            let key: Vec<(DamageType, i64)> = panel
                .damage
                .iter_nonzero()
                .map(|(t, v)| (t, (v * 1e6).round() as i64))
                .collect();
            if seen_vectors.contains(&key) {
                stats.deduped += 1;
                continue;
            }
            seen_vectors.push(key);
            out.push(Candidate {
                ordered: ordered.clone(),
                panel,
                base_panel: second_form.map(|b| resolve(b, &refs, StackPolicy::Emergent)),
                plan: plan.clone(),
                variant,
                exilus: xi as u32,
            });
        }
    }
}

fn permutations(rest: &[DamageType], acc: &mut Vec<DamageType>, out: &mut Vec<Vec<DamageType>>) {
    if rest.is_empty() {
        out.push(acc.clone());
        return;
    }
    for (i, &t) in rest.iter().enumerate() {
        let mut r = rest.to_vec();
        r.remove(i);
        acc.push(t);
        permutations(&r, acc, out);
        acc.pop();
    }
}

/// The benchmark engagement (target, aim, duration, equipped extras).
/// The arcane is a SEARCH DIMENSION (user, 2026-07-25) — passed per
/// evaluation job, not fixed here.
#[derive(Clone)]
pub struct Scenario {
    pub target: TargetParams,
    pub body_parts: Vec<BodyPart>,
    pub duration_secs: f64,
    /// Run the REAL Incarnon two-form cycle (full gauge start → dump →
    /// revert → rebuild 9 weakpoint charges → transmute → …) instead of
    /// the locked-gauge pseudo-reload model. Needs candidates enumerated
    /// with a second form.
    pub incarnon_cycle: bool,
    /// Frenzy's per-buff lock setting for the base-form phase.
    pub frenzy_lock: LockMode,
    /// Per-buff configured policy applied to every evaluated build (same id
    /// scheme as the web Sim panel). Empty = the emergent default.
    pub buff_cfg: BuffConfig,
}

/// Evaluate one candidate with a given arcane: engine Monte Carlo only.
pub fn evaluate(
    c: &Candidate,
    arcane: &wfsim_engine::arcanes_data::ArcaneFx,
    s: &Scenario,
    runs: u32,
    seed: u64,
) -> Summary {
    let mut params = if s.incarnon_cycle {
        DummyParams::incarnon_cycle_from_panels(
            &c.panel,
            c.base_panel.as_ref().expect("cycle needs the base panel"),
            s.frenzy_lock,
            s.target.clone(),
            s.body_parts.clone(),
            s.duration_secs,
        )
    } else {
        DummyParams::from_panel(
            &c.panel,
            s.target.clone(),
            s.body_parts.clone(),
            s.duration_secs,
        )
    };
    params.arcane = arcane.clone();
    // Per-buff configured policy (weapon-scoped; recurses into the cycle base
    // form). Empty cfg = no-op → the emergent default.
    if !s.buff_cfg.is_empty() {
        params.apply_buff_config(&s.buff_cfg);
    }
    monte_carlo(&params, runs, seed)
}

/// Dominance pruning (prescribed-mods preset): mods whose every effect is the
/// same KIND as another pool mod's but strictly smaller are excluded up
/// front — they can never appear in an optimum (drain differences only
/// change Forma count, never damage ranking). NOTE: plain Barrel
/// Diffusion is BACK in the pool under `EmergentFromZero` — Galvanized
/// Diffusion's unconditional +110% sits below its +120% until a stack is
/// earned, so that dominance no longer holds a priori.
pub fn dominated_mods() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "pistol_gambit",
            "primed_pistol_gambit has strictly more crit chance",
        ),
        (
            "target_cracker",
            "primed_target_cracker has strictly more crit damage",
        ),
        (
            "heated_charge",
            "primed_heated_charge has strictly more heat",
        ),
        (
            "convulsion",
            "primed_convulsion has strictly more electricity",
        ),
        (
            "amalgam_barrel_diffusion",
            "barrel_diffusion has strictly more multishot (109.5% < 120%)",
        ),
    ]
}

/// One evaluation job: a candidate paired with an arcane INDEX into the
/// search's resolved arcane list (the arcane is a search dimension like
/// the mod choice; data-driven `ArcaneFx` is not `Copy`, so jobs carry
/// the index).
pub type Job = (usize, usize);

/// Live progress of a running funnel, shared between the worker threads and
/// an observer (the web UI's status endpoint). Counters are plain atomics so
/// per-job updates stay lock-free; finished-round summaries land in `notes`
/// under a mutex. Setting `cancel` stops the funnel between jobs: the
/// in-flight round is discarded and `run_funnel` returns the last COMPLETED
/// round's leaderboard.
#[derive(Default)]
pub struct FunnelState {
    /// Monte-Carlo runs completed / planned across ALL rounds. The plan is
    /// exact (the schedule fixes every round's field × runs up front), so
    /// `done / planned` is a true overall percentage.
    pub sims_done: AtomicU64,
    pub sims_planned: AtomicU64,
    /// 1-based round in progress, and the total round count.
    pub round: AtomicUsize,
    pub rounds: AtomicUsize,
    /// The in-progress round's field size and per-job run count.
    pub round_jobs: AtomicUsize,
    pub round_runs: AtomicU32,
    /// Observer → funnel: request a stop (checked before each job AND
    /// inside candidate enumeration — a huge scope must stay cancellable).
    pub cancel: AtomicBool,
    /// Candidates emitted so far by a running enumeration (progress for
    /// the "enumerating" phase, where sims_done is still 0).
    pub enumerated: AtomicU64,
    /// One entry per FINISHED round.
    pub notes: Mutex<Vec<RoundNote>>,
}

/// A finished funnel round, for progress display.
#[derive(Debug, Clone, Copy)]
pub struct RoundNote {
    pub round: usize,
    pub jobs: usize,
    pub runs: u32,
    pub by_kills: bool,
    pub kept: usize,
    /// Best score under the round's own metric (kill progress on kill
    /// rounds, mean effective damage on screen rounds).
    pub best: f64,
    pub ms: u64,
}

/// Self-scaling successive-halving schedule with the historical defaults
/// (final = 1024 runs × 24 finalists). See [`schedule_to`].
pub fn schedule(n_jobs: usize) -> Vec<(u32, usize, bool)> {
    schedule_to(n_jobs, 1024, 24)
}

/// Successive-halving schedule honoring the user's FINAL-ROUND CONTRACT
/// (2026-07-28): the last round is guaranteed to evaluate EXACTLY
/// `finalists` candidates at `final_runs` runs each — everything before it
/// only whittles the field down to that size.
///
/// The cadence is AUTO-PLANNED from the inputs (user, 2026-07-28: "derive
/// the elimination rhythm from N directly"):
/// - round count: k = ceil(log₈(N/F)) — "cull at most ×8 per round" is the
///   pace anchor and decides ONLY how many rounds exist;
/// - per-round cull ratio: ρ = (N/F)^(1/k), spread EVENLY in log space so
///   the last cut lands exactly on `finalists` (no floor-clamped tail
///   rounds; a small N/F gets proportionally gentler cuts);
/// - per-round runs: rᵢ = (ρ/2)^i — derived from a halving cost budget
///   (each round costs about half the previous; ρ ≤ 8 keeps growth ≤ ×4),
///   capped at `final_runs / 4` so the final stays a real step up.
///
/// EVERY round ranks by kill score (mean kill progress — just as
/// continuous as the old effective-damage screen, and it IS the
/// objective). The plan is an upper bound, not a promise of work:
/// [`run_funnel`] adapts it both ways at runtime (3σ racing cuts deeper,
/// tie amnesty keeps up to 2×, empty rounds are skipped).
pub fn schedule_to(n_jobs: usize, final_runs: u32, finalists: usize) -> Vec<(u32, usize, bool)> {
    let finalists = finalists.max(1);
    let final_runs = final_runs.max(1);
    let mut rounds = Vec::new();
    if n_jobs > finalists {
        let ratio_total = n_jobs as f64 / finalists as f64;
        let k = (ratio_total.ln() / 8f64.ln()).ceil().max(1.0) as usize;
        let rho = ratio_total.powf(1.0 / k as f64);
        let growth = (rho / 2.0).max(1.0);
        let cap = (final_runs / 4).max(1);
        let mut field = n_jobs as f64;
        let mut runs_f = 1.0f64;
        for i in 0..k {
            let keep = if i + 1 == k {
                finalists
            } else {
                ((field / rho).round() as usize).max(finalists)
            };
            rounds.push(((runs_f.round() as u32).clamp(1, cap), keep, true));
            field = keep as f64;
            runs_f *= growth;
        }
    }
    rounds.push((final_runs, finalists, true));
    rounds
}

/// Worker-thread budget for [`evaluate_batch`] / [`stream_screen`]. 0 =
/// auto: ALL CORES MINUS TWO — the optimizer must not freeze the machine
/// it runs on (user, 2026-07-29: the full-core default made the whole
/// system stutter). Set per request via [`set_worker_threads`]; the seeds
/// never depend on the thread count, so any setting reproduces the same
/// numbers.
static WORKER_THREADS: AtomicUsize = AtomicUsize::new(0);

pub fn set_worker_threads(n: usize) {
    WORKER_THREADS.store(n, Ordering::Relaxed);
}

#[cfg(not(target_arch = "wasm32"))] // wasm is single-threaded; the budget is native-only
fn worker_threads() -> usize {
    let n = WORKER_THREADS.load(Ordering::Relaxed);
    if n > 0 {
        return n;
    }
    std::thread::available_parallelism()
        .map(|c| c.get())
        .unwrap_or(8)
        .saturating_sub(2)
        .max(1)
}

/// Drop the CURRENT thread to below-normal scheduling priority (Windows;
/// no-op elsewhere): the optimizer soaks idle cycles at full speed but
/// yields the moment the user does anything interactive.
pub fn deprioritize_current_thread() {
    #[cfg(windows)]
    unsafe {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThread() -> *mut core::ffi::c_void;
            fn SetThreadPriority(h: *mut core::ffi::c_void, p: i32) -> i32;
        }
        const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

// ---- wasm busy-loop progress hook --------------------------------------
// A single-threaded Web Worker cannot be polled while it computes — before
// this hook, the whole enumeration/screen phase was SILENT and a big scope
// looked dead (user, 2026-07-29: "it just doesn't compute"). The hot loops
// call `tick()`; the wasm host installs a throttled hook that posts live
// status out of the worker. Native builds compile `tick()` to a no-op —
// the status endpoint polls `FunnelState` instead.
#[cfg(target_arch = "wasm32")]
thread_local! {
    static TICK_HOOK: std::cell::RefCell<Option<Box<dyn Fn()>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
pub fn set_tick_hook(hook: Option<Box<dyn Fn()>>) {
    TICK_HOOK.with(|h| *h.borrow_mut() = hook);
}

/// Native no-op twin — lets the wasm host crate compile for the host
/// target too (the workspace builds it there for tests/clippy).
#[cfg(not(target_arch = "wasm32"))]
pub fn set_tick_hook(_hook: Option<Box<dyn Fn()>>) {}

#[inline]
pub fn tick() {
    #[cfg(target_arch = "wasm32")]
    TICK_HOOK.with(|h| {
        if let Some(f) = h.borrow().as_ref() {
            f();
        }
    });
}

/// Deterministic per-job seed, mixed per round. One definition for both
/// evaluation strategies: the seed depends only on (candidate, arcane), never
/// on thread count or chunking, so serial wasm evaluation reproduces native
/// results bit-for-bit.
fn job_seed(seed: u64, ci: usize, ai: usize) -> u64 {
    seed ^ (ci as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ ((ai as u64) << 56)
}

/// Evaluate jobs concurrently across all cores (single-threaded on wasm32 —
/// same seeds, same order, identical results, just serial). Returns summaries
/// index-aligned with `jobs`; entries are `None` only when a cancel
/// request stopped the batch before that job ran.
#[cfg(not(target_arch = "wasm32"))]
pub fn evaluate_batch(
    cands: &[Candidate],
    jobs: &[Job],
    arcanes: &[wfsim_engine::arcanes_data::ArcaneFx],
    scenario: &Scenario,
    runs: u32,
    seed: u64,
    state: Option<&FunnelState>,
) -> Vec<Option<Summary>> {
    let threads = worker_threads();
    let chunk = jobs.len().div_ceil(threads).max(1);
    let mut results: Vec<Option<Summary>> = vec![None; jobs.len()];
    std::thread::scope(|scope| {
        for (ids, res) in jobs.chunks(chunk).zip(results.chunks_mut(chunk)) {
            let scenario = scenario.clone();
            scope.spawn(move || {
                deprioritize_current_thread();
                for (k, &(ci, ai)) in ids.iter().enumerate() {
                    if state.is_some_and(|st| st.cancel.load(Ordering::Relaxed)) {
                        return;
                    }
                    res[k] = Some(evaluate(
                        &cands[ci],
                        &arcanes[ai],
                        &scenario,
                        runs,
                        job_seed(seed, ci, ai),
                    ));
                    if let Some(st) = state {
                        st.sims_done.fetch_add(runs as u64, Ordering::Relaxed);
                    }
                }
            });
        }
    });
    results
}

/// wasm32 (docs/WASM.md phase 3): no threads in a Web Worker — evaluate the
/// jobs sequentially with the identical per-job seeds.
#[cfg(target_arch = "wasm32")]
pub fn evaluate_batch(
    cands: &[Candidate],
    jobs: &[Job],
    arcanes: &[wfsim_engine::arcanes_data::ArcaneFx],
    scenario: &Scenario,
    runs: u32,
    seed: u64,
    state: Option<&FunnelState>,
) -> Vec<Option<Summary>> {
    let mut results: Vec<Option<Summary>> = vec![None; jobs.len()];
    for (k, &(ci, ai)) in jobs.iter().enumerate() {
        if state.is_some_and(|st| st.cancel.load(Ordering::Relaxed)) {
            break;
        }
        results[k] = Some(evaluate(
            &cands[ci],
            &arcanes[ai],
            scenario,
            runs,
            job_seed(seed, ci, ai),
        ));
        if let Some(st) = state {
            st.sims_done.fetch_add(runs as u64, Ordering::Relaxed);
        }
        tick(); // wasm heartbeat: intra-round progress leaves the worker
    }
    results
}

/// One streamed job that survived the screen: the candidate (shared — the
/// same build may survive with several arcanes), its arcane index and the
/// screen-run summary.
pub struct ScreenedJob {
    pub cand: std::sync::Arc<Candidate>,
    pub ai: usize,
    pub summary: Summary,
}

/// Screen ordering: kill progress, then effective damage, then earliest
/// (seq, arcane) — a STRICT total order, so the surviving top-K set is
/// unique regardless of worker interleaving.
struct Scored {
    kp: f64,
    eff: f64,
    seq: usize,
    ai: usize,
    cand: std::sync::Arc<Candidate>,
    summary: Summary,
}
impl PartialEq for Scored {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == std::cmp::Ordering::Equal
    }
}
impl Eq for Scored {}
impl PartialOrd for Scored {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Scored {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.kp
            .total_cmp(&o.kp)
            .then(self.eff.total_cmp(&o.eff))
            .then(o.seq.cmp(&self.seq)) // earlier candidate wins exact ties
            .then(o.ai.cmp(&self.ai))
    }
}

/// Screen an UNBOUNDED candidate stream: `produce` drives the enumeration
/// and hands each candidate over; the screen evaluates it against every
/// arcane at `runs` (typically 1) and keeps only the best `keep`
/// (candidate, arcane) jobs — memory stays O(keep) however large the scope
/// is (this is what makes a no-cap optimizer possible). Returns the
/// survivors best-first plus `true` iff the stream ran to completion
/// (`false` = cancelled — the survivors are then a best-so-far). Per-job
/// seeds derive from the candidate's global sequence number, so a given
/// scope screens deterministically.
#[cfg(not(target_arch = "wasm32"))]
pub fn stream_screen(
    produce: impl FnOnce(&mut dyn FnMut(Candidate) -> bool),
    arcanes: &[wfsim_engine::arcanes_data::ArcaneFx],
    scenario: &Scenario,
    runs: u32,
    keep: usize,
    seed: u64,
    state: Option<&FunnelState>,
) -> (Vec<ScreenedJob>, bool) {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    use std::sync::Arc;
    let threads = worker_threads();
    let (tx, rx) = std::sync::mpsc::sync_channel::<(usize, Arc<Candidate>)>(4096);
    let rx = std::sync::Mutex::new(rx);
    let top: std::sync::Mutex<BinaryHeap<Reverse<Scored>>> =
        std::sync::Mutex::new(BinaryHeap::new());
    // Fast-path floor: bits of the k-th kill progress once the heap is full
    // (kp ≥ 0 → to_bits is order-preserving). Strictly-below scores skip
    // the lock; boundary ties take the slow path and resolve under it.
    let floor = std::sync::atomic::AtomicU64::new(0);
    let full = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            let (rx, top, floor, full) = (&rx, &top, &floor, &full);
            scope.spawn(move || {
                deprioritize_current_thread();
                loop {
                    let msg = rx.lock().unwrap().recv();
                    let Ok((seq, cand)) = msg else { return };
                    for (ai, arc) in arcanes.iter().enumerate() {
                        if state.is_some_and(|st| st.cancel.load(Ordering::Relaxed)) {
                            return;
                        }
                        let s = evaluate(&cand, arc, scenario, runs, job_seed(seed, seq, ai));
                        if let Some(st) = state {
                            st.sims_done.fetch_add(runs as u64, Ordering::Relaxed);
                        }
                        let kp = s.mean_kill_progress.max(0.0);
                        if full.load(Ordering::Relaxed)
                            && kp.to_bits() < floor.load(Ordering::Relaxed)
                        {
                            continue;
                        }
                        let item = Scored {
                            kp,
                            eff: s.mean_effective_damage,
                            seq,
                            ai,
                            cand: cand.clone(),
                            summary: s,
                        };
                        let mut h = top.lock().unwrap();
                        if h.len() < keep {
                            h.push(Reverse(item));
                            if h.len() == keep {
                                floor.store(h.peek().unwrap().0.kp.to_bits(), Ordering::Relaxed);
                                full.store(true, Ordering::Relaxed);
                            }
                        } else if h.peek().is_some_and(|Reverse(min)| item > *min) {
                            h.pop();
                            h.push(Reverse(item));
                            floor.store(h.peek().unwrap().0.kp.to_bits(), Ordering::Relaxed);
                        }
                    }
                }
            });
        }
        // The enumeration runs HERE; a full channel blocks send() — that
        // backpressure is the memory bound.
        let mut seq = 0usize;
        produce(&mut |c: Candidate| {
            if state.is_some_and(|st| st.cancel.load(Ordering::Relaxed)) {
                return false;
            }
            let ok = tx.send((seq, Arc::new(c))).is_ok();
            seq += 1;
            ok
        });
        drop(tx); // close the channel: workers drain and exit, the scope joins them
    });
    let mut out: Vec<Scored> = top.into_inner().unwrap().into_iter().map(|r| r.0).collect();
    out.sort_by(|a, b| b.cmp(a));
    let complete = !state.is_some_and(|st| st.cancel.load(Ordering::Relaxed));
    (
        out.into_iter()
            .map(|s| ScreenedJob {
                cand: s.cand,
                ai: s.ai,
                summary: s.summary,
            })
            .collect(),
        complete,
    )
}

/// wasm32 (docs/WASM.md phase 3): no threads in a Web Worker — the same
/// screen runs inline on the producer, identical seeds and survivor set.
#[cfg(target_arch = "wasm32")]
pub fn stream_screen(
    produce: impl FnOnce(&mut dyn FnMut(Candidate) -> bool),
    arcanes: &[wfsim_engine::arcanes_data::ArcaneFx],
    scenario: &Scenario,
    runs: u32,
    keep: usize,
    seed: u64,
    state: Option<&FunnelState>,
) -> (Vec<ScreenedJob>, bool) {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    use std::sync::Arc;
    let mut top: BinaryHeap<Reverse<Scored>> = BinaryHeap::new();
    let mut seq = 0usize;
    produce(&mut |c: Candidate| {
        if state.is_some_and(|st| st.cancel.load(Ordering::Relaxed)) {
            return false;
        }
        let cand = Arc::new(c);
        for (ai, arc) in arcanes.iter().enumerate() {
            let s = evaluate(&cand, arc, scenario, runs, job_seed(seed, seq, ai));
            if let Some(st) = state {
                st.sims_done.fetch_add(runs as u64, Ordering::Relaxed);
            }
            let item = Scored {
                kp: s.mean_kill_progress.max(0.0),
                eff: s.mean_effective_damage,
                seq,
                ai,
                cand: cand.clone(),
                summary: s,
            };
            if top.len() < keep {
                top.push(Reverse(item));
            } else if top.peek().is_some_and(|Reverse(min)| item > *min) {
                top.pop();
                top.push(Reverse(item));
            }
        }
        seq += 1;
        true
    });
    let mut out: Vec<Scored> = top.into_iter().map(|r| r.0).collect();
    out.sort_by(|a, b| b.cmp(a));
    let complete = !state.is_some_and(|st| st.cancel.load(Ordering::Relaxed));
    (
        out.into_iter()
            .map(|s| ScreenedJob {
                cand: s.cand,
                ai: s.ai,
                summary: s.summary,
            })
            .collect(),
        complete,
    )
}

/// Round wall-clock, compiled out on wasm32: `std::time::Instant` does not
/// exist on wasm32-unknown-unknown (it would panic at runtime), so there
/// `ms()` reports 0 — progress display simply shows no round timing.
struct RoundTimer {
    #[cfg(not(target_arch = "wasm32"))]
    t: std::time::Instant,
}

impl RoundTimer {
    fn start() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            t: std::time::Instant::now(),
        }
    }
    fn ms(&self) -> u64 {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.t.elapsed().as_millis() as u64
        }
        #[cfg(target_arch = "wasm32")]
        {
            0
        }
    }
}

/// Drive the multi-round funnel: for each `(runs, keep, by_kills)` round,
/// evaluate the surviving jobs, sort (kill-progress on kill rounds, effective
/// damage on screen rounds), and cull to `keep`. Returns the final sorted,
/// truncated leaderboard — `[0]` is the winner, `[..10]` the top-10. `verbose`
/// prints per-round progress (the CLI wants it; the web endpoint reads the
/// same numbers off `state` instead). `state` (optional) receives live
/// progress and carries the cancel flag: on cancel the in-flight round is
/// discarded and the last COMPLETED round's leaderboard is returned.
///
/// `on_round` (optional) fires after every COMPLETED round (docs/WASM.md
/// phase 3): single-threaded wasm cannot poll `state` from outside a busy
/// worker, so the callback is where progress leaves the funnel — the caller
/// reads `state` inside it. Native callers pass `None` and poll instead.
#[allow(clippy::too_many_arguments)] // search-config surface, like enumerate_candidates
pub fn run_funnel(
    cands: &[Candidate],
    arcanes: &[wfsim_engine::arcanes_data::ArcaneFx],
    scenario: &Scenario,
    mut alive: Vec<Job>,
    rounds: &[(u32, usize, bool)],
    seed_base: u64,
    verbose: bool,
    state: Option<&FunnelState>,
    on_round: Option<&dyn Fn()>,
) -> Vec<(Job, Summary)> {
    if let Some(st) = state {
        st.rounds.store(rounds.len(), Ordering::Relaxed);
        // The schedule fixes every round's field size up front (truncate is a
        // no-op when the field is already below `keep`), so the total sim
        // count is exact — done/planned is a true percentage.
        let mut field = alive.len() as u64;
        let mut sims = 0u64;
        for &(runs, keep, _) in rounds {
            sims += field * runs as u64;
            field = field.min(keep as u64);
        }
        st.sims_planned.store(sims, Ordering::Relaxed);
    }
    let mut last: Vec<(Job, Summary)> = Vec::new();
    let floor = rounds.last().map_or(1, |&(_, f, _)| f);
    for (round, &(runs, keep, by_kills)) in rounds.iter().enumerate() {
        // Racing/amnesty may reach the finalists count early — remaining
        // intermediate rounds have nothing left to cut; jump to the final.
        if round + 1 < rounds.len() && alive.len() <= floor {
            continue;
        }
        if let Some(st) = state {
            if st.cancel.load(Ordering::Relaxed) {
                break;
            }
            st.round.store(round + 1, Ordering::Relaxed);
            st.round_jobs.store(alive.len(), Ordering::Relaxed);
            st.round_runs.store(runs, Ordering::Relaxed);
        }
        let t = RoundTimer::start();
        let started = alive.len();
        let summaries = evaluate_batch(
            cands,
            &alive,
            arcanes,
            scenario,
            runs,
            seed_base + round as u64,
            state,
        );
        if state.is_some_and(|st| st.cancel.load(Ordering::Relaxed)) {
            // Cancelled mid-round. The previous COMPLETED round's leaderboard
            // is preferred (uniform estimates) — but when no round ever
            // finished (a huge round 1), rank whatever DID evaluate: a rough
            // best-so-far beats returning nothing (user, 2026-07-28).
            if last.is_empty() {
                let mut partial: Vec<(Job, Summary)> = alive
                    .iter()
                    .copied()
                    .zip(summaries)
                    .filter_map(|(j, s)| s.map(|s| (j, s)))
                    .collect();
                partial.sort_by(|a, b| b.1.mean_kill_progress.total_cmp(&a.1.mean_kill_progress));
                partial.truncate(keep);
                last = partial;
            }
            break;
        }
        let mut scored: Vec<(Job, Summary)> = alive
            .iter()
            .copied()
            .zip(summaries)
            .map(|(j, s)| (j, s.expect("uncancelled batch evaluates every job")))
            .collect();
        // Kill rounds rank by kill PROGRESS (kills + depleted fraction of the
        // final target's pool); screen rounds by mean effective damage.
        scored.sort_by(|a, b| {
            let ka = if by_kills {
                a.1.mean_kill_progress
            } else {
                a.1.mean_effective_damage
            };
            let kb = if by_kills {
                b.1.mean_kill_progress
            } else {
                b.1.mean_effective_damage
            };
            kb.total_cmp(&ka)
        });
        // SOFT cut line (user, 2026-07-28: "can the fixed 1/8 be dynamic?"):
        // the planned 1/8 keep stays as the BUDGET SKELETON — predictable
        // cost, guaranteed progress — but the line itself is statistical,
        // not a hard rank. Candidates below the line whose score still TIES
        // the cut-line score (within a ±3·SE band, SE from the field's
        // POOLED per-run σ — thousands of jobs give the pooled estimate
        // huge effective dof even at 2 runs; at 1 run, no σ exists anywhere,
        // so a small relative gap stands in) get amnesty, capped at 2× the
        // plan. Rank order AT the line is noise — a hard cut would gamble
        // true contenders away; the cap keeps the budget bounded. The final
        // round never extends (its field is the contract), but the round
        // FEEDING it may — ties with the last finalist deserve the full-runs
        // final to settle them.
        let planned = keep.min(scored.len());
        let keep_n = if scored.len() > planned && round + 1 < rounds.len() {
            let cap = (planned * 2).min(scored.len());
            let cut_score = scored[planned - 1].1.mean_kill_progress;
            let tol = if runs >= 2 {
                let pooled = (scored
                    .iter()
                    .map(|(_, s)| s.std_kills * s.std_kills)
                    .sum::<f64>()
                    / scored.len() as f64)
                    .sqrt();
                3.0 * pooled / f64::from(runs).sqrt()
            } else {
                cut_score.abs() * 0.05
            };
            let mut k = planned;
            while k < cap && scored[k].1.mean_kill_progress >= cut_score - tol {
                k += 1;
            }
            k
        } else {
            planned
        };
        scored.truncate(keep_n);
        // Adaptive racing cull (user, 2026-07-28: "reduce cleverly before
        // the final"): beyond the planned 1/8, drop every survivor whose 3σ
        // upper confidence bound still misses the finalists boundary's 3σ
        // lower bound — statistically hopeless candidates never see another
        // (4× more expensive) round. Needs runs ≥ 4 for a usable per-job σ;
        // the final round's field is untouchable.
        if round + 1 < rounds.len() && runs >= 4 && scored.len() > floor {
            let se3 = |s: &Summary| 3.0 * s.std_kills / f64::from(runs).sqrt();
            let cut = {
                let b = &scored[floor - 1].1;
                b.mean_kill_progress - se3(b)
            };
            let mut i = 0usize;
            scored.retain(|(_, s)| {
                let keep_it = i < floor || s.mean_kill_progress + se3(s) >= cut;
                i += 1;
                keep_it
            });
        }
        if verbose {
            println!(
                "[round {}] {} jobs x {} runs ({}) -> keep {} in {:.1}s; best {}",
                round + 1,
                started,
                runs,
                if by_kills { "kills" } else { "eff dmg" },
                scored.len(),
                t.ms() as f64 / 1000.0,
                if by_kills {
                    format!("{:.2} kill score", scored[0].1.mean_kill_progress)
                } else {
                    format!("{:.3e} eff", scored[0].1.mean_effective_damage)
                }
            );
        }
        if let Some(st) = state {
            let best = if by_kills {
                scored[0].1.mean_kill_progress
            } else {
                scored[0].1.mean_effective_damage
            };
            st.notes.lock().unwrap().push(RoundNote {
                round: round + 1,
                jobs: started,
                runs,
                by_kills,
                kept: scored.len(),
                best,
                ms: t.ms(),
            });
        }
        alive = scored.iter().map(|(j, _)| *j).collect();
        last = scored;
        if let Some(st) = state {
            // Replan the remaining work: adaptive culls shrink every later
            // round (and rounds the early-exit will skip cost nothing), so
            // done/planned stays a true percentage.
            let mut field = alive.len() as u64;
            let mut sims = st.sims_done.load(Ordering::Relaxed);
            for (idx2, &(r2, k2, _)) in rounds.iter().enumerate().skip(round + 1) {
                if idx2 + 1 == rounds.len() || field > floor as u64 {
                    sims += field * u64::from(r2);
                }
                field = field.min(k2 as u64);
            }
            st.sims_planned.store(sims, Ordering::Relaxed);
        }
        if let Some(cb) = on_round {
            cb();
        }
    }
    last
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_loads_from_yaml_with_family_exclusivity() {
        // The pool is data-driven (data/mods/*.yaml); it grows as mods are
        // added, so assert structure, not a fixed count.
        let p = pool();
        assert!(p.len() >= 26, "pool has {} mods", p.len());
        let diffusions = p
            .iter()
            .filter(|m| m.family == Some("barrel_diffusion"))
            .count();
        assert_eq!(diffusions, 3, "barrel_diffusion family exclusivity");
    }

    #[test]
    fn canonical_enumeration_counts_match_the_generating_function() {
        // Family-exclusive 8-mod subsets = coefficient of x^8 in
        //   Π_families (1 + size·x) · (1+x)^singles.
        // Validated on a FIXED 12-mod sub-pool: keeps the algorithm test fast
        // and stable as the full data-driven pool grows (the optimizer never
        // enumerates the whole pool in practice — it searches a scoped subset).
        let ids = [
            "hornet_strike",
            "barrel_diffusion",
            "amalgam_barrel_diffusion",
            "galvanized_diffusion",
            "pistol_gambit",
            "primed_pistol_gambit",
            "creeping_bullseye",
            "target_cracker",
            "primed_target_cracker",
            "lethal_torrent",
            "frostbite",
            "jolt",
        ];
        let p: Vec<ModDef> = pool().into_iter().filter(|m| ids.contains(&m.id)).collect();
        assert_eq!(p.len(), ids.len(), "test sub-pool ids all present");
        let mut fam: std::collections::HashMap<&str, u64> = std::collections::HashMap::new();
        let mut singles = 0u32;
        for m in &p {
            match m.family {
                Some(f) => *fam.entry(f).or_default() += 1,
                None => singles += 1,
            }
        }
        let mul = |a: &[u64], b: &[u64]| {
            let mut out = vec![0u64; a.len() + b.len() - 1];
            for (i, &x) in a.iter().enumerate() {
                for (j, &y) in b.iter().enumerate() {
                    out[i + j] += x * y;
                }
            }
            out
        };
        let mut poly = vec![1u64];
        for &size in fam.values() {
            poly = mul(&poly, &[1, size]);
        }
        for _ in 0..singles {
            poly = mul(&poly, &[1, 1]);
        }
        let expected = poly.get(8).copied().unwrap_or(0);

        let base = WeaponBase::from_data(
            "dual_toxocyst_incarnon",
            true,
            &[
                "dt_commodores_fortune",
                "dt_evolved_autoloader",
                "dt_fevered_frenzy",
            ],
        );
        let (cands, stats) = enumerate_candidates(
            &p,
            &base,
            Some(&WeaponBase::from_data(
                "dual_toxocyst",
                true,
                &[
                    "dt_commodores_fortune",
                    "dt_evolved_autoloader",
                    "dt_fevered_frenzy",
                ],
            )),
            0,
            8,
            8,
            60,
            &wfsim_engine::weapons_data::innate_slots("dual_toxocyst"),
            &Constraints::default(),
            &[None],
        );
        assert_eq!(
            stats.subsets, expected,
            "subset count vs generating function"
        );
        assert_eq!(
            cands.len() as u64 + stats.deduped,
            stats.order_variants,
            "every order variant is kept or deduped"
        );
        // Sanity: every candidate is exactly 8 mods and within capacity.
        assert!(cands.iter().all(|c| c.ordered.len() == 8));
        assert!(cands.iter().all(|c| c.plan.total_drain <= 60));

        // Slots may be left EMPTY: min 0 enumerates every size ≤ 8, whose
        // subset count is the SUM of the generating function's coefficients
        // 0..=8 (the empty build included).
        let expected_le: u64 = (0..=8).map(|k| poly.get(k).copied().unwrap_or(0)).sum();
        let (cands_le, stats_le) = enumerate_candidates(
            &p,
            &base,
            Some(&WeaponBase::from_data(
                "dual_toxocyst",
                true,
                &[
                    "dt_commodores_fortune",
                    "dt_evolved_autoloader",
                    "dt_fevered_frenzy",
                ],
            )),
            0,
            0,
            8,
            60,
            &wfsim_engine::weapons_data::innate_slots("dual_toxocyst"),
            &Constraints::default(),
            &[None],
        );
        assert_eq!(
            stats_le.subsets, expected_le,
            "≤8 subset count vs Σ coefficients"
        );
        assert!(
            cands_le.iter().any(|c| c.ordered.is_empty()),
            "the empty build is a candidate"
        );
        assert!(cands_le.iter().all(|c| c.ordered.len() <= 8));
    }

    #[test]
    fn exilus_is_a_search_dimension_with_real_drain() {
        // A fixed 8-mod scope has exactly one subset. With exilus options
        // [empty, mod] the space doubles: same mods, different exilus-slot
        // choice — and the occupied option must cost something (more drain,
        // or more Forma to squeeze back under the cap).
        let ids = [
            "hornet_strike",
            "barrel_diffusion",
            "primed_pistol_gambit",
            "primed_target_cracker",
            "lethal_torrent",
            "frostbite",
            "jolt",
            "magnum_force",
        ];
        let p: Vec<ModDef> = pool().into_iter().filter(|m| ids.contains(&m.id)).collect();
        assert_eq!(p.len(), ids.len());
        let full = wfsim_engine::mods_data::pistol_pool();
        let ex = full
            .iter()
            .find(|m| m.exilus)
            .expect("an exilus mod exists")
            .clone();

        let base = WeaponBase::from_data(
            "dual_toxocyst_incarnon",
            true,
            &[
                "dt_commodores_fortune",
                "dt_evolved_autoloader",
                "dt_fevered_frenzy",
            ],
        );
        let run = |opts: &[Option<&ModDef>]| {
            enumerate_candidates(
                &p,
                &base,
                None,
                0,
                8,
                8,
                60,
                &wfsim_engine::weapons_data::innate_slots("dual_toxocyst"),
                &Constraints::default(),
                opts,
            )
        };
        let (empty_only, _) = run(&[None]);
        let (both, _) = run(&[None, Some(&ex)]);
        assert_eq!(
            both.len(),
            empty_only.len() * 2,
            "each exilus option expands every build"
        );
        let w0 = &both.iter().find(|c| c.exilus == 0).unwrap().plan;
        let x0 = &both.iter().find(|c| c.exilus == 1).unwrap().plan;
        assert!(
            x0.total_drain > w0.total_drain || x0.forma_used > w0.forma_used,
            "exilus drain must count (drain {} -> {}, forma {} -> {})",
            w0.total_drain,
            x0.total_drain,
            w0.forma_used,
            x0.forma_used
        );
        // The occupied option plans a real 9th slot; the empty one stays 8.
        assert_eq!(x0.slots.len(), 9);
        assert_eq!(w0.slots.len(), 8);
    }

    #[test]
    fn schedule_plans_the_cadence_from_the_inputs() {
        // ~2M jobs, defaults (1024 × 24): k = ceil(log8(N/F)) rounds, even
        // log-space culls ending EXACTLY on the finalists, runs from a
        // halving cost budget, every round by kill score.
        let s = schedule(1_950_192);
        let n = s.len();
        assert_eq!(n, 7, "k = ceil(log8(81258)) = 6 intermediates + final");
        assert_eq!(s[0].0, 1, "the screen starts at 1 run");
        assert!(s.windows(2).all(|w| w[1].0 >= w[0].0 && w[1].1 <= w[0].1));
        assert!(
            s.iter().all(|&(_, _, by_kills)| by_kills),
            "kill score everywhere"
        );
        assert!(
            s[..n - 1].iter().all(|&(r, _, _)| r <= 256),
            "intermediates ≤ final/4"
        );
        assert_eq!(s.last(), Some(&(1024, 24, true)));
        // The round BEFORE the final already reaches the finalists count —
        // the final evaluates exactly that field (the contract).
        assert_eq!(s[n - 2].1, 24);
        // Cull ratios are even: every intermediate cut is ≤ ×8 and the
        // ratios stay within rounding of each other.
        let mut field = 1_950_192usize;
        for &(_, keep, _) in &s[..n - 1] {
            let r = field as f64 / keep as f64;
            assert!(r <= 8.01, "cull ratio {r}");
            field = keep;
        }
        // Total sims stays well below flat evaluation (halving budget ≈ 2N).
        let mut field = 1_950_192usize;
        let mut sims = 0usize;
        for &(runs, keep, _) in &s {
            sims += field * runs as usize;
            field = field.min(keep);
        }
        assert!(sims < 3 * 1_950_192, "sims {sims}");
        // A small N/F gap plans proportionally GENTLER cuts (no ÷8 overshoot
        // straight into the floor).
        let small = schedule(500);
        assert_eq!(small.len(), 3, "two intermediates + final");
        assert_eq!(small.first(), Some(&(1, 110, true)));
        assert_eq!(small[1].1, 24);
        assert_eq!(small.last(), Some(&(1024, 24, true)));

        // The user's contract verbatim: final = 10_000 runs × 20 finalists.
        let big = schedule_to(437_000, 10_000, 20);
        assert_eq!(big.last(), Some(&(10_000, 20, true)));
        assert_eq!(big[big.len() - 2].1, 20);
        assert!(big[..big.len() - 1].iter().all(|&(r, _, _)| r <= 2500));
        // Tiny fields skip straight to the final.
        assert_eq!(schedule_to(15, 10_000, 20), vec![(10_000, 20, true)]);
    }

    #[test]
    #[ignore = "enumerates the FULL pool; explodes now that the pistol pool grew \
                to ~80 mods (C(73,7)). The optimizer is being re-planned around a \
                UI-selected scoped subset (2026-07-26) — re-enable against a scope."]
    fn constraints_filter_the_space() {
        let p = pool();
        let base = WeaponBase::from_data(
            "dual_toxocyst_incarnon",
            true,
            &[
                "dt_commodores_fortune",
                "dt_evolved_autoloader",
                "dt_fevered_frenzy",
            ],
        );
        let cons = Constraints {
            require: vec!["hornet_strike".into()],
            forbid: vec!["magnetic_might".into()],
        };
        let (cands, _) = enumerate_candidates(
            &p,
            &base,
            None,
            0,
            8,
            8,
            60,
            &wfsim_engine::weapons_data::innate_slots("dual_toxocyst"),
            &cons,
            &[None],
        );
        assert!(!cands.is_empty());
        let hornet = p.iter().position(|m| m.id == "hornet_strike").unwrap();
        let mm = p.iter().position(|m| m.id == "magnetic_might").unwrap();
        assert!(cands.iter().all(|c| c.ordered.contains(&hornet)));
        assert!(cands.iter().all(|c| !c.ordered.contains(&mm)));
    }
}
