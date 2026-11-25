// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json::{json, Value};
use wfsim_engine::{
    fight,
    record::{Kind, Record},
    rules::{geometry::Vec3, math::Portable},
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
fn standing_head_aim_centers_the_shotgun_pattern_at_contact_and_five_metres() {
    let mut v = request("hek", 1, false);
    v["aiming"] = json!(true);
    v["target_at"] = json!([0., 5.]);
    for kind in ["contact", "center_distance"] {
        v["scene_3d"] = json!({"preset":"source_default", "placement":{"kind":kind}});
        if kind == "center_distance" {
            v["scene_3d"]["placement"]["distance"] = json!(5.);
        }
        let mut p = params(&v);
        for zero_spread in [false, true] {
            if zero_spread {
                p.spread = None;
            }
            let recorded = record(&p);
            let flights = recorded.spatial_flights();
            let first_shot = flights[0].shot;
            let first: Vec<_> = flights.iter().take_while(|f| f.shot == first_shot).collect();
            assert_eq!(first.len(), 7);
            for trace in first {
                let flight = &trace.flight;
                assert!(flight.aim_target.minus(flight.head_anchor).length() < 1e-6);
                let impact = flight.impacts.first().expect("a pellet reaches the aimed enemy");
                assert_eq!(impact.body, 0);
                if kind == "center_distance" {
                    assert_eq!(impact.part, 1);
                }
            }
        }
    }
}

#[test]
fn accuracy_on_hit_narrows_only_later_emissions_in_the_standing_scene() {
    let mut v = request("hek", 1, false);
    v["aiming"] = json!(true);
    v["scene_3d"] = json!({
        "preset": "source_default",
        "placement": {"kind": "center_distance", "distance": 5.}
    });
    v["mods"] = json!(["vicious_spread"]);
    let control = record(&params(&v));
    v["mods"] = json!(["vicious_spread", "narrow_barrel"]);
    let equipped = record(&params(&v));
    let plain = control.spatial_flights();
    let narrowed = equipped.spatial_flights();
    assert!(plain.len() >= 14 && narrowed.len() >= 14);
    assert!(narrowed[..7].iter().any(|row| !row.flight.impacts.is_empty()));
    for (a, b) in plain[..7].iter().zip(&narrowed[..7]) {
        assert_eq!(a.shot, b.shot);
        assert_eq!(a.flight.direction, b.flight.direction);
    }
    let deviation = |row: &wfsim_engine::scene::FlightTrace| {
        let flight = &row.flight;
        let forward = flight.aim_target.minus(flight.muzzle).normalized().unwrap();
        let dir = flight.direction.normalized().unwrap();
        1.0 - forward.dot(dir)
    };
    assert!(plain[7..14].iter().zip(&narrowed[7..14]).all(|(a, b)| deviation(a) > deviation(b)));
}

#[test]
fn cernos_prime_launches_its_declared_fan_on_the_first_release() {
    let mut v = request("cernos_prime", 1, false);
    v["mode"] = json!("charged");
    v["aiming"] = json!(true);
    v["scene_3d"] = json!({
        "preset": "source_default",
        "placement": {"kind": "center_distance", "distance": 5.}
    });
    let result = record(&params(&v));
    let flights = result.spatial_flights();
    assert!(flights.len() >= 3);
    for (row, expected) in flights[..3].iter().zip([1.433333, 0.133333, 1.7]) {
        let flight = &row.flight;
        let forward = flight.aim_target.minus(flight.muzzle).normalized().unwrap();
        let dir = flight.direction.normalized().unwrap();
        assert!((forward.dot(dir).clamp(-1.0, 1.0).acos_portable().to_degrees() - expected).abs() < 1e-4);
    }
}

#[test]
fn coda_bubonico_emits_added_multishot_as_independent_rounds() {
    let mut v = request("coda_bubonico", 1, false);
    v["aiming"] = json!(true);
    v["scene_3d"] = json!({
        "preset": "source_default",
        "placement": {"kind": "center_distance", "distance": 5.}
    });
    let mut p = params(&v);
    p.multishot = 14.;
    let recorded = record(&p);
    let flights = recorded.spatial_flights();
    let first = flights[0].shot;
    assert_eq!(flights.iter().take_while(|row| row.shot == first).count(), 14);
    for (index, row) in flights[..14].iter().enumerate() {
        assert!(flights[..index].iter().all(|other| other.flight.direction != row.flight.direction));
    }
}

#[test]
fn a_standing_scene_resolves_the_enemy_and_reports_a_reference_muzzle() {
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
    let log = crate::log::log_json(&v);
    assert_eq!(log["ok"], true, "{log}");
    assert_eq!(log["scene_3d"]["emitter"], "player_mount");
    v["weapon"] = json!("verglas_prime");
    let error = crate::fight::parse_fight(&v).err().unwrap();
    assert!(error["error"].as_str().unwrap().contains("posed companion carrier"));
}

#[test]
fn the_standing_scene_uses_nataruks_declared_bow_and_arrow_origin() {
    let mut v = request("nataruk_perfect", 1, false);
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"center_distance","distance":5.}});
    let p = params(&v);
    let fire = p.scene_3d.as_ref().unwrap().input().player.equipped_fire.as_ref().unwrap();
    assert_eq!(fire.attachments[0].mount.bone, 17);
    assert_eq!(fire.attachments[0].bones.len(), 1);
    let log = crate::log::log_json(&v);
    assert_eq!(log["ok"], true, "{log}");
    assert_eq!(log["scene_3d"]["emitter"], "attachment");
    let flights = log["scene_3d"]["flights"].as_array().unwrap();
    assert!(!flights.is_empty());
    assert!(flights.iter().all(|row| row["flight"]["fire_origin"].is_object()));
}

#[test]
fn a_variant_inherits_the_declared_hand_mount_transform() {
    let mut v = request("hek", 1, false);
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"center_distance","distance":5.}});
    let mount = |v: &Value| {
        let p = params(v);
        p.scene_3d.unwrap().input().player.equipped_fire.as_ref().unwrap().attachments[0].mount.clone()
    };
    let base = mount(&v);
    v["weapon"] = json!("vaykor_hek");
    let variant = mount(&v);
    assert_eq!(variant.bone, 48);
    assert_eq!(variant.local.0, base.local.0);
    assert!(variant.local.0[1].abs() > 0.08);
}

#[test]
fn all_retained_acolyte_poses_resolve_standing_weak_point_hits() {
    let mut v = request("laetum", 1, false);
    v["target_at"] = json!([0., 5.]);
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"supplied"}});
    for enemy in ["angst", "mania", "misery", "torment", "violence"] {
        v["enemy"] = json!(enemy);
        let mut p = params(&v);
        p.spread = None;
        let scene = p.scene_3d.as_ref().unwrap();
        let body = &scene.input().enemies[0];
        assert_eq!(body.proxies.len(), 10);
        assert_eq!(body.controller.scale, 1.1);
        let head = &body.proxies[scene.input().head_proxy];
        assert_eq!(head.part, 1);
        assert_eq!(body.candidate_anchor, head.world.point(Vec3([0.; 3])));
        assert!(direct(&record(&p)).contains(&(0, true)), "{enemy}");
    }
}

#[test]
fn standing_melee_uses_the_height_of_the_posed_targets() {
    let mut v = request("magistar", 1, false);
    v["target_at"] = json!([0., 1.5]);
    let mut input = v["scene_3d"].clone();
    input["placement"] = json!({"kind":"supplied"});
    input["enemies"][0]["world"][11] = json!(1.5);
    v["scene_3d"] = input;
    let on_floor = record(&params(&v));
    assert!(on_floor.events().iter().any(|e| matches!(&e.kind, Kind::Damage(d) if !d.radial)));
    assert!(on_floor.spatial_flights().is_empty());
    v["scene_3d"]["enemies"][0]["world"][7] = json!(8.);
    let raised = record(&params(&v));
    assert!(!raised.events().iter().any(|e| matches!(&e.kind, Kind::Damage(_))));
}

#[test]
fn a_declared_single_iteration_pair_alternates_its_posed_mounts() {
    let mut v = request("akbolto_prime", 1, false);
    v["target_at"] = json!([0., 5.]);
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"supplied"}});
    let replay = record(&params(&v));
    let flights = replay.spatial_flights();
    let origins = flights.iter().take(4)
        .map(|row| serde_json::to_value(row.flight.fire_origin).unwrap()["attachment"].clone())
        .collect::<Vec<_>>();
    assert_eq!(origins, [json!(0), json!(1), json!(0), json!(1)]);
    assert_ne!(flights[0].flight.muzzle, flights[1].flight.muzzle);
}

#[test]
fn standing_sources_select_form_profiles_and_cycle_their_sockets() {
    let mut v = request("dera", 1, false);
    v.as_object_mut().unwrap().remove("mode");
    v["target_at"] = json!([0., 5.]);
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"supplied"}});
    for (form, attachment, profile) in [("base", 0, 0), ("incarnon", 1, 1)] {
        v["form"] = json!(form);
        let replay = record(&params(&v));
        let f = replay.spatial_flights();
        assert!(f.len() >= 3);
        for (shot, row) in f.iter().take(3).enumerate() {
            let fire = serde_json::to_value(row.flight.fire_origin).unwrap();
            assert_eq!(fire["attachment"], attachment);
            assert_eq!(fire["profile"], profile);
            assert_eq!(fire["cursor"], shot % 2);
        }
        assert_ne!(f[0].flight.muzzle, f[1].flight.muzzle);
        assert_eq!(f[0].flight.muzzle, f[2].flight.muzzle);
    }
}

#[test]
fn a_rebound_projectile_can_damage_a_body_from_expiry_without_direct_contact() {
    let mut v = request("latron_prime", 1, false);
    v.as_object_mut().unwrap().remove("mode");
    v["form"] = json!("incarnon");
    v["target_at"] = json!([0., 5.]);
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"supplied"}});
    let mut p = params(&v);
    p.spread = None;
    p.projectile_speed_mps = Some(8.);
    p.rebound_blast.as_mut().unwrap().lifetime_seconds = 0.5;
    let replay = record(&p);
    let trace = replay.spatial_flights().first().unwrap();
    let [expiry] = trace.flight.detonations.as_slice() else { panic!("one expiry") };
    assert!(expiry.expired && trace.flight.impacts.is_empty());
    assert_eq!(expiry.seconds, 0.5);
    assert!(trace.flight.radial_impacts.iter().any(|hit| hit.body == 0));
    assert!(replay.events().iter().any(|e| e.t == trace.time + expiry.seconds
        && e.subject == Some(0) && matches!(&e.kind, Kind::Damage(d) if d.radial)));
}

#[test]
fn a_single_direct_beam_uses_the_posed_contact_and_its_physical_range() {
    let mut v = request("spectra_vandal", 1, false);
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"center_distance","distance":5.}});
    let rec = record(&params(&v));
    assert!(direct(&rec).iter().any(|&(actor, _)| actor == 0));
    assert!(rec.spatial_flights().iter().any(|trace| !trace.flight.impacts.is_empty()));
    v["scene_3d"]["placement"]["distance"] = json!(100.);
    let rec = record(&params(&v));
    assert!(direct(&rec).is_empty());
    assert!(!rec.spatial_flights().is_empty());
}

#[test]
fn torids_first_spatial_cloud_tick_lands_with_contact() {
    let mut v = request("torid", 1, false);
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"center_distance","distance":5.}});
    v["duration"] = json!(10.);
    let mut p = params(&v);
    p.spread = None;
    p.fire_rate = 0.08;
    let replay = record(&p);
    let impacts: Vec<_> = replay.events().iter().filter_map(|event| match &event.kind {
        Kind::Damage(hit) if event.subject == Some(0) => Some((event.t, hit.origin)),
        _ => None,
    }).collect();
    use wfsim_engine::record::Origin;
    let contact = impacts.iter().find(|(_, origin)| *origin == Origin::Own).unwrap().0;
    let ticks: Vec<_> = impacts.iter().filter(|(_, origin)| *origin == Origin::Field).map(|(time, _)| *time).collect();
    assert_eq!(ticks.len(), 10, "{impacts:?}");
    for (i, tick) in ticks.into_iter().enumerate() {
        assert!((tick - contact - i as f64).abs() < 1e-9);
    }
}

#[test]
fn torids_spatial_sphere_seeds_separate_chains_without_paying_the_direct_hit_twice() {
    let mut v = request("torid", 1, false);
    v.as_object_mut().unwrap().remove("mode");
    v["form"] = json!("incarnon");
    v["duration"] = json!(0.1);
    v["target_at"] = json!([0., 5.]);
    v["formation"] = json!([
        {"id":"near","at":[1.2,5.]},
        {"id":"far","at":[4.,5.]},
        {"id":"outer","at":[9.,5.]},
    ]);
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"supplied"}});
    let mut p = params(&v);
    p.spread = None;
    let replay = record(&p);
    let trace = replay.spatial_flights().first().expect("Torid beam tick");
    let [direct] = trace.flight.impacts.as_slice() else { panic!("one direct contact") };
    assert_eq!(direct.body, 0);
    assert_eq!(trace.flight.beam_endpoint, Some(direct.point));
    use wfsim_engine::scene::BeamContactKind::{Chain, Radius};
    let recipients = &trace.flight.beam_contacts;
    let caught: Vec<_> = recipients.iter().filter(|s| s.kind == Radius).map(|s| s.impact.body).collect();
    assert_eq!(caught, [1]);
    let links: Vec<_> = recipients.iter().filter(|s| s.kind == Chain).collect();
    assert_eq!((links[0].impact.body, links[0].share), (2, 0.75));
    assert_eq!((links[1].impact.body, links[1].share), (3, 0.5625));
    assert!(replay.events().iter().any(|e| e.t == trace.time && e.subject == Some(3)
        && matches!(&e.kind, Kind::Damage(d) if d.origin == wfsim_engine::record::Origin::Chain)));
}

#[test]
fn boars_spatial_acquisition_and_chains_reach_nine_distinct_bodies() {
    let mut v = request("boar_prime", 1, false);
    v.as_object_mut().unwrap().remove("mode");
    v["form"] = json!("incarnon");
    v["headshot_pct"] = json!(0);
    v["target_at"] = json!([0., 10.]);
    v["formation"] = json!((0..5).flat_map(|row| {
        (-2..=2).filter(move |&column| row != 0 || column != 0)
            .map(move |column| json!({
                "id": format!("rank{row}-{column}"),
                "at": [f64::from(column) * 1.15, 10. + f64::from(row) * 1.15],
            }))
    }).collect::<Vec<_>>());
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"supplied"}});
    let replay = record(&params(&v));
    let beam = &replay.spatial_flights().first().expect("Boar beam tick").flight;
    let acquired = beam.beam_contacts.iter()
        .filter(|c| c.kind == wfsim_engine::scene::BeamContactKind::Acquired).count();
    let reached: std::collections::BTreeSet<_> = beam.impacts.iter().map(|h| h.body)
        .chain(beam.beam_contacts.iter().map(|c| c.impact.body)).collect();
    assert_eq!(acquired, 2);
    assert_eq!(reached.len(), 9);
}

#[test]
fn a_companion_uses_the_supplied_carrier_muzzle() {
    let mut v = request("verglas_prime", 1, false);
    v["target_at"] = json!([0., 5.]);
    v["scene_3d"] = json!({"preset":"source_default","carrier":{
        "player":{
            "world":[1,0,0,1.2,0,1,0,2.45,0,0,1,-0.65],
            "controller":{"height":0.3,"radius":0.18,"skin":0.005,"scale":1.0},
            "bones":[[1,0,0,0,0,1,0,0,0,0,1,0]],
            "muzzle":{"bone":0,"local":[1,0,0,0,0,1,0,0.1,0,0,1,0.15]}
        },
        "camera":{"bone":0,"local":[1,0,0,0,0,1,0,0.5,0,0,1,0]},
        "reference_point":[1.2,2.65,-0.65]
    }});
    let replay = record(&params(&v));
    let path = &replay.spatial_flights().first().expect("carrier beam").flight;
    assert!(path.muzzle.minus(wfsim_engine::rules::geometry::Vec3([1.2, 2.55, -0.5])).length() < 1e-12);
    assert_eq!(path.impacts[0].body, 0);
}

#[test]
fn stugs_pile_uses_its_actual_host_and_carries_the_impact_clock() {
    let mut v = request("stug", 1, false);
    v["target_at"] = json!([0., 8.]);
    v["formation"] = json!([{"id":"front","at":[0., 5.]}]);
    v["duration"] = json!(8.);
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"supplied"}});
    let mut p = params(&v);
    p.spread = None;
    p.fire_rate = 0.08;
    let replay = record(&p);
    let trace = replay.spatial_flights().first().expect("Stug round");
    let hit = trace.flight.impacts.first().expect("Stug attachment");
    assert_eq!(hit.body, 1);
    let ticks: Vec<_> = replay.events().iter().filter(|event| event.subject == Some(1)
        && matches!(&event.kind, Kind::Damage(d) if d.origin == wfsim_engine::record::Origin::Blob))
        .map(|e| e.t).collect();
    assert_eq!(ticks.len(), 3, "{ticks:?}");
    for (elapsed, tick) in [0., 1., p.blob.unwrap().lifespan_seconds].into_iter().zip(ticks) {
        assert!((tick - trace.time - hit.seconds - elapsed).abs() < 1e-9);
    }
}

#[test]
fn grimoire_alt_fire_deploys_a_posed_orb_and_resolves_its_fuse() {
    let mut v = request("grimoire", 1, false);
    v.as_object_mut().unwrap().remove("mode");
    v["form"] = json!("alt_fire");
    v["target_at"] = json!([0., 3.]);
    v["formation"] = json!([{"id":"rear","at":[0.,13.]}]);
    v["headshot_pct"] = json!(0);
    v["duration"] = json!(8.);
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"supplied"}});
    let mut p = params(&v);
    p.fire_rate = 0.08;
    let replay = record(&p);
    let trace = replay.spatial_flights().iter().find(|t| t.flight.entity == Some("orb")).unwrap();
    let damages: Vec<_> = replay.events().iter()
        .filter_map(|e| match &e.kind {
            Kind::Damage(d) if d.origin == wfsim_engine::record::Origin::Orb => Some((e.t, d.radial, e.subject)),
            _ => None,
        }).collect();
    assert!(trace.flight.muzzle.0[1] > 0.);
    assert!(damages.iter().any(|&(t, radial, actor)| t > trace.time && !radial && actor == Some(0)), "{damages:?}");
    assert!(damages.iter().any(|&(t, radial, actor)| t > trace.time && radial && actor == Some(1)), "{damages:?}");
}

#[test]
fn dual_toxocysts_redirect_hits_a_different_posed_body() {
    let mut v = request("dual_toxocyst", 1, false);
    v.as_object_mut().unwrap().remove("mode");
    v["form"] = json!("incarnon");
    v["target_at"] = json!([0., 5.]);
    v["formation"] = json!([{"id":"near","at":[2.,5.]},{"id":"far","at":[0.,12.]}]);
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"supplied"}});
    let replay = record(&params(&v));
    let first = &replay.spatial_flights().first().expect("Toxocyst round").flight;
    assert_eq!(first.impacts[0].body, 0);
    assert_eq!(first.redirects[0].body, 1);
    assert!(replay.events().iter().any(|e| e.subject == Some(1)
        && matches!(&e.kind, Kind::Damage(d) if d.origin == wfsim_engine::record::Origin::Ricochet)));
}

#[test]
fn irradiate_overlaps_the_near_actor_and_excludes_the_victim() {
    let mut v = request("laetum", 1, false);
    v["arcane"] = json!(["secondary_irradiate"]);
    v["target_at"] = json!([0., 5.]);
    v["formation"] = json!([{"id":"near","at":[2.,5.]},{"id":"far","at":[10.,5.]}]);
    v["duration"] = json!(10.);
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"supplied"}});
    let mut p = params(&v);
    assert!(p.arcane.echo_share > 0.);
    p.spread = None;
    p.forced_procs = vec![wfsim_engine::rules::damage::DamageType::Radiation];
    let replay = record(&p);
    let recipients: Vec<_> = replay.events().iter().filter_map(|e| match &e.kind {
        Kind::Damage(d) if d.origin == wfsim_engine::record::Origin::Echo => e.subject,
        _ => None,
    }).collect();
    assert!(!recipients.is_empty());
    assert!(recipients.iter().all(|&body| body == 1), "{recipients:?}");
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

#[test]
fn standard_placement_matches_the_weapon_class() {
    for (weapon, expected) in [("laetum", 5.0), ("magistar", 0.7825)] {
        let mut input = request(weapon, 1, false);
        input["scene_3d"] = json!({"preset": "source_standard"});
        let fight = crate::fight::parse_fight(&input).unwrap();
        let arena = fight.arena;
        let distance = arena.target_at.distance(arena.player_at);
        assert!((distance - expected).abs() < 1e-6, "{weapon}: {distance}");
    }
}

#[test]
fn deliberate_misses_fire_but_never_land() {
    let mut input = request("laetum", 1, false);
    input["scene_3d"] = json!({"preset": "source_standard"});
    input["weakpoint_aim_pct"] = json!(100);
    input["miss_pct"] = json!(100);
    let misses = record(&params(&input));
    assert!(misses.events().iter().any(|e| matches!(e.kind, Kind::Strike { .. })));
    assert!(misses.events().iter().any(|e| matches!(e.kind, Kind::Miss { reason: "scenario miss" })));
    assert!(!misses.spatial_flights().is_empty());
    assert!(misses.spatial_flights().iter().all(|f| f.flight.impacts.is_empty()));
    input["miss_pct"] = json!(0);
    let shots = record(&params(&input));
    assert!(shots.spatial_flights().iter().any(|f| !f.flight.impacts.is_empty()));
}

#[test]
fn official_rulers_resolve_3d_for_every_weapon_class() {
    use wfsim_engine::board::benchmarks;
    for ruler in benchmarks::all() {
        let scenario = serde_json::to_value(&ruler.scenario).unwrap();
        for (weapon, is_melee) in [
            ("braton", false), ("laetum", false), ("imperator", false),
            ("verglas", false), ("magistar", true), ("adarza_claws", true),
        ] {
            let mut request = benchmarks::scenario_for(&scenario, weapon);
            request["weapon"] = json!(weapon);
            assert_eq!(request["miss_pct"], 0, "{}", ruler.id);
            assert_eq!(request["weakpoint_aim_pct"], request["headshot_pct"], "{}", ruler.id);
            let arena = crate::fight::parse_fight(&request)
                .unwrap_or_else(|e| panic!("{} {weapon}: {e}", ruler.id)).arena;
            assert!(arena.scene_3d.is_some());
            let distance = arena.player_at.distance(arena.target_at);
            assert!(if is_melee { distance < 1. } else { (distance - 5.).abs() < 1e-6 },
                "{} {weapon}: {distance}", ruler.id);
            request["runs"] = json!(1);
            request["duration"] = json!(1);
            let outcome = crate::simulate::simulate_json(&request);
            assert_eq!(outcome["ok"], true, "{} {weapon}: {}", ruler.id, outcome["error"]);
        }
    }
}

#[test]
fn standard_scene_replays_an_additional_wielder_with_the_reference_mount() {
    let mut input = request("laetum", 1, false);
    input["scene_3d"] = json!({"preset": "source_standard"});
    input["also_acting"] = json!([{"weapon": "torid", "mode": "base", "mods": []}]);
    let p = params(&input);
    assert!(p.scene_3d.as_ref().unwrap().input().player.equipped_fire.is_some());
    assert!(p.also_acting[0].scene_3d.as_ref().unwrap().input().player.equipped_fire.is_none());
    assert_eq!(crate::simulate::simulate_json(&input)["ok"], true);
    assert_eq!(crate::log::log_json(&input)["ok"], true);
}

#[test]
fn direct_beam_without_special_geometry_restarts_at_each_spawn() {
    let mut v = request("spectra_vandal", 1, false);
    v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"center_distance","distance":5.}});
    let mut p = params(&v);
    p.foe.mode = wfsim_engine::target::TargetMode::InstantRespawn;
    p.foe.eximus = false;
    p.foe.steel_path = false;
    p.foe.level = p.foe.base_level;
    p.foe.base_health = 0.01;
    p.foe.base_overguard = 0.;
    p.foe.base_armor = 0.;
    p.foe.base_shield = 0.;
    p.status_chance = 0.;
    p.spread = None;
    p.base_crit_chance = 0.;
    p.unmodded_crit_chance = 0.;
    p.critical_inputs = None;
    use wfsim_engine::record::{Factor, Layer, Origin};
    let factors = |p: &fight::FightParams| {
        record(p).events().iter().filter_map(|e| match &e.kind {
            Kind::Damage(d) if d.origin == Origin::Own => Some(d.layers.iter().find_map(|l| match l {
                Layer::Mul { factor: Factor::BeamRamp, value, .. } => Some(*value),
                _ => None,
            }).unwrap_or(1.)),
            _ => None,
        }).collect::<Vec<_>>()
    };
    let replay = record(&p);
    assert!(replay.events().iter().filter(|e| matches!(&e.kind, Kind::Damage(d) if d.killed)).count() > 5);
    let per_target = factors(&p);
    assert!(per_target.len() > 5);
    assert!(per_target.iter().all(|v| (*v - 0.2).abs() < 1e-9), "{per_target:?}");
    p.beam_ramp.as_mut().unwrap().per_target = false;
    let global = factors(&p);
    assert!(global.iter().any(|v| *v > 0.9), "{global:?}");
}

#[test]
fn athodai_beam_bursts_use_target_attenuation() {
    use wfsim_engine::record::{Factor, Layer, Origin};
    for id in ["athodai", "athodai_prime"] {
        let mut v = request(id, 1, false);
        v.as_object_mut().unwrap().remove("mode");
        v["form"] = json!("alt_fire");
        v["scene_3d"] = json!({"preset":"source_default","placement":{"kind":"center_distance","distance":5.}});
        let mut p = params(&v);
        assert!(!p.continuous && p.beam_ramp.unwrap().per_target);
        p.spread = None;
        let replay = record(&p);
        let hits = replay.events().iter().filter_map(|e| match &e.kind {
            Kind::Damage(d) if d.origin == Origin::Own => Some(d.layers.iter().find_map(|l| match l {
                Layer::Mul { factor: Factor::BeamRamp, value, .. } => Some(*value),
                _ => None,
            }).unwrap_or(1.)),
            _ => None,
        }).collect::<Vec<_>>();
        assert!(hits.len() > 5, "{id}: {hits:?}");
        assert!((hits[0] - 0.2).abs() < 1e-9, "{id}: {hits:?}");
        assert!(hits.iter().any(|v| *v > 0.9), "{id}: {hits:?}");
    }
}
