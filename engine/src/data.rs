//! The compile-time-embedded `data/` tree (docs/WASM.md phase 1).
//!
//! `build.rs` scans `../data` and generates the `FILES` table; every loader
//! (mods, arcanes, evolutions, enemies) reads from it. Native binaries, the
//! CLI, and the wasm build therefore all carry the identical data set, and
//! nothing depends on the current working directory or a filesystem — the
//! browser has neither.
//!
//! Paths are relative to `data/` and use forward slashes on every platform:
//! `"mods/pistol/hornet_strike.yaml"`.

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
        for prefix in ["mods/pistol/", "arcanes/secondary/", "evolutions/", "enemies/", "perks/", "weapons/", "i18n/", "tenno/"] {
            assert!(files_under(prefix).count() > 0, "no embedded files under {prefix}");
        }
        assert!(file("assets.yaml").is_some());
        assert!(file("enemies/thrax_centurion.yaml").is_some());
        assert!(file("no/such/file.yaml").is_none());
    }
}
