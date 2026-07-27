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

use wfsim_engine::damage::DamageType;
use wfsim_engine::dummy::{monte_carlo, BodyPart, DummyParams, LockMode, Summary, TargetParams};
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
    wfsim_engine::mods_data::pistol_pool().into_iter().filter(|m| !m.exilus).collect()
}

/// Dual Toxocyst's innate slot polarities (fully unlocked: Madurai +
/// Naramon; the Naramon exilus is utility-only and outside this search).
pub fn dual_toxocyst_innate_slots() -> [Option<Polarity>; 8] {
    let mut s = [None; 8];
    s[0] = Some(Polarity::Madurai);
    s[1] = Some(Polarity::Naramon);
    s
}

/// "命题作文" constraints: forced inclusions/exclusions by mod id.
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
    /// Weapon-config variant label (the Evolution II choice — a search
    /// dimension enumerated by resolving against each variant's base).
    pub variant: &'static str,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EnumStats {
    pub subsets: u64,
    pub illegal: u64,
    pub order_variants: u64,
    pub deduped: u64,
}

/// Enumerate all canonical candidates: 8-mod subsets (family-exclusive,
/// constraint-filtered) × distinct-element orders, legalized and deduped by
/// resolved damage vector.
#[allow(clippy::too_many_arguments)] // search-config surface; a params struct isn't warranted yet
pub fn enumerate_candidates(
    pool: &[ModDef],
    base: &WeaponBase,
    second_form: Option<&WeaponBase>,
    variant: &'static str,
    slots: u32,
    cap: u32,
    innate: &[Option<Polarity>],
    constraints: &Constraints,
) -> (Vec<Candidate>, EnumStats) {
    let usable: Vec<usize> = (0..pool.len())
        .filter(|&i| !constraints.forbid.iter().any(|f| f == pool[i].id))
        .collect();
    let required: Vec<usize> = constraints
        .require
        .iter()
        .filter_map(|r| pool.iter().position(|m| m.id == *r))
        .collect();

    let mut stats = EnumStats::default();
    let mut out = Vec::new();
    let mut subset = Vec::with_capacity(slots as usize);
    enumerate_rec(
        pool,
        base,
        second_form,
        variant,
        cap,
        innate,
        &usable,
        &required,
        slots as usize,
        0,
        &mut subset,
        &mut stats,
        &mut out,
    );
    (out, stats)
}

#[allow(clippy::too_many_arguments)]
fn enumerate_rec(
    pool: &[ModDef],
    base: &WeaponBase,
    second_form: Option<&WeaponBase>,
    variant: &'static str,
    cap: u32,
    innate: &[Option<Polarity>],
    usable: &[usize],
    required: &[usize],
    want: usize,
    from: usize,
    subset: &mut Vec<usize>,
    stats: &mut EnumStats,
    out: &mut Vec<Candidate>,
) {
    if subset.len() == want {
        if required.iter().all(|r| subset.contains(r)) {
            stats.subsets += 1;
            expand_subset(
                pool,
                base,
                second_form,
                variant,
                cap,
                innate,
                subset,
                stats,
                out,
            );
        }
        return;
    }
    if usable.len() - from < want - subset.len() {
        return;
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
        enumerate_rec(
            pool,
            base,
            second_form,
            variant,
            cap,
            innate,
            usable,
            required,
            want,
            k + 1,
            subset,
            stats,
            out,
        );
        subset.pop();
    }
}

#[allow(clippy::too_many_arguments)]
fn expand_subset(
    pool: &[ModDef],
    base: &WeaponBase,
    second_form: Option<&WeaponBase>,
    variant: &'static str,
    cap: u32,
    innate: &[Option<Polarity>],
    subset: &[usize],
    stats: &mut EnumStats,
    out: &mut Vec<Candidate>,
) {
    // Legalization is order-independent (drain/polarity multiset only).
    let planned: Vec<PlannedMod> = subset
        .iter()
        .map(|&i| PlannedMod {
            base_drain: pool[i].base_drain,
            polarity: pool[i].polarity,
        })
        .collect();
    let Ok(plan) = plan_forma(cap, innate, &planned) else {
        stats.illegal += 1;
        return;
    };

    // Distinct primary elements in this subset (position-sensitive).
    let mut elems: Vec<DamageType> = Vec::new();
    for &i in subset {
        if let Some(t) = pool[i].primary_element() {
            if !elems.contains(&t) {
                elems.push(t);
            }
        }
    }

    let mut seen_vectors: Vec<Vec<(DamageType, i64)>> = Vec::new();
    let mut orders = Vec::new();
    permutations(&elems, &mut Vec::new(), &mut orders);
    for order in &orders {
        stats.order_variants += 1;
        // Canonical form: element mods first, grouped by the chosen element
        // order; the (order-free) rest after.
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

        let refs: Vec<&ModDef> = ordered.iter().map(|&i| &pool[i]).collect();
        // On-kill stacks start at ZERO and are earned live (user policy).
        let panel = resolve(base, &refs, StackPolicy::Emergent);

        // Second-level dedup: orders resolving to the same combined vector
        // are the same build (docs/OPTIMIZER.md §1). Deduping on the
        // PRIMARY form's vector is safe for the second form too: both are
        // functions of the element partition, which the vector determines
        // (an injected element pairs with the partition's leftover).
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
        });
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
    monte_carlo(&params, runs, seed)
}

/// Dominance pruning (命题作文 preset): mods whose every effect is the
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

/// Self-scaling successive-halving schedule (user, 2026-07-25: derive
/// the funnel from the job count instead of hand-written constants).
/// Geometric: each round multiplies runs ×4 and keeps 1/8 of the field
/// (gentler per-cut than the old fixed table while costing LESS overall:
/// total sims ≈ 2 × N vs 3 × N for the old first round alone). Rounds
/// rank by mean effective damage until runs reach 48, then by kill
/// score; a 1024-run final on the last ≤64 always closes the funnel
/// (the full power-of-four ladder: 1, 4, 16, 64, 256, 1024).
pub fn schedule(n_jobs: usize) -> Vec<(u32, usize, bool)> {
    let mut rounds = Vec::new();
    let mut runs: u32 = 1;
    let mut keep = n_jobs;
    while keep > 64 && runs < 1024 {
        keep = (keep / 8).max(64);
        rounds.push((runs, keep, runs >= 48));
        runs = (runs * 4).min(1024);
    }
    rounds.push((1024, 24, true));
    rounds
}

/// Evaluate jobs concurrently across all cores. Returns summaries
/// index-aligned with `jobs`.
pub fn evaluate_batch(
    cands: &[Candidate],
    jobs: &[Job],
    arcanes: &[wfsim_engine::arcanes_data::ArcaneFx],
    scenario: &Scenario,
    runs: u32,
    seed: u64,
) -> Vec<Summary> {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
        .max(1);
    let chunk = jobs.len().div_ceil(threads).max(1);
    let mut results: Vec<Option<Summary>> = vec![None; jobs.len()];
    std::thread::scope(|scope| {
        for (ids, res) in jobs.chunks(chunk).zip(results.chunks_mut(chunk)) {
            let scenario = scenario.clone();
            scope.spawn(move || {
                for (k, &(ci, ai)) in ids.iter().enumerate() {
                    // Deterministic per-job seed, mixed per round.
                    let s = seed
                        ^ (ci as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                        ^ ((ai as u64) << 56);
                    res[k] = Some(evaluate(&cands[ci], &arcanes[ai], &scenario, runs, s));
                }
            });
        }
    });
    results.into_iter().map(|r| r.expect("evaluated")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wfsim_engine::loadout::DtEvo2;


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
            "hornet_strike", "barrel_diffusion", "amalgam_barrel_diffusion",
            "galvanized_diffusion", "pistol_gambit", "primed_pistol_gambit",
            "creeping_bullseye", "target_cracker", "primed_target_cracker",
            "lethal_torrent", "frostbite", "jolt",
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

        let base = WeaponBase::dual_toxocyst_incarnon(true, DtEvo2::FeveredFrenzy);
        let (cands, stats) = enumerate_candidates(
            &p,
            &base,
            Some(&WeaponBase::dual_toxocyst_base(true, DtEvo2::FeveredFrenzy)),
            "fevered",
            8,
            60,
            &dual_toxocyst_innate_slots(),
            &Constraints::default(),
        );
        assert_eq!(stats.subsets, expected, "subset count vs generating function");
        assert_eq!(
            cands.len() as u64 + stats.deduped,
            stats.order_variants,
            "every order variant is kept or deduped"
        );
        // Sanity: every candidate is exactly 8 mods and within capacity.
        assert!(cands.iter().all(|c| c.ordered.len() == 8));
        assert!(cands.iter().all(|c| c.plan.total_drain <= 60));
    }

    #[test]
    fn schedule_scales_with_the_job_count() {
        // ~2M jobs: 1-run screen keeps 1/8, runs ×4 per round, kill-score
        // ranking from 48 runs on, always closed by a 1024-run final.
        let s = schedule(1_950_192);
        assert_eq!(s.first(), Some(&(1, 243_774, false)));
        assert!(s.windows(2).all(|w| w[1].0 > w[0].0 && w[1].1 <= w[0].1));
        assert_eq!(s.last(), Some(&(1024, 24, true)));
        // Total sims ≈ Σ runs × field ≈ 2 × N — cheaper than the old
        // fixed table's 3 × N first round alone.
        let mut field = 1_950_192usize;
        let mut sims = 0usize;
        for &(runs, keep, _) in &s {
            sims += field * runs as usize;
            field = keep;
        }
        assert!(sims < 3 * 1_950_192, "sims {sims}");
        // Small pools still get a sane funnel ending in the final.
        let small = schedule(500);
        assert_eq!(small.first(), Some(&(1, 64, false)));
        assert_eq!(small.last(), Some(&(1024, 24, true)));
    }

    #[test]
    #[ignore = "enumerates the FULL pool; explodes now that the pistol pool grew \
                to ~80 mods (C(73,7)). The optimizer is being re-planned around a \
                UI-selected scoped subset (2026-07-26) — re-enable against a scope."]
    fn constraints_filter_the_space() {
        let p = pool();
        let base = WeaponBase::dual_toxocyst_incarnon(true, DtEvo2::FeveredFrenzy);
        let cons = Constraints {
            require: vec!["hornet_strike".into()],
            forbid: vec!["magnetic_might".into()],
        };
        let (cands, _) = enumerate_candidates(
            &p,
            &base,
            None,
            "fevered",
            8,
            60,
            &dual_toxocyst_innate_slots(),
            &cons,
        );
        assert!(!cands.is_empty());
        let hornet = p.iter().position(|m| m.id == "hornet_strike").unwrap();
        let mm = p.iter().position(|m| m.id == "magnetic_might").unwrap();
        assert!(cands.iter().all(|c| c.ordered.contains(&hornet)));
        assert!(cands.iter().all(|c| !c.ordered.contains(&mm)));
    }
}
