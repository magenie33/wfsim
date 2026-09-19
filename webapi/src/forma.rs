// SPDX-License-Identifier: AGPL-3.0-or-later
//! `/api/forma/plan` and `/api/forma/optimize` (docs/INVESTMENT.md).

use serde_json::{json, Value};
use wfsim_engine::rules::capacity::Polarity;
use crate::request::{err_json, get_bool, get_str, get_u32};

/// THE FORMA PLAN for one item and any number of its configs
/// (`engine::forma::plan`, docs/INVESTMENT.md §The planner).
///
/// Cards travel as `{drain, polarity}` at the rank the page set them to, so a
/// riven or a lowered mod needs nothing from here; the capacity, the bill and
/// the layout are the engine's.
fn placed_json(l: &wfsim_engine::build::forma::Placed) -> Value {
    json!({ "slots": l.slots, "drain": l.drain, "grant": l.grant, "spare": l.spare, "moved": l.moved })
}

fn plan_json(p: &wfsim_engine::build::forma::Plan) -> Value {
    let name = |p: Option<Polarity>| p.map(|p| format!("{p:?}"));
    json!({
        "layout": {
            "main": p.layout.main.iter().map(|&x| name(x)).collect::<Vec<_>>(),
            "exilus": name(p.layout.exilus),
            "grant": name(p.layout.grant),
        },
        "regular": p.cost.regular,
        "omni": p.cost.omni,
        "umbra": p.cost.umbra,
        "rank": p.rank,
        "capacity": p.capacity,
        "exhaustive": p.exhaustive,
        "loadouts": p.loadouts.iter().map(placed_json).collect::<Vec<_>>(),
    })
}

/// What both Forma endpoints read: the item, the player's rules, and what the
/// item already carries.
struct FormaAsk {
    board: wfsim_engine::build::forma::Board,
    rules: wfsim_engine::build::forma::Rules,
    start: Option<wfsim_engine::build::forma::Start>,
}

fn forma_polarity(x: &Value) -> Result<Option<Polarity>, String> {
    match x.as_str() {
        None => Ok(None),
        Some(s) => match s.to_lowercase().as_str() {
            "omni" | "universal" => Ok(Some(Polarity::Omni)),
            "aura" => Ok(Some(Polarity::Aura)),
            p @ ("madurai" | "naramon" | "vazarin" | "zenurik" | "unairu" | "penjaga" | "umbra") => {
                Ok(Some(wfsim_engine::data::weapons::polarity(p)))
            }
            other => Err(format!("unknown polarity: {other}")),
        },
    }
}

/// A card travels as `{drain, polarity, ordered}` at the rank the page set it
/// to: `ordered` marks an element-bearing mod, whose order is part of the build.
fn forma_card(x: &Value) -> Result<Option<wfsim_engine::build::forma::Card>, String> {
    if x.is_null() {
        return Ok(None);
    }
    let polarity = forma_polarity(&x["polarity"])?.ok_or("a card needs a polarity")?;
    Ok(Some(wfsim_engine::build::forma::Card {
        drain: get_u32(x, "drain", 0),
        polarity,
        ordered: get_bool(x, "ordered", false),
    }))
}

fn forma_loadout(l: &Value) -> Result<wfsim_engine::build::forma::Loadout, String> {
    Ok(wfsim_engine::build::forma::Loadout {
        main: l["main"].as_array().map(Vec::as_slice).unwrap_or(&[]).iter().map(forma_card).collect::<Result<_, _>>()?,
        exilus: forma_card(&l["exilus"])?,
        grant: forma_card(&l["grant"])?,
    })
}

fn forma_loadouts(v: &Value) -> Result<Vec<wfsim_engine::build::forma::Loadout>, String> {
    v.as_array().map(Vec::as_slice).unwrap_or(&[]).iter().map(forma_loadout).collect()
}

fn forma_ask(v: &Value) -> Result<FormaAsk, String> {
    use wfsim_engine::build::forma::{Board, Layout, OmniUse, Rules, Start, UmbraUse};
    let board = if let Some(id) = v["weapon"].as_str() {
        Board::weapon(id).ok_or_else(|| format!("unknown weapon: {id}"))?
    } else if let Some(id) = v["warframe"].as_str() {
        Board::warframe(
            wfsim_engine::data::warframes::warframe(id).ok_or_else(|| format!("unknown Warframe: {id}"))?,
        )
    } else {
        return Err("name a weapon or a warframe".into());
    };
    let r = &v["rules"];
    let rules = Rules {
        catalyst: get_bool(r, "catalyst", true),
        reach_max_rank: get_bool(r, "reach_max_rank", true),
        grant_slot_first: get_bool(r, "grant_slot_first", true),
        omni: match get_str(r, "omni_forma", "never") {
            "never" => OmniUse::Never,
            "allowed" => OmniUse::Allowed,
            "preferred" => OmniUse::Preferred,
            other => return Err(format!("omni_forma: {other}")),
        },
        umbra: match get_str(r, "umbra_forma", "when_needed") {
            "never" => UmbraUse::Never,
            "when_needed" => UmbraUse::WhenNeeded,
            "allowed" => UmbraUse::Allowed,
            other => return Err(format!("umbra_forma: {other}")),
        },
        forma_limit: r["forma_limit"].as_u64().map(|n| n as u32),
        fixed_order: get_bool(r, "fixed_order", false),
    };
    let start = match &v["start"] {
        Value::Null => None,
        s => {
            let x = &s["layout"];
            Some(Start {
                layout: Layout {
                    main: x["main"].as_array().map(Vec::as_slice).unwrap_or(&[]).iter().map(forma_polarity).collect::<Result<_, _>>()?,
                    exilus: forma_polarity(&x["exilus"])?,
                    grant: forma_polarity(&x["grant"])?,
                },
                forma_spent: get_u32(s, "forma_spent", 0),
                pinned: get_bool(s, "pinned", false),
            })
        }
    };
    Ok(FormaAsk { board, rules, start })
}

/// A refusal is an answer: the reason, in a shape the page words itself.
fn forma_refusal(e: &wfsim_engine::build::forma::PlanError) -> Result<Value, String> {
    use wfsim_engine::build::forma::PlanError;
    Ok(match e {
        PlanError::Invalid(_) => return Err(e.to_string()),
        PlanError::OverLimit { need, limit } => json!({ "kind": "over_limit", "need": need, "limit": limit }),
        PlanError::DoesNotFit { loadouts, umbra_off } => {
            json!({ "kind": "does_not_fit", "loadouts": loadouts, "umbra_off": umbra_off })
        }
        PlanError::CannotShare => json!({ "kind": "cannot_share" }),
    })
}

/// THE FORMA PLAN for one item and any number of its configs
/// (`engine::forma::plan`, docs/INVESTMENT.md §The planner).
///
/// Cards travel at the rank the page set them to, so a riven or a lowered mod
/// needs nothing from here; the capacity, the bill and the layout are the
/// engine's.
pub fn forma_plan_json(v: &Value) -> Value {
    use wfsim_engine::build::forma::{self, PlanError};
    let run = || -> Result<Value, String> {
        let ask = forma_ask(v)?;
        let loadouts = forma_loadouts(&v["loadouts"])?;
        match forma::plan(&ask.board, &loadouts, ask.rules, ask.start.as_ref()) {
            Ok(p) => {
                let mut out = plan_json(&p);
                out["ok"] = json!(true);
                out["fits"] = json!(true);
                Ok(out)
            }
            Err(e) => {
                let reason = forma_refusal(&e)?;
                // …AND THE OPEN BUILD'S NEAREST MISS when it is the one that
                // cannot fit, so the page can show how far over it is.
                let near = match &e {
                    PlanError::DoesNotFit { loadouts: bad, .. } if bad.contains(&0) => {
                        forma::closest(&ask.board, &loadouts[..1], ask.rules, ask.start.as_ref()).map(|p| plan_json(&p))
                    }
                    _ => None,
                };
                Ok(json!({
                    "ok": true, "fits": false, "reason": reason, "message": e.to_string(), "closest": near,
                }))
            }
        }
    };
    run().unwrap_or_else(err_json)
}

/// THE PLAN'S OPTIMIZER (`engine::forma::optimize`): the Forma-to-coverage
/// curve over groups of builds, with the page's own configs as hard loadouts.
/// A group is `{builds: [{loadout, ratio}]}`; `floor` drops what is under it.
pub fn forma_optimize_json(v: &Value) -> Value {
    use wfsim_engine::build::forma::{self, Group, GroupBuild};
    let run = || -> Result<Value, String> {
        let ask = forma_ask(v)?;
        let hard = forma_loadouts(&v["hard"])?;
        let mut groups = Vec::new();
        for g in v["groups"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
            let mut builds = Vec::new();
            for b in g["builds"].as_array().map(Vec::as_slice).unwrap_or(&[]) {
                builds.push(GroupBuild { loadout: forma_loadout(&b["loadout"])?, ratio: b["ratio"].as_f64().unwrap_or(0.0) });
            }
            groups.push(Group { builds });
        }
        let floor = v["floor"].as_f64().unwrap_or(0.0);
        match forma::optimize(&ask.board, &hard, &groups, floor, ask.rules, ask.start.as_ref()) {
            Ok(cov) => Ok(json!({
                "ok": true,
                "fits": true,
                "exhaustive": cov.exhaustive,
                "curve": cov.curve.iter().map(|p| json!({
                    "plan": plan_json(&p.plan),
                    "worst": p.worst,
                    "picks": p.picks.iter().map(|x| x.as_ref().map(|x| json!({
                        "build": x.build, "ratio": x.ratio, "placed": placed_json(&x.placed),
                    }))).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            })),
            Err(e) => Ok(json!({ "ok": true, "fits": false, "reason": forma_refusal(&e)?, "message": e.to_string() })),
        }
    };
    run().unwrap_or_else(err_json)
}

#[cfg(test)]
mod forma_plan_tests {
    use super::*;

    /// The wire carries the plan both ways, for a weapon and for a frame, and a
    /// refusal is an answer with a reason.
    #[test]
    fn the_plan_travels_for_a_weapon_and_a_frame() {
        let heavy = json!({ "drain": 14, "polarity": "Madurai" });
        let w = forma_plan_json(&json!({
            "weapon": "torid",
            "loadouts": [{ "main": [heavy, heavy, heavy, heavy, heavy, heavy, null, null] }],
        }));
        assert_eq!(w["ok"], true, "{w}");
        assert_eq!(w["capacity"], 60);
        assert!(w["loadouts"][0]["spare"].as_u64().is_some());
        assert_eq!(w["loadouts"][0]["slots"][6], Value::Null);

        let aura = json!({ "drain": 7, "polarity": "madurai" });
        let f = forma_plan_json(&json!({
            "warframe": "valkyr",
            "rules": { "umbra_forma": "never" },
            "loadouts": [{ "main": [heavy, heavy], "grant": aura }],
        }));
        assert_eq!(f["ok"], true, "{f}");
        assert_eq!(f["regular"].as_u64().unwrap() + f["umbra"].as_u64().unwrap(), 0);
        assert_eq!(f["loadouts"][0]["grant"], 14, "Valkyr's aura slot is Madurai");

        let no = forma_plan_json(&json!({
            "weapon": "torid",
            "rules": { "forma_limit": 1 },
            "loadouts": [{ "main": [heavy, heavy, heavy, heavy, heavy, heavy, heavy, heavy] }],
        }));
        assert_eq!(no["fits"], false, "{no}");
        assert_eq!(no["reason"]["kind"], "over_limit");

        let light = json!({ "drain": 6, "polarity": "Madurai" });
        let cov = forma_optimize_json(&json!({
            "weapon": "torid",
            "rules": { "reach_max_rank": false },
            "groups": [
                { "builds": [
                    { "loadout": { "main": [heavy, heavy, heavy, heavy, heavy, heavy] }, "ratio": 1.0 },
                    { "loadout": { "main": [light, light] }, "ratio": 0.4 },
                ] },
            ],
        }));
        assert_eq!(cov["fits"], true, "{cov}");
        let curve = cov["curve"].as_array().unwrap();
        assert_eq!(curve.first().unwrap()["worst"], 0.4, "{cov}");
        assert_eq!(curve.last().unwrap()["worst"], 1.0, "{cov}");
        assert_eq!(curve.last().unwrap()["picks"][0]["build"], 0, "{cov}");
    }
}
