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

use crate::weapon_options::board_assembly_of;
use crate::request::get_str;
use crate::simulate::{riven_request, simulate_merged_json, simulate_request, PreparedSimulation};

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

/// A legal submitted build retains its declared ranks and riven rolls.
pub fn library_build(rec: &Value) -> Result<ValidBuild, String> {
    let mut record = rec.clone();
    let bonuses = ids(rec, "riven_pos");
    if let Some(raw) = rec.get("riven_rolls") {
        let rolls: Vec<f64> = raw.as_array().ok_or("riven rolls must be an array")?
            .iter().map(|v| v.as_f64().filter(|n| n.is_finite()).ok_or("invalid riven roll"))
            .collect::<Result<_, _>>()?;
        let malus = !get_str(rec, "riven_neg", "").is_empty();
        if rolls.len() != bonuses.len() + usize::from(malus) { return Err("riven roll count does not match its stats".into()); }
        let mut paired: Vec<_> = bonuses.iter().cloned().zip(rolls.iter().copied()).collect();
        paired.sort_by(|a,b| a.0.cmp(&b.0));
        record["riven_pos"] = json!(paired.iter().map(|p| &p.0).collect::<Vec<_>>());
        let mut sorted: Vec<_> = paired.iter().map(|p| p.1).collect();
        if malus { sorted.push(*rolls.last().unwrap()); }
        record["riven_rolls"] = json!(sorted);
    }
    let mut build = board_build(&record)?;
    if let Some(shape) = &build.riven {
        if build.riven_rolls.is_empty() { build.riven_rolls = wfsim_engine::build::rivens::default_rolls(shape); }
        let class = wfsim_engine::build::rivens::class_for_weapon(&build.weapon).ok_or("no riven class")?;
        let illegal = shape.at(class, &build.riven_rolls).illegal();
        if !illegal.is_empty() { return Err(illegal.join("; ")); }
    }
    Ok(build)
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
        o.extend(a.parts().as_object().expect("parts object").clone());
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
    let v = board_build(rec)?;
    admitted(rec, bench_id, &v)?;
    Ok(v)
}

/// THE RULER-FREE HALF OF [`scored_build`]: the record read as a legal build,
/// its riven's rolls included. A caller crossing one record with every ruler
/// reads it once here and asks [`admitted`] per ruler.
pub fn board_build(rec: &Value) -> Result<ValidBuild, String> {
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
    let v = wfsim_engine::board::builds::validate_with(
        get_str(rec, "weapon", ""),
        &ids(rec, "mods"),
        &ids(rec, "evolutions"),
        &ids(rec, "arcanes"),
        get_str(rec, "valence", ""),
        shape.as_ref(),
        rec.get("exilus").and_then(Value::as_str).filter(|x| !x.is_empty()),
        board_assembly_of(rec).as_ref(),
    )?
    .with_wielder(wielder.as_ref())?;
    // …AND THE NUMBERS ITS RIVEN ROLLED, if the record names them. They are part
    // of the fight, so they are part of the identity every key is taken from.
    Ok(match rec.get("riven_rolls").and_then(Value::as_array) {
        Some(rolls) => v.with_riven_rolls(rolls.iter().filter_map(Value::as_f64).collect()),
        None => v,
    })
}

/// …AND THE RULER'S HALF: whether that build is the shape `bench_id` admits.
pub fn admitted(rec: &Value, bench_id: &str, v: &ValidBuild) -> Result<(), String> {
    wfsim_engine::board::builds::admit_for_board(bench_id, get_str(rec, "weapon", ""), v)
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

/// One fight per sustainable, supported input of this build, under one ruler.
pub fn row_requests(
    v: &ValidBuild,
    scenario: &Value,
) -> Vec<(wfsim_engine::data::weapons::WeaponPlayMode, Value)> {
    v.board_modes()
        .into_iter()
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
/// refuses the build owes nothing, and so does an unsupported or unsustainable mode.
pub fn owed_rows(record: &Value) -> Vec<(String, String)> {
    let Ok(b) = board_build(record) else { return Vec::new() };
    let modes: Vec<String> = b.board_modes().iter()
        .map(|m| if m.id.is_empty() { "base" } else { m.id }.to_string()).collect();
    wfsim_engine::board::benchmarks::all()
        .iter()
        .filter(|bench| admitted(record, &bench.id, &b).is_ok())
        .flat_map(|bench| modes.iter().map(move |m| (bench.id.clone(), m.clone())))
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
    let range = from..from.saturating_add(count);
    if range.is_empty() {
        return Ok(());
    }
    let prepared = PreparedSimulation::new(req)?;
    for k in range {
        let piece = prepared.shard_json(k, 1);
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
        Some((_, req)) => {
            let mut out=json!({ "ok":true,"request":req,"identity":wfsim_engine::board::builds::build_id(&b) });
            if let Some(actual)=v.get("observation") {
                out["matches_observation"]=crate::board_flow::observation(&record,Some(&json!({
                    "ruler":ruler,"mode":mode,"request":actual
                })))["valid"].clone();
            }
            out
        },
        None => crate::request::err_json("this build has no supported sustainable input for that mode"),
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
    let range = from..from.saturating_add(count);
    if !range.is_empty() {
        let prepared = match PreparedSimulation::new(&req) {
            Ok(p) => p,
            Err(e) => return e,
        };
        for k in range {
            let piece = prepared.shard_json(k, 1);
            if serde_json::from_value::<wfsim_engine::fight::Shard>(piece.clone()).is_err() {
                return piece;
            }
            shards.push(piece);
        }
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

/// `/api/board/score` — `{ruler, request, acc}` → `{score, metric, work, counts}`, the
/// scorer's own ending: the report, then the ruler's metric — and the WORK the
/// fold held (`Shard::work`), which a compute order's clients must agree on
/// beside the score (docs/BOARD.md §"Contribution").
pub fn board_score_json(v: &Value) -> Value {
    let req = v.get("request").cloned().unwrap_or(Value::Null);
    let metric = if let Some(metric)=req.get("metric").and_then(Value::as_str).and_then(wfsim_engine::rules::metrics::get) {
        metric
    } else if let Some(bench)=bench_named(get_str(v,"ruler","")) { bench.metric() }
    else {return crate::request::err_json("unknown score metric");};
    let duration=req.get("duration").and_then(Value::as_f64).unwrap_or(300.0);
    let acc: wfsim_engine::fight::Shard = match serde_json::from_value(v.get("acc").cloned().unwrap_or(Value::Null)) {
        Ok(s) => s,
        Err(e) => return crate::request::err_json(format!("acc: {e}")),
    };
    let out = measured(&req, &acc);
    if !out.get("ok").and_then(Value::as_bool).unwrap_or(false) {
        return out;
    }
    // …AND THE COUNTS THE WORK WAS WEIGHED FROM, which the fact factory keeps
    // beside the answer to fit what a point is worth (never credited).
    let counts: serde_json::Map<String, Value> =
        wfsim_engine::fight::WORK_COUNTERS.iter().zip(acc.work_counts()).map(|(k, n)| ((*k).to_string(), json!(n))).collect();
    let mut counts = counts;
    counts.insert("gas".to_string(), json!(acc.gas));
    json!({ "ok": true, "score": metric.of(out.get(metric.field).and_then(Value::as_f64).unwrap_or(0.0),duration), "metric": metric.id, "work": acc.work(), "counts": counts })
}
