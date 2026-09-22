// SPDX-License-Identifier: AGPL-3.0-or-later
//! `/api/warframe/catalog`, `/api/warframe/panel` and `/api/operator/panel`:
//! the Warframe builder's doors. A POST like every endpoint but meta and i18n,
//! and fetched only by the Warframe page, so a weapon page pays nothing for a
//! catalogue it never reads. docs/WARFRAMES.md.

use serde_json::{json, Value};
use crate::registry::assets;
use crate::request::err_json;

/// `madurai` -> `Madurai`, the spelling the page's polarity art is keyed by.
fn polarity_label(p: &str) -> String {
    let mut c = p.chars();
    c.next().map_or(String::new(), |f| f.to_uppercase().collect::<String>() + c.as_str())
}

pub fn warframe_catalog_json() -> Value {
    use wfsim_engine::data::warframes as wf;
    let a = assets();
    let tags = |t: &[wf::TagGrant]| t.iter().map(|g| json!({ "tag": g.tag.id(), "when": g.when })).collect::<Vec<_>>();
    json!({
        "ok": true,
        "frames": wf::warframes().iter().map(|f| json!({
            "id": f.id,
            "name": f.name,
            "health": f.health,
            "shield": f.shield,
            "armor": f.armor,
            "energy": f.energy,
            "sprint": f.sprint,
            "polarities": f.polarities.iter().map(|p| polarity_label(p)).collect::<Vec<_>>(),
            "aura_polarity": f.aura_polarity.as_deref().map(polarity_label),
            "exilus_polarity": f.exilus_polarity.as_deref().map(polarity_label),
            "passive": f.passive,
            "passive_tags": tags(&f.passive_tags),
            "abilities": f.abilities,
            "image": a.warframes.get(&f.id),
            "url": f.url,
        })).collect::<Vec<_>>(),
        "mods": wf::mods().iter().map(|m| json!({
            "id": m.id,
            "name": m.name,
            "rarity": m.rarity,
            "polarity": polarity_label(&m.polarity),
            // AT MAX RANK, the convention the weapon picker's `modDrain` reads.
            "drain": m.base_drain + m.max_rank,
            "max_rank": m.max_rank,
            "exilus": m.exilus,
            "aura": m.aura,
            "family": m.family,
            "set": m.set,
            "augments": m.augments,
            "image": if m.aura || a.auras.contains_key(&m.id) {
                a.auras.get(&m.id)
            } else {
                a.warframe_mods.get(&m.id)
            },
            "description": m.description,
            "effects": m.description.lines().collect::<Vec<_>>(),
            "desc_ranks": (0..=m.max_rank).map(|r| m.card_at(r).join("\n")).collect::<Vec<_>>(),
            "tags": tags(&m.tags),
        })).collect::<Vec<_>>(),
        "arcanes": wf::arcanes().iter().map(|x| json!({
            "id": x.id,
            "name": x.name,
            "rarity": x.rarity,
            "max_rank": x.max_rank,
            "image": a.warframe_arcanes.get(&x.id),
            "description": x.description,
            "effects": x.description.lines().collect::<Vec<_>>(),
            "desc_ranks": (0..=x.max_rank).map(|r| x.card_at(r).join("\n")).collect::<Vec<_>>(),
            "tags": tags(&x.tags),
        })).collect::<Vec<_>>(),
        // THE COMPANION POOL, in the same catalogue the Warframe builder reads:
        // a companion page is the same builder over a different pool, and one
        // fetch serves both. Every card is unmodelled — `data/companion_mods/`.
        "companion_slots": wfsim_engine::data::companions::COMPANION_MOD_SLOTS,
        "companion_polarities": std::iter::repeat_n(
            polarity_label(wfsim_engine::data::companions::COMPANION_POLARITY),
            wfsim_engine::data::companions::COMPANION_INNATE_POLARITIES).collect::<Vec<_>>(),
        "companion_mods": wfsim_engine::data::companions::companion_mods().iter().map(|m| json!({
            "id": m.id,
            "name": m.name,
            "rarity": m.rarity,
            "polarity": polarity_label(&m.polarity),
            // AT MAX RANK, the convention every other picker's `drain` reads.
            "drain": m.base_drain + m.max_rank,
            "max_rank": m.max_rank,
            "compat": m.compat,
            "precept": m.precept,
            "description": m.description,
            "effects": m.effects,
            "url": m.url,
        })).collect::<Vec<_>>(),
        "artifact_slots": wf::ARTIFACT_MOD_SLOTS,
        "artifact_mods": wf::artifact_mods().iter().map(|m| json!({
            "id": m.id,
            "name": m.name,
            "school": m.school,
            "rarity": m.rarity,
            "max_rank": m.max_rank,
            "bonus": m.bonus.as_ref().map(|b| json!({ "per": b.per })),
            "image": a.artifact_mods.get(&m.id),
            "effects": m.description.lines().collect::<Vec<_>>(),
            "url": m.url,
        })).collect::<Vec<_>>(),
        "artifact_arcanes": wf::artifact_arcanes().iter().map(|x| json!({
            "id": x.id,
            "name": x.name,
            "rarity": x.rarity,
            "max_rank": x.max_rank,
            "image": a.artifact_arcanes.get(&x.id),
            "effects": x.description.lines().collect::<Vec<_>>(),
            "url": x.url,
        })).collect::<Vec<_>>(),
        // THE OPERATOR'S FOCUS, for the Operator page and for what a Warframe
        // build shows it linking.
        "focus": wf::focus_schools().iter().map(|s| json!({
            "id": s.id,
            "name": s.name,
            "url": s.url,
            "artifact": s.artifact.as_ref().map(|x| json!({
                "id": x.id,
                "name": x.name,
                "image": a.artifacts.get(&x.id),
            })),
            "nodes": s.nodes.iter().map(|n| json!({
                "id": n.id,
                "name": n.name,
                "text": n.text,
                "always": n.always,
                "when": n.when,
                "tags": tags(&n.tags),
            })).collect::<Vec<_>>(),
            // THE TWO WAYBOUNDS — shown at max rank whichever school is
            // active, and never a choice: unlocking one cannot be undone. No
            // `always`, no `when` and no tags, because none of them reaches the
            // Warframe (`data/notes.yaml` focus_waybound).
            "waybound": s.waybound.iter().map(|w| json!({
                "id": w.id,
                "name": w.name,
                "text": w.text,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "abilities": wf::abilities().iter().map(|x| json!({
            "id": x.id,
            "name": x.name,
            "frame": x.frame,
            "slot": x.slot,
            "energy_cost": x.energy_cost,
            "cost_type": x.cost_type,
            "subsumable": x.subsumable,
            "augments": x.augments,
            "icon": x.icon,
            "description": x.description,
            "tags": tags(&x.tags),
            "url": x.url,
        })).collect::<Vec<_>>(),
    })
}

/// `/api/operator/panel`: the Operator page's artifact, each seated card with its
/// bonus line paid out for what is seated beside it.
pub fn operator_panel_json(v: &Value) -> Value {
    use wfsim_engine::data::warframes as wf;
    let pick: wf::ArtifactPick = match serde_json::from_value(v.get("artifact").cloned().unwrap_or_else(|| json!({}))) {
        Ok(p) => p,
        Err(e) => return err_json(format!("bad artifact: {e}")),
    };
    let seated: Vec<&wf::ArtifactMod> = pick.mods.iter().filter_map(|id| wf::artifact_mod_by_id(id)).collect();
    json!({
        "ok": true,
        "mods": seated.iter().map(|m| json!({
            "id": m.id,
            "lines": m.card_with(&seated),
            "count": m.bonus_count(&seated),
        })).collect::<Vec<_>>(),
        "refused": wf::artifact_refusals(&pick),
    })
}

pub fn warframe_panel_json(v: &Value) -> Value {
    use wfsim_engine::data::warframes as wf;
    let build: wf::Build = match serde_json::from_value(v.clone()) {
        Ok(b) => b,
        Err(e) => return err_json(format!("bad Warframe build: {e}")),
    };
    let r = match wf::resolve(&build) {
        Ok(r) => r,
        Err(e) => return err_json(e),
    };
    let scaling = |s: wf::Scaling| match s {
        wf::Scaling::Strength => "strength",
        wf::Scaling::Duration => "duration",
        wf::Scaling::Range => "range",
        wf::Scaling::CastingSpeed => "casting_speed",
        wf::Scaling::None => "none",
    };
    json!({
        "ok": true,
        "frame": r.frame.id,
        "stats": r.stats.iter().map(|l| json!({
            "id": l.stat.id(),
            "label": l.stat.label(),
            "ratio": l.stat.is_ratio(),
            "base": l.base,
            "bonus": l.bonus,
            "flat": l.flat,
            "value": l.value,
            "sources": l.sources.iter().map(|c| json!({
                "from": c.from, "value": c.value, "flat": c.flat, "times": c.times,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "abilities": r.abilities.iter().map(|x| json!({
            "slot": x.slot,
            "id": x.ability.id,
            "helminth": x.helminth,
            "base_energy_cost": if x.helminth {
                x.ability.infused_energy_cost.unwrap_or(x.ability.energy_cost)
            } else {
                x.ability.energy_cost
            },
            // WHAT THE INFUSED VERSION DOES DIFFERENTLY, only when it is infused.
            "infused_notes": if x.helminth { x.ability.infused_notes.clone() } else { Vec::new() },
            "energy_cost": x.energy_cost,
            "cost_type": x.ability.cost_type,
            "base_drain_per_second": x.ability.drain_per_second,
            "drain_per_second": x.drain_per_second,
            "lines": x.lines.iter().map(|l| json!({
                "id": l.stat.id,
                "label": l.stat.label,
                "unit": l.stat.unit,
                "scales_with": scaling(l.stat.scales_with),
                "base": l.base,
                "value": l.value,
            })).collect::<Vec<_>>(),
            "derived": x.derived.iter().map(|d| json!({
                "label": d.label, "stat": d.stat.id(), "value": d.value,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        // EVERY TAG WITH EVERY SOURCE, so the page can say where each came from.
        "tags": r.tags.iter().map(|t| json!({
            "tag": t.tag.id(), "label": t.tag.label(), "from": t.from, "when": t.when,
        })).collect::<Vec<_>>(),
        "shield_gate": json!({
            "max_shields": r.shield_gate.max_shields,
            "full_seconds": r.shield_gate.full_seconds,
            "fixed_by": r.shield_gate.fixed_by,
            "energy_to_shield": r.shield_gate.energy_to_shield,
            "sources": r.shield_gate.sources.iter().map(|(from, v)| json!({ "from": from, "value": v })).collect::<Vec<_>>(),
            "casts": r.shield_gate.casts.iter().map(|c| json!({
                "slot": c.slot, "ability": c.ability, "energy": c.energy, "shields": c.shields,
                "full": c.full, "seconds": c.seconds,
            })).collect::<Vec<_>>(),
        }),
        "admissions": r.admissions.iter().map(|x| json!({
            "from": x.from,
            "text": x.text,
            "kind": match x.kind {
                wf::AdmissionKind::Unmodelled => "unmodelled",
                wf::AdmissionKind::OutOfScope => "out_of_scope",
                wf::AdmissionKind::Inert => "inert",
            },
        })).collect::<Vec<_>>(),
        "refused": r.refused,
    })
}
