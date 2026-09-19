
/// The HELD-TRIGGER SPOOL after `shots` consecutive pulls — a fraction of the
/// live fire rate, 1.0 where the weapon has none.
///
/// BOTH DIRECTIONS, one line: the Phenmor falls from 1.0 to 0.6 and a Gorgon
/// climbs from 0.2 to 1.0, and nothing here needs to know which. It is linear
/// because every source gives two ends and a count and nothing in between.
///
/// The ramp above is a different animal despite the family resemblance: a beam
/// climbs in SECONDS and holds, this moves in SHOTS and is a cadence — which is
/// why a fire-rate mod does not buy its way out of it (the mod raises both ends
/// together).
pub(super) fn spool_factor(spec: Option<crate::model::SustainedFireRate>, shots: f64) -> f64 {
    match spec {
        Some(s) if s.over_shots > 0.0 => {
            s.start + (s.end - s.start) * (shots / s.over_shots).min(1.0)
        }
        _ => 1.0,
    }
}

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

impl BeamRamp {
    /// The multiplier for a tick at `now`, then advance the ramp by one tick's
    /// worth of held fire. The tick landing NOW is scaled by the progress it
    /// arrives with, so the first tick of a burst deals the floor.
    pub(super) fn tick(&mut self, now: f64, tick_seconds: f64, floor: f64) -> f64 {
        if let Some(prev) = self.last_tick {
            let idle = now - prev - tick_seconds;
            if idle > BEAM_DECAY_DELAY {
                self.progress =
                    (self.progress - (idle - BEAM_DECAY_DELAY) / BEAM_DECAY_SECONDS).max(0.0);
            }
        }
        let mult = floor + (1.0 - floor) * self.progress;
        self.progress = (self.progress + tick_seconds / BEAM_RAMP_SECONDS).min(1.0);
        self.last_tick = Some(now);
        mult
    }
}
