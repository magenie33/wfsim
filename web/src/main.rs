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
use wfsim_engine::dummy::{monte_carlo, Arcane, BodyPart, DummyParams, LockMode, TargetMode};
use wfsim_engine::enemy_data::EnemySpec;
use wfsim_engine::loadout::{
    resolve, DtEvo2, ModDef, ModEffect, ResolvedPanel, StackPolicy, WeaponBase,
};
use wfsim_engine::mods::{plan_forma, PlannedMod, Polarity};
use wfsim_optimizer::{dual_toxocyst_innate_slots, pool};

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
    mod_class: &'static str, // "pistol" | "rifle"
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
    use ModEffect::*;
    // (id, max-rank drain, max_rank, polarity, family, effects)
    let md = |id, base_drain, max_rank, polarity, family, effects| ModDef {
        id,
        base_drain,
        max_rank,
        polarity,
        family,
        effects,
    };
    vec![
        // Damage
        md("serration", 14, 10, Polarity::Madurai, None, vec![BaseDamage(1.65)]),
        md("heavy_caliber", 16, 10, Polarity::Madurai, None, vec![BaseDamage(1.65)]), // accuracy downside = no-op
        // Multishot (Split Chamber ↔ Galvanized Chamber share the "chamber" family)
        md("split_chamber", 15, 5, Polarity::Madurai, Some("chamber"), vec![Multishot(0.90)]),
        md("galvanized_chamber", 16, 10, Polarity::Madurai, Some("chamber"), vec![
            Multishot(0.80),
            OnKillMultishot { per_stack: 0.30, max_stacks: 5, duration: 20.0 },
        ]),
        md("vigilante_armaments", 9, 5, Polarity::Naramon, None, vec![Multishot(0.60)]),
        // Crit
        md("point_strike", 9, 5, Polarity::Madurai, None, vec![CritChance(1.50)]),
        md("vital_sense", 9, 5, Polarity::Madurai, None, vec![CritDamage(1.20)]),
        md("critical_delay", 9, 5, Polarity::Naramon, None, vec![CritChance(2.00), FireRate(-0.20)]), // corrupted
        // Status
        md("galvanized_aptitude", 12, 10, Polarity::Vazarin, None, vec![
            StatusChance(0.80),
            ConditionOverload { per_stack: 0.40, max_stacks: 2, duration: 20.0 },
        ]),
        // Single elements (+90%)
        md("cryo_rounds", 11, 5, Polarity::Vazarin, None, vec![Element(D::Cold, 0.90)]),
        md("hellfire", 11, 5, Polarity::Naramon, None, vec![Element(D::Heat, 0.90)]),
        md("stormbringer", 11, 5, Polarity::Naramon, None, vec![Element(D::Electricity, 0.90)]),
        md("infected_clip", 11, 5, Polarity::Naramon, None, vec![Element(D::Toxin, 0.90)]),
        // Dual-stat elements (60/60)
        md("rime_rounds", 7, 3, Polarity::Madurai, None, vec![Element(D::Cold, 0.60), StatusChance(0.60)]),
        md("malignant_force", 7, 3, Polarity::Madurai, None, vec![Element(D::Toxin, 0.60), StatusChance(0.60)]),
        md("high_voltage", 7, 3, Polarity::Madurai, None, vec![Element(D::Electricity, 0.60), StatusChance(0.60)]),
        md("thermite_rounds", 7, 3, Polarity::Madurai, None, vec![Element(D::Heat, 0.60), StatusChance(0.60)]),
        // Fire rate
        md("vile_acceleration", 9, 5, Polarity::Naramon, None, vec![FireRate(0.90), BaseDamage(-0.15)]), // corrupted
        md("speed_trigger", 9, 5, Polarity::Madurai, None, vec![FireRate(0.60)]),
        md("shred", 11, 5, Polarity::Madurai, None, vec![FireRate(0.30)]), // punch-through = no-op single-target
        // Headshot-gated crit (Galvanized ↔ Argon share the "scope" family; both
        // do nothing on a sentinel — recorded so the picker shows them honestly)
        md("galvanized_scope", 12, 10, Polarity::Madurai, Some("scope"), vec![
            OnHeadshotCritChance { bonus: 1.20, duration: 12.0 },
            OnHeadshotKillCritChance { per_stack: 0.40, max_stacks: 5, duration: 12.0 },
        ]),
        md("argon_scope", 7, 5, Polarity::Madurai, Some("scope"), vec![
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
        _ => respond(&mut stream, "404 Not Found", "text/plain; charset=utf-8", b"not found"),
    }
}

// ---- /api/meta ---------------------------------------------------------

fn pct(v: f64) -> String {
    // Trim trailing zeros for readability (+90% not +90.0%).
    let x = (v * 100.0 * 10.0).round() / 10.0;
    if x.fract() == 0.0 {
        format!("{:+}", x as i64)
    } else {
        format!("{x:+}")
    }
}

fn effect_str(e: &ModEffect) -> String {
    use ModEffect::*;
    match e {
        BaseDamage(v) => format!("{}% base damage", pct(*v)),
        Multishot(v) => format!("{}% multishot", pct(*v)),
        CritChance(v) => format!("{}% crit chance", pct(*v)),
        CritDamage(v) => format!("{}% crit damage", pct(*v)),
        StatusChance(v) => format!("{}% status chance", pct(*v)),
        FireRate(v) => format!("{}% fire rate", pct(*v)),
        ReloadSpeed(v) => format!("{}% reload speed", pct(*v)),
        StatusDamage(v) => format!("{}% status damage", pct(*v)),
        Element(t, v) => format!("{}% {t:?}", pct(*v)),
        CombinedElement(t, v) => format!("{}% {t:?}", pct(*v)),
        OnKillMultishot { per_stack, max_stacks, .. } => {
            format!("on kill: {}% multishot ×{max_stacks}", pct(*per_stack))
        }
        ConditionOverload { per_stack, max_stacks, .. } => {
            format!("CO {}%/type ×{max_stacks}", pct(*per_stack))
        }
        OnHeadshotCritChance { bonus, .. } => format!("on headshot: {}% crit chance", pct(*bonus)),
        OnHeadshotKillCritChance { per_stack, max_stacks, .. } => {
            format!("on headshot kill: {}% crit chance ×{max_stacks}", pct(*per_stack))
        }
    }
}

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
            json!({
                "id": m.id,
                "name": prettify(m.id),
                "drain": m.base_drain,
                "max_rank": m.max_rank,
                "polarity": format!("{:?}", m.polarity),
                "family": m.family,
                "category": mod_category(m),
                "image": assets().mods.get(m.id),
                "effects": m.effects.iter().map(effect_str).collect::<Vec<_>>(),
            })
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
        "arcanes": [
            {"id": "none", "name": "None", "image": null},
            {"id": "enervate", "name": "Secondary Enervate", "image": assets().arcanes.get("secondary_enervate")},
            {"id": "deadhead", "name": "Secondary Deadhead", "image": assets().arcanes.get("secondary_deadhead")},
            {"id": "flare", "name": "Cascadia Flare", "image": assets().arcanes.get("cascadia_flare")},
        ],
        "evo2": [
            {"id": "fevered", "name": "Fevered Frenzy"},
            {"id": "carnage", "name": "Carnage Reign"},
        ],
        "defaults": {
            "weapon": "dual_toxocyst",
            "form": "incarnon_cycle",
            "evo2": "fevered",
            "arcane": "deadhead",
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
    let evo2 = match get_str(v, "evo2", "fevered") {
        "carnage" => DtEvo2::CarnageReign,
        _ => DtEvo2::FeveredFrenzy,
    };
    let arcane = if info.uses_arcane {
        match get_str(v, "arcane", "deadhead") {
            "enervate" => Arcane::Enervate,
            "deadhead" => Arcane::Deadhead,
            "flare" => Arcane::CascadiaFlare,
            _ => Arcane::None,
        }
    } else {
        Arcane::None // sentinels / robotic weapons cannot equip arcanes
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
    params.arcane = arcane;
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
