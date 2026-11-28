// SPDX-License-Identifier: AGPL-3.0-or-later
//! Firing behavior and accuracy-property joins projected from the client.
use crate::model::{
    BeamSpreadMode, ExtraSpread, MultishotCone, SpreadFan, SpreadRuntime, SpreadSpec, SpreadTiming,
};
use serde::{Deserialize, Deserializer, de::MapAccess, de::Visitor};
use std::{fmt, sync::OnceLock};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct Profile {
    pub runtime: [SpreadRuntime; 2],
    pub resting: Option<SpreadSpec>,
}

struct BorrowedAttacks<'a>(Vec<(&'a str, Attack)>);

impl<'de: 'a, 'a> Deserialize<'de> for BorrowedAttacks<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct MapVis;
        impl<'de> Visitor<'de> for MapVis {
            type Value = Vec<(&'de str, Attack)>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("attack id to accuracy profile map")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut out = Vec::with_capacity(map.size_hint().unwrap_or(530));
                while let Some((k, v)) = map.next_entry::<&'de str, Attack>()? {
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
    profiles: Vec<SpreadTiming>,
    #[serde(borrow)]
    attacks: BorrowedAttacks<'a>,
}

#[derive(Deserialize)]
struct RawExtraSpread {
    uniform: bool,
    extra_deg: f32,
    multiplier: f32,
    rotation_offset_deg: f32,
    #[serde(default)]
    rings: Vec<[u8; 2]>,
}

impl RawExtraSpread {
    fn into_model(self) -> ExtraSpread {
        assert!(self.rings.len() <= 4, "at most four projectile rings");
        let mut rings = [(0u8, 0u8); 4];
        for (i, &[lo, hi]) in self.rings.iter().enumerate() {
            rings[i] = (lo, hi);
        }
        ExtraSpread {
            uniform: self.uniform,
            extra_deg: self.extra_deg,
            multiplier: self.multiplier,
            rotation_offset_deg: self.rotation_offset_deg,
            rings,
            ring_count: self.rings.len() as u8,
        }
    }
}

const fn default_beam_horizontal_ratio() -> f32 {
    0.5
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RawBeamSpreadMode {
    Smoothed {
        smooth_time: f32,
        #[serde(default = "default_beam_horizontal_ratio")]
        horizontal_ratio: f32,
    },
    ChildQuat {
        horizontal_ratio: f32,
        quats: [[f32; 4]; 4],
    },
    Fan {
        base_deg: f32,
        base_rays: u8,
        atten_max: f32,
        expansion_time: f32,
        reverse: bool,
        force_loop: bool,
        inner_ring: bool,
        horizontal_ratio: f32,
    },
}

impl RawBeamSpreadMode {
    fn into_model(self) -> BeamSpreadMode {
        match self {
            Self::Smoothed {
                smooth_time,
                horizontal_ratio,
            } => BeamSpreadMode::Smoothed {
                smooth_time,
                horizontal_ratio,
            },
            Self::ChildQuat {
                horizontal_ratio,
                quats,
            } => BeamSpreadMode::ChildQuat {
                horizontal_ratio,
                quats: Box::leak(Box::new(quats)),
            },
            Self::Fan {
                base_deg,
                base_rays,
                atten_max,
                expansion_time,
                reverse,
                force_loop,
                inner_ring,
                horizontal_ratio,
            } => BeamSpreadMode::Fan {
                base_deg,
                base_rays,
                atten_max,
                expansion_time,
                reverse,
                force_loop,
                inner_ring,
                horizontal_ratio,
            },
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Attack {
    aimed: Option<usize>,
    hip: Option<usize>,
    inverse: bool,
    fan: Option<SpreadFan>,
    charge_attenuation: Option<f32>,
    extra_floor: Option<f32>,
    multishot_cone: Option<MultishotCone>,
    first_pellet_pinpoint: bool,
    extra_spread: Option<RawExtraSpread>,
    beam_mode: Option<RawBeamSpreadMode>,
    resting: Option<SpreadSpec>,
}

struct Store {
    profiles: Box<[Profile]>,
    attacks: Box<[(&'static str, u16)]>,
}

pub(super) fn profile(id: &str) -> Option<Profile> {
    static STORE: OnceLock<Store> = OnceLock::new();
    let store = STORE.get_or_init(|| {
        let Some(text) = crate::data::file("geometry/accuracy-runtime.yaml") else {
            return Store {
                profiles: Box::new([]),
                attacks: Box::new([]),
            };
        };
        let source: Source<'static> =
            serde_json::from_str(text).expect("geometry/accuracy-runtime.yaml");
        let timings = source.profiles;
        let mut unique_profiles: Vec<Profile> = Vec::new();
        let mut attacks: Vec<(&'static str, u16)> = Vec::with_capacity(source.attacks.0.len());
        for (id, attack) in source.attacks.0 {
            let extra_spread = attack.extra_spread.map(RawExtraSpread::into_model);
            let beam_mode = attack.beam_mode.map(RawBeamSpreadMode::into_model);
            let runtime = [attack.aimed, attack.hip].map(|t_idx| SpreadRuntime {
                timing: t_idx.map(|i| timings[i]),
                inverse: attack.inverse,
                fan: attack.fan,
                charge_attenuation: attack.charge_attenuation,
                extra_floor: attack.extra_floor,
                multishot_cone: attack.multishot_cone,
                first_pellet_pinpoint: attack.first_pellet_pinpoint,
                extra_spread,
                beam_mode,
            });
            let candidate = Profile {
                runtime,
                resting: attack.resting,
            };
            let idx = if let Some(pos) = unique_profiles.iter().position(|p| *p == candidate) {
                pos as u16
            } else {
                let pos = unique_profiles.len() as u16;
                unique_profiles.push(candidate);
                pos
            };
            attacks.push((id, idx));
        }
        attacks.sort_unstable_by(|a, b| a.0.cmp(b.0));
        Store {
            profiles: unique_profiles.into_boxed_slice(),
            attacks: attacks.into_boxed_slice(),
        }
    });
    store
        .attacks
        .binary_search_by(|&(key, _)| key.cmp(id))
        .ok()
        .map(|i| store.profiles[store.attacks[i].1 as usize])
}
