// SPDX-License-Identifier: AGPL-3.0-or-later
//! DATA — the catalogs: `data/` read into the vocabulary (`crate::model`),
//! one module per family, and the compile-time-embedded tree they read from
//! (docs/WASM.md phase 1).
//!
//! `build.rs` scans `../data` and generates the `FILES` table; every loader
//! (mods, arcanes, evolutions, enemies) reads it through `file` and
//! `files_under`. Native binaries and the CLI embed every file; the wasm build
//! embeds none and is handed the same lean files as packs (`install`), so
//! nothing depends on the current working directory or a filesystem — the
//! browser has neither.
//!
//! Paths are relative to `data/` and use forward slashes on every platform:
//! `"mods/pistol/hornet_strike.yaml"`.

pub mod abilities;
pub mod apl;
pub mod arcanes;
pub mod auras;
pub mod casting;
pub mod catalogue;
pub mod concepts;
pub mod boards;
pub mod companions;
pub mod buff_events;
pub mod enemies;
pub mod equipment_slots;
pub mod evolutions;
pub mod factions;
pub mod i18n;
pub mod market;
pub mod melee;
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

/// A CATALOG READ BY KEY, filled on first ask and kept for the process.
///
/// A hit shares the lock and allocates nothing, so every core reads it at
/// once; a build's door asks several of these per record, and behind a mutex
/// the cores queued on them. A miss builds outside the lock — two racing
/// misses build twice and the first kept wins, which costs one leak.
pub(crate) struct Memo<T: ?Sized + 'static>(
    std::sync::OnceLock<std::sync::RwLock<std::collections::HashMap<String, &'static T>>>,
);

impl<T: ?Sized + Sync + 'static> Memo<T> {
    pub(crate) const fn new() -> Self {
        Memo(std::sync::OnceLock::new())
    }

    pub(crate) fn get_or(&self, key: &str, make: impl FnOnce() -> &'static T) -> &'static T {
        let map = self.0.get_or_init(Default::default);
        if let Some(v) = map.read().unwrap_or_else(|e| e.into_inner()).get(key) {
            return v;
        }
        let v = make();
        map.write().unwrap_or_else(|e| e.into_inner()).entry(key.to_string()).or_insert(v)
    }
}

type Files = Vec<(&'static str, &'static str)>;

/// Packs handed in by the host (`install`), until the first read seals them.
static INSTALLED: std::sync::Mutex<(Files, Vec<String>)> = std::sync::Mutex::new((Vec::new(), Vec::new()));
static ALL: std::sync::OnceLock<Files> = std::sync::OnceLock::new();

/// THE FILES THIS BUILD READS, in path order: each installed pack's, and the
/// embedded files of every pack not installed. Sealed by the first read, so
/// every catalog is cached from one set.
fn all() -> &'static [(&'static str, &'static str)] {
    ALL.get_or_init(|| {
        let (installed, names) = INSTALLED.lock().map(|g| g.clone()).unwrap_or_default();
        let mut out: Files = FILES
            .iter()
            .filter(|(_, pack, _)| !names.iter().any(|n| n == pack))
            .map(|(p, _, c)| (*p, *c))
            .collect();
        out.extend(installed);
        out.sort_by(|a, b| a.0.cmp(b.0));
        out
    })
}

/// HAND THIS BUILD A PACK (`build.rs` `pack_of`): `name` and its files as
/// (path, lean contents). It REPLACES whatever this build embeds of that pack,
/// so one host installs on any build; a build made apart reads nothing before
/// every pack in `EXPECTED_PACKS` is here. After the first read a pack would be
/// invisible to every cached catalog, so it is refused.
pub fn install(name: &str, files: Vec<(String, String)>) -> Result<usize, String> {
    if ALL.get().is_some() {
        return Err(format!("pack {name}: the data was read before it was installed"));
    }
    let mut g = INSTALLED.lock().map_err(|_| "pack registry poisoned".to_string())?;
    if g.1.iter().any(|n| n == name) {
        return Err(format!("pack {name} is already installed"));
    }
    let n = files.len();
    let mut add: Files = Vec::with_capacity(n);
    for (p, c) in files {
        if g.0.iter().any(|(q, _)| *q == p) {
            return Err(format!("pack {name}: {p} is already in another pack"));
        }
        add.push((Box::leak(p.into_boxed_str()), Box::leak(c.into_boxed_str())));
    }
    g.0.extend(add);
    g.1.push(name.to_string());
    Ok(n)
}

/// The packs this build still waits for; empty once it can answer.
pub fn missing_packs() -> Vec<&'static str> {
    let g = INSTALLED.lock();
    EXPECTED_PACKS
        .iter()
        .copied()
        .filter(|p| !g.as_ref().is_ok_and(|g| g.1.iter().any(|n| n == p)))
        .collect()
}

/// The file at `path` (relative to `data/`, forward slashes).
pub fn file(path: &str) -> Option<&'static str> {
    all().iter().find(|(p, _)| *p == path).map(|(_, c)| *c)
}

/// All files under `prefix` (e.g. `"mods/pistol/"`), in path order.
pub fn files_under(prefix: &str) -> impl Iterator<Item = (&'static str, &'static str)> + '_ {
    all().iter().copied().filter(move |(p, _)| p.starts_with(prefix)).inspect(|_| crate::alive::beat())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_norway::Value;

    /// ONE WORD, ONE MEANING, ACROSS EVERY CATALOG. A mod, an evolution and an
    /// arcane that name the same trigger, grant or condition spell it the same
    /// way, and the spelling is the one `crate::model` parses — so a word from
    /// before the vocabulary was unified (`on_kill`, `while_aiming`, a
    /// `base_damage` that meant a flat add) cannot come back unnoticed.
    #[test]
    fn every_word_in_the_catalogs_is_the_vocabulary() {
        use crate::model::{ArcTrigger, BuffGrant, BuffTrigger, IndirectStat, TennoCondition, UNSIMULATED_EVENTS};
        let trigger = |w: &str| {
            BuffTrigger::from_id(w).is_some() || ArcTrigger::from_id(w).is_some() || UNSIMULATED_EVENTS.contains(&w)
        };
        // A grant is a buff's bracket, an indirect stat, or one of the few
        // payloads a card pays that is neither (and says so where it is read).
        let grant = |w: &str| {
            BuffGrant::from_id(w).is_some()
                || IndirectStat::from_id(w).is_some()
                || [
                    "status_damage", "condition_overload", "magazine_refill", "crit_and_status", "toxin_damage",
                    "element_and_status_chance",
                    "final_damage", "ammo_efficiency", "weakpoint_crit_chance", "flat_base_magazine",
                ]
                .contains(&w)
        };
        let condition = |w: &str| {
            TennoCondition::from_id(w).is_some()
                || w.starts_with("fire_rate_below_")
                || ["buffing_ally_warframes", "target_has_10_radiation_stacks"].contains(&w)
                || BuffTrigger::from_id(w).is_some()
                || ArcTrigger::from_id(w).is_some()
        };
        let decay = |w: &str| ["lose_one_and_reset", "per_stack_expiry", "all_at_once"].contains(&w);
        let cleared_by = |w: &str| ["reload", "magazine_refilled", "empty_magazine", "partial_reload"].contains(&w);
        let mut bad = Vec::new();
        for family in ["mods/", "evolutions/", "arcanes/", "warframe_mods/", "warframe_arcanes/", "perks/"] {
            for (path, text) in files_under(family) {
                let Ok(doc) = serde_norway::from_str::<Value>(text) else { continue };
                let mut stack = vec![&doc];
                while let Some(v) = stack.pop() {
                    match v {
                        Value::Mapping(m) => {
                            for (k, x) in m {
                                let (Some(k), Some(w)) = (k.as_str(), x.as_str()) else {
                                    stack.push(x);
                                    continue;
                                };
                                let ok = match k {
                                    "trigger" => trigger(w),
                                    "grants" => grant(w),
                                    "condition" => condition(w),
                                    "decay" => decay(w),
                                    "cleared_by" => cleared_by(w),
                                    _ => true,
                                };
                                if !ok {
                                    bad.push(format!("{path}: {k}: {w}"));
                                }
                            }
                        }
                        Value::Sequence(s) => stack.extend(s.iter()),
                        _ => {}
                    }
                }
            }
        }
        assert!(bad.is_empty(), "words outside the vocabulary:\n{}", bad.join("\n"));
    }

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
