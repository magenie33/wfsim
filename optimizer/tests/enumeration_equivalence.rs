//! THE INDEX SPACE IS THE WALK.
//!
//! `SubsetSpace` exists to replace the depth-first enumeration, and the only
//! thing that licenses the replacement is that a full sweep of it produces
//! exactly the subsets the walk produces — on a real pool, with real families
//! and real capacity legalization, not on a hand-made fixture.
//!
//! Both directions matter and they fail differently. A subset the walk finds
//! and the sweep misses is a build the new search can never return. A subset
//! the sweep finds and the walk misses is a build the old search could never
//! return — which would be interesting, but must not be silently true.

use std::collections::BTreeSet;

use wfsim_engine::loadout::{ModDef, StackPolicy, WeaponBase};
use wfsim_engine::mods::{plan_forma, PlannedMod};
use wfsim_optimizer::space::SubsetSpace;
use wfsim_optimizer::{enumerate_candidates_observed, Constraints};

/// Twelve rifle mods with real families in them (the two Cryo Rounds tiers,
/// the banes) and a real capacity cost, so both the family rejection and the
/// Forma legalization are exercised rather than assumed away.
const SCOPE: &[&str] = &[
    "serration",
    "split_chamber",
    "point_strike",
    "vital_sense",
    "hammer_shot",
    "cryo_rounds",
    "primed_cryo_rounds",
    "infected_clip",
    "hellfire",
    "stormbringer",
    "bane_of_grineer",
    "primed_bane_of_grineer",
];

fn pool() -> Vec<ModDef> {
    let p: Vec<ModDef> = wfsim_engine::mods_data::pool_for_weapon("verglas_prime")
        .into_iter()
        .filter(|m| SCOPE.contains(&m.id))
        .collect();
    assert_eq!(p.len(), SCOPE.len(), "the fixture scope must all be equippable");
    p
}

fn sorted(v: &[usize]) -> Vec<usize> {
    let mut v = v.to_vec();
    v.sort_unstable();
    v
}

fn run(required: &[&str], min: u32, max: u32) {
    let pool = pool();
    let base = WeaponBase::from_data("verglas_prime", true, &[]);
    let innate = wfsim_engine::weapons_data::innate_slots("verglas_prime");
    const CAP: u32 = 60;
    let constraints = Constraints {
        require: required.iter().map(|s| s.to_string()).collect(),
        forbid: Vec::new(),
    };

    // ---- what the production walk emits ----
    let (cands, _stats, complete) = enumerate_candidates_observed(
        &pool,
        &base,
        None,
        0,
        min,
        max,
        CAP,
        &innate,
        &constraints,
        &[None],
        None,
        0,
        wfsim_engine::tenno_data::default_tenno(),
        StackPolicy::Emergent,
    );
    assert!(complete, "the fixture must be walkable to the end");
    let walked: BTreeSet<Vec<usize>> = cands.iter().map(|c| sorted(&c.ordered)).collect();

    // ---- what a full sweep of the index space produces ----
    let families: Vec<Option<&'static str>> = pool.iter().map(|m| m.family).collect();
    let usable: Vec<usize> = (0..pool.len()).collect();
    let req_ix: Vec<usize> = required
        .iter()
        .map(|r| pool.iter().position(|m| m.id == *r).expect("required mod in scope"))
        .collect();
    let space = SubsetSpace::new(&families, &usable, &req_ix, min as usize, max as usize);
    let mut swept: BTreeSet<Vec<usize>> = BTreeSet::new();
    let mut buf = Vec::new();
    for i in 0..space.len() {
        if !space.nth(i, &mut buf) {
            continue; // family collision — the walk prunes these too
        }
        // The walk also drops what cannot be legalized under the capacity cap;
        // apply the SAME filter, through the same function, so the comparison
        // is about enumeration and not about Forma.
        let planned: Vec<PlannedMod> = buf
            .iter()
            .map(|&i| PlannedMod { base_drain: pool[i].base_drain, polarity: pool[i].polarity })
            .collect();
        if plan_forma(CAP, &innate, &planned).is_err() {
            continue;
        }
        swept.insert(sorted(&buf));
    }

    let missing: Vec<&Vec<usize>> = walked.difference(&swept).collect();
    let extra: Vec<&Vec<usize>> = swept.difference(&walked).collect();
    let name = |v: &Vec<usize>| v.iter().map(|&i| pool[i].id).collect::<Vec<_>>().join("+");
    assert!(
        missing.is_empty(),
        "the sweep MISSES {} subsets the walk finds — builds the new search could never return, e.g. {}",
        missing.len(),
        missing.iter().take(3).map(|v| name(v)).collect::<Vec<_>>().join(", ")
    );
    assert!(
        extra.is_empty(),
        "the sweep finds {} subsets the walk never did, e.g. {}",
        extra.len(),
        extra.iter().take(3).map(|v| name(v)).collect::<Vec<_>>().join(", ")
    );
    assert!(!swept.is_empty(), "the fixture enumerated nothing");
    println!("  {min}..={max} required {required:?}: {} subsets, walk == sweep", swept.len());
}

#[test]
fn a_full_sweep_of_the_index_space_is_exactly_the_walk() {
    run(&[], 1, 4);
    run(&[], 8, 8);
    run(&[], 3, 6);
}

#[test]
fn required_mods_do_not_change_the_agreement() {
    run(&["serration"], 2, 5);
    run(&["serration", "split_chamber"], 4, 6);
    // A required mod whose FAMILY also has a pooled member: every subset that
    // would pair them is illegal, and both sides must drop the same ones.
    run(&["cryo_rounds"], 2, 4);
}
