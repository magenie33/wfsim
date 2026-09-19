// SPDX-License-Identifier: AGPL-3.0-or-later
//! The leaderboard's doors: `/api/board/check`, `/api/build/keys` and
//! `/api/targets`.

use serde_json::{json, Value};
use wfsim_engine::target::TargetMode;
use crate::kitgun::board_assembly_of;
use crate::registry::enemies;
use crate::request::{get_bool, get_str, get_u32};

/// THE RIVEN SHAPE a board payload carries — two flat fields, the way the
/// endpoint stores them.
///
/// The ROLLS are deliberately not on the wire: a row states a SHAPE and the
/// scorer finds that shape's own best corner for the ruler's fight
/// (`build::rivens::perfect`). Sending a roll would be sending something nobody
/// ranks, and would invite the question of why the board's number is not the
/// one on the submitter's card.
pub(crate) fn riven_shape_from(v: &Value) -> Option<wfsim_engine::build::rivens::RivenShape> {
    let bonuses: Vec<String> = v
        .get("riven_pos")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
        .unwrap_or_default();
    if bonuses.is_empty() {
        return None;
    }
    let mut bonuses = bonuses;
    bonuses.sort();
    Some(wfsim_engine::build::rivens::RivenShape {
        bonuses,
        malus: Some(get_str(v, "riven_neg", "").to_string()).filter(|x| !x.is_empty()),
    })
}

/// THE BOARD'S ROW KEY FOR EACH OF A LIST OF BUILDS.
///
/// It answers **is what I am looking at already a row?** A pointer answer
/// (`officialBuildActive`) says only whether the ACTIVE PRESET is a builtin, so
/// a board build copied into a preset of your own, or reached by moving one mod
/// between two slots, is offered for upload as if it were new.
///
/// IT IS THE ENGINE'S ANSWER, and that is the whole point of the endpoint. The
/// normalisation behind it is not something a page can reproduce: a mod list is
/// canonicalised (`canonical_mods` sorts the non-elementals by drain and leaves
/// the elementals in the order that PAIRS them), evolutions are a set, a riven
/// is its shape. A JS copy of that is a second answer, and this repo has been
/// bitten three times by one axis spelled in two places.
///
/// A LIST rather than one build, because the caller's question is a MEMBERSHIP
/// one: it keys its own build and every row its weapon holds in the same call,
/// so the two sides cannot be keyed by two different builds of the engine.
/// A build that does not validate answers `null` rather than failing the batch
/// — a stored row this engine can no longer read is not an error in the build
/// on screen.
pub fn build_keys_json(v: &Value) -> Value {
    let keys: Vec<Value> = v
        .get("builds")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .map(|b| {
            let list = |k: &str| -> Vec<String> {
                b.get(k)
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
                    .unwrap_or_default()
            };
            match wfsim_engine::board::builds::validate_with(
                get_str(b, "weapon", ""),
                &list("mods"),
                &list("evolutions"),
                &list("arcanes"),
                get_str(b, "valence", ""),
                riven_shape_from(b).as_ref(),
                Some(get_str(b, "exilus", "")).filter(|x| !x.is_empty()),
                board_assembly_of(b).as_ref(),
            ) {
                Ok(vb) => json!(wfsim_engine::board::builds::board_key(&vb, get_str(b, "mode", ""))),
                Err(_) => Value::Null,
            }
        })
        .collect();
    json!({ "ok": true, "keys": keys })
}

/// WOULD THE BOARD TAKE THIS BUILD? The scorer's own door, asked before
/// knocking on it.
///
/// A submission is written to a store and validated an HOUR LATER by the
/// scoring job, which prints its reason to a workflow log — so a build the
/// board will never accept looks exactly like one it took, forever. Three Kuva
/// Nukor submissions sat in that state, refused on every run since they
/// arrived, while the page said "sent".
///
/// It is the SAME function the scorer calls rather than a copy of its rules:
/// the answer the player gets has to be the one the board will give. What the
/// app can check itself it still checks, because a reason on screen beats a
/// round trip; this is the backstop for the rest.
pub fn board_check_json(v: &Value) -> Value {
    let bench = get_str(v, "benchmark", "");
    let weapon = get_str(v, "weapon", "");
    let list = |k: &str| -> Vec<String> {
        v.get(k)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
            .unwrap_or_default()
    };
    let valence = get_str(v, "valence", "");
    match wfsim_engine::board::builds::validate_for_board_with(
        bench,
        weapon,
        &list("mods"),
        &list("evolutions"),
        &list("arcanes"),
        valence,
        riven_shape_from(v).as_ref(),
        // THE EXILUS SLOT'S MOD, empty on almost every submission. It is its own
        // field on the wire for the reason it is its own field in `ValidBuild`:
        // an exilus-eligible mod is legal in a MAIN slot, so a flat list cannot
        // say which entry came out of the exilus slot.
        Some(get_str(v, "exilus", "")).filter(|x| !x.is_empty()),
        // …AND THE PARTS, read exactly as the scorer reads them off the stored
        // record. The door and the scorer answer the same question or the door
        // is not one.
        board_assembly_of(v).as_ref(),
    ) {
        // A REFUSAL IS A RESULT, not a transport error: `ok` says the question
        // was answered, `accepted` says what the answer was.
        Ok(b) => json!({ "ok": true, "accepted": true, "forma": b.forma, "drain": b.drain }),
        Err(e) => json!({ "ok": true, "accepted": false, "reason": e }),
    }
}

/// Resolve a riven SPEC against a weapon: the values it shows, its generated
/// name, its drain, and every reason it could not exist.
///
/// The page owns the knobs and this owns the arithmetic — one implementation
/// of the formula, so a slider cannot drift from what the sim would build.
/// EVERY TARGET'S POOLS AT THE FIGHT'S LEVEL — what the picker shows.
///
/// A unit's OWN base level (a Corrupted Heavy Gunner at 700 health, 500 armor)
/// is the number nobody fights: at level 9999 Steel Path the same unit is four
/// orders of magnitude bigger, so choosing between two units on their base
/// stats is choosing on the wrong axis.
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
                        "armor_dr": wfsim_engine::rules::scaling::armor_damage_reduction(armor),
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
