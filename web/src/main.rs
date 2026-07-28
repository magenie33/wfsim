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
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::{json, Value};
use wfsim_engine::dummy::{monte_carlo, BodyPart, BuffLock, DummyParams, LockMode, LockedBuff, TargetMode};
use wfsim_engine::enemy_data::EnemySpec;
use wfsim_engine::loadout::{
    pct as fpct, resolve, DtEvo2, ModDef, ModEffect, ResolvedPanel, StackPolicy, WeaponBase,
};
use wfsim_engine::mods::{plan_forma, PlannedMod, Polarity};
use wfsim_engine::mods_data::pistol_pool as pool; // FULL pool incl. exilus (the optimizer's pool() excludes exilus)
use wfsim_optimizer::{
    dual_toxocyst_innate_slots, enumerate_candidates, run_funnel, schedule_to, Candidate,
    Constraints, FunnelState, Job, Scenario,
};

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
    // Parentheses admit the wiki's evolution-icon names ("…(xWhite).png").
    let safe = !name.is_empty()
        && name.len() < 128
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '(' | ')'));
    if safe {
        let path = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/cache/img"))
            .join(name);
        if let Ok(bytes) = std::fs::read(&path) {
            let ct = if name.ends_with(".png") { "image/png" } else { "image/jpeg" };
            return respond_asset(stream, ct, &bytes);
        }
    }
    // Cache miss: wiki-hosted art (evolution icons) redirects to the wiki
    // file path; everything else to the WFCD CDN.
    if name.contains('(') {
        return respond_redirect(
            stream,
            &format!("https://wiki.warframe.com/w/Special:FilePath/{name}"),
        );
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
        ("POST", "/api/optimize") => {
            let value = serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
            respond_json(&mut stream, &optimize_start(&value))
        }
        ("POST", "/api/optimize/status") => {
            let value = serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
            respond_json(&mut stream, &optimize_status(&value))
        }
        ("POST", "/api/optimize/cancel") => {
            let value = serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
            respond_json(&mut stream, &optimize_cancel(&value))
        }
        ("POST", "/api/opt-buffs") => {
            let value = serde_json::from_slice::<Value>(&req.body).unwrap_or(Value::Null);
            respond_json(&mut stream, &opt_buffs_json(&value))
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
        // Choosable evolution tiers from data/evolutions/*.yaml (tier 1 =
        // the Incarnon Form unlock — deselecting it means no transformation,
        // so the panel/sim fall back to the base form). Every tier also gets
        // an implicit EMPTY choice in the UI (nothing installed); `broken` =
        // wiki-flagged non-functional — the engine applies ZERO for those,
        // and the UI must say so in red. `desc` lines are the verbatim
        // effect text (like the mod/arcane cards).
        "evolutions": (1u32..=4)
            .map(|tier| json!({
                "tier": tier,
                "options": wfsim_engine::evolutions_data::options("dual_toxocyst", tier)
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
            .collect::<Vec<_>>(),
        "defaults": {
            "weapon": "dual_toxocyst",
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
            "duration": 120.0,
            "runs": 300,
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
    v.get(key).and_then(|x| x.as_u64()).map(|n| n as u32).unwrap_or(default)
}
fn get_bool(v: &Value, key: &str, default: bool) -> bool {
    v.get(key).and_then(|x| x.as_bool()).unwrap_or(default)
}

fn err_json(msg: impl Into<String>) -> Value {
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
    // Weapon passive: Frenzy (Dual Toxocyst). Default LOCKED — the sim's cycle
    // assumes it permanently; a single on/off "stack".
    if info.id == "dual_toxocyst" {
        push(BuffMeta { id: "frenzy".into(), name: "Frenzy".into(), max_stacks: 1,
            kind: "toggle", default_stacks: 1, default_locked: true, permanent: false });
    }
    // Mod-granted buffs.
    for m in refs {
        let nm = prettify(m.id);
        for e in &m.effects {
            use ModEffect::*;
            match *e {
                OnKillMultishot { max_stacks, .. } => push(BuffMeta { id: "on_kill_multishot".into(),
                    name: nm.clone(), max_stacks, kind: "stacking", default_stacks: max_stacks, default_locked: false, permanent: false }),
                ConditionOverload { max_stacks, .. } => push(BuffMeta { id: "condition_overload".into(),
                    name: nm.clone(), max_stacks, kind: "stacking", default_stacks: max_stacks, default_locked: false, permanent: false }),
                OnHeadshotCritChance { .. } => push(BuffMeta { id: "on_headshot_cc".into(),
                    name: nm.clone(), max_stacks: 1, kind: "toggle", default_stacks: 1, default_locked: false, permanent: false }),
                OnHeadshotKillCritChance { max_stacks, .. } => push(BuffMeta { id: "on_headshot_kill_cc".into(),
                    name: nm.clone(), max_stacks, kind: "stacking", default_stacks: max_stacks, default_locked: false, permanent: false }),
                OnKillCritDamage { .. } => push(BuffMeta { id: "on_kill_cd".into(),
                    name: nm.clone(), max_stacks: 1, kind: "toggle", default_stacks: 0, default_locked: false, permanent: false }),
                OnReloadFireRate { .. } => push(BuffMeta { id: "on_reload_fr".into(),
                    name: nm.clone(), max_stacks: 1, kind: "toggle", default_stacks: 0, default_locked: false, permanent: false }),
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
            let id = if multi { format!("arcane:{}:{}", arcane.id, i) } else { format!("arcane:{}", arcane.id) };
            let name = if multi { format!("{} ({})", aname, grant_label(b.grant)) } else { aname.clone() };
            let kind = if b.max_stacks > 1 { "stacking" } else { "toggle" };
            push(BuffMeta { id, name, max_stacks: b.max_stacks, kind,
                default_stacks: b.max_stacks, default_locked: false, permanent: false });
        }
    }
    out
}

/// Evolution-granted configurable buffs (Fevered Frenzy's permanent stacked
/// multishot): one card per evolution with an `ms_buff`. PERMANENT — no
/// in-sim trigger and no decay, so the stack count is a static choice (full
/// by default) and the lock is display-only.
fn evo_buffs(evo_ids: &[String]) -> Vec<BuffMeta> {
    evo_ids
        .iter()
        .filter_map(|id| wfsim_engine::evolutions_data::get(id))
        .filter_map(|def| {
            def.ms_buff().map(|(_, max_stacks)| BuffMeta {
                id: "evo_multishot".into(),
                name: def.name.clone(),
                max_stacks,
                kind: "stacking",
                default_stacks: max_stacks,
                default_locked: true,
                permanent: true,
            })
        })
        .collect()
}

fn buffs_json(list: &[BuffMeta]) -> Vec<Value> {
    list.iter().map(|b| json!({
        "id": b.id, "name": b.name, "max_stacks": b.max_stacks, "kind": b.kind,
        "default_stacks": b.default_stacks, "default_locked": b.default_locked,
        "permanent": b.permanent,
    })).collect()
}

// The build's resolved arcane fx (buff specs are policy-independent in shape);
// used for buff enumeration. `none` when the weapon can't equip arcanes.
fn arcane_fx_for(v: &Value, info: &WeaponInfo, base: &WeaponBase, policy: StackPolicy) -> wfsim_engine::arcanes_data::ArcaneFx {
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
            def.fx(rank, policy, base.base_crit_chance, base.base_crit_damage, base.traits)
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

fn panel_json(v: &Value) -> Value {
    let info = weapon(get_str(v, "weapon", "dual_toxocyst"));
    let policy = if info.sentinel { StackPolicy::BaseOnly } else { StackPolicy::AssumedMax };
    // (`form` in the request is ignored: every available form renders.)
    let evos = match chosen_evolutions(v) {
        Ok(e) => e,
        Err(e) => return err_json(e),
    };
    let evo_refs: Vec<&str> = evos.iter().map(String::as_str).collect();

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

    // ---- forms: EVERY available form renders side by side (no switching;
    // user decision). The Incarnon Form section exists only while its
    // tier-1 unlock is selected. `meta` states the trigger/shot mechanics
    // from the weapon data (data/weapons yamls).
    let mut forms_list: Vec<(&'static str, &'static str, WeaponBase)> = Vec::new();
    if info.id == "verglas_prime" {
        forms_list.push((
            "Standard",
            "Held trigger · hitscan · sentinel weapon",
            WeaponBase::verglas_prime(),
        ));
    } else {
        forms_list.push((
            "Base Form",
            "Semi-Auto · hitscan",
            WeaponBase::dual_toxocyst_base_evos(true, &evo_refs),
        ));
        if evo_refs.contains(&"dt_evo1_incarnon_form") {
            forms_list.push((
                "Incarnon Form",
                "Auto · hitscan · ricochet to 1 enemy within 5 m",
                WeaponBase::dual_toxocyst_incarnon_evos(true, &evo_refs),
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
    // Non-mod sources: the CHOSEN evolutions (data-driven). Flat base
    // damage and flat base crit chance alter the WEAPON BASE before mods —
    // the stat rows show the raw base and attribute the delta here; the
    // multishot stacks and CO rate join their buckets. Broken evolutions
    // report zero via the accessors, so nothing is listed for them.
    // (key, source name, PRE-FORMATTED value, note)
    let mut evo_src: Vec<(&'static str, String, String, Option<String>)> = Vec::new();
    let (mut evo_flat_bd, mut evo_flat_cc) = (0.0f64, 0.0f64);
    if info.id == "dual_toxocyst" {
        let tiername = |t: u32| ["", "EVO I", "EVO II", "EVO III", "EVO IV"][t.min(4) as usize];
        for def in evo_refs.iter().filter_map(|id| wfsim_engine::evolutions_data::get(id)) {
            let name = format!("{} ({})", def.name, tiername(def.tier));
            let v = def.flat_base_damage();
            if v > 0.0 {
                evo_flat_bd += v;
                evo_src.push(("base_damage", name.clone(), format!("+{v:.0} flat"),
                    Some("added to the weapon base pro-rata, before mods".into())));
            }
            let v = def.flat_base_crit_chance();
            if v > 0.0 {
                evo_flat_cc += v;
                evo_src.push(("crit_chance", name.clone(), format!("+{:.0}% base", v * 100.0),
                    Some("into the BASE crit chance — crit mods multiply it".into())));
            }
            let v = def.assumed_multishot();
            if v > 0.0 {
                evo_src.push(("multishot", name.clone(), fpct(v),
                    Some("on-ability-cast stacks, assumed full".into())));
            }
            let v = def.co_per_type();
            if v > 0.0 {
                evo_src.push(("co", name.clone(), fpct(v),
                    Some("innate, per status type on target".into())));
            }
        }
    }

    // One stats section per form; the closure names its params `base` /
    // `panel` so every row reads the ACTIVE form's numbers.
    let section = |label: &'static str, meta: &'static str, base: &WeaponBase, panel: &ResolvedPanel| -> Value {
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
        if x >= 100.0 { format!("{x:.0}") } else { format!("{x:.1}") }
    };
    let pc = |x: f64| format!("{:.1}%", x * 100.0);
    let mut stats = Vec::new();
    // Every base stat is ALWAYS listed (user: the panel must state the whole
    // base panel, not just what changed) — the UI drops the arrow when
    // base == final.
    let mut row = |key: &'static str, label: &str, base_s: String, final_s: String| {
        stats.push(json!({ "key": key, "label": label, "base": base_s, "final": final_s,
            "sources": sources(key, None) }));
    };
    // Base columns show the RAW weapon base (pre-evolution): the evolution
    // flat deltas are attributed as named source rows, not hidden in "base".
    let raw_bd = base.base_vector.total() - evo_flat_bd;
    let raw_cc = base.base_crit_chance - evo_flat_cc;
    row("base_damage", "Base Damage", num(raw_bd), num(panel.modified_base));
    row("multishot", "Multishot", format!("×{}", num(base.base_multishot)), format!("×{}", num(panel.multishot)));
    row("crit_chance", "Crit Chance", pc(raw_cc), pc(panel.crit_chance));
    row("crit_damage", "Crit Damage", format!("×{}", num(base.base_crit_damage)), format!("×{}", num(panel.crit_damage)));
    row("status_chance", "Status Chance", pc(base.base_status_chance), pc(panel.status_chance));
    // Identical formatting on both sides — the UI drops the arrow only
    // when the strings match ("×1" vs "×1.0" must not differ).
    row("status_damage", "Status Damage", format!("×{}", num(1.0)), format!("×{}", num(panel.status_damage_mult)));
    row("status_duration", "Status Duration", format!("×{}", num(1.0)), format!("×{}", num(panel.status_duration_mult)));
    row("fire_rate", "Fire Rate", format!("{}/s", num(base.base_fire_rate)), format!("{}/s", num(panel.fire_rate)));
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
        row("magazine", "Magazine", num(base.magazine_size), num(panel.magazine_size));
        row("reload", "Reload", format!("{}s", num(base.base_reload)), format!("{}s", num(panel.reload_seconds)));
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
            let fx = def.fx(rank, policy, base.base_crit_chance, base.base_crit_damage, base.traits);
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
        "label": label,
        "meta": meta,
        "stats": stats,
        "elements": elem_rows,
        "indirect": indirect_rows,
        "damage": damage,
        "damage_total": num(dmg_total),
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
    let evo2 = match get_str(v, "evo2", "dt_fevered_frenzy") {
        "carnage" | "dt_carnage_reign" => DtEvo2::CarnageReign,
        _ => DtEvo2::FeveredFrenzy,
    };
    Ok(vec![
        "dt_commodores_fortune".to_string(),
        "dt_evolved_autoloader".to_string(),
        evo2.evolution_id().to_string(),
    ])
}

fn simulate_json(v: &Value) -> Value {
    // ---- parse inputs ----
    let info = weapon(get_str(v, "weapon", "dual_toxocyst"));
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
    let form = get_str(v, "form", "incarnon_cycle");
    let evos = match chosen_evolutions(v) {
        Ok(e) => e,
        Err(e) => return err_json(e),
    };
    let evo_refs: Vec<&str> = evos.iter().map(String::as_str).collect();
    // No Incarnon Form unlock (tier 1) in an explicit selection = the weapon
    // cannot transform: honest fallback to the base form.
    let form = if v.get("evolutions").is_some()
        && info.id == "dual_toxocyst"
        && !evos.iter().any(|e| e == "dt_evo1_incarnon_form")
    {
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
        let incarnon_base = WeaponBase::dual_toxocyst_incarnon_evos(true, &evo_refs);
        let base_base = WeaponBase::dual_toxocyst_base_evos(true, &evo_refs);
        let incarnon_panel = resolve(&incarnon_base, &refs, policy);
        let base_panel = resolve(&base_base, &refs, policy);
        let report = if form == "base" { base_panel.clone() } else { incarnon_panel.clone() };
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
        let ab = if info.id == "verglas_prime" {
            WeaponBase::verglas_prime()
        } else {
            WeaponBase::dual_toxocyst_incarnon_evos(true, &evo_refs)
        };
        def.fx(rank, policy, ab.base_crit_chance, ab.base_crit_damage, ab.traits)
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

// ---- /api/optimize -----------------------------------------------------
//
// Scoped-subset search (devlog thread #4): the user fixes some mods/arcanes/
// evolutions, opens others to SEARCH, and gets the top-10 builds ranked by
// kills-in-duration. Reuses the optimizer lib's `enumerate_candidates` +
// `run_funnel`, with the same per-buff configured policy as the Sim panel.
// No cap (user directive): the funnel's cheap early rounds cull the space.
//
// The search runs as a BACKGROUND JOB: POST /api/optimize validates the scope
// synchronously (bad input still fails fast) and returns a `job_id`; a worker
// thread enumerates + runs the funnel, publishing live progress through the
// optimizer lib's `FunnelState`; the frontend polls POST /api/optimize/status
// and can POST /api/optimize/cancel. One job runs at a time — a single run
// already saturates every core via `evaluate_batch`, so a second concurrent
// run would only slow both down.

struct OptJob {
    id: u64,
    started: std::time::Instant,
    state: Arc<FunnelState>,
    /// "enumerating" → "running" → "done" | "cancelled" | "error".
    phase: Mutex<&'static str>,
    /// (candidates, jobs) — known once enumeration finishes.
    counts: Mutex<Option<(usize, usize)>>,
    /// The finished payload (exactly the old synchronous endpoint's JSON).
    /// A cancelled job still carries the last COMPLETED round's top-10 when
    /// at least one funnel round finished before the cancel.
    result: Mutex<Option<Value>>,
}

impl OptJob {
    fn active(&self) -> bool {
        matches!(*self.phase.lock().unwrap(), "enumerating" | "running")
    }
}

fn opt_jobs() -> &'static Mutex<Vec<Arc<OptJob>>> {
    static J: OnceLock<Mutex<Vec<Arc<OptJob>>>> = OnceLock::new();
    J.get_or_init(|| Mutex::new(Vec::new()))
}

/// Find a job by `id`, defaulting to the most recent one (lets the frontend
/// reattach after a page reload without persisting the id).
fn opt_job(v: &Value) -> Option<Arc<OptJob>> {
    let jobs = opt_jobs().lock().unwrap();
    match v.get("id").and_then(|x| x.as_u64()) {
        Some(id) => jobs.iter().find(|j| j.id == id).cloned(),
        None => jobs.last().cloned(),
    }
}

fn optimize_status(v: &Value) -> Value {
    let Some(j) = opt_job(v) else {
        return err_json("no such optimize job");
    };
    let st = &j.state;
    let notes: Vec<Value> = st
        .notes
        .lock()
        .unwrap()
        .iter()
        .map(|n| {
            json!({
                "round": n.round, "jobs": n.jobs, "runs": n.runs,
                "by_kills": n.by_kills, "kept": n.kept, "best": n.best, "ms": n.ms,
            })
        })
        .collect();
    let mut out = json!({
        "ok": true,
        "job_id": j.id,
        "phase": *j.phase.lock().unwrap(),
        "elapsed_s": j.started.elapsed().as_secs_f64(),
        "round": st.round.load(Ordering::Relaxed),
        "rounds": st.rounds.load(Ordering::Relaxed),
        "round_jobs": st.round_jobs.load(Ordering::Relaxed),
        "round_runs": st.round_runs.load(Ordering::Relaxed),
        "sims_done": st.sims_done.load(Ordering::Relaxed),
        "sims_planned": st.sims_planned.load(Ordering::Relaxed),
        "notes": notes,
    });
    if let Some((cands, jobs)) = *j.counts.lock().unwrap() {
        out["candidates"] = json!(cands);
        out["jobs"] = json!(jobs);
    }
    if let Some(r) = j.result.lock().unwrap().clone() {
        out["result"] = r;
    }
    out
}

fn optimize_cancel(v: &Value) -> Value {
    let Some(j) = opt_job(v) else {
        return err_json("no such optimize job");
    };
    // The flag is checked between jobs (not during enumeration, which is
    // bounded and quick relative to the funnel); the worker flips the phase.
    j.state.cancel.store(true, Ordering::Relaxed);
    json!({ "ok": true, "job_id": j.id })
}

// All buffs the scope could produce (union over every fixed/search mod + every
// searched arcane + the weapon passive) — the optimizer's buff panel enumerates
// over the WHOLE scope, not one build. `apply_buff_config` applies each per
// candidate where present.
fn opt_buffs_json(v: &Value) -> Value {
    let info = weapon(get_str(v, "weapon", "dual_toxocyst"));
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
    let full = mod_pool_for(info.mod_class);
    let refs: Vec<&ModDef> = full.iter().filter(|m| ids.iter().any(|id| id.as_str() == m.id)).collect();
    let mut out: Vec<BuffMeta> = Vec::new();
    let none = wfsim_engine::arcanes_data::ArcaneFx::none();
    merge(&mut out, enumerate_buffs(&refs, &none, info));
    let arc_base = WeaponBase::dual_toxocyst_base_evos(true, &[]);
    if let Some(arr) = v.get("arcanes").and_then(|x| x.as_array()) {
        for a in arr.iter().filter_map(|x| x.as_str()) {
            if a == "none" {
                continue;
            }
            if let Some(def) = wfsim_engine::arcanes_data::secondary(a) {
                let fx = def.fx(def.max_rank, StackPolicy::Emergent, arc_base.base_crit_chance, arc_base.base_crit_damage, arc_base.traits);
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

fn optimize_start(v: &Value) -> Value {
    let info = weapon(get_str(v, "weapon", "dual_toxocyst"));
    if info.id != "dual_toxocyst" {
        return err_json("the optimizer supports Dual Toxocyst only (v1)");
    }
    {
        let jobs = opt_jobs().lock().unwrap();
        if let Some(j) = jobs.iter().find(|j| j.active()) {
            return json!({
                "ok": false,
                "error": "an optimization is already running — cancel it or wait",
                "job_id": j.id,
            });
        }
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
    let full = mod_pool_for(info.mod_class);
    for id in fixed_ids.iter().chain(search_ids.iter()) {
        if !full.iter().any(|m| m.id == id.as_str()) {
            return err_json(format!("unknown mod id: {id}"));
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
    for id in ex_fixed.iter().chain(ex_search.iter()).filter(|id| id.as_str() != "none") {
        let Some(m) = full.iter().find(|m| m.id == id.as_str()) else {
            return err_json(format!("unknown exilus mod id: {id}"));
        };
        if !m.exilus {
            return err_json(format!("{id} is not exilus-eligible"));
        }
    }
    if ex_fixed.len() > 1 {
        return err_json(format!(
            "only one exilus slot — {} cannot all be required",
            ex_fixed.join(", ")
        ));
    }
    if let Some(f) = ex_fixed.first() {
        if fixed_ids.contains(f) {
            return err_json(format!(
                "{f} is required in both a main slot and the exilus slot — a mod equips once"
            ));
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
        .map(|o| o.as_ref().and_then(|id| full.iter().find(|m| m.id == id.as_str()).cloned()))
        .collect();

    // The MAXIMUM main slots a build may fill (1..=8; the exilus slot is the
    // +1 on top). Slots may stay empty — sizes 0..=build_size all enumerate,
    // so a scope smaller than the cap (even zero mods) is legal.
    let build_size = get_u32(v, "build_size", 8).clamp(1, 8) as usize;
    let mut pool_ids: Vec<String> = fixed_ids.iter().chain(search_ids.iter()).cloned().collect();
    pool_ids.sort();
    pool_ids.dedup();
    if fixed_ids.len() > build_size {
        return err_json(format!("more required mods ({}) than build slots ({build_size})", fixed_ids.len()));
    }
    // The pool GROUP occupies ≥1 slot whenever anything is pooled — every
    // searched build then uses at least one pooled mod (mark no pools for an
    // exactly-required build). Hence required can fill at most size−1 slots
    // while pools exist, and enumeration starts above the required count.
    if !search_ids.is_empty() && fixed_ids.len() >= build_size {
        return err_json(format!(
            "pooled mods occupy at least one of the {build_size} slots — required ({}) leaves none",
            fixed_ids.len()
        ));
    }
    let min_slots = fixed_ids.len() + usize::from(!search_ids.is_empty());
    let pool: Vec<ModDef> = full
        .iter()
        .filter(|m| pool_ids.iter().any(|id| id.as_str() == m.id))
        .cloned()
        .collect();
    let constraints = Constraints { require: fixed_ids.clone(), forbid: Vec::new() };

    // ---- evolution scope: per-tier options → the Cartesian product ----
    let evo_req = v.get("evolutions").and_then(|x| x.as_object());
    let mut evo_sets: Vec<Vec<String>> = vec![Vec::new()];
    for tier in 1u32..=4 {
        let opts: Vec<Option<String>> = evo_req
            .and_then(|o| o.get(&tier.to_string()))
            .and_then(|a| a.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| Some(s.to_string()))).collect())
            .unwrap_or_default();
        let picks = if opts.is_empty() { vec![None] } else { opts }; // empty = nothing at this tier
        let mut next = Vec::new();
        for base in &evo_sets {
            for pick in &picks {
                let mut e = base.clone();
                if let Some(id) = pick { e.push(id.clone()); }
                next.push(e);
            }
        }
        evo_sets = next;
    }
    for set in &evo_sets {
        for id in set {
            if wfsim_engine::evolutions_data::get(id).is_none() {
                return err_json(format!("unknown evolution id: {id}"));
            }
        }
    }

    // ---- arcane scope ----
    let arc_ids: Vec<String> = v
        .get("arcanes")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_else(|| vec!["none".into()]);
    let arc_base = WeaponBase::dual_toxocyst_base_evos(true, &[]);
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
        return err_json("no arcanes selected");
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
    let duration = get_f64(v, "duration", 120.0).clamp(1.0, 3600.0);
    let specs = enemies();
    let Some(spec) = specs.iter().find(|e| e.id == enemy_id) else {
        return err_json(format!("unknown enemy: {enemy_id}"));
    };
    let target = match spec.target_params(level, steel_path, false, TargetMode::InstantRespawn) {
        Ok(t) => t,
        Err(e) => return err_json(e),
    };
    let body_parts = build_body_parts(spec, headshot_pct);
    let buff_cfg = parse_buff_config(v).unwrap_or_default();
    let frenzy_lock = frenzy_lock_mode(buff_cfg.get("frenzy"));
    let scenario = Scenario {
        target,
        body_parts,
        duration_secs: duration,
        incarnon_cycle: true,
        frenzy_lock,
        buff_cfg,
    };

    // ---- register the job and hand the heavy work to a worker thread ----
    static NEXT_JOB_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT_JOB_ID.fetch_add(1, Ordering::Relaxed);
    let job = Arc::new(OptJob {
        id,
        started: std::time::Instant::now(),
        state: Arc::new(FunnelState::default()),
        phase: Mutex::new("enumerating"),
        counts: Mutex::new(None),
        result: Mutex::new(None),
    });
    {
        let mut jobs = opt_jobs().lock().unwrap();
        jobs.push(job.clone());
        // Prune old finished jobs; the running one is never removed.
        while jobs.len() > 6 {
            match jobs.iter().position(|j| !j.active()) {
                Some(pos) => drop(jobs.remove(pos)),
                None => break,
            }
        }
    }

    let target_name = s_name(&specs, enemy_id);
    let worker = job.clone();
    std::thread::spawn(move || {
        // ---- enumerate candidates per evo-set × exilus option ----
        let innate = dual_toxocyst_innate_slots();
        let exilus_refs: Vec<Option<&ModDef>> = exilus_defs.iter().map(|o| o.as_ref()).collect();
        let mut cands: Vec<Candidate> = Vec::new();
        for (vi, set) in evo_sets.iter().enumerate() {
            let refs: Vec<&str> = set.iter().map(String::as_str).collect();
            let base = WeaponBase::dual_toxocyst_incarnon_evos(true, &refs);
            let base_form = WeaponBase::dual_toxocyst_base_evos(true, &refs);
            let (mut c, _stats) = enumerate_candidates(
                &pool, &base, Some(&base_form), vi as u32, min_slots as u32, build_size as u32,
                60, &innate, &constraints, &exilus_refs,
            );
            cands.append(&mut c);
        }
        if cands.is_empty() {
            *worker.result.lock().unwrap() =
                Some(err_json("no legal builds in this scope (Forma / family constraints eliminated all)"));
            *worker.phase.lock().unwrap() = "error";
            return;
        }
        let jobs: Vec<Job> = (0..cands.len())
            .flat_map(|i| (0..arcanes.len()).map(move |a| (i, a)))
            .collect();
        let n_jobs = jobs.len();
        *worker.counts.lock().unwrap() = Some((cands.len(), n_jobs));
        *worker.phase.lock().unwrap() = "running";

        // ---- run the funnel (progress via FunnelState) ----
        let rounds = schedule_to(n_jobs, final_runs, finalists);
        let last = run_funnel(
            &cands, &arcanes, &scenario, jobs, &rounds, 0xDEAD_BEEF, false,
            Some(&worker.state),
        );
        let cancelled = worker.state.cancel.load(Ordering::Relaxed);

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
                let arcane_id = if arc.id.is_empty() { "none".to_string() } else { arc.id.clone() };
                let arcane_rank = if arc.id.is_empty() {
                    0
                } else {
                    wfsim_engine::arcanes_data::secondary(&arc.id).map(|d| d.max_rank).unwrap_or(0)
                };
                json!({
                    "rank": rank + 1,
                    "kills": s.mean_kills,
                    "kill_progress": s.mean_kill_progress,
                    "effective_dps": s.effective_dps,
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

        *worker.result.lock().unwrap() = Some(json!({
            "ok": true,
            "candidates": cands.len(),
            "jobs": n_jobs,
            "cancelled": cancelled,
            "final_runs": final_runs,
            "finalists": finalists,
            "results": results,
            "target": { "name": target_name, "level": level, "steel_path": steel_path },
        }));
        *worker.phase.lock().unwrap() = if cancelled { "cancelled" } else { "done" };
    });

    json!({ "ok": true, "job_id": id })
}
