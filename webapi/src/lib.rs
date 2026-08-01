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
use wfsim_engine::dummy::Summary;

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
    /// The pools this weapon draws from, as a union ("primary" + "rifle" for a
    /// launcher). Compatibility is not one list: DE tags a mod PRIMARY, Rifle,
    /// or narrower (Assault Rifle / Bow / Sniper), and a weapon takes every
    /// tag that applies to it.
    mod_pools: Vec<String>,
    /// Continuous (beam) weapon, from the BASE form's trigger — what the
    /// beam-only mods gate on.
    continuous: bool,
    /// Riven disposition. 1.0 when the data does not say, so a weapon with no
    /// disposition yet reads as neutral rather than as zero.
    disposition: f64,
    // Precise weapon type within that group (Dual Toxocyst = Dual Pistols).
    subtype: String,
    sentinel: bool,
    /// The forms this weapon REGISTERS (`data/weapons/*.yaml` `form:`), default
    /// first: `(wire id, display name, is the arsenal's default)`. Data-driven
    /// — it used to be hardcoded as "the three Incarnon options, or a single
    /// fake form called `primary`". The default travels with the list because
    /// it is the form a weapon is FIRED in when nothing else is asked for.
    forms: Vec<(&'static str, String, bool)>,
    /// Does a form have to be TRANSFORMED into (gauge + transmute animations)?
    /// Only then is there a two-form cycle to simulate; without it the weapon
    /// is fired in one form and asking for a cycle is meaningless.
    has_cycle: bool,
    uses_arcane: bool,
    /// Which arcane pool this weapon draws from — its own slot
    /// ("secondary" / "primary"). The picker filters on it.
    /// The EQUIPMENT slot — primary / secondary / sentinel / archgun. What
    /// the home grid groups by. It used to be read off `arcane_slot`, which
    /// worked only while every weapon's arcane pool was named after its slot;
    /// an Arch-Gun seats two pools and is neither of them, so the two facts
    /// are now two fields.
    slot: String,
    /// The arcane POOLS this weapon seats, one arcane each. Almost always a
    /// single pool named after the equipment slot; a sentinel seats none; an
    /// Arch-Gun seats TWO — "Archguns possess two Arcane Enhancement slots to
    /// equip one Primary Arcane and one Secondary Arcane" (wiki Arch-Gun),
    /// which is also why it is "not considered either primary or secondary".
    arcane_pools: Vec<String>,
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
                // The weapon's OWN forms, in its own order — every entry of
                // its transform group, each registering a kind from the
                // closed vocabulary. The two-form CYCLE is not in this list:
                // it is a mode over two of these forms, published separately
                // as `has_cycle`.
                let forms = wfsim_engine::weapons_data::forms_of(&s.id)
                    .into_iter()
                    .map(|f| (f.kind.id(), f.kind.label().to_string(), f.is_default))
                    .collect();
                WeaponInfo {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    mod_pools: if s.mod_pools.is_empty() {
                        vec![s.slot.clone()]
                    } else {
                        s.mod_pools.clone()
                    },
                    continuous: s.attack.trigger == "held",
                    disposition: s.disposition.unwrap_or(1.0),
                    subtype: title_case(&s.class),
                    sentinel,
                    forms,
                    has_cycle: wfsim_engine::weapons_data::has_gauge_switched_form(&s.id),
                    slot: s.slot.clone(),
                    uses_arcane: !sentinel,
                    // Keyed on the equipment SLOT, which is what the game
                    // keys it on — a category rule, like `sentinel` above,
                    // not per-weapon data.
                    arcane_pools: match s.slot.as_str() {
                        _ if sentinel => Vec::new(),
                        "archgun" => vec!["primary".to_string(), "secondary".to_string()],
                        other => vec![other.to_string()],
                    },
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

/// The headshot rate a weapon is played at when nothing says otherwise.
///
/// A SENTINEL weapon is fired by the companion, which picks its own targets
/// and does not aim for the head — so 0, not the player's 100 (user,
/// 2026-07-31). It stays a knob: this is the default, not a ceiling.
fn default_headshot_pct(info: &WeaponInfo) -> f64 {
    if info.sentinel {
        0.0
    } else {
        100.0
    }
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
/// The pool a weapon actually sees: its pools unioned, minus mods it cannot
/// equip (the beam-only mods need a continuous weapon).
fn mod_pool_for(weapon_id: &str) -> Vec<ModDef> {
    wfsim_engine::mods_data::pool_for_weapon(weapon_id)
}

/// A mod id that must outlive the request. Riven ids are made from a name the
/// visitor typed, so they cannot be `&'static` on their own — interning keeps
/// one copy per distinct id instead of leaking a fresh one per keystroke.
fn intern(s: String) -> &'static str {
    use std::sync::{Mutex, OnceLock};
    static POOL: OnceLock<Mutex<std::collections::HashSet<&'static str>>> = OnceLock::new();
    let set = POOL.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    let mut g = set.lock().expect("intern");
    if let Some(x) = g.get(s.as_str()) {
        return x;
    }
    let leaked: &'static str = Box::leak(s.into_boxed_str());
    g.insert(leaked);
    leaked
}

/// The rivens a request carries, as ordinary [`ModDef`]s.
///
/// A riven is not part of any pool — it is the visitor's own item, built
/// against THIS weapon's disposition — so it travels with the request and
/// joins the pool only for the build being resolved. That keeps one shared
/// mod pool for everyone and still lets a riven be equipped, searched and
/// optimized exactly like a mod.
///
/// An INCOMPLETE riven is fine and resolves to whatever it does say. A card
/// with no stats is a mod that does nothing, which is a perfectly ordinary
/// thing for a build to contain (user, 2026-07-31).
///
/// An UNKNOWN stat id is not that, and it is an ERROR. `resolved_slots` drops
/// a stat it cannot find, so a typo used to equip a riven that occupied a
/// slot, drained capacity and granted nothing — silently, with the card still
/// naming the stats. That is the one failure a damage calculator must never
/// hide, and it is the same rule `mods` already follows ("unknown mod id").
/// Why a mod id did not resolve against THIS weapon's pool.
///
/// "unknown mod id: amalgam_serration" is true of the pool and false of the
/// world, and it is what a saved build got the moment the pool learned a rule
/// (Amalgam mods off sentinel weapons, ammo mods off an infinite reserve).
/// A mod that exists but does not fit says so.
fn mod_not_here(id: &str, weapon: &WeaponInfo) -> String {
    let known = wfsim_engine::mods_data::classes()
        .into_iter()
        .any(|c| wfsim_engine::mods_data::class_pool(c).iter().any(|m| m.id == id));
    if known {
        format!("{id} cannot be equipped on {} — it is not in this weapon's pool", weapon.name)
    } else {
        format!("unknown mod id: {id}")
    }
}

fn riven_stat_ids_ok(v: &Value, info: &WeaponInfo) -> Result<(), String> {
    let class = riven_class(info);
    let pool = wfsim_engine::rivens_data::pool(&class);
    let known = |x: &Value| -> Result<(), String> {
        let Some(id) = x.get("id").and_then(|i| i.as_str()) else { return Ok(()) };
        if id.is_empty() || pool.iter().any(|s| s.id == id) {
            return Ok(());
        }
        Err(format!("unknown riven stat id: {id} (pool: {class})"))
    };
    for r in v.get("rivens").and_then(|a| a.as_array()).into_iter().flatten() {
        let Some(s) = r.get("spec") else { continue };
        for b in s
            .get("bonuses")
            .or_else(|| s.get("positives"))
            .and_then(|x| x.as_array())
            .into_iter()
            .flatten()
        {
            known(b)?;
        }
        if let Some(c) = s.get("malus").or_else(|| s.get("curse")) {
            known(c)?;
        }
    }
    Ok(())
}

fn rivens_from(v: &Value, info: &WeaponInfo) -> Vec<ModDef> {
    use wfsim_engine::rivens_data::{RivenSpec, RolledStat};
    let class = riven_class(info);
    let rolled = |x: &Value| -> Option<RolledStat> {
        if !x.is_object() {
            return None;
        }
        Some(RolledStat {
            id: x.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string(),
            roll: x.get("roll").and_then(|r| r.as_f64()).unwrap_or(1.0),
        })
    };
    v.get("rivens")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    let name = r.get("name")?.as_str()?;
                    let s = r.get("spec")?;
                    let spec = RivenSpec {
                        class: class.clone(),
                        // Both spellings: a riven saved before Bonus/Malus
                        // still equips (see `riven_json`).
                        bonuses: s
                            .get("bonuses")
                            .or_else(|| s.get("positives"))
                            .and_then(|x| x.as_array())
                            .map(|x| x.iter().filter_map(rolled).collect())
                            .unwrap_or_default(),
                        malus: s.get("malus").or_else(|| s.get("curse")).and_then(rolled),
                        rank: s.get("rank").and_then(|x| x.as_u64()).unwrap_or(8) as u32,
                        polarity: match s.get("polarity").and_then(|x| x.as_str()).unwrap_or("madurai") {
                            "vazarin" => wfsim_engine::mods::Polarity::Vazarin,
                            "naramon" => wfsim_engine::mods::Polarity::Naramon,
                            _ => wfsim_engine::mods::Polarity::Madurai,
                        },
                    };
                    Some(spec.to_mod_def(intern(format!("riven:{name}")), info.disposition))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Which riven stat pool a weapon draws from: the NARROWEST of its mod pools
/// that actually has one.
///
/// It is derived rather than declared because a weapon's riven class is not
/// always its mod class. A bow's mod pool is `bow`, and there is no bow riven
/// — the wiki's table has no bow column and its RIFLE row is the one that
/// reads "(x2 for Bows)". Walking outward from the narrowest finds `rifle`
/// for a bow today, and would find a real bow pool the day one exists,
/// without a weapon having to name it.
fn riven_class(info: &WeaponInfo) -> String {
    info.mod_pools
        .iter()
        .rev()
        .find(|c| !wfsim_engine::rivens_data::pool(c).is_empty())
        .cloned()
        .unwrap_or_default()
}

/// The weapon's pool PLUS the request's own rivens.
fn mod_pool_with_rivens(v: &Value, info: &WeaponInfo) -> Vec<ModDef> {
    let mut p = mod_pool_for(&info.id);
    p.extend(rivens_from(v, info));
    p
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
                // Equip restriction beyond the pool tag: "continuous" for the
                // beam-only mods. The picker filters on it, the same way the
                // engine's `pool_for_weapon` does.
                "requires_weapon": m.requires_weapon,
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
                // The pools to union, in order. `mod_class` stays as the
                // NARROWEST one, which is what labels and filters read.
                "mod_pools": w.mod_pools,
                // The BASE form's trigger decides this — the Torid's Incarnon
                // form is a beam and the weapon still is not a continuous one
                // for modding purposes.
                "continuous": w.continuous,
                "disposition": w.disposition,
                // The riven stat pool this weapon draws from — not always its
                // mod class (a bow's mods are `bow`, its rivens are `rifle`).
                "riven_class": riven_class(w),
                // …minus the stats THIS weapon cannot roll. The pool is per
                // class, but a sentinel weapon has no Zoom and no Recoil, a
                // hit-scan one has no flight speed, an infinite-ammo one has
                // no Ammo Maximum, and a weapon with no IPS rolls no physical
                // attribute (wiki's 25% rule). Sent as a list rather than a
                // filtered pool so the class table stays shared.
                "riven_excludes": wfsim_engine::rivens_data::excluded_for(&w.id),
                // Has this weapon a reserve that can RUN OUT? False for every
                // sentinel weapon ("Ammo Max: infinity / Ammo Type: None") and
                // for anything else the data leaves infinite, which is what
                // makes the Infinite-ammo box ticked-and-disabled there.
                "finite_reserve": wfsim_engine::weapons_data::spec(&w.id)
                    .is_some_and(|s| s.finite_reserve),
                // The mods this weapon can actually EQUIP, by id. The client
                // used to union the class tables and re-apply the rules in JS,
                // which is one fact stated twice — and the copy went stale the
                // moment the engine learned a new rule (Amalgam mods off
                // sentinel weapons, ammo mods off an infinite reserve: neither
                // reached the builder or the optimizer). `pool_for_weapon` is
                // now the only place that decides, and this is it speaking.
                "mods": wfsim_engine::mods_data::pool_for_weapon(&w.id)
                    .iter()
                    .map(|m| m.id)
                    .collect::<Vec<_>>(),
                "mod_class": w.mod_pools.last().cloned().unwrap_or_default(),
                "subtype": w.subtype,
                "sentinel": w.sentinel,
                // The EQUIPMENT slot ("primary" / "secondary"), which is what
                // the home grid groups by. `arcane_slot` happens to hold the
                // same string today because a weapon draws its arcane from its
                // own slot — but that is a coincidence of the arcane rule, not
                // a name the UI should be reading for grouping.
                "slot": w.slot,
                "uses_arcane": w.uses_arcane,
                // The POOLS, in slot order — the page draws one picker per
                // entry and sends one arcane per entry.
                "arcane_pools": w.arcane_pools,
                "uses_evo2": w.uses_evo2,
                // A sentinel weapon has no arcane slot. This was hardcoded to
                // 1 while every weapon in the roster had one.
                "arcane_slots": w.arcane_pools.len(),
                "image": assets().weapons.get(&w.id),
                "innate_polarities": innate_slots_for(&w.id).iter()
                    .map(|p| p.map(|x| format!("{x:?}")))
                    .collect::<Vec<_>>(),
                "forms": w.forms.iter()
                    .map(|(id, name, def)| json!({"id": id, "name": name, "is_default": def}))
                    .collect::<Vec<_>>(),
                // Is there a form to TRANSFORM into? Then the sim can run the
                // real two-form loop as a MODE over the forms above; without
                // one the weapon is fired in a single form (`forms` may still
                // hold several — charged vs uncharged is a free choice, not a
                // transformation).
                "has_cycle": w.has_cycle,
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
        // Riven stat pools, keyed by mod class. The builder needs the whole
        // pool to offer choices; the VALUES it must ask for, because the
        // formula lives in one place (`/api/riven`).
        "riven_stats": wfsim_engine::mods_data::classes()
            .into_iter()
            .filter_map(|c| {
                let p = wfsim_engine::rivens_data::pool(c);
                (!p.is_empty()).then(|| {
                    (
                        c.to_string(),
                        json!(p
                            .iter()
                            .map(|s| json!({
                                "id": s.id, "text": s.text, "base": s.base,
                                "prefix": s.prefix, "suffix": s.suffix,
                                "malus": s.malus, "modeled": s.kind != "unmodeled",
                            }))
                            .collect::<Vec<_>>()),
                    )
                })
            })
            .collect::<serde_json::Map<String, Value>>(),
        "riven_rules": {
            "roll_min": wfsim_engine::rivens_data::ROLL_MIN,
            "roll_max": wfsim_engine::rivens_data::ROLL_MAX,
            "max_rank": wfsim_engine::rivens_data::MAX_RANK,
            // The polarities a riven rolls (wiki: one of three).
            "polarities": ["madurai", "vazarin", "naramon"],
            "mastery_min": 8,
            "mastery_max": 16,
        },
        // Choosable evolution tiers from data/evolutions/*.yaml (tier 1 =
        // the Incarnon Form unlock — deselecting it means no transformation,
        // so the panel/sim fall back to the base form). Every tier also gets
        // an implicit EMPTY choice in the UI (nothing installed); `broken` =
        // wiki-flagged non-functional — the engine applies ZERO for those,
        // and the UI must say so in red. `desc` lines are the verbatim
        // effect text (like the mod/arcane cards).
        "defaults": {
            "weapon": default_weapon_id(),
            // Per-weapon, because "the form this is played in" is: the
            // Incarnon cycle where there is one, and the weapon's own default
            // form (`default_form` in data/weapons) where there is not. A
            // fixed string could only ever be right for one of the two.
            "form": "default",
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
            // INFINITE AMMO by default — see `simulate_json` for why.
            "infinite_ammo": true,
            // Test precision (user, 2026-08-01): 300 s x 100 runs everywhere,
            // and the optimizer's last round is 100 runs on the top 10. Kept
            // in step with `simulate_json` / `parse_optimize`, whose own
            // fallbacks are what an API caller naming none of these gets.
            "duration": 300.0,
            "runs": 100,
            "final_runs": 100,
            "finalists": 10,
            "mods": [],
        },
    })
}

/// Resolve a riven SPEC against a weapon: the values it shows, its generated
/// name, its drain, and every reason it could not exist.
///
/// The page owns the knobs and this owns the arithmetic — one implementation
/// of the formula, so a slider cannot drift from what the sim would build.
pub fn riven_json(v: &Value) -> Value {
    use wfsim_engine::rivens_data::{RivenSpec, RolledStat};
    let info = weapon(get_str(v, "weapon", default_weapon_id()));
    let class = riven_class(info);
    // A slot may carry a `roll` OR a `value`. `value` is what you type off a
    // riven you already own; it is turned into the roll it implies and
    // clamped into the legal band, so a number from anywhere lands legal
    // instead of being refused. An empty id is a slot not filled in yet.
    let rolled = |x: &Value| -> Option<RolledStat> {
        // A null malus is NO malus — the shape depends on it, so reading one
        // as an empty slot made every riven resolve as if it had one.
        if !x.is_object() {
            return None;
        }
        Some(RolledStat {
            id: x.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string(),
            roll: x.get("roll").and_then(|r| r.as_f64()).unwrap_or(1.0),
        })
    };
    // The wiki's words are Bonus and Malus and so are ours. A riven saved
    // before the rename still arrives spelled the old way, and a stored riven
    // is the visitor's own — it outlives our vocabulary.
    let field = |a: &str, b: &str| v.get(a).or_else(|| v.get(b));
    let spec = RivenSpec {
        class: class.clone(),
        bonuses: field("bonuses", "positives")
            .and_then(|a| a.as_array())
            .map(|a| a.iter().filter_map(rolled).collect())
            .unwrap_or_default(),
        malus: field("malus", "curse").and_then(rolled),
        rank: get_u32(v, "rank", wfsim_engine::rivens_data::MAX_RANK),
        polarity: match get_str(v, "polarity", "madurai") {
            "vazarin" => wfsim_engine::mods::Polarity::Vazarin,
            "naramon" => wfsim_engine::mods::Polarity::Naramon,
            _ => wfsim_engine::mods::Polarity::Madurai,
        },
    };
    let evo_refs: Vec<&str> = Vec::new();
    let base = WeaponBase::from_data(&info.id, true, &evo_refs);
    let disposition = info.disposition;
    let n_pos = spec.bonuses.len();
    // A typed VALUE overrides the roll, once the stat is known.
    let mut spec = spec;
    let want_value = |arr: Option<&Value>, i: usize| -> Option<f64> {
        arr?.as_array()?.get(i)?.get("value")?.as_f64()
    };
    let p = wfsim_engine::rivens_data::pool(&class);
    // A typed value arrives in the units the CARD shows — "200" means 200%,
    // "0.59" on a faction stat means a x0.59 multiplier. `from_shown` is the
    // engine's own inverse of what it printed, so a number copied off a real
    // riven means what it says instead of being clamped to an end.
    for i in 0..spec.bonuses.len() {
        let Some(v) = want_value(field("bonuses", "positives"), i) else { continue };
        let id = spec.bonuses[i].id.clone();
        if let Some(def) = p.iter().find(|x| x.id == id) {
            spec.bonuses[i].roll = spec.roll_for_value(def, true, disposition, def.from_shown(v));
        }
    }
    if let (Some(c), Some(v)) = (
        spec.malus.clone(),
        field("malus", "curse").and_then(|c| c.get("value")).and_then(|x| x.as_f64()),
    ) {
        if let Some(def) = p.iter().find(|x| x.id == c.id) {
            let r = spec.roll_for_value(def, false, disposition, def.from_shown(v));
            if let Some(cc) = spec.malus.as_mut() { cc.roll = r; }
        }
    }
    let stats: Vec<Value> = spec
        .resolved_slots(disposition)
        .into_iter()
        .map(|(slot, def, value)| {
            let bonus = slot < n_pos;
            let (lo, hi) = spec.bounds_of(def, bonus, disposition);
            let roll = if bonus { spec.bonuses[slot].roll } else { spec.malus.as_ref().map_or(1.0, |c| c.roll) };
            json!({
                // The SLOT, because a half-described card still has real
                // numbers and skipping the empty slots would slide the rest.
                "slot": if bonus { slot.to_string() } else { "malus".to_string() },
                "id": def.id, "text": def.print(value), "value": value,
                "shown": def.shown(value), "roll": roll,
                // The card's precision, so a box cannot offer a decimal the
                // game never showed anyone.
                "decimals": def.decimals(),
                // Where the roll landed in its own band, 0-100 — the one
                // number that compares two stats on one card.
                "percentile": wfsim_engine::rivens_data::percentile(roll),
                // The ends of the roll band, in shown units — what a number
                // box may be typed to without leaving the legal riven.
                "min": def.shown(lo), "max": def.shown(hi),
                // A multiplier has no sign to read, so the box needs to be
                // told the number it holds is not a percentage.
                "unit": match def.shown_as() {
                    wfsim_engine::rivens_data::Shown::Percent => "%",
                    wfsim_engine::rivens_data::Shown::Multiplier => "x",
                    wfsim_engine::rivens_data::Shown::Number => "",
                },
                "bonus": bonus, "modeled": def.kind != "unmodeled",
            })
        })
        .collect();
    json!({
        "ok": true,
        "class": class,
        "disposition": disposition,
        "name": spec.name(disposition),
        "drain": spec.drain(),
        "stats": stats,
        // Every reason at once, so the UI can point at the knob that is wrong
        // instead of only refusing.
        "illegal": spec.illegal_on(&base),
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
        StatusChance => "Status Chance",
        AmmoEfficiency => "Ammo Efficiency",
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
                OnReloadDamage { .. } => push(BuffMeta {
                    id: "on_reload_bd".into(),
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
        // The buff's OWN arcane names it, not the merged `arcane.id`. A weapon
        // that seats two folds them into one `ArcaneFx` whose id is
        // "primary_deadhead+secondary_deadhead", and every card read that —
        // two identically-named cards the player could not tell apart, which
        // is the whole reason `ArcBuffSpec::owner` exists (2026-08-01).
        let named = |id: &str| {
            wfsim_engine::arcanes_data::secondary(id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| prettify(id))
        };
        let multi = arcane.buffs.len() > 1;
        for (i, b) in arcane.buffs.iter().enumerate() {
            let aname = named(if b.owner.is_empty() { &arcane.id } else { &b.owner });
            let id = if multi {
                format!("arcane:{}:{}", arcane.id, i)
            } else {
                format!("arcane:{}", arcane.id)
            };
            // Two arcanes CAN each grant one buff, so "more than one card"
            // no longer implies "one arcane with several grants": qualify the
            // name by its grant only when the same arcane owns both.
            let same_owner = arcane
                .buffs
                .iter()
                .filter(|x| x.owner == b.owner)
                .count()
                > 1;
            let name = if same_owner {
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
    let parts: Vec<wfsim_engine::arcanes_data::ArcaneFx> = arcane_choices(v, info)
        .into_iter()
        .filter_map(|(pool, aid, rank)| {
            // POOL-scoped: an arcane from another pool is not equippable in
            // that slot, so it resolves to nothing rather than being applied.
            let def = wfsim_engine::arcanes_data::for_slot(&pool, &aid)?;
            let rank = rank.unwrap_or(def.max_rank).min(def.max_rank);
            Some(def.fx(rank, policy, base.traits))
        })
        .collect();
    // Two arcanes are one effect set — see `ArcaneFx::merged`.
    wfsim_engine::arcanes_data::ArcaneFx::merged(&parts)
}

/// An arcane by id, in ANY pool this weapon seats. The optimizer's scope is a
/// flat list of ids rather than one per slot, so it asks this question
/// instead of "is it in THE pool" — an Arch-Gun's scope legitimately mixes
/// Primary and Secondary arcanes.
fn arcane_in_pools(
    info: &WeaponInfo,
    id: &str,
) -> Option<&'static wfsim_engine::arcanes_data::ArcaneDef> {
    info.arcane_pools
        .iter()
        .find_map(|p| wfsim_engine::arcanes_data::for_slot(p, id))
}

/// The arcane chosen for each of the weapon's pools: `(pool, id, rank)`.
///
/// ONE wire shape — a LIST, one entry per pool, in the weapon's pool order:
///
/// ```text
/// "arcane": ["primary_deadhead", "secondary_merciless"]
/// "arcane_rank": [5, 5]
/// ```
///
/// A build saved before a weapon could seat two held a bare value under a
/// pre-data short name ("deadhead"). Both are rewritten ONCE, in the client's
/// storage (`migrateArcaneShape`), so nothing here has to know there was ever
/// another shape — the alternative is two ways of saying the same thing, kept
/// alive forever by the code that reads both (user, 2026-08-01).
///
/// Entries past the weapon's pool count are dropped: what a weapon can seat is
/// the weapon's business, not the caller's.
fn arcane_choices(v: &Value, info: &WeaponInfo) -> Vec<(String, String, Option<u32>)> {
    let ids: Vec<String> = v
        .get("arcane")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str()).map(String::from).collect())
        .unwrap_or_default();
    let ranks: Vec<Option<u32>> = v
        .get("arcane_rank")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().map(|x| x.as_u64().map(|n| n as u32)).collect())
        .unwrap_or_default();
    info.arcane_pools
        .iter()
        .enumerate()
        .filter_map(|(i, pool)| {
            let id = ids.get(i)?;
            (id != "none").then(|| (pool.clone(), id.clone(), ranks.get(i).copied().flatten()))
        })
        .collect()
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
    let evos = match chosen_evolutions(v, info) {
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
    if let Err(e) = riven_stat_ids_ok(v, info) {
        return err_json(e);
    }
    let p = mod_pool_with_rivens(v, info);
    let mut refs: Vec<&ModDef> = Vec::with_capacity(mod_ids.len());
    for id in &mod_ids {
        match p.iter().find(|m| m.id == id) {
            Some(m) => refs.push(m),
            None => return err_json(mod_not_here(id, info)),
        }
    }

    // ---- forms: EVERY available form renders side by side (no switching;
    // user decision). The Incarnon Form section exists only while its
    // tier-1 unlock is selected. `meta` states the trigger/shot mechanics
    // from the weapon data (data/weapons yamls).
    // Section titles come from the REGISTERED form (`data/weapons` `form:`),
    // so a bow's section says "Charged Shot" instead of the "Base Form" every
    // weapon's first section used to be called.
    let mut forms_list: Vec<(&'static str, String, WeaponBase)> = Vec::new();
    for f in wfsim_engine::weapons_data::forms_of(&info.id) {
        // A gauge-switched form exists only while its tier-1 unlock is chosen.
        if f.kind.is_gauge_switched()
            && !form_unlock_evo(info).is_some_and(|u| evo_refs.contains(&u))
        {
            continue;
        }
        forms_list.push((
            f.kind.label(),
            attack_desc(wspec(f.weapon_id)),
            WeaponBase::from_data(f.weapon_id, true, &evo_refs),
        ));
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
                // Its own row: the chance is not a status chance and does not
                // pool with one - it is a separate roll off a critical hit.
                SlashOnCrit(x) => push("slash_on_crit", x, None),
                FireRate(x) => push("fire_rate", x, None),
                // Its own row, not fire rate's: a charge-rate mod shortens the
                // DRAW and leaves an uncharged form's cadence alone.
                ChargeRate(x) => push("charge_rate", x, None),
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
                        "why": "a companion weapon cannot TRIGGER this - the on-kill roll comes from the Tenno's own weapons - and this arena simulates one weapon alone, so the stacks the Tenno would share never arrive"})),
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
                        "why": "a companion weapon cannot TRIGGER this - the on-kill roll comes from the Tenno's own weapons - and this arena simulates one weapon alone, so the stacks the Tenno would share never arrive"})),
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
                // Attributed on the radius rows of whichever parts have one.
                BlastRadius(x) => push("radius", x, None),
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
                        B::ReloadSpeed => "reload",
                    };
                    // "assumed active" is exactly what this panel is: it
                    // resolves under AssumedMax, so the number belongs here.
                    // What it does NOT mean is that the SIM has it — see
                    // MECHANICS "Conditional buffs with no live model".
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
                        "why": "a companion weapon cannot TRIGGER this - the on-kill roll comes from the Tenno's own weapons - and this arena simulates one weapon alone, so the stacks the Tenno would share never arrive"})),
                    _ => push(
                        "crit_damage",
                        bonus,
                        Some("on kill, buff assumed up".into()),
                    ),
                },
                OnReloadDamage { bonus, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "a companion weapon cannot TRIGGER this - the reload is the Tenno's - and this arena simulates one weapon alone, so the buff the Tenno would share never arrives"})),
                    _ => push(
                        "base_damage",
                        bonus,
                        Some("on reload from empty, buff assumed up".into()),
                    ),
                },
                OnReloadFireRate { bonus, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "a companion weapon cannot TRIGGER this - the reload is the Tenno's - and this arena simulates one weapon alone, so the buff the Tenno would share never arrives"})),
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
    let (mut evo_flat_sc, mut evo_flat_mag) = (0.0f64, 0.0f64);
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
            let v = def.flat_base_status_chance();
            if v > 0.0 {
                evo_flat_sc += v;
                evo_src.push((
                    "status_chance",
                    name.clone(),
                    format!("+{:.0}% base", v * 100.0),
                    Some("into the BASE status chance — status mods multiply it".into()),
                ));
            }
            let v = def.flat_base_magazine();
            if v > 0.0 {
                evo_flat_mag += v;
                evo_src.push((
                    "magazine",
                    name.clone(),
                    format!("+{v:.0} rounds"),
                    Some("into the BASE magazine — magazine mods multiply it".into()),
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
        let raw_sc = base.base_status_chance - evo_flat_sc;
        let raw_mag = base.magazine_size - evo_flat_mag;
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
            pc(raw_sc),
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
                num(raw_mag),
                num(panel.magazine_size),
            );
            row(
                "reload",
                "Reload",
                format!("{}s", num(base.base_reload)),
                format!("{}s", num(panel.reload_seconds)),
            );
        }
        // BOWS state their cadence, because the Fire Rate row above is NOT it:
        // wiki Fire Rate gives bows a formula of their own — "Effective Fire
        // Rate = 1 / (Modded Charge Time + Modded Reload Time)" — which has no
        // fire-rate term at all. So the panel prints the draw (the half a
        // fire-rate mod actually shortens, at double value on a bow) and the
        // rate that formula yields, or a build reads 1.6/s where it fires 1.0.
        //
        // A tapped shot has NO draw: its row would be a constant 0.00s, so it
        // is left out and only the effective rate is stated.
        //
        // INSERTED beside the Fire Rate row it belongs next to rather than
        // pushed here: `row` holds the `stats` borrow until its last call.
        if let (Some(b), Some(f)) = (base.charge_seconds, panel.charge_seconds) {
            let at = stats
                .iter()
                .position(|s| s["key"] == "fire_rate")
                .map_or(stats.len(), |i| i + 1);
            let mut rows = Vec::new();
            if b > 0.0 {
                // Two decimals: a doubled bow bonus lands on values like
                // 0.31 s that `num`'s one decimal would round away.
                rows.push(json!({ "key": "charge_time", "label": "Charge Time",
                    "base": format!("{b:.2}s"), "final": format!("{f:.2}s"),
                    "sources": sources("fire_rate", None) }));
            }
            let eff = |charge: f64, reload: f64| format!("{:.2}/s", 1.0 / (charge + reload));
            rows.push(json!({ "key": "effective_fire_rate", "label": "Effective Fire Rate",
                "base": eff(b, base.base_reload), "final": eff(f, panel.reload_seconds),
                "note": "a bow's real cadence: 1 / (charge + reload), with no fire-rate term \
                         (wiki Fire Rate)".to_string(),
                "sources": json!([]) }));
            for (i, r) in rows.into_iter().enumerate() {
                stats.insert(at + i, r);
            }
        }
        // A continuous beam's impact SPHERE. Firestorm enlarges it, and without
        // this row the mod reads as equipped-but-doing-nothing on this form.
        // The note carries the honest part: the sphere adds no damage to a
        // target the beam already struck, so it is worth nothing single-target
        // and a great deal in a crowd, where every enemy it catches starts its
        // own chain.
        if let (Some(bb), Some(bp)) = (base.beam, panel.beam) {
            stats.push(json!({ "key": "radius", "label": "Beam Radius",
                "base": format!("{} m", num(bb.damage_radius_m)),
                "final": format!("{} m", num(bp.damage_radius_m)),
                "note": format!(
                    "no single-target damage (a struck target is hit once); in a crowd every enemy it catches starts its own chain ({} hops, {} m, x{} per hop)",
                    bp.chain_hops, num(bp.chain_range_m), num(bp.chain_damage_per_hop)),
                "sources": sources("radius", None) }));
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
        for (pool, aid, want_rank) in arcane_choices(v, info) {
            if let Some(def) = wfsim_engine::arcanes_data::for_slot(&pool, &aid) {
                let rank = want_rank.unwrap_or(def.max_rank).min(def.max_rank);
                let fx = def.fx(rank, policy, base.traits);
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
        // Not every indirect stat is a fraction — punch through and beam range
        // are METRES, a double-jump refresh is a COUNT, an explosion-on-kill is
        // flat DAMAGE. `IndirectStat::format` owns that, so this table and the
        // effect line on the card cannot drift apart.
        for (stat, total) in &panel.indirect {
            indirect_rows.push(
                json!({ "key": "indirect", "label": stat.label(), "base": "—",
            "final": stat.format(*total), "sources": sources("indirect", Some(stat.label())) }),
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
                    "base": pc(rb.base_crit_chance - evo_flat_cc),
                    "final": pc(rr.crit_chance),
                    "sources": rsrc("crit_chance") }),
                json!({ "key": "crit_damage", "label": "Crit Damage",
                    "base": format!("×{}", num(rb.base_crit_damage)),
                    "final": format!("×{}", num(rr.crit_damage)),
                    "sources": rsrc("crit_damage") }),
                json!({ "key": "status_chance", "label": "Status Chance",
                    "base": pc(rb.base_status_chance - evo_flat_sc),
                    "final": pc(rr.status_chance),
                    "sources": rsrc("status_chance") }),
                json!({ "key": "status_damage", "label": "Status Damage",
                    "base": format!("×{}", num(1.0)),
                    "final": format!("×{}", num(panel.status_damage_mult)),
                    "sources": rsrc("status_damage") }),
                json!({ "key": "status_duration", "label": "Status Duration",
                    "base": format!("×{}", num(1.0)),
                    "final": format!("×{}", num(panel.status_duration_mult)),
                    "sources": rsrc("status_duration") }),
                json!({ "key": "radius", "label": "Blast Radius",
                    "base": format!("{} m", dist(rb.radius_m)),
                    "final": format!("{} m", dist(rr.radius_m)),
                    "sources": rsrc("radius") }),
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

        // The lingering FIELD is a THIRD kind of part (MECHANICS §7): it does
        // not land once, it ticks. So it states its own clock — rate, lifetime
        // and the resulting total — on top of the same per-instance stats,
        // because "40 damage" means nothing here without "×10 ticks".
        if let (Some(fb), Some(fr)) = (base.lingering.as_ref(), panel.lingering.as_ref()) {
            let fsrc = |key: &'static str| sources(key, None);
            let dist = |x: f64| format!("{x}");
            // ✅ measured (MEASUREMENTS M13): the first tick lands WITH the
            // impact, so the count is the plain product — ten for a 10 s cloud.
            let ticks = (fr.duration_s * fr.tick_rate).round();
            // Renewed Horror: the shot after an empty reload gets a longer
            // cloud. 1.0 = the evolution is not equipped, and the rows stay
            // silent about it rather than stating a boost of ×1.
            let boost = panel.field_duration_on_empty_reload;
            let boosted = (boost > 1.0).then_some((fr.duration_s * boost, ticks * boost));
            let rows = vec![
                json!({ "key": "base_damage", "label": "Damage per Tick",
                    "base": num(fb.base_vector.total()), "final": num(fr.modified_base),
                    "sources": fsrc("base_damage") }),
                json!({ "key": "crit_chance", "label": "Crit Chance",
                    "base": pc(fb.base_crit_chance - evo_flat_cc),
                    "final": pc(fr.crit_chance),
                    "sources": fsrc("crit_chance") }),
                json!({ "key": "crit_damage", "label": "Crit Damage",
                    "base": format!("×{}", num(fb.base_crit_damage)),
                    "final": format!("×{}", num(fr.crit_damage)),
                    "sources": fsrc("crit_damage") }),
                json!({ "key": "status_chance", "label": "Status Chance",
                    "base": pc(fb.base_status_chance - evo_flat_sc),
                    "final": pc(fr.status_chance),
                    "sources": fsrc("status_chance") }),
                json!({ "key": "status_damage", "label": "Status Damage",
                    "base": format!("×{}", num(1.0)),
                    "final": format!("×{}", num(panel.status_damage_mult)),
                    "sources": fsrc("status_damage") }),
                json!({ "key": "status_duration", "label": "Status Duration",
                    "base": format!("×{}", num(1.0)),
                    "final": format!("×{}", num(panel.status_duration_mult)),
                    "sources": fsrc("status_duration") }),
                // The clock. Neither is mod-scaled: fire-rate mods change shots
                // per second, not the cloud's own tick rate, and the cloud is
                // not a status effect so status duration does not reach it.
                json!({ "key": "tick_rate", "label": "Tick Rate", "base": "—",
                    "final": format!("{}/s", dist(fr.tick_rate)), "sources": json!([]) }),
                json!({ "key": "field_duration", "label": "Field Duration",
                    "base": "—", "final": format!("{} s", dist(fr.duration_s)),
                    "note": match boosted {
                        // The doubled cloud is one shot in `magazine`, so state
                        // both numbers rather than an average nobody can check
                        // against a damage number in game.
                        Some((d, n)) => format!(
                            "{} ticks per field, the first landing with the impact; \
                             the shot after an empty reload gets {} s = {} ticks",
                            dist(ticks), dist(d), dist(n)),
                        None => format!("{} ticks per field, the first landing with the impact",
                            dist(ticks)),
                    },
                    "sources": json!([]) }),
                json!({ "key": "field_total", "label": "Total per Field",
                    "base": num(fb.base_vector.total() * ticks),
                    "final": num(fr.modified_base * ticks),
                    "note": "one grenade, before crit and Condition Overload".to_string(),
                    "sources": json!([]) }),
                json!({ "key": "radius", "label": "Field Radius",
                    "base": format!("{} m", dist(fb.radius_m)),
                    "final": format!("{} m", dist(fr.radius_m)),
                    "sources": fsrc("radius") }),
                json!({ "key": "falloff", "label": "Damage Falloff", "base": "—",
                    "final": format!("{}% at {} m",
                        dist((1.0 - fr.falloff_reduction) * 100.0), dist(fr.radius_m)),
                    "note": "the grenade sticks, so the target stands at the epicentre"
                        .to_string(),
                    "sources": json!([]) }),
                // Worth up to ~5x here, so it is stated on the panel rather
                // than buried in the yaml.
                json!({ "key": "field_stacking", "label": "Overlapping Fields",
                    "base": "—",
                    "final": match fr.stacking {
                        wfsim_engine::loadout::FieldStacking::Stack => "stack",
                        wfsim_engine::loadout::FieldStacking::Refresh => "refresh",
                    },
                    "note": "measured (MEASUREMENTS M13)".to_string(),
                    "sources": json!([]) }),
            ];
            parts.push(json!({
                "id": "field",
                "label": "Lingering field",
                "meta": format!("{} m, {} s", dist(fr.radius_m), dist(fr.duration_s)),
                "stats": rows,
                "damage": vector_rows(&fr.damage),
                "damage_total": num(fr.damage.total()),
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
/// The evolutions this run installs — always filtered to the ones that BELONG
/// to this weapon.
///
/// An evolution is a per-weapon item, so another weapon's is not "unselected",
/// it is nonsense — and two ways of getting one were live: the legacy default
/// below (written when Dual Toxocyst was the whole roster, and applied to
/// every weapon that omitted the key — a bow silently gained its +50 base
/// damage, +20% crit and +100% multishot), and a preset copied across weapons
/// by the builder's "⇤ import". Both are dropped here rather than refused: a
/// build is still a legal build without another weapon's perks.
fn chosen_evolutions(v: &Value, info: &WeaponInfo) -> Result<Vec<String>, String> {
    let mine = |ids: Vec<String>| -> Vec<String> {
        let group = evo_group(info);
        ids.into_iter()
            .filter(|id| {
                wfsim_engine::evolutions_data::get(id).is_some_and(|e| e.weapon == group)
            })
            .collect()
    };
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
        return Ok(mine(ids));
    }
    // No `evolutions` key: the historical default build, which is Dual
    // Toxocyst's. `mine` reduces it to nothing on every other weapon — an
    // omitted key means "unstated", and the honest reading of unstated is a
    // weapon with no evolutions installed, not another weapon's.
    let evo2 = match get_str(v, "evo2", "dual_toxocyst_fevered_frenzy") {
        "carnage" | "dual_toxocyst_carnage_reign" => "dual_toxocyst_carnage_reign",
        _ => "dual_toxocyst_fevered_frenzy",
    };
    Ok(mine(vec![
        "dual_toxocyst_commodores_fortune".to_string(),
        "dual_toxocyst_evolved_autoloader".to_string(),
        evo2.to_string(),
    ]))
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
    let form = get_str(v, "form", "default");
    let evos = match chosen_evolutions(v, info) {
        Ok(e) => e,
        Err(e) => return err_json(e),
    };
    let evo_refs: Vec<&str> = evos.iter().map(String::as_str).collect();
    // No Incarnon Form unlock (tier 1) in an explicit selection = the weapon
    // cannot transform: honest fallback to the DEFAULT form.
    let unlock = form_unlock_evo(info);
    let form =
        if v.get("evolutions").is_some() && unlock.is_some_and(|u| !evos.iter().any(|e| e == u)) {
            "base"
        } else {
            form
        };
    // ---- WHICH FORM (or the two-form CYCLE) this run simulates -------------
    // A cycle is a MODE over two forms, not a form, and it exists only where a
    // form must be TRANSFORMED into. Requiring that is a fix, not a tidy-up:
    // the default used to fall through to the cycle for every weapon, so a
    // weapon with no Incarnon form — a sentinel weapon, a bow — was simulated
    // transforming on a borrowed gauge (9 weakpoint hits, 2.35 s + 1.0 s of
    // animation), and the dead time came straight off its DPS.
    let registered = wfsim_engine::weapons_data::forms_of(&info.id);
    // `default` = however THIS weapon is played: the cycle where there is one
    // to run, its own default form where there is not. A weapon that
    // transforms is played transforming (user, 2026-07-31).
    let form = if form == "default" && info.has_cycle { "incarnon_cycle" } else { form };
    // Otherwise the cycle is asked for BY NAME. It used to be "any form
    // string this weapon does not register", which made it the destination of
    // every typo as well — a stale preset naming another weapon's form now
    // falls back to a real form instead of transforming.
    let run_cycle = form == "incarnon_cycle" && info.has_cycle && incarnon_id(info).is_some();
    // The single form to fire: the requested kind if this weapon registers it,
    // else its default (which is what an unknown or stale preset value gets).
    let single_form = registered
        .iter()
        .find(|f| f.kind.id() == form)
        .or_else(|| registered.iter().find(|f| f.is_default))
        .map(|f| f.weapon_id)
        .unwrap_or(&info.id);
    let enemy_id = get_str(v, "enemy", "thrax_centurion");
    let level = get_u32(v, "level", 9999).clamp(1, 9999);
    let steel_path = get_bool(v, "steel_path", true);
    let headshot_pct = get_f64(v, "headshot_pct", default_headshot_pct(info));
    // Is the player HOLDING AIM? Gates the `while_aiming` mod effects
    // (Galvanized Crosshairs / Scope, Argon Scope, Sharpened Bullets, …).
    // Defaults TRUE, which is what the sim silently assumed before this
    // existed — so no stored preset changes meaning.
    // A SENTINEL WEAPON IS ALWAYS AIMING (user, 2026-08-01, settling M18a).
    // What it cannot do is trigger the on-HEADSHOT half of an aiming mod,
    // because it never aims at the head — which the sim already gets right
    // from the other end: `default_headshot_pct` is 0 for a sentinel, so no
    // headshot lands and no on-headshot buff fires. So the state is on, the
    // triggers stay dead, and the request cannot say otherwise.
    let aiming = info.sentinel || get_bool(v, "aiming", true);
    // INFINITE AMMO, and it is the DEFAULT for every weapon (user, 2026-08-01).
    // The sim models no ammo PICKUPS, so a finite reserve is the pessimistic
    // half of a mechanic we only half have — and the headline number people
    // compare across weapons is the one where ammo is not the limit. A weapon
    // whose reserve is infinite in game (every sentinel weapon: "Ammo Max: ∞ /
    // Ammo Type: None") cannot be switched off it, which the UI shows as a
    // ticked, disabled box rather than a control that does nothing.
    //
    // The MAGAZINE is unaffected either way: this is the reserve behind it, so
    // reload cadence — and `ammo_cost` — still bite.
    let infinite_ammo = get_bool(v, "infinite_ammo", true);
    let duration = get_f64(v, "duration", 300.0).clamp(1.0, 3600.0);
    let runs = get_u32(v, "runs", 100).clamp(1, 20_000);
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
    if let Err(e) = riven_stat_ids_ok(v, info) {
        return err_json(e);
    }
    let p = mod_pool_with_rivens(v, info);
    let mut refs: Vec<&ModDef> = Vec::with_capacity(mod_ids.len());
    for id in &mod_ids {
        match p.iter().find(|m| m.id == id) {
            Some(m) => refs.push(m),
            None => return err_json(mod_not_here(id, info)),
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
    // Either ONE registered form, or the real two-form cycle (which needs the
    // gauge form and the form it transforms out of, so it resolves both).
    let (report_panel, mut params): (ResolvedPanel, DummyParams) = {
        let panel_of = |id: &str| {
            resolve_with(
                &WeaponBase::from_data(id, true, &evo_refs),
                &refs,
                policy,
                aiming,
            )
        };
        if run_cycle {
            let incarnon_panel = panel_of(incarnon_id(info).unwrap_or(&info.id));
            let base_panel = panel_of(&info.id);
            let params = DummyParams::incarnon_cycle_from_panels(
                &incarnon_panel,
                &base_panel,
                frenzy_single,
                cycle_frenzy_lock,
                target,
                body_parts,
                duration,
            );
            // The cycle reports the form it transforms INTO, as it always has.
            let mut params = params;
            params.infinite_reserve = infinite_ammo || !incarnon_panel.finite_reserve;
            (incarnon_panel, params)
        } else {
            let panel = panel_of(single_form);
            let mut d = DummyParams::from_panel(&panel, target, body_parts, duration);
            d.infinite_reserve = infinite_ammo || !panel.finite_reserve;
            // Frenzy is the WEAPON's passive: it persists across its forms
            // (user-confirmed 2026-07-24), so it rides whichever one is fired.
            d.frenzy = frenzy_single;
            d.locked_buffs = frenzy_locks.clone();
            (panel, d)
        }
    };
    // An arcane the weapon cannot seat is an ERROR here, not a silent drop:
    // the sim is the one place a visitor is owed a reason.
    for (pool, aid, _) in arcane_choices(v, info) {
        if wfsim_engine::arcanes_data::for_slot(&pool, &aid).is_none() {
            return err_json(match wfsim_engine::arcanes_data::slot_of(&aid) {
                Some(s) => format!(
                    "{aid} is a {s} arcane — {} seats {}",
                    info.name,
                    info.arcane_pools.join(" + ")
                ),
                None => format!("unknown arcane id: {aid}"),
            });
        }
    }
    // Relative crit conditionals resolve against the weapon's BASE crit
    // stats; `requires` gates on the weapon traits (Akimbo Slip Shot). Under
    // the sim's Emergent policy the non-simmable conditionals are honest
    // no-ops (same rule as mods' CondBuff).
    let ab = WeaponBase::from_data(incarnon_id(info).unwrap_or(&info.id), true, &evo_refs);
    params.arcane = arcane_fx_for(v, info, &ab, policy);
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
    // A WEAPON-damage row expands into the vector that dealt it — a status
    // row is already one type, which is what a proc is. Parts are ordered
    // biggest-first, the same rule the rows themselves follow.
    let by_type = |split: &[f64; 15]| -> Option<Value> {
        let mut parts: Vec<(&str, f64)> = split
            .iter()
            .enumerate()
            .filter(|(_, v)| **v > 0.0)
            .map(|(i, &v)| (TYPE_NAMES[i], v))
            .collect();
        if parts.is_empty() {
            return None;
        }
        parts.sort_by(|a, b| b.1.total_cmp(&a.1));
        Some(json!(parts
            .iter()
            .map(|(t, v)| json!({ "type": t, "dmg": v }))
            .collect::<Vec<Value>>()))
    };
    let mut sources: Vec<(String, f64, Option<Value>)> = vec![
        ("direct".to_string(), sd.direct, by_type(&sd.direct_by_type)),
        ("radial".to_string(), sd.radial, by_type(&sd.radial_by_type)),
        // The lingering FIELD is its own bucket — on the Torid it is most of the
        // output, and leaving it out silently lost it from the damage meter.
        ("field".to_string(), sd.field, by_type(&sd.field_by_type)),
        // Cascadia Empowered's instance matches the PROC's type, so this row
        // expands like the weapon-damage ones (user's rule for the direct row,
        // 2026-08-01: the damage has elements, so the meter should say which).
        ("arcane".to_string(), sd.arcane_on_status, by_type(&sd.arcane_by_type)),
    ];
    sources.extend(
        sd.status
            .iter()
            .enumerate()
            .map(|(i, &v)| (TYPE_NAMES[i].to_string(), v, None)),
    );
    sources.retain(|(_, v, _)| *v > 0.0);
    sources.sort_by(|a, b| b.1.total_cmp(&a.1));
    let damage_sources: Vec<Value> = sources
        .iter()
        .map(|(k, v, parts)| match parts {
            Some(p) => json!({ "source": k, "dmg": v, "by_type": p }),
            None => json!({ "source": k, "dmg": v }),
        })
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
        // The tier, because the RATE stops saying anything past 100% crit
        // chance: every pellet crits, so it reads 1.0 whether the build is
        // at 110% or 410%. Uncapped — red is not the top.
        "crit_tier": m.crit_tier_sum as f64 / pel,
        "headshot_rate": m.headshots as f64 / pel,
        "procs": m.procs,
        "field_ticks": m.field_ticks,
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
    // Rivens the request carries join the searchable pool like any mod.
    if let Err(e) = riven_stat_ids_ok(v, info) {
        return err_json(e);
    }
    let full = mod_pool_with_rivens(v, info);
    let refs: Vec<&ModDef> = full
        .iter()
        .filter(|m| ids.iter().any(|id| id.as_str() == m.id))
        .collect();
    let mut out: Vec<BuffMeta> = Vec::new();
    let none = wfsim_engine::arcanes_data::ArcaneFx::none();
    merge(&mut out, enumerate_buffs(&refs, &none, info));
    let arc_base = WeaponBase::from_data(&info.id, true, &[]);
    // The scope is a MARK MAP (id -> "search" | "fixed"), the same shape as
    // `mods`; every marked arcane's buffs are configurable, pins included.
    if let Some(obj) = v.get("arcanes").and_then(|x| x.as_object()) {
        for a in obj.keys().filter(|k| k.as_str() != "none") {
            if let Some(def) = arcane_in_pools(info, a) {
                let fx = def.fx(def.max_rank, StackPolicy::Emergent, arc_base.traits);
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
    /// What each entry of `arcanes` IS, in pool order — one id per slot,
    /// "none" for an empty one. The effects are merged and cannot be read
    /// back apart, so the naming travels beside them.
    arcane_sets: Vec<Vec<String>>,
    scenario: Scenario,
    final_runs: u32,
    finalists: usize,
    headshot_pct: f64,
    duration: f64,
    target_name: String,
    level: u32,
    steel_path: bool,
    /// The form this run FIRES — a weapon id, because a form is a weapon
    /// entry. Every weapon has one; only a gauge-switched pair has two.
    fire_id: String,
    /// The form the cycle transforms OUT of. `Some` only when the run is the
    /// two-form cycle, which is a MODE over forms rather than a form.
    cycle_from: Option<String>,
    /// The evolution that UNLOCKS the second form, and the form to fall back
    /// to without it. Evolutions are a search dimension, so one scope can
    /// hold sets that transform and sets that cannot — which of the two a
    /// candidate is depends on its own set, not on the run.
    unlock_evo: Option<String>,
    untransformed_id: String,
    /// Worker-thread budget; 0 = auto (all cores minus two).
    threads: usize,
}

/// Validate an optimize request. `Err` is the ready-to-send error response.
pub fn parse_optimize(v: &Value) -> Result<OptimizePlan, Value> {
    let info = weapon(get_str(v, "weapon", default_weapon_id()));
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
    // Rivens the request carries join the searchable pool like any mod.
    if let Err(e) = riven_stat_ids_ok(v, info) {
        return Err(err_json(e));
    }
    let full = mod_pool_with_rivens(v, info);
    for id in fixed_ids.iter().chain(search_ids.iter()) {
        if !full.iter().any(|m| m.id == id.as_str()) {
            return Err(err_json(mod_not_here(id, info)));
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
    // The same shape as `mods` and `exilus`: id -> "search" | "fixed". A pin
    // says "this slot is settled", which a flat list of ids cannot say.
    let arc_marks: Vec<(String, String)> = v
        .get("arcanes")
        .and_then(|x| x.as_object())
        .map(|o| {
            o.iter()
                .filter_map(|(k, m)| m.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let arc_base = WeaponBase::from_data(&info.id, true, &[]);
    // ONE AXIS PER SLOT, then their CROSS PRODUCT — a weapon that seats two
    // arcanes is searched over pairs, because "the best Primary" and "the
    // best Secondary" are not independent questions: an on-kill Secondary is
    // worth more next to a Primary that gets you the kill.
    //
    // The funnel is untouched by this. Its arcane axis has always been a flat
    // `Vec<ArcaneFx>` indexed by a job, and a merged pair IS one `ArcaneFx`
    // (see `ArcaneFx::merged`) — so the product is built here and nothing
    // downstream learns that a weapon can seat two.
    //
    // An arcane the weapon cannot equip is DROPPED from the scope, not mapped
    // to the empty slot: collapsing it would search "no arcane" once per
    // rejected id and report those runs as if they had been real options.
    // Each slot also always offers the EMPTY choice, so "one arcane, not two"
    // stays reachable — the scope says what MAY be worn, not what must be.
    let per_slot: Vec<Vec<(String, wfsim_engine::arcanes_data::ArcaneFx)>> = info
        .arcane_pools
        .iter()
        .map(|pool| {
            let mine: Vec<(&String, &String)> = arc_marks
                .iter()
                .filter(|(id, _)| wfsim_engine::arcanes_data::for_slot(pool, id).is_some())
                .map(|(id, m)| (id, m))
                .collect();
            let fx = |id: &str| {
                wfsim_engine::arcanes_data::for_slot(pool, id)
                    .map(|d| d.fx(d.max_rank, StackPolicy::Emergent, arc_base.traits))
                    .unwrap_or_else(wfsim_engine::arcanes_data::ArcaneFx::none)
            };
            // A PIN settles the slot: one option, and no empty choice.
            if let Some((id, _)) = mine.iter().find(|(_, m)| m.as_str() == "fixed") {
                return vec![((*id).clone(), fx(id))];
            }
            // EMPTY is an option only when the slot has no candidates.
            //
            // An arcane slot costs nothing — no capacity, no Forma — so
            // leaving it empty can never beat filling it with something that
            // helps, and marking a candidate IS the statement that the slot
            // should be filled (user, 2026-08-01: "没有理由放空一个"). Keeping
            // `none` alongside doubled the space per slot and put builds with
            // a hole in them on the results board, where they can only ever
            // tie the same build with the arcane in it.
            //
            // A slot with nothing marked still resolves to `none`, which is
            // what an empty slot IS — that case is the `else` below.
            let marked: Vec<_> = mine.iter().filter(|(_, m)| m.as_str() == "search").collect();
            if marked.is_empty() {
                return vec![("none".to_string(), wfsim_engine::arcanes_data::ArcaneFx::none())];
            }
            marked.into_iter().map(|(id, _)| ((*id).clone(), fx(id))).collect()
        })
        .collect();
    // The product, in pool order: `arcane_sets[i]` names what `arcanes[i]` is.
    let mut arcane_sets: Vec<Vec<String>> = vec![Vec::new()];
    let mut arcanes: Vec<wfsim_engine::arcanes_data::ArcaneFx> =
        vec![wfsim_engine::arcanes_data::ArcaneFx::none()];
    for slot in &per_slot {
        let mut ids = Vec::new();
        let mut fxs = Vec::new();
        for (set, fx) in arcane_sets.iter().zip(arcanes.iter()) {
            for (id, add) in slot {
                let mut s = set.clone();
                s.push(id.clone());
                ids.push(s);
                fxs.push(wfsim_engine::arcanes_data::ArcaneFx::merged(&[
                    fx.clone(),
                    add.clone(),
                ]));
            }
        }
        arcane_sets = ids;
        arcanes = fxs;
    }

    // No cap (user: allow spending local resources). The funnel handles large
    // spaces by culling obviously-bad combos in cheap early rounds.

    // ---- final-round contract (user, 2026-07-28): the last round is
    // guaranteed `finalists` candidates × `final_runs` runs; everything
    // before only whittles the field down (schedule + adaptive racing).
    let final_runs = get_u32(v, "final_runs", 100).clamp(1, 100_000);
    let finalists = get_u32(v, "finalists", 10).clamp(1, 100) as usize;

    // ---- scenario (reuse the Sim inputs) ----
    let enemy_id = get_str(v, "enemy", "thrax_centurion");
    let level = get_u32(v, "level", 9999).clamp(1, 9999);
    let steel_path = get_bool(v, "steel_path", true);
    let headshot_pct = get_f64(v, "headshot_pct", default_headshot_pct(info));
    // Same scenario knob as the Sim: the optimizer must score builds under the
    // assumption the sim will replay them with, or the winner is scored on a
    // buff the replay never grants.
    let aiming = get_bool(v, "aiming", true);
    let duration = get_f64(v, "duration", 300.0).clamp(1.0, 3600.0);
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
    // ---- WHICH FORM this search fires — the Sim's rule, not a second one.
    //
    // A cycle is a MODE over two forms and exists only where there is a form
    // to transform INTO. Demanding one of every weapon is what refused the
    // Verglas outright (user, 2026-07-31): a sentinel beam HAS a form, it
    // just has one, and a bow has two that are not a cycle. The optimizer now
    // assembles the same way the builder and the sim do — from the weapon's
    // own registered forms.
    let form = get_str(v, "form", "default");
    let registered = wfsim_engine::weapons_data::forms_of(&info.id);
    let form = if form == "default" && info.has_cycle { "incarnon_cycle" } else { form };
    let run_cycle = form == "incarnon_cycle" && info.has_cycle && incarnon_id(info).is_some();
    let fire_id = if run_cycle {
        incarnon_id(info).unwrap_or(&info.id).to_string()
    } else {
        // The requested kind if this weapon registers it, else its default —
        // which is what an unknown or stale preset value gets.
        registered
            .iter()
            .find(|f| f.kind.id() == form)
            .or_else(|| registered.iter().find(|f| f.is_default))
            .map(|f| f.weapon_id)
            .unwrap_or(&info.id)
            .to_string()
    };
    let cycle_from = run_cycle.then(|| info.id.clone());
    // Tier 1 is the Incarnon Form unlock; an evolution set without it is a
    // weapon that cannot transform, exactly as the builder shows it.
    let unlock_evo = form_unlock_evo(info).map(String::from);
    let untransformed_id = registered
        .iter()
        .find(|f| f.is_default && !f.kind.is_gauge_switched())
        .or_else(|| registered.iter().find(|f| !f.kind.is_gauge_switched()))
        .map(|f| f.weapon_id)
        .unwrap_or(&info.id)
        .to_string();

    let scenario = Scenario {
        aiming,
        target,
        body_parts,
        frenzy,
        duration_secs: duration,
        incarnon_cycle: run_cycle,
        frenzy_lock,
        frenzy_locks: frenzy_apply(buff_cfg.get("frenzy")).1,
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
        arcane_sets,
        scenario,
        final_runs,
        finalists,
        headshot_pct,
        duration,
        target_name: s_name(&specs, enemy_id),
        level,
        steel_path,
        fire_id,
        cycle_from,
        unlock_evo,
        untransformed_id,
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
/// Where a previous session stopped: the round to start at, and the field that
/// round takes as input, each entry the IDENTITY of a job — (ordered pool
/// indices, evolution-set index, exilus choice, arcane index). Identities only:
/// resolved panels are rebuilt, so a checkpoint stays small and cannot drift
/// from what the enumerator would produce.
#[derive(Debug, Clone)]
pub enum ResumeFrom {
    /// Mid-SCREEN, on a scope large enough to stream. The screen has no rounds
    /// in it — it is one pass over the whole scope — so this is the only way a
    /// reload during it does not cost the whole pass.
    Screen {
        /// Candidates the previous session had walked.
        start_seq: usize,
        /// The survivors at that cut, `(sequence number, arcane index)`. Not
        /// builds: the walk is deterministic, so re-walking regenerates them.
        keepers: Vec<(usize, usize)>,
    },
    /// After a completed funnel ROUND.
    Round {
        round: usize,
        alive: Vec<JobIdentity>,
        /// The job count the ORIGINAL run's schedule was built from. The round
        /// plan is a function of it, so replaying round N needs the same
        /// number — deriving it from the (already narrowed) survivor list
        /// would shorten the schedule and change what round N means.
        jobs_at_start: usize,
    },
}

/// One surviving job, by identity: (ordered pool indices, evolution-set index,
/// exilus choice, arcane index).
pub type JobIdentity = (Vec<usize>, u32, u32, usize);

/// Where a completed round publishes its field: `(next_round, jobs the
/// schedule was built from, survivors, that round's leaderboard)`. The
/// leaderboard rides along because the same snapshot answers both questions a
/// killed run leaves open — where to continue, and what it had found.
pub type CheckpointSink<'a> = dyn Fn(usize, usize, &[JobIdentity], &Value) + 'a;

/// Where the SCREEN publishes its best-so-far. Result-shaped, so a cancel
/// renders it through the same path a finished run takes. Display only: the
/// screen is one pass over the whole scope, so a snapshot of it is NOT a
/// resume point — continuing from one would silently drop the unwalked part.
pub type BoardSink<'a> = dyn Fn(&Value) + 'a;

/// The uninterrupted entry point — no checkpointing, no resume.
pub fn run_optimize(
    plan: OptimizePlan,
    state: &FunnelState,
    on_enumerated: impl FnOnce(usize, usize),
    on_round: Option<&dyn Fn()>,
) -> Value {
    run_optimize_resumable(plan, state, on_enumerated, on_round, None, None, None, None)
}

/// As [`run_optimize`], plus the two halves of resumability: `resume` skips
/// straight to a saved round, and `on_checkpoint` publishes the field after
/// every completed round.
#[allow(clippy::too_many_arguments)]
pub fn run_optimize_resumable(
    plan: OptimizePlan,
    state: &FunnelState,
    on_enumerated: impl FnOnce(usize, usize),
    on_round: Option<&dyn Fn()>,
    resume: Option<ResumeFrom>,
    // (next_round, jobs_at_start, identities). The funnel hands back candidate
    // INDICES, which mean nothing outside this call — translate them here,
    // where the candidate table is, so a caller can persist something that
    // survives the process.
    on_checkpoint: Option<&CheckpointSink<'_>>,
    // Best-so-far during the screen. A browser cancel TERMINATES the worker,
    // so a leaderboard that has not already left it is lost (user 2026-07-30:
    // 20 minutes, cancelled, nothing shown).
    on_board: Option<&BoardSink<'_>>,
    // `(candidates walked, survivors as (seq, arcane))` — a mid-screen resume
    // point. Only the serial screen produces one; see `ScreenSnapshotFn`.
    on_screen_snapshot: Option<&wfsim_optimizer::ScreenSnapshotFn<'_>>,
) -> Value {
    let OptimizePlan {
        pool,
        constraints,
        min_slots,
        build_size,
        evo_sets,
        exilus_defs,
        arcanes,
        arcane_sets,
        scenario,
        final_runs,
        finalists,
        headshot_pct,
        duration,
        target_name,
        level,
        steel_path,
        weapon_id,
        fire_id,
        cycle_from,
        unlock_evo,
        untransformed_id,
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
    // The form(s) an evolution set resolves to, decided once in
    // `parse_optimize`. A single-form weapon has NO second panel — handing
    // the enumerator a duplicate of the first would tell it there was a cycle
    // to simulate, and the scenario says there is not.
    let forms_for = |set: &[String], refs: &[&str]| {
        // Can THIS evolution set reach the second form? Without the unlock
        // there is nothing to transform into, so the candidate is fired in
        // the form it has and carries no second panel — which is what tells
        // `evaluate` not to run a cycle for it.
        let unlocked = match unlock_evo.as_deref() {
            Some(u) => set.iter().any(|e| e == u),
            None => true,
        };
        if !unlocked {
            return (
                WeaponBase::from_data(&untransformed_id, true, refs),
                None,
            );
        }
        (
            WeaponBase::from_data(&fire_id, true, refs),
            cycle_from
                .as_ref()
                .map(|id| WeaponBase::from_data(id, true, refs)),
        )
    };
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

    // One leaderboard row. The finished result and every best-so-far snapshot
    // both go through this, so a cancelled run renders in exactly the same UI
    // as a completed one — same fields, same renderer.
    let entry = |rank: usize, c: &Candidate, ai: usize, s: &Summary| -> Value {
        let mods: Vec<&str> = c.ordered.iter().map(|&i| pool[i].id).collect();
        // One id per slot, in pool order — the same shape the builder takes,
        // because "apply this result" should be a copy and not a translation.
        let ids: Vec<String> = arcane_sets
            .get(ai)
            .cloned()
            .unwrap_or_else(|| vec!["none".to_string()]);
        let ranks: Vec<u32> = ids
            .iter()
            .map(|id| {
                wfsim_engine::arcanes_data::slot_of(id)
                    .and_then(|s| wfsim_engine::arcanes_data::for_slot(s, id))
                    .map(|d| d.max_rank)
                    .unwrap_or(0)
            })
            .collect();
        json!({
            "rank": rank + 1,
            "kills": s.mean_kills,
            "kill_progress": s.mean_kill_progress,
            "dps": s.effective_dps,
            "kills_min": s.min_kills,
            "kills_max": s.max_kills,
            "mods": mods,
            "arcane": ids,
            "arcane_rank": ranks,
            "evolutions": evo_sets[c.variant as usize],
            "exilus": exilus_defs[c.exilus as usize].as_ref().map(|m| m.id).unwrap_or("none"),
            "forma": { "used": c.plan.forma_used, "total_drain": c.plan.total_drain },
        })
    };
    // A whole result payload. Snapshots carry `cancelled: true` — a board only
    // ever gets shown because a run stopped early, and the flag is what makes
    // the UI label it best-so-far (lower precision than a full run).
    let board_json = |rows: Vec<Value>, n_cands: usize, n_jobs: usize| -> Value {
        json!({
            "ok": true, "cancelled": true,
            "candidates": n_cands, "jobs": n_jobs,
            "final_runs": final_runs, "finalists": finalists,
            "headshot_pct": headshot_pct, "duration": duration,
            "results": rows,
            "target": { "name": target_name, "level": level, "steel_path": steel_path },
        })
    };

    let mut cands: Vec<Candidate> = Vec::new();
    // Decide the regime BEFORE walking, from the scope's own size. Walking
    // first and waiting for the count to cross MATERIALIZE_LIMIT is what made
    // a full pool look dead: the legal builds run out early (a 60-capacity cap
    // rejects most subsets) so the counter freezes, while the walk still has
    // C(72,8) ~ 1.1e10 nodes of illegal territory to grind before it can say
    // it is finished. The estimate is exact enough — it is a threshold test,
    // not a number anyone reads.
    let n_usable = pool
        .iter()
        .filter(|m| !constraints.forbid.iter().any(|f| f == m.id))
        .count();
    let subsets: f64 = (min_slots..=build_size)
        .map(|k| {
            // C(n, k), saturating: anything astronomically large only has to
            // compare greater than the limit.
            (0..k).fold(1.0f64, |acc, i| acc * (n_usable as f64 - i as f64) / (i as f64 + 1.0))
        })
        .sum();
    let scope_estimate = subsets
        * evo_sets.len().max(1) as f64
        * exilus_refs.len().max(1) as f64;
    let mut overflow = scope_estimate > MATERIALIZE_LIMIT as f64;
    for (vi, set) in evo_sets.iter().enumerate() {
        if overflow {
            break;
        }
        let refs: Vec<&str> = set.iter().map(String::as_str).collect();
        let (base, base_form) = forms_for(set, &refs);
        let (mut c, _stats, complete) = enumerate_candidates_observed(
            &pool,
            &base,
            base_form.as_ref(),
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

    // The two resume kinds land in different regimes: a screen cut only ever
    // comes from the streaming path and goes straight back into it, a round
    // checkpoint skips the walk entirely.
    let screen_resume: Option<wfsim_optimizer::ScreenResume> = match &resume {
        Some(ResumeFrom::Screen { start_seq, keepers }) => {
            let mut map: std::collections::HashMap<usize, Vec<usize>> =
                std::collections::HashMap::new();
            for &(s, a) in keepers {
                map.entry(s).or_default().push(a);
            }
            Some(wfsim_optimizer::ScreenResume { start_seq: *start_seq, keepers: map })
        }
        _ => None,
    };
    let round_resume = match resume {
        Some(ResumeFrom::Round { round, alive, jobs_at_start }) => {
            Some((round, alive, jobs_at_start))
        }
        _ => None,
    };
    let (cands, last, cancelled, n_jobs) = if let Some((r_round, r_alive, r_jobs_at_start)) = round_resume {
        // ---- RESUME: no walk at all. The checkpoint holds identities, so the
        // candidates are rebuilt with the same plan_forma / resolve_with the
        // enumerator uses and come out bit-identical. Seeds key off the
        // absolute round index, so the numbers match an uninterrupted run.
        let mut cands: Vec<Candidate> = Vec::new();
        let mut jobs: Vec<Job> = Vec::new();
        for (ordered, variant, exilus, ai) in &r_alive {
            let Some(set) = evo_sets.get(*variant as usize) else { continue };
            let refs: Vec<&str> = set.iter().map(String::as_str).collect();
            let (base, base_form) = forms_for(set, &refs);
            let Some(c) = wfsim_optimizer::rebuild_candidate(
                &pool, &base, base_form.as_ref(), &innate, 60, scenario.aiming,
                ordered, *variant, *exilus, &exilus_refs,
            ) else { continue };
            if *ai >= arcanes.len() {
                continue;
            }
            cands.push(c);
            jobs.push((cands.len() - 1, *ai));
        }
        if jobs.is_empty() {
            // A checkpoint that survives a pool or Forma change resolves to
            // nothing: say so rather than returning an empty leaderboard.
            return err_json("this saved run no longer matches the current scope — start a new one");
        }
        let n_jobs = jobs.len();
        on_enumerated(cands.len(), n_jobs);
        // The schedule is a function of the ORIGINAL field size, which the
        // checkpoint's round index indexes into — rebuild it the same way so
        // round N means the same thing it did before the reload.
        let rounds = schedule_to(r_jobs_at_start.max(n_jobs), final_runs, finalists);
        let ids_at = |alive: &[(Job, Summary)]| -> Vec<JobIdentity> {
            alive.iter()
                .map(|&((ci, ai), _)| (cands[ci].ordered.clone(), cands[ci].variant, cands[ci].exilus, ai))
                .collect()
        };
        let board_of = |alive: &[(Job, Summary)], nc: usize, nj: usize| -> Value {
            board_json(
                alive.iter().take(finalists).enumerate()
                    .map(|(rank, ((ci, ai), s))| entry(rank, &cands[*ci], *ai, s))
                    .collect(),
                nc, nj,
            )
        };
        let started_with = r_jobs_at_start.max(n_jobs);
        let n_cands = cands.len();
        // A round is ONE blocking call, and round 1 is the whole field before
        // any culling — round boundaries are too coarse to answer a cancel.
        let rboard = on_board.map(|b| move |top: &[(Job, Summary)]| {
            b(&board_of(top, n_cands, n_jobs));
        });
        let wrap = on_checkpoint.map(|cp| move |round: usize, alive: &[(Job, Summary)]| {
            cp(round, started_with, &ids_at(alive), &board_of(alive, n_cands, n_jobs));
        });
        let last = run_funnel(
            &cands, &arcanes, &scenario, jobs, &rounds, 0xDEAD_BEEF, false,
            Some(state), on_round, r_round,
            wrap.as_ref().map(|f| f as &wfsim_optimizer::CheckpointFn<'_>),
            rboard.as_ref().map(|f| f as &wfsim_optimizer::RoundBoardFn<'_>),
        );
        let c = state.cancel.load(std::sync::atomic::Ordering::Relaxed);
        (cands, last, c, n_jobs)
    } else if !overflow && screen_resume.is_none() {
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
        let ids_at = |alive: &[(Job, Summary)]| -> Vec<JobIdentity> {
            alive.iter()
                .map(|&((ci, ai), _)| (cands[ci].ordered.clone(), cands[ci].variant, cands[ci].exilus, ai))
                .collect()
        };
        let board_of = |alive: &[(Job, Summary)], nc: usize, nj: usize| -> Value {
            board_json(
                alive.iter().take(finalists).enumerate()
                    .map(|(rank, ((ci, ai), s))| entry(rank, &cands[*ci], *ai, s))
                    .collect(),
                nc, nj,
            )
        };
        let n_cands = cands.len();
        let rboard = on_board.map(|b| move |top: &[(Job, Summary)]| {
            b(&board_of(top, n_cands, n_jobs));
        });
        let wrap = on_checkpoint.map(|cp| move |round: usize, alive: &[(Job, Summary)]| {
            cp(round, n_jobs, &ids_at(alive), &board_of(alive, n_cands, n_jobs));
        });
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
            0,
            wrap.as_ref().map(|f| f as &wfsim_optimizer::CheckpointFn<'_>),
            rboard.as_ref().map(|f| f as &wfsim_optimizer::RoundBoardFn<'_>),
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
        // The screen is the long silent phase; publish its running top slice so
        // a cancel there has numbers instead of an empty page.
        let screen_board = on_board.map(|b| {
            move |top: &[wfsim_optimizer::ScreenedJob]| {
                let rows: Vec<Value> = top.iter().take(finalists).enumerate()
                    .map(|(rank, sj)| entry(rank, &sj.cand, sj.ai, &sj.summary))
                    .collect();
                // Report what has been WALKED and SCREENED, not the snapshot's
                // own length — the screen runs at one run per job, so
                // `sims_done` is exactly the jobs it has ranked so far.
                let walked = state.enumerated.load(std::sync::atomic::Ordering::Relaxed) as usize;
                let screened = state.sims_done.load(std::sync::atomic::Ordering::Relaxed) as usize;
                b(&board_json(rows, walked, screened));
            }
        });
        let (screened, complete) = stream_screen(
            |emit| {
                for (vi, set) in evo_sets.iter().enumerate() {
                    let refs: Vec<&str> = set.iter().map(String::as_str).collect();
                    let (base, base_form) = forms_for(set, &refs);
                    if !enumerate_candidates_each(
                        &pool,
                        &base,
                        base_form.as_ref(),
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
            screen_board.as_ref().map(|f| f as &wfsim_optimizer::ScreenBoardFn<'_>),
            screen_resume.as_ref(),
            on_screen_snapshot,
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
            // The screen itself is not resumable — it is a single walk of the
            // whole scope. Its OUTPUT is: once the survivors are a candidate
            // table, every funnel round can be checkpointed by identity, and a
            // resume rebuilds them directly instead of screening again.
            let ids_at = |alive: &[(Job, Summary)]| -> Vec<JobIdentity> {
                alive.iter()
                    .map(|&((ci, ai), _)| (sc[ci].ordered.clone(), sc[ci].variant, sc[ci].exilus, ai))
                    .collect()
            };
            let board_of_sc = |alive: &[(Job, Summary)], nc: usize, nj: usize| -> Value {
                board_json(
                    alive.iter().take(finalists).enumerate()
                        .map(|(rank, ((ci, ai), s))| entry(rank, &sc[*ci], *ai, s))
                        .collect(),
                    nc, nj,
                )
            };
            let n_sc = sc.len();
            let rboard = on_board.map(|b| move |top: &[(Job, Summary)]| {
                b(&board_of_sc(top, n_sc, n));
            });
            let wrap = on_checkpoint.map(|cp| move |round: usize, alive: &[(Job, Summary)]| {
                cp(round, n, &ids_at(alive), &board_of_sc(alive, n_sc, n));
            });
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
                0, // the streaming path always screens first, so it starts fresh
                wrap.as_ref().map(|f| f as &wfsim_optimizer::CheckpointFn<'_>),
                rboard.as_ref().map(|f| f as &wfsim_optimizer::RoundBoardFn<'_>),
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
        .map(|(rank, ((ci, ai), s))| entry(rank, &cands[*ci], *ai, s))
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

#[cfg(test)]
mod asset_tests {
    use super::*;

    /// Every weapon, mod and arcane in `data/` must have an image entry.
    ///
    /// A missing one does not fail anything at runtime — it renders as
    /// nothing, and the card just looks empty (Verglas Prime and ten mods
    /// shipped that way, user 2026-07-31). The map is filled by
    /// `scripts/gen_assets.py` from the committed WFCD export, so a failure
    /// here is one command away from fixed, and this is what makes anyone
    /// run it.
    #[test]
    fn every_data_entry_has_an_image() {
        let a = assets();
        let mut missing: Vec<String> = Vec::new();
        for w in weapons() {
            if !a.weapons.contains_key(&w.id) {
                missing.push(format!("weapon {}", w.id));
            }
        }
        for class in wfsim_engine::mods_data::classes() {
            for m in wfsim_engine::mods_data::class_pool(class) {
                if !a.mods.contains_key(m.id) {
                    missing.push(format!("mod {}", m.id));
                }
            }
        }
        for slot in wfsim_engine::arcanes_data::slots() {
            for arc in wfsim_engine::arcanes_data::slot_pool(slot) {
                if !a.arcanes.contains_key(arc.id.as_str()) {
                    missing.push(format!("arcane {}", arc.id));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "no image in data/assets.yaml for {} entries: {}
             run `python scripts/gen_assets.py --write`",
            missing.len(),
            missing.join(", ")
        );
    }
}

#[cfg(test)]
mod optimizer_arcane_tests {
    use super::*;

    fn sets(arcanes: Value) -> Vec<Vec<String>> {
        let plan = parse_optimize(&json!({
            "weapon": "larkspur_prime",
            "size": 1,
            "mods": { "rubedo_lined_barrel": "search" },
            "arcanes": arcanes,
        }))
        .expect("a plan");
        plan.arcane_sets
    }

    /// A slot with candidates is never left EMPTY.
    ///
    /// An arcane slot costs nothing — no capacity, no Forma — so empty can
    /// never beat filled, and marking a candidate IS the statement that the
    /// slot should be filled (user, 2026-08-01). Every slot used to carry an
    /// implicit "none", so marking 3 primaries and 4 secondaries enumerated
    /// 4 x 5 = 20 sets, eight of which had a hole in them and could only ever
    /// tie the same build with the arcane in it.
    #[test]
    fn a_slot_with_candidates_is_never_left_empty() {
        let s = sets(json!({
            "primary_deadhead": "search",
            "primary_merciless": "search",
            "primary_crux": "search",
            "secondary_deadhead": "search",
            "secondary_merciless": "search",
            "secondary_fortifier": "search",
            "secondary_shiver": "search",
        }));
        assert_eq!(s.len(), 12, "3 primaries x 4 secondaries, and nothing else");
        assert!(
            !s.iter().any(|set| set.iter().any(|id| id == "none")),
            "no set may leave a marked slot empty: {s:?}"
        );

        // A slot with NOTHING marked is still empty — that is what an empty
        // slot IS, and it is the only way "none" survives.
        let s = sets(json!({ "primary_deadhead": "search" }));
        assert_eq!(s, vec![vec!["primary_deadhead".to_string(), "none".to_string()]]);
        let s = sets(json!({}));
        assert_eq!(s, vec![vec!["none".to_string(), "none".to_string()]]);
    }
}

#[cfg(test)]
mod riven_stat_id_tests {
    use super::*;

    fn req(stat: &str) -> Value {
        json!({
            "weapon": "torid",
            "mods": ["riven:Test"],
            "rivens": [{"name": "Test", "spec": {
                "bonuses": [{"id": stat, "roll": 1.1}],
                "rank": 8, "polarity": "madurai"}}],
        })
    }

    /// A typo'd riven stat is an ERROR, not a blank card.
    ///
    /// `resolved_slots` drops a stat whose id is not in the pool, so before
    /// this the riven still equipped, still drained capacity, still showed
    /// its name — and granted nothing. The failure that found it: a request
    /// built with the stats' `kind` names (`multishot_bonus`) instead of
    /// their ids (`multishot`) simulated a full build to the DECIMAL of the
    /// same build with no riven at all.
    #[test]
    fn an_unknown_riven_stat_id_is_rejected() {
        let bad = simulate_json(&req("multishot_bonus"));
        assert_eq!(bad["ok"], json!(false), "typo'd stat id must not simulate");
        assert!(
            bad["error"].as_str().unwrap_or_default().contains("multishot_bonus"),
            "the error must name the offending id: {bad:?}"
        );
        let good = simulate_json(&req("multishot"));
        assert_eq!(good["ok"], json!(true), "the real id still works: {good:?}");
        assert!(
            (good["panel"]["multishot"].as_f64().unwrap_or(0.0) - 1.0).abs() > 0.5,
            "and it actually grants multishot: {:?}",
            good["panel"]
        );
    }
}

#[cfg(test)]
mod arcane_slot_tests {
    use super::*;

    /// An Arch-Gun seats TWO arcanes, one from each pool — "Archguns possess
    /// two Arcane Enhancement slots to equip one Primary Arcane and one
    /// Secondary Arcane" (wiki Arch-Gun) — and it is neither a Primary nor a
    /// Secondary weapon itself.
    #[test]
    fn an_archgun_seats_one_arcane_from_each_pool() {
        let lark = weapon("larkspur_prime");
        assert_eq!(lark.slot, "archgun", "its own equipment slot");
        assert_eq!(lark.arcane_pools, vec!["primary", "secondary"]);

        // Every other weapon still seats exactly one, named after its slot.
        assert_eq!(weapon("torid").arcane_pools, vec!["primary"]);
        assert_eq!(weapon("laetum").arcane_pools, vec!["secondary"]);
        // And a sentinel weapon seats none.
        assert!(weapon("verglas_prime").arcane_pools.is_empty());
    }

    /// Both chosen arcanes reach the sim, folded into one effect set. The
    /// pools are ORDERED, so entry i is the arcane for pool i and an id from
    /// the wrong pool resolves to nothing rather than being applied anyway.
    #[test]
    fn both_arcanes_apply_and_the_pools_stay_ordered() {
        let lark = weapon("larkspur_prime");
        let base = WeaponBase::from_data("larkspur_prime", true, &[]);
        let fx = |v: Value| arcane_fx_for(&v, lark, &base, StackPolicy::AssumedMax);

        let one = fx(json!({ "arcane": ["primary_deadhead"] }));
        let two = fx(json!({ "arcane": ["primary_deadhead", "cascadia_overcharge"] }));
        assert!(!one.id.is_empty(), "the primary alone resolves");
        assert!(two.id.contains('+'), "two folded: {}", two.id);
        assert!(
            two.cc_rel > one.cc_rel,
            "the secondary's crit chance joined: {} vs {}",
            two.cc_rel,
            one.cc_rel
        );

        // SWAPPED: each id is now in the other's slot, so neither is
        // equippable and nothing applies.
        let swapped = fx(json!({ "arcane": ["cascadia_overcharge", "primary_deadhead"] }));
        assert!(swapped.id.is_empty(), "wrong pool, wrong slot: {}", swapped.id);
    }

    /// ONE wire shape: a list, one entry per pool. A bare value is not a
    /// second spelling the server understands — the client rewrites storage
    /// to the list shape once, so nothing here reads two formats.
    #[test]
    fn the_wire_shape_is_a_list_and_only_a_list() {
        let torid = weapon("torid");
        let base = WeaponBase::from_data("torid", true, &[]);
        let fx = |v: Value| arcane_fx_for(&v, torid, &base, StackPolicy::AssumedMax);

        let listed = fx(json!({ "arcane": ["primary_deadhead"], "arcane_rank": [5] }));
        assert_eq!(listed.id, "primary_deadhead");
        assert!(listed.headshot_mult_bonus > 0.0);

        // A bare value is not a shape: it resolves to nothing rather than
        // being quietly accepted as a second way to say the same thing.
        assert!(fx(json!({ "arcane": "primary_deadhead" })).id.is_empty());

        // A weapon with one pool ignores a second entry: what it can seat is
        // the weapon's business, not the caller's.
        let extra = fx(json!({ "arcane": ["primary_deadhead", "cascadia_overcharge"] }));
        assert_eq!(extra.id, "primary_deadhead");
    }
}

#[cfg(test)]
mod form_tests {
    use super::*;

    fn sim(weapon: &str, form: &str) -> Value {
        simulate_json(&json!({
            "weapon": weapon, "form": form, "mods": [], "arcane": "none",
            "enemy": "thrax_centurion", "duration": 30.0, "runs": 8,
            "headshot_pct": 100.0, "seed": 7,
        }))
    }

    /// A weapon is fired in ITS OWN forms, with ITS OWN evolutions. Both used
    /// to leak across weapons from the same place — a request that named
    /// neither got Dual Toxocyst's evolutions and, for anything but `base`,
    /// an Incarnon cycle built on a borrowed gauge. A bow has neither, so it
    /// is the case that shows both.
    #[test]
    fn a_weapon_never_inherits_another_weapons_form_or_evolutions() {
        // The gauge one first: asking a bow for the two-form cycle cannot
        // produce a transformation, because it has nothing to transform into.
        for form in ["charged", "base", "incarnon_cycle", "primary", ""] {
            let r = sim("cernos_prime", form);
            assert_eq!(r["ok"], json!(true), "form {form}");
            assert_eq!(r["transforms"], json!(0), "bow transformed on form {form}");
        }
        assert_eq!(sim("verglas_prime", "incarnon_cycle")["transforms"], json!(0));

        // The evolution one: an unstated `evolutions` key falls back to the
        // historical Dual Toxocyst build, which must reduce to nothing here.
        // Cernos Prime's unmodded base is 184 per arrow (3 x 184 = the 552 the
        // wiki quotes); Dual Toxocyst's Commodore's Fortune would add 50.
        let bow = sim("cernos_prime", "charged");
        let base_of = |r: &Value| r["panel"]["total"].as_f64().expect("a base damage");
        assert!((base_of(&bow) - 184.0).abs() < 1e-6, "{}", base_of(&bow));
        assert_eq!(bow["panel"]["multishot"], json!(3.0));
        assert_eq!(bow["panel"]["crit_chance"], json!(0.35));

        // Both bow forms are the same weapon: half the base damage per arrow,
        // and the tap fires more often for it (no draw, wiki Fire Rate's bow
        // formula) — the trade that makes tapping a real pattern.
        let tapped = sim("cernos_prime", "base");
        assert!((base_of(&tapped) - 92.0).abs() < 1e-6, "{}", base_of(&tapped));
        let (drawn_shots, tapped_shots) = (bow["shots"].as_u64(), tapped["shots"].as_u64());
        assert_eq!(drawn_shots, Some(27), "30 s / (0.5 + 0.65) + 1");
        assert_eq!(tapped_shots, Some(47), "30 s / 0.65 + 1");
    }

    /// The registry publishes what each weapon actually has: its own forms,
    /// and separately whether any of them is transformed into.
    #[test]
    fn the_registry_publishes_each_weapons_own_forms() {
        let ids = |w: &WeaponInfo| w.forms.iter().map(|(i, _, _)| *i).collect::<Vec<_>>();
        let get = |id: &str| weapons().iter().find(|w| w.id == id).expect(id);

        let bow = get("cernos_prime");
        assert_eq!(ids(bow), ["charged", "base"], "the arsenal's form first");
        assert!(!bow.has_cycle, "two forms, but nothing to transform into");

        let torid = get("torid");
        assert_eq!(ids(torid), ["base", "incarnon"]);
        assert!(torid.has_cycle);

        let verglas = get("verglas_prime");
        assert_eq!(ids(verglas), ["base"]);
        assert!(!verglas.has_cycle);
    }
}
