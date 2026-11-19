// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use std::time::{Duration, Instant};
use wfsim_engine::data::{
    arcanes::ArcaneFx,
    mods::{at_rank, class_pool},
    tenno::default_tenno,
};
use wfsim_engine::fight::{monte_carlo, FightParams};
use wfsim_engine::model::{ModEffect, TennoCondition};
use wfsim_engine::rules::damage::DamageType::*;

#[derive(Default)]
pub(super) struct Probe {
    pub(super) legacy: bool,
    pub(super) primary_resolves: usize,
    pub(super) second_resolves: usize,
}

fn cards() -> Vec<ModDef> {
    let rifle = class_pool("rifle");
    [
        "stormbringer",
        "thermite_rounds@1",
        "cryo_rounds@2",
        "infected_clip",
        "hellfire",
        "serration",
        "terminal_velocity@1",
    ]
    .into_iter()
    .map(|id| {
        at_rank(id)
            .or_else(|| rifle.iter().find(|m| m.id == id).cloned())
            .unwrap_or_else(|| panic!("missing {id}"))
    })
    .collect()
}

struct Case<'a> {
    pool: &'a [ModDef],
    base: &'a WeaponBase,
    second: Option<&'a WeaponBase>,
    exilus: &'a [Option<&'a ModDef>],
    cap: u32,
}

impl Case<'_> {
    fn run(&self, subset: &[usize], legacy: bool) -> (Vec<Candidate>, EnumStats, Probe) {
        let mut probe = Probe {
            legacy,
            ..Probe::default()
        };
        let mut stats = EnumStats::default();
        let mut out = Vec::new();
        expand_subset(
            self.pool,
            self.base,
            self.second,
            17,
            self.cap,
            &[
                Some(Polarity::Madurai),
                None,
                Some(Polarity::Vazarin),
                None,
                None,
                None,
                None,
                None,
            ],
            self.exilus,
            subset,
            default_tenno(),
            StackPolicy::Emergent,
            &mut stats,
            &mut out,
            Some(&mut probe),
        );
        (out, stats, probe)
    }

    fn diff(&self, subset: &[usize], calls: (usize, usize)) -> Vec<Candidate> {
        let (old, old_stats, old_probe) = self.run(subset, true);
        let (new, new_stats, new_probe) = self.run(subset, false);
        assert_eq!(
            (old_probe.primary_resolves, new_probe.primary_resolves),
            calls
        );
        assert_eq!(old_stats.order_variants as usize, calls.0);
        assert_eq!(
            old_stats.order_variants,
            old_stats.deduped + old.len() as u64
        );
        assert_eq!(format!("{old_stats:?}"), format!("{new_stats:?}"));
        assert_eq!(new.len(), old.len());
        assert_eq!(new_probe.second_resolves, old_probe.second_resolves);
        assert_eq!(
            new_probe.second_resolves,
            new.len() * usize::from(self.second.is_some())
        );
        for (old, new) in old.iter().zip(&new) {
            // ResolvedPanel has no PartialEq. Its derived Debug covers every
            // field (including both forms, ordered vectors and Forma slots).
            assert_eq!(format!("{old:?}"), format!("{new:?}"));
        }
        new
    }
}

#[test]
fn zero_to_four_groups_preserve_candidates_and_simulation_work() {
    let mut pool = cards();
    let mut combined = pool[5].clone();
    combined.id = "test_direct_magnetic";
    combined.effects = vec![ModEffect::CombinedElement(Magnetic, 0.37)];
    pool.push(combined);
    let mut base = WeaponBase::from_data("braton_prime", false, &[]);
    base.base_vector.add(Cold, 7.0);
    base.base_vector.add(Electricity, 9.0);
    base.injected_elements.push((Toxin, 0.4));
    let mut second = WeaponBase::from_data("braton_prime", false, &[]);
    second.base_vector.add(Heat, 13.0);
    let case = Case {
        pool: &pool,
        base: &base,
        second: Some(&second),
        exilus: &[None, Some(&pool[6])],
        cap: 60,
    };

    // Non-sorted discovery order; repeated heat cards keep their own order.
    let groups = [vec![4, 1], vec![3], vec![0], vec![2]];
    for n in 0..=4 {
        let subset: Vec<_> = groups[..n]
            .iter()
            .flatten()
            .copied()
            .chain([5, 7])
            .collect();
        let calls = ([2, 2, 4, 12, 48][n], [2, 2, 2, 6, 6][n]);
        let got = case.diff(&subset, calls);
        assert_eq!(got[0].ordered, subset);
        assert_eq!(got.last().unwrap().exilus, 1);
        assert_eq!(got.last().unwrap().variant, 17);
        if n == 4 {
            let (legacy, _, _) = case.run(&subset, true);
            let arena = wfsim_engine::arena::Arena::training(2.0);
            for (i, (old, new)) in legacy.iter().zip(&got).enumerate() {
                let seed = crate::job_seed(0xBEEF_CAFE, i, 0);
                let run = |c: &Candidate| {
                    let params = FightParams::from_panel(&c.panel, &arena, &ArcaneFx::none());
                    monte_carlo(&params, 2, seed)
                };
                let (old, new) = (run(old), run(new));
                if option_env!("WFSIM_GAS_OFF").is_none() {
                    assert!(new.work > wfsim_engine::fight::gas::work_of(0, 2));
                }
                assert_eq!(old.work, new.work);
                assert_eq!(format!("{old:?}"), format!("{new:?}"));
            }
        }
    }

    // Same groups, different discovery order and a different FP fold within
    // the repeated group. The oracle is the corresponding legacy DFS.
    let got = case.diff(&[2, 1, 0, 3, 4, 5, 7], (48, 6));
    assert_eq!(got[0].ordered, [2, 1, 4, 0, 3, 5, 7]);
}

#[test]
fn same_class_direct_vectors_match_bit_for_bit() {
    let mut pool = cards();
    // Exercise signed cancellation, rounding within a repeated group, and
    // an innate element merged into one of the paired amounts.
    pool[4].effects = vec![ModEffect::Element(Heat, -0.103_630_103_5)];
    let mut base = WeaponBase::from_data("braton_prime", false, &[]);
    base.base_vector.add(Electricity, 0.123_456_789);
    base.injected_elements.push((Toxin, 0.047_110_007));
    let groups = [Heat, Electricity, Cold, Toxin];
    let subset = [1, 0, 4, 2, 3, 5];
    for n in 2..=4 {
        let mut orders = Vec::new();
        permutations(&groups[..n], &mut Vec::new(), &mut orders);
        let mut vectors = std::collections::HashMap::new();
        for order in orders {
            let mods: Vec<_> = order
                .iter()
                .flat_map(|&t| {
                    subset
                        .iter()
                        .map(|&i| &pool[i])
                        .filter(move |m| m.primary_element() == Some(t))
                })
                .chain([&pool[5], &pool[6]])
                .collect();
            let panel = resolve_for(&base, &mods, StackPolicy::Emergent, default_tenno());
            let bits = DamageType::ALL.map(|t| panel.damage.get(t).to_bits());
            let class = order_classes::pair_class(&order, groups[0]);
            if let Some(previous) = vectors.insert(class, bits) {
                assert_eq!(bits, previous, "{order:?}");
            }
        }
        assert_eq!(vectors.len(), if n == 2 { 1 } else { 3 });
    }
}

#[test]
fn cross_class_collisions_still_use_the_legacy_first_key() {
    let mut pool = cards();
    for i in [0, 1, 2, 3] {
        pool[i].effects = vec![ModEffect::Element(pool[i].primary_element().unwrap(), 0.0)];
    }
    let base = WeaponBase::from_data("braton_prime", false, &[]);
    let case = Case {
        pool: &pool,
        base: &base,
        second: Some(&base),
        exilus: &[None],
        cap: 60,
    };
    let got = case.diff(&[2, 1, 3, 0], (24, 3));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].ordered, [2, 1, 3, 0]);
}

#[test]
fn complex_cards_nonfinite_arithmetic_and_exilus_are_fail_closed() {
    let base = WeaponBase::from_data("braton_prime", false, &[]);
    let subset = &[0, 1, 2, 3, 5];
    let fallback = |pool: &[ModDef], base| {
        Case {
            pool,
            base,
            second: Some(base),
            exilus: &[None],
            cap: 60,
        }
        .diff(subset, (24, 24));
    };
    for effect in [
        ModEffect::Element(Toxin, 0.31), // Riven: last listed enters first.
        ModEffect::Element(Electricity, 0.37), // Two lines of the same element.
        ModEffect::WhileTenno(
            TennoCondition::Aiming,
            Box::new(ModEffect::Element(Heat, 0.3)),
        ),
        ModEffect::DamageConversion {
            to: Cold,
            physical: false,
        },
        ModEffect::BaseDamage(0.15), // A moving card can reorder other buckets.
        ModEffect::StatusChance(f64::NAN),
    ] {
        let mut pool = cards();
        pool[0].effects.push(effect);
        fallback(&pool, &base);
    }
    let mut pool = cards();
    pool[0].effects = vec![ModEffect::Element(Electricity, f64::MAX)];
    fallback(&pool, &base); // Finite input, overflow in build_in.
    let mut pool = cards();
    pool[0].disables.push("base_damage");
    fallback(&pool, &base);
    let mut pool = cards();
    pool[5].effects = vec![ModEffect::CombinedElement(Cold, 0.1)];
    fallback(&pool, &base); // Not really a secondary.

    let pool = cards();
    let mut bad_exilus = pool[6].clone();
    bad_exilus.effects.push(ModEffect::Element(Cold, 0.43));
    let mut same_card = pool[0].clone();
    same_card.exilus = true;
    let exilus = &[None, Some(&bad_exilus), Some(&same_card)];
    let case = Case {
        pool: &pool,
        base: &base,
        second: None,
        exilus,
        cap: 60,
    };
    let got = case.diff(subset, (48, 27));
    assert!(got.iter().all(|c| c.exilus < 2));
    let no_capacity = Case { cap: 0, ..case };
    let (_, stats, _) = no_capacity.run(subset, false);
    assert_eq!(stats.illegal, 2);
    assert!(no_capacity.diff(subset, (0, 0)).is_empty());
}

#[test]
#[ignore = "short local timing, run explicitly with --ignored --nocapture"]
fn time_legacy_and_classes() {
    let mut pool = cards();
    pool[6].effects.push(ModEffect::Element(Cold, 0.3));
    let base = WeaponBase::from_data("braton_prime", false, &[]);
    for (label, subset, options) in [
        ("zero", &[5][..], &[None][..]),
        ("four", &[0, 1, 2, 3, 4, 5], &[None]),
        ("fallback", &[0, 1, 2, 3, 4, 5], &[Some(&pool[6])]),
    ] {
        let case = Case {
            pool: &pool,
            base: &base,
            second: Some(&base),
            exilus: options,
            cap: 60,
        };
        let measure = |legacy| {
            let start = Instant::now();
            for _ in 0..128 {
                std::hint::black_box(case.run(std::hint::black_box(subset), legacy));
            }
            start.elapsed()
        };
        let mut times = [[Duration::ZERO; 5]; 2];
        measure(true);
        measure(false);
        let [old_times, new_times] = &mut times;
        for (i, (old, new)) in old_times.iter_mut().zip(new_times).enumerate() {
            if i % 2 == 0 {
                *old = measure(true);
                *new = measure(false);
            } else {
                *new = measure(false);
                *old = measure(true);
            }
        }
        for sample in &mut times {
            sample.sort_unstable();
        }
        let (_, _, old) = case.run(subset, true);
        let (_, _, new) = case.run(subset, false);
        eprintln!(
            "{label}: primary {} -> {}, second {} -> {}, median per 128 expansions {:?} -> {:?}",
            old.primary_resolves,
            new.primary_resolves,
            old.second_resolves,
            new.second_resolves,
            times[0][2],
            times[1][2]
        );
    }
}
