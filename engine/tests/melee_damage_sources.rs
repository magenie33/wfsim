// SPDX-License-Identifier: AGPL-3.0-or-later
use wfsim_engine::{build::loadout, data::{mods, tenno}, model::{BuffDecay, BuffGrant, BuffTrigger, ClearedBy, StackPolicy, StackingBuff, WeaponBase}};

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-8, "{actual} != {expected}");
}

#[test]
fn a_projectile_can_exclude_melee_damage_while_retaining_universal_damage() {
    let mut base = WeaponBase::from_data("syam_heavy", true, &[]);
    let pool = mods::pool_for_weapon("syam_heavy");
    let pressure = pool.iter().find(|m| m.id == "pressure_point").unwrap();
    let neutral = tenno::default_tenno();
    let mut bonus = neutral.clone();
    bonus.bonuses.base_damage = 2.0;
    let plain = loadout::resolve_for(&base, &[], StackPolicy::Emergent, neutral);
    let boosted = loadout::resolve_for(&base, &[pressure], StackPolicy::Emergent, &bonus);
    assert!(boosted.modified_base > plain.modified_base * 3.0);
    base.melee_damage_mods_apply = false;
    let excluded = loadout::resolve_for(&base, &[pressure], StackPolicy::Emergent, neutral);
    close(excluded.modified_base, plain.modified_base);
    let universal = loadout::resolve_for(&base, &[pressure], StackPolicy::Emergent, &bonus);
    close(universal.modified_base, plain.modified_base * 3.0);
    assert!(!universal.melee_damage_mods_apply);
    assert!(universal.universal_damage_mods_apply);
}

#[test]
fn a_pool_can_retain_melee_damage_while_excluding_universal_damage() {
    let mut base = WeaponBase::from_data("syam_heavy", true, &[]);
    let pool = mods::pool_for_weapon("syam_heavy");
    let pressure = pool.iter().find(|m| m.id == "pressure_point").unwrap();
    let mut tenno = tenno::default_tenno().clone();
    tenno.bonuses.base_damage = 2.0;
    let melee_only = loadout::resolve_for(&base, &[pressure], StackPolicy::Emergent, tenno::default_tenno());
    base.universal_damage_mods_apply = false;
    let excluded = loadout::resolve_for(&base, &[pressure], StackPolicy::Emergent, &tenno);
    close(excluded.modified_base, melee_only.modified_base);
    assert!(excluded.melee_damage_mods_apply);
    assert!(!excluded.universal_damage_mods_apply);
}

#[test]
fn melee_sourced_live_damage_keeps_its_source_when_the_universal_bucket_is_disabled() {
    let mut base = WeaponBase::from_data("syam_heavy", true, &[]);
    base.stacking_buffs.push(StackingBuff {
        id: "fixture_melee_damage", trigger: BuffTrigger::Crit, grant: BuffGrant::BaseDamage,
        per_stack: 1.8, max_stacks: 1, duration: 18.0, chance: 1.0,
        decay: BuffDecay::LoseOneAndReset, initial_stacks: 1, stacks_per_trigger: 1,
        per_shell: false, cleared_by: ClearedBy::Nothing, precombat: false,
    });
    base.universal_damage_mods_apply = false;
    let p = loadout::resolve(&base, &[], StackPolicy::Emergent);
    assert_eq!(p.stacking_buffs[0].grant, BuffGrant::MeleeDamage);
    base.melee_damage_mods_apply = false;
    assert!(loadout::resolve(&base, &[], StackPolicy::Emergent).stacking_buffs.is_empty());
}
