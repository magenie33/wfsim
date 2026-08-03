//! OFFICIAL TEST SCENARIOS — `data/benchmarks/`.
//!
//! A benchmark is a scenario that belongs to no weapon. Every other scenario in
//! the app is a PRESET: per weapon, editable, stored in the browser. This one is
//! read-only, defined once, and appears on every weapon — which is the whole
//! point, because a number only means something against a ruler someone else
//! can pick up.
//!
//! # The rule that makes it scale
//!
//! **A benchmark may not name a weapon, a mod, an arcane, an evolution, or a
//! weapon form.** [`no_benchmark_names_a_weapon`] enforces it, and the
//! enforcement is the feature: a definition that never speaks about a member of
//! the roster cannot be wrong about a member it has never seen. Adding weapon
//! 201 can therefore never require editing a benchmark.
//!
//! It is a stronger guarantee than tagging weapons would give. A tag scheme is
//! correct only while every weapon is tagged correctly — one missed tag on
//! weapon 201 is a silently wrong board. "Mentions no weapon at all" has no
//! per-weapon step to get wrong.
//!
//! What replaces the per-weapon settings is POLICY, which the engine already
//! resolves: `form: default` is the Incarnon cycle where a weapon has one and
//! its arsenal default where it does not; an omitted `headshot_pct` is 0 for a
//! sentinel weapon and 100 otherwise; omitted `buffs` means each buff opens at
//! the start state docs/BUFFS.md gives it, which is a per-BUFF property and so
//! identical on every weapon.
//!
//! # Versioning
//!
//! `id` is stable across rewordings and `version` moves with the DEFINITION, so
//! "an updated benchmark voids the old board" is mechanical: a board row stores
//! the benchmark id it was measured under, and rows under a retired id are
//! void. Because a submission stores the BUILD and never a score, a new version
//! does not ask anyone to resubmit — every stored build is simply re-scored.

use std::sync::OnceLock;

use serde::Deserialize;

/// One official scenario, as `data/benchmarks/*.yaml` states it.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Benchmark {
    /// Stable slug. Survives a reworded `name`; changes only when the
    /// definition does (and then it is a new file, not an edit).
    pub id: String,
    /// The display name, which states the whole definition — see the yaml.
    /// Localized through the ordinary i18n overlay, never in this file.
    pub name: String,
    /// Bumped when any scenario field changes. Boards measured under an older
    /// version are void.
    pub version: u32,
    /// The fight, as the wire scenario the web api already parses. Kept as a
    /// free-form map ON PURPOSE: a benchmark is defined in the SAME vocabulary
    /// a scenario preset uses, so a field added to scenarios needs no second
    /// definition here, and the "names no weapon" check below still covers it.
    pub scenario: serde_norway::Value,
}

/// Every official benchmark, parsed once, in path order.
pub fn all() -> &'static [Benchmark] {
    static B: OnceLock<Vec<Benchmark>> = OnceLock::new();
    B.get_or_init(|| {
        crate::data::files_under("benchmarks/")
            .filter(|(p, _)| p.ends_with(".yaml"))
            .map(|(p, text)| {
                serde_norway::from_str::<Benchmark>(text).unwrap_or_else(|e| panic!("{p}: {e}"))
            })
            .collect()
    })
}

/// The benchmark with this id.
pub fn get(id: &str) -> Option<&'static Benchmark> {
    all().iter().find(|b| b.id == id)
}

// The two halves of the rule below live under `cfg(test)`: the check is a DATA
// discipline enforced in CI, the same way perk-id uniqueness and the mod-value
// sweep are, and neither is something a running engine re-derives.
/// Every string a benchmark's `scenario` map holds, keys included.
///
/// Keys count: a scenario field named after a weapon would be as wrong as a
/// value, and the map is free-form precisely so new fields need no code here.
#[cfg(test)]
fn strings_in(v: &serde_norway::Value, out: &mut Vec<String>) {
    match v {
        serde_norway::Value::String(s) => out.push(s.clone()),
        serde_norway::Value::Sequence(xs) => xs.iter().for_each(|x| strings_in(x, out)),
        serde_norway::Value::Mapping(m) => {
            for (k, val) in m {
                if let serde_norway::Value::String(s) = k {
                    out.push(s.clone());
                }
                strings_in(val, out);
            }
        }
        _ => {}
    }
}

/// Ids a benchmark may never contain: everything that belongs to ONE weapon.
///
/// Enemies are deliberately absent — an enemy is a property of the fight, which
/// is exactly what a benchmark is allowed to choose.
#[cfg(test)]
fn weapon_bound_ids() -> Vec<String> {
    let mut ids: Vec<String> = crate::weapons_data::all()
        .iter()
        .map(|w| w.id.clone())
        .collect();
    ids.extend(crate::evolutions_data::pool().iter().map(|e| e.id.clone()));
    for class in crate::mods_data::classes() {
        ids.extend(
            crate::mods_data::pool_union(&[class.to_string()])
                .into_iter()
                .map(|m| m.id.to_string()),
        );
    }
    for slot in crate::arcanes_data::slots() {
        ids.extend(
            crate::arcanes_data::slot_pool(slot)
                .iter()
                .map(|a| a.id.to_string()),
        );
    }
    // Named FORMS. `default` is the policy and is the only legal value: it is
    // what resolves per weapon. `base` / `incarnon` / `charged` name a form
    // some weapons have and others do not, so a benchmark asking for one gets
    // a silent per-weapon fallback — one name, two different tests.
    ids.extend(
        crate::weapons_data::all()
            .iter()
            .map(|w| w.form.clone())
            .filter(|f| f != "default"),
    );
    ids.sort();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    /// THE RULE, enforced. A benchmark that names a weapon — or a mod, an
    /// arcane, an evolution, or a form only some weapons register — is a ruler
    /// that measures different things on different weapons, and it fails the
    /// build rather than the board.
    #[test]
    fn no_benchmark_names_a_weapon() {
        let banned = weapon_bound_ids();
        assert!(!banned.is_empty(), "the ban list itself has to load");
        for b in all() {
            let mut seen = Vec::new();
            strings_in(&b.scenario, &mut seen);
            for s in seen {
                assert!(
                    !banned.contains(&s),
                    "benchmark {} names '{s}', which belongs to one weapon — a benchmark \
                     must hold for a roster it has never seen (see this module's header)",
                    b.id
                );
            }
        }
    }

    /// The official ruler, pinned. Every field here is a term of a public
    /// claim, so a change to one is a change to what every published number
    /// MEANS — it belongs in a new version, and this test is where that gets
    /// noticed.
    #[test]
    fn the_official_single_target_benchmark_is_what_we_published() {
        let b = get("single_target_v1").expect("data/benchmarks/single_target_v1.yaml");
        assert_eq!(b.version, 1);
        assert_eq!(b.name, "Single Target · Thrax Centurion Lv 9999 SP · 300 s · KPM");
        let s = |k: &str| b.scenario.get(k).cloned();
        assert_eq!(s("enemy").and_then(|v| v.as_str().map(String::from)).as_deref(), Some("thrax_centurion"));
        assert_eq!(s("level").and_then(|v| v.as_u64()), Some(9999));
        assert_eq!(s("steel_path").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(s("duration").and_then(|v| v.as_u64()), Some(300));
        assert_eq!(s("runs").and_then(|v| v.as_u64()), Some(100));
        assert_eq!(s("metric").and_then(|v| v.as_str().map(String::from)).as_deref(), Some("kpm"));
        assert_eq!(s("form").and_then(|v| v.as_str().map(String::from)).as_deref(), Some("default"));

        // OMITTED ON PURPOSE, and the omission is the policy — see the yaml.
        // Pinning either one here would put a sentinel weapon on the board at a
        // headshot rate it cannot reach, or assert buff stacks the fight never
        // handed out.
        assert!(s("headshot_pct").is_none(), "resolved per weapon, not pinned");
        assert!(s("buffs").is_none(), "each buff opens where docs/BUFFS.md says");
    }
}
