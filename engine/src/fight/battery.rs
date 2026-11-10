use super::*;

/// Passive recovery belongs to the magazine; choosing to wait does not reload it.
#[derive(Debug, Clone, Copy)]
pub(super) struct BatteryClock {
    shot_at: f64,
    credited_at: f64,
    starts_at: f64,
    unlocked_at: f64,
    depleted: bool,
}

impl Default for BatteryClock {
    fn default() -> Self {
        Self {
            shot_at: f64::NEG_INFINITY,
            credited_at: f64::NEG_INFINITY,
            starts_at: f64::NEG_INFINITY,
            unlocked_at: f64::NEG_INFINITY,
            depleted: false,
        }
    }
}

#[wfsim_gas_macro::metered]
impl Ammo {
    pub(super) fn recover_battery(&mut self, battery: crate::model::Battery, last_shot: f64, at: f64) {
        if !last_shot.is_finite() || battery.regen_per_second <= 0.0 {
            return;
        }
        if self.battery.shot_at != last_shot {
            let empty = self.loaded < 1e-9;
            self.battery = BatteryClock {
                shot_at: last_shot,
                credited_at: last_shot,
                starts_at: last_shot + if empty { battery.delay_empty_seconds } else { battery.delay_partial_seconds },
                unlocked_at: last_shot + if empty { battery.empty_animation_seconds.unwrap_or(0.0) } else { 0.0 },
                depleted: empty,
            };
        }
        let elapsed = (at - self.battery.credited_at.max(self.battery.starts_at)).max(0.0);
        self.loaded = (self.loaded + elapsed * battery.regen_per_second).min(self.cap);
        self.battery.credited_at = at;
    }

    pub(super) fn battery_needs_refill(&self, next_cost: f64, at: f64) -> bool {
        !can_fire(self.loaded, next_cost)
            || (next_cost > 1e-9 && ((self.battery.depleted && self.loaded < self.cap - 1e-9)
                || at < self.battery.unlocked_at))
    }

    pub(super) fn battery_full_at(&self, battery: crate::model::Battery, at: f64) -> f64 {
        (at.max(self.battery.starts_at)
            + (self.cap - self.loaded).max(0.0) / battery.regen_per_second.max(1e-9))
            .max(self.battery.unlocked_at)
    }
}
