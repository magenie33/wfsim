//! WHICH WEAPONS REACH A FORMATION, and which reach only the body they hit.
//!
//! Three mechanisms spread a shot (MECHANICS §12) and a weapon carries none,
//! one or more of them. This walks the roster and says which — so "does my
//! weapon do anything in a crowd" is a lookup rather than a guess, and so a
//! weapon that SHOULD spread and does not is visible instead of silent.
//!
//!   cargo run --release --bin spread_audit
use std::collections::BTreeMap;

fn main() {
    let mut rows: Vec<(String, String, String)> = Vec::new();
    let mut tally: BTreeMap<&str, usize> = BTreeMap::new();
    for w in wfsim_engine::weapons_data::all() {
        let a = &w.attack;
        let mut how: Vec<String> = Vec::new();
        if let Some(b) = &a.beam {
            how.push(format!(
                "chain {}x{}m@{:.0}%{} + sphere {:.1}m",
                b.chain.hops,
                b.chain.range_m,
                b.chain.damage_per_hop * 100.0,
                // COMPOUNDING IS THE COMMON SHAPE, so the line marks the one
                // that is not: the Kuva Nukor pays every hop the same.
                if b.chain.compounds { "^n" } else { " flat" },
                b.damage_radius_m
            ));
        }
        if let Some(r) = &a.radial {
            how.push(format!("blast {:.1}m", r.radius_m));
        }
        if let Some(f) = &a.lingering {
            how.push(format!("cloud {:.1}m for {:.0}s", f.radius_m, f.duration_seconds));
        }
        // TENDRILS are the fourth mechanism and the only one that is not a
        // spread of the shot: extra BEAMS, each on a body the main one is not
        // on (MECHANICS §12).
        if let Some(td) = &w.tendrils {
            how.push(format!(
                "{} tendrils {:.0}m within {:.0}deg",
                td.max, td.range_m, td.acquire_deg
            ));
        }
        // PUNCH THROUGH is the sixth way and the only one that is not a spread
        // at all: the same shot, still travelling (MECHANICS §13). It reaches a
        // formation with no radius, no chain and no tendril, which is why the
        // tally under-reported by 29 entries until it was listed (2026-08-17).
        if a.punch_through_m > 0.0 {
            how.push(if a.punch_through_m >= 999.0 {
                "punches through bodies without limit".to_string()
            } else {
                format!(
                    "punches {:.1} m of material ({} bodies)",
                    a.punch_through_m,
                    1 + (a.punch_through_m / wfsim_engine::space::BODY_MATERIAL_M) as usize
                )
            });
        }
        // …AND WHETHER MODS CAN ADD ANY, which is a per-entry catalog answer
        // and the difference between "brings none" and "can take none".
        let takes_mods = a
            .punch_through_mods
            .unwrap_or(a.radial.is_none() && a.lingering.is_none());
        if !takes_mods {
            how.push("takes no punch-through mods".to_string());
        }
        let kind = if a.beam.is_some() {
            "beam"
        } else if w.tendrils.is_some() {
            "tendrils"
        } else if a.radial.is_some() || a.lingering.is_some() {
            "explosive"
        } else if a.punch_through_m > 0.0 {
            "punch"
        } else {
            "single"
        };
        *tally.entry(kind).or_default() += 1;
        if !how.is_empty() {
            rows.push((w.id.clone(), kind.to_string(), how.join(" · ")));
        }
    }
    rows.sort();
    println!("{:<32}{:<11}how it reaches a formation", "weapon", "kind");
    for (id, k, how) in &rows {
        println!("{id:<32}{k:<11}{how}");
    }
    println!("\n{} entries reach a formation, of {} in the roster", rows.len(),
             wfsim_engine::weapons_data::all().len());
    for (k, n) in &tally {
        println!("  {k:<10} {n}");
    }
}
