// SPDX-License-Identifier: AGPL-3.0-or-later
//! Unit tests for weapon-specific bespoke combat systems.
use super::*;
use crate::arena::Arena;
use crate::build::loadout::{resolve, resolve_for};
use crate::data::arcanes::ArcaneFx;
use crate::model::{StackPolicy, WeaponBase};
use crate::rules::damage::DamageType;

fn build_fight(id: &str, mod_ids: &[&str], duration: f64) -> FightParams {
    let base = WeaponBase::from_data(id, true, &[]);
    let pool = crate::data::mods::pool_for_weapon(id);
    let picked: Vec<&crate::model::ModDef> = mod_ids
        .iter()
        .map(|mid| pool.iter().find(|m| m.id == *mid).unwrap_or_else(|| panic!("{id}: missing mod {mid}")))
        .collect();
    let panel = resolve(&base, &picked, StackPolicy::Emergent);
    let mut p = FightParams::from_panel(&panel, &Arena::training(duration), &ArcaneFx::none());
    p.foe = frail_target(TargetMode::InfiniteHealth, 0.0, 0.0);
    p
}

#[test]
fn battery_reload_speed_scales_delay_and_keeps_refill_rate_constant() {
    // Shedu: 7 magazine, 28/s regen (0.25 s refill), 1.0 s empty delay -> 1.25 s base reload.
    // With +100% reload speed, delay halves to 0.5 s while refill stays 0.25 s -> 0.75 s total.
    let mut p = build_fight("shedu", &[], 10.0);
    p.reload_bonus = 1.0;
    let mut arc = ArcRuntime::init(&p);
    let t = super::super::reload::live_reload_time_for_capacity(&p, &p, &mut arc, 0.0, 0.0, true, p.magazine_size);
    assert!((t - 0.75).abs() < 1e-9, "expected 0.5 + 7/28 = 0.75 s, got {t}");
}

#[test]
fn battery_refill_does_not_trigger_reload_completion_buffs() {
    let mut p = build_fight("shedu", &[], 20.0);
    p.stacking_buffs.push(crate::model::StackingBuff {
        id: "test_reload_buff",
        trigger: crate::model::BuffTrigger::ReloadFromEmpty,
        grant: crate::model::BuffGrant::BaseDamage,
        per_stack: 5.0,
        max_stacks: 1,
        duration: 30.0,
        chance: 1.0,
        decay: crate::model::BuffDecay::AllAtOnce,
        initial_stacks: 0,
        stacks_per_trigger: 1,
        per_shell: false,
        cleared_by: crate::model::ClearedBy::Nothing,
        precombat: false,
    });
    let with_buff = monte_carlo(&p, 1, 7);
    p.stacking_buffs.clear();
    let without_buff = monte_carlo(&p, 1, 7);
    assert!(with_buff.mean_reloads >= 2.0);
    assert!((with_buff.mean_damage - without_buff.mean_damage).abs() < 1e-6);
}

#[test]
fn kohm_pellet_and_ammo_spool_ramps_and_decays_across_pauses() {
    let ps = crate::data::weapons::spec("kuva_kohm")
        .and_then(|s| s.bespoke.as_deref())
        .and_then(|b| b.pellet_spool)
        .expect("kuva_kohm pellet spool");
    let expected_ammo = [1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 3.0, 3.0, 3.0, 4.0, 4.0, 4.0];
    for (shot, &want_ammo) in expected_ammo.iter().enumerate() {
        assert_eq!(ps.pellets_at(shot as f64), (shot as u32) + 1);
        assert_eq!(ps.ammo_cost_at(shot as f64), want_ammo);
    }
    let p = build_fight("kuva_kohm", &[], 6.0);
    let r = monte_carlo(&p, 1, 11);
    assert!(r.mean_shots > 10.0);
    // Average pellets per shot across ramp is strictly below the spooled 12 ceiling on a short magazine cycle.
    let mut unspooled = p.clone();
    unspooled.bespoke.as_mut().unwrap().pellet_spool = None;
    let r_flat = monte_carlo(&unspooled, 1, 11);
    assert!(r.mean_damage < r_flat.mean_damage, "pellet spool starts at 1 pellet: {} vs {}", r.mean_damage, r_flat.mean_damage);
}

#[test]
fn fan_hammer_burst_grows_with_modded_magazine_capacity() {
    let bare = build_fight("pandero_alt", &[], 15.0);
    let modded = build_fight("pandero_alt", &["primed_slip_magazine"], 15.0);
    assert_eq!(bare.burst.unwrap().count, 8);
    assert!(modded.burst.unwrap().count > 8, "modded magazine grows FanHammer burst count: {}", modded.burst.unwrap().count);
}

#[test]
fn reconifex_perfect_reload_shortens_downtime_and_adds_heat_procs() {
    let modes = crate::data::weapons::play_modes("reconifex");
    let pr_mode = modes.iter().find(|m| m.id == "perfect_reload").expect("perfect_reload mode");
    let mut base_p = build_fight("reconifex", &[], 45.0);
    base_p.status_chance = 0.0;
    base_p.base_status_chance = 0.0;
    let mut pr_p = base_p.clone();
    pr_p.apl_inserted = pr_mode.rules();

    let normal = monte_carlo(&base_p, 1, 5);
    let perfect = monte_carlo(&pr_p, 1, 5);
    assert!(perfect.mean_downtime_seconds < normal.mean_downtime_seconds * 0.35);
    assert!(perfect.mean_damage > normal.mean_damage * 1.25);
    assert!(perfect.median_run.procs > 0, "perfect reload forces Heat status even at 0% base status chance");
}

#[test]
fn incarnon_gauge_ignores_overkill_pellets_on_same_pull_replacement_body() {
    let base = WeaponBase::from_data("boar_prime", true, &[]);
    let inc = WeaponBase::from_data("boar_prime_incarnon", true, &[]);
    let bp = resolve(&base, &[], StackPolicy::Emergent);
    let ip = resolve(&inc, &[], StackPolicy::Emergent);
    let mut arena = Arena::training(1.0);
    arena.body_parts = vec![BodyPart {
        name: "head".into(),
        aim_weight: 1.0,
        multiplier: 3.0,
        is_head: true,
        is_weak_point: true,
        crit_bonus: true,
    }];
    let mut p = FightParams::incarnon_cycle_from_panels(&ip, &bp, false, LockMode::Initial(0), &arena, &ArcaneFx::none());
    // Frail 1 HP target that respawns immediately: 1st pellet kills generation 0, remaining 7 pellets hit generation 1 on the SAME pull.
    p.foe = frail_target(TargetMode::InstantRespawn, 1.0, 0.0);
    p.fire_rate = 0.5;
    let s = monte_carlo(&p, 1, 3);
    // 1 shot in 1.0 s: only 1 pellet on generation 0 may charge the gauge, never filling Boar Prime's multi-hit gauge in one overkill pull.
    assert_eq!(s.median_run.transforms, 0);
}

#[test]
fn buzlok_beacon_mode_adds_flat_crit_and_scoped_combo_resets_completely() {
    let modes = crate::data::weapons::play_modes("buzlok");
    let guided = modes.iter().find(|m| m.id == "beacon_guided").expect("beacon_guided mode");
    let base = WeaponBase::from_data("buzlok", false, &[]);
    let beacon = WeaponBase::from_data("buzlok_beacon", false, &[]);
    let bp = resolve(&base, &[], StackPolicy::Emergent);
    let ap = resolve(&beacon, &[], StackPolicy::Emergent);
    let arena = Arena::training(15.0);
    let mut p_base = FightParams::from_panel(&bp, &arena, &ArcaneFx::none());
    let mut p_alt = FightParams::from_panel(&ap, &arena, &ArcaneFx::none());
    p_base.apl_inserted = guided.rules();
    p_alt.apl_inserted = guided.rules();
    p_base.foe = frail_target(TargetMode::InfiniteHealth, 0.0, 0.0);
    p_alt.foe = frail_target(TargetMode::InfiniteHealth, 0.0, 0.0);
    let p_guided = p_base.with_toggle(p_alt, 0.0, 0.0);

    let mut p_plain = FightParams::from_panel(&bp, &arena, &ArcaneFx::none());
    p_plain.foe = frail_target(TargetMode::InfiniteHealth, 0.0, 0.0);

    let g = monte_carlo(&p_guided, 4, 9);
    let u = monte_carlo(&p_plain, 4, 9);
    assert!(g.mean_damage > u.mean_damage * 1.35, "beacon +50% flat crit boosts DPS: {} vs {}", g.mean_damage, u.mean_damage);
    assert!(bp.bespoke.as_ref().unwrap().scoped_combo.unwrap().resets_completely);
}

#[test]
fn cedo_innate_gunco_and_nonrepeating_primary_glaive_procs_work() {
    let base_p = build_fight("cedo", &[], 5.0);
    assert!((base_p.co_per_type - 0.60000002).abs() < 1e-6);

    let mut alt = build_fight("cedo_alt", &[], 4.0);
    alt.status_chance = 0.0;
    alt.base_status_chance = 0.0;
    if let Some(r) = alt.radial.as_mut() {
        r.status_chance = 0.0;
    }
    let s = monte_carlo(&alt, 1, 13);
    assert!(s.median_run.procs >= 3, "glaive radial forces primary elemental procs even at 0% status chance");
}

#[test]
fn zenistar_disc_duration_and_melee_counter_overrides_apply() {
    let zh = build_fight("zenistar_heavy", &[], 30.0);
    let disc = zh.melee_profiles.get("disc").expect("zenistar disc profile");
    let ms = disc.lingering.as_ref().unwrap().melee_scaling.unwrap();
    assert!(!ms.duration_per_combo);
    assert_eq!(ms.duration_per_combo_seconds, 10.0);
    assert!((zh.bespoke.as_ref().unwrap().deployed_fire_rate_bonus - 0.20).abs() < 1e-6);

    let fragor = build_fight("fragor_prime_heavy", &[], 10.0);
    assert_eq!(fragor.initial_combo, 30.0);

    let venka = build_fight("venka_prime", &[], 10.0);
    assert_eq!(super::super::melee::melee_combo_multiplier_for(&venka, 240.0), 13.0);

    let dex = build_fight("dex_nikana", &[], 10.0);
    assert_eq!(super::super::melee::melee_combo_multiplier_for(&dex, 110.0), 11.0);
    assert_eq!(super::super::melee::melee_combo_multiplier_for(&dex, 220.0), 11.0);
}

#[test]
fn warframe_signature_bonuses_activate_only_for_matching_wielder() {
    let base = WeaponBase::from_data("knell_prime", false, &[]);
    let default_tenno = crate::data::tenno::default_tenno();
    let mut harrow = default_tenno.clone();
    harrow.id = "harrow_prime".to_string();

    let p_default = resolve_for(&base, &[], StackPolicy::Emergent, default_tenno);
    let p_harrow = resolve_for(&base, &[], StackPolicy::Emergent, &harrow);
    assert_eq!(p_default.magazine_size, 1.0);
    assert_eq!(p_harrow.magazine_size, 2.0);

    let steflos = WeaponBase::from_data("steflos_prime", false, &[]);
    let mut citrine = default_tenno.clone();
    citrine.id = "citrine_prime".to_string();
    let s_def = resolve_for(&steflos, &[], StackPolicy::Emergent, default_tenno);
    let s_cit = resolve_for(&steflos, &[], StackPolicy::Emergent, &citrine);
    assert!(s_cit.projectile_speed_mps.unwrap() > s_def.projectile_speed_mps.unwrap() * 1.39);
}

#[test]
fn bespoke_melee_passives_apply_damage_and_status_bonuses() {
    // Galariak Prime: +10% flat crit chance per unique status type on target.
    let mut gal = build_fight("galariak_prime", &[], 8.0);
    gal.base_crit_chance = 0.0;
    gal.crit_multiplier = 3.0;
    if let Some(ci) = gal.critical_inputs.as_mut() {
        ci.set_supplied_chance(0.0, 0.0);
        ci.set_supplied_damage(3.0, 3.0, None);
    }
    gal.status_chance = 1.0;
    let with_passive = monte_carlo(&gal, 4, 17);
    gal.bespoke.as_mut().unwrap().flat_crit_chance_per_status_type = 0.0;
    let without_passive = monte_carlo(&gal, 4, 17);
    assert!(with_passive.mean_damage > without_passive.mean_damage * 1.15);

    // Slaytra: 2x Slash status duration.
    let slaytra = build_fight("slaytra", &[], 12.0);
    let mut plain = slaytra.clone();
    plain.bespoke.as_mut().unwrap().slash_duration_multiplier = 1.0;
    assert!(monte_carlo(&slaytra, 4, 23).mean_damage > monte_carlo(&plain, 4, 23).mean_damage);
    let _ = DamageType::Slash;
}
