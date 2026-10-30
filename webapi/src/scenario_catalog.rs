// SPDX-License-Identifier: AGPL-3.0-or-later
//! The global scenario vocabulary, projected from every weapon's legal buff scope.

use crate::registry::{evo_group, weapons, WeaponInfo};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::OnceLock;

fn scope(info: &WeaponInfo) -> Value {
    let pool = crate::rivens::mod_pool_with_rivens(&json!({}), info, &[]);
    let mods = pool
        .iter()
        .map(|m| (m.id.to_string(), json!("search")))
        .collect::<serde_json::Map<String, Value>>();
    let arcanes = info
        .arcane_pools
        .iter()
        .flat_map(|p| wfsim_engine::data::arcanes::pool_for_weapon(&info.id, p))
        .map(|a| (a.id.clone(), json!("search")))
        .collect::<serde_json::Map<String, Value>>();
    let group = evo_group(info);
    let evolutions = (1..=wfsim_engine::data::evolutions::tier_count(group))
        .map(|tier| {
            (
                tier.to_string(),
                json!(wfsim_engine::data::evolutions::options(group, tier)
                    .iter()
                    .map(|e| &e.id)
                    .collect::<Vec<_>>()),
            )
        })
        .collect::<serde_json::Map<String, Value>>();
    crate::opt_buffs_json(
        &json!({"weapon":info.id,"mods":mods,"arcanes":arcanes,"evolutions":evolutions}),
    )
}

pub(crate) fn buff_catalog_json() -> Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            let mut entries = BTreeMap::<String, Value>::new();
            for weapon in weapons() {
                let answer = scope(weapon);
                let Some(buffs) = answer["buffs"].as_array() else {
                    continue;
                };
                for buff in buffs {
                    let Some(id) = buff["id"].as_str() else {
                        continue;
                    };
                    let entry = entries.entry(id.to_owned()).or_insert_with(|| {
                        let mut entry = buff.clone();
                        entry["weapons"] = json!([]);
                        entry["variants"] = json!([]);
                        entry
                    });
                    entry["weapons"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!(weapon.id));
                    let variants = entry["variants"].as_array_mut().unwrap();
                    if let Some(variant) =
                        variants.iter_mut().find(|variant| variant["buff"] == *buff)
                    {
                        variant["weapons"]
                            .as_array_mut()
                            .unwrap()
                            .push(json!(weapon.id));
                    } else {
                        variants.push(json!({"buff":buff,"weapons":[weapon.id]}));
                    }
                }
            }
            json!({"ok":true,"buffs":entries.into_values().collect::<Vec<_>>()})
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_catalog_preserves_every_weapon_scope_and_its_variants() {
        let catalog = buff_catalog_json();
        let entries = catalog["buffs"].as_array().unwrap();
        assert!(!entries.is_empty());
        for weapon in weapons() {
            let answer = scope(weapon);
            for buff in answer["buffs"].as_array().unwrap() {
                let entry = entries
                    .iter()
                    .find(|entry| entry["id"] == buff["id"])
                    .unwrap();
                assert!(entry["weapons"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(weapon.id)));
                assert!(entry["variants"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|variant| variant["buff"] == *buff
                        && variant["weapons"]
                            .as_array()
                            .unwrap()
                            .contains(&json!(weapon.id))));
            }
        }
        let unique = entries
            .iter()
            .map(|entry| entry["id"].as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), entries.len());
    }
}
