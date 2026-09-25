// SPDX-License-Identifier: AGPL-3.0-or-later
//! `/api/optimize` and `/api/opt-buffs`: the scoped search, validated up front
//! and run to completion by the caller's transport.

use serde_json::{json, Value};
use wfsim_engine::model::WeaponBase;
use wfsim_engine::model::{ModDef, StackPolicy};
use wfsim_optimizer::{
    enumerate_candidates_observed, run_funnel, schedule_to, Candidate, Constraints, FunnelState,
    Job, Scenario,
};
use wfsim_engine::fight::Summary;
use crate::buffs::{BuffMeta, arcane_at_rank, arcane_in_pools, buffs_json, enumerate_buffs, evo_buffs};
use crate::fight::{ladder_prefix, parse_fight};
use crate::kitgun::valence_element_of;
use crate::registry::{WeaponInfo, default_weapon_id, form_unlock_evo, innate_slots_for, mod_not_here, weapon, wspec};
use crate::request::{err_json, get_f64, get_str, get_u32};
use crate::rivens::{mod_pool_with_rivens, riven_stat_ids_ok};
use crate::tenno::tenno_from;

// All buffs the scope could produce (union over every fixed/search mod + every
// searched arcane + the weapon passive) — the optimizer's buff panel enumerates
// over the WHOLE scope, not one build. `apply_buff_config` applies each per
// candidate where present.
pub fn opt_buffs_json(v: &Value) -> Value {
    let info = weapon(get_str(v, "weapon", default_weapon_id()));
    fn merge(out: &mut Vec<BuffMeta>, list: Vec<BuffMeta>) {
        for b in list {
            if !out.iter().any(|x| x.id == b.id) {
                out.push(b);
            }
        }
    }
    let mut ids: Vec<String> = Vec::new();
    // REQUIRED mods, kept apart: only a mod in every build may lock a stat and
    // suppress another mod's buff card. A "search" mark is a candidate, and one
    // candidate's `disables:` says nothing about the builds without it.
    let mut required: Vec<String> = Vec::new();
    if let Some(obj) = v.get("mods").and_then(|x| x.as_object()) {
        for (id, st) in obj {
            if matches!(st.as_str(), Some("fixed") | Some("search")) {
                ids.push(id.clone());
            }
            if st.as_str() == Some("fixed") {
                required.push(id.clone());
            }
        }
    }
    ids.sort();
    ids.dedup();
    required.sort();
    required.dedup();
    // Rivens the request carries join the searchable pool like any mod.
    if let Err(e) = riven_stat_ids_ok(v, info) {
        return err_json(e);
    }
    // The WIDEST pool (nothing installed): this lists the buffs a scope could
    // produce, and a mod only some evolution variants can equip still produces
    // its buff in the variants that can.
    let full = mod_pool_with_rivens(v, info, &[]);
    let refs: Vec<&ModDef> = full
        .iter()
        .filter(|m| ids.iter().any(|id| id.as_str() == m.id))
        .collect();
    let mut out: Vec<BuffMeta> = Vec::new();
    let none = wfsim_engine::data::arcanes::ArcaneFx::none();
    let arc_base = WeaponBase::from_data(&info.id, true, &[]);
    let tenno = tenno_from(v, info);
    let always: Vec<&ModDef> = full
        .iter()
        .filter(|m| required.iter().any(|id| id.as_str() == m.id))
        .collect();
    merge(&mut out, enumerate_buffs(&refs, &always, &none, info, &tenno));
    // The scope is a MARK MAP (id -> "search" | "fixed"), the same shape as
    // `mods`; every marked arcane's buffs are configurable, pins included.
    if let Some(obj) = v.get("arcanes").and_then(|x| x.as_object()) {
        for a in obj.keys().filter(|k| k.as_str() != "none") {
            if let Some(def) = arcane_in_pools(info, a) {
                let fx = def.fx(def.max_rank, StackPolicy::Emergent, arc_base.traits, &tenno);
                merge(&mut out, enumerate_buffs(&[], &[], &fx, info, &tenno));
            }
        }
    }
    // Evolution-granted buffs across the scope (every tier option listed —
    // Fevered Frenzy's permanent stacks show whenever it could be searched).
    if let Some(obj) = v.get("evolutions").and_then(|x| x.as_object()) {
        let evo_ids: Vec<String> = obj
            .values()
            .filter_map(|a| a.as_array())
            .flatten()
            .filter_map(|x| x.as_str().map(String::from))
            .collect();
        merge(&mut out, evo_buffs(&evo_ids));
    }
    json!({ "ok": true, "buffs": buffs_json(&out) })
}

// ---- /api/optimize -----------------------------------------------------
//
// Scoped-subset search (devlog thread #4): the user fixes some mods/arcanes/
// evolutions, opens others to SEARCH, and gets the top-10 builds ranked by
// kills-in-duration. Reuses the optimizer lib's `enumerate_candidates` +
// `run_funnel`, with the same per-buff configured policy as the Sim panel.
// No cap (user directive): the funnel's cheap early rounds cull the space.
//
// Transport-independent split: [`parse_optimize`] validates the request
// synchronously (bad input still fails fast) into an [`OptimizePlan`];
// [`run_optimize`] does the heavy work — enumerate + funnel — publishing
// live progress through the caller's `FunnelState` and honoring its
// `cancel` flag. The native server wraps this pair in a background-job
// registry; the wasm build runs it inside a Web Worker.

/// ONE MODE, resolved into the weapon entries it fires.
///
/// Everything here is derived from `data::weapons::play_modes` — a mode names
/// the entry it fires and, for a cycle, the one it returns to — plus the
/// evolution that unlocks a second form and what to fire without it. It is the
/// same resolution `parse_fight` does for the single mode a simulate names,
/// which is why the two agree on what "cycle" means.
#[derive(Debug, Clone)]
pub(crate) struct ModeForms {
    /// The mode's own id (`base`, `cycle`, `alternate`, `transformed`) — what a
    /// build names and what a result row reports back.
    id: String,
    /// The entry that FIRES. For a cycle this is the transformed half.
    fire_id: String,
    /// The entry a cycle returns to between transmutes; `None` for a single
    /// form, which is also what tells `evaluate` this candidate is not cycling.
    cycle_from: Option<String>,
    /// The evolution that unlocks the second form, and what to fire without it.
    /// Evolutions are their own dimension, so one scope holds sets that
    /// transform and sets that cannot — and which of the two a candidate is
    /// depends on ITS set, not on the mode.
    unlock_evo: Option<String>,
    untransformed_id: String,
}

/// Resolve one mode into the entries it fires — the optimizer's counterpart of
/// what `parse_fight` does for a simulate.
pub(crate) fn mode_forms(info: &WeaponInfo, mode_id: &str) -> ModeForms {
    let modes = wfsim_engine::data::weapons::play_modes(&info.id);
    let m = modes.iter().find(|m| m.id == mode_id).or(modes.first());
    let registered = wfsim_engine::data::weapons::forms_of(&info.id);
    let untransformed = registered
        .iter()
        .find(|f| f.is_default)
        .or(registered.first())
        .map(|f| f.weapon_id)
        .unwrap_or(info.id.as_str())
        .to_string();
    match m {
        // A CYCLE fires the transformed half and returns to the other one.
        Some(m) if m.other_id.is_some() => ModeForms {
            id: m.id.to_string(),
            fire_id: m.other_id.unwrap().to_string(),
            cycle_from: Some(m.weapon_id.to_string()),
            unlock_evo: form_unlock_evo(info).map(String::from),
            untransformed_id: untransformed,
        },
        Some(m) => ModeForms {
            id: m.id.to_string(),
            fire_id: m.weapon_id.to_string(),
            cycle_from: None,
            // A mode that fires the weapon's OWN default entry needs no
            // unlocking; any other one is a second form, and the tier-1
            // evolution is what installs it.
            unlock_evo: (m.weapon_id != untransformed)
                .then(|| form_unlock_evo(info).map(String::from))
                .flatten(),
            untransformed_id: untransformed,
        },
        None => ModeForms {
            id: "base".into(),
            fire_id: info.id.clone(),
            cycle_from: None,
            unlock_evo: None,
            untransformed_id: untransformed,
        },
    }
}

/// Everything the heavy phase needs, validated up front.
pub struct OptimizePlan {
    weapon_id: String,
    pool: Vec<ModDef>,
    /// The capacity the Forma planner is given — the weapon's own, plus what a
    /// STANCE hands back. Not a literal: a melee build reads five to ten points
    /// of headroom the weapon's rank did not buy.
    cap: u32,
    constraints: Constraints,
    min_slots: usize,
    build_size: usize,
    evo_sets: Vec<Vec<String>>,
    /// Per evolution set, which `pool` indices that set cannot EQUIP. An equip
    /// rule is asked of every firing mode a weapon has, and installing the
    /// Incarnon form adds one — so a Cannonade belongs to the variants that
    /// leave tier 1 out and to no others. Same length as `evo_sets`, each entry
    /// as long as `pool`.
    variant_forbids: Vec<Vec<bool>>,
    exilus_defs: Vec<Option<ModDef>>,
    arcanes: Vec<wfsim_engine::data::arcanes::ArcaneFx>,
    /// What each entry of `arcanes` IS, in pool order — one id per slot,
    /// "none" for an empty one. The effects are merged and cannot be read
    /// back apart, so the naming travels beside them.
    arcane_sets: Vec<Vec<String>>,
    /// The DEPLOYMENT every candidate is built in — see `base_for`. Empty =
    /// the weapon's own column.
    deployment: String,
    /// THE VALENCE ELEMENTS this scope searches, and the roll they are all
    /// built at. A SET, because the progenitor element is a dimension like the
    /// mode: a different element is a different build, so a scope may ask which
    /// of them wins.
    ///
    /// One entry is the ordinary case and reproduces exactly what a pinned
    /// element did; an empty list is a weapon with no valence at all, and the
    /// variant table still holds one slot for it.
    ///
    /// It rides the plan for the same reason the deployment does: the search
    /// builds its bases in a worker that never sees the request.
    valences: Vec<String>,
    valence_bonus: f64,
    pub(crate) scenario: Scenario,
    final_runs: u32,
    finalists: usize,
    headshot_pct: f64,
    duration: f64,
    target_name: String,
    level: u32,
    steel_path: bool,
    /// EVERY WAY THE SEARCH MAY PLAY IT, one entry per mode in the scope.
    ///
    /// Mode is a search dimension like the mods, the arcane and the evolution
    /// set — "which of these is the better Phantasma, the
    /// charged one or the plain one" is the same question as "which of these is
    /// the better mod", and it is the one axis the builder had and the search
    /// did not. Before this the whole plan fired ONE mode, taken from the
    /// request's `mode`, and the optimizer tab did not send it — so picking
    /// charged there searched the base form and said nothing.
    modes: Vec<ModeForms>,
    /// The VARIANT TABLE: one entry per (mode, evolution set) pair, which is
    /// what `Candidate::variant` indexes. Evolutions were the only thing in it
    /// when a variant was an evolution set, and every consumer already treats a
    /// variant as "the pair of weapon entries this candidate fires" — so the
    /// mode joins it rather than becoming a second index to thread through.
    variants: Vec<(usize, usize, usize)>,
    /// Worker-thread budget; 0 = auto (all cores minus two).
    threads: usize,
    /// Screen evaluations the SEARCH may spend before it hands its elites to
    /// the funnel. 0 = uncapped, and then the host's clock is the only bound
    /// (the browser sets one; a native run has a Cancel button instead).
    max_evals: u64,
    /// WHICH SEARCH picks the builds the funnel ranks: the sampler, or the
    /// descent (`"strategy": "descent"`).
    descent: bool,
    /// Where the descent begins — [`parse_starts`]. Empty = one start per
    /// primary element.
    starts: Vec<wfsim_optimizer::descent::Start>,
    /// How many positions the descent may change at once when single
    /// changes stop paying (`"swap_width"`, 1 = single changes only).
    swap_width: u32,
    /// This run's STRIDE of the search space, of `shards` total. The browser
    /// buys coverage by running several Web Workers over disjoint strides and
    /// merging their leaderboards; a native run is one shard of one.
    shard: u32,
    shards: u32,
    /// THE REQUEST THIS PLAN CAME FROM, so every ranked row can carry a
    /// simulate request that reproduces it.
    ///
    /// THE SIMULATOR IS THE TRUTH, and a search's number is only worth
    /// something if the simulator will say it back. That was a claim about the
    /// engine — which holds, `parse_fight` sees to it — and it said nothing
    /// about the PAGE, which had its own hand-written translation of a ranked
    /// row into a build and dropped an axis out of it four times. The last one
    /// a player measured: 22.34 KPM on the ranking, 17.44 in the simulator, the
    /// same eight cards on the wrong progenitor element.
    ///
    /// So the row stops DESCRIBING a build and starts CARRYING one. The base is
    /// the optimize request itself rather than a list of build fields, which is
    /// what makes it forget-proof: every field that reaches the optimizer rides
    /// along, including ones nobody has invented yet, and `entry` overwrites
    /// only the axes the search ranged over. `runs` becomes the FINAL ROUND's,
    /// because that is the precision the row's number was measured at.
    replay_base: Value,
}

/// Validate an optimize request. `Err` is the ready-to-send error response.
pub fn parse_optimize(v: &Value) -> Result<OptimizePlan, Value> {
    // THE FIGHT FIRST, and everything below derives from it. Nothing here reads
    // the request for anything the simulator already decided — not the weapon,
    // not the player, not the run count. The optimizer parses its SCOPE and its
    // BUDGET, and that is the whole of its own business.
    let fight = parse_fight(v)?;
    let info = fight.info;
    // ---- mod scope (MAIN 8 slots): fixed ∪ search = pool; fixed = required.
    // Exilus-flagged mods MAY appear here too — all 9 slots accept them
    // (game rule), so putting one in the main scope makes it compete for a
    // main slot like any other mod.
    let mut fixed_ids: Vec<String> = Vec::new();
    let mut search_ids: Vec<String> = Vec::new();
    if let Some(obj) = v.get("mods").and_then(|x| x.as_object()) {
        for (id, st) in obj {
            match st.as_str() {
                Some("fixed") => fixed_ids.push(id.clone()),
                Some("search") => search_ids.push(id.clone()),
                _ => {}
            }
        }
    }
    fixed_ids.sort();
    fixed_ids.dedup();
    search_ids.retain(|s| !fixed_ids.contains(s)); // fixed wins over search
    // Rivens the request carries join the searchable pool like any mod.
    if let Err(e) = riven_stat_ids_ok(v, info) {
        return Err(err_json(e));
    }
    // The WIDEST pool (nothing installed). Evolutions are a search DIMENSION, so
    // a mod can be legal in one variant and not in the next — which variant is
    // decided per candidate, below, not by narrowing the scope here.
    let full = mod_pool_with_rivens(v, info, &[]);
    for id in fixed_ids.iter().chain(search_ids.iter()) {
        if !full.iter().any(|m| m.id == id.as_str()) {
            return Err(err_json(mod_not_here(id, info, &[])));
        }
    }

    // ---- THE STANCE, IN EVERY CANDIDATE ---------------------------------
    //
    // IT IS NOT A SEARCH AXIS AND IT IS NOT OPTIONAL. A stance is the card that
    // decides what a swing IS — the combo script, its multipliers, its forced
    // procs — so a melee search that leaves the slot empty ranks builds nobody
    // holds: measured on a Praedos, 4 kills against 5 and 15% of the DPS. It
    // was left empty because the page's mod list filters stances out (a stance
    // is legal in the stance slot and nowhere else) and nothing put it back.
    //
    // PINNED, THE WAY THE VALENCE AND THE ASSEMBLY ARE: the page holds one and
    // the search does not range over it, so it is a constant of every candidate
    // rather than a dimension. `require` is exactly that mechanism.
    //
    // ITS SLOT IS ITS OWN, so the ceiling rises with it — otherwise pinning it
    // would cost a MAIN slot and the search would rank seven-mod builds. It
    // costs no capacity either (`base_drain: 0`, see notes
    // `stance_grants_capacity`); what it does is HAND capacity back, which is
    // the grant below.
    let stance_def = match get_str(v, "stance", "") {
        "" => None,
        id => match full.iter().find(|m| m.id == id) {
            Some(m) if m.stance.is_some() => Some(m.clone()),
            Some(_) => return Err(err_json(format!("{id} is not a stance"))),
            None => return Err(err_json(mod_not_here(id, info, &[]))),
        },
    };
    if let Some(m) = &stance_def {
        if !fixed_ids.iter().any(|id| id == m.id) {
            fixed_ids.push(m.id.to_string());
            fixed_ids.sort();
        }
        search_ids.retain(|s| s != m.id);
    }
    // THE CAP THE PLANNER IS GIVEN. The stance slot at its INNATE colour, which
    // is what the builder's own capacity line reads — the planner may polarize
    // that slot for more, and this does not model it, which errs toward
    // refusing a build the simulator would accept rather than crowning one it
    // would not.
    let cap = 60 + stance_def.as_ref().map_or(0, |m| {
        wfsim_engine::rules::capacity::stance_capacity(
            m.polarity,
            wfsim_engine::data::weapons::stance_polarity(&info.id),
        )
    });

    // ---- exilus scope (the +1 slot, exilus-eligible mods only): its own
    // block. "search" entries are slot OPTIONS alongside "leave empty"; a
    // "fixed" one pins the slot (max one — there is only one exilus slot).
    // Absent/empty = the slot stays empty. A mod listed in BOTH scopes is
    // fine unless double-required: enumeration never equips it twice (the
    // exilus option is skipped for subsets that already contain it).
    let mut ex_fixed: Vec<String> = Vec::new();
    let mut ex_search: Vec<String> = Vec::new();
    if let Some(obj) = v.get("exilus").and_then(|x| x.as_object()) {
        for (id, st) in obj {
            match st.as_str() {
                Some("fixed") => ex_fixed.push(id.clone()),
                Some("search") => ex_search.push(id.clone()),
                _ => {}
            }
        }
    }
    ex_fixed.sort();
    ex_fixed.dedup();
    ex_search.retain(|s| !ex_fixed.contains(s));
    // "none" is a first-class option id: pool it to keep "leave empty" among
    // the searched options, req it to pin the slot empty.
    for id in ex_fixed
        .iter()
        .chain(ex_search.iter())
        .filter(|id| id.as_str() != "none")
    {
        let Some(m) = full.iter().find(|m| m.id == id.as_str()) else {
            return Err(err_json(format!("unknown exilus mod id: {id}")));
        };
        if !m.exilus {
            return Err(err_json(format!("{id} is not exilus-eligible")));
        }
    }
    if ex_fixed.len() > 1 {
        return Err(err_json(format!(
            "only one exilus slot — {} cannot all be required",
            ex_fixed.join(", ")
        )));
    }
    if let Some(f) = ex_fixed.first() {
        if fixed_ids.contains(f) {
            return Err(err_json(format!(
                "{f} is required in both a main slot and the exilus slot — a mod equips once"
            )));
        }
    }
    // Marked pools OCCUPY the slot (same rule as the main block's pool
    // group): the empty option exists only when NOTHING is marked or when
    // "none" itself is pooled/req'd — never implicitly next to pooled mods.
    let exilus_ids: Vec<Option<String>> = match ex_fixed.first() {
        Some(f) if f == "none" => vec![None],
        Some(f) => vec![Some(f.clone())],
        None if ex_search.is_empty() => vec![None],
        None => ex_search
            .iter()
            .map(|id| if id == "none" { None } else { Some(id.clone()) })
            .collect(),
    };
    let exilus_defs: Vec<Option<ModDef>> = exilus_ids
        .iter()
        .map(|o| {
            o.as_ref()
                .and_then(|id| full.iter().find(|m| m.id == id.as_str()).cloned())
        })
        .collect();

    // The MAXIMUM main slots a build may fill (0..=8; the exilus slot is the
    // +1 on top). Slots may stay empty — sizes 0..=build_size all enumerate,
    // so a scope smaller than the cap is legal.
    //
    // ZERO IS ONE OF THEM. Every other axis can be set to
    // "search this slot empty, and keep the marks"; this one was clamped to 1,
    // so the only way to reach the bare weapon was to unmark everything —
    // which costs the reader exactly what 0–0 exists to protect. It also makes
    // the ceiling mean the same thing on all five axes, which is the point of
    // there being one control.
    let build_size = get_u32(v, "build_size", 8).clamp(0, 8) as usize
        + usize::from(stance_def.is_some());
    let mut pool_ids: Vec<String> = fixed_ids.iter().chain(search_ids.iter()).cloned().collect();
    pool_ids.sort();
    pool_ids.dedup();
    if fixed_ids.len() > build_size {
        return Err(err_json(format!(
            "more required mods ({}) than build slots ({build_size})",
            fixed_ids.len()
        )));
    }
    // The pool GROUP occupies ≥1 slot whenever anything is pooled — every
    // searched build then uses at least one pooled mod (mark no pools for an
    // exactly-required build). Hence required can fill at most size−1 slots
    // while pools exist, and enumeration starts above the required count.
    //
    // …UNLESS THE CEILING IS 0, which is the reader saying "not this time, but
    // keep the marks" — the same 0–0 every other axis has. This guard is the
    // derived floor in refusal form, so it must give way to the same rule
    // `min_slots` does below, or a legal scope comes back as a contradiction.
    if build_size > 0 && !search_ids.is_empty() && fixed_ids.len() >= build_size {
        return Err(err_json(format!(
            "pooled mods occupy at least one of the {build_size} slots — required ({}) leaves none",
            fixed_ids.len()
        )));
    }
    // How FULL a build must be, as its own axis.
    // `build_size` is the ceiling and `build_min` the floor, so "exactly 8" is
    // (8, 8), "up to 8" is (1, 8) and "up to 7" is (1, 7) — three settings
    // rather than three behaviours.
    //
    // The DERIVED floor stays a floor: pooling mods is the statement that they
    // should be used, so at least one pooled mod is in every searched build,
    // and every required mod is in all of them. A `build_min` below that is
    // raised to it rather than rejected — it asks for builds the scope itself
    // has already ruled out.
    //
    // THE FLOOR STARTS AT 0, AND 0 IS THE DEFAULT. Every
    // other axis of a search reads "nothing marked" as the EMPTY option — an
    // unmarked exilus slot stays empty (see `exilus_ids` above), an unmarked
    // arcane seat searches no arcane — and the mods axis alone answered it with
    // "no legal builds in this scope". The page has claimed "an empty scope =
    // the bare weapon, still a legal search" since the estimate was written,
    // and a floor of 1 made that sentence false.
    // It changes nothing else, by arithmetic: the moment anything is marked,
    // `derived_min` is at least 1 and wins, so 0 and 1 differ in exactly the
    // one case above — pinned by `an_empty_scope_searches_the_bare_weapon`.
    let derived_min = fixed_ids.len() + usize::from(!search_ids.is_empty());
    let build_min = get_u32(v, "build_min", 0).clamp(0, 8) as usize;
    if build_min > build_size {
        return Err(err_json(format!(
            "a build cannot hold at least {build_min} mods and at most {build_size}"
        )));
    }
    // A CEILING OF 0 OUTRANKS THE DERIVED FLOOR, and it has to: `derived_min`
    // is "the marks say use these", and 0–0 is the reader saying "not this
    // time, but keep them". Without this the two contradict and the space is
    // empty — `SubsetSpace::new(1, 0)` enumerates nothing, which would report
    // a legal request as "no legal builds in this scope". It is the same rule
    // `slotRange` writes as a pinned empty mark on every single-slot axis.
    let min_slots = if build_size == 0 { 0 } else { derived_min.max(build_min) };
    let pool: Vec<ModDef> = full
        .iter()
        .filter(|m| pool_ids.iter().any(|id| id.as_str() == m.id))
        .cloned()
        .collect();
    let constraints = Constraints {
        require: fixed_ids.clone(),
        forbid: Vec::new(),
    };

    // ---- evolution scope: per-tier options → the Cartesian product ----
    // The tier COUNT is per weapon (DT 4, Laetum 5) — read it from the data.
    let evo_req = v.get("evolutions").and_then(|x| x.as_object());
    let mut evo_sets: Vec<Vec<String>> = vec![Vec::new()];
    let evo_tiers = wfsim_engine::data::evolutions::tier_count(
        wspec(&info.id).transform_group.as_deref().unwrap_or(&info.id),
    );
    for tier in 1u32..=evo_tiers {
        // `"none"` IS AN OPTION THE LIST MAY NAME, which is
        // how a tier says "0–1": search this tier both unfilled and filled.
        // Unambiguous here where it is not for arcanes, because the wire is
        // already one array PER TIER. An array holding only "none" is "0–0" —
        // search this tier empty while its candidates stay marked for later.
        let opts: Vec<Option<String>> = evo_req
            .and_then(|o| o.get(&tier.to_string()))
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str())
                    .map(|s| if s == "none" { None } else { Some(s.to_string()) })
                    .collect()
            })
            .unwrap_or_default();
        let picks = if opts.is_empty() { vec![None] } else { opts }; // unmarked = nothing at this tier
        let mut next = Vec::new();
        for base in &evo_sets {
            for pick in &picks {
                let mut e = base.clone();
                if let Some(id) = pick {
                    e.push(id.clone());
                }
                next.push(e);
            }
        }
        evo_sets = next;
    }
    // The product can skip a tier — mark tier 2 and leave tier 1 unmarked and
    // it pairs "nothing at 1" with "Final Fusillade at 2", which is a build
    // the game cannot make. Cut each set to its reachable prefix (the same
    // rule every other entry point applies) and dedupe, rather than searching
    // variants that would be filtered away at the moment they were scored.
    for set in evo_sets.iter_mut() {
        *set = ladder_prefix(std::mem::take(set));
    }
    evo_sets.sort();
    evo_sets.dedup();
    for set in &evo_sets {
        for id in set {
            if wfsim_engine::data::evolutions::get(id).is_none() {
                return Err(err_json(format!("unknown evolution id: {id}")));
            }
        }
    }

    // ---- what each variant may not EQUIP -----------------------------------
    //
    // An equip rule is asked of every firing mode a weapon has, and a variant
    // that installs the Incarnon form has two — so a Cannonade is legal in the
    // variants that leave tier 1 out and illegal in the ones that do not. That
    // is a per-CANDIDATE fact, not a per-scope one: narrowing the pool to what
    // every variant can equip would throw away the builds where the mod is the
    // point, and leaving it alone would crown a build the game refuses.
    //
    // Same rule the simulator applies, from the same engine call — the pool the
    // sim resolves a build against IS `pool_for_build` (hard rule: the optimizer
    // obeys the simulator).
    let variant_pools: Vec<Vec<ModDef>> = evo_sets
        .iter()
        .map(|set| {
            let refs: Vec<&str> = set.iter().map(String::as_str).collect();
            mod_pool_with_rivens(v, info, &refs)
        })
        .collect();
    // Per variant, which SCOPE indices it cannot equip — the shape the walk
    // wants, resolved once instead of per subset.
    let variant_forbids: Vec<Vec<bool>> = variant_pools
        .iter()
        .map(|legal| {
            pool.iter().map(|m| !legal.iter().any(|x| x.id == m.id)).collect()
        })
        .collect();
    // A REQUIRED mod no variant can equip is a contradiction the search cannot
    // resolve by choosing differently — say it now rather than answer with an
    // empty leaderboard.
    for (i, m) in pool.iter().enumerate() {
        if constraints.require.iter().any(|r| r == m.id) && variant_forbids.iter().all(|f| f[i]) {
            return Err(err_json(format!(
                "{} is required, and no evolution set in this scope can equip it — \
                 it needs the same trigger on every firing mode",
                m.name
            )));
        }
    }
    // The EXILUS table is shared across variants (a candidate stores its option
    // by INDEX, and the index has to name the same option in every one), so an
    // exilus option must be equippable under all of them. Vacuous today — no
    // exilus mod carries an equip rule — and stated so it stays a rule rather
    // than an accident if one ever does.
    for d in exilus_defs.iter().flatten() {
        if let Some(legal) = variant_pools
            .iter()
            .find(|legal| !legal.iter().any(|x| x.id == d.id))
        {
            let _ = legal;
            return Err(err_json(format!(
                "{} cannot be equipped under every evolution set in this scope, so it \
                 cannot be an exilus option — pin the evolutions, or drop it",
                d.name
            )));
        }
    }

    // ---- arcane scope ----
    // The same shape as `mods` and `exilus`: id -> "search" | "fixed". A pin
    // says "this slot is settled", which a flat list of ids cannot say.
    let arc_marks: Vec<(String, String)> = v
        .get("arcanes")
        .and_then(|x| x.as_object())
        .map(|o| {
            o.iter()
                .filter_map(|(k, m)| m.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let arc_base = WeaponBase::from_data(&info.id, true, &[]);
    // The FIGHT's player, not a second one built the same way. Identical today
    // — same function, same request — which is exactly why it was easy to leave
    // and exactly why it should not be: two constructions of one fact is how
    // they come to differ.
    let tenno = &fight.tenno;
    // ONE AXIS PER SLOT, then their CROSS PRODUCT — a weapon that seats two
    // arcanes is searched over pairs, because "the best Primary" and "the
    // best Secondary" are not independent questions: an on-kill Secondary is
    // worth more next to a Primary that gets you the kill.
    //
    // The funnel is untouched by this. Its arcane axis has always been a flat
    // `Vec<ArcaneFx>` indexed by a job, and a merged pair IS one `ArcaneFx`
    // (see `ArcaneFx::merged`) — so the product is built here and nothing
    // downstream learns that a weapon can seat two.
    //
    // An arcane the weapon cannot equip is DROPPED from the scope, not mapped
    // to the empty slot: collapsing it would search "no arcane" once per
    // rejected id and report those runs as if they had been real options.
    // Each slot also always offers the EMPTY choice, so "one arcane, not two"
    // stays reachable — the scope says what MAY be worn, not what must be.
    let per_slot: Vec<Vec<(String, wfsim_engine::data::arcanes::ArcaneFx)>> = info
        .arcane_pools
        .iter()
        .map(|pool| {
            // THE EMPTY CHOICE IS AN OPTION LIKE ANY OTHER, and it is marked
            // like any other — `none:<pool>`. The id names
            // its POOL because a weapon can seat two arcanes and the marks are
            // one flat map: a bare "none" could not say which seat it is about.
            // It is the exilus slot's own `none` mechanism, made per seat.
            let empty_id = format!("none:{pool}");
            let empty_mark = arc_marks
                .iter()
                .find(|(id, _)| *id == empty_id || (info.arcane_pools.len() == 1 && id == "none"))
                .map(|(_, m)| m.as_str());
            // AN ARCANE BELOW ITS MAX RANK IS `<id>@<rank>`, the mods' spelling.
            let mine: Vec<(&String, &String)> = arc_marks
                .iter()
                .filter(|(id, _)| arcane_at_rank(pool, id).is_some())
                .map(|(id, m)| (id, m))
                .collect();
            let fx = |id: &str| {
                arcane_at_rank(pool, id)
                    .map(|(d, rank)| d.fx(rank, StackPolicy::Emergent, arc_base.traits, tenno))
                    .unwrap_or_else(wfsim_engine::data::arcanes::ArcaneFx::none)
            };
            let empty = || {
                ("none".to_string(), wfsim_engine::data::arcanes::ArcaneFx::none())
            };
            // A PIN settles the slot: one option, and no other choice —
            // including a pinned EMPTY, which is "search this seat unworn"
            // while the candidates stay marked for later.
            if empty_mark == Some("fixed") {
                return vec![empty()];
            }
            if let Some((id, _)) = mine.iter().find(|(_, m)| m.as_str() == "fixed") {
                return vec![((*id).clone(), fx(id))];
            }
            // EMPTY IS NOT ADDED BY ITSELF, and that has not changed: an arcane
            // slot costs nothing — no capacity, no Forma — so leaving it empty
            // can never beat filling it with something that helps, and marking
            // a candidate IS the statement that the seat should be filled. Keeping `none` alongside doubled the space
            // per slot and put builds with a hole in them on the results
            // board, where they can only ever tie the same build with the
            // arcane in it.
            //
            // WHAT CHANGED IS THAT IT CAN BE ASKED FOR.
            // That decision was against the empty seat being a DEFAULT, and a
            // scope is now allowed to say "0–1" out loud — which is the same
            // thing the exilus slot has always been able to say, and the page
            // draws all four axes as one range control. The derived answer is
            // untouched, so no search grows unless somebody widens it.
            //
            // A slot with nothing marked still resolves to `none`, which is
            // what an empty slot IS — that case is the `else` below.
            let marked: Vec<_> = mine.iter().filter(|(_, m)| m.as_str() == "search").collect();
            if marked.is_empty() {
                return vec![empty()];
            }
            let mut opts: Vec<(String, wfsim_engine::data::arcanes::ArcaneFx)> =
                if empty_mark == Some("search") { vec![empty()] } else { Vec::new() };
            opts.extend(marked.into_iter().map(|(id, _)| ((*id).clone(), fx(id))));
            opts
        })
        .collect();
    // The product, in pool order: `arcane_sets[i]` names what `arcanes[i]` is.
    let deployment = get_str(v, "deployment", "").to_string();
    // THE VALENCE AXIS. `valence` is a MARK MAP (element -> "search"), the same
    // shape `modes` and `arcanes` use; `valence_element` is what a request with
    // no axis pins, and what every caller written before the axis existed sends.
    let valence_bonus = {
        let min = wfsim_engine::data::weapons::valence_of(&info.id).map_or(0.0, |s| s.min);
        get_f64(v, "valence_bonus", min)
    };
    let valences: Vec<String> = {
        let spec = wfsim_engine::data::weapons::valence_of(&info.id);
        let marked: Vec<String> = v
            .get("valence")
            .and_then(|x| x.as_object())
            .map(|o| {
                o.iter()
                    .filter(|(_, m)| m.as_str().is_some_and(|s| s != "off"))
                    .map(|(k, _)| k.clone())
                    .filter(|k| spec.is_some_and(|s| s.elements.iter().any(|e| e == k)))
                    .collect()
            })
            .unwrap_or_default();
        if !marked.is_empty() {
            let mut m = marked;
            m.sort();
            m
        } else {
            // No axis: the pinned element, or the single empty slot an ordinary
            // weapon has. A one-entry table is what every scope had before.
            // Through the same resolver the panel uses, so a search that names
            // no element scores the weapon the replay will fire.
            vec![valence_element_of(v, &info.id)]
        }
    };
    let mut arcane_sets: Vec<Vec<String>> = vec![Vec::new()];
    let mut arcanes: Vec<wfsim_engine::data::arcanes::ArcaneFx> =
        vec![wfsim_engine::data::arcanes::ArcaneFx::none()];
    for slot in &per_slot {
        let mut ids = Vec::new();
        let mut fxs = Vec::new();
        for (set, fx) in arcane_sets.iter().zip(arcanes.iter()) {
            for (id, add) in slot {
                let mut s = set.clone();
                s.push(id.clone());
                ids.push(s);
                fxs.push(wfsim_engine::data::arcanes::ArcaneFx::merged(&[
                    fx.clone(),
                    add.clone(),
                ]));
            }
        }
        arcane_sets = ids;
        arcanes = fxs;
    }

    // No cap (allow spending local resources). The funnel handles large
    // spaces by culling obviously-bad combos in cheap early rounds.

    // ---- final-round contract: the last round is
    // guaranteed `finalists` candidates × `final_runs` runs; everything
    // before only whittles the field down (schedule + adaptive racing).
    //
    // `final_runs` FALLS BACK TO THE SCENARIO'S `runs`.
    // How hard you measure is the scenario's question and it is already
    // answered there — a second default here is how a winner gets crowned at a
    // precision the replay never used. The web client stops sending its own
    // and this is what it lands on.
    // Falls back to the FIGHT's run count rather than a second reading of
    // `runs`. The two differed only past 20,000 — where the sim clamps and the
    // search did not — which is a divergence nobody would have gone looking for.
    let final_runs = get_u32(v, "final_runs", fight.runs).clamp(1, 100_000);
    let finalists = get_u32(v, "finalists", 10).clamp(1, 100) as usize;

    // ---- THE FIGHT: the simulator's, not a second reading of it ----------
    //
    // Parsing every field below a second time here gives two readings that
    // drift (see `parse_fight`). The optimizer's winner is replayed under the
    // simulator's fight, so the only safe number of places to decide what that
    // fight IS, is one.
    //
    // What stays here is what is genuinely the optimizer's: the scope to
    // search and the budget to spend.
    let fight = parse_fight(v)?;
    // ---- MODE IS A SEARCH DIMENSION ---------------------------------------
    //
    // Marked like the arcanes and the evolutions: `search` puts a mode in the
    // scope, `fixed` pins it and drops the rest, absent means the axis was not
    // sent at all — and then the run plays the ONE mode the request named,
    // which is what every caller written before this does and what a share
    // link or a board submission means.
    let playable = wfsim_engine::data::weapons::play_modes(&info.id);
    let mode_ids: Vec<String> = match v.get("modes").and_then(Value::as_object) {
        Some(m) => {
            let pinned: Vec<String> = m
                .iter()
                .filter(|(_, s)| s.as_str() == Some("fixed"))
                .map(|(k, _)| k.clone())
                .collect();
            let searched: Vec<String> = if pinned.is_empty() {
                m.iter()
                    .filter(|(_, s)| s.as_str() == Some("search"))
                    .map(|(k, _)| k.clone())
                    .collect()
            } else {
                pinned
            };
            // Only modes this weapon actually has. A scope carried over from
            // another weapon names modes that do not exist here, and a search
            // over none of them is not a scope.
            let mut keep: Vec<String> = searched
                .into_iter()
                .filter(|id| playable.iter().any(|p| p.id == id))
                .collect();
            keep.sort();
            keep.dedup();
            if keep.is_empty() { vec![fight.mode.clone()] } else { keep }
        }
        None => vec![fight.mode.clone()],
    };
    let modes: Vec<ModeForms> = mode_ids.iter().map(|id| mode_forms(info, id)).collect();
    // THE VARIANT TABLE: every (mode, evolution set) the scope holds. One mode
    // is the ordinary case and reproduces exactly what a single `fire_id` did.
    // A VARIANT IS A (MODE, EVOLUTION SET, VALENCE) TRIPLE. Each is a fact about
    // WHICH WEAPON a candidate fires, which is why they belong together: the
    // enumerator asks one question — "what am I building on" — and gets one
    // index back.
    let n_val = valences.len().max(1);
    let variants: Vec<(usize, usize, usize)> = (0..modes.len())
        .flat_map(|mi| {
            (0..evo_sets.len()).flat_map(move |ei| (0..n_val).map(move |vi| (mi, ei, vi)))
        })
        .collect();
    // Read off the fight before the arena is moved into the scenario. These
    // are what the PLAN reports about itself, not decisions it makes.
    let (headshot_pct, duration, level, steel_path) =
        (fight.headshot_pct, fight.duration, fight.level, fight.steel_path);
    let fight_enemy_name = fight.enemy_name.clone();
    // THE REST OF THE ROSTER, resolved once for the search — see `seats_beside`.
    let (roster, _) = crate::simulate::seats_beside(v, &fight.arena, info)?;


    // Assembled ENTIRELY from the fight — no field is re-read from the request
    // here, which is what makes "the search and the replay run the same fight"
    // structural rather than a thing to keep checking.
    let scenario = Scenario {
        // THE FIGHT'S OWN ROSTER, resolved through `seat_the_rest` — the same
        // function `simulate` and the combat record go through, so a search
        // ranks builds in the fight its winner will be replayed in.
        also_acting: roster,
        arena: fight.arena,
        denied_buff_triggers: fight.denied_buff_triggers.clone(),
        frenzy: fight.has_frenzy,
        // ANY mode in the scope that cycles turns this on; a candidate that is
        // not cycling has no second form, and `evaluate` reads the PAIR
        // (`incarnon_cycle`, `base_panel`) — so a scope holding both plays each
        // variant its own way rather than forcing one on the other.
        incarnon_cycle: variants
            .iter()
            .any(|&(mi, _, _)| modes[mi].cycle_from.is_some()),
        frenzy_lock: fight.cycle_frenzy_lock,
        frenzy_locks: fight.frenzy_locks,
        buff_cfg: fight.buff_cfg.unwrap_or_default(),
        infinite_ammo: fight.infinite_ammo,
        policy: fight.policy,
    };

    let starts = parse_starts(v, &pool, &arcane_sets).map_err(err_json)?;
    Ok(OptimizePlan {
        weapon_id: info.id.clone(),
        pool,
        cap,
        constraints,
        min_slots,
        build_size,
        evo_sets,
        variant_forbids,
        modes,
        variants,
        exilus_defs,
        arcanes,
        arcane_sets,
        deployment: deployment.clone(),
        valences: valences.clone(),
        valence_bonus,
        scenario,
        final_runs,
        finalists,
        headshot_pct,
        duration,
        target_name: fight_enemy_name,
        level,
        steel_path,
        threads: v
            .get("threads")
            .and_then(|x| x.as_u64())
            .unwrap_or(0)
            .min(256) as usize,
        max_evals: v.get("max_evals").and_then(|x| x.as_u64()).unwrap_or(0),
        descent: v.get("strategy").and_then(|x| x.as_str()) == Some("descent"),
        swap_width: v.get("swap_width").and_then(|x| x.as_u64()).unwrap_or(1).clamp(1, 8) as u32,
        starts,
        shards: v.get("shards").and_then(|x| x.as_u64()).unwrap_or(1).clamp(1, 64) as u32,
        shard: v.get("shard").and_then(|x| x.as_u64()).unwrap_or(0).min(63) as u32,
        replay_base: {
            // A CLONE, not a rebuild. Listing the fields to copy is the mistake
            // this exists to end — the whole request is the fight plus the
            // build, `simulate_json` reads exactly the keys it knows and
            // ignores the rest, so the optimizer's own marks (`arcanes`,
            // `modes`, `valence`, `exilus`, the budget) simply ride along
            // inert while `entry` overwrites the axes that differ per row.
            let mut r = v.clone();
            if let Some(o) = r.as_object_mut() {
                // The FINAL ROUND's precision, because that is what the row's
                // number is the mean of. The fight's own count is what the
                // replay would otherwise use, and a row measured at one
                // precision re-run at another is a comparison of two things.
                o.insert("runs".into(), json!(final_runs));
                // The one field worth stripping: a resume checkpoint is the
                // whole surviving field, and twenty rows would each carry a
                // copy of it.
                o.remove("__resume");
            }
            r
        },
    })
}

/// Where a previous session stopped: the round to start at, and the field that
/// round takes as input, each entry the IDENTITY of a job — (ordered pool
/// indices, evolution-set index, exilus choice, arcane index). Identities only:
/// resolved panels are rebuilt, so a checkpoint stays small and cannot drift
/// from what the enumerator would produce.
#[derive(Debug, Clone)]
pub enum ResumeFrom {
    /// After a completed funnel ROUND.
    Round {
        round: usize,
        alive: Vec<JobIdentity>,
        /// The job count the ORIGINAL run's schedule was built from. The round
        /// plan is a function of it, so replaying round N needs the same
        /// number — deriving it from the (already narrowed) survivor list
        /// would shorten the schedule and change what round N means.
        jobs_at_start: usize,
    },
}

/// One surviving job, by identity: (ordered pool indices, evolution-set index,
/// exilus choice, arcane index).
pub type JobIdentity = (Vec<usize>, u32, u32, usize);

/// Where a completed round publishes its field: `(next_round, jobs the
/// schedule was built from, survivors, that round's leaderboard)`. The
/// leaderboard rides along because the same snapshot answers both questions a
/// killed run leaves open — where to continue, and what it had found.
pub type CheckpointSink<'a> = dyn Fn(usize, usize, &[JobIdentity], &Value) + 'a;

/// Where the SEARCH publishes its best-so-far. Result-shaped, so a cancel
/// renders it through the same path a finished run takes. Display only: the
/// screen is one pass over the whole scope, so a snapshot of it is NOT a
/// resume point — continuing from one would silently drop the unwalked part.
pub type BoardSink<'a> = dyn Fn(&Value) + 'a;

/// The descent's starts. Each is a list of mod ids, or an object that also
/// pins: `{"mods": [...], "locked": [...], "arcane": id | [ids], "lock_arcane":
/// bool}` — a locked card or arcane is never swapped out of THAT start. An id
/// the scope does not hold is refused, not dropped: a start that silently lost
/// its pin searches something the player did not ask for.
fn parse_starts(
    v: &Value,
    pool: &[ModDef],
    arcane_sets: &[Vec<String>],
) -> Result<Vec<wfsim_optimizer::descent::Start>, String> {
    let ids = |x: Option<&Value>| -> Vec<String> {
        match x {
            Some(Value::String(s)) => vec![s.clone()],
            Some(Value::Array(a)) => a.iter().filter_map(|i| i.as_str().map(str::to_string)).collect(),
            _ => Vec::new(),
        }
    };
    let mod_ix = |id: &str| -> Result<usize, String> {
        pool.iter()
            .position(|m| m.id == id)
            .ok_or_else(|| format!("start: {id} is not in this search's mod scope"))
    };
    let mut out = Vec::new();
    for s in v.get("starts").and_then(Value::as_array).into_iter().flatten() {
        let (mods, locked, arcane, lock_arcane) = match s {
            Value::Array(_) => (ids(Some(s)), Vec::new(), Vec::new(), false),
            _ => (
                ids(s.get("mods")),
                ids(s.get("locked")),
                ids(s.get("arcane")),
                s.get("lock_arcane").and_then(Value::as_bool).unwrap_or(false),
            ),
        };
        // An arcane start names the WORN arcanes; the set it matches may fill
        // its other seats with "none".
        let arcane = if arcane.is_empty() {
            None
        } else {
            let mut want = arcane.clone();
            want.sort();
            let found = arcane_sets.iter().position(|set| {
                let mut worn: Vec<String> = set.iter().filter(|a| *a != "none").cloned().collect();
                worn.sort();
                worn == want
            });
            Some(found.ok_or_else(|| {
                format!("start: arcane {} is not in this search's arcane scope", arcane.join(" + "))
            })?)
        };
        out.push(wfsim_optimizer::descent::Start {
            mods: mods.iter().map(|m| mod_ix(m)).collect::<Result<_, _>>()?,
            locked: locked.iter().map(|m| mod_ix(m)).collect::<Result<_, _>>()?,
            arcane,
            lock_arcane,
        });
    }
    Ok(out)
}

/// GRADE the search against ground truth — the same request, the same plan,
/// the same fight, answered twice: once by the production search and once by
/// exhausting the scope and evaluating every job flat.
///
/// This goes through [`parse_optimize`] rather than assembling a scenario of
/// its own, because a grader that builds its own fight grades a different one
/// — the exact failure this repo has already been bitten by (OPTIMIZER.md,
/// "The search and the replay must be the SAME fight").
///
/// It REFUSES a scope it cannot exhaust. A reference that samples is not a
/// reference; if the scope is too big to enumerate, the honest answer is to
/// say so and let the caller narrow it, not to grade against a guess.
pub fn grade_optimize(
    v: &Value,
    truth_runs: u32,
    max_jobs: usize,
    search_evals: u64,
    explore_frac: f64,
) -> Value {
    use wfsim_optimizer::truth::{judge, Truth};
    let plan = match parse_optimize(v) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let OptimizePlan {
        pool,
        cap,
        constraints,
        min_slots,
        build_size,
        evo_sets,
        variant_forbids,
        exilus_defs,
        arcanes,
        scenario,
        final_runs,
        finalists,
        deployment,
        valences,
        valence_bonus,
        modes,
        variants,
        weapon_id,
        threads,
        descent: plan_descent,
        starts,
        swap_width,
        ..
    } = plan;
    wfsim_optimizer::set_worker_threads(threads);
    let info = weapon(&weapon_id);
    // THE WEAPON'S OWN SLOTS, exilus included: a polarity belongs to the weapon
    // and not to the slot it sits on, so the exilus one is in the pool whether or
    // not a candidate seats an exilus mod (docs/INVESTMENT.md). The search read
    // the eight main slots alone and charged a Forma the board did not.
    let innate = innate_slots_for(&info.id);
    let exilus_refs: Vec<Option<&ModDef>> = exilus_defs.iter().map(|o| o.as_ref()).collect();
    // THE VALENCE IS PER VARIANT, so this takes it rather than capturing one:
    // a scope searching three progenitor elements builds three different
    // weapons, and a closure that knew only one would score all of them as the
    // first.
    let deployed = |id: &str, refs: &[&str], val: &str| {
        let mut b = WeaponBase::from_data(id, true, refs);
        if !deployment.is_empty() {
            wfsim_engine::data::weapons::apply_deployment(&mut b, id, &deployment);
        }
        if !val.is_empty() {
            wfsim_engine::data::weapons::apply_valence(&mut b, id, val, valence_bonus);
        }
        b
    };

    // ---- exhaust the scope (the same walk the search starts from) ----
    let state = FunnelState::default();
    let mut cands: Vec<Candidate> = Vec::new();
    for (vi, &(mi, ei, li)) in variants.iter().enumerate() {
        let (m, set) = (&modes[mi], &evo_sets[ei]);
        let val: &str = valences.get(li).map_or("", String::as_str);
        let refs: Vec<&str> = set.iter().map(String::as_str).collect();
        let unlocked = match m.unlock_evo.as_deref() {
            Some(u) => set.iter().any(|e| e == u),
            None => true,
        };
        let (base, base_form) = if unlocked {
            (
                deployed(&m.fire_id, &refs, val),
                m.cycle_from.as_ref().map(|id| deployed(id, &refs, val)),
            )
        } else {
            (deployed(&m.untransformed_id, &refs, val), None)
        };
        // What THIS variant cannot equip is a forbid like any other: a mod that
        // needs the same trigger on every firing mode is out of the sets that
        // install a second one. The grader must walk exactly the space the
        // search walks, so it applies the same list.
        let vc = Constraints {
            require: constraints.require.clone(),
            forbid: constraints
                .forbid
                .iter()
                .cloned()
                .chain(
                    variant_forbids[ei]
                        .iter()
                        .enumerate()
                        .filter(|(_, &f)| f)
                        .map(|(i, _)| pool[i].id.to_string()),
                )
                .collect(),
        };
        // A required mod this variant cannot equip empties it — `parse_optimize`
        // has already refused a scope where NO variant can, so this is the
        // ordinary "this set is not the one" case.
        if vc.require.iter().any(|r| vc.forbid.iter().any(|f| f == r)) {
            continue;
        }
        let (mut c, _stats, complete) = enumerate_candidates_observed(
            &pool,
            &base,
            base_form.as_ref(),
            vi as u32,
            min_slots as u32,
            build_size as u32,
            60,
            &innate,
            &vc,
            &exilus_refs,
            Some(&state),
            max_jobs.saturating_sub(cands.len()).max(1),
            &scenario.arena.tenno,
            scenario.policy,
        );
        cands.append(&mut c);
        if !complete || cands.len() >= max_jobs {
            return err_json(format!(
                "this scope is too big to grade: it does not fit {max_jobs} candidates. \
                 Ground truth means evaluating EVERY build, so narrow the scope \
                 (fewer pooled mods, or pin some) and grade that."
            ));
        }
    }
    if cands.is_empty() {
        return err_json("no legal builds in this scope");
    }
    let jobs: Vec<Job> = (0..cands.len())
        .flat_map(|i| (0..arcanes.len()).map(move |a| (i, a)))
        .collect();
    if jobs.len() > max_jobs {
        return err_json(format!(
            "{} jobs ({} builds x {} arcane sets) exceeds the {max_jobs} the grader will exhaust",
            jobs.len(),
            cands.len(),
            arcanes.len()
        ));
    }

    // ---- the reference, twice: a reference that cannot reproduce itself
    // under a second seed has not established anything.
    let a = Truth::measure(&cands, &jobs, &arcanes, &scenario, truth_runs, 0xA11CE);
    let b = Truth::measure(&cands, &jobs, &arcanes, &scenario, truth_runs, 0xB0B);
    let answer = a.indistinguishable(3.0);
    let settled = answer.contains(&b.best()) && b.indistinguishable(3.0).contains(&a.best());
    let overlap = a.agrees_with(&b, finalists);

    // ---- the PRODUCTION PIPELINE, on the same scope ----
    //
    // Search AND funnel, not just the funnel. Grading the funnel alone was
    // grading the half that was already good: it is handed a job list, and the
    // half that decides WHAT IS IN that list is the half that could lose the
    // winner. `search_evals` is the budget under test — 0 runs the search to
    // the end of the space, which is what a scope small enough to exhaust gets
    // in production too.
    let families: Vec<Option<&'static str>> = pool.iter().map(|m| m.family).collect();
    let usable: Vec<usize> = (0..pool.len()).collect();
    let required: Vec<usize> = constraints
        .require
        .iter()
        .filter_map(|r| pool.iter().position(|m| m.id == *r))
        .collect();
    let space =
        wfsim_optimizer::space::SubsetSpace::new(&families, &usable, &required, min_slots, build_size);
    let forms: Vec<(WeaponBase, Option<WeaponBase>)> = variants
        .iter()
        .map(|&(mi, ei, li)| {
            let (m, set) = (&modes[mi], &evo_sets[ei]);
            let val: &str = valences.get(li).map_or("", String::as_str);
            let refs: Vec<&str> = set.iter().map(String::as_str).collect();
            let unlocked = match m.unlock_evo.as_deref() {
                Some(u) => set.iter().any(|e| e == u),
                None => true,
            };
            if unlocked {
                (
                    deployed(&m.fire_id, &refs, val),
                    m.cycle_from.as_ref().map(|id| deployed(id, &refs, val)),
                )
            } else {
                (deployed(&m.untransformed_id, &refs, val), None)
            }
        })
        .collect();
    let expand = |subset: &[usize]| -> Vec<Candidate> {
        let mut out = Vec::new();
        for (vi, (base, base_form)) in forms.iter().enumerate() {
            // A mod this variant cannot equip vetoes the (subset, variant)
            // PAIR, not the subset: the same eight mods are a legal build under
            // an evolution set that leaves the Incarnon form out. Indexed by
            // the EVOLUTION SET rather than by the variant: an equip rule is
            // asked of every firing mode the weapon has, so which mode is being
            // played cannot change the answer — only which forms are installed.
            if subset.iter().any(|&i| variant_forbids[variants[vi].1][i]) {
                continue;
            }
            wfsim_optimizer::expand_one(
                &pool, base, base_form.as_ref(), vi as u32, cap, &innate, &exilus_refs,
                subset, &scenario.arena.tenno, scenario.policy, &mut out,
            );
        }
        out
    };
    let cfg = wfsim_optimizer::search::SearchConfig {
        max_evals: search_evals,
        explore_frac,
        keep: 65_536,
        seed: 0xDEAD_BEEF,
        swap_width,
        ..Default::default()
    };
    let (screened, sstats) = if plan_descent {
        wfsim_optimizer::descent::descent(
            &space, &pool, &starts, &expand, &arcanes, &scenario, &cfg, None, None,
        )
    } else {
        wfsim_optimizer::search::search(&space, &expand, &arcanes, &scenario, &cfg, None, None)
    };
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
    let rounds = schedule_to(sjobs.len(), final_runs, finalists);
    let planned: u64 = {
        let mut field = sjobs.len() as u64;
        let mut n = sstats.evals;
        for &(r, keep, _) in &rounds {
            n += field * u64::from(r);
            field = field.min(keep as u64);
        }
        n
    };
    let last = run_funnel(
        &sc, &arcanes, &scenario, sjobs, &rounds, 0xDEAD_BEEF, false,
        None, None, 0, None, None,
    );
    // Map each result back to its position in the exhaustive job list. Both
    // sides build candidates through `expand_one` from an ascending subset, so
    // the identity is exact rather than a resolved-vector comparison.
    let ix_of: std::collections::HashMap<(Vec<usize>, u32, u32, usize), usize> = jobs
        .iter()
        .enumerate()
        .map(|(ji, &(ci, ai))| {
            ((cands[ci].ordered.clone(), cands[ci].variant, cands[ci].exilus, ai), ji)
        })
        .collect();
    let mut board: Vec<usize> = Vec::new();
    let mut unmatched = 0usize;
    for &((ci, ai), _) in last.iter() {
        let k = (sc[ci].ordered.clone(), sc[ci].variant, sc[ci].exilus, ai);
        match ix_of.get(&k) {
            Some(&ji) => board.push(ji),
            None => unmatched += 1,
        }
    }
    if board.is_empty() {
        return err_json(
            "the search returned nothing the exhaustive enumeration contains —              the two disagree about the scope, which is a bug in one of them",
        );
    }
    let verdict = judge(&a, &board, finalists, planned);

    let row = |ji: usize| -> Value {
        let (ci, ai) = jobs[ji];
        let c = &cands[ci];
        json!({
            "mods": c.ordered.iter().map(|&i| pool[i].id).collect::<Vec<_>>(),
            "arcane": ai,
            "vector": c.panel.damage.iter_nonzero()
                .map(|(t, v)| format!("{t:?} {v:.0}")).collect::<Vec<_>>(),
            "mean": a.est[ji].mean,
            "se": a.est[ji].se,
        })
    };
    json!({
        "ok": true,
        "scope": { "builds": cands.len(), "jobs": jobs.len(), "exhaustive": true },
        "reference": {
            "runs": truth_runs,
            "sims": verdict.reference_sims,
            "answer_set": answer.len(),
            "settled": settled,
            "cross_seed_overlap": overlap,
            "top": a.order.iter().take(finalists).map(|&j| row(j)).collect::<Vec<_>>(),
        },
        "search": {
            "unmatched": unmatched,
            "coverage": sstats.coverage(),
            "space": sstats.space as f64,
            "exhaustive": sstats.exhaustive,
            "subsets": sstats.subsets,
            "neighbours": sstats.neighbours,
            "screen_evals": sstats.evals,
            "rank": verdict.rank,
            "regret": verdict.regret,
            "within_noise": verdict.within_noise,
            "recall": verdict.recall,
            "sims": verdict.sims,
            "top": board.iter().take(finalists).map(|&j| row(j)).collect::<Vec<_>>(),
        },
    })
}

/// The heavy phase: enumerate candidates, run the funnel, build the result
/// payload. Blocking — the caller decides where it runs (a worker thread on
/// the native server, a Web Worker under wasm). Progress is published
/// through `state` (poll it from another thread, or — single-threaded —
/// read it inside `on_round`, which fires after every completed funnel
/// round); cancellation is `state.cancel`. `on_enumerated(candidates,
/// jobs)` fires once when enumeration finishes and the funnel is about to
/// start. Native callers poll and pass `on_round: None`; the wasm build has
/// no second thread to poll from, so the callback is its progress channel.
/// The uninterrupted entry point — no checkpointing, no resume.
pub fn run_optimize(
    plan: OptimizePlan,
    state: &FunnelState,
    on_enumerated: impl FnOnce(usize, usize),
    on_round: Option<&dyn Fn()>,
) -> Value {
    run_optimize_resumable(plan, state, on_enumerated, on_round, None, None, None)
}

/// As [`run_optimize`], plus the two halves of resumability: `resume` skips
/// straight to a saved round, and `on_checkpoint` publishes the field after
/// every completed round.
#[allow(clippy::too_many_arguments)]
pub fn run_optimize_resumable(
    plan: OptimizePlan,
    state: &FunnelState,
    on_enumerated: impl FnOnce(usize, usize),
    on_round: Option<&dyn Fn()>,
    resume: Option<ResumeFrom>,
    // (next_round, jobs_at_start, identities). The funnel hands back candidate
    // INDICES, which mean nothing outside this call — translate them here,
    // where the candidate table is, so a caller can persist something that
    // survives the process.
    on_checkpoint: Option<&CheckpointSink<'_>>,
    // Best-so-far during the screen. A browser cancel TERMINATES the worker,
    // so a leaderboard that has not already left it is lost (:
    // 20 minutes, cancelled, nothing shown).
    on_board: Option<&BoardSink<'_>>,
) -> Value {
    let OptimizePlan {
        pool,
        cap,
        constraints,
        min_slots,
        build_size,
        evo_sets,
        variant_forbids,
        exilus_defs,
        arcanes,
        arcane_sets,
        deployment,
        valences,
        valence_bonus,
        scenario,
        final_runs,
        finalists,
        headshot_pct,
        duration,
        target_name,
        level,
        steel_path,
        weapon_id,
        modes,
        variants,
        threads,
        max_evals,
        descent,
        starts,
        swap_width,
        shard,
        shards,
        replay_base,
    } = plan;
    // Compute budget: 0 = auto (all cores minus two — the machine must stay
    // usable while the search runs). Applies to the screen and every round.
    wfsim_optimizer::set_worker_threads(threads);
    let info = weapon(&weapon_id);

    // How many screened jobs survive the search into the funnel. The search
    // holds a heap this size, so memory is O(SCREEN_KEEP) whatever the scope.
    const SCREEN_KEEP: usize = 65_536;
    // THE WEAPON'S OWN SLOTS, exilus included: a polarity belongs to the weapon
    // and not to the slot it sits on, so the exilus one is in the pool whether or
    // not a candidate seats an exilus mod (docs/INVESTMENT.md). The search read
    // the eight main slots alone and charged a Forma the board did not.
    let innate = innate_slots_for(&info.id);
    let exilus_refs: Vec<Option<&ModDef>> = exilus_defs.iter().map(|o| o.as_ref()).collect();
    // The form(s) an evolution set resolves to, decided once in
    // `parse_optimize`. A single-form weapon has NO second panel — handing
    // the enumerator a duplicate of the first would tell it there was a cycle
    // to simulate, and the scenario says there is not.
    // Every base the worker builds sits in the run's DEPLOYMENT, so a search
    // scores the same environment the sim would replay it in.
    // THE VALENCE IS PER VARIANT, so this takes it rather than capturing one:
    // a scope searching three progenitor elements builds three different
    // weapons, and a closure that knew only one would score all of them as the
    // first.
    let deployed = |id: &str, refs: &[&str], val: &str| {
        let mut b = WeaponBase::from_data(id, true, refs);
        if !deployment.is_empty() {
            wfsim_engine::data::weapons::apply_deployment(&mut b, id, &deployment);
        }
        if !val.is_empty() {
            wfsim_engine::data::weapons::apply_valence(&mut b, id, val, valence_bonus);
        }
        b
    };
    // A VARIANT IS A (MODE, EVOLUTION SET) PAIR, so the forms come from both:
    // the mode says which entries this candidate fires, the set says whether it
    // can reach the second one.
    let forms_for = |vi: usize, set: &[String], refs: &[&str]| {
        let m = &modes[variants[vi].0];
        // …and WHICH WEAPON this variant is: the progenitor element is the
        // third leg of the triple, so a scope searching several builds several.
        let val: &str = valences.get(variants[vi].2).map_or("", String::as_str);
        // Can THIS evolution set reach the second form? Without the unlock
        // there is nothing to transform into, so the candidate is fired in
        // the form it has and carries no second panel — which is what tells
        // `evaluate` not to run a cycle for it.
        let unlocked = match m.unlock_evo.as_deref() {
            Some(u) => set.iter().any(|e| e == u),
            None => true,
        };
        if !unlocked {
            return (deployed(&m.untransformed_id, refs, val), None);
        }
        (
            deployed(&m.fire_id, refs, val),
            m.cycle_from.as_ref().map(|id| deployed(id, refs, val)),
        )
    };
    let cancelled_json = |n_cands: usize| {
        // Cancelled before anything was ranked — a clean empty cancellation.
        json!({
            "ok": true, "cancelled": true,
            "candidates": n_cands, "jobs": 0,
            "final_runs": final_runs, "finalists": finalists,
            "headshot_pct": headshot_pct, "duration": duration,
            "results": [],
            "target": { "name": target_name, "level": level, "steel_path": steel_path },
        })
    };

    // One leaderboard row. The finished result and every best-so-far snapshot
    // both go through this, so a cancelled run renders in exactly the same UI
    // as a completed one — same fields, same renderer.
    let entry = |rank: usize, c: &Candidate, ai: usize, s: &Summary| -> Value {
        let mods: Vec<&str> = c.ordered.iter().map(|&i| pool[i].id).collect();
        // One id per slot, in pool order — the same shape the builder takes,
        // because "apply this result" should be a copy and not a translation.
        let marked: Vec<String> = arcane_sets
            .get(ai)
            .cloned()
            .unwrap_or_else(|| vec!["none".to_string()]);
        let (ids, ranks): (Vec<String>, Vec<u32>) = marked
            .iter()
            .map(|id| {
                let card = wfsim_engine::data::mods::split_rank(id).0;
                let rank = wfsim_engine::data::arcanes::slot_of(card)
                    .and_then(|s| arcane_at_rank(s, id))
                    .map_or(0, |(_, r)| r);
                (card.to_string(), rank)
            })
            .unzip();
        // THE ROW, AS A REQUEST THAT REPRODUCES IT. POST this to
        // `/api/simulate` and the answer is this row's number — no assembly, no
        // translation, nothing for a caller to forget.
        //
        // The named fields below still say what the build IS, because a reader
        // and a build editor both need that; this says how to RUN it, and it is
        // the half that must never be reconstructed by hand. See `replay_base`:
        // everything not overwritten here rode in from the request, so an axis
        // added tomorrow arrives without this function being touched.
        let replay = {
            let mut r = replay_base.clone();
            if let Some(o) = r.as_object_mut() {
                // ONE FLAT LIST OF MOD IDS, exilus included — the shape
                // `simulate_json` reads and the shape the builder's own payload
                // has. The exilus slot is a slot.
                let mut all: Vec<&str> = mods.clone();
                if let Some(m) = exilus_defs[c.exilus as usize].as_ref() {
                    all.push(m.id);
                }
                o.insert("mods".into(), json!(all));
                o.insert("arcane".into(), json!(ids));
                o.insert("arcane_rank".into(), json!(ranks));
                o.insert(
                    "evolutions".into(),
                    json!(evo_sets[variants[c.variant as usize].1]),
                );
                o.insert("mode".into(), json!(modes[variants[c.variant as usize].0].id));
                // The ELEMENT is the axis; the BONUS is the scope's and rode in
                // with the request. A row that is not out of a Lich leaves both
                // alone.
                if let Some(el) = valences.get(variants[c.variant as usize].2) {
                    o.insert("valence_element".into(), json!(el));
                }
            }
            r
        };
        json!({
            "rank": rank + 1,
            "kills": s.mean_kills,
            "kill_progress": s.mean_kill_progress,
            // HOW WELL THE SEARCH KNOWS ITS OWN NUMBER. The page re-measures
            // this row through `/api/simulate` and shows THAT; this is what
            // lets it say whether the two disagree by more than the dice.
            // Without it the comparison needs a hand-picked tolerance, which is
            // a number that is too tight at 40 runs and too loose at 1000.
            "kill_progress_se": s.std_kill_progress / f64::from(final_runs.max(1)).sqrt(),
            "dps": s.effective_dps,
            "kills_min": s.min_kills,
            "kills_max": s.max_kills,
            "mods": mods,
            "arcane": ids,
            "arcane_rank": ranks,
            "evolutions": evo_sets[variants[c.variant as usize].1],
            // HOW THE WINNER IS PLAYED. A row without it cannot be added to a
            // build: mode is part of a build, and a search that ranged over
            // modes has no other way to say which one won.
            "mode": modes[variants[c.variant as usize].0].id,
            // WHICH WEAPON THIS ROW IS, when the scope searched more than one:
            // the progenitor element it was scored with, so the build a row
            // becomes fires the weapon the row was measured on.
            "valence": valences
                .get(variants[c.variant as usize].2)
                .cloned()
                .unwrap_or_default(),
            "exilus": exilus_defs[c.exilus as usize].as_ref().map(|m| m.id).unwrap_or("none"),
            "forma": { "used": c.plan.forma_used, "total_drain": c.plan.total_drain },
            "replay": replay,
        })
    };
    // A whole result payload. Snapshots carry `cancelled: true` — a board only
    // ever gets shown because a run stopped early, and the flag is what makes
    // the UI label it best-so-far (lower precision than a full run).
    let board_json = |rows: Vec<Value>, n_cands: usize, n_jobs: usize| -> Value {
        json!({
            "ok": true, "cancelled": true,
            "candidates": n_cands, "jobs": n_jobs,
            "final_runs": final_runs, "finalists": finalists,
            "headshot_pct": headshot_pct, "duration": duration,
            "results": rows,
            "target": { "name": target_name, "level": level, "steel_path": steel_path },
        })
    };

    // ---- ONE PATH, at every scope ----
    //
    // NOT two regimes split by a candidate threshold — materialize-then-funnel
    // below it, walk-and-screen above. Both walk the space depth-first, so both
    // leave a lexicographic CORNER behind when they are cut short, and being
    // cut short is the normal case (docs/OPTIMIZER.md). Two regimes are also
    // two sets of bugs; the
    // tenno/policy leak of 2026-08-03 existed in exactly one of them.
    //
    // Now the subset space is an INDEX RANGE and the search walks a
    // pseudorandom bijection over it: run it to the end and it is an
    // exhaustive enumeration, stop it early and what it has is a uniform
    // sample. Same loop, same code, no threshold.
    let usable: Vec<usize> = (0..pool.len())
        .filter(|&i| !constraints.forbid.iter().any(|f| f == pool[i].id))
        .collect();
    let required: Vec<usize> = constraints
        .require
        .iter()
        .filter_map(|r| pool.iter().position(|m| m.id == *r))
        .collect();
    let families: Vec<Option<&'static str>> = pool.iter().map(|m| m.family).collect();
    let space =
        wfsim_optimizer::space::SubsetSpace::new(&families, &usable, &required, min_slots, build_size);

    // The bases each VARIANT resolves to, built ONCE. `forms_for` reads data
    // and applies the deployment, which is far too expensive to repeat per
    // proposal.
    let forms: Vec<(WeaponBase, Option<WeaponBase>)> = variants
        .iter()
        .enumerate()
        .map(|(vi, &(_, ei, _))| {
            let set = &evo_sets[ei];
            let refs: Vec<&str> = set.iter().map(String::as_str).collect();
            forms_for(vi, set, &refs)
        })
        .collect();
    // One subset -> every candidate it can produce. The axes INSIDE a subset
    // (element order, exilus option, evolution set) stay exhaustive: a couple
    // of dozen cheap combinations, and handing an exact subproblem to a
    // stochastic search is how an answer gets lost for no reason.
    let expand = |subset: &[usize]| -> Vec<Candidate> {
        let mut out = Vec::new();
        for (vi, (base, base_form)) in forms.iter().enumerate() {
            // A mod this variant cannot equip vetoes the (subset, variant)
            // PAIR, not the subset: the same eight mods are a legal build under
            // an evolution set that leaves the Incarnon form out. Indexed by
            // the EVOLUTION SET rather than by the variant: an equip rule is
            // asked of every firing mode the weapon has, so which mode is being
            // played cannot change the answer — only which forms are installed.
            if subset.iter().any(|&i| variant_forbids[variants[vi].1][i]) {
                continue;
            }
            wfsim_optimizer::expand_one(
                &pool,
                base,
                base_form.as_ref(),
                vi as u32,
                cap,
                &innate,
                &exilus_refs,
                subset,
                &scenario.arena.tenno,
                scenario.policy,
                &mut out,
            );
        }
        out
    };

    // MID-SEARCH RESUME IS GONE, and only the ROUND checkpoint survives. The
    // old one stored a position in a depth-first walk plus the survivors at
    // that cut; the search walks a shuffled index range and climbs from an
    // elite pool, so a position alone no longer describes where it was. It is
    // also worth much less than it was: the screen it protected could run for
    // twenty minutes, while the search runs to a stated budget and publishes a
    // best-so-far the whole way. Restoring it means checkpointing the elite
    // pool by identity and re-screening it on resume — a follow-up, recorded
    // here so it reads as a decision and not as an omission.
    let round_resume = match resume {
        Some(ResumeFrom::Round { round, alive, jobs_at_start }) => {
            Some((round, alive, jobs_at_start))
        }
        _ => None,
    };
    // What the search covered — `None` on a round resume, which does not search.
    let mut search_stats: Option<wfsim_optimizer::search::SearchStats> = None;
    let (cands, last, cancelled, n_jobs) = if let Some((r_round, r_alive, r_jobs_at_start)) = round_resume {
        // ---- RESUME: no walk at all. The checkpoint holds identities, so the
        // candidates are rebuilt with the same plan_forma / resolve_with the
        // enumerator uses and come out bit-identical. Seeds key off the
        // absolute round index, so the numbers match an uninterrupted run.
        let mut cands: Vec<Candidate> = Vec::new();
        let mut jobs: Vec<Job> = Vec::new();
        for (ordered, variant, exilus, ai) in &r_alive {
            // A checkpoint written when the variant table had a different shape
            // — before mode joined it, say — names variants that no longer
            // exist. Dropping those is the same answer the walk would give now;
            // if that empties the list, the error below says so.
            let Some(&(_, ei, _)) = variants.get(*variant as usize) else { continue };
            let set = &evo_sets[ei];
            // A checkpoint predating an equip rule can name a build this variant
            // can no longer wear.
            let Some(forbid) = variant_forbids.get(ei) else { continue };
            if ordered.iter().any(|&i| forbid.get(i).copied().unwrap_or(false)) {
                continue;
            }
            let refs: Vec<&str> = set.iter().map(String::as_str).collect();
            let (base, base_form) = forms_for(*variant as usize, set, &refs);
            let Some(c) = wfsim_optimizer::rebuild_candidate(
                &pool, &base, base_form.as_ref(), &innate, plan.cap, &scenario.arena.tenno, scenario.policy,
                ordered, *variant, *exilus, &exilus_refs,
            ) else { continue };
            if *ai >= arcanes.len() {
                continue;
            }
            cands.push(c);
            jobs.push((cands.len() - 1, *ai));
        }
        if jobs.is_empty() {
            // A checkpoint that survives a pool or Forma change resolves to
            // nothing: say so rather than returning an empty leaderboard.
            return err_json("this saved run no longer matches the current scope — start a new one");
        }
        let n_jobs = jobs.len();
        on_enumerated(cands.len(), n_jobs);
        // The schedule is a function of the ORIGINAL field size, which the
        // checkpoint's round index indexes into — rebuild it the same way so
        // round N means the same thing it did before the reload.
        let rounds = schedule_to(r_jobs_at_start.max(n_jobs), final_runs, finalists);
        let ids_at = |alive: &[(Job, Summary)]| -> Vec<JobIdentity> {
            alive.iter()
                .map(|&((ci, ai), _)| (cands[ci].ordered.clone(), cands[ci].variant, cands[ci].exilus, ai))
                .collect()
        };
        let board_of = |alive: &[(Job, Summary)], nc: usize, nj: usize| -> Value {
            board_json(
                alive.iter().take(finalists).enumerate()
                    .map(|(rank, ((ci, ai), s))| entry(rank, &cands[*ci], *ai, s))
                    .collect(),
                nc, nj,
            )
        };
        let started_with = r_jobs_at_start.max(n_jobs);
        let n_cands = cands.len();
        // A round is ONE blocking call, and round 1 is the whole field before
        // any culling — round boundaries are too coarse to answer a cancel.
        let rboard = on_board.map(|b| move |top: &[(Job, Summary)]| {
            b(&board_of(top, n_cands, n_jobs));
        });
        let wrap = on_checkpoint.map(|cp| move |round: usize, alive: &[(Job, Summary)]| {
            cp(round, started_with, &ids_at(alive), &board_of(alive, n_cands, n_jobs));
        });
        let last = run_funnel(
            &cands, &arcanes, &scenario, jobs, &rounds, 0xDEAD_BEEF, false,
            Some(state), on_round, r_round,
            wrap.as_ref().map(|f| f as &wfsim_optimizer::CheckpointFn<'_>),
            rboard.as_ref().map(|f| f as &wfsim_optimizer::RoundBoardFn<'_>),
        );
        let c = state.cancel.load(std::sync::atomic::Ordering::Relaxed);
        (cands, last, c, n_jobs)
    } else {
        // ---- THE SEARCH ----
        if space.is_empty() {
            return err_json("no builds in this scope (the size range leaves nothing to search)");
        }
        let board = on_board.map(|f| {
            move |top: &[wfsim_optimizer::ScreenedJob]| {
                let rows: Vec<Value> = top.iter().take(finalists).enumerate()
                    .map(|(rank, sj)| entry(rank, &sj.cand, sj.ai, &sj.summary))
                    .collect();
                let walked = state.enumerated.load(std::sync::atomic::Ordering::Relaxed) as usize;
                let screened = state.sims_done.load(std::sync::atomic::Ordering::Relaxed) as usize;
                f(&board_json(rows, walked, screened));
            }
        });
        let cfg = wfsim_optimizer::search::SearchConfig {
            max_evals,
            keep: SCREEN_KEEP,
            seed: 0xDEAD_BEEF,
            swap_width,
            shard,
            shards,
            ..Default::default()
        };
        let board = board.as_ref().map(|f| f as &wfsim_optimizer::ScreenBoardFn<'_>);
        let (screened, stats) = if descent {
            wfsim_optimizer::descent::descent(
                &space, &pool, &starts, &expand, &arcanes, &scenario, &cfg, Some(state), board,
            )
        } else {
            wfsim_optimizer::search::search(&space, &expand, &arcanes, &scenario, &cfg, Some(state), board)
        };
        search_stats = Some(stats);
        if screened.is_empty() {
            if state.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return cancelled_json(0);
            }
            // AN EMPTY SHARD IS NOT A FAILURE. With more workers than index
            // positions — 8 workers over a scope holding one build — every
            // shard but the first owns no ground at all, and each of them used
            // to answer "no legal builds in this scope (Forma / family
            // constraints eliminated all)", a sentence about the pool that was
            // really about the arithmetic. The fleet then surfaced one of those
            // as the whole run's error.
            //
            // Walking nothing is different from walking and finding nothing.
            // Only the second is the Forma/family case.
            if stats.sampled > 0 {
                return err_json(
                    "no legal builds in this scope (Forma / family constraints eliminated all)",
                );
            }
            return json!({
                "ok": true, "cancelled": false,
                "exhaustive": true, "coverage": 0.0,
                "space": stats.space as f64, "searched": 0, "sampled": 0.0,
                "shard": shard, "shards": shards,
                "candidates": 0, "jobs": 0,
                "final_runs": final_runs, "finalists": finalists,
                "headshot_pct": headshot_pct, "duration": duration,
                "results": [],
                "target": { "name": target_name, "level": level, "steel_path": steel_path },
            });
        }
        // Survivors -> a deduplicated candidate table (one build survives under
        // several arcanes) + (job, screen summary) pairs, best first.
        let mut sc: Vec<Candidate> = Vec::new();
        let mut by_ptr: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        let mut slast = Vec::new();
        for sj in &screened {
            let key = std::sync::Arc::as_ptr(&sj.cand) as usize;
            let ci = *by_ptr.entry(key).or_insert_with(|| {
                sc.push((*sj.cand).clone());
                sc.len() - 1
            });
            slast.push(((ci, sj.ai), sj.summary));
        }
        if state.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            // Cancelled mid-search: the screen's own ranking (1-run precision)
            // is the best-so-far leaderboard.
            let n = slast.len();
            (sc, slast, true, n)
        } else {
            let jobs: Vec<Job> = slast.iter().map(|(j, _)| *j).collect();
            let n = jobs.len();
            state.sims_done.store(0, std::sync::atomic::Ordering::Relaxed); // fresh % for the funnel
            on_enumerated(sc.len(), n);
            let rounds = schedule_to(n, final_runs, finalists);
            let ids_at = |alive: &[(Job, Summary)]| -> Vec<JobIdentity> {
                alive.iter()
                    .map(|&((ci, ai), _)| (sc[ci].ordered.clone(), sc[ci].variant, sc[ci].exilus, ai))
                    .collect()
            };
            let board_of_sc = |alive: &[(Job, Summary)], nc: usize, nj: usize| -> Value {
                board_json(
                    alive.iter().take(finalists).enumerate()
                        .map(|(rank, ((ci, ai), s))| entry(rank, &sc[*ci], *ai, s))
                        .collect(),
                    nc, nj,
                )
            };
            let n_sc = sc.len();
            let rboard = on_board.map(|b| move |top: &[(Job, Summary)]| {
                b(&board_of_sc(top, n_sc, n));
            });
            let wrap = on_checkpoint.map(|cp| move |round: usize, alive: &[(Job, Summary)]| {
                cp(round, n, &ids_at(alive), &board_of_sc(alive, n_sc, n));
            });
            let last = run_funnel(
                &sc, &arcanes, &scenario, jobs, &rounds, 0xDEAD_BEEF, false,
                Some(state), on_round,
                0, // the search always screens first, so the funnel starts fresh
                wrap.as_ref().map(|f| f as &wfsim_optimizer::CheckpointFn<'_>),
                rboard.as_ref().map(|f| f as &wfsim_optimizer::RoundBoardFn<'_>),
            );
            let c = state.cancel.load(std::sync::atomic::Ordering::Relaxed);
            (sc, last, c, n)
        }
    };

    // ---- the finalists leaderboard (on cancel: the last completed
    // round's top slice — intermediate rounds can be huge) ----
    let results: Vec<Value> = last
        .iter()
        .take(finalists)
        .enumerate()
        .map(|(rank, ((ci, ai), s))| entry(rank, &cands[*ci], *ai, s))
        .collect();

    // WHAT THE SEARCH ACTUALLY COVERED. A run that did not reach the end of
    // its space has not searched the scope it was given, and it must not read
    // like one that did. `cancelled` cannot carry this — that means "you
    // stopped it" — and neither can a bare flag, because the useful question
    // is HOW MUCH. `exhaustive` says the search reached the end of the index
    // range, in which case its winner is THE winner and not a best-so-far.
    let (exhaustive, coverage, space_size, searched, sampled) = match search_stats {
        Some(st) => (
            st.exhaustive,
            st.coverage(),
            // A space can exceed f64's integer range only in absurd scopes; the
            // UI prints an order of magnitude either way.
            st.space as f64,
            st.subsets,
            st.sampled as f64,
        ),
        // A round resume did not search: it continues a funnel over a field
        // some earlier run already chose, and claiming coverage for it would
        // be claiming credit for a search this call never ran.
        None => (false, 0.0, 0.0, 0, 0.0),
    };
    json!({
        "ok": true,
        "candidates": cands.len(),
        "jobs": n_jobs,
        "cancelled": cancelled,
        "exhaustive": exhaustive,
        "coverage": coverage,
        "space": space_size,
        "searched": searched,
        // INDEX POSITIONS this shard consumed. Coverage above is this shard's
        // alone; a sharded run sums these and divides by `space` to get the
        // coverage of the whole fleet.
        "sampled": sampled,
        "shard": shard,
        "shards": shards,
        "final_runs": final_runs,
        "finalists": finalists,
        "headshot_pct": headshot_pct,
        "duration": duration,
        "results": results,
        "target": { "name": target_name, "level": level, "steel_path": steel_path },
    })
}

/// A SEARCH'S PROGRESS, as both transports report it: the native
/// `/api/optimize/status` and the wasm worker's progress message. Each adds
/// only what it alone owns — the native job id and result, the worker's
/// `rewalking` — so the shared fields cannot drift between the two.
pub fn funnel_status_json(
    state: &FunnelState,
    phase: &str,
    counts: Option<(usize, usize)>,
    elapsed_s: f64,
) -> Value {
    use std::sync::atomic::Ordering;
    let notes: Vec<Value> = state
        .notes
        .lock()
        .unwrap()
        .iter()
        .map(|n| {
            json!({
                "round": n.round, "jobs": n.jobs, "runs": n.runs,
                "by_kills": n.by_kills, "kept": n.kept, "best": n.best, "multishot": n.multishot,
            })
        })
        .collect();
    let mut out = json!({
        "ok": true,
        "phase": phase,
        "elapsed_s": elapsed_s,
        "round": state.round.load(Ordering::Relaxed),
        "rounds": state.rounds.load(Ordering::Relaxed),
        "round_jobs": state.round_jobs.load(Ordering::Relaxed),
        "round_runs": state.round_runs.load(Ordering::Relaxed),
        "sims_done": state.sims_done.load(Ordering::Relaxed),
        "sims_planned": state.sims_planned.load(Ordering::Relaxed),
        "enumerated": state.enumerated.load(Ordering::Relaxed),
        "notes": notes,
    });
    if let Some((cands, jobs)) = counts {
        out["candidates"] = json!(cands);
        out["jobs"] = json!(jobs);
    }
    out
}

#[cfg(test)]
mod descent_start_tests {
    use super::*;

    fn plan(starts: Value) -> Result<OptimizePlan, Value> {
        parse_optimize(&json!({
            "weapon": "larkspur_prime",
            "build_size": 2,
            "mods": { "rubedo_lined_barrel": "search", "critical_focus": "search" },
            "arcanes": { "primary_merciless": "search", "primary_deadhead": "search" },
            "strategy": "descent",
            "starts": starts,
        }))
    }

    /// Both spellings resolve: a bare list is a start with no pins; an object
    /// names its locks and its arcane, matched to the scope's arcane set.
    #[test]
    fn a_start_resolves_its_cards_and_its_arcane() {
        let p = plan(json!([
            ["rubedo_lined_barrel"],
            { "mods": ["critical_focus"], "locked": ["critical_focus"],
              "arcane": "primary_deadhead", "lock_arcane": true },
        ]))
        .expect("a plan");
        let ix = |id: &str| p.pool.iter().position(|m| m.id == id).expect("in scope");
        assert_eq!(p.starts[0].mods, vec![ix("rubedo_lined_barrel")]);
        assert!(p.starts[0].locked.is_empty() && p.starts[0].arcane.is_none());
        let s = &p.starts[1];
        assert_eq!(s.locked, vec![ix("critical_focus")]);
        let arcane = s.arcane.expect("an arcane");
        assert!(p.arcane_sets[arcane].iter().any(|a| a == "primary_deadhead"));
        assert!(s.lock_arcane);
    }

    /// A pin the scope cannot honour is refused, not dropped.
    #[test]
    fn a_start_naming_something_outside_the_scope_is_refused() {
        for starts in [json!([{ "mods": ["serration"] }]), json!([{ "arcane": "primary_crux" }])] {
            let err = plan(starts).err().expect("refused");
            let msg = err["error"].as_str().unwrap_or("");
            assert!(msg.starts_with("start:"), "refused for another reason: {msg}");
        }
    }
}

#[cfg(test)]
mod optimizer_arcane_tests {
    use super::*;

    fn sets(arcanes: Value) -> Vec<Vec<String>> {
        let plan = parse_optimize(&json!({
            "weapon": "larkspur_prime",
            "size": 1,
            "mods": { "rubedo_lined_barrel": "search" },
            "arcanes": arcanes,
        }))
        .expect("a plan");
        plan.arcane_sets
    }

    /// A slot with candidates is never left EMPTY.
    ///
    /// An arcane slot costs nothing — no capacity, no Forma — so empty can
    /// never beat filled, and marking a candidate IS the statement that the
    /// slot should be filled. An implicit "none" on every slot turns 3 marked
    /// primaries and 4 marked secondaries into 4 x 5 = 20 sets, eight of them
    /// with a hole that can only ever tie the same build with the arcane in
    /// it.
    #[test]
    fn a_slot_with_candidates_is_never_left_empty() {
        let s = sets(json!({
            "primary_deadhead": "search",
            "primary_merciless": "search",
            "primary_crux": "search",
            "secondary_deadhead": "search",
            "secondary_merciless": "search",
            "secondary_fortifier": "search",
            "secondary_shiver": "search",
        }));
        assert_eq!(s.len(), 12, "3 primaries x 4 secondaries, and nothing else");
        assert!(
            !s.iter().any(|set| set.iter().any(|id| id == "none")),
            "no set may leave a marked slot empty: {s:?}"
        );

        // A slot with NOTHING marked is still empty — that is what an empty
        // slot IS, and it is the only way "none" survives.
        let s = sets(json!({ "primary_deadhead": "search" }));
        assert_eq!(s, vec![vec!["primary_deadhead".to_string(), "none".to_string()]]);
        let s = sets(json!({}));
        assert_eq!(s, vec![vec!["none".to_string(), "none".to_string()]]);
    }
}

#[cfg(test)]
mod optimizer_evolution_tests {
    use super::*;

    fn sets(evolutions: Value) -> Vec<Vec<String>> {
        parse_optimize(&json!({
            "weapon": "torid",
            "size": 1,
            "mods": { "serration": "search" },
            "evolutions": evolutions,
        }))
        .expect("a plan")
        .evo_sets
    }

    /// The scope's Cartesian product must not enumerate a gapped LADDER.
    ///
    /// A tier with no marks contributes "nothing here", so marking tier 2 and
    /// leaving tier 1 blank pairs them into a set the game cannot make. Every
    /// such set would be truncated at the moment it was scored, so searching
    /// it is not a wrong answer, it is a wasted variant — and a reported one,
    /// since the winner prints the set it was given.
    #[test]
    fn the_search_never_enumerates_a_tier_without_the_one_below_it() {
        // Tier 2 alone: nothing to search, one empty set.
        assert_eq!(sets(json!({ "2": ["torid_final_fusillade"] })), vec![Vec::<String>::new()]);

        // Tier 1 alone: the tier's own two options, both legal.
        let one = sets(json!({ "1": ["torid_evo1_incarnon_form"] }));
        assert_eq!(one, vec![vec!["torid_evo1_incarnon_form".to_string()]]);

        // Both marked: the product stands, because now every set is reachable.
        let two = sets(json!({
            "1": ["torid_evo1_incarnon_form"],
            "2": ["torid_final_fusillade", "torid_survivors_edge"],
        }));
        assert_eq!(two.len(), 2, "{two:?}");
        assert!(two.iter().all(|s| s.contains(&"torid_evo1_incarnon_form".to_string())));

        // A gap ABOVE a legal prefix cuts only what is above it.
        let gapped = sets(json!({
            "1": ["torid_evo1_incarnon_form"],
            "3": ["torid_extended_volley"],
        }));
        assert_eq!(gapped, vec![vec!["torid_evo1_incarnon_form".to_string()]]);
    }

    /// THE SEARCH WEARS THE STANCE IT WAS HANDED, in every candidate.
    ///
    /// A stance decides what a swing IS, so a melee search without one ranks
    /// builds nobody holds: measured on a Praedos, the same four cards scored
    /// 0.65 kills bare and 0.95 wearing Sovereign Outcast. It is pinned rather
    /// than searched, so it belongs in `require` — and its SLOT is its own, so
    /// the ceiling has to rise with it or pinning it costs a main slot.
    #[test]
    fn a_melee_search_is_handed_the_stance_and_it_costs_no_main_slot() {
        let base = json!({
            "weapon": "praedos",
            "mods": { "primed_pressure_point": "fixed" },
            "build_size": 8,
        });
        let bare = super::parse_optimize(&base).expect("bare");
        assert!(!bare.constraints.require.iter().any(|m| m == "sovereign_outcast"));
        assert_eq!(bare.build_size, 8);
        assert_eq!(bare.cap, 60);

        let mut with = base.clone();
        with["stance"] = json!("sovereign_outcast");
        let p = super::parse_optimize(&with).expect("with a stance");
        assert!(
            p.constraints.require.iter().any(|m| m == "sovereign_outcast"),
            "{:?}",
            p.constraints.require
        );
        assert_eq!(p.build_size, 9, "the stance's slot is its own");
        // …AND IT HANDS CAPACITY BACK rather than taking it. Naramon on a slot
        // of another colour is 80% of the listed drain, rounded down: 4.
        assert_eq!(p.cap, 64);

        // A CARD THAT IS NOT A STANCE IS NOT ONE, said rather than accepted:
        // pinning an ordinary mod here would give it a ninth slot for free.
        let mut wrong = base.clone();
        wrong["stance"] = json!("primed_fury");
        assert!(super::parse_optimize(&wrong).is_err());
    }
}

/// INSTALLING THE INCARNON FORM TAKES THE CANNONADE OFF THE WEAPON.
///
/// "Weapons with an Incarnon mode must have Semi-Auto trigger type for both
/// firing modes in order to equip this mod" (wiki, Semi-Pistol_Cannonade), and
/// Dual Toxocyst transforms into a full-auto one. So the pool is a question
/// about the BUILD: with tier 1 unpicked the weapon is still pure semi-auto and
/// the mod fits; with it picked the weapon has two firing modes and it does not.
///
/// Both modules are pinned here, and that is the point — the optimizer obeys
/// the simulator's rule by CALLING it (`pool_for_build`), so the two cannot
/// answer differently about the same build.
#[cfg(test)]
mod equip_rule_tests {
    use super::*;
    use crate::meta::meta_json;
    
    use crate::simulate::simulate_json;

    const EVO1: &str = "dual_toxocyst_evo1_incarnon_form";
    const CANNON: &str = "semi_pistol_cannonade";

    fn sim(form: &str, evos: Value) -> Value {
        simulate_json(&json!({
            "weapon": "dual_toxocyst", "form": form, "mods": [CANNON], "arcane": "none",
            "evolutions": evos,
            "enemy": "thrax_centurion", "duration": 10.0, "runs": 2,
            "headshot_pct": 100.0, "seed": 7,
        }))
    }

    #[test]
    fn the_simulator_refuses_a_cannonade_beside_an_unlocked_incarnon_form() {
        // Nothing installed, base form: an ordinary build.
        let ok = sim("base", json!([]));
        assert_eq!(ok["ok"], json!(true), "{ok}");

        // Tier 1 installed: the weapon gained a full-auto firing mode.
        let bad = sim("base", json!([EVO1]));
        assert_eq!(bad["ok"], json!(false), "{bad}");
        let msg = bad["error"].as_str().unwrap_or_default();
        assert!(msg.contains("firing mode"), "the error says WHY: {msg}");

        // ...and ASKING FOR THE FORM is installing it (`parse_fight` implies the
        // unlock), so the cycle refuses it with no evolution named at all. This
        // is the case the page starts in on this weapon, and the alternative —
        // scoring the mod while firing a form it cannot be worn beside — is a
        // number nobody can reproduce.
        for form in ["incarnon", "incarnon_cycle"] {
            assert_eq!(sim(form, json!([]))["ok"], json!(false), "form {form}");
        }
    }

    /// Evolutions are a search DIMENSION, so the scope holds sets that can wear
    /// the mod and sets that cannot. Narrowing the pool to their intersection
    /// would throw away the builds the mod is FOR; leaving it alone would crown
    /// one the game refuses. It is decided per candidate instead.
    #[test]
    fn the_optimizer_forbids_the_pair_and_not_the_mod() {
        let plan = |evolutions: Value| {
            parse_optimize(&json!({
                "weapon": "dual_toxocyst",
                "build_size": 1,
                "mods": { CANNON: "search", "hornet_strike": "search" },
                "evolutions": evolutions,
            }))
            .expect("a plan")
        };
        let forbids = |p: &OptimizePlan, m: &str| -> Vec<bool> {
            let i = p.pool.iter().position(|x| x.id == m).expect("in scope");
            p.variant_forbids.iter().map(|f| f[i]).collect()
        };

        // Tier 1 unmarked: one variant, nothing installed, both mods legal.
        let bare = plan(json!({}));
        assert_eq!(bare.evo_sets.len(), 1);
        assert_eq!(forbids(&bare, CANNON), vec![false]);

        // Tier 1 marked: every set installs the form, so the Cannonade is out of
        // all of them — and `hornet_strike` is out of none, because this rule
        // excludes one mod and does not narrow the pool.
        let inc = plan(json!({ "1": [EVO1] }));
        assert!(inc.evo_sets.iter().all(|s| s.iter().any(|e| e == EVO1)));
        assert!(forbids(&inc, CANNON).iter().all(|&f| f), "the pair is illegal");
        assert!(forbids(&inc, "hornet_strike").iter().all(|&f| !f), "the pool is not");
    }

    /// EVERY SLOT AXIS SAYS HOW MANY OF ITS SLOTS TO FILL, and the empty
    /// choice is an option marked like any other.
    ///
    /// A single-slot axis has three answers and every one of them is now
    /// reachable — 0–0 (search it unfilled, candidates kept for later), 0–1
    /// (both), 1–1 (always filled, the derived answer and the old one). The
    /// exilus slot has been able to say all three since it was written; the
    /// arcane seats and the evolution tiers could say only 0–0 and 1–1, and
    /// which of those you got was decided by whether you had marked anything.
    ///
    /// THE DERIVED ANSWER IS UNTOUCHED, which is the half that must not
    /// regress: marking candidates and saying nothing else still means 1–1, so
    /// no existing scope grows. `an_arcane_seat_marked_none_is_not_a_default`
    /// is that assertion, and it is the 2026-08-01 decision restated.
    #[test]
    fn a_slot_axis_can_be_asked_to_search_itself_empty() {
        // The Laetum: one arcane seat, five evolution tiers.
        let plan = |arcanes: Value, evolutions: Value| {
            parse_optimize(&json!({
                "weapon": "laetum",
                "build_size": 1,
                "mods": { "hornet_strike": "search" },
                "arcanes": arcanes,
                "evolutions": evolutions,
            }))
            .expect("a plan")
        };
        let seat = |p: &OptimizePlan| -> Vec<String> {
            p.arcane_sets.iter().map(|s| s[0].clone()).collect()
        };

        // 1–1, the derived answer: a marked seat is a filled seat.
        let filled = plan(json!({ "secondary_deadhead": "search" }), json!({}));
        assert_eq!(seat(&filled), vec!["secondary_deadhead"]);

        // 0–1: the empty choice asked for by name, beside the candidate.
        let both = plan(
            json!({ "secondary_deadhead": "search", "none:secondary": "search" }),
            json!({}),
        );
        let mut got = seat(&both);
        got.sort();
        assert_eq!(got, vec!["none", "secondary_deadhead"]);

        // 0–0: pinned empty. The candidate stays marked and is not searched —
        // widening the range back must not cost the reader their marks.
        let unworn = plan(
            json!({ "secondary_deadhead": "search", "none:secondary": "fixed" }),
            json!({}),
        );
        assert_eq!(seat(&unworn), vec!["none"]);

        // AND THE SAME THREE ON AN EVOLUTION TIER, where the wire is an array
        // per tier and "none" is simply one of its entries.
        let one_tier = |ids: Value| {
            let p = plan(json!({}), json!({ "1": ids }));
            let mut n: Vec<usize> = p.evo_sets.iter().map(|s| s.len()).collect();
            n.sort();
            n
        };
        assert_eq!(one_tier(json!(["laetum_evo1_incarnon_form"])), vec![1], "1–1");
        assert_eq!(
            one_tier(json!(["laetum_evo1_incarnon_form", "none"])),
            vec![0, 1],
            "0–1"
        );
        assert_eq!(one_tier(json!(["none"])), vec![0], "0–0");
    }

    /// THE EMPTY MARK NAMES ITS SEAT, and this is the case that forces it.
    ///
    /// An Arch-Gun holds TWO arcanes — "Archguns possess two Arcane Enhancement
    /// slots to equip one Primary Arcane and one Secondary Arcane" — and the
    /// marks are ONE FLAT MAP, so a bare `none` could not say which seat a
    /// range was about. `none:<pool>` can, and widening one seat must leave the
    /// other exactly where it was.
    #[test]
    fn an_empty_arcane_mark_widens_only_the_seat_it_names() {
        let pools = wfsim_engine::data::weapons::arcane_pools("mausolon");
        assert_eq!(pools, vec!["primary", "secondary"], "an Arch-Gun seats two");
        let plan = parse_optimize(&json!({
            "weapon": "mausolon",
            "build_size": 1,
            "mods": {},
            "build_min": 0,
            "arcanes": {
                "primary_merciless": "search",
                "secondary_merciless": "search",
                "none:primary": "search",
            },
        }))
        .expect("a plan");
        // `arcane_sets[i]` is one entry per seat, in pool order.
        let primary: Vec<&str> = plan.arcane_sets.iter().map(|s| s[0].as_str()).collect();
        let secondary: Vec<&str> = plan.arcane_sets.iter().map(|s| s[1].as_str()).collect();
        assert!(primary.contains(&"none"), "the seat that was widened: {primary:?}");
        assert!(
            !secondary.contains(&"none"),
            "…and the one that was not: {secondary:?}"
        );
        // Two options on one seat and one on the other is two pairs, which is
        // also what the page's candidate count has to come to.
        assert_eq!(plan.arcane_sets.len(), 2);
    }

    /// …AND IT IS NEVER ADDED BY ITSELF. The 2026-08-01 decision, restated as
    /// an assertion: an arcane seat costs no capacity and no Forma, so an empty
    /// one can only ever tie the same build with the arcane in it. Marking a
    /// candidate is the statement that the seat should be filled, and widening
    /// to 0–1 has to be asked for.
    #[test]
    fn an_arcane_seat_marked_none_is_not_a_default() {
        let plan = parse_optimize(&json!({
            "weapon": "laetum",
            "build_size": 1,
            "mods": { "hornet_strike": "search" },
            "arcanes": { "secondary_deadhead": "search", "secondary_merciless": "search" },
        }))
        .expect("a plan");
        let seats: Vec<String> = plan.arcane_sets.iter().map(|s| s[0].clone()).collect();
        assert!(!seats.iter().any(|s| s == "none"), "{seats:?}");
        assert_eq!(seats.len(), 2, "two candidates, no hole beside them");
    }

    /// AN EMPTY SCOPE IS THE BARE WEAPON, and it is a legal search.
    ///
    /// Every other axis reads "nothing marked" as the EMPTY option — an
    /// unmarked exilus slot stays empty, an unmarked arcane seat searches no
    /// arcane — and the mods axis alone answered it with "no legal builds in
    /// this scope", because `build_min` was clamped to 1.
    ///
    /// The other half is the one that must not regress: 0 is now the DEFAULT,
    /// so this also asserts that the derived floor still wins the moment
    /// anything is marked. Without that, pooling would stop meaning "use these"
    /// and every search would gain a candidate nobody asked for.
    #[test]
    fn an_empty_scope_searches_the_bare_weapon() {
        let plan = |mods: Value, min: Value| {
            let mut req = json!({ "weapon": "braton_prime", "build_size": 8, "mods": mods });
            if !min.is_null() {
                req["build_min"] = min;
            }
            parse_optimize(&req)
        };
        // Nothing marked, no `build_min` at all: the request an untouched
        // optimizer tab sends, and it is a legal scope.
        let bare = plan(json!({}), Value::Null).expect("an empty scope is a scope");
        assert_eq!(bare.min_slots, 0, "no floor of anyone's");
        // …and it is genuinely ONE candidate rather than none.
        let space = wfsim_optimizer::space::SubsetSpace::new(&[], &[], &[], bare.min_slots, 8);
        assert_eq!(space.len(), 1, "the bare weapon");

        // THE DERIVED FLOOR STILL WINS. One required and one pooled means every
        // candidate carries both, so the floor is 2 however low the box is set.
        let marked = plan(
            json!({ "serration": "fixed", "split_chamber": "search" }),
            json!(0),
        )
        .expect("a plan");
        assert_eq!(marked.min_slots, 2, "1 required + at least 1 pooled");

        // …and a floor ABOVE the derived one is still the reader's to raise.
        let full = plan(json!({ "serration": "search" }), json!(8)).expect("a plan");
        assert_eq!(full.min_slots, 8, "exactly 8 mods");
    }

    /// A CEILING OF 0 IS "SEARCH IT EMPTY, AND KEEP THE MARKS" — the same thing
    /// 0–0 means on every other axis.
    ///
    /// It has to outrank the DERIVED floor, which is the sharp part: the marks
    /// say "use these" and a 0 ceiling says "not this time", and without a rule
    /// the two contradict — `SubsetSpace::new(1, 0)` enumerates nothing, so a
    /// legal request would come back as "no legal builds in this scope".
    #[test]
    fn a_ceiling_of_zero_searches_the_bare_weapon_with_the_marks_kept() {
        let plan = parse_optimize(&json!({
            "weapon": "braton_prime",
            "build_size": 0,
            "mods": { "serration": "search", "split_chamber": "search" },
        }))
        .expect("0 mods per build is a legal scope");
        assert_eq!(plan.min_slots, 0, "the ceiling outranks the derived floor");
        assert_eq!(plan.build_size, 0);
        // THE MARKS ARE KEPT — raising the ceiling back must cost nothing, so
        // they are still in the pool the search would draw from.
        let ids: Vec<&str> = plan.pool.iter().map(|m| m.id).collect();
        assert!(ids.contains(&"serration") && ids.contains(&"split_chamber"), "{ids:?}");
        // …and it really is one candidate rather than none.
        let space =
            wfsim_optimizer::space::SubsetSpace::new(&[], &[], &[], plan.min_slots, plan.build_size);
        assert_eq!(space.len(), 1, "the bare weapon");
    }

    /// A mod the scope REQUIRES and no variant can equip is a contradiction: the
    /// search cannot answer it by choosing differently, so it is refused up
    /// front rather than answered with an empty leaderboard.
    #[test]
    fn a_required_mod_no_variant_can_wear_is_refused() {
        let r = parse_optimize(&json!({
            "weapon": "dual_toxocyst",
            "build_size": 2,
            "mods": { CANNON: "fixed", "hornet_strike": "search" },
            "evolutions": { "1": [EVO1] },
        }));
        let e = match r {
            Err(e) => e,
            Ok(_) => panic!("a required mod no variant can wear is a contradiction"),
        };
        let msg = e["error"].as_str().unwrap_or_default();
        assert!(msg.contains("firing mode"), "{msg}");
    }

    /// The client is told the CONSEQUENCE, not the rule — the last time it
    /// re-derived a pool rule in JS the copy went stale within the week.
    #[test]
    fn meta_states_what_each_evolution_costs() {
        let meta = meta_json();
        let w = meta["weapons"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["id"] == json!("dual_toxocyst"))
            .expect("dual toxocyst");
        assert_eq!(w["evo_forbids"][EVO1], json!([CANNON]));
        // A stat evolution costs nothing, and says so by being absent.
        assert!(w["evo_forbids"]["dual_toxocyst_carnage_reign"].is_null());
        // The form the cost belongs to is flagged, so the Form control can grey
        // it out instead of letting the run be refused after the fact.
        let forms = w["forms"].as_array().unwrap();
        assert_eq!(forms.iter().filter(|f| f["gauge_switched"] == json!(true)).count(), 1);
    }

    /// A PASSIVE THAT BELONGS TO A FORM still reaches the roster row.
    ///
    /// The Phenmor's Incarnon fire rate spools down to 60% and the sentence is
    /// declared on `phenmor_incarnon`, which is not a roster row — so reading
    /// the base entry's passives alone published nothing about the one thing
    /// that makes the printed 13.33 rounds/s wrong.
    #[test]
    fn a_forms_passive_reaches_the_weapon_it_belongs_to() {
        let meta = meta_json();
        let w = meta["weapons"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["id"] == json!("phenmor"))
            .expect("phenmor");
        let lines: Vec<&str> =
            w["passives"].as_array().unwrap().iter().filter_map(|x| x.as_str()).collect();
        assert!(
            lines.iter().any(|l| l.contains("FALLS while the trigger is held")),
            "{lines:?}"
        );
        // …and each line ONCE: the union walks every form of the group.
        let mut sorted = lines.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), lines.len(), "{lines:?}");
    }
}

/// A CANDIDATE'S LOCK IS NOT A FACT ABOUT THE OTHER CANDIDATES.
///
/// `fetchAllBuffs` asks "what buffs could this weapon ever produce" by marking
/// the whole pool `search`. Under the old rule the enumeration then applied
/// every marked mod's `disables:` at once — a build of eighty mods, which
/// nobody assembles — so Primary Acuity's `disables: [multishot]` deleted
/// Galvanized Chamber's on-kill multishot from the list of every rifle whose
/// pool contains Acuity. The simulator showed that buff and the optimizer's
/// read-only copy of the same fight did not; `check_preset_independence` had
/// been failing on it.
///
/// Only a REQUIRED mod is in every build, so only a required mod may lock.
#[cfg(test)]
mod scope_lock_tests {
    use super::*;

    fn buff_ids(mods: Value) -> Vec<String> {
        let out = opt_buffs_json(&json!({ "weapon": "torid", "mods": mods }));
        out["buffs"]
            .as_array()
            .expect("buffs")
            .iter()
            .map(|b| b["id"].as_str().unwrap_or("").to_string())
            .collect()
    }

    #[test]
    fn a_searched_mod_does_not_lock_another_mods_buff() {
        let alone = buff_ids(json!({ "galvanized_chamber": "search" }));
        assert!(alone.iter().any(|i| i == "on_kill_multishot"), "{alone:?}");
        // Acuity disables multishot, but as a CANDIDATE it is only in the
        // builds that take it — and those are not the builds this card is for.
        let with = buff_ids(json!({ "galvanized_chamber": "search", "primary_acuity": "search" }));
        assert!(
            with.iter().any(|i| i == "on_kill_multishot"),
            "a candidate's lock deleted another candidate's buff: {with:?}"
        );
    }

    #[test]
    fn a_required_mod_still_locks() {
        // Required means every searched build carries it, so the buff genuinely
        // cannot arm and the card would be a lie.
        let with = buff_ids(json!({ "galvanized_chamber": "search", "primary_acuity": "fixed" }));
        assert!(
            !with.iter().any(|i| i == "on_kill_multishot"),
            "a required lock must still suppress: {with:?}"
        );
    }
}

#[cfg(test)]
mod lower_ranks {
    use super::*;
    use crate::panel::panel_json;

    /// Every source value a panel states for `key`, wherever the panel puts it.
    fn finals(v: &Value, key: &str, out: &mut Vec<String>) {
        match v {
            Value::Object(o) => {
                if o.get("key").and_then(Value::as_str) == Some(key) {
                    let sources = o.get("sources").and_then(Value::as_array).into_iter().flatten();
                    out.extend(sources.filter_map(|x| x["value"].as_str().map(String::from)));
                }
                o.values().for_each(|x| finals(x, key, out));
            }
            Value::Array(a) => a.iter().for_each(|x| finals(x, key, out)),
            _ => {}
        }
    }

    /// A BUILD NAMING A LOWER RANK IS FOUGHT AT IT: Hunter Track is +15% at
    /// rank 0 and +90% at rank 5.
    #[test]
    fn a_ranked_id_reaches_the_resolved_stats() {
        let duration = |id: &str| {
            let mut out = Vec::new();
            let panel = panel_json(&json!({ "weapon": "burston_prime", "mods": [id] }));
            finals(&panel, "status_duration", &mut out);
            out
        };
        assert_eq!(duration("hunter_track"), vec!["+90%"]);
        assert_eq!(duration("hunter_track@0"), vec!["+15%"]);
        let refused = panel_json(&json!({ "weapon": "burston_prime", "mods": ["hunter_track@9"] }));
        assert!(refused["error"].as_str().is_some_and(|e| e.contains("rank 9")), "{refused}");
    }

    /// A SCOPE NAMING THREE RANKS OF ONE CARD searches each, and never two of
    /// them in one build: the variants share the card's family.
    #[test]
    fn a_scope_searches_each_rank_and_never_two_at_once() {
        let plan = parse_optimize(&json!({
            "weapon": "burston_prime",
            "build_size": 2,
            "mods": { "hunter_track": "search", "hunter_track@0": "search", "hunter_track@2": "search" },
        }))
        .expect("a plan");
        let fams: Vec<Option<&str>> = plan.pool.iter().map(|m| m.family).collect();
        assert_eq!(plan.pool.len(), 3);
        assert!(fams.iter().all(|f| *f == Some("hunter_track")), "{fams:?}");
    }

    /// AN ARCANE MARK MAY NAME A RANK; max rank is the bare id.
    #[test]
    fn an_arcane_mark_names_its_rank() {
        let id = wfsim_engine::data::arcanes::slot_pool("primary")
            .iter()
            .find(|a| a.max_rank > 1)
            .map(|a| a.id.clone())
            .expect("a ranked primary arcane");
        let max = arcane_at_rank("primary", &id).map(|x| x.1).expect("the bare id");
        assert_eq!(arcane_at_rank("primary", &format!("{id}@1")).map(|x| x.1), Some(1));
        assert!(arcane_at_rank("primary", &format!("{id}@{max}")).is_none());
    }
}
