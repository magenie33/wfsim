// SPDX-License-Identifier: AGPL-3.0-or-later
//! EVERY RIVEN SHAPE A SURVEY ASKS OF A WEAPON — docs/AGENT.md §"Riven survey".
//! One JSON line per weapon: `{ weapon, class, stats, malus, shapes }`, each
//! shape a card as an appraisal stores it. `scripts/riven_survey.py` opens them.
//!
//!   cargo run --release --bin riven_shapes -- ocucor burston_prime
use wfsim_engine::build::rivens::{class_for_weapon, excluded_for, pool, unconfirmed_for};

/// Stats that move no number under the survey's single-target ruler: a shape
/// carrying one only repeats the shape without it.
const IRRELEVANT: [&str; 4] = ["ammo_maximum", "punch_through", "weapon_recoil", "zoom"];
/// The malus a shape carries when it carries one: what costs nothing here,
/// the first of these the weapon's riven can roll. A riven that rolls neither
/// is surveyed without one.
const HARMLESS_MALUS: [&str; 2] = ["zoom", "weapon_recoil"];
const BONUS_ROLL: f64 = 1.1;
const MALUS_ROLL: f64 = 0.9;
const RANK: u32 = 8;

fn main() {
    let mut failed = false;
    for weapon in std::env::args().skip(1) {
        match shapes(&weapon) {
            Ok(line) => println!("{line}"),
            Err(e) => {
                eprintln!("{weapon}: {e}");
                failed = true;
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}

fn shapes(weapon: &str) -> Result<String, String> {
    let class = class_for_weapon(weapon).ok_or("no riven class")?;
    // MELEE IS NOT SURVEYED: which of its combo, slide and heavy stats move the
    // single-target ruler has not been decided, and a guess opens shapes nobody wants.
    if class == "melee" {
        return Err("melee rivens are not surveyed".into());
    }
    let out = excluded_for(weapon);
    let unconfirmed = unconfirmed_for(weapon);
    let legal = |id: &str| !out.contains(&id) && !unconfirmed.contains(&id);
    let stats = pool(class);
    let mut normal: Vec<&str> = Vec::new();
    let mut spliced: Vec<&str> = Vec::new();
    for s in stats {
        let id = s.id.as_str();
        // A FACTION STAT IS NOT THE RULER'S FACTION, so it reads as nothing.
        if !s.bonus || id.starts_with("damage_to_") || IRRELEVANT.contains(&id) || !legal(id) {
            continue;
        }
        if s.spliced {
            spliced.push(id)
        } else {
            normal.push(id)
        }
    }
    let malus = HARMLESS_MALUS
        .iter()
        .find(|m| stats.iter().any(|s| s.id == **m && s.malus) && legal(m));
    let mut all: Vec<&str> = normal.iter().chain(spliced.iter()).copied().collect();
    all.sort_unstable();
    let mut cards = Vec::new();
    for k in [2usize, 3] {
        for combo in combinations(&all, k) {
            // ONE SPLICED STAT TO A CARD (engine `too_many_spliced`).
            if combo.iter().filter(|id| spliced.contains(id)).count() > 1 {
                continue;
            }
            let bonuses: Vec<String> = combo
                .iter()
                .map(|id| format!(r#"{{"id":"{id}","roll":{BONUS_ROLL}}}"#))
                .collect();
            let b = bonuses.join(",");
            cards.push(format!(r#"{{"bonuses":[{b}],"malus":null,"rank":{RANK}}}"#));
            if let Some(m) = malus {
                cards.push(format!(r#"{{"bonuses":[{b}],"malus":{{"id":"{m}","roll":{MALUS_ROLL}}},"rank":{RANK}}}"#));
            }
        }
    }
    let quote = |v: &[&str]| {
        v.iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(",")
    };
    Ok(format!(
        r#"{{"weapon":"{weapon}","class":"{class}","stats":[{}],"malus":{},"shapes":[{}]}}"#,
        quote(&all),
        malus.map_or("null".to_string(), |m| format!("\"{m}\"")),
        cards.join(",")
    ))
}

fn combinations<'a>(items: &[&'a str], k: usize) -> Vec<Vec<&'a str>> {
    if k == 0 {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    for (i, first) in items.iter().enumerate() {
        for mut rest in combinations(&items[i + 1..], k - 1) {
            rest.insert(0, first);
            out.push(rest);
        }
    }
    out
}
