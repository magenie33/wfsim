// SPDX-License-Identifier: AGPL-3.0-or-later
//! Supplied current models and state for the opt-in 3D fight (docs/GEOMETRY.md).
use std::collections::BTreeMap;

use crate::rules::geometry::{Affine, Proxy, Shape, Vec3};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene3d {
    pub player: PlayerModel,
    /// Same order as the arena: aimed enemy first, then its formation.
    pub enemies: Vec<EnemyModel>,
    /// Fixed physical substep duration.
    pub frame_seconds: f64,
    /// Optional initial projectile updates, followed by one substep and publication per update.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guided_updates: Vec<GuidedUpdate>,
    /// Supplied clear line-of-sight result for acquisition queries.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clear_homing_visibility: bool,
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

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuidedUpdate {
    pub substeps: u16,
    pub publish_transform: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerModel {
    pub world: Affine,
    pub controller: Controller,
    pub bones: Vec<Affine>,
    pub muzzle: Mount,
    /// Installed weapon attachment poses and the supplied round order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equipped_fire: Option<EquippedFire>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mount {
    pub bone: usize,
    pub local: Affine,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EquippedFire {
    pub attachments: Vec<WeaponAttachment>,
    pub round_order: Vec<FireRound>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub round_order_by_form: BTreeMap<String, Vec<FireRound>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponAttachment {
    /// Mount relative to the current avatar bone.
    pub mount: Mount,
    /// Current weapon-local attachment bone cache.
    pub bones: Vec<Affine>,
    /// One ordered firing-position list per weapon mode.
    pub profiles: Vec<Vec<FirePosition>>,
    #[serde(default)]
    pub initial_cursor: usize,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirePosition {
    pub bone: Option<usize>,
    pub local: Affine,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FireRound {
    pub attachment: usize,
    pub mode: usize,
    pub refresh_per_iteration: bool,
    pub advance_after_round: bool,
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
    /// Current installed overlap shape in avatar-local coordinates, when supplied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radial_proxy: Option<RadialProxy>,
    pub steering_center: Vec3,
    pub candidate_anchor: Vec3,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RadialProxy {
    pub local: Affine,
    pub shape: Shape,
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
