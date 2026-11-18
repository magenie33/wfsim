// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json::{json, Value};
use wfsim_engine::{
    fight,
    record::{Kind, Record},
    scene::Scene,
};
#[path = "../../tests/fixtures/geometry/scene.rs"]
mod fixture;

fn request(weapon: &str, rows: usize, aligned: bool) -> Value {
    let mut scene = fixture::scene(1, rows, false, false, false);
    if aligned {
        align(&mut scene, rows * rows);
    }
    json!({"weapon":weapon,"mode":"base","mods":[],"enemy":"corrupted_heavy_gunner","level":225,
        "duration":2.,"runs":2,"headshot_pct":100,"arcane":"none","seed":7,"infinite_ammo":true,
        "formation":fixture::formation(&scene),"scene_3d":scene})
}

fn align(scene: &mut Value, count: usize) {
    let s = Scene::new(serde_json::from_value(scene.clone()).unwrap(), count).unwrap();
    let f = s.trace(
        true,
        0.,
        0.,
        Some(100.),
        None,
        0.,
        false,
        0.,
        100.,
        &vec![true; count],
    );
    let at = s.camera().along(f.direction, 3.);
    for (axis, i) in [3, 7, 11].iter().enumerate() {
        scene["player"]["bones"][48][i] = json!(at.0[axis]);
    }
}

fn params(v: &Value) -> fight::FightParams {
    crate::simulate::ready(v, false)
        .unwrap_or_else(|e| panic!("{e}"))
        .params
}
fn record(p: &fight::FightParams) -> Record {
    fight::record(p, 7, 0., 10., 20000, 0)
}
fn direct(rec: &Record) -> Vec<(u16, bool)> {
    rec.events()
        .iter()
        .filter_map(|e| match (&e.kind, e.subject) {
            (Kind::Damage(d), Some(i)) if !d.radial && d.part.is_some() => Some((i, d.head)),
            _ => None,
        })
        .collect()
}

#[test]
fn head_intent_does_not_override_homing_contact_or_gauge() {
    let mut p = params(&request("akarius_prime", 1, true));
    p.spread = None;
    let homing = record(&p);
    p.homing = None;
    let straight = record(&p);
    assert!(!direct(&homing).is_empty());
    assert!(direct(&homing).iter().all(|(_, head)| !*head));
    assert!(direct(&straight).iter().all(|(_, head)| *head));
    let mut cycle = request("laetum", 1, false);
    cycle["mode"] = json!("cycle");
    cycle["duration"] = json!(15.);
    let low = record(&params(&cycle));
    assert!(direct(&low).iter().all(|(_, head)| !*head));
    assert!(!low
        .events()
        .iter()
        .any(|e| matches!(e.kind, Kind::TransformEnd { transmuted: true })));
    align(&mut cycle["scene_3d"], 1);
    let high = record(&params(&cycle));
    assert!(direct(&high).iter().any(|(_, head)| *head));
    assert!(
        high.events()
            .iter()
            .any(|e| matches!(e.kind, Kind::TransformEnd { transmuted: true })),
        "actual head contacts must fill the gauge"
    );
}

#[test]
fn a_penetrated_enemy_uses_its_own_collision_part_and_damage_ledger() {
    let mut v = request("laetum", 1, true);
    let mut rear = v["scene_3d"]["enemies"][0].clone();
    rear["world"][11] = json!(8.);
    rear["world"][7] = json!(0.9);
    v["scene_3d"]["enemies"].as_array_mut().unwrap().push(rear);
    v["scene_3d"]["placement"] = json!({"kind":"supplied"});
    v["formation"] = fixture::formation(&v["scene_3d"]);
    let mut p = params(&v);
    p.spread = None;
    p.punch_through_m = 10.;
    let rec = record(&p);
    let hits = direct(&rec);
    assert!(hits.contains(&(0, true)), "{hits:?}");
    assert!(hits.contains(&(1, false)), "{hits:?}");
    assert!(
        !hits.contains(&(1, true)),
        "rear body must not inherit the first head contact"
    );
    let mut rng = wfsim_engine::rules::rng::Rng::new(7);
    let result = fight::run_once(&p, &mut rng);
    assert!(result.taken.by_body().0[0] > 0. && result.taken.by_body().0[1] > 0.);
}

#[test]
fn a_miss_never_detonates_on_the_nominal_target() {
    let mut v = request("akarius_prime", 5, false);
    v["scene_3d"]["player"]["bones"][48][3] = json!(100.);
    let mut p = params(&v);
    p.homing = None;
    p.spread = None;
    let rec = record(&p);
    assert!(rec
        .events()
        .iter()
        .any(|e| matches!(e.kind, Kind::Strike { .. })));
    assert!(!rec
        .events()
        .iter()
        .any(|e| matches!(e.kind, Kind::Damage(_))));
    assert!(rec
        .spatial_flights()
        .iter()
        .all(|f| f.flight.impacts.is_empty()));
}

#[test]
fn parse_refuses_an_incomplete_scene_and_projects_controller_contact() {
    let mut v = request("laetum", 5, false);
    v["scene_3d"]["placement"] = json!({"kind":"contact"});
    let f = crate::fight::parse_fight(&v).unwrap_or_else(|e| panic!("{e}"));
    assert!((f.arena.target_at.y - 0.7825).abs() < 1e-6);
    assert_eq!(f.arena.others.len(), 24);
    let foreign=crate::registry::enemies().into_iter().find(|enemy|enemy.target_params(225,true,false,
        wfsim_engine::target::TargetMode::InstantRespawn).is_ok_and(|target|target.faction!=f.arena.target.faction)).expect("a second combat faction");
    let mut mixed=v.clone();mixed["formation"][0]["enemy"]=json!(foreign.id);
    let error=match crate::simulate::ready(&mixed,false) {Ok(_)=>panic!("a mixed faction bracket must be refused"),Err(e)=>e};
    assert!(error["error"].as_str().unwrap().contains("one combat faction"),"{error}");
    v["scene_3d"]["enemies"].as_array_mut().unwrap().pop();
    assert!(crate::fight::parse_fight(&v).is_err());
}

#[test]
fn a_scene_search_winner_replays_under_the_same_models() {
    let mut v = request("laetum", 5, true);
    v["mods"] = json!({"hornet_strike":"search","seeker":"search","pistol_acuity":"search"});
    v["strategy"] = json!("quick");
    v["modes"] = json!({"base":"fixed"});
    v["arcane"] = json!(["none"]);
    v["build_min"] = json!(1);
    v["build_size"] = json!(1);
    v["candidate_runs"] = json!(2);
    v["finalists"] = json!(2);
    let plan = crate::parse_optimize(&v).unwrap_or_else(|e| panic!("{e}"));
    let out = crate::run_optimize(
        plan,
        &wfsim_optimizer::FunnelState::default(),
        |_, _| {},
        None,
    );
    assert_eq!(out["ok"], true, "{out}");
    let rows = out["results"].as_array().unwrap();
    assert!(!rows.is_empty());
    for row in rows {
        assert_eq!(row["replay"]["scene_3d"], v["scene_3d"]);
        let replay = crate::simulate::simulate_json(&row["replay"]);
        assert_eq!(row["kill_progress"], replay["score"]);
        assert_eq!(row["dps"], replay["dps"]);
    }
}
