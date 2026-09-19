// SPDX-License-Identifier: AGPL-3.0-or-later
//! RAGE — a Warframe passive that is a meter on melee damage (Valkyr's). The
//! numbers are data on the frame (`rage:`, `data::warframes::RageSpec`) and the
//! rule is `docs/WARFRAMES.md` §Passives.

use crate::data::warframes::RageSpec;

/// The buff roster's id for the meter, and the buff card's.
pub const BUFF_ID: &str = "valkyr_rage";

/// The meter in a fight. `value` is a share of base damage: 3.0 is 300%.
#[derive(Debug, Clone, Copy)]
pub struct Rage {
    spec: RageSpec,
    value: f64,
    built_at: f64,
    held: bool,
}

impl Rage {
    /// Opens at `start`; a `held` meter never decays, which is the buff card's lock.
    pub fn new(spec: RageSpec, start: f64, held: bool) -> Self {
        Self { spec, value: start.clamp(0.0, spec.cap), built_at: 0.0, held }
    }

    pub fn spec(&self) -> RageSpec {
        self.spec
    }

    /// The bonus at `now`, as a share of base damage.
    pub fn bonus(&self, now: f64) -> f64 {
        let idle = now - self.built_at - self.spec.idle_seconds;
        if self.held || idle <= 0.0 {
            return self.value;
        }
        decayed(&self.spec, self.value, idle)
    }

    /// A hit or a kill adds `gain`. Gaining nothing is not building Rage, so
    /// the idle clock keeps running.
    pub fn build(&mut self, now: f64, gain: f64) {
        if gain <= 0.0 {
            return;
        }
        self.value = (self.bonus(now) + gain).min(self.spec.cap);
        self.built_at = now;
    }
}

/// `value` after `idle` seconds of decay. The wiki's curve is a function of the
/// time since a FULL meter, and the rate is "dependent on the amount of Rage
/// Valkyr has" — so a partial meter is placed on the curve where it stands and
/// moves along it from there.
///
/// W`Valkyr/Abilities/Passive`: `d(t) = e^(-λt)` up to `n = ln(1000λ)/λ`, then
/// `e^(-λn) - 0.001(t - n)`; `Bonus(t) = ⌊300 · d(t)⌋`. The knee is where the
/// curve's slope reaches the tail's, so it is derived rather than stored.
fn decayed(s: &RageSpec, value: f64, idle: f64) -> f64 {
    let (lambda, tail) = (s.decay_rate, s.tail_per_second);
    let knee = (lambda / tail).ln() / lambda;
    let at_knee = tail / lambda;
    let from = value / s.cap;
    let placed = if from > at_knee { -from.ln() / lambda } else { knee + (at_knee - from) / tail };
    let t = placed + idle;
    let d = if t <= knee { (-lambda * t).exp() } else { at_knee - tail * (t - knee) };
    ((s.cap * 100.0 * d).floor() / 100.0).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valkyr() -> RageSpec {
        crate::data::warframes::warframe("valkyr").and_then(|f| f.rage).expect("valkyr has rage")
    }

    fn full() -> Rage {
        Rage::new(valkyr(), 3.0, false)
    }

    /// The page's own summary of its formula: "Above 18%, the meter decays by
    /// half over 43 seconds. Below 18%, the meter decays by 0.3% every second."
    #[test]
    fn the_meter_holds_five_seconds_then_halves_in_43_then_loses_a_third_of_a_percent_a_second() {
        let r = full();
        assert_eq!(r.bonus(5.0), 3.0);
        let half = 5.0 + std::f64::consts::LN_2 / 0.016;
        assert!((r.bonus(half) - 1.5).abs() <= 0.01, "{}", r.bonus(half));
        let knee = 5.0 + (0.016f64 / 0.001).ln() / 0.016;
        assert_eq!(r.bonus(knee + 1e-9), 0.18);
        assert_eq!(r.bonus(knee + 10.0), 0.15);
        assert_eq!(r.bonus(knee + 1000.0), 0.0);
    }

    #[test]
    fn a_partial_meter_decays_from_where_it_stands_on_the_curve() {
        let half = Rage::new(valkyr(), 1.5, false);
        let offset = std::f64::consts::LN_2 / 0.016;
        for idle in [1.0, 20.0, 150.0, 200.0] {
            assert!((half.bonus(5.0 + idle) - full().bonus(5.0 + offset + idle)).abs() <= 0.01, "{idle}");
        }
    }

    #[test]
    fn building_caps_at_300_and_nothing_gained_keeps_the_clock() {
        let mut r = Rage::new(valkyr(), 2.9, false);
        r.build(1.0, 0.12);
        assert_eq!(r.bonus(6.0), 3.0);
        r.build(5.9, 0.0);
        assert!(r.bonus(7.0) < 3.0, "a hit that built nothing restarted the idle clock");
        assert_eq!(Rage::new(valkyr(), 3.0, true).bonus(500.0), 3.0, "a held meter decays");
    }
}
