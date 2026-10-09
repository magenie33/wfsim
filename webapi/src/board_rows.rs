// SPDX-License-Identifier: AGPL-3.0-or-later
//! The rows a build owes the board, said ONCE for every party that measures
//! one: `wfsim-intake` (what a submission becomes in the library), the scorer
//! (what a library record fights under each ruler) and the page, which runs
//! the same fights for a compute order (`/api/board/order`, `/api/board/runs`,
//! `/api/board/fold`, `/api/board/score`). A client that built its own request
//! would be measuring a second fight under the first one's name —
//! docs/BOARD.md §"Compute orders".

use serde_json::{json, Value};
use wfsim_engine::board::builds::ValidBuild;

use crate::kitgun::board_assembly_of;
use crate::request::get_str;
use crate::simulate::{riven_request, simulate_merged_json, simulate_request, simulate_shard_json};

fn ids(rec: &Value, key: &str) -> Vec<String> {
    rec.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect())
        .unwrap_or_default()
}

/// A CARD'S RANK IS NOT PART OF WHAT ARRIVES: every card is stored at max rank,
/// and intake's corner search asks the fight about the ones the every-rank list
/// names.
pub fn at_max_rank(id: &str) -> String {
    wfsim_engine::data::mods::split_rank(id).0.to_string()
}

/// WHAT A SUBMISSION IS, by the library's door: legal, normalised, its riven a
/// shape. `Err` is the reason intake refuses it. The record is the inbox one —
/// the fields `offerBoardSubmit` sends, as the worker stores them.
pub fn library_build(rec: &Value) -> Result<ValidBuild, String> {
    let weapon = get_str(rec, "weapon", "");
    let riven = {
        let bonuses = ids(rec, "riven_pos");
        (!bonuses.is_empty()).then(|| wfsim_engine::build::rivens::RivenShape {
            bonuses,
            malus: Some(get_str(rec, "riven_neg", "").to_string()).filter(|m| !m.is_empty()),
        })
    };
    let assembly = wfsim_engine::data::weapons::kitguns::assembly_of(
        weapon,
        get_str(rec, "grip", ""),
        get_str(rec, "loader", ""),
    );
    let mods: Vec<String> = ids(rec, "mods").iter().map(|m| at_max_rank(m)).collect();
    let exilus = at_max_rank(get_str(rec, "exilus", ""));
    wfsim_engine::board::builds::validate_with(
        weapon,
        &mods,
        &ids(rec, "evolutions"),
        &ids(rec, "arcanes"),
        get_str(rec, "valence", ""),
        riven.as_ref(),
        Some(exilus.as_str()).filter(|x| !x.is_empty()),
        assembly.as_ref(),
    )
    // THE WARFRAME, by the board's own rule rather than by a second copy of it:
    // dropped where a ruler pins a frameless Tenno, required on an Exalted
    // weapon whose numbers are that frame's.
    .and_then(|v| {
        let w: Option<wfsim_engine::data::warframes::Build> =
            rec.get("wielder").and_then(|x| serde_json::from_value(x.clone()).ok());
        v.with_wielder(w.as_ref())
    })
}

/// THE CANONICAL BUILD, as the library stores one.
///
/// WRITTEN FROM THE `ValidBuild` and not copied from the submission: the mod
/// order canonicalised to one representative per pairing, the evolution ladder
/// truncated to what the weapon has, the arcane list padded to its seats. A
/// record that kept the submitted spelling would key one way and simulate
/// another. `mode` does not survive: mods are equipped on the WEAPON and a mode
/// is how it is fired, so nothing about a build changes by being played
/// differently.
pub fn canonical_record(v: &ValidBuild) -> Value {
    let mut rec = json!({
        "weapon": v.weapon,
        "mods": v.mods,
        "evolutions": v.evolutions,
        "arcanes": v.arcanes,
    });
    let o = rec.as_object_mut().expect("object");
    if !v.valence.is_empty() {
        o.insert("valence".into(), json!(v.valence));
    }
    if let Some(x) = &v.exilus {
        o.insert("exilus".into(), json!(x));
    }
    if let Some(a) = &v.assembly {
        o.insert("grip".into(), json!(a.grip));
        o.insert("loader".into(), json!(a.loader));
    }
    if let Some(w) = &v.wielder {
        o.insert("wielder".into(), serde_json::to_value(w).unwrap_or(Value::Null));
    }
    if let Some(r) = &v.riven {
        o.insert("riven_pos".into(), json!(r.bonuses));
        if let Some(m) = &r.malus {
            o.insert("riven_neg".into(), json!(m));
        }
    }
    rec
}

/// …AND THE ROLLS THE RIVEN IS STORED WITH, bonuses first, then the malus —
/// the order `RivenShape::at` reads them back in.
pub fn with_rolls(mut rec: Value, rolls: &[f64]) -> Value {
    if let Some(o) = rec.as_object_mut() {
        o.insert("riven_rolls".into(), json!(rolls));
    }
    rec
}

/// WHAT A LIBRARY RECORD FIGHTS AS under one ruler — the board's door, not the
/// legality one: a row must be a COMPLETE build. `Err` is the scorer's printed
/// refusal.
pub fn scored_build(rec: &Value, bench_id: &str) -> Result<ValidBuild, String> {
    let shape = {
        let bonuses = ids(rec, "riven_pos");
        (!bonuses.is_empty()).then(|| wfsim_engine::build::rivens::RivenShape {
            bonuses: {
                let mut b = bonuses;
                b.sort();
                b
            },
            malus: rec
                .get("riven_neg")
                .and_then(Value::as_str)
                .filter(|x| !x.is_empty())
                .map(String::from),
        })
    };
    let wielder: Option<wfsim_engine::data::warframes::Build> =
        rec.get("wielder").and_then(|x| serde_json::from_value(x.clone()).ok());
    let v = wfsim_engine::board::builds::validate_for_board_with(
        bench_id,
        get_str(rec, "weapon", ""),
        &ids(rec, "mods"),
        &ids(rec, "evolutions"),
        &ids(rec, "arcanes"),
        get_str(rec, "valence", ""),
        shape.as_ref(),
        rec.get("exilus").and_then(Value::as_str).filter(|x| !x.is_empty()),
        board_assembly_of(rec).as_ref(),
        wielder.as_ref(),
    )?;
    // …AND THE NUMBERS ITS RIVEN ROLLED, if the record names them. They are part
    // of the fight, so they are part of the identity every key is taken from.
    Ok(match rec.get("riven_rolls").and_then(Value::as_array) {
        Some(rolls) => v.with_riven_rolls(rolls.iter().filter_map(Value::as_f64).collect()),
        None => v,
    })
}

/// THE CARD A BUILD NAMES — its own rolls, or the god roll if it names none
/// (what the library held before intake resolved them, and what it would
/// resolve to today).
pub fn card_of(v: &ValidBuild, shape: &wfsim_engine::build::rivens::RivenShape) -> Vec<f64> {
    if !v.riven_rolls.is_empty() {
        return v.riven_rolls.clone();
    }
    let cls = wfsim_engine::build::rivens::class_for_weapon(&v.weapon).unwrap_or("");
    let g = wfsim_engine::build::rivens::god_roll(shape, cls);
    g.bonuses.iter().map(|b| b.roll).chain(g.malus.iter().map(|m| m.roll)).collect()
}

/// ONE FIGHT PER MODE THE WEAPON CAN SUSTAIN, under one ruler's scenario. An
/// unsustainable mode ("Always Incarnon") is not a way to play for the length
/// of a ruler, so an empty list is a refusal.
pub fn row_requests(
    v: &ValidBuild,
    scenario: &Value,
) -> Vec<(wfsim_engine::data::weapons::WeaponPlayMode, Value)> {
    wfsim_engine::data::weapons::play_modes(&v.weapon)
        .into_iter()
        .filter(|m| m.sustainable)
        .map(|played| {
            let mut req = simulate_request(scenario, v, played);
            if let Some(shape) = &v.riven {
                let cls = wfsim_engine::build::rivens::class_for_weapon(&v.weapon).unwrap_or("");
                let spec = shape.at(cls, &card_of(v, shape));
                if let Some(o) = req.as_object_mut() {
                    o.insert("rivens".into(), riven_request(&spec));
                }
            }
            (played, req)
        })
        .collect()
}

/// EVERY (RULER, MODE) A LIBRARY RECORD CAN BE SCORED ON — the rows a new
/// build owes, opened as compute orders the moment it arrives. A ruler that
/// refuses the build owes nothing, and so does a mode it cannot sustain.
pub fn owed_rows(record: &Value) -> Vec<(String, String)> {
    wfsim_engine::board::benchmarks::all()
        .iter()
        .filter_map(|bench| scored_build(record, &bench.id).ok().map(|b| (bench, b)))
        .flat_map(|(bench, b)| {
            wfsim_engine::data::weapons::play_modes(&b.weapon)
                .into_iter()
                .filter(|m| m.sustainable)
                .map(move |m| (bench.id.clone(), if m.id.is_empty() { "base".to_string() } else { m.id.to_string() }))
        })
        .collect()
}

/// FOLD RUNS `from..from+count` INTO `acc`, ONE RUN A PIECE.
///
/// A piece is one run because nothing else reproduces the number: a coarser
/// piece regroups the sums and moves the last bit of everything derived from
/// one. An `acc` carried between calls as text is the same accumulator, since
/// serde_json parses numbers exactly (`float_roundtrip`). `Err` is the shard
/// that would not parse.
pub fn fold_runs(
    req: &Value,
    acc: &mut wfsim_engine::fight::Shard,
    from: u32,
    count: u32,
) -> Result<(), Value> {
    for k in from..from.saturating_add(count) {
        let piece = simulate_shard_json(req, k, 1, &mut |_, _| {});
        match serde_json::from_value::<wfsim_engine::fight::Shard>(piece.clone()) {
            Ok(s) => acc.merge(&s),
            Err(_) => return Err(piece),
        }
    }
    Ok(())
}

/// THE REPORT A FOLDED ACCUMULATOR MAKES — the scorer's `run_budgeted` ending.
pub fn measured(req: &Value, acc: &wfsim_engine::fight::Shard) -> Value {
    let shard = serde_json::to_value(acc).unwrap_or(Value::Null);
    simulate_merged_json(req, std::slice::from_ref(&shard))
}

/// THE ROW'S NUMBER IN THE RULER'S OWN UNITS. `score` off the wire is kill
/// PROGRESS over the whole engagement, so a `kpm` ruler turns it into a rate and
/// a `dps` one reads a different field entirely.
pub fn row_score(bench: &wfsim_engine::board::benchmarks::Benchmark, out: &Value) -> f64 {
    let metric = bench.metric();
    let duration = serde_json::to_value(&bench.scenario)
        .ok()
        .and_then(|s| s.get("duration").and_then(Value::as_f64))
        .unwrap_or(300.0);
    metric.of(out.get(metric.field).and_then(Value::as_f64).unwrap_or(0.0), duration)
}

fn bench_named(id: &str) -> Option<&'static wfsim_engine::board::benchmarks::Benchmark> {
    wfsim_engine::board::benchmarks::all().iter().find(|b| b.id == id)
}

/// `/api/board/order` — `{record, ruler, mode}` → `{request}`: the fight a
/// compute order names, read off its LIBRARY record exactly as the scorer reads
/// one (`scored_build`, `row_requests`) — a riven's rolls included.
pub fn board_order_json(v: &Value) -> Value {
    let record = v.get("record").cloned().unwrap_or(Value::Null);
    let ruler = get_str(v, "ruler", "");
    let mode = get_str(v, "mode", "");
    let Some(bench) = bench_named(ruler) else {
        return crate::request::err_json("unknown ruler");
    };
    let b = match scored_build(&record, ruler) {
        Ok(b) => b,
        Err(e) => return crate::request::err_json(e),
    };
    let scenario = serde_json::to_value(&bench.scenario).unwrap_or(Value::Null);
    match row_requests(&b, &scenario)
        .into_iter()
        .find(|(p, _)| (if p.id.is_empty() { "base" } else { p.id }) == mode)
    {
        Some((_, req)) => json!({ "ok": true, "request": req }),
        None => crate::request::err_json("the weapon cannot sustain that mode"),
    }
}

/// `/api/board/runs` — `{request, from, count}` → `{shards}`: runs
/// `from..from+count`, ONE SHARD A RUN and none of them merged, so a row's runs
/// can be fought on every lane at once and still fold in the scorer's order.
pub fn board_runs_json(v: &Value) -> Value {
    let req = v.get("request").cloned().unwrap_or(Value::Null);
    let from = v.get("from").and_then(Value::as_u64).unwrap_or(0) as u32;
    let count = v.get("count").and_then(Value::as_u64).unwrap_or(0) as u32;
    let mut shards = Vec::with_capacity(count as usize);
    for k in from..from.saturating_add(count) {
        let piece = simulate_shard_json(&req, k, 1, &mut |_, _| {});
        if serde_json::from_value::<wfsim_engine::fight::Shard>(piece.clone()).is_err() {
            return piece;
        }
        shards.push(piece);
    }
    json!({ "ok": true, "shards": shards })
}

/// `/api/board/fold` — `{request, from, count, acc?}` → `{acc}`. The page
/// carries `acc` between calls so a long row yields to the reader between
/// pieces and still folds exactly as the scorer does. With `pieces` — the
/// `/api/board/runs` shards of the runs after `acc`, IN RUN ORDER — it merges
/// those instead of fighting: the same merges in the same order, so the same bits.
pub fn board_fold_json(v: &Value) -> Value {
    let req = v.get("request").cloned().unwrap_or(Value::Null);
    let mut acc: wfsim_engine::fight::Shard = match v.get("acc") {
        None | Some(Value::Null) => Default::default(),
        Some(a) => match serde_json::from_value(a.clone()) {
            Ok(s) => s,
            Err(e) => return crate::request::err_json(format!("acc: {e}")),
        },
    };
    if let Some(pieces) = v.get("pieces").and_then(Value::as_array) {
        for p in pieces {
            match serde_json::from_value::<wfsim_engine::fight::Shard>(p.clone()) {
                Ok(s) => acc.merge(&s),
                Err(e) => return crate::request::err_json(format!("piece: {e}")),
            }
        }
        return json!({ "ok": true, "acc": acc });
    }
    let from = v.get("from").and_then(Value::as_u64).unwrap_or(0) as u32;
    let count = v.get("count").and_then(Value::as_u64).unwrap_or(0) as u32;
    if let Err(piece) = fold_runs(&req, &mut acc, from, count) {
        return piece;
    }
    json!({ "ok": true, "acc": acc })
}

/// `/api/board/score` — `{ruler, request, acc}` → `{score, metric, work}`, the
/// scorer's own ending: the report, then the ruler's metric — and the WORK the
/// fold held (`Shard::work`), which a compute order's clients must agree on
/// beside the score (docs/BOARD.md §"Contribution").
pub fn board_score_json(v: &Value) -> Value {
    let Some(bench) = bench_named(get_str(v, "ruler", "")) else {
        return crate::request::err_json("unknown ruler".to_string());
    };
    let req = v.get("request").cloned().unwrap_or(Value::Null);
    let acc: wfsim_engine::fight::Shard = match serde_json::from_value(v.get("acc").cloned().unwrap_or(Value::Null)) {
        Ok(s) => s,
        Err(e) => return crate::request::err_json(format!("acc: {e}")),
    };
    let out = measured(&req, &acc);
    if !out.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return out;
    }
    json!({ "ok": true, "score": row_score(bench, &out), "metric": bench.metric().id, "work": acc.work() })
}
