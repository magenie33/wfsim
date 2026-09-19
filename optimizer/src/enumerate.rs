// SPDX-License-Identifier: AGPL-3.0-or-later
//! The canonical-form walk: mod subsets x element orders x exilus options,
//! legalized and deduplicated into [`Candidate`]s.

use wfsim_engine::build::loadout::{resolve_for, ResolvedPanel};
use wfsim_engine::data::tenno::Tenno;
use wfsim_engine::model::WeaponBase;
use wfsim_engine::model::{ModDef, StackPolicy};
use wfsim_engine::rules::capacity::{plan_forma, FormaPlan, PlannedMod, Polarity};
use wfsim_engine::rules::damage::DamageType;

use crate::{tick, FunnelState};

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
/// tie on score and differ in Forma/drain — still distinct builds. The
/// exilus dimension is NEVER special-cased out of the search: an exilus mod may affect the final outcome, so it stays a
/// full search dimension like any other slot. Pass `&[None]` (or `&[]`,
/// treated the same) for a plain 8-slot search.
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
        // The NEUTRAL Tenno and the ordinary policy: this front predates the
        // scenario and is used by the CLI and the tests, which have no fight to
        // draw either from.
        wfsim_engine::data::tenno::default_tenno(),
        StackPolicy::Emergent,
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
///
/// `tenno` and `policy` are the FIGHT's, exactly as [`enumerate_candidates_each`]
/// takes them: a candidate's panel is resolved against them, so hardcoding a
/// neutral player and `Emergent` here scored every materialized scope under
/// buffs the replay may refuse — a SENTINEL resolves `BaseOnly`.
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
    tenno: &Tenno,
    policy: StackPolicy,
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
        tenno,
        policy,
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
    // The fight's TENNO. Every `condition:` a mod card states is a question
    // about this player (aiming, invisible, airborne), so the optimizer scores
    // builds under the same player the sim will replay them with — score a
    // build aiming and replay it hip-firing and you crown a buff the replay
    // never grants.
    tenno: &Tenno,
    policy: StackPolicy,
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
        tenno,
        policy,
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
    tenno: &Tenno,
    policy: StackPolicy,
    stats: &mut EnumStats,
    scratch: &mut Vec<Candidate>,
    sink: &mut S,
    state: Option<&FunnelState>,
) -> bool {
    if let Some(st) = state {
        // Either signal stops the walk; the CALLER tells them apart, because
        // they mean opposite things about the result (empty vs best-so-far).
        if st.cancel.load(std::sync::atomic::Ordering::Relaxed)
            || st.stop_enumeration.load(std::sync::atomic::Ordering::Relaxed)
        {
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
            tenno,
            policy,
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
            tenno,
            policy,
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

/// Every candidate ONE subset can produce: element orders x exilus options,
/// legalized and deduplicated by resolved damage vector — the exact
/// subproblem the search keeps exhaustive inside each proposal (search.rs).
/// Appends to `out` so a caller can accumulate across evolution sets.
#[allow(clippy::too_many_arguments)]
pub fn expand_one(
    pool: &[ModDef],
    base: &WeaponBase,
    second_form: Option<&WeaponBase>,
    variant: u32,
    cap: u32,
    innate: &[Option<Polarity>],
    exilus_opts: &[Option<&ModDef>],
    subset: &[usize],
    tenno: &Tenno,
    policy: StackPolicy,
    out: &mut Vec<Candidate>,
) {
    let default_opts = [None];
    let exilus_opts = if exilus_opts.is_empty() { &default_opts[..] } else { exilus_opts };
    let mut stats = EnumStats::default();
    expand_subset(
        pool, base, second_form, variant, cap, innate, exilus_opts, subset, tenno, policy,
        &mut stats, out,
    );
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
    tenno: &Tenno,
    policy: StackPolicy,
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
        // The exilus option is a 9th planned mod in the weapon's 9th slot
        // (matching the web UI's exilus model); its drain counts against the
        // cap like any other (game rule).
        let mut planned = base_planned.clone();
        if let Some(x) = xopt {
            planned.push(PlannedMod {
                base_drain: x.base_drain,
                polarity: x.polarity,
            });
        }
        let mut slots_vec = Vec::new();
        let slots: &[Option<Polarity>] = padded(innate, planned.len(), &mut slots_vec);
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
            let panel = resolve_for(base, &refs, policy, tenno);

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
                base_panel: second_form
                    .map(|b| resolve_for(b, &refs, policy, tenno)),
                plan: plan.clone(),
                variant,
                exilus: xi as u32,
            });
        }
    }
}

/// Rebuild ONE candidate from its identity — the three things that are not
/// derived: the ordered pool indices, the evolution-set index, and the exilus
/// choice. Everything else on a [`Candidate`] (`panel`, `base_panel`, `plan`)
/// is a pure function of those plus the weapon, so a rebuilt candidate is
/// bit-identical to the enumerated one; this deliberately calls the SAME
/// `plan_forma` / `resolve_with` the walk does rather than reimplementing them.
///
/// This is what makes a funnel run RESUMABLE: a checkpoint need only carry
/// identities, not resolved panels, so it stays small enough for localStorage
/// and cannot drift from what the enumerator would have produced.
///
/// `None` when the build is not legal under the capacity cap — a checkpoint
/// written against a different pool or Forma budget is rejected rather than
/// silently resolving to something else.
#[allow(clippy::too_many_arguments)]
pub fn rebuild_candidate(
    pool: &[ModDef],
    base: &WeaponBase,
    second_form: Option<&WeaponBase>,
    innate: &[Option<Polarity>],
    cap: u32,
    tenno: &Tenno,
    policy: StackPolicy,
    ordered: &[usize],
    variant: u32,
    exilus: u32,
    exilus_opts: &[Option<&ModDef>],
) -> Option<Candidate> {
    if ordered.iter().any(|&i| i >= pool.len()) {
        return None;
    }
    let xopt = *exilus_opts.get(exilus as usize)?;
    let mut planned: Vec<PlannedMod> = ordered
        .iter()
        .map(|&i| PlannedMod { base_drain: pool[i].base_drain, polarity: pool[i].polarity })
        .collect();
    if let Some(x) = xopt {
        planned.push(PlannedMod { base_drain: x.base_drain, polarity: x.polarity });
    }
    let mut slots_vec = Vec::new();
    let slots: &[Option<Polarity>] = padded(innate, planned.len(), &mut slots_vec);
    let plan = plan_forma(cap, slots, &planned).ok()?;
    let mut refs: Vec<&ModDef> = ordered.iter().map(|&i| &pool[i]).collect();
    if let Some(x) = xopt {
        refs.push(x);
    }
    Some(Candidate {
        ordered: ordered.to_vec(),
        panel: resolve_for(base, &refs, policy, tenno),
        base_panel: second_form.map(|b| resolve_for(b, &refs, policy, tenno)),
        plan,
        variant,
        exilus,
    })
}

/// The slot list one candidate is planned against.
///
/// THE CALLER'S LIST IS THE WEAPON'S SLOT COUNT, and that length decides where a
/// leftover innate colour can sit for free (`engine::mods::plan_forma`). Padding
/// is the assert's floor and nothing else: a caller handing over fewer slots
/// than the build has mods gets blank ones rather than a panic.
fn padded<'a>(
    innate: &'a [Option<Polarity>],
    mods: usize,
    buf: &'a mut Vec<Option<Polarity>>,
) -> &'a [Option<Polarity>] {
    if innate.len() >= mods {
        return innate;
    }
    *buf = innate.to_vec();
    buf.resize(mods, None);
    buf
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool;

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
                "dual_toxocyst_commodores_fortune",
                "dual_toxocyst_evolved_autoloader",
                "dual_toxocyst_fevered_frenzy",
            ],
        );
        let (cands, stats) = enumerate_candidates(
            &p,
            &base,
            Some(&WeaponBase::from_data(
                "dual_toxocyst",
                true,
                &[
                    "dual_toxocyst_commodores_fortune",
                    "dual_toxocyst_evolved_autoloader",
                    "dual_toxocyst_fevered_frenzy",
                ],
            )),
            0,
            8,
            8,
            60,
            &wfsim_engine::data::weapons::innate_slots("dual_toxocyst"),
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
                    "dual_toxocyst_commodores_fortune",
                    "dual_toxocyst_evolved_autoloader",
                    "dual_toxocyst_fevered_frenzy",
                ],
            )),
            0,
            0,
            8,
            60,
            &wfsim_engine::data::weapons::innate_slots("dual_toxocyst"),
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
        let full = wfsim_engine::data::mods::pistol_pool();
        let ex = full
            .iter()
            .find(|m| m.exilus)
            .expect("an exilus mod exists")
            .clone();

        let base = WeaponBase::from_data(
            "dual_toxocyst_incarnon",
            true,
            &[
                "dual_toxocyst_commodores_fortune",
                "dual_toxocyst_evolved_autoloader",
                "dual_toxocyst_fevered_frenzy",
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
                &wfsim_engine::data::weapons::innate_slots("dual_toxocyst"),
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
    #[ignore = "enumerates the FULL pool; explodes now that the pistol pool grew \
                to ~80 mods (C(73,7)). The optimizer is being re-planned around a \
                UI-selected scoped subset — re-enable against a scope."]
    fn constraints_filter_the_space() {
        let p = pool();
        let base = WeaponBase::from_data(
            "dual_toxocyst_incarnon",
            true,
            &[
                "dual_toxocyst_commodores_fortune",
                "dual_toxocyst_evolved_autoloader",
                "dual_toxocyst_fevered_frenzy",
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
            &wfsim_engine::data::weapons::innate_slots("dual_toxocyst"),
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
