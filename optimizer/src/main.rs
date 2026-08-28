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
    let mut duration_seconds = 120.0f64;
    let mut final_runs: Option<u32> = None;
    let mut final_round_runs: Option<u32> = None;
    let mut arcane_only: Option<String> = None;
    for arg in std::env::args().skip(1) {
        if let Some(id) = arg.strip_prefix("require=") {
            constraints.require.push(id.to_string());
        } else if let Some(id) = arg.strip_prefix("forbid=") {
            constraints.forbid.push(id.to_string());
        } else if arg == "flat" {
            // Validation mode: no funnel — EVERY candidate gets the full
            // 1024 x 60 s treatment (much slower; verifies the funnel).
            flat = true;
        } else if arg == "target=thrax" {
            target_file = "thrax_centurion";
        } else if arg == "evo2=both" {
            evo2_both = true;
        } else if let Some(v) = arg.strip_prefix("duration=") {
            duration_seconds = v.parse().expect("duration=<secs>");
        } else if let Some(v) = arg.strip_prefix("runs=") {
            // Playoff mode: skip the funnel, run every job at this count.
            final_runs = Some(v.parse().expect("runs=<n>"));
        } else if let Some(v) = arg.strip_prefix("final=") {
            // Keep the funnel; override only the LAST round's run count.
            final_round_runs = Some(v.parse().expect("final=<n>"));
        } else if let Some(v) = arg.strip_prefix("arcane=") {
            // Legacy short names map to the data-driven pool ids; any
            // data/arcanes/secondary id is accepted directly.
            arcane_only = Some(
                match v {
                    "deadhead" => "secondary_deadhead",
                    "enervate" => "secondary_enervate",
                    "flare" => "cascadia_flare",
                    other => other,
                }
                .to_string(),
            );
        } else {
            eprintln!(
                "unknown arg: {arg} (use require=<id> / forbid=<id> / flat / target=thrax / evo2=both)"
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
        arena: wfsim_engine::arena::Arena {
            target_id: "e1".to_string(),
            // The CLI has no scenario UI, so it fights the NEUTRAL Tenno:
            // aiming, no frame, nothing running — resolve()'s own default.
            tenno: wfsim_engine::tenno_data::default_tenno().clone(),
            target: spec
                .target_params(9999, true, false, TargetMode::InstantRespawn)
                .expect("valid target"),
            body_parts: spec.aim_parts(&[("head", 1.0)]).expect("head aim"),
            // …at point blank, for the same reason: a range is a term of a
            // fight the CLI has no way to state, and 0 is the fight every
            // number this engine has reported was measured under.
            player_at: wfsim_engine::space::Vec2::ORIGIN,
            target_at: wfsim_engine::space::Vec2::new(0.0, wfsim_engine::space::CONTACT_RANGE_M),
            duration_seconds,
            // …and nothing is being cast on them either. A CLI search is a
            // statement about the WEAPON.
            abilities: Vec::new(),
            ability_picks: Vec::new(),
            ability_strength: 1.0,
            // ONE BODY — a fixture, not a formation.
            others: Vec::new(),
            // …and the weapon points AT it.
            aim_at: None,
        },
        // The CLI drives Dual Toxocyst, which carries the Frenzy passive.
        frenzy: wfsim_engine::weapons_data::has_perk("dual_toxocyst", "frenzy"),
        // Unused here: the CLI runs the cycle, which bakes its own lock.
        frenzy_locks: Vec::new(),
        // The CLI drives one weapon with an infinite reserve and no companion,
        // so these are the ordinary answers rather than a choice it offers.
        infinite_ammo: true,
        policy: wfsim_engine::loadout::StackPolicy::Emergent,
        // The REAL Incarnon cycle (user flow): full gauge start -> dump ->
        // revert 1.0 s -> rebuild 9 weakpoint charges in base form ->
        // transmute 2.35 s -> repeat. Frenzy locked Permanent (chosen
        // sim setting) - its +100% Toxin injection is folded into the
        // base-form panel.
        incarnon_cycle: true,
        frenzy_lock: LockMode::Permanent,
        // CLI: no per-buff configured policy (the emergent default).
        buff_cfg: wfsim_engine::dummy::BuffConfig::new(),
    };
    println!(
        "[scenario] {} @9999 STEEL PATH, instant respawn, 100% headshots, {} s, REAL incarnon cycle",
        scenario.arena.target.name, scenario.arena.duration_seconds
    );
    println!(
        "  pools: overguard {:.3e}, shield {:.3e}, health {:.3e}, armor {:.0}{}",
        scenario.arena.target.overguard(),
        scenario.arena.target.max_shield(),
        scenario.arena.target.max_health(),
        scenario.arena.target.armor(),
        match scenario.arena.target.attenuation {
            Some(a) => format!(
                " | attenuation: instance {:.0}% / dps {:.0}% of max HP (ESTIMATES)",
                a.instance_fraction * 100.0,
                a.dps_fraction * 100.0
            ),
            None => String::new(),
        }
    );

    let p = pool();
    let t0 = Instant::now();
    // The Evolution II choice is a SEARCH DIMENSION (user, 2026-07-25):
    // the whole canonical space is enumerated against BOTH weapon
    // configs (both forms resolved with Frenzy's Toxin injection).
    let mut cands = Vec::new();
    // Fevered Frenzy is the LOCKED official Evolution II (user,
    // 2026-07-25: it swept all three benchmark scenarios); `evo2=both`
    // re-opens the comparison. Evolution ids are DATA — the engine's
    // from_data builds either form from them.
    let evo2s: &[(&'static str, &'static str)] = if evo2_both {
        &[
            ("dual_toxocyst_fevered_frenzy", "fevered"),
            ("dual_toxocyst_carnage_reign", "carnage"),
        ]
    } else {
        &[("dual_toxocyst_fevered_frenzy", "fevered")]
    };
    // `variant` is now an index into this evo-set label table (widened from a
    // &str so the web can search arbitrary per-tier evolution sets).
    let variant_labels: Vec<&'static str> = evo2s.iter().map(|(_, l)| *l).collect();
    for (vi, &(evo2, label)) in evo2s.iter().enumerate() {
        let evos = ["dual_toxocyst_commodores_fortune", "dual_toxocyst_evolved_autoloader", evo2];
        let base = WeaponBase::from_data("dual_toxocyst_incarnon", true, &evos);
        let base_form = WeaponBase::from_data("dual_toxocyst", true, &evos);
        let (mut c, stats) = enumerate_candidates(
            &p,
            &base,
            Some(&base_form),
            vi as u32,
            8, // exact 8-mod builds: the CLI stress test's classic space
            8,
            60,
            &wfsim_engine::weapons_data::innate_slots("dual_toxocyst"),
            &constraints,
            &[None], // CLI stress test: exilus slot left empty
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

    // The arcane is a SEARCH DIMENSION like the mod choice (user,
    // 2026-07-25): every candidate is evaluated under each arcane,
    // resolved at MAX RANK from data/arcanes/secondary under the Emergent
    // policy (non-simmable triggers are honest no-ops there).
    use wfsim_engine::arcanes_data::{self, ArcaneFx};
    use wfsim_engine::loadout::StackPolicy;
    let arc_base = wfsim_engine::loadout::WeaponBase::from_data(
        "dual_toxocyst",
        true,
        &[
            "dual_toxocyst_commodores_fortune",
            "dual_toxocyst_evolved_autoloader",
            "dual_toxocyst_fevered_frenzy",
        ],
    );
    let fx_of = |id: &str| -> ArcaneFx {
        if id == "none" {
            return ArcaneFx::none();
        }
        let d = arcanes_data::secondary(id).unwrap_or_else(|| panic!("unknown arcane id: {id}"));
        d.fx(d.max_rank, StackPolicy::Emergent, arc_base.traits, wfsim_engine::tenno_data::default_tenno())
    };
    let arcanes: Vec<ArcaneFx> = match &arcane_only {
        Some(a) => vec![fx_of(a)],
        None => ["secondary_enervate", "secondary_deadhead", "cascadia_flare"]
            .iter()
            .map(|id| fx_of(id))
            .collect(),
    };
    let alive: Vec<Job> = (0..cands.len())
        .flat_map(|i| (0..arcanes.len()).map(move |a| (i, a)))
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
    // The multi-round funnel now lives in the lib (shared with the web
    // endpoint); the CLI runs it verbosely for per-round progress.
    let last = run_funnel(
        &cands,
        &arcanes,
        &scenario,
        alive,
        &rounds,
        0xDEAD_BEEF,
        true,
        None,
        None,
        0,    // the CLI always runs a fresh funnel
        None, // and has nowhere to checkpoint to
        None, // (nor anywhere to publish a best-so-far — it prints per round)
    );

    println!();
    println!("=== FINAL LEADERBOARD (1024 x 60 s, kill score; searched: mods x element order x arcane x evo2) ===");
    for (rank, ((ci, ai), s)) in last.iter().take(10).enumerate() {
        let c = &cands[*ci];
        let arcane = if arcanes[*ai].id.is_empty() {
            "none"
        } else {
            &arcanes[*ai].id
        };
        let names: Vec<&str> = c.ordered.iter().map(|&i| p[i].id).collect();
        let vec_desc: Vec<String> = c
            .panel
            .damage
            .iter_nonzero()
            .map(|(t, v)| format!("{t:?} {v:.0}"))
            .collect();
        println!(
            "#{:<2} score {:.3} (kills {:.3} ± {:.3}, min {} max {}) | {} + {} | eff DPS {:.3e} | {:.1} transforms",
            rank + 1,
            s.mean_kill_progress,
            s.mean_kills,
            s.std_kills,
            s.min_kills,
            s.max_kills,
            arcane,
            variant_labels[c.variant as usize],
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
        let ms_desc = match &c.panel.multishot_stack {
            Some(s) => format!(
                "{:.2}+{:.1}x{} earned",
                c.panel.multishot, s.per_stack, s.max_stacks
            ),
            None => format!("{:.2}", c.panel.multishot),
        };
        println!(
            "    panel: {} | cc {:.1}% cd {:.2}x sc {:.1}% fr {:.2} multishot {} | {} | {} forma, {}/60",
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
