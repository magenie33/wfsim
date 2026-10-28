// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared package actions, referenced by stance and weapon input contexts.

use std::collections::BTreeMap;
use std::sync::OnceLock;
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
    CATALOG.get_or_init(|| serde_norway::from_str(super::file("melee/actions.yaml").expect("melee action data installed"))
        .expect("melee action catalog"))
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

pub fn deserialize_script<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<ComboHit>, D::Error> {
    MeleeCombo::deserialize(deserializer).map(|c| compile(&c))
}
