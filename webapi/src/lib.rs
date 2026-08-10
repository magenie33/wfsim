// The weapon entry in `meta_json` is one `json!` literal deep enough to hit
// the macro's default expansion limit — reached when the evolution tile
// gained its "what this does not do yet" fields. A limit, not a smell:
// splitting the literal to please a counter would scatter one payload
// across helpers that exist for no other reason.
#![recursion_limit = "512"]
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
    pct as fpct, resolve, resolve_for, ModDef, ModEffect, ResolvedPanel, StackPolicy,
    WeaponBase,
};
use wfsim_engine::mods::{PlannedMod, Polarity};
use wfsim_optimizer::{
    enumerate_candidates_observed, run_funnel, schedule_to, Candidate, Constraints, FunnelState,
    Job, Scenario,
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
    /// DE's own icon per damage type, keyed by the lowercase type name. Wiki-
    /// hosted (the CDN 404s every one), so each carries the `wiki:` prefix.
    #[serde(default)]
    damage_types: std::collections::HashMap<String, String>,
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
    /// WHAT THIS WEAPON'S ENTRY DOES NOT MODEL, one sentence per gap, straight
    /// from the weapon file. Shown to the reader — a number that omits
    /// something owes them the sentence, not just the omission.
    unmodeled: Vec<String>,
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
    if let Some(st) = s.attack.shot_type {
        parts.push(st.label().to_string());
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

/// Every FORM's passive lines, deduped.
///
/// A roster row is one weapon and both halves are the reader's to know about —
/// the same rule `unmodeled` follows. This was the base entry's lines alone, so
/// a passive belonging to an Incarnon form had nowhere to appear: the Phenmor's
/// spool-down is declared on `phenmor_incarnon` and the page said nothing about
/// it (2026-08-10). Deduped, because a group's forms can carry the same perk.
fn passives_of(id: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in wfsim_engine::weapons_data::forms_of(id) {
        for line in wfsim_engine::weapons_data::passive_lines(f.weapon_id) {
            if !out.contains(&line) {
                out.push(line);
            }
        }
    }
    out
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
                    // The BASE entry's gaps and its Incarnon form's, together:
                    // a reader is looking at one weapon and both halves are
                    // theirs to know about.
                    unmodeled: wfsim_engine::weapons_data::forms_of(&s.id)
                        .iter()
                        .filter_map(|f| wfsim_engine::weapons_data::spec(f.weapon_id))
                        .flat_map(|x| x.unmodeled.iter().cloned())
                        .collect(),
                    subtype: title_case(&s.class),
                    sentinel,
                    forms,
                    has_cycle: wfsim_engine::weapons_data::has_gauge_switched_form(&s.id),
                    slot: s.slot.clone(),
                    uses_arcane: !sentinel,
                    // THE ENGINE'S ANSWER, not a second copy of the rule:
                    // `builds::validate_for_board` needs the same seat count to
                    // decide whether every arcane seat is filled.
                    arcane_pools: wfsim_engine::weapons_data::arcane_pools(&s.id)
                        .into_iter()
                        .map(String::from)
                        .collect(),
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
/// Per evolution of this weapon's group, the mods installing it takes OFF the
/// weapon — `{ "<evo id>": ["<mod id>", …] }`, entries with nothing to say
/// omitted.
///
/// Only a form-unlocking evolution can say anything today (it is the one that
/// gives the weapon a second firing mode), but it is computed by ASKING the
/// pool, not by assuming that: a stat evolution that ever changed a trigger
/// would be answered correctly without a line changing here.
fn evo_forbids(info: &WeaponInfo) -> serde_json::Map<String, Value> {
    let bare = wfsim_engine::mods_data::pool_for_weapon(&info.id);
    let group = evo_group(info);
    let mut out = serde_json::Map::new();
    for e in wfsim_engine::evolutions_data::pool().iter().filter(|e| e.weapon == group) {
        let with = wfsim_engine::mods_data::pool_for_build(&info.id, &[e.id.as_str()]);
        let lost: Vec<&str> = bare
            .iter()
            .map(|m| m.id)
            .filter(|id| !with.iter().any(|m| m.id == *id))
            .collect();
        if !lost.is_empty() {
            out.insert(e.id.clone(), json!(lost));
        }
    }
    out
}

fn form_unlock_evo(info: &WeaponInfo) -> Option<&'static str> {
    // BY ITS TAG, not by ladder position. It used to be "tier 1's first
    // option", which is a guess that happens to hold for the four Incarnon
    // weapons in the roster and says nothing about the fifth.
    let group = evo_group(info);
    wfsim_engine::evolutions_data::pool()
        .iter()
        .find(|e| e.weapon == group && e.unlocks_form().is_some())
        .map(|e| e.id.as_str())
}

/// The headshot rate a weapon is played at when nothing says otherwise.
///
/// A SENTINEL weapon is fired by the companion, which picks its own targets
/// and does not aim for the head — so 0, not the player's 100 (user,
/// 2026-07-31). It stays a knob: this is the default, not a ceiling.
/// The fight's TENNO — who is holding this weapon, and what they are doing.
///
/// ONE builder for both the simulator and the optimizer, on purpose: the
/// optimizer must score builds under the player the sim will replay them with,
/// and two readers of the same JSON is how that drifts a field at a time
/// (user, 2026-08-02). The neutral entry in `data/tenno/` is the starting
/// point and the request overrides what it knows, so a field nobody sent keeps
/// its documented default instead of a zero invented here.
///
/// A SENTINEL WEAPON IS ALWAYS AIMING (user, 2026-08-01, settling M18a). What
/// it cannot do is trigger the on-HEADSHOT half of an aiming mod, because it
/// never aims at the head — which the sim already gets right from the other
/// end: `default_headshot_pct` is 0 for a sentinel, so no headshot lands and
/// no on-headshot buff fires. So the state is on, the triggers stay dead, and
/// the request cannot say otherwise.
fn tenno_from(v: &Value, info: &WeaponInfo) -> wfsim_engine::tenno_data::Tenno {
    let mut t = wfsim_engine::tenno_data::default_tenno().clone();
    t.state.aiming = info.sentinel || get_bool(v, "aiming", true);
    t.state.invisible = get_bool(v, "invisible", t.state.invisible);
    t.state.airborne = get_bool(v, "airborne", t.state.airborne);
    // The WARFRAME behind the gun. Armor and energy are the two stats a weapon
    // arcane reads (Primary Bulwark, Primary Overcharge); 0 means "no frame
    // chosen", which is what the neutral Tenno says and what makes those
    // arcanes contribute nothing until you say otherwise.
    t.armor = get_f64(v, "wf_armor", t.armor).clamp(0.0, 100_000.0);
    t.energy = get_f64(v, "wf_energy", t.energy).clamp(0.0, 100_000.0);
    t.state.energy_pct = get_f64(v, "wf_energy_pct", t.state.energy_pct).clamp(0.0, 1.0);
    t
}

/// One and three decimals — a replay ships 600 frames per series, and full
/// f64 text triples the payload for digits no chart can draw.
fn r1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}
fn r3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

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
/// The pool a BUILD actually sees: the weapon's pools unioned, minus mods it
/// cannot equip (the beam-only mods need a continuous weapon; a Cannonade needs
/// semi-auto on every firing mode, so an unlocked Incarnon form takes it off).
///
/// `evos` is the build's chosen evolutions — the pool is a question about the
/// weapon AS CONFIGURED, not about the weapon.
fn mod_pool_for(weapon_id: &str, evos: &[&str]) -> Vec<ModDef> {
    wfsim_engine::mods_data::pool_for_build(weapon_id, evos)
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
///
/// It is also what a saved build gets the moment an EVOLUTION takes a mod off
/// the weapon (a Cannonade under an unlocked Incarnon form): the mod is in the
/// weapon's own pool and out of this build's, and saying "not in this weapon's
/// pool" there would be flatly untrue. Which of the two it is, is decided by
/// asking the pool twice.
fn mod_not_here(id: &str, weapon: &WeaponInfo, evos: &[&str]) -> String {
    let known = wfsim_engine::mods_data::classes()
        .into_iter()
        .any(|c| wfsim_engine::mods_data::class_pool(c).iter().any(|m| m.id == id));
    if !known {
        return format!("unknown mod id: {id}");
    }
    let bare = wfsim_engine::mods_data::pool_for_weapon(&weapon.id);
    if !evos.is_empty() && bare.iter().any(|m| m.id == id) {
        let name = bare.iter().find(|m| m.id == id).map(|m| m.name).unwrap_or(id);
        return format!(
            "{name} cannot be equipped on {} with these evolutions installed — \
             it needs the same trigger on every firing mode",
            weapon.name
        );
    }
    format!("{id} cannot be equipped on {} — it is not in this weapon's pool", weapon.name)
}

/// A weapon's base for THIS request, with the chosen DEPLOYMENT applied.
///
/// Where an Arch-Gun is fired changes its sustain and nothing else — same
/// damage, same mods, same riven — so the environment is a scenario knob and
/// not a second weapon (user, 2026-08-01). Absent or unknown leaves the
/// weapon on its own column, which is the one its fields state.
fn base_for(v: &Value, id: &str, evos: &[&str]) -> WeaponBase {
    let mut b = WeaponBase::from_data(id, true, evos);
    let dep = get_str(v, "deployment", "");
    if !dep.is_empty() {
        wfsim_engine::weapons_data::apply_deployment(&mut b, id, dep);
    }
    b
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

/// The build's pool PLUS the request's own rivens.
fn mod_pool_with_rivens(v: &Value, info: &WeaponInfo, evos: &[&str]) -> Vec<ModDef> {
    let mut p = mod_pool_for(&info.id, evos);
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
                "abilities": l.abilities,
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
                // DE's own name, straight from the yaml. This used to be
                // `prettify(m.id)` — a title-cased id, which is not the same
                // string: "Semi-Shotgun Cannonade" came back without its
                // hyphen, so the card's wiki link 404'd (user, 2026-08-03).
                "name": m.name,
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
                // WHAT WE DO NOT MODEL, said out loud. The card prefers DE's
                // own text, so an "out of scope" line that only lived in the
                // model description was never rendered — the mod looked like
                // it worked and did nothing.
                "not_modeled": m.unmodeled,
                "out_of_scope": m.out_of_scope,
                // ...and the PARTLY modelled case, which neither flag above can
                // say: Winds of Purity lands its Purity radial and does not
                // model its life steal, so calling the whole card unmodelled
                // would be a second untruth. Derived from what the loader
                // actually dropped.
                "unmodeled_effects": wfsim_engine::mods_data::unmodeled_effects(m.id),
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
                // WHERE it is fired, when that changes the weapon. Fewer than
                // two means the axis does not exist for it and nothing should
                // offer a choice - the same rule every other axis follows.
                "deployments": wfsim_engine::weapons_data::deployments_of(&w.id),
                // HOW THIS WEAPON CAN BE PLAYED, and which of those a ruler may
                // rank. Derived from its forms and one question about the second
                // one — does entering it cost a gauge you have to earn — so a
                // weapon added later needs no entry anywhere for the board to
                // hold it twice. See `weapons_data::play_modes`.
                "modes": wfsim_engine::weapons_data::play_modes(&w.id)
                    .iter()
                    .filter(|m| m.sustainable)
                    .map(|m| m.id)
                    .collect::<Vec<_>>(),
                // TWO FACTS ABOUT AMMO, and they were one until 2026-08-04.
                // `has_reserve` is whether there is a pool behind the magazine
                // at all — false only for a sentinel weapon ("Ammo Max: ∞ /
                // Ammo Type: None"), which is what makes the Infinite-ammo box
                // ticked-and-disabled there. `no_resupply` is whether the game
                // gives any way to refill it — false for everything but a
                // ground Arch-Gun, which is removed when empty.
                //
                // Reading one as the other disabled the box on the whole
                // roster, so the only weapon whose ammo you could adjust was
                // the one weapon whose ammo the game does not let you adjust.
                "has_reserve": wfsim_engine::weapons_data::spec(&w.id)
                    .and_then(|s| s.ammo_max)
                    .is_some_and(|a| a > 0.0),
                "no_resupply": wfsim_engine::weapons_data::spec(&w.id)
                    .is_some_and(|s| s.no_resupply),
                // A PASSIVE WE DO NOT MODEL, so the page can say the number is
                // a floor rather than let it read as the weapon's real output.
                // Empty today — Gotva Prime's was the only one and it is
                // modelled now — and kept because the NEXT weapon with a prose
                // passive should have somewhere honest to sit while it waits.
                "passive_unmodeled": false,
                // WHAT THIS WEAPON DOES BEYOND ITS STATS, generated by the
                // engine from the data that implements it — never a sentence
                // stored in the weapon file.
                //
                // EVERY FORM'S, like `unmodeled` beside it and for the same
                // reason: a roster row is one weapon and the reader is owed
                // both halves. It was the base entry's alone, so a passive that
                // belongs to an Incarnon form had nowhere to appear — the
                // Phenmor's spool-down is declared on `phenmor_incarnon` and
                // the page said nothing about it (2026-08-10). Deduped, since
                // a group's forms can carry the same perk.
                "passives": passives_of(&w.id),
                // WHAT THIS ENTRY DOES NOT MODEL, verbatim from the weapon file
                // — the one place a weapon yaml carries prose as a value, the
                // way the enemy files already do. A reader is owed the gap in
                // words, not only the number that omits it.
                "unmodeled": w.unmodeled.clone(),
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
                // ...and which of them each EVOLUTION takes away. An equip rule
                // is asked of every firing mode a weapon has, and installing the
                // Incarnon form adds one — so Dual Toxocyst wears a Cannonade
                // until tier 1 goes in (wiki, Semi-Pistol_Cannonade: "must have
                // Semi-Auto trigger type for both firing modes").
                //
                // The CONSEQUENCE, not the rule: the client used to re-implement
                // pool rules in JS and every one of them went stale (see `mods`
                // above). The engine answers "what does picking this cost you",
                // and the picker just subtracts.
                "evo_forbids": evo_forbids(w),
                "mod_class": w.mod_pools.last().cloned().unwrap_or_default(),
                "subtype": w.subtype,
                // The RAW class, beside the display one. An arcane's
                // `equip_classes` is keyed on it, and title-casing for display
                // is exactly the kind of transform that makes a comparison
                // silently fail.
                "class": wfsim_engine::weapons_data::spec(&w.id)
                    .map(|s| s.class.clone())
                    .unwrap_or_default(),
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
                // Evolution tiers THIS weapon has, keyed on its transform group
                // — the page needs it to tell a complete ladder from a partial
                // one, and the count differs per weapon (Laetum 5, a rifle 0).
                "evo_tiers": wfsim_engine::evolutions_data::tier_count(
                    wfsim_engine::weapons_data::spec(&w.id)
                        .and_then(|s| s.transform_group.as_deref())
                        .unwrap_or(&w.id),
                ),
                "uses_evo2": w.uses_evo2,
                // The tier-1 evolution that UNLOCKS the second form. Without
                // it there is nothing to transform into, and the sim already
                // falls back to the base form — but the client was offering
                // "Incarnon cycle" anyway, so the panel said one thing and the
                // run did another (user, 2026-08-01). Now it can ask.
                "unlock_evo": form_unlock_evo(w),
                // A sentinel weapon has no arcane slot. This was hardcoded to
                // 1 while every weapon in the roster had one.
                "arcane_slots": w.arcane_pools.len(),
                "image": assets().weapons.get(&w.id),
                // NO `board` HERE. The board changes hourly and `data/` is embedded
                // at COMPILE time, so serving it from meta made every board
                // update a full wasm rebuild — install wasm-bindgen, fetch 300
                // images, recompile — to change a few numbers. It is fetched at
                // runtime from `/board.json` instead (`loadBoard` in app.js),
                // written by `wfsim-board` beside the canonical yaml.
                "innate_polarities": innate_slots_for(&w.id).iter()
                    .map(|p| p.map(|x| format!("{x:?}")))
                    .collect::<Vec<_>>(),
                "forms": w.forms.iter()
                    .map(|(id, name, def)| json!({
                        "id": id, "name": name, "is_default": def,
                        // Is this the form the GAUGE switches into? Then it
                        // exists only while its unlock is installed — and a mod
                        // that cannot be worn beside that unlock (`evo_forbids`)
                        // says the weapon does not have one, so the option goes
                        // with it rather than the sim refusing the build later.
                        "gauge_switched": wfsim_engine::weapons_data::forms_of(&w.id)
                            .iter()
                            .any(|f| f.kind.id() == *id && f.kind.is_gauge_switched()),
                    }))
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
                                // THE CAVEAT BELONGS WHERE THE CHOICE IS MADE.
                                // This perk's flat base damage does not feed
                                // the Condition Overload term, so a reader
                                // comparing it against the tier's other option
                                // sees "+60 base and +33% per status" and
                                // concludes it is strictly better. The stats
                                // panel said so on the CO row; the tile you
                                // pick from did not, and that is where the
                                // question gets asked (reported 2026-08-05).
                                "co_excluded": e.co_base_excludes_this_evolution,
                                // WHAT IT DOES NOT DO YET, on the tile where
                                // the choice is made. An evolution with an
                                // inert effect used to look exactly like a
                                // working one: same card, same tier, and a
                                // number that never moved. Naming the gap is
                                // the whole point — a tier where two of three
                                // options do nothing is not a choice, and the
                                // player is the last person who should have to
                                // discover that by measuring.
                                // DERIVED from the loaded effects, so it can
                                // never drift from what is actually modelled.
                                "unmodeled": e.unmodeled_effects(),
                                "fully_unmodeled": e.fully_unmodeled(),
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
                // The portrait comes from the enemy's own file, not from
                // assets.yaml: enemy art is wiki-hosted (see the yaml).
                "image": e.image,
                "base_level": e.stats.base_level,
                "can_be_eximus": e.can_be_eximus,
                // What the TARGET PICKER searches and shows. A name alone is
                // not enough to pick between units that differ in the two
                // things a build cares about — who they belong to and what
                // they are made of.
                // The COMBAT faction (what a Bane mod answers to), not the
                // scaling one — they differ, and the picker is about what a
                // build cares about.
                "faction": e.combat_faction.clone().unwrap_or_else(|| "unknown".into()),
                "scaling": format!("{:?}", e.scaling_faction).to_lowercase(),
                "health": e.stats.health,
                "shield": e.stats.shield,
                "armor": e.stats.armor,
                "overguard": e.stats.overguard,
                // Known gaps, stated on the card rather than left implicit.
                "unmodeled": e.unmodeled,
                // The post-U36 vulnerability COLUMN (System B), only the
                // entries that are not 1.0 — what this unit takes more or
                // less of, which is half of what picks a build's elements.
                // Keyed by FactionDamageOverride ?? Faction, so a Thrax shows
                // Zariman's Void x1.5 while answering to no faction mod.
                "type_modifiers": wfsim_engine::factions_data::columns_for(e.damage_column_key())
                    .faction
                    .listed()
                    .into_iter()
                    .map(|(t, m)| json!({ "type": t.name(), "mult": m }))
                    .collect::<Vec<_>>(),
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
            "not_modeled": a.has_unmodeled(),
            // …and the PARTLY-modelled case, which was silent until 2026-08-08.
            // Same field name and same meaning as a mod's, so the card renders
            // both with one function: everything else on this arcane works and
            // these do not.
            "unmodeled_effects": a.unmodeled_effects(),
            // WHICH WEAPON CLASSES MAY EQUIP IT. Empty = any weapon whose slot
            // seats it. The page filters its picker on this so the arsenal and
            // the app offer the same set — `arcanes_data::pool_for_weapon` is
            // the engine's own answer and this is it speaking.
            "equip_classes": a.equip_classes,
            "out_of_scope": a.has_out_of_scope(),
            // …and the FOURTH admission, which is not a shortfall: this is
            // modelled, it matches the live game, and it is a bug (M37). A
            // player is reading a number a hotfix can take away, and only the
            // card can tell them which kind of number it is.
            "live_bugs": a.live_bugs,
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
        // WARFRAME ABILITY BUFFS, the catalogue the scenario's own section
        // draws from (`data/abilities/`). `value` and `duration_s` are the
        // wiki's max-rank figures at 100% strength; the page multiplies by the
        // strength you set, so the numbers it SHOWS are computed on screen from
        // exactly these two fields and nothing hidden.
        //
        // `family` travels because the "only the strongest runs" rule has to be
        // visible while you tick the boxes, not just enforced afterwards — the
        // engine settles it either way (`abilities_data::resolve`), and a page
        // that showed both as active would be lying about a number it printed.
        "abilities": wfsim_engine::abilities_data::all().iter().map(|a| {
            let (kind, element) = match a.effect {
                wfsim_engine::abilities_data::AbilityEffect::FactionDamage(_) =>
                    ("faction_damage", None),
                wfsim_engine::abilities_data::AbilityEffect::FinalDamage(_) =>
                    ("final_damage", None),
                wfsim_engine::abilities_data::AbilityEffect::AddElement(t, _) =>
                    ("add_element", Some(t.name())),
                wfsim_engine::abilities_data::AbilityEffect::ExtraHit { element, .. } =>
                    ("extra_hit", Some(element.name())),
            };
            json!({
                "id": a.id,
                "name": a.name,
                "frame": a.frame,
                "family": a.family,
                "helminth": a.helminth,
                "value": a.value,
                "duration_s": a.duration_s,
                // The elements this one lets you CHOOSE (Resupply's ten), empty
                // where it fixes one — the page draws its picker from this.
                "elements": a.elements,
                "class_bonus": a.class_bonus.map(|(c, x)| json!({ "class": c, "x": x })),
                "kind": kind,
                "element": element,
                // THE SAME TWO ADMISSIONS A MOD AND AN ARCANE CARD CARRY, under
                // the same keys, so the page renders all three with one
                // function. Xata's Whisper is the first ability with either:
                // its Void proc is a Bullet Attractor this sim has nothing to
                // point at, and its Blast interaction is DE's own bug.
                "unmodeled_effects": a.unmodelled,
                "live_bugs": a.live_bugs,
                "url": a.url,
            })
        }).collect::<Vec<_>>(),
        // A RIVEN's card image, once. Rivens are made by the visitor, so no
        // per-riven entry could exist in data/assets.yaml — the game draws
        // every riven with the same card and so does this.
        "riven_image": assets().mods.get("riven"),
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
        // THE OFFICIAL SCENARIOS (data/benchmarks/). They are not presets: no
        // weapon owns them, nothing stores them, and nobody can edit them —
        // they exist so a number has a ruler someone else can pick up. The
        // client shows them on every weapon alongside the player's own.
        // HOW MANY MAIN SLOTS A BUILD HAS. Not the admission rule — that is the
        // benchmark's, and travels with it below — just the one number the page
        // needs to count filled slots against.
        "board_build_mods": wfsim_engine::builds::MAIN_SLOTS,
        "benchmarks": wfsim_engine::benchmarks_data::all().iter().map(|b| json!({
            "id": b.id,
            "name": b.name,
            // The standard AT LENGTH — the name is the same thing in one line.
            // A reader deciding whether a ranking answers their question needs
            // the terms, and a term that only exists in a yaml comment is one
            // nobody can check the board against.
            "rules": b.rules,
            // WHAT THIS RULER ADMITS, so the page can say what a build is still
            // missing instead of letting the server refuse in silence. It rides
            // with the benchmark because it IS the benchmark's — a second ruler
            // will answer differently and the page must not assume otherwise.
            "build": {
                "mods": b.build.mods,
                "evolutions": b.build.evolutions,
                "arcanes": b.build.arcanes,
                "exilus": b.build.exilus,
            },
            "scenario": b.scenario,
        })).collect::<Vec<_>>(),
        // DE's own icon per damage type — the meter and the charts colour and
        // label by TYPE, so both halves of that (colour in style.css, file
        // here) come from the same wiki module.
        "damage_type_icons": assets().damage_types,
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
            // NULL, not a boolean — "whatever this unit is by default", which
            // `parse_fight` resolves to `can_be_eximus`. A fixed `true` would
            // be a lie about the default target (a Thrax has no Eximus
            // variant) and a fixed `false` would contradict the rule that the
            // elite unit is the one you meet. The page renders the effective
            // answer per enemy and only stores a boolean once you say
            // otherwise.
            "eximus": Value::Null,
            "headshot_pct": 100.0,
            // ---- the TENNO, the fight's other actor. Every field here is
            // `data/tenno/default.yaml`'s: the NEUTRAL player, aiming, no
            // frame chosen, no ability running. Aiming is true because that is
            // the sim's behaviour before the knob existed, so no stored preset
            // silently changes meaning; the rest are false/0 because "some
            // max-rank neutral Warframe" is doing none of them and wearing
            // nothing (user, 2026-08-02).
            "aiming": true,
            "invisible": false,
            "airborne": false,
            "wf_armor": 0.0,
            "wf_energy": 0.0,
            // INFINITE AMMO by default — see `simulate_json` for why.
            "infinite_ammo": true,
            // Test precision (user, 2026-08-01): 300 s x 100 runs everywhere,
            // and the optimizer's last round is 100 runs on the top 10. Kept
            // in step with `simulate_json` / `parse_optimize`, whose own
            // fallbacks are what an API caller naming none of these gets.
            // WHAT THE RUN IS JUDGED BY. KPM is the default because it is
            // what a build is for; DPS is the other honest answer and some
            // targets cannot be killed at all. The scenario carries it, so
            // whatever ranks — the headline number, the picker's gain scan —
            // ranks by the same thing (user, 2026-08-01).
            "metric": "kpm",
            "duration": 300.0,
            "runs": 100,
            // The final-round contract, for an API caller. The WEB does not
            // read these: `final_runs` is the scenario's `runs` and
            // `finalists` is a fixed 10, because neither is a setting the
            // optimizer tab offers any more (user, 2026-08-02).
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
/// EVERY TARGET'S POOLS AT THE FIGHT'S LEVEL — what the picker shows.
///
/// The picker used to print each unit's stats at its OWN base level (a
/// Corrupted Heavy Gunner as 700 health, 500 armor), which is the number
/// nobody fights: the scenario runs at level 9999 Steel Path, where the same
/// unit is four orders of magnitude bigger and its armor has stopped mattering
/// the way the raw figure suggests. Choosing between two units on their base
/// stats is choosing on the wrong axis (owner, 2026-08-05).
///
/// It is an ENDPOINT and not a formula in the page, because the level curves
/// are the engine's and a second implementation in JavaScript is a second
/// answer waiting to drift. The same `target_params` the fight uses builds
/// these, so what the picker promises is what the sim delivers.
///
/// Takes the fight's `level`, `steel_path` and — per unit — the same Eximus
/// default `parse_fight` applies, so the row reads as what you would get by
/// picking it.
pub fn targets_json(v: &Value) -> Value {
    let level = get_u32(v, "level", 9999).clamp(1, 9999);
    let steel_path = get_bool(v, "steel_path", true);
    let rows: Vec<Value> = enemies()
        .iter()
        .map(|e| {
            // The unit's own default, the one `parse_fight` would pick.
            let eximus = e.can_be_eximus;
            match e.target_params(level, steel_path, eximus, TargetMode::InstantRespawn) {
                Ok(t) => {
                    let armor = t.armor();
                    json!({
                        "id": e.id,
                        "eximus": eximus,
                        "health": t.max_health(),
                        "shield": t.max_shield(),
                        "armor": armor,
                        "overguard": t.overguard(),
                        // The armour figure alone says little at this level —
                        // what a build feels is the reduction it buys.
                        "armor_dr": wfsim_engine::scaling::armor_damage_reduction(armor),
                    })
                }
                // Unreachable with the unit's own flag, and reported rather
                // than unwrapped: a panic here would take the picker down.
                Err(msg) => json!({ "id": e.id, "error": msg }),
            }
        })
        .collect();
    json!({ "level": level, "steel_path": steel_path, "targets": rows })
}

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
    /// What this one stack count buys, when the source grants more than one
    /// thing off the same trigger ("Critical Damage + Multishot"). Empty for
    /// a single-grant buff, where the name already says it.
    grants: String,
    max_stacks: u32,
    kind: &'static str, // "stacking" | "toggle"
    default_stacks: u32,
    default_locked: bool,
    /// PERMANENT stacks (no in-sim trigger, no decay — Fevered Frenzy): the
    /// count is a static choice, so the lock control is meaningless and the
    /// UI greys it out with a hint.
    permanent: bool,
    /// NO CEILING. Secondary Enervate gains a stack per hit until a big crit
    /// wipes the pile, so `max_stacks` has nothing honest to hold — the card
    /// shows `∞` and the input takes no maximum (user, 2026-08-03).
    uncapped: bool,
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
    // `always`: the mods in EVERY build this call describes, and therefore the
    // only ones whose `disables:` may suppress a buff card. For a real build
    // that is all of them; for a SCOPE it is the REQUIRED ones only.
    //
    // `fetchAllBuffs` marks the whole pool "search" to ask what buffs a weapon
    // could ever produce, and one candidate's lock is not a fact about the
    // others. Primary Acuity disables multishot, so under the old rule its mere
    // presence in a rifle's pool deleted Galvanized Chamber's on-kill multishot
    // from the list — a build nobody would ever assemble (eighty mods at once)
    // silently removing a card from a build they would.
    always: &[&ModDef],
    arcane: &wfsim_engine::arcanes_data::ArcaneFx,
    info: &WeaponInfo,
    tenno: &wfsim_engine::tenno_data::Tenno,
) -> Vec<BuffMeta> {
    // Sentinels resolve under BaseOnly — conditional buffs never fire, so
    // there is nothing to configure.
    if info.sentinel {
        return Vec::new();
    }
    // A stat an equipped mod has LOCKED has nothing to configure. `resolve` has
    // already emptied every bucket feeding it — "set to its default ignoring
    // other bonuses" — so a card for one would be a control that moves no
    // number: Frenzy under a Cannonade, Galvanized Diffusion under an Acuity.
    let locked = |s: &'static str| always.iter().any(|m| m.disables.contains(&s));
    let mut out: Vec<BuffMeta> = Vec::new();
    let mut push = |b: BuffMeta| {
        if !out.iter().any(|x| x.id == b.id) {
            out.push(b);
        }
    };
    // Weapon passive: Frenzy (Dual Toxocyst); a single on/off "stack".
    // EARNED like every other timed buff (user, 2026-08-02): it lasts 3 s off
    // a headshot, so a fight that has not started has not got it. Cheap to
    // earn — the first headshot turns it on — which is exactly why seeding it
    // bought nothing and cost the truth.
    if has_frenzy(info) && !locked("fire_rate") {
        push(BuffMeta {
            id: "frenzy".into(),
            name: "Frenzy".into(),
            grants: String::new(),
            max_stacks: 1,
            kind: "toggle",
            default_stacks: 0,
            default_locked: false,
            permanent: false,
            uncapped: false,
        });
    }
    // Mod-granted buffs.
    for m in refs {
        let nm = m.name.to_string();
        for e in &m.effects {
            use ModEffect::*;
            // UNWRAP the player condition, and DROP the effect when the
            // condition does not hold. Argon Scope's on-headshot crit is
            // `WhileTenno(Aiming, OnHeadshotCritChance)`: matching the outer
            // value meant it never produced a card at all, so the sim ran a
            // buff the panel offered no way to set. Unwrapping it
            // unconditionally then made the opposite mistake — a card for a
            // buff that cannot arm, in a fight where the player is not aiming.
            // The resolver already drops it; this is the same question, asked
            // of the same Tenno (user, 2026-08-02).
            let e = match e {
                WhileTenno(c, inner) if c.holds(tenno) => &**inner,
                WhileTenno(..) => continue,
                other => other,
            };
            match *e {
                OnKillMultishot { max_stacks, .. } if !locked("multishot") => push(BuffMeta {
                    id: "on_kill_multishot".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks,
                    kind: "stacking",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                uncapped: false,
                }),
                ConditionOverload { max_stacks, .. } => push(BuffMeta {
                    id: "condition_overload".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks,
                    kind: "stacking",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                uncapped: false,
                }),
                OnHeadshotCritChance { .. } => push(BuffMeta {
                    id: "on_headshot_cc".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks: 1,
                    kind: "toggle",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                uncapped: false,
                }),
                OnHeadshotKillCritChance { max_stacks, .. } => push(BuffMeta {
                    id: "on_headshot_kill_cc".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks,
                    kind: "stacking",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                uncapped: false,
                }),
                OnKillCritDamage { .. } => push(BuffMeta {
                    id: "on_kill_cd".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks: 1,
                    kind: "toggle",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                uncapped: false,
                }),
                OnReloadDamage { .. } => push(BuffMeta {
                    id: "on_reload_bd".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks: 1,
                    kind: "toggle",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                uncapped: false,
                }),
                // SENTIENT SURGE reads the Ocucor's TENDRILS, and the count is
                // a buff like any other: gained on a kill, cleared by a
                // magazine event, capped by the weapon. Its cap comes from the
                // WEAPON (`tendrils.max`) — the mod states only the rate, so a
                // card that carried its own maximum would be free to disagree
                // with the passive that produces it.
                //
                // The card is the whole point of the report: a tendril costs a
                // kill, so at a level where kills are slow — or against a
                // target that never dies — the weapon's own augment measures
                // as nothing and there was no knob to say otherwise (player
                // report, 2026-08-08). One count buys two stats by
                // construction, so it is ONE card that names both, the same
                // rule Frostbite's follows.
                PerTendril { .. } => {
                    let cap = wfsim_engine::weapons_data::spec(&info.id)
                        .and_then(|w| w.tendrils)
                        .map_or(0, |t| t.max);
                    if cap > 0 {
                        push(BuffMeta {
                            // Named for the mod (the client localizes it off
                            // META) with what the stacks ARE in the tail —
                            // "Sentient Surge" alone would leave the reader
                            // guessing what a stack of it is.
                            id: "tendrils".into(),
                            name: format!("{nm} (Tendrils)"),
                            grants: "Critical Chance + Status Chance".into(),
                            max_stacks: cap,
                            kind: "stacking",
                            default_stacks: 0,
                            default_locked: false,
                            permanent: false,
                            uncapped: false,
                        });
                    }
                }
                OnReloadFireRate { .. } if !locked("fire_rate") => push(BuffMeta {
                    id: "on_reload_fr".into(),
                    name: nm.clone(),
                    grants: String::new(),
                    max_stacks: 1,
                    kind: "toggle",
                    default_stacks: 0,
                    default_locked: false,
                    permanent: false,
                uncapped: false,
                }),
                _ => {}
            }
        }
    }
    // Secondary Enervate is a PERK, not an `ArcBuffSpec`, so the arcane loop
    // below never saw it and the one arcane whose whole point is a stack count
    // had no card. Untimed, UNCAPPED, and consumed by a big crit — which is
    // why it starts at 0 like everything else that can be spent (user,
    // 2026-08-03: "失活是独立的buff啊，就像配置这个buff一样配置失活buff").
    if arcane.enervate_rank.is_some() {
        push(BuffMeta {
            id: "arcane:secondary_enervate".into(),
            name: wfsim_engine::arcanes_data::secondary("secondary_enervate")
                .map(|d| d.name.clone())
                .unwrap_or_else(|| prettify("secondary_enervate")),
            grants: String::new(),
            max_stacks: 0,
            kind: "stacking",
            default_stacks: 0,
            default_locked: false,
            permanent: false,
            uncapped: true,
        });
    }
    // Arcane buffs — ONE CARD PER ARCANE, not per grant (user, 2026-08-02).
    //
    // Frostbite grants crit damage AND multishot off the same Cold proc, and
    // they are the same stack count by construction: there is no state of the
    // game where one is at 1 and the other at 10. Two cards invited a setting
    // that cannot exist, and the sim had to pick one of them anyway.
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
        let mut seen: Vec<String> = Vec::new();
        for b in arcane.buffs.iter() {
            // A `tenno_scaled` arcane is NOT a card. Primary Bulwark's value
            // is a WARFRAME STAT — not a stack anybody earns or loses — and a
            // "0/1" knob for it would invite switching off a number the frame
            // simply has. It rides the buff machinery to reach its bucket;
            // that is an implementation detail and it stops here (user,
            // 2026-08-02). Its own control is WF Armor, in the Tenno block.
            if b.trigger == wfsim_engine::arcanes_data::ArcTrigger::Passive {
                continue;
            }
            let owner = if b.owner.is_empty() { arcane.id.clone() } else { b.owner.clone() };
            if seen.contains(&owner) {
                continue;
            }
            seen.push(owner.clone());
            // Every grant this arcane makes, so the card can say what the one
            // stack count is buying.
            let grants: Vec<&'static str> = arcane
                .buffs
                .iter()
                .filter(|x| {
                    let o = if x.owner.is_empty() { &arcane.id } else { &x.owner };
                    *o == owner
                })
                .map(|x| grant_label(x.grant))
                .collect();
            let max_stacks = arcane
                .buffs
                .iter()
                .filter(|x| {
                    let o = if x.owner.is_empty() { &arcane.id } else { &x.owner };
                    *o == owner
                })
                .map(|x| x.max_stacks)
                .max()
                .unwrap_or(1);
            push(BuffMeta {
                id: format!("arcane:{owner}"),
                name: named(&owner),
                grants: grants.join(" + "),
                max_stacks,
                kind: if max_stacks > 1 { "stacking" } else { "toggle" },
                default_stacks: 0,
                default_locked: false,
                permanent: false,
                uncapped: false,
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
                grants: String::new(),
                max_stacks: c.max_stacks,
                kind: "stacking",
                // WHERE THE CARD OPENS, and the engine says which rule
                // applies. A permanent buff (no trigger, no decay) survives a
                // lull so it starts full; an earned one starts at zero. No
                // card's default depends on the weapon — the ceiling is the
                // same for every weapon that has the perk.
                default_stacks: match c.opens_at {
                    wfsim_engine::evolutions_data::CardOpens::Full => c.max_stacks,
                    wfsim_engine::evolutions_data::CardOpens::Zero => 0,
                },
                default_locked: false,
                permanent: c.permanent,
                uncapped: false,
            })
        })
        .collect()
}

fn buffs_json(list: &[BuffMeta]) -> Vec<Value> {
    list.iter()
        .map(|b| {
            json!({
                "id": b.id, "name": b.name, "grants": b.grants, "max_stacks": b.max_stacks,
                "kind": b.kind,
                "default_stacks": b.default_stacks, "default_locked": b.default_locked,
                "permanent": b.permanent,
                "uncapped": b.uncapped,
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
    // The same player the sim and the optimizer fight as: an arcane that
    // scales off Warframe armor or energy reads it from here.
    let tenno = tenno_from(v, info);
    let parts: Vec<wfsim_engine::arcanes_data::ArcaneFx> = arcane_choices(v, info)
        .into_iter()
        .filter_map(|(pool, aid, rank)| {
            // POOL-scoped: an arcane from another pool is not equippable in
            // that slot, so it resolves to nothing rather than being applied.
            let def = wfsim_engine::arcanes_data::for_slot(&pool, &aid)?;
            let rank = rank.unwrap_or(def.max_rank).min(def.max_rank);
            Some(def.fx(rank, policy, base.traits, &tenno))
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
    let p = mod_pool_with_rivens(v, info, &evo_refs);
    let mut refs: Vec<&ModDef> = Vec::with_capacity(mod_ids.len());
    for id in &mod_ids {
        match p.iter().find(|m| m.id == id) {
            Some(m) => refs.push(m),
            None => return err_json(mod_not_here(id, info, &evo_refs)),
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
            base_for(v, f.weapon_id, &evo_refs),
        ));
    }

    // ---- per-bucket source attribution (mirrors resolve()'s buckets) ----
    // key -> [(mod name, contribution fraction, note)]
    let mut src: Vec<(&'static str, String, f64, Option<String>)> = Vec::new();
    let mut conditionals: Vec<Value> = Vec::new(); // lines that never merge into a bucket
    for m in &refs {
        let name = m.name.to_string();
        for e in &m.effects {
            use ModEffect::*;
            let before = src.len();
            let mut push = |key: &'static str, v: f64, note: Option<String>| {
                src.push((key, name.clone(), v, note));
            };
            // A player-gated effect still LISTS, tagged with the state it
            // waits on: the reader needs to see that the mod contributes, and
            // under what condition. When the fight's Tenno is not doing it the
            // row says what the mod WOULD give and gives nothing — which is
            // the whole reason the condition is modelled rather than folded
            // in. Unwrap here, let the ordinary arms push, tag them below.
            let (e, tenno_gate): (&ModEffect, Option<&'static str>) = match e {
                WhileTenno(c, inner) => (&**inner, Some(c.label())),
                other => (other, None),
            };
            match *e {
                WhileTenno(..) => unreachable!("unwrapped above"),
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
                // SENTIENT SURGE: the bonuses scale with ACTIVE TENDRILS, and
                // a tendril costs a kill — so the panel cannot state a number
                // without assuming a fight. It states the CAP under
                // assumed-max (4 tendrils, the Ocucor's own limit, which is
                // where the wiki's "up to 240%" comes from) and lists it as a
                // conditional otherwise, the same shape every other on-kill
                // mod here takes.
                //
                // `tendril_max` is read off the WEAPON rather than written
                // into the mod, so the cap cannot disagree with the passive
                // that produces it.
                // The refill buys uptime, not damage, so it has no bucket to
                // join — it is the reason the bonuses above survive, and it is
                // listed on the card rather than attributed to a stat.
                MagazineRefillOnKill(..) => {}
                // A SYNDICATE RADIAL is not a stat bucket — it is a flat
                // explosion on its own clock, so there is no percentage to
                // attribute to a damage source. The card states it in full
                // (`describe`) and the sim reports what it dealt under its own
                // heading.
                SyndicateRadial { .. } => {}
                PerTendril { crit_chance, status_chance } => {
                    let cap = f64::from(
                        wfsim_engine::weapons_data::spec(&info.id)
                            .and_then(|w| w.tendrils)
                            .map_or(0, |t| t.max),
                    );
                    match policy {
                        StackPolicy::BaseOnly => conditionals.push(json!({
                            "mod": name, "desc": e.describe(), "active": false,
                            "why": "the bonus scales with active tendrils, and a tendril costs a kill — none are up at the start of a fight, and a reload clears every one"})),
                        _ => {
                            let note = Some(format!("{cap} tendrils assumed (the cap)"));
                            push("crit_chance", crit_chance * cap, note.clone());
                            push("status_chance", status_chance * cap, note);
                        }
                    }
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
            // Tag whatever the arms just pushed, so the panel never shows a
            // contribution without the condition that earns it.
            if let Some(cond) = tenno_gate {
                for row in src.iter_mut().skip(before) {
                    row.3 = Some(match row.3.take() {
                        Some(t) => format!("{t}; {cond}"),
                        None => cond.to_string(),
                    });
                }
            }
        }
    }
    // A `tenno_scaled` ARCANE contributes without being a mod, a bucket or a
    // buff card: its value is a WARFRAME STAT read off the fight's Tenno, so
    // there is no stack to configure and nothing in the resolved panel to
    // attribute it to. It gets a conditional line — the one channel for "this
    // pays, and here is what decides it" — because a contribution the sim
    // applies and the panel never mentions is exactly the disagreement the
    // rest of this function exists to prevent (user, 2026-08-02).
    {
        let arc = arcane_fx_for(v, info, &forms_list[0].2, policy);
        let t = tenno_from(v, info);
        for b in arc.buffs.iter() {
            if b.trigger != wfsim_engine::arcanes_data::ArcTrigger::Passive {
                continue;
            }
            let what = match b.grant {
                wfsim_engine::arcanes_data::ArcGrant::Multishot => "Multishot",
                _ => "Base Damage",
            };
            conditionals.push(json!({
                "mod": prettify(&b.owner),
                "desc": format!("{} {what}", fpct(b.per_stack)),
                "active": true,
                "why": format!("from your Warframe — armor {:.0}, max energy {:.0}. Set them in the Tenno block; with no frame this pays nothing",
                    t.armor, t.energy),
            }));
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
                        // `frac` is the RAW fraction beside the formatted text,
                        // so the panel can show the arithmetic — `40 × (1 +
                        // 1.65 + 0.60)` — instead of only its answer. Everything
                        // in one bracket is one multiplicative bucket, and that
                        // shape teaches the bucket better than any sentence.
                        //
                        // Evolution sources carry no `frac` on purpose: several
                        // are FLAT additions rather than percentages, so an
                        // expression built from them would assert arithmetic
                        // that is not what the engine did. The page draws the
                        // line only when every term in the row is a fraction.
                        json!({ "mod": name, "value": fpct(*v), "frac": v,
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
        // A LOCKED row says so. Base == final and an empty source list is what
        // a stat nothing touched looks like too, and the difference matters: one
        // is a build that bought nothing, the other is a build whose mods are
        // being ignored on purpose ("Fire Rate cannot be modified"). Named after
        // the mod that did it, because that is the thing to take off.
        let lock_by = |key: &str| -> Option<String> {
            panel.locked.contains(&key).then(|| {
                refs.iter()
                    .find(|m| m.disables.contains(&key))
                    .map_or_else(String::new, |m| m.name.to_string())
            })
        };
        let mut row = |key: &'static str, label: &str, base_s: String, final_s: String| {
            let mut srcs = sources(key, None);
            let mut j = json!({ "key": key, "label": label, "base": base_s, "final": final_s });
            if let Some(by) = lock_by(key) {
                // ...AND WHAT THE LOCK IS THROWING AWAY. The row kept listing
                // every bonus feeding a stat it had already zeroed, so a build
                // with Critical Deceleration under a Cannonade read "3.3/s ·
                // locked · −20% Critical Deceleration" — a pinned number and a
                // contribution to it, on the same row, with nothing saying
                // which one won (owner, 2026-08-08). The number was always
                // right; the row argued with itself about it. Marking them is
                // better than dropping them: "this mod does nothing here" is
                // exactly what the reader came for, and a missing line says it
                // to nobody.
                for s in srcs.iter_mut() {
                    s["ignored"] = json!(true);
                }
                j["locked_by"] = json!(by);
            }
            j["sources"] = json!(srcs);
            stats.push(j);
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
            // HOW THE GAUGE FILLS, which the panel never said. The engine has
            // always read it — `charge_on` is weapon data and the shot loop
            // counts headshots or pellets accordingly — but a player could not
            // SEE it, and the two rules do not merely differ in speed: at a 0%
            // headshot rate a weakpoint-charged weapon never transforms at all
            // (measured: Burston Prime 0 transforms, Torid 4, same fight).
            // That is the largest thing an Incarnon weapon can do, decided by a
            // field with no row.
            let (what, why) = match inc.charge_on {
                wfsim_engine::loadout::ChargeOn::WeakpointHits => (
                    "weakpoint hits",
                    "weakpoint hits only — at a 0% headshot rate this weapon never reaches its Incarnon form. A radial or field instance can never contribute: it has no hit location",
                ),
                wfsim_engine::loadout::ChargeOn::DirectHits => (
                    "direct hits",
                    "ANY direct hit, so the form does not depend on the headshot rate (wiki Incarnon: \"Angstrum Incarnon Genesis and Torid Incarnon Genesis are instead charged through direct hits\"). A lingering field is not a direct hit and does not charge it",
                ),
            };
            stats.push(json!({ "key": "gauge", "label": "Gauge Fills On",
            "base": "—",
            // A COUNT, so no decimal: "5 direct hits", not "5.0".
            "final": format!("{:.0} {what}", inc.charges_to_fill),
            "note": why,
            "sources": sources("incarnon_charge_rate", None) }));
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
        // WHICH PARTS take it. "Direct hits only" is the rule the mod cards
        // state and it is what every unlisted weapon does — but an AoE part
        // carries its own eligibility and a few entries have it (MECHANICS
        // §6), so the note has to be built from the weapon rather than
        // asserted. It was a hardcoded "direct hits only", which the Burston
        // Incarnon makes false: its explosion takes CO.
        let radial_co = base.radial.as_ref().is_some_and(|r| r.takes_condition_overload);
        let field_co = base.lingering.as_ref().is_some_and(|f| f.takes_condition_overload);
        let co_parts = match (radial_co, field_co) {
            (false, false) => "direct hits only".to_string(),
            _ => {
                let extra = if radial_co && field_co {
                    "the radial explosion and the lingering field"
                } else if radial_co {
                    "the radial explosion"
                } else {
                    "the lingering field"
                };
                format!("direct hits AND {extra} — an AoE part taking CO is a per-entry exception, declared by this weapon")
            }
        };
        let behavior = match panel.co_behavior {
        wfsim_engine::loadout::CoBehavior::AdditiveWithBaseDamage =>
            format!("joins the base-damage bracket on this weapon (additive with Hornet Strike), {co_parts}"),
        wfsim_engine::loadout::CoBehavior::Independent =>
            format!("an independent multiplier on this weapon, {co_parts}"),
        wfsim_engine::loadout::CoBehavior::Inert =>
            "INERT on this weapon — the bonus does not apply".to_string(),
    };
        let gunco_note = if (panel.co_base_fraction - 1.0).abs() > 1e-9 {
            format!(
            "computed on the ORIGINAL {:.0} base only — evolution flat damage is excluded ({:.0}% effectiveness); {behavior}",
            raw_bd,
            panel.co_base_fraction * 100.0
        )
        } else {
            behavior.clone()
        };
        if panel.co_per_type > 0.0 {
            stats.push(json!({ "key": "co", "label": "Condition Overload",
            "base": "—", "final": format!("{} per status type on target", fpct(panel.co_per_type)),
            "note": gunco_note,
            "sources": sources("co", None) }));
        }

        // The equipped arcane on the panel: Secondary Shiver is a GunCO-family
        // source, so its row carries the SAME per-weapon caveat as the CO row.
        let tenno = tenno_from(v, info);
        // Cascadia Accuracy's weak-point crit joins Acuity's in the sim
        // (`ap.weakpoint_cc_rel + params.arcane.weakpoint_cc_rel`), so the row
        // below has to add it or it would state less than the sim applies.
        let mut arcane_wp_cc = 0.0;
        for (pool, aid, want_rank) in arcane_choices(v, info) {
            if let Some(def) = wfsim_engine::arcanes_data::for_slot(&pool, &aid) {
                let rank = want_rank.unwrap_or(def.max_rank).min(def.max_rank);
                let fx = def.fx(rank, policy, base.traits, &tenno);
                arcane_wp_cc += fx.weakpoint_cc_rel;
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

        // WEAK-POINT bonuses (Acuity, Cascadia Accuracy). They had NO rows at
        // all: the damage half was invisible and the crit half was folded into
        // the flat Crit Chance row, so a mod worth +350% on heads read as
        // either nothing or as an unconditional 126%. Both halves are
        // conditional on where the bullet lands, and the number a reader can
        // act on is the one that holds THERE — stated next to the plain one,
        // never in place of it.
        let wp_cc_total = panel.weakpoint_cc_rel + arcane_wp_cc;
        if wp_cc_total > 0.0 {
            stats.push(json!({ "key": "weakpoint_cc", "label": "Weak Point Crit Chance",
            "base": pc(panel.crit_chance),
            "final": format!("{} on a weak point", pc(panel.crit_chance + panel.base_crit_chance * wp_cc_total)),
            "note": format!(
                "{} relative to the {} base, additive with Point Strike — on WEAK-POINT hits only. \
                 Everywhere else the crit chance above stands, and the radial explosion never gets \
                 it at all (an explosion has no hit location)",
                fpct(wp_cc_total), pc(panel.base_crit_chance)),
            "sources": sources("crit_chance", None) }));
        }
        if panel.weakpoint_damage > 0.0 {
            stats.push(json!({ "key": "weakpoint_damage", "label": "Weak Point Damage",
            "base": "—",
            // Two decimals, not the panel's usual one: the wiki's worked
            // example is "3 + 3.5 x 1.5 = 8.25x" and a row printing 8.2 no
            // longer matches the source it cites.
            "final": format!("+{:.2} to the weak-point multiplier", 1.5 * panel.weakpoint_damage),
            "note": format!(
                "the listed {} is ADDED to the enemy's own weak-point multiplier at 1.5x on a true \
                 weak point (wiki: a 3x head becomes 3 + {:.2} = {:.2}x), and headshot-multiplier \
                 bonuses multiply the sum. Weak-point hits only",
                fpct(panel.weakpoint_damage), 1.5 * panel.weakpoint_damage,
                3.0 + 1.5 * panel.weakpoint_damage),
            "sources": Vec::<Value>::new() }));
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
            // Weak-point bonuses belong to the PROJECTILE that lands on the
            // weak point, and to that one only: the explosion has no hit
            // location, so leaving them among the weapon-wide rows would read
            // as a claim over both parts.
            "weakpoint_cc",
            "weakpoint_damage",
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
            // CONDITION OVERLOAD, stated on the explosion ITSELF — because the
            // answer is normally "no" and this reader is looking at one of the
            // entries where it is "yes". The direct hit's row cannot carry it:
            // it names one bonus, and the two parts do not get the same one.
            // Shown only when a CO source is equipped, like the direct row.
            if panel.co_per_type > 0.0 {
                let (value, note) = if rr.takes_condition_overload {
                    // The Burston Incarnon's own base fraction is 13/55: the
                    // explosion takes the evolution's flat damage but not into
                    // the base CO multiplies. Its catalog row prints the 24%.
                    let orig = rb.base_vector.total() * rr.co_base_fraction;
                    let cut = (rr.co_base_fraction - 1.0).abs() > 1e-9;
                    (
                        format!("{} per status type on target", fpct(panel.co_per_type)),
                        if cut {
                            format!(
                                "THE EXCEPTION: CO normally reaches direct hits only, and this \
                                 explosion is declared to take it — on the enemy the bullet \
                                 directly hit, which a single target always is. Computed on the \
                                 ORIGINAL {} base only ({:.0}% effectiveness): evolution flat \
                                 damage raises the explosion's damage but not its CO base",
                                num(orig),
                                rr.co_base_fraction * 100.0
                            )
                        } else {
                            "THE EXCEPTION: CO normally reaches direct hits only, and this \
                             explosion is declared to take it — on the enemy the bullet directly \
                             hit, which a single target always is"
                                .to_string()
                        },
                    )
                } else {
                    (
                        "excluded".to_string(),
                        "the rule: Condition Overload reaches DIRECT hits only, so this \
                         explosion takes none of it. Weapon-wide damage buckets still reach it — \
                         CO is the one thing an AoE part loses"
                            .to_string(),
                    )
                };
                rows.push(json!({ "key": "co", "label": "Condition Overload",
                    "base": "—", "final": value, "note": note,
                    "sources": if rr.takes_condition_overload { sources("co", None) } else { vec![] } }));
            }
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
    // A real build: every mod on it is on it, so every lock is real.
    let mut buffs = enumerate_buffs(&refs, &refs, &arcane_fx, info, &tenno_from(v, info));
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
/// The evolutions of `ids` that the weapon can actually REACH, in order.
///
/// The tiers are a LADDER: tier N is installed only after tier N-1, so a set
/// that skips one does not describe a weapon anyone can hold. Everything from
/// the first gap upward is dropped. The UI locks the rows, but it cannot be
/// the only place the rule holds — a preset saved before it existed, or a
/// hand-built request, still carries the gap, and the engine would price it.
fn ladder_prefix(ids: Vec<String>) -> Vec<String> {
    let tier_of = |id: &String| wfsim_engine::evolutions_data::get(id).map(|e| e.tier);
    let mut tiers: Vec<u32> = ids.iter().filter_map(tier_of).collect();
    tiers.sort_unstable();
    let reach = tiers
        .iter()
        .enumerate()
        .take_while(|(i, t)| **t == *i as u32 + 1)
        .count() as u32;
    ids.into_iter()
        .filter(|id| wfsim_engine::evolutions_data::get(id).is_some_and(|e| e.tier <= reach))
        .collect()
}

fn chosen_evolutions(v: &Value, info: &WeaponInfo) -> Result<Vec<String>, String> {
    let mine = |ids: Vec<String>| -> Vec<String> {
        let group = evo_group(info);
        ladder_prefix(
            ids.into_iter()
                .filter(|id| {
                    wfsim_engine::evolutions_data::get(id).is_some_and(|e| e.weapon == group)
                })
                .collect(),
        )
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

/// THE FIGHT — parsed ONCE, for both modules.
///
/// `simulate_json` used to parse this and `parse_optimize` parsed it again, and
/// the two drifted three times in three days: the form-unlock fallback
/// (2026-08-04), a caller omitting `evolutions` getting the Incarnon cycle free
/// while the search scored the base form (2026-08-03), and the optimizer
/// keeping a buff config of its own (2026-08-02). Every one of them scored
/// builds under a fight the replay would not run.
///
/// So this is not a helper both agree to call — it is the ONE parse, and the
/// simulator is the truth (user, 2026-08-04: "我希望 optimizer 执行的，是
/// simulator 的规矩"). The optimizer adds only what is its own: the scope to
/// search and the budget to spend.
///
/// Measured when it was written: the two parsers read 9 of the same request
/// fields and called 10 of the same 11 helpers. The one they did not share was
/// `chosen_evolutions`, which is exactly where it kept breaking.
pub(crate) struct Fight {
    pub(crate) info: &'static WeaponInfo,
    pub(crate) policy: StackPolicy,
    /// `None` = the legacy `assume_max`/`frenzy` knobs; `Some` = per-buff
    /// config, which is what the Sim panel sends.
    pub(crate) buff_cfg: Option<BuffCfg>,
    /// Both actors and how long they are at it.
    pub(crate) arena: wfsim_engine::arena::Arena,
    /// After the ladder is applied AND the form's own unlock is implied.
    pub(crate) evos: Vec<String>,
    /// Is this the two-form CYCLE, or a single form fired throughout?
    pub(crate) run_cycle: bool,
    /// The single form to fire (the cycle's Incarnon half when cycling).
    pub(crate) single_form: &'static str,
    /// The weapon's own default form — what a cycle returns to.
    pub(crate) untransformed_id: String,
    /// The form ASKED FOR, after `default` is resolved. Owned: it comes from
    /// the request, not from the weapon table.
    pub(crate) form: String,
    pub(crate) enemy_id: String,
    pub(crate) level: u32,
    pub(crate) steel_path: bool,
    /// Is the target its ELITE variant? A property of the fight, like the
    /// level and the Steel Path switch, so it lives here and both modules
    /// read it from one place.
    pub(crate) eximus: bool,
    pub(crate) headshot_pct: f64,
    pub(crate) tenno: wfsim_engine::tenno_data::Tenno,
    pub(crate) infinite_ammo: bool,
    pub(crate) duration: f64,
    pub(crate) runs: u32,
    pub(crate) seed: u64,
    pub(crate) has_frenzy: bool,
    pub(crate) frenzy_single: bool,
    /// The single-form frenzy locks, built beside `frenzy_single`.
    pub(crate) frenzy_locks: Vec<BuffLock>,
    pub(crate) cycle_frenzy_lock: LockMode,
}

/// WHICH WEAPON ENTRY FIRES, in ONE place.
///
/// Not `single_form`: for a CYCLE the Incarnon half is what fires and
/// `untransformed_id` is what it returns to between transmutes, while
/// `single_form` answers "the one form to fire when there is no cycle" and
/// resolves `incarnon_cycle` — a mode, not a form — to the weapon's default.
/// Mapping one onto the other made the search run the BASE form of every
/// cycling weapon (the Torid lost 9x, the Boar GAINED; caught by the optimizer
/// baseline, 2026-08-04). It is a function of the FIGHT, so it is written here
/// rather than at each caller: `parse_optimize` had the only copy, and the
/// pairing endpoint needs the same answer or it would label the wrong form's
/// elements.
pub(crate) fn firing_entry(fight: &Fight) -> String {
    if fight.run_cycle {
        incarnon_id(fight.info).unwrap_or(&fight.info.id).to_string()
    } else {
        fight.single_form.to_string()
    }
}

/// ELEMENT PAIRINGS for a list of mod sets — the quick calc's enabling call.
///
/// A mod set with three distinct elements is not one build but THREE, and on
/// the Burston Prime the best of them is 3.3x the worst (2.074 against 0.627
/// kills/min at Thrax Lv 9999 SP). So a marginal-gain number has to be
/// measured at the BEST pairing, against a baseline measured the same way, and
/// the caller needs to know which orders to run.
///
/// One call for the whole scan rather than one per candidate: the client sends
/// every set it means to measure (the reference, and the reference plus each
/// candidate) and gets back the orders to simulate. The alternative — teaching
/// the browser to pair elements — would be a second copy of `elements::combine`
/// rules 2 and 3, and it would be wrong about innate elements the first time a
/// weapon carried one.
pub fn pairings_json(v: &Value) -> Value {
    let fight = match parse_fight(v) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let fire = firing_entry(&fight);
    let pool = wfsim_engine::mods_data::pool_for_weapon(&fire);
    let sets = v.get("sets").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let out: Vec<Value> = sets
        .iter()
        .map(|entry| {
            // A set is either a bare mod list or `{mods, evolutions}`. It needs
            // its own evolutions because an EVOLUTION can move the pairings:
            // tier 1 on the Burston unlocks the Incarnon form, whose base
            // damage is Heat, so installing it gives the build an innate
            // element the base form does not have.
            let set = entry.get("mods").unwrap_or(entry);
            let evos: Vec<String> = entry
                .get("evolutions")
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str()).map(String::from).collect())
                .unwrap_or_else(|| fight.evos.clone());
            // Unknown ids are DROPPED, not rejected: the client's scope can
            // name a mod this form cannot equip (an evolution forbids it), and
            // the honest answer there is the set without it — the same rule
            // `builds::normalize` applies to a submission.
            let ids: Vec<String> = set
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .filter(|id| pool.iter().any(|m| m.id == *id))
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            let orders: Vec<Value> = wfsim_engine::builds::element_orders(&fire, &ids, &evos)
                .into_iter()
                .map(|o| {
                    let name = |t: wfsim_engine::damage::DamageType| format!("{t:?}");
                    json!({
                        "mods": o.mods,
                        "combined": o.combined.iter().copied().map(name).collect::<Vec<_>>(),
                        "leftover": o.leftover.iter().copied().map(name).collect::<Vec<_>>(),
                    })
                })
                .collect();
            json!({ "orders": orders })
        })
        .collect();
    json!({ "ok": true, "form": fire, "sets": out })
}

pub(crate) fn parse_fight(v: &Value) -> Result<Fight, Value> {
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
    // HOW THE WEAPON IS PLAYED, from the BUILD side of the request.
    //
    // `mode` is the vocabulary now — `base`, `cycle`, `alternate` — and it is a
    // property of the entrant, so it arrives with the mods rather than with the
    // fight. `WeaponPlayMode::form` is the one place it becomes a form, which
    // is what lets "played without ever transmuting" be asked for at all.
    //
    // `form` is still READ when no mode is named, because share links and
    // scenario presets written before this carry it. A stale `form` is not
    // migrated, it is simply obeyed one last time.
    let modes = wfsim_engine::weapons_data::play_modes(&info.id);
    let form = match v.get("mode").and_then(Value::as_str) {
        Some(want) => modes
            .iter()
            .find(|m| m.id == want)
            .map(|m| m.form())
            .unwrap_or("default"),
        None => get_str(v, "form", "default"),
    };
    let evos = match chosen_evolutions(v, info) {
        Ok(e) => e,
        Err(e) => return Err(err_json(e)),
    };
    // ASKING FOR A FORM IMPLIES THE EVOLUTION THAT IS THAT FORM.
    //
    // This used to fall back to "base" when the tier-1 unlock was not among the
    // chosen evolutions, which made the form control lie: with no evolutions
    // picked — the state the page STARTS in — all three options produced the
    // base form's number and nothing said why (user, 2026-08-04: "灵化循环和基
    // 础，纯灵化好像都不起作用").
    //
    // Implying it is the honest model, not a shortcut. Tier 1 is
    // `selection: fixed` on every Incarnon ladder: it is not a choice, it is
    // what installing the Genesis grants. And it carries no stat of its own —
    // `UnlocksForm` applies nothing, because the form it unlocks is a separate
    // weapon entry with its own numbers. So the form and the evolution were two
    // controls for ONE fact, and this is which of them decides.
    let unlock = form_unlock_evo(info);
    let mut evos = evos;
    if form != "base" {
        if let Some(u) = unlock {
            if !evos.iter().any(|e| e == u) {
                evos.push(u.to_string());
            }
        }
    }
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
    // A SENTINEL'S HEADSHOT RATE IS NOT THE PLAYER'S TO SET. Its companion
    // picks its own targets and never aims for a head, so this is 0 whatever
    // the request says — the same shape as `tenno_from` forcing its stance.
    //
    // It used to be only a DEFAULT, which was enough while no benchmark pinned
    // the field; the aimed board pins 100 now, and without this a sentinel
    // would be ranked at a headshot rate it cannot reach. Two boards that
    // differ only in the player's aim therefore give a sentinel the same score
    // twice, which is the honest answer to "how much of this weapon is your
    // aim": none of it.
    let headshot_pct = if info.sentinel {
        0.0
    } else {
        get_f64(v, "headshot_pct", default_headshot_pct(info))
    };
    let tenno = tenno_from(v, info);
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

    // ---- WARFRAME ABILITY BUFFS (`data/abilities/`) -----------------------
    // `ability_strength` is a FRACTION (1.0 = 100%), because that is what it
    // multiplies. `abilities` is what the player ticked, each with its own
    // seconds — omit `secs` (or send null) for the whole fight.
    //
    // Resolved right here rather than carried raw: `abilities_data::resolve`
    // applies the strength AND settles the same-family conflicts, so nothing
    // downstream — not the sim, not the optimizer, not the replay — can end up
    // adding two Roars together.
    let strength = get_f64(v, "ability_strength", 1.0).clamp(0.0, 10.0);
    let picks: Vec<wfsim_engine::abilities_data::AbilityPick<'_>> = v
        .get("abilities")
        .and_then(Value::as_array)
        .map(|seq| {
            seq.iter()
                .filter_map(|e| {
                    let id = e.get("id").and_then(Value::as_str)?;
                    Some(wfsim_engine::abilities_data::AbilityPick {
                        id,
                        duration_s: e
                            .get("secs")
                            .and_then(Value::as_f64)
                            .filter(|s| *s > 0.0),
                        // WHICH ELEMENT, where the ability offers a choice
                        // (Resupply's gear wheel). Absent everywhere else, and
                        // absent on a pick stored before the picker existed —
                        // the definition's own first choice stands in.
                        element: e.get("element").and_then(Value::as_str),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    // …and the WEAPON'S CLASS, because one member is worth double on a class:
    // Resupply is 20/30/40/50% on Sniper Rifles. `resolve` is the one function
    // handed both the ability and the weapon, so nothing downstream has to know
    // what a sniper is.
    let abilities = wfsim_engine::abilities_data::resolve(&picks, strength, wfsim_engine::weapons_data::spec(&info.id).map_or("", |s| s.class.as_str()));

    let specs = enemies();
    let Some(spec) = specs.iter().find(|e| e.id == enemy_id) else {
        return Err(err_json(format!("unknown enemy: {enemy_id}")));
    };
    // ELITE VARIANT, and it DEFAULTS ON wherever the unit has one (owner,
    // 2026-08-05: "默认我们就选上"). The Eximus is what a Steel Path player
    // actually meets — extra health and a pool of Overguard in front of it —
    // so the ordinary unit is the special case to ask for, not the elite one.
    //
    // The default is the UNIT's answer rather than a flat `true`, because the
    // engine REJECTS a combination that does not exist in game (a Thrax has no
    // Eximus variant; its overguard is innate). A blanket default would turn
    // every Thrax fight into an error, so the fallback is what this unit can
    // be — and an explicit `true` on a unit that cannot still fails, which is
    // the rigor being kept rather than worked around.
    let eximus = get_bool(v, "eximus", spec.can_be_eximus);
    let target = match spec.target_params(level, steel_path, eximus, TargetMode::InstantRespawn) {
        Ok(t) => t,
        Err(e) => return Err(err_json(e)),
    };
    // (The target's pools are read off the ARENA by whoever reports them —
    // one target, one place it lives.)
    let body_parts = build_body_parts(spec, headshot_pct);
    // ---- the ARENA: both actors, and how long they are at it. Assembled
    // once and handed whole to whichever constructor runs, so the two forms
    // of a cycle cannot end up fighting two different fights.
    let arena = wfsim_engine::arena::Arena {
        tenno: tenno.clone(),
        target,
        body_parts,
        duration_secs: duration,
        // WARFRAME ABILITY BUFFS — parsed HERE, in `parse_fight`, which is what
        // makes the optimizer score under them without a line of optimizer code
        // (the house rule: anything that is a property of the fight goes in the
        // one module both read).
        abilities,
    };
    Ok(Fight {
        info,
        policy,
        buff_cfg,
        arena,
        evos,
        run_cycle,
        single_form,
        untransformed_id: registered
            .iter()
            .find(|f| f.is_default && !f.kind.is_gauge_switched())
            .or_else(|| registered.iter().find(|f| !f.kind.is_gauge_switched()))
            .map(|f| f.weapon_id)
            .unwrap_or(&info.id)
            .to_string(),
        form: form.to_string(),
        enemy_id: enemy_id.to_string(),
        level,
        steel_path,
        eximus,
        headshot_pct,
        tenno,
        infinite_ammo,
        duration,
        runs,
        seed,
        has_frenzy,
        frenzy_single,
        frenzy_locks,
        cycle_frenzy_lock,
    })
}


pub fn simulate_json(v: &Value) -> Value {
    // THE FIGHT, parsed by the ONE function that parses it. The optimizer
    // calls the same one — see `parse_fight`.
    let fight = match parse_fight(v) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let Fight {
        info, policy, buff_cfg, arena, evos, run_cycle, single_form,
        enemy_id, level, steel_path, eximus, tenno, infinite_ammo, runs, seed,
        frenzy_single, frenzy_locks, cycle_frenzy_lock, ..
    } = fight;
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

    // No count validation here (user, 2026-07-28): the sim runs whatever it
    // is given — slot legality (8 main + 1 exilus) is the UI's job, and the
    // engine resolves any mod list honestly.

    // ---- resolve mods against the weapon's pool (honoring the given order) ----
    if let Err(e) = riven_stat_ids_ok(v, info) {
        return err_json(e);
    }
    // THE FIGHT'S evolutions, which is what decides the pool: asking to fire the
    // Incarnon form implies its unlock (see `parse_fight`), and a weapon with
    // that form installed has a second firing mode — so a Cannonade equipped
    // beside it is a build the game refuses, and the sim must say so rather than
    // report a number nobody can reproduce.
    let p = mod_pool_with_rivens(v, info, &evo_refs);
    let mut refs: Vec<&ModDef> = Vec::with_capacity(mod_ids.len());
    for id in &mod_ids {
        match p.iter().find(|m| m.id == id) {
            Some(m) => refs.push(m),
            None => return err_json(mod_not_here(id, info, &evo_refs)),
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
    // The target's pools, for the report. Read off the arena rather than kept
    // beside it: one target, one place it lives.
    let specs = enemies();
    let (og, sh, hp, ar) = (
        arena.target.overguard(),
        arena.target.max_shield(),
        arena.target.max_health(),
        arena.target.armor(),
    );

    // ---- forma legality (order-independent; needs only the mod multiset) ----
    let planned: Vec<PlannedMod> = refs
        .iter()
        .map(|m| PlannedMod {
            base_drain: m.base_drain,
            polarity: m.polarity,
        })
        .collect();
    // CAPACITY IS NOT A CONSTANT. It follows the weapon's rank, and a rank-40
    // weapon reaches 80 — the literal 60 here was a rank-30 answer standing in
    // for the rule (docs/INVESTMENT.md). `fit` owns the whole question: the
    // rank the Forma buy, the capacity that gives, and the bill by item.
    let inv = wfsim_engine::mods::Investment::default();
    let forma = match wfsim_engine::mods::fit(
        wspec(&info.id).max_rank,
        &innate_slots_for(&info.id),
        &planned,
        inv,
    ) {
        Ok(f) => json!({
            "legal": true,
            "used": f.cost.total(),
            "regular": f.cost.regular,
            "omni": f.cost.omni,
            "umbra": f.cost.umbra,
            "total_drain": f.drain,
            "rank": f.rank,
            "cap": f.capacity,
        }),
        Err(e) => json!({ "legal": false, "error": e }),
    };

    // ---- resolve panel(s) and build sim params, per weapon ----
    // Either ONE registered form, or the real two-form cycle (which needs the
    // gauge form and the form it transforms out of, so it resolves both).
    let (report_panel, mut params): (ResolvedPanel, DummyParams) = {
        let panel_of = |id: &str| resolve_for(&base_for(v, id, &evo_refs), &refs, policy, &tenno);
        if run_cycle {
            let incarnon_panel = panel_of(incarnon_id(info).unwrap_or(&info.id));
            let base_panel = panel_of(&info.id);
            let params = DummyParams::incarnon_cycle_from_panels(
                &incarnon_panel,
                &base_panel,
                frenzy_single,
                cycle_frenzy_lock,
                &arena,
            );
            // The cycle reports the form it transforms INTO, as it always has.
            let mut params = params;
            params.infinite_reserve = incarnon_panel.reserve_is_infinite(infinite_ammo);
            (incarnon_panel, params)
        } else {
            let panel = panel_of(single_form);
            let mut d = DummyParams::from_panel(&panel, &arena);
            d.infinite_reserve = panel.reserve_is_infinite(infinite_ammo);
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
        // A SYNDICATE RADIAL is its own row for the same reason the field is:
        // it is neither the weapon's hit nor a status tick, it lands on its own
        // clock, and folding it into "direct" would credit the build for damage
        // no mod on it scaled.
        ("syndicate".to_string(), sd.syndicate, by_type(&sd.syndicate_by_type)),
        // AN EXTRA HIT is its own row too, and it is the one row the build
        // cannot move directly: Xata's Whisper takes a percentage of everything
        // else on this list, so a player tuning mods watches it follow. Folding
        // it into "direct" would hide that a fifth of the output is an ability's
        // and vanishes when the buff does.
        ("extra hit".to_string(), sd.extra_hit, by_type(&sd.extra_hit_by_type)),
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

    // THE REPLAY: the median engagement, re-run from the RNG state it started
    // from and sampled into frames. Buff series ride the same frames as the
    // pools, because "what were my stacks when its overguard broke" is one
    // question.
    //
    // OPT-IN, and that is not a micro-optimisation: the marginal-gain scan
    // calls this endpoint once per CANDIDATE — seventy mods on an axis — and
    // shows none of it. Only the Simulator's own Run asks for it, and pays one
    // extra engagement plus the frames on the wire (user, 2026-08-02).
    let replay = if get_bool(v, "replay", false) {
        let rep = wfsim_engine::dummy::replay(
            &params,
            m.rng_state,
            wfsim_engine::dummy::REPLAY_FRAMES,
        );
        // The panel's OWN shapes, one array per series instead of one number.
        // A frame is not a separate format: `kpi` mirrors the KPI row and
        // `sources` mirrors `damage_sources` key for key, so the client draws
        // an instant of the fight with the same code that draws the end of it.
        let pel = |f: &wfsim_engine::dummy::Frame| f.pellets.max(1) as f64;
        let series = |g: fn(&wfsim_engine::dummy::Frame) -> f64| {
            rep.frames.iter().map(&g).map(r1).collect::<Vec<_>>()
        };
        // Every (source, type) pair that carries damage BY THE END — the set
        // only ever grows, so the last frame names all of them and an earlier
        // frame simply reads zero there.
        let last = rep.frames.last().cloned().unwrap_or_default();
        let pick = |f: &wfsim_engine::dummy::Frame, k: &str| -> (f64, [f64; 15]) {
            match k {
                "direct" => (f.sources.direct, f.sources.direct_by_type),
                "radial" => (f.sources.radial, f.sources.radial_by_type),
                "field" => (f.sources.field, f.sources.field_by_type),
                "arcane" => (f.sources.arcane_on_status, f.sources.arcane_by_type),
                "syndicate" => (f.sources.syndicate, f.sources.syndicate_by_type),
                "extra hit" => (f.sources.extra_hit, f.sources.extra_hit_by_type),
                other => {
                    let i = TYPE_NAMES.iter().position(|n| *n == other).unwrap_or(0);
                    (f.sources.status[i], [0.0; 15])
                }
            }
        };
        let rp_sources: Vec<Value> = sources
            .iter()
            .map(|(name, _, by)| {
                let dmg: Vec<f64> =
                    rep.frames.iter().map(|f| pick(f, name).0.round()).collect();
                let types: Vec<Value> = if by.is_some() {
                    (0..15)
                        .filter(|&i| pick(&last, name).1[i] > 0.0)
                        .map(|i| {
                            json!({
                                "type": TYPE_NAMES[i],
                                "dmg": rep.frames.iter()
                                    .map(|f| pick(f, name).1[i].round())
                                    .collect::<Vec<_>>(),
                            })
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                json!({ "source": name, "dmg": dmg, "by_type": types })
            })
            .collect();
        json!({
            "dt": rep.dt,
            // Ids are the buff cards' own — the client joins on them for names.
            "buffs": rep.buffs.iter().map(|(id, max)| json!({ "id": id, "max": max }))
                .collect::<Vec<_>>(),
            "t": rep.frames.iter().map(|f| (f.t * 100.0).round() / 100.0).collect::<Vec<_>>(),
            "og": series(|f| f.overguard),
            "sh": series(|f| f.shield),
            "hp": series(|f| f.health),
            "dmg": series(|f| f.damage),
            "kills": rep.frames.iter().map(|f| f.kills).collect::<Vec<_>>(),
            "kpi": {
                "dps": rep.frames.iter()
                    .map(|f| if f.t > 0.0 { (f.damage / f.t).round() } else { 0.0 })
                    .collect::<Vec<_>>(),
                "procs": rep.frames.iter().map(|f| f.procs).collect::<Vec<_>>(),
                "shots": rep.frames.iter().map(|f| f.shots).collect::<Vec<_>>(),
                "reloads": rep.frames.iter().map(|f| f.reloads).collect::<Vec<_>>(),
                "transforms": rep.frames.iter().map(|f| f.transforms).collect::<Vec<_>>(),
                "crit_tier": series(|f| f.crit_tier_sum as f64 / f.pellets.max(1) as f64),
                "crit_rate": rep.frames.iter()
                    .map(|f| r3(f.crits as f64 / pel(f))).collect::<Vec<_>>(),
                "big_crit_rate": rep.frames.iter()
                    .map(|f| r3(f.big_crits as f64 / pel(f))).collect::<Vec<_>>(),
                "headshot_rate": rep.frames.iter()
                    .map(|f| r3(f.headshots as f64 / pel(f))).collect::<Vec<_>>(),
            },
            "sources": rp_sources,
            // Per BUFF, not per frame: a flat array per series is what a chart
            // wants, and it compresses far better than 600 tiny objects.
            "stacks": (0..rep.buffs.len())
                .map(|i| rep.frames.iter().map(|f| f.stacks[i]).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
        })
    } else {
        Value::Null
    };

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
        "replay": replay,
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
            "name": s_name(&specs, &enemy_id),
            "level": level,
            "steel_path": steel_path,
            // What was actually fought, not what was asked for — the default
            // is the unit's own answer, so a caller that said nothing still
            // needs telling which variant it got.
            "eximus": eximus,
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
    // REQUIRED mods, kept apart: only a mod in every build may lock a stat and
    // suppress another mod's buff card. A "search" mark is a candidate, and one
    // candidate's `disables:` says nothing about the builds without it.
    let mut required: Vec<String> = Vec::new();
    if let Some(obj) = v.get("mods").and_then(|x| x.as_object()) {
        for (id, st) in obj {
            if matches!(st.as_str(), Some("fixed") | Some("search")) {
                ids.push(id.clone());
            }
            if st.as_str() == Some("fixed") {
                required.push(id.clone());
            }
        }
    }
    ids.sort();
    ids.dedup();
    required.sort();
    required.dedup();
    // Rivens the request carries join the searchable pool like any mod.
    if let Err(e) = riven_stat_ids_ok(v, info) {
        return err_json(e);
    }
    // The WIDEST pool (nothing installed): this lists the buffs a scope could
    // produce, and a mod only some evolution variants can equip still produces
    // its buff in the variants that can.
    let full = mod_pool_with_rivens(v, info, &[]);
    let refs: Vec<&ModDef> = full
        .iter()
        .filter(|m| ids.iter().any(|id| id.as_str() == m.id))
        .collect();
    let mut out: Vec<BuffMeta> = Vec::new();
    let none = wfsim_engine::arcanes_data::ArcaneFx::none();
    let arc_base = WeaponBase::from_data(&info.id, true, &[]);
    let tenno = tenno_from(v, info);
    let always: Vec<&ModDef> = full
        .iter()
        .filter(|m| required.iter().any(|id| id.as_str() == m.id))
        .collect();
    merge(&mut out, enumerate_buffs(&refs, &always, &none, info, &tenno));
    // The scope is a MARK MAP (id -> "search" | "fixed"), the same shape as
    // `mods`; every marked arcane's buffs are configurable, pins included.
    if let Some(obj) = v.get("arcanes").and_then(|x| x.as_object()) {
        for a in obj.keys().filter(|k| k.as_str() != "none") {
            if let Some(def) = arcane_in_pools(info, a) {
                let fx = def.fx(def.max_rank, StackPolicy::Emergent, arc_base.traits, &tenno);
                merge(&mut out, enumerate_buffs(&[], &[], &fx, info, &tenno));
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
    /// Per evolution set, which `pool` indices that set cannot EQUIP. An equip
    /// rule is asked of every firing mode a weapon has, and installing the
    /// Incarnon form adds one — so a Cannonade belongs to the variants that
    /// leave tier 1 out and to no others. Same length as `evo_sets`, each entry
    /// as long as `pool`.
    variant_forbids: Vec<Vec<bool>>,
    exilus_defs: Vec<Option<ModDef>>,
    arcanes: Vec<wfsim_engine::arcanes_data::ArcaneFx>,
    /// What each entry of `arcanes` IS, in pool order — one id per slot,
    /// "none" for an empty one. The effects are merged and cannot be read
    /// back apart, so the naming travels beside them.
    arcane_sets: Vec<Vec<String>>,
    /// The DEPLOYMENT every candidate is built in — see `base_for`. Empty =
    /// the weapon's own column.
    deployment: String,
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
    /// Screen evaluations the SEARCH may spend before it hands its elites to
    /// the funnel. 0 = uncapped, and then the host's clock is the only bound
    /// (the browser sets one; a native run has a Cancel button instead).
    max_evals: u64,
    /// This run's STRIDE of the search space, of `shards` total. The browser
    /// buys coverage by running several Web Workers over disjoint strides and
    /// merging their leaderboards; a native run is one shard of one.
    shard: u32,
    shards: u32,
}

/// Validate an optimize request. `Err` is the ready-to-send error response.
pub fn parse_optimize(v: &Value) -> Result<OptimizePlan, Value> {
    // THE FIGHT FIRST, and everything below derives from it. Nothing here reads
    // the request for anything the simulator already decided — not the weapon,
    // not the player, not the run count. The optimizer parses its SCOPE and its
    // BUDGET, and that is the whole of its own business.
    let fight = parse_fight(v)?;
    let info = fight.info;
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
    // The WIDEST pool (nothing installed). Evolutions are a search DIMENSION, so
    // a mod can be legal in one variant and not in the next — which variant is
    // decided per candidate, below, not by narrowing the scope here.
    let full = mod_pool_with_rivens(v, info, &[]);
    for id in fixed_ids.iter().chain(search_ids.iter()) {
        if !full.iter().any(|m| m.id == id.as_str()) {
            return Err(err_json(mod_not_here(id, info, &[])));
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
    // How FULL a build must be, as its own axis (user, 2026-08-03: "搜索器可以
    // 有个设置，例如必须8个，<=8个，<=7个"). `build_size` is the ceiling and
    // `build_min` the floor, so "exactly 8" is (8, 8), "up to 8" is (1, 8) and
    // "up to 7" is (1, 7) — three settings rather than three behaviours.
    //
    // The DERIVED floor stays a floor: pooling mods is the statement that they
    // should be used, so at least one pooled mod is in every searched build,
    // and every required mod is in all of them. A `build_min` below that is
    // raised to it rather than rejected — it asks for builds the scope itself
    // has already ruled out.
    let derived_min = fixed_ids.len() + usize::from(!search_ids.is_empty());
    let build_min = get_u32(v, "build_min", 1).clamp(1, 8) as usize;
    if build_min > build_size {
        return Err(err_json(format!(
            "a build cannot hold at least {build_min} mods and at most {build_size}"
        )));
    }
    let min_slots = derived_min.max(build_min);
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
    // The product can skip a tier — mark tier 2 and leave tier 1 unmarked and
    // it pairs "nothing at 1" with "Final Fusillade at 2", which is a build
    // the game cannot make. Cut each set to its reachable prefix (the same
    // rule every other entry point applies) and dedupe, rather than searching
    // variants that would be filtered away at the moment they were scored.
    for set in evo_sets.iter_mut() {
        *set = ladder_prefix(std::mem::take(set));
    }
    evo_sets.sort();
    evo_sets.dedup();
    for set in &evo_sets {
        for id in set {
            if wfsim_engine::evolutions_data::get(id).is_none() {
                return Err(err_json(format!("unknown evolution id: {id}")));
            }
        }
    }

    // ---- what each variant may not EQUIP -----------------------------------
    //
    // An equip rule is asked of every firing mode a weapon has, and a variant
    // that installs the Incarnon form has two — so a Cannonade is legal in the
    // variants that leave tier 1 out and illegal in the ones that do not. That
    // is a per-CANDIDATE fact, not a per-scope one: narrowing the pool to what
    // every variant can equip would throw away the builds where the mod is the
    // point, and leaving it alone would crown a build the game refuses.
    //
    // Same rule the simulator applies, from the same engine call — the pool the
    // sim resolves a build against IS `pool_for_build` (hard rule: the optimizer
    // obeys the simulator).
    let variant_pools: Vec<Vec<ModDef>> = evo_sets
        .iter()
        .map(|set| {
            let refs: Vec<&str> = set.iter().map(String::as_str).collect();
            mod_pool_with_rivens(v, info, &refs)
        })
        .collect();
    // Per variant, which SCOPE indices it cannot equip — the shape the walk
    // wants, resolved once instead of per subset.
    let variant_forbids: Vec<Vec<bool>> = variant_pools
        .iter()
        .map(|legal| {
            pool.iter().map(|m| !legal.iter().any(|x| x.id == m.id)).collect()
        })
        .collect();
    // A REQUIRED mod no variant can equip is a contradiction the search cannot
    // resolve by choosing differently — say it now rather than answer with an
    // empty leaderboard.
    for (i, m) in pool.iter().enumerate() {
        if constraints.require.iter().any(|r| r == m.id) && variant_forbids.iter().all(|f| f[i]) {
            return Err(err_json(format!(
                "{} is required, and no evolution set in this scope can equip it — \
                 it needs the same trigger on every firing mode",
                m.name
            )));
        }
    }
    // The EXILUS table is shared across variants (a candidate stores its option
    // by INDEX, and the index has to name the same option in every one), so an
    // exilus option must be equippable under all of them. Vacuous today — no
    // exilus mod carries an equip rule — and stated so it stays a rule rather
    // than an accident if one ever does.
    for d in exilus_defs.iter().flatten() {
        if let Some(legal) = variant_pools
            .iter()
            .find(|legal| !legal.iter().any(|x| x.id == d.id))
        {
            let _ = legal;
            return Err(err_json(format!(
                "{} cannot be equipped under every evolution set in this scope, so it \
                 cannot be an exilus option — pin the evolutions, or drop it",
                d.name
            )));
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
    // The FIGHT's player, not a second one built the same way. Identical today
    // — same function, same request — which is exactly why it was easy to leave
    // and exactly why it should not be: two constructions of one fact is how
    // they come to differ (user, 2026-08-04: "真相源要单一").
    let tenno = &fight.tenno;
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
                    .map(|d| d.fx(d.max_rank, StackPolicy::Emergent, arc_base.traits, tenno))
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
    let deployment = get_str(v, "deployment", "").to_string();
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
    //
    // `final_runs` FALLS BACK TO THE SCENARIO'S `runs` (user, 2026-08-02).
    // How hard you measure is the scenario's question and it is already
    // answered there — a second default here is how a winner gets crowned at a
    // precision the replay never used. The web client stops sending its own
    // and this is what it lands on.
    // Falls back to the FIGHT's run count rather than a second reading of
    // `runs`. The two differed only past 20,000 — where the sim clamps and the
    // search did not — which is a divergence nobody would have gone looking for.
    let final_runs = get_u32(v, "final_runs", fight.runs).clamp(1, 100_000);
    let finalists = get_u32(v, "finalists", 10).clamp(1, 100) as usize;

    // ---- THE FIGHT: the simulator's, not a second reading of it ----------
    //
    // Every field below used to be parsed again here, and the two readings
    // drifted three times in three days (see `parse_fight`). The optimizer's
    // winner is replayed under the simulator's fight, so the only safe number
    // of places to decide what that fight IS, is one.
    //
    // What stays here is what is genuinely the optimizer's: the scope to
    // search and the budget to spend.
    let fight = parse_fight(v)?;
    let untransformed_id = fight.untransformed_id.clone();
    let unlock_evo = if fight.form == "base" {
        form_unlock_evo(fight.info).map(String::from)
    } else {
        // Asking for a form implies the evolution that IS that form, so there
        // is nothing left to gate on.
        None
    };
    // WHICH WEAPON ENTRY FIRES. Not `single_form`: for a CYCLE the optimizer
    // fires the Incarnon half and returns to `untransformed_id` between
    // transmutes, while `single_form` answers "the one form to fire when there
    // is no cycle" and resolves `incarnon_cycle` — a mode, not a form — to the
    // weapon's default. Mapping one onto the other made the search run the
    // BASE form of every cycling weapon: the Torid lost 9x and the Boar GAINED,
    // which is exactly the shape of "both ran their base form" (caught by the
    // optimizer baseline, 2026-08-04).
    let fire_id = firing_entry(&fight);
    let cycle_from = fight.run_cycle.then(|| fight.info.id.clone());
    // Read off the fight before the arena is moved into the scenario. These
    // are what the PLAN reports about itself, not decisions it makes.
    let (headshot_pct, duration, level, steel_path) =
        (fight.headshot_pct, fight.duration, fight.level, fight.steel_path);
    let enemy_id = fight.enemy_id.clone();
    let specs = enemies();


    // Assembled ENTIRELY from the fight — no field is re-read from the request
    // here, which is what makes "the search and the replay run the same fight"
    // structural rather than a thing to keep checking.
    let scenario = Scenario {
        arena: fight.arena,
        frenzy: fight.has_frenzy,
        incarnon_cycle: fight.run_cycle,
        frenzy_lock: fight.cycle_frenzy_lock,
        frenzy_locks: fight.frenzy_locks,
        buff_cfg: fight.buff_cfg.unwrap_or_default(),
        infinite_ammo: fight.infinite_ammo,
        policy: fight.policy,
    };

    Ok(OptimizePlan {
        weapon_id: info.id.clone(),
        pool,
        constraints,
        min_slots,
        build_size,
        evo_sets,
        variant_forbids,
        exilus_defs,
        arcanes,
        arcane_sets,
        deployment: deployment.clone(),
        scenario,
        final_runs,
        finalists,
        headshot_pct,
        duration,
        target_name: s_name(&specs, &enemy_id),
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
        max_evals: v.get("max_evals").and_then(|x| x.as_u64()).unwrap_or(0),
        shards: v.get("shards").and_then(|x| x.as_u64()).unwrap_or(1).clamp(1, 64) as u32,
        shard: v.get("shard").and_then(|x| x.as_u64()).unwrap_or(0).min(63) as u32,
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

/// Where the SEARCH publishes its best-so-far. Result-shaped, so a cancel
/// renders it through the same path a finished run takes. Display only: the
/// screen is one pass over the whole scope, so a snapshot of it is NOT a
/// resume point — continuing from one would silently drop the unwalked part.
pub type BoardSink<'a> = dyn Fn(&Value) + 'a;

/// The uninterrupted entry point — no checkpointing, no resume.
/// GRADE the search against ground truth — the same request, the same plan,
/// the same fight, answered twice: once by the production search and once by
/// exhausting the scope and evaluating every job flat.
///
/// This goes through [`parse_optimize`] rather than assembling a scenario of
/// its own, because a grader that builds its own fight grades a different one
/// — the exact failure this repo has already been bitten by (OPTIMIZER.md,
/// "The search and the replay must be the SAME fight").
///
/// It REFUSES a scope it cannot exhaust. A reference that samples is not a
/// reference; if the scope is too big to enumerate, the honest answer is to
/// say so and let the caller narrow it, not to grade against a guess.
pub fn grade_optimize(
    v: &Value,
    truth_runs: u32,
    max_jobs: usize,
    search_evals: u64,
    explore_frac: f64,
) -> Value {
    use wfsim_optimizer::truth::{judge, Truth};
    let plan = match parse_optimize(v) {
        Ok(p) => p,
        Err(e) => return e,
    };
    let OptimizePlan {
        pool,
        constraints,
        min_slots,
        build_size,
        evo_sets,
        variant_forbids,
        exilus_defs,
        arcanes,
        scenario,
        final_runs,
        finalists,
        deployment,
        fire_id,
        cycle_from,
        unlock_evo,
        untransformed_id,
        weapon_id,
        threads,
        ..
    } = plan;
    wfsim_optimizer::set_worker_threads(threads);
    let info = weapon(&weapon_id);
    let innate = wfsim_engine::weapons_data::innate_slots(&info.id);
    let exilus_refs: Vec<Option<&ModDef>> = exilus_defs.iter().map(|o| o.as_ref()).collect();
    let deployed = |id: &str, refs: &[&str]| {
        let mut b = WeaponBase::from_data(id, true, refs);
        if !deployment.is_empty() {
            wfsim_engine::weapons_data::apply_deployment(&mut b, id, &deployment);
        }
        b
    };

    // ---- exhaust the scope (the same walk the search starts from) ----
    let state = FunnelState::default();
    let mut cands: Vec<Candidate> = Vec::new();
    for (vi, set) in evo_sets.iter().enumerate() {
        let refs: Vec<&str> = set.iter().map(String::as_str).collect();
        let unlocked = match unlock_evo.as_deref() {
            Some(u) => set.iter().any(|e| e == u),
            None => true,
        };
        let (base, base_form) = if unlocked {
            (deployed(&fire_id, &refs), cycle_from.as_ref().map(|id| deployed(id, &refs)))
        } else {
            (deployed(&untransformed_id, &refs), None)
        };
        // What THIS variant cannot equip is a forbid like any other: a mod that
        // needs the same trigger on every firing mode is out of the sets that
        // install a second one. The grader must walk exactly the space the
        // search walks, so it applies the same list.
        let vc = Constraints {
            require: constraints.require.clone(),
            forbid: constraints
                .forbid
                .iter()
                .cloned()
                .chain(
                    variant_forbids[vi]
                        .iter()
                        .enumerate()
                        .filter(|(_, &f)| f)
                        .map(|(i, _)| pool[i].id.to_string()),
                )
                .collect(),
        };
        // A required mod this variant cannot equip empties it — `parse_optimize`
        // has already refused a scope where NO variant can, so this is the
        // ordinary "this set is not the one" case.
        if vc.require.iter().any(|r| vc.forbid.iter().any(|f| f == r)) {
            continue;
        }
        let (mut c, _stats, complete) = enumerate_candidates_observed(
            &pool,
            &base,
            base_form.as_ref(),
            vi as u32,
            min_slots as u32,
            build_size as u32,
            60,
            &innate,
            &vc,
            &exilus_refs,
            Some(&state),
            max_jobs.saturating_sub(cands.len()).max(1),
            &scenario.arena.tenno,
            scenario.policy,
        );
        cands.append(&mut c);
        if !complete || cands.len() >= max_jobs {
            return err_json(format!(
                "this scope is too big to grade: it does not fit {max_jobs} candidates. \
                 Ground truth means evaluating EVERY build, so narrow the scope \
                 (fewer pooled mods, or pin some) and grade that."
            ));
        }
    }
    if cands.is_empty() {
        return err_json("no legal builds in this scope");
    }
    let jobs: Vec<Job> = (0..cands.len())
        .flat_map(|i| (0..arcanes.len()).map(move |a| (i, a)))
        .collect();
    if jobs.len() > max_jobs {
        return err_json(format!(
            "{} jobs ({} builds x {} arcane sets) exceeds the {max_jobs} the grader will exhaust",
            jobs.len(),
            cands.len(),
            arcanes.len()
        ));
    }

    // ---- the reference, twice: a reference that cannot reproduce itself
    // under a second seed has not established anything.
    let a = Truth::measure(&cands, &jobs, &arcanes, &scenario, truth_runs, 0xA11CE);
    let b = Truth::measure(&cands, &jobs, &arcanes, &scenario, truth_runs, 0xB0B);
    let answer = a.indistinguishable(3.0);
    let settled = answer.contains(&b.best()) && b.indistinguishable(3.0).contains(&a.best());
    let overlap = a.agrees_with(&b, finalists);

    // ---- the PRODUCTION PIPELINE, on the same scope ----
    //
    // Search AND funnel, not just the funnel. Grading the funnel alone was
    // grading the half that was already good: it is handed a job list, and the
    // half that decides WHAT IS IN that list is the half that could lose the
    // winner. `search_evals` is the budget under test — 0 runs the search to
    // the end of the space, which is what a scope small enough to exhaust gets
    // in production too.
    let families: Vec<Option<&'static str>> = pool.iter().map(|m| m.family).collect();
    let usable: Vec<usize> = (0..pool.len()).collect();
    let required: Vec<usize> = constraints
        .require
        .iter()
        .filter_map(|r| pool.iter().position(|m| m.id == *r))
        .collect();
    let space =
        wfsim_optimizer::space::SubsetSpace::new(&families, &usable, &required, min_slots, build_size);
    let forms: Vec<(WeaponBase, Option<WeaponBase>)> = evo_sets
        .iter()
        .map(|set| {
            let refs: Vec<&str> = set.iter().map(String::as_str).collect();
            let unlocked = match unlock_evo.as_deref() {
                Some(u) => set.iter().any(|e| e == u),
                None => true,
            };
            if unlocked {
                (deployed(&fire_id, &refs), cycle_from.as_ref().map(|id| deployed(id, &refs)))
            } else {
                (deployed(&untransformed_id, &refs), None)
            }
        })
        .collect();
    let expand = |subset: &[usize]| -> Vec<Candidate> {
        let mut out = Vec::new();
        for (vi, (base, base_form)) in forms.iter().enumerate() {
            // A mod this variant cannot equip vetoes the (subset, variant)
            // PAIR, not the subset: the same eight mods are a legal build under
            // an evolution set that leaves the Incarnon form out.
            if subset.iter().any(|&i| variant_forbids[vi][i]) {
                continue;
            }
            wfsim_optimizer::expand_one(
                &pool, base, base_form.as_ref(), vi as u32, 60, &innate, &exilus_refs,
                subset, &scenario.arena.tenno, scenario.policy, &mut out,
            );
        }
        out
    };
    let cfg = wfsim_optimizer::search::SearchConfig {
        max_evals: search_evals,
        explore_frac,
        keep: 65_536,
        seed: 0xDEAD_BEEF,
        ..Default::default()
    };
    let (screened, sstats) =
        wfsim_optimizer::search::search(&space, &expand, &arcanes, &scenario, &cfg, None, None);
    let mut sc: Vec<Candidate> = Vec::new();
    let mut by_ptr: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut sjobs: Vec<Job> = Vec::new();
    for sj in &screened {
        let key = std::sync::Arc::as_ptr(&sj.cand) as usize;
        let ci = *by_ptr.entry(key).or_insert_with(|| {
            sc.push((*sj.cand).clone());
            sc.len() - 1
        });
        sjobs.push((ci, sj.ai));
    }
    let rounds = schedule_to(sjobs.len(), final_runs, finalists);
    let planned: u64 = {
        let mut field = sjobs.len() as u64;
        let mut n = sstats.evals;
        for &(r, keep, _) in &rounds {
            n += field * u64::from(r);
            field = field.min(keep as u64);
        }
        n
    };
    let last = run_funnel(
        &sc, &arcanes, &scenario, sjobs, &rounds, 0xDEAD_BEEF, false,
        None, None, 0, None, None,
    );
    // Map each result back to its position in the exhaustive job list. Both
    // sides build candidates through `expand_one` from an ascending subset, so
    // the identity is exact rather than a resolved-vector comparison.
    let ix_of: std::collections::HashMap<(Vec<usize>, u32, u32, usize), usize> = jobs
        .iter()
        .enumerate()
        .map(|(ji, &(ci, ai))| {
            ((cands[ci].ordered.clone(), cands[ci].variant, cands[ci].exilus, ai), ji)
        })
        .collect();
    let mut board: Vec<usize> = Vec::new();
    let mut unmatched = 0usize;
    for &((ci, ai), _) in last.iter() {
        let k = (sc[ci].ordered.clone(), sc[ci].variant, sc[ci].exilus, ai);
        match ix_of.get(&k) {
            Some(&ji) => board.push(ji),
            None => unmatched += 1,
        }
    }
    if board.is_empty() {
        return err_json(
            "the search returned nothing the exhaustive enumeration contains —              the two disagree about the scope, which is a bug in one of them",
        );
    }
    let verdict = judge(&a, &board, finalists, planned);

    let row = |ji: usize| -> Value {
        let (ci, ai) = jobs[ji];
        let c = &cands[ci];
        json!({
            "mods": c.ordered.iter().map(|&i| pool[i].id).collect::<Vec<_>>(),
            "arcane": ai,
            "vector": c.panel.damage.iter_nonzero()
                .map(|(t, v)| format!("{t:?} {v:.0}")).collect::<Vec<_>>(),
            "mean": a.est[ji].mean,
            "se": a.est[ji].se,
        })
    };
    json!({
        "ok": true,
        "scope": { "builds": cands.len(), "jobs": jobs.len(), "exhaustive": true },
        "reference": {
            "runs": truth_runs,
            "sims": verdict.reference_sims,
            "answer_set": answer.len(),
            "settled": settled,
            "cross_seed_overlap": overlap,
            "top": a.order.iter().take(finalists).map(|&j| row(j)).collect::<Vec<_>>(),
        },
        "search": {
            "unmatched": unmatched,
            "coverage": sstats.coverage(),
            "space": sstats.space as f64,
            "exhaustive": sstats.exhaustive,
            "subsets": sstats.subsets,
            "neighbours": sstats.neighbours,
            "screen_evals": sstats.evals,
            "rank": verdict.rank,
            "regret": verdict.regret,
            "within_noise": verdict.within_noise,
            "recall": verdict.recall,
            "sims": verdict.sims,
            "top": board.iter().take(finalists).map(|&j| row(j)).collect::<Vec<_>>(),
        },
    })
}

pub fn run_optimize(
    plan: OptimizePlan,
    state: &FunnelState,
    on_enumerated: impl FnOnce(usize, usize),
    on_round: Option<&dyn Fn()>,
) -> Value {
    run_optimize_resumable(plan, state, on_enumerated, on_round, None, None, None)
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
) -> Value {
    let OptimizePlan {
        pool,
        constraints,
        min_slots,
        build_size,
        evo_sets,
        variant_forbids,
        exilus_defs,
        arcanes,
        arcane_sets,
        deployment,
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
        max_evals,
        shard,
        shards,
    } = plan;
    // Compute budget: 0 = auto (all cores minus two — the machine must stay
    // usable while the search runs). Applies to the screen and every round.
    wfsim_optimizer::set_worker_threads(threads);
    let info = weapon(&weapon_id);

    // How many screened jobs survive the search into the funnel. The search
    // holds a heap this size, so memory is O(SCREEN_KEEP) whatever the scope.
    const SCREEN_KEEP: usize = 65_536;
    let innate = wfsim_engine::weapons_data::innate_slots(&info.id);
    let exilus_refs: Vec<Option<&ModDef>> = exilus_defs.iter().map(|o| o.as_ref()).collect();
    // The form(s) an evolution set resolves to, decided once in
    // `parse_optimize`. A single-form weapon has NO second panel — handing
    // the enumerator a duplicate of the first would tell it there was a cycle
    // to simulate, and the scenario says there is not.
    // Every base the worker builds sits in the run's DEPLOYMENT, so a search
    // scores the same environment the sim would replay it in.
    let deployed = |id: &str, refs: &[&str]| {
        let mut b = WeaponBase::from_data(id, true, refs);
        if !deployment.is_empty() {
            wfsim_engine::weapons_data::apply_deployment(&mut b, id, &deployment);
        }
        b
    };
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
            return (deployed(&untransformed_id, refs), None);
        }
        (
            deployed(&fire_id, refs),
            cycle_from
                .as_ref()
                .map(|id| deployed(id, refs)),
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

    // ---- ONE PATH, at every scope ----
    //
    // There used to be two regimes with a 2,000,000-candidate threshold
    // between them: materialize-then-funnel below it, walk-and-screen above.
    // Both walked the space depth-first, so both left a lexicographic CORNER
    // behind when they were cut short — and being cut short is the normal case
    // (docs/OPTIMIZER.md). Two regimes also meant two sets of bugs; the
    // tenno/policy leak of 2026-08-03 existed in exactly one of them.
    //
    // Now the subset space is an INDEX RANGE and the search walks a
    // pseudorandom bijection over it: run it to the end and it is an
    // exhaustive enumeration, stop it early and what it has is a uniform
    // sample. Same loop, same code, no threshold (user, 2026-08-03: 不要搞
    // 大小区分 — 严谨性大于便利性).
    let usable: Vec<usize> = (0..pool.len())
        .filter(|&i| !constraints.forbid.iter().any(|f| f == pool[i].id))
        .collect();
    let required: Vec<usize> = constraints
        .require
        .iter()
        .filter_map(|r| pool.iter().position(|m| m.id == *r))
        .collect();
    let families: Vec<Option<&'static str>> = pool.iter().map(|m| m.family).collect();
    let space =
        wfsim_optimizer::space::SubsetSpace::new(&families, &usable, &required, min_slots, build_size);

    // The bases each evolution set resolves to, built ONCE. `forms_for` reads
    // data and applies the deployment, which is far too expensive to repeat per
    // proposal.
    let forms: Vec<(WeaponBase, Option<WeaponBase>)> = evo_sets
        .iter()
        .map(|set| {
            let refs: Vec<&str> = set.iter().map(String::as_str).collect();
            forms_for(set, &refs)
        })
        .collect();
    // One subset -> every candidate it can produce. The axes INSIDE a subset
    // (element order, exilus option, evolution set) stay exhaustive: a couple
    // of dozen cheap combinations, and handing an exact subproblem to a
    // stochastic search is how an answer gets lost for no reason.
    let expand = |subset: &[usize]| -> Vec<Candidate> {
        let mut out = Vec::new();
        for (vi, (base, base_form)) in forms.iter().enumerate() {
            // A mod this variant cannot equip vetoes the (subset, variant)
            // PAIR, not the subset: the same eight mods are a legal build under
            // an evolution set that leaves the Incarnon form out.
            if subset.iter().any(|&i| variant_forbids[vi][i]) {
                continue;
            }
            wfsim_optimizer::expand_one(
                &pool,
                base,
                base_form.as_ref(),
                vi as u32,
                60,
                &innate,
                &exilus_refs,
                subset,
                &scenario.arena.tenno,
                scenario.policy,
                &mut out,
            );
        }
        out
    };

    // MID-SEARCH RESUME IS GONE, and only the ROUND checkpoint survives. The
    // old one stored a position in a depth-first walk plus the survivors at
    // that cut; the search walks a shuffled index range and climbs from an
    // elite pool, so a position alone no longer describes where it was. It is
    // also worth much less than it was: the screen it protected could run for
    // twenty minutes, while the search runs to a stated budget and publishes a
    // best-so-far the whole way. Restoring it means checkpointing the elite
    // pool by identity and re-screening it on resume — a follow-up, recorded
    // here so it reads as a decision and not as an omission.
    let round_resume = match resume {
        Some(ResumeFrom::Round { round, alive, jobs_at_start }) => {
            Some((round, alive, jobs_at_start))
        }
        _ => None,
    };
    // What the search covered — `None` on a round resume, which does not search.
    let mut search_stats: Option<wfsim_optimizer::search::SearchStats> = None;
    let (cands, last, cancelled, n_jobs) = if let Some((r_round, r_alive, r_jobs_at_start)) = round_resume {
        // ---- RESUME: no walk at all. The checkpoint holds identities, so the
        // candidates are rebuilt with the same plan_forma / resolve_with the
        // enumerator uses and come out bit-identical. Seeds key off the
        // absolute round index, so the numbers match an uninterrupted run.
        let mut cands: Vec<Candidate> = Vec::new();
        let mut jobs: Vec<Job> = Vec::new();
        for (ordered, variant, exilus, ai) in &r_alive {
            let Some(set) = evo_sets.get(*variant as usize) else { continue };
            // A checkpoint predating an equip rule can name a build this variant
            // can no longer wear. Dropping it is the same answer the walk would
            // give now; if that empties the list, the error below says so.
            let Some(forbid) = variant_forbids.get(*variant as usize) else { continue };
            if ordered.iter().any(|&i| forbid.get(i).copied().unwrap_or(false)) {
                continue;
            }
            let refs: Vec<&str> = set.iter().map(String::as_str).collect();
            let (base, base_form) = forms_for(set, &refs);
            let Some(c) = wfsim_optimizer::rebuild_candidate(
                &pool, &base, base_form.as_ref(), &innate, 60, &scenario.arena.tenno, scenario.policy,
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
    } else {
        // ---- THE SEARCH ----
        if space.is_empty() {
            return err_json("no builds in this scope (the size range leaves nothing to search)");
        }
        let board = on_board.map(|f| {
            move |top: &[wfsim_optimizer::ScreenedJob]| {
                let rows: Vec<Value> = top.iter().take(finalists).enumerate()
                    .map(|(rank, sj)| entry(rank, &sj.cand, sj.ai, &sj.summary))
                    .collect();
                let walked = state.enumerated.load(std::sync::atomic::Ordering::Relaxed) as usize;
                let screened = state.sims_done.load(std::sync::atomic::Ordering::Relaxed) as usize;
                f(&board_json(rows, walked, screened));
            }
        });
        let cfg = wfsim_optimizer::search::SearchConfig {
            max_evals,
            keep: SCREEN_KEEP,
            seed: 0xDEAD_BEEF,
            shard,
            shards,
            ..Default::default()
        };
        let (screened, stats) = wfsim_optimizer::search::search(
            &space,
            &expand,
            &arcanes,
            &scenario,
            &cfg,
            Some(state),
            board.as_ref().map(|f| f as &wfsim_optimizer::ScreenBoardFn<'_>),
        );
        search_stats = Some(stats);
        if screened.is_empty() {
            if state.cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return cancelled_json(0);
            }
            // AN EMPTY SHARD IS NOT A FAILURE. With more workers than index
            // positions — 8 workers over a scope holding one build — every
            // shard but the first owns no ground at all, and each of them used
            // to answer "no legal builds in this scope (Forma / family
            // constraints eliminated all)", a sentence about the pool that was
            // really about the arithmetic. The fleet then surfaced one of those
            // as the whole run's error.
            //
            // Walking nothing is different from walking and finding nothing.
            // Only the second is the Forma/family case.
            if stats.sampled > 0 {
                return err_json(
                    "no legal builds in this scope (Forma / family constraints eliminated all)",
                );
            }
            return json!({
                "ok": true, "cancelled": false,
                "exhaustive": true, "coverage": 0.0,
                "space": stats.space as f64, "searched": 0, "sampled": 0.0,
                "shard": shard, "shards": shards,
                "candidates": 0, "jobs": 0,
                "final_runs": final_runs, "finalists": finalists,
                "headshot_pct": headshot_pct, "duration": duration,
                "results": [],
                "target": { "name": target_name, "level": level, "steel_path": steel_path },
            });
        }
        // Survivors -> a deduplicated candidate table (one build survives under
        // several arcanes) + (job, screen summary) pairs, best first.
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
        if state.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            // Cancelled mid-search: the screen's own ranking (1-run precision)
            // is the best-so-far leaderboard.
            let n = slast.len();
            (sc, slast, true, n)
        } else {
            let jobs: Vec<Job> = slast.iter().map(|(j, _)| *j).collect();
            let n = jobs.len();
            state.sims_done.store(0, std::sync::atomic::Ordering::Relaxed); // fresh % for the funnel
            on_enumerated(sc.len(), n);
            let rounds = schedule_to(n, final_runs, finalists);
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
                &sc, &arcanes, &scenario, jobs, &rounds, 0xDEAD_BEEF, false,
                Some(state), on_round,
                0, // the search always screens first, so the funnel starts fresh
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

    // WHAT THE SEARCH ACTUALLY COVERED. A run that did not reach the end of
    // its space has not searched the scope it was given, and it must not read
    // like one that did. `cancelled` cannot carry this — that means "you
    // stopped it" — and neither can a bare flag, because the useful question
    // is HOW MUCH. `exhaustive` says the search reached the end of the index
    // range, in which case its winner is THE winner and not a best-so-far.
    let (exhaustive, coverage, space_size, searched, sampled) = match search_stats {
        Some(st) => (
            st.exhaustive,
            st.coverage(),
            // A space can exceed f64's integer range only in absurd scopes; the
            // UI prints an order of magnitude either way.
            st.space as f64,
            st.subsets,
            st.sampled as f64,
        ),
        // A round resume did not search: it continues a funnel over a field
        // some earlier run already chose, and claiming coverage for it would
        // be claiming credit for a search this call never ran.
        None => (false, 0.0, 0.0, 0, 0.0),
    };
    json!({
        "ok": true,
        "candidates": cands.len(),
        "jobs": n_jobs,
        "cancelled": cancelled,
        "exhaustive": exhaustive,
        "coverage": coverage,
        "space": space_size,
        "searched": searched,
        // INDEX POSITIONS this shard consumed. Coverage above is this shard's
        // alone; a sharded run sums these and divides by `space` to get the
        // coverage of the whole fleet.
        "sampled": sampled,
        "shard": shard,
        "shards": shards,
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
    /// Every weapon can be given a riven, so every weapon must reach a stat
    /// pool. `riven_class` walks outward from the narrowest mod pool and stops
    /// at the first one that has stats — with none, it returns "" and the
    /// editor renders a riven with NOTHING to roll, which is how the Larkspur
    /// Prime shipped until `data/rivens/archgun.yaml` existed (user,
    /// 2026-08-01). The next class added lands here instead of in the UI.
    #[test]
    fn every_weapon_reaches_a_riven_stat_pool() {
        let mut orphans: Vec<String> = Vec::new();
        for w in weapons() {
            let class = riven_class(w);
            let n = wfsim_engine::rivens_data::pool(&class).len();
            // What is left after the weapon's own exclusions is what the
            // editor actually offers — a pool the weapon excludes down to
            // nothing is the same empty card by another route.
            let excluded = wfsim_engine::rivens_data::excluded_for(&w.id).len();
            if n == 0 || n <= excluded {
                orphans.push(format!("{} (pools {:?} -> {class:?}, {n} stats, {excluded} excluded)",
                    w.id, w.mod_pools));
            }
        }
        assert!(orphans.is_empty(), "weapons with no riven stats: {orphans:#?}");
    }

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

    /// ASKING FOR A FORM IS ENOUGH — the evolution that IS that form is
    /// implied, not demanded.
    ///
    /// This is the state the page STARTS in: no evolutions chosen. It used to
    /// fall back to the base form for every request, so all three options
    /// produced one number and the control said otherwise (user, 2026-08-04:
    /// "灵化循环和基础，纯灵化好像都不起作用"). Three options, one outcome.
    ///
    /// Implying it is exact rather than generous: tier 1 is `selection: fixed`
    /// on every Incarnon ladder — not a choice but what installing the Genesis
    /// grants — and it applies no stat, so the numbers below are IDENTICAL to
    /// the same request with the evolution named explicitly. That equality is
    /// the real assertion; three different numbers alone would not prove the
    /// implication is free.
    #[test]
    fn a_form_request_implies_its_own_unlock() {
        let dps = |v: &Value| v.get("dps").and_then(Value::as_f64).unwrap_or(0.0);
        let with_evo = |form: &str| {
            simulate_json(&json!({
                "weapon": "boar_prime", "form": form, "mods": [], "arcane": "none",
                "evolutions": ["boar_prime_evo1_incarnon_form"],
                "enemy": "thrax_centurion", "duration": 30.0, "runs": 8,
                "headshot_pct": 100.0, "seed": 7,
            }))
        };
        let (base, inc, cyc) = (sim("boar_prime", "base"), sim("boar_prime", "incarnon"),
                                sim("boar_prime", "incarnon_cycle"));
        // Three options, three fights — which is BEST depends on the build, so
        // no ordering is asserted, only that the choice does something.
        for (a, b) in [(&base, &inc), (&base, &cyc), (&inc, &cyc)] {
            assert!((dps(a) - dps(b)).abs() > 1e-6, "{} vs {}", dps(a), dps(b));
        }
        // ...and naming the evolution changes nothing, because it carries none.
        for form in ["base", "incarnon", "incarnon_cycle"] {
            assert!(
                (dps(&sim("boar_prime", form)) - dps(&with_evo(form))).abs() < 1e-9,
                "{form}: implying the unlock is not the same as naming it"
            );
        }
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

        // ...and the POSITIVE case, because "0 transforms" is also what a
        // weapon that IS supposed to cycle looks like when its base entry was
        // never wired to its Incarnon entry. Boar Prime shipped that way for
        // an afternoon: the second weapon file existed, the evolutions
        // existed, and `transforms_to` did not — so the cycle silently ran the
        // base form and reported a number that looked fine on its own
        // (2026-08-03).
        let cycled = simulate_json(&json!({
            "weapon": "boar_prime", "form": "incarnon_cycle", "mods": [],
            "evolutions": ["boar_prime_evo1_incarnon_form"],
            "enemy": "thrax_centurion", "duration": 120.0, "runs": 4,
            "headshot_pct": 100.0, "seed": 7,
        }));
        assert_eq!(cycled["ok"], json!(true));
        assert!(
            cycled["transforms"].as_f64().unwrap_or(0.0) > 0.0,
            "Boar Prime's Incarnon cycle never transformed — is `transforms_to` wired? got {}",
            cycled["transforms"]
        );

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

    /// Evolutions are a LADDER, not a menu: tier N needs tier N-1 installed.
    /// The UI locks the rows, but a preset saved before the rule existed can
    /// still name a tier-4 perk with nothing under it — that build is not
    /// weaker, it is unreachable, so its orphans are dropped rather than
    /// priced. Commodore's Fortune (tier 4, +20% base crit) shows it: alone it
    /// must change nothing, and only the full 1-2-3-4 chain may pay out.
    #[test]
    fn an_evolution_tier_needs_the_one_below_it() {
        let with = |evos: Value| {
            let p = panel_json(&json!({ "weapon": "torid", "evolutions": evos }));
            // Crit chance rides the projectile, so it is a row of the "direct
            // hit" PART rather than of the weapon block.
            p["forms"][0]["parts"][0]["stats"]
                .as_array()
                .expect("stat rows")
                .iter()
                .find(|r| r["key"] == json!("crit_chance"))
                .expect("a crit chance row")["final"]
                .as_str()
                .expect("a formatted crit chance")
                .to_string()
        };
        let bare = with(json!([]));
        assert_eq!(bare, "15.0%", "the Torid's unmodded crit chance");
        assert_eq!(with(json!(["torid_commodores_fortune"])), bare, "tier 4 alone paid out");
        assert_eq!(
            with(json!(["torid_evo1_incarnon_form", "torid_commodores_fortune"])),
            bare,
            "tier 4 paid out over a gap at tiers 2-3"
        );
        // Order in the array is the client's, not the ladder's — the whole
        // chain counts however it arrives.
        let full = json!([
            "torid_commodores_fortune", "torid_extended_volley",
            "torid_final_fusillade", "torid_evo1_incarnon_form",
        ]);
        assert_eq!(with(full), "35.0%", "the full chain did not pay out");
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

#[cfg(test)]
mod optimizer_evolution_tests {
    use super::*;

    fn sets(evolutions: Value) -> Vec<Vec<String>> {
        parse_optimize(&json!({
            "weapon": "torid",
            "size": 1,
            "mods": { "serration": "search" },
            "evolutions": evolutions,
        }))
        .expect("a plan")
        .evo_sets
    }

    /// The scope's Cartesian product must not enumerate a gapped LADDER.
    ///
    /// A tier with no marks contributes "nothing here", so marking tier 2 and
    /// leaving tier 1 blank pairs them into a set the game cannot make. Every
    /// such set would be truncated at the moment it was scored, so searching
    /// it is not a wrong answer, it is a wasted variant — and a reported one,
    /// since the winner prints the set it was given.
    #[test]
    fn the_search_never_enumerates_a_tier_without_the_one_below_it() {
        // Tier 2 alone: nothing to search, one empty set.
        assert_eq!(sets(json!({ "2": ["torid_final_fusillade"] })), vec![Vec::<String>::new()]);

        // Tier 1 alone: the tier's own two options, both legal.
        let one = sets(json!({ "1": ["torid_evo1_incarnon_form"] }));
        assert_eq!(one, vec![vec!["torid_evo1_incarnon_form".to_string()]]);

        // Both marked: the product stands, because now every set is reachable.
        let two = sets(json!({
            "1": ["torid_evo1_incarnon_form"],
            "2": ["torid_final_fusillade", "torid_survivors_edge"],
        }));
        assert_eq!(two.len(), 2, "{two:?}");
        assert!(two.iter().all(|s| s.contains(&"torid_evo1_incarnon_form".to_string())));

        // A gap ABOVE a legal prefix cuts only what is above it.
        let gapped = sets(json!({
            "1": ["torid_evo1_incarnon_form"],
            "3": ["torid_extended_volley"],
        }));
        assert_eq!(gapped, vec![vec!["torid_evo1_incarnon_form".to_string()]]);
    }
}

/// INSTALLING THE INCARNON FORM TAKES THE CANNONADE OFF THE WEAPON.
///
/// "Weapons with an Incarnon mode must have Semi-Auto trigger type for both
/// firing modes in order to equip this mod" (wiki, Semi-Pistol_Cannonade), and
/// Dual Toxocyst transforms into a full-auto one. So the pool is a question
/// about the BUILD: with tier 1 unpicked the weapon is still pure semi-auto and
/// the mod fits; with it picked the weapon has two firing modes and it does not
/// (user, 2026-08-04).
///
/// Both modules are pinned here, and that is the point — the optimizer obeys
/// the simulator's rule by CALLING it (`pool_for_build`), so the two cannot
/// answer differently about the same build.
#[cfg(test)]
mod equip_rule_tests {
    use super::*;

    const EVO1: &str = "dual_toxocyst_evo1_incarnon_form";
    const CANNON: &str = "semi_pistol_cannonade";

    fn sim(form: &str, evos: Value) -> Value {
        simulate_json(&json!({
            "weapon": "dual_toxocyst", "form": form, "mods": [CANNON], "arcane": "none",
            "evolutions": evos,
            "enemy": "thrax_centurion", "duration": 10.0, "runs": 2,
            "headshot_pct": 100.0, "seed": 7,
        }))
    }

    #[test]
    fn the_simulator_refuses_a_cannonade_beside_an_unlocked_incarnon_form() {
        // Nothing installed, base form: an ordinary build.
        let ok = sim("base", json!([]));
        assert_eq!(ok["ok"], json!(true), "{ok}");

        // Tier 1 installed: the weapon gained a full-auto firing mode.
        let bad = sim("base", json!([EVO1]));
        assert_eq!(bad["ok"], json!(false), "{bad}");
        let msg = bad["error"].as_str().unwrap_or_default();
        assert!(msg.contains("firing mode"), "the error says WHY: {msg}");

        // ...and ASKING FOR THE FORM is installing it (`parse_fight` implies the
        // unlock), so the cycle refuses it with no evolution named at all. This
        // is the case the page starts in on this weapon, and the alternative —
        // scoring the mod while firing a form it cannot be worn beside — is a
        // number nobody can reproduce.
        for form in ["incarnon", "incarnon_cycle"] {
            assert_eq!(sim(form, json!([]))["ok"], json!(false), "form {form}");
        }
    }

    /// Evolutions are a search DIMENSION, so the scope holds sets that can wear
    /// the mod and sets that cannot. Narrowing the pool to their intersection
    /// would throw away the builds the mod is FOR; leaving it alone would crown
    /// one the game refuses. It is decided per candidate instead.
    #[test]
    fn the_optimizer_forbids_the_pair_and_not_the_mod() {
        let plan = |evolutions: Value| {
            parse_optimize(&json!({
                "weapon": "dual_toxocyst",
                "build_size": 1,
                "mods": { CANNON: "search", "hornet_strike": "search" },
                "evolutions": evolutions,
            }))
            .expect("a plan")
        };
        let forbids = |p: &OptimizePlan, m: &str| -> Vec<bool> {
            let i = p.pool.iter().position(|x| x.id == m).expect("in scope");
            p.variant_forbids.iter().map(|f| f[i]).collect()
        };

        // Tier 1 unmarked: one variant, nothing installed, both mods legal.
        let bare = plan(json!({}));
        assert_eq!(bare.evo_sets.len(), 1);
        assert_eq!(forbids(&bare, CANNON), vec![false]);

        // Tier 1 marked: every set installs the form, so the Cannonade is out of
        // all of them — and `hornet_strike` is out of none, because this rule
        // excludes one mod and does not narrow the pool.
        let inc = plan(json!({ "1": [EVO1] }));
        assert!(inc.evo_sets.iter().all(|s| s.iter().any(|e| e == EVO1)));
        assert!(forbids(&inc, CANNON).iter().all(|&f| f), "the pair is illegal");
        assert!(forbids(&inc, "hornet_strike").iter().all(|&f| !f), "the pool is not");
    }

    /// A mod the scope REQUIRES and no variant can equip is a contradiction: the
    /// search cannot answer it by choosing differently, so it is refused up
    /// front rather than answered with an empty leaderboard.
    #[test]
    fn a_required_mod_no_variant_can_wear_is_refused() {
        let r = parse_optimize(&json!({
            "weapon": "dual_toxocyst",
            "build_size": 2,
            "mods": { CANNON: "fixed", "hornet_strike": "search" },
            "evolutions": { "1": [EVO1] },
        }));
        let e = match r {
            Err(e) => e,
            Ok(_) => panic!("a required mod no variant can wear is a contradiction"),
        };
        let msg = e["error"].as_str().unwrap_or_default();
        assert!(msg.contains("firing mode"), "{msg}");
    }

    /// The client is told the CONSEQUENCE, not the rule — the last time it
    /// re-derived a pool rule in JS the copy went stale within the week.
    #[test]
    fn meta_states_what_each_evolution_costs() {
        let meta = meta_json();
        let w = meta["weapons"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["id"] == json!("dual_toxocyst"))
            .expect("dual toxocyst");
        assert_eq!(w["evo_forbids"][EVO1], json!([CANNON]));
        // A stat evolution costs nothing, and says so by being absent.
        assert!(w["evo_forbids"]["dual_toxocyst_carnage_reign"].is_null());
        // The form the cost belongs to is flagged, so the Form control can grey
        // it out instead of letting the run be refused after the fact.
        let forms = w["forms"].as_array().unwrap();
        assert_eq!(forms.iter().filter(|f| f["gauge_switched"] == json!(true)).count(), 1);
    }

    /// A PASSIVE THAT BELONGS TO A FORM still reaches the roster row.
    ///
    /// The Phenmor's Incarnon fire rate spools down to 60% and the sentence is
    /// declared on `phenmor_incarnon`, which is not a roster row — so reading
    /// the base entry's passives alone published nothing about the one thing
    /// that makes the printed 13.33 rounds/s wrong.
    #[test]
    fn a_forms_passive_reaches_the_weapon_it_belongs_to() {
        let meta = meta_json();
        let w = meta["weapons"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["id"] == json!("phenmor"))
            .expect("phenmor");
        let lines: Vec<&str> =
            w["passives"].as_array().unwrap().iter().filter_map(|x| x.as_str()).collect();
        assert!(
            lines.iter().any(|l| l.contains("FALLS while the trigger is held")),
            "{lines:?}"
        );
        // …and each line ONCE: the union walks every form of the group.
        let mut sorted = lines.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), lines.len(), "{lines:?}");
    }
}

/// A weak-point bonus is conditional on WHERE THE BULLET LANDS, and no policy
/// can turn a body shot into a head shot. The panel used to fold Acuity's crit
/// half into the flat Crit Chance row (AssumedMax) while leaving its damage
/// half conditional — one mod, two treatments — so the Burston Incarnon read
/// 126% crit chance on every shot AND handed the same 126% to its explosion,
/// which has no hit location at all.
#[cfg(test)]
mod weakpoint_panel_tests {
    use super::*;

    fn incarnon_panel(mods: Value) -> Value {
        panel_json(&json!({
            "weapon": "burston_prime",
            "mods": mods,
            "evolutions": ["burston_prime_evo1_incarnon_form",
                           "burston_prime_forceful_finality"],
        }))
    }

    fn form<'a>(p: &'a Value, label: &str) -> &'a Value {
        p["forms"]
            .as_array()
            .expect("forms")
            .iter()
            .find(|f| f["label"] == json!(label))
            .unwrap_or_else(|| panic!("no {label}"))
    }

    fn part<'a>(f: &'a Value, id: &str) -> &'a Value {
        f["parts"]
            .as_array()
            .expect("parts")
            .iter()
            .find(|p| p["id"] == json!(id))
            .unwrap_or_else(|| panic!("no {id} part"))
    }

    fn row<'a>(p: &'a Value, key: &str) -> Option<&'a Value> {
        p["stats"].as_array()?.iter().find(|r| r["key"] == json!(key))
    }

    #[test]
    fn acuity_does_not_inflate_the_unconditional_crit_chance() {
        let inc = incarnon_panel(json!(["primary_acuity"]));
        let f = form(&inc, "Incarnon Form");
        let direct = part(f, "direct");
        assert_eq!(
            row(direct, "crit_chance").expect("crit row")["final"],
            json!("28.0%"),
            "the plain crit chance is the weapon's, not the weak-point one"
        );
        let wp = row(direct, "weakpoint_cc").expect("a weak-point crit row exists");
        assert_eq!(wp["final"], json!("126.0% on a weak point"));
        assert!(row(direct, "weakpoint_damage").is_some(), "and the damage half is stated");
    }

    #[test]
    fn the_explosion_gets_neither_half() {
        let inc = incarnon_panel(json!(["primary_acuity"]));
        let radial = part(form(&inc, "Incarnon Form"), "radial");
        assert_eq!(
            row(radial, "crit_chance").expect("crit row")["final"],
            json!("28.0%"),
            "an explosion has no hit location, so a weak-point bonus cannot reach it"
        );
        assert!(row(radial, "weakpoint_cc").is_none());
        assert!(row(radial, "weakpoint_damage").is_none());
    }

    /// Equipping it must still CHANGE something, or the fix would have been
    /// "delete the mod's effect" and the test above would pass anyway.
    /// PISTOL ACUITY IS THE SAME EFFECT, so it must not need its own fix —
    /// the bucket is chosen by `ModEffect`, never per mod. Asserted on a
    /// secondary because a rifle-only test would pass just as happily if the
    /// arm were duplicated per mod class.
    #[test]
    fn the_pistol_twin_behaves_identically() {
        let p = panel_json(&json!({
            "weapon": "laetum",
            "mods": ["pistol_acuity"],
            "evolutions": ["laetum_evo1_incarnon_form"],
        }));
        let f = form(&p, "Incarnon Form");
        let direct = part(f, "direct");
        assert_eq!(
            row(direct, "crit_chance").expect("crit row")["final"],
            json!("22.0%"),
            "the plain crit chance stays the weapon's"
        );
        assert_eq!(
            row(direct, "weakpoint_cc").expect("weak-point crit row")["final"],
            json!("99.0% on a weak point")
        );
        // 1.5 x the listed +350%, printed to two decimals so it matches the
        // wiki's own worked example (3 + 5.25 = 8.25x).
        assert_eq!(
            row(direct, "weakpoint_damage").expect("weak-point damage row")["final"],
            json!("+5.25 to the weak-point multiplier")
        );
        // And the Laetum's explosion is the ORDINARY case: no weak-point
        // bonus, and no CO either (it declares neither).
        let radial = part(f, "radial");
        assert_eq!(row(radial, "crit_chance").expect("crit row")["final"], json!("22.0%"));
        assert!(row(radial, "weakpoint_cc").is_none());
    }

    #[test]
    fn without_the_mod_there_are_no_weak_point_rows() {
        let bare = incarnon_panel(json!([]));
        let direct = part(form(&bare, "Incarnon Form"), "direct");
        assert!(row(direct, "weakpoint_cc").is_none());
        assert!(row(direct, "weakpoint_damage").is_none());
    }
}

/// A CANDIDATE'S LOCK IS NOT A FACT ABOUT THE OTHER CANDIDATES.
///
/// `fetchAllBuffs` asks "what buffs could this weapon ever produce" by marking
/// the whole pool `search`. Under the old rule the enumeration then applied
/// every marked mod's `disables:` at once — a build of eighty mods, which
/// nobody assembles — so Primary Acuity's `disables: [multishot]` deleted
/// Galvanized Chamber's on-kill multishot from the list of every rifle whose
/// pool contains Acuity. The simulator showed that buff and the optimizer's
/// read-only copy of the same fight did not; `check_preset_independence` had
/// been failing on it.
///
/// Only a REQUIRED mod is in every build, so only a required mod may lock.
#[cfg(test)]
mod scope_lock_tests {
    use super::*;

    fn buff_ids(mods: Value) -> Vec<String> {
        let out = opt_buffs_json(&json!({ "weapon": "torid", "mods": mods }));
        out["buffs"]
            .as_array()
            .expect("buffs")
            .iter()
            .map(|b| b["id"].as_str().unwrap_or("").to_string())
            .collect()
    }

    #[test]
    fn a_searched_mod_does_not_lock_another_mods_buff() {
        let alone = buff_ids(json!({ "galvanized_chamber": "search" }));
        assert!(alone.iter().any(|i| i == "on_kill_multishot"), "{alone:?}");
        // Acuity disables multishot, but as a CANDIDATE it is only in the
        // builds that take it — and those are not the builds this card is for.
        let with = buff_ids(json!({ "galvanized_chamber": "search", "primary_acuity": "search" }));
        assert!(
            with.iter().any(|i| i == "on_kill_multishot"),
            "a candidate's lock deleted another candidate's buff: {with:?}"
        );
    }

    #[test]
    fn a_required_mod_still_locks() {
        // Required means every searched build carries it, so the buff genuinely
        // cannot arm and the card would be a lie.
        let with = buff_ids(json!({ "galvanized_chamber": "search", "primary_acuity": "fixed" }));
        assert!(
            !with.iter().any(|i| i == "on_kill_multishot"),
            "a required lock must still suppress: {with:?}"
        );
    }
}

/// EVERY BUFF CARD MUST HAVE A SIM ARM, AND EVERY SIM BUFF MUST HAVE A CARD.
///
/// `enumerate_buffs` (what is drawn) and `DummyParams::buff_roster` (what the
/// fight runs) are two independent enumerations over the same data — one
/// matches `ModEffect` arms, the other reads resolved fields — so nothing but
/// a check makes them the same list. Both failure directions are silent on
/// screen: a card with no arm is a control that moves no number, and an armed
/// buff with no card cannot be configured or seen on the replay.
///
/// Written derived rather than listed (memory: derive triggers, don't list
/// them). It walks the whole roster and the whole mod pool, so a weapon, mod
/// or effect added later is covered without anyone remembering to come back.
#[cfg(test)]
mod card_and_sim_agree {
    use super::*;
    use wfsim_engine::dummy::DummyParams;
    use wfsim_engine::loadout::{resolve, StackPolicy, WeaponBase};

    /// Buffs the params do not own. `frenzy` is a weapon passive the api
    /// applies (`frenzy_apply`) rather than a field of the build; `arcane:*`
    /// ids come from the arcane and are checked by their own test below.
    fn theirs(id: &str) -> bool {
        id == "frenzy" || id.starts_with("arcane:")
    }

    fn roster_of(weapon: &str, refs: &[&ModDef]) -> Vec<String> {
        let base = WeaponBase::from_data(weapon, false, &[]);
        // EMERGENT, because that is the policy the fight runs under: a buff
        // rostered only at AssumedMax would be a card for a number the sim
        // never earns.
        let p = resolve(&base, refs, StackPolicy::Emergent);
        let params = DummyParams::from_panel(&p, &wfsim_engine::arena::Arena::training(30.0));
        params.buff_roster().into_iter().map(|(i, _)| i).collect()
    }

    #[test]
    fn every_mod_that_draws_a_card_arms_the_sim() {
        let tenno = wfsim_engine::tenno_data::default_tenno().clone();
        let none = wfsim_engine::arcanes_data::ArcaneFx::none();
        let mut pairs = 0;
        for w in wfsim_engine::weapons_data::roster() {
            let info = weapon(&w.id);
            // A sentinel resolves BaseOnly: no conditional ever fires, so
            // `enumerate_buffs` returns nothing by design.
            if info.sentinel {
                continue;
            }
            for m in wfsim_engine::mods_data::pool_for_build(&w.id, &[]) {
                let refs = vec![&m];
                let cards: Vec<String> = enumerate_buffs(&refs, &refs, &none, info, &tenno)
                    .into_iter()
                    .map(|b| b.id)
                    .collect();
                let roster = roster_of(&w.id, &refs);
                pairs += 1;
                for c in cards.iter().filter(|c| !theirs(c)) {
                    assert!(
                        roster.contains(c),
                        "{}+{} draws a card `{c}` the sim never rosters: a control that moves no number",
                        w.id, m.id
                    );
                }
                for r in roster.iter().filter(|r| !theirs(r)) {
                    assert!(
                        cards.contains(r),
                        "{}+{} arms `{r}` in the sim with no card: unconfigurable, and absent from the replay",
                        w.id, m.id
                    );
                }
            }
        }
        assert!(pairs > 500, "the walk collapsed: only {pairs} weapon-mod pairs");
    }

    /// The same rule for ARCANES, whose cards come from a third enumeration.
    #[test]
    fn every_arcane_that_draws_a_card_arms_the_sim() {
        let tenno = wfsim_engine::tenno_data::default_tenno().clone();
        let mut seen = 0;
        for w in wfsim_engine::weapons_data::roster() {
            let info = weapon(&w.id);
            if info.sentinel {
                continue;
            }
            let base = WeaponBase::from_data(&info.id, true, &[]);
            for a in info
                .arcane_pools
                .iter()
                .flat_map(|p| wfsim_engine::arcanes_data::pool_for_weapon(&info.id, p))
            {
                let fx = a.fx(a.max_rank, StackPolicy::Emergent, base.traits, &tenno);
                let cards: Vec<String> = enumerate_buffs(&[], &[], &fx, info, &tenno)
                    .into_iter()
                    .map(|b| b.id)
                    .filter(|id| id.starts_with("arcane:"))
                    .collect();
                let mut params =
                    DummyParams::from_panel(
                        &resolve(&base, &[], StackPolicy::Emergent),
                        &wfsim_engine::arena::Arena::training(30.0),
                    );
                params.arcane = fx.clone();
                let roster: Vec<String> = params
                    .buff_roster()
                    .into_iter()
                    .map(|(i, _)| i)
                    .filter(|id| id.starts_with("arcane:"))
                    .collect();
                seen += 1;
                for c in &cards {
                    assert!(
                        roster.contains(c),
                        "{} on {} draws `{c}` with no sim arm",
                        a.id, w.id
                    );
                }
                for r in &roster {
                    assert!(
                        cards.contains(r),
                        "{} on {} arms `{r}` with no card",
                        a.id, w.id
                    );
                }
            }
        }
        assert!(seen > 50, "the walk collapsed: only {seen} weapon-arcane pairs");
    }
}

/// TWO WEAPONS MUST NOT WEAR ONE PICTURE.
///
/// `data/assets.yaml` is filled from WFCD's `imageName`, and for some weapons
/// that field is a SIBLING'S file: the export gives MK1-Furis `Furis.png` and
/// Ocucor `CrpSentExperimentPistol.png` (which the CDN does not serve at all).
/// Both are hand-overridden to `wiki:` entries, and both were silently
/// re-derived — wrongly — the one time `scripts/gen_assets.py --write` ran with
/// them absent. Nothing downstream notices: the file exists, the fetcher caches
/// it, the build's missing-art guard passes, and the page shows a Furis where
/// an MK1-Furis should be.
///
/// The one legitimate collision is two FORMS of one weapon — an Incarnon form
/// shows its base weapon's image on purpose ("not the Genesis adapter icon"),
/// and an uncharged bow is the same bow. That is what a TRANSFORM GROUP already
/// means, so the exemption is read off the weapon data rather than written as a
/// list of pairs or guessed from the id's suffix.
#[cfg(test)]
mod one_picture_one_weapon {
    use super::*;

    /// The weapon an id draws its art from — its transform group, so every
    /// form of one weapon is one subject.
    fn subject(id: &str) -> &str {
        wfsim_engine::weapons_data::spec(id).map_or(id, |s| s.group())
    }

    #[test]
    fn no_two_weapons_share_an_image() {
        let mut by_image: std::collections::HashMap<&str, Vec<&str>> = Default::default();
        for (id, image) in &assets().weapons {
            by_image.entry(image.as_str()).or_default().push(id.as_str());
        }
        for (image, mut ids) in by_image {
            ids.sort_unstable();
            let subjects: std::collections::BTreeSet<&str> =
                ids.iter().map(|i| subject(i)).collect();
            assert!(
                subjects.len() <= 1,
                "`{image}` is worn by {ids:?}, which are different weapons — one of \
                 them has a sibling's picture. WFCD's `imageName` is wrong for these; \
                 set the right file by hand (a `wiki:` prefix if the CDN lacks it)."
            );
        }
    }

    /// ...and every weapon in the roster HAS one. The build fails on a missing
    /// file; this fails on a missing ENTRY, which is the earlier and clearer
    /// error.
    #[test]
    fn every_weapon_has_an_image() {
        for w in wfsim_engine::weapons_data::roster() {
            assert!(
                assets().weapons.contains_key(&w.id),
                "{} has no entry in data/assets.yaml",
                w.id
            );
        }
    }
}
