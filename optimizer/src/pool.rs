// SPDX-License-Identifier: AGPL-3.0-or-later
//! The searchable mod pool, and the mods pruned from it before the search.

use wfsim_engine::model::ModDef;

/// The searchable mod pool of one CLASS at MAX RANK (drain = base +
/// max_rank), from `data/mods/<class>/*.yaml`. Exilus (utility) mods have no
/// damage model — enumerating them only multiplies the search space, so the
/// optimizer's pool excludes them; the exilus SLOT is its own dimension.
pub fn class_pool(class: &str) -> Vec<ModDef> {
    wfsim_engine::data::mods::class_pool(class)
        .into_iter()
        .filter(|m| !m.exilus)
        .collect()
}

/// The pistol pool — the historical default, kept for the CLI and tests.
pub fn pool() -> Vec<ModDef> {
    class_pool("pistol")
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
}
