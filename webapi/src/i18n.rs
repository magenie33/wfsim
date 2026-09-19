// SPDX-License-Identifier: AGPL-3.0-or-later
//! `/api/i18n`: the display-name overlay for every locale.

use serde_json::{json, Value};

/// Display-name overlays for every locale: `{ "<code>": { weapons: {id:
/// name}, enemies: {...}, damage_types: {...}, mods/arcanes/evolutions } }`.
/// English is the fallback built into every entity's own `name` — it has no
/// overlay.
pub fn i18n_json() -> Value {
    let mut out = serde_json::Map::new();
    for (code, l) in wfsim_engine::data::i18n::locales() {
        out.insert(
            code.clone(),
            json!({
                "weapons": l.weapons,
                "enemies": l.enemies,
                "damage_types": l.damage_types,
                "mods": l.mods,
                "arcanes": l.arcanes,
                "evolutions": l.evolutions,
                "abilities": l.abilities,
                // WHAT THE WARFRAME BRINGS. `shards` is keyed BOTH by a
                // colour's id and by `<shard>/<effect>`, which is why the page
                // looks a socket up by the same composite key it sends.
                "auras": l.auras,
                "shards": l.shards,
                "warframe_mods": l.warframe_mods,
                "warframe_arcanes": l.warframe_arcanes,
                "warframe_abilities": l.warframe_abilities,
                "artifact_mods": l.artifact_mods,
                "warframe_mod_descriptions": l.warframe_mod_descriptions,
                "warframe_arcane_descriptions": l.warframe_arcane_descriptions,
                "warframe_ability_descriptions": l.warframe_ability_descriptions,
                "ui": l.ui,
                "effect_phrases": l.effect_phrases,
                // DE's OWN card text, per rank — what the UI shows instead of
                // running the phrase table over our English line. The phrase
                // table stays for what DE never wrote (our engine-generated
                // effect lines, panel labels, Incarnon evolutions).
                "mod_descriptions": l.mod_descriptions,
                "arcane_descriptions": l.arcane_descriptions,
                // Evolutions carry no ranks, so one string each — and no
                // export to generate them from (data/i18n/zh/evolutions.yaml).
                "evolution_descriptions": l.evolution_descriptions,
            }),
        );
    }
    Value::Object(out)
}
