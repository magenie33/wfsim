// SPDX-License-Identifier: AGPL-3.0-or-later
//! The player who holds the weapon: the wielder a build links, and the Tenno a
//! fight adds to it.

use serde_json::{json, Value};
use crate::registry::WeaponInfo;
use crate::request::{get_bool, get_f64};

/// THE WIELDER, before anything the fight adds: the Warframe build the weapon's
/// build links (`wielder`), the first frame a weapon is locked to when that link
/// names no frame it allows (`WeaponSpec::wielders`), or the Prototype. Its stats
/// are the Warframe module's resolve of that build, and its archon shards and
/// own aura come with it.
///
/// A COMPANION WEAPON IS CARRIED BY A COMPANION, a Sentinel or a MOA, whose stat
/// block this keeps — 367/130/80 against a Warframe's 250/0/105 — while the
/// Warframe that owns it still brings the shards and the aura: `rifle_amp`
/// reaches an Artax.
/// THE WARFRAME A REQUEST NAMES, as the board's door wants it: the build
/// itself, not the Tenno it resolves to.
///
/// ONE READING OF ONE FIELD. `wielder_from` resolves this into stats for the
/// fight and the door records it as part of the build, and the two must be
/// looking at the same object — a door that read a different `wielder` than
/// the scorer would admit one build and measure another.
pub(crate) fn wielder_build_of(v: &Value) -> Option<wfsim_engine::data::warframes::Build> {
    v.get("wielder").and_then(|x| serde_json::from_value(x.clone()).ok())
}

pub(crate) fn wielder_from(v: &Value, info: &WeaponInfo) -> wfsim_engine::data::tenno::Tenno {
    use wfsim_engine::data::warframes as wf;
    let mut t = if info.sentinel {
        wfsim_engine::data::tenno::sentinel_wielder().clone()
    } else {
        wfsim_engine::data::tenno::default_tenno().clone()
    };
    let allowed: &[String] = wfsim_engine::data::weapons::spec(&info.id).map_or(&[], |s| s.wielders.as_slice());
    let asked: Option<wf::Build> = v.get("wielder").and_then(|x| serde_json::from_value(x.clone()).ok());
    let build = match asked {
        Some(b) if allowed.is_empty() || allowed.contains(&b.frame) => Some(b),
        _ => allowed.first().map(|f| wf::Build { frame: f.clone(), ..Default::default() }),
    };
    let Some(mut build) = build else { return t };
    // A NODE THE ACTION LIST EARNS IS NOT ALSO ASSUMED: naming its action makes
    // the fight simulate it (`data::casting`), and counting the tick as well
    // would pay it twice.
    let earned: Vec<wf::NodeTrigger> = v
        .get("apl")
        .and_then(|a| serde_json::from_value::<wfsim_engine::data::apl::Apl>(a.clone()).ok())
        .map_or_else(Vec::new, |apl| {
            apl.planned()
                .iter()
                .filter_map(|(a, _)| match a {
                    wfsim_engine::data::apl::Action::Operator { sling: true, .. } => Some(wf::NodeTrigger::OperatorSling),
                    _ => None,
                })
                .collect()
        });
    let school = build.operator.as_ref().map_or(wf::FLOOR_SCHOOL, |o| o.school.as_str()).to_string();
    if let (Some(o), Some(s)) = (build.operator.as_mut(), wf::focus_school(&school)) {
        o.assumed.retain(|id| {
            !s.nodes.iter().any(|n| &n.id == id && n.trigger.is_some_and(|(tr, _)| earned.contains(&tr)))
        });
    }
    t.operator_school = school;
    let Ok(r) = wf::resolve(&build) else { return t };
    // THE WIELDER'S ABILITY STRENGTH COMES WITH THE BUILD, whatever is holding
    // the gun: an EXALTED weapon is summoned by the Warframe behind the
    // companion too, and its damage is that ability's.
    t.ability_strength = r.stat(wf::FrameStat::AbilityStrength).value;
    t.ability_duration = r.stat(wf::FrameStat::AbilityDuration).value;
    t.augments = r.augments.iter().map(|a| (*a).to_string()).collect();
    t.cast_arcanes = r.cast_arcanes.clone();
    t.weapon_buffs = r.weapon_buffs.clone();
    t.ability_efficiency = r.stat(wf::FrameStat::AbilityEfficiency).value;
    t.casting_speed_bonus = r.stat(wf::FrameStat::CastingSpeed).value - 1.0;
    if !info.sentinel {
        let stat = |s: wf::FrameStat| r.stat(s).value;
        t.id = r.frame.id.clone();
        t.name = r.frame.name.clone();
        t.health = stat(wf::FrameStat::Health);
        t.shield = stat(wf::FrameStat::Shield);
        t.armor = stat(wf::FrameStat::Armor);
        t.energy = stat(wf::FrameStat::Energy);
        t.sprint = stat(wf::FrameStat::SprintSpeed);
    }
    // FIVE SOCKETS, and a sixth is a typo rather than a build.
    t.shards = build.shards.iter().take(5).cloned().collect();
    if let Some(a) = build.aura.as_ref().filter(|a| wfsim_engine::data::auras::by_id(&a.id).is_some()) {
        t.auras.push(wfsim_engine::data::auras::AuraPick { id: a.id.clone(), count: 1 });
    }
    t
}

/// The fight's TENNO — the wielder ([`wielder_from`]), then what the fight adds:
/// the state, its own stat bonuses, the squad's auras, and last the ticked
/// overrides, which win over every one of them.
///
/// ONE builder for both the simulator and the optimizer, on purpose: the
/// optimizer must score builds under the player the sim will replay them with,
/// and two readers of the same JSON is how that drifts a field at a time.
///
/// A SENTINEL WEAPON IS ALWAYS AIMING. What it cannot do is trigger the
/// on-HEADSHOT half of an aiming mod: `default_headshot_pct` is 0 for a
/// sentinel, so no headshot lands and no on-headshot buff fires. So the state is
/// on, the triggers stay dead, and the request cannot say otherwise.
pub(crate) fn tenno_from(v: &Value, info: &WeaponInfo) -> wfsim_engine::data::tenno::Tenno {
    let mut t = wielder_from(v, info);
    t.state.aiming = info.sentinel || get_bool(v, "aiming", true);
    t.state.invisible = get_bool(v, "invisible", t.state.invisible);
    t.state.airborne = get_bool(v, "airborne", t.state.airborne);
    // Haven Foray / Guardian's Might: "With Overshields". Nothing here takes
    // them away, so it is a declaration and not something the fight tracks.
    t.state.overshields = get_bool(v, "overshields", t.state.overshields);
    // Daring Reverie, Hunter's Mantra: "With Channeled Ability active". The
    // card's note defines it — the ability must be DRAINING ENERGY over time.
    t.state.channeling = get_bool(v, "channeling", t.state.channeling);
    // DRAWN unless the scenario says otherwise: *"With Melee Weapon Equipped"*
    // is not satisfied by a quick-melee swing, and every ruler runs it drawn.
    t.state.melee_equipped = get_bool(v, "melee_equipped", t.state.melee_equipped);
    // THE LOADOUT — the Vasto's Lone Gun, "With No Primary Equipped". FALSE by
    // default, which is the fight every stored scenario and every board row was
    // measured under: the Tenno walks in carrying everything. A SENTINEL weapon
    // is not the Tenno's loadout at all, and nothing in the roster asks a
    // companion about its other slots, so it takes the same field.
    t.state.solo_weapon = get_bool(v, "solo_weapon", t.state.solo_weapon);
    // THE FIGHT'S OWN STAT BONUSES — "the effect equals stuffing in another
    // mod". One flat object, every key a bucket the mods
    // already feed, and every key optional: a fight that says nothing gets the
    // neutral player's zeroes, which is what the board is scored under.
    //
    // Read HERE rather than in `parse_fight` for the reason every other player
    // field is: the panel endpoint has no Fight and needs the same answer, and
    // two readers of one JSON is how a field drifts.
    if let Some(o) = v.get("extra_stats").and_then(|x| x.as_object()) {
        let g = |k: &str| o.get(k).and_then(Value::as_f64).unwrap_or(0.0);
        t.bonuses = wfsim_engine::data::tenno::StatBonuses {
            base_damage: g("base_damage"),
            multishot: g("multishot"),
            crit_chance: g("crit_chance"),
            crit_damage: g("crit_damage"),
            status_chance: g("status_chance"),
            status_damage: g("status_damage"),
            fire_rate: g("fire_rate"),
            reload_speed: g("reload_speed"),
            magazine: g("magazine"),
            // THE BUCKET THE PANEL HAD NO BOX FOR. The engine has always had
            // the quantity — several arcanes grant it — so this is the reader
            // finally able to say it.
            ammo_efficiency: g("ammo_efficiency"),
            status_duration: g("status_duration"),
            flat_crit_chance: g("flat_crit_chance"),
        };
    }
    // THE SQUAD'S AURAS, beside the wielder's own. On the fight rather than the
    // build for the reason `data/abilities/` is: two players with the same gun
    // and different squads are two fights — and it is what keeps them off the
    // BOARD. An aura the wielder already wears is not counted twice.
    if let Some(a) = v.get("auras").and_then(Value::as_array) {
        for p in a.iter().filter_map(|x| serde_json::from_value::<wfsim_engine::data::auras::AuraPick>(x.clone()).ok()) {
            if !t.auras.iter().any(|w| w.id == p.id) {
                t.auras.push(p);
            }
        }
    }
    // THE OVERRIDES: a ticked stat replaces the wielder's, whatever built it —
    // the one gate no frame can open ("With Energy Max Over 700") is only
    // askable by typing. The Basmu's Dreadful Killshot reads health.
    t.health = get_f64(v, "wf_health", t.health).clamp(0.0, 100_000.0);
    t.shield = get_f64(v, "wf_shield", t.shield).clamp(0.0, 100_000.0);
    t.armor = get_f64(v, "wf_armor", t.armor).clamp(0.0, 100_000.0);
    t.energy = get_f64(v, "wf_energy", t.energy).clamp(0.0, 100_000.0);
    t.sprint = get_f64(v, "wf_sprint", t.sprint).clamp(0.0, 10.0);
    t.state.energy_pct = get_f64(v, "wf_energy_pct", t.state.energy_pct).clamp(0.0, 1.0);
    // …AND ABILITY STRENGTH IS AN OVERRIDE LIKE THE REST: the linked build's,
    // until a fight types one. Same field the ability buffs read, because there
    // is one such number and a fight that disagreed with its own wielder about
    // it would scale Roar by one strength and the claws by another.
    t.ability_strength = get_f64(v, "ability_strength", t.ability_strength).clamp(0.0, 10.0);
    t
}

/// The five numbers a wielder floor is, for `/api/meta` — and its Ability
/// Strength, which the fight reads unless one is typed over it.
pub(crate) fn floor_json(t: &wfsim_engine::data::tenno::Tenno) -> Value {
    json!({
        "name": t.name, "health": t.health, "shield": t.shield,
        "armor": t.armor, "energy": t.energy, "sprint": t.sprint,
        "ability_strength": t.ability_strength,
    })
}

#[cfg(test)]
mod wielder_tests {
    use super::*;
    use crate::panel::panel_json;
    use crate::registry::weapon;
    use crate::simulate::simulate_json;

    fn resolved(frame: &str) -> wfsim_engine::data::warframes::Resolved {
        let b = wfsim_engine::data::warframes::Build { frame: frame.into(), ..Default::default() };
        wfsim_engine::data::warframes::resolve(&b).expect("a modelled frame")
    }

    /// THE WIELDER IS THE LINKED BUILD, THE LOCKED FRAME, OR THE PROTOTYPE — and
    /// a ticked override still wins over whichever it is.
    #[test]
    fn the_wielder_is_the_linked_build_the_locked_frame_or_the_prototype() {
        use wfsim_engine::data::warframes::FrameStat;
        let praedos = weapon("praedos");
        let bare = tenno_from(&json!({}), praedos);
        assert_eq!((bare.name.as_str(), bare.health, bare.armor), ("Prototype", 250.0, 105.0));

        let valkyr = wielder_from(&json!({"wielder": {"frame": "valkyr"}}), praedos);
        let r = resolved("valkyr");
        assert_eq!(valkyr.name, "Valkyr");
        assert_eq!(valkyr.health, r.stat(FrameStat::Health).value);
        assert_eq!(valkyr.armor, r.stat(FrameStat::Armor).value);
        assert_eq!(valkyr.sprint, r.stat(FrameStat::SprintSpeed).value);

        // A LOCKED WEAPON: its own frame when nothing is linked, and when the link
        // names a frame that cannot hold it; the other allowed frame when asked.
        let talons = weapon("valkyr_talons");
        assert_eq!(wielder_from(&json!({}), talons).name, "Valkyr Prime");
        assert_eq!(wielder_from(&json!({"wielder": {"frame": "nobody"}}), talons).name, "Valkyr Prime");
        assert_eq!(wielder_from(&json!({"wielder": {"frame": "valkyr"}}), talons).name, "Valkyr");

        let ticked = tenno_from(&json!({"wielder": {"frame": "valkyr"}, "wf_armor": 2000.0}), praedos);
        assert_eq!(ticked.armor, 2000.0, "an override beats the wielder");
    }

    /// THE SHARDS AND THE OWN AURA COME WITH THE WIELDER, and a squad naming the
    /// same aura does not count it twice.
    #[test]
    fn a_melee_weapon_in_valkyrs_hands_builds_rage_and_it_pays() {
        let req = |extra: serde_json::Value| {
            let b = wfsim_engine::board::benchmarks::get("standard_multi_target").expect("the ruler");
            let mut m = serde_json::to_value(&b.scenario).expect("a scenario is json");
            let o = m.as_object_mut().expect("a mapping");
            o.insert("weapon".into(), json!("praedos"));
            o.insert("mods".into(), json!(["primed_pressure_point", "organ_shatter"]));
            o.insert("runs".into(), json!(6));
            for (k, v) in extra.as_object().expect("a mapping") {
                o.insert(k.clone(), v.clone());
            }
            m
        };
        let cards = |v: &serde_json::Value| -> Vec<String> {
            panel_json(v)["buffs"].as_array().map_or(Vec::new(), |a| {
                a.iter().filter_map(|b| b["id"].as_str().map(String::from)).collect()
            })
        };
        let valkyr = json!({"wielder": {"frame": "valkyr"}});
        assert!(cards(&req(valkyr.clone())).iter().any(|c| c == "valkyr_rage"));
        assert!(!cards(&req(json!({}))).iter().any(|c| c == "valkyr_rage"), "the Prototype has no Rage");

        let score = |v: serde_json::Value| {
            let r = simulate_json(&v);
            assert!(r.get("error").is_none(), "{r}");
            r["score"].as_f64().expect("a score")
        };
        let prototype = score(req(json!({})));
        let earned = score(req(valkyr));
        let held = score(req(json!({
            "wielder": {"frame": "valkyr"},
            "buffs": {"valkyr_rage": {"stacks": 300, "locked": true}},
        })));
        assert!(earned > prototype, "Rage bought nothing: {earned} against {prototype}");
        assert!(held > earned, "a full meter held bought nothing: {held} against {earned}");
    }

    #[test]
    fn the_wielder_brings_its_shards_and_its_aura_once() {
        let praedos = weapon("praedos");
        let v = json!({
            "wielder": {
                "frame": "valkyr",
                "aura": {"id": "steel_charge"},
                "shards": [{"shard": "crimson_archon_shard", "effect": "melee_critical_damage"}],
            },
            "auras": [{"id": "steel_charge"}, {"id": "rifle_amp"}],
        });
        let t = tenno_from(&v, praedos);
        assert_eq!(t.shards.len(), 1);
        assert_eq!(t.auras.iter().filter(|a| a.id == "steel_charge").count(), 1);
        assert!(t.auras.iter().any(|a| a.id == "rifle_amp"));
        // …AND THE FIGHT NO LONGER NAMES A FRAME OR SHARDS OF ITS OWN.
        let old = tenno_from(&json!({"frame": "valkyr", "shards": [{"shard": "x", "effect": "y"}]}), praedos);
        assert_eq!((old.name.as_str(), old.shards.len()), ("Prototype", 0));
    }

    /// **THE CLAWS ARE THE STRENGTH THEIR SUMMONING READ.** Sling first and
    /// Hysteria inside Sling Strength's window takes +40% for the whole fight;
    /// Hysteria first takes the frame's own, however often the sling follows.
    /// A tick on the Operator build is the assumed reading, and naming the
    /// sling in the list replaces it rather than adding to it.
    #[test]
    fn the_claws_are_the_strength_their_summoning_read() {
        let summoned = |apl: serde_json::Value, assumed: &[&str]| {
            let v = json!({
                "weapon": "valkyr_talons",
                "wielder": {"frame": "valkyr", "operator": {"school": "madurai", "assumed": assumed}},
                "apl": apl,
            });
            let f = crate::fight::parse_fight(&v).expect("a fight");
            (f.arena.tenno.ability_strength, f.arena.tenno.summon_strength.expect("an exalted weapon"))
        };
        let sling = json!({"action": {"do": "operator", "sling": true}, "when": {"if": "buff_remains_under", "ability": "sling_strength", "seconds": 0.0}});
        let hysteria = json!({"action": {"do": "cast", "ability": "hysteria"}, "when": {"if": "always"}});
        let near = |a: f64, b: f64| (a - b).abs() < 1e-9;

        let (own, first) = summoned(json!([sling.clone(), hysteria.clone()]), &[]);
        assert!(near(own, 1.0) && near(first, 1.4), "sling, then summon: {own} {first}");
        let (_, late) = summoned(json!([hysteria.clone(), sling.clone()]), &[]);
        assert!(near(late, 1.0), "summoned before the sling: {late}");

        // THE TICK IS THE ASSUMED READING: up before the fight, so in the claws.
        let (ticked, out) = summoned(json!([]), &["sling_strength"]);
        assert!(near(ticked, 1.4) && near(out, 1.4), "{ticked} {out}");
        // …AND THE LIST REPLACES IT: the sling is earned, never paid twice.
        let (both, both_claws) = summoned(json!([sling, hysteria]), &["sling_strength"]);
        assert!(near(both, 1.0) && near(both_claws, 1.4), "{both} {both_claws}");
    }
}
