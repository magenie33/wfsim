// SPDX-License-Identifier: AGPL-3.0-or-later
//! EVERY RIVEN SHAPE A SURVEY ASKS OF A WEAPON — docs/AGENT.md §"Riven survey".
//! One JSON line per weapon: `{ weapon, class, stats, malus, shapes }`, each
//! shape a card as an appraisal stores it. `scripts/riven_survey.py` opens them.
//!
//!   cargo run --release --bin riven_shapes -- ocucor burston_prime
//!   cargo run --release --bin riven_shapes -- torid --bonus=toxin,multishot,critical_chance
//!   cargo run --release --bin riven_shapes -- torid --malus=none,status_duration@1.1
use wfsim_engine::build::rivens::{class_for_weapon, excluded_for, pool, unconfirmed_for};

/// Stats that move no number under the survey's single-target ruler: a shape
/// carrying one only repeats the shape without it.
const IRRELEVANT: [&str; 4] = ["ammo_maximum", "punch_through", "weapon_recoil", "zoom"];
/// `harmless` in `--malus`: what costs nothing here, the first of these the
/// weapon's riven can roll.
const HARMLESS_MALUS: [&str; 2] = ["zoom", "weapon_recoil"];
/// THE MALUSES EACH COMBINATION IS ASKED WITH unless `--malus` says otherwise.
/// Status duration at both ends: a deeper malus strips armour faster until it
/// nullifies the burn at -100% (`DURATION_STATS`), so its best can be either.
const DEFAULT_MALUS: &str = "none,harmless,status_duration@0.9,status_duration@1.1";
const BONUS_ROLL: f64 = 1.1;
const MALUS_ROLL: f64 = 0.9;
const RANK: u32 = 8;

fn main() {
    let (mut weapons, mut bonus, mut malus) = (Vec::new(), None, DEFAULT_MALUS.to_string());
    for a in std::env::args().skip(1) {
        if let Some(v) = a.strip_prefix("--bonus=") {
            bonus = Some(
                v.split(',')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
            );
        } else if let Some(v) = a.strip_prefix("--malus=") {
            malus = v.to_string();
        } else {
            weapons.push(a);
        }
    }
    let mut failed = false;
    for weapon in weapons {
        match shapes(&weapon, bonus.as_deref(), &malus) {
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

fn shapes(weapon: &str, chosen: Option<&[String]>, malus_spec: &str) -> Result<String, String> {
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
    // A CHOSEN STAT THE WEAPON CANNOT ROLL IS REFUSED, never dropped: a survey
    // narrower than asked reads as complete.
    if let Some(chosen) = chosen {
        for id in chosen {
            if !stats.iter().any(|s| s.id == *id && s.bonus) || !legal(id) {
                return Err(format!("{id} is not a bonus this riven rolls"));
            }
        }
    }
    let mut normal: Vec<&str> = Vec::new();
    let mut spliced: Vec<&str> = Vec::new();
    for s in stats {
        let id = s.id.as_str();
        let wanted = match chosen {
            Some(c) => c.iter().any(|x| x == id),
            // A FACTION STAT IS NOT THE RULER'S FACTION, so it reads as nothing.
            None => !id.starts_with("damage_to_") && !IRRELEVANT.contains(&id),
        };
        if !s.bonus || !wanted || !legal(id) {
            continue;
        }
        if s.spliced {
            spliced.push(id)
        } else {
            normal.push(id)
        }
    }
    let maluses = maluses(malus_spec, |m| {
        stats.iter().any(|s| s.id == m && s.malus) && legal(m)
    })?;
    let mut all: Vec<&str> = normal.iter().chain(spliced.iter()).copied().collect();
    all.sort_unstable();
    let mut cards = Vec::new();
    for k in [2usize, 3] {
        for combo in combinations(&all, k) {
            // ONE SPLICED STAT TO A CARD (engine `too_many_spliced`).
            if combo.iter().filter(|id| spliced.contains(id)).count() > 1 {
                continue;
            }
            // A MALUS IS NEVER ONE OF THE CARD'S OWN BONUSES.
            for m in maluses.iter().filter(|m| {
                m.as_ref()
                    .is_none_or(|(id, _)| !combo.contains(&id.as_str()))
            }) {
                let bonuses: Vec<String> = combo
                    .iter()
                    .map(|id| format!(r#"{{"id":"{id}","roll":{BONUS_ROLL}}}"#))
                    .collect();
                let b = bonuses.join(",");
                let malus = m.as_ref().map_or("null".to_string(), |(id, roll)| {
                    format!(r#"{{"id":"{id}","roll":{roll}}}"#)
                });
                cards.push(format!(
                    r#"{{"bonuses":[{b}],"malus":{malus},"rank":{RANK}}}"#
                ));
            }
        }
    }
    let quote = |v: &[String]| {
        v.iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(",")
    };
    let named: Vec<String> = maluses
        .iter()
        .map(|m| {
            m.as_ref()
                .map_or("none".to_string(), |(id, roll)| format!("{id}@{roll}"))
        })
        .collect();
    let stats: Vec<String> = all.iter().map(|s| s.to_string()).collect();
    Ok(format!(
        r#"{{"weapon":"{weapon}","class":"{class}","stats":[{}],"malus":[{}],"shapes":[{}]}}"#,
        quote(&stats),
        quote(&named),
        cards.join(",")
    ))
}

/// `none`, `harmless`, `<id>` or `<id>@<roll>`, comma-separated, as the cards'
/// malus choices in order. One the weapon cannot roll as a malus is left out
/// and said; `harmless` when it rolls neither of its stats is left out silently.
fn maluses(spec: &str, rolls: impl Fn(&str) -> bool) -> Result<Vec<Option<(String, f64)>>, String> {
    let mut out: Vec<Option<(String, f64)>> = Vec::new();
    for part in spec.split(',').filter(|s| !s.is_empty()) {
        let m = match part {
            "none" => None,
            "harmless" => match HARMLESS_MALUS.iter().find(|m| rolls(m)) {
                Some(m) => Some((m.to_string(), MALUS_ROLL)),
                None => continue,
            },
            _ => {
                let (id, roll) = match part.split_once('@') {
                    Some((id, r)) => (
                        id,
                        r.parse::<f64>()
                            .map_err(|_| format!("{part}: a roll is a number"))?,
                    ),
                    None => (part, MALUS_ROLL),
                };
                if !(0.9..=1.1).contains(&roll) {
                    return Err(format!("{part}: a roll is 0.9 to 1.1"));
                }
                if !rolls(id) {
                    eprintln!("{id}: not a malus this riven rolls, left out");
                    continue;
                }
                Some((id.to_string(), roll))
            }
        };
        if !out.contains(&m) {
            out.push(m);
        }
    }
    if out.is_empty() {
        return Err("no malus choice left".into());
    }
    Ok(out)
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
