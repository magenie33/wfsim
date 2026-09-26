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
pub mod apl;
pub mod arcanes;
pub mod auras;
pub mod casting;
pub mod boards;
pub mod companions;
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
                || ["sliding_or_aim_gliding", "buffing_ally_warframes", "target_has_10_radiation_stacks"].contains(&w)
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
