//! WHAT A RADIUS MOD IS WORTH ON A FORMATION — the first number of its kind.
//!
//! A damage radius has nothing to act on against one target, so every
//! calculator prices Firestorm at zero or at a guess. Its value is a question
//! about GEOMETRY: how many bodies the sphere catches, and therefore how many
//! chains start. This answers it exactly, for a formation you describe.
//!
//!   cargo run --release --bin formation_value -- [cols] [rows] [spacing_m]
//!
//! # What this is, and what it is not
//!
//! It is the SHOT's geometry: `chain::resolve` over a formation, summed. Every
//! body is assumed identical and at full health, which is what makes the answer
//! a clean multiplier — the damage the formation takes, relative to the same
//! shot against one enemy.
//!
//! It is NOT the fight. Per-body armor, status, death and re-targeting live in
//! the run loop, which does not consume `chain` yet (`engine::formation` is the
//! layer it will). Those change WHO dies and WHEN; they do not change the
//! multiplier below, because it is what one shot delivers before anything on
//! the receiving end has had a say.
use wfsim_engine::chain::{resolve, Spec, Splash};
use wfsim_engine::formation::Formation;

/// The Torid Incarnon's, and the only chaining beam in the roster today.
const TORID: Spec = Spec { hops: 5, range_m: 7.0, falloff: 0.75, compounds: true };
const RADIUS_M: f64 = 2.3;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let cols: usize = a.first().and_then(|s| s.parse().ok()).unwrap_or(3);
    let rows: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(3);
    let spacing: f64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(3.0);

    let f = Formation::grid(
        wfsim_engine::dummy::TargetParams::training_dummy(),
        wfsim_engine::dummy::DummyParams::humanoid_parts(),
        cols,
        rows,
        spacing,
        wfsim_engine::space::Vec2::new(0.0, 5.0),
    );
    let pos = f.positions();
    let at = pos[f.aimed];

    // …and the mods that move the radius. Firestorm is +24%, Primed +44%
    // (`data/mods/rifle/`), and neither touches the chain range — the weapon's
    // own page credits Firestorm with the damage radius and nothing else
    // (MECHANICS §12).
    let mods: [(&str, f64); 3] = [("nothing", 0.0), ("Firestorm", 0.24), ("Primed Firestorm", 0.44)];

    println!(
        "{cols}x{rows} formation at {spacing} m, aimed at the front row's middle body\n\
         chain: {} hops, {} m, {}% a hop\n",
        TORID.hops,
        TORID.range_m,
        TORID.falloff * 100.0
    );
    println!(
        "{:<20}{:>9}{:>8}{:>11}{:>13}{:>11}{:>12}",
        "radius mod", "radius", "seeds", "instances", "total damage", "vs bare", "headshot"
    );
    let mut bare = 0.0;
    for (name, bonus) in mods {
        let r = RADIUS_M * (1.0 + bonus);
        let v = resolve(&pos, &[f.aimed], Splash { at, radius_m: r }, TORID);
        let total: f64 = v.iter().map(|i| i.share).sum();
        let seeds = v.iter().filter(|i| i.share == 1.0).count();
        if bonus == 0.0 {
            bare = total;
        }
        println!(
            "{name:<20}{r:>8.2}m{seeds:>8}{:>11}{total:>13.2}{:>10.2}x{:>11.1}%",
            v.len(),
            total / bare,
            100.0 / total
        );
    }

    // WHERE THE STEP EDGES ARE, which is the actionable half: a radius mod is
    // worth nothing at all past its own reach, and worth nothing again once the
    // bare radius already covers the crowd.
    println!("\nthe same formation at other spacings — Primed Firestorm's worth:\n");
    println!("{:<10}{:>8}{:>9}{:>11}", "spacing", "bare", "primed", "worth");
    for s in [1.0, 1.5, 2.0, 2.34, 2.5, 3.0, 3.31, 3.5, 4.0, 5.0] {
        let g = Formation::grid(
            wfsim_engine::dummy::TargetParams::training_dummy(),
            wfsim_engine::dummy::DummyParams::humanoid_parts(),
            cols,
            rows,
            s,
            wfsim_engine::space::Vec2::new(0.0, 5.0),
        );
        let p = g.positions();
        let a0 = p[g.aimed];
        let tot = |r: f64| -> f64 {
            resolve(&p, &[g.aimed], Splash { at: a0, radius_m: r }, TORID)
                .iter()
                .map(|i| i.share)
                .sum()
        };
        let (b, pf) = (tot(RADIUS_M), tot(RADIUS_M * 1.44));
        println!("{s:<9.2}m{b:>8.2}{pf:>9.2}{:>10.2}x", pf / b);
    }
    // THE EDGES ARE THE REACH, NOT THE RADIUS. Any part of a body touching the
    // sphere is caught (owner, 2026-08-17), so what a mod really covers is its
    // radius plus a body radius — and that is the number every step above sits
    // on. Reading the radius alone put both edges one body too close in, and
    // said a mod was worth nothing that is worth four times the shot.
    let br = wfsim_engine::space::BODY_RADIUS_M;
    println!(
        "
the edges are the REACH — radius plus a body radius, because any part of a
body touching the sphere is caught: bare {:.2} m, Firestorm {:.2} m, primed
{:.2} m. {:.2} m = the primed reach over sqrt(2) is where diagonals come in.",
        RADIUS_M + br,
        RADIUS_M * 1.24 + br,
        RADIUS_M * 1.44 + br,
        (RADIUS_M * 1.44 + br) / 2f64.sqrt()
    );
}
