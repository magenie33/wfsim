// SPDX-License-Identifier: AGPL-3.0-or-later
//! Supplied current models and state for the opt-in 3D fight (docs/GEOMETRY.md).
use crate::rules::geometry::{Affine, Proxy, Vec3};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene3d {
    pub player: PlayerModel,
    /// Same order as the arena: aimed enemy first, then its formation.
    pub enemies: Vec<EnemyModel>,
    pub frame_seconds: f64,
    /// Camera position is supplied relative to this posed player bone.
    pub camera: Mount,
    /// Native Avatar+4b0 input, kept distinct from an assumed eye or head.
    pub reference_point: Vec3,
    /// Index of the visible head/body proxy selected by perfect reticle control.
    pub head_proxy: usize,
    pub body_proxy: usize,
    pub maximum_flight_seconds: f64,
    pub placement: Placement,
    pub preselected_target: Option<usize>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Placement {
    Supplied,
    Contact,
    CenterDistance { distance: f64 },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerModel {
    pub world: Affine,
    pub controller: Controller,
    pub bones: Vec<Affine>,
    pub muzzle: Mount,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mount {
    pub bone: usize,
    pub local: Affine,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Controller {
    pub height: f32,
    pub radius: f32,
    pub skin: f32,
    pub scale: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnemyModel {
    pub world: Affine,
    pub controller: Controller,
    /// Posed proxies in avatar-local coordinates. World scale is applied once.
    pub proxies: Vec<Proxy>,
    pub steering_center: Vec3,
    pub candidate_anchor: Vec3,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Homing {
    pub collision_radius: f32,
    pub initial_speed: f32,
    pub maximum_speed: f32,
    pub acceleration: f32,
    pub turning_seconds: f32,
    pub seek_distance: f32,
    pub seek_cone_deg: f32,
    pub minimum_flight_before_seek: f32,
}
