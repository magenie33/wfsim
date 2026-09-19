// SPDX-License-Identifier: AGPL-3.0-or-later
//! DATA — the catalogs: `data/` read into the vocabulary (`crate::model`),
//! one module per family, and the compile-time-embedded tree they read from
//! (docs/WASM.md phase 1).
//!
//! `build.rs` scans `../data` and generates the `FILES` table; every loader
//! (mods, arcanes, evolutions, enemies) reads from it. Native binaries, the
//! CLI, and the wasm build therefore all carry the identical data set, and
//! nothing depends on the current working directory or a filesystem — the
//! browser has neither.
//!
//! Paths are relative to `data/` and use forward slashes on every platform:
//! `"mods/pistol/hornet_strike.yaml"`.

pub mod abilities;
pub mod arcanes;
pub mod auras;
pub mod boards;
pub mod buff_events;
pub mod enemies;
pub mod evolutions;
pub mod factions;
pub mod i18n;
pub mod market;
pub mod mod_sets;
pub mod mods;
pub mod rage;
pub mod shards;
pub mod share_order;
pub mod syndicates;
pub mod tenno;
pub mod warframes;
pub mod weapons;

include!(concat!(env!("OUT_DIR"), "/embedded_data.rs"));

/// The embedded file at `path` (relative to `data/`, forward slashes).
pub fn file(path: &str) -> Option<&'static str> {
    FILES.iter().find(|(p, _)| *p == path).map(|(_, c)| *c)
}

/// All embedded files under `prefix` (e.g. `"mods/pistol/"`), in path order.
pub fn files_under(prefix: &str) -> impl Iterator<Item = (&'static str, &'static str)> + '_ {
    FILES.iter().copied().filter(move |(p, _)| p.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_tree_covers_every_data_family() {
        for prefix in ["mods/pistol/", "arcanes/secondary/", "evolutions/", "enemies/", "perks/", "weapons/", "i18n/", "tenno/", "benchmarks/"] {
            assert!(files_under(prefix).count() > 0, "no embedded files under {prefix}");
        }
        assert!(file("assets.yaml").is_some());
        assert!(file("enemies/thrax_centurion.yaml").is_some());
        assert!(file("no/such/file.yaml").is_none());
    }
}
