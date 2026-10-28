// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared package actions, referenced by stance and weapon input contexts.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};
use crate::model::{ComboHit, MeleeAction, MeleeCombo, MeleeInput, MeleeTimeline};
use serde::{Deserialize, Deserializer};

#[derive(Deserialize)]
struct Catalog {
    actions: BTreeMap<String, MeleeAction>,
    trees: BTreeMap<String, BTreeMap<String, Vec<String>>>,
    #[serde(default)]
    geometry: BTreeMap<String, Vec<bool>>,
    #[serde(default)]
    repeat_from_input: BTreeMap<String, BTreeMap<String, usize>>,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        // Generated JSON-compatible YAML avoids retaining a full YAML parse
        // document while the weapon catalog is loading (docs/WASM.md).
        let mut source = super::file("melee/actions.yaml").expect("melee action data installed");
        while source.trim_start().starts_with('#') {
            source = source.split_once('\n').map_or("", |(_, rest)| rest);
        }
        serde_json::from_str(source).expect("melee action catalog")
    })
}

pub fn compile(combo: &MeleeCombo) -> Vec<ComboHit> {
    let MeleeCombo::Package { tree, context } = combo else { return combo.compile() };
    let c = catalog();
    let sequence = c.trees.get(tree).and_then(|t| t.get(context))
        .unwrap_or_else(|| panic!("missing melee package context {tree}/{context}"));
    let inputs = sequence.iter().map(|id| {
        let a = c.actions.get(id).unwrap_or_else(|| panic!("missing melee package action {id}"));
        let mut a = a.clone();
        if let Some(mask) = c.geometry.get(id) {
            assert_eq!(mask.len(), a.events.len(), "melee geometry mask must match {id}");
            for (event, spin) in a.events.iter_mut().zip(mask) { event.all_around = *spin; }
        }
        MeleeInput { actions: vec![a] }
    }).collect();
    let repeat_from_input = c.repeat_from_input.get(tree).and_then(|t| t.get(context)).copied().unwrap_or(0);
    MeleeTimeline { inputs, repeat_from_input }.compile()
}

pub fn resolve(combos: &BTreeMap<String, MeleeCombo>, form: &str) -> Vec<ComboHit> {
    let mut seen = Vec::new();
    let mut key = form;
    loop {
        assert!(!seen.contains(&key), "cyclic melee combo alias: {seen:?}");
        seen.push(key);
        match combos.get(key).unwrap_or_else(|| panic!("unknown melee combo alias: {key}")) {
            MeleeCombo::Alias { reuse } => key = reuse,
            combo => return compile(combo),
        }
    }
}

type SharedScripts = BTreeMap<(String, String), Arc<[ComboHit]>>;

pub fn deserialize_script<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Arc<[ComboHit]>, D::Error> {
    let combo = MeleeCombo::deserialize(deserializer)?;
    let MeleeCombo::Package { tree, context } = &combo else { return Ok(compile(&combo).into()); };
    // A package input is immutable; its weapon-specific charge is resolved
    // only when constructing a build, so the catalog can share its rows.
    static SCRIPTS: OnceLock<Mutex<SharedScripts>> = OnceLock::new();
    let mut scripts = SCRIPTS.get_or_init(Default::default).lock().expect("melee scripts cache");
    let key = (tree.clone(), context.clone());
    Ok(Arc::clone(scripts.entry(key).or_insert_with(|| compile(&combo).into())))
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_inputs_share_rows_but_builds_resolve_their_own_copy() {
        let a = crate::data::weapons::spec("magistar").unwrap();
        let b = crate::data::weapons::spec("sancti_magistar").unwrap();
        assert!(std::sync::Arc::ptr_eq(&a.attack.combo_script, &b.attack.combo_script));
        let expected = b.attack.combo_script[0].multiplier;
        let mut build = crate::model::WeaponBase::from_data("magistar", true, &[]);
        build.combo_script[0].multiplier += 100.0;
        assert_eq!(b.attack.combo_script[0].multiplier, expected);
        assert_eq!(crate::model::WeaponBase::from_data("magistar", true, &[]).combo_script[0].multiplier, expected);
    }
}
