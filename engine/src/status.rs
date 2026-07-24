//! Status / proc layer — pipeline layer [5], first slice: proc **selection**.
//!
//! Rules (wiki `Status_Effect`; docs/MECHANICS.md §6 — unverified until
//! golden-tested):
//! - The listed status chance `SC` rolls **per pellet/hit**. `SC > 100%`
//!   gives `floor(SC)` guaranteed rolls plus a `frac(SC)` chance of one more.
//! - Only a successful roll draws a **type**, weighted by the hit's (modded)
//!   damage vector: `P(type) = damage / total`. Draws are independent — the
//!   same type can repeat within one hit.
//! - **Forced procs** are weapon-data attributes declared per attack part.
//!   They are independent of both `SC` and the distribution, and are appended
//!   **outside** the rolling pipeline ("not the same as 100% status chance").
//!
//! Official average: `procs per trigger pull = multishot × (forced + SC)`.
//!
//! What a proc *does* (the debuff applied to the target's DebuffBar) is
//! defined in `data/status_effects/` — this module only decides *which*
//! procs a hit produces.

use crate::damage::{DamageType, DamageVector};
use crate::rng::Rng;

/// Number of rolled (non-forced) procs for one hit at `status_chance`
/// (fraction; may exceed 1.0).
pub fn roll_proc_count(status_chance: f64, rng: &mut Rng) -> u32 {
    let sc = status_chance.max(0.0);
    sc.floor() as u32 + rng.chance(sc.fract()) as u32
}

/// Draw one proc type, weighted by damage share. `None` if the vector has no
/// positive damage (nothing to type a proc with).
pub fn draw_proc_type(vector: &DamageVector, rng: &mut Rng) -> Option<DamageType> {
    let total = vector.total();
    if total <= 0.0 {
        return None;
    }
    let mut x = rng.next_f64() * total;
    let mut last = None;
    for (t, amount) in vector.iter_nonzero() {
        x -= amount;
        last = Some(t);
        if x < 0.0 {
            break;
        }
    }
    last
}

/// The full proc set of one hit: forced procs first (declared by the weapon's
/// attack part), then `roll_proc_count` weighted draws.
pub fn procs_for_hit(
    forced: &[DamageType],
    status_chance: f64,
    vector: &DamageVector,
    rng: &mut Rng,
) -> Vec<DamageType> {
    let mut procs = forced.to_vec();
    for _ in 0..roll_proc_count(status_chance, rng) {
        if let Some(t) = draw_proc_type(vector, rng) {
            procs.push(t);
        }
    }
    procs
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The worked example from MECHANICS.md §6: SC 40%, vector
    /// Impact 20 / Slash 30 / Heat 25 / Toxin 15 / Corrosive 10,
    /// forced Stagger (Impact) on every hit.
    fn example_vector() -> DamageVector {
        DamageVector::new()
            .with(DamageType::Impact, 20.0)
            .with(DamageType::Slash, 30.0)
            .with(DamageType::Heat, 25.0)
            .with(DamageType::Toxin, 15.0)
            .with(DamageType::Corrosive, 10.0)
    }

    #[test]
    fn worked_example_frequencies_match_hand_computation() {
        let v = example_vector();
        let forced = [DamageType::Impact];
        let mut rng = Rng::new(0xE1E);
        let n = 200_000;
        let (mut two_impact, mut slash, mut heat, mut toxin, mut corr, mut only_forced) =
            (0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
        let mut total_procs = 0u64;

        for _ in 0..n {
            let procs = procs_for_hit(&forced, 0.40, &v, &mut rng);
            total_procs += procs.len() as u64;
            // The forced Impact is always present.
            assert_eq!(procs[0], DamageType::Impact);
            match procs.len() {
                1 => only_forced += 1,
                2 => match procs[1] {
                    DamageType::Impact => two_impact += 1,
                    DamageType::Slash => slash += 1,
                    DamageType::Heat => heat += 1,
                    DamageType::Toxin => toxin += 1,
                    DamageType::Corrosive => corr += 1,
                    other => panic!("impossible proc type {other:?}"),
                },
                l => panic!("impossible proc count {l}"),
            }
        }

        let f = |c: u32| c as f64 / n as f64;
        // Hand-computed: 60% forced-only; rolled = SC × share:
        // 8% double-Stagger, 12% Bleed, 10% Ignite, 6% Poison, 4% Corrosion.
        assert!((f(only_forced) - 0.60).abs() < 0.01, "{}", f(only_forced));
        assert!((f(two_impact) - 0.08).abs() < 0.01, "{}", f(two_impact));
        assert!((f(slash) - 0.12).abs() < 0.01, "{}", f(slash));
        assert!((f(heat) - 0.10).abs() < 0.01, "{}", f(heat));
        assert!((f(toxin) - 0.06).abs() < 0.01, "{}", f(toxin));
        assert!((f(corr) - 0.04).abs() < 0.01, "{}", f(corr));
        // Official average: forced + SC = 1.4 procs per hit.
        let avg = total_procs as f64 / n as f64;
        assert!((avg - 1.4).abs() < 0.01, "avg {avg}");
    }

    #[test]
    fn status_chance_over_100_percent_rolls_floor_plus_fraction() {
        let mut rng = Rng::new(7);
        let n = 100_000;
        let mut sum = 0u64;
        for _ in 0..n {
            let c = roll_proc_count(1.4, &mut rng);
            assert!(c == 1 || c == 2, "count {c}");
            sum += c as u64;
        }
        let avg = sum as f64 / n as f64;
        assert!((avg - 1.4).abs() < 0.01, "avg {avg}");
    }

    #[test]
    fn forced_procs_ignore_status_chance_and_distribution() {
        // SC 0 and an empty damage vector: the forced proc still happens,
        // nothing else does.
        let mut rng = Rng::new(1);
        let procs = procs_for_hit(&[DamageType::Impact], 0.0, &DamageVector::new(), &mut rng);
        assert_eq!(procs, vec![DamageType::Impact]);
    }

    #[test]
    fn rolled_procs_need_positive_damage_to_be_typed() {
        // 100% SC but an all-zero vector: the roll succeeds but there is no
        // type to draw -> no rolled proc.
        let mut rng = Rng::new(2);
        let procs = procs_for_hit(&[], 1.0, &DamageVector::new(), &mut rng);
        assert!(procs.is_empty());
    }

    #[test]
    fn single_type_vector_always_draws_that_type() {
        let v = DamageVector::new().with(DamageType::Radiation, 5.0);
        let mut rng = Rng::new(3);
        for _ in 0..1000 {
            assert_eq!(draw_proc_type(&v, &mut rng), Some(DamageType::Radiation));
        }
    }
}
