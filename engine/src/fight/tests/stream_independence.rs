//! A CHANGE THAT PAYS NOTHING MUST READ AS NOTHING.
//!
//! The simulator is a sampler, so two builds are compared by running both and
//! subtracting — and that only means anything if the seed means the same thing
//! in both. It did not: every roll came off one stream, so a status chance high
//! enough to land one more proc drew one more number to pick its element, and
//! every crit and body part after it was a different draw. Two builds that
//! differ in nothing that pays came back differing by noise, and the page
//! printed that noise as a recommendation.
//!
//! IMPACT is the clean case. It pushes a stagger stack and nothing else — a
//! single-target damage sim has no notion of an enemy being interrupted — so
//! more Impact procs must be worth EXACTLY nothing.
//!
//! COLD IS NOT that case, which is worth writing down because it was the one
//! reported. A Cold status raises the crit damage the target TAKES (+10% on the
//! first stack, +5% on each further, +100% while Frozen), so more Cold procs
//! really are more damage. That is the buff-shaped effect the owner expected to
//! be the only way status can pay — it is simply that Cold has one.
use super::*;

/// Pure Impact, ordinary crit, nothing on the build that reads status.
fn inert(status_chance: f64) -> FightParams {
    FightParams {
        damage: DamageVector::new().with(DamageType::Impact, 100.0),
        status_chance,
        base_status_chance: status_chance,
        base_crit_chance: 0.3,
        crit_multiplier: 2.0,
        multishot: 1.6,
        base_multishot: 1.6,
        duration_seconds: 60.0,
        ..FightParams::default()
    }
}

#[test]
fn more_status_that_pays_nothing_reads_as_nothing() {
    let low = monte_carlo(&inert(0.1), 40, 12345);
    let high = monte_carlo(&inert(0.9), 40, 12345);
    // It really did change the fight...
    assert!(
        high.mean_procs > low.mean_procs * 2.0,
        "the premise is wrong — procs {} vs {}",
        low.mean_procs, high.mean_procs
    );
    // ...and none of it was worth a point of damage.
    assert!(
        (high.mean_damage - low.mean_damage).abs() < 1e-9,
        "an Impact-only build paid {} for status it cannot spend (low {} high {})",
        high.mean_damage - low.mean_damage, low.mean_damage, high.mean_damage
    );
    // The crits are the same crits, not merely the same average.
    assert!(
        (high.mean_crit_rate - low.mean_crit_rate).abs() < 1e-12,
        "the crit stream moved: {} vs {}", low.mean_crit_rate, high.mean_crit_rate
    );
    assert!(
        (high.mean_headshot_rate - low.mean_headshot_rate).abs() < 1e-12,
        "the body-part stream moved: {} vs {}", low.mean_headshot_rate, high.mean_headshot_rate
    );
}

/// ...and the split did not cost the sampler its randomness: the spine
/// still answers to the seed.
#[test]
fn the_spine_still_varies_with_the_seed() {
    let a = monte_carlo(&inert(0.5), 20, 1);
    let b = monte_carlo(&inert(0.5), 20, 2);
    assert!(
        (a.mean_damage - b.mean_damage).abs() > 1e-9,
        "two seeds produced one fight"
    );
}
