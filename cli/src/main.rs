//! wfsim command-line entry point.
//!
//! For now it runs the basic "shoot the training dummy" Monte Carlo: Dual
//! Toxocyst base form + Secondary Enervate, 50% headshots, 1000 runs x 10 s.
//! See `engine::dummy` for the (deliberately basic) model and its assumptions.

use std::path::Path;
use wfsim_engine::dummy::{monte_carlo, DummyParams, TargetMode};
use wfsim_engine::enemy_data::EnemySpec;
use wfsim_engine::scaling;

fn main() {
    let params = DummyParams::default();
    let runs = 1000;
    let seed = 0xC0FFEE;

    println!("wfsim {} — dummy engagement", env!("CARGO_PKG_VERSION"));
    println!(
        "weapon: Dual Toxocyst (base) | {:.0} dmg, {:.0}% crit, {:.1}x crit, {:.1} fire/s",
        params.base_damage,
        params.base_crit_chance * 100.0,
        params.crit_multiplier,
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
        parts, runs, params.duration_secs,
    );
    println!("excluded: status/elements, armor, Frenzy | infinite ammo | assumptions unverified");
    println!();

    let s = monte_carlo(&params, runs, seed);

    println!("shots/run:        {:.0}", s.mean_shots);
    println!("crit rate:        {:.1}%", s.mean_crit_rate * 100.0);
    println!("big-crit rate:    {:.1}%", s.mean_big_crit_rate * 100.0);
    println!("headshot rate:    {:.1}%", s.mean_headshot_rate * 100.0);
    println!();
    println!("damage / {:.0}s run:", s.duration_secs);
    println!("  mean:  {:>10.1}", s.mean_damage);
    println!("  std:   {:>10.1}", s.std_damage);
    println!("  min:   {:>10.1}", s.min_damage);
    println!("  max:   {:>10.1}", s.max_damage);
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

    // Same loadout vs a level-cap Steel Path Thrax, instant respawn, no
    // on-death transformation (spectral form skipped by decision).
    let thrax_params = DummyParams {
        target: thrax
            .target_params(9999, true, false, TargetMode::InstantRespawn)
            .expect("valid thrax target"),
        duration_secs: 60.0,
        ..DummyParams::default()
    };
    let ts = monte_carlo(&thrax_params, 200, seed);
    println!();
    println!("vs Thrax @9999 SP (instant respawn, 200 runs x 60 s):");
    println!("  raw DPS:        {:.1}", ts.dps);
    println!("  effective DPS:  {:.1}", ts.effective_dps);
    println!("  kills/run:      {:.3}", ts.mean_kills);
}
