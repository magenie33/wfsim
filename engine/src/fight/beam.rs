
/// What a KILL gives the weapon that made it, as a fraction of the enemy's
/// affinity. VERBATIM (wiki Affinity): "Kill with weapons: Half Affinity goes
/// to the Warframe and half to the killing weapon."
///
/// The general 25/75 split on that page is for SHARED affinity — orbs, ally
/// kills, ability casts — and is not this. A weapon that killed something takes
/// half, whatever else is equipped.
pub(super) const WEAPON_AFFINITY_SHARE: f64 = 0.5;
pub(super) use crate::model::{BEAM_DECAY_DELAY, BEAM_DECAY_SECONDS, BEAM_RAMP_SECONDS};

/// Progress along a continuous weapon's damage ramp: 0 = the 20% floor, 1 =
/// full damage. Advanced by holding fire on a target and decayed by stopping.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct BeamRamp {
    pub(super) progress: f64,
    pub(super) last_tick: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct TargetRamp {
    generation: Option<u32>,
    clock: BeamRamp,
    factor: f64,
}

/// Inactive-behavior beam ramp state, frozen while another mode or Incarnon form
/// is active (`WeaponEx::SwitchBehavior` `0x614fe0` + `WeaponEx::Update` `0x332338`).
#[derive(Debug, Clone, Default)]
pub(super) struct FrozenBeam {
    beam: BeamRamp,
    targets: Vec<TargetRamp>,
    suspended_at: Option<f64>,
}

#[wfsim_gas_macro::metered]
impl FrozenBeam {
    pub(super) fn swap(
        &mut self,
        beam: &mut BeamRamp,
        targets: &mut Vec<TargetRamp>,
        suspend_at: f64,
        resume_at: f64,
    ) {
        if let Some(t0) = self.suspended_at {
            let delta = resume_at - t0;
            self.beam.shift_clock(delta);
            for tr in &mut self.targets {
                tr.shift_clock(delta);
            }
        }
        std::mem::swap(&mut self.beam, beam);
        std::mem::swap(&mut self.targets, targets);
        self.suspended_at = Some(suspend_at);
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TargetRampCtx {
    pub(super) base_ramp: f64,
    pub(super) tick: f64,
    pub(super) floor: f64,
    pub(super) spec: Option<crate::model::BeamRampSpec>,
}

#[wfsim_gas_macro::metered]
impl TargetRampCtx {
    /// Per-target ramp ratio relative to the aimed target's `base_ramp`,
    /// advancing `ramps[body]` once per tick `t` and resetting on respawn.
    pub(super) fn ratio(
        self,
        ramps: &mut Vec<TargetRamp>,
        bodies_len: usize,
        body: usize,
        generation: u32,
        t: f64,
    ) -> f64 {
        if ramps.len() <= body {
            ramps.resize(bodies_len, TargetRamp::default());
        }
        let here = ramps[body].hit(generation, t, self.tick, self.floor, self.spec);
        here / self.base_ramp.max(1e-9)
    }
}

#[wfsim_gas_macro::metered]
impl TargetRamp {
    pub(super) fn shift_clock(&mut self, delta: f64) {
        self.clock.shift_clock(delta);
    }

    pub(super) fn hit(
        &mut self, generation: u32, at: f64, step: f64, floor: f64, source: Option<crate::model::BeamRampSpec>,
    ) -> f64 {
        if self.generation != Some(generation) {
            *self = Self { generation: Some(generation), ..Default::default() };
        }
        if self.clock.last_tick != Some(at) {
            self.factor = self.clock.tick(at, step, floor, source);
        }
        self.factor
    }
}

#[wfsim_gas_macro::metered]
impl BeamRamp {
    pub(super) fn shift_clock(&mut self, delta: f64) {
        if let Some(last) = &mut self.last_tick {
            *last += delta;
        }
    }

    /// Advance the ramp by one tick's elapsed time before reading the factor,
    /// matching DE's native beam impact update (`0x1788204..0x17882ad`): contact
    /// initializes the first tick countdown to `1 / fire_rate` and accumulates
    /// elapsed time before evaluating attenuation.
    pub(super) fn tick(&mut self, now: f64, tick_seconds: f64, floor: f64, source: Option<crate::model::BeamRampSpec>) -> f64 {
        let rise = source.map_or(BEAM_RAMP_SECONDS, |s| s.rise_seconds);
        let ceiling = source.map_or(1.0, |s| s.ceiling);
        let rise_ticks = source.map_or(0, |s| s.rise_ticks);
        let reset_on_release = source.is_some_and(|s| s.reset_on_release);
        let delay = source.map_or(BEAM_DECAY_DELAY, |s| s.cooldown_delay_seconds);
        let decay = source.map_or(BEAM_DECAY_SECONDS, |s| s.rise_seconds * s.cooldown_divisor);
        if let Some(prev) = self.last_tick {
            let idle = now - prev - tick_seconds;
            if reset_on_release && idle > 1e-9 {
                self.progress = 0.0;
            } else if idle > delay {
                self.progress = if decay > 0.0 {
                    (self.progress - (idle - delay) / decay).max(0.0)
                } else {
                    0.0
                };
            }
        }
        self.progress = if rise_ticks > 0 {
            (self.progress + 1.0 / f64::from(rise_ticks)).min(1.0)
        } else if rise > 0.0 {
            (self.progress + tick_seconds / rise).min(1.0)
        } else {
            1.0
        };
        let mult = floor + (ceiling - floor) * self.progress;
        self.last_tick = Some(now);
        mult
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simultaneous_links_advance_a_target_clock_once_and_respawn_starts_cold() {
        let source = Some(crate::model::BeamRampSpec {
            rise_seconds: 0.6, ceiling: 1.0, rise_ticks: 0, cooldown_delay_seconds: 0.8,
            cooldown_divisor: 2.0, per_target: true, reset_on_release: false,
        });
        let mut ramp = TargetRamp::default();
        let first = 0.2 + 0.8 / 6.0;
        let second = 0.2 + 0.8 * 2.0 / 6.0;
        assert!((ramp.hit(1, 0., 0.1, 0.2, source) - first).abs() < 1e-12);
        assert!((ramp.hit(1, 0., 0.1, 0.2, source) - first).abs() < 1e-12);
        assert!((ramp.hit(1, 0.1, 0.1, 0.2, source) - second).abs() < 1e-12);
        assert!((ramp.hit(2, 0.2, 0.1, 0.2, source) - first).abs() < 1e-12);
    }

    #[test]
    fn native_cooldown_divides_time_state_before_normalizing_progress() {
        let source = Some(crate::model::BeamRampSpec {
            rise_seconds: 0.6, ceiling: 1.0, rise_ticks: 0, cooldown_delay_seconds: 0.8,
            cooldown_divisor: 2.0, per_target: false, reset_on_release: false,
        });
        let mut native = BeamRamp { progress: 1.0, last_tick: Some(0.0) };
        let mut ordinary = native;
        // 0.1 next-tick spacing + 0.8 delay + 0.6 of decay leaves 0.5 progress,
        // then the tick's own 0.1 s advances progress by 1/6 before sampling.
        assert!((native.tick(1.5, 0.1, 0.2, source) - (0.2 + 0.8 * (0.5 + 1.0 / 6.0))).abs() < 1e-12);
        assert!((ordinary.tick(1.5, 0.1, 0.2, None) - (0.2 + 0.8 * (0.7 + 1.0 / 6.0))).abs() < 1e-12);
        let mut empty = BeamRamp { progress: 1.0, last_tick: Some(0.0) };
        assert!((empty.tick(2.1, 0.1, 0.2, source) - (0.2 + 0.8 / 6.0)).abs() < 1e-12);
    }

    #[test]
    fn faster_fire_rate_lowers_first_tick_and_matches_phantasma_m133() {
        let torid = Some(crate::model::BeamRampSpec {
            rise_seconds: 0.6, ceiling: 1.0, rise_ticks: 0, cooldown_delay_seconds: 0.8,
            cooldown_divisor: 2.0, per_target: true, reset_on_release: false,
        });
        let slow = BeamRamp::default().tick(0.0, 1.0 / 6.4, 0.2, torid);
        let base = BeamRamp::default().tick(0.0, 1.0 / 8.0, 0.2, torid);
        let fast = BeamRamp::default().tick(0.0, 1.0 / 15.2, 0.2, torid);
        assert!(slow > base && base > fast, "slow={slow} base={base} fast={fast}");
        assert!((base - (0.2 + 0.8 * (0.125 / 0.6))).abs() < 1e-12);

        // Phantasma Prime client floor 0.02 at 12 ticks/s -> 0.15611 * 90 = 14.05 (M133: 14).
        let pp = BeamRamp::default().tick(0.0, 1.0 / 12.0, 0.02, torid);
        assert!((pp * 90.0 - 14.05).abs() < 0.01, "pp first tick {}", pp * 90.0);
    }
}
