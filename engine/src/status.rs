//! Status / proc layer — pipeline layer [5], first slice: proc **selection**.
//!
//! Rules (wiki `Status_Effect`; docs/MECHANICS.md §6 — unverified until
//! golden-tested):
//! - The listed status chance `SC` rolls **per pellet/hit**. `SC > 100%`
//!   gives `floor(SC)` guaranteed rolls plus a `fraction(SC)` chance of one more.
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
//! defined in `data/debuffs/` — this module only decides *which*
//! procs a hit produces.

use crate::damage::{DamageType, DamageVector};
use crate::rng::Rng;

/// Number of rolled (non-forced) procs for one hit at `status_chance`
/// (fraction; may exceed 1.0).
pub fn roll_proc_count(status_chance: f64, rng: &mut Rng) -> u32 {
    let sc = status_chance.max(0.0);
    sc.floor() as u32 + rng.chance(sc.fract()) as u32
}

/// Draw one proc type, weighted by damage share. Types the target is
/// status-immune to are EXCLUDED and the remaining weights renormalize
/// (wiki `Status_Effect` §Status Immunity Interactions) - immunity shifts
/// probability onto the other types instead of wasting the roll.
/// `None` if no eligible type has positive damage.
pub fn draw_proc_type(
    vector: &DamageVector,
    immune: &[DamageType],
    rng: &mut Rng,
) -> Option<DamageType> {
    let eligible = |t: DamageType| !immune.contains(&t);
    let total: f64 = vector
        .iter_nonzero()
        .filter(|&(t, _)| eligible(t))
        .map(|(_, a)| a)
        .sum();
    if total <= 0.0 {
        return None;
    }
    let mut x = rng.next_f64() * total;
    let mut last = None;
    for (t, amount) in vector.iter_nonzero().filter(|&(t, _)| eligible(t)) {
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
///
/// AN IMMUNITY BEATS A GUARANTEE. A forced proc of a type the target cannot
/// take does not land — the game shows the proc icon on the damage number and
/// the status never appears on the enemy (with Valence
/// Formation's forced Radiation on a Radiation-immune unit). Only the DISPLAY
/// is fooled; nothing is applied, and the random draws below renormalise
/// exactly as they already did.
///
/// Filtered here rather than at each caller because "forced" arrives from three
/// places — the attack part, an extra hit, a syndicate radial — and an immunity
/// that held for two of them would be the worst kind of half-rule.
pub fn procs_for_hit(
    forced: &[DamageType],
    status_chance: f64,
    vector: &DamageVector,
    immune: &[DamageType],
    rng: &mut Rng,
) -> Vec<DamageType> {
    let mut procs: Vec<DamageType> =
        forced.iter().copied().filter(|t| !immune.contains(t)).collect();
    for _ in 0..roll_proc_count(status_chance, rng) {
        if let Some(t) = draw_proc_type(vector, immune, rng) {
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

    /// THE WIKI'S OWN WORKED EXAMPLE for a status immunity, reproduced to the
    /// digit. VERBATIM (`Status_Effect` §Status Immunity Interactions):
    ///
    /// > Proc type chances are not altered by enemy resistances or weaknesses
    /// > to the damage components used in their computation; however, they are
    /// > modified by enemy status immunities. When an attack procs a status
    /// > effect on an enemy which is immune to a particular proc type, the
    /// > respective damage type is excluded from proc type chance calculations
    /// > for status effects on that enemy.
    ///
    /// Its table, for Impact 20 / Puncture 5 / Slash 10 / Heat 25 / Corrosive
    /// 50 against a unit immune to the Corrosion STATUS:
    ///
    /// | Impact | Puncture | Slash | Heat | Corrosive |
    /// | 33.33% | 8.33% | 16.67% | 41.67% | N/A |
    ///
    /// i.e. the denominator loses the 50 and becomes 60. And the clause that
    /// keeps the two mechanics apart is in the same paragraph: this holds
    /// "regardless of whether that enemy is also immune to Corrosive damage".
    #[test]
    fn the_wikis_worked_example_for_a_status_immunity_reproduces() {
        let v = DamageVector::new()
            .with(DamageType::Impact, 20.0)
            .with(DamageType::Puncture, 5.0)
            .with(DamageType::Slash, 10.0)
            .with(DamageType::Heat, 25.0)
            .with(DamageType::Corrosive, 50.0);
        let immune = [DamageType::Corrosive];

        let n = 400_000;
        let mut rng = Rng::new(0xC0FFEE);
        let (mut imp, mut pun, mut sla, mut hea, mut cor) = (0u32, 0u32, 0u32, 0u32, 0u32);
        for _ in 0..n {
            match draw_proc_type(&v, &immune, &mut rng) {
                Some(DamageType::Impact) => imp += 1,
                Some(DamageType::Puncture) => pun += 1,
                Some(DamageType::Slash) => sla += 1,
                Some(DamageType::Heat) => hea += 1,
                Some(DamageType::Corrosive) => cor += 1,
                _ => {}
            }
        }
        assert_eq!(cor, 0, "N/A means it never draws, not that it draws rarely");
        let pct = |k: u32| k as f64 / n as f64 * 100.0;
        for (got, want, name) in [
            (pct(imp), 33.33, "Impact"),
            (pct(pun), 8.33, "Puncture"),
            (pct(sla), 16.67, "Slash"),
            (pct(hea), 41.67, "Heat"),
        ] {
            assert!(
                (got - want).abs() < 0.3,
                "{name}: {got:.2}% against the wiki's {want}%"
            );
        }

        // …AND THE DAMAGE COLUMN DOES NOT TOUCH ANY OF IT. The same vector
        // against no immunity is the unrenormalised table, which is what makes
        // the two mechanics separable: a x0 Corrosive column would change what
        // the hit DEALS and leave these five numbers exactly as they are.
        let mut rng = Rng::new(0xC0FFEE);
        let mut cor_free = 0u32;
        for _ in 0..n {
            if draw_proc_type(&v, &[], &mut rng) == Some(DamageType::Corrosive) {
                cor_free += 1;
            }
        }
        assert!(
            (pct(cor_free) - 45.45).abs() < 0.3,
            "without the immunity Corrosive is 50/110 = 45.45%, got {:.2}%",
            pct(cor_free)
        );
    }

    /// AN IMMUNITY BEATS A GUARANTEE, and the random draws are untouched by it.
    ///
    /// Valence Formation forces a Radiation proc on every hit. Against a
    /// Radiation-immune unit the game still draws the proc icon beside the
    /// damage number and the status never lands — so the
    /// forced list is filtered by the same immunity the draw already obeys.
    ///
    /// The second half is the one worth pinning: the ordinary rolls behave
    /// exactly as they do without a forced proc at all. An implementation that
    /// "used up" the forced proc, or that let it through and then removed the
    /// status later, would move the other types' frequencies.
    #[test]
    fn a_forced_proc_of_an_immune_type_never_lands() {
        let v = DamageVector::new()
            .with(DamageType::Radiation, 50.0)
            .with(DamageType::Slash, 50.0);
        let forced = [DamageType::Radiation];
        let immune = [DamageType::Radiation];

        let mut rng = Rng::new(0x7A1E);
        let (mut rads, mut slashes, mut total) = (0u32, 0u32, 0u32);
        for _ in 0..20_000 {
            let procs = procs_for_hit(&forced, 0.40, &v, &immune, &mut rng);
            total += procs.len() as u32;
            rads += procs.iter().filter(|&&t| t == DamageType::Radiation).count() as u32;
            slashes += procs.iter().filter(|&&t| t == DamageType::Slash).count() as u32;
        }
        assert_eq!(rads, 0, "the guarantee does not beat the immunity");
        assert_eq!(slashes, total, "…and Slash takes every roll, renormalised");

        // THE CONTROL: the same hit on a unit that CAN take it gets the forced
        // proc on every single hit, so the count is at least one per hit.
        let mut rng = Rng::new(0x7A1E);
        let mut rads = 0u32;
        for _ in 0..20_000 {
            rads += procs_for_hit(&forced, 0.40, &v, &[], &mut rng)
                .iter()
                .filter(|&&t| t == DamageType::Radiation)
                .count() as u32;
        }
        assert!(rads >= 20_000, "forced means every hit: {rads}");
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
            let procs = procs_for_hit(&forced, 0.40, &v, &[], &mut rng);
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
        let procs = procs_for_hit(
            &[DamageType::Impact],
            0.0,
            &DamageVector::new(),
            &[],
            &mut rng,
        );
        assert_eq!(procs, vec![DamageType::Impact]);
    }

    #[test]
    fn rolled_procs_need_positive_damage_to_be_typed() {
        // 100% SC but an all-zero vector: the roll succeeds but there is no
        // type to draw -> no rolled proc.
        let mut rng = Rng::new(2);
        let procs = procs_for_hit(&[], 1.0, &DamageVector::new(), &[], &mut rng);
        assert!(procs.is_empty());
    }

    #[test]
    fn immunity_renormalizes_the_type_draw() {
        // Wiki example: I20/P5/S10/H25/C50 vs a Corrosion-status-immune
        // enemy -> Corrosive excluded, weights renormalize over 60:
        // Impact 33.33%, Puncture 8.33%, Slash 16.67%, Heat 41.67%.
        let v = DamageVector::new()
            .with(DamageType::Impact, 20.0)
            .with(DamageType::Puncture, 5.0)
            .with(DamageType::Slash, 10.0)
            .with(DamageType::Heat, 25.0)
            .with(DamageType::Corrosive, 50.0);
        let immune = [DamageType::Corrosive];
        let mut rng = Rng::new(0xC0DE);
        let n = 120_000;
        let (mut i, mut p, mut sl, mut h) = (0u32, 0u32, 0u32, 0u32);
        for _ in 0..n {
            match draw_proc_type(&v, &immune, &mut rng).unwrap() {
                DamageType::Impact => i += 1,
                DamageType::Puncture => p += 1,
                DamageType::Slash => sl += 1,
                DamageType::Heat => h += 1,
                DamageType::Corrosive => panic!("immune type drawn"),
                other => panic!("impossible type {other:?}"),
            }
        }
        let f = |c: u32| c as f64 / n as f64;
        assert!((f(i) - 1.0 / 3.0).abs() < 0.01);
        assert!((f(p) - 1.0 / 12.0).abs() < 0.01);
        assert!((f(sl) - 1.0 / 6.0).abs() < 0.01);
        assert!((f(h) - 5.0 / 12.0).abs() < 0.01);
        // All-immune vector: no proc possible.
        assert_eq!(
            draw_proc_type(
                &DamageVector::new().with(DamageType::Viral, 9.0),
                &[DamageType::Viral],
                &mut rng
            ),
            None
        );
    }

    #[test]
    fn single_type_vector_always_draws_that_type() {
        let v = DamageVector::new().with(DamageType::Radiation, 5.0);
        let mut rng = Rng::new(3);
        for _ in 0..1000 {
            assert_eq!(
                draw_proc_type(&v, &[], &mut rng),
                Some(DamageType::Radiation)
            );
        }
    }
}
