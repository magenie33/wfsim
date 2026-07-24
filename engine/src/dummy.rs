//! Minimal "shoot the training dummy" Monte Carlo — the first end-to-end sim.
//!
//! Scope (deliberately basic, see devlog 2026-07-24):
//! - Dual Toxocyst **base form** raw damage only (75 dmg, 5% crit, 2.0x crit).
//! - **Secondary Enervate** equipped (max rank): flat crit stacks on hit, resets
//!   after 6 big crits — driven through the real [`Perk`]/[`BuffBar`] machinery.
//! - The dummy is a **humanoid target made of body parts**; each shot lands on
//!   one part chosen by aim weight (default: 50% body 1x / 50% head 3x).
//! - **No** status/elements/damage-type effects, **no** armor, **no** Frenzy.
//!   Infinite ammo (we fire every shot regardless of magazine).
//!
//! Body-part model (source: wiki `Enemy_Body_Parts`; see docs/MECHANICS.md §7):
//! - Each part has its own **location multiplier** (humanoid head = 3.0x,
//!   body = 1x; other enemies have arbitrary parts: MOA "fanny pack" 3x,
//!   Nox exposed head 4x, boss weak points on a 0x body, ...).
//! - **Headshot is a trigger, not a damage stat.** Effects that specify
//!   headshots (e.g. Frenzy) fire **only** when the struck part is the head
//!   (`is_head`), never on other weak spots (wiki `Enemy_Body_Parts`
//!   §Weak Spot Bonuses). `Hit::headshot` is sourced from the part.
//! - The **critical-location bonus** is a separate per-part eligibility
//!   (`crit_bonus`): a crit on an eligible part with multiplier > 1x doubles
//!   the crit damage multiplier inside the tier formula:
//!   `part_mult * (1 + k*(2*cd - 1))` (wiki `Critical_Hit`
//!   §Critical Headshots). Ineligible examples: MOA fanny pack, helmeted
//!   Corpus heads, any 1x location.
//!
//! Crit tiering (wiki `Critical_Hit` §Critical Tiers; see docs/MECHANICS.md §5):
//! effective crit chance can exceed 100%; guaranteed tier `floor(cc)`, one more
//! with probability `cc - floor(cc)`; a tier-`k` hit multiplies by
//! `1 + k*(cd - 1)`.
//!
//! All of the above is **unverified** until golden-tested. Enervate's stack for
//! a shot applies to *subsequent* shots (we read the buff bar before the shot,
//! then register the hit).

use crate::buffs::BuffBar;
use crate::perks::secondary_enervate::SecondaryEnervate;
use crate::perks::Perk;
use crate::rng::Rng;
use crate::sim::{Event, Hit};

/// One aimable location on the target (wiki `Enemy_Body_Parts`).
#[derive(Debug, Clone)]
pub struct BodyPart {
    pub name: &'static str,
    /// Relative probability of a shot landing here (weights are normalized).
    pub aim_weight: f64,
    /// Location damage multiplier.
    pub multiplier: f64,
    /// True head: fires on-headshot effects (`Hit::headshot`). Other weak
    /// spots never trigger headshot conditions.
    pub is_head: bool,
    /// Eligible for the critical-location bonus (the `2*cd` fold-in). False
    /// for e.g. MOA fanny packs and helmeted Corpus heads; locations at 1x
    /// never get the bonus regardless of this flag.
    pub crit_bonus: bool,
}

/// Parameters of the dummy engagement.
#[derive(Debug, Clone)]
pub struct DummyParams {
    pub base_damage: f64,
    pub base_crit_chance: f64,
    pub crit_multiplier: f64,
    pub fire_rate: f64,
    pub body_parts: Vec<BodyPart>,
    pub duration_secs: f64,
}

impl DummyParams {
    /// A generic humanoid: body 1x, head 3x (headshot-triggering, crit-bonus
    /// eligible), aimed at 50/50.
    pub fn humanoid_parts() -> Vec<BodyPart> {
        vec![
            BodyPart {
                name: "body",
                aim_weight: 0.5,
                multiplier: 1.0,
                is_head: false,
                crit_bonus: false,
            },
            BodyPart {
                name: "head",
                aim_weight: 0.5,
                multiplier: 3.0, // humanoid head (wiki: Enemy_Body_Parts)
                is_head: true,
                crit_bonus: true,
            },
        ]
    }
}

impl Default for DummyParams {
    /// Dual Toxocyst base form + Secondary Enervate, humanoid dummy, 10 s.
    fn default() -> Self {
        Self {
            base_damage: 75.0,
            base_crit_chance: 0.05,
            crit_multiplier: 2.0,
            fire_rate: 1.0,
            body_parts: Self::humanoid_parts(),
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
    pub headshots: u32, // hits on an `is_head` part
}

/// Roll a critical tier for an effective crit chance that may exceed 1.0.
fn roll_crit_tier(effective_cc: f64, rng: &mut Rng) -> u32 {
    let guaranteed = effective_cc.floor().max(0.0);
    let extra_chance = effective_cc - guaranteed;
    guaranteed as u32 + rng.chance(extra_chance) as u32
}

/// Pick the body part a shot lands on, by normalized aim weight.
fn pick_part<'a>(parts: &'a [BodyPart], rng: &mut Rng) -> &'a BodyPart {
    let total: f64 = parts.iter().map(|p| p.aim_weight).sum();
    let mut x = rng.next_f64() * total;
    for p in parts {
        x -= p.aim_weight;
        if x < 0.0 {
            return p;
        }
    }
    parts.last().expect("dummy needs at least one body part")
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

        let part = pick_part(&params.body_parts, rng);
        // Wiki Critical_Hit §Critical Headshots: a crit on an eligible >1x
        // location doubles the crit damage multiplier inside the tier formula.
        let cd = if part.crit_bonus && part.multiplier > 1.0 {
            2.0 * params.crit_multiplier
        } else {
            params.crit_multiplier
        };
        let crit_mult = 1.0 + tier as f64 * (cd - 1.0);

        r.total_damage += params.base_damage * part.multiplier * crit_mult;
        r.shots += 1;
        r.crits += (tier >= 1) as u32;
        r.big_crits += (tier >= 2) as u32;
        r.headshots += part.is_head as u32;

        // Register the hit so Enervate stacks/resets for the next shot.
        enervate.on_event(
            &Event::Hit(Hit {
                big_crit: tier >= 2,
                headshot: part.is_head,
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

    fn single_part(part: BodyPart) -> DummyParams {
        DummyParams {
            body_parts: vec![part],
            ..DummyParams::default()
        }
    }

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
    fn mean_damage_matches_hand_computed_expectation() {
        // Default params, 10 shots: Enervate ramps cc = 5%,15%,...,95% (sum 5.0).
        // Per shot: E = 0.5*75*(1+cc) + 0.5*(75*3)*(1+3cc) = 150 + 375*cc,
        // so E[total] = 10*150 + 375*5.0 = 3375.
        let s = monte_carlo(&DummyParams::default(), 2000, 42);
        assert!(
            (s.mean_damage - 3375.0).abs() / 3375.0 < 0.02,
            "mean damage was {}",
            s.mean_damage
        );
    }

    #[test]
    fn non_head_weak_spot_never_triggers_headshot() {
        // MOA-fanny-pack-like: 3x location, not a head, no crit bonus.
        // Headshot effects must never fire; damage uses plain cd.
        // Per shot: E = 225*(1+cc) -> total = 2250 + 225*5.0 = 3375.
        let p = single_part(BodyPart {
            name: "fanny pack",
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: false,
            crit_bonus: false,
        });
        let s = monte_carlo(&p, 2000, 11);
        assert_eq!(s.mean_headshot_rate, 0.0);
        assert!(
            (s.mean_damage - 3375.0).abs() / 3375.0 < 0.02,
            "mean damage was {}",
            s.mean_damage
        );
    }

    #[test]
    fn helmeted_head_triggers_headshot_without_crit_bonus() {
        // Helmeted-Corpus-like: true head (triggers headshot effects) but not
        // eligible for the critical-location bonus -> same expectation as the
        // fanny pack: 3375, yet headshot rate is 100%.
        let p = single_part(BodyPart {
            name: "helmeted head",
            aim_weight: 1.0,
            multiplier: 3.0,
            is_head: true,
            crit_bonus: false,
        });
        let s = monte_carlo(&p, 2000, 13);
        assert_eq!(s.mean_headshot_rate, 1.0);
        assert!(
            (s.mean_damage - 3375.0).abs() / 3375.0 < 0.02,
            "mean damage was {}",
            s.mean_damage
        );
    }

    #[test]
    fn one_x_location_gets_no_crit_bonus_even_if_flagged() {
        // Charger-mouth-like: 1x, not a head. Even with crit_bonus set, a 1x
        // location never receives the critical-location bonus.
        // Per shot: E = 75*(1+cc) -> total = 750 + 75*5.0 = 1125.
        let p = single_part(BodyPart {
            name: "mouth",
            aim_weight: 1.0,
            multiplier: 1.0,
            is_head: false,
            crit_bonus: true,
        });
        let s = monte_carlo(&p, 2000, 17);
        assert!(
            (s.mean_damage - 1125.0).abs() / 1125.0 < 0.02,
            "mean damage was {}",
            s.mean_damage
        );
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
