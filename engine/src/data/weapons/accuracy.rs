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
    profiles: BTreeMap<String, SpreadTiming>,
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

#[derive(Deserialize)]
#[serde(default)]
#[derive(Default)]
struct Attack {
    aimed: Option<String>,
    hip: Option<String>,
    inverse: bool,
    fan: Option<SpreadFan>,
    charge_attenuation: Option<f32>,
    extra_floor: Option<f32>,
    multishot_cone: Option<MultishotCone>,
    first_pellet_pinpoint: bool,
    extra_spread: Option<RawExtraSpread>,
    beam_mode: Option<BeamSpreadMode>,
    resting: Option<SpreadSpec>,
}

pub(super) fn profile(id: &str) -> Option<Profile> {
    static PROFILES: OnceLock<BTreeMap<String, Profile>> = OnceLock::new();
    PROFILES
        .get_or_init(|| {
            let Some(text) = crate::data::file("geometry/accuracy-runtime.yaml") else {
                return BTreeMap::new();
            };
            let source: Source =
                serde_json::from_str(text).expect("geometry/accuracy-runtime.yaml");
            source
                .attacks
                .into_iter()
                .map(|(id, attack)| {
                    let extra_spread = attack.extra_spread.map(RawExtraSpread::into_model);
                    let runtime = [attack.aimed, attack.hip].map(|name| SpreadRuntime {
                        timing: name.map(|name| source.profiles[&name]),
                        inverse: attack.inverse,
                        fan: attack.fan,
                        charge_attenuation: attack.charge_attenuation,
                        extra_floor: attack.extra_floor,
                        multishot_cone: attack.multishot_cone,
                        first_pellet_pinpoint: attack.first_pellet_pinpoint,
                        extra_spread,
                        beam_mode: attack.beam_mode,
                    });
                    (
                        id,
                        Profile {
                            runtime,
                            resting: attack.resting,
                        },
                    )
                })
                .collect()
        })
        .get(id)
        .copied()
}
