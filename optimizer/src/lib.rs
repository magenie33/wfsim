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
use wfsim_engine::dummy::{monte_carlo, BodyPart, DummyParams, Summary, TargetParams};
use wfsim_engine::loadout::{resolve, ModDef, ModEffect, ResolvedPanel, StackPolicy, WeaponBase};
use wfsim_engine::mods::{plan_forma, FormaPlan, PlannedMod, Polarity};

/// The pistol mod pool, mirrored from `data/mods/*.yaml` at MAX RANK
/// (drain = base + max_rank). The YAML files are the source of record; this
/// table is the engine-facing view until the declarative mod loader lands.
pub fn pool() -> Vec<ModDef> {
    use DamageType::*;
    use ModEffect::*;
    use Polarity::*;
    let m = |id, drain, polarity, family, effects| ModDef {
        id,
        base_drain: drain,
        polarity,
        family,
        effects,
    };
    vec![
        m("hornet_strike", 14, Madurai, None, vec![BaseDamage(2.20)]),
        m(
            "barrel_diffusion",
            11,
            Madurai,
            Some("barrel_diffusion"),
            vec![Multishot(1.20)],
        ),
        m(
            "amalgam_barrel_diffusion",
            15,
            Madurai,
            Some("barrel_diffusion"),
            vec![Multishot(1.095)],
        ),
        m(
            "galvanized_diffusion",
            14,
            Madurai,
            Some("barrel_diffusion"),
            vec![
                Multishot(1.10),
                OnKillMultishot {
                    per_stack: 0.30,
                    max_stacks: 4,
                },
            ],
        ),
        m(
            "target_cracker",
            9,
            Madurai,
            Some("target_cracker"),
            vec![CritDamage(0.60)],
        ),
        m(
            "primed_target_cracker",
            14,
            Madurai,
            Some("target_cracker"),
            vec![CritDamage(1.10)],
        ),
        m(
            "lethal_torrent",
            11,
            Madurai,
            None,
            vec![FireRate(0.60), Multishot(0.60)],
        ),
        m(
            "pistol_gambit",
            9,
            Madurai,
            Some("pistol_gambit"),
            vec![CritChance(1.20)],
        ),
        m(
            "primed_pistol_gambit",
            12,
            Madurai,
            Some("pistol_gambit"),
            vec![CritChance(1.87)],
        ),
        m(
            "creeping_bullseye",
            9,
            Naramon,
            Some("pistol_gambit"),
            vec![CritChance(2.00), FireRate(-0.20)],
        ),
        m(
            "galvanized_shot",
            12,
            Vazarin,
            Some("sure_shot"),
            vec![
                StatusChance(0.80),
                ConditionOverload {
                    per_stack: 0.40,
                    max_stacks: 3,
                },
            ],
        ),
        m(
            "frostbite",
            7,
            Madurai,
            None,
            vec![Element(Cold, 0.60), StatusChance(0.60)],
        ),
        m(
            "pistol_pestilence",
            7,
            Madurai,
            None,
            vec![Element(Toxin, 0.60), StatusChance(0.60)],
        ),
        m(
            "scorch",
            7,
            Madurai,
            None,
            vec![Element(Heat, 0.60), StatusChance(0.60)],
        ),
        m(
            "jolt",
            7,
            Madurai,
            None,
            vec![Element(Electricity, 0.60), StatusChance(0.60)],
        ),
        m(
            "deep_freeze",
            7,
            Vazarin,
            Some("deep_freeze"),
            vec![Element(Cold, 0.90)],
        ),
        m(
            "heated_charge",
            11,
            Naramon,
            Some("heated_charge"),
            vec![Element(Heat, 0.90)],
        ),
        m(
            "primed_heated_charge",
            16,
            Naramon,
            Some("heated_charge"),
            vec![Element(Heat, 1.65)],
        ),
        m(
            "convulsion",
            9,
            Naramon,
            Some("convulsion"),
            vec![Element(Electricity, 0.90)],
        ),
        m(
            "primed_convulsion",
            16,
            Naramon,
            Some("convulsion"),
            vec![Element(Electricity, 1.65)],
        ),
        m(
            "pathogen_rounds",
            11,
            Naramon,
            Some("pathogen_rounds"),
            vec![Element(Toxin, 0.90)],
        ),
        m(
            "pistol_elementalist",
            9,
            Vazarin,
            None,
            vec![StatusDamage(0.90), ReloadSpeed(0.60)],
        ),
        m(
            "magnetic_might",
            7,
            Madurai,
            None,
            vec![CombinedElement(Magnetic, 0.60), CritDamage(0.40)],
        ),
    ]
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
    pub plan: FormaPlan,
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
pub fn enumerate_candidates(
    pool: &[ModDef],
    base: &WeaponBase,
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
            expand_subset(pool, base, cap, innate, subset, stats, out);
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

fn expand_subset(
    pool: &[ModDef],
    base: &WeaponBase,
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
        let panel = resolve(base, &refs, StackPolicy::AssumedMax);

        // Second-level dedup: orders resolving to the same combined vector
        // are the same build (docs/OPTIMIZER.md §1).
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
            plan: plan.clone(),
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
#[derive(Clone)]
pub struct Scenario {
    pub target: TargetParams,
    pub body_parts: Vec<BodyPart>,
    pub duration_secs: f64,
    /// Secondary Enervate equipped (the user's chosen arcane). Fixed
    /// equipment, NOT a search dimension.
    pub arcane_enervate: bool,
}

/// Evaluate one candidate: engine Monte Carlo, nothing else.
pub fn evaluate(c: &Candidate, s: &Scenario, runs: u32, seed: u64) -> Summary {
    let mut params = DummyParams::from_panel(
        &c.panel,
        s.target.clone(),
        s.body_parts.clone(),
        s.duration_secs,
    );
    params.arcane_enervate = s.arcane_enervate;
    monte_carlo(&params, runs, seed)
}

/// Dominance pruning (命题作文 preset): mods whose every effect is the
/// same KIND as another pool mod's but strictly smaller are excluded up
/// front — they can never appear in an optimum under `AssumedMax` (drain
/// differences only change Forma count, never damage ranking).
pub fn dominated_mods() -> Vec<(&'static str, &'static str)> {
    vec![
        ("pistol_gambit", "primed_pistol_gambit has strictly more crit chance"),
        ("target_cracker", "primed_target_cracker has strictly more crit damage"),
        ("heated_charge", "primed_heated_charge has strictly more heat"),
        ("convulsion", "primed_convulsion has strictly more electricity"),
        (
            "barrel_diffusion",
            "galvanized_diffusion gives strictly more multishot at assumed max stacks",
        ),
        (
            "amalgam_barrel_diffusion",
            "barrel_diffusion (itself dominated) already exceeds its 109.5%",
        ),
    ]
}

/// Evaluate `idx` candidates concurrently across all cores. Returns
/// summaries index-aligned with `idx`.
pub fn evaluate_batch(
    cands: &[Candidate],
    idx: &[usize],
    scenario: &Scenario,
    runs: u32,
    seed: u64,
) -> Vec<Summary> {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
        .max(1);
    let chunk = idx.len().div_ceil(threads).max(1);
    let mut results: Vec<Option<Summary>> = vec![None; idx.len()];
    std::thread::scope(|scope| {
        for (ids, res) in idx.chunks(chunk).zip(results.chunks_mut(chunk)) {
            let scenario = scenario.clone();
            scope.spawn(move || {
                for (k, &ci) in ids.iter().enumerate() {
                    // Deterministic per-candidate seed, mixed per round.
                    let s = seed ^ (ci as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                    res[k] = Some(evaluate(&cands[ci], &scenario, runs, s));
                }
            });
        }
    });
    results.into_iter().map(|r| r.expect("evaluated")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_has_23_mods_with_family_exclusivity() {
        let p = pool();
        assert_eq!(p.len(), 23);
        let diffusions = p
            .iter()
            .filter(|m| m.family == Some("barrel_diffusion"))
            .count();
        assert_eq!(diffusions, 3);
    }

    #[test]
    fn canonical_enumeration_counts_match_the_generating_function() {
        // Families (3,3,2,2,2 members) + 11 singles, choose 8:
        // coefficient of x^8 in (1+3x)^2 (1+2x)^3 (1+x)^11 = 155,727.
        let p = pool();
        let base = WeaponBase::dual_toxocyst_incarnon();
        let (cands, stats) = enumerate_candidates(
            &p,
            &base,
            8,
            60,
            &dual_toxocyst_innate_slots(),
            &Constraints::default(),
        );
        assert_eq!(stats.subsets, 155_727, "subset count");
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
    fn constraints_filter_the_space() {
        let p = pool();
        let base = WeaponBase::dual_toxocyst_incarnon();
        let cons = Constraints {
            require: vec!["hornet_strike".into()],
            forbid: vec!["magnetic_might".into()],
        };
        let (cands, _) =
            enumerate_candidates(&p, &base, 8, 60, &dual_toxocyst_innate_slots(), &cons);
        assert!(!cands.is_empty());
        let hornet = p.iter().position(|m| m.id == "hornet_strike").unwrap();
        let mm = p.iter().position(|m| m.id == "magnetic_might").unwrap();
        assert!(cands.iter().all(|c| c.ordered.contains(&hornet)));
        assert!(cands.iter().all(|c| !c.ordered.contains(&mm)));
    }
}
