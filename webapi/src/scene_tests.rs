// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json::{json, Value};
use wfsim_engine::{
    fight,
    record::{Kind, Record},
    scene::Scene,
};
#[path = "../../tests/fixtures/geometry/scene.rs"]
mod fixture;
#[path = "../../tests/fixtures/geometry/equipped.rs"]
mod equipped;

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
        false,
        0.,
        100.,
        &vec![true; count],
        None,
    );
    let view = f.aim_target.minus(f.camera).normalized().unwrap();
    let at = s.camera().along(view, 3.);
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
    cycle["headshot_pct"] = json!(0);
    let low = record(&params(&cycle));
    assert!(direct(&low).iter().all(|(_, head)| !*head));
    assert!(!low
        .events()
        .iter()
        .any(|e| matches!(e.kind, Kind::TransformEnd { transmuted: true })));
    cycle["gauge_charging_headshots"] = json!(true);
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
fn a_delayed_incarnon_hit_cannot_charge_the_next_cycle() {
    let mut input = request("laetum", 1, false);
    input["mode"] = json!("cycle");
    input["duration"] = json!(18.);
    input["scene_3d"]["placement"] = json!({"kind":"center_distance","distance":10.});
    let mut p = params(&input);
    // Put a head hit from the outgoing magazine inside the return animation.
    p.spread = None;
    p.projectile_speed_mps = Some(20.);
    p.magazine_size = 4.;
    p.multishot = 12.;
    let base = &mut p.cycle.as_mut().unwrap().base_form;
    base.spread = None;
    base.multishot = 6.;

    for spatial in [true, false] {
        if !spatial {
            p.scene_3d = None;
        }
        let rec = fight::record(&p, 7, 0., 18., 20000, 0);
        let events = rec.events();
        let origin = |cause| {
            events.iter().find(|e| Some(e.id) == cause && matches!(e.kind, Kind::Strike { .. }))
        };
        let return_start = events.iter().find(|e| {
            matches!(e.kind, Kind::TransformStart { into_transmuted: false, .. })
        }).expect("the first magazine runs out");
        let return_end = events.iter().find(|e| {
            matches!(e.kind, Kind::TransformEnd { transmuted: false })
        }).expect("the first return completes");
        assert!(events.iter().any(|e| {
            return_start.t < e.t && e.t < return_end.t
                && matches!(&e.kind, Kind::Damage(d) if d.head && !d.radial)
                && origin(e.cause).is_some_and(|shot| shot.weapon.transmuted)
        }), "an in-flight Incarnon round still hits during the return, spatial={spatial}");
        let next_entry = events.iter().find(|e| {
            e.t > return_start.t
                && matches!(e.kind, Kind::TransformStart { into_transmuted: true, .. })
        }).expect("the base shots fill the next gauge");
        let shot = origin(next_entry.cause).expect("the hit that fills the next gauge");
        assert!(!shot.weapon.transmuted && shot.t >= return_end.t,
            "only a base shot after the return can fill the gauge, spatial={spatial}");
    }
}

#[test]
fn laetum_cycle_switches_between_its_equipped_muzzles() {
    let mut input = request("laetum", 1, false);
    input["mode"] = json!("cycle");
    input["duration"] = json!(18.);
    equipped::equip(&mut input["scene_3d"], "laetum");
    let mut p = params(&input);
    p.magazine_size = 4.;
    p.spread = None;
    let base = &mut p.cycle.as_mut().unwrap().base_form;
    base.spread = None;
    base.multishot = 6.;

    let rec = fight::record(&p, 7, 0., 18., 20000, 0);
    let mut base_muzzle = None;
    let mut incarnon_muzzle = None;
    let mut returned = false;
    for row in rec.spatial_flights() {
        let shot = rec.events().iter().find(|e| Some(e.id) == row.shot).unwrap();
        let fire = serde_json::to_value(row.flight.fire_origin).unwrap();
        let (attachment, muzzle) = if shot.weapon.transmuted {
            (1, &mut incarnon_muzzle)
        } else {
            returned |= incarnon_muzzle.is_some();
            (0, &mut base_muzzle)
        };
        assert_eq!(fire["attachment"], attachment);
        assert_eq!(*muzzle.get_or_insert(row.flight.muzzle), row.flight.muzzle);
    }
    assert!(returned, "the recorded cycle must fire again after returning to base");
    assert_ne!(base_muzzle.unwrap(), incarnon_muzzle.unwrap());

    let fire = &mut input["scene_3d"]["player"]["equipped_fire"];
    fire["round_order_by_form"]["incarnon"][0]["attachment"] = json!(2);
    assert!(crate::fight::parse_fight(&input).is_err());
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
    let mut control = params(&v);
    control.spread = None;
    assert!(direct(&record(&control)).iter().all(|&(i, _)| i == 0));
    v["mods"] = json!(["seeker"]);
    let mut p = params(&v);
    assert!(p.projectile_reverse_query && p.punch_through_m > 0.);
    p.spread = None;
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
    assert!(rec.spatial_flights().iter().any(|f| {
        let h = &f.flight.impacts;
        h.len() == 2 && h[0].material > 0. && h[0].seconds < h[1].seconds
    }));
}

#[test]
fn equipped_rounds_use_the_selected_attachment_without_rotating_the_eye_basis() {
    let mut v = request("akarius_prime", 5, false);
    v["duration"] = json!(6.);
    v["scene_3d"] = fixture::scene(1, 5, false, false, true);
    equipped::equip(&mut v["scene_3d"], "akarius_prime");
    let mut p = params(&v);
    p.spread = None;
    let first = record(&p);
    let again = record(&p);
    let flights = first.spatial_flights();
    assert!(flights.len() >= 12);
    assert_eq!(serde_json::to_value(flights).unwrap(), serde_json::to_value(again.spatial_flights()).unwrap());
    for (i, row) in flights.iter().take(12).enumerate() {
        let trace = &row.flight;
        let fire = serde_json::to_value(trace.fire_origin).unwrap();
        assert_eq!(fire["attachment"], i % 2);
        assert_eq!(fire["cursor"], (i / 2) % 5);
        assert_eq!(fire["profile"], 0);
        assert_eq!(trace.aim_target, flights[0].flight.aim_target);
        let toward = trace.aim_target.minus(trace.muzzle).normalized().unwrap();
        assert!((1. - trace.direction.normalized().unwrap().dot(toward)).abs() < 1e-12);
        assert_eq!(trace.basis_origin, flights[0].flight.basis_origin);
        assert_ne!(trace.basis_origin, trace.muzzle);
        assert_eq!(trace.muzzle.0[0].is_sign_positive(), i % 2 == 0);
    }
    assert_ne!(flights[0].flight.muzzle, flights[2].flight.muzzle);
    assert_ne!(flights[0].flight.direction, flights[1].flight.direction);
    assert_eq!(flights[0].flight.muzzle, flights[10].flight.muzzle);
}

#[test]
fn a_miss_never_detonates_on_the_nominal_target() {
    let mut v = request("akarius_prime", 5, false);
    v["scene_3d"]["player"]["bones"][48][3] = json!(100.);
    let mut p = params(&v);
    p.homing = None;
    p.spread = None;
    p.range_m = 1.;
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
fn five_meter_head_intent_uses_the_standing_player_and_the_actual_muzzle() {
    let mut v = request("akarius_prime", 5, false);
    equipped::equip(&mut v["scene_3d"], "akarius_prime");
    let equipment = v["scene_3d"]["player"]["equipped_fire"].take();
    let scene = v["scene_3d"].as_object_mut().unwrap();
    scene.insert("player".into(), json!({"equipped_fire":equipment}));
    scene.remove("camera");
    scene.remove("reference_point");
    let p = params(&v);
    let scene = p.scene_3d.as_ref().unwrap();
    assert_eq!(scene.input().player.bones.len(), 83);
    assert_eq!(scene.input().player.controller.height, 1.5);
    assert_eq!(scene.root(0).0, [0., 0., 5.]);
    assert_eq!(scene.camera().0, [0., 1.8, -3.]);

    let enemy = &scene.input().enemies[0];
    let head = enemy.proxies.iter().find(|p| p.cache_index == scene.input().head_proxy).unwrap();
    let center = enemy.world.compose(head.world).point(wfsim_engine::rules::geometry::Vec3([0.; 3]));
    let rec = record(&p);
    let flights = rec.spatial_flights();
    assert!(flights.len() > 2);
    for row in flights {
        let flight = &row.flight;
        assert_eq!(flight.head_anchor, center);
        let aim = flight.aim_target.minus(flight.camera).normalized().unwrap();
        let wanted = center.minus(flight.camera).normalized().unwrap();
        assert!((1. - aim.dot(wanted)).abs() < 1e-12);
        assert_ne!(flight.muzzle, flight.basis_origin);
        assert!(flight.fire_origin.is_some());
    }
    assert!(flights.iter().any(|row| row.flight.impacts.iter().any(|hit| hit.part != 1)));

    v["scene_3d"].as_object_mut().unwrap().remove("player");
    let without_equipment = params(&v);
    assert_eq!(without_equipment.scene_3d.as_ref().unwrap().camera().0, [0., 1.8, -3.]);
}

#[test]
fn the_source_default_preset_runs_and_records_the_five_by_five_arena() {
    let mut v = request("akarius_prime", 5, false);
    v["scene_3d"] = json!({
        "preset":"source_default",
        "placement":{"kind":"center_distance","distance":5.}
    });
    let result = crate::simulate::simulate_json(&v);
    assert_eq!(result["ok"], true, "{result}");
    v["run"] = result["run"].clone();
    let log = crate::log::log_json(&v);
    let actors = log["scene_3d"]["actors"].as_array().unwrap();
    assert_eq!(actors.len(), 25);
    assert_eq!(actors[0]["root"], json!([0.,0.,5.]));
    let flights = log["scene_3d"]["flights"].as_array().unwrap();
    assert!(flights.len() >= 2);
    let first = &flights[0]["flight"];
    let second = &flights[1]["flight"];
    assert_eq!(first["fire_origin"]["attachment"], 0);
    assert_eq!(second["fire_origin"]["attachment"], 1);
    assert_ne!(first["muzzle"], second["muzzle"]);
    assert_eq!(first["head_anchor"], second["head_anchor"]);
    assert!(flights.iter().any(|row| row["flight"]["steering_target"].is_number()));

    v["scene_3d"]["placement"] = json!({"kind":"contact"});
    let fight = crate::fight::parse_fight(&v).unwrap();
    assert!((fight.arena.target_at.y - 0.7825).abs() < 1e-6);
    assert_eq!(fight.arena.others.len(), 24);
}

#[test]
fn the_source_default_preset_poses_its_registered_enemy_and_refuses_an_unknown_profile() {
    let mut v = request("laetum", 1, false);
    v["enemy"] = json!("demolisher_devourer");
    v["target_at"] = json!([5., 0.]);
    v["scene_3d"] = json!({"preset":"source_default","enemy_yaw_deg":90.,
        "placement":{"kind":"center_distance","distance":10.}});
    let fight = crate::fight::parse_fight(&v).unwrap();
    let scene = fight.arena.scene_3d.unwrap();
    assert_eq!(scene.root(0).0, [10., 0., 0.]);
    assert_eq!(scene.camera().0, [-3., 1.8, 0.]);
    assert_eq!(scene.input().head_proxy, 11);
    assert_eq!(scene.input().enemies[0].proxies.len(), 15);
    assert!(scene.input().enemies[0].world.0[10] > 1.);

    v["enemy"] = json!("crewman");
    let error = crate::fight::parse_fight(&v).err().unwrap();
    assert!(error["error"].as_str().unwrap().contains("enemy profile for crewman"));
    v["enemy"] = json!("demolisher_devourer");
    v["weapon"] = json!("braton");
    let error = crate::fight::parse_fight(&v).err().unwrap();
    assert!(error["error"].as_str().unwrap().contains("weapon profile for braton"));
}

#[test]
fn a_scene_search_winner_replays_under_the_same_models() {
    let mut v = request("laetum", 5, true);
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"supplied"}});
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
