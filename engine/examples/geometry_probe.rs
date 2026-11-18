// SPDX-License-Identifier: AGPL-3.0-or-later
//! Supplied-pose diagnostics; not native reticle-to-impact or homing certification.
use serde_json::{json, Value};
use wfsim_engine::rules::{geometry::*, rng::Rng};
use wfsim_engine::{
    arena::Arena,
    build::loadout,
    data,
    fight::FightParams,
    model::{StackPolicy, WeaponBase},
};

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3([
        a.0[1] * b.0[2] - a.0[2] * b.0[1],
        a.0[2] * b.0[0] - a.0[0] * b.0[2],
        a.0[0] * b.0[1] - a.0[1] * b.0[0],
    ])
}

fn proxies(enemy: &Value, distance: f64, scale: f64, facing: f64) -> Vec<Proxy> {
    let world = Affine([
        scale * facing,
        0.,
        0.,
        0.,
        0.,
        scale,
        0.,
        0.,
        0.,
        0.,
        scale * facing,
        distance,
    ]);
    enemy["proxies"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| {
            let radius = source["shape"]["radius"].as_f64().unwrap();
            Proxy {
                cache_index: source["cache_index"].as_u64().unwrap() as usize,
                bone_token: source["bone_token"].as_u64().unwrap_or(u64::from(u32::MAX)) as u32,
                part: source["part"].as_u64().unwrap() as u8,
                shape: if source["shape"]["type"] == 1 {
                    Shape::Capsule {
                        radius,
                        height: source["shape"]["capsule_height"].as_f64().unwrap(),
                    }
                } else {
                    Shape::Sphere { radius }
                },
                world: world.compose(Affine(std::array::from_fn(|i| {
                    source["source_default_rows"][i].as_f64().unwrap()
                }))),
                ray_enabled: true,
            }
        })
        .collect()
}

fn main() {
    let destination = std::env::args().nth(1).expect("geometry_probe output.json");
    let input: Value =
        serde_json::from_str(include_str!("../src/rules/geometry/fixtures/native.json")).unwrap();
    let started = std::time::Instant::now();
    let samples = 20_000;
    let seed = 24301;
    let mut rows = Vec::new();
    for enemy in input["enemies"].as_array().unwrap() {
        for distance in [0.72, 5., 10., 30.] {
            for scale in [1., 1.6] {
                for facing in [1., -1.] {
                    let scene = proxies(enemy, distance, scale, facing);
                    let head = scene
                        .iter()
                        .find(|p| p.part == 1)
                        .unwrap()
                        .world
                        .point(Vec3([0.; 3]));
                    let origin = Vec3([0., 1.5, 0.3]);
                    let forward = head.minus(origin).normalized().unwrap();
                    let right = cross(Vec3([0., 1., 0.]), forward).normalized().unwrap();
                    let up = cross(forward, right);
                    for weapon in ["laetum", "laetum_incarnon", "akarius_prime"] {
                        let base = WeaponBase::from_data(weapon, true, &[]);
                        let pool = data::mods::pool_for_weapon(weapon);
                        for mod_id in [None, Some("magnum_force"), Some("steady_hands")] {
                            let mods: Vec<_> = mod_id
                                .map(|id| pool.iter().find(|m| m.id == id).expect("compatible mod"))
                                .into_iter()
                                .collect();
                            let panel = loadout::resolve(&base, &mods, StackPolicy::Emergent);
                            let p = FightParams::from_panel(
                                &panel,
                                &Arena::training(1.),
                                &data::arcanes::ArcaneFx::none(),
                            );
                            let mut rng = Rng::new(seed);
                            let (mut hits, mut heads, mut planar_hits) = (0, 0, 0);
                            let mut first = Vec::new();
                            for index in 0..samples {
                                let cone = rng.next_f64();
                                let azimuth = rng.next_f64();
                                let radius = rng.next_f64();
                                let (deviation, phase) =
                                    p.spread.filter(|s| !s.is_pinpoint()).map_or((0., 0.), |s| {
                                        s.sample(cone, azimuth, radius, index, 1)
                                    });
                                let (sin, cos) = libm::sincos(deviation.to_radians());
                                let (sp, cp) = libm::sincos(phase * std::f64::consts::TAU);
                                let direction = Vec3(std::array::from_fn(|i| {
                                    forward.0[i] * cos + right.0[i] * sin * cp + up.0[i] * sin * sp
                                }));
                                let ray = Ray {
                                    origin,
                                    direction,
                                    maximum: 100.,
                                    previous_best: 100.,
                                    excluded_parts: 0,
                                };
                                if let Refinement::Contact(hit) =
                                    refine(1, &scene, ray, Gate::Active)
                                {
                                    hits += 1;
                                    heads += u32::from(hit.part == 1);
                                    if first.len() < 3 {
                                        first.push(json!({"point":hit.point.0,"part":hit.part,"cache_index":hit.cache_index}));
                                    }
                                }
                                let range =
                                    (distance - wfsim_engine::rules::space::BODY_RADIUS_M).max(0.);
                                planar_hits += u32::from(
                                    wfsim_engine::rules::space::miss_distance(range, deviation)
                                        <= wfsim_engine::rules::space::BODY_RADIUS_M,
                                );
                            }
                            rows.push(json!({"enemy":enemy["path"],"weapon":weapon,"mods":mod_id.into_iter().collect::<Vec<_>>(),
                            "supplied_root_distance":distance,"supplied_yaw_deg":if facing > 0. { 0 } else { 180 },"supplied_world_scale":scale,"muzzle":origin.0,"aim_anchor":head.0,
                            "head_hits":heads,"body_hits":hits-heads,"misses":samples-hits,"planar_hits":planar_hits,
                            "first_contacts":first}));
                        }
                    }
                }
            }
        }
    }
    let output = json!({"scope":"Supplied head-directed launch and source-default pose; no camera/attachment/homing/sweep/explosion binding",
        "gameplay_headshot_probability":null,"full_3d_fight_validated":false,
        "seed":seed,"samples_per_row":samples,"query_count":rows.len() as u64 * u64::from(samples),"elapsed_seconds":started.elapsed().as_secs_f64(),"rows":rows});
    std::fs::write(destination, serde_json::to_vec_pretty(&output).unwrap()).unwrap();
}
