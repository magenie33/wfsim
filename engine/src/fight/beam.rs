
/// What a KILL gives the weapon that made it, as a fraction of the enemy's
/// affinity. VERBATIM (wiki Affinity): "Kill with weapons: Half Affinity goes
/// to the Warframe and half to the killing weapon."
///
/// The general 25/75 split on that page is for SHARED affinity — orbs, ally
/// kills, ability casts — and is not this. A weapon that killed something takes
/// half, whatever else is equipped.
pub(super) const WEAPON_AFFINITY_SHARE: f64 = 0.5;
pub(super) const BEAM_RAMP_SECONDS: f64 = 0.6;
pub(super) const BEAM_DECAY_DELAY: f64 = 0.8;
pub(super) const BEAM_DECAY_SECONDS: f64 = 2.0;

/// Progress along a continuous weapon's damage ramp: 0 = the 20% floor, 1 =
/// full damage. Advanced by holding fire on a target and decayed by stopping.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct BeamRamp {
    pub(super) progress: f64,
    pub(super) last_tick: Option<f64>,
}

#[wfsim_gas_macro::metered]
impl BeamRamp {
    /// The multiplier for a tick at `now`, then advance the ramp by one tick's
    /// worth of held fire. The tick landing NOW is scaled by the progress it
    /// arrives with, so the first tick of a burst deals the floor.
    pub(super) fn tick(&mut self, now: f64, tick_seconds: f64, floor: f64, source: Option<crate::model::BeamRampSpec>) -> f64 {
        let rise = source.map_or(BEAM_RAMP_SECONDS, |s| s.rise_seconds);
        let delay = source.map_or(BEAM_DECAY_DELAY, |s| s.cooldown_delay_seconds);
        let decay = source.map_or(BEAM_DECAY_SECONDS, |s| s.rise_seconds * s.cooldown_divisor);
        if let Some(prev) = self.last_tick {
            let idle = now - prev - tick_seconds;
            if idle > delay {
                self.progress =
                    (self.progress - (idle - delay) / decay).max(0.0);
            }
        }
        let mult = floor + (1.0 - floor) * self.progress;
        self.progress = (self.progress + tick_seconds / rise).min(1.0);
        self.last_tick = Some(now);
        mult
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_cooldown_divides_time_state_before_normalizing_progress() {
        let source = Some(crate::model::BeamRampSpec {
            rise_seconds: 0.6, cooldown_delay_seconds: 0.8, cooldown_divisor: 2.0,
        });
        let mut native = BeamRamp { progress: 1.0, last_tick: Some(0.0) };
        let mut ordinary = native;
        // 0.1 next-tick spacing + 0.8 delay + 0.6 of decay: half the native state.
        assert!((native.tick(1.5, 0.1, 0.2, source) - 0.6).abs() < 1e-12);
        assert!((ordinary.tick(1.5, 0.1, 0.2, None) - 0.76).abs() < 1e-12);
        let mut empty = BeamRamp { progress: 1.0, last_tick: Some(0.0) };
        assert!((empty.tick(2.1, 0.1, 0.2, source) - 0.2).abs() < 1e-12);
    }
}
