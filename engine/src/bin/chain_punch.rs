//! DOES PUNCH THROUGH ADD CHAINS? A player says it does not, in game or here.
//!
//! Reported for the Tenet Glaxion (信条·冷冻光束步枪), a chaining cold beam, and
//! said to be the same mechanic as the Larkspur's. The wiki's rule is *"Each
//! enemy hit by the main beam from Punch Through can generate a new set of 3
//! chains"*, and `chain::resolve` takes the struck bodies as its seeds — so the
//! engine is supposed to do this already. This is the experiment that says
//! whether it does.
//!
//! A LINE, because that is the only shape where punch through has anywhere to
//! go: bodies straight behind the target, one contact-width apart, so a round
//! with N metres of punch through reaches deeper into the line.
//!
//!   cargo run --release --bin chain_punch
use wfsim_engine::arena::Arena;
use wfsim_engine::arcanes_data::ArcaneFx;
use wfsim_engine::dummy::{monte_carlo, DummyParams, TargetMode};
use wfsim_engine::enemy_data;
use wfsim_engine::formation::FoeSpec;
use wfsim_engine::loadout::{resolve, StackPolicy, WeaponBase};
use wfsim_engine::space::Vec2;

const RUNS: u32 = 200;
const SEED: u64 = 0x5eed;
const DURATION: f64 = 20.0;

fn arena(weapon_bodies: usize, spacing_m: f64) -> Arena {
    let e = enemy_data::all()
        .into_iter()
        .find(|e| e.id == "thrax_centurion")
        .expect("the ruler's enemy");
    let target_at = Vec2::new(0.0, 3.0);
    let mut others = Vec::new();
    for i in 1..weapon_bodies {
        others.push(FoeSpec {
            id: format!("e{}", i + 1),
            params: e
                .target_params(150, true, false, TargetMode::InstantRespawn)
                .expect("target"),
            body_parts: e.aim_parts(&[("body", 1.0)]).expect("a body"),
            at: Vec2::new(0.0, 3.0 + spacing_m * i as f64),
        });
    }
    Arena {
        target_id: "e1".to_string(),
        tenno: wfsim_engine::tenno_data::default_tenno().clone(),
        target: e
            .target_params(150, true, false, TargetMode::InstantRespawn)
            .expect("target"),
        body_parts: e.aim_parts(&[("body", 1.0)]).expect("a body"),
        player_at: Vec2::ORIGIN,
        target_at,
        duration_seconds: DURATION,
        abilities: Vec::new(),
        others,
        aim_at: None,
    }
}

fn run(weapon: &str, mods: &[&str], bodies: usize, spacing: f64) -> (f64, f64, usize) {
    let base = WeaponBase::from_data(weapon, true, &[]);
    let pool = wfsim_engine::mods_data::pool_for_weapon(weapon);
    let mut refs = Vec::new();
    for id in mods {
        match pool.iter().find(|m| m.id == *id) {
            Some(d) => refs.push(d),
            None => panic!("{weapon} cannot hold {id}"),
        }
    }
    let panel = resolve(&base, &refs, StackPolicy::Emergent);
    let a = arena(bodies, spacing);
    let p = DummyParams::from_panel(&panel, &a, &ArcaneFx::none());
    let punch = p.punch_through_m;
    let s = monte_carlo(&p, RUNS, SEED);
    let touched = s.mean_damage_by_body.0.iter().filter(|d| **d > 0.0).count();
    (s.mean_damage, punch, touched)
}

fn main() {
    // PER POOL, because the punch-through mod an Arch-Gun can hold is not the
    // one a rifle can: Sabot Rounds is the whole of that pool's answer, and
    // asking the Larkspur for Shred reports "cannot hold" and proves nothing.
    let cases: &[(&str, &[&str])] = &[
        ("bare", &[]),
        ("+shred", &["shred"]),
        ("+primed shred", &["primed_shred"]),
        ("+metal auger", &["metal_auger"]),
        ("+primed shred +metal auger", &["primed_shred", "metal_auger"]),
        ("+sabot rounds", &["sabot_rounds"]),
    ];
    for weapon in ["tenet_glaxion", "larkspur", "amprex"] {
        if wfsim_engine::weapons_data::all().iter().all(|w| w.id != weapon) {
            println!("(no entry: {weapon})");
            continue;
        }
        println!("\n=== {weapon} — 7 bodies in a line, 0.5 m apart ===");
        println!("{:<28} {:>10} {:>12} {:>8}", "build", "punch (m)", "damage", "bodies");
        for (label, mods) in cases {
            let ok = mods.iter().all(|id| {
                wfsim_engine::mods_data::pool_for_weapon(weapon)
                    .iter()
                    .any(|m| m.id == *id)
            });
            if !ok {
                println!("{label:<28} {:>10} {:>12} {:>8}", "-", "(cannot hold)", "-");
                continue;
            }
            let (dmg, punch, bodies) = run(weapon, mods, 7, 0.5);
            println!("{label:<28} {punch:>10.2} {dmg:>12.0} {bodies:>8}");
        }
    }
}
