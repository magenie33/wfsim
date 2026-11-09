// SPDX-License-Identifier: AGPL-3.0-or-later
//! Companion bodies, legal carriers, modular assemblies and body mod pools.

use std::sync::OnceLock;

use serde::Deserialize;

mod bodies;
mod assembly;
mod resolve;
#[cfg(test)]
mod source_tests;
pub use bodies::*;
pub use assembly::*;
pub use resolve::*;

/// **COMPANIONS HAVE 10 GENERAL SLOTS** (W`Mod`: *"Companions have 10 general
/// slots"*), and no special slot: a robotic weapon's Exilus and Arcane belong to
/// the weapon, and a companion has neither.
pub const COMPANION_MOD_SLOTS: usize = 10;

/// WHO CAN SEAT A CARD, as DE's export states it (`compatName`). `Companion` and
/// `Robotic` name distinct groups; the rest name a kind or a single model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compat {
    /// Every companion, robotic or beast.
    Companion,
    /// Every robotic companion.
    Robotic,
    /// One kind or one model, named.
    Only,
}

/// A body card: stat bonuses affect the host; per-hit effects affect its weapon.
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
    /// Weapon effect at max rank.
    pub element_per_hit: Option<crate::model::ElementPerHit>,
    pub stat_bonuses: Vec<(String, f64)>,
    pub family: Option<String>,
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
    pub fn models_body(&self) -> bool { !self.stat_bonuses.is_empty() }
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
    #[serde(default, rename = "exclusive_to")]
    _exclusive_to: Vec<String>,
    #[serde(default)]
    family: Option<String>,
    #[serde(default, rename = "combos")]
    _combos: Option<serde_norway::Value>,
    #[serde(default, rename = "set")]
    _set: Option<String>,
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
    effects: Vec<serde_norway::Value>,
    #[serde(default)]
    source: RawSource,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEffect {
    #[serde(default, rename = "rank0")]
    _rank0: Option<f64>,
    #[serde(default, rename = "rankMax")]
    _rank_max: Option<f64>,
    #[serde(default, rename = "from")]
    _from: Option<String>,
    kind: String,
    text: String,
    #[serde(default)]
    stat: Option<String>,
    #[serde(default)]
    bonus: Option<f64>,
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
            .chain(crate::data::files_under("mods/beast_claws/"))
            .map(|(p, text)| {
                let weapon_mod = p.starts_with("mods/beast_claws/");
                let r: RawMod = serde_norway::from_str(text).unwrap_or_else(|e| panic!("{p}: {e}"));
                // Weapon cards use the weapon loader's effect vocabulary.
                let effects: Vec<RawEffect> = if weapon_mod { Vec::new() } else {
                    r.effects.into_iter().map(|e| serde_norway::from_value(e)
                        .unwrap_or_else(|e| panic!("{p}: {e}"))).collect()
                };
                // Body stats, weapon effects and explicitly unsupported lines are distinct.
                let mut element_per_hit = None;
                let mut stat_bonuses = Vec::new();
                for e in &effects {
                    match e.kind.as_str() {
                        "unmodelled" => {}
                        "element_per_hit" => element_per_hit = Some(element_per_hit_of(p, e)),
                        "body_stat" => {
                            let stat = e.stat.as_ref().expect("body stat name");
                            assert!(BODY_STATS.contains(&stat.as_str()), "{p}: invalid body stat {stat}");
                            stat_bonuses.push((stat.clone(), e.bonus.expect("body stat bonus")));
                        }
                        _ if weapon_mod => {}
                        k => panic!("{p}: unknown companion effect `{k}`"),
                    }
                }
                let weapon_description = weapon_mod.then(|| crate::data::mods::desc_info(&r.id)
                    .expect("weapon card description").at(r.max_rank));
                let description = weapon_description.as_ref()
                    .map_or(r.description, |d| crate::prose::Prose::from(d.clone()));
                let effects = weapon_description.map_or_else(
                    || effects.into_iter().map(|e| e.text).collect(), |d| vec![d]);
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
                    description,
                    effects,
                    element_per_hit,
                    stat_bonuses,
                    family: r.family,
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
    let groups = companion(host).map(host_groups).unwrap_or_default();
    companion_mods()
        .iter()
        .filter(move |m| groups.contains(&m.compat))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_companion_host_is_named_and_its_stats_are_the_floors() {
        let c = companions();
        assert_eq!(c.len(), 39);
        assert_eq!(c.iter().filter(|c| c.prototype_scope.is_none()).count(), 36);
        assert_eq!(companion(SENTINEL_PROTOTYPE).unwrap().name, crate::data::companions::sentinel_wielder().name);
        assert!(c.windows(2).all(|pair| !pair[0].prototype_scope.is_some() || pair[1].prototype_scope.is_some()));
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
        assert!(m.iter().filter(|x| x.precept).count() > 40);
        let paying: Vec<&str> = m.iter().filter(|x| x.pays()).map(|x| x.id.as_str()).collect();
        assert_eq!(paying, ["fired_up"]);
    }

    /// A BUILD BRINGS ITS PILE ONLY FROM A HOST THAT SEATS THE CARD, at the
    /// rank seated; the floor host seats no Sentinel's own card.
    #[test]
    fn a_sentinel_build_brings_fired_up_at_its_rank() {
        use crate::data::warframes::SlotPick;
        let pick = |rank| SlotPick { id: "fired_up".into(), rank, stacks: None };
        let on = |host: &str, mods: Vec<SlotPick>| CompanionBuild { companion: host.into(), mods, ..Default::default() }.element_per_hit();
        let max = on("sentinel", vec![pick(None)]).expect("a Sentinel seats it");
        assert!((max.per_stack - 0.05).abs() < 1e-12 && (max.max_bonus - 1.0).abs() < 1e-12);
        let r2 = on("sentinel", vec![SlotPick { id: "enhanced_vitality".into(), ..Default::default() }, pick(Some(2))]).expect("rank 2");
        assert!((r2.per_stack - 0.025).abs() < 1e-12 && (r2.max_bonus - 0.5).abs() < 1e-12);
        assert_eq!(on("prototype_moa", vec![pick(None)]), None, "the floor host cannot seat it");
        assert_eq!(on("nobody", vec![pick(None)]), None);
        assert_eq!(on("sentinel", vec![]), None);
    }

    #[test]
    fn sentinel_prototype_seats_shared_sentinel_cards_without_model_exclusives() {
        let seen: Vec<_> = seatable_by(SENTINEL_PROTOTYPE).collect();
        assert!(seen.iter().all(|m| ["companion", "robotic", "sentinel"].contains(&m.compat.as_str())));
        for id in ["enhanced_vitality", "assault_mode", "fired_up"] { assert!(seen.iter().any(|m| m.id == id)); }
        assert!(!seen.iter().any(|m| m.id == "investigator"));
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
