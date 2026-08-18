//! WHAT A FORMATION COSTS TO SIMULATE — the other half of `formation_value`.
//!
//! That tool asks what a crowd is WORTH; this one asks what it costs to find
//! out, which is the question a BOARD RULER has to answer before it exists: the
//! rulers run 1000 runs of every stored row on every push, so a fight nobody
//! can afford is a fight nobody can score.
//!
//!   cargo run --release --bin formation_cost -- [spacing_m] [runs] [duration_s]
//!
//! It walks odd-sided grids — odd because a ruler wants an exact CENTRE to aim
//! at — and reports ms per run, so the cost of a proposed ruler is read off
//! rather than argued about. `bodies` is how many the weapon actually reached,
//! which is the other thing a size has to buy: a grid whose extra rows are
//! never touched is paying for nothing.
//!
//! The fixture is the Torid's Incarnon form: a chaining beam with a 2.3 m
//! damage sphere, so every spread mechanism the engine has is live at once and
//! the number is an upper bound rather than a typical one.
use std::time::Instant;
use wfsim_engine::dummy::{run_once, DummyParams};
use wfsim_engine::rng::Rng;
use wfsim_engine::space::Vec2;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let spacing: f64 = a.first().and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let runs: u32 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(100);
    let secs: f64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(180.0);
    // WHICH WEAPON, because the answer is not one number: a chaining beam
    // saturates at a handful of bodies however big the grid gets, while an
    // infinite-punch-through weapon reaches as deep as the grid is.
    let weapon: String = a
        .get(3)
        .filter(|s| !s.contains('='))
        .cloned()
        .unwrap_or_else(|| "torid_incarnon".into());
    // …AND WHAT IT IS CARRYING, because a bare weapon is not what costs
    // anything. A full status build generates several times the procs, DoTs and
    // ticks a bare one does, and an arcane that fires an EXTRA HIT doubles the
    // instances every one of those is counted from — which is the shape the
    // owner reported as unusable on a phone (2026-08-18: the Phantasma Prime
    // with Primary Debilitate). `k=v` anywhere in the arguments, like one_fight.
    let kv = |k: &str| {
        a.iter()
            .find_map(|s| s.strip_prefix(&format!("{k}=")))
            .map(str::to_string)
    };
    let split = |s: Option<String>| -> Vec<String> {
        s.map(|s| s.split(',').filter(|x| !x.is_empty()).map(str::to_string).collect())
            .unwrap_or_default()
    };
    let mod_ids = split(kv("mods"));
    let evos = split(kv("evolutions"));
    let arcane = kv("arcane");

    // A REAL UNIT at a level it can die at: a training dummy has infinite
    // health, and a crowd that cannot die never re-targets, never feeds an
    // on-kill buff and never shortens a run — which would flatter the cost.
    let specs = wfsim_engine::enemy_data::all();
    let unit = specs
        .iter()
        .find(|e| e.id == "corrupted_heavy_gunner")
        .expect("the roster has one");
    let foe = unit
        .target_params(60, false, false, wfsim_engine::dummy::TargetMode::InstantRespawn)
        .expect("an ordinary unit is legal");

    let evo_refs: Vec<&str> = evos.iter().map(String::as_str).collect();
    let base = wfsim_engine::loadout::WeaponBase::from_data(&weapon, false, &evo_refs);
    let pool = wfsim_engine::mods_data::pool_for_weapon(&weapon);
    let refs: Vec<&wfsim_engine::loadout::ModDef> = mod_ids
        .iter()
        .map(|id| {
            pool.iter()
                .find(|m| m.id == *id)
                .unwrap_or_else(|| panic!("no mod {id} in {weapon}'s pool"))
        })
        .collect();
    let panel =
        wfsim_engine::loadout::resolve(&base, &refs, wfsim_engine::loadout::StackPolicy::Emergent);
    let fx = match &arcane {
        Some(id) => {
            let def = wfsim_engine::arcanes_data::slots()
                .iter()
                .find_map(|s| wfsim_engine::arcanes_data::for_slot(s, id))
                .unwrap_or_else(|| panic!("no arcane {id}"));
            def.fx(
                def.max_rank,
                wfsim_engine::loadout::StackPolicy::Emergent,
                base.traits,
                wfsim_engine::tenno_data::default_tenno(),
            )
        }
        None => wfsim_engine::arcanes_data::ArcaneFx::none(),
    };

    println!(
        "{weapon} · {} mods{} · {spacing} m spacing · {runs} runs · {secs} s\n\
         (the cap is formation::MAX_BODIES = {})\n",
        refs.len(),
        arcane.as_deref().map(|a| format!(" · {a}")).unwrap_or_default(),
        wfsim_engine::formation::MAX_BODIES
    );
    println!(
        "{:>7}{:>9}{:>9}{:>11}{:>11}{:>12}",
        "grid", "placed", "touched", "ms/run", "vs 1x1", "1000 runs"
    );

    let mut baseline = 0.0f64;
    for n in [1usize, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23] {
        let cells = n * n;
        if cells > wfsim_engine::formation::MAX_BODIES {
            println!(
                "{:>7}{:>9}{:>9}{:>11}{:>11}{:>12}",
                format!("{n}x{n}"), cells, "—", "—", "—", "over cap"
            );
            continue;
        }
        let f = wfsim_engine::formation::Formation::grid(
            foe.clone(),
            DummyParams::humanoid_parts(),
            n,
            n,
            spacing,
            // The front row a few metres out, so the shooter is outside the
            // crowd rather than standing in it.
            Vec2::new(0.0, 5.0),
        );
        let mut arena = wfsim_engine::arena::Arena::training(secs);
        let pos = f.positions();
        arena.target_at = pos[f.aimed];
        arena.others = f
            .foes
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != f.aimed)
            .map(|(_, x)| x.clone())
            .collect();
        let p = DummyParams::from_panel(&panel, &arena, &fx);

        let t0 = Instant::now();
        let mut touched = 0usize;
        for r in 0..runs {
            let out = run_once(&p, &mut Rng::new(0x5EED ^ u64::from(r)));
            touched = touched.max(out.bodies_touched());
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0 / f64::from(runs);
        if n == 1 {
            baseline = ms;
        }
        println!(
            "{:>7}{:>9}{:>9}{:>11.3}{:>11}{:>12}",
            format!("{n}x{n}"),
            cells,
            touched,
            ms,
            format!("{:.1}x", ms / baseline.max(1e-9)),
            format!("{:.1} s", ms * 1000.0 / 1000.0)
        );
    }
}
