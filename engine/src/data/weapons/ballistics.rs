// SPDX-License-Identifier: AGPL-3.0-or-later
//! Client projectile and hitscan runtime profiles bound from `data/geometry/ballistics-runtime.yaml`.
use crate::model::{BallisticsRuntime, BounceSeek, WaveResize};
use serde::{Deserialize, Deserializer, de::MapAccess, de::Visitor};
use std::{fmt, sync::OnceLock};

struct BorrowedAttacks<'a>(Vec<(&'a str, u16)>);

impl<'de: 'a, 'a> Deserialize<'de> for BorrowedAttacks<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MapVis;
        impl<'de> Visitor<'de> for MapVis {
            type Value = Vec<(&'de str, u16)>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("attack id to profile index map")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut out = Vec::with_capacity(map.size_hint().unwrap_or(328));
                while let Some((k, v)) = map.next_entry::<&'de str, u16>()? {
                    out.push((k, v));
                }
                Ok(out)
            }
        }
        deserializer.deserialize_map(MapVis).map(BorrowedAttacks)
    }
}

#[derive(Deserialize)]
struct Source<'a> {
    profiles: Vec<RawProfile>,
    #[serde(borrow)]
    attacks: BorrowedAttacks<'a>,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawWaveResize {
    initial_radius_m: f32,
    final_radius_m: f32,
    resize_duration_seconds: f32,
    initial_height_m: f32,
    final_height_m: f32,
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawBounceSeek {
    cone_deg: f32,
    range_m: f32,
}

const fn default_true() -> bool {
    true
}

#[derive(Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct RawProfile {
    infinite_puncture_depth: bool,
    max_life_seconds: f32,
    initial_speed_mps: f32,
    max_speed_mps: Option<f32>,
    min_speed_mps: f32,
    accel_mps2: f32,
    gravity_multiplier: f32,
    gravity_delay_seconds: f32,
    arming_distance_m: f32,
    #[serde(default = "default_true")]
    explode_on_impact: bool,
    #[serde(default = "default_true")]
    explode_on_expiry: bool,
    free_avatar_pierce: bool,
    can_bounce: bool,
    max_bounces: u16,
    restitution_normal: Option<f32>,
    restitution_tangent: Option<f32>,
    wave_resize: Option<RawWaveResize>,
    bounce_seek: Option<RawBounceSeek>,
}

impl RawProfile {
    fn into_model(self) -> BallisticsRuntime {
        BallisticsRuntime {
            infinite_puncture_depth: self.infinite_puncture_depth,
            max_life_seconds: self.max_life_seconds,
            initial_speed_mps: self.initial_speed_mps,
            max_speed_mps: self.max_speed_mps.unwrap_or(self.initial_speed_mps),
            min_speed_mps: self.min_speed_mps,
            accel_mps2: self.accel_mps2,
            gravity_multiplier: self.gravity_multiplier,
            gravity_delay_seconds: self.gravity_delay_seconds,
            arming_distance_m: self.arming_distance_m,
            explode_on_impact: self.explode_on_impact,
            explode_on_expiry: self.explode_on_expiry,
            free_avatar_pierce: self.free_avatar_pierce,
            can_bounce: self.can_bounce,
            max_bounces: self.max_bounces,
            restitution_normal: self
                .restitution_normal
                .unwrap_or(if self.can_bounce { 0.25 } else { 0.0 }),
            restitution_tangent: self
                .restitution_tangent
                .unwrap_or(if self.can_bounce { 0.8 } else { 0.0 }),
            wave_resize: self.wave_resize.map(|w| WaveResize {
                initial_radius_m: w.initial_radius_m,
                final_radius_m: w.final_radius_m,
                resize_duration_seconds: w.resize_duration_seconds,
                initial_height_m: w.initial_height_m,
                final_height_m: w.final_height_m,
            }),
            bounce_seek: self.bounce_seek.map(|s| BounceSeek {
                cone_deg: s.cone_deg,
                range_m: s.range_m,
            }),
        }
    }
}

struct Store {
    profiles: Box<[BallisticsRuntime]>,
    attacks: Box<[(&'static str, u16)]>,
}

pub(super) fn profile(id: &str) -> Option<&'static BallisticsRuntime> {
    static STORE: OnceLock<Store> = OnceLock::new();
    let store = STORE.get_or_init(|| {
        let Some(text) = crate::data::file("geometry/ballistics-runtime.yaml") else {
            return Store {
                profiles: Box::new([]),
                attacks: Box::new([]),
            };
        };
        let source: Source<'static> =
            serde_json::from_str(text).expect("geometry/ballistics-runtime.yaml");
        let profiles: Box<[BallisticsRuntime]> = source
            .profiles
            .into_iter()
            .map(RawProfile::into_model)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let mut attacks = source.attacks.0;
        attacks.sort_unstable_by(|a, b| a.0.cmp(b.0));
        Store {
            profiles,
            attacks: attacks.into_boxed_slice(),
        }
    });
    store
        .attacks
        .binary_search_by(|&(key, _)| key.cmp(id))
        .ok()
        .map(|i| &store.profiles[store.attacks[i].1 as usize])
}
