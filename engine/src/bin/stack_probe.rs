//! HOW MANY STACKS DOES THE PANEL ACTUALLY REPORT?
//!
//! A player asks why Heat, Toxin and Electricity show one stack when they
//! visibly pile up (owner relaying, 2026-08-21). The debuff table is fed by
//! `DebuffState::sample`, so this runs a real engagement and prints the PEAK of
//! every roster row — which is the number that reaches the chart.
//!
//!   cargo run --release --bin stack_probe
use wfsim_engine::arcanes_data::ArcaneFx;
use wfsim_engine::arena::Arena;
use wfsim_engine::dummy::{replay, DummyParams, TargetMode, DEBUFF_ROSTER};
use wfsim_engine::enemy_data;
use wfsim_engine::loadout::{resolve, StackPolicy, WeaponBase};
use wfsim_engine::space::Vec2;

fn main() {
    let e = enemy_data::all()
        .into_iter()
        .find(|x| x.id == "thrax_centurion")
        .expect("the ruler's enemy");
    // A LONG fight against a body that cannot die, so the piles have time to
    // build and nothing resets them: the question is how high a row can read,
    // not how fast this weapon kills.
    let arena = Arena {
        target_id: "e1".to_string(),
        tenno: wfsim_engine::tenno_data::default_tenno().clone(),
        target: e
            .target_params(9999, true, false, TargetMode::InstantRespawn)
            .expect("target"),
        body_parts: e.aim_parts(&[("body", 1.0)]).expect("a body"),
        player_at: Vec2::ORIGIN,
        target_at: Vec2::new(0.0, wfsim_engine::space::CONTACT_RANGE_M),
        duration_seconds: 60.0,
        abilities: Vec::new(),
        others: Vec::new(),
        aim_at: None,
    };

    // HIGH STATUS, HIGH FIRE RATE — the shape that piles DoTs up. One weapon
    // per element so a row that never moves is the element's absence rather
    // than the weapon's.
    // ONE ELEMENT AT A TIME. Four elemental mods COMBINE — Toxin+Electricity
    // is Corrosive and Heat+Cold is Blast — so a build carrying all of them
    // produces none of the single-element DoTs this is about.
    let cases: &[(&str, &[&str])] = &[
        ("soma_prime heat", &["thermite_rounds"]),
        ("soma_prime toxin", &["malignant_force"]),
        ("soma_prime electricity", &["high_voltage"]),
        ("soma_prime slash", &[]),
        ("torid gas", &["malignant_force", "thermite_rounds"]),
    ];

    for (label, mods) in cases {
        let weapon = label.split(' ').next().expect("a weapon id");
        if wfsim_engine::weapons_data::all().iter().all(|w| w.id != weapon) {
            println!("(no entry: {weapon})");
            continue;
        }
        let base = WeaponBase::from_data(weapon, true, &[]);
        let pool = wfsim_engine::mods_data::pool_for_weapon(weapon);
        let refs: Vec<_> = mods
            .iter()
            .filter_map(|id| pool.iter().find(|m| m.id == *id))
            .collect();
        let missing: Vec<&str> = mods
            .iter()
            .filter(|id| !pool.iter().any(|m| m.id == **id))
            .copied()
            .collect();
        let panel = resolve(&base, &refs, StackPolicy::Emergent);
        let p = DummyParams::from_panel(&panel, &arena, &ArcaneFx::none());
        let rep = replay(&p, 0x5eed, 600);
        println!("\n=== {weapon} ({} mods on, missing {:?}) ===", refs.len(), missing);
        let mut any = false;
        for (i, (id, cap)) in DEBUFF_ROSTER.iter().enumerate() {
            let peak = rep
                .frames
                .iter()
                .filter_map(|f| f.debuffs.first().and_then(|s| s.get(i)).copied())
                .max()
                .unwrap_or(0);
            if peak == 0 {
                continue;
            }
            any = true;
            println!("  {id:<12} peak {peak:>4}   chart cap {}", cap.map_or("∞".to_string(), |c| c.to_string()));
        }
        if !any {
            println!("  (every row flat at zero)");
        }
    }
}
