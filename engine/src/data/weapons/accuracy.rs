// SPDX-License-Identifier: AGPL-3.0-or-later
//! Firing behavior and accuracy-property joins projected from the client.
use crate::model::{
    BeamSpreadMode, ExtraSpread, MultishotCone, SpreadFan, SpreadRuntime, SpreadSpec, SpreadTiming,
};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Clone, Copy)]
pub(super) struct Profile {
    pub runtime: [SpreadRuntime; 2],
    pub resting: Option<SpreadSpec>,
}

#[derive(Deserialize)]
struct Source {
    profiles: Vec<SpreadTiming>,
    attacks: BTreeMap<String, Attack>,
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

#[derive(Deserialize)]
#[serde(default)]
#[derive(Default)]
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

type ProfileTable = Box<[(Box<str>, Profile)]>;

pub(super) fn profile(id: &str) -> Option<Profile> {
    static PROFILES: OnceLock<ProfileTable> = OnceLock::new();
    let table = PROFILES.get_or_init(|| {
        let Some(text) = crate::data::file("geometry/accuracy-runtime.yaml") else {
            return Box::new([]);
        };
        let source: Source = serde_json::from_str(text).expect("geometry/accuracy-runtime.yaml");
        let mut rows: Vec<(Box<str>, Profile)> = source
            .attacks
            .into_iter()
            .map(|(id, attack)| {
                let extra_spread = attack.extra_spread.map(RawExtraSpread::into_model);
                let beam_mode = attack.beam_mode.map(RawBeamSpreadMode::into_model);
                let runtime = [attack.aimed, attack.hip].map(|idx| SpreadRuntime {
                    timing: idx.map(|idx| source.profiles[idx]),
                    inverse: attack.inverse,
                    fan: attack.fan,
                    charge_attenuation: attack.charge_attenuation,
                    extra_floor: attack.extra_floor,
                    multishot_cone: attack.multishot_cone,
                    first_pellet_pinpoint: attack.first_pellet_pinpoint,
                    extra_spread,
                    beam_mode,
                });
                (
                    id.into_boxed_str(),
                    Profile {
                        runtime,
                        resting: attack.resting,
                    },
                )
            })
            .collect();
        rows.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        rows.into_boxed_slice()
    });
    table
        .binary_search_by(|(key, _)| key.as_ref().cmp(id))
        .ok()
        .map(|i| table[i].1)
}
