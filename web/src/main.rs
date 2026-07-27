//! wfsim-web: a tiny, dependency-light web UI for the engine.
//!
//! A std-only HTTP server (no web framework) that serves a static frontend and
//! two JSON endpoints:
//!   GET  /api/meta      — the mod pool, enemy list, arcane/evo2/form options.
//!   POST /api/simulate  — resolve a build, run the Monte Carlo, return results.
//!
//! The compute is the SAME engine the CLI and optimizer use — this is just a
//! different front door. Static assets and the enemy library are embedded via
//! `include_str!`, so the binary is self-contained (no cwd assumptions). The
//! frontend talks to the server over `fetch`; porting to a pure-WASM static
//! site later is a matter of swapping those two fetch calls for WASM calls.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

use serde_json::{json, Value};
use wfsim_engine::dummy::{monte_carlo, BodyPart, DummyParams, LockMode, TargetMode};
use wfsim_engine::enemy_data::EnemySpec;
use wfsim_engine::loadout::{
    pct as fpct, resolve, DtEvo2, ModDef, ModEffect, ResolvedPanel, StackPolicy, WeaponBase,
};
use wfsim_engine::mods::{plan_forma, PlannedMod, Polarity};
use wfsim_engine::mods_data::pistol_pool as pool; // FULL pool incl. exilus (the optimizer's pool() excludes exilus)
use wfsim_optimizer::dual_toxocyst_innate_slots;

// ---- Embedded static assets (self-contained binary) --------------------

const INDEX_HTML: &str = include_str!("static/index.html");
const APP_JS: &str = include_str!("static/app.js");
const STYLE_CSS: &str = include_str!("static/style.css");

// ---- Embedded enemy library --------------------------------------------
// Loaded at request time from these YAMLs (single source of truth: the same
// data/ files the CLI and optimizer read).
const ENEMY_YAMLS: &[&str] = &[
    include_str!("../../data/enemies/thrax_centurion.yaml"),
    include_str!("../../data/enemies/acolyte.yaml"),
    include_str!("../../data/enemies/custom/fortress.yaml"),
    include_str!("../../data/enemies/custom/glass_wall.yaml"),
];

fn enemies() -> Vec<EnemySpec> {
    ENEMY_YAMLS
        .iter()
        .map(|y| EnemySpec::from_yaml_str(y).expect("embedded enemy yaml parses"))
        .collect()
}

// ---- Embedded image asset map (data/assets.yaml) -----------------------
// id -> WFCD imageName; the frontend builds https://cdn.warframestat.us/img/<name>.
const ASSETS_YAML: &str = include_str!("../../data/assets.yaml");

#[derive(serde::Deserialize, Default)]
struct Assets {
    #[serde(default)]
    weapons: std::collections::HashMap<String, String>,
    #[serde(default)]
    mods: std::collections::HashMap<String, String>,
    #[serde(default)]
    arcanes: std::collections::HashMap<String, String>,
}

fn assets() -> &'static Assets {
    use std::sync::OnceLock;
    static A: OnceLock<Assets> = OnceLock::new();
    A.get_or_init(|| serde_norway::from_str(ASSETS_YAML).unwrap_or_default())
}

// ---- weapon registry ---------------------------------------------------
// The UI is weapon-aware. Each weapon declares its mod class (which mod pool
// the picker shows), whether it takes an arcane / Evolution II, its available
// forms, and whether it is a sentinel (BaseOnly resolution — Galvanized
// conditionals never fire).

struct WeaponInfo {
    id: &'static str,
    name: &'static str,
    // The MOD-ELIGIBILITY group, not a cosmetic label. "pistol" = the Pistol
    // Mods pool, which (wiki Pistol_Mods) equips on secondary Pistols, Dual
    // Pistols, Shotgun Sidearms, Crossbows, and Tomes. This is the ACTUAL way
    // mods take effect, so the eligibility group is what drives the pool.
    mod_class: &'static str, // "pistol" | "rifle"
    // Precise weapon type within that group (Dual Toxocyst = Dual Pistols).
    // Kept explicit because the subtype IS the real mod-eligibility path.
    subtype: &'static str,
    sentinel: bool,
    forms: &'static [(&'static str, &'static str)],
    uses_arcane: bool,
    uses_evo2: bool,
}

const WEAPONS: &[WeaponInfo] = &[
    WeaponInfo {
        id: "dual_toxocyst",
        name: "Dual Toxocyst",
        mod_class: "pistol",
        subtype: "Dual Pistols",
        sentinel: false,
        forms: &[
            ("incarnon_cycle", "Incarnon cycle (real two-form loop)"),
            ("incarnon", "Incarnon form only"),
            ("base", "Base form only"),
        ],
        uses_arcane: true,
        uses_evo2: true,
    },
    WeaponInfo {
        id: "verglas_prime",
        name: "Verglas Prime (sentinel)",
        mod_class: "rifle",
        subtype: "Sentinel Weapon",
        sentinel: true,
        forms: &[("primary", "Standard")],
        uses_arcane: false,
        uses_evo2: false,
    },
];

fn weapon(id: &str) -> &'static WeaponInfo {
    WEAPONS.iter().find(|w| w.id == id).unwrap_or(&WEAPONS[0])
}

fn mod_pool_for(class: &str) -> Vec<ModDef> {
    if class == "rifle" {
        rifle_pool()
    } else {
        pool()
    }
}

fn innate_slots_for(id: &str) -> Vec<Option<Polarity>> {
    match id {
        // Robotic weapon: 8 mod slots, one innate Naramon, no exilus.
        "verglas_prime" => {
            let mut v = vec![None; 8];
            v[0] = Some(Polarity::Naramon);
            v
        }
        _ => dual_toxocyst_innate_slots().to_vec(),
    }
}

/// Rifle mod pool (standard primary rifle mods; Verglas Prime accepts these).
/// Values transcribed at max rank from the wiki (2026-07-25). Galvanized mods
/// carry BOTH their unconditional base part and their conditional part; under
/// `StackPolicy::BaseOnly` (sentinels) only the base applies. Galvanized/Argon
/// Scope are entirely headshot-gated, so on a sentinel they contribute nothing.
fn rifle_pool() -> Vec<ModDef> {
    use wfsim_engine::damage::DamageType as D;
    use wfsim_engine::loadout::Rarity as R;
    use ModEffect::*;
    // (id, max-rank drain, max_rank, polarity, rarity, family, effects)
    let md = |id, base_drain, max_rank, polarity, rarity, family, effects| ModDef {
        id,
        base_drain,
        max_rank,
        polarity,
        rarity,
        exilus: false, // no rifle exilus mods authored yet
        family,
        requires: None,
        disables: Vec::new(),
        effects,
    };
    vec![
        // Damage
        md("serration", 14, 10, Polarity::Madurai, R::Uncommon, None, vec![BaseDamage(1.65)]),
        md("heavy_caliber", 16, 10, Polarity::Madurai, R::Rare, None, vec![BaseDamage(1.65)]), // accuracy downside = no-op
        // Multishot (Split Chamber ↔ Galvanized Chamber share the "chamber" family)
        md("split_chamber", 15, 5, Polarity::Madurai, R::Rare, Some("chamber"), vec![Multishot(0.90)]),
        md("galvanized_chamber", 16, 10, Polarity::Madurai, R::Rare, Some("chamber"), vec![
            Multishot(0.80),
            OnKillMultishot { per_stack: 0.30, max_stacks: 5, duration: 20.0 },
        ]),
        md("vigilante_armaments", 9, 5, Polarity::Naramon, R::Common, None, vec![Multishot(0.60)]),
        // Crit
        md("point_strike", 9, 5, Polarity::Madurai, R::Uncommon, None, vec![CritChance(1.50)]),
        md("vital_sense", 9, 5, Polarity::Madurai, R::Rare, None, vec![CritDamage(1.20)]),
        md("critical_delay", 9, 5, Polarity::Naramon, R::Rare, None, vec![CritChance(2.00), FireRate(-0.20)]), // corrupted
        // Status
        md("galvanized_aptitude", 12, 10, Polarity::Vazarin, R::Rare, None, vec![
            StatusChance(0.80),
            ConditionOverload { per_stack: 0.40, max_stacks: 2, duration: 20.0 },
        ]),
        // Single elements (+90%)
        md("cryo_rounds", 11, 5, Polarity::Vazarin, R::Uncommon, None, vec![Element(D::Cold, 0.90)]),
        md("hellfire", 11, 5, Polarity::Naramon, R::Uncommon, None, vec![Element(D::Heat, 0.90)]),
        md("stormbringer", 11, 5, Polarity::Naramon, R::Uncommon, None, vec![Element(D::Electricity, 0.90)]),
        md("infected_clip", 11, 5, Polarity::Naramon, R::Uncommon, None, vec![Element(D::Toxin, 0.90)]),
        // Dual-stat elements (60/60)
        md("rime_rounds", 7, 3, Polarity::Madurai, R::Rare, None, vec![Element(D::Cold, 0.60), StatusChance(0.60)]),
        md("malignant_force", 7, 3, Polarity::Madurai, R::Rare, None, vec![Element(D::Toxin, 0.60), StatusChance(0.60)]),
        md("high_voltage", 7, 3, Polarity::Madurai, R::Rare, None, vec![Element(D::Electricity, 0.60), StatusChance(0.60)]),
        md("thermite_rounds", 7, 3, Polarity::Madurai, R::Rare, None, vec![Element(D::Heat, 0.60), StatusChance(0.60)]),
        // Fire rate
        md("vile_acceleration", 9, 5, Polarity::Naramon, R::Rare, None, vec![FireRate(0.90), BaseDamage(-0.15)]), // corrupted
        md("speed_trigger", 9, 5, Polarity::Madurai, R::Common, None, vec![FireRate(0.60)]),
        md("shred", 11, 5, Polarity::Madurai, R::Rare, None, vec![FireRate(0.30)]), // punch-through = no-op single-target
        // Headshot-gated crit (Galvanized ↔ Argon share the "scope" family; both
        // do nothing on a sentinel — recorded so the picker shows them honestly)
        md("galvanized_scope", 12, 10, Polarity::Madurai, R::Rare, Some("scope"), vec![
            OnHeadshotCritChance { bonus: 1.20, duration: 12.0 },
            OnHeadshotKillCritChance { per_stack: 0.40, max_stacks: 5, duration: 12.0 },
        ]),
        md("argon_scope", 7, 5, Polarity::Madurai, R::Rare, Some("scope"), vec![
            OnHeadshotCritChance { bonus: 1.35, duration: 9.0 },
        ]),
    ]
}

// ---- main / server loop ------------------------------------------------

fn main() {
    let port: u16 = std::env::var("WFSIM_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8787);
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
        eprintln!("wfsim-web: cannot bind {addr}: {e}");
        std::process::exit(1);
    });
    println!("wfsim-web listening on http://{addr}  (Ctrl-C to stop)");

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                std::thread::spawn(move || {
                    if let Err(e) = handle(s) {
                        eprintln!("connection error: {e}");
                    }
                });
            }
            Err(e) => eprintln!("accept error: {e}"),
        }
    }
}

// ---- minimal HTTP ------------------------------------------------------

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn read_request(stream: &TcpStream) -> std::io::Result<Option<Request>> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(None); // connection closed
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break; // end of headers
        }
        if let Some(v) = line
            .to_ascii_lowercase()
            .strip_prefix("content-length:")
            .map(|s| s.trim().to_string())
        {
            content_length = v.parse().unwrap_or(0);
        }
    }

    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    Ok(Some(Request { method, path, body }))
}

fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &[u8]) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn respond_json(stream: &mut TcpStream, value: &Value) -> std::io::Result<()> {
    respond(
        stream,
        "200 OK",
        "application/json; charset=utf-8",
        value.to_string().as_bytes(),
    )
}

/// Serve a static asset (icon/image) with a long cache lifetime.
fn respond_asset(stream: &mut TcpStream, content_type: &str, body: &[u8]) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: public, max-age=604800\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// 302 redirect (used as the /img CDN fallback when the local cache misses).
fn respond_redirect(stream: &mut TcpStream, location: &str) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(header.as_bytes())?;
    stream.flush()
}

// Polarity/damage icons are vendored (tiny, stable) so they load instantly —
// no more slow wiki `Special:FilePath` 302 redirects.
const POL_MADURAI: &[u8] = include_bytes!("static/pol/Madurai_Pol.svg");
const POL_NARAMON: &[u8] = include_bytes!("static/pol/Naramon_Pol.svg");
const POL_VAZARIN: &[u8] = include_bytes!("static/pol/Vazarin_Pol.svg");
const POL_UMBRA: &[u8] = include_bytes!("static/pol/Umbra_Pol.svg");
const POL_ANY: &[u8] = include_bytes!("static/pol/Any_Pol.png");

/// Serve a vendored polarity icon by filename.
fn pol_icon(file: &str) -> Option<(&'static [u8], &'static str)> {
    Some(match file {
        "Madurai_Pol.svg" => (POL_MADURAI, "image/svg+xml"),
        "Naramon_Pol.svg" => (POL_NARAMON, "image/svg+xml"),
        "Vazarin_Pol.svg" => (POL_VAZARIN, "image/svg+xml"),
        "Umbra_Pol.svg" => (POL_UMBRA, "image/svg+xml"),
        "Any_Pol.png" => (POL_ANY, "image/png"),
        _ => return None,
    })
}

/// Weapon/mod/arcane art: served from a local on-disk cache (web/cache/img/,
/// gitignored, pre-warmed by scripts/fetch_images.py) so it loads locally and
/// works offline. On a cache miss, 302-redirect to the WFCD CDN — so it always
/// works, and DE art never has to be committed to the repo. `name` is a bare
/// filename (traversal-guarded).
fn img_response(stream: &mut TcpStream, name: &str) -> std::io::Result<()> {
    let safe = !name.is_empty()
        && name.len() < 128
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if safe {
        let path = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/cache/img"))
            .join(name);
        if let Ok(bytes) = std::fs::read(&path) {
            let ct = if name.ends_with(".png") { "image/png" } else { "image/jpeg" };
            return respond_asset(stream, ct, &bytes);
        }
    }
    respond_redirect(stream, &format!("https://cdn.warframestat.us/img/{name}"))
}

fn handle(mut stream: TcpStream) -> std::io::Result<()> {
    let Some(req) = read_request(&stream)? else {
        return Ok(());
    };
    // Strip any query string.
    let path = req.path.split('?').next().unwrap_or("/");

    match (req.method.as_str(), path) {
        ("GET", "/") => respond(&mut stream, "200 OK", "text/html; charset=utf-8", INDEX_HTML.as_bytes()),
        ("GET", "/app.js") => respond(
            &mut stream,
            "200 OK",
            "text/javascript; charset=utf-8",
            APP_JS.as_bytes(),
        ),
        ("GET", "/style.css") => respond(&mut stream, "200 OK", "text/css; charset=utf-8", STYLE_CSS.as_bytes()),
        ("GET", "/api/meta") => respond_json(&mut stream, &meta_json()),
        ("POST", "/api/simulate") => {
            let value = serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
            respond_json(&mut stream, &simulate_json(&value))
        }
        ("POST", "/api/panel") => {
            let value = serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
            respond_json(&mut stream, &panel_json(&value))
        }
        ("GET", p) if p.starts_with("/pol/") => match pol_icon(&p[5..]) {
            Some((bytes, ct)) => respond_asset(&mut stream, ct, bytes),
            None => respond(&mut stream, "404 Not Found", "text/plain; charset=utf-8", b"not found"),
        },
        ("GET", p) if p.starts_with("/img/") => img_response(&mut stream, &p[5..]),
        _ => respond(&mut stream, "404 Not Found", "text/plain; charset=utf-8", b"not found"),
    }
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
    } else if has(|e| matches!(e, ModEffect::StatusChance(..) | ModEffect::StatusDamage(..) | ModEffect::ConditionOverload { .. })) {
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

fn meta_json() -> Value {
    let weapons: Vec<Value> = WEAPONS
        .iter()
        .map(|w| {
            json!({
                "id": w.id,
                "name": w.name,
                "mod_class": w.mod_class,
                "subtype": w.subtype,
                "sentinel": w.sentinel,
                "uses_arcane": w.uses_arcane,
                "uses_evo2": w.uses_evo2,
                "arcane_slots": if w.id == "verglas_prime" { 0 } else { 1 },
                "image": assets().weapons.get(w.id),
                "innate_polarities": innate_slots_for(w.id).iter()
                    .map(|p| p.map(|x| format!("{x:?}")))
                    .collect::<Vec<_>>(),
                "forms": w.forms.iter().map(|(id, name)| json!({"id": id, "name": name})).collect::<Vec<_>>(),
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

    // Arcanes: the FULL data-driven pool (data/arcanes/secondary/*.yaml via
    // engine::arcanes_data). Per-rank effect lines come from the same
    // describe used by the model, so the picker states what the sim computes.
    let mut arcanes_json: Vec<Value> = vec![json!(
        {"id": "none", "name": "None", "image": null, "ranks": [], "max_rank": 0, "rarity": null}
    )];
    for a in wfsim_engine::arcanes_data::secondary_pool() {
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
        }));
    }

    // A pleasant default: the standing Thrax official champion (devlog 2026-07-25).
    let default_mods = [
        "primed_convulsion",
        "galvanized_diffusion",
        "primed_target_cracker",
        "lethal_torrent",
        "galvanized_shot",
        "magnetic_might",
        "anemic_agility",
        "galvanized_crosshairs",
    ];

    json!({
        "weapons": weapons,
        "mod_pools": {
            "pistol": mods_json(&pool()),
            "rifle": mods_json(&rifle_pool()),
        },
        "enemies": enemies,
        // Arcanes mirror the mod pool: per-rank effect lines (`ranks[r]`),
        // max_rank, rarity — so the web picker searches effects and the slot
        // steps ranks with the strength updating per rank. `arcane_rank` in
        // the sim request selects the modeled rank (default: max).
        "arcanes": arcanes_json,
        // The EVO II choice list comes from the evolution yamls
        // (data/perks/dt_*.yaml, tier 2) — names and ids are data.
        "evo2": wfsim_engine::evolutions_data::options("dual_toxocyst", 2)
            .iter()
            .map(|e| json!({"id": e.id, "name": e.name}))
            .collect::<Vec<_>>(),
        "defaults": {
            "weapon": "dual_toxocyst",
            "form": "incarnon_cycle",
            "evo2": "dt_fevered_frenzy",
            "arcane": "secondary_deadhead",
            "enemy": "thrax_centurion",
            "level": 9999,
            "steel_path": true,
            "headshot_pct": 100.0,
            "duration": 120.0,
            "runs": 300,
            "mods": default_mods,
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
    v.get(key).and_then(|x| x.as_u64()).map(|n| n as u32).unwrap_or(default)
}
fn get_bool(v: &Value, key: &str, default: bool) -> bool {
    v.get(key).and_then(|x| x.as_bool()).unwrap_or(default)
}

fn err_json(msg: impl Into<String>) -> Value {
    json!({ "ok": false, "error": msg.into() })
}

// ---- /api/panel --------------------------------------------------------
//
// The FINAL stats panel: every bucket merged across the build, each stated
// with its per-source breakdown ("who contributed what") — the panel is where
// the model explains itself. Static view: max-rank mods; conditionals at max
// stacks (AssumedMax), except sentinels where conditionals never fire.

fn panel_json(v: &Value) -> Value {
    let info = weapon(get_str(v, "weapon", "dual_toxocyst"));
    let policy = if info.sentinel { StackPolicy::BaseOnly } else { StackPolicy::AssumedMax };
    let form = get_str(v, "form", "incarnon");
    // Evolution II selector: data ids (legacy short names accepted).
    let evo2 = match get_str(v, "evo2", "dt_fevered_frenzy") {
        "carnage" | "dt_carnage_reign" => DtEvo2::CarnageReign,
        _ => DtEvo2::FeveredFrenzy,
    };

    let mod_ids: Vec<String> = v
        .get("mods")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|m| m.as_str().map(String::from)).collect())
        .unwrap_or_default();
    if mod_ids.len() > 9 {
        return err_json("at most 8 slots + 1 exilus");
    }
    let p = mod_pool_for(info.mod_class);
    let mut refs: Vec<&ModDef> = Vec::with_capacity(mod_ids.len());
    for id in &mod_ids {
        match p.iter().find(|m| m.id == id) {
            Some(m) => refs.push(m),
            None => return err_json(format!("unknown mod id: {id}")),
        }
    }

    let base = if info.id == "verglas_prime" {
        WeaponBase::verglas_prime()
    } else if form == "base" {
        WeaponBase::dual_toxocyst_base(true, evo2)
    } else {
        WeaponBase::dual_toxocyst_incarnon(true, evo2)
    };
    let panel = resolve(&base, &refs, policy);

    // ---- per-bucket source attribution (mirrors resolve()'s buckets) ----
    // key -> [(mod name, contribution fraction, note)]
    let mut src: Vec<(&'static str, String, f64, Option<String>)> = Vec::new();
    let mut conditionals: Vec<Value> = Vec::new(); // lines that never merge into a bucket
    for m in &refs {
        let name = prettify(m.id);
        for e in &m.effects {
            use ModEffect::*;
            let mut push = |key: &'static str, v: f64, note: Option<String>| {
                src.push((key, name.clone(), v, note));
            };
            match *e {
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
                OnKillMultishot { per_stack, max_stacks, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "sentinel weapons cannot proc on-kill stacks"})),
                    _ => push("multishot", per_stack * max_stacks as f64,
                        Some(format!("on kill, {max_stacks} stacks assumed"))),
                },
                ConditionOverload { per_stack, max_stacks, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "sentinel weapons cannot proc on-kill stacks"})),
                    _ => push("co", per_stack * max_stacks as f64,
                        Some(format!("on kill, {max_stacks} stacks assumed, per status type on target"))),
                },
                OnHeadshotCritChance { bonus, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "sentinel weapons cannot headshot"})),
                    _ => push("crit_chance", bonus, Some("on headshot, buff assumed up".into())),
                },
                OnHeadshotKillCritChance { per_stack, max_stacks, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "sentinel weapons cannot headshot"})),
                    _ => push("crit_chance", per_stack * max_stacks as f64,
                        Some(format!("on headshot kill, {max_stacks} stacks assumed"))),
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
                    _ => push("crit_damage", bonus, Some("on kill, buff assumed up".into())),
                },
                OnReloadFireRate { bonus, .. } => match policy {
                    StackPolicy::BaseOnly => conditionals.push(json!({
                        "mod": name, "desc": e.describe(), "active": false,
                        "why": "sentinel weapons cannot proc on-reload buffs"})),
                    _ => push("fire_rate", bonus, Some("on reload, buff assumed up".into())),
                },
                // Event mechanic — no static stat; the sim rolls it per hit.
                ProcConversion { .. } => conditionals.push(json!({
                    "mod": name, "desc": e.describe(), "active": true,
                    "why": "rolled per damage instance in the sim"})),
            }
        }
    }
    // Non-mod sources baked into the weapon config (EVO II at assumed max).
    if base.buff_multishot_bonus.abs() > 1e-12 {
        src.push(("multishot", "Fevered Frenzy (EVO II)".into(), base.buff_multishot_bonus,
            Some("on-hit stacks, 20 assumed".into())));
    }
    if base.innate_co_per_type > 1e-12 {
        src.push(("co", "Carnage Reign (EVO II)".into(), base.innate_co_per_type,
            Some("innate, per status type on target".into())));
    }

    let sources = |key: &str, tag: Option<&str>| -> Vec<Value> {
        src.iter()
            .filter(|(k, _, _, note)| {
                *k == key && tag.is_none_or(|t| note.as_deref() == Some(t))
            })
            .map(|(_, name, v, note)| {
                json!({ "mod": name, "value": fpct(*v),
                        "note": if tag.is_some() { Value::Null } else { json!(note) } })
            })
            .collect()
    };
    // ---- stat rows: base -> final, with the merged bonus and its sources ----
    let num = |x: f64| -> String {
        if x >= 100.0 { format!("{x:.0}") } else { format!("{x:.1}") }
    };
    let pc = |x: f64| format!("{:.1}%", x * 100.0);
    let mut stats = Vec::new();
    let mut row = |key: &'static str, label: &str, base_s: String, final_s: String, changed: bool| {
        let s = sources(key, None);
        if !s.is_empty() || changed {
            stats.push(json!({ "key": key, "label": label, "base": base_s, "final": final_s, "sources": s }));
        }
    };
    row("base_damage", "Base Damage", num(base.base_vector.total()), num(panel.modified_base),
        (panel.modified_base - base.base_vector.total()).abs() > 1e-9);
    row("multishot", "Multishot", format!("×{}", num(base.base_multishot)), format!("×{}", num(panel.multishot)),
        (panel.multishot - base.base_multishot).abs() > 1e-9);
    row("crit_chance", "Crit Chance", pc(base.base_crit_chance), pc(panel.crit_chance),
        (panel.crit_chance - base.base_crit_chance).abs() > 1e-9);
    row("crit_damage", "Crit Damage", format!("×{}", num(base.base_crit_damage)), format!("×{}", num(panel.crit_damage)),
        (panel.crit_damage - base.base_crit_damage).abs() > 1e-9);
    row("status_chance", "Status Chance", pc(base.base_status_chance), pc(panel.status_chance),
        (panel.status_chance - base.base_status_chance).abs() > 1e-9);
    row("status_damage", "Status Damage", "×1".into(), format!("×{}", num(panel.status_damage_mult)),
        (panel.status_damage_mult - 1.0).abs() > 1e-9);
    row("fire_rate", "Fire Rate", format!("{}/s", num(base.base_fire_rate)), format!("{}/s", num(panel.fire_rate)),
        (panel.fire_rate - base.base_fire_rate).abs() > 1e-9);
    row("reload", "Reload", format!("{}s", num(base.base_reload)), format!("{}s", num(panel.reload_seconds)),
        (panel.reload_seconds - base.base_reload).abs() > 1e-9);
    if panel.co_per_type > 0.0 {
        stats.push(json!({ "key": "co", "label": "Condition Overload",
            "base": "—", "final": format!("{} per status type on target", fpct(panel.co_per_type)),
            "sources": sources("co", None) }));
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
        let total: f64 = src.iter()
            .filter(|(k, _, _, n)| *k == "elements" && n.as_deref() == Some(t))
            .map(|(_, _, v, _)| v).sum();
        elem_rows.push(json!({ "key": "elements", "label": t, "base": "—",
            "final": format!("{} of modified base", fpct(total)),
            "sources": sources("elements", Some(t)) }));
    }

    // Indirect stats (recoil, accuracy, ammo…): not in theoretical DPS,
    // real in practice; base is unmodified (0%), final = Σ.
    let mut indirect_rows = Vec::new();
    for (stat, total) in &panel.indirect {
        indirect_rows.push(json!({ "key": "indirect", "label": stat.label(), "base": "—",
            "final": fpct(*total), "sources": sources("indirect", Some(stat.label())) }));
    }

    // The combined damage vector (post element-hierarchy).
    let dmg_total = panel.damage.total();
    let damage: Vec<Value> = panel.damage.iter_nonzero()
        .map(|(t, amt)| json!({ "type": format!("{t:?}"), "amount": num(amt),
            "share": format!("{:.0}%", amt / dmg_total * 100.0) }))
        .collect();

    json!({
        "ok": true,
        "weapon": info.name,
        "form": if info.id == "verglas_prime" { "standard" } else { form },
        "policy": if info.sentinel { "base only (sentinel)" } else { "conditionals at max stacks" },
        "stats": stats,
        "elements": elem_rows,
        "indirect": indirect_rows,
        "damage": damage,
        "damage_total": num(dmg_total),
        "conditionals": conditionals,
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

fn simulate_json(v: &Value) -> Value {
    // ---- parse inputs ----
    let info = weapon(get_str(v, "weapon", "dual_toxocyst"));
    let policy = if info.sentinel {
        StackPolicy::BaseOnly
    } else {
        StackPolicy::Emergent
    };
    let form = get_str(v, "form", "incarnon_cycle");
    // Evolution II selector: data ids (legacy short names accepted).
    let evo2 = match get_str(v, "evo2", "dt_fevered_frenzy") {
        "carnage" | "dt_carnage_reign" => DtEvo2::CarnageReign,
        _ => DtEvo2::FeveredFrenzy,
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
    let duration = get_f64(v, "duration", 120.0).clamp(1.0, 3600.0);
    let runs = get_u32(v, "runs", 300).clamp(1, 20_000);
    let seed = v.get("seed").and_then(|x| x.as_u64()).unwrap_or(0xC0FFEE);

    let mod_ids: Vec<String> = v
        .get("mods")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|m| m.as_str().map(String::from)).collect())
        .unwrap_or_default();

    if mod_ids.len() > 8 {
        return err_json("a Dual Toxocyst build has at most 8 mod slots");
    }

    // ---- resolve mods against the weapon's pool (honoring the given order) ----
    let p = mod_pool_for(info.mod_class);
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
    let (og, sh, hp, ar) = (target.overguard(), target.max_shield(), target.max_health(), target.armor());
    let body_parts = build_body_parts(spec, headshot_pct);

    // ---- forma legality (order-independent; needs only the mod multiset) ----
    let planned: Vec<PlannedMod> = refs
        .iter()
        .map(|m| PlannedMod { base_drain: m.base_drain, polarity: m.polarity })
        .collect();
    let forma = match plan_forma(60, &innate_slots_for(info.id), &planned) {
        Ok(fp) => json!({
            "legal": true,
            "used": fp.forma_used,
            "total_drain": fp.total_drain,
            "cap": 60,
        }),
        Err(e) => json!({ "legal": false, "error": e, "cap": 60 }),
    };

    // ---- resolve panel(s) and build sim params, per weapon ----
    let (report_panel, mut params): (ResolvedPanel, DummyParams) = if info.id == "verglas_prime" {
        // Sentinel weapon: single form, BaseOnly resolution, no arcane, no Frenzy.
        let base = WeaponBase::verglas_prime();
        let panel = resolve(&base, &refs, policy);
        let params = DummyParams::from_panel(&panel, target, body_parts, duration);
        (panel, params)
    } else {
        // Dual Toxocyst: two forms + the real Incarnon cycle.
        let incarnon_base = WeaponBase::dual_toxocyst_incarnon(true, evo2);
        let base_base = WeaponBase::dual_toxocyst_base(true, evo2);
        let incarnon_panel = resolve(&incarnon_base, &refs, policy);
        let base_panel = resolve(&base_base, &refs, policy);
        let report = if form == "base" { base_panel.clone() } else { incarnon_panel.clone() };
        let params = match form {
            "base" => {
                let mut d = DummyParams::from_panel(&base_panel, target, body_parts, duration);
                d.frenzy = true; // base-form Frenzy passive (×2.5 on true headshots)
                d
            }
            "incarnon" => {
                let mut d = DummyParams::from_panel(&incarnon_panel, target, body_parts, duration);
                d.frenzy = true; // Frenzy persists in the Incarnon form (user-confirmed)
                d
            }
            _ => DummyParams::incarnon_cycle_from_panels(
                &incarnon_panel,
                &base_panel,
                LockMode::Permanent,
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
        let ab = if info.id == "verglas_prime" {
            WeaponBase::verglas_prime()
        } else {
            WeaponBase::dual_toxocyst_incarnon(true, evo2)
        };
        def.fx(rank, policy, ab.base_crit_chance, ab.base_crit_damage, ab.traits)
    };
    let report_panel = &report_panel;

    // ---- run ----
    let s = monte_carlo(&params, runs, seed);

    let damage: Vec<Value> = report_panel
        .damage
        .iter_nonzero()
        .map(|(t, val)| json!({ "type": format!("{t:?}"), "value": val }))
        .collect();

    json!({
        "ok": true,
        "score": s.mean_kill_progress,
        "kills": s.mean_kills,
        "kills_std": s.std_kills,
        "kills_min": s.min_kills,
        "kills_max": s.max_kills,
        "dps": s.dps,
        "effective_dps": s.effective_dps,
        "shots": s.mean_shots,
        "pellets": s.mean_pellets,
        "crit_rate": s.mean_crit_rate,
        "big_crit_rate": s.mean_big_crit_rate,
        "headshot_rate": s.mean_headshot_rate,
        "procs": s.mean_procs,
        "dot": s.mean_dot_damage,
        "transforms": s.mean_transforms,
        "reloads": s.mean_reloads,
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
