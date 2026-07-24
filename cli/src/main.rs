//! wfsim command-line entry point.
//!
//! For now it runs the basic "shoot the training dummy" Monte Carlo: Dual
//! Toxocyst base form + Secondary Enervate, 50% headshots, 1000 runs x 10 s.
//! See `engine::dummy` for the (deliberately basic) model and its assumptions.

use wfsim_engine::dummy::{monte_carlo, DummyParams};

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
}
