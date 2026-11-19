// SPDX-License-Identifier: AGPL-3.0-or-later
//! Exact classes for the primary form's `panel.damage`, not whole panels.
//!
//! In `resolve_for`'s `build_in`, the moving prefix contributes only direct
//! `Element`s. Each group retains its card order (thus its exact FP fold in
//! `ElementalInput::push`). The fixed tail cannot place another primary.
//! `StatusChance` may fold differently but never feeds `build_in`'s damage.
//! The physical vector, element scale and direct secondary stream are fixed.
//!
//! `rules::elements::combine` merges innate (HCET) and injected terms into
//! existing amounts by type or appends them in a fixed order. Two orders in
//! a class differ only by swapping complete pairs or reversing a pair, so the
//! odd leftover and its appended partner are unchanged. Each pair writes a
//! distinct type (`combined_of`); reversing two finite summands is exact, and
//! changing the order of independent writes changes no sum. Fixed secondaries
//! land afterward. Nothing here equates different partitions by magnitude.

use wfsim_engine::build::loadout::ResolvedPanel;
use wfsim_engine::model::{IndirectStat, ModDef, ModEffect, WeaponBase};
use wfsim_engine::rules::damage::DamageType;

pub(super) fn anchor(
    elems: &[DamageType],
    pool: &[ModDef],
    subset: &[usize],
    base: &WeaponBase,
) -> Option<DamageType> {
    if !(2..=4).contains(&elems.len())
        || !elems.iter().all(|t| t.is_primary_element())
        || !DamageType::ALL
            .iter()
            .all(|&t| base.base_vector.get(t).is_finite())
        || !base
            .injected_elements
            .iter()
            .all(|(t, v)| t.is_primary_element() && v.is_finite())
    {
        return None;
    }
    subset
        .iter()
        .all(|&i| {
            let m = &pool[i];
            let Some(t) = m.primary_element() else {
                return fixed_tail(m);
            };
            let mut found = false;
            plain(m)
                && m.effects.iter().all(|e| match *e {
                    ModEffect::Element(ty, v) if !found && ty == t && v.is_finite() => {
                        found = true;
                        true
                    }
                    ModEffect::StatusChance(v) => v.is_finite(),
                    _ => false,
                })
                && found
        })
        .then_some(elems[0])
}

fn plain(m: &ModDef) -> bool {
    m.requires.is_none() && m.disables.is_empty() && m.set.is_none() && m.stance.is_none()
}

pub(super) fn fixed_tail(m: &ModDef) -> bool {
    plain(m)
        && m.effects.iter().all(|e| match *e {
            ModEffect::BaseDamage(v)
            | ModEffect::Multishot(v)
            | ModEffect::CritChance(v)
            | ModEffect::CritDamage(v)
            | ModEffect::StatusChance(v)
            | ModEffect::FireRate(v)
            | ModEffect::ReloadSpeed(v)
            | ModEffect::MagazineCapacity(v)
            | ModEffect::Indirect(
                IndirectStat::ProjectileSpeed
                | IndirectStat::Recoil
                | IndirectStat::Noise
                | IndirectStat::AmmoMax
                | IndirectStat::AmmoConversion,
                v,
            ) => v.is_finite(),
            ModEffect::CombinedElement(t, v) => t.is_secondary_element() && v.is_finite(),
            _ => false,
        })
}

pub(super) fn pair_class(order: &[DamageType], anchor: DamageType) -> u32 {
    // With at most four fixed groups, the anchor's partner (or itself when
    // unpaired) uniquely identifies the partition, including any leftover.
    let pos = order.iter().position(|&t| t == anchor).unwrap();
    1 << order.get(pos ^ 1).copied().unwrap_or(anchor) as u32
}

pub(super) fn finite_damage(panel: &ResolvedPanel) -> bool {
    // Inspect raw slots, including negative values and NaNs hidden by
    // iter_nonzero. With only additive writes, nonfinite pair arithmetic
    // cannot disappear from its damage slot. Other classes remain independent.
    panel.modified_base.is_finite()
        && DamageType::ALL
            .iter()
            .all(|&t| panel.damage.get(t).is_finite())
}
