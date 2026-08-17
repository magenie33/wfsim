//! `one_fight` — WHAT AN ENGAGEMENT COSTS, and whether your change to the
//! engine made it cheaper WITHOUT making it wrong.
//!
//! The repo has [`wfsim-truth`] for the search's ACCURACY and had nothing for
//! its COST, which is half of every performance conversation and the half
//! nobody could settle: "it feels faster" and "it got dumber" are
//! indistinguishable without both numbers (community request via the owner,
//! 2026-08-14).
//!
//! # The loop this is for
//!
//! ```text
//! cargo run --release --bin one_fight -- save     # remember where you started
//! …edit the engine…
//! cargo run --release --bin one_fight             # what did it cost, what did it change
//! ```
//!
//! The second command prints a delta against your saved baseline AND whether
//! the answer moved. **An optimisation that changes a number is not an
//! optimisation, it is a bug**, and that is the one thing this must never let
//! you miss — so a moved answer is a non-zero exit code, not a line of text you
//! might scroll past.
//!
//! # Why a SUITE and not one weapon
//!
//! A change to the inner loop rarely moves every weapon the same way, and
//! picking one to measure is how you optimise for the shape you happened to
//! choose. Measured while writing this: `-C target-cpu=native` is −23% on the
//! Torid, −36% on the Scourge and **+31% on the Gotva Prime**. One weapon would
//! have said "ship it" and one would have said "revert", both truthfully.
//!
//! So the default is three shapes that stress different parts of the engine,
//! and the table is read across, not down.
//!
//! # One shape, every knob
//!
//! ```text
//! cargo run --release --bin one_fight -- weapon=gotva_prime runs=2000 duration=60
//! cargo run --release --bin one_fight -- weapon=torid mods=serration,split_chamber
//! cargo run --release --bin one_fight -- enemy=training     # no mitigation at all
//! ```
//!
//! `enemy=training` is the bare dummy: it isolates the weapon's own arithmetic
//! from armour, shields and overguard, which is the right fixture for "what did
//! I do to the damage pipeline" and the wrong one for "what does a search pay"
//! — the dummy cannot be killed, so kill progress reads 0 and the answer column
//! stops being able to catch anything.
//!
//! # How to read it
//!
//! Repeats are separate `monte_carlo` calls at the same seed, so they differ
//! only by the machine. The tool takes the MINIMUM and prints the spread it
//! saw; a delta smaller than that spread is not a result, and it says so
//! rather than leaving you to decide.
//!
//! Native, not wasm. The product runs in a browser and this is a proxy for it —
//! good for ranking two versions of the same code, not for predicting what a
//! phone will do.

use std::time::Instant;

use wfsim_engine::arcanes_data::ArcaneFx;
use wfsim_engine::arena::Arena;
use wfsim_engine::dummy::{monte_carlo, DummyParams, TargetMode};
use wfsim_engine::loadout::{resolve, StackPolicy, WeaponBase};

/// The default build: eight mods a real rifle build carries, so the fight
/// exercises the elemental hierarchy, crit, status and DoTs rather than a bare
/// weapon's arithmetic.
const DEFAULT_MODS: &str = "serration,split_chamber,point_strike,vital_sense,\
                            hellfire,cryo_rounds,infected_clip,stormbringer";

/// THE SUITE, and each entry is here because it stresses something the others
/// do not. A change that helps one and hurts another is the normal case, not
/// the exception, so the default measurement has to be able to say so.
const SUITE: &[(&str, &str)] = &[
    // A launcher with a LINGERING FIELD: few shots, ~900 procs, and the DoT
    // bookkeeping dominates.
    ("torid", "the field/DoT shape — 180 shots, ~900 procs"),
    // A high fire-rate rifle: ~1800 shots, and the per-shot path dominates.
    ("gotva_prime", "the per-shot shape — ~1800 shots"),
    // A projectile with a RADIAL on every shot: two damage instances a trigger
    // pull, each with its own crit and status roll.
    ("scourge", "the radial shape — an explosion on every shot"),
];

/// Where a baseline lives: under `target/`, which is machine-local and already
/// ignored. A baseline is a property of THIS machine on THIS day — committing
/// one would be publishing somebody else's CPU.
const BASELINE: &str = "target/one_fight.baseline";

struct Shape {
    weapon: String,
    ms_per_run: f64,
    ns_per_shot: f64,
    /// The spread across repeats, as a fraction of the best. The noise floor
    /// this machine is offering today.
    spread: f64,
    shots: f64,
    procs: f64,
    /// The ANSWER. Two means over every run, so they are stable to many digits
    /// and a real change is unmissable.
    kill_progress: f64,
    damage: f64,
}

fn arg<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    args.iter().find_map(|a| a.strip_prefix(&format!("{key}=")))
}

/// THE FIGHT, as one value. Ten positional arguments is a call nobody can read
/// and two of them are `u32` next to each other — `runs` and `level` — which is
/// a swap the compiler would never catch.
struct Cfg<'a> {
    mod_ids: &'a [&'a str],
    runs: u32,
    duration: f64,
    seed: u64,
    repeats: u32,
    enemy: &'a str,
    level: u32,
    steel_path: bool,
    verbose: bool,
}

/// A saved row: the shape's name, its two costs, and its two answers.
type BaseRow = (String, f64, f64, f64, f64);

/// THE FIGHT, built once. `measure` and `ablate` both need it, and when they
/// each built their own one of them was against a training dummy while the
/// other was against the ruler's Thrax — a profile and a cost table describing
/// two different fights, printed as if they described one.
///
/// `enemy=training` is the bare dummy, kept because it isolates the weapon's
/// own arithmetic from every mitigation layer: the right fixture for "what did
/// I do to the damage pipeline", the wrong one for "what does a search pay".
fn arena_for(c: &Cfg) -> Arena {
    if c.enemy == "training" {
        return Arena::training(c.duration);
    }
    let e = wfsim_engine::enemy_data::all()
        .into_iter()
        .find(|e| e.id == c.enemy)
        .unwrap_or_else(|| panic!("unknown enemy: {}", c.enemy));
    Arena {
        tenno: wfsim_engine::tenno_data::default_tenno().clone(),
        target: e
            .target_params(c.level, c.steel_path, e.can_be_eximus, TargetMode::InstantRespawn)
            .expect("the target this fight names"),
        body_parts: e.aim_parts(&[("body", 1.0)]).expect("a body to hit"),
        // POINT BLANK. This harness measures the COST of the engine and asserts
        // the ANSWER did not move, so its fight has to be the one every saved
        // baseline was taken under — a range would move a falloff weapon's
        // number and report an optimisation as a bug.
        player_at: wfsim_engine::space::Vec2::ORIGIN,
        target_at: wfsim_engine::space::Vec2::new(0.0, wfsim_engine::space::CONTACT_RANGE_M),
        duration_secs: c.duration,
        abilities: Vec::new(),
        // ONE BODY — a fixture, not a formation.
        others: Vec::new(),
        // …and the weapon points AT it.
        aim_at: None,
    }
}

fn measure(weapon: &str, c: &Cfg) -> Shape {
    let Cfg { mod_ids, runs, seed, repeats, verbose, .. } = *c;
    let base = WeaponBase::from_data(weapon, true, &[]);
    let pool = wfsim_engine::mods_data::pool_for_weapon(weapon);
    let mut refs = Vec::new();
    for id in mod_ids {
        match pool.iter().find(|m| m.id == *id) {
            Some(d) => refs.push(d),
            // NAMED, not counted. A mod this weapon cannot hold is a different
            // build from the one you asked for, and dropping it silently is how
            // two runs of "the same" benchmark stop being comparable.
            None => println!("  ! {weapon} cannot equip {id} — dropped"),
        }
    }
    let panel = resolve(&base, &refs, StackPolicy::Emergent);
    let arena = arena_for(c);
    let params = DummyParams::from_panel(&panel, &arena, &ArcaneFx::none());

    // Warm: the first call pays for whatever the allocator and the branch
    // predictors have not seen, which is not what a search pays per candidate.
    monte_carlo(&params, 20.min(runs), seed);

    let (mut best, mut worst) = (f64::INFINITY, 0.0f64);
    let mut last = None;
    for i in 0..repeats.max(1) {
        let t = Instant::now();
        let s = monte_carlo(&params, runs, seed);
        let el = t.elapsed().as_secs_f64();
        best = best.min(el);
        worst = worst.max(el);
        if verbose {
            println!("    repeat {}  {el:>7.3} s", i + 1);
        }
        last = Some(s);
    }
    let s = last.expect("at least one repeat");
    Shape {
        weapon: weapon.to_string(),
        ms_per_run: best / f64::from(runs) * 1e3,
        ns_per_shot: best / f64::from(runs) / s.mean_shots.max(1.0) * 1e9,
        spread: if best > 0.0 { (worst - best) / best } else { 0.0 },
        shots: s.mean_shots,
        procs: s.mean_procs,
        kill_progress: s.mean_kill_progress,
        damage: s.mean_effective_damage,
    }
}

/// WHERE THE TIME GOES, measured by TAKING WORK AWAY.
///
/// The harness validates a candidate; it could not help you find one, and
/// "read the code and guess" is not a method you can hand to somebody else. A
/// sampling profiler is the proper tool and is platform-specific — `perf` on
/// Linux, dtrace on macOS, and on Windows neither is installed by default —
/// so this is the one that works on every contributor's machine and needs
/// nothing but the repo.
///
/// It answers a coarser question than a profiler, and the right one to start
/// from: which SUBSYSTEM is worth attacking. Each row disables one and reports
/// what the fight then costs; the share it names is an upper bound on what
/// optimising that subsystem can ever return.
///
/// THE NUMBERS IT PRINTS ARE NOT SIMULATIONS OF ANYTHING. A fight with its
/// status turned off is not a fight anyone plays — it is this fight minus one
/// subsystem, which is exactly what a profile is.
fn ablate(weapon: &str, c: &Cfg) {
    let base = WeaponBase::from_data(weapon, true, &[]);
    let pool = wfsim_engine::mods_data::pool_for_weapon(weapon);
    let refs: Vec<_> = c
        .mod_ids
        .iter()
        .filter_map(|id| pool.iter().find(|m| m.id == *id))
        .collect();
    let panel = resolve(&base, &refs, StackPolicy::Emergent);
    // A FIXED-LENGTH FIGHT, always — `Arena::training` has infinite health, so
    // no variant can kill faster and change how much work there is to do.
    //
    // That is the methodological problem with ablation in a simulation whose
    // LENGTH depends on its damage, and it is not hypothetical: against the
    // ruler's Thrax, truncating the body-part list reported **−71.8%**. Removing
    // work made the fight SLOWER, because a build that stops headshotting takes
    // longer to kill and therefore does more. It profiled nothing.
    let full = DummyParams::from_panel(&panel, &Arena::training(c.duration), &ArcaneFx::none());

    // …and a fixed length is necessary, not sufficient: the SHOT COUNT has to
    // come out identical too, or the variant changed the fight rather than
    // removing work from it.
    let time = |p: &DummyParams| -> (f64, f64) {
        monte_carlo(p, 20.min(c.runs), c.seed);
        let (mut best, mut shots) = (f64::INFINITY, 0.0);
        for _ in 0..c.repeats.max(1) {
            let t0 = Instant::now();
            let s = monte_carlo(p, c.runs, c.seed);
            best = best.min(t0.elapsed().as_secs_f64());
            shots = s.mean_shots;
        }
        (best / f64::from(c.runs) * 1e3, shots)
    };

    // Each entry removes ONE thing. They are not additive — turning off status
    // also removes the DoTs it would have started — so the shares overlap and
    // the table is read as "at most this much", never summed.
    let (whole, whole_shots) = time(&full);
    let mut no_status = full.clone();
    no_status.status_chance = 0.0;
    no_status.base_status_chance = 0.0;
    no_status.forced_procs.clear();
    let mut no_crit = full.clone();
    no_crit.base_crit_chance = 0.0;
    let mut no_radial = full.clone();
    no_radial.radial = None;
    // NOT "one body part": truncating the list does not REMOVE work, it makes
    // every hit a headshot, which adds it — it reported −77% with an identical
    // shot count, which is the shape of a variant that is doing MORE. An
    // ablation axis has to take something away.
    let mut no_buffs = full.clone();
    no_buffs.stacking_buffs.clear();
    // THE LINGERING FIELD (the Torid'''s cloud). Its own axis because it is the
    // Torid'''s dominant cost and no other axis touches it: turning STATUS off
    // leaves the field ticking, which is why the status row reads ~0% there
    // while the fight is plainly doing something.
    let mut no_field = full.clone();
    no_field.lingering = None;

    println!(
        "{weapon} · {:.0} s · {} runs · fixed-length fight — where the time goes",
        c.duration, c.runs
    );
    println!("  whole fight                        {whole:>8.3} ms/run");
    for (name, p) in [
        ("status and everything it starts", &no_status),
        ("critical hits", &no_crit),
        ("the explosion", &no_radial),
        ("the stacking buffs", &no_buffs),
        ("the lingering field", &no_field),
    ] {
        let (t, shots) = time(p);
        let share = (whole - t) / whole * 100.0;
        // TWO WAYS A ROW IS NOT AN ABLATION, and both have happened here.
        //
        // A different SHOT COUNT means the variant is a different fight, and
        // the time difference is the two mixed together. A NEGATIVE share means
        // the variant costs more — so it did not remove work, it changed which
        // work happens (truncating the body-part list made every hit a
        // headshot: −77%, with the shot count unmoved). Neither is a profile,
        // and printing a percentage for either would be worse than printing
        // nothing.
        if (shots - whole_shots).abs() > 1e-9 {
            println!(
                "  without {name:<32} — not an ablation: {shots:.0} shots against {whole_shots:.0}"
            );
        } else if share < -1.0 {
            println!("  without {name:<32} — not an ablation: it costs MORE ({t:.3} ms/run)");
        } else {
            println!(
                "  without {name:<32} {t:>8.3}   at most {share:>5.1}% is here{}",
                if share < 1.0 { "  (nothing to win)" } else { "" }
            );
        }
    }
    println!(
        "\n  Shares OVERLAP — removing status also removes the DoTs it starts — so\n\
        \x20 read each as a ceiling on what optimising that part can return, and\n\
        \x20 never add them up. A row near 0% is a part this fight does not use.\n"
    );
}

/// A baseline is a CONFIG line and then four numbers a shape: the two costs
/// and the two answers. Plain text on purpose — the workspace has no JSON
/// dependency and this file is meant to be readable when it disagrees with you.
fn write_baseline(cfg: &str, shapes: &[Shape]) -> std::io::Result<()> {
    let mut body = format!("#{cfg}\n");
    for s in shapes {
        // `{:?}` on an f64 is Rust's shortest ROUND-TRIPPING form; `{:.6}` was
        // not, so the baseline lost the last digits of the damage and the
        // answer column fired on the tool's own writing.
        body.push_str(&format!(
            "{}\t{:.6}\t{:.3}\t{:?}\t{:?}\n",
            s.weapon, s.ms_per_run, s.ns_per_shot, s.kill_progress, s.damage
        ));
    }
    std::fs::write(BASELINE, body)
}

/// The saved rows AND the config they were taken under.
///
/// A baseline at 300 runs diffed against a measurement at 200 reads as a 10%
/// win and is nothing of the kind. The tool caught its own author out that way
/// within an hour of being written, so the guard is not hypothetical — and a
/// contributor comparing two engines is exactly who would hit it.
fn read_baseline() -> (String, Vec<BaseRow>) {
    let text = std::fs::read_to_string(BASELINE).unwrap_or_default();
    let cfg = text
        .lines()
        .find_map(|l| l.strip_prefix('#'))
        .unwrap_or_default()
        .to_string();
    let rows = text
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() < 5 {
                return None;
            }
            Some((
                f[0].to_string(),
                f[1].parse().ok()?,
                f[2].parse().ok()?,
                f[3].parse().ok()?,
                f[4].parse().ok()?,
            ))
        })
        .collect();
    (cfg, rows)
}


fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "one_fight — what an engagement costs, and whether your change made it wrong.\n\n\
             \x20 cargo run --release --bin one_fight -- save    remember where you started\n\
             \x20 cargo run --release --bin one_fight            measure, and diff the baseline\n\n\
             Knobs (any of them, in any order):\n\
             \x20 weapon=torid          one shape instead of the suite\n\
             \x20 mods=a,b,c            default is eight a real rifle build carries\n\
             \x20 runs=1000  duration=180  seed=24301  repeats=3\n\
             \x20 enemy=thrax_centurion  level=9999  steel_path=1\n\
             \x20 enemy=training        no mitigation — the weapon's own arithmetic\n\
             \x20 -v                    print every repeat\n\
             \x20 ablate                where the time goes, by subsystem\n\n\
             Exit code is non-zero when an ANSWER moved: that is not a speed-up."
        );
        return std::process::ExitCode::SUCCESS;
    }
    let save = args.iter().any(|a| a == "save");
    let ablate_mode = args.iter().any(|a| a == "ablate");
    let verbose = args.iter().any(|a| a == "-v");
    let mod_ids: Vec<&str> = arg(&args, "mods").unwrap_or(DEFAULT_MODS).split(',').collect();
    let runs: u32 = arg(&args, "runs").and_then(|s| s.parse().ok()).unwrap_or(1000);
    let duration: f64 = arg(&args, "duration").and_then(|s| s.parse().ok()).unwrap_or(180.0);
    let seed: u64 = arg(&args, "seed").and_then(|s| s.parse().ok()).unwrap_or(0x5EED);
    let repeats: u32 = arg(&args, "repeats").and_then(|s| s.parse().ok()).unwrap_or(3);
    let enemy = arg(&args, "enemy").unwrap_or("thrax_centurion");
    let level: u32 = arg(&args, "level").and_then(|s| s.parse().ok()).unwrap_or(9999);
    let steel_path = arg(&args, "steel_path").is_none_or(|s| s != "0");

    let shapes: Vec<(&str, &str)> = match arg(&args, "weapon") {
        Some(w) => vec![(w, "")],
        None => SUITE.to_vec(),
    };
    // ONE STRING FOR BOTH: what the header prints is what the baseline
    // stores and what the guard compares, so the three can never describe
    // different fights.
    let cfg = format!(
        "{} mods · {enemy} lv {level}{} · {duration:.0} s · {runs} runs × {repeats} · seed {seed:#x}",
        mod_ids.len(),
        if steel_path { " SP" } else { "" }
    );
    println!("{cfg}
");

    let cfg_v0 = Cfg {
        mod_ids: &mod_ids, runs, duration, seed, repeats, enemy, level, steel_path, verbose,
    };
    if ablate_mode {
        for (w, _) in &shapes {
            ablate(w, &cfg_v0);
        }
        return std::process::ExitCode::SUCCESS;
    }
    let cfg_v = cfg_v0;
    let measured: Vec<Shape> = shapes
        .iter()
        .map(|(w, note)| {
            if verbose && !note.is_empty() {
                println!("  {w} — {note}");
            }
            measure(w, &cfg_v)
        })
        .collect();

    if save {
        match write_baseline(&cfg, &measured) {
            Ok(()) => println!("baseline saved to {BASELINE} — edit the engine, then run again"),
            Err(e) => println!("could not save the baseline: {e}"),
        }
    }

    let (base_cfg, base) = read_baseline();
    // A DIFFERENT FIGHT IS NOT A COMPARISON. Refused rather than silently
    // diffed, and it says what to do about it.
    let same_fight = base_cfg == cfg;
    if !base.is_empty() && !save && !same_fight {
        println!("  ! the baseline was taken under a different fight, so no delta is shown");
        println!("      baseline: {base_cfg}");
        println!("      now:      {cfg}");
        println!("    re-run with those settings, or `save` a new baseline.
");
    }
    let has_base = !base.is_empty() && !save && same_fight;
    let col = if has_base { "vs base" } else { "noise" };
    println!("{:<14} {:>9} {:>10} {col:>9}  answer", "shape", "ms/run", "ns/shot");

    let mut moved = 0usize;
    let (mut sum_now, mut sum_was) = (0.0f64, 0.0f64);
    for s in &measured {
        let prior = base.iter().find(|b| b.0 == s.weapon);
        // THE ANSWER FIRST, because it decides whether the cost column means
        // anything at all. Compared EXACTLY: both are means over the same runs
        // from the same seed, so a correct change reproduces them bit for bit
        // and a tolerance here would only hide the thing this is for.
        let answer = match prior.filter(|_| has_base) {
            None => "—".to_string(),
            Some(b) => {
                if b.3 == s.kill_progress && b.4 == s.damage {
                    "same".to_string()
                } else {
                    moved += 1;
                    // NAME THE ONE THAT MOVED, and print it round-trippably.
                    // Reporting "MOVED 0.185849125 → 0.185849125" because the
                    // OTHER number changed is a message that reads as a bug in
                    // the tool, and the reader stops trusting the column.
                    let (what, was, now) = if b.3 != s.kill_progress {
                        ("kill progress", b.3, s.kill_progress)
                    } else {
                        ("damage", b.4, s.damage)
                    };
                    format!("MOVED  {what} {was:?} → {now:?}")
                }
            }
        };
        let delta = match prior {
            Some(b) if has_base => {
                sum_now += s.ms_per_run;
                sum_was += b.1;
                let d = (s.ms_per_run - b.1) / b.1;
                // A DELTA UNDER THE MACHINE'S OWN SPREAD IS NOT A RESULT, and
                // saying "−1.8%" for it invites a conclusion the measurement
                // cannot support.
                if d.abs() < s.spread.max(0.01) {
                    "  ~same".to_string()
                } else {
                    format!("{:>+7.1}%", d * 100.0)
                }
            }
            _ => format!("±{:.1}%", s.spread * 100.0),
        };
        println!(
            "{:<14} {:>9.3} {:>10.0} {:>9}  {}",
            s.weapon, s.ms_per_run, s.ns_per_shot, delta, answer
        );
    }
    if verbose {
        println!();
        for s in &measured {
            println!(
                "  {:<14} shots {:>7.1}  procs {:>7.1}  kill progress {:.9}",
                s.weapon, s.shots, s.procs, s.kill_progress
            );
        }
    }

    println!();
    if moved > 0 {
        // NON-ZERO EXIT, so a script cannot ignore it and a person cannot
        // scroll past it. A faster engine that computes something else is the
        // one failure this tool exists to catch.
        println!(
            "{moved} of {} answers MOVED. This is not a speed-up — the engine now computes\n\
             something else. Find out what before reading the cost column.",
            measured.len()
        );
        return std::process::ExitCode::FAILURE;
    }
    if has_base && sum_was > 0.0 {
        let d = (sum_now - sum_was) / sum_was;
        println!(
            "every answer unchanged · suite total {:+.1}%{}",
            d * 100.0,
            if d.abs() < 0.02 { "  (inside the noise — not a result)" } else { "" }
        );
    } else if !save {
        println!("no baseline yet — `cargo run --release --bin one_fight -- save` to make one");
    }
    std::process::ExitCode::SUCCESS
}
