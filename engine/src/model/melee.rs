// SPDX-License-Identifier: AGPL-3.0-or-later
//! Authored melee inputs and their animation events (docs/MELEE.md).

use super::ComboHit;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MeleeCombo {
    Hits(Vec<ComboHit>),
    Timeline(MeleeTimeline),
    Alias { reuse: String },
    Package { tree: String, context: String },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeleeTimeline {
    pub inputs: Vec<MeleeInput>,
    #[serde(default)]
    pub repeat_from_input: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeleeInput {
    pub actions: Vec<MeleeAction>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeleeAction {
    pub source: String,
    pub animation: String,
    pub duration_seconds: f64,
    #[serde(default)]
    pub charge: Option<MeleeCharge>,
    pub events: Vec<MeleeEvent>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeleeCharge {
    #[serde(default)]
    pub animation: Option<String>,
    #[serde(default)]
    pub duration_seconds: Option<f64>,
    #[serde(default)]
    pub weapon_heavy: bool,
    #[serde(default)]
    pub weapon_throw: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeleeEventKind { Sweep, Slam, Ranged, Throw, Idle }

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeleeEvent {
    pub at_seconds: f64,
    pub kind: MeleeEventKind,
    #[serde(default)]
    pub hand: Option<u32>,
    pub multiplier: f64,
    pub combo_points: f64,
    #[serde(default)]
    pub forced_procs: Vec<String>,
    #[serde(default)]
    pub all_around: bool,
    #[serde(default = "super::one")]
    pub slash_multiplier: f64,
    #[serde(default = "super::one")]
    pub impact_multiplier: f64,
    #[serde(default = "super::one")]
    pub puncture_multiplier: f64,
    #[serde(default = "super::one")]
    pub status_multiplier: f64,
    #[serde(default)]
    pub damage_type_multipliers: std::collections::BTreeMap<String, f64>,
}

/// Derived execution metadata; the authored timeline is the source.
#[derive(Debug, Clone, Copy)]
pub struct MeleeEventTiming {
    pub input: usize,
    pub action: usize,
    pub starts_input: bool,
    pub kind: MeleeEventKind,
    pub weapon_charge: bool,
    pub throw_charge: bool,
}

#[wfsim_gas_macro::metered]
impl MeleeCombo {
    pub fn compile(&self) -> Vec<ComboHit> {
        match self {
            Self::Hits(hits) => hits.clone(),
            Self::Timeline(timeline) => timeline.compile(),
            Self::Alias { .. } | Self::Package { .. } => panic!("melee reference needs its catalog context"),
        }
    }

}

#[wfsim_gas_macro::metered]
impl MeleeTimeline {
    /// Lower event gaps onto the scheduler without losing input boundaries.
    pub fn compile(&self) -> Vec<ComboHit> {
        assert!(!self.inputs.is_empty(), "melee timeline needs inputs");
        assert!(self.repeat_from_input < self.inputs.len(), "melee repeat input is out of range");
        let mut script = Vec::new();
        let mut repeat_row = 0;
        for (input, i) in self.inputs.iter().enumerate() {
            if input == self.repeat_from_input { repeat_row = script.len(); }
            let first = script.len();
            assert!(!i.actions.is_empty(), "melee input needs actions");
            for (action, a) in i.actions.iter().enumerate() {
                assert!(!a.source.is_empty() && !a.animation.is_empty(), "melee action needs its source and animation");
                assert!(a.duration_seconds.is_finite() && a.duration_seconds > 0.0, "invalid melee animation duration");
                let charge = a.charge.as_ref().map_or(0.0, |c| {
                    assert!(c.animation.as_ref().is_none_or(|a| !a.is_empty())
                        && (usize::from(c.weapon_heavy) + usize::from(c.weapon_throw) + usize::from(c.duration_seconds.is_some()) == 1)
                        && c.duration_seconds.is_none_or(|t| t.is_finite() && t >= 0.0),
                        "invalid melee charge");
                    c.duration_seconds.unwrap_or(0.0)
                });
                let mut previous = 0.0;
                let idle = MeleeEvent {
                    at_seconds: a.duration_seconds, kind: MeleeEventKind::Idle,
                    hand: None, multiplier: 0.0, combo_points: 0.0, forced_procs: Vec::new(),
                    all_around: false, slash_multiplier: 1.0, impact_multiplier: 1.0,
                    puncture_multiplier: 1.0, status_multiplier: 1.0,
                    damage_type_multipliers: Default::default(),
                };
                let events: &[MeleeEvent] = if a.events.is_empty() { std::slice::from_ref(&idle) } else { &a.events };
                for (event, e) in events.iter().enumerate() {
                    let at = e.at_seconds;
                    assert!(at.is_finite() && at >= previous && at <= a.duration_seconds,
                        "melee events must be ordered inside their animation");
                    assert!(e.multiplier.is_finite() && e.multiplier >= 0.0 && e.combo_points.is_finite() && e.combo_points >= 0.0,
                        "invalid melee event damage or combo points");
                    let delta = at - previous;
                    let recovery = if event + 1 == events.len() { a.duration_seconds - at } else { 0.0 };
                    let slam = matches!(e.kind, MeleeEventKind::Slam);
                    script.push(ComboHit {
                        multiplier: if slam { 0.0 } else { e.multiplier },
                        delay_seconds: delta + recovery,
                        hit_delay_seconds: Some(delta),
                        input_end: Some(false),
                        next_swing_index: None,
                        windup_seconds: if event == 0 { charge } else { 0.0 },
                        all_around: e.all_around,
                        hits: 1,
                        impact_multiplier: e.impact_multiplier,
                        puncture_multiplier: e.puncture_multiplier,
                        slash_multiplier: e.slash_multiplier,
                        status_multiplier: e.status_multiplier,
                        damage_type_multipliers: e.damage_type_multipliers.clone(),
                        slam_multiplier: slam.then_some(e.multiplier),
                        forced_procs: e.forced_procs.clone(),
                        forced_hits: None,
                        combo_points: e.combo_points,
                        combo_points_base: e.combo_points,
                        sends_shockwave: false,
                        event_timing: Some(MeleeEventTiming {
                            input, action, starts_input: script.len() == first, kind: e.kind,
                            weapon_charge: event == 0 && a.charge.as_ref().is_some_and(|c| c.weapon_heavy),
                            throw_charge: event == 0 && a.charge.as_ref().is_some_and(|c| c.weapon_throw),
                        }),
                    });
                    previous = at;
                }
            }
            script.last_mut().unwrap().input_end = Some(true);
        }
        script.last_mut().unwrap().next_swing_index = Some(repeat_row);
        script
    }
}
