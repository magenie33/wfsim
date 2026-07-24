//! wfsim-optimizer: the full search flow against the ULTIMATE STRESS TEST.
//!
//! Scenario (user, 2026-07-24): Dual Toxocyst Incarnon Form (fixed evolution
//! build; no arcanes), 8 mods from the full pistol pool, vs Thrax Centurion
//! @9999 STEEL PATH, instant respawn, 100% headshots, 60 s engagements.
//! Objective: mean KILLS over 1000 runs (screening rounds rank by mean
//! effective damage — continuous, low-variance — finals rank by kills).
//!
//! Usage: wfsim-optimizer [require=mod_id]... [forbid=mod_id]...

use std::time::Instant;
use wfsim_engine::dummy::{LockMode, TargetMode};
use wfsim_engine::enemy_data::EnemySpec;
use wfsim_engine::loadout::WeaponBase;
use wfsim_optimizer::*;

fn main() {
    let mut constraints = Constraints::default();
    let mut flat = false;
    for arg in std::env::args().skip(1) {
        if let Some(id) = arg.strip_prefix("require=") {
            constraints.require.push(id.to_string());
        } else if let Some(id) = arg.strip_prefix("forbid=") {
            constraints.forbid.push(id.to_string());
        } else if arg == "flat" {
            // Validation mode: no funnel — EVERY candidate gets the full
            // 1000 x 60 s treatment (much slower; verifies the funnel).
            flat = true;
        } else {
            eprintln!("unknown arg: {arg} (use require=<id> / forbid=<id>)");
            std::process::exit(2);
        }
    }

    let spec = EnemySpec::load(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/enemies/thrax_centurion.yaml"
    )))
    .expect("thrax spec");
    // Dominance pruning: same-effect lower tiers can never win (user
    // directive 2026-07-24). Printed, never silent.
    for (id, why) in dominated_mods() {
        if !constraints.forbid.iter().any(|f| f == id) {
            println!("[prune] {id}: {why}");
            constraints.forbid.push(id.to_string());
        }
    }

    let scenario = Scenario {
        target: spec
            .target_params(9999, true, false, TargetMode::InstantRespawn)
            .expect("valid target"),
        body_parts: spec.aim_parts(&[("head", 1.0)]).expect("head aim"),
        duration_secs: 60.0,
        // The chosen loadout's arcane is EQUIPPED (fixed, not searched).
        arcane_enervate: true,
        // The REAL Incarnon cycle (user flow): full gauge start -> dump ->
        // revert 1.0 s -> rebuild 9 weakpoint charges in base form ->
        // transmute 2.35 s -> repeat. Frenzy locked Permanent (chosen
        // sim setting) - its +100% Toxin injection is folded into the
        // base-form panel.
        incarnon_cycle: true,
        frenzy_lock: LockMode::Permanent,
    };
    println!(
        "[scenario] {} @9999 STEEL PATH, instant respawn, 100% headshots, 60 s, REAL incarnon cycle",
        scenario.target.name
    );
    println!(
        "  pools: overguard {:.3e}, health {:.3e}, armor {:.0}",
        scenario.target.overguard(),
        scenario.target.max_health(),
        scenario.target.armor()
    );

    let p = pool();
    let base = WeaponBase::dual_toxocyst_incarnon();
    // The base form resolved with Frenzy active (its +100% Toxin joins the
    // hierarchy at the end).
    let base_form = WeaponBase::dual_toxocyst_base(true);
    let t0 = Instant::now();
    let (cands, stats) = enumerate_candidates(
        &p,
        &base,
        Some(&base_form),
        8,
        60,
        &dual_toxocyst_innate_slots(),
        &constraints,
    );
    println!(
        "[enumerate] {} subsets ({} illegal) -> {} order variants, {} deduped -> {} candidates in {:.1?}",
        stats.subsets,
        stats.illegal,
        stats.order_variants,
        stats.deduped,
        cands.len(),
        t0.elapsed()
    );

    // Successive halving: (runs, keep). Early rounds rank by mean effective
    // damage; the last two rank by mean kills (the objective).
    let rounds: Vec<(u32, usize, bool)> = if flat {
        vec![(1000, 24, true)]
    } else {
        vec![
            (3, 16384, false),
            (12, 3072, false),
            (48, 512, true),
            (200, 64, true),
            (1000, 24, true),
        ]
    };
    let mut alive: Vec<usize> = (0..cands.len()).collect();
    let mut last: Vec<(usize, wfsim_engine::dummy::Summary)> = Vec::new();
    for (round, &(runs, keep, by_kills)) in rounds.iter().enumerate() {
        let t = Instant::now();
        let summaries = evaluate_batch(&cands, &alive, &scenario, runs, 0xDEAD_BEEF + round as u64);
        let mut scored: Vec<(usize, wfsim_engine::dummy::Summary)> =
            alive.iter().copied().zip(summaries).collect();
        // Kill rounds rank by kill PROGRESS: kills + the depleted fraction
        // of the final target's pool (partial credit, no step function).
        scored.sort_by(|a, b| {
            let ka = if by_kills {
                a.1.mean_kill_progress
            } else {
                a.1.mean_effective_damage
            };
            let kb = if by_kills {
                b.1.mean_kill_progress
            } else {
                b.1.mean_effective_damage
            };
            kb.total_cmp(&ka)
        });
        scored.truncate(keep);
        println!(
            "[round {}] {} candidates x {} runs ({}) -> keep {} in {:.1?}; best {}",
            round + 1,
            alive.len(),
            runs,
            if by_kills { "kills" } else { "eff dmg" },
            scored.len(),
            t.elapsed(),
            if by_kills {
                format!("{:.2} kill score", scored[0].1.mean_kill_progress)
            } else {
                format!("{:.3e} eff", scored[0].1.mean_effective_damage)
            }
        );
        alive = scored.iter().map(|(i, _)| *i).collect();
        last = scored;
    }

    println!();
    println!("=== FINAL LEADERBOARD (1000 x 60 s, mean kill score = kills + partial) ===");
    for (rank, (ci, s)) in last.iter().take(10).enumerate() {
        let c = &cands[*ci];
        let names: Vec<&str> = c.ordered.iter().map(|&i| p[i].id).collect();
        let vec_desc: Vec<String> = c
            .panel
            .damage
            .iter_nonzero()
            .map(|(t, v)| format!("{t:?} {v:.0}"))
            .collect();
        println!(
            "#{:<2} score {:.3} (kills {:.3} ± {:.3}, min {} max {}) | eff DPS {:.3e} | {:.1} transforms",
            rank + 1,
            s.mean_kill_progress,
            s.mean_kills,
            s.std_kills,
            s.min_kills,
            s.max_kills,
            s.effective_dps,
            s.mean_transforms
        );
        println!("    mods: {}", names.join(", "));
        println!(
            "    panel: {} | cc {:.1}% cd {:.2}x sc {:.1}% fr {:.2} ms {:.2} | CO/type +{:.0}% | {} forma, {}/60",
            vec_desc.join(" / "),
            c.panel.crit_chance * 100.0,
            c.panel.crit_damage,
            c.panel.status_chance * 100.0,
            c.panel.fire_rate,
            c.panel.multishot,
            c.panel.co_per_type * 100.0,
            c.plan.forma_used,
            c.plan.total_drain
        );
    }
    println!("[total] {:.1?}", t0.elapsed());
}
