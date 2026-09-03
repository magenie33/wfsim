//! wfsim command-line entry point.
//!
//! For now it runs the basic "shoot the training dummy" Monte Carlo: Dual
//! Toxocyst base form + Secondary Enervate, 50% headshots, 1000 runs x 10 s.
//! See `engine::dummy` for the (deliberately basic) model and its assumptions.

use std::path::Path;
use wfsim_engine::arcanes_data::ArcaneFx;
use wfsim_engine::damage::{DamageType, DamageVector};
use wfsim_engine::dummy::{
    monte_carlo, BuffLock, DummyParams, LockedBuff, TargetMode, TargetParams,
};
use wfsim_engine::enemy_data::EnemySpec;
use wfsim_engine::loadout::CoBehavior;
use wfsim_engine::scaling;

// ---- Demo-build fixtures (harness-local) --------------------------------
// The ENGINE knows no specific weapon; this demo harness does. Values are
// the historical calibration profile: Dual Toxocyst default build
// (Commodore's Fortune + Evolved Autoloader + Fevered Frenzy) + Secondary
// Enervate, humanoid dummy, 10 s.

fn dual_toxocyst_baseline() -> DummyParams {
    DummyParams {
        acid_shells: None,
        // NOT A MELEE WEAPON: no combo counter, no script, no follow through.
        combo_script: Vec::new(),
        follow_through: None,
        slam: None,
        heavy: None,
        tennokai: wfsim_engine::loadout::Tennokai::default(),
        spends_combo: false,
        combo_duration_seconds: 0.0,
        combo_frozen: false,
        initial_combo: 0.0,
        heavy_attack_efficiency: 0.0,
        crit_chance_per_combo: 0.0,
        combo_count_chance_on_lifted: 0.0,
        combo_count_on_slam_hit: 0.0,
        status_chance_on_lifted: 0.0,
        status_chance_per_combo: 0.0,
        combo_count_chance: 0.0,
        heavy_attack_damage: 0.0,
        slam_damage: 0.0,
        // Ordinary — only the Laetum's Incarnon form differs, and this is not it.
        echo_multiplier: 1.0,
        // NO WARFRAME behind this fixture: no auras, no shards.
        squad: wfsim_engine::tenno_data::SquadEffects::default(),
        abilities: Vec::new(),
        target_id: "e1".to_string(),
        punch_through_m: 0.0,
        projectile_width_m: 0.0,
        // AIMED: the calibration profile is a pistol pointed at a dummy.
        unaimed_headshot_chance: None,
        // …AND IT DEPLOYS NOTHING: this fixture throws no orb.
        orb: None,
        orb_strike: None,
        orb_blast: None,
        // A ROUND LEAVES ON THE TRIGGER, like every gun but one.
        windup_seconds: 0.0,
        no_magazine: false,
        strip_on_kill_in_range: None,
        // NO METER: this fixture has nothing gated behind a clock, and one
        // player is behind it.
        meter: None,
        squad_size: 1,
        // No weapon: nothing to be out of range of.
        range_m: f64::INFINITY,
        super_crit_on_status: None,
        weakpoint_stacks: None,
        spawn_on_kill: None,
        enervate_stacks: 0,
        influence_open: None,
        tenno: wfsim_engine::tenno_data::default_tenno().clone(),
        // The historical calibration profile is a point-blank fight, and this
        // weapon lists no falloff to notice a range with anyway.
        falloff: None,
        // …and nothing misses: the calibration profile is a fixed reference.
        spread: None,
        player_at: wfsim_engine::space::Vec2::ORIGIN,
        target_at: wfsim_engine::space::Vec2::new(0.0, wfsim_engine::space::CONTACT_RANGE_M),
        // Not a charge weapon: inert without charge_seconds.
        charge_cadence: wfsim_engine::weapons_data::ChargeCadence::DrawThenRate,
        sustained_fire_rate: None,
        battery: None,
        rs_on_reload: 0.0,
        armor_strip_per_puncture: 0.0,
        instant_reload: None,
        headshot_streak: None,
        crit_damage_below_status_count: None,
        burst: None,
        beam_ramp_floor: 0.20,
        syndicate_radial: None,
        pellet_damage: Vec::new(),
        multishot_adds_damage: false,
        sniper_combo: None,
        combo_initial: 0,
        combo_held: false,
        tendril_max: 0,
        tendril_range_m: 0.0,
        tendril_acquire_deg: 0.0,
        crit_chance_per_tendril: 0.0,
        sc_per_tendril: 0.0,
        tendrils_initial: 0,
        tendrils_held: false,
        magazine_refill_on_kill: 0.0,
        radial: None,
        lingering: None,
        continuous: false,
        field_duration_on_empty_reload: 1.0,
        multishot_on_last_round: 0.0,
        base_multishot_on_last_round: 0.0,
        multishot_ammo_bonus: 0.0,
        headshot_damage_bonus: 0.0,
        noncrit_bonus: None,
        stacking_buffs: Vec::new(),
        damage: DamageVector::new()
            .with(DamageType::Impact, 7.5)
            .with(DamageType::Puncture, 60.0)
            .with(DamageType::Slash, 7.5),
        base_crit_chance: 0.05,
        crit_tier_upgrade_chance: 0.0,
        slash_on_crit: 0.0,
        crit_multiplier: 2.0,
        crit_multiplier_below_crit_chance: None,
        unmodded_crit_chance: 0.05,
        unmodded_crit_damage: 2.0,
        status_chance: 0.37,
        base_status_chance: 0.37,
        forced_procs: Vec::new(),
        attractor_seconds: None,
        status_duration_multiplier: 1.0,
        fire_rate: 1.0,
        charge_seconds: None, // not a charge weapon (that is a bow's cadence)
        frenzy: false,
        locked_stats: Vec::new(),
        locked_buffs: Vec::new(),
        cycle: None,
        magazine_size: 12.0,
        reload_seconds: 2.35,
        infinite_reserve: true,
        ammo_cost: 1.0,
        headshot_bonus_multiplicative: false,
        applies_microwave: false,
        independent_procs: &[],
        last_round_damage: 0.0,
        first_round_damage: 0.0,
        base_damage_on_reload: None,
        base_damage_on_eximus_weakpoint: None,
        crit_chance_per_hit: None,
        crit_chance_per_hit_initial_stacks: 0,
        crit_chance_per_hit_held: false,
        reserve_ammo: 72.0,
        compression_multiplier: 1.0,
        compression_base_damage: 0.0,
        base_damage_below_half_health: 0.0,
        crit_chance_on_undamaged: 0.0,
        crit_damage_on_undamaged: 0.0,
        ammo_efficiency_applies: true,
        multishot: 1.0,
        base_multishot: 1.0,
        evo_multishot: None,
        evo_base_damage: None,
        base_damage_bonus: 0.0,
        co_per_type: 0.0,
        co_behavior: CoBehavior::AdditiveWithBaseDamage,
        co_base_fraction: 1.0,
        co_stack: None,
        multishot_stack: None,
        crit_chance_on_headshot: None,
        crit_chance_stack: None,
        status_damage_multiplier: 1.0,
        elem_dot_bonus: Vec::new(),
        faction_multiplier: 1.0,
        dot_modified_base: None,
        reload_bonus: 0.0,
        weakpoint_damage: 0.0,
        headshot_multiplier: None,
        weakpoint_crit_chance_relative: 0.0,
        bodyshot_crit_chance_multiplier: 1.0,
        derived_status_from_crit: None,
        derived_crit_from_status: None,
        consecutive_hit_damage: None,
        round_restore_on_status: None,
        instant_reload_on_kill: None,
        magazine_growth_on_empty_reload: None,
        crit_damage_on_kill: None,
        fire_rate_on_reload: None,
        proc_conversion: None,
        arcane: ArcaneFx {
            id: "secondary_enervate".to_string(),
            enervate_rank: Some(5),
            ..ArcaneFx::none()
        },
        body_parts: DummyParams::humanoid_parts(),
        target: TargetParams::training_dummy(),
        duration_seconds: 10.0,
        // ONE BODY — a fixture, not a formation.
        others: Vec::new(),
        // …and the weapon points AT it.
        aim_at: None,
        beam: None,
        ricochet: None,
    }
}

/// Base form as played: Fevered Frenzy's +50 base scales the vector
/// pro-rata (75 → 125), Commodore's Fortune sets base crit to 25%; Frenzy
/// locked, Fevered pre-stacked to 20 (+100% multishot).
fn dual_toxocyst_base_params() -> DummyParams {
    DummyParams {
        damage: DamageVector::new()
            .with(DamageType::Impact, 7.5)
            .with(DamageType::Puncture, 60.0)
            .with(DamageType::Slash, 7.5)
            .scale(125.0 / 75.0),
        base_crit_chance: 0.25,
        frenzy: true,
        locked_buffs: vec![BuffLock::permanent(LockedBuff::Frenzy)],
        multishot: 2.0,
        ..dual_toxocyst_baseline()
    }
}

/// Incarnon Form (pseudo-reload model, gauge locked full).
fn dual_toxocyst_incarnon_params() -> DummyParams {
    DummyParams {
        damage: DamageVector::new()
            .with(DamageType::Impact, 25.0)
            .with(DamageType::Puncture, 62.5)
            .with(DamageType::Slash, 37.5),
        base_crit_chance: 0.31,
        crit_multiplier: 3.0,
        status_chance: 0.43,
        fire_rate: 4.5,
        frenzy: true,
        magazine_size: 270.0,
        reload_seconds: 3.35,
        ammo_efficiency_applies: false,
        multishot: 2.0,
        ..dual_toxocyst_baseline()
    }
}

fn main() {
    // transform_modes: the two Dual Toxocyst forms are separate weapons.
    let params = dual_toxocyst_base_params(); // Frenzy passive live
    let runs = 1000;
    let seed = 0xC0FFEE;

    println!("wfsim {} — dummy engagement", env!("CARGO_PKG_VERSION"));
    println!(
        "weapon: Dual Toxocyst (base, built) | {:.0} dmg (12.5I/100P/12.5S), {:.0}% crit, {:.1}x crit, {:.0}% status, {:.1} fire/s",
        params.damage.total(),
        params.base_crit_chance * 100.0,
        params.crit_multiplier,
        params.status_chance * 100.0,
        params.fire_rate,
    );
    let total_weight: f64 = params.body_parts.iter().map(|p| p.aim_weight).sum();
    let parts = params
        .body_parts
        .iter()
        .map(|p| {
            format!(
                "{} {:.0}% (x{:.1}{})",
                p.name,
                p.aim_weight / total_weight * 100.0,
                p.multiplier,
                if p.is_head { ", head" } else { "" },
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    println!(
        "perk: Secondary Enervate (max) | aim: {} | {} runs x {:.0} s",
        parts, runs, params.duration_seconds,
    );
    println!(
        "status sim v1: Stagger/Weakened/Bleed | Frenzy live (fire rate) | no elements yet | unverified"
    );
    println!();

    let s = monte_carlo(&params, runs, seed);

    println!("shots/run:        {:.0}", s.mean_shots);
    println!("crit rate:        {:.1}%", s.mean_crit_rate * 100.0);
    println!("big-crit rate:    {:.1}%", s.mean_big_crit_rate * 100.0);
    println!("headshot rate:    {:.1}%", s.mean_headshot_rate * 100.0);
    println!();
    println!("damage / {:.0}s run:", s.duration_seconds);
    println!("  mean:  {:>10.1}", s.mean_damage);
    println!("  std:   {:>10.1}", s.std_damage);
    println!("  min:   {:>10.1}", s.min_damage);
    println!("  max:   {:>10.1}", s.max_damage);
    println!(
        "  procs: {:>5.1}/run | DoT: {:>7.1} ({:.0}% of effective)",
        s.mean_procs,
        s.mean_dot_damage,
        s.mean_dot_damage / s.mean_effective_damage * 100.0,
    );
    println!();
    println!("sustained DPS:    {:.1}", s.dps);

    // Enemy library, loaded from data/enemies/ (single source of truth).
    let enemies_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../data/enemies");
    let thrax = EnemySpec::load(&enemies_dir.join("thrax_centurion.yaml"))
        .expect("load data/enemies/thrax_centurion.yaml");

    // Thrax Centurion stat table at a few interesting levels.
    println!();
    println!(
        "{} (base @L{}: {:.0} HP / {:.0} armor / {:.0} overguard):",
        thrax.name,
        thrax.stats.base_level,
        thrax.stats.health,
        thrax.stats.armor,
        thrax.stats.overguard,
    );
    println!(
        "  {:>7} {:>3}  {:>13} {:>8} {:>5} {:>13}",
        "level", "SP", "health", "armor", "DR", "overguard"
    );
    for (level, sp) in [(55u32, false), (155, true), (9999, false), (9999, true)] {
        let t = thrax
            .target_params(level, sp, false, TargetMode::InstantRespawn)
            .expect("valid thrax target");
        let armor = t.armor();
        println!(
            "  {:>7} {:>3}  {:>13.0} {:>8.0} {:>4.0}% {:>13.0}",
            level,
            if sp { "yes" } else { "no" },
            t.max_health(),
            armor,
            scaling::armor_damage_reduction(armor) * 100.0,
            t.overguard(),
        );
    }

    // Rigor check: combinations that don't exist in-game are rejected.
    if let Err(e) = thrax.target_params(100, false, true, TargetMode::InstantRespawn) {
        println!("  eximus toggle: REJECTED ({e})");
    }

    // ------------------------------------------------------------------
    // FOCUS: Incarnon Form damage. Two INDEPENDENT tests - separate runs,
    // separate targets, zero interaction (single-target sim; no shared AoE).
    let inc = dual_toxocyst_incarnon_params();

    // Test 1: custom dummy - infinite health, zero armor/overguard, no
    // resistances. Pure throughput measurement.
    let s1 = monte_carlo(&inc, runs, seed);
    println!();
    println!("[Incarnon test 1] vs plain dummy (infinite HP, no resistances, 10 s):");
    println!(
        "  pulls {:.0} | pellets {:.0} | crit {:.1}% | big-crit {:.1}% | head {:.1}%",
        s1.mean_shots,
        s1.mean_pellets,
        s1.mean_crit_rate * 100.0,
        s1.mean_big_crit_rate * 100.0,
        s1.mean_headshot_rate * 100.0,
    );
    println!(
        "  procs {:.1}/run ({:.2}/pellet) | DoT {:.0} ({:.0}% of effective)",
        s1.mean_procs,
        s1.mean_procs / s1.mean_pellets,
        s1.mean_dot_damage,
        s1.mean_dot_damage / s1.mean_effective_damage * 100.0,
    );
    println!(
        "  raw = effective DPS: {:.0} (no mitigation on this target)",
        s1.dps
    );

    // Test 2 - THE ULTIMATE STRESS TEST (standard benchmark for every
    // weapon,): Thrax Centurion @9999 STEEL PATH, instant
    // respawn, Secondary Enervate equipped (always on in this sim).
    // 9.67M health behind 15.5M neutral Overguard.
    let inc2 = DummyParams {
        target: thrax
            .target_params(9999, true, false, TargetMode::InstantRespawn)
            .expect("valid thrax target"),
        duration_seconds: 60.0,
        ..dual_toxocyst_incarnon_params()
    };
    let s2 = monte_carlo(&inc2, 300, seed);
    println!();
    println!("[ULTIMATE STRESS TEST] vs Thrax @9999 STEEL PATH (instant respawn, 300 x 60 s):");
    println!(
        "  raw DPS {:.0} | effective DPS {:.0} | kills/run {:.3}",
        s2.dps, s2.effective_dps, s2.mean_kills
    );
    println!(
        "  procs {:.0}/run | DoT {:.0} - every instance lands on the 15.5M
   neutral Overguard pool (armor never engages), hence raw == effective",
        s2.mean_procs, s2.mean_dot_damage
    );
}
