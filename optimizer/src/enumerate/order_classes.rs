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
use wfsim_engine::model::{AcidShellsPart, ModDef, ModEffect, WeaponBase};
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
    m.stance.is_none() && m.effects.iter().all(fixed_tail_effect)
}

fn fixed_tail_effect(e: &ModEffect) -> bool {
    match *e {
        ModEffect::Element(..)
        | ModEffect::DamageConversion { .. }
        | ModEffect::PostCoalesceElement(..) => false,
        ModEffect::CombinedElement(t, v) => t.is_secondary_element() && v.is_finite(),
        ModEffect::WhileTenno(_, ref inner) => fixed_tail_effect(inner),
        ModEffect::BaseDamage(v)
        | ModEffect::Multishot(v)
        | ModEffect::CritChance(v)
        | ModEffect::CritDamage(v)
        | ModEffect::StatusChance(v)
        | ModEffect::PostModStatusChance(v)
        | ModEffect::FireRate(v)
        | ModEffect::ChargeRate(v)
        | ModEffect::ReloadSpeed(v)
        | ModEffect::MagazineCapacity(v)
        | ModEffect::FlatBaseMagazine(v)
        | ModEffect::StatusDamage(v)
        | ModEffect::StatusDuration(v)
        | ModEffect::SlashOnCrit(v)
        | ModEffect::AmmoEfficiency(v)
        | ModEffect::BlastRadius(v)
        | ModEffect::WeakpointDamage(v)
        | ModEffect::HeadshotDamage(v)
        | ModEffect::WeakpointCritChance(v)
        | ModEffect::LastRoundDamage(v)
        | ModEffect::FirstRoundDamage(v)
        | ModEffect::AddedSpread(v)
        | ModEffect::OverguardDamage(v)
        | ModEffect::NonCritDamage(v)
        | ModEffect::MagazineRefillOnKill(v)
        | ModEffect::ComboDuration(v)
        | ModEffect::LingeringAreaFraction(v)
        | ModEffect::Indirect(_, v)
        | ModEffect::FactionDamage(_, v)
        | ModEffect::Physical(_, v)
        | ModEffect::CondBuff(_, v) => v.is_finite(),
        ModEffect::GrantsStackingBuff(ref b) => {
            b.per_stack.is_finite() && b.duration.is_finite() && b.chance.is_finite()
        }
        ModEffect::OnKillMultishot { per_stack, duration, .. }
        | ModEffect::ConditionOverload { per_stack, duration, .. }
        | ModEffect::OnHeadshotKillCritChance { per_stack, duration, .. }
        | ModEffect::ConsecutiveHitDamage { per_stack, duration, .. } => {
            per_stack.is_finite() && duration.is_finite()
        }
        ModEffect::OnHeadshotCritChance { bonus, duration }
        | ModEffect::OnKillCritDamage { bonus, duration }
        | ModEffect::OnReloadFireRate { bonus, duration }
        | ModEffect::OnReloadDamage { bonus, duration }
        | ModEffect::OnEximusWeakpointDamage { bonus, duration } => {
            bonus.is_finite() && duration.is_finite()
        }
        ModEffect::BaseDamageIfNoEnemyWithin { bonus, metres }
        | ModEffect::BaseDamageBeyondTargetRange { bonus, metres } => {
            bonus.is_finite() && metres.is_finite()
        }
        ModEffect::PerTendril { crit_chance, status_chance } => {
            crit_chance.is_finite() && status_chance.is_finite()
        }
        ModEffect::CritChancePerHit(c) => c.per_stack.is_finite() && c.max_bonus.is_finite(),
        ModEffect::ProcConversion { chance, low_rate_threshold, low_rate_multiplier, .. } => {
            chance.is_finite() && low_rate_threshold.is_finite() && low_rate_multiplier.is_finite()
        }
        ModEffect::SyndicateRadial { amount, .. } => amount.is_finite(),
        ModEffect::AcidShells(part) => match part {
            AcidShellsPart::FlatDamage(v)
            | AcidShellsPart::HealthFraction(v)
            | AcidShellsPart::RadiusM(v) => v.is_finite(),
        },
        ModEffect::GrantsLingering(_) | ModEffect::AdditionalForcedProcs(..) => true,
        _ => false,
    }
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
