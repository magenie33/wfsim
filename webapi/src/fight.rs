// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE FIGHT, parsed once for the simulator and the optimizer, and
//! `/api/pairings`.

use serde_json::{json, Value};
use wfsim_engine::fight::{BuffLock, LockMode};
use wfsim_engine::target::{BodyPart, TargetMode};
use wfsim_engine::data::enemies::EnemySpec;
use wfsim_engine::model::StackPolicy;
use crate::buffs::{BuffCfg, frenzy_apply, frenzy_lock_mode, parse_buff_config};
use crate::registry::{WeaponInfo, default_weapon_id, enemies, evo_group, form_unlock_evo, incarnon_id, weapon};
use crate::request::{err_json, get_bool, get_f64, get_str, get_u32};
use crate::tenno::tenno_from;

/// **WHAT THIS FIGHT SAYS ABOUT THE CLASS THIS WEAPON IS IN.**
///
/// `class_rules` is `{ "<slot>": { "<axis>": value } }` on the wire — a fight
/// carries rules for every class, and a weapon reads only its own column.
/// Returned typed, so `build::scenario::resolve` never sees a `serde_json::Value`.
///
/// A RULE FOR AN AXIS THAT TAKES NEITHER SHAPE IS DROPPED here rather than
/// reaching the resolver as a guess: the resolver's own answer for an axis
/// nobody ruled is the capability's, which is the safe direction.
fn class_rules_for(
    v: &Value,
    weapon_id: &str,
) -> std::collections::BTreeMap<String, wfsim_engine::build::scenario::AxisValue> {
    use wfsim_engine::build::scenario::AxisValue;
    let mut out = std::collections::BTreeMap::new();
    let Some(class) = wfsim_engine::build::scenario::class_of(weapon_id) else {
        return out;
    };
    let Some(map) = v.get("class_rules").and_then(|c| c.get(class)).and_then(|c| c.as_object())
    else {
        return out;
    };
    for (k, val) in map {
        let typed = match val {
            Value::Bool(b) => Some(AxisValue::Flag(*b)),
            Value::Number(n) => n.as_f64().map(AxisValue::Number),
            _ => None,
        };
        if let Some(t) = typed {
            out.insert(k.clone(), t);
        }
    }
    out
}

/// The fight's answer for a FLAG axis: the reader's box, the weapon's
/// capability and this scenario's class rule folded into one.
fn resolved_flag(
    axis_id: &str,
    weapon_id: &str,
    rule: Option<wfsim_engine::build::scenario::AxisValue>,
    readers: bool,
) -> bool {
    use wfsim_engine::build::scenario::{self, AxisValue};
    let Some(a) = scenario::axis(axis_id) else { return readers };
    scenario::resolve(a, weapon_id, rule)
        .value(AxisValue::Flag(readers))
        .as_flag()
        .unwrap_or(readers)
}

/// The same, for a NUMBER axis.
fn resolved_number(
    axis_id: &str,
    weapon_id: &str,
    rule: Option<wfsim_engine::build::scenario::AxisValue>,
    readers: f64,
) -> f64 {
    use wfsim_engine::build::scenario::{self, AxisValue};
    let Some(a) = scenario::axis(axis_id) else { return readers };
    scenario::resolve(a, weapon_id, rule)
        .value(AxisValue::Number(readers))
        .as_number()
        .unwrap_or(readers)
}

/// The headshot rate a weapon is played at when nothing says otherwise.
///
/// A SENTINEL weapon is fired by the companion, which picks its own targets
/// and does not aim for the head — so 0, not the player's 100. It stays a knob: this is the default, not a ceiling.
fn default_headshot_pct(info: &WeaponInfo) -> f64 {
    if info.sentinel {
        0.0
    } else {
        100.0
    }
}

/// THE ENEMIES A REQUEST BRINGS WITH IT, beside the published roster.
///
/// A custom enemy is a CUSTOM in the sense `AGENTS.md` gives the word: a thing
/// the player MADE, which the other modules consume — here, an entry in the
/// scenario's enemy list. It exists only on the machine that made it, so it
/// travels inline exactly as a riven does, and for the same reason: the server
/// has never heard of it and a share link has to carry it or lie.
///
/// It is the SAME TYPE as a published unit (`EnemySpec`) rather than a reduced
/// one, so nothing downstream learns that an enemy can be homemade — level
/// scaling, the vulnerability column, body parts, Eximus legality and the
/// target card all read the one shape they already read.
///
/// A BROKEN ONE IS AN ERROR, not a silent fallback to the default target: the
/// number a fight produces is meaningless if the target was quietly not the
/// one asked for.
fn custom_enemies(v: &Value) -> Result<Vec<wfsim_engine::data::enemies::EnemySpec>, String> {
    let Some(arr) = v.get("custom_enemies").and_then(|a| a.as_array()) else {
        return Ok(Vec::new());
    };
    let published = enemies();
    let mut out: Vec<wfsim_engine::data::enemies::EnemySpec> = Vec::new();
    for e in arr {
        let spec: wfsim_engine::data::enemies::EnemySpec =
            serde_json::from_value(e.clone()).map_err(|err| format!("custom enemy: {err}"))?;
        if spec.body_parts.is_empty() {
            return Err(format!("{}: an enemy needs at least one body part", spec.name));
        }
        // A CUSTOM MAY NOT WEAR A PUBLISHED ID. The id is what a scenario, a
        // board row and a share link name, so one that shadows `thrax_centurion`
        // would silently redefine the fight every ruler is measured under.
        if published.iter().any(|p| p.id == spec.id) || out.iter().any(|p| p.id == spec.id) {
            return Err(format!("{} is already an enemy id", spec.id));
        }
        // Built here and thrown away: it is the one place that reports a bad
        // damage-type name or an impossible multiplier, and a request is the
        // only moment anyone can be told.
        spec.target_params(1, false, false, TargetMode::InstantRespawn)?;
        out.push(spec);
    }
    Ok(out)
}

fn build_body_parts(spec: &EnemySpec, headshot_pct: f64) -> Vec<BodyPart> {
    let h = (headshot_pct / 100.0).clamp(0.0, 1.0);
    let heads: Vec<_> = spec.body_parts.iter().filter(|p| p.is_head).collect();
    let bodies: Vec<_> = spec.body_parts.iter().filter(|p| !p.is_head).collect();

    let make = |b: &wfsim_engine::data::enemies::BodyPartSpec, w: f64| BodyPart {
        name: b.name.clone(),
        aim_weight: w,
        multiplier: b.multiplier,
        is_head: b.is_head,
        is_weak_point: b.is_weak_point(),
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

/// The evolutions of `ids` that the weapon can actually REACH, in order.
///
/// The tiers are a LADDER: tier N is installed only after tier N-1, so a set
/// that skips one does not describe a weapon anyone can hold. Everything from
/// the first gap upward is dropped. The UI locks the rows, but it cannot be
/// the only place the rule holds — a preset saved before it existed, or a
/// hand-built request, still carries the gap, and the engine would price it.
pub(crate) fn ladder_prefix(ids: Vec<String>) -> Vec<String> {
    let tier_of = |id: &String| wfsim_engine::data::evolutions::get(id).map(|e| e.tier);
    let mut tiers: Vec<u32> = ids.iter().filter_map(tier_of).collect();
    tiers.sort_unstable();
    let reach = tiers
        .iter()
        .enumerate()
        .take_while(|(i, t)| **t == *i as u32 + 1)
        .count() as u32;
    ids.into_iter()
        .filter(|id| wfsim_engine::data::evolutions::get(id).is_some_and(|e| e.tier <= reach))
        .collect()
}

/// THE EVOLUTIONS ARE EXACTLY THE LIST: no ladder trims it and no form implies
/// its unlock. The Shapley analysis's subsets are built this way.
fn evolutions_as_given(v: &Value) -> bool {
    v.get("evolutions_as_given").and_then(Value::as_bool).unwrap_or(false)
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
///
/// `evolutions_as_given: true` SKIPS THE LADDER, and only the Shapley analysis
/// sends it (docs/SHAPLEY.md): taking tier 2 out must take out tier 2's perk,
/// not tiers 3 and 4 with it. No build, share link or board row carries it.
pub(crate) fn chosen_evolutions(v: &Value, info: &WeaponInfo) -> Result<Vec<String>, String> {
    let ladder = !evolutions_as_given(v);
    let mine = |ids: Vec<String>| -> Vec<String> {
        let group = evo_group(info);
        let ids: Vec<String> = ids
            .into_iter()
            .filter(|id| wfsim_engine::data::evolutions::get(id).is_some_and(|e| e.weapon == group))
            .collect();
        if ladder { ladder_prefix(ids) } else { ids }
    };
    if let Some(arr) = v.get("evolutions").and_then(|x| x.as_array()) {
        let ids: Vec<String> = arr
            .iter()
            .filter_map(|s| s.as_str())
            .filter(|s| !s.is_empty() && *s != "none")
            .map(String::from)
            .collect();
        for id in &ids {
            if wfsim_engine::data::evolutions::get(id).is_none() {
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
/// Two parsers — one in `simulate_json`, one in `parse_optimize` — drift, and
/// every way they can drift scores builds under a fight the replay will not
/// run: a form-unlock fallback, a caller omitting `evolutions` getting the
/// Incarnon cycle free while the search scores the base form, an optimizer
/// keeping a buff config of its own.
///
/// So this is not a helper both agree to call — it is the ONE parse, and the
/// simulator is the truth. The optimizer adds only
/// what is its own: the scope to search and the budget to spend.
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
    /// WHICH TRIGGERS FIRE NO BUFF HERE (`engine::buff_events`). Beside
    /// `aiming` rather than in `buff_cfg` because it is the ENGAGEMENT's: a
    /// benchmark states it once, where a per-buff map would name ids it cannot
    /// know.
    pub(crate) denied_buff_triggers: Vec<String>,
    /// Both actors and how long they are at it.
    pub(crate) arena: wfsim_engine::arena::Arena,
    /// After the ladder is applied AND the form's own unlock is implied.
    pub(crate) evos: Vec<String>,
    /// THE FORM A CYCLE FILLS ITS GAUGE IN, `None` when a single form is fired
    /// throughout. Not the weapon's default: there is one cycle per form it can
    /// be fed from (`play_modes`), and the MODE is what says which.
    pub(crate) cycle_from: Option<&'static str>,
    /// The single form to fire (the cycle's Incarnon half when cycling).
    pub(crate) single_form: &'static str,
    /// The target's display NAME, resolved once. A second lookup would go to
    /// the published roster, which has never heard of an enemy the player
    /// built — so a custom target reported its own id as its name.
    pub(crate) enemy_name: String,
    /// WHAT A RUN IS JUDGED BY, and therefore which run is the benchmark fight.
    pub(crate) metric: &'static wfsim_engine::rules::metrics::MetricDef,
    /// The MODE asked for, resolved to one this weapon has. The optimizer reads
    /// it as the fallback for a request that names no mode axis. It replaced
    /// `form` and `untransformed_id`, which the optimizer was the only reader
    /// of: a mode resolves to BOTH of those (`mode_forms`) and it does so per
    /// variant now, so a fight carrying one copy of them was a fight claiming
    /// the search fires one form.
    ///
    /// FOR THE SIMULATE PATH the pair below is still what fires — one mode,
    /// one form, which is what a simulate is.
    /// it as the fallback for a request that names no mode axis, so "the mode
    /// a simulate would run" and "the mode a search runs" are one answer.
    pub(crate) mode: String,
    pub(crate) level: u32,
    pub(crate) steel_path: bool,
    /// Is the target its ELITE variant? A property of the fight, like the
    /// level and the Steel Path switch, so it lives here and both modules
    /// read it from one place.
    pub(crate) eximus: bool,
    pub(crate) headshot_pct: f64,
    pub(crate) tenno: wfsim_engine::data::tenno::Tenno,
    pub(crate) infinite_ammo: bool,
    /// DO THE BODIES DROP AMMO, and how far is a pack collected from — the
    /// other half of the ammo economy, and the half that decides nothing while
    /// `infinite_ammo` is on (which is what every ruler is scored under).
    pub(crate) ammo_drops: bool,
    pub(crate) pickup_range_m: f64,
    /// An OPEN-WORLD fight: every drop rate is higher (`engine::ammo`).
    pub(crate) landscape: bool,
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
/// baseline). It is a function of the FIGHT, so it is written here
/// rather than at each caller: `parse_optimize` had the only copy, and the
/// pairing endpoint needs the same answer or it would label the wrong form's
/// elements.
pub(crate) fn firing_entry(fight: &Fight) -> String {
    if fight.cycle_from.is_some() {
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
/// the browser to pair elements — would be a second copy of `rules::elements::combine`
/// rules 2 and 3, and it would be wrong about innate elements the first time a
/// weapon carried one.
pub fn pairings_json(v: &Value) -> Value {
    let fight = match parse_fight(v) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let fire = firing_entry(&fight);
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
            // `board::builds::normalize` applies to a submission.
            let named: Vec<&str> =
                set.as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
            let pool = wfsim_engine::data::mods::pool_naming(&fire, &named);
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
            let orders: Vec<Value> = wfsim_engine::board::builds::element_orders(&fire, &ids, &evos)
                .into_iter()
                .map(|o| {
                    let name = |t: wfsim_engine::rules::damage::DamageType| format!("{t:?}");
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
    // EVERY SCENARIO HAS ONE CORE METRIC, and it has to be one this build
    // declares. The fight reads it once — it ranks the runs to pick the
    // benchmark fight — and the ranking surfaces read it for everything else.
    //
    // REFUSED, not defaulted. A share link or a saved scenario from a newer
    // build is exactly where this arrives, and answering it with kills per
    // minute is a number the reader cannot tell from a right one.
    let metric_id = match get_str(v, "metric", "") {
        "" => wfsim_engine::rules::metrics::DEFAULT,
        id => id,
    };
    let Some(metric) = wfsim_engine::rules::metrics::get(metric_id) else {
        return Err(err_json(format!(
            "unknown metric: {metric_id} — a scenario is judged by one of {}",
            wfsim_engine::rules::metrics::ALL
                .iter()
                .map(|m| m.id)
                .collect::<Vec<_>>()
                .join(", ")
        )));
    };
    let info = weapon(get_str(v, "weapon", default_weapon_id()));
    // Per-buff configured policy (Sim panel section 2). Present ⇒ Emergent sim
    // with each buff carrying its own initial stacks + lock. Absent ⇒ the
    // legacy `assume_max`/`frenzy` knobs (byte-for-byte with the old path).
    let buff_cfg = parse_buff_config(v);
    // AN UNKNOWN NAME IS DROPPED, not refused: this list travels in share links,
    // so a fight written against a newer vocabulary has to stay openable.
    let denied_buff_triggers: Vec<String> = v
        .get("buff_triggers_off")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .filter(|s| wfsim_engine::data::buff_events::ALL.iter().any(|(id, _)| id == s))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
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
    let has_frenzy = wfsim_engine::data::weapons::has_perk(&info.id, "frenzy")
        || incarnon_id(info).is_some_and(|i| wfsim_engine::data::weapons::has_perk(i, "frenzy"));
    // One value for all three forms: the weapon must OWN the passive, and
    // the request may still switch it off (or configure it via buff_cfg).
    // The cycle reads it too, or its knob is dead.
    let frenzy_single = frenzy_single && has_frenzy;
    let PlayedMode { mode_id, evos, cycle_from, single_form } = played_mode(v, info)?;
    let enemy_id = get_str(v, "enemy", "thrax_centurion");
    let level = get_u32(v, "level", 9999).clamp(1, 9999);
    let steel_path = get_bool(v, "steel_path", true);
    // HOW MANY PEOPLE ARE SHOOTING — a property of the FIGHT, so it is parsed
    // here and nowhere else. It decides two things that must not disagree: how
    // often a body drops ammo (`engine::ammo`), and how much health a unit
    // whose health reads the squad has (a Demolisher, and nothing else).
    //
    // ONE PLAYER STILL FIRES. A bigger squad is a HARDER TARGET here, not three
    // more guns — this arena has one shooter and always has — so a ruler that
    // names four is asking what one weapon takes off the Demolisher a full
    // squad meets. That is a real fight and a stated one; it is not a claim
    // about squad DPS.
    let squad_size = get_u32(v, "squad_size", 1).clamp(1, 4);
    // A SENTINEL'S HEADSHOT RATE IS NOT THE PLAYER'S TO SET. Its companion
    // picks its own targets and never aims for a head, so this is 0 whatever
    // the request says — the same shape as `tenno_from` forcing its stance.
    //
    // A DEFAULT is not enough once a benchmark pins the field: the aimed board
    // pins 100, and without this a sentinel
    // would be ranked at a headshot rate it cannot reach. Two boards that
    // differ only in the player's aim therefore give a sentinel the same score
    // twice, which is the honest answer to "how much of this weapon is your
    // aim": none of it.
    // THE SCENARIO'S OWN HOUSE RULES, per weapon class. A
    // fight is ONE DOCUMENT that any weapon can be tested against, so the rules
    // it holds for the classes it is not currently pointed at travel with it and
    // are legible on any weapon's page. Looked up once here and handed to
    // `build::scenario::resolve`, which keeps that module free of the wire format.
    let class_rules = class_rules_for(v, &info.id);
    let rule = |id: &str| class_rules.get(id).copied();
    // OVERRIDES SIT BEHIND LEGALITY. A companion cannot put a shot on a head,
    // so `AimsAtHead` is a GAME FACT and a scenario saying otherwise is refused
    // — the engine decides that, here and on the page, from the one table.
    //
    // A DEFAULT is not enough once a benchmark pins the field: the aimed board
    // pins 100, and without this a sentinel
    // would be ranked at a headshot rate it cannot reach. Two boards that
    // differ only in the player's aim therefore give a sentinel the same score
    // twice, which is the honest answer to "how much of this weapon is your
    // aim": none of it.
    let headshot_pct = resolved_number(
        "headshot_pct",
        &info.id,
        rule("headshot_pct"),
        get_f64(v, "headshot_pct", default_headshot_pct(info)),
    );
    let tenno = tenno_from(v, info);
    // INFINITE AMMO, and it is the DEFAULT for every weapon.
    // The sim models no ammo PICKUPS, so a finite reserve is the pessimistic
    // half of a mechanic we only half have — and the headline number people
    // compare across weapons is the one where ammo is not the limit. A weapon
    // whose reserve is infinite in game (every sentinel weapon: "Ammo Max: ∞ /
    // Ammo Type: None") cannot be switched off it, which the UI shows as a
    // ticked, disabled box rather than a control that does nothing.
    //
    // The MAGAZINE is unaffected either way: this is the reserve behind it, so
    // reload cadence — and `ammo_cost` — still bite.
    //
    // THE RESUPPLY HALF IS RESOLVED HERE rather than inside
    // `reserve_is_infinite`: it is `Capability::CanResupply`, and
    // it is the one capability whose absence is OURS rather than the game's, so
    // it is also the one a scenario may argue with. `false` for a ground
    // Arch-Gun as it always was — unless this fight's class rules say Arch-Guns
    // are resupplied in here, which is a legal thing for a fight to say.
    let infinite_ammo = resolved_flag(
        "infinite_ammo",
        &info.id,
        rule("infinite_ammo"),
        get_bool(v, "infinite_ammo", true),
    );
    // THE PICKUPS. On by default because that is what the game does; the reach
    // is infinite by default because this arena's Tenno does not walk, so a
    // finite one is a wall rather than a walk (`FightParams::pickup_range_m`).
    let ammo_drops = get_bool(v, "ammo_drops", true);
    let pickup_range_m = get_f64(v, "pickup_range_m", f64::INFINITY).max(0.0);
    let landscape = get_bool(v, "landscape", false);
    let duration = get_f64(v, "duration", 180.0).clamp(1.0, 3600.0);
    let (player_at, target_at) = positions(v);
    let runs = get_u32(v, "runs", 100).clamp(1, 20_000);
    let seed = v.get("seed").and_then(|x| x.as_u64()).unwrap_or(0xC0FFEE);

    // ---- WARFRAME ABILITY BUFFS (`data/abilities/`) -----------------------
    // `ability_strength` is a FRACTION (1.0 = 100%), because that is what it
    // multiplies. `abilities` is what the player ticked, each with its own
    // seconds — omit `secs` (or send null) for the whole fight.
    //
    // Resolved right here rather than carried raw: `data::abilities::resolve`
    // applies the strength AND settles the same-family conflicts, so nothing
    // downstream — not the sim, not the optimizer, not the replay — can end up
    // adding two Roars together.
    // THE WIELDER'S, unless the fight types one over it — `tenno_from` has
    // already settled that, and reading the request again here would let the two
    // disagree about one number.
    let strength = tenno.ability_strength;
    let picks = ability_picks(v);
    // …and the WEAPON'S CLASS, because one member is worth double on a class:
    // Resupply is 20/30/40/50% on Sniper Rifles. `resolve` is the one function
    // handed both the ability and the weapon, so nothing downstream has to know
    // what a sniper is.
    // THE FRAME CASTING THEM: every one of these is the wielder's build, which
    // is why they travel together (`abilities::Caster`).
    let seated: Vec<&str> = tenno.augments.iter().map(String::as_str).collect();
    let mut abilities = wfsim_engine::data::abilities::resolve(
        &picks,
        &wfsim_engine::data::abilities::Caster {
            strength,
            duration: tenno.ability_duration,
            efficiency: tenno.ability_efficiency,
            casting_speed_bonus: tenno.casting_speed_bonus,
            augments: &seated,
        },
        wfsim_engine::data::weapons::spec(&info.id).map_or("", |s| s.class.as_str()),
        wfsim_engine::data::weapons::spec(&info.id).map_or("", |s| s.slot.as_str()),
    );
    // …AND WHETHER THEY ARE CAST OR ASSUMED, WHICH IS THE ACTION LIST'S ANSWER.
    // An ability the list names is cast — energy out of the pool and, when it
    // roots the frame, the shooting with it; one it never names is handed to
    // you, which is what every stored scenario and every board row was measured
    // under. An empty list is that reading for all of them.
    // THE RULES THIS REQUEST INSERTED. The fight's own half is added where the
    // params are built, off the one fact that decides it (`FightParams::apl`),
    // so nothing here has to know what a mode resolves to.
    let apl = inserted_apl(v)?;
    let cast_interrupts =
        wfsim_engine::data::abilities::plan_casts(&mut abilities, &apl.abilities(), tenno.energy, duration)
            .interrupts;

    // The published roster PLUS whatever this request brought with it. A
    // custom shadows nothing (`custom_enemies` refuses a published id), so the
    // order here decides nothing — it is one list.
    let mut specs = enemies();
    match custom_enemies(v) {
        Ok(extra) => specs.extend(extra),
        Err(e) => return Err(err_json(e)),
    }
    let Some(spec) = specs.iter().find(|e| e.id == enemy_id) else {
        return Err(err_json(format!("unknown enemy: {enemy_id}")));
    };
    // ELITE VARIANT, and it DEFAULTS ON wherever the unit has one. The Eximus is what a Steel Path player actually meets —
    // extra health and a pool of Overguard in front of it — so the ordinary
    // unit is the special case to ask for, not the elite one.
    //
    // The default is the UNIT's answer rather than a flat `true`, because the
    // engine REJECTS a combination that does not exist in game (a Thrax has no
    // Eximus variant; its overguard is innate). A blanket default would turn
    // every Thrax fight into an error, so the fallback is what this unit can
    // be — and an explicit `true` on a unit that cannot still fails, which is
    // the rigor being kept rather than worked around.
    let eximus = get_bool(v, "eximus", spec.can_be_eximus);
    let mut target = match spec.target_params(level, steel_path, eximus, TargetMode::InstantRespawn) {
        Ok(t) => t,
        Err(e) => return Err(err_json(e)),
    };
    // …AND THE SQUAD'S SHARE OF IT, applied to the BASE so the level curve
    // carries it exactly as it carries the unit's own health. A unit with no
    // ladder multiplies by 1 and this line is not a special case for it.
    target.base_health *= spec.squad_health_multiplier(squad_size);
    // THE SECOND HALF OF A THRAX'S DEATH, and the fight's own switch. OFF by
    // default, which is what every number this app has published assumes: the
    // physical form falls and that is the kill. Ticked, the kill — and with it
    // every on-kill buff and every drop — waits for a spectre no weapon can
    // reach, so a gun's ceiling on that fight is zero kills.
    if get_bool(v, "spectral_form", false) {
        target.spectral = spec.spectral_form;
    }
    // (The target's pools are read off the ARENA by whoever reports them —
    // one target, one place it lives.)
    let body_parts = build_body_parts(spec, headshot_pct);

    let formation = formation_from(v, &specs, enemy_id, level, steel_path, headshot_pct)?;

    let aim_at = v
        .get("aim_at")
        .and_then(Value::as_array)
        .filter(|a| a.len() == 2)
        .map(|a| {
            wfsim_engine::rules::space::Vec2::new(
                a[0].as_f64().unwrap_or(0.0),
                a[1].as_f64().unwrap_or(0.0),
            )
        });
    let (target, body_parts, target_at, others, aimed_id) =
        aimed_body(v, aim_at, player_at, target, body_parts, target_at, formation);
    // ---- the ARENA: both actors, and how long they are at it. Assembled
    // once and handed whole to whichever constructor runs, so the two forms
    // of a cycle cannot end up fighting two different fights.
    let arena = wfsim_engine::arena::Arena {
        apl,
        // THE SQUAD THE SCENARIO NAMED. It rides on the arena so the optimizer
        // inherits it from the same constructor rather than re-deriving it.
        squad_size,
        // THE BODY THE WEAPON IS ON, by name. It may be any of them: the shot
        // goes where it is pointed, and the nearest body on the LINE is the one
        // it hits — which need not be the fight's nominal target.
        target_id: aimed_id,
        tenno: tenno.clone(),
        target,
        body_parts,
        // THE 2D LAYER, as two POINTS — what the scene drags and what the next
        // enemy will need, since a second body cannot be described by a
        // distance (`engine::space`).
        player_at,
        target_at,
        // THE REST OF THE FORMATION. Parsed HERE for the same reason the
        // abilities are: a formation is a property of the FIGHT, so the
        // optimizer searches the one the replay will run without a line of
        // optimizer code.
        others,
        // WHERE THE WEAPON POINTS. `None` is at the target, which is the fight
        // every golden value and both boards rest on.
        aim_at,
        duration_seconds: duration,
        // WARFRAME ABILITY BUFFS — parsed HERE, in `parse_fight`, which is what
        // makes the optimizer score under them without a line of optimizer code
        // (the house rule: anything that is a property of the fight goes in the
        // one module both read).
        //
        // …AND THE TARGET GETS A SAY. A Demolisher pulses every 5 s and
        // dispels every Warframe ability within range and on itself, so against
        // one they are simply not up. Applied here rather than inside the sim
        // because it is a property of the FIGHT — the same reason the abilities
        // are parsed here at all — and so the optimizer scores under it too,
        // without knowing what a Demolisher is.
        abilities: if spec.nullifies_warframe_abilities {
            Vec::new()
        } else {
            abilities
        },
        // A NULLIFIER EATS THE CASTS TOO — what it dispels is the ability, so
        // the pauses it would have cost are not paid either.
        cast_interrupts: if spec.nullifies_warframe_abilities { Vec::new() } else { cast_interrupts },
        // …AND THE PICKS THAT PRODUCED THEM, so a build carrying an Invocation
        // can resolve them again at its own Ability Strength. A Nullifier eats
        // the picks too: what it dispels is the ability, not the number that
        // scaled it.
        ability_picks: if spec.nullifies_warframe_abilities {
            Vec::new()
        } else {
            picks
                .iter()
                .map(|p| wfsim_engine::arena::OwnedAbilityPick {
                    id: p.id.to_string(),
                    duration_seconds: p.duration_seconds,
                    element: p.element.map(str::to_string),
                })
                .collect()
        },
        ability_strength: strength,
    };
    Ok(Fight {
        info,
        policy,
        buff_cfg,
        denied_buff_triggers,
        arena,
        evos,
        cycle_from,
        single_form,
        mode: mode_id,
        enemy_name: spec.name.clone(),
        metric,
        level,
        steel_path,
        eximus,
        headshot_pct,
        tenno,
        infinite_ammo,
        ammo_drops,
        pickup_range_m,
        landscape,
        duration,
        runs,
        seed,
        has_frenzy,
        frenzy_single,
        frenzy_locks,
        cycle_frenzy_lock,
    })
}

/// A mode as [`played_mode`] resolves it: its name, the evolutions it installs,
/// the half a cycle returns to, and the single form to fire.
struct PlayedMode {
    mode_id: String,
    evos: Vec<String>,
    cycle_from: Option<&'static str>,
    single_form: &'static str,
}

/// HOW THE WEAPON IS PLAYED, from the BUILD side of the request.
///
/// `mode` is the vocabulary now — `base`, `cycle`, `alternate` — and it is a
/// property of the entrant, so it arrives with the mods rather than with the
/// fight. `WeaponPlayMode::form` is the one place it becomes a form, which
/// is what lets "played without ever transmuting" be asked for at all.
///
/// `form` is still READ when no mode is named, because share links and
/// scenario presets written before this carry it. A stale `form` is not
/// migrated, it is simply obeyed one last time.
fn played_mode(v: &Value, info: &'static WeaponInfo) -> Result<PlayedMode, Value> {
    let modes = wfsim_engine::data::weapons::play_modes(&info.id);
    let asked = v.get("mode").and_then(Value::as_str);
    let form = match asked {
        Some(want) => modes
            .iter()
            .find(|m| m.id == want)
            .map(|m| m.form())
            .unwrap_or("default"),
        None => get_str(v, "form", "default"),
    };
    // The mode NAME, kept beside the form it resolved to. A request naming a
    // mode this weapon does not have gets the weapon's own first one, which is
    // the same fallback the form takes.
    let mode_id = asked
        .filter(|want| modes.iter().any(|m| m.id == *want))
        .map(String::from)
        .or_else(|| {
            // No mode named: say which one the FORM means, so a share link
            // written before the vocabulary existed still reports honestly.
            modes
                .iter()
                .find(|m| m.form() == form)
                .or(modes.first())
                .map(|m| m.id.to_string())
        })
        .unwrap_or_else(|| "base".to_string());
    let evos = match chosen_evolutions(v, info) {
        Ok(e) => e,
        Err(e) => return Err(err_json(e)),
    };
    // ASKING FOR A FORM IMPLIES THE EVOLUTION THAT IS THAT FORM.
    //
    // Falling back to "base" when the tier-1 unlock is not among the chosen
    // evolutions makes the form control lie: with no evolutions picked — the
    // state the page STARTS in — all three options produce the base form's
    // number and nothing says why.
    //
    // Implying it is the honest model, not a shortcut. Tier 1 is
    // `selection: fixed` on every Incarnon ladder: it is not a choice, it is
    // what installing the Genesis grants. And it carries no stat of its own —
    // `UnlocksForm` applies nothing, because the form it unlocks is a separate
    // weapon entry with its own numbers. So the form and the evolution were two
    // controls for ONE fact, and this is which of them decides.
    let unlock = form_unlock_evo(info);
    let mut evos = evos;
    if form != "base" && !evolutions_as_given(v) {
        if let Some(u) = unlock {
            if !evos.iter().any(|e| e == u) {
                evos.push(u.to_string());
            }
        }
    }
    // ---- WHICH FORM (or the two-form CYCLE) this run simulates -------------
    // A cycle is a MODE over two forms, not a form, and it exists only where a
    // form must be TRANSFORMED into. Requiring that is a fix, not a tidy-up:
    // a default that falls through to the cycle for every weapon simulates a
    // weapon with no Incarnon form — a sentinel weapon, a bow — transforming on
    // a borrowed gauge (9 weakpoint hits, 2.35 s + 1.0 s of animation), and the
    // dead time comes straight off its DPS.
    let registered = wfsim_engine::data::weapons::forms_of(&info.id);
    // `default` = however THIS weapon is played: the cycle where there is one
    // to run, its own default form where there is not. A weapon that
    // transforms is played transforming.
    let form = if form == "default" && info.has_cycle { "gauge_cycle" } else { form };
    // Otherwise the cycle is asked for BY NAME, never as "any form string this
    // weapon does not register" — that makes it the destination of every typo,
    // so a stale preset naming another weapon's form transforms instead of
    // falling back to a real form.
    // BOTH SPELLINGS. `incarnon_cycle` was the token until 2026-08-15 and was
    // never persisted anywhere, so this is belt-and-braces rather than a
    // migration — but a request is a request and refusing one costs a fight.
    // THE HALF THE CYCLE RETURNS TO, off the mode that asked for it — and a
    // request naming no mode gets the default form, which is what
    // `form: gauge_cycle` has always meant.
    // NO UNLOCK, NO FORM TO TRANSFORM INTO — so no cycle, and the list the
    // fight runs has no transmute in it. Only an `evolutions_as_given` request
    // gets here without one; everything else had it implied above.
    let unlocked = unlock.is_none_or(|u| evos.iter().any(|e| e == u));
    let cycle_from = ((form == "gauge_cycle" || form == "incarnon_cycle")
        && info.has_cycle
        && unlocked
        && incarnon_id(info).is_some())
    .then(|| {
        modes
            .iter()
            .find(|m| Some(m.id) == asked && m.other_id.is_some())
            .map_or(info.id.as_str(), |m| m.weapon_id)
    });
    // The single form to fire: the requested kind if this weapon registers it,
    // else its default (which is what an unknown or stale preset value gets).
    let has_gauge =
        |id: &str| wfsim_engine::data::weapons::spec(id).is_some_and(|s| s.has_gauge());
    let single_form = registered
        .iter()
        .find(|f| f.kind.id() == form)
        .or_else(|| registered.iter().find(|f| f.is_default))
        .map(|f| f.weapon_id)
        .unwrap_or(&info.id);
    // …and the form it fires is the one the mode RETURNS to, never the locked one.
    let single_form = if unlocked {
        single_form
    } else {
        modes
            .iter()
            .find(|m| Some(m.id) == asked)
            .map(|m| m.weapon_id)
            .filter(|id| !has_gauge(id))
            .or_else(|| registered.iter().find(|f| f.is_default).map(|f| f.weapon_id))
            .unwrap_or(&info.id)
    };
    Ok(PlayedMode { mode_id, evos, cycle_from, single_form })
}

/// WHERE THE TWO OF THEM STAND, in metres — the fight's 2D layer. Two
/// POINTS, which is what the engine takes and what a dragged scene produces;
/// `distance` is still accepted and read as "the target, that far up the y
/// axis", so a scenario or a link written before the scene existed opens as
/// the same fight.
///
/// THE FLOOR IS CONTACT, not zero (`space::CONTACT_RANGE_M`): two bodies of
/// 0.2 m cannot stand closer than 0.4 m apart, and a zero would put them in
/// the same place. Anything nearer is pushed out along the line between
/// them, which is what a drag does on screen and what a stale `distance: 0`
/// resolves to.
///
/// Capped at 300 m rather than at nothing: past the longest falloff window
/// and the widest cone in the roster every extra metre is the same answer,
/// and a fight at 10 km is a typo rather than a scenario.
fn positions(v: &Value) -> (wfsim_engine::rules::space::Vec2, wfsim_engine::rules::space::Vec2) {
    let point = |k: &str, dflt: wfsim_engine::rules::space::Vec2| match v.get(k).and_then(|p| p.as_array()) {
        Some(a) if a.len() == 2 => wfsim_engine::rules::space::Vec2::new(
            a[0].as_f64().unwrap_or(0.0).clamp(-300.0, 300.0),
            a[1].as_f64().unwrap_or(0.0).clamp(-300.0, 300.0),
        ),
        _ => dflt,
    };
    let player_at = point("player_at", wfsim_engine::rules::space::Vec2::ORIGIN);
    let target_at = point(
        "target_at",
        wfsim_engine::rules::space::Vec2::new(
            0.0,
            // A GAP, so the two CENTRES stand one contact further apart —
            // the legacy field and the arena agree on what a distance means.
            get_f64(v, "distance", 0.0).clamp(0.0, 300.0) + wfsim_engine::rules::space::CONTACT_RANGE_M,
        ),
    );
    // …and the bodies are pushed apart if the request put them through each
    // other. Along the line between them, so a drag that overshoots slides
    // rather than snapping to an axis; straight up the y axis when they are on
    // the same spot and there is no line to speak of.
    let target_at = {
        let d = player_at.distance(target_at);
        let floor = wfsim_engine::rules::space::CONTACT_RANGE_M;
        if d >= floor {
            target_at
        } else if d <= 0.0 {
            wfsim_engine::rules::space::Vec2::new(player_at.x, player_at.y + floor)
        } else {
            let k = floor / d;
            wfsim_engine::rules::space::Vec2::new(
                player_at.x + (target_at.x - player_at.x) * k,
                player_at.y + (target_at.y - player_at.y) * k,
            )
        }
    };
    (player_at, target_at)
}

/// The abilities the player ticked, each with its own seconds (`None` for the
/// whole fight) and, where the ability offers one, its element.
/// **THE RULES THE PLAYER INSERTED**, above whatever the mode already does.
///
/// REFUSED RATHER THAN IGNORED. Every action and every condition is a typed
/// field (`data::apl`), so a rule naming something this engine does not have
/// comes back as an error the reader can see — SimC's own failure is a
/// mistyped condition that quietly never fires, and a fight that silently
/// dropped a cast would report a kpm nobody's build produces.
fn inserted_apl(v: &Value) -> Result<wfsim_engine::data::apl::Apl, Value> {
    let Some(raw) = v.get("apl") else {
        return Ok(wfsim_engine::data::apl::Apl::default());
    };
    serde_json::from_value(raw.clone())
        .map_err(|e| err_json(format!("bad action priority list: {e}")))
}

fn ability_picks(v: &Value) -> Vec<wfsim_engine::data::abilities::AbilityPick<'_>> {
    v
        .get("abilities")
        .and_then(Value::as_array)
        .map(|seq| {
            seq.iter()
                .filter_map(|e| {
                    let id = e.get("id").and_then(Value::as_str)?;
                    Some(wfsim_engine::data::abilities::AbilityPick {
                        id,
                        duration_seconds: e
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
        .unwrap_or_default()
}

/// THE FORMATION: every OTHER body on the floor.
///
/// Each entry is wholly its own: its own unit, its own
/// level, its own Eximus answer, its own place. Nothing about one reaches
/// another — which is why this loop is the same six lines the single target
/// above runs, repeated, rather than a variation on it.
///
/// Anything a body omits it takes from the AIMED one, so a formation of
/// nine identical enemies is nine positions and nothing else.
fn formation_from(
    v: &Value,
    specs: &[EnemySpec],
    enemy_id: &str,
    level: u32,
    steel_path: bool,
    headshot_pct: f64,
) -> Result<Vec<wfsim_engine::formation::FoeSpec>, Value> {
    let mut formation: Vec<wfsim_engine::formation::FoeSpec> = Vec::new();
    if let Some(list) = v.get("formation").and_then(Value::as_array) {
        // FIFTY, and it is DECLARED IN THE ENGINE (`formation::MAX_BODIES`),
        // because that is who pays for it: every body is a full target with its
        // own pools, procs and DoTs, a chain resolves against all of them on
        // every shot, and `RunResult::damage_by_body` is sized off the same
        // number. Two copies of a cap is one cap and one bug.
        use wfsim_engine::formation::MAX_BODIES;
        if list.len() > MAX_BODIES {
            return Err(err_json(format!(
                "{} enemies, and a formation holds at most {MAX_BODIES}",
                list.len()
            )));
        }
        for (i, e) in list.iter().enumerate() {
            // AN EMPTY STRING IS NOT AN ID. The page stores a body's unit as
            // `""` for "same as the target", which is the common case and what
            // every body a formation is built from carries — so reading it as a
            // name looked the id up, failed, and refused the whole fight with
            // `unknown enemy: ` and nothing after the colon.
            //
            // The same applies to a LEVEL of null and an EXIMUS of null: absent
            // and blank are one state here, and it means "the aimed body's".
            let id = e
                .get("enemy")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or(enemy_id);
            let Some(es) = specs.iter().find(|s| s.id == id) else {
                return Err(err_json(format!("unknown enemy: {id}")));
            };
            let lv = e
                .get("level")
                .and_then(Value::as_f64)
                .filter(|v| *v >= 1.0)
                .map_or(level, |v| v as u32);
            let ex = e
                .get("eximus")
                .and_then(Value::as_bool)
                .unwrap_or(es.can_be_eximus);
            let tp = match es.target_params(lv, steel_path, ex, TargetMode::InstantRespawn) {
                Ok(t) => t,
                Err(err) => return Err(err_json(format!("enemy {}: {err}", i + 1))),
            };
            let at = e
                .get("at")
                .and_then(Value::as_array)
                .filter(|a| a.len() == 2)
                .map(|a| {
                    wfsim_engine::rules::space::Vec2::new(
                        a[0].as_f64().unwrap_or(0.0),
                        a[1].as_f64().unwrap_or(0.0),
                    )
                });
            let Some(at) = at else {
                return Err(err_json(format!("enemy {} has no position", i + 1)));
            };
            formation.push(wfsim_engine::formation::FoeSpec {
                // WHO THIS ONE IS. The page may name it; a blank or absent one
                // is filled in BY POSITION, which is what every scenario
                // written before ids existed means and what keeps them all
                // readable. `+ 2` because the AIMED body is `e1`.
                id: e
                    .get("id")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map_or_else(|| format!("e{}", i + 2), str::to_string),
                params: tp,
                // ITS OWN HITBOXES, off its own unit — a formation of two
                // different units has two different head multipliers, and the
                // headshot share is the FIGHT's either way.
                body_parts: build_body_parts(es, headshot_pct),
                at,
            });
        }
    }
    Ok(formation)
}

/// WHICH BODY THE BEAM IS ON.
///
/// AIM IS A DIRECTION: the request names a PLACE, and
/// whatever the line from the muzzle runs through is what gets hit —
/// `space::first_hit`. Aiming at the floor two metres short of a body still
/// hits it, because the body's circle is still on the line.
///
/// RESOLVED HERE, ONCE, and that is exact rather than a shortcut: bodies do
/// not move, so the first one on the line is the first one on the line for
/// the whole engagement. The sim keeps its aimed body as a distinguished
/// one and never has to re-decide.
fn aimed_body(
    v: &Value,
    aim_at: Option<wfsim_engine::rules::space::Vec2>,
    player_at: wfsim_engine::rules::space::Vec2,
    target: wfsim_engine::target::Foe,
    body_parts: Vec<BodyPart>,
    target_at: wfsim_engine::rules::space::Vec2,
    formation: Vec<wfsim_engine::formation::FoeSpec>,
) -> (
    wfsim_engine::target::Foe,
    Vec<BodyPart>,
    wfsim_engine::rules::space::Vec2,
    Vec<wfsim_engine::formation::FoeSpec>,
    String,
) {
    if let Some(aim) = aim_at {
        // WHICHEVER BODY THE LINE CROSSES FIRST — and a shot that crosses
        // NOBODY is a legal shot: *"the engine mechanically
        // fires toward the aim point; if it hits, it hits, and if it does not,
        // it is zero"*. Refusing it was wrong twice over — a miss is an answer,
        // and aiming BESIDE a crowd so the splash catches more of it is a real
        // tactic rather than a mistake.
        //
        // The arena still names a target, because a fight has one and every
        // report reads its pools; what changes is that the weapon is not
        // pointed at it. `FightParams::off_axis_deg` is how far off the line it
        // sits, the spread cone is measured from the LINE, and a body far
        // enough off it is simply never hit.
        let mut all: Vec<wfsim_engine::formation::FoeSpec> =
            vec![wfsim_engine::formation::FoeSpec {
                // THE AIMED BODY IS `e1`, always. It is not in the `formation`
                // list — it is the fight's own target — so it takes the first
                // name rather than one out of that list's numbering.
                id: v
                    .get("target_id")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map_or_else(|| "e1".to_string(), str::to_string),
                params: target,
                body_parts,
                at: target_at,
            }];
        all.extend(formation);
        let muzzle = wfsim_engine::rules::space::muzzle(player_at, aim);
        let dir = wfsim_engine::rules::space::Vec2::new(aim.x - muzzle.x, aim.y - muzzle.y);
        let bodies: Vec<_> = all.iter().map(|f| f.at).collect();
        // NOBODY ON THE LINE keeps the NEAREST body as the arena's target. It
        // is not being shot at — the geometry says so and the numbers follow —
        // but it is the body whose pools the run reports, and the one a chain
        // or a splash is most likely to reach.
        let aimed = wfsim_engine::rules::space::first_hit(muzzle, dir, &bodies)
            .map(|(i, _)| i)
            .or_else(|| {
                (0..bodies.len()).min_by(|&a, &b| {
                    bodies[a]
                        .distance(aim)
                        .partial_cmp(&bodies[b].distance(aim))
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then(a.cmp(&b))
                })
            })
            .unwrap_or(0);
        let aimed = all.remove(aimed);
        (aimed.params, aimed.body_parts, aimed.at, all, aimed.id)
    } else {
        // NO AIM POINT is the fight this engine has always run: the beam is on
        // the target, wherever it stands.
        // NO AIM POINT, so the aimed body is the fight's own target and it keeps
        // the first name — the formation is numbered from `e2`.
        let id = v
            .get("target_id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map_or_else(|| "e1".to_string(), str::to_string);
        (target, body_parts, target_at, formation, id)
    }
}

/// **A GUARANTEED STATUS IS WHAT OVERWHELMING ATTRITION ASKS NOT TO HAPPEN.**
///
/// Valence Formation imbues an element *"with guaranteed Status for 20s"* (wiki
/// `Valence_Formation`); the Laetum's Overwhelming Attrition pays *"On Hit that
/// is neither Critical nor applies a Status Effect"*. A hit that always procs
/// can never be such a hit, so the two cannot be played together and the
/// augment silently wins — reported from the game.
///
/// The engine had the Attrition condition right (`tier == 0 && procs.is_empty()`)
/// and no way to know a status was being forced: this file's `add_element` read
/// the element and the size and dropped the third clause of the card.
///
/// ASSERTED AS AN ANSWER, not as a flag. What is checked is that the perk is
/// worth NOTHING with the augment up — and, as the control that makes that
/// meaningful, that it is worth a great deal without it. A test that only
/// asserted the first would pass on an engine that had lost the perk entirely.
#[cfg(test)]
mod the_demolisher_ruler {
    
    use serde_json::json;

    /// **THE SQUAD THE RULER NAMES REACHES THE TARGET, AND NOTHING ELSE MOVES.**
    ///
    /// `squad_size` is parsed in `parse_fight` and rides on the Arena, so this
    /// asserts the whole path a ruler's term takes: yaml -> scenario ->
    /// `parse_fight` -> the pools the fight actually has. A term that stopped
    /// anywhere along it would leave the board ranking a solo Demolisher under
    /// a name that says four.
    ///
    /// HEALTH ONLY. A squad makes the unit fatter and does not touch its armour
    /// — asserted rather than assumed, because a multiplier applied to the
    /// wrong field is the one mistake that still reads as "the number went up".
    #[test]
    fn a_squads_share_reaches_the_demolishers_health_and_no_other_pool() {
        let fight = |squad: u32| {
            let v = json!({
                "weapon": "braton",
                "enemy": "demolisher_devourer",
                "level": 9999,
                "steel_path": true,
                "squad_size": squad,
                "duration": 10,
            });
            let f = super::parse_fight(&v).expect("the demolisher fight parses");
            (f.arena.target.max_health(), f.arena.target.armor(), f.arena.squad_size)
        };
        let (solo_hp, solo_armor, solo_n) = fight(1);
        let (full_hp, full_armor, full_n) = fight(4);
        assert_eq!((solo_n, full_n), (1, 4), "the arena carries the squad it was given");
        assert!(
            (full_hp / solo_hp - 3.0).abs() < 1e-6,
            "+200% health at four: {full_hp} against {solo_hp}"
        );
        assert!(
            (full_armor - solo_armor).abs() < 1e-6,
            "armour is not the squad's business: {full_armor} against {solo_armor}"
        );

        // …AND A UNIT WITH NO LADDER IS UNMOVED, which is what keeps the two
        // Thrax boards where they were when this term was added.
        let thrax = |squad: u32| {
            let v = json!({
                "weapon": "braton",
                "enemy": "thrax_centurion",
                "level": 9999,
                "steel_path": true,
                "squad_size": squad,
                "duration": 10,
            });
            super::parse_fight(&v).expect("the thrax fight parses").arena.target.max_health()
        };
        assert!((thrax(4) - thrax(1)).abs() < 1e-6, "a Thrax does not read the squad");
    }

    /// **THE RULER SAYS FOUR, AND IT DIFFERS FROM THE AIMED BOARD IN TWO TERMS.**
    ///
    /// The file claims exactly that in its own header, and a claim about a
    /// ruler's terms is the one thing a reader cannot check for themselves
    /// without diffing two files by eye.
    #[test]
    fn the_demolisher_ruler_changes_the_target_and_the_squad_and_nothing_else() {
        let of = |id: &str| {
            wfsim_engine::board::benchmarks::all()
                .iter()
                .find(|b| b.id == id)
                .unwrap_or_else(|| panic!("no ruler {id}"))
                .scenario
                .clone()
        };
        // A NEW RULER MUST NOT BECOME THE ONE THE APP OPENS ON. `all()` sorts
        // the primary to the front and leaves the rest in path order, and the
        // page seeds both the board view and a first visitor's SCENARIO from
        // the first entry — which is how adding `standard_multi_target.yaml` once put
        // every newcomer in a 361-body fight. Alphabetically this file lands
        // between `standard_single_target` and `single_target_no_aim`, so the guard is
        // worth an assertion rather than a reading of the filename.
        let order: Vec<&str> = wfsim_engine::board::benchmarks::all()
            .iter()
            .map(|b| b.id.as_str())
            .collect();
        assert_eq!(order.first(), Some(&"standard_single_target"), "order: {order:?}");
        assert!(order.contains(&"demolisher"), "order: {order:?}");

        let (aimed, demo) = (of("standard_single_target"), of("demolisher"));
        let get = |s: &serde_norway::Value, k: &str| s.get(k).cloned();

        assert_eq!(
            get(&demo, "enemy").and_then(|v| v.as_str().map(String::from)).as_deref(),
            Some("demolisher_devourer")
        );
        assert_eq!(get(&demo, "squad_size").and_then(|v| v.as_u64()), Some(4));
        // The aimed board names no squad at all, which is what "solo" is.
        assert_eq!(get(&aimed, "squad_size"), None);

        // EVERY OTHER KEY IS THE AIMED BOARD'S, both directions — a key added to
        // one and not the other is the third difference this ruler promises not
        // to have.
        let keys = |s: &serde_norway::Value| -> Vec<String> {
            s.as_mapping()
                .expect("a scenario is a mapping")
                .keys()
                .filter_map(|k| k.as_str().map(String::from))
                .collect()
        };
        let (ka, kd) = (keys(&aimed), keys(&demo));
        for k in &ka {
            assert!(kd.contains(k), "the demolisher ruler drops `{k}`");
        }
        for k in &kd {
            assert!(k == "squad_size" || ka.contains(k), "the demolisher ruler invents `{k}`");
        }
        for k in ka.iter().filter(|k| *k != "enemy") {
            assert_eq!(get(&aimed, k), get(&demo, k), "`{k}` differs and is not a declared term");
        }
    }
}
