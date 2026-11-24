// SPDX-License-Identifier: AGPL-3.0-or-later
//! Firing behavior and accuracy-property joins projected from the client.
use crate::model::{SpreadFan, SpreadRuntime, SpreadSpec, SpreadTiming};
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
#[serde(default)]
#[derive(Default)]
struct Attack {
    aimed: Option<String>,
    hip: Option<String>,
    inverse: bool,
    fan: Option<SpreadFan>,
    charge_attenuation: Option<f32>,
    extra_floor: Option<f32>,
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
                    let runtime = [attack.aimed, attack.hip].map(|name| SpreadRuntime {
                        timing: name.map(|name| source.profiles[&name]),
                        inverse: attack.inverse,
                        fan: attack.fan,
                        charge_attenuation: attack.charge_attenuation,
                        extra_floor: attack.extra_floor,
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
