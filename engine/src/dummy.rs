//! Minimal "shoot the training dummy" Monte Carlo — the first end-to-end sim.
//!
//! Scope (deliberately basic, see devlog 2026-07-24):
//! - Dual Toxocyst **base form** raw damage only (75 dmg, 5% crit, 2.0x crit).
//! - **Secondary Enervate** equipped (max rank): flat crit stacks on hit, resets
//!   after 6 big crits — driven through the real [`Perk`]/[`BuffBar`] machinery.
//! - **50% headshot rate**; headshots apply a damage multiplier.
//! - **No** status/elements/damage-type effects, **no** armor, **no** Frenzy.
//!   Infinite ammo (we fire every shot regardless of magazine).
//!
//! ASSUMPTIONS (status: **unverified** — refine with a golden test):
//! - Headshot multiplier = 2.0x, applied multiplicatively, with **no** special
//!   critical-headshot interaction. The wiki's exact critical-headshot formula
//!   was not captured; this is a placeholder.
//! - Crit tiering: effective crit chance can exceed 100%; guaranteed tier
//!   `floor(cc)`, one more with probability `cc - floor(cc)`; a tier-`k` hit
//!   multiplies by `1 + k*(cd - 1)`.
//! - Enervate's stack for a shot applies to *subsequent* shots (we read the buff
//!   bar before the shot, then register the hit).

use crate::buffs::BuffBar;
use crate::perks::secondary_enervate::SecondaryEnervate;
use crate::perks::Perk;
use crate::rng::Rng;
use crate::sim::{Event, Hit};

/// Parameters of the dummy engagement.
#[derive(Debug, Clone)]
pub struct DummyParams {
    pub base_damage: f64,
    pub base_crit_chance: f64,
    pub crit_multiplier: f64,
    pub fire_rate: f64,
    pub headshot_rate: f64,
    pub headshot_multiplier: f64,
    pub duration_secs: f64,
}

impl Default for DummyParams {
    /// Dual Toxocyst base form + Secondary Enervate, 50% headshots, 10 s.
    fn default() -> Self {
        Self {
            base_damage: 75.0,
            base_crit_chance: 0.05,
            crit_multiplier: 2.0,
            fire_rate: 1.0,
            headshot_rate: 0.5,
            headshot_multiplier: 2.0, // ASSUMPTION (unverified)
            duration_secs: 10.0,
        }
    }
}

/// Result of a single engagement.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunResult {
    pub total_damage: f64,
    pub shots: u32,
    pub crits: u32,     // tier >= 1
    pub big_crits: u32, // tier >= 2
    pub headshots: u32,
}

/// Roll a critical tier for an effective crit chance that may exceed 1.0.
fn roll_crit_tier(effective_cc: f64, rng: &mut Rng) -> u32 {
    let guaranteed = effective_cc.floor().max(0.0);
    let extra_chance = effective_cc - guaranteed;
    guaranteed as u32 + rng.chance(extra_chance) as u32
}

/// Run one engagement with a fresh buff bar and a fresh Secondary Enervate.
pub fn run_once(params: &DummyParams, rng: &mut Rng) -> RunResult {
    let mut bar = BuffBar::new();
    let mut enervate = SecondaryEnervate::default();
    let mut r = RunResult::default();

    // Fire at t = k / fire_rate while t < duration. Integer k avoids float drift.
    let mut k: u64 = 0;
    loop {
        let t = k as f64 / params.fire_rate;
        if t >= params.duration_secs {
            break;
        }
        k += 1;

        // Crit chance for this shot reflects stacks from previous hits.
        let flat_crit = bar.total_contributions().flat_crit_chance;
        let effective_cc = params.base_crit_chance + flat_crit;
        let tier = roll_crit_tier(effective_cc, rng);
        let crit_mult = 1.0 + tier as f64 * (params.crit_multiplier - 1.0);

        let headshot = rng.chance(params.headshot_rate);
        let hs_mult = if headshot {
            params.headshot_multiplier
        } else {
            1.0
        };

        r.total_damage += params.base_damage * crit_mult * hs_mult;
        r.shots += 1;
        r.crits += (tier >= 1) as u32;
        r.big_crits += (tier >= 2) as u32;
        r.headshots += headshot as u32;

        // Register the hit so Enervate stacks/resets for the next shot.
        enervate.on_event(
            &Event::Hit(Hit {
                big_crit: tier >= 2,
                headshot,
                target_alive: true,
            }),
            t,
            &mut bar,
        );
    }

    r
}

/// Aggregate statistics over many engagements.
#[derive(Debug, Clone, Copy)]
pub struct Summary {
    pub runs: u32,
    pub duration_secs: f64,
    pub mean_damage: f64,
    pub dps: f64,
    pub std_damage: f64,
    pub min_damage: f64,
    pub max_damage: f64,
    pub mean_shots: f64,
    pub mean_crit_rate: f64,
    pub mean_big_crit_rate: f64,
    pub mean_headshot_rate: f64,
}

/// Run `runs` engagements from a single seed and summarize.
pub fn monte_carlo(params: &DummyParams, runs: u32, seed: u64) -> Summary {
    let mut rng = Rng::new(seed);
    let mut sum = 0.0f64;
    let mut sum_sq = 0.0f64;
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let (mut shots, mut crits, mut big_crits, mut headshots) = (0u64, 0u64, 0u64, 0u64);

    for _ in 0..runs {
        let r = run_once(params, &mut rng);
        sum += r.total_damage;
        sum_sq += r.total_damage * r.total_damage;
        min = min.min(r.total_damage);
        max = max.max(r.total_damage);
        shots += r.shots as u64;
        crits += r.crits as u64;
        big_crits += r.big_crits as u64;
        headshots += r.headshots as u64;
    }

    let n = runs.max(1) as f64;
    let mean = sum / n;
    let variance = (sum_sq / n - mean * mean).max(0.0);
    let total_shots = shots.max(1) as f64;

    Summary {
        runs,
        duration_secs: params.duration_secs,
        mean_damage: mean,
        dps: mean / params.duration_secs,
        std_damage: variance.sqrt(),
        min_damage: if min.is_finite() { min } else { 0.0 },
        max_damage: if max.is_finite() { max } else { 0.0 },
        mean_shots: shots as f64 / n,
        mean_crit_rate: crits as f64 / total_shots,
        mean_big_crit_rate: big_crits as f64 / total_shots,
        mean_headshot_rate: headshots as f64 / total_shots,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_shots_in_ten_seconds_at_one_per_second() {
        let s = monte_carlo(&DummyParams::default(), 100, 1);
        assert!((s.mean_shots - 10.0).abs() < 1e-9);
    }

    #[test]
    fn monte_carlo_is_deterministic() {
        let a = monte_carlo(&DummyParams::default(), 500, 12345);
        let b = monte_carlo(&DummyParams::default(), 500, 12345);
        assert_eq!(a.mean_damage, b.mean_damage);
        assert_eq!(a.std_damage, b.std_damage);
    }

    #[test]
    fn headshot_rate_is_about_half() {
        let s = monte_carlo(&DummyParams::default(), 1000, 999);
        assert!((s.mean_headshot_rate - 0.5).abs() < 0.02);
    }

    #[test]
    fn produces_positive_damage() {
        let s = monte_carlo(&DummyParams::default(), 1000, 7);
        assert!(s.mean_damage > 0.0);
        assert!(s.dps > 0.0);
    }

    #[test]
    fn enervate_raises_crit_rate_above_base() {
        // Base crit is 5%, but Enervate stacks flat crit as the fight goes on, so
        // the observed crit rate should exceed 5%.
        let s = monte_carlo(&DummyParams::default(), 2000, 3);
        assert!(
            s.mean_crit_rate > 0.05,
            "crit rate was {}",
            s.mean_crit_rate
        );
    }
}
