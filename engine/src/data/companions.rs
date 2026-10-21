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

/// The floor host a robotic weapon falls back to when nothing else is linked.
pub const PROTOTYPE_COMPANION: &str = "prototype_companion";

/// Every companion host, in file order — THE PROTOTYPE LAST: it is the floor a
/// weapon falls back to, never a host anyone picks first, so every list that
/// reads this one ends with it.
pub fn companions() -> &'static [Companion] {
    static C: OnceLock<Vec<Companion>> = OnceLock::new();
    C.get_or_init(|| {
        let mut all: Vec<Companion> = crate::data::files_under("companions/")
            .map(|(p, text)| serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}")))
            .collect();
        all.sort_by_key(|c| c.id == PROTOTYPE_COMPANION);
        all
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
/// **ALMOST EVERY LINE IS UNMODELLED, AND THAT IS THE FACT RATHER THAN A GAP.**
/// No mod, evolution or arcane in any of the 21 robotic weapon pools reads a
/// wielder's stat, and no precept is run, so a card moves no damage figure —
/// except a line stating what it does to the companion's WEAPON: Fired Up's
/// heat per hit (`element_per_hit`), which the fight runs.
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
    pub description: crate::prose::Prose,
    /// The card's lines, verbatim.
    pub effects: Vec<String>,
    /// WHAT THE FIGHT RUNS, at max rank: the one kind of line that pays.
    pub element_per_hit: Option<crate::model::ElementPerHit>,
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
    /// Does any line of it pay in a fight?
    pub fn pays(&self) -> bool {
        self.element_per_hit.is_some()
    }
    /// Its per-hit element at a rank, on the ladder every ranked card climbs
    /// (`data::warframes::at_rank`): rate and ceiling both, so the pile is the
    /// same count of hits at every rank.
    pub fn element_per_hit_at(&self, rank: u32) -> Option<crate::model::ElementPerHit> {
        self.element_per_hit.map(|e| crate::model::ElementPerHit {
            per_stack: crate::data::warframes::at_rank(e.per_stack, rank, self.max_rank),
            max_bonus: crate::data::warframes::at_rank(e.max_bonus, rank, self.max_rank),
            ..e
        })
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
    description: crate::prose::Prose,
    effects: Vec<RawEffect>,
    #[serde(default)]
    source: RawSource,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEffect {
    kind: String,
    text: String,
    /// `element_per_hit` only: the element, its share per hit and its ceiling
    /// at max rank, and the pause that empties it.
    #[serde(default)]
    element: Option<String>,
    #[serde(default)]
    per_hit: Option<f64>,
    #[serde(default)]
    max_bonus: Option<f64>,
    #[serde(default)]
    reset_seconds: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSource {
    #[serde(default)]
    url: Option<String>,
}

fn element_per_hit_of(p: &str, e: &RawEffect) -> crate::model::ElementPerHit {
    let need = |v: Option<f64>, k: &str| v.unwrap_or_else(|| panic!("{p}: element_per_hit needs `{k}:`"));
    let el = e.element.as_deref().unwrap_or_else(|| panic!("{p}: element_per_hit needs `element:`"));
    crate::model::ElementPerHit {
        element: crate::rules::damage::DamageType::from_name(el).unwrap_or_else(|| panic!("{p}: element `{el}`")),
        per_stack: need(e.per_hit, "per_hit"),
        max_bonus: need(e.max_bonus, "max_bonus"),
        reset_seconds: need(e.reset_seconds, "reset_seconds"),
    }
}

/// The pool, parsed once, in name order.
pub fn companion_mods() -> &'static [CompanionMod] {
    static M: OnceLock<Vec<CompanionMod>> = OnceLock::new();
    M.get_or_init(|| {
        let mut out: Vec<CompanionMod> = crate::data::files_under("companion_mods/")
            .map(|(p, text)| {
                let r: RawMod = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
                // TWO KINDS: a line the fight runs on the companion's weapon,
                // and one it does not. Anything else is refused.
                let mut element_per_hit = None;
                for e in &r.effects {
                    match e.kind.as_str() {
                        "unmodelled" => {}
                        "element_per_hit" => element_per_hit = Some(element_per_hit_of(p, e)),
                        k => panic!("{p}: a companion card's line is `unmodelled` or `element_per_hit`, not `{k}`"),
                    }
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
                    element_per_hit,
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

/// A COMPANION BUILD as a robotic weapon's `wielder` sends it: the host and
/// the cards it seats, each at its rank (`None` = max).
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct CompanionBuild {
    pub companion: String,
    #[serde(default)]
    pub mods: Vec<crate::data::warframes::SlotPick>,
}

impl CompanionBuild {
    /// WHAT ITS CARDS PILE ON THE WEAPON per hit — the first seated card that
    /// states one, at its rank. A host nobody files, a card it cannot seat or
    /// one past the ten slots is no card at all.
    pub fn element_per_hit(&self) -> Option<crate::model::ElementPerHit> {
        if !companions().iter().any(|c| c.id == self.companion) {
            return None;
        }
        self.mods.iter().take(COMPANION_MOD_SLOTS).find_map(|p| {
            let m = seatable_by(&self.companion).find(|m| m.id == p.id)?;
            m.element_per_hit_at(p.rank.unwrap_or(m.max_rank))
        })
    }
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
        assert_eq!(c.len(), 2);
        assert_eq!(c.last().map(|x| x.id.as_str()), Some(PROTOTYPE_COMPANION), "the floor host ends the list");
        assert_eq!(c[1].name, crate::data::tenno::sentinel_wielder().name);
        assert_eq!(c[0].id, "sentinel");
    }

    /// THE POOL LOADS, AND ONE CARD PAYS: Fired Up, the only line about the
    /// companion's weapon.
    #[test]
    fn every_card_loads_and_only_fired_up_pays() {
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
        let paying: Vec<&str> = m.iter().filter(|x| x.pays()).map(|x| x.id.as_str()).collect();
        assert_eq!(paying, ["fired_up"]);
    }

    /// A BUILD BRINGS ITS PILE ONLY FROM A HOST THAT SEATS THE CARD, at the
    /// rank seated; the floor host seats no Sentinel's own card.
    #[test]
    fn a_sentinel_build_brings_fired_up_at_its_rank() {
        use crate::data::warframes::SlotPick;
        let pick = |rank| SlotPick { id: "fired_up".into(), rank, stacks: None };
        let on = |host: &str, mods: Vec<SlotPick>| CompanionBuild { companion: host.into(), mods }.element_per_hit();
        let max = on("sentinel", vec![pick(None)]).expect("a Sentinel seats it");
        assert!((max.per_stack - 0.05).abs() < 1e-12 && (max.max_bonus - 1.0).abs() < 1e-12);
        let r2 = on("sentinel", vec![SlotPick { id: "enhanced_vitality".into(), ..Default::default() }, pick(Some(2))]).expect("rank 2");
        assert!((r2.per_stack - 0.025).abs() < 1e-12 && (r2.max_bonus - 0.5).abs() < 1e-12);
        assert_eq!(on(PROTOTYPE_COMPANION, vec![pick(None)]), None, "the floor host cannot seat it");
        assert_eq!(on("nobody", vec![pick(None)]), None);
        assert_eq!(on("sentinel", vec![]), None);
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
