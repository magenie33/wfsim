//! wfsim-webapi: the JSON API layer, independent of any transport.
//!
//! Every endpoint the frontend talks to lives here as a plain
//! `&Value -> Value` function: [`meta_json`], [`panel_json`],
//! [`simulate_json`], [`opt_buffs_json`], and the optimize pair
//! [`parse_optimize`] / [`run_optimize`]. The native HTTP server
//! (`wfsim-web`) routes requests to these; the wasm build calls them
//! directly in the browser (docs/WASM.md phase 2). The compute is the SAME
//! engine the CLI and optimizer use — this crate only shapes JSON.

use serde_json::{json, Value};
use wfsim_engine::dummy::{
    monte_carlo, BodyPart, BuffLock, DummyParams, LockMode, LockedBuff, TargetMode,
};
use wfsim_engine::enemy_data::EnemySpec;
use wfsim_engine::loadout::{
    pct as fpct, resolve, resolve_with, ModDef, ModEffect, ResolvedPanel, StackPolicy,
    WeaponBase,
};
use wfsim_engine::mods::{plan_forma, PlannedMod, Polarity};
use wfsim_optimizer::{
    enumerate_candidates_each, enumerate_candidates_observed, run_funnel, schedule_to,
    stream_screen, Candidate, Constraints, FunnelState, Job, Scenario,
};

// ---- Enemy library (the engine's embedded data/enemies/**) -------------
// Single source of truth: the same data/ files the CLI and optimizer read,
// embedded by the engine's build script. The UI lists the classics first;
// anything new in data/enemies/ appends after them in path order.
fn enemies() -> Vec<EnemySpec> {
    let preferred = ["thrax_centurion"];
    let mut specs = wfsim_engine::enemy_data::all();
    specs.sort_by_key(|s| {
        preferred
            .iter()
            .position(|p| *p == s.id)
            .unwrap_or(preferred.len())
    });
    specs
}

#[derive(serde::Deserialize, Default)]
struct Assets {
    #[serde(default)]
    weapons: std::collections::HashMap<String, String>,
    #[serde(default)]
    mods: std::collections::HashMap<String, String>,
    #[serde(default)]
    arcanes: std::collections::HashMap<String, String>,
}

// ---- Image asset map (data/assets.yaml, embedded by the engine) --------
// id -> WFCD imageName; the frontend builds https://cdn.warframestat.us/img/<name>.
fn assets() -> &'static Assets {
    use std::sync::OnceLock;
    static A: OnceLock<Assets> = OnceLock::new();
    A.get_or_init(|| {
        let yaml = wfsim_engine::data::file("assets.yaml").expect("embedded data/assets.yaml");
        serde_norway::from_str(yaml).unwrap_or_default()
    })
}

// ---- weapon registry ---------------------------------------------------
// The UI is weapon-aware. Each weapon declares its mod class (which mod pool
// the picker shows), whether it takes an arcane / Evolution II, its available
// forms, and whether it is a sentinel (BaseOnly resolution — Galvanized
// conditionals never fire).

struct WeaponInfo {
    id: String,
    name: String,
    // The MOD-ELIGIBILITY group, not a cosmetic label. "pistol" = the Pistol
    // Mods pool, which (wiki Pistol_Mods) equips on secondary Pistols, Dual
    // Pistols, Shotgun Sidearms, Crossbows, and Tomes. This is the ACTUAL way
    // mods take effect, so the eligibility group is what drives the pool.
    mod_class: String, // "pistol" | "rifle"
    // Precise weapon type within that group (Dual Toxocyst = Dual Pistols).
    subtype: String,
    sentinel: bool,
    forms: Vec<(&'static str, String)>,
    uses_arcane: bool,
    /// Which arcane pool this weapon draws from — its own slot
    /// ("secondary" / "primary"). The picker filters on it.
    arcane_slot: String,
    uses_evo2: bool,
}

/// "dual_pistols" → "Dual Pistols".
fn title_case(snake: &str) -> String {
    snake
        .split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// One line stating a form's trigger/shot mechanics, from the weapon data.
fn attack_desc(s: &wfsim_engine::weapons_data::WeaponSpec) -> String {
    let mut parts = vec![title_case(&s.attack.trigger).replace(' ', "-")];
    if let Some(st) = &s.attack.shot_type {
        parts.push(st.clone());
    }
    if let Some(r) = &s.attack.ricochet {
        parts.push(format!(
            "ricochet to {} enem{} within {} m",
            r.targets,
            if r.targets == 1 { "y" } else { "ies" },
            r.range_m
        ));
    }
    parts.join(" · ")
}

// The weapon registry, derived from data/weapons/*.yaml (roster = transform
// group base entries; an Incarnon form is a form, not a roster row).
fn weapons() -> &'static [WeaponInfo] {
    use std::sync::OnceLock;
    static W: OnceLock<Vec<WeaponInfo>> = OnceLock::new();
    W.get_or_init(|| {
        wfsim_engine::weapons_data::roster()
            .map(|s| {
                let sentinel = s.class.contains("sentinel");
                let incarnon = s.transforms_to.is_some();
                let forms = if incarnon {
                    vec![
                        (
                            "incarnon_cycle",
                            "Incarnon cycle (real two-form loop)".to_string(),
                        ),
                        ("incarnon", "Incarnon form only".to_string()),
                        ("base", "Base form only".to_string()),
                    ]
                } else {
                    vec![("primary", "Standard".to_string())]
                };
                WeaponInfo {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    mod_class: s
                        .mod_eligibility
                        .as_deref()
                        .map(|m| m.trim_end_matches("_mods").to_string())
                        .unwrap_or_else(|| s.slot.clone()),
                    subtype: title_case(&s.class),
                    sentinel,
                    forms,
                    uses_arcane: !sentinel,
                    arcane_slot: s.slot.clone(),
                    uses_evo2: incarnon,
                }
            })
            .collect()
    })
}

fn weapon(id: &str) -> &'static WeaponInfo {
    weapons()
        .iter()
        .find(|w| w.id == id)
        .unwrap_or(&weapons()[0])
}

// ---- spec-derived lookups: no weapon ids are hardcoded anywhere below ----
fn wspec(id: &str) -> &'static wfsim_engine::weapons_data::WeaponSpec {
    wfsim_engine::weapons_data::spec(id).expect("weapon data")
}

/// The transform group's second-form entry (the Incarnon form), if any.
fn incarnon_id(info: &WeaponInfo) -> Option<&'static str> {
    wspec(&info.id).transforms_to.as_deref()
}

/// Evolutions-data key for this weapon: the transform group name.
fn evo_group(info: &WeaponInfo) -> &'static str {
    let s = wspec(&info.id);
    s.transform_group.as_deref().unwrap_or(&s.id)
}

/// The tier-1 evolution that unlocks the second form (deselecting it means
/// no transformation).
fn form_unlock_evo(info: &WeaponInfo) -> Option<&'static str> {
    wfsim_engine::evolutions_data::options(evo_group(info), 1)
        .first()
        .map(|e| e.id.as_str())
}

/// Whether the weapon's data declares the Frenzy perk (data/perks/).
fn has_frenzy(info: &WeaponInfo) -> bool {
    wspec(&info.id).perks.iter().any(|p| p.id() == "frenzy")
}

fn default_weapon_id() -> &'static str {
    &weapons()[0].id
}

// The FULL pool (exilus included) of a weapon's mod class — the picker and
// every id lookup go through here, so a weapon whose `mod_eligibility` names
// a class with no data yet gets an empty pool rather than another weapon's.
fn mod_pool_for(class: &str) -> Vec<ModDef> {
    wfsim_engine::mods_data::class_pool(class)
}

// 8 main slots (innate polarities from the weapon yaml) + the exilus slot
// as the UI's 9th slot, carrying ITS innate polarity too (wiki "Exilus
// Polarity" — caught in the 2026-07-28 wiki cross-check; it was modeled as
// unpolarized before). Same model as autoForma and the optimizer — without
// the 9th slot a 9-mod build trips plan_forma's mods≤slots assert.
fn innate_slots_for(id: &str) -> Vec<Option<Polarity>> {
    let mut v = wfsim_engine::weapons_data::innate_slots(id).to_vec();
    v.push(wfsim_engine::weapons_data::exilus_polarity(id));
    v
}

// ---- /api/i18n ---------------------------------------------------------

/// Display-name overlays for every locale: `{ "<code>": { weapons: {id:
/// name}, enemies: {...}, damage_types: {...}, mods/arcanes/evolutions } }`.
/// English is the fallback built into every entity's own `name` — it has no
/// overlay.
pub fn i18n_json() -> Value {
    let mut out = serde_json::Map::new();
    for (code, l) in wfsim_engine::i18n_data::locales() {
        out.insert(
            code.clone(),
            json!({
                "weapons": l.weapons,
                "enemies": l.enemies,
                "damage_types": l.damage_types,
                "mods": l.mods,
                "arcanes": l.arcanes,
                "evolutions": l.evolutions,
                "ui": l.ui,
                "effect_phrases": l.effect_phrases,
            }),
        );
    }
    Value::Object(out)
}

// ---- /api/meta ---------------------------------------------------------

/// A coarse category for grouping mods in the picker UI.
fn mod_category(m: &ModDef) -> &'static str {
    let has = |f: fn(&ModEffect) -> bool| m.effects.iter().any(f);
    if has(|e| matches!(e, ModEffect::Element(..) | ModEffect::CombinedElement(..))) {
        "element"
    } else if has(|e| {
        matches!(
            e,
            ModEffect::CritChance(..)
                | ModEffect::CritDamage(..)
                | ModEffect::OnHeadshotCritChance { .. }
                | ModEffect::OnHeadshotKillCritChance { .. }
        )
    }) {
        "crit"
    } else if has(|e| {
        matches!(
            e,
            ModEffect::StatusChance(..)
                | ModEffect::StatusDamage(..)
                | ModEffect::ConditionOverload { .. }
        )
    }) {
        "status"
    } else if has(|e| matches!(e, ModEffect::FireRate(..) | ModEffect::ReloadSpeed(..))) {
        "handling"
    } else {
        "damage"
    }
}

fn mods_json(p: &[ModDef]) -> Vec<Value> {
    p.iter()
        .map(|m| {
            let mut j = json!({
                "id": m.id,
                "name": prettify(m.id),
                "drain": m.base_drain,
                "max_rank": m.max_rank,
                "polarity": format!("{:?}", m.polarity),
                "rarity": format!("{:?}", m.rarity).to_lowercase(),
                "exilus": m.exilus,
                "family": m.family,
                "category": mod_category(m),
                "image": assets().mods.get(m.id),
                // One line per modeled effect — engine describe() stays the
                // model's own statement (search + panel attribution).
                "effects": m.effects.iter().map(|e| e.describe()).collect::<Vec<_>>(),
            });
            // The verbatim in-game DESCRIPTION per rank (X filled) — what
            // the picker and the configured slot display. Absent for pools
            // without yaml descriptions (the hardcoded rifle pool): the UI
            // falls back to the effect lines.
            if let Some(info) = wfsim_engine::mods_data::desc_info(m.id) {
                let dr: Vec<String> = (0..=info.max_rank).map(|r| info.at(r)).collect();
                j["desc_ranks"] = json!(dr);
            }
            j
        })
        .collect()
}

pub fn meta_json() -> Value {
    let weapons: Vec<Value> = weapons()
        .iter()
        .map(|w| {
            json!({
                "id": w.id,
                "name": w.name,
                "mod_class": w.mod_class,
                "subtype": w.subtype,
                "sentinel": w.sentinel,
                "uses_arcane": w.uses_arcane,
                "arcane_slot": w.arcane_slot,
                "uses_evo2": w.uses_evo2,
                "arcane_slots": 1,
                "image": assets().weapons.get(&w.id),
                "innate_polarities": innate_slots_for(&w.id).iter()
                    .map(|p| p.map(|x| format!("{x:?}")))
                    .collect::<Vec<_>>(),
                "forms": w.forms.iter().map(|(id, name)| json!({"id": id, "name": name})).collect::<Vec<_>>(),
                "evolutions": (1u32..=wfsim_engine::evolutions_data::tier_count(evo_group(w)))
                    .map(|tier| json!({
                        "tier": tier,
                        "options": wfsim_engine::evolutions_data::options(evo_group(w), tier)
                            .iter()
                            .map(|e| json!({
                                "id": e.id,
                                "name": e.name,
                                "icon": e.icon,
                                "broken": e.currently_broken,
                                "desc": e.description.split('\n').collect::<Vec<_>>(),
                                "effects": e.describe(),
                            }))
                            .collect::<Vec<_>>(),
                    }))
                    .filter(|t| !t["options"].as_array().unwrap().is_empty())
                    .collect::<Vec<_>>(),
            })
        })
        .collect();

    let enemies: Vec<Value> = enemies()
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "name": e.name,
                "synthetic": e.synthetic,
                "base_level": e.stats.base_level,
                "can_be_eximus": e.can_be_eximus,
                "parts": e.body_parts.iter().map(|b| json!({
                    "name": b.name, "multiplier": b.multiplier, "is_head": b.is_head
                })).collect::<Vec<_>>(),
            })
        })
        .collect();

    // Arcanes: every slot found under data/arcanes/ (secondary today,
    // primary next), each entry TAGGED with its slot so the picker can show
    // only the ones a weapon can equip. Per-rank effect lines come from the
    // same describe the model uses, so the picker states what the sim
    // computes. "none" belongs to every slot.
    let mut arcanes_json: Vec<Value> = vec![json!(
        {"id": "none", "name": "None", "image": null, "ranks": [], "max_rank": 0, "rarity": null, "slot": null}
    )];
    for slot in wfsim_engine::arcanes_data::slots() {
    for a in wfsim_engine::arcanes_data::slot_pool(slot) {
        let ranks: Vec<Vec<String>> = (0..=a.max_rank).map(|r| a.describe_at(r)).collect();
        // The verbatim in-game description per rank (X filled) — the display
        // text; `ranks` (model describe lines) stays for search.
        let desc_ranks: Vec<String> = (0..=a.max_rank).map(|r| a.desc_at(r)).collect();
        arcanes_json.push(json!({
            "id": a.id,
            "name": a.name,
            "image": assets().arcanes.get(&a.id),
            "ranks": ranks,
            "desc_ranks": desc_ranks,
            "max_rank": a.max_rank,
            "rarity": format!("{:?}", a.rarity).to_lowercase(),
            "slot": slot,
        }));
    }
    }

    json!({
        "weapons": weapons,
        // One pool per mod CLASS present in data/mods/ — a weapon's
        // `mod_class` (derived from its mod_eligibility) indexes into this.
        // Adding data/mods/rifle/ publishes a rifle pool with no code change.
        "mod_pools": wfsim_engine::mods_data::classes()
            .into_iter()
            .map(|c| (c.to_string(), json!(mods_json(&wfsim_engine::mods_data::class_pool(c)))))
            .collect::<serde_json::Map<String, Value>>(),
        "enemies": enemies,
        // Arcanes mirror the mod pool: per-rank effect lines (`ranks[r]`),
        // max_rank, rarity — so the web picker searches effects and the slot
        // steps ranks with the strength updating per rank. `arcane_rank` in
        // the sim request selects the modeled rank (default: max).
        "arcanes": arcanes_json,
        // Choosable evolution tiers from data/evolutions/*.yaml (tier 1 =
        // the Incarnon Form unlock — deselecting it means no transformation,
        // so the panel/sim fall back to the base form). Every tier also gets
        // an implicit EMPTY choice in the UI (nothing installed); `broken` =
        // wiki-flagged non-functional — the engine applies ZERO for those,
        // and the UI must say so in red. `desc` lines are the verbatim
        // effect text (like the mod/arcane cards).
        "defaults": {
            "weapon": default_weapon_id(),
            "form": "incarnon_cycle",
            // The page starts EMPTY (user decision): no mods, no arcane, no
            // evolutions — a bare weapon. Reference builds live as presets /
            // data/builds, not as the initial state.
            "evolutions": {},
            "arcane": "none",
            "enemy": "thrax_centurion",
            "level": 9999,
            "steel_path": true,
            "headshot_pct": 100.0,
            // Aiming ASSUMED by default - the sim's behaviour before the knob
            // existed, so no stored preset silently changes meaning.
            "aiming": true,
            "duration": 120.0,
            "runs": 100,
            "mods": [],
        },
    })
}

/// mod-id → Title Case display name ("primed_target_cracker" → "Primed Target Cracker").
fn prettify(id: &str) -> String {
    id.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ---- /api/simulate -----------------------------------------------------

fn get_str<'a>(v: &'a Value, key: &str, default: &'a str) -> &'a str {
    v.get(key).and_then(|x| x.as_str()).unwrap_or(default)
}
fn get_f64(v: &Value, key: &str, default: f64) -> f64 {
    v.get(key).and_then(|x| x.as_f64()).unwrap_or(default)
}
fn get_u32(v: &Value, key: &str, default: u32) -> u32 {
    v.get(key)
        .and_then(|x| x.as_u64())
        .map(|n| n as u32)
        .unwrap_or(default)
}
fn get_bool(v: &Value, key: &str, default: bool) -> bool {
    v.get(key).and_then(|x| x.as_bool()).unwrap_or(default)
}

pub fn err_json(msg: impl Into<String>) -> Value {
    json!({ "ok": false, "error": msg.into() })
}

// ---- buff enumeration --------------------------------------------------
//
// The configurable buffs of a build (weapon-scoped; shared across forms), for
// the Sim panel's per-buff cards and for `simulate_json` to map config → spec.
// Enumerated from the SOURCE vocabulary (mod effects + arcane buffs + the
// weapon passive), NOT from resolved Option fields — `panel_json` runs
// AssumedMax where those are empty. `default_*` encode today's behavior so the
// UI pre-fills sensibly. Each buff-type appears at most once in a legal build.
struct BuffMeta {
    id: String,
    name: String,
    max_stacks: u32,
    kind: &'static str, // "stacking" | "toggle"
    default_stacks: u32,
    default_locked: bool,
    /// PERMANENT stacks (no in-sim trigger, no decay — Fevered Frenzy): the
    /// count is a static choice, so the lock control is meaningless and the
    /// UI greys it out with a hint.
    permanent: bool,
}

fn grant_label(g: wfsim_engine::arcanes_data::ArcGrant) -> &'static str {
    use wfsim_engine::arcanes_data::ArcGrant::*;
    match g {
        BaseDamage => "Base Damage",
        Multishot => "Multishot",
        ReloadSpeed => "Reload Speed",
        CritDamage => "Critical Damage",
    }
}

fn enumerate_buffs(
    refs: &[&ModDef],
    arcane: &wfsim_engine::arcanes_data::ArcaneFx,
    info: &WeaponInfo,
) -> Vec<BuffMeta> {
    // Sentinels resolve under BaseOnly — conditional buffs never fire, so
    // there is nothing to configure.
    if info.sentinel {
        return Vec::new();
    }
    let mut out: Vec<BuffMeta> = Vec::new();
    let mut push = |b: BuffMeta| {
        if !out.iter().any(|x| x.id == b.id) {
            out.push(b);
        }
    };
    // Weapon passive: Frenzy (Dual Toxocyst); a single on/off "stack".
    // Default UNLOCKED (user, 2026-07-28): starts active, then lives by its
    // real triggers (headshot refresh) instead of an assumed 100% uptime.
    if has_frenzy(info) {
        push(BuffMeta {
            id: "frenzy".into(),
            name: "Frenzy".into(),
            max_stacks: 1,
            kind: "toggle",
            default_stacks: 1,
            default_locked: false,
            permanent: false,
        });
    }
    // Mod-granted buffs.
    for m in refs {
        let nm = prettify(m.id);
        for e in &m.effects {
            use ModEffect::*;
            match *e {
                OnKillMultishot { max_stacks, .. } => push(BuffMeta {
                    id: "on_kill_multishot".into(),
                    name: nm.clone(),
                    max_stacks,
                    kind: "stacking",
                    default_stacks: max_stacks,
                    default_locked: false,
                    permanent: false,
                }),
                ConditionOverload { max_stacks, .. } => push(BuffMeta {
                    id: "condition_overload".into(),
                    name: nm.clone(),
                    max_stacks,
                    kind: "stacking",
                    default_stacks: max_stacks,
                    default_locked: false,
                    permanent: false,
                }),
                OnHeadshotCritChance { .. } => push(BuffMeta {
                    id: "on_headshot_cc".into(),
                    name: nm.clone(),
                    max_stacks: 1,
                    kind: "toggle",
                    default_stacks: 1,
                    default_locked: false,
                    permanent: false,
                }),
                OnHeadshotKillCritChance { max_stacks, .. } => push(BuffMeta {
                    id: "on_headshot_kill_cc".into(),
                    name: nm.clone(),
                    max_stacks,
                    kind: "stacking",
                    default_stacks: max_stacks,
                    default_locked: false,
                    permanent: false,
                }),
                OnKillCritDamage { .. } => push(BuffMeta {
                    id: "on_kill_cd".into(),
                    name: nm.clone(),
                    max_stacks: 1,
                    kind: "toggle",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                }),
                OnReloadFireRate { .. } => push(BuffMeta {
                    id: "on_reload_fr".into(),
                    name: nm.clone(),
                    max_stacks: 1,
                    kind: "toggle",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                }),
                _ => {}
            }
        }
    }
    // Arcane buffs (one card per spec; stacking arcanes start full).
    if !arcane.buffs.is_empty() {
        let aname = wfsim_engine::arcanes_data::secondary(&arcane.id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| prettify(&arcane.id));
        let multi = arcane.buffs.len() > 1;
        for (i, b) in arcane.buffs.iter().enumerate() {
            let id = if multi {
                format!("arcane:{}:{}", arcane.id, i)
            } else {
                format!("arcane:{}", arcane.id)
            };
            let name = if multi {
                format!("{} ({})", aname, grant_label(b.grant))
            } else {
                aname.clone()
            };
            let kind = if b.max_stacks > 1 {
                "stacking"
            } else {
                "toggle"
            };
            push(BuffMeta {
                id,
                name,
                max_stacks: b.max_stacks,
                kind,
                default_stacks: b.max_stacks,
                default_locked: false,
                permanent: false,
            });
        }
    }
    out
}

/// Evolution-granted configurable buffs (Fevered Frenzy's permanent stacked
/// multishot): one card per evolution with an `ms_buff`. PERMANENT — no
/// in-sim trigger and no decay, so the stack count is a static choice (full
/// by default) and the lock is display-only.
fn evo_buffs(evo_ids: &[String]) -> Vec<BuffMeta> {
    // NO per-effect knowledge here: the engine decides what is a
    // configurable buff (`EvolutionDef::buff_cards`, an exhaustive match),
    // so a new evolution mechanic surfaces on the cards the moment it is
    // modeled — nothing to remember to add on this side.
    evo_ids
        .iter()
        .filter_map(|id| wfsim_engine::evolutions_data::get(id))
        .flat_map(|def| {
            def.buff_cards().into_iter().map(move |c| BuffMeta {
                id: c.id.into(),
                name: def.name.clone(),
                max_stacks: c.max_stacks,
                kind: "stacking",
                // Start FULL, like every other stacking buff in the
                // product; only permanent stacks (no trigger, no decay)
                // default LOCKED, because they cannot move either way.
                default_stacks: c.max_stacks,
                default_locked: c.permanent,
                permanent: c.permanent,
            })
        })
        .collect()
}

fn buffs_json(list: &[BuffMeta]) -> Vec<Value> {
    list.iter()
        .map(|b| {
            json!({
                "id": b.id, "name": b.name, "max_stacks": b.max_stacks, "kind": b.kind,
                "default_stacks": b.default_stacks, "default_locked": b.default_locked,
                "permanent": b.permanent,
            })
        })
        .collect()
}

// The build's resolved arcane fx (buff specs are policy-independent in shape);
// used for buff enumeration. `none` when the weapon can't equip arcanes.
fn arcane_fx_for(
    v: &Value,
    info: &WeaponInfo,
    base: &WeaponBase,
    policy: StackPolicy,
) -> wfsim_engine::arcanes_data::ArcaneFx {
    if !info.uses_arcane {
        return wfsim_engine::arcanes_data::ArcaneFx::none();
    }
    let aid = match get_str(v, "arcane", "none") {
        "enervate" => "secondary_enervate",
        "deadhead" => "secondary_deadhead",
        "flare" => "cascadia_flare",
        other => other,
    };
    match wfsim_engine::arcanes_data::secondary(aid) {
        Some(def) => {
            let rank = get_u32(v, "arcane_rank", def.max_rank).min(def.max_rank);
            def.fx(
                rank,
                policy,
                base.base_crit_chance,
                base.base_crit_damage,
                base.traits,
            )
        }
        None => wfsim_engine::arcanes_data::ArcaneFx::none(),
    }
}

// ---- per-buff configured policy ----------------------------------------
//
// The Sim panel's section 2: `buffs: { "<id>": { stacks, locked } }`. Present
// ⇒ the sim runs Emergent and each buff carries its own initial stacks + lock;
// absent ⇒ the legacy `assume_max`/`frenzy` knobs apply (byte-for-byte).
type BuffCfg = std::collections::HashMap<String, (u32, bool)>;

fn parse_buff_config(v: &Value) -> Option<BuffCfg> {
    let obj = v.get("buffs")?.as_object()?;
    let mut m = BuffCfg::new();
    for (id, cfg) in obj {
        let stacks = cfg.get("stacks").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        let locked = cfg.get("locked").and_then(|x| x.as_bool()).unwrap_or(false);
        m.insert(id.clone(), (stacks, locked));
    }
    Some(m)
}

/// Frenzy config → (passive active?, the buff-lock vector for a single form).
/// Locked = Permanent (100% uptime); unlocked+stacks = seed once then natural;
/// unlocked+0 = pure natural (no t=0 seed). The passive is always "present".
fn frenzy_apply(cfg: Option<&(u32, bool)>) -> (bool, Vec<BuffLock>) {
    match cfg {
        Some(&(_, true)) => (true, vec![BuffLock::permanent(LockedBuff::Frenzy)]),
        Some(&(stacks, false)) if stacks > 0 => {
            (true, vec![BuffLock::initial(LockedBuff::Frenzy, stacks)])
        }
        Some(&(_, false)) => (true, Vec::new()),
        None => (true, Vec::new()),
    }
}

/// The Frenzy lock mode for the incarnon cycle (baked at construction).
fn frenzy_lock_mode(cfg: Option<&(u32, bool)>) -> LockMode {
    match cfg {
        Some(&(_, true)) | None => LockMode::Permanent, // legacy cycle default
        Some(&(stacks, false)) => LockMode::Initial(stacks),
    }
}

// The per-buff config application lives in the engine (shared with the
// optimizer); `BuffCfg` is its `BuffConfig`.
// ---- /api/panel --------------------------------------------------------
//
// The FINAL stats panel: every bucket merged across the build, each stated
// with its per-source breakdown ("who contributed what") — the panel is where
// the model explains itself. Static view: max-rank mods; conditionals at max
// stacks (AssumedMax), except sentinels where conditionals never fire.

pub fn panel_json(v: &Value) -> Value {
    let info = weapon(get_str(v, "weapon", default_weapon_id()));
    let policy = if info.sentinel {
        StackPolicy::BaseOnly
    } else {
        StackPolicy::AssumedMax
    };
    // (`form` in the request is ignored: every available form renders.)
    let evos = match chosen_evolutions(v) {
        Ok(e) => e,
        Err(e) => return err_json(e),
    };
    let evo_refs: Vec<&str> = evos.iter().map(String::as_str).collect();

    let mod_ids: Vec<String> = v
        .get("mods")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    if mod_ids.len() > 9 {
        return err_json("at most 8 slots + 1 exilus");
    }
    let p = mod_pool_for(&info.mod_class);
    let mut refs: Vec<&ModDef> = Vec::with_capacity(mod_ids.len());
    for id in &mod_ids {
        match p.iter().find(|m| m.id == id) {
            Some(m) => refs.push(m),
            None => return err_json(format!("unknown mod id: {id}")),
        }
    }

    // ---- forms: EVERY available form renders side by side (no switching;
    // user decision). The Incarnon Form section exists only while its
    // tier-1 unlock is selected. `meta` states the trigger/shot mechanics
    // from the weapon data (data/weapons yamls).
    let mut forms_list: Vec<(&'static str, String, WeaponBase)> = Vec::new();
    forms_list.push((
        "Base Form",
        attack_desc(wspec(&info.id)),
        WeaponBase::from_data(&info.id, true, &evo_refs),
    ));
    if let Some(inc) = incarnon_id(info) {
        if form_unlock_evo(info).is_some_and(|u| evo_refs.contains(&u)) {
            forms_list.push((
                "Incarnon Form",
                attack_desc(wspec(inc)),
                WeaponBase::from_data(inc, true, &evo_refs),
            ));
        }
    }

    // ---- per-bucket source attribution (mirrors resolve()'s buckets) ----
    // key -> [(mod name, contribution fraction, note)]
    let mut src: Vec<(&'static str, String, f64, Option<String>)> = Vec::new();
    let mut conditionals: Vec<Value> = Vec::new(); // lines that never merge into a bucket
    for m in &refs {
        let name = prettify(m.id);
        for e in &m.effects {
            use ModEffect::*;
            let before = src.len();
            let mut push = |key: &'static str, v: f64, note: Option<String>| {
                src.push((key, name.clone(), v, note));
            };
            // An aim-gated effect still LISTS: the reader needs to see the
            // mod contributes, and under what condition. Unwrap it, let the
            // ordinary arms push as usual, then tag those rows below.
            let (e, aim_gated): (&ModEffect, bool) = match e {
                WhileAiming(inner) => (inner, true),
                other => (other, false),
            };
            match *e {
                WhileAiming(_) => unreachable!("unwrapped above"),
                BaseDamage(x) => push("base_damage", x, None),
                Multishot(x) => push("multishot", x, None),
                CritChance(x) => push("crit_chance", x, None),
                CritDamage(x) => push("crit_damage", x, None),
                StatusChance(x) => push("status_chance", x, None),
                StatusDamage(x) => push("status_damage", x, None),
                FireRate(x) => push("fire_rate", x, None),
                ReloadSpeed(x) => push("reload", x, None),
                Element(t, x) | CombinedElement(t, x) => {
                    src.push(("elements", name.clone(), x, Some(format!("{t:?}"))));
                }
                // Physical (IPS) mod: scales the base of that physical type.
                Physical(t, x) => {
                    src.push(("physical", name.clone(), x, Some(format!("{t:?}"))));
                }
                OnKillMultishot {
                    per_stack,
                    max_stacks,
                    ..
                } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "sentinel weapons cannot proc on-kill stacks"})),
                    _ => push(
                        "multishot",
                        per_stack * max_stacks as f64,
                        Some(format!("on kill, {max_stacks} stacks assumed")),
                    ),
                },
                ConditionOverload {
                    per_stack,
                    max_stacks,
                    ..
                } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "sentinel weapons cannot proc on-kill stacks"})),
                    _ => push(
                        "co",
                        per_stack * max_stacks as f64,
                        Some(format!(
                            "on kill, {max_stacks} stacks assumed, per status type on target"
                        )),
                    ),
                },
                OnHeadshotCritChance { bonus, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "sentinel weapons cannot headshot"})),
                    _ => push(
                        "crit_chance",
                        bonus,
                        Some("on headshot, buff assumed up".into()),
                    ),
                },
                OnHeadshotKillCritChance {
                    per_stack,
                    max_stacks,
                    ..
                } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "sentinel weapons cannot headshot"})),
                    _ => push(
                        "crit_chance",
                        per_stack * max_stacks as f64,
                        Some(format!("on headshot kill, {max_stacks} stacks assumed")),
                    ),
                },
                Indirect(stat, x) => {
                    src.push(("indirect", name.clone(), x, Some(stat.label().to_string())));
                }
                OnEquipHandling { .. } => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "temporary on weapon swap-in; never a static stat"})),
                // Faction bonus is conditional on the target's faction, so the
                // static panel lists it rather than folding it into a bucket.
                FactionDamage(fac, x) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": false,
                    "why": format!("+{}% total damage only vs {fac:?} (applied ×2 on DoT ticks)",
                        (x * 100.0).round())})),
                MagazineCapacity(x) => push("magazine", x, None),
                StatusDuration(x) => push("status_duration", x, None),
                // Conditional buff, assumed active at max in this static panel.
                CondBuff(b, x) => {
                    use wfsim_engine::loadout::CondBucket as B;
                    let key = match b {
                        B::BaseDamage => "base_damage",
                        B::Multishot => "multishot",
                        B::CritChance => "crit_chance",
                        B::CritDamage => "crit_damage",
                        B::StatusChance => "status_chance",
                        B::StatusDamage => "status_damage",
                        B::FireRate => "fire_rate",
                    };
                    push(key, x, Some("conditional buff, assumed active".into()));
                }
                // Weak-point effects: conditional on the part hit — listed,
                // never folded into a static bucket.
                WeakpointDamage(x) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": format!("+{}% added to the weak-point multiplier ON weak-point hits \
                        (1.5× listed on true weak points)", (x * 100.0).round())})),
                WeakpointCritChance(x) => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": format!("+{}% relative crit chance ON weak-point hits only",
                        (x * 100.0).round())})),
                OnKillCritDamage { bonus, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "sentinel weapons cannot proc on-kill buffs"})),
                    _ => push(
                        "crit_damage",
                        bonus,
                        Some("on kill, buff assumed up".into()),
                    ),
                },
                OnReloadFireRate { bonus, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "sentinel weapons cannot proc on-reload buffs"})),
                    _ => push(
                        "fire_rate",
                        bonus,
                        Some("on reload, buff assumed up".into()),
                    ),
                },
                // Event mechanic — no static stat; the sim rolls it per hit.
                ProcConversion { .. } => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "rolled per damage instance in the sim"})),
            }
            // Tag whatever the arms just pushed as aim-gated, so the panel
            // never shows a contribution without the condition that earns it.
            if aim_gated {
                for row in src.iter_mut().skip(before) {
                    row.3 = Some(match row.3.take() {
                        Some(t) => format!("{t}; while aiming"),
                        None => "while aiming".to_string(),
                    });
                }
            }
        }
    }
    // Non-mod sources: the CHOSEN evolutions (data-driven). Flat base
    // damage and flat base crit chance alter the WEAPON BASE before mods —
    // the stat rows show the raw base and attribute the delta here; the
    // multishot stacks and CO rate join their buckets. Broken evolutions
    // report zero via the accessors, so nothing is listed for them.
    // (key, source name, PRE-FORMATTED value, note)
    let mut evo_src: Vec<(&'static str, String, String, Option<String>)> = Vec::new();
    let (mut evo_flat_bd, mut evo_flat_cc) = (0.0f64, 0.0f64);
    if form_unlock_evo(info).is_some() {
        // Tiers are per weapon (adapters I-IV, Zariman weapons I-V), so the
        // numeral is BUILT, not indexed - a fixed table silently rendered the
        // Laetum's fifth tier as "EVO IV".
        let tiername = |t: u32| {
            let mut n = t;
            let mut out = String::from("EVO ");
            for (v, sym) in [(10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")] {
                while n >= v {
                    out.push_str(sym);
                    n -= v;
                }
            }
            out
        };
        for def in evo_refs
            .iter()
            .filter_map(|id| wfsim_engine::evolutions_data::get(id))
        {
            let name = format!("{} ({})", def.name, tiername(def.tier));
            let v = def.flat_base_damage();
            if v > 0.0 {
                evo_flat_bd += v;
                evo_src.push((
                    "base_damage",
                    name.clone(),
                    format!("+{v:.0} flat"),
                    Some("added to the weapon base pro-rata, before mods".into()),
                ));
            }
            let v = def.flat_base_crit_chance();
            if v > 0.0 {
                evo_flat_cc += v;
                evo_src.push((
                    "crit_chance",
                    name.clone(),
                    format!("+{:.0}% base", v * 100.0),
                    Some("into the BASE crit chance — crit mods multiply it".into()),
                ));
            }
            let v = def.assumed_multishot();
            if v > 0.0 {
                evo_src.push((
                    "multishot",
                    name.clone(),
                    fpct(v),
                    Some("on-ability-cast stacks, assumed full".into()),
                ));
            }
            let v = def.co_per_type();
            if v > 0.0 {
                evo_src.push((
                    "co",
                    name.clone(),
                    fpct(v),
                    Some("innate, per status type on target".into()),
                ));
            }
        }
    }

    // One stats section per form; the closure names its params `base` /
    // `panel` so every row reads the ACTIVE form's numbers.
    let section = |label: &'static str,
                   meta: &str,
                   base: &WeaponBase,
                   panel: &ResolvedPanel|
     -> Value {
        let sources = |key: &str, tag: Option<&str>| -> Vec<Value> {
            let evo = evo_src
                .iter()
                .filter(move |(k, _, _, _)| *k == key && tag.is_none())
                .map(|(_, name, v, note)| json!({ "mod": name, "value": v, "note": note }));
            evo.chain(
                src.iter()
                    .filter(|(k, _, _, note)| {
                        *k == key && tag.is_none_or(|t| note.as_deref() == Some(t))
                    })
                    .map(|(_, name, v, note)| {
                        json!({ "mod": name, "value": fpct(*v),
                            "note": if tag.is_some() { Value::Null } else { json!(note) } })
                    }),
            )
            .collect()
        };
        // ---- stat rows: base -> final, with the merged bonus and its sources ----
        let num = |x: f64| -> String {
            if x >= 100.0 {
                format!("{x:.0}")
            } else {
                format!("{x:.1}")
            }
        };
        let pc = |x: f64| format!("{:.1}%", x * 100.0);
        let mut stats = Vec::new();
        // Every base stat is ALWAYS listed (user: the panel must state the whole
        // base panel, not just what changed) — the UI drops the arrow when
        // base == final.
        let mut row = |key: &'static str, label: &str, base_s: String, final_s: String| {
            stats.push(
                json!({ "key": key, "label": label, "base": base_s, "final": final_s,
            "sources": sources(key, None) }),
            );
        };
        // Base columns show the RAW weapon base (pre-evolution): the evolution
        // flat deltas are attributed as named source rows, not hidden in "base".
        let raw_bd = base.base_vector.total() - evo_flat_bd;
        let raw_cc = base.base_crit_chance - evo_flat_cc;
        row(
            "base_damage",
            "Base Damage",
            num(raw_bd),
            num(panel.modified_base),
        );
        row(
            "multishot",
            "Multishot",
            format!("×{}", num(base.base_multishot)),
            format!("×{}", num(panel.multishot)),
        );
        row(
            "crit_chance",
            "Crit Chance",
            pc(raw_cc),
            pc(panel.crit_chance),
        );
        row(
            "crit_damage",
            "Crit Damage",
            format!("×{}", num(base.base_crit_damage)),
            format!("×{}", num(panel.crit_damage)),
        );
        row(
            "status_chance",
            "Status Chance",
            pc(base.base_status_chance),
            pc(panel.status_chance),
        );
        // Identical formatting on both sides — the UI drops the arrow only
        // when the strings match ("×1" vs "×1.0" must not differ).
        row(
            "status_damage",
            "Status Damage",
            format!("×{}", num(1.0)),
            format!("×{}", num(panel.status_damage_mult)),
        );
        row(
            "status_duration",
            "Status Duration",
            format!("×{}", num(1.0)),
            format!("×{}", num(panel.status_duration_mult)),
        );
        row(
            "fire_rate",
            "Fire Rate",
            format!("{}/s", num(base.base_fire_rate)),
            format!("{}/s", num(panel.fire_rate)),
        );
        // Incarnon form: the magazine is a charge-backed resource (Max Charges,
        // inert to magazine mods) and there is no reload — instead two transition
        // times, each scaled by the reload formula base/(1 + reload bonus).
        if let Some(inc) = base.incarnon {
            let rl = panel.reload_bonus;
            stats.push(json!({ "key": "magazine", "label": "Max Charges",
            "base": num(inc.max_charges), "final": num(inc.max_charges),
            "sources": json!([]) }));
            stats.push(json!({ "key": "transmute_in", "label": "Transmute In",
            "base": format!("{}s", num(inc.transmute_in)),
            "final": format!("{}s", num(inc.transmute_in / (1.0 + rl))),
            "sources": sources("reload", None) }));
            stats.push(json!({ "key": "transmute_out", "label": "Transmute Out",
            "base": format!("{}s", num(inc.transmute_out)),
            "final": format!("{}s", num(inc.transmute_out / (1.0 + rl))),
            "sources": sources("reload", None) }));
        } else {
            row(
                "magazine",
                "Magazine",
                num(base.magazine_size),
                num(panel.magazine_size),
            );
            row(
                "reload",
                "Reload",
                format!("{}s", num(base.base_reload)),
                format!("{}s", num(panel.reload_seconds)),
            );
        }
        // PER-WEAPON behavior: GunCO sources (Galvanized Shot, Carnage Reign,
        // Secondary Shiver) combine differently per weapon class, and their base
        // EXCLUDES evolution flat damage — this note states what the model
        // actually computes on THIS weapon, and is shared by every GunCO row.
        let behavior = match panel.co_behavior {
        wfsim_engine::loadout::CoBehavior::AdditiveWithBaseDamage =>
            "joins the base-damage bracket on this weapon (additive with Hornet Strike), direct hits only",
        wfsim_engine::loadout::CoBehavior::Independent =>
            "an independent multiplier on this weapon, direct hits only",
        wfsim_engine::loadout::CoBehavior::Inert =>
            "INERT on this weapon — the bonus does not apply",
    };
        let gunco_note = if (panel.co_base_fraction - 1.0).abs() > 1e-9 {
            format!(
            "computed on the ORIGINAL {:.0} base only — evolution flat damage is excluded ({:.0}% effectiveness); {behavior}",
            raw_bd,
            panel.co_base_fraction * 100.0
        )
        } else {
            behavior.to_string()
        };
        if panel.co_per_type > 0.0 {
            stats.push(json!({ "key": "co", "label": "Condition Overload",
            "base": "—", "final": format!("{} per status type on target", fpct(panel.co_per_type)),
            "note": gunco_note,
            "sources": sources("co", None) }));
        }

        // The equipped arcane on the panel: Secondary Shiver is a GunCO-family
        // source, so its row carries the SAME per-weapon caveat as the CO row.
        if info.uses_arcane {
            let aid = match get_str(v, "arcane", "none") {
                "enervate" => "secondary_enervate",
                "deadhead" => "secondary_deadhead",
                "flare" => "cascadia_flare",
                other => other,
            };
            if let Some(def) = wfsim_engine::arcanes_data::secondary(aid) {
                let rank = get_u32(v, "arcane_rank", def.max_rank).min(def.max_rank);
                let fx = def.fx(
                    rank,
                    policy,
                    base.base_crit_chance,
                    base.base_crit_damage,
                    base.traits,
                );
                if fx.per_cold_bd > 0.0 {
                    stats.push(json!({ "key": "shiver", "label": "Per Cold Status (Shiver)",
                    "base": "—",
                    "final": format!("{} damage per Cold status on target (cap {})",
                        fpct(fx.per_cold_bd), fx.cold_cap),
                    "note": format!("GunCO family — {gunco_note}"),
                    "sources": [json!({ "mod": format!("{} (arcane, rank {rank})", def.name),
                        "value": fpct(fx.per_cold_bd), "note": "per Cold stack; Frozen counts as the full 10" })] }));
                }
            }
        }

        // Elements: one row per contributed element (position/order matters for
        // combining — the damage section shows the combined result).
        let mut elem_rows = Vec::new();
        let mut seen_elems: Vec<String> = Vec::new();
        for (k, _, _, note) in &src {
            if *k == "elements" {
                if let Some(t) = note {
                    if !seen_elems.contains(t) {
                        seen_elems.push(t.clone());
                    }
                }
            }
        }
        for t in &seen_elems {
            let total: f64 = src
                .iter()
                .filter(|(k, _, _, n)| *k == "elements" && n.as_deref() == Some(t))
                .map(|(_, _, v, _)| v)
                .sum();
            elem_rows.push(json!({ "key": "elements", "label": t, "base": "—",
            "final": format!("{} of modified base", fpct(total)),
            "sources": sources("elements", Some(t)) }));
        }

        // Indirect stats (recoil, accuracy, ammo…): not in theoretical DPS,
        // real in practice; base is unmodified (0%), final = Σ.
        let mut indirect_rows = Vec::new();
        for (stat, total) in &panel.indirect {
            indirect_rows.push(
                json!({ "key": "indirect", "label": stat.label(), "base": "—",
            "final": fpct(*total), "sources": sources("indirect", Some(stat.label())) }),
            );
        }

        // A weapon is the GUN plus the PROJECTILE(s) it launches (user,
        // 2026-07-29): the gun carries cadence and capacity, each projectile
        // carries its own damage, crit, status — and, when it is a radial,
        // its blast geometry. Split the flat row list along that line
        // instead of stating a single "base attack" that belongs to neither.
        const ON_PROJECTILE: &[&str] = &[
            "base_damage",
            "crit_chance",
            "crit_damage",
            "status_chance",
            "status_damage",
            "status_duration",
            "co",
            "shiver",
        ];
        let key_of = |r: &Value| r["key"].as_str().unwrap_or("").to_string();
        let (direct_rows, weapon_rows): (Vec<Value>, Vec<Value>) = stats
            .into_iter()
            .partition(|r| ON_PROJECTILE.contains(&key_of(r).as_str()));

        // A damage vector as displayed rows: type, amount, share of the total.
        let vector_rows = |v: &wfsim_engine::damage::DamageVector| {
            let total = v.total();
            v.iter_nonzero()
                .map(|(t, amt)| {
                    json!({ "type": format!("{t:?}"), "amount": num(amt),
                    "share": format!("{:.0}%", amt / total * 100.0) })
                })
                .collect::<Vec<Value>>()
        };

        let mut parts = vec![json!({
            "id": "direct",
            "label": "Direct hit",
            "meta": "on contact",
            "stats": direct_rows,
            "damage": vector_rows(&panel.damage),
            "damage_total": num(panel.damage.total()),
        })];

        // The radial explosion is a SECOND projectile-borne damage instance
        // with its own crit and status (MECHANICS §7) — the panel states it
        // in full rather than leaving the reader to assume it copies the
        // direct hit. Status damage/duration are weapon-wide multipliers, so
        // they repeat: they describe the procs THIS instance applies.
        if let (Some(rb), Some(rr)) = (base.radial.as_ref(), panel.radial.as_ref()) {
            let rsrc = |key: &'static str| sources(key, None);
            // Geometry reads as a distance, not a stat: 2 m, not 2.0.
            let dist = |x: f64| format!("{x}");
            let mut rows = vec![
                json!({ "key": "base_damage", "label": "Base Damage",
                    "base": num(rb.base_vector.total()), "final": num(rr.modified_base),
                    "sources": rsrc("base_damage") }),
                json!({ "key": "crit_chance", "label": "Crit Chance",
                    "base": pc(rb.base_crit_chance), "final": pc(rr.crit_chance),
                    "sources": rsrc("crit_chance") }),
                json!({ "key": "crit_damage", "label": "Crit Damage",
                    "base": format!("×{}", num(rb.base_crit_damage)),
                    "final": format!("×{}", num(rr.crit_damage)),
                    "sources": rsrc("crit_damage") }),
                json!({ "key": "status_chance", "label": "Status Chance",
                    "base": pc(rb.base_status_chance), "final": pc(rr.status_chance),
                    "sources": rsrc("status_chance") }),
                json!({ "key": "status_damage", "label": "Status Damage",
                    "base": format!("×{}", num(1.0)),
                    "final": format!("×{}", num(panel.status_damage_mult)),
                    "sources": rsrc("status_damage") }),
                json!({ "key": "status_duration", "label": "Status Duration",
                    "base": format!("×{}", num(1.0)),
                    "final": format!("×{}", num(panel.status_duration_mult)),
                    "sources": rsrc("status_duration") }),
                json!({ "key": "radius", "label": "Blast Radius", "base": "—",
                    "final": format!("{} m", dist(rr.radius_m)), "sources": json!([]) }),
            ];
            // Falloff: full damage inside `start`, then linear down to
            // (1 − reduction) at the rim. Stated as what the rim actually
            // takes, which is the number a reader can act on.
            rows.push(json!({ "key": "falloff", "label": "Damage Falloff", "base": "—",
                "final": format!("{}% at {} m", dist((1.0 - rr.falloff_reduction) * 100.0),
                    dist(rr.radius_m)),
                "note": if rr.falloff_start_m > 0.0 {
                    format!("full damage within {} m, then linear", dist(rr.falloff_start_m))
                } else {
                    "linear from the epicentre; a directly-hit enemy takes 100%".to_string()
                },
                "sources": json!([]) }));
            parts.push(json!({
                "id": "radial",
                "label": "Radial explosion",
                "meta": format!("{} m radius", dist(rr.radius_m)),
                "stats": rows,
                "damage": vector_rows(&rr.damage),
                "damage_total": num(rr.damage.total()),
            }));
        }

        json!({
            "label": label,
            "meta": meta,
            "stats": weapon_rows,
            "elements": elem_rows,
            "indirect": indirect_rows,
            "parts": parts,
        })
    };

    let forms: Vec<Value> = forms_list
        .iter()
        .map(|(label, meta, b)| section(label, meta, b, &resolve(b, &refs, policy)))
        .collect();

    // Configurable buffs of this build (weapon-scoped) for the Sim panel —
    // mods + arcane + the weapon passive, plus evolution-granted buffs
    // (Fevered Frenzy's permanent stacks).
    let arcane_fx = arcane_fx_for(v, info, &forms_list[0].2, policy);
    let mut buffs = enumerate_buffs(&refs, &arcane_fx, info);
    for b in evo_buffs(&evos) {
        if !buffs.iter().any(|x| x.id == b.id) {
            buffs.push(b);
        }
    }

    json!({
        "ok": true,
        "weapon": info.name,
        "policy": if info.sentinel { "base only (sentinel)" } else { "conditionals at max stacks" },
        "forms": forms,
        "conditionals": conditionals,
        "buffs": buffs_json(&buffs),
    })
}

fn build_body_parts(spec: &EnemySpec, headshot_pct: f64) -> Vec<BodyPart> {
    let h = (headshot_pct / 100.0).clamp(0.0, 1.0);
    let heads: Vec<_> = spec.body_parts.iter().filter(|p| p.is_head).collect();
    let bodies: Vec<_> = spec.body_parts.iter().filter(|p| !p.is_head).collect();

    let make = |b: &wfsim_engine::enemy_data::BodyPartSpec, w: f64| BodyPart {
        name: b.name.clone(),
        aim_weight: w,
        multiplier: b.multiplier,
        is_head: b.is_head,
        crit_bonus: b.crit_bonus,
    };

    let mut out = Vec::new();
    match (heads.is_empty(), bodies.is_empty()) {
        (true, _) => {
            // No head part: spread all aim across the body parts.
            let w = 1.0 / spec.body_parts.len() as f64;
            for b in &spec.body_parts {
                out.push(make(b, w));
            }
        }
        (false, true) => {
            // Only head part(s): all aim on the head(s).
            let w = 1.0 / heads.len() as f64;
            for b in &heads {
                out.push(make(b, w));
            }
        }
        (false, false) => {
            let hw = h / heads.len() as f64;
            let bw = (1.0 - h) / bodies.len() as f64;
            for b in &heads {
                out.push(make(b, hw));
            }
            for b in &bodies {
                out.push(make(b, bw));
            }
        }
    }
    out
}

/// The chosen evolution set: `evolutions` (an array of data ids; ABSENT
/// entries = empty tier — nothing installed) wins; a legacy `evo2` string
/// (short names accepted) maps to the historical default trio.
fn chosen_evolutions(v: &Value) -> Result<Vec<String>, String> {
    if let Some(arr) = v.get("evolutions").and_then(|x| x.as_array()) {
        let ids: Vec<String> = arr
            .iter()
            .filter_map(|s| s.as_str())
            .filter(|s| !s.is_empty() && *s != "none")
            .map(String::from)
            .collect();
        for id in &ids {
            if wfsim_engine::evolutions_data::get(id).is_none() {
                return Err(format!("unknown evolution id: {id}"));
            }
        }
        return Ok(ids);
    }
    let evo2 = match get_str(v, "evo2", "dual_toxocyst_fevered_frenzy") {
        "carnage" | "dual_toxocyst_carnage_reign" => "dual_toxocyst_carnage_reign",
        _ => "dual_toxocyst_fevered_frenzy",
    };
    Ok(vec![
        "dual_toxocyst_commodores_fortune".to_string(),
        "dual_toxocyst_evolved_autoloader".to_string(),
        evo2.to_string(),
    ])
}

pub fn simulate_json(v: &Value) -> Value {
    // ---- parse inputs ----
    let info = weapon(get_str(v, "weapon", default_weapon_id()));
    // Per-buff configured policy (Sim panel section 2). Present ⇒ Emergent sim
    // with each buff carrying its own initial stacks + lock. Absent ⇒ the
    // legacy `assume_max`/`frenzy` knobs (byte-for-byte with the old path).
    let buff_cfg = parse_buff_config(v);
    let assume_max = get_bool(v, "assume_max", false);
    let policy = if info.sentinel {
        StackPolicy::BaseOnly
    } else if buff_cfg.is_some() {
        // Configured: run Emergent; per-buff `pinned`/`initial_stacks` are
        // honored at the sim's read sites.
        StackPolicy::Emergent
    } else if assume_max {
        StackPolicy::AssumedMax
    } else {
        StackPolicy::Emergent
    };
    // Frenzy weapon passive. Configured ⇒ from the buff config; legacy ⇒ the
    // `frenzy` on/off knob. Cycle bakes the LockMode at construction; single
    // forms take (active?, lock vector).
    let frenzy_on = get_bool(v, "frenzy", true);
    let frenzy_cfg = buff_cfg.as_ref().and_then(|m| m.get("frenzy"));
    let cycle_frenzy_lock = if buff_cfg.is_some() {
        frenzy_lock_mode(frenzy_cfg)
    } else {
        LockMode::Permanent // legacy cycle default
    };
    let (frenzy_single, frenzy_locks) = if buff_cfg.is_some() {
        frenzy_apply(frenzy_cfg)
    } else {
        (frenzy_on, Vec::new()) // legacy single-form: on/off, natural triggering
    };
    // The passive belongs to the WEAPON: a request can only turn Frenzy off
    // or configure it, never grant it to a weapon that does not list the
    // perk. Without this the Laetum inherited Dual Toxocyst's ×2.5 fire rate.
    let has_frenzy = wfsim_engine::weapons_data::has_perk(&info.id, "frenzy")
        || incarnon_id(info).is_some_and(|i| wfsim_engine::weapons_data::has_perk(i, "frenzy"));
    // One value for all three forms: the weapon must OWN the passive, and
    // the request may still switch it off (or configure it via buff_cfg).
    // The cycle used to ignore this entirely — its knob was dead.
    let frenzy_single = frenzy_single && has_frenzy;
    let form = get_str(v, "form", "incarnon_cycle");
    let evos = match chosen_evolutions(v) {
        Ok(e) => e,
        Err(e) => return err_json(e),
    };
    let evo_refs: Vec<&str> = evos.iter().map(String::as_str).collect();
    // No Incarnon Form unlock (tier 1) in an explicit selection = the weapon
    // cannot transform: honest fallback to the base form.
    let unlock = form_unlock_evo(info);
    let form =
        if v.get("evolutions").is_some() && unlock.is_some_and(|u| !evos.iter().any(|e| e == u)) {
            "base"
        } else {
            form
        };
    // Arcane: a data-driven pool id (legacy short names accepted for old
    // saved builds) + optional `arcane_rank` (default: max).
    let arcane_id = if info.uses_arcane {
        match get_str(v, "arcane", "secondary_deadhead") {
            "enervate" => "secondary_enervate",
            "deadhead" => "secondary_deadhead",
            "flare" => "cascadia_flare",
            other => other,
        }
        .to_string()
    } else {
        "none".to_string() // sentinels / robotic weapons cannot equip arcanes
    };
    let enemy_id = get_str(v, "enemy", "thrax_centurion");
    let level = get_u32(v, "level", 9999).clamp(1, 9999);
    let steel_path = get_bool(v, "steel_path", true);
    let headshot_pct = get_f64(v, "headshot_pct", 100.0);
    // Is the player HOLDING AIM? Gates the `while_aiming` mod effects
    // (Galvanized Crosshairs / Scope, Argon Scope, Sharpened Bullets, …).
    // Defaults TRUE, which is what the sim silently assumed before this
    // existed — so no stored preset changes meaning.
    let aiming = get_bool(v, "aiming", true);
    let duration = get_f64(v, "duration", 120.0).clamp(1.0, 3600.0);
    let runs = get_u32(v, "runs", 300).clamp(1, 20_000);
    let seed = v.get("seed").and_then(|x| x.as_u64()).unwrap_or(0xC0FFEE);

    let mod_ids: Vec<String> = v
        .get("mods")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // No count validation here (user, 2026-07-28): the sim runs whatever it
    // is given — slot legality (8 main + 1 exilus) is the UI's job, and the
    // engine resolves any mod list honestly.

    // ---- resolve mods against the weapon's pool (honoring the given order) ----
    let p = mod_pool_for(&info.mod_class);
    let mut refs: Vec<&ModDef> = Vec::with_capacity(mod_ids.len());
    for id in &mod_ids {
        match p.iter().find(|m| m.id == id) {
            Some(m) => refs.push(m),
            None => return err_json(format!("unknown mod id: {id}")),
        }
    }
    // Reject family collisions (wiki Incompatible mods).
    for i in 0..refs.len() {
        for j in (i + 1)..refs.len() {
            if let (Some(fi), Some(fj)) = (refs[i].family, refs[j].family) {
                if fi == fj {
                    return err_json(format!(
                        "{} and {} are incompatible (both in the {fi} family)",
                        refs[i].id, refs[j].id
                    ));
                }
            }
        }
    }

    // ---- enemy / target ----
    let specs = enemies();
    let Some(spec) = specs.iter().find(|e| e.id == enemy_id) else {
        return err_json(format!("unknown enemy: {enemy_id}"));
    };
    let target = match spec.target_params(level, steel_path, false, TargetMode::InstantRespawn) {
        Ok(t) => t,
        Err(e) => return err_json(e),
    };
    let (og, sh, hp, ar) = (
        target.overguard(),
        target.max_shield(),
        target.max_health(),
        target.armor(),
    );
    let body_parts = build_body_parts(spec, headshot_pct);

    // ---- forma legality (order-independent; needs only the mod multiset) ----
    let planned: Vec<PlannedMod> = refs
        .iter()
        .map(|m| PlannedMod {
            base_drain: m.base_drain,
            polarity: m.polarity,
        })
        .collect();
    let forma = match plan_forma(60, &innate_slots_for(&info.id), &planned) {
        Ok(fp) => json!({
            "legal": true,
            "used": fp.forma_used,
            "total_drain": fp.total_drain,
            "cap": 60,
        }),
        Err(e) => json!({ "legal": false, "error": e, "cap": 60 }),
    };

    // ---- resolve panel(s) and build sim params, per weapon ----
    // Dual Toxocyst: two forms + the real Incarnon cycle.
    let (report_panel, mut params): (ResolvedPanel, DummyParams) = {
        let incarnon_base =
            WeaponBase::from_data(incarnon_id(info).unwrap_or(&info.id), true, &evo_refs);
        let base_base = WeaponBase::from_data(&info.id, true, &evo_refs);
        let incarnon_panel = resolve_with(&incarnon_base, &refs, policy, aiming);
        let base_panel = resolve_with(&base_base, &refs, policy, aiming);
        let report = if form == "base" {
            base_panel.clone()
        } else {
            incarnon_panel.clone()
        };
        let params = match form {
            "base" => {
                let mut d = DummyParams::from_panel(&base_panel, target, body_parts, duration);
                d.frenzy = frenzy_single; // base-form Frenzy passive (×2.5 on true headshots)
                d.locked_buffs = frenzy_locks.clone();
                d
            }
            "incarnon" => {
                let mut d = DummyParams::from_panel(&incarnon_panel, target, body_parts, duration);
                d.frenzy = frenzy_single; // Frenzy persists in the Incarnon form (user-confirmed)
                d.locked_buffs = frenzy_locks.clone();
                d
            }
            _ => DummyParams::incarnon_cycle_from_panels(
                &incarnon_panel,
                &base_panel,
                frenzy_single,
                cycle_frenzy_lock,
                target,
                body_parts,
                duration,
            ),
        };
        (report, params)
    };
    params.arcane = if arcane_id == "none" {
        wfsim_engine::arcanes_data::ArcaneFx::none()
    } else {
        let Some(def) = wfsim_engine::arcanes_data::secondary(&arcane_id) else {
            return err_json(format!("unknown arcane id: {arcane_id}"));
        };
        let rank = get_u32(v, "arcane_rank", def.max_rank).min(def.max_rank);
        // Relative crit conditionals resolve against the weapon's BASE crit
        // stats; `requires` gates on the weapon traits (Akimbo Slip Shot).
        // Under the sim's Emergent policy the non-simmable conditionals are
        // honest no-ops (same rule as mods' CondBuff).
        let ab = WeaponBase::from_data(incarnon_id(info).unwrap_or(&info.id), true, &evo_refs);
        def.fx(
            rank,
            policy,
            ab.base_crit_chance,
            ab.base_crit_damage,
            ab.traits,
        )
    };
    // ---- apply the per-buff configured policy onto the live specs ----
    // (weapon-scoped: recurses into the incarnon cycle's base form). Frenzy is
    // already applied above (cycle lock at construction / single-form vector).
    if let Some(cfg) = &buff_cfg {
        params.apply_buff_config(cfg);
    }
    let report_panel = &report_panel;

    // ---- run ----
    let s = monte_carlo(&params, runs, seed);

    let damage: Vec<Value> = report_panel
        .damage
        .iter_nonzero()
        .map(|(t, val)| json!({ "type": format!("{t:?}"), "value": val }))
        .collect();

    // EVERY displayed number comes from the MEDIAN engagement (user,
    // 2026-07-29: one internally consistent run — the meter, the curve,
    // the kills and the handling stats all line up). The cross-run
    // spread (min–max ±σ) stays as explicit spread stats.
    let m = &s.median_run;
    const TYPE_NAMES: [&str; 15] = [
        "Impact",
        "Puncture",
        "Slash",
        "Cold",
        "Electricity",
        "Heat",
        "Toxin",
        "Blast",
        "Corrosive",
        "Gas",
        "Magnetic",
        "Radiation",
        "Viral",
        "True",
        "Void",
    ];
    let sd = &m.sources;
    let mut sources: Vec<(String, f64)> = vec![
        ("direct".to_string(), sd.direct),
        ("radial".to_string(), sd.radial),
        ("arcane".to_string(), sd.arcane_on_status),
    ];
    sources.extend(
        sd.status
            .iter()
            .enumerate()
            .map(|(i, &v)| (TYPE_NAMES[i].to_string(), v)),
    );
    sources.retain(|(_, v)| *v > 0.0);
    sources.sort_by(|a, b| b.1.total_cmp(&a.1));
    let damage_sources: Vec<Value> = sources
        .iter()
        .map(|(k, v)| json!({ "source": k, "dmg": v }))
        .collect();
    // One-second buckets, sliced to the engagement's actual duration.
    let nb = (s.duration_secs.ceil() as usize).clamp(1, m.timeline.0.len());
    let pel = m.pellets.max(1) as f64;

    json!({
        "ok": true,
        "score": m.kill_progress,
        "kills": m.kills,
        "kills_std": s.std_kills,
        "kills_min": s.min_kills,
        "kills_max": s.max_kills,
        "dps": m.effective_damage / s.duration_secs.max(1e-9),
        "shots": m.shots,
        "pellets": m.pellets,
        "crit_rate": m.crits as f64 / pel,
        "big_crit_rate": m.big_crits as f64 / pel,
        "headshot_rate": m.headshots as f64 / pel,
        "procs": m.procs,
        "damage_sources": damage_sources,
        "timeline": m.timeline.0[..nb].to_vec(),
        "transforms": m.transforms,
        "reloads": m.reloads,
        "duration": s.duration_secs,
        "runs": s.runs,
        "panel": {
            "damage": damage,
            "total": report_panel.damage.total(),
            "crit_chance": report_panel.crit_chance,
            "crit_damage": report_panel.crit_damage,
            "status_chance": report_panel.status_chance,
            "fire_rate": report_panel.fire_rate,
            "multishot": report_panel.multishot,
            "modified_base": report_panel.modified_base,
            "co_per_type": report_panel.co_per_type,
        },
        "forma": forma,
        "target": {
            "name": s_name(&specs, enemy_id),
            "level": level,
            "steel_path": steel_path,
            "overguard": og,
            "shield": sh,
            "health": hp,
            "armor": ar,
        },
    })
}

fn s_name(specs: &[EnemySpec], id: &str) -> String {
    specs
        .iter()
        .find(|e| e.id == id)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| id.to_string())
}
// All buffs the scope could produce (union over every fixed/search mod + every
// searched arcane + the weapon passive) — the optimizer's buff panel enumerates
// over the WHOLE scope, not one build. `apply_buff_config` applies each per
// candidate where present.
pub fn opt_buffs_json(v: &Value) -> Value {
    let info = weapon(get_str(v, "weapon", default_weapon_id()));
    fn merge(out: &mut Vec<BuffMeta>, list: Vec<BuffMeta>) {
        for b in list {
            if !out.iter().any(|x| x.id == b.id) {
                out.push(b);
            }
        }
    }
    let mut ids: Vec<String> = Vec::new();
    if let Some(obj) = v.get("mods").and_then(|x| x.as_object()) {
        for (id, st) in obj {
            if matches!(st.as_str(), Some("fixed") | Some("search")) {
                ids.push(id.clone());
            }
        }
    }
    ids.sort();
    ids.dedup();
    let full = mod_pool_for(&info.mod_class);
    let refs: Vec<&ModDef> = full
        .iter()
        .filter(|m| ids.iter().any(|id| id.as_str() == m.id))
        .collect();
    let mut out: Vec<BuffMeta> = Vec::new();
    let none = wfsim_engine::arcanes_data::ArcaneFx::none();
    merge(&mut out, enumerate_buffs(&refs, &none, info));
    let arc_base = WeaponBase::from_data(&info.id, true, &[]);
    if let Some(arr) = v.get("arcanes").and_then(|x| x.as_array()) {
        for a in arr.iter().filter_map(|x| x.as_str()) {
            if a == "none" {
                continue;
            }
            if let Some(def) = wfsim_engine::arcanes_data::secondary(a) {
                let fx = def.fx(
                    def.max_rank,
                    StackPolicy::Emergent,
                    arc_base.base_crit_chance,
                    arc_base.base_crit_damage,
                    arc_base.traits,
                );
                merge(&mut out, enumerate_buffs(&[], &fx, info));
            }
        }
    }
    // Evolution-granted buffs across the scope (every tier option listed —
    // Fevered Frenzy's permanent stacks show whenever it could be searched).
    if let Some(obj) = v.get("evolutions").and_then(|x| x.as_object()) {
        let evo_ids: Vec<String> = obj
            .values()
            .filter_map(|a| a.as_array())
            .flatten()
            .filter_map(|x| x.as_str().map(String::from))
            .collect();
        merge(&mut out, evo_buffs(&evo_ids));
    }
    json!({ "ok": true, "buffs": buffs_json(&out) })
}

// ---- /api/optimize -----------------------------------------------------
//
// Scoped-subset search (devlog thread #4): the user fixes some mods/arcanes/
// evolutions, opens others to SEARCH, and gets the top-10 builds ranked by
// kills-in-duration. Reuses the optimizer lib's `enumerate_candidates` +
// `run_funnel`, with the same per-buff configured policy as the Sim panel.
// No cap (user directive): the funnel's cheap early rounds cull the space.
//
// Transport-independent split: [`parse_optimize`] validates the request
// synchronously (bad input still fails fast) into an [`OptimizePlan`];
// [`run_optimize`] does the heavy work — enumerate + funnel — publishing
// live progress through the caller's `FunnelState` and honoring its
// `cancel` flag. The native server wraps this pair in a background-job
// registry; the wasm build runs it inside a Web Worker.

/// Everything the heavy phase needs, validated up front.
pub struct OptimizePlan {
    weapon_id: String,
    pool: Vec<ModDef>,
    constraints: Constraints,
    min_slots: usize,
    build_size: usize,
    evo_sets: Vec<Vec<String>>,
    exilus_defs: Vec<Option<ModDef>>,
    arcanes: Vec<wfsim_engine::arcanes_data::ArcaneFx>,
    scenario: Scenario,
    final_runs: u32,
    finalists: usize,
    headshot_pct: f64,
    duration: f64,
    target_name: String,
    level: u32,
    steel_path: bool,
    /// Worker-thread budget; 0 = auto (all cores minus two).
    threads: usize,
}

/// Validate an optimize request. `Err` is the ready-to-send error response.
pub fn parse_optimize(v: &Value) -> Result<OptimizePlan, Value> {
    let info = weapon(get_str(v, "weapon", default_weapon_id()));
    if incarnon_id(info).is_none() {
        return Err(err_json(
            "the optimizer needs a transform-group weapon (v1)",
        ));
    }
    // ---- mod scope (MAIN 8 slots): fixed ∪ search = pool; fixed = required.
    // Exilus-flagged mods MAY appear here too — all 9 slots accept them
    // (game rule), so putting one in the main scope makes it compete for a
    // main slot like any other mod.
    let mut fixed_ids: Vec<String> = Vec::new();
    let mut search_ids: Vec<String> = Vec::new();
    if let Some(obj) = v.get("mods").and_then(|x| x.as_object()) {
        for (id, st) in obj {
            match st.as_str() {
                Some("fixed") => fixed_ids.push(id.clone()),
                Some("search") => search_ids.push(id.clone()),
                _ => {}
            }
        }
    }
    fixed_ids.sort();
    fixed_ids.dedup();
    search_ids.retain(|s| !fixed_ids.contains(s)); // fixed wins over search
    let full = mod_pool_for(&info.mod_class);
    for id in fixed_ids.iter().chain(search_ids.iter()) {
        if !full.iter().any(|m| m.id == id.as_str()) {
            return Err(err_json(format!("unknown mod id: {id}")));
        }
    }

    // ---- exilus scope (the +1 slot, exilus-eligible mods only): its own
    // block. "search" entries are slot OPTIONS alongside "leave empty"; a
    // "fixed" one pins the slot (max one — there is only one exilus slot).
    // Absent/empty = the slot stays empty. A mod listed in BOTH scopes is
    // fine unless double-required: enumeration never equips it twice (the
    // exilus option is skipped for subsets that already contain it).
    let mut ex_fixed: Vec<String> = Vec::new();
    let mut ex_search: Vec<String> = Vec::new();
    if let Some(obj) = v.get("exilus").and_then(|x| x.as_object()) {
        for (id, st) in obj {
            match st.as_str() {
                Some("fixed") => ex_fixed.push(id.clone()),
                Some("search") => ex_search.push(id.clone()),
                _ => {}
            }
        }
    }
    ex_fixed.sort();
    ex_fixed.dedup();
    ex_search.retain(|s| !ex_fixed.contains(s));
    // "none" is a first-class option id: pool it to keep "leave empty" among
    // the searched options, req it to pin the slot empty.
    for id in ex_fixed
        .iter()
        .chain(ex_search.iter())
        .filter(|id| id.as_str() != "none")
    {
        let Some(m) = full.iter().find(|m| m.id == id.as_str()) else {
            return Err(err_json(format!("unknown exilus mod id: {id}")));
        };
        if !m.exilus {
            return Err(err_json(format!("{id} is not exilus-eligible")));
        }
    }
    if ex_fixed.len() > 1 {
        return Err(err_json(format!(
            "only one exilus slot — {} cannot all be required",
            ex_fixed.join(", ")
        )));
    }
    if let Some(f) = ex_fixed.first() {
        if fixed_ids.contains(f) {
            return Err(err_json(format!(
                "{f} is required in both a main slot and the exilus slot — a mod equips once"
            )));
        }
    }
    // Marked pools OCCUPY the slot (same rule as the main block's pool
    // group): the empty option exists only when NOTHING is marked or when
    // "none" itself is pooled/req'd — never implicitly next to pooled mods.
    let exilus_ids: Vec<Option<String>> = match ex_fixed.first() {
        Some(f) if f == "none" => vec![None],
        Some(f) => vec![Some(f.clone())],
        None if ex_search.is_empty() => vec![None],
        None => ex_search
            .iter()
            .map(|id| if id == "none" { None } else { Some(id.clone()) })
            .collect(),
    };
    let exilus_defs: Vec<Option<ModDef>> = exilus_ids
        .iter()
        .map(|o| {
            o.as_ref()
                .and_then(|id| full.iter().find(|m| m.id == id.as_str()).cloned())
        })
        .collect();

    // The MAXIMUM main slots a build may fill (1..=8; the exilus slot is the
    // +1 on top). Slots may stay empty — sizes 0..=build_size all enumerate,
    // so a scope smaller than the cap (even zero mods) is legal.
    let build_size = get_u32(v, "build_size", 8).clamp(1, 8) as usize;
    let mut pool_ids: Vec<String> = fixed_ids.iter().chain(search_ids.iter()).cloned().collect();
    pool_ids.sort();
    pool_ids.dedup();
    if fixed_ids.len() > build_size {
        return Err(err_json(format!(
            "more required mods ({}) than build slots ({build_size})",
            fixed_ids.len()
        )));
    }
    // The pool GROUP occupies ≥1 slot whenever anything is pooled — every
    // searched build then uses at least one pooled mod (mark no pools for an
    // exactly-required build). Hence required can fill at most size−1 slots
    // while pools exist, and enumeration starts above the required count.
    if !search_ids.is_empty() && fixed_ids.len() >= build_size {
        return Err(err_json(format!(
            "pooled mods occupy at least one of the {build_size} slots — required ({}) leaves none",
            fixed_ids.len()
        )));
    }
    let min_slots = fixed_ids.len() + usize::from(!search_ids.is_empty());
    let pool: Vec<ModDef> = full
        .iter()
        .filter(|m| pool_ids.iter().any(|id| id.as_str() == m.id))
        .cloned()
        .collect();
    let constraints = Constraints {
        require: fixed_ids.clone(),
        forbid: Vec::new(),
    };

    // ---- evolution scope: per-tier options → the Cartesian product ----
    // The tier COUNT is per weapon (DT 4, Laetum 5) — read it from the data.
    let evo_req = v.get("evolutions").and_then(|x| x.as_object());
    let mut evo_sets: Vec<Vec<String>> = vec![Vec::new()];
    let evo_tiers = wfsim_engine::evolutions_data::tier_count(
        wspec(&info.id).transform_group.as_deref().unwrap_or(&info.id),
    );
    for tier in 1u32..=evo_tiers {
        let opts: Vec<Option<String>> = evo_req
            .and_then(|o| o.get(&tier.to_string()))
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(|s| Some(s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let picks = if opts.is_empty() { vec![None] } else { opts }; // empty = nothing at this tier
        let mut next = Vec::new();
        for base in &evo_sets {
            for pick in &picks {
                let mut e = base.clone();
                if let Some(id) = pick {
                    e.push(id.clone());
                }
                next.push(e);
            }
        }
        evo_sets = next;
    }
    for set in &evo_sets {
        for id in set {
            if wfsim_engine::evolutions_data::get(id).is_none() {
                return Err(err_json(format!("unknown evolution id: {id}")));
            }
        }
    }

    // ---- arcane scope ----
    let arc_ids: Vec<String> = v
        .get("arcanes")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_else(|| vec!["none".into()]);
    let arc_base = WeaponBase::from_data(&info.id, true, &[]);
    let arcanes: Vec<wfsim_engine::arcanes_data::ArcaneFx> = arc_ids
        .iter()
        .map(|id| {
            if id == "none" {
                wfsim_engine::arcanes_data::ArcaneFx::none()
            } else {
                match wfsim_engine::arcanes_data::secondary(id) {
                    Some(def) => def.fx(
                        def.max_rank,
                        StackPolicy::Emergent,
                        arc_base.base_crit_chance,
                        arc_base.base_crit_damage,
                        arc_base.traits,
                    ),
                    None => wfsim_engine::arcanes_data::ArcaneFx::none(),
                }
            }
        })
        .collect();
    if arcanes.is_empty() {
        return Err(err_json("no arcanes selected"));
    }

    // No cap (user: allow spending local resources). The funnel handles large
    // spaces by culling obviously-bad combos in cheap early rounds.

    // ---- final-round contract (user, 2026-07-28): the last round is
    // guaranteed `finalists` candidates × `final_runs` runs; everything
    // before only whittles the field down (schedule + adaptive racing).
    let final_runs = get_u32(v, "final_runs", 1024).clamp(1, 100_000);
    let finalists = get_u32(v, "finalists", 20).clamp(1, 100) as usize;

    // ---- scenario (reuse the Sim inputs) ----
    let enemy_id = get_str(v, "enemy", "thrax_centurion");
    let level = get_u32(v, "level", 9999).clamp(1, 9999);
    let steel_path = get_bool(v, "steel_path", true);
    let headshot_pct = get_f64(v, "headshot_pct", 100.0);
    // Same scenario knob as the Sim: the optimizer must score builds under the
    // assumption the sim will replay them with, or the winner is scored on a
    // buff the replay never grants.
    let aiming = get_bool(v, "aiming", true);
    let duration = get_f64(v, "duration", 120.0).clamp(1.0, 3600.0);
    let specs = enemies();
    let Some(spec) = specs.iter().find(|e| e.id == enemy_id) else {
        return Err(err_json(format!("unknown enemy: {enemy_id}")));
    };
    let target = match spec.target_params(level, steel_path, false, TargetMode::InstantRespawn) {
        Ok(t) => t,
        Err(e) => return Err(err_json(e)),
    };
    let body_parts = build_body_parts(spec, headshot_pct);
    let buff_cfg = parse_buff_config(v).unwrap_or_default();
    let frenzy_lock = frenzy_lock_mode(buff_cfg.get("frenzy"));
    // Frenzy is the weapon's own perk — the optimizer must not hand it to a
    // weapon that lacks it any more than the sim does.
    let frenzy = wfsim_engine::weapons_data::has_perk(&info.id, "frenzy")
        || incarnon_id(info).is_some_and(|i| wfsim_engine::weapons_data::has_perk(i, "frenzy"));
    let scenario = Scenario {
        aiming,
        target,
        body_parts,
        frenzy,
        duration_secs: duration,
        incarnon_cycle: true,
        frenzy_lock,
        buff_cfg,
    };

    Ok(OptimizePlan {
        weapon_id: info.id.clone(),
        pool,
        constraints,
        min_slots,
        build_size,
        evo_sets,
        exilus_defs,
        arcanes,
        scenario,
        final_runs,
        finalists,
        headshot_pct,
        duration,
        target_name: s_name(&specs, enemy_id),
        level,
        steel_path,
        threads: v
            .get("threads")
            .and_then(|x| x.as_u64())
            .unwrap_or(0)
            .min(256) as usize,
    })
}

/// The heavy phase: enumerate candidates, run the funnel, build the result
/// payload. Blocking — the caller decides where it runs (a worker thread on
/// the native server, a Web Worker under wasm). Progress is published
/// through `state` (poll it from another thread, or — single-threaded —
/// read it inside `on_round`, which fires after every completed funnel
/// round); cancellation is `state.cancel`. `on_enumerated(candidates,
/// jobs)` fires once when enumeration finishes and the funnel is about to
/// start. Native callers poll and pass `on_round: None`; the wasm build has
/// no second thread to poll from, so the callback is its progress channel.
pub fn run_optimize(
    plan: OptimizePlan,
    state: &FunnelState,
    on_enumerated: impl FnOnce(usize, usize),
    on_round: Option<&dyn Fn()>,
) -> Value {
    let OptimizePlan {
        pool,
        constraints,
        min_slots,
        build_size,
        evo_sets,
        exilus_defs,
        arcanes,
        scenario,
        final_runs,
        finalists,
        headshot_pct,
        duration,
        target_name,
        level,
        steel_path,
        weapon_id,
        threads,
    } = plan;
    // Compute budget: 0 = auto (all cores minus two — the machine must stay
    // usable while the search runs). Applies to the screen and every round.
    wfsim_optimizer::set_worker_threads(threads);
    let info = weapon(&weapon_id);

    // ---- enumerate candidates per evo-set × exilus option ----
    // Two regimes, NO scope cap (user, 2026-07-29: "no enumeration limit —
    // find the smarter way"):
    //  - a scope that fits MATERIALIZE_LIMIT is collected into a Vec and
    //    runs the exact classic funnel (unchanged results);
    //  - past the limit the partial Vec is discarded and the WHOLE scope is
    //    re-walked STREAMING: each candidate is screened (1 run × every
    //    arcane) as it is born and only the best SCREEN_KEEP jobs survive
    //    into the funnel — memory stays O(SCREEN_KEEP) at any scope size,
    //    and the walk answers `cancel` at every node.
    const MATERIALIZE_LIMIT: usize = 2_000_000;
    const SCREEN_KEEP: usize = 65_536;
    let innate = wfsim_engine::weapons_data::innate_slots(&info.id);
    let exilus_refs: Vec<Option<&ModDef>> = exilus_defs.iter().map(|o| o.as_ref()).collect();
    let cancelled_json = |n_cands: usize| {
        // Cancelled before anything was ranked — a clean empty cancellation.
        json!({
            "ok": true, "cancelled": true,
            "candidates": n_cands, "jobs": 0,
            "final_runs": final_runs, "finalists": finalists,
            "headshot_pct": headshot_pct, "duration": duration,
            "results": [],
            "target": { "name": target_name, "level": level, "steel_path": steel_path },
        })
    };

    let mut cands: Vec<Candidate> = Vec::new();
    let mut overflow = false;
    for (vi, set) in evo_sets.iter().enumerate() {
        let refs: Vec<&str> = set.iter().map(String::as_str).collect();
        let base = WeaponBase::from_data(incarnon_id(info).unwrap_or(&info.id), true, &refs);
        let base_form = WeaponBase::from_data(&info.id, true, &refs);
        let (mut c, _stats, complete) = enumerate_candidates_observed(
            &pool,
            &base,
            Some(&base_form),
            vi as u32,
            min_slots as u32,
            build_size as u32,
            60,
            &innate,
            &constraints,
            &exilus_refs,
            Some(state),
            MATERIALIZE_LIMIT - cands.len(),
        );
        cands.append(&mut c);
        if !complete {
            if state.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return cancelled_json(cands.len());
            }
            overflow = true;
            break;
        }
        // Exactly-full guard: the next iteration would pass max_out = 0,
        // which the observed walk reads as "no cap".
        if cands.len() >= MATERIALIZE_LIMIT && vi + 1 < evo_sets.len() {
            overflow = true;
            break;
        }
    }

    let (cands, last, cancelled, n_jobs) = if !overflow {
        // ---- classic path: materialized candidates, full funnel ----
        if cands.is_empty() {
            return err_json(
                "no legal builds in this scope (Forma / family constraints eliminated all)",
            );
        }
        let jobs: Vec<Job> = (0..cands.len())
            .flat_map(|i| (0..arcanes.len()).map(move |a| (i, a)))
            .collect();
        let n_jobs = jobs.len();
        on_enumerated(cands.len(), n_jobs);
        let rounds = schedule_to(n_jobs, final_runs, finalists);
        let last = run_funnel(
            &cands,
            &arcanes,
            &scenario,
            jobs,
            &rounds,
            0xDEAD_BEEF,
            false,
            Some(state),
            on_round,
        );
        let c = state.cancel.load(std::sync::atomic::Ordering::Relaxed);
        (cands, last, c, n_jobs)
    } else {
        // ---- streaming path: re-walk the whole scope through the screen ----
        drop(cands); // the partial materialization is dead weight
        state
            .enumerated
            .store(0, std::sync::atomic::Ordering::Relaxed);
        state
            .sims_done
            .store(0, std::sync::atomic::Ordering::Relaxed);
        let (screened, complete) = stream_screen(
            |emit| {
                for (vi, set) in evo_sets.iter().enumerate() {
                    let refs: Vec<&str> = set.iter().map(String::as_str).collect();
                    let base =
                        WeaponBase::from_data(incarnon_id(info).unwrap_or(&info.id), true, &refs);
                    let base_form = WeaponBase::from_data(&info.id, true, &refs);
                    if !enumerate_candidates_each(
                        &pool,
                        &base,
                        Some(&base_form),
                        vi as u32,
                        min_slots as u32,
                        build_size as u32,
                        60,
                        &innate,
                        &constraints,
                        &exilus_refs,
                        Some(state),
                        scenario.aiming,
                        emit,
                    ) {
                        break;
                    }
                }
            },
            &arcanes,
            &scenario,
            1,
            SCREEN_KEEP,
            0xDEAD_BEEF,
            Some(state),
        );
        if screened.is_empty() {
            if state.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return cancelled_json(0);
            }
            return err_json(
                "no legal builds in this scope (Forma / family constraints eliminated all)",
            );
        }
        // Survivors → a dedup'd candidate table (the same build survives
        // with several arcanes) + (job, screen summary) pairs, best-first.
        let mut sc: Vec<Candidate> = Vec::new();
        let mut by_ptr: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        let mut slast = Vec::new();
        for sj in &screened {
            let key = std::sync::Arc::as_ptr(&sj.cand) as usize;
            let ci = *by_ptr.entry(key).or_insert_with(|| {
                sc.push((*sj.cand).clone());
                sc.len() - 1
            });
            slast.push(((ci, sj.ai), sj.summary));
        }
        if !complete {
            // Cancelled mid-screen: the screen's own ranking (1-run
            // precision) is the best-so-far leaderboard.
            let n = slast.len();
            (sc, slast, true, n)
        } else {
            let jobs: Vec<Job> = slast.iter().map(|(j, _)| *j).collect();
            let n = jobs.len();
            state
                .sims_done
                .store(0, std::sync::atomic::Ordering::Relaxed); // fresh % for the funnel
            on_enumerated(sc.len(), n);
            let rounds = schedule_to(n, final_runs, finalists);
            let last = run_funnel(
                &sc,
                &arcanes,
                &scenario,
                jobs,
                &rounds,
                0xDEAD_BEEF,
                false,
                Some(state),
                on_round,
            );
            let c = state.cancel.load(std::sync::atomic::Ordering::Relaxed);
            (sc, last, c, n)
        }
    };

    // ---- the finalists leaderboard (on cancel: the last completed
    // round's top slice — intermediate rounds can be huge) ----
    let results: Vec<Value> = last
        .iter()
        .take(finalists)
        .enumerate()
        .map(|(rank, ((ci, ai), s))| {
            let c = &cands[*ci];
            let mods: Vec<&str> = c.ordered.iter().map(|&i| pool[i].id).collect();
            let arc = &arcanes[*ai];
            let arcane_id = if arc.id.is_empty() {
                "none".to_string()
            } else {
                arc.id.clone()
            };
            let arcane_rank = if arc.id.is_empty() {
                0
            } else {
                wfsim_engine::arcanes_data::secondary(&arc.id)
                    .map(|d| d.max_rank)
                    .unwrap_or(0)
            };
            json!({
                "rank": rank + 1,
                "kills": s.mean_kills,
                "kill_progress": s.mean_kill_progress,
                "dps": s.effective_dps,
                "kills_min": s.min_kills,
                "kills_max": s.max_kills,
                "mods": mods,
                "arcane": arcane_id,
                "arcane_rank": arcane_rank,
                "evolutions": evo_sets[c.variant as usize],
                "exilus": exilus_defs[c.exilus as usize].as_ref().map(|m| m.id).unwrap_or("none"),
                "forma": { "used": c.plan.forma_used, "total_drain": c.plan.total_drain },
            })
        })
        .collect();

    json!({
        "ok": true,
        "candidates": cands.len(),
        "jobs": n_jobs,
        "cancelled": cancelled,
        "final_runs": final_runs,
        "finalists": finalists,
        "headshot_pct": headshot_pct,
        "duration": duration,
        "results": results,
        "target": { "name": target_name, "level": level, "steel_path": steel_path },
    })
}
