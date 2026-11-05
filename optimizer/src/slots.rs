// SPDX-License-Identifier: AGPL-3.0-or-later
//! Element orders that can occupy prescribed physical slots.
use crate::{rebuild_candidate, Candidate};
use std::collections::HashSet;
use wfsim_engine::{
    data::tenno::Tenno,
    model::{ModDef, StackPolicy, WeaponBase},
    rules::{capacity::Polarity, damage::DamageType},
};

/// Enumerate each reachable element order, retaining one layout per damage vector.
#[allow(clippy::too_many_arguments)]
pub fn expand_with_slots(
    pool: &[ModDef],
    base: &WeaponBase,
    variant: u32,
    cap: u32,
    innate: &[Option<Polarity>],
    exilus: Option<&ModDef>,
    subset: &[usize],
    slots: &[Option<usize>; 8],
    fixed: u8,
    tenno: &Tenno,
    policy: StackPolicy,
) -> Vec<Candidate> {
    let mut elements = Vec::new();
    for &i in subset {
        if let Some(t) = pool[i].primary_element() {
            if !elements.contains(&t) {
                elements.push(t);
            }
        }
    }
    let mut orders = Vec::new();
    permutations(&elements, &mut Vec::new(), &mut orders);
    let mut free: Vec<usize> = subset
        .iter()
        .copied()
        .filter(|&i| {
            pool[i].stance.is_none()
                && !(0..8).any(|s| fixed & (1 << s) != 0 && slots[s] == Some(i))
        })
        .collect();
    free.sort_unstable();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for order in orders {
        let mut layout = [None; 8];
        let mut dead = HashSet::new();
        if !seat(
            pool,
            &free,
            slots,
            fixed,
            &order,
            &mut layout,
            0,
            0,
            0,
            &mut dead,
        ) {
            continue;
        }
        let mut ordered: Vec<usize> = layout.iter().flatten().copied().collect();
        ordered.extend(subset.iter().copied().filter(|&i| pool[i].stance.is_some()));
        let Some(mut c) = rebuild_candidate(
            pool,
            base,
            None,
            innate,
            cap,
            tenno,
            policy,
            &ordered,
            variant,
            0,
            &[exilus],
        ) else {
            continue;
        };
        let vector: Vec<_> = c
            .panel
            .damage
            .iter_nonzero()
            .map(|(t, v)| (t, (v * 1e6).round() as i64))
            .collect();
        if seen.insert(vector) {
            c.slots = Some(layout);
            out.push(c);
        }
    }
    out
}

fn permutations(rest: &[DamageType], prefix: &mut Vec<DamageType>, out: &mut Vec<Vec<DamageType>>) {
    if rest.is_empty() {
        out.push(prefix.clone());
        return;
    }
    for (i, &t) in rest.iter().enumerate() {
        prefix.push(t);
        let next: Vec<_> = rest
            .iter()
            .enumerate()
            .filter_map(|(j, &v)| (i != j).then_some(v))
            .collect();
        permutations(&next, prefix, out);
        prefix.pop();
    }
}

#[allow(clippy::too_many_arguments)]
fn seat(
    pool: &[ModDef],
    free: &[usize],
    pinned: &[Option<usize>; 8],
    fixed: u8,
    order: &[DamageType],
    layout: &mut [Option<usize>; 8],
    slot: usize,
    used: u8,
    seen: usize,
    dead: &mut HashSet<(usize, u8, usize)>,
) -> bool {
    if slot == 8 {
        return used.count_ones() as usize == free.len() && seen == order.len();
    }
    if dead.contains(&(slot, used, seen)) {
        return false;
    }
    let next_seen = |m: Option<usize>| -> Option<usize> {
        let Some(t) = m.and_then(|i| pool[i].primary_element()) else {
            return Some(seen);
        };
        let i = order.iter().position(|&x| x == t)?;
        (i <= seen).then_some(seen.max(i + 1))
    };
    if fixed & (1 << slot) != 0 {
        layout[slot] = pinned[slot];
        if let Some(n) = next_seen(pinned[slot]) {
            if seat(
                pool,
                free,
                pinned,
                fixed,
                order,
                layout,
                slot + 1,
                used,
                n,
                dead,
            ) {
                return true;
            }
        }
    } else {
        let mut kinds = Vec::new();
        for (i, &m) in free.iter().enumerate() {
            let kind = pool[m].primary_element();
            if used & (1 << i) != 0 || kinds.contains(&kind) {
                continue;
            }
            kinds.push(kind);
            let Some(n) = next_seen(Some(m)) else {
                continue;
            };
            layout[slot] = Some(m);
            if seat(
                pool,
                free,
                pinned,
                fixed,
                order,
                layout,
                slot + 1,
                used | (1 << i),
                n,
                dead,
            ) {
                return true;
            }
        }
        layout[slot] = None;
        if seat(
            pool,
            free,
            pinned,
            fixed,
            order,
            layout,
            slot + 1,
            used,
            seen,
            dead,
        ) {
            return true;
        }
    }
    dead.insert((slot, used, seen));
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use wfsim_engine::build::loadout::resolve_for;

    fn vector(panel: &wfsim_engine::build::loadout::ResolvedPanel) -> Vec<(DamageType, i64)> {
        panel
            .damage
            .iter_nonzero()
            .map(|(t, v)| (t, (v * 1e6).round() as i64))
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn brute(
        pool: &[ModDef],
        base: &WeaponBase,
        pinned: &[Option<usize>; 8],
        fixed: u8,
        free: &mut Vec<usize>,
        layout: &mut [Option<usize>; 8],
        at: usize,
        out: &mut HashSet<Vec<(DamageType, i64)>>,
    ) {
        if at == 8 {
            if free.is_empty() {
                let refs: Vec<_> = layout.iter().flatten().map(|&i| &pool[i]).collect();
                out.insert(vector(&resolve_for(
                    base,
                    &refs,
                    StackPolicy::Emergent,
                    wfsim_engine::data::tenno::default_tenno(),
                )));
            }
            return;
        }
        if fixed & (1 << at) != 0 {
            layout[at] = pinned[at];
            brute(pool, base, pinned, fixed, free, layout, at + 1, out);
            return;
        }
        layout[at] = None;
        brute(pool, base, pinned, fixed, free, layout, at + 1, out);
        for i in 0..free.len() {
            let card = free.remove(i);
            layout[at] = Some(card);
            brute(pool, base, pinned, fixed, free, layout, at + 1, out);
            free.insert(i, card);
        }
    }

    #[test]
    fn constrained_orders_equal_independent_physical_slot_enumeration() {
        let ids = [
            "heated_charge",
            "scorch",
            "deep_freeze",
            "pathogen_rounds",
            "hornet_strike",
        ];
        let pool: Vec<_> = crate::pool()
            .into_iter()
            .filter(|m| ids.contains(&m.id))
            .collect();
        assert_eq!(pool.len(), ids.len());
        let id = |s| pool.iter().position(|m| m.id == s).unwrap();
        let base = WeaponBase::from_data("lex", true, &[]);
        for entries in [
            vec![
                (0, Some(id("hornet_strike"))),
                (1, Some(id("heated_charge"))),
                (4, Some(id("deep_freeze"))),
                (6, None),
                (7, None),
            ],
            vec![
                (0, Some(id("pathogen_rounds"))),
                (3, Some(id("deep_freeze"))),
                (7, Some(id("heated_charge"))),
            ],
        ] {
            let mut pinned = [None; 8];
            let mut fixed = 0;
            for (slot, card) in entries {
                pinned[slot] = card;
                fixed |= 1 << slot;
            }
            let subset: Vec<_> = (0..pool.len()).collect();
            let actual = expand_with_slots(
                &pool,
                &base,
                0,
                100,
                &[None; 9],
                None,
                &subset,
                &pinned,
                fixed,
                wfsim_engine::data::tenno::default_tenno(),
                StackPolicy::Emergent,
            );
            let mut expected = HashSet::new();
            let mut free: Vec<_> = subset
                .iter()
                .copied()
                .filter(|i| !pinned.contains(&Some(*i)))
                .collect();
            brute(
                &pool,
                &base,
                &pinned,
                fixed,
                &mut free,
                &mut [None; 8],
                0,
                &mut expected,
            );
            assert_eq!(
                actual
                    .iter()
                    .map(|c| vector(&c.panel))
                    .collect::<HashSet<_>>(),
                expected
            );
            for c in actual {
                for (i, &pin) in pinned.iter().enumerate() {
                    if fixed & (1 << i) != 0 {
                        assert_eq!(c.slots.unwrap()[i], pin);
                    }
                }
            }
        }
    }
}
