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
    let mut target_file = "thrax_centurion";
    let mut evo2_both = false;
    // Official default: 120 s (user, 2026-07-25) — long windows measure
    // steady-state sustain instead of window-phase burst artifacts.
    let mut duration_secs = 120.0f64;
    let mut final_runs: Option<u32> = None;
    let mut final_round_runs: Option<u32> = None;
    let mut arcane_only: Option<wfsim_engine::dummy::Arcane> = None;
    for arg in std::env::args().skip(1) {
        if let Some(id) = arg.strip_prefix("require=") {
            constraints.require.push(id.to_string());
        } else if let Some(id) = arg.strip_prefix("forbid=") {
            constraints.forbid.push(id.to_string());
        } else if arg == "flat" {
            // Validation mode: no funnel — EVERY candidate gets the full
            // 1024 x 60 s treatment (much slower; verifies the funnel).
            flat = true;
        } else if arg == "target=acolyte" {
            target_file = "acolyte";
        } else if arg == "target=thrax" {
            target_file = "thrax_centurion";
        } else if arg == "evo2=both" {
            evo2_both = true;
        } else if let Some(v) = arg.strip_prefix("duration=") {
            duration_secs = v.parse().expect("duration=<secs>");
        } else if let Some(v) = arg.strip_prefix("runs=") {
            // Playoff mode: skip the funnel, run every job at this count.
            final_runs = Some(v.parse().expect("runs=<n>"));
        } else if let Some(v) = arg.strip_prefix("final=") {
            // Keep the funnel; override only the LAST round's run count.
            final_round_runs = Some(v.parse().expect("final=<n>"));
        } else if let Some(v) = arg.strip_prefix("arcane=") {
            use wfsim_engine::dummy::Arcane;
            arcane_only = Some(match v {
                "deadhead" => Arcane::Deadhead,
                "enervate" => Arcane::Enervate,
                "flare" => Arcane::CascadiaFlare,
                "none" => Arcane::None,
                _ => panic!("arcane=deadhead|enervate|flare|none"),
            });
        } else {
            eprintln!(
                "unknown arg: {arg} (use require=<id> / forbid=<id> / flat / target=thrax|acolyte / evo2=both)"
            );
            std::process::exit(2);
        }
    }

    let spec = EnemySpec::load(
        &std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../data/enemies"))
            .join(format!("{target_file}.yaml")),
    )
    .expect("enemy spec");
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
        duration_secs,
        // The REAL Incarnon cycle (user flow): full gauge start -> dump ->
        // revert 1.0 s -> rebuild 9 weakpoint charges in base form ->
        // transmute 2.35 s -> repeat. Frenzy locked Permanent (chosen
        // sim setting) - its +100% Toxin injection is folded into the
        // base-form panel.
        incarnon_cycle: true,
        frenzy_lock: LockMode::Permanent,
    };
    println!(
        "[scenario] {} @9999 STEEL PATH, instant respawn, 100% headshots, {} s, REAL incarnon cycle",
        scenario.target.name, scenario.duration_secs
    );
    println!(
        "  pools: overguard {:.3e}, shield {:.3e}, health {:.3e}, armor {:.0}{}",
        scenario.target.overguard(),
        scenario.target.max_shield(),
        scenario.target.max_health(),
        scenario.target.armor(),
        match scenario.target.attenuation {
            Some(a) => format!(
                " | attenuation: instance {:.0}% / dps {:.0}% of max HP (ESTIMATES)",
                a.instance_frac * 100.0,
                a.dps_frac * 100.0
            ),
            None => String::new(),
        }
    );

    let p = pool();
    let t0 = Instant::now();
    // The Evolution II choice is a SEARCH DIMENSION (user, 2026-07-25):
    // the whole canonical space is enumerated against BOTH weapon
    // configs (both forms resolved with Frenzy's Toxin injection).
    use wfsim_engine::loadout::DtEvo2;
    let mut cands = Vec::new();
    // Fevered Frenzy is the LOCKED official Evolution II (user,
    // 2026-07-25: it swept all three benchmark scenarios); `evo2=both`
    // re-opens the comparison.
    let evo2s: &[(DtEvo2, &'static str)] = if evo2_both {
        &[
            (DtEvo2::FeveredFrenzy, "fevered"),
            (DtEvo2::CarnageReign, "carnage"),
        ]
    } else {
        &[(DtEvo2::FeveredFrenzy, "fevered")]
    };
    for &(evo2, label) in evo2s {
        let base = WeaponBase::dual_toxocyst_incarnon(true, evo2);
        let base_form = WeaponBase::dual_toxocyst_base(true, evo2);
        let (mut c, stats) = enumerate_candidates(
            &p,
            &base,
            Some(&base_form),
            label,
            8,
            60,
            &dual_toxocyst_innate_slots(),
            &constraints,
        );
        println!(
            "[enumerate {label}] {} subsets ({} illegal) -> {} order variants, {} deduped -> {} candidates",
            stats.subsets,
            stats.illegal,
            stats.order_variants,
            stats.deduped,
            c.len(),
        );
        cands.append(&mut c);
    }
    println!(
        "[enumerate] {} total candidates in {:.1?}",
        cands.len(),
        t0.elapsed()
    );

    use wfsim_engine::dummy::Arcane;
    // The arcane is a SEARCH DIMENSION like the mod choice (user,
    // 2026-07-25): every candidate is evaluated under each arcane.
    let arcanes: Vec<Arcane> = match arcane_only {
        Some(a) => vec![a],
        None => vec![Arcane::Enervate, Arcane::Deadhead, Arcane::CascadiaFlare],
    };
    let mut alive: Vec<Job> = (0..cands.len())
        .flat_map(|i| arcanes.iter().map(move |&a| (i, a)))
        .collect();
    println!(
        "[search] {} jobs = {} candidates x {} arcanes",
        alive.len(),
        cands.len(),
        arcanes.len()
    );
    // Self-scaling successive halving derived from the job count (user,
    // 2026-07-25); `flat` bypasses the funnel for validation runs.
    let mut rounds: Vec<(u32, usize, bool)> = if let Some(r) = final_runs {
        vec![(r, 24, true)]
    } else if flat {
        vec![(1024, 24, true)]
    } else {
        schedule(alive.len())
    };
    if let Some(r) = final_round_runs {
        if let Some(last) = rounds.last_mut() {
            last.0 = r;
        }
    }
    let mut last: Vec<(Job, wfsim_engine::dummy::Summary)> = Vec::new();
    for (round, &(runs, keep, by_kills)) in rounds.iter().enumerate() {
        let t = Instant::now();
        let summaries = evaluate_batch(&cands, &alive, &scenario, runs, 0xDEAD_BEEF + round as u64);
        let mut scored: Vec<(Job, wfsim_engine::dummy::Summary)> =
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
            "[round {}] {} jobs x {} runs ({}) -> keep {} in {:.1?}; best {}",
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
        alive = scored.iter().map(|(j, _)| *j).collect();
        last = scored;
    }

    println!();
    println!("=== FINAL LEADERBOARD (1024 x 60 s, kill score; searched: mods x element order x arcane x evo2) ===");
    for (rank, ((ci, arcane), s)) in last.iter().take(10).enumerate() {
        let c = &cands[*ci];
        let names: Vec<&str> = c.ordered.iter().map(|&i| p[i].id).collect();
        let vec_desc: Vec<String> = c
            .panel
            .damage
            .iter_nonzero()
            .map(|(t, v)| format!("{t:?} {v:.0}"))
            .collect();
        println!(
            "#{:<2} score {:.3} (kills {:.3} ± {:.3}, min {} max {}) | {:?} + {} | eff DPS {:.3e} | {:.1} transforms",
            rank + 1,
            s.mean_kill_progress,
            s.mean_kills,
            s.std_kills,
            s.min_kills,
            s.max_kills,
            arcane,
            c.variant,
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
    println!("[total] {:.1?}", t0.elapsed());
}
