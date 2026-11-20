// SPDX-License-Identifier: AGPL-3.0-or-later
use super::{uniform_scale, Affine, Scene, Vec3};
use crate::model::{EquippedFire, FireRound, Mount, WeaponAttachment};

#[derive(Debug, Clone)]
pub(super) struct PreparedFire {
    attachments: Vec<PreparedAttachment>,
    round_order: Vec<FireRound>,
}

#[derive(Debug, Clone)]
pub(super) struct PreparedAttachment {
    world: Affine,
    profiles: Vec<Vec<Affine>>,
    initial_cursor: usize,
}

#[derive(Default)]
pub(crate) struct FireState {
    round: usize,
    cursors: Vec<usize>,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct FireOrigin {
    #[serde(skip)]
    pub(super) position: Vec3,
    attachment: usize,
    profile: Option<usize>,
    cursor: usize,
}

#[wfsim_gas_macro::metered]
impl PreparedAttachment {
    pub(super) fn new(world: Affine, pose: &WeaponAttachment) -> Result<Self, String> {
        uniform_scale(world)?;
        let profiles = pose
            .profiles
            .iter()
            .map(|profile| {
                profile
                    .iter()
                    .map(|p| {
                        let bone = p
                            .bone
                            .and_then(|i| pose.bones.get(i).copied())
                            .unwrap_or(Affine::IDENTITY);
                        uniform_scale(bone)?;
                        uniform_scale(p.local)?;
                        let firing_transform = world.compose(bone).compose(p.local);
                        uniform_scale(firing_transform)?;
                        Ok(firing_transform)
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            world,
            profiles,
            initial_cursor: pose.initial_cursor,
        })
    }

    // Native 56CDE0 falls back to profile zero. An empty array uses attachment world.
    pub(super) fn profile(&self, mode: usize) -> Option<usize> {
        (!self.profiles.is_empty()).then_some(if mode < self.profiles.len() { mode } else { 0 })
    }

    pub(super) fn read(&self, profile: Option<usize>, cursor: usize) -> Affine {
        let Some(points) = profile.map(|i| &self.profiles[i]).filter(|p| !p.is_empty()) else {
            return self.world;
        };
        points[cursor % points.len()]
    }

    // Native DE85D0 advances only a profile with two or more cycling entries.
    pub(super) fn advance(&self, profile: Option<usize>, cursor: &mut usize) {
        if let Some(points) = profile.map(|i| &self.profiles[i]).filter(|p| p.len() > 1) {
            *cursor = (*cursor % points.len() + 1) % points.len();
        }
    }
}

#[wfsim_gas_macro::metered]
impl PreparedFire {
    pub(super) fn new(
        equipped: &EquippedFire,
        mount: impl Fn(&Mount) -> Result<Affine, String>,
    ) -> Result<Self, String> {
        if equipped.round_order.is_empty()
            || equipped
                .round_order
                .iter()
                .any(|round| round.attachment >= equipped.attachments.len())
        {
            return Err("scene_3d firing order needs valid equipped attachments".into());
        }
        let attachments = equipped
            .attachments
            .iter()
            .map(|a| {
                uniform_scale(a.mount.local)?;
                PreparedAttachment::new(mount(&a.mount)?, a)
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Self {
            attachments,
            round_order: equipped.round_order.clone(),
        })
    }
}

#[wfsim_gas_macro::metered]
impl Scene {
    pub(crate) fn fire_origins(
        &self,
        state: &mut FireState,
        groups: &[u32],
    ) -> Option<Vec<Option<FireOrigin>>> {
        let equipped = self.equipped_fire.as_ref()?;
        if state.cursors.is_empty() {
            state.cursors = equipped
                .attachments
                .iter()
                .map(|a| a.initial_cursor)
                .collect();
        }
        let round = equipped.round_order[state.round];
        state.round = (state.round + 1) % equipped.round_order.len();
        let attachment = &equipped.attachments[round.attachment];
        let cursor = &mut state.cursors[round.attachment];
        let profile = attachment.profile(round.mode);
        let mut emitted = false;
        let origins = groups
            .iter()
            .map(|&copies| {
                if copies == 0 {
                    return None;
                }
                if emitted && round.refresh_per_iteration {
                    attachment.advance(profile, cursor);
                }
                emitted = true;
                Some(FireOrigin {
                    position: attachment.read(profile, *cursor).point(Vec3([0.; 3])),
                    attachment: round.attachment,
                    profile,
                    cursor: *cursor,
                })
            })
            .collect();
        if emitted && round.advance_after_round {
            attachment.advance(profile, cursor);
        }
        Some(origins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FirePosition;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Attachment {
        id: String,
        bones: Vec<Affine>,
        profiles: Vec<Vec<FirePosition>>,
    }

    #[derive(Deserialize)]
    struct Case {
        attachment: String,
        world: Affine,
        mode: usize,
        profile: Option<usize>,
        cursor: usize,
        next_cursor: usize,
        expected: Affine,
    }

    #[derive(Deserialize)]
    struct Fixture {
        attachments: Vec<Attachment>,
        cases: Vec<Case>,
    }

    #[test]
    fn native_weapon_attachment_reads_and_advances_match_all_source_controls() {
        let fixture: Fixture = serde_json::from_str(include_str!(
            "../rules/geometry/fixtures/native-attachments.json"
        ))
        .unwrap();
        assert_eq!(fixture.cases.len(), 108);
        for (index, case) in fixture.cases.iter().enumerate() {
            let source = fixture
                .attachments
                .iter()
                .find(|a| a.id == case.attachment)
                .unwrap();
            let pose = WeaponAttachment {
                mount: Mount {
                    bone: 0,
                    local: Affine::IDENTITY,
                },
                bones: source.bones.clone(),
                profiles: source.profiles.clone(),
                initial_cursor: case.cursor,
            };
            let attachment = PreparedAttachment::new(case.world, &pose).unwrap();
            let profile = attachment.profile(case.mode);
            assert_eq!(profile, case.profile, "profile in case {index}");
            let actual = attachment.read(profile, case.cursor);
            for (i, (a, b)) in actual.0.iter().zip(case.expected.0).enumerate() {
                assert!(
                    (a - b).abs() < 0.000003,
                    "case {index}, matrix cell {i}: {a} vs {b}"
                );
            }
            let mut cursor = case.cursor;
            attachment.advance(profile, &mut cursor);
            assert_eq!(cursor, case.next_cursor, "advance in case {index}");
        }
    }
}
