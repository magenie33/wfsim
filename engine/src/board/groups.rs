// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE GROUPS A BOARD CAN HOLD, derived from the data and never counted off the
//! scores: every (weapon, ruler, mode, riven-ness) the roster, the rulers'
//! ladders and each weapon's sustainable modes make. An axis or a rung added to
//! `data/benchmarks/` moves the count by itself; `wfsim-board --groups` says it.

use crate::board::benchmarks;
use crate::build::rivens::class_for_weapon;
use crate::data::weapons::{play_modes, roster};

/// ONE GROUP: the rows that compete for one answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub weapon: &'static str,
    pub ruler: String,
    pub mode: String,
    pub riven: bool,
}

/// THE MODES A WEAPON IS SCORED IN on any ruler — those it can sustain, the
/// default one named `base`. The rows a build owes take theirs from here.
pub fn board_modes(weapon: &str) -> Vec<String> {
    play_modes(weapon)
        .into_iter()
        .filter(|m| m.sustainable)
        .map(|m| if m.id.is_empty() { "base".to_string() } else { m.id.to_string() })
        .collect()
}

/// EVERY GROUP, in roster order, then ruler order, then mode. A weapon no riven
/// can roll for has only its riven-free groups.
pub fn all() -> Vec<Group> {
    let rulers = benchmarks::all();
    let mut out = Vec::new();
    for w in roster() {
        let weapon = w.id.as_str();
        let modes = board_modes(weapon);
        let rivens: &[bool] = if class_for_weapon(weapon).is_some() { &[false, true] } else { &[false] };
        for r in rulers {
            for m in &modes {
                for &riven in rivens {
                    out.push(Group { weapon, ruler: r.id.clone(), mode: m.clone(), riven });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ruler_and_every_sustainable_mode_is_a_group_twice_where_a_riven_rolls() {
        let rulers = benchmarks::all().len();
        let g = all();
        for w in roster().take(40) {
            let n = g.iter().filter(|x| x.weapon == w.id).count();
            let sides = if class_for_weapon(&w.id).is_some() { 2 } else { 1 };
            assert_eq!(n, rulers * board_modes(&w.id).len() * sides, "{}", w.id);
        }
    }
}
