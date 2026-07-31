//! Riven mods: the stat pool from `data/rivens/<class>.yaml`, the value
//! formula, the generated name, and the [`ModDef`] a riven resolves to.
//!
//! A riven is not a mod with fixed numbers — it is a mod whose numbers are
//! CONSTRUCTED from a roll. Everything here is that construction, so the
//! builder can offer every legal riven and no illegal one.
//!
//! ```text
//! shown value = base x 10 x (rank + 1) x disposition x config x roll
//! ```
//!
//! `base` is DE's own per-stat number (`upgradeEntries` in the export). The
//! `10 x (rank + 1)` term is 90 at rank 8, and TWO independent sources agree
//! on it: the wiki publishes its own base-value column and it is DE's number
//! times 90, to four figures including the ugly ones — Damage 165%, Critical
//! Chance 149.99%, Fire Rate 60.03% against 164.9997 / 149.9940 / 60.0300.
//! Three values that are not round cannot match by coincidence.
//!
//! Every stat rolls its 0.9-1.1 INDEPENDENTLY — there is no shared per-riven
//! quality. That is what makes a "god roll" a thing to hunt rather than one
//! number, and it means the corner where every bonus is maximal and the
//! malus minimal is legal, merely astronomically unlikely. This builder is a
//! CONSTRUCTOR, not a roller, so it must be able to reach that corner: it is
//! the ceiling the optimizer wants.

use std::sync::OnceLock;

use serde::Deserialize;

use crate::damage::DamageType;
use crate::loadout::{Faction, IndirectStat, ModDef, ModEffect, Rarity};
use crate::mods::Polarity;

/// The random band every stat rolls within, independently (wiki).
pub const ROLL_MIN: f64 = 0.9;
pub const ROLL_MAX: f64 = 1.1;
/// Ranks 0..=8; the value scales with `(rank + 1) / 9`.
pub const MAX_RANK: u32 = 8;
/// `base x 10 x (rank + 1)`, so 90 at max rank.
const PER_RANK: f64 = 10.0;

/// How many bonuses a riven carries, and whether it carries a malus. This
/// is the ONLY thing that decides the config multipliers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shape {
    pub bonuses: u32,
    pub malus: bool,
}

impl Shape {
    /// Multiplier on every POSITIVE stat — the wiki's table.
    ///
    /// Three published tables disagree and only one survives checking:
    ///
    /// - **wiki**: 0.99 / 1.2375 / 0.75 / 0.9375, maluses -0.495 / -0.75.
    ///   Stated verbatim as a table, and internally consistent — 1.2375 is
    ///   exactly 0.99 x 1.25 and 0.9375 exactly 0.75 x 1.25, so a malus pays
    ///   the bonuses 25% in both rows. A typo would not propagate like that.
    /// - "community calculators read 1.0 / 1.25": could not be confirmed on
    ///   any page. semlar's calculator computes client-side and states no
    ///   constants; the claim only ever appeared in search summaries.
    /// - codingace: 1.30 / 1.10 / 0.90 by bonus COUNT with no malus row.
    ///   It also has a "1 bonus" case, which weapon rivens do not roll, so
    ///   its table is describing something else.
    ///
    /// We take the wiki's, as the only one verified from a primary source.
    ///
    /// AN EARLIER VERSION OF THIS CODE TOOK 1.0, arguing that it made
    /// `base x 90` land on round canonical numbers (165 / 150 / 120 / 90).
    /// That argument was wrong: the wiki publishes its own base-value column
    /// and it IS `base x 90` — Damage 165%, Critical Chance 149.99%, Fire Rate
    /// 60.03%, matching DE's export to four figures including the ugly ones.
    /// So those numbers are the BASE, reached before any config multiplier,
    /// and their roundness says nothing about it.
    ///
    /// What remains is a 1% uncertainty on two-bonus rivens, and one
    /// in-game riven with a known roll settles it.
    pub fn bonus_mult(&self) -> f64 {
        match (self.bonuses, self.malus) {
            (2, false) => 0.99,
            (2, true) => 1.2375,
            (3, false) => 0.75,
            (3, true) => 0.9375,
            // Not a shape the game rolls; treated as plain so a caller that
            // constructs one still gets a number instead of a panic.
            _ => 0.99,
        }
    }

    /// Multiplier on the MALUS. Negative: it flips the stat's sign.
    pub fn malus_mult(&self) -> f64 {
        if self.bonuses >= 3 {
            -0.75
        } else {
            -0.495
        }
    }

    pub fn is_legal(&self) -> bool {
        (2..=3).contains(&self.bonuses)
    }
}

/// One stat a riven of this class can roll.
#[derive(Debug, Clone, Deserialize)]
pub struct RivenStat {
    /// Stable English slug ("critical_chance"), our id — never DE's tag.
    pub id: String,
    /// DE's internal tag, the join key back to the export.
    pub tag: String,
    /// DE's own index in `upgradeEntries`. Seven rifle stats share one base
    /// value, so two of them at the same roll are worth EXACTLY the same and
    /// the name's magnitude ordering ties — this is what breaks it.
    #[serde(default)]
    pub order: u32,
    /// DE's per-stat base number.
    pub base: f64,
    /// Name fragments. A riven's name is GENERATED from its stats; these are
    /// the pieces.
    pub prefix: String,
    pub suffix: String,
    /// Display template, `|val|` where the number goes.
    pub text: String,
    /// Our effect kind, or `unmodeled`.
    pub kind: String,
    /// Element / physical type / faction, where the kind needs one.
    #[serde(default)]
    pub arg: Option<String>,
    /// May this stat be the MALUS? Wiki lists five that are bonus-only.
    #[serde(default = "yes")]
    pub malus: bool,
}

fn yes() -> bool {
    true
}

/// How the CARD prints a stat's number — the stored value is a fraction and
/// the card is not obliged to agree with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    /// `x100`, with a sign. Most stats.
    Percent,
    /// The raw number, with a sign. Punch Through is metres.
    Number,
    /// A MULTIPLIER off 1, no sign: the card reads "x0.59 Damage to Corpus"
    /// where the stored value is -0.41 (user, 2026-07-31). Only the three
    /// faction stats print this way, and it is why their range runs 0.xx-1.xx
    /// instead of straddling zero.
    Multiplier,
}

impl RivenStat {
    pub fn shown_as(&self) -> Shown {
        if self.kind == "faction_damage_bonus" {
            Shown::Multiplier
        } else if self.text.contains('%') {
            Shown::Percent
        } else {
            Shown::Number
        }
    }

    /// Stored fraction -> the number printed on the card.
    pub fn shown(&self, value: f64) -> f64 {
        match self.shown_as() {
            Shown::Percent => value * 100.0,
            Shown::Number => value,
            Shown::Multiplier => 1.0 + value,
        }
    }

    /// The number printed on the card -> the stored fraction. Exactly the
    /// inverse of [`Self::shown`], so a value typed off a real riven means
    /// what it says.
    pub fn from_shown(&self, shown: f64) -> f64 {
        match self.shown_as() {
            Shown::Percent => shown / 100.0,
            Shown::Number => shown,
            Shown::Multiplier => shown - 1.0,
        }
    }

    /// Decimals the CARD shows — and therefore all anyone can read off a
    /// riven they own (user, 2026-07-31). A percentage shows ONE, so a box
    /// offering two invites a precision the game never gave you.
    ///
    /// The full-precision value is still what the sim computes with: this is
    /// the reading, not the number. A stat entered at 144.8 keeps whatever
    /// roll 144.8 implies, and that roll is exact.
    pub fn decimals(&self) -> usize {
        match self.shown_as() {
            // The card's own two, and the reason the faction stats read
            // x0.59 rather than x0.6.
            Shown::Multiplier => 2,
            Shown::Percent | Shown::Number => 1,
        }
    }

    /// The whole line, template filled in.
    pub fn print(&self, value: f64) -> String {
        let s = self.shown(value);
        let d = self.decimals();
        let n = match self.shown_as() {
            // A multiplier carries its meaning in the `x`, not in a sign:
            // x0.59 is already the bad one.
            Shown::Multiplier => format!("x{s:.d$}"),
            _ => format!("{}{s:.d$}", if s >= 0.0 { "+" } else { "" }),
        };
        self.text.replace("|val|", &n)
    }
}

/// Where in its 0.9-1.1 band a roll landed, as 0-100.
///
/// This is the number riven traders read first: it says how good the ROLL is
/// with the stat, the weapon's disposition and the shape all divided out, so
/// two stats on one card are comparable and so are two cards. 100 is the top
/// of the band for a bonus AND for a malus — it is the size of the roll, not
/// a judgement about it (user, 2026-07-31).
///
/// Uniform in the roll, because that is what the band is: the wiki gives a
/// +/-10% randomisation and no shape to it.
pub fn percentile(roll: f64) -> f64 {
    ((roll - ROLL_MIN) / (ROLL_MAX - ROLL_MIN) * 100.0).clamp(0.0, 100.0)
}

#[derive(Debug, Deserialize)]
struct PoolFile {
    #[allow(dead_code)]
    class: String,
    stats: Vec<RivenStat>,
}

/// The stat pool of one mod class — `data/rivens/<class>.yaml`.
pub fn pool(class: &str) -> &'static [RivenStat] {
    static POOLS: OnceLock<std::sync::Mutex<std::collections::BTreeMap<String, &'static [RivenStat]>>> =
        OnceLock::new();
    let cache = POOLS.get_or_init(|| std::sync::Mutex::new(std::collections::BTreeMap::new()));
    let mut g = cache.lock().expect("riven pool cache");
    g.entry(class.to_string())
        .or_insert_with(|| {
            let loaded = crate::data::files_under("rivens/")
                .filter_map(|(p, text)| {
                    let want = format!("rivens/{class}.yaml");
                    (p == want).then(|| serde_norway::from_str::<PoolFile>(text).ok())?
                })
                .next()
                .map(|f| f.stats)
                .unwrap_or_default();
            &*Box::leak(loaded.into_boxed_slice())
        })
}

/// One rolled stat on a riven.
#[derive(Debug, Clone)]
pub struct RolledStat {
    pub id: String,
    /// Where in the 0.9-1.1 band this stat landed.
    pub roll: f64,
}

/// A riven, as constructed. `disposition` belongs to the WEAPON, not here —
/// it is passed in, so one riven spec reads differently on two weapons
/// exactly as it does in game.
#[derive(Debug, Clone)]
pub struct RivenSpec {
    pub class: String,
    pub bonuses: Vec<RolledStat>,
    pub malus: Option<RolledStat>,
    pub rank: u32,
    pub polarity: Polarity,
}

impl RivenSpec {
    pub fn shape(&self) -> Shape {
        Shape {
            bonuses: self.bonuses.len() as u32,
            malus: self.malus.is_some(),
        }
    }

    /// Every reason this riven could not exist in game. Empty = legal.
    pub fn illegal(&self) -> Vec<String> {
        let mut out = Vec::new();
        let p = pool(&self.class);
        if !self.shape().is_legal() {
            out.push(format!("a riven has 2 or 3 bonuses, not {}", self.bonuses.len()));
        }
        if self.rank > MAX_RANK {
            out.push(format!("rank {} is above {MAX_RANK}", self.rank));
        }
        let mut seen: Vec<&str> = Vec::new();
        for s in self.bonuses.iter().chain(self.malus.iter()) {
            // An EMPTY slot is a riven not described yet, not an illegal one.
            // The caller decides when a card is finished; this only judges
            // what has actually been said.
            if s.id.is_empty() {
                continue;
            }
            let Some(def) = p.iter().find(|x| x.id == s.id) else {
                out.push(format!("{} is not a {} riven stat", s.id, self.class));
                continue;
            };
            // One stat cannot appear twice, malus included.
            if seen.contains(&def.id.as_str()) {
                out.push(format!("{} appears twice", def.id));
            }
            seen.push(&def.id);
            if !(ROLL_MIN - 1e-9..=ROLL_MAX + 1e-9).contains(&s.roll) {
                out.push(format!("{} rolled {:.3}, outside {ROLL_MIN}-{ROLL_MAX}", def.id, s.roll));
            }
        }
        if let Some(c) = &self.malus {
            if let Some(def) = p.iter().find(|x| x.id == c.id) {
                if !def.malus {
                    out.push(format!("{} is bonus-only and can never be the malus", def.id));
                }
            }
        }
        out
    }

    /// Legality that depends on the WEAPON, not just on the riven.
    ///
    /// Wiki: "Weapons without more than 25% of a physical damage type usually
    /// cannot roll that respective attribute. For example, a Simulor Riven
    /// will never have +/- Slash Damage stat."
    ///
    /// It matters immediately: the Torid is pure Toxin, so no Impact, Puncture
    /// or Slash riven stat can exist on it at all. On the Dual Toxocyst
    /// (7.5 Impact / 60 Puncture / 7.5 Slash) only Puncture clears the bar.
    ///
    /// The wiki says "usually", and "Exceptions exist on a case by case
    /// basis" — so this is the general rule and a named exception would have
    /// to be data on the weapon, not a hole in this check.
    pub fn illegal_on(&self, base: &crate::loadout::WeaponBase) -> Vec<String> {
        let mut out = self.illegal();
        let total = base.base_vector.total();
        let p = pool(&self.class);
        for s in self.bonuses.iter().chain(self.malus.iter()) {
            let Some(def) = p.iter().find(|x| x.id == s.id) else { continue };
            if def.kind != "physical_damage_bonus" {
                continue;
            }
            let Some(t) = def.arg.as_deref().and_then(|a| match a {
                "impact" => Some(DamageType::Impact),
                "puncture" => Some(DamageType::Puncture),
                "slash" => Some(DamageType::Slash),
                _ => None,
            }) else {
                continue;
            };
            let share = if total > 0.0 { base.base_vector.get(t) / total } else { 0.0 };
            if share <= 0.25 {
                out.push(format!(
                    "{} needs more than 25% {t:?} in the weapon's base, and it has {:.0}%",
                    def.id,
                    share * 100.0
                ));
            }
        }
        out
    }

    /// The value a stat SHOWS, sign included. `bonus = false` applies the
    /// malus multiplier, which is negative and flips the stat.
    pub fn value_of(&self, stat: &RivenStat, roll: f64, bonus: bool, disposition: f64) -> f64 {
        let rank_scale = PER_RANK * (self.rank + 1) as f64;
        let shape = self.shape();
        let cfg = if bonus { shape.bonus_mult() } else { shape.malus_mult() };
        stat.base * rank_scale * disposition * cfg * roll
    }

    /// `(slot, stat, shown value)` for every FILLED slot.
    ///
    /// The slot travels with the value because a card can be half-described
    /// and still have real numbers: the shape is settled the moment it is
    /// chosen, so a stat's value never depends on the other slots being
    /// filled. Skipping the empty ones would slide the rest up by one.
    pub fn resolved_slots(&self, disposition: f64) -> Vec<(usize, &'static RivenStat, f64)> {
        let p = pool(&self.class);
        let find = |id: &str| p.iter().find(|x| x.id == id);
        let mut out = Vec::new();
        for (i, s) in self.bonuses.iter().enumerate() {
            if let Some(def) = find(&s.id) {
                out.push((i, def, self.value_of(def, s.roll, true, disposition)));
            }
        }
        if let Some(c) = &self.malus {
            if let Some(def) = find(&c.id) {
                out.push((self.bonuses.len(), def, self.value_of(def, c.roll, false, disposition)));
            }
        }
        out
    }

    /// `(stat, shown value)` for every rolled stat, bonuses then the malus.
    pub fn resolved(&self, disposition: f64) -> Vec<(&'static RivenStat, f64)> {
        self.resolved_slots(disposition).into_iter().map(|(_, d, v)| (d, v)).collect()
    }

    /// The value a stat would show at the ENDS of its roll band, lowest
    /// first. This is what lets a number box be typed into and stay legal:
    /// the bounds come from the same formula that produces the value.
    pub fn bounds_of(&self, stat: &RivenStat, bonus: bool, disposition: f64) -> (f64, f64) {
        let a = self.value_of(stat, ROLL_MIN, bonus, disposition);
        let b = self.value_of(stat, ROLL_MAX, bonus, disposition);
        if a <= b { (a, b) } else { (b, a) }
    }

    /// The roll a desired VALUE implies, clamped into the legal band — so a
    /// number typed straight from a riven you own lands on a legal roll
    /// instead of being refused.
    pub fn roll_for_value(&self, stat: &RivenStat, bonus: bool, disposition: f64, value: f64) -> f64 {
        let unit = self.value_of(stat, 1.0, bonus, disposition);
        if unit.abs() < 1e-12 {
            return 1.0;
        }
        (value / unit).clamp(ROLL_MIN, ROLL_MAX)
    }

    /// The riven's NAME, generated from its stats — it is not a free field.
    ///
    /// Wiki: the prefix and the core come from the two highest bonus
    /// magnitudes and the suffix from the lowest, in the pattern
    /// `Prefix-CoreSuffix`; the MALUS never contributes a fragment.
    ///
    /// With two bonuses there is no third fragment and the pattern the wiki
    /// also names, `CoreSuffix`, applies. That reading of the two-stat case is
    /// inference from the two patterns it lists, not something it states.
    pub fn name(&self, _disposition: f64) -> String {
        let p = pool(&self.class);
        let mut pos: Vec<(&'static RivenStat, f64)> = self
            .bonuses
            .iter()
            .filter_map(|s| p.iter().find(|x| x.id == s.id).map(|d| (d, s.roll)))
            .collect();
        // Ranked by the ROLL, not by the value. The wiki is explicit —
        // "determined by the magnitude of the randomized MODIFIER on that
        // stat" — and its example proves it: Vectis "Sati-critaata" has
        // Multishot leading, and Multishot's base (90%) is the SMALLEST of
        // the three it carries. Only its roll can have been the largest.
        //
        // This is why the name is disposition-independent: disposition scales
        // every stat alike and cannot reorder anything, and the value never
        // enters at all.
        //
        // TIES ARE THE NORM here rather than the exception, and more so than
        // when this ranked by value: in a CONSTRUCTOR every stat can sit at
        // 1.1 at once, and then all three rolls are equal (user, 2026-07-31).
        // The tiebreak is DE's own `upgradeEntries` index — explicit, because
        // a stable sort would have fallen back to the order the stats were
        // added, and a name must not depend on which one someone clicked
        // first. Whether the game breaks the tie the same way is UNVERIFIED;
        // what is verified is that our answer does not move.
        pos.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.order.cmp(&b.0.order)));
        let cap = |s: &str| {
            let mut c = s.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        };
        match pos.len() {
            0 => String::new(),
            1 => cap(&pos[0].0.prefix),
            2 => format!("{}{}", cap(&pos[0].0.prefix), pos[1].0.suffix),
            _ => format!(
                "{}-{}{}",
                cap(&pos[0].0.prefix),
                pos[1].0.prefix,
                pos[2].0.suffix
            ),
        }
    }

    /// Capacity drain. DE's base is 2 and a riven gains 2 per rank, so an
    /// unpolarised riven costs 18 at rank 8 — the number the game shows.
    pub fn drain(&self) -> u32 {
        2 + 2 * self.rank
    }

    /// The riven as a [`ModDef`], so every part of the engine that already
    /// understands a mod understands a riven — resolve, the panel, Forma
    /// planning, the optimizer. `id` must be unique across the pool.
    pub fn to_mod_def(&self, id: &'static str, disposition: f64) -> ModDef {
        let effects = self
            .resolved(disposition)
            .into_iter()
            .filter_map(|(def, v)| effect_of(def, v))
            .collect();
        ModDef {
            id,
            base_drain: self.drain(),
            max_rank: MAX_RANK,
            polarity: self.polarity,
            rarity: Rarity::Legendary,
            exilus: false,
            // A weapon takes ONE riven (user, 2026-07-31). Rivens all share
            // one family, which is the rule the pool already has for mutually
            // exclusive mods — so the picker greys the others out, the panel
            // refuses the pair, and the optimizer never enumerates a build
            // holding two, with nothing riven-specific added anywhere.
            family: Some("riven"),
            requires_weapon: None,
            set: None,
            requires: None,
            disables: Vec::new(),
            effects,
        }
    }
}

/// One resolved riven stat as a [`ModEffect`]. `None` = a stat the engine does
/// not model; it stays on the card and contributes nothing.
fn effect_of(def: &RivenStat, v: f64) -> Option<ModEffect> {
    let element = |n: &str| match n {
        "heat" => Some(DamageType::Heat),
        "cold" => Some(DamageType::Cold),
        "electricity" => Some(DamageType::Electricity),
        "toxin" => Some(DamageType::Toxin),
        "impact" => Some(DamageType::Impact),
        "puncture" => Some(DamageType::Puncture),
        "slash" => Some(DamageType::Slash),
        _ => None,
    };
    Some(match def.kind.as_str() {
        "base_damage_bonus" => ModEffect::BaseDamage(v),
        "multishot_bonus" => ModEffect::Multishot(v),
        "crit_chance_bonus" => ModEffect::CritChance(v),
        "crit_damage_bonus" => ModEffect::CritDamage(v),
        "status_chance_bonus" => ModEffect::StatusChance(v),
        "status_duration_bonus" => ModEffect::StatusDuration(v),
        "fire_rate_bonus" => ModEffect::FireRate(v),
        "reload_speed_bonus" => ModEffect::ReloadSpeed(v),
        "magazine_capacity_bonus" => ModEffect::MagazineCapacity(v),
        "elemental_damage_bonus" => ModEffect::Element(element(def.arg.as_deref()?)?, v),
        "physical_damage_bonus" => ModEffect::Physical(element(def.arg.as_deref()?)?, v),
        "faction_damage_bonus" => {
            let f = Faction::from_name(def.arg.as_deref()?);
            if f == Faction::Unknown {
                return None;
            }
            ModEffect::FactionDamage(f, v)
        }
        "punch_through_bonus" => ModEffect::Indirect(IndirectStat::PunchThrough, v),
        "ammo_max_bonus" => ModEffect::Indirect(IndirectStat::AmmoMax, v),
        "recoil_reduction" => ModEffect::Indirect(IndirectStat::Recoil, v),
        "projectile_speed_bonus" => ModEffect::Indirect(IndirectStat::ProjectileSpeed, v),
        "zoom_bonus" => ModEffect::Indirect(IndirectStat::Zoom, v),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(ids: &[&str], malus: Option<&str>, rank: u32) -> RivenSpec {
        RivenSpec {
            class: "rifle".into(),
            bonuses: ids
                .iter()
                .map(|id| RolledStat { id: (*id).into(), roll: 1.0 })
                .collect(),
            malus: malus.map(|id| RolledStat { id: id.into(), roll: 1.0 }),
            rank,
            polarity: Polarity::Madurai,
        }
    }

    #[test]
    fn both_pools_load() {
        assert_eq!(pool("rifle").len(), 24);
        assert_eq!(pool("pistol").len(), 24);
        assert!(pool("nonexistent").is_empty());
    }

    /// The scale is 90 at rank 8, and TWO sources say so. DE's export gives
    /// the base; the wiki publishes its own "base value" column, and that
    /// column IS `base x 90`. The check is the UGLY entries — 149.99 and
    /// 60.03 are not round, so matching them to four figures is not luck.
    ///
    /// Note what this test does NOT do: it does not apply a config
    /// multiplier. These are BASE values, reached before the shape is known,
    /// which is precisely why their roundness says nothing about whether the
    /// two-bonus multiplier is 0.99 or 1.0.
    #[test]
    fn the_scale_is_90_and_the_wikis_base_column_agrees() {
        let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
        for (id, wiki_pct) in [
            ("damage", 165.00),
            ("critical_chance", 149.99),
            ("fire_rate", 60.03),
            ("critical_damage", 120.00),
            ("multishot", 90.00),
        ] {
            let ours = by(id).base * 90.0 * 100.0;
            assert!(
                (ours - wiki_pct).abs() < 0.01,
                "{id}: DE base x 90 = {ours:.4}%, wiki publishes {wiki_pct}%"
            );
        }
    }

    /// Rank scales the value linearly in `(rank + 1) / 9` — rank 0 is a ninth
    /// of rank 8, which is what the wiki's worked example shows.
    #[test]
    fn rank_scales_by_one_ninth_steps() {
        let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
        let at = |r: u32| spec(&["damage", "multishot"], None, r).value_of(by("damage"), 1.0, true, 1.3);
        let full = at(8);
        assert!((at(0) - full / 9.0).abs() < 1e-9, "rank 0 is a ninth of rank 8");
        assert!((at(4) - full * 5.0 / 9.0).abs() < 1e-9);
    }

    /// The malus flips the sign, and a third bonus costs every stat 25%.
    #[test]
    fn the_shape_moves_every_stat() {
        let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
        let two = spec(&["damage", "multishot"], None, 8);
        let two_with_malus = spec(&["damage", "multishot"], Some("weapon_recoil"), 8);
        let three = spec(&["damage", "multishot", "critical_chance"], None, 8);

        let d = |s: &RivenSpec| s.value_of(by("damage"), 1.0, true, 1.0);
        assert!((d(&two) - 1.65 * 0.99).abs() < 5e-4);
        // A malus pays the bonuses exactly 25%, in both shapes.
        assert!((d(&two_with_malus) - 1.65 * 0.99 * 1.25).abs() < 5e-4);
        assert!((d(&three) - 1.65 * 0.75).abs() < 5e-4, "a third bonus costs 25%");
        let three_with_malus = spec(&["damage", "multishot", "critical_chance"], Some("weapon_recoil"), 8);
        assert!((d(&three_with_malus) - 1.65 * 0.75 * 1.25).abs() < 5e-4);

        // The malus itself: negative multiplier, so the stat inverts.
        let c = two_with_malus.value_of(by("multishot"), 1.0, false, 1.0);
        assert!(c < 0.0, "a malus is negative: {c}");
        assert!((c + 0.90 * 0.495).abs() < 5e-4);
    }

    /// Disposition is the WEAPON's, so one riven reads differently on two.
    #[test]
    fn disposition_scales_the_whole_riven() {
        let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
        let s = spec(&["damage", "multishot"], None, 8);
        let torid = s.value_of(by("damage"), 1.0, true, 1.3);
        assert!((torid - 1.65 * 0.99 * 1.3).abs() < 5e-4, "Torid at 1.3: {torid:.3}");
    }

    #[test]
    fn a_riven_is_a_mod_the_rest_of_the_engine_understands() {
        let s = spec(&["damage", "multishot"], Some("weapon_recoil"), 8);
        let m = s.to_mod_def("riven_test", 1.3);
        assert_eq!(m.base_drain, 18, "rank 8 costs 18");
        assert_eq!(m.max_rank, MAX_RANK);
        assert_eq!(m.effects.len(), 3, "two bonuses and the malus");
        assert!(matches!(m.effects[0], ModEffect::BaseDamage(v) if (v - 1.65 * 1.2375 * 1.3).abs() < 1e-3));
    }

    /// Illegal rivens are refused by REASON, so the builder can say which
    /// knob is wrong rather than just greying out.
    #[test]
    fn illegality_is_reported_per_reason() {
        assert!(spec(&["damage", "multishot"], None, 8).illegal().is_empty());
        // Four bonuses is not a riven.
        let too_many = spec(&["damage", "multishot", "critical_chance", "critical_damage"], None, 8);
        assert!(too_many.illegal().iter().any(|r| r.contains("2 or 3 bonuses")));
        // Toxin is bonus-only; it can never be the malus.
        let bad_malus = spec(&["damage", "multishot"], Some("toxin"), 8);
        assert!(
            bad_malus.illegal().iter().any(|r| r.contains("bonus-only")),
            "{:?}",
            bad_malus.illegal()
        );
        // A stat cannot appear twice.
        let dup = spec(&["damage", "damage"], None, 8);
        assert!(dup.illegal().iter().any(|r| r.contains("twice")));
        // Rolls live in 0.9-1.1.
        let mut wild = spec(&["damage", "multishot"], None, 8);
        wild.bonuses[0].roll = 1.5;
        assert!(wild.illegal().iter().any(|r| r.contains("outside")));
        // And a stat from the wrong pool.
        let mut alien = spec(&["damage", "multishot"], None, 8);
        alien.bonuses[1].id = "not_a_stat".into();
        assert!(alien.illegal().iter().any(|r| r.contains("not a rifle riven stat")));
    }

    /// A physical stat needs the weapon to actually deal that type.
    #[test]
    fn a_physical_stat_needs_more_than_25_percent_of_that_type() {
        use crate::loadout::WeaponBase;
        // The Torid is pure Toxin: no physical riven stat exists on it.
        let torid = WeaponBase::from_data("torid", true, &[]);
        for id in ["impact", "puncture", "slash"] {
            let s = spec(&["damage", id], None, 8);
            assert!(
                s.illegal_on(&torid).iter().any(|r| r.contains("more than 25%")),
                "{id} should be impossible on the Torid: {:?}",
                s.illegal_on(&torid)
            );
        }
        // And it is the RIVEN that is fine — only the pairing is wrong.
        assert!(spec(&["damage", "slash"], None, 8).illegal().is_empty());

        // Dual Toxocyst is 7.5 Impact / 60 Puncture / 7.5 Slash = 75, so only
        // Puncture (80%) clears the bar; the other two sit at 10%.
        let toxo = WeaponBase::from_data("dual_toxocyst", true, &[]);
        let ok = RivenSpec { class: "pistol".into(), ..spec(&["damage", "puncture"], None, 8) };
        assert!(ok.illegal_on(&toxo).is_empty(), "{:?}", ok.illegal_on(&toxo));
        let no = RivenSpec { class: "pistol".into(), ..spec(&["damage", "slash"], None, 8) };
        assert!(no.illegal_on(&toxo).iter().any(|r| r.contains("10%")), "{:?}", no.illegal_on(&toxo));

        // A malus is restricted the same way.
        let with_malus = RivenSpec { class: "pistol".into(), ..spec(&["damage", "multishot"], Some("impact"), 8) };
        assert!(with_malus.illegal_on(&toxo).iter().any(|r| r.contains("more than 25%")));
    }

    /// A value can be typed IN, not just rolled to — that is how a riven you
    /// already own gets entered. Out of range snaps to the nearest legal end
    /// rather than being refused, so typing is never a dead end.
    #[test]
    fn a_typed_value_becomes_the_roll_it_implies_and_stays_legal() {
        let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
        let s = spec(&["damage", "multishot"], None, 8);
        let dmg = by("damage");
        let (lo, hi) = s.bounds_of(dmg, true, 1.3);
        // Torid, two bonuses, no malus: base x 90 x 1.3 x 0.99, at each
        // end. Written from the stored base rather than the round 1.65,
        // because the stored number is 0.018333299 and 90x it is 1.6499969.
        let unit = dmg.base * 90.0 * 1.3 * 0.99;
        assert!((lo - unit * ROLL_MIN).abs() < 1e-9);
        assert!((hi - unit * ROLL_MAX).abs() < 1e-9);

        // A value inside the band round-trips to itself.
        let want = (lo + hi) / 2.0;
        let r = s.roll_for_value(dmg, true, 1.3, want);
        assert!((s.value_of(dmg, r, true, 1.3) - want).abs() < 1e-9, "round trip");
        assert!((ROLL_MIN..=ROLL_MAX).contains(&r));

        // Outside it, each end. Note both clamps: asking for far too little
        // must land on the MINIMUM, not the maximum — the sign of the miss
        // has to be respected.
        assert!((s.roll_for_value(dmg, true, 1.3, 0.01) - ROLL_MIN).abs() < 1e-9);
        assert!((s.roll_for_value(dmg, true, 1.3, 99.0) - ROLL_MAX).abs() < 1e-9);

        // A MALUS flips its stat, and Weapon Recoil is stored NEGATIVE, so
        // the malus flips it upward — recoil going UP, which is the harm.
        // Bounds still come back ordered, and the smallest roll is still the
        // gentlest malus.
        let with_malus = spec(&["damage", "multishot"], Some("weapon_recoil"), 8);
        let rec = by("weapon_recoil");
        let (clo, chi) = with_malus.bounds_of(rec, false, 1.3);
        assert!(clo <= chi, "bounds come back ordered");
        assert!(clo > 0.0, "a malus flips a negative stat upward: {clo}");
        assert!(clo.abs() < chi.abs(), "the gentlest malus is the smallest");
        assert!((with_malus.roll_for_value(rec, false, 1.3, clo) - ROLL_MIN).abs() < 1e-9);
        assert!((with_malus.roll_for_value(rec, false, 1.3, chi) - ROLL_MAX).abs() < 1e-9);
    }

    /// A weapon takes ONE riven, and the pool already has a word for that.
    #[test]
    fn two_rivens_cannot_be_equipped_together() {
        let a = spec(&["damage", "multishot"], None, 8).to_mod_def("riven:a", 1.3);
        let b = spec(&["critical_chance", "heat"], None, 8).to_mod_def("riven:b", 1.3);
        assert_eq!(a.family, Some("riven"));
        assert_eq!(a.family, b.family, "any two rivens exclude each other");
        // And it is the same mechanism ordinary mods use, not a parallel one:
        // the pool already excludes mods this way.
        assert!(crate::mods_data::class_pool("rifle")
            .iter()
            .any(|m| m.family.is_some_and(|f| f != "riven")));
    }

    /// Faction damage prints as a MULTIPLIER, because that is what the card
    /// says: a malus the sim stores as -0.41 reads "x0.59 Damage to Corpus"
    /// (user, 2026-07-31), so its range runs 0.xx-1.xx and never shows a
    /// minus sign. Everything else keeps its sign and its percent.
    #[test]
    fn a_faction_stat_prints_the_multiplier_the_card_shows() {
        let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
        let corpus = by("damage_to_corpus");
        assert_eq!(corpus.shown_as(), Shown::Multiplier);
        assert!((corpus.shown(-0.41) - 0.59).abs() < 1e-9, "the card's own number");
        assert!(corpus.print(-0.41).starts_with("x0.59"), "{}", corpus.print(-0.41));
        assert!(corpus.print(0.45).starts_with("x1.45"), "{}", corpus.print(0.45));
        // And a typed card number comes back to the stored fraction.
        assert!((corpus.from_shown(0.59) + 0.41).abs() < 1e-9);
        assert!((corpus.from_shown(corpus.shown(0.123)) - 0.123).abs() < 1e-12);

        // A real malus on a real weapon lands in 0.xx, never below zero: the
        // malus multiplier is -0.495 at two bonuses, so 0.45 x 1.3 x 0.495.
        let s = spec(&["damage", "multishot"], Some("damage_to_corpus"), 8);
        let v = s.value_of(corpus, 1.0, false, 1.3);
        assert!(v < 0.0, "stored as a loss: {v}");
        let (lo, hi) = s.bounds_of(corpus, false, 1.3);
        assert!(corpus.shown(lo) > 0.0 && corpus.shown(hi) < 1.0, "0.xx band");

        // Percent and plain-number stats are untouched.
        assert_eq!(by("damage").shown_as(), Shown::Percent);
        assert!(by("damage").print(1.65).starts_with("+165.0%"));
        assert_eq!(by("punch_through").shown_as(), Shown::Number);
        assert!(by("punch_through").print(2.7).starts_with("+2.7 "));
    }

    /// The card shows ONE decimal on a percentage, so we show one: nobody can
    /// read a second one off a riven they own (user, 2026-07-31). What is NOT
    /// rounded is the arithmetic — the roll behind a displayed 144.8 is
    /// whatever 144.8 implies, exactly.
    #[test]
    fn the_reading_is_the_cards_precision_and_the_maths_is_not() {
        let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
        let ms = by("multishot");
        assert_eq!(ms.decimals(), 1);
        assert_eq!(ms.print(1.447_87), "+144.8% Multishot");
        assert_eq!(by("damage_to_corpus").decimals(), 2, "the card's own two");

        // The value behind it is untouched: `shown` is exact and the roll a
        // typed reading implies is exact too, one decimal in or not. Read off
        // the middle of the band so the case is an ordinary roll and not a
        // clamp — the band moves with shape and disposition, so it is asked
        // for rather than written down.
        let s = spec(&["multishot", "damage"], None, 8);
        let (lo, hi) = s.bounds_of(ms, true, 1.3);
        let reading = (ms.shown((lo + hi) / 2.0) * 10.0).round() / 10.0;
        let r = s.roll_for_value(ms, true, 1.3, ms.from_shown(reading));
        assert!((ms.shown(s.value_of(ms, r, true, 1.3)) - reading).abs() < 1e-9);
        assert!(r > ROLL_MIN && r < ROLL_MAX, "an ordinary roll, not an end: {r}");
    }

    /// The percentile is the roll's place in its own band, so it compares two
    /// stats on one card and two cards on different weapons — disposition,
    /// shape and base all divide out.
    #[test]
    fn the_percentile_is_where_the_roll_landed_in_its_band() {
        assert!((percentile(ROLL_MIN) - 0.0).abs() < 1e-9);
        assert!((percentile(ROLL_MAX) - 100.0).abs() < 1e-9);
        assert!((percentile(1.0) - 50.0).abs() < 1e-9);
        assert!((percentile(1.08) - 90.0).abs() < 1e-9);
        // Out-of-band rolls cannot report out-of-band positions.
        assert!((percentile(2.0) - 100.0).abs() < 1e-9);
        assert!((percentile(0.0) - 0.0).abs() < 1e-9);

        // It is the SIZE of the roll, not a judgement: a malus at the top of
        // its band is the 100th too, and it is the worst one there is.
        let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
        let s = spec(&["damage", "multishot"], Some("weapon_recoil"), 8);
        let worst = s.value_of(by("weapon_recoil"), ROLL_MAX, false, 1.3);
        let mild = s.value_of(by("weapon_recoil"), ROLL_MIN, false, 1.3);
        assert!(worst.abs() > mild.abs());
        assert!(percentile(ROLL_MAX) > percentile(ROLL_MIN));
    }

    /// A tie is the NORM in a constructor, because ranking is by ROLL and
    /// every stat can sit at 1.1 at once (user, 2026-07-31: what if all three
    /// roll 1.1?). The name must then still be one name, and must not depend
    /// on the order the stats were entered.
    #[test]
    fn a_tied_name_does_not_depend_on_the_order_the_stats_were_entered() {
        let all_max = |ids: &[&str]| {
            let mut s = spec(ids, None, 8);
            for st in &mut s.bonuses {
                st.roll = ROLL_MAX;
            }
            s
        };
        let a = all_max(&["heat", "cold", "multishot"]);
        let b = all_max(&["multishot", "heat", "cold"]);
        let c = all_max(&["cold", "multishot", "heat"]);
        assert_eq!(a.name(1.3), b.name(1.3), "declaration order must not show");
        assert_eq!(a.name(1.3), c.name(1.3));

        // And it holds for stats with DIFFERENT bases too, because the value
        // never enters the ranking — three at 1.1 tie however big they are.
        let x = all_max(&["damage", "critical_chance", "multishot"]);
        let y = all_max(&["multishot", "damage", "critical_chance"]);
        assert_eq!(x.name(1.3), y.name(1.3));
    }

    /// Every stat rolls its band INDEPENDENTLY, so the corner where all three
    /// bonuses are maximal and the malus minimal is a legal riven — merely
    /// astronomically unlikely (user, 2026-07-31: "can they all be max at
    /// once?"). It has to be constructible, because it is the CEILING, which
    /// is exactly the riven an optimizer wants to know about.
    #[test]
    fn the_god_roll_corner_is_legal_and_is_the_ceiling() {
        let by = |id: &str| pool("rifle").iter().find(|x| x.id == id).unwrap();
        let mut god = spec(&["damage", "multishot", "critical_chance"], Some("weapon_recoil"), 8);
        for s in &mut god.bonuses {
            s.roll = ROLL_MAX;
        }
        god.malus.as_mut().unwrap().roll = ROLL_MIN;
        assert!(god.illegal().is_empty(), "{:?}", god.illegal());

        // Nothing legal beats it on a bonus...
        let mid = spec(&["damage", "multishot", "critical_chance"], Some("weapon_recoil"), 8);
        assert!(
            god.value_of(by("damage"), ROLL_MAX, true, 1.3)
                > mid.value_of(by("damage"), 1.0, true, 1.3)
        );
        // ...and nothing legal has a gentler malus: the malus is negative, so
        // the smallest roll is the least harmful.
        let worst = mid.value_of(by("weapon_recoil"), ROLL_MAX, false, 1.3);
        let best = god.value_of(by("weapon_recoil"), ROLL_MIN, false, 1.3);
        assert!(best.abs() < worst.abs(), "min roll is the kindest malus");

        // Independence is the point: two stats at different rolls is legal,
        // which it would not be if one quality applied to the whole riven.
        let mut mixed = spec(&["damage", "multishot"], None, 8);
        mixed.bonuses[0].roll = ROLL_MIN;
        mixed.bonuses[1].roll = ROLL_MAX;
        assert!(mixed.illegal().is_empty());
    }

    /// The name is GENERATED, from the bonuses, ranked by ROLL.
    ///
    /// The wiki's own worked example is the test: a Vectis riven named
    /// "Sati-critaata" has "Multishot as the highest stat, Critical Chance as
    /// the second highest, and Base Damage as the lowest". Multishot's base
    /// (90%) is the SMALLEST of those three — Damage is 165% — so "highest"
    /// cannot mean the value. It means the roll.
    #[test]
    fn the_name_comes_from_the_stats_ranked_by_roll() {
        let rolled = |pairs: &[(&str, f64)], malus: Option<&str>| RivenSpec {
            class: "rifle".into(),
            bonuses: pairs
                .iter()
                .map(|(id, r)| RolledStat { id: (*id).into(), roll: *r })
                .collect(),
            malus: malus.map(|id| RolledStat { id: id.into(), roll: 1.0 }),
            rank: 8,
            polarity: Polarity::Madurai,
        };

        // The wiki's Vectis, verbatim.
        let vectis = rolled(
            &[("multishot", 1.10), ("critical_chance", 1.05), ("damage", 0.95)],
            None,
        );
        assert_eq!(vectis.name(1.0), "Sati-critaata");

        // Value ordering would have said Visi- (damage 165% is the biggest),
        // which is the whole point of the example.
        assert!(!vectis.name(1.0).starts_with("Visi"));

        // Declaration order does not matter; the roll does.
        let shuffled = rolled(
            &[("damage", 0.95), ("multishot", 1.10), ("critical_chance", 1.05)],
            None,
        );
        assert_eq!(shuffled.name(1.0), "Sati-critaata");

        // Two bonuses: no core, so "CoreSuffix" — the higher roll's PREFIX
        // fragment and the lower one's suffix.
        let two = rolled(&[("damage", 1.10), ("multishot", 0.95)], None);
        assert_eq!(two.name(1.0), "Visican");

        // Disposition cannot rename a riven: it scales every stat alike, and
        // the ranking never looks at the value anyway.
        assert_eq!(vectis.name(1.0), vectis.name(1.55));

        // The malus contributes no fragment.
        //
        // UNVERIFIED: the wiki says names are drawn from "the randomized
        // attributes the mod has" without saying whether the malus is one of
        // them, and its example has three bonuses so it cannot settle this.
        // An in-game 2-bonus-plus-malus riven does.
        let with_malus = rolled(&[("damage", 1.10), ("multishot", 0.95)], Some("weapon_recoil"));
        assert_eq!(with_malus.name(1.0), two.name(1.0));
    }

}
