// SPDX-License-Identifier: AGPL-3.0-or-later
//! COMPANIONS — what carries a robotic weapon, and the mods it seats.
//!
//! A companion is a Sentinel or a MOA. Its STAT BLOCK is not here: it is the
//! wielder floor, `data/tenno/sentinel.yaml`, stated once
//! ([`crate::data::tenno::sentinel_wielder`]) so a host cannot disagree with it.
//! What is here is the roster of hosts (`data/companions/`) and the pool they
//! seat (`data/companion_mods/`). docs/WARFRAMES.md §Companions.

use std::sync::OnceLock;

use serde::Deserialize;

/// A COMPANION TYPE A ROBOTIC WEAPON CAN BE HELD BY (`data/companions/`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Companion {
    pub id: String,
    pub name: String,
}

/// Every companion host, in file order.
pub fn companions() -> &'static [Companion] {
    static C: OnceLock<Vec<Companion>> = OnceLock::new();
    C.get_or_init(|| {
        crate::data::files_under("companions/")
            .map(|(p, text)| serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}")))
            .collect()
    })
}

/// **COMPANIONS HAVE 10 GENERAL SLOTS** (W`Mod`: *"Companions have 10 general
/// slots"*), and no special slot: a robotic weapon's Exilus and Arcane belong to
/// the weapon, and a companion has neither.
pub const COMPANION_MOD_SLOTS: usize = 10;

/// **FOUR PENJAGA POLARITIES, ON EVERY COMPANION.** W`Sentinel`: *"Sentinels
/// have four Penjaga Polarity slots"*; W`MOA_(Companion)`: *"Like Sentinels,
/// MOAs start with four Penjaga polarities"*, a bracket adding at most one more.
/// The floor host takes the four they share and no bracket's fifth.
pub const COMPANION_INNATE_POLARITIES: usize = 4;
pub const COMPANION_POLARITY: &str = "penjaga";

/// WHO CAN SEAT A CARD, as DE's export states it (`compatName`). `Companion` and
/// `Robotic` fit every companion; the rest name a kind or a single model, and
/// only a host of that kind seats them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compat {
    /// Every companion, robotic or beast.
    Companion,
    /// Every robotic companion — a Sentinel or a MOA.
    Robotic,
    /// One kind or one model, named.
    Only,
}

/// A CARD A COMPANION SEATS (`data/companion_mods/`).
///
/// **EVERY LINE IS UNMODELLED, AND THAT IS THE FACT RATHER THAN A GAP.** No mod,
/// evolution or arcane in any of the 21 robotic weapon pools reads a wielder's
/// stat, and no precept is run, so a card here moves no damage figure. It is
/// filed so the builder can seat it, plan its Forma and say what it does — and
/// so the first mechanic that reads one configures a card instead of inventing
/// the pool.
#[derive(Debug, Clone)]
pub struct CompanionMod {
    pub id: String,
    pub name: String,
    pub rarity: String,
    pub polarity: String,
    /// RANK-0 drain.
    pub base_drain: u32,
    pub max_rank: u32,
    /// `compatName` verbatim, lowercased: `companion`, `robotic`, `sentinel`,
    /// `moa`, or one model's id.
    pub compat: String,
    /// A behaviour the companion runs, as against a stat on a card.
    pub precept: bool,
    pub internal_name: Option<String>,
    pub description: String,
    /// The card's lines, verbatim. None of them pays — see the type's note.
    pub effects: Vec<String>,
    pub url: Option<String>,
}

impl CompanionMod {
    /// Which hosts seat it.
    pub fn fits(&self) -> Compat {
        match self.compat.as_str() {
            "companion" => Compat::Companion,
            "robotic" => Compat::Robotic,
            _ => Compat::Only,
        }
    }
    /// Drain at a rank: one more point per rank over the rank-0 cost.
    pub fn drain_at(&self, rank: u32) -> u32 {
        self.base_drain + rank.min(self.max_rank)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMod {
    id: String,
    name: String,
    rarity: String,
    polarity: String,
    base_drain: u32,
    max_rank: u32,
    compat: String,
    precept: bool,
    #[serde(default)]
    internal_name: Option<String>,
    description: String,
    effects: Vec<RawEffect>,
    #[serde(default)]
    source: RawSource,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEffect {
    kind: String,
    text: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSource {
    #[serde(default)]
    url: Option<String>,
}

/// The pool, parsed once, in name order.
pub fn companion_mods() -> &'static [CompanionMod] {
    static M: OnceLock<Vec<CompanionMod>> = OnceLock::new();
    M.get_or_init(|| {
        let mut out: Vec<CompanionMod> = crate::data::files_under("companion_mods/")
            .map(|(p, text)| {
                let r: RawMod = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
                for e in &r.effects {
                    assert_eq!(e.kind, "unmodelled", "{p}: a companion card pays nothing yet");
                }
                CompanionMod {
                    id: r.id,
                    name: r.name,
                    rarity: r.rarity,
                    polarity: r.polarity,
                    base_drain: r.base_drain,
                    max_rank: r.max_rank,
                    compat: r.compat,
                    precept: r.precept,
                    internal_name: r.internal_name,
                    description: r.description,
                    effects: r.effects.into_iter().map(|e| e.text).collect(),
                    url: r.source.url,
                }
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    })
}

pub fn companion_mod(id: &str) -> Option<&'static CompanionMod> {
    companion_mods().iter().find(|m| m.id == id)
}

/// The cards a host seats: the universal ones, plus those naming it.
pub fn seatable_by(host: &str) -> impl Iterator<Item = &'static CompanionMod> {
    let host = host.to_string();
    companion_mods()
        .iter()
        .filter(move |m| m.fits() != Compat::Only || m.compat == host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_companion_host_is_named_and_its_stats_are_the_floors() {
        let c = companions();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].id, "prototype_companion");
        assert_eq!(c[0].name, crate::data::tenno::sentinel_wielder().name);
    }

    /// THE POOL LOADS, AND NOTHING IN IT CLAIMS TO PAY.
    #[test]
    fn every_card_loads_and_pays_nothing() {
        let m = companion_mods();
        assert!(m.len() > 70, "{}", m.len());
        let mut ids: Vec<&str> = m.iter().map(|x| x.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), m.len(), "a card is filed twice");
        for x in m {
            assert!(!x.effects.is_empty(), "{}: a card states its lines", x.id);
            assert!(!x.description.is_empty(), "{}", x.id);
            assert!(x.internal_name.is_some(), "{}: the join key", x.id);
        }
        // A PRECEPT IS THE BULK OF THE POOL, and it is the half that is a
        // behaviour rather than a stat.
        assert!(m.iter().filter(|x| x.precept).count() > m.len() / 2);
    }

    /// THE FLOOR HOST SEATS WHAT EVERY COMPANION SEATS, and no model's own card:
    /// it is the floor of the Sentinels and the MOA, not one of them.
    #[test]
    fn the_floor_host_seats_only_the_universal_cards() {
        let seen: Vec<&CompanionMod> = seatable_by("prototype_companion").collect();
        assert!(seen.iter().all(|m| m.fits() != Compat::Only));
        assert!(seen.len() >= 30 && seen.len() < companion_mods().len(), "{}", seen.len());
        // Enhanced Vitality is every companion's; Assault Mode is a Sentinel's.
        assert!(seen.iter().any(|m| m.id == "enhanced_vitality"));
        assert!(!seen.iter().any(|m| m.id == "assault_mode"));
        // …and a Sentinel host, when there is one, seats its own.
        assert!(seatable_by("sentinel").any(|m| m.id == "assault_mode"));
    }

    /// A RANK COSTS A POINT, the same ladder every other pool uses.
    #[test]
    fn drain_climbs_one_point_a_rank() {
        let m = companion_mod("enhanced_vitality").expect("a common card");
        assert_eq!(m.drain_at(0), m.base_drain);
        assert_eq!(m.drain_at(m.max_rank), m.base_drain + m.max_rank);
        assert_eq!(m.drain_at(99), m.base_drain + m.max_rank, "a rank past the card's is its last");
    }
}
