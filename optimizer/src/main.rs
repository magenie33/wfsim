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
    let mut compare_arcanes = false;
    for arg in std::env::args().skip(1) {
        if let Some(id) = arg.strip_prefix("require=") {
            constraints.require.push(id.to_string());
        } else if let Some(id) = arg.strip_prefix("forbid=") {
            constraints.forbid.push(id.to_string());
        } else if arg == "flat" {
            // Validation mode: no funnel — EVERY candidate gets the full
            // 1000 x 60 s treatment (much slower; verifies the funnel).
            flat = true;
        } else if arg == "compare-arcanes" {
            compare_arcanes = true;
        } else {
            eprintln!(
                "unknown arg: {arg} (use require=<id> / forbid=<id> / flat / compare-arcanes)"
            );
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
        // Overwritten per arcane in the comparison loop below.
        arcane: wfsim_engine::dummy::Arcane::Enervate,
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
    let base = WeaponBase::dual_toxocyst_incarnon(true);
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
    use wfsim_engine::dummy::Arcane;
    // Official configuration (user, 2026-07-25): Secondary Deadhead is
    // the equipped arcane; `compare-arcanes` re-runs the full comparison.
    let arcanes: &[Arcane] = if compare_arcanes {
        &[Arcane::Enervate, Arcane::Deadhead, Arcane::CascadiaFlare]
    } else {
        &[Arcane::Deadhead]
    };
    let mut champions: Vec<(Arcane, String, f64)> = Vec::new();
    for &arcane in arcanes {
        let mut scenario = scenario.clone();
        scenario.arcane = arcane;
        println!();
        println!("################ ARCANE: {arcane:?} ################");
        let mut alive: Vec<usize> = (0..cands.len()).collect();
        let mut last: Vec<(usize, wfsim_engine::dummy::Summary)> = Vec::new();
        for (round, &(runs, keep, by_kills)) in rounds.iter().enumerate() {
            let t = Instant::now();
            let summaries =
                evaluate_batch(&cands, &alive, &scenario, runs, 0xDEAD_BEEF + round as u64);
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
            let co_desc = match (&c.panel.co_stack, c.panel.co_per_type) {
                (Some(s), _) => format!(
                    "CO {:.0}%/type x{} earned (eff {:.0}%)",
                    s.per_stack * 100.0,
                    s.max_stacks,
                    c.panel.co_base_fraction * 100.0
                ),
                (None, co) if co > 0.0 => format!("CO/type +{:.0}%", co * 100.0),
                _ => "no CO".into(),
            };
            let ms_desc = match &c.panel.ms_stack {
                Some(s) => format!(
                    "{:.2}+{:.1}x{} earned",
                    c.panel.multishot, s.per_stack, s.max_stacks
                ),
                None => format!("{:.2}", c.panel.multishot),
            };
            println!(
                "    panel: {} | cc {:.1}% cd {:.2}x sc {:.1}% fr {:.2} ms {} | {} | {} forma, {}/60",
                vec_desc.join(" / "),
                c.panel.crit_chance * 100.0,
                c.panel.crit_damage,
                c.panel.status_chance * 100.0,
                c.panel.fire_rate,
                ms_desc,
                co_desc,
                c.plan.forma_used,
                c.plan.total_drain
            );
        }

        let (ci, sbest) = &last[0];
        champions.push((
            arcane,
            cands[*ci]
                .ordered
                .iter()
                .map(|&i| p[i].id)
                .collect::<Vec<_>>()
                .join(", "),
            sbest.mean_kill_progress,
        ));
    }

    println!();
    println!("=== ARCANE COMPARISON (each arcane's best build, kill score) ===");
    for (a, mods, score) in &champions {
        println!("{a:?}: {score:.3} | {mods}");
    }
    println!("[total] {:.1?}", t0.elapsed());
}
