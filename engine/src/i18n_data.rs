//! Display-name i18n overlays: `data/i18n/<locale>.yaml` → id→name maps.
//!
//! English is not a locale here — it is the source of truth living on each
//! entity's `name` field. Overlay files exist only for other languages, may
//! be arbitrarily incomplete (missing entries fall back to English in the
//! UI), and never touch ids. Referential integrity is enforced by the tests
//! below: every key must be a real id.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
pub struct LocaleSpec {
    #[serde(default)]
    pub weapons: BTreeMap<String, String>,
    #[serde(default)]
    pub enemies: BTreeMap<String, String>,
    #[serde(default)]
    pub damage_types: BTreeMap<String, String>,
    #[serde(default)]
    pub mods: BTreeMap<String, String>,
    #[serde(default)]
    pub arcanes: BTreeMap<String, String>,
    #[serde(default)]
    pub evolutions: BTreeMap<String, String>,
}

/// Every overlay locale, `(code, spec)` — the code is the filename stem
/// (`zh.yaml` → `"zh"`).
pub fn locales() -> &'static [(String, LocaleSpec)] {
    static L: OnceLock<Vec<(String, LocaleSpec)>> = OnceLock::new();
    L.get_or_init(|| {
        crate::data::files_under("i18n/")
            .filter(|(p, _)| p.ends_with(".yaml"))
            .map(|(p, text)| {
                let spec = serde_norway::from_str::<LocaleSpec>(text)
                    .unwrap_or_else(|e| panic!("parse {p}: {e}"));
                let code = p.rsplit('/').next().unwrap_or(p).trim_end_matches(".yaml");
                (code.to_string(), spec)
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zh_overlay_loads() {
        let (_, zh) = locales().iter().find(|(c, _)| c == "zh").expect("zh overlay");
        assert_eq!(zh.weapons.get("dual_toxocyst").map(String::as_str), Some("毒囊双枪"));
        assert!(!zh.damage_types.is_empty());
    }

    /// Overlay keys must reference REAL ids — a translator's typo fails the
    /// build instead of silently showing English forever.
    #[test]
    fn overlay_keys_reference_real_ids() {
        let damage_types = [
            "impact", "puncture", "slash", "cold", "electricity", "heat", "toxin",
            "blast", "corrosive", "gas", "magnetic", "radiation", "viral", "void", "true",
        ];
        for (code, spec) in locales() {
            for id in spec.weapons.keys() {
                assert!(
                    crate::weapons_data::spec(id.as_str()).is_some(),
                    "i18n/{code}: unknown weapon id '{id}'"
                );
            }
            for id in spec.enemies.keys() {
                assert!(
                    crate::enemy_data::all().iter().any(|e| &e.id == id),
                    "i18n/{code}: unknown enemy id '{id}'"
                );
            }
            for id in spec.damage_types.keys() {
                assert!(
                    damage_types.contains(&id.as_str()),
                    "i18n/{code}: unknown damage type '{id}'"
                );
            }
            for id in spec.mods.keys() {
                assert!(
                    crate::mods_data::pistol_pool().iter().any(|m| m.id == id.as_str()),
                    "i18n/{code}: unknown mod id '{id}'"
                );
            }
            for id in spec.arcanes.keys() {
                assert!(
                    crate::arcanes_data::secondary(id.as_str()).is_some(),
                    "i18n/{code}: unknown arcane id '{id}'"
                );
            }
            for id in spec.evolutions.keys() {
                assert!(
                    crate::evolutions_data::get(id.as_str()).is_some(),
                    "i18n/{code}: unknown evolution id '{id}'"
                );
            }
        }
    }
}
