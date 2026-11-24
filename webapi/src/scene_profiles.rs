// SPDX-License-Identifier: AGPL-3.0-or-later
//! Resolve a source-default arena or explicit posed models for the shared fight.
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

use serde::Deserialize;
use serde_json::{Value, json};
use wfsim_engine::{
    data,
    model::{EnemyModel, EquippedFire, Mount, Placement, PlayerModel, Scene3d},
    rules::{
        geometry::{Affine, Vec3},
        math::Portable,
        space::Vec2,
    },
    scene::Scene,
};

#[derive(Deserialize)]
struct StandingPlayer {
    player: PlayerModel,
    camera: Mount,
    reference_bone: usize,
}

#[derive(Deserialize)]
struct SourceEnemy {
    model: EnemyModel,
    head_proxy: usize,
    body_proxy: usize,
}

#[derive(Deserialize)]
struct SourceProfiles {
    weapons: BTreeMap<String, EquippedFire>,
    enemies: BTreeMap<String, SourceEnemy>,
}

type WeaponMounts = BTreeMap<String, EquippedFire>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceRequest {
    preset: Preset,
    placement: Option<Placement>,
    #[serde(default)]
    enemy_yaw_deg: f64,
    carrier: Option<CarrierPose>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CarrierPose {
    player: PlayerModel,
    camera: Mount,
    reference_point: Vec3,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Preset {
    SourceDefault,
    SourceStandard,
}

fn profiles() -> Result<&'static SourceProfiles, String> {
    static SOURCE: OnceLock<Result<SourceProfiles, String>> = OnceLock::new();
    SOURCE
        .get_or_init(|| {
            let source = data::file("geometry/source-default.yaml")
                .ok_or("missing source-default geometry")?;
            serde_json::from_str(source).map_err(|e| format!("source-default geometry: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn catalog_mounts() -> Result<&'static WeaponMounts, String> {
    static SOURCE: OnceLock<Result<WeaponMounts, String>> = OnceLock::new();
    SOURCE
        .get_or_init(|| {
            let yaml = data::file("geometry/weapon-attachments.yaml")
                .ok_or("missing catalog attachment geometry")?;
            serde_json::from_str(yaml).map_err(|e| format!("catalog attachment geometry: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn standing_gun(id: &str) -> bool {
    data::weapons::spec(id)
        .is_some_and(|w| matches!(w.weapon_type.as_str(), "primary" | "secondary" | "archgun"))
}

fn standing_melee(id: &str) -> bool {
    data::weapons::spec(id).is_some_and(|w| w.weapon_type.as_str() == "melee")
}

pub(crate) fn source_default_choices() -> Value {
    let (Ok(source), Ok(mounts)) = (profiles(), catalog_mounts()) else {
        return Value::Null;
    };
    let weapons = crate::registry::weapons().iter().filter(|w| !w.sentinel);
    let guns = weapons.clone().filter(|w| standing_gun(&w.id));
    json!({
        "weapons": weapons.clone().filter(|w| standing_gun(&w.id) || standing_melee(&w.id)).map(|w| &w.id).collect::<Vec<_>>(),
        "melee_weapons": weapons.filter(|w| standing_melee(&w.id)).map(|w| &w.id).collect::<Vec<_>>(),
        "reference_weapons": guns.filter(|w| !source.weapons.contains_key(&w.id)
            && !mounts.contains_key(&w.id)).map(|w| &w.id).collect::<Vec<_>>(),
        "carrier_weapons": crate::registry::weapons().iter()
            .filter(|w| w.sentinel && (standing_gun(&w.id) || standing_melee(&w.id)))
            .map(|w| &w.id).collect::<Vec<_>>(),
        "enemies": source.enemies.keys().collect::<Vec<_>>(),
        "enemy_controller_radii_m": source.enemies.iter().map(|(id, enemy)| {
            let c = &enemy.model.controller;
            (id, c.radius * c.scale)
        }).collect::<BTreeMap<_, _>>(),
        "player_controller_radius_m": standing(Affine::IDENTITY, None).ok().map(|(player, _, _)| {
            player.controller.radius * player.controller.scale
        }),
    })
}

fn standing(
    world: Affine,
    fire: Option<EquippedFire>,
) -> Result<(PlayerModel, Mount, Vec3), String> {
    static SOURCE: OnceLock<Result<StandingPlayer, String>> = OnceLock::new();
    let source = SOURCE
        .get_or_init(|| {
            let yaml = data::file("geometry/excalibur-standing.yaml")
                .ok_or("missing Excalibur standing geometry")?;
            serde_norway::from_str(yaml).map_err(|e| format!("Excalibur standing geometry: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)?;
    let mut player = source.player.clone();
    player.world = world;
    player.equipped_fire = fire;
    let reference = player
        .bones
        .get(source.reference_bone)
        .ok_or("missing standing aim reference")?;
    let reference_point = player.world.compose(*reference).point(Vec3([0.; 3]));
    Ok((player, source.camera.clone(), reference_point))
}

fn pose(at: Vec2, forward_x: f64, forward_z: f64, scale: f64) -> Affine {
    Affine([
        scale * forward_z,
        0.,
        scale * forward_x,
        at.x,
        0.,
        scale,
        0.,
        0.,
        -scale * forward_x,
        0.,
        scale * forward_z,
        at.y,
    ])
}

pub(crate) fn source_default<'a>(
    raw: &Value,
    weapon: &crate::registry::WeaponInfo,
    player_at: Vec2,
    aimed: (&'a str, Vec2),
    formation: impl Iterator<Item = (&'a str, Vec2)>,
) -> Result<Arc<Scene>, String> {
    let SourceRequest {
        preset,
        placement,
        enemy_yaw_deg,
        carrier,
    } = serde_json::from_value(raw.clone()).map_err(|e| format!("scene_3d: {e}"))?;
    if !enemy_yaw_deg.is_finite() {
        return Err("scene_3d enemy yaw must be finite".into());
    }
    let source = profiles()?;
    if !standing_gun(&weapon.id) && !standing_melee(&weapon.id) {
        return Err("scene_3d standing preset needs a supported weapon".into());
    }
    if weapon.sentinel && carrier.is_none() {
        return Err("scene_3d needs a posed companion carrier and its weapon mount".into());
    }
    if !weapon.sentinel && carrier.is_some() {
        return Err("scene_3d carrier is only available for companion weapons".into());
    }
    let fire = source.weapons.get(&weapon.id).or(catalog_mounts()?.get(&weapon.id));
    let enemy_profile = |id: &str| {
        source
            .enemies
            .get(id)
            .ok_or_else(|| format!("scene_3d has no source-default enemy profile for {id}"))
    };
    let first = enemy_profile(aimed.0)?;
    let player_at = carrier.as_ref().map_or(player_at, |p| Vec2::new(p.player.world.0[3], p.player.world.0[11]));
    let dx = aimed.1.x - player_at.x;
    let dz = aimed.1.y - player_at.y;
    let heading = Vec3([dx, 0., dz])
        .normalized()
        .ok_or("scene_3d needs a target direction")?;
    let [x, _, z] = heading.0;
    let (player, camera, reference_point) = match carrier {
        Some(c) => (c.player, c.camera, c.reference_point),
        None => standing(pose(player_at, x, z, 1.), fire.cloned())?,
    };
    let (sin, cos) = enemy_yaw_deg.to_radians().sin_cos_portable();
    let enemies = std::iter::once(aimed)
        .chain(formation)
        .map(|(id, at)| {
            let mut model = enemy_profile(id)?.model.clone();
            model.world = pose(
                at,
                -x * cos - z * sin,
                x * sin - z * cos,
                f64::from(model.controller.scale),
            );
            Ok(model)
        })
        .collect::<Result<Vec<_>, String>>()?;
    let count = enemies.len();
    let default_placement = if weapon.sentinel {
        Placement::Supplied
    } else if matches!(preset, Preset::SourceStandard) && standing_gun(&weapon.id) {
        Placement::CenterDistance { distance: 5. }
    } else {
        Placement::Contact
    };
    Scene::new_standing(
        Scene3d {
            player,
            enemies,
            camera,
            reference_point,
            frame_seconds: 1. / 64.,
            guided_updates: Vec::new(),
            clear_homing_visibility: true,
            head_proxy: first.head_proxy,
            body_proxy: first.body_proxy,
            maximum_flight_seconds: 2.,
            placement: placement.unwrap_or(default_placement),
            preselected_target: None,
        },
        count,
    )
    .map(Arc::new)
}

pub(crate) fn explicit(raw: &Value, count: usize, player_at: Vec2) -> Result<Arc<Scene>, String> {
    let mut input = raw.clone();
    let fields = input.as_object_mut().ok_or("scene_3d must be an object")?;
    let use_default = fields.get("player").is_none_or(|v| {
        v.as_object()
            .is_some_and(|player| player.keys().all(|key| key == "equipped_fire"))
    });
    if use_default {
        let equipment = fields
            .get("player")
            .and_then(|v| v.get("equipped_fire"))
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()
            .map_err(|e| format!("scene_3d player equipment: {e}"))?
            .flatten();
        let (player, camera, reference_point) = standing(pose(player_at, 0., 1., 1.), equipment)?;
        fields.insert("player".into(), json!(player));
        fields.entry("camera").or_insert_with(|| json!(camera));
        fields
            .entry("reference_point")
            .or_insert_with(|| json!(reference_point));
    }
    let scene = serde_json::from_value(input).map_err(|e| format!("scene_3d: {e}"))?;
    Scene::new(scene, count).map(Arc::new)
}
