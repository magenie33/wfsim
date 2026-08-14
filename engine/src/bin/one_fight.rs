//! `one_fight` — WHAT ONE ENGAGEMENT COSTS, and what a change to the engine
//! did to it.
//!
//! The repo has [`wfsim-truth`] for the search's ACCURACY and had nothing for
//! its COST, which is half of every performance conversation and the half
//! nobody could settle. "It feels faster" and "it got dumber" are
//! indistinguishable without both numbers (community request via the owner,
//! 2026-08-14).
//!
//! It is a plain binary rather than `cargo bench`: no nightly, no criterion,
//! no dependency — the workspace has none and this is not the place to start.
//!
//! ```text
//! cargo run --release --bin one_fight
//! cargo run --release --bin one_fight -- weapon=gotva_prime runs=2000 duration=60
//! cargo run --release --bin one_fight -- mods=serration,split_chamber repeats=5
//! cargo run --release --bin one_fight -- enemy=training      # no target at all
//! ```
//!
//! THE RULER'S OWN FIGHT by default — a Thrax Centurion at 9999 Steel Path,
//! the target the board is scored against. Its armour, shields and overguard
//! are real work, and against a training dummy the weapon fires into a wall:
//! kill progress reads 0 there, so the answer column cannot catch a change
//! either.
//!
//! WHAT IT PRINTS is a cost per RUN and a cost per SHOT, because the two answer
//! different questions: per-run is what the optimizer multiplies by, per-shot
//! is what a change to the inner loop moves. It also prints the mean damage, so
//! a "speed-up" that changed the answer is visible in the same output — the one
//! failure mode this tool exists to catch.
//!
//! HOW TO READ IT. Repeats are separate `monte_carlo` calls at the same seed,
//! so they differ only by the machine. Take the MINIMUM, not the mean: the
//! minimum is the run that was interrupted least, and on this machine the
//! spread between repeats is around 2%. A change under 5% is not a result.

use std::time::Instant;

use wfsim_engine::arcanes_data::ArcaneFx;
use wfsim_engine::arena::Arena;
use wfsim_engine::dummy::TargetMode;
use wfsim_engine::dummy::{monte_carlo, DummyParams};
use wfsim_engine::loadout::{resolve, StackPolicy, WeaponBase};

/// The default build: eight mods a real rifle build carries, so the fight
/// exercises the elemental hierarchy, crit, status and DoTs rather than a bare
/// weapon's arithmetic.
const DEFAULT_MODS: &str = "serration,split_chamber,point_strike,vital_sense,\
                            hellfire,cryo_rounds,infected_clip,stormbringer";

fn arg<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    args.iter()
        .find_map(|a| a.strip_prefix(&format!("{key}=")))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "one_fight [weapon=torid] [mods=a,b,…] [runs=1000] [duration=180]\n\
                       [enemy=thrax_centurion] [level=9999] [steel_path=1]\n\
                       [seed=24301] [repeats=3]\n\n\
             Cost of one engagement, and the answer beside it so a speed-up\n\
             that moved a number is visible. Take the MINIMUM of the repeats."
        );
        return;
    }
    let weapon = arg(&args, "weapon").unwrap_or("torid");
    let mod_ids: Vec<&str> = arg(&args, "mods").unwrap_or(DEFAULT_MODS).split(',').collect();
    let runs: u32 = arg(&args, "runs").and_then(|s| s.parse().ok()).unwrap_or(1000);
    let duration: f64 = arg(&args, "duration").and_then(|s| s.parse().ok()).unwrap_or(180.0);
    let seed: u64 = arg(&args, "seed").and_then(|s| s.parse().ok()).unwrap_or(0x5EED);
    let repeats: u32 = arg(&args, "repeats").and_then(|s| s.parse().ok()).unwrap_or(3);
    let enemy = arg(&args, "enemy").unwrap_or("thrax_centurion");
    let level: u32 = arg(&args, "level").and_then(|s| s.parse().ok()).unwrap_or(9999);
    let steel_path = arg(&args, "steel_path").is_none_or(|s| s != "0");

    // `enemy=training` is the bare dummy, kept because it isolates the
    // weapon's own arithmetic from every mitigation layer — the right
    // fixture for "what did I do to the damage pipeline", the wrong one for
    // "what does a search pay".
    let arena = if enemy == "training" {
        Arena::training(duration)
    } else {
        let e = wfsim_engine::enemy_data::all()
            .into_iter()
            .find(|e| e.id == enemy)
            .unwrap_or_else(|| panic!("unknown enemy: {enemy}"));
        Arena {
            tenno: wfsim_engine::tenno_data::default_tenno().clone(),
            target: e
                .target_params(level, steel_path, e.can_be_eximus, TargetMode::InstantRespawn)
                .expect("the ruler's own target"),
            body_parts: e.aim_parts(&[("body", 1.0)]).expect("a body to hit"),
            duration_secs: duration,
            abilities: Vec::new(),
        }
    };

    let base = WeaponBase::from_data(weapon, true, &[]);
    let pool = wfsim_engine::mods_data::pool_for_weapon(weapon);
    let mut refs = Vec::new();
    for id in &mod_ids {
        match pool.iter().find(|m| m.id == *id) {
            Some(d) => refs.push(d),
            // NAMED, not counted. A mod this weapon cannot hold is a different
            // build from the one you asked to measure, and silently dropping it
            // is how two runs of "the same" benchmark stop being comparable.
            None => println!("  ! {weapon} cannot equip {id} — dropped"),
        }
    }
    let panel = resolve(&base, &refs, StackPolicy::Emergent);
    let params = DummyParams::from_panel(&panel, &arena, &ArcaneFx::none());

    // Warm: the first call pays for whatever the allocator and the branch
    // predictors have not seen yet, and it is not what a search pays per
    // candidate.
    monte_carlo(&params, 20.min(runs), seed);

    println!(
        "{weapon} · {} mods · {enemy} lv {level}{} · {duration:.0} s · {runs} runs · seed {seed:#x}",
        refs.len(),
        if steel_path { " SP" } else { "" }
    );
    let mut best = f64::INFINITY;
    let mut answer = (0.0f64, 0.0f64);
    for i in 0..repeats.max(1) {
        let t = Instant::now();
        let s = monte_carlo(&params, runs, seed);
        let el = t.elapsed().as_secs_f64();
        best = best.min(el);
        answer = (s.mean_kill_progress, s.mean_effective_damage);
        println!(
            "  run {}  {el:>7.3} s   {:>7.3} ms/run   {:>7.0} ns/shot",
            i + 1,
            el / f64::from(runs) * 1e3,
            el / f64::from(runs) / s.mean_shots.max(1.0) * 1e9,
        );
    }
    let s = monte_carlo(&params, runs.min(200), seed);
    println!(
        "  best  {best:>7.3} s   {:>7.3} ms/run   {:>7.0} ns/shot",
        best / f64::from(runs) * 1e3,
        best / f64::from(runs) / s.mean_shots.max(1.0) * 1e9,
    );
    // THE ANSWER, so a faster engine that computes something else is caught in
    // the same glance. Both are means over every run, so they are stable to
    // many digits at this run count and a real change is obvious.
    println!(
        "  shots/run {:>8.1}   procs/run {:>7.1}\n  kill progress {:.9}   effective damage {:.4}",
        s.mean_shots, s.mean_procs, answer.0, answer.1
    );
}
